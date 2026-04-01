from __future__ import annotations

from collections import Counter
from dataclasses import dataclass
from functools import lru_cache
from itertools import combinations
from typing import Literal

from app.domain.models import PlayerState, RoundState, Tile
from app.domain.reducer import can_declare_flower, can_declare_hu, can_declare_self_kong

WIND_ORDER = ("east", "south", "west", "north")
DRAGON_KEYS = {"red", "green", "white"}
HONOR_KEYS = {"east", "south", "west", "north", *DRAGON_KEYS}
SUITED_TILE_KEYS = tuple(
    f"{suit}{rank}"
    for suit in ("w", "t", "b")
    for rank in range(1, 10)
)
ALL_TILE_KEYS = SUITED_TILE_KEYS + tuple(WIND_ORDER) + tuple(sorted(DRAGON_KEYS))
BotPersona = Literal["menzen_attacker", "balanced", "defender"]


@dataclass(frozen=True)
class BotDecision:
    action_type: str
    tile_ids: list[str]


@dataclass(frozen=True)
class _DiscardPlan:
    tile_id: str
    shanten: int
    effective_tiles: int
    hand_score: float
    risk_score: float
    keep_score: tuple[int, int, float]


@dataclass(frozen=True)
class _ClaimPlan:
    action_type: str
    tile_ids: list[str]
    shanten: int
    effective_tiles: int
    hand_score: float


@dataclass(frozen=True)
class _BotStyleProfile:
    persona: BotPersona
    aggression: float
    defense_bias: float
    fold_pressure: float
    hard_fold_pressure: float
    risk_tolerance: float
    shanten_tolerance: int
    chow_call_bias: float
    pung_call_bias: float
    kong_call_bias: float
    closed_hand_bias: float


def choose_active_turn_action(
    state: RoundState,
    seat_index: int,
    aggression: float = 0.5,
    persona: BotPersona = "balanced",
) -> BotDecision:
    style = _style_profile(persona=persona, aggression=aggression)
    if can_declare_hu(state, seat_index, None):
        return BotDecision(action_type="hu", tile_ids=[])

    if can_declare_flower(state, seat_index):
        flower_tile = next(
            tile for tile in state.players[seat_index].concealed_tiles if tile.kind == "flower"
        )
        return BotDecision(action_type="flower", tile_ids=[flower_tile.tile_id])

    if can_declare_self_kong(state, seat_index):
        kong_choice = _choose_self_kong(state, seat_index, style=style)
        if kong_choice is not None:
            return kong_choice

    discard_plan = _choose_discard_plan_for_state(
        state,
        seat_index=seat_index,
        concealed_tiles=state.players[seat_index].concealed_tiles,
        open_meld_count=len(state.players[seat_index].melds),
        style=style,
    )
    return BotDecision(action_type="discard", tile_ids=[discard_plan.tile_id])


def choose_claim_action(
    state: RoundState,
    seat_index: int,
    aggression: float = 0.5,
    persona: BotPersona = "balanced",
) -> BotDecision:
    style = _style_profile(persona=persona, aggression=aggression)
    pending_action = state.pending_action or {}
    pending_type = pending_action.get("type")

    if pending_type == "rob_kong_window":
        offered_hu_seats = set(pending_action.get("offered_hu_seats", []))
        if seat_index in offered_hu_seats and can_declare_hu(state, seat_index, state.last_discard.tile_key if state.last_discard else None):
            return BotDecision(action_type="hu", tile_ids=[])
        return BotDecision(action_type="pass", tile_ids=[])

    claim_window = pending_action.get("claim_window", [])
    offered_claims = set(claim_window[seat_index]) if seat_index < len(claim_window) else set()
    if "hu" in offered_claims and state.last_discard is not None and can_declare_hu(
        state,
        seat_index,
        state.last_discard.tile_key,
    ):
        return BotDecision(action_type="hu", tile_ids=[])

    best_claim = _choose_structured_claim(
        state,
        seat_index,
        offered_claims,
        style=style,
    )
    if best_claim is not None:
        return BotDecision(action_type=best_claim.action_type, tile_ids=best_claim.tile_ids)
    return BotDecision(action_type="pass", tile_ids=[])


