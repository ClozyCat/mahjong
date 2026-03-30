from __future__ import annotations

from app.domain.actions import (
    claim_made_event,
    flower_exposed_event,
    replacement_draw_event,
    self_kong_declared_event,
    tile_discarded_event,
    tile_drawn_event,
)
from app.domain.hand_eval import decompose_winning_hand
from app.domain.claims import compute_claim_window, is_valid_chow_sequence, resolve_claims
from app.domain.fan_eval import evaluate_fans
from app.domain.hand_features import extract_hand_features
from app.domain.hand_eval import is_winning_hand
from app.domain.models import PlayerState, RoundState, Tile
from app.domain.wall import WallState, build_wall, draw_live_tile, draw_replacement_tile

PLAYER_COUNT = 4
TILES_PER_PLAYER = 13
MINIMUM_WINNING_FAN = 8
WIND_ORDER = ("east", "south", "west", "north")


def _draw_live_tile_once(
    wall: WallState,
) -> tuple[Tile, WallState]:
    return draw_live_tile(wall)


def _draw_replacement_tile_once(
    wall: WallState,
) -> tuple[Tile, WallState]:
    return draw_replacement_tile(wall)


def _draw_tile_handling_flowers(
    wall: WallState,
) -> tuple[Tile, WallState, list[Tile], list[Tile]]:
    exposures: list[Tile] = []
    replacements: list[Tile] = []

    tile, wall = draw_live_tile(wall)

    while tile.kind == "flower":
        exposures.append(tile)
        tile, wall = draw_replacement_tile(wall)
        replacements.append(tile)

    return tile, wall, exposures, replacements


def _draw_replacement_handling_flowers(
    wall: WallState,
) -> tuple[Tile, WallState, list[Tile], list[Tile]]:
    exposures: list[Tile] = []
    replacements: list[Tile] = []

    tile, wall = draw_replacement_tile(wall)

    while tile.kind == "flower":
        exposures.append(tile)
        tile, wall = draw_replacement_tile(wall)
        replacements.append(tile)

    return tile, wall, exposures, replacements


def initialize_round(
    seed: int | None = None,
    *,
    dealer_seat: int = 0,
    round_id: str | None = None,
    round_wind: str = "east",
    enforce_minimum_eight_fan: bool = True,
) -> RoundState:
    wall = build_wall(seed)
    players: list[PlayerState] = []

    for seat in range(PLAYER_COUNT):
        concealed_tiles: list[Tile] = []
        for _ in range(TILES_PER_PLAYER):
            tile, wall = _draw_live_tile_once(wall)
            concealed_tiles.append(tile)

        players.append(
            PlayerState(
                seat=seat,
                concealed_tiles=tuple(concealed_tiles),
                melds=(),
                flowers=(),
                discards=(),
            )
        )

    resolved_round_id = round_id or f"round-{seed if seed is not None else 0}"

    return RoundState(
        round_id=resolved_round_id,
        dealer_seat=dealer_seat,
        current_actor=dealer_seat,
        wall=wall,
        players=tuple(players),
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
        score_trackers=_empty_score_trackers(),
        last_action_context=None,
        round_wind=round_wind,
        enforce_minimum_eight_fan=enforce_minimum_eight_fan,
    )


def _replace_round_state(state: RoundState, **changes) -> RoundState:
    payload = {
        "round_id": state.round_id,
        "dealer_seat": state.dealer_seat,
        "current_actor": state.current_actor,
        "wall": state.wall,
        "players": state.players,
        "last_discard": state.last_discard,
        "pending_action": state.pending_action,
        "phase": state.phase,
        "settlement": state.settlement,
        "version": state.version,
        "score_trackers": state.score_trackers,
        "last_action_context": state.last_action_context,
        "round_wind": state.round_wind,
        "enforce_minimum_eight_fan": state.enforce_minimum_eight_fan,
    }
    payload.update(changes)
    return RoundState(**payload)


def _opening_flowers_completed(state: RoundState) -> bool:
    trackers = state.score_trackers or {}
    return bool(trackers.get("opening_flowers_completed", False))


def _set_opening_flowers_completed(score_trackers: dict | None, completed: bool) -> dict:
    trackers = dict(score_trackers or _empty_score_trackers())
    trackers["opening_flowers_completed"] = completed
    return trackers


def _seat_with_opening_flower(state: RoundState, start_seat: int) -> int | None:
    seat_count = len(state.players)
    for offset in range(seat_count):
        seat = (start_seat + offset) % seat_count
        if any(tile.kind == "flower" for tile in state.players[seat].concealed_tiles):
            return seat
    return None


def _start_opening_flowers_if_needed(state: RoundState) -> RoundState:
    if _opening_flowers_completed(state):
        return state

    if _seat_with_opening_flower(state, state.dealer_seat) is None:
        return _replace_round_state(
            state,
            score_trackers=_set_opening_flowers_completed(state.score_trackers, True),
        )

    return _replace_round_state(
        state,
        current_actor=state.dealer_seat,
        pending_action={
            "type": "opening_flowers",
            "dealer_seat": state.dealer_seat,
        },
        score_trackers=_set_opening_flowers_completed(state.score_trackers, False),
    )


def _advance_opening_flowers_or_finish(
    state: RoundState,
    *,
    seat: int,
) -> RoundState:
    if any(tile.kind == "flower" for tile in state.players[seat].concealed_tiles):
        return _replace_round_state(
            state,
            current_actor=seat,
            pending_action={
                "type": "opening_flowers",
                "dealer_seat": state.dealer_seat,
            },
        )

    next_seat = (seat + 1) % len(state.players)
    if next_seat == state.dealer_seat:
        return _replace_round_state(
            state,
            current_actor=state.dealer_seat,
            pending_action=None,
            score_trackers=_set_opening_flowers_completed(state.score_trackers, True),
        )

    return _replace_round_state(
        state,
        current_actor=next_seat,
        pending_action={
            "type": "opening_flowers",
            "dealer_seat": state.dealer_seat,
        },
    )


