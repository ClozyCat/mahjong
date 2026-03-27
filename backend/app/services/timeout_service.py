from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from typing import Literal

from app.domain.models import RoundState
from app.domain.reducer import (
    apply_flower_action,
    apply_opening_flowers_pass,
    discard_tile,
    resolve_recorded_claims,
)

MATCH_OPERATION_TIMEOUT_SECONDS = 10 * 60
ACTIVE_TURN_TIMEOUT_SECONDS = MATCH_OPERATION_TIMEOUT_SECONDS
CLAIM_WINDOW_TIMEOUT_SECONDS = MATCH_OPERATION_TIMEOUT_SECONDS

TimeoutKind = Literal["active_turn", "claim_window", "opening_flowers"]


@dataclass(frozen=True)
class PendingTimeout:
    kind: TimeoutKind
    seat_index: int
    deadline_at: datetime
    drawn_tile_id: str | None = None


@dataclass(frozen=True)
class TimeoutResolution:
    state: RoundState
    messages: list[dict]
    room_snapshot_required: bool
    next_timeout: PendingTimeout | None = None


def schedule_active_turn_timeout(
    *,
    state: RoundState,
    drawn_tile_id: str,
    now: datetime | None = None,
) -> PendingTimeout:
    return PendingTimeout(
        kind="active_turn",
        seat_index=state.current_actor,
        deadline_at=_resolve_now(now) + timedelta(seconds=ACTIVE_TURN_TIMEOUT_SECONDS),
        drawn_tile_id=drawn_tile_id,
    )


def schedule_claim_window_timeout(
    *,
    state: RoundState,
    now: datetime | None = None,
) -> PendingTimeout:
    pending_action = state.pending_action
    if pending_action is None or pending_action.get("type") not in {"claim_window", "rob_kong_window"}:
        raise ValueError("No active claim window")

    discarder_seat = pending_action.get("discarder_seat")
    if pending_action.get("type") == "rob_kong_window":
        discarder_seat = pending_action.get("actor_seat")
    if not isinstance(discarder_seat, int):
        raise ValueError("Discarder seat required for claim window timeout")

    return PendingTimeout(
        kind="claim_window",
        seat_index=discarder_seat,
        deadline_at=_resolve_now(now) + timedelta(seconds=CLAIM_WINDOW_TIMEOUT_SECONDS),
    )


def schedule_opening_flowers_timeout(
    *,
    state: RoundState,
    now: datetime | None = None,
) -> PendingTimeout:
    pending_action = state.pending_action or {}
    if pending_action.get("type") != "opening_flowers":
        raise ValueError("No active opening flower declaration")

    flower_tile_id = next(
        (
            tile.tile_id
            for tile in state.players[state.current_actor].concealed_tiles
            if tile.kind == "flower"
        ),
        None,
    )
    return PendingTimeout(
        kind="opening_flowers",
        seat_index=state.current_actor,
        deadline_at=_resolve_now(now) + timedelta(seconds=ACTIVE_TURN_TIMEOUT_SECONDS),
        drawn_tile_id=flower_tile_id,
    )


def resolve_timeout(
    *,
    state: RoundState,
    pending_timeout: PendingTimeout,
) -> TimeoutResolution:
    if pending_timeout.kind == "active_turn":
        return _resolve_active_turn_timeout(state=state, pending_timeout=pending_timeout)
    if pending_timeout.kind == "claim_window":
        return _resolve_claim_window_timeout(state=state, pending_timeout=pending_timeout)
    if pending_timeout.kind == "opening_flowers":
        return _resolve_opening_flowers_timeout(state=state, pending_timeout=pending_timeout)
    raise ValueError(f"Unsupported timeout kind: {pending_timeout.kind}")


def _resolve_active_turn_timeout(
    *,
    state: RoundState,
    pending_timeout: PendingTimeout,
) -> TimeoutResolution:
    if pending_timeout.seat_index != state.current_actor:
        raise ValueError("Timeout seat no longer matches current actor")
    if pending_timeout.drawn_tile_id is None:
        raise ValueError("Active turn timeout requires the most recently drawn tile")

    next_state, events = discard_tile(
        state,
        seat=state.current_actor,
        tile_id=pending_timeout.drawn_tile_id,
    )
    next_timeout = None
    if next_state.pending_action is not None:
        next_timeout = schedule_claim_window_timeout(state=next_state)

    return TimeoutResolution(
        state=next_state,
        messages=[_round_event_message(event) for event in events],
        room_snapshot_required=True,
        next_timeout=next_timeout,
    )