def choose_discard(player: PlayerState) -> Tile:
    open_meld_count = len(player.melds)
    candidate_plans = [
        _discard_plan_from_tiles(
            player.concealed_tiles,
            tile.tile_id,
            open_meld_count=open_meld_count,
        )
        for tile in player.concealed_tiles
        if tile.kind != "flower"
    ]
    if not candidate_plans:
        return player.concealed_tiles[-1]

    best_plan = min(
        candidate_plans,
        key=lambda plan: (
            plan.shanten,
            -plan.effective_tiles,
            -plan.hand_score,
            plan.risk_score,
            plan.tile_id,
        ),
    )
    return next(tile for tile in player.concealed_tiles if tile.tile_id == best_plan.tile_id)


def _choose_structured_claim(
    state: RoundState,
    seat_index: int,
    offered_claims: set[str],
    *,
    style: _BotStyleProfile,
) -> _ClaimPlan | None:
    if state.last_discard is None:
        return None

    player = state.players[seat_index]
    current_shanten, current_effective, current_score = _hand_quality(
        [tile.tile_key for tile in player.concealed_tiles],
        open_meld_count=len(player.melds),
    )

    candidate_plans: list[_ClaimPlan] = []
    for action_type in ("kong", "pung", "chow"):
        if action_type not in offered_claims:
            continue
        candidate_plans.extend(
            _claim_plans_for_type(
                state,
                seat_index,
                action_type,
                style=style,
            )
        )

    if not candidate_plans:
        return None

    best_plan = min(
        candidate_plans,
        key=lambda plan: (
            plan.shanten,
            -plan.effective_tiles,
            -plan.hand_score,
            {"kong": 0, "pung": 1, "chow": 2}[plan.action_type],
        ),
    )

    if not _claim_is_worth_taking(
        state=state,
        seat_index=seat_index,
        claim=best_plan,
        current_shanten=current_shanten,
        current_effective=current_effective,
        current_score=current_score,
        style=style,
    ):
        return None
    return best_plan


def _claim_plans_for_type(
    state: RoundState,
    seat_index: int,
    action_type: str,
    *,
    style: _BotStyleProfile,
) -> list[_ClaimPlan]:
    player = state.players[seat_index]
    candidate_tile_ids = _candidate_claim_tile_ids(
        player=player,
        discard=state.last_discard,
        action_type=action_type,
    )
    open_meld_count = len(player.melds) + 1

    plans: list[_ClaimPlan] = []
    for tile_ids in candidate_tile_ids:
        remaining_tiles = _remove_tiles_by_id(player.concealed_tiles, tile_ids)
        remaining_keys = [tile.tile_key for tile in remaining_tiles]
        if action_type == "kong":
            shanten, effective_tiles, hand_score = _hand_quality(
                remaining_keys,
                open_meld_count=open_meld_count,
            )
        else:
            best_discard = _choose_discard_plan_for_state(
                state,
                seat_index=seat_index,
                concealed_tiles=tuple(remaining_tiles),
                open_meld_count=open_meld_count,
                style=style,
            )
            shanten = best_discard.shanten
            effective_tiles = best_discard.effective_tiles
            hand_score = best_discard.hand_score
        plans.append(
            _ClaimPlan(
                action_type=action_type,
                tile_ids=tile_ids,
                shanten=shanten,
                effective_tiles=effective_tiles,
                hand_score=hand_score,
            )
        )
    return plans


