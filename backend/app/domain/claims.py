from __future__ import annotations

from collections import Counter

from app.domain.models import RoundState, Tile


def _parse_suit_rank(tile_key: str) -> tuple[str, int] | None:
    if not tile_key:
        return None
    prefix = tile_key[0]
    if prefix not in {"w", "t", "b"}:
        return None
    try:
        rank = int(tile_key[1:])
    except ValueError:
        return None
    return prefix, rank


def _can_chow(discard: Tile, counts: Counter[str]) -> bool:
    if discard.kind != "suit":
        return False
    parsed = _parse_suit_rank(discard.tile_key)
    if parsed is None:
        return False
    prefix, rank = parsed
    candidate_pairs = (
        (rank - 2, rank - 1),
        (rank - 1, rank + 1),
        (rank + 1, rank + 2),
    )
    for left_rank, right_rank in candidate_pairs:
        if left_rank < 1 or right_rank > 9:
            continue
        left_key = f"{prefix}{left_rank}"
        right_key = f"{prefix}{right_rank}"
        if counts.get(left_key, 0) > 0 and counts.get(right_key, 0) > 0:
            return True
    return False


def is_valid_chow_sequence(discard: Tile, tiles: list[Tile]) -> bool:
    if discard.kind != "suit" or len(tiles) != 2:
        return False
    parsed_discard = _parse_suit_rank(discard.tile_key)
    if parsed_discard is None:
        return False
    prefix, discard_rank = parsed_discard
    ranks: list[int] = []
    for tile in tiles:
        parsed_tile = _parse_suit_rank(tile.tile_key)
        if parsed_tile is None:
            return False
        tile_prefix, rank = parsed_tile
        if tile_prefix != prefix:
            return False
        ranks.append(rank)
    ranks.append(discard_rank)
    ranks.sort()
    return ranks[0] + 1 == ranks[1] and ranks[1] + 1 == ranks[2]


def compute_claim_window(state: RoundState) -> dict[int, set[str]]:
    claim_window: dict[int, set[str]] = {
        seat: set() for seat in range(len(state.players))
    }
    discard = state.last_discard
    if discard is None:
        return claim_window

    discarder_seat = state.current_actor
    next_player = (discarder_seat + 1) % len(state.players)
    ltw_after_discard = _is_last_tile_wall_point_after_discard(state)
    from app.domain.reducer import can_declare_hu

    for seat, player in enumerate(state.players):
        if seat == discarder_seat:
            continue
        counts = Counter(tile.tile_key for tile in player.concealed_tiles)
        if not ltw_after_discard and counts.get(discard.tile_key, 0) >= 2:
            claim_window[seat].add("pung")
        if not ltw_after_discard and counts.get(discard.tile_key, 0) >= 3:
            claim_window[seat].add("kong")
        if not ltw_after_discard and seat == next_player and _can_chow(discard, counts):
            claim_window[seat].add("chow")
        if can_declare_hu(state, seat, discard.tile_key):
            claim_window[seat].add("hu")

    return claim_window


def _is_last_tile_wall_point_after_discard(state: RoundState) -> bool:
    context = state.last_action_context or {}
    return context.get("kind") == "discard" and bool(context.get("was_last_discard"))


def resolve_claims(
    claim_requests: list[dict], discarder_seat: int
) -> dict | None:
    if not claim_requests:
        return None

    priority = {"hu": 3, "kong": 2, "pung": 2, "chow": 1}
    player_count = 4
    next_player = (discarder_seat + 1) % player_count
    candidates = []
    for request in claim_requests:
        claim_type = request.get("type")
        if priority.get(claim_type, 0) <= 0:
            continue
        if claim_type == "chow" and request.get("seat") != next_player:
            continue
        candidates.append(request)
    if not candidates:
        return None

    def sort_key(request: dict) -> tuple[int, int]:
        claim_priority = priority.get(request["type"], 0)
        distance = (request["seat"] - discarder_seat) % player_count
        if distance == 0:
            distance = player_count
        return (-claim_priority, distance)

    return sorted(candidates, key=sort_key)[0]
