import asyncio
from datetime import datetime, timedelta, timezone

import pytest

from app.db.repositories import (
    create_player_session,
    create_table_record,
    get_player_session,
    get_reconnect_token,
)
from app.domain.models import PlayerState, RoundState, Tile
from app.domain.reducer import discard_tile, draw_for_turn, initialize_round
from app.domain.wall import WallState
from app.services.presence_service import mark_connected, mark_disconnected
from app.services.reconnect_service import (
    consume_reconnect_token,
    issue_reconnect_token,
)
from app.services.game_service import RoomState
from app.services.timeout_service import (
    ACTIVE_TURN_TIMEOUT_SECONDS,
    CLAIM_WINDOW_TIMEOUT_SECONDS,
    PendingTimeout,
    resolve_timeout,
    schedule_active_turn_timeout,
    schedule_claim_window_timeout,
)


def _make_suit_tile(tile_key: str, tile_id: str) -> Tile:
    suit_map = {"w": "characters", "t": "bamboos", "b": "dots"}
    return Tile(
        tile_id=tile_id,
        tile_key=tile_key,
        kind="suit",
        suit=suit_map[tile_key[0]],
        rank=int(tile_key[1:]),
        name=f"Test {tile_key}",
    )


class FakeWebSocket:
    def __init__(self) -> None:
        self.messages: list[dict] = []

    async def send_json(self, payload: dict) -> None:
        self.messages.append(payload)


def test_issue_and_consume_reconnect_token_marks_token_consumed(db_session) -> None:
    table = create_table_record(db_session, table_code="ROOM0001")
    player_session = create_player_session(
        db_session,
        table_id=table.id,
        seat_index=0,
        nickname="P0",
    )

    issued = issue_reconnect_token(
        db_session,
        table_id=table.id,
        seat_index=0,
        player_session_id=player_session.id,
    )

    persisted = get_reconnect_token(db_session, issued.token)
    assert persisted is not None
    assert persisted.consumed_at is None

    consumed = consume_reconnect_token(db_session, token=issued.token)

    assert consumed is not None
    assert consumed.table_id == table.id
    assert consumed.seat_index == 0
    assert consumed.player_session_id == player_session.id

    persisted = get_reconnect_token(db_session, issued.token)
    assert persisted is not None
    assert persisted.consumed_at is not None
    assert consume_reconnect_token(db_session, token=issued.token) is None


def test_mark_connected_and_disconnected_updates_player_session(db_session) -> None:
    table = create_table_record(db_session, table_code="ROOM0002")
    player_session = create_player_session(
        db_session,
        table_id=table.id,
        seat_index=1,
        nickname="P1",
        connected=True,
    )

    disconnected = mark_disconnected(db_session, player_session_id=player_session.id)
    assert disconnected is not None
    assert disconnected.connected is False
    assert get_player_session(db_session, player_session.id).connected is False

    connected = mark_connected(db_session, player_session_id=player_session.id)
    assert connected is not None
    assert connected.connected is True
    assert get_player_session(db_session, player_session.id).connected is True


def test_active_turn_timeout_auto_discards_most_recently_drawn_tile() -> None:
    now = datetime(2026, 1, 1, tzinfo=timezone.utc)
    state = initialize_round(seed=7)
    state, draw_events = draw_for_turn(state)
    draw_event = next(event for event in reversed(draw_events) if "tile_id" in event)

    pending_timeout = schedule_active_turn_timeout(
        state=state,
        drawn_tile_id=draw_event["tile_id"],
        now=now,
    )

    assert pending_timeout.deadline_at - now == timedelta(seconds=ACTIVE_TURN_TIMEOUT_SECONDS)

    resolution = resolve_timeout(state=state, pending_timeout=pending_timeout)

    assert resolution.messages[0]["type"] == "round_event"
    assert resolution.messages[0]["payload"]["event_type"] == "tile_discarded"
    assert resolution.messages[0]["payload"]["event"]["tile_id"] == draw_event["tile_id"]
    assert resolution.room_snapshot_required is True
    assert all(
        tile.tile_id != draw_event["tile_id"]
        for tile in resolution.state.players[state.current_actor].concealed_tiles
    )


def test_active_turn_timeout_discards_last_concealed_tile_when_no_drawn_tile_is_marked() -> None:
    now = datetime(2026, 1, 1, tzinfo=timezone.utc)
    state = RoundState(
        round_id="round-claim-discard-timeout",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(
            PlayerState(
                seat=0,
                concealed_tiles=(
                    _make_suit_tile("w3", "w3#keep"),
                    _make_suit_tile("w7", "w7#fallback"),
                ),
                melds=((_make_suit_tile("b2", "b2#m1"), _make_suit_tile("b3", "b3#m2"), _make_suit_tile("b4", "b4#m3")),),
                flowers=(),
                discards=(),
            ),
        )
        + tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(1, 4)
        ),
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
        score_trackers={"kong_entries": []},
        last_action_context={
            "kind": "discard",
            "seat": 3,
            "tile_id": "b4#discard",
            "from_kong_replacement": False,
            "was_last_live_tile": False,
            "was_last_discard": False,
        },
    )

    pending_timeout = schedule_active_turn_timeout(
        state=state,
        drawn_tile_id=None,
        now=now,
    )

    resolution = resolve_timeout(state=state, pending_timeout=pending_timeout)

    assert resolution.messages[0]["payload"]["event_type"] == "tile_discarded"
    assert resolution.messages[0]["payload"]["event"]["tile_id"] == "w7#fallback"
    assert [tile.tile_id for tile in resolution.state.players[0].concealed_tiles] == ["w3#keep"]