def _claim_is_worth_taking(
    *,
    state: RoundState,
    seat_index: int,
    claim: _ClaimPlan,
    current_shanten: int,
    current_effective: int,
    current_score: float,
    style: _BotStyleProfile,
) -> bool:
    discard_tile = state.last_discard
    if discard_tile is None:
        return False

    has_open_meld = bool(state.players[seat_index].melds)
    value_tile = _is_value_tile(
        discard_tile.tile_key,
        seat_index=seat_index,
        dealer_seat=state.dealer_seat,
        round_wind=state.round_wind,
    )
    pressure = _table_pressure(state, seat_index)
    improves_shape = (
        claim.shanten < current_shanten
        or (
            claim.shanten == current_shanten
            and (
                claim.effective_tiles > current_effective + 2
                or claim.hand_score > current_score + 1.5
            )
        )
    )
    if claim.shanten == current_shanten and claim.effective_tiles > current_effective:
        improves_shape = True

    if (
        style.persona == "menzen_attacker"
        and not has_open_meld
        and claim.action_type in {"chow", "pung"}
        and not value_tile
        and current_shanten >= 1
        and claim.shanten >= current_shanten
    ):
        return False

    if claim.action_type == "kong":
        if pressure >= max(0.5, style.fold_pressure - 0.05) and claim.shanten >= current_shanten and style.kong_call_bias < 1.0:
            return False
        return (
            value_tile
            or has_open_meld
            or claim.shanten < current_shanten
            or (
                claim.shanten == current_shanten
                and (claim.effective_tiles >= current_effective or style.kong_call_bias > 1.05)
            )
        )

    if claim.action_type == "pung":
        if value_tile:
            return claim.shanten <= current_shanten
        if current_shanten >= 3 and not has_open_meld:
            return False
        if (
            not has_open_meld
            and style.closed_hand_bias > 1.0
            and claim.shanten == current_shanten
            and claim.effective_tiles <= current_effective + 1
        ):
            return False
        if (
            pressure >= style.fold_pressure
            and claim.shanten >= current_shanten
            and style.pung_call_bias < 1.0
        ):
            return False
        if not improves_shape:
            return False
        if style.persona == "defender" and pressure >= 0.45 and not value_tile and not has_open_meld:
            return False
        return has_open_meld or current_shanten <= 2 or style.pung_call_bias > 1.05

    if claim.action_type == "chow":
        if current_shanten >= 3 and not has_open_meld:
            return False
        if (
            not has_open_meld
            and style.closed_hand_bias > 1.0
            and claim.shanten == current_shanten
            and claim.effective_tiles <= current_effective + 2
        ):
            return False
        if (
            pressure >= style.fold_pressure - 0.05
            and claim.shanten >= current_shanten
            and style.chow_call_bias < 1.0
        ):
            return False
        if not improves_shape:
            return False
        if style.persona == "menzen_attacker" and not has_open_meld and claim.shanten >= current_shanten:
            return False
        if style.persona == "defender" and pressure >= 0.4 and not has_open_meld:
            return False
        return has_open_meld or claim.shanten <= 1 or style.chow_call_bias > 1.08

    return False


def _choose_self_kong(
    state: RoundState,
    seat_index: int,
    *,
    style: _BotStyleProfile,
) -> BotDecision | None:
    player = state.players[seat_index]
    candidate_groups = _self_kong_tile_groups(player)
    if not candidate_groups:
        return None

    current_shanten, current_effective, current_score = _hand_quality(
        [tile.tile_key for tile in player.concealed_tiles],
        open_meld_count=len(player.melds),
    )
    pressure = _table_pressure(state, seat_index)
    best_candidate: tuple[int, int, float, list[str]] | None = None
    for tile_ids in candidate_groups:
        remaining_tiles = _remove_tiles_by_id(player.concealed_tiles, tile_ids)
        shanten, effective_tiles, hand_score = _hand_quality(
            [tile.tile_key for tile in remaining_tiles],
            open_meld_count=len(player.melds) + 1,
        )
        candidate = (shanten, -effective_tiles, -hand_score, tile_ids)
        if best_candidate is None or candidate < best_candidate:
            best_candidate = candidate

    if best_candidate is None:
        return None

    shanten = best_candidate[0]
    effective_tiles = -best_candidate[1]
    hand_score = -best_candidate[2]
    if (
        shanten < current_shanten
        or shanten == current_shanten
        or effective_tiles >= current_effective
        or hand_score >= current_score
    ):
        if pressure >= style.fold_pressure and shanten >= current_shanten and style.kong_call_bias < 1.0:
            return None
        return BotDecision(action_type="kong", tile_ids=best_candidate[3])
    return None


def _self_kong_tile_groups(player: PlayerState) -> list[list[str]]:
    by_key: dict[str, list[Tile]] = {}
    for tile in player.concealed_tiles:
        by_key.setdefault(tile.tile_key, []).append(tile)

    groups: list[list[str]] = []
    for tiles in by_key.values():
        if len(tiles) >= 4:
            groups.append([tile.tile_id for tile in tiles[:4]])

    meld_keys = {
        meld[0].tile_key
        for meld in player.melds
        if len(meld) == 3 and len({tile.tile_key for tile in meld}) == 1
    }
    for meld_key in meld_keys:
        if by_key.get(meld_key):
            groups.append([by_key[meld_key][0].tile_id])
    return groups