def draw_for_turn(state: RoundState) -> tuple[RoundState, list[dict]]:
    if state.pending_action is not None:
        raise ValueError("Cannot draw while a pending action is unresolved")
    seat = state.current_actor
    player = state.players[seat]
    wall = state.wall
    events: list[dict] = []

    try:
        tile, wall = _draw_live_tile_once(wall)
    except IndexError:
        return settle_exhaustive_draw(state)
    events.append(tile_drawn_event(seat, tile))

    new_concealed = player.concealed_tiles + (tile,)

    updated_player = PlayerState(
        seat=seat,
        concealed_tiles=new_concealed,
        melds=player.melds,
        flowers=player.flowers,
        discards=player.discards,
    )

    new_players = tuple(
        updated_player if idx == seat else current_player
        for idx, current_player in enumerate(state.players)
    )

    new_state = RoundState(
        round_id=state.round_id,
        dealer_seat=state.dealer_seat,
        current_actor=seat,
        wall=wall,
        players=new_players,
        last_discard=state.last_discard,
        pending_action=state.pending_action,
        phase=state.phase,
        settlement=state.settlement,
        version=state.version + 1,
        score_trackers=state.score_trackers,
        last_action_context={
            "kind": "draw",
            "seat": seat,
            "tile_id": tile.tile_id,
            "from_kong_replacement": False,
            "was_last_live_tile": wall.head_index > wall.tail_index,
            "was_last_discard": False,
        },
        round_wind=state.round_wind,
        enforce_minimum_eight_fan=state.enforce_minimum_eight_fan,
    )

    return new_state, events


def can_declare_flower(state: RoundState, seat: int) -> bool:
    if state.phase != "playing":
        return False
    if seat != state.current_actor:
        return False
    if _is_last_tile_wall_point_for_actor(state):
        return False
    pending_action = state.pending_action or {}
    if pending_action and pending_action.get("type") != "opening_flowers":
        return False
    return any(tile.kind == "flower" for tile in state.players[seat].concealed_tiles)


def apply_opening_flowers_pass(
    state: RoundState,
    seat: int,
) -> tuple[RoundState, list[dict]]:
    pending_action = state.pending_action or {}
    if pending_action.get("type") != "opening_flowers":
        raise ValueError("No opening flower declaration is active")
    if seat != state.current_actor:
        raise ValueError("Only the current actor may resolve opening flowers")
    if any(tile.kind == "flower" for tile in state.players[seat].concealed_tiles):
        raise ValueError("Seats with flowers must declare them before passing")
    return _advance_opening_flowers_or_finish(state, seat=seat), []


def discard_tile(
    state: RoundState, seat: int, tile_id: str
) -> tuple[RoundState, list[dict]]:
    if seat != state.current_actor:
        raise ValueError("Only the current actor may discard")
    if state.pending_action is not None:
        raise ValueError("Cannot discard while a pending action is unresolved")

    player = state.players[seat]
    concealed_list = list(player.concealed_tiles)

    tile_index = next(
        (idx for idx, tile in enumerate(concealed_list) if tile.tile_id == tile_id),
        None,
    )
    if tile_index is None:
        raise ValueError("Tile not found in concealed hand")

    tile = concealed_list.pop(tile_index)
    new_concealed = tuple(concealed_list)
    new_discards = player.discards + (tile,)

    updated_player = PlayerState(
        seat=seat,
        concealed_tiles=new_concealed,
        melds=player.melds,
        flowers=player.flowers,
        discards=new_discards,
    )

    new_players = tuple(
        updated_player if idx == seat else current_player
        for idx, current_player in enumerate(state.players)
    )

    base_state = RoundState(
        round_id=state.round_id,
        dealer_seat=state.dealer_seat,
        current_actor=seat,
        wall=state.wall,
        players=new_players,
        last_discard=tile,
        pending_action=None,
        phase=state.phase,
        settlement=state.settlement,
        version=state.version + 1,
        score_trackers=state.score_trackers,
        last_action_context={
            "kind": "discard",
            "seat": seat,
            "tile_id": tile.tile_id,
            "from_kong_replacement": False,
            "was_last_live_tile": False,
            "was_last_discard": bool(
                (state.last_action_context or {}).get("kind") == "draw"
                and (state.last_action_context or {}).get("was_last_live_tile", False)
            ),
        },
        round_wind=state.round_wind,
        enforce_minimum_eight_fan=state.enforce_minimum_eight_fan,
    )

    claim_window = compute_claim_window(base_state)
    has_claim = any(claims for claims in claim_window.values())
    if has_claim:
        serializable_window = [
            sorted(list(claim_window[seat])) for seat in range(len(state.players))
        ]
        pending_action = {
            "type": "claim_window",
            "discarder_seat": seat,
            "claim_window": serializable_window,
            "responded_seats": [],
        }
        new_state = RoundState(
            round_id=base_state.round_id,
            dealer_seat=base_state.dealer_seat,
            current_actor=seat,
            wall=base_state.wall,
            players=base_state.players,
            last_discard=base_state.last_discard,
            pending_action=pending_action,
            phase=base_state.phase,
            settlement=base_state.settlement,
            version=base_state.version,
            score_trackers=base_state.score_trackers,
            last_action_context=base_state.last_action_context,
            round_wind=base_state.round_wind,
            enforce_minimum_eight_fan=base_state.enforce_minimum_eight_fan,
        )
    else:
        next_actor = (seat + 1) % len(state.players)
        new_state = RoundState(
            round_id=base_state.round_id,
            dealer_seat=base_state.dealer_seat,
            current_actor=next_actor,
            wall=base_state.wall,
            players=base_state.players,
            last_discard=base_state.last_discard,
            pending_action=None,
            phase=base_state.phase,
            settlement=base_state.settlement,
            version=base_state.version,
            score_trackers=base_state.score_trackers,
            last_action_context=base_state.last_action_context,
            round_wind=base_state.round_wind,
            enforce_minimum_eight_fan=base_state.enforce_minimum_eight_fan,
        )

    events = [tile_discarded_event(seat, tile)]

    return new_state, events


