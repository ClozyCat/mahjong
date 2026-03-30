from __future__ import annotations

import asyncio
import random
import secrets
import threading
import weakref
from dataclasses import asdict, dataclass, field
from datetime import datetime, timedelta, timezone

from anyio import ClosedResourceError
from fastapi import WebSocket
from sqlalchemy.orm import Session, sessionmaker
from starlette.websockets import WebSocketDisconnect

from app.db.repositories import (
    create_player_session,
    get_room_snapshot,
    create_table_seat,
    get_player_session,
    get_reconnect_token,
    get_table_record_by_code,
    save_room_snapshot,
)
from app.domain.models import PlayerState, RoundState, Tile
from app.domain.reducer import (
    _start_opening_flowers_if_needed,
    apply_claim_action,
    apply_discard_win,
    apply_flower_action,
    apply_opening_flowers_pass,
    apply_self_kong_action,
    apply_self_draw_win,
    can_declare_flower,
    can_declare_hu,
    can_declare_self_kong,
    discard_tile,
    draw_for_turn,
    initialize_round,
)
from app.domain.wall import WallState
from app.services.presence_service import mark_connected, mark_disconnected
from app.services.reconnect_service import consume_reconnect_token, issue_reconnect_token
from app.services.table_service import close_table, get_table_by_code
from app.services.timeout_service import (
    ACTIVE_TURN_TIMEOUT_SECONDS,
    PendingTimeout,
    resolve_timeout,
    schedule_active_turn_timeout,
    schedule_claim_window_timeout,
    schedule_opening_flowers_timeout,
)

MAX_SEATS = 4
WIND_ORDER = ("east", "south", "west", "north")


class LoopSafeLock:
    def __init__(self) -> None:
        self._locks: weakref.WeakKeyDictionary[asyncio.AbstractEventLoop, asyncio.Lock] = (
            weakref.WeakKeyDictionary()
        )
        self._guard = threading.Lock()
        self._task_locks: dict[asyncio.Task[object], asyncio.Lock] = {}

    def _current_lock(self) -> asyncio.Lock:
        loop = asyncio.get_running_loop()
        with self._guard:
            lock = self._locks.get(loop)
            if lock is None:
                lock = asyncio.Lock()
                self._locks[loop] = lock
            return lock

    async def acquire(self) -> bool:
        lock = self._current_lock()
        await lock.acquire()
        task = asyncio.current_task()
        if task is None:
            raise RuntimeError("LoopSafeLock requires a running task")
        self._task_locks[task] = lock
        return True

    def release(self) -> None:
        task = asyncio.current_task()
        if task is None:
            raise RuntimeError("LoopSafeLock requires a running task")
        lock = self._task_locks.pop(task, None)
        if lock is None:
            raise RuntimeError("LoopSafeLock released without acquire")
        lock.release()

    async def __aenter__(self) -> LoopSafeLock:
        await self.acquire()
        return self

    async def __aexit__(self, exc_type, exc, tb) -> None:
        self.release()


@dataclass
class MatchState:
    prevailing_wind: str
    hand_number: int
    dealer_seat: int
    cumulative_scores: dict[int, int]
    match_finished: bool = False
    last_completed_round_id: str | None = None


@dataclass
class SeatReservation:
    seat_index: int
    nickname: str
    reconnect_token: str | None
    player_session_id: int
    websocket: WebSocket | None = None
    connected: bool = False
    ready: bool = False
    is_bot: bool = False


@dataclass
class RoomState:
    table_code: str
    phase: str = "waiting"
    test_mode: bool = False
    seats: dict[int, SeatReservation] = field(default_factory=dict)
    match_state: MatchState | None = None
    round_state: RoundState | None = None
    pending_timeout: PendingTimeout | None = None
    timeout_task: asyncio.Task[None] | None = None
    send_lock: LoopSafeLock = field(default_factory=LoopSafeLock)