def _candidate_claim_tile_ids(
    *,
    player: PlayerState,
    discard: Tile | None,
    action_type: str,
) -> list[list[str]]:
    if discard is None:
        return []

    by_key: dict[str, list[Tile]] = {}
    for tile in player.concealed_tiles:
        by_key.setdefault(tile.tile_key, []).append(tile)

    if action_type in {"pung", "kong"}:
        needed = 2 if action_type == "pung" else 3
        tiles = by_key.get(discard.tile_key, [])
        return [
            [tile.tile_id for tile in combo]
            for combo in combinations(tiles, needed)
        ]

    parsed = _parse_suit_rank(discard.tile_key)
    if parsed is None:
        return []

    suit, rank = parsed
    candidates: list[list[str]] = []
    for left_rank, right_rank in (
        (rank - 2, rank - 1),
        (rank - 1, rank + 1),
        (rank + 1, rank + 2),
    ):
        if left_rank < 1 or right_rank > 9:
            continue
        left_key = f"{suit}{left_rank}"
        right_key = f"{suit}{right_rank}"
        left_tiles = by_key.get(left_key, [])
        right_tiles = by_key.get(right_key, [])
        if not left_tiles or not right_tiles:
            continue
        candidates.append([left_tiles[0].tile_id, right_tiles[0].tile_id])
    return candidates


def _discard_plan_from_tiles(
    concealed_tiles: tuple[Tile, ...] | list[Tile],
    tile_id: str,
    *,
    open_meld_count: int,
) -> _DiscardPlan:
    remaining_tiles = [tile for tile in concealed_tiles if tile.tile_id != tile_id]
    remaining_keys = [tile.tile_key for tile in remaining_tiles]
    shanten, effective_tiles, hand_score = _hand_quality(
        remaining_keys,
        open_meld_count=open_meld_count,
    )
    return _DiscardPlan(
        tile_id=tile_id,
        shanten=shanten,
        effective_tiles=effective_tiles,
        hand_score=hand_score,
        risk_score=0.0,
        keep_score=(shanten, effective_tiles, hand_score),
    )


def _choose_discard_plan_for_state(
    state: RoundState,
    *,
    seat_index: int,
    concealed_tiles: tuple[Tile, ...],
    open_meld_count: int,
    style: _BotStyleProfile,
) -> _DiscardPlan:
    candidate_plans = [
        _discard_plan_with_risk(
            state,
            seat_index=seat_index,
            concealed_tiles=concealed_tiles,
            tile_id=tile.tile_id,
            open_meld_count=open_meld_count,
        )
        for tile in concealed_tiles
        if tile.kind != "flower"
    ]
    if not candidate_plans:
        fallback_tile = concealed_tiles[-1]
        return _discard_plan_with_risk(
            state,
            seat_index=seat_index,
            concealed_tiles=concealed_tiles,
            tile_id=fallback_tile.tile_id,
            open_meld_count=open_meld_count,
        )

    pressure = _table_pressure(state, seat_index)
    best_shanten = min(plan.shanten for plan in candidate_plans)
    if pressure >= style.hard_fold_pressure:
        safest_risk = min(plan.risk_score for plan in candidate_plans)
        filtered = [
            plan
            for plan in candidate_plans
            if plan.risk_score <= safest_risk + max(0.08, style.risk_tolerance * 0.6)
            and plan.shanten <= best_shanten + max(1, style.shanten_tolerance)
        ]
        allowed_plans = filtered or candidate_plans
        return min(
            allowed_plans,
            key=lambda plan: (
                plan.risk_score,
                plan.shanten,
                -plan.effective_tiles,
                -plan.hand_score,
                plan.tile_id,
            ),
        )

    allowed_plans = candidate_plans
    if pressure >= style.fold_pressure:
        safest_risk = min(plan.risk_score for plan in candidate_plans)
        filtered = [
            plan
            for plan in candidate_plans
            if plan.risk_score <= safest_risk + style.risk_tolerance
            and plan.shanten <= best_shanten + style.shanten_tolerance
        ]
        if filtered:
            allowed_plans = filtered
        return min(
            allowed_plans,
            key=lambda plan: (
                plan.shanten,
                plan.risk_score * style.defense_bias * pressure,
                -plan.effective_tiles,
                -plan.hand_score,
                plan.tile_id,
            ),
        )

    return min(
        candidate_plans,
        key=lambda plan: (
            plan.shanten,
            -plan.effective_tiles,
            -plan.hand_score,
            plan.risk_score * style.defense_bias * max(0.05, pressure),
            plan.tile_id,
        ),
    )