def apply_claim_action(
    state: RoundState, seat: int, action_type: str, tiles: list[str]
) -> tuple[RoundState, list[dict]]:
    if action_type not in {"chow", "pung", "kong", "pass", "hu"}:
        raise ValueError("Unsupported claim action")

    if state.pending_action is None:
        raise ValueError("No active claim window")
    pending = state.pending_action
    pending_type = pending.get("type")
    responded_seats = set(pending.get("responded_seats", []))
    if seat in responded_seats:
        raise ValueError("Seat has already responded to this claim window")

    if pending_type == "rob_kong_window":
        offered = set(pending.get("offered_hu_seats", []))
        if seat not in offered:
            raise ValueError("Claim not available for this seat")
        if action_type not in {"hu", "pass"}:
            raise ValueError("Unsupported claim action")
        return _record_claim_response(state, seat=seat, action_type=action_type, tiles=tiles)

    if pending_type != "claim_window":
        raise ValueError("No active claim window")

    claim_window = pending.get("claim_window", [])
    allowed_claims = set(claim_window[seat]) if seat < len(claim_window) else set()
    if not allowed_claims:
        raise ValueError("Pass is only allowed when a claim is available")
    if action_type != "pass" and action_type not in allowed_claims:
        raise ValueError("Claim not available for this seat")

    if action_type in {"chow", "pung", "kong"}:
        _validate_claim_selection(state, seat=seat, action_type=action_type, tiles=tiles)

    return _record_claim_response(state, seat=seat, action_type=action_type, tiles=tiles)


def resolve_recorded_claims(state: RoundState) -> tuple[RoundState, list[dict]]:
    pending = state.pending_action or {}
    pending_type = pending.get("type")

    if pending_type == "claim_window":
        discarder_seat = pending.get("discarder_seat")
        if not isinstance(discarder_seat, int):
            raise ValueError("Discarder seat required for claim resolution")
        winner = resolve_claims(list(pending.get("claim_responses", [])), discarder_seat)
        if winner is None:
            return (
                _replace_round_state(
                    state,
                    current_actor=(discarder_seat + 1) % len(state.players),
                    pending_action=None,
                    version=state.version + 1,
                ),
                [],
            )
        if winner["type"] == "hu":
            return apply_discard_win(state, winner_seat=winner["seat"])
        return _apply_selected_claim(
            state,
            seat=winner["seat"],
            action_type=winner["type"],
            tiles=list(winner.get("tiles", [])),
        )

    if pending_type == "rob_kong_window":
        actor_seat = pending.get("actor_seat")
        if not isinstance(actor_seat, int):
            raise ValueError("Actor seat required for rob-kong resolution")
        winner = resolve_claims(
            [
                {"seat": response["seat"], "type": "hu", "tiles": []}
                for response in pending.get("claim_responses", [])
                if response.get("type") == "hu"
            ],
            actor_seat,
        )
        if winner is None:
            return _complete_add_kong_after_passes(state)
        return apply_discard_win(state, winner_seat=winner["seat"])

    raise ValueError("No active claim window")


def _record_claim_response(
    state: RoundState,
    *,
    seat: int,
    action_type: str,
    tiles: list[str],
) -> tuple[RoundState, list[dict]]:
    pending = dict(state.pending_action or {})
    responded_seats = set(pending.get("responded_seats", []))
    responded_seats.add(seat)

    claim_responses = list(pending.get("claim_responses", []))
    if action_type != "pass":
        claim_responses.append(
            {
                "seat": seat,
                "type": action_type,
                "tiles": list(tiles),
            }
        )

    pending["responded_seats"] = sorted(responded_seats)
    pending["claim_responses"] = claim_responses
    updated_state = _replace_round_state(
        state,
        pending_action=pending,
        version=state.version + 1,
    )

    offered_seats = _offered_claim_seats(pending)
    unresolved = [seat_index for seat_index in offered_seats if seat_index not in responded_seats]
    if unresolved:
        return updated_state, []
    return resolve_recorded_claims(updated_state)


def _offered_claim_seats(pending_action: dict) -> list[int]:
    if pending_action.get("type") == "rob_kong_window":
        return list(pending_action.get("offered_hu_seats", []))
    return [
        seat_index
        for seat_index, claims in enumerate(pending_action.get("claim_window", []))
        if claims
    ]


def _validate_claim_selection(
    state: RoundState,
    *,
    seat: int,
    action_type: str,
    tiles: list[str],
) -> None:
    if state.last_discard is None:
        raise ValueError("No discard available to claim")

    expected_tiles = {"chow": 2, "pung": 2, "kong": 3}
    if len(tiles) != expected_tiles[action_type]:
        raise ValueError("Incorrect number of tiles for claim")

    player = state.players[seat]
    concealed_list = list(player.concealed_tiles)
    claimed_tiles: list[Tile] = []

    for tile_id in tiles:
        tile_index = next(
            (idx for idx, tile in enumerate(concealed_list) if tile.tile_id == tile_id),
            None,
        )
        if tile_index is None:
            raise ValueError("Tile not found in concealed hand")
        claimed_tiles.append(concealed_list.pop(tile_index))

    if action_type in {"pung", "kong"} and any(
        tile.tile_key != state.last_discard.tile_key for tile in claimed_tiles
    ):
        raise ValueError("Invalid pung/kong tiles")

    if action_type == "chow" and not is_valid_chow_sequence(
        state.last_discard, claimed_tiles
    ):
        raise ValueError("Invalid chow sequence")