def _resolve_claim_window_timeout(
    *,
    state: RoundState,
    pending_timeout: PendingTimeout,
) -> TimeoutResolution:
    pending_action = state.pending_action
    if pending_action is None or pending_action.get("type") not in {"claim_window", "rob_kong_window"}:
        raise ValueError("No active claim window")
    if pending_action.get("type") == "rob_kong_window":
        return _resolve_rob_kong_timeout(state=state, pending_timeout=pending_timeout)

    discarder_seat = pending_action.get("discarder_seat")
    if not isinstance(discarder_seat, int):
        raise ValueError("Discarder seat required for claim resolution")
    if pending_timeout.seat_index != discarder_seat:
        raise ValueError("Timeout seat no longer matches the discarder")

    unresolved_seats = [
        seat_index
        for seat_index, claims in enumerate(pending_action.get("claim_window", []))
        if claims and seat_index not in set(pending_action.get("responded_seats", []))
    ]
    next_state, resolution_events = resolve_recorded_claims(
        RoundState(
            round_id=state.round_id,
            dealer_seat=state.dealer_seat,
            current_actor=state.current_actor,
            wall=state.wall,
            players=state.players,
            last_discard=state.last_discard,
            pending_action={
                **pending_action,
                "responded_seats": sorted(
                    set(pending_action.get("responded_seats", [])) | set(unresolved_seats)
                ),
                "claim_responses": list(pending_action.get("claim_responses", [])),
            },
            phase=state.phase,
            settlement=state.settlement,
            version=state.version,
            score_trackers=state.score_trackers,
            last_action_context=state.last_action_context,
            round_wind=state.round_wind,
        )
    )

    return TimeoutResolution(
        state=next_state,
        messages=[
            _round_event_message(
                {
                    "type": "claim_auto_passed",
                    "discarder_seat": discarder_seat,
                    "seats": unresolved_seats,
                }
            )
        ] + [_round_event_message(event) for event in resolution_events],
        room_snapshot_required=True,
    )


def _resolve_opening_flowers_timeout(
    *,
    state: RoundState,
    pending_timeout: PendingTimeout,
) -> TimeoutResolution:
    pending_action = state.pending_action or {}
    if pending_action.get("type") != "opening_flowers":
        raise ValueError("No active opening flower declaration")
    if pending_timeout.seat_index != state.current_actor:
        raise ValueError("Timeout seat no longer matches the current actor")
    if pending_timeout.drawn_tile_id is None:
        next_state, events = apply_opening_flowers_pass(
            state,
            seat=state.current_actor,
        )
    else:
        next_state, events = apply_flower_action(
            state,
            seat=state.current_actor,
            tile_ids=[pending_timeout.drawn_tile_id],
        )

    return TimeoutResolution(
        state=next_state,
        messages=[_round_event_message(event) for event in events],
        room_snapshot_required=True,
    )


def _resolve_rob_kong_timeout(
    *,
    state: RoundState,
    pending_timeout: PendingTimeout,
) -> TimeoutResolution:
    pending_action = state.pending_action
    assert pending_action is not None
    actor_seat = pending_action.get("actor_seat")
    if not isinstance(actor_seat, int):
        raise ValueError("Actor seat required for rob-kong resolution")
    if pending_timeout.seat_index != actor_seat:
        raise ValueError("Timeout seat no longer matches the kong actor")

    unresolved_seats = [
        seat_index
        for seat_index in pending_action.get("offered_hu_seats", [])
        if seat_index not in set(pending_action.get("responded_seats", []))
    ]
    next_state, events = resolve_recorded_claims(
        RoundState(
            round_id=state.round_id,
            dealer_seat=state.dealer_seat,
            current_actor=state.current_actor,
            wall=state.wall,
            players=state.players,
            last_discard=state.last_discard,
            pending_action={
                **pending_action,
                "responded_seats": sorted(
                    set(pending_action.get("responded_seats", [])) | set(unresolved_seats)
                ),
                "claim_responses": list(pending_action.get("claim_responses", [])),
            },
            phase=state.phase,
            settlement=state.settlement,
            version=state.version,
            score_trackers=state.score_trackers,
            last_action_context=state.last_action_context,
            round_wind=state.round_wind,
        )
    )

    messages = [
        _round_event_message(
            {
                "type": "rob_kong_auto_passed",
                "actor_seat": actor_seat,
                "seats": unresolved_seats,
            }
        )
    ] + [_round_event_message(event) for event in events]

    return TimeoutResolution(
        state=next_state,
        messages=messages,
        room_snapshot_required=True,
    )


def _round_event_message(event: dict) -> dict:
    return {
        "type": "round_event",
        "payload": {
            "event_type": event["type"],
            "event": event,
        },
    }


def _resolve_now(now: datetime | None) -> datetime:
    return now or datetime.now(timezone.utc)