def _discard_plan_with_risk(
    state: RoundState,
    *,
    seat_index: int,
    concealed_tiles: tuple[Tile, ...],
    tile_id: str,
    open_meld_count: int,
) -> _DiscardPlan:
    offensive_plan = _discard_plan_from_tiles(
        concealed_tiles,
        tile_id,
        open_meld_count=open_meld_count,
    )
    tile = next(candidate for candidate in concealed_tiles if candidate.tile_id == tile_id)
    return _DiscardPlan(
        tile_id=tile_id,
        shanten=offensive_plan.shanten,
        effective_tiles=offensive_plan.effective_tiles,
        hand_score=offensive_plan.hand_score,
        risk_score=_discard_risk_score(state, seat_index=seat_index, tile=tile),
        keep_score=offensive_plan.keep_score,
    )


def _hand_quality(tile_keys: list[str], *, open_meld_count: int) -> tuple[int, int, float]:
    shanten = _best_shanten(tile_keys, open_meld_count=open_meld_count)
    effective_tiles = _effective_tile_count(tile_keys, open_meld_count=open_meld_count, current_shanten=shanten)
    shape_score = _shape_score(tile_keys)
    return shanten, effective_tiles, shape_score


def _effective_tile_count(
    tile_keys: list[str],
    *,
    open_meld_count: int,
    current_shanten: int,
) -> int:
    counts = Counter(tile_keys)
    total = 0
    for tile_key in ALL_TILE_KEYS:
        if counts.get(tile_key, 0) >= 4:
            continue
        next_shanten = _best_shanten(tile_keys + [tile_key], open_meld_count=open_meld_count)
        if next_shanten < current_shanten:
            total += 4 - counts.get(tile_key, 0)
    return total


def _best_shanten(tile_keys: list[str], *, open_meld_count: int) -> int:
    standard_shanten = _standard_shanten(tile_keys, open_meld_count=open_meld_count)
    seven_pairs = _seven_pairs_shanten(tile_keys) if open_meld_count == 0 else 8
    return min(standard_shanten, seven_pairs)


def _standard_shanten(tile_keys: list[str], *, open_meld_count: int) -> int:
    counts = Counter(tile_keys)
    initial_signature = tuple(sorted(counts.items()))

    def _calculate_standard_shanten(melds: int, pairs: int, taatsu: int) -> int:
        melds_total = melds + open_meld_count
        if melds_total > 4:
            melds_total = 4
        taatsu_total = min(taatsu, max(0, 4 - melds_total))
        pair_used = 1 if pairs > 0 else 0
        extra_pairs = max(0, pairs - pair_used)
        taatsu_total = min(4 - melds_total, taatsu_total + extra_pairs)
        shanten = 8 - melds_total * 2 - taatsu_total - pair_used
        return max(-1, shanten)

    @lru_cache(maxsize=None)
    def dfs(
        signature: tuple[tuple[str, int], ...],
        melds: int,
        pairs: int,
        taatsu: int,
    ) -> int:
        if not signature:
            return _calculate_standard_shanten(melds, pairs, taatsu)

        counts_state = Counter(dict(signature))
        tile_key = min(counts_state)
        tile_count = counts_state[tile_key]
        best = 8

        next_counts = counts_state.copy()
        next_counts[tile_key] -= 1
        if next_counts[tile_key] == 0:
            del next_counts[tile_key]
        best = min(best, dfs(tuple(sorted(next_counts.items())), melds, pairs, taatsu))

        if tile_count >= 3:
            triplet_counts = counts_state.copy()
            triplet_counts[tile_key] -= 3
            if triplet_counts[tile_key] == 0:
                del triplet_counts[tile_key]
            best = min(
                best,
                dfs(tuple(sorted(triplet_counts.items())), melds + 1, pairs, taatsu),
            )

        parsed = _parse_suit_rank(tile_key)
        if parsed is not None:
            suit, rank = parsed
            second = f"{suit}{rank + 1}"
            third = f"{suit}{rank + 2}"
            if counts_state.get(second, 0) > 0 and counts_state.get(third, 0) > 0:
                sequence_counts = counts_state.copy()
                for key in (tile_key, second, third):
                    sequence_counts[key] -= 1
                    if sequence_counts[key] == 0:
                        del sequence_counts[key]
                best = min(
                    best,
                    dfs(tuple(sorted(sequence_counts.items())), melds + 1, pairs, taatsu),
                )

        if tile_count >= 2:
            pair_counts = counts_state.copy()
            pair_counts[tile_key] -= 2
            if pair_counts[tile_key] == 0:
                del pair_counts[tile_key]
            pair_signature = tuple(sorted(pair_counts.items()))
            best = min(best, dfs(pair_signature, melds, pairs + 1, taatsu))
            best = min(best, dfs(pair_signature, melds, pairs, taatsu + 1))

        if parsed is not None:
            suit, rank = parsed
            second = f"{suit}{rank + 1}"
            gap = f"{suit}{rank + 2}"
            if counts_state.get(second, 0) > 0:
                ryanmen_counts = counts_state.copy()
                for key in (tile_key, second):
                    ryanmen_counts[key] -= 1
                    if ryanmen_counts[key] == 0:
                        del ryanmen_counts[key]
                best = min(
                    best,
                    dfs(tuple(sorted(ryanmen_counts.items())), melds, pairs, taatsu + 1),
                )
            if counts_state.get(gap, 0) > 0:
                kanchan_counts = counts_state.copy()
                for key in (tile_key, gap):
                    kanchan_counts[key] -= 1
                    if kanchan_counts[key] == 0:
                        del kanchan_counts[key]
                best = min(
                    best,
                    dfs(tuple(sorted(kanchan_counts.items())), melds, pairs, taatsu + 1),
                )
        return best

    return dfs(initial_signature, 0, 0, 0)