def _apply_selected_claim(
    state: RoundState,
    *,
    seat: int,
    action_type: str,
    tiles: list[str],
) -> tuple[RoundState, list[dict]]:
    pending = state.pending_action or {}
    discarder_seat = pending.get("discarder_seat")
    if not isinstance(discarder_seat, int):
        raise ValueError("Discarder seat required for claim resolution")
    if state.last_discard is None:
        raise ValueError("No discard available to claim")

    player = state.players[seat]
    concealed_list = list(player.concealed_tiles)
    claimed_tiles: list[Tile] = []

    for tile_id in tiles:
        tile_index = next(
            (idx for idx, tile in enumerate(concealed_list) if tile.tile_id == tile_id),
            None,
        )
        if tile_index is None:
            raise ValueError("Tile not found in concealed hand")
        claimed_tiles.append(concealed_list.pop(tile_index))

    meld = tuple(claimed_tiles + [state.last_discard])
    updated_player = PlayerState(
        seat=player.seat,
        concealed_tiles=tuple(concealed_list),
        melds=player.melds + (meld,),
        flowers=player.flowers,
        discards=player.discards,
    )

    new_players = [
        updated_player if idx == seat else current_player
        for idx, current_player in enumerate(state.players)
    ]
    wall = state.wall
    events = [claim_made_event(seat, action_type, state.last_discard)]

    if action_type == "kong":
        try:
            tile, wall = _draw_replacement_tile_once(wall)
        except IndexError:
            replacement_state = _replace_round_state(
                state,
                current_actor=seat,
                players=tuple(new_players),
                pending_action=None,
                version=state.version + 1,
                score_trackers=_append_kong_entry(
                    state.score_trackers,
                    kong_type="exposed_kong",
                    actor_seat=seat,
                    payer_seats=[discarder_seat],
                    tile_key=state.last_discard.tile_key,
                ),
                last_action_context={
                    "kind": "replacement_draw",
                    "seat": seat,
                    "tile_id": None,
                    "from_kong_replacement": True,
                    "was_last_live_tile": False,
                    "was_last_discard": False,
                },
            )
            settled_state, settled_events = settle_exhaustive_draw(replacement_state)
            return settled_state, events + settled_events

        updated_player = PlayerState(
            seat=updated_player.seat,
            concealed_tiles=updated_player.concealed_tiles + (tile,),
            melds=updated_player.melds,
            flowers=updated_player.flowers,
            discards=updated_player.discards,
        )
        new_players = [
            updated_player if idx == seat else current_player
            for idx, current_player in enumerate(new_players)
        ]
        events.append(replacement_draw_event(seat, tile))

    return (
        _replace_round_state(
            state,
            current_actor=seat,
            wall=wall,
            players=tuple(new_players),
            pending_action=None,
            version=state.version + 1,
            score_trackers=_append_kong_entry(
                state.score_trackers,
                kong_type="exposed_kong",
                actor_seat=seat,
                payer_seats=[discarder_seat],
                tile_key=state.last_discard.tile_key,
            )
            if action_type == "kong"
            else state.score_trackers,
            last_action_context={
                "kind": "replacement_draw",
                "seat": seat,
                "tile_id": tile.tile_id,
                "from_kong_replacement": True,
                "was_last_live_tile": False,
                "was_last_discard": False,
            }
            if action_type == "kong"
            else state.last_action_context,
        ),
        events,
    )


def can_declare_self_kong(state: RoundState, seat: int) -> bool:
    if state.phase != "playing" or state.pending_action is not None:
        return False
    if seat != state.current_actor:
        return False
    if _is_last_tile_wall_point_for_actor(state):
        return False
    return bool(_available_self_kongs(state.players[seat]))


def apply_self_kong_action(
    state: RoundState,
    seat: int,
    tile_ids: list[str],
) -> tuple[RoundState, list[dict]]:
    if state.phase != "playing":
        raise ValueError("Round is not in a playable phase")
    if state.pending_action is not None:
        raise ValueError("Self kong is not available during a claim window")
    if seat != state.current_actor:
        raise ValueError("Only the current actor may declare a kong")
    if _is_last_tile_wall_point_for_actor(state):
        raise ValueError("Self kong is not available after the last live wall tile")

    player = state.players[seat]
    kong_type, selected_tiles, meld_index = _resolve_self_kong_selection(player, tile_ids)
    if kong_type == "add_kong":
        rob_kong_state = _maybe_start_rob_kong_window(
            state,
            seat=seat,
            selected_tile=selected_tiles[0],
            meld_index=meld_index,
        )
        if rob_kong_state is not None:
            return (
                rob_kong_state,
                [
                    self_kong_declared_event(
                        seat,
                        kong_type,
                        selected_tiles[0].tile_key,
                        [tile.tile_id for tile in selected_tiles],
                    )
                ],
            )
    return _complete_self_kong(
        state,
        seat=seat,
        kong_type=kong_type,
        selected_tiles=selected_tiles,
        meld_index=meld_index,
    )


def apply_flower_action(
    state: RoundState,
    seat: int,
    tile_ids: list[str],
) -> tuple[RoundState, list[dict]]:
    if state.phase != "playing":
        raise ValueError("Round is not in a playable phase")
    pending_action = state.pending_action or {}
    if pending_action and pending_action.get("type") != "opening_flowers":
        raise ValueError("Flower declaration is not available during a claim window")
    if seat != state.current_actor:
        raise ValueError("Only the current actor may declare a flower")
    if _is_last_tile_wall_point_for_actor(state):
        raise ValueError("Flower declaration is not available after the last live wall tile")
    if len(tile_ids) != 1:
        raise ValueError("Flower declaration requires selecting exactly one tile")

    player = state.players[seat]
    concealed_list = list(player.concealed_tiles)
    tile_index = next(
        (idx for idx, tile in enumerate(concealed_list) if tile.tile_id == tile_ids[0]),
        None,
    )
    if tile_index is None:
        raise ValueError("Tile not found in concealed hand")
    flower_tile = concealed_list.pop(tile_index)
    if flower_tile.kind != "flower":
        raise ValueError("Selected tile is not a flower")

    updated_player = PlayerState(
        seat=player.seat,
        concealed_tiles=tuple(concealed_list),
        melds=player.melds,
        flowers=player.flowers + (flower_tile,),
        discards=player.discards,
    )
    new_players = tuple(
        updated_player if idx == seat else current_player
        for idx, current_player in enumerate(state.players)
    )
    events = [flower_exposed_event(seat, flower_tile)]
    base_state = _replace_round_state(
        state,
        players=new_players,
        version=state.version + 1,
        last_action_context={
            "kind": "flower",
            "seat": seat,
            "tile_id": flower_tile.tile_id,
            "from_kong_replacement": False,
            "was_last_live_tile": False,
            "was_last_discard": False,
        },
    )

    try:
        replacement_tile, wall = _draw_replacement_tile_once(state.wall)
    except IndexError:
        settled_state, settled_events = settle_exhaustive_draw(base_state)
        return settled_state, events + settled_events

    updated_player = PlayerState(
        seat=updated_player.seat,
        concealed_tiles=updated_player.concealed_tiles + (replacement_tile,),
        melds=updated_player.melds,
        flowers=updated_player.flowers,
        discards=updated_player.discards,
    )
    new_players = tuple(
        updated_player if idx == seat else current_player
        for idx, current_player in enumerate(new_players)
    )
    events.append(replacement_draw_event(seat, replacement_tile))

    next_state = _replace_round_state(
        base_state,
        wall=wall,
        players=new_players,
        last_action_context={
            "kind": "replacement_draw",
            "seat": seat,
            "tile_id": replacement_tile.tile_id,
            "from_kong_replacement": False,
            "was_last_live_tile": False,
            "was_last_discard": False,
        },
    )
    if pending_action.get("type") == "opening_flowers":
        next_state = _advance_opening_flowers_or_finish(next_state, seat=seat)
    return next_state, events