def test_claim_window_timeout_auto_passes_unresolved_claims() -> None:
    now = datetime(2026, 1, 1, tzinfo=timezone.utc)
    discard = _make_suit_tile("t3", "t3#discard")
    state = RoundState(
        round_id="round-timeout",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=tuple(
            PlayerState(
                seat=seat,
                concealed_tiles=(),
                melds=(),
                flowers=(),
                discards=(discard,) if seat == 0 else (),
            )
            for seat in range(4)
        ),
        last_discard=discard,
        pending_action={
            "type": "claim_window",
            "discarder_seat": 0,
            "claim_window": [[], ["pung"], ["hu"], []],
            "responded_seats": [1],
        },
        phase="playing",
        settlement=None,
        version=0,
    )

    pending_timeout = schedule_claim_window_timeout(state=state, now=now)

    assert pending_timeout.deadline_at - now == timedelta(seconds=CLAIM_WINDOW_TIMEOUT_SECONDS)

    resolution = resolve_timeout(state=state, pending_timeout=pending_timeout)

    assert resolution.state.pending_action is None
    assert resolution.state.current_actor == 1
    assert resolution.messages == [
        {
            "type": "round_event",
            "payload": {
                "event_type": "claim_auto_passed",
                "event": {
                    "type": "claim_auto_passed",
                    "discarder_seat": 0,
                    "seats": [2],
                },
            },
        }
    ]
    assert resolution.room_snapshot_required is True


def test_claim_window_timeout_auto_passes_rob_kong_window() -> None:
    now = datetime(2026, 1, 1, tzinfo=timezone.utc)
    replacement_tile = _make_suit_tile("t9", "t9#replacement")
    robbed_tile = _make_suit_tile("t5", "t5#add")
    actor = PlayerState(
        seat=0,
        concealed_tiles=(
            robbed_tile,
            _make_suit_tile("w2", "w2#a"),
        ),
        melds=((_make_suit_tile("t5", "t5#m1"), _make_suit_tile("t5", "t5#m2"), _make_suit_tile("t5", "t5#m3")),),
        flowers=(),
        discards=(),
    )
    state = RoundState(
        round_id="round-rob-kong",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(replacement_tile,), head_index=0, tail_index=0),
        players=(actor,) + tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(1, 4)
        ),
        last_discard=robbed_tile,
        pending_action={
            "type": "rob_kong_window",
            "actor_seat": 0,
            "tile_id": "t5#add",
            "tile_key": "t5",
            "meld_index": 0,
            "offered_hu_seats": [1],
            "responded_seats": [],
        },
        phase="playing",
        settlement=None,
        version=0,
        score_trackers={"kong_entries": []},
        last_action_context=None,
    )

    pending_timeout = schedule_claim_window_timeout(state=state, now=now)
    resolution = resolve_timeout(state=state, pending_timeout=pending_timeout)

    assert resolution.state.pending_action is None
    assert len(resolution.state.players[0].melds[0]) == 4
    assert any(
        message["payload"]["event_type"] == "replacement_draw"
        for message in resolution.messages
    )


def test_round_seed_is_not_derived_deterministically_from_table_code(test_app) -> None:
    game_service = test_app.state.game_service

    assert game_service._round_seed("ROOM0003") != game_service._round_seed("ROOM0003")


@pytest.mark.anyio
async def test_disconnect_updates_do_not_overwrite_reconnect_presence(test_app, monkeypatch) -> None:
    game_service = test_app.state.game_service
    session_factory = test_app.state.session_factory
    with session_factory() as session:
        create_table_record(session, table_code="ROOM0004")

    ws_0 = FakeWebSocket()
    ws_1 = FakeWebSocket()
    await game_service.join_table(table_code="ROOM0004", nickname="P0", websocket=ws_0)
    await game_service.join_table(table_code="ROOM0004", nickname="P1", websocket=ws_1)
    reconnect_token = ws_0.messages[0]["payload"]["reconnect_token"]
    ws_1.messages.clear()

    started = asyncio.Event()
    release = asyncio.Event()
    original_send_disconnect_updates = game_service._send_disconnect_updates

    async def delayed_disconnect_updates(**kwargs) -> None:
        started.set()
        await release.wait()
        await original_send_disconnect_updates(**kwargs)

    monkeypatch.setattr(
        game_service,
        "_send_disconnect_updates",
        delayed_disconnect_updates,
    )

    await game_service.disconnect("ROOM0004", ws_0)
    await started.wait()

    ws_reconnect = FakeWebSocket()
    await game_service.reconnect(
        table_code="ROOM0004",
        token=reconnect_token,
        websocket=ws_reconnect,
    )

    release.set()
    await asyncio.sleep(0)

    peer_snapshots = [
        message
        for message in ws_1.messages
        if message["type"] == "room_snapshot"
    ]

    assert peer_snapshots[-1]["payload"]["seats"][0]["connected"] is True


@pytest.mark.anyio
async def test_next_timeout_seconds_clamps_due_timeouts_above_zero(test_app) -> None:
    game_service = test_app.state.game_service
    room = game_service._rooms["ROOM0005"] = RoomState(
        table_code="ROOM0005",
        phase="playing",
        pending_timeout=PendingTimeout(
            kind="active_turn",
            seat_index=0,
            deadline_at=datetime.now(timezone.utc),
            drawn_tile_id="tile-1",
        ),
    )

    timeout_seconds = await game_service.next_timeout_seconds("ROOM0005")

    assert timeout_seconds is not None
    assert timeout_seconds > 0