def _seven_pairs_shanten(tile_keys: list[str]) -> int:
    counts = Counter(tile_keys)
    pair_count = sum(1 for count in counts.values() if count >= 2)
    distinct_count = len(counts)
    needed_distinct = max(0, 7 - distinct_count)
    return max(-1, 6 - pair_count + needed_distinct)


def _shape_score(tile_keys: list[str]) -> float:
    counts = Counter(tile_keys)
    score = 0.0
    for tile_key, count in counts.items():
        parsed = _parse_suit_rank(tile_key)
        if parsed is None:
            if count >= 3:
                score += 12.0
            elif count == 2:
                score += 5.0
            else:
                score -= 2.5
            continue

        _, rank = parsed
        left = counts.get(_offset_tile(tile_key, -1), 0)
        right = counts.get(_offset_tile(tile_key, 1), 0)
        gap_left = counts.get(_offset_tile(tile_key, -2), 0)
        gap_right = counts.get(_offset_tile(tile_key, 2), 0)
        center_bonus = 1.5 if 3 <= rank <= 7 else 0.5

        if count >= 3:
            score += 10.0 + center_bonus
        elif count == 2:
            score += 4.5 + center_bonus
        else:
            score += center_bonus - (1.5 if rank in {1, 9} else 0.0)

        score += min(left, 1) * 2.2
        score += min(right, 1) * 2.2
        score += min(gap_left, 1) * 1.1
        score += min(gap_right, 1) * 1.1
        if left == 0 and right == 0 and gap_left == 0 and gap_right == 0:
            score -= 3.0 if rank in {1, 9} else 2.0
    return score


def _remove_tiles_by_id(tiles: tuple[Tile, ...], tile_ids: list[str]) -> list[Tile]:
    pending = Counter(tile_ids)
    remaining: list[Tile] = []
    for tile in tiles:
        if pending.get(tile.tile_id, 0) > 0:
            pending[tile.tile_id] -= 1
            continue
        remaining.append(tile)
    return remaining


def _table_pressure(state: RoundState, seat_index: int) -> float:
    return max(
        (
            _opponent_threat(state, seat_index=seat_index, opponent_seat=opponent_seat)
            for opponent_seat in range(len(state.players))
            if opponent_seat != seat_index
        ),
        default=0.0,
    )


def _discard_risk_score(state: RoundState, *, seat_index: int, tile: Tile) -> float:
    total = 0.0
    for opponent_seat in range(len(state.players)):
        if opponent_seat == seat_index:
            continue
        threat = _opponent_threat(state, seat_index=seat_index, opponent_seat=opponent_seat)
        if threat <= 0.0:
            continue
        total += threat * _tile_risk_against_opponent(
            state,
            seat_index=seat_index,
            opponent_seat=opponent_seat,
            tile=tile,
        )
    return total


def _opponent_threat(state: RoundState, *, seat_index: int, opponent_seat: int) -> float:
    player = state.players[opponent_seat]
    open_melds = len(player.melds)
    discards = len(player.discards)
    threat = 0.0
    if open_melds >= 3:
        threat += 0.75
    elif open_melds == 2:
        threat += 0.58
    elif open_melds == 1:
        threat += 0.3

    if open_melds >= 2 and discards >= 6:
        threat += 0.15
    elif open_melds >= 1 and discards >= 9:
        threat += 0.1
    elif discards >= 12:
        threat += 0.08

    value_melds = sum(
        1
        for meld in player.melds
        if len(meld) >= 3
        and len({meld_tile.tile_key for meld_tile in meld[:3]}) == 1
        and _is_value_tile(
            meld[0].tile_key,
            seat_index=opponent_seat,
            dealer_seat=state.dealer_seat,
            round_wind=state.round_wind,
        )
    )
    threat += min(0.2, value_melds * 0.1)

    return min(1.0, threat)