def can_declare_hu(state: RoundState, seat: int, incoming_tile: str | None) -> bool:
    if (state.pending_action or {}).get("type") == "opening_flowers":
        return False
    if not is_winning_hand(_player_tile_keys(state, seat=seat, incoming_tile=incoming_tile)):
        return False

    try:
        fan_result = _fan_result_for_win(
            state,
            winner_seat=seat,
            incoming_tile=incoming_tile,
        )
    except ValueError:
        return False
    if not state.enforce_minimum_eight_fan:
        return True
    return fan_result.get("minimum_qualifying_fan_total", fan_result["fan_total"]) >= MINIMUM_WINNING_FAN


def apply_self_draw_win(
    state: RoundState, winner_seat: int
) -> tuple[RoundState, list[dict]]:
    if state.phase != "playing":
        raise ValueError("Round is not in a playable phase")
    if winner_seat != state.current_actor:
        raise ValueError("Self-draw winner must be the current actor")
    if not can_declare_hu(state, winner_seat, None):
        raise ValueError("Hand is not a winning self-draw")
    player = state.players[winner_seat]
    fan_result = _fan_result_for_win(state, winner_seat=winner_seat, incoming_tile=None)
    settlement = {
        "provisional": True,
        "win_type": "self_draw",
        "winner_seat": winner_seat,
        "discarder_seat": None,
        "display_win_label": (
            "屁和"
            if not state.enforce_minimum_eight_fan and fan_result["fan_total"] < MINIMUM_WINNING_FAN
            else None
        ),
        "fan_total": fan_result["fan_total"],
        "fan_keys": fan_result["fan_keys"],
        "fan_breakdown": fan_result["fan_breakdown"],
        "score_delta": fan_result["score_delta"],
        "flower_count": len(player.flowers),
        "kong_score_detail": fan_result["kong_score_detail"],
    }
    new_state = RoundState(
        round_id=state.round_id,
        dealer_seat=state.dealer_seat,
        current_actor=winner_seat,
        wall=state.wall,
        players=state.players,
        last_discard=state.last_discard,
        pending_action=None,
        phase="settlement",
        settlement=settlement,
        version=state.version + 1,
        score_trackers=state.score_trackers,
        last_action_context=state.last_action_context,
        round_wind=state.round_wind,
        enforce_minimum_eight_fan=state.enforce_minimum_eight_fan,
    )
    events = [{"type": "settlement_ready", "round_id": state.round_id}]
    return new_state, events


def apply_discard_win(
    state: RoundState, winner_seat: int
) -> tuple[RoundState, list[dict]]:
    if state.phase != "playing":
        raise ValueError("Round is not in a playable phase")
    if state.last_discard is None:
        raise ValueError("No discard available for win")
    if state.pending_action is None or state.pending_action.get("type") not in {
        "claim_window",
        "rob_kong_window",
    }:
        raise ValueError("Discard win requires an active claim window")
    if state.pending_action.get("type") == "rob_kong_window":
        offered = set(state.pending_action.get("offered_hu_seats", []))
        if winner_seat not in offered:
            raise ValueError("Discard win not available for this seat")
        discarder_seat = state.pending_action.get("actor_seat")
    else:
        claim_window = state.pending_action.get("claim_window", [])
        offered = set(claim_window[winner_seat]) if winner_seat < len(claim_window) else set()
        if "hu" not in offered:
            raise ValueError("Discard win not available for this seat")
        discarder_seat = state.pending_action.get("discarder_seat")
    if not isinstance(discarder_seat, int):
        raise ValueError("Discarder seat required for discard win")
    if not can_declare_hu(state, winner_seat, state.last_discard.tile_key):
        raise ValueError("Hand is not a winning discard")
    player = state.players[winner_seat]
    fan_result = _fan_result_for_win(
        state,
        winner_seat=winner_seat,
        incoming_tile=state.last_discard.tile_key,
        discarder_seat=discarder_seat,
    )
    settlement = {
        "provisional": True,
        "win_type": "discard",
        "winner_seat": winner_seat,
        "discarder_seat": discarder_seat,
        "display_win_label": (
            "屁和"
            if not state.enforce_minimum_eight_fan and fan_result["fan_total"] < MINIMUM_WINNING_FAN
            else None
        ),
        "fan_total": fan_result["fan_total"],
        "fan_keys": fan_result["fan_keys"],
        "fan_breakdown": fan_result["fan_breakdown"],
        "score_delta": fan_result["score_delta"],
        "flower_count": len(player.flowers),
        "kong_score_detail": fan_result["kong_score_detail"],
    }
    new_state = RoundState(
        round_id=state.round_id,
        dealer_seat=state.dealer_seat,
        current_actor=winner_seat,
        wall=state.wall,
        players=state.players,
        last_discard=state.last_discard,
        pending_action=None,
        phase="settlement",
        settlement=settlement,
        version=state.version + 1,
        score_trackers=state.score_trackers,
        last_action_context=state.last_action_context,
        round_wind=state.round_wind,
        enforce_minimum_eight_fan=state.enforce_minimum_eight_fan,
    )
    events = [{"type": "settlement_ready", "round_id": state.round_id}]
    return new_state, events