class GameService:
    def __init__(
        self,
        session_factory: sessionmaker[Session],
        *,
        test_mode: bool = False,
    ) -> None:
        self._session_factory = session_factory
        self._test_mode = test_mode
        self._rooms: dict[str, RoomState] = {}
        self._lock = LoopSafeLock()

    async def join_table(
        self, *, table_code: str, nickname: str, websocket: WebSocket
    ) -> dict:
        table = self._get_table(table_code)
        if table is None:
            return self._action_rejected("table_not_found")

        async with self._lock:
            room = self._get_or_restore_room_locked(table_code) or RoomState(
                table_code=table_code,
                test_mode=self._test_mode,
            )
            self._rooms[table_code] = room
            available_seats = [
                seat_index for seat_index in range(MAX_SEATS) if seat_index not in room.seats
            ]
            if not available_seats:
                return self._action_rejected("table_full")

            seat_index = available_seats[0]
            player_session_id, reconnect_token = self._issue_join_records(
                table_code=table_code,
                seat_index=seat_index,
                nickname=nickname,
            )
            room.seats[seat_index] = SeatReservation(
                seat_index=seat_index,
                nickname=nickname,
                reconnect_token=reconnect_token,
                player_session_id=player_session_id,
                websocket=websocket,
                connected=True,
                ready=False,
            )

            if room.test_mode and room.round_state is None:
                self._add_bot_reservations_locked(room)

            if room.test_mode and len(room.seats) == MAX_SEATS and room.round_state is None:
                self._start_match_locked(room)

            self._persist_room_state_locked(room)
            joiner_snapshot = self._room_snapshot(room=room, local_seat=seat_index)
            peer_updates = self._peer_snapshot_updates_locked(room, exclude_seat=seat_index)
            prompt_targets = self._prompt_targets_locked(room)

        async with room.send_lock:
            await self._send_room_messages(
                websocket,
                self._room_messages(room=room, local_seat=seat_index),
            )
            await self._send_presence_and_snapshots(
                table_code=table_code,
                seat_index=seat_index,
                connected=True,
                peer_updates=peer_updates,
            )
            await self._send_prompt_targets(prompt_targets)
        return joiner_snapshot

    async def reconnect(
        self, *, table_code: str, token: str, websocket: WebSocket
    ) -> dict:
        reconnect_record = self._get_reconnect_record(token)
        table_record = self._get_table_record(table_code)
        if reconnect_record is None:
            return self._action_rejected("invalid_reconnect_token")
        if table_record is None or reconnect_record.table_id != table_record.id:
            return self._action_rejected("table_not_found")

        async with self._lock:
            room = self._get_or_restore_room_locked(table_code) or RoomState(
                table_code=table_code,
                test_mode=self._test_mode,
            )
            self._rooms[table_code] = room
            reservation = room.seats.get(reconnect_record.seat_index)
            if (
                reservation is not None
                and reservation.player_session_id != reconnect_record.player_session_id
            ):
                return self._action_rejected("invalid_reconnect_token")
            if reservation is None:
                reservation = self._load_seat_reservation(
                    player_session_id=reconnect_record.player_session_id,
                    seat_index=reconnect_record.seat_index,
                    reconnect_token=token,
                )
                if reservation is None:
                    return self._action_rejected("invalid_reconnect_token")
                room.seats[reconnect_record.seat_index] = reservation

            consumed_record = self._consume_reconnect_token(token)
            if consumed_record is None:
                return self._action_rejected("invalid_reconnect_token")

            reservation.reconnect_token = self._issue_reconnect_token(
                table_id=table_record.id,
                seat_index=reservation.seat_index,
                player_session_id=reservation.player_session_id,
            )
            reservation.websocket = websocket
            reservation.connected = True
            self._mark_connected(player_session_id=reservation.player_session_id)
            self._persist_room_state_locked(room)

            snapshot = self._room_snapshot(room=room, local_seat=reservation.seat_index)
            peer_updates = self._peer_snapshot_updates_locked(
                room, exclude_seat=reservation.seat_index
            )
            prompt_targets = self._prompt_targets_locked(
                room, only_seats={reservation.seat_index}
            )

        async with room.send_lock:
            await self._send_room_messages(
                websocket,
                self._room_messages(room=room, local_seat=reservation.seat_index),
            )
            await self._send_presence_and_snapshots(
                table_code=table_code,
                seat_index=reservation.seat_index,
                connected=True,
                peer_updates=peer_updates,
            )
            await self._send_prompt_targets(prompt_targets)
        return snapshot

    async def mark_ready(
        self,
        *,
        table_code: str,
        websocket: WebSocket,
        ready: bool,
    ) -> dict:
        room: RoomState | None = None
        snapshot_targets: list[tuple[WebSocket, dict]] = []

        async with self._lock:
            room = self._get_or_restore_room_locked(table_code)
            if room is None:
                return self._action_rejected("table_not_found")
            if room.round_state is not None:
                return self._action_rejected("room_already_started")

            owned_seat = next(
                (
                    seat_index
                    for seat_index, reservation in room.seats.items()
                    if reservation.websocket is websocket
                ),
                None,
            )
            if owned_seat is None:
                return self._action_rejected("seat_not_owned")

            room.seats[owned_seat].ready = ready
            self._persist_room_state_locked(room)
            snapshot_targets = self._snapshot_targets_locked(room)

        async with room.send_lock:
            await self._send_snapshot_targets(snapshot_targets)
        return {"type": "ready_accepted", "payload": {"ready": ready}}

    async def start_match(
        self,
        *,
        table_code: str,
        websocket: WebSocket,
    ) -> dict:
        room: RoomState | None = None
        snapshot_targets: list[tuple[WebSocket, dict]] = []
        prompt_targets: list[tuple[WebSocket, dict]] = []

        async with self._lock:
            room = self._get_or_restore_room_locked(table_code)
            if room is None:
                return self._action_rejected("table_not_found")
            if room.round_state is not None:
                return self._action_rejected("room_already_started")

            owned_seat = next(
                (
                    seat_index
                    for seat_index, reservation in room.seats.items()
                    if reservation.websocket is websocket
                ),
                None,
            )
            if owned_seat is None:
                return self._action_rejected("seat_not_owned")
            if not self._room_ready_to_start_locked(room):
                return self._action_rejected("room_not_ready")

            self._start_match_locked(room)
            self._persist_room_state_locked(room)
            snapshot_targets = self._snapshot_targets_locked(room)
            prompt_targets = self._prompt_targets_locked(room)

        async with room.send_lock:
            await self._send_snapshot_targets(snapshot_targets)
            await self._send_prompt_targets(prompt_targets)
        return {"type": "start_match_accepted", "payload": {}}

    async def start_next_round(
        self,
        *,
        table_code: str,
        websocket: WebSocket,
    ) -> dict:
        room: RoomState | None = None
        snapshot_targets: list[tuple[WebSocket, dict]] = []
        prompt_targets: list[tuple[WebSocket, dict]] = []

        async with self._lock:
            room = self._get_or_restore_room_locked(table_code)
            if room is None:
                return self._action_rejected("table_not_found")
            if room.round_state is None or room.round_state.phase != "settlement":
                return self._action_rejected("round_not_ready")

            owned_seat = next(
                (
                    seat_index
                    for seat_index, reservation in room.seats.items()
                    if reservation.websocket is websocket
                ),
                None,
            )
            if owned_seat is None:
                return self._action_rejected("seat_not_owned")

            self._apply_settlement_to_match_locked(room)
            if room.match_state is None:
                return self._action_rejected("round_not_ready")

            next_match_state = self._next_match_state_after_settlement(room.match_state, room.round_state)
            if next_match_state.match_finished:
                room.match_state = next_match_state
                room.phase = "finished"
                self._persist_room_state_locked(room)
                snapshot_targets = self._snapshot_targets_locked(room)
            else:
                room.match_state = next_match_state
                room.round_state = initialize_round(
                    seed=self._round_seed(table_code),
                    dealer_seat=next_match_state.dealer_seat,
                    round_id=self._round_id_for_match(next_match_state),
                    round_wind=next_match_state.prevailing_wind,
                )
                room.phase = room.round_state.phase
                room.pending_timeout = None
                self._advance_round_locked(room)
                self._auto_advance_bot_seats_locked(room)
                self._sync_timeout_task_locked(room)
                self._persist_room_state_locked(room)
                snapshot_targets = self._snapshot_targets_locked(room)
                prompt_targets = self._prompt_targets_locked(room)

        if room is None:
            return self._action_rejected("table_not_found")
        async with room.send_lock:
            await self._send_snapshot_targets(snapshot_targets)
            await self._send_prompt_targets(prompt_targets)
        return self._room_snapshot(room=room, local_seat=owned_seat)

    async def restart_match(
        self,
        *,
        table_code: str,
        websocket: WebSocket,
    ) -> dict:
        room: RoomState | None = None
        snapshot_targets: list[tuple[WebSocket, dict]] = []
        prompt_targets: list[tuple[WebSocket, dict]] = []

        async with self._lock:
            room = self._get_or_restore_room_locked(table_code)
            if room is None:
                return self._action_rejected("table_not_found")
            if room.phase != "finished":
                return self._action_rejected("match_not_finished")

            owned_seat = next(
                (
                    seat_index
                    for seat_index, reservation in room.seats.items()
                    if reservation.websocket is websocket
                ),
                None,
            )
            if owned_seat is None:
                return self._action_rejected("seat_not_owned")

            self._start_match_locked(room)
            self._persist_room_state_locked(room)
            snapshot_targets = self._snapshot_targets_locked(room)
            prompt_targets = self._prompt_targets_locked(room)

        async with room.send_lock:
            await self._send_snapshot_targets(snapshot_targets)
            await self._send_prompt_targets(prompt_targets)
        return {"type": "restart_match_accepted", "payload": {}}

    async def leave_table(
        self,
        *,
        table_code: str,
        websocket: WebSocket,
    ) -> dict:
        room: RoomState | None = None
        peer_updates: list[tuple[WebSocket, list[dict]]] = []
        broadcast_targets: list[WebSocket] = []
        messages: list[dict] = []
        snapshot_targets: list[tuple[WebSocket, dict | list[dict]]] = []
        prompt_targets: list[tuple[WebSocket, dict]] = []
        owned_seat: int | None = None

        async with self._lock:
            room = self._get_or_restore_room_locked(table_code)
            if room is None:
                return self._action_rejected("table_not_found")

            owned_seat = next(
                (
                    seat_index
                    for seat_index, reservation in room.seats.items()
                    if reservation.websocket is websocket
                ),
                None,
            )
            if owned_seat is None:
                return self._action_rejected("seat_not_owned")

            if room.round_state is None:
                reservation = room.seats.pop(owned_seat)
                if reservation.reconnect_token:
                    self._consume_reconnect_token(reservation.reconnect_token)
                self._mark_disconnected(player_session_id=reservation.player_session_id)

                if room.seats:
                    self._persist_room_state_locked(room)
                    peer_updates = self._peer_snapshot_updates_locked(
                        room, exclude_seat=owned_seat
                    )
                else:
                    self._close_room_locked(room)
            else:
                reservation = room.seats[owned_seat]
                reservation.websocket = None
                reservation.connected = True
                reservation.ready = True
                reservation.is_bot = True
                if reservation.reconnect_token:
                    self._consume_reconnect_token(reservation.reconnect_token)
                    reservation.reconnect_token = None
                self._mark_disconnected(player_session_id=reservation.player_session_id)

                messages = self._auto_advance_bot_seats_locked(room)
                self._sync_timeout_task_locked(room)
                self._persist_room_state_locked(room)
                broadcast_targets = self._connected_websockets_locked(room)
                snapshot_targets = self._snapshot_targets_locked(room)
                prompt_targets = self._prompt_targets_locked(room)

        async with room.send_lock:
            if peer_updates:
                await self._send_presence_and_snapshots(
                    table_code=table_code,
                    seat_index=owned_seat,
                    connected=False,
                    peer_updates=peer_updates,
                )
            else:
                await self._broadcast_messages(broadcast_targets, messages)
                await self._send_snapshot_targets(snapshot_targets)
                await self._send_prompt_targets(prompt_targets)
        return {
            "type": "leave_table_accepted",
            "payload": {
                "table_code": table_code,
                "seat_index": owned_seat,
            },
        }

    async def disconnect(self, table_code: str, websocket: WebSocket) -> None:
        async with self._lock:
            room = self._get_or_restore_room_locked(table_code)
            if room is None:
                return

            disconnected_seat: int | None = None
            for seat_index, reservation in room.seats.items():
                if reservation.websocket is websocket:
                    disconnected_seat = seat_index
                    reservation.websocket = None
                    reservation.connected = False
                    self._mark_disconnected(player_session_id=reservation.player_session_id)
                    self._persist_room_state_locked(room)
                    break

            if disconnected_seat is None:
                return

        asyncio.create_task(
            self._send_disconnect_updates(
                room=room,
                table_code=table_code,
                seat_index=disconnected_seat,
            )
        )

    async def next_timeout_seconds(self, table_code: str) -> float | None:
        async with self._lock:
            room = self._get_or_restore_room_locked(table_code)
            if room is None or room.pending_timeout is None:
                return None
            remaining = (
                room.pending_timeout.deadline_at - datetime.now(timezone.utc)
            ).total_seconds()
            return max(0.001, remaining)

    async def process_due_timeout(self, table_code: str) -> None:
        await self._process_due_timeout(table_code)

    async def _process_due_timeout(
        self,
        table_code: str,
        *,
        expected_timeout: PendingTimeout | None = None,
    ) -> None:
        async with self._lock:
            room = self._get_or_restore_room_locked(table_code)
            if (
                room is None
                or room.round_state is None
                or room.pending_timeout is None
            ):
                return
            if expected_timeout is not None and room.pending_timeout != expected_timeout:
                return
            if room.pending_timeout.deadline_at > datetime.now(timezone.utc):
                return

            resolution = resolve_timeout(
                state=room.round_state,
                pending_timeout=room.pending_timeout,
            )
            room.round_state = resolution.state
            room.phase = resolution.state.phase
            room.pending_timeout = resolution.next_timeout
            if room.pending_timeout is None:
                self._advance_round_locked(room)
            self._apply_settlement_to_match_locked(room)
            additional_messages = self._auto_advance_bot_seats_locked(room)
            self._sync_timeout_task_locked(room)
            self._persist_room_state_locked(room)

            broadcast_targets = self._connected_websockets_locked(room)
            snapshot_targets = self._snapshot_targets_locked(room)
            prompt_targets = self._prompt_targets_locked(room)

        async with room.send_lock:
            await self._broadcast_messages(
                broadcast_targets,
                resolution.messages + additional_messages,
            )
            if resolution.room_snapshot_required:
                await self._send_snapshot_targets(snapshot_targets)
            await self._send_prompt_targets(prompt_targets)

    async def handle_action_request(
        self, table_code: str, websocket: WebSocket, payload: dict
    ) -> None:
        reason: str | None = "seat_not_owned"
        room: RoomState | None = None
        messages: list[dict] = []
        snapshot_targets: list[tuple[WebSocket, dict]] = []
        prompt_targets: list[tuple[WebSocket, dict]] = []
        broadcast_targets: list[WebSocket] = []
        async with self._lock:
            room = self._get_or_restore_room_locked(table_code)
            if room is None or room.round_state is None:
                reason = "round_not_ready"
            else:
                owned_seat = next(
                    (
                        seat_index
                        for seat_index, reservation in room.seats.items()
                        if reservation.websocket is websocket
                    ),
                    None,
                )
                if owned_seat is None:
                    reason = "seat_not_owned"
                else:
                    action_type = str(payload.get("action_type", "")).strip()
                    tile_ids = payload.get("tile_ids", [])
                    try:
                        next_state, events = self._resolve_action_locked(
                            room.round_state,
                            seat_index=owned_seat,
                            action_type=action_type,
                            tile_ids=list(tile_ids),
                        )
                    except ValueError as exc:
                        reason = self._map_action_error(str(exc))
                    else:
                        room.round_state = next_state
                        room.phase = next_state.phase
                        room.pending_timeout = None
                        if next_state.phase != "settlement":
                            if (
                                next_state.pending_action is not None
                                and next_state.pending_action.get("type") == "opening_flowers"
                            ):
                                room.pending_timeout = schedule_opening_flowers_timeout(
                                    state=next_state
                                )
                            elif next_state.pending_action is not None:
                                room.pending_timeout = schedule_claim_window_timeout(
                                    state=next_state
                                )
                            else:
                                self._advance_round_locked(room)
                        self._apply_settlement_to_match_locked(room)
                        auto_messages = self._auto_advance_bot_seats_locked(room)
                        self._sync_timeout_task_locked(room)
                        self._persist_room_state_locked(room)
                        reason = None
                        messages = [
                            self.round_event(event["type"], event) for event in events
                        ] + auto_messages
                        broadcast_targets = self._connected_websockets_locked(room)
                        snapshot_targets = self._snapshot_targets_locked(room)
                        prompt_targets = self._prompt_targets_locked(room)

        if reason is not None:
            await websocket.send_json(self._action_rejected(reason))
            return

        if room is None:
            return
        async with room.send_lock:
            await self._broadcast_messages(broadcast_targets, messages)
            await self._send_snapshot_targets(snapshot_targets)
            await self._send_prompt_targets(prompt_targets)

    async def handle_heartbeat(self, websocket: WebSocket, payload: dict) -> None:
        await websocket.send_json({"type": "heartbeat", "payload": payload})

    def action_prompt(
        self,
        seat_index: int,
        *,
        options: list[str] | None = None,
        deadline_at: str | None = None,
    ) -> dict:
        resolved_deadline = deadline_at or (
            datetime.now(timezone.utc)
            + timedelta(seconds=ACTIVE_TURN_TIMEOUT_SECONDS)
        ).isoformat()
        return {
            "type": "action_prompt",
            "payload": {
                "seat_index": seat_index,
                "options": options or [],
                "deadline_at": resolved_deadline,
            },
        }

    def round_event(self, event_type: str, payload: dict | None = None) -> dict:
        return {
            "type": "round_event",
            "payload": {"event_type": event_type, "event": payload or {}},
        }

    def _advance_round_locked(self, room: RoomState) -> None:
        if room.round_state is None or room.round_state.phase != "playing":
            room.pending_timeout = None
            return

        if (
            room.round_state.pending_action is not None
            and room.round_state.pending_action.get("type") == "opening_flowers"
        ):
            room.pending_timeout = schedule_opening_flowers_timeout(state=room.round_state)
            return

        if room.round_state.pending_action is not None:
            room.pending_timeout = schedule_claim_window_timeout(state=room.round_state)
            return

        actor = room.round_state.current_actor
        concealed_count = len(room.round_state.players[actor].concealed_tiles)
        if concealed_count % 3 == 1:
            room.round_state, _ = draw_for_turn(room.round_state)
            if (
                room.round_state.current_actor == room.round_state.dealer_seat
                and not room.round_state.score_trackers.get("opening_flowers_completed", False)
            ):
                room.round_state = _start_opening_flowers_if_needed(room.round_state)
            room.phase = room.round_state.phase
            if room.round_state.phase != "playing":
                room.pending_timeout = None
                return

        if (
            room.round_state.pending_action is not None
            and room.round_state.pending_action.get("type") == "opening_flowers"
        ):
            room.pending_timeout = schedule_opening_flowers_timeout(state=room.round_state)
            return

        if room.round_state.pending_action is not None:
            room.pending_timeout = schedule_claim_window_timeout(state=room.round_state)
            return

        drawn_tile_id = self._active_turn_drawn_tile_id(room.round_state)
        if drawn_tile_id is None:
            room.pending_timeout = None
            return
        room.pending_timeout = schedule_active_turn_timeout(
            state=room.round_state,
            drawn_tile_id=drawn_tile_id,
        )

    def _sync_timeout_task_locked(self, room: RoomState) -> None:
        current_task = asyncio.current_task()
        if room.timeout_task is not None and room.timeout_task is not current_task:
            room.timeout_task.cancel()
        room.timeout_task = None
        if room.pending_timeout is None:
            return
        room.timeout_task = asyncio.create_task(
            self._timeout_runner(room.table_code, room.pending_timeout)
        )

    def _add_bot_reservations_locked(self, room: RoomState) -> None:
        for seat_index in range(MAX_SEATS):
            if seat_index in room.seats:
                continue
            room.seats[seat_index] = SeatReservation(
                seat_index=seat_index,
                nickname=f"Bot {seat_index}",
                reconnect_token=None,
                player_session_id=-(seat_index + 1),
                connected=True,
                ready=True,
                is_bot=True,
            )

    def _auto_advance_bot_seats_locked(self, room: RoomState) -> list[dict]:
        if room.round_state is None or not any(
            reservation.is_bot for reservation in room.seats.values()
        ):
            return []

        messages: list[dict] = []
        while room.round_state is not None and room.round_state.phase == "playing":
            if room.pending_timeout is None:
                self._advance_round_locked(room)
                if room.pending_timeout is None:
                    break

            if room.pending_timeout.kind == "claim_window":
                pending_action = room.round_state.pending_action or {}
                pending_type = pending_action.get("type")
                responded_before = tuple(pending_action.get("responded_seats", []))
                auto_pass_messages = self._auto_pass_claim_window_locked(room)
                messages.extend(auto_pass_messages)
                next_pending_action = room.round_state.pending_action or {}
                responded_after = tuple(next_pending_action.get("responded_seats", []))
                if (
                    room.pending_timeout is not None
                    and room.pending_timeout.kind == "claim_window"
                    and next_pending_action.get("type") == pending_type
                    and responded_after == responded_before
                ):
                    break
                continue

            if room.pending_timeout.kind == "opening_flowers":
                actor = room.round_state.current_actor
                if not self._is_bot_seat(room, actor):
                    break
                flower_tile_id = next(
                    (
                        tile.tile_id
                        for tile in room.round_state.players[actor].concealed_tiles
                        if tile.kind == "flower"
                    ),
                    None,
                )
                if flower_tile_id is None:
                    room.round_state, events = apply_opening_flowers_pass(
                        room.round_state,
                        seat=actor,
                    )
                else:
                    room.round_state, events = apply_flower_action(
                        room.round_state,
                        seat=actor,
                        tile_ids=[flower_tile_id],
                    )
                room.phase = room.round_state.phase
                room.pending_timeout = None
                messages.extend(self.round_event(event["type"], event) for event in events)
                continue

            actor = room.round_state.current_actor
            if not self._is_bot_seat(room, actor):
                break

            room.round_state, events = discard_tile(
                room.round_state,
                actor,
                self._random_bot_discard_tile_id(room.round_state, actor),
            )
            room.phase = room.round_state.phase
            room.pending_timeout = None
            messages.extend(self.round_event(event["type"], event) for event in events)

        return messages

    def _auto_pass_claim_window_locked(self, room: RoomState) -> list[dict]:
        assert room.round_state is not None

        while (
            room.round_state.pending_action is not None
            and room.round_state.pending_action.get("type") == "claim_window"
        ):
            pending_action = room.round_state.pending_action
            responded = set(pending_action.get("responded_seats", []))
            offered_seats = [
                seat_index
                for seat_index, claims in enumerate(pending_action.get("claim_window", []))
                if claims and seat_index not in responded
            ]
            if not offered_seats:
                break

            bot_offered_seats = [
                seat_index
                for seat_index in offered_seats
                if self._is_bot_seat(room, seat_index)
            ]
            if not bot_offered_seats:
                return []

            seat_index = bot_offered_seats[0]
            room.round_state, _ = apply_claim_action(
                room.round_state,
                seat=seat_index,
                action_type="pass",
                tiles=[],
            )
            room.phase = room.round_state.phase

        room.pending_timeout = None
        if room.round_state.phase != "settlement":
            self._advance_round_locked(room)
        return []

    def _is_bot_seat(self, room: RoomState, seat_index: int) -> bool:
        reservation = room.seats.get(seat_index)
        return reservation.is_bot if reservation is not None else False

    def _random_bot_discard_tile_id(self, state: RoundState, seat_index: int) -> str:
        concealed_tiles = state.players[seat_index].concealed_tiles
        return random.choice(concealed_tiles).tile_id

    async def _timeout_runner(
        self,
        table_code: str,
        pending_timeout: PendingTimeout,
    ) -> None:
        delay = max(
            0.0,
            (pending_timeout.deadline_at - datetime.now(timezone.utc)).total_seconds(),
        )
        try:
            await asyncio.sleep(delay)
        except asyncio.CancelledError:
            return
        await self._process_due_timeout(table_code, expected_timeout=pending_timeout)

    def _active_turn_drawn_tile_id(self, state: RoundState) -> str | None:
        actor = state.current_actor
        concealed_tiles = state.players[actor].concealed_tiles
        if len(concealed_tiles) % 3 != 2 or not concealed_tiles:
            return None
        return concealed_tiles[-1].tile_id

    def _peer_snapshot_updates_locked(
        self, room: RoomState, *, exclude_seat: int
    ) -> list[tuple[WebSocket, list[dict]]]:
        return [
            (
                reservation.websocket,
                self._room_messages(room=room, local_seat=reservation.seat_index),
            )
            for seat_index, reservation in sorted(room.seats.items())
            if seat_index != exclude_seat and reservation.websocket is not None
        ]

    def _snapshot_targets_locked(self, room: RoomState) -> list[tuple[WebSocket, list[dict]]]:
        return [
            (
                reservation.websocket,
                self._room_messages(room=room, local_seat=reservation.seat_index),
            )
            for _, reservation in sorted(room.seats.items())
            if reservation.websocket is not None
        ]

    def _connected_websockets_locked(self, room: RoomState) -> list[WebSocket]:
        return [
            reservation.websocket
            for _, reservation in sorted(room.seats.items())
            if reservation.websocket is not None
        ]

    def _prompt_targets_locked(
        self,
        room: RoomState,
        *,
        only_seats: set[int] | None = None,
    ) -> list[tuple[WebSocket, dict]]:
        if room.round_state is None or room.pending_timeout is None:
            return []

        pending_timeout = room.pending_timeout
        state = room.round_state
        targets: list[tuple[WebSocket, dict]] = []
        if pending_timeout.kind == "opening_flowers":
            seat_index = state.current_actor
            if only_seats is not None and seat_index not in only_seats:
                return []
            reservation = room.seats.get(seat_index)
            if reservation is None or reservation.websocket is None:
                return []
            options = ["flower"] if can_declare_flower(state, seat_index) else ["pass"]
            targets.append(
                (
                    reservation.websocket,
                    self.action_prompt(
                        seat_index,
                        options=options,
                        deadline_at=pending_timeout.deadline_at.isoformat(),
                    ),
                )
            )
            return targets
        if pending_timeout.kind == "active_turn":
            seat_index = state.current_actor
            if only_seats is not None and seat_index not in only_seats:
                return []
            reservation = room.seats.get(seat_index)
            if reservation is None or reservation.websocket is None:
                return []
            options = ["discard"]
            if can_declare_flower(state, seat_index):
                options.append("flower")
            if can_declare_self_kong(state, seat_index):
                options.append("kong")
            if can_declare_hu(state, seat_index, None):
                options.append("hu")
            targets.append(
                (
                    reservation.websocket,
                    self.action_prompt(
                        seat_index,
                        options=options,
                        deadline_at=pending_timeout.deadline_at.isoformat(),
                    ),
                )
            )
            return targets

        pending_action = state.pending_action or {}
        if pending_action.get("type") == "rob_kong_window":
            responded_seats = set(pending_action.get("responded_seats", []))
            for seat_index in pending_action.get("offered_hu_seats", []):
                if seat_index in responded_seats:
                    continue
                if only_seats is not None and seat_index not in only_seats:
                    continue
                reservation = room.seats.get(seat_index)
                if reservation is None or reservation.websocket is None:
                    continue
                targets.append(
                    (
                        reservation.websocket,
                        self.action_prompt(
                            seat_index,
                            options=["hu", "pass"],
                            deadline_at=pending_timeout.deadline_at.isoformat(),
                        ),
                    )
                )
            return targets

        responded_seats = set(pending_action.get("responded_seats", []))
        claim_window = pending_action.get("claim_window", [])
        for seat_index, claims in enumerate(claim_window):
            if not claims or seat_index in responded_seats:
                continue
            if only_seats is not None and seat_index not in only_seats:
                continue
            reservation = room.seats.get(seat_index)
            if reservation is None or reservation.websocket is None:
                continue
            options = sorted(claims) + ["pass"]
            targets.append(
                (
                    reservation.websocket,
                    self.action_prompt(
                        seat_index,
                        options=options,
                        deadline_at=pending_timeout.deadline_at.isoformat(),
                    ),
                )
            )
        return targets

    async def _send_presence_and_snapshots(
        self,
        *,
        table_code: str,
        seat_index: int,
        connected: bool,
        peer_updates: list[tuple[WebSocket, list[dict]]],
    ) -> None:
        presence_message = self._player_presence(table_code, seat_index, connected)
        for peer_websocket, room_messages in peer_updates:
            try:
                await peer_websocket.send_json(presence_message)
                for message in room_messages:
                    await peer_websocket.send_json(message)
            except (ClosedResourceError, WebSocketDisconnect, RuntimeError):
                continue

    async def _broadcast_messages(
        self,
        websockets: list[WebSocket],
        messages: list[dict],
    ) -> None:
        for websocket in websockets:
            try:
                for message in messages:
                    await websocket.send_json(message)
            except (ClosedResourceError, WebSocketDisconnect, RuntimeError):
                continue

    async def _send_disconnect_updates(
        self,
        *,
        room: RoomState,
        table_code: str,
        seat_index: int,
    ) -> None:
        async with self._lock:
            current_room = self._rooms.get(table_code)
            if current_room is not room:
                return
            reservation = current_room.seats.get(seat_index)
            if reservation is None or reservation.connected:
                return
            peer_updates = self._peer_snapshot_updates_locked(
                current_room, exclude_seat=seat_index
            )
        async with room.send_lock:
            await self._send_presence_and_snapshots(
                table_code=table_code,
                seat_index=seat_index,
                connected=False,
                peer_updates=peer_updates,
            )

    async def _send_snapshot_targets(
        self, snapshot_targets: list[tuple[WebSocket, dict | list[dict]]]
    ) -> None:
        for websocket, snapshot_messages in snapshot_targets:
            try:
                await self._send_room_messages(
                    websocket,
                    self._resolve_room_messages(snapshot_messages),
                )
            except (ClosedResourceError, WebSocketDisconnect, RuntimeError):
                continue

    async def _send_prompt_targets(
        self, prompt_targets: list[tuple[WebSocket, dict]]
    ) -> None:
        for websocket, prompt in prompt_targets:
            try:
                await websocket.send_json(prompt)
            except (ClosedResourceError, WebSocketDisconnect, RuntimeError):
                continue

    def _issue_join_records(
        self, *, table_code: str, seat_index: int, nickname: str
    ) -> tuple[int, str]:
        with self._session_factory() as session:
            table = self._require_table(session, table_code)
            player_session = create_player_session(
                session,
                table_id=table.id,
                seat_index=seat_index,
                nickname=nickname,
            )
            create_table_seat(
                session,
                table_id=table.id,
                seat_index=seat_index,
                player_session_id=player_session.id,
            )
            reconnect_token = issue_reconnect_token(
                session,
                table_id=table.id,
                seat_index=seat_index,
                player_session_id=player_session.id,
            )
        return player_session.id, reconnect_token.token

    def _load_seat_reservation(
        self,
        *,
        player_session_id: int,
        seat_index: int,
        reconnect_token: str,
    ) -> SeatReservation | None:
        with self._session_factory() as session:
            player_session = get_player_session(session, player_session_id)
            if player_session is None:
                return None
            return SeatReservation(
                seat_index=seat_index,
                nickname=player_session.nickname,
                reconnect_token=reconnect_token,
                player_session_id=player_session.id,
                connected=player_session.connected,
                ready=False,
            )

    def _get_reconnect_record(self, token: str):
        with self._session_factory() as session:
            return get_reconnect_token(session, token)

    def _get_table(self, table_code: str):
        with self._session_factory() as session:
            return get_table_by_code(session, table_code)

    def _get_table_record(self, table_code: str):
        with self._session_factory() as session:
            return get_table_record_by_code(session, table_code)

    def _consume_reconnect_token(self, token: str):
        with self._session_factory() as session:
            return consume_reconnect_token(session, token=token)

    def _issue_reconnect_token(
        self,
        *,
        table_id: int,
        seat_index: int,
        player_session_id: int,
    ) -> str:
        with self._session_factory() as session:
            reconnect_token = issue_reconnect_token(
                session,
                table_id=table_id,
                seat_index=seat_index,
                player_session_id=player_session_id,
            )
        return reconnect_token.token

    def _mark_connected(self, *, player_session_id: int) -> None:
        with self._session_factory() as session:
            mark_connected(session, player_session_id=player_session_id)

    def _mark_disconnected(self, *, player_session_id: int) -> None:
        with self._session_factory() as session:
            mark_disconnected(session, player_session_id=player_session_id)

    def _get_or_restore_room_locked(self, table_code: str) -> RoomState | None:
        room = self._rooms.get(table_code)
        if room is not None:
            return room

        room = self._restore_room(table_code)
        if room is None:
            return None

        self._rooms[table_code] = room
        self._sync_timeout_task_locked(room)
        return room

    def _restore_room(self, table_code: str) -> RoomState | None:
        with self._session_factory() as session:
            table = get_table_record_by_code(session, table_code)
            if table is None:
                return None
            snapshot = get_room_snapshot(session, table_id=table.id)
            if snapshot is None:
                return None
            return self._deserialize_room(snapshot.payload)

    def _persist_room_state_locked(self, room: RoomState) -> None:
        with self._session_factory() as session:
            table = self._require_table(session, room.table_code)
            save_room_snapshot(
                session,
                table_id=table.id,
                room_version=self._room_version(room),
                payload=self._serialize_room(room),
            )

    def _close_room_locked(self, room: RoomState) -> None:
        if room.timeout_task is not None:
            room.timeout_task.cancel()
            room.timeout_task = None
        self._rooms.pop(room.table_code, None)
        with self._session_factory() as session:
            close_table(session, room.table_code)

    def _require_table(self, session: Session, table_code: str):
        record = get_table_record_by_code(session, table_code)
        if record is None:
            raise ValueError("table_not_found")
        return record

    def _resolve_action_locked(
        self,
        state: RoundState,
        *,
        seat_index: int,
        action_type: str,
        tile_ids: list[str],
    ) -> tuple[RoundState, list[dict]]:
        if action_type == "discard":
            if not tile_ids:
                raise ValueError("select_tile_first")
            return discard_tile(state, seat_index, tile_ids[0])
        if action_type == "flower":
            return apply_flower_action(state, seat=seat_index, tile_ids=tile_ids)
        if action_type == "pass" and (state.pending_action or {}).get("type") == "opening_flowers":
            return apply_opening_flowers_pass(state, seat=seat_index)
        if action_type == "kong" and state.pending_action is None:
            return apply_self_kong_action(state, seat=seat_index, tile_ids=tile_ids)
        if action_type == "hu":
            if state.pending_action is not None:
                return apply_claim_action(
                    state,
                    seat=seat_index,
                    action_type="hu",
                    tiles=tile_ids,
                )
            return apply_self_draw_win(state, winner_seat=seat_index)
        if action_type in {"chow", "pung", "kong", "pass"}:
            return apply_claim_action(
                state,
                seat=seat_index,
                action_type=action_type,
                tiles=tile_ids,
            )
        raise ValueError("invalid_action")

    def _map_action_error(self, error_message: str) -> str:
        normalized = error_message.lower()
        if "current actor" in normalized or "current actor may discard" in normalized:
            return "not_your_turn"
        if "seat has already responded" in normalized:
            return "invalid_action"
        if "tile not found" in normalized:
            return "invalid_action"
        if "unsupported" in normalized:
            return "invalid_action"
        if "select_tile_first" in normalized:
            return "select_tile_first"
        if "playable phase" in normalized:
            return "round_not_ready"
        return "invalid_action"

    def _room_snapshot(self, *, room: RoomState, local_seat: int) -> dict:
        seats = [
            {
                "seat_index": seat_index,
                "nickname": room.seats[seat_index].nickname,
                "connected": room.seats[seat_index].connected,
                "ready": room.seats[seat_index].ready,
            }
            for seat_index in sorted(room.seats)
        ]
        reconnect_token = room.seats[local_seat].reconnect_token
        private_state = (
            self._private_round_state(room=room, local_seat=local_seat)
            if room.round_state is not None
            else None
        )
        return {
            "type": "room_snapshot",
            "payload": {
                "table_code": room.table_code,
                "phase": room.phase,
                "seats": seats,
                "local_seat": local_seat,
                "reconnect_token": reconnect_token,
                "match_state": self._public_match_state(room.match_state),
                "private_state": private_state,
            },
        }

    def _room_messages(self, *, room: RoomState, local_seat: int) -> list[dict]:
        messages = [self._room_snapshot(room=room, local_seat=local_seat)]
        match_result = self._match_result_message(room)
        if match_result is not None:
            messages.append(match_result)
        return messages

    def _match_result_message(self, room: RoomState) -> dict | None:
        state = room.round_state
        if state is None or state.phase != "settlement" or state.settlement is None:
            return None

        return {
            "type": "match_result",
            "payload": {
                "table_code": room.table_code,
                "round_id": state.round_id,
                "phase": "settlement",
                **state.settlement,
            },
        }

    def _match_result_from_snapshot(self, snapshot: dict) -> dict | None:
        if snapshot.get("type") != "room_snapshot":
            return None

        payload = snapshot.get("payload", {})
        if payload.get("phase") != "settlement":
            return None

        private_state = payload.get("private_state") or {}
        return {
            "type": "match_result",
            "payload": {
                "table_code": payload.get("table_code"),
                "round_id": private_state.get("round_id"),
                "phase": "settlement",
            },
        }

    def _resolve_room_messages(self, snapshot_messages: dict | list[dict]) -> list[dict]:
        if isinstance(snapshot_messages, list):
            return snapshot_messages

        messages = [snapshot_messages]
        match_result = self._match_result_from_snapshot(snapshot_messages)
        if match_result is not None:
            messages.append(match_result)
        return messages

    async def _send_room_messages(self, websocket: WebSocket, messages: list[dict]) -> None:
        for message in messages:
            await websocket.send_json(message)

    def _private_round_state(self, *, room: RoomState, local_seat: int) -> dict:
        assert room.round_state is not None
        state = room.round_state
        players = []
        for player in state.players:
            reservation = room.seats.get(player.seat)
            players.append(
                {
                    "seat_index": player.seat,
                    "nickname": reservation.nickname if reservation is not None else None,
                    "connected": reservation.connected if reservation is not None else False,
                    "concealed_count": len(player.concealed_tiles),
                    "concealed_tiles": (
                        [
                            {"tile_id": tile.tile_id, "tile_key": tile.tile_key}
                            for tile in player.concealed_tiles
                        ]
                        if player.seat == local_seat
                        else None
                    ),
                    "melds": [
                        [tile.tile_key for tile in meld]
                        for meld in player.melds
                    ],
                    "flowers": [tile.tile_key for tile in player.flowers],
                    "discards": [tile.tile_key for tile in player.discards],
                }
            )

        return {
            "round_id": state.round_id,
            "round_wind": state.round_wind,
            "dealer_seat": state.dealer_seat,
            "current_actor": state.current_actor,
            "wall_tiles_remaining": max(0, state.wall.tail_index - state.wall.head_index + 1),
            "last_discard": state.last_discard.tile_key if state.last_discard else None,
            "pending_action": self._private_pending_action(room=room, local_seat=local_seat),
            "score_state": self._round_score_state(room),
            "players": players,
        }

    def _round_score_state(self, room: RoomState) -> dict:
        assert room.round_state is not None
        state = room.round_state
        seat_count = len(state.players)
        base_scores = (
            dict(room.match_state.cumulative_scores)
            if room.match_state is not None
            else {seat: 0 for seat in range(seat_count)}
        )
        kong_entries = list((state.score_trackers or {}).get("kong_entries", []))
        kong_score_detail: list[dict] = []
        kong_delta_by_seat = {seat: 0 for seat in range(seat_count)}

        for entry in kong_entries:
            actor_seat = entry["actor_seat"]
            payer_seats = list(entry["payer_seats"])
            delta_by_seat = {seat: 0 for seat in range(seat_count)}
            for payer_seat in payer_seats:
                delta_by_seat[payer_seat] -= 1
                delta_by_seat[actor_seat] += 1
                kong_delta_by_seat[payer_seat] -= 1
                kong_delta_by_seat[actor_seat] += 1
            kong_score_detail.append(
                {
                    "kong_type": entry["kong_type"],
                    "actor_seat": actor_seat,
                    "payer_seats": payer_seats,
                    "delta_by_seat": delta_by_seat,
                }
            )

        current_round_delta_by_seat = dict(kong_delta_by_seat)
        projected_cumulative_scores = {
            seat: base_scores.get(seat, 0) + current_round_delta_by_seat[seat]
            for seat in range(seat_count)
        }
        flower_count_by_seat = {
            player.seat: len(player.flowers)
            for player in state.players
        }
        return {
            "flower_count_by_seat": flower_count_by_seat,
            "kong_score_detail": kong_score_detail,
            "kong_delta_by_seat": kong_delta_by_seat,
            "current_round_delta_by_seat": current_round_delta_by_seat,
            "base_cumulative_scores": base_scores,
            "projected_cumulative_scores": projected_cumulative_scores,
        }

    def _private_pending_action(self, *, room: RoomState, local_seat: int) -> dict | None:
        if room.round_state is None or room.pending_timeout is None:
            return None

        pending_timeout = room.pending_timeout
        if pending_timeout.kind == "opening_flowers":
            if room.round_state.current_actor != local_seat:
                return None
            options = (
                ["flower"]
                if can_declare_flower(room.round_state, local_seat)
                else ["pass"]
            )
            return {
                "type": "opening_flowers",
                "seat_index": local_seat,
                "deadline_at": pending_timeout.deadline_at.isoformat(),
                "options": options,
            }
        if pending_timeout.kind == "active_turn":
            if room.round_state.current_actor != local_seat:
                return None
            options = ["discard"]
            if can_declare_flower(room.round_state, local_seat):
                options.append("flower")
            if can_declare_self_kong(room.round_state, local_seat):
                options.append("kong")
            if can_declare_hu(room.round_state, local_seat, None):
                options.append("hu")
            return {
                "type": "active_turn",
                "seat_index": local_seat,
                "deadline_at": pending_timeout.deadline_at.isoformat(),
                "drawn_tile_id": pending_timeout.drawn_tile_id,
                "options": options,
            }

        pending_action = room.round_state.pending_action or {}
        if pending_action.get("type") == "rob_kong_window":
            responded_seats = sorted(pending_action.get("responded_seats", []))
            options = []
            if (
                local_seat in pending_action.get("offered_hu_seats", [])
                and local_seat not in responded_seats
            ):
                options = ["hu", "pass"]
            return {
                "type": "rob_kong_window",
                "actor_seat": pending_action.get("actor_seat"),
                "tile_key": pending_action.get("tile_key"),
                "deadline_at": pending_timeout.deadline_at.isoformat(),
                "responded_seats": responded_seats,
                "options": options,
            }

        responded_seats = sorted(pending_action.get("responded_seats", []))
        claim_window = pending_action.get("claim_window", [])
        offered_claims = (
            list(claim_window[local_seat]) if local_seat < len(claim_window) else []
        )
        options = []
        if offered_claims and local_seat not in responded_seats:
            options = sorted(offered_claims) + ["pass"]
        return {
            "type": "claim_window",
            "discarder_seat": pending_action.get("discarder_seat"),
            "deadline_at": pending_timeout.deadline_at.isoformat(),
            "responded_seats": responded_seats,
            "options": options,
        }

    def _player_presence(self, table_code: str, seat_index: int, connected: bool) -> dict:
        return {
            "type": "player_presence",
            "payload": {
                "table_code": table_code,
                "seat_index": seat_index,
                "connected": connected,
            },
        }

    def _action_rejected(self, reason: str) -> dict:
        return {"type": "action_rejected", "payload": {"reason": reason}}

    def _round_seed(self, table_code: str | None = None) -> int:
        return secrets.randbits(64)

    def _initial_match_state(self) -> MatchState:
        return MatchState(
            prevailing_wind="east",
            hand_number=1,
            dealer_seat=0,
            cumulative_scores={seat: 0 for seat in range(MAX_SEATS)},
        )

    def _round_id_for_match(self, match_state: MatchState) -> str:
        return (
            f"{match_state.prevailing_wind}-"
            f"{match_state.hand_number}-"
            f"dealer-{match_state.dealer_seat}-"
            f"{self._round_seed()}"
        )

    def _apply_settlement_to_match_locked(self, room: RoomState) -> None:
        if (
            room.match_state is None
            or room.round_state is None
            or room.round_state.phase != "settlement"
            or room.round_state.settlement is None
            or room.match_state.last_completed_round_id == room.round_state.round_id
        ):
            return

        total_delta = (
            room.round_state.settlement.get("score_delta", {}).get("total_delta_by_seat", {})
        )
        cumulative_scores = dict(room.match_state.cumulative_scores)
        for seat in range(MAX_SEATS):
            cumulative_scores[seat] = cumulative_scores.get(seat, 0) + int(
                total_delta.get(seat, 0)
            )
        room.match_state = MatchState(
            prevailing_wind=room.match_state.prevailing_wind,
            hand_number=room.match_state.hand_number,
            dealer_seat=room.match_state.dealer_seat,
            cumulative_scores=cumulative_scores,
            match_finished=room.match_state.match_finished,
            last_completed_round_id=room.round_state.round_id,
        )

    def _next_match_state_after_settlement(
        self,
        match_state: MatchState,
        _round_state: RoundState,
    ) -> MatchState:
        current_wind_index = WIND_ORDER.index(match_state.prevailing_wind)
        next_dealer = (match_state.dealer_seat + 1) % MAX_SEATS
        next_hand_number = match_state.hand_number + 1
        next_wind = match_state.prevailing_wind
        if next_hand_number > MAX_SEATS:
            next_hand_number = 1
            if current_wind_index == len(WIND_ORDER) - 1:
                return MatchState(
                    prevailing_wind=match_state.prevailing_wind,
                    hand_number=match_state.hand_number,
                    dealer_seat=match_state.dealer_seat,
                    cumulative_scores=dict(match_state.cumulative_scores),
                    match_finished=True,
                    last_completed_round_id=match_state.last_completed_round_id,
                )
            next_wind = WIND_ORDER[current_wind_index + 1]

        return MatchState(
            prevailing_wind=next_wind,
            hand_number=next_hand_number,
            dealer_seat=next_dealer,
            cumulative_scores=dict(match_state.cumulative_scores),
            match_finished=False,
            last_completed_round_id=match_state.last_completed_round_id,
        )

    def _room_ready_to_start_locked(self, room: RoomState) -> bool:
        return len(room.seats) == MAX_SEATS and all(
            reservation.ready for reservation in room.seats.values()
        )

    def _start_match_locked(self, room: RoomState) -> None:
        room.match_state = self._initial_match_state()
        room.round_state = initialize_round(
            seed=self._round_seed(room.table_code),
            dealer_seat=room.match_state.dealer_seat,
            round_id=self._round_id_for_match(room.match_state),
            round_wind=room.match_state.prevailing_wind,
        )
        room.phase = room.round_state.phase
        self._advance_round_locked(room)
        self._auto_advance_bot_seats_locked(room)
        self._sync_timeout_task_locked(room)

    def _room_version(self, room: RoomState) -> int:
        if room.round_state is not None:
            return room.round_state.version
        return len(room.seats)

    def _serialize_room(self, room: RoomState) -> dict:
        return {
            "table_code": room.table_code,
            "phase": room.phase,
            "test_mode": room.test_mode,
            "seats": [
                {
                    "seat_index": seat_index,
                    "nickname": reservation.nickname,
                    "reconnect_token": reservation.reconnect_token,
                    "player_session_id": reservation.player_session_id,
                    "connected": reservation.connected,
                    "ready": reservation.ready,
                    "is_bot": reservation.is_bot,
                }
                for seat_index, reservation in sorted(room.seats.items())
            ],
            "match_state": self._serialize_match_state(room.match_state),
            "round_state": asdict(room.round_state) if room.round_state is not None else None,
            "pending_timeout": self._serialize_pending_timeout(room.pending_timeout),
        }

    def _deserialize_room(self, payload: dict) -> RoomState:
        room = RoomState(
            table_code=payload["table_code"],
            phase=payload.get("phase", "waiting"),
            test_mode=payload.get("test_mode", False),
        )
        for seat_payload in payload.get("seats", []):
            seat_index = seat_payload["seat_index"]
            room.seats[seat_index] = SeatReservation(
                seat_index=seat_index,
                nickname=seat_payload["nickname"],
                reconnect_token=seat_payload.get("reconnect_token"),
                player_session_id=seat_payload["player_session_id"],
                connected=seat_payload.get("connected", False),
                ready=seat_payload.get("ready", False),
                is_bot=seat_payload.get("is_bot", False),
            )

        room.match_state = self._deserialize_match_state(payload.get("match_state"))
        round_payload = payload.get("round_state")
        if round_payload is not None:
            room.round_state = self._deserialize_round_state(round_payload)
            room.phase = room.round_state.phase

        room.pending_timeout = self._deserialize_pending_timeout(
            payload.get("pending_timeout")
        )
        return room

    def _deserialize_round_state(self, payload: dict) -> RoundState:
        wall_payload = payload["wall"]
        players = tuple(
            PlayerState(
                seat=player_payload["seat"],
                concealed_tiles=tuple(
                    self._deserialize_tile(tile_payload)
                    for tile_payload in player_payload["concealed_tiles"]
                ),
                melds=tuple(
                    tuple(
                        self._deserialize_tile(tile_payload) for tile_payload in meld_payload
                    )
                    for meld_payload in player_payload["melds"]
                ),
                flowers=tuple(
                    self._deserialize_tile(tile_payload)
                    for tile_payload in player_payload["flowers"]
                ),
                discards=tuple(
                    self._deserialize_tile(tile_payload)
                    for tile_payload in player_payload["discards"]
                ),
            )
            for player_payload in payload["players"]
        )
        last_discard_payload = payload.get("last_discard")
        return RoundState(
            round_id=payload["round_id"],
            dealer_seat=payload["dealer_seat"],
            current_actor=payload["current_actor"],
            wall=WallState(
                tiles=tuple(
                    self._deserialize_tile(tile_payload)
                    for tile_payload in wall_payload["tiles"]
                ),
                head_index=wall_payload["head_index"],
                tail_index=wall_payload["tail_index"],
            ),
            players=players,
            last_discard=(
                self._deserialize_tile(last_discard_payload)
                if last_discard_payload is not None
                else None
            ),
            pending_action=payload.get("pending_action"),
            phase=payload["phase"],
            settlement=payload.get("settlement"),
            version=payload["version"],
            score_trackers=payload.get("score_trackers"),
            last_action_context=payload.get("last_action_context"),
            round_wind=payload.get("round_wind", "east"),
        )

    def _deserialize_tile(self, payload: dict) -> Tile:
        return Tile(
            tile_id=payload["tile_id"],
            tile_key=payload["tile_key"],
            kind=payload["kind"],
            suit=payload.get("suit"),
            rank=payload.get("rank"),
            name=payload["name"],
        )

    def _serialize_pending_timeout(
        self,
        pending_timeout: PendingTimeout | None,
    ) -> dict | None:
        if pending_timeout is None:
            return None
        return {
            "kind": pending_timeout.kind,
            "seat_index": pending_timeout.seat_index,
            "deadline_at": pending_timeout.deadline_at.isoformat(),
            "drawn_tile_id": pending_timeout.drawn_tile_id,
        }

    def _deserialize_pending_timeout(
        self,
        payload: dict | None,
    ) -> PendingTimeout | None:
        if payload is None:
            return None
        return PendingTimeout(
            kind=payload["kind"],
            seat_index=payload["seat_index"],
            deadline_at=datetime.fromisoformat(payload["deadline_at"]),
            drawn_tile_id=payload.get("drawn_tile_id"),
        )

    def _serialize_match_state(self, match_state: MatchState | None) -> dict | None:
        if match_state is None:
            return None
        return {
            "prevailing_wind": match_state.prevailing_wind,
            "hand_number": match_state.hand_number,
            "dealer_seat": match_state.dealer_seat,
            "cumulative_scores": match_state.cumulative_scores,
            "match_finished": match_state.match_finished,
            "last_completed_round_id": match_state.last_completed_round_id,
        }

    def _deserialize_match_state(self, payload: dict | None) -> MatchState | None:
        if payload is None:
            return None
        return MatchState(
            prevailing_wind=payload["prevailing_wind"],
            hand_number=payload["hand_number"],
            dealer_seat=payload["dealer_seat"],
            cumulative_scores={int(seat): score for seat, score in payload["cumulative_scores"].items()},
            match_finished=payload.get("match_finished", False),
            last_completed_round_id=payload.get("last_completed_round_id"),
        )

    def _public_match_state(self, match_state: MatchState | None) -> dict | None:
        serialized = self._serialize_match_state(match_state)
        if serialized is None:
            return None
        serialized["cumulative_scores"] = {
            int(seat): score for seat, score in serialized["cumulative_scores"].items()
        }
        return serialized