def _tile_risk_against_opponent(
    state: RoundState,
    *,
    seat_index: int,
    opponent_seat: int,
    tile: Tile,
) -> float:
    opponent = state.players[opponent_seat]
    opponent_discard_keys = [discard.tile_key for discard in opponent.discards]
    opponent_discards = set(opponent_discard_keys)
    if tile.tile_key in opponent_discards:
        return 0.01

    visible_count = _visible_tile_count(state, seat_index=seat_index, tile_key=tile.tile_key)
    if visible_count >= 4:
        return 0.01

    if tile.tile_key in HONOR_KEYS:
        base = 0.78 if tile.tile_key in DRAGON_KEYS else 0.68
        base -= visible_count * 0.18
        if visible_count >= 3:
            base -= 0.2
        return max(0.01, base)

    parsed = _parse_suit_rank(tile.tile_key)
    assert parsed is not None
    suit, rank = parsed
    base = 0.62
    if rank in {4, 5, 6}:
        base += 0.28
    elif rank in {3, 7}:
        base += 0.13
    elif rank in {1, 9}:
        base -= 0.22
    elif rank in {2, 8}:
        base -= 0.08
    base -= visible_count * 0.08
    if visible_count >= 3:
        base -= 0.08

    base -= _suji_safety_reduction(opponent_discard_keys, suit=suit, rank=rank)
    base -= _kabe_safety_reduction(state, seat_index=seat_index, suit=suit, rank=rank)

    meld_suits = [
        meld[0].tile_key[0]
        for meld in opponent.melds
        if meld and _parse_suit_rank(meld[0].tile_key) is not None
    ]
    if meld_suits and meld_suits.count(suit) >= max(1, len(meld_suits) - 1):
        base += 0.15

    return max(0.01, min(1.25, base))


def _visible_tile_count(state: RoundState, *, seat_index: int, tile_key: str) -> int:
    visible_count = 0
    own_player = state.players[seat_index]
    visible_count += sum(1 for tile in own_player.concealed_tiles if tile.tile_key == tile_key)
    for player in state.players:
        visible_count += sum(1 for tile in player.discards if tile.tile_key == tile_key)
        visible_count += sum(
            1
            for meld in player.melds
            for tile in meld
            if tile.tile_key == tile_key
        )
        visible_count += sum(1 for tile in player.flowers if tile.tile_key == tile_key)
    return visible_count


def _suji_safety_reduction(opponent_discards: list[str], *, suit: str, rank: int) -> float:
    if rank in {1, 9}:
        return 0.0

    matching_turns = [
        index
        for index, tile_key in enumerate(opponent_discards)
        for candidate_rank in (rank - 3, rank + 3)
        if 1 <= candidate_rank <= 9 and tile_key == f"{suit}{candidate_rank}"
    ]
    if not matching_turns:
        return 0.0

    single_suji = {
        2: 0.2,
        3: 0.17,
        4: 0.12,
        5: 0.1,
        6: 0.12,
        7: 0.17,
        8: 0.2,
    }.get(rank, 0.0)
    double_suji_bonus = {
        2: 0.08,
        3: 0.1,
        4: 0.13,
        5: 0.15,
        6: 0.13,
        7: 0.1,
        8: 0.08,
    }.get(rank, 0.0)
    late_suji_bonus = 0.04 if max(matching_turns) >= max(0, len(opponent_discards) - 4) else 0.0

    if len(set(matching_turns)) >= 2:
        return single_suji + double_suji_bonus + late_suji_bonus
    return single_suji + late_suji_bonus


def _wall_safety_reduction(visible_count: int, *, complete: float, one_chance: float) -> float:
    if visible_count >= 4:
        return complete
    if visible_count == 3:
        return one_chance
    return 0.0