def settle_exhaustive_draw(state: RoundState) -> tuple[RoundState, list[dict]]:
    if state.phase != "playing":
        raise ValueError("Round is not in a playable phase")
    if state.wall.head_index <= state.wall.tail_index:
        raise ValueError("Wall is not exhausted")
    settlement = {
        "provisional": True,
        "win_type": "draw",
        "winner_seat": None,
        "discarder_seat": None,
        "fan_total": 0,
        "fan_keys": [],
        "fan_breakdown": [],
        "score_delta": {
            "provisional": True,
            "fan_total": 0,
            "fan_delta_by_seat": {seat: 0 for seat in range(len(state.players))},
            "kong_delta_by_seat": _kong_delta_by_seat(state),
            "total_delta_by_seat": _kong_delta_by_seat(state),
        },
        "flower_count": 0,
        "draw_type": "exhaustive",
        "kong_score_detail": _kong_entries(state),
    }
    new_state = RoundState(
        round_id=state.round_id,
        dealer_seat=state.dealer_seat,
        current_actor=state.current_actor,
        wall=state.wall,
        players=state.players,
        last_discard=state.last_discard,
        pending_action=None,
        phase="settlement",
        settlement=settlement,
        version=state.version + 1,
        score_trackers=state.score_trackers,
        last_action_context=state.last_action_context,
        round_wind=state.round_wind,
        enforce_minimum_eight_fan=state.enforce_minimum_eight_fan,
    )
    events = [{"type": "round_drawn", "round_id": state.round_id}]
    return new_state, events


def _available_self_kongs(
    player: PlayerState,
) -> list[tuple[str, tuple[Tile, ...], int | None]]:
    available: list[tuple[str, tuple[Tile, ...], int | None]] = []
    by_key: dict[str, list[Tile]] = {}
    for tile in player.concealed_tiles:
        by_key.setdefault(tile.tile_key, []).append(tile)

    for tiles in by_key.values():
        if len(tiles) >= 4:
            available.append(("concealed_kong", tuple(tiles[:4]), None))

    for meld_index, meld in enumerate(player.melds):
        if len(meld) != 3:
            continue
        tile_keys = {tile.tile_key for tile in meld}
        if len(tile_keys) != 1:
            continue
        tile_key = next(iter(tile_keys))
        matching_tiles = by_key.get(tile_key, [])
        if matching_tiles:
            available.append(("add_kong", (matching_tiles[0],), meld_index))

    return available


def _resolve_self_kong_selection(
    player: PlayerState,
    tile_ids: list[str],
) -> tuple[str, tuple[Tile, ...], int | None]:
    normalized_ids = list(tile_ids)
    if not normalized_ids or len(set(normalized_ids)) != len(normalized_ids):
        raise ValueError("Invalid kong tile selection")

    for kong_type, tiles, meld_index in _available_self_kongs(player):
        candidate_ids = [tile.tile_id for tile in tiles]
        if sorted(candidate_ids) == sorted(normalized_ids):
            return kong_type, tiles, meld_index

    raise ValueError("Invalid kong tile selection")


def _maybe_start_rob_kong_window(
    state: RoundState,
    *,
    seat: int,
    selected_tile: Tile,
    meld_index: int | None,
) -> RoundState | None:
    if meld_index is None:
        return None
    robbers = [
        other_seat
        for other_seat in range(len(state.players))
        if other_seat != seat and can_declare_hu(state, other_seat, selected_tile.tile_key)
    ]
    if not robbers:
        return None
    return RoundState(
        round_id=state.round_id,
        dealer_seat=state.dealer_seat,
        current_actor=seat,
        wall=state.wall,
        players=state.players,
        last_discard=selected_tile,
        pending_action={
            "type": "rob_kong_window",
            "actor_seat": seat,
            "tile_id": selected_tile.tile_id,
            "tile_key": selected_tile.tile_key,
            "meld_index": meld_index,
            "offered_hu_seats": robbers,
            "responded_seats": [],
        },
        phase=state.phase,
        settlement=state.settlement,
        version=state.version + 1,
        score_trackers=state.score_trackers,
        last_action_context=state.last_action_context,
        round_wind=state.round_wind,
        enforce_minimum_eight_fan=state.enforce_minimum_eight_fan,
    )


def _apply_rob_kong_pass(
    state: RoundState,
    seat: int,
) -> tuple[RoundState, list[dict]]:
    pending = state.pending_action or {}
    offered_hu_seats = list(pending.get("offered_hu_seats", []))
    if seat not in offered_hu_seats:
        raise ValueError("Pass is only allowed when a rob-kong claim is available")
    responded_seats = set(pending.get("responded_seats", []))
    if seat in responded_seats:
        raise ValueError("Seat has already responded to this claim window")
    responded_seats.add(seat)
    unresolved = [offered for offered in offered_hu_seats if offered not in responded_seats]
    if unresolved:
        return (
            RoundState(
                round_id=state.round_id,
                dealer_seat=state.dealer_seat,
                current_actor=state.current_actor,
                wall=state.wall,
                players=state.players,
                last_discard=state.last_discard,
                pending_action={
                    "type": "rob_kong_window",
                    "actor_seat": pending["actor_seat"],
                    "tile_id": pending["tile_id"],
                    "tile_key": pending["tile_key"],
                    "meld_index": pending["meld_index"],
                    "offered_hu_seats": offered_hu_seats,
                    "responded_seats": sorted(responded_seats),
                },
                phase=state.phase,
                settlement=state.settlement,
                version=state.version + 1,
                score_trackers=state.score_trackers,
                last_action_context=state.last_action_context,
                round_wind=state.round_wind,
                enforce_minimum_eight_fan=state.enforce_minimum_eight_fan,
            ),
            [],
        )
    return _complete_add_kong_after_passes(state)


def _complete_add_kong_after_passes(state: RoundState) -> tuple[RoundState, list[dict]]:
    pending = state.pending_action or {}
    actor_seat = pending["actor_seat"]
    tile_id = pending["tile_id"]
    tile_key = pending["tile_key"]
    meld_index = pending["meld_index"]
    actor = state.players[actor_seat]
    selected_tile = next(
        tile for tile in actor.concealed_tiles if tile.tile_id == tile_id and tile.tile_key == tile_key
    )
    replacement_state, events = _complete_self_kong(
        RoundState(
            round_id=state.round_id,
            dealer_seat=state.dealer_seat,
            current_actor=state.current_actor,
            wall=state.wall,
            players=state.players,
            last_discard=state.last_discard,
            pending_action=None,
            phase=state.phase,
            settlement=state.settlement,
            version=state.version,
            score_trackers=state.score_trackers,
            last_action_context=state.last_action_context,
            round_wind=state.round_wind,
            enforce_minimum_eight_fan=state.enforce_minimum_eight_fan,
        ),
        seat=actor_seat,
        kong_type="add_kong",
        selected_tiles=(selected_tile,),
        meld_index=meld_index,
    )
    return replacement_state, events


def _complete_self_kong(
    state: RoundState,
    *,
    seat: int,
    kong_type: str,
    selected_tiles: tuple[Tile, ...],
    meld_index: int | None,
) -> tuple[RoundState, list[dict]]:
    player = state.players[seat]
    concealed_list = list(player.concealed_tiles)

    for tile in selected_tiles:
        concealed_list.remove(tile)

    melds = list(player.melds)
    if kong_type == "concealed_kong":
        melds.append(tuple(selected_tiles))
    else:
        assert meld_index is not None
        melds[meld_index] = tuple(list(melds[meld_index]) + [selected_tiles[0]])

    updated_player = PlayerState(
        seat=player.seat,
        concealed_tiles=tuple(concealed_list),
        melds=tuple(melds),
        flowers=player.flowers,
        discards=player.discards,
    )
    new_players = tuple(
        updated_player if idx == seat else current_player
        for idx, current_player in enumerate(state.players)
    )
    events = [
        self_kong_declared_event(
            seat,
            kong_type,
            selected_tiles[0].tile_key,
            [tile.tile_id for tile in selected_tiles],
        )
    ]

    base_state = RoundState(
        round_id=state.round_id,
        dealer_seat=state.dealer_seat,
        current_actor=seat,
        wall=state.wall,
        players=new_players,
        last_discard=state.last_discard,
        pending_action=None,
        phase=state.phase,
        settlement=state.settlement,
        version=state.version,
        score_trackers=_append_kong_entry(
            state.score_trackers,
            kong_type=kong_type,
            actor_seat=seat,
            payer_seats=[
                other_seat for other_seat in range(len(state.players)) if other_seat != seat
            ],
            tile_key=selected_tiles[0].tile_key,
        ),
        last_action_context={
            "kind": "replacement_draw",
            "seat": seat,
            "tile_id": None,
            "from_kong_replacement": True,
            "was_last_live_tile": False,
            "was_last_discard": False,
        },
        round_wind=state.round_wind,
        enforce_minimum_eight_fan=state.enforce_minimum_eight_fan,
    )

    try:
        tile, wall = _draw_replacement_tile_once(state.wall)
    except IndexError:
        settled_state, settled_events = settle_exhaustive_draw(base_state)
        return settled_state, events + settled_events

    new_concealed = updated_player.concealed_tiles + (tile,)

    updated_player = PlayerState(
        seat=updated_player.seat,
        concealed_tiles=new_concealed,
        melds=updated_player.melds,
        flowers=updated_player.flowers,
        discards=updated_player.discards,
    )
    new_players = tuple(
        updated_player if idx == seat else current_player
        for idx, current_player in enumerate(new_players)
    )

    events.append(replacement_draw_event(seat, tile))

    return (
        RoundState(
            round_id=state.round_id,
            dealer_seat=state.dealer_seat,
            current_actor=seat,
            wall=wall,
            players=new_players,
            last_discard=state.last_discard,
            pending_action=None,
            phase=state.phase,
            settlement=state.settlement,
            version=state.version + 1,
            score_trackers=base_state.score_trackers,
            last_action_context={
                "kind": "replacement_draw",
                "seat": seat,
                "tile_id": tile.tile_id,
                "from_kong_replacement": True,
                "was_last_live_tile": False,
                "was_last_discard": False,
            },
            round_wind=state.round_wind,
            enforce_minimum_eight_fan=state.enforce_minimum_eight_fan,
        ),
        events,
    )


def _player_tile_keys(
    state: RoundState,
    *,
    seat: int,
    incoming_tile: str | None,
) -> list[str]:
    player = state.players[seat]
    tile_keys = [tile.tile_key for tile in player.concealed_tiles]
    for meld in player.melds:
        meld_keys = [tile.tile_key for tile in meld]
        if len(meld_keys) == 4:
            tile_keys.extend(meld_keys[:3])
        else:
            tile_keys.extend(meld_keys)
    if incoming_tile:
        tile_keys.append(incoming_tile)
    return tile_keys


def _open_meld_tile_key_groups(state: RoundState, *, seat: int) -> list[list[str]]:
    player = state.players[seat]
    return [
        [tile.tile_key for tile in meld]
        for meld in player.melds
        if _meld_is_open(state, seat=seat, meld=meld)
    ]


def _meld_is_open(state: RoundState, *, seat: int, meld: tuple[Tile, ...]) -> bool:
    if len(meld) != 4 or len({tile.tile_key for tile in meld}) != 1:
        return True

    tile_key = meld[0].tile_key
    for entry in reversed(_kong_entries(state)):
        if entry.get("actor_seat") != seat:
            continue
        if entry.get("tile_key") not in {None, tile_key}:
            continue
        return entry.get("kong_type") != "concealed_kong"
    return True