def _kabe_safety_reduction(
    state: RoundState,
    *,
    seat_index: int,
    suit: str,
    rank: int,
) -> float:
    visible_neighbors = {
        neighbor_rank: _visible_tile_count(
            state,
            seat_index=seat_index,
            tile_key=f"{suit}{neighbor_rank}",
        )
        for neighbor_rank in (rank - 1, rank + 1)
        if 1 <= neighbor_rank <= 9
    }

    if rank == 1:
        return _wall_safety_reduction(visible_neighbors.get(2, 0), complete=0.34, one_chance=0.18)
    if rank == 9:
        return _wall_safety_reduction(visible_neighbors.get(8, 0), complete=0.34, one_chance=0.18)

    reduction = 0.0
    for neighbor_rank, neighbor_count in visible_neighbors.items():
        distance_from_center = abs(5 - rank)
        complete = 0.12 if distance_from_center >= 2 else 0.1
        one_chance = 0.06 if distance_from_center >= 2 else 0.05
        reduction += _wall_safety_reduction(
            neighbor_count,
            complete=complete,
            one_chance=one_chance,
        )

    if rank in {2, 8}:
        edge_rank = 1 if rank == 2 else 9
        edge_count = _visible_tile_count(
            state,
            seat_index=seat_index,
            tile_key=f"{suit}{edge_rank}",
        )
        reduction += _wall_safety_reduction(
            edge_count,
            complete=0.14,
            one_chance=0.07,
        )

    complete_walls = sum(1 for count in visible_neighbors.values() if count >= 4)
    one_chance_walls = sum(1 for count in visible_neighbors.values() if count == 3)
    if complete_walls >= 2:
        reduction += 0.06
    elif one_chance_walls >= 2:
        reduction += 0.03

    return reduction


def _style_profile(*, persona: BotPersona, aggression: float) -> _BotStyleProfile:
    bounded_aggression = min(1.0, max(0.0, aggression))
    if persona == "menzen_attacker":
        return _BotStyleProfile(
            persona=persona,
            aggression=bounded_aggression,
            defense_bias=max(0.18, 0.65 - bounded_aggression * 0.35),
            fold_pressure=0.82,
            hard_fold_pressure=0.96,
            risk_tolerance=0.34 + bounded_aggression * 0.1,
            shanten_tolerance=0,
            chow_call_bias=0.76 + bounded_aggression * 0.18,
            pung_call_bias=0.82 + bounded_aggression * 0.16,
            kong_call_bias=0.92 + bounded_aggression * 0.14,
            closed_hand_bias=1.25,
        )
    if persona == "defender":
        return _BotStyleProfile(
            persona=persona,
            aggression=bounded_aggression,
            defense_bias=1.25 - bounded_aggression * 0.2,
            fold_pressure=0.42,
            hard_fold_pressure=0.62,
            risk_tolerance=0.14 + bounded_aggression * 0.08,
            shanten_tolerance=2,
            chow_call_bias=0.52 + bounded_aggression * 0.14,
            pung_call_bias=0.62 + bounded_aggression * 0.14,
            kong_call_bias=0.76 + bounded_aggression * 0.12,
            closed_hand_bias=1.08,
        )
    return _BotStyleProfile(
        persona=persona,
        aggression=bounded_aggression,
        defense_bias=0.7 + (0.55 - bounded_aggression * 0.3),
        fold_pressure=0.58,
        hard_fold_pressure=0.8,
        risk_tolerance=0.22 + bounded_aggression * 0.12,
        shanten_tolerance=0,
        chow_call_bias=0.8 + bounded_aggression * 0.26,
        pung_call_bias=0.8 + bounded_aggression * 0.16,
        kong_call_bias=0.9 + bounded_aggression * 0.12,
        closed_hand_bias=1.0,
    )


def _is_value_tile(
    tile_key: str,
    *,
    seat_index: int,
    dealer_seat: int,
    round_wind: str,
) -> bool:
    if tile_key in DRAGON_KEYS:
        return True
    if tile_key == round_wind:
        return True
    return tile_key == _seat_wind_key(seat_index, dealer_seat)


def _seat_wind_key(seat_index: int, dealer_seat: int) -> str:
    return WIND_ORDER[(seat_index - dealer_seat) % 4]


def _offset_tile(tile_key: str, delta: int) -> str | None:
    parsed = _parse_suit_rank(tile_key)
    if parsed is None:
        return None
    suit, rank = parsed
    next_rank = rank + delta
    if next_rank < 1 or next_rank > 9:
        return None
    return f"{suit}{next_rank}"


def _parse_suit_rank(tile_key: str) -> tuple[str, int] | None:
    if not tile_key:
        return None
    suit = tile_key[0]
    if suit not in {"w", "t", "b"}:
        return None
    try:
        rank = int(tile_key[1:])
    except ValueError:
        return None
    if rank < 1 or rank > 9:
        return None
    return suit, rank