def _visible_tile_keys(state: RoundState) -> list[str]:
    tile_keys: list[str] = []
    for player in state.players:
        tile_keys.extend(tile.tile_key for tile in player.discards)
        for meld in player.melds:
            if len(meld) == 4 and len({tile.tile_key for tile in meld}) == 1:
                # Meld openness is not modeled separately yet, so counting three tiles
                # keeps exposed/add-kong visibility useful without overcounting the set.
                tile_keys.extend(tile.tile_key for tile in meld[:3])
            else:
                tile_keys.extend(tile.tile_key for tile in meld)
    return tile_keys


def _fan_result_for_win(
    state: RoundState,
    *,
    winner_seat: int,
    incoming_tile: str | None,
    discarder_seat: int | None = None,
) -> dict:
    player = state.players[winner_seat]
    meld_tile_key_groups = [
        [tile.tile_key for tile in meld]
        for meld in player.melds
    ]
    open_meld_tile_key_groups = _open_meld_tile_key_groups(state, seat=winner_seat)
    meld_open_flags = [
        meld_tile_keys in open_meld_tile_key_groups
        for meld_tile_keys in meld_tile_key_groups
    ]
    concealed_tile_keys = [tile.tile_key for tile in player.concealed_tiles]
    tile_keys = _player_tile_keys(state, seat=winner_seat, incoming_tile=incoming_tile)
    decompositions = decompose_winning_hand(tile_keys)
    win_type = "self_draw" if incoming_tile is None else "discard"
    timing = (
        _timing_features_for_self_draw(state)
        if incoming_tile is None
        else _timing_features_for_discard_win(state)
    )
    winner_kong_entries = [
        entry
        for entry in _kong_entries(state)
        if entry.get("actor_seat") == winner_seat
    ]
    return evaluate_fans(
        win_type=win_type,
        winner_seat=winner_seat,
        discarder_seat=discarder_seat,
        flower_count=len(player.flowers),
        seat_count=len(state.players),
        features=_winner_features(
            state,
            winner_seat=winner_seat,
            incoming_tile=incoming_tile,
            meld_open_flags=meld_open_flags,
            decompositions=decompositions,
        ),
        timing=timing,
        kong_entries=winner_kong_entries,
        tile_keys=tile_keys,
        visible_tile_keys=_visible_tile_keys(state),
        concealed_tile_keys=concealed_tile_keys,
        meld_tile_key_groups=meld_tile_key_groups,
        open_meld_tile_key_groups=open_meld_tile_key_groups,
        incoming_tile=incoming_tile,
        decompositions=decompositions,
        seat_wind_key=_seat_wind_key(winner_seat, state.dealer_seat),
        round_wind_key=state.round_wind,
    )


def _winner_features(
    state: RoundState,
    *,
    winner_seat: int,
    incoming_tile: str | None,
    meld_open_flags: list[bool],
    decompositions: list[dict],
) -> dict:
    player = state.players[winner_seat]
    return extract_hand_features(
        concealed_tile_keys=[tile.tile_key for tile in player.concealed_tiles],
        meld_tile_key_groups=[
            [tile.tile_key for tile in meld]
            for meld in player.melds
        ],
        meld_open_flags=meld_open_flags,
        incoming_tile=incoming_tile,
        seat_wind_key=_seat_wind_key(winner_seat, state.dealer_seat),
        round_wind_key=state.round_wind,
        decompositions=decompositions,
    )


def _empty_score_trackers() -> dict:
    return {"kong_entries": [], "opening_flowers_completed": False}


def _append_kong_entry(
    score_trackers: dict | None,
    *,
    kong_type: str,
    actor_seat: int,
    payer_seats: list[int],
    tile_key: str | None = None,
) -> dict:
    trackers = {
        "kong_entries": list((score_trackers or _empty_score_trackers()).get("kong_entries", []))
    }
    entry = {
        "kong_type": kong_type,
        "actor_seat": actor_seat,
        "payer_seats": payer_seats,
    }
    if tile_key is not None:
        entry["tile_key"] = tile_key
    trackers["kong_entries"].append(entry)
    return trackers


def _opening_flowers_tile_id(state: RoundState, seat: int) -> str | None:
    player = state.players[seat]
    for tile in player.concealed_tiles:
        if tile.kind == "flower":
            return tile.tile_id
    return None


def _kong_entries(state: RoundState) -> list[dict]:
    trackers = state.score_trackers or _empty_score_trackers()
    return list(trackers.get("kong_entries", []))


def _kong_delta_by_seat(state: RoundState) -> dict[int, int]:
    seat_count = len(state.players)
    deltas = {seat: 0 for seat in range(seat_count)}
    for entry in _kong_entries(state):
        actor_seat = entry["actor_seat"]
        for payer_seat in entry["payer_seats"]:
            deltas[payer_seat] -= 1
            deltas[actor_seat] += 1
    return deltas


def _seat_wind_key(seat: int, dealer_seat: int) -> str:
    return WIND_ORDER[(seat - dealer_seat) % 4]


def _is_last_tile_wall_point_for_actor(state: RoundState) -> bool:
    context = state.last_action_context or {}
    return (
        context.get("kind") == "draw"
        and bool(context.get("was_last_live_tile"))
        and not bool(context.get("from_kong_replacement"))
    )


def _timing_features_for_self_draw(state: RoundState) -> dict:
    context = state.last_action_context or {}
    is_replacement = bool(context.get("from_kong_replacement"))
    return {
        "gang_shang_hua": is_replacement,
        "hai_di_lao_yue": bool(
            not is_replacement
            and context.get("kind") == "draw"
            and context.get("was_last_live_tile", False)
        ),
        "he_di_lao_yu": False,
        "robbing_the_kong": False,
    }


def _timing_features_for_discard_win(state: RoundState) -> dict:
    context = state.last_action_context or {}
    return {
        "gang_shang_hua": False,
        "hai_di_lao_yue": False,
        "he_di_lao_yu": bool(
            context.get("kind") == "discard" and context.get("was_last_discard", False)
        ),
        "robbing_the_kong": bool(
            state.pending_action is not None
            and state.pending_action.get("type") == "rob_kong_window"
        ),
    }
