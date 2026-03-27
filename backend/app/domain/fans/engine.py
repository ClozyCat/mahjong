from __future__ import annotations

from dataclasses import dataclass

from app.domain.fans.context import build_fan_context
from app.domain.fans.registry import get_registered_fan_rules

KONG_SCORES = {
    "exposed_kong": 1,
    "concealed_kong": 1,
    "add_kong": 1,
}
MCR_BASE_POINTS = 8


@dataclass(frozen=True)
class _FanCandidate:
    fan_key: str
    fan_value: int
    order: int
    excludes: tuple[str, ...]
    forbidden_with: tuple[str, ...]


def evaluate_fan_context(context: dict) -> dict:
    context = build_fan_context(**context)
    scenario_results = [_evaluate_scenario(scenario) for scenario in _fan_scenarios(context)]
    best_result = max(
        scenario_results,
        key=lambda result: (
            result["minimum_qualifying_fan_total"],
            result["fan_total"],
            len(result["fan_breakdown"]),
        ),
    )
    fan_keys = list(best_result["fan_keys"])
    fan_breakdown = list(best_result["fan_breakdown"])
    fan_total = int(best_result["fan_total"])
    minimum_qualifying_fan_total = int(best_result["minimum_qualifying_fan_total"])

    seat_count = int(context.get("seat_count", 4) or 4)
    winner_seat = context.get("winner_seat")
    discarder_seat = context.get("discarder_seat")
    kong_entries = _normalize_kong_entries(context.get("kong_entries", []), seat_count)
    kong_delta_by_seat = _sum_delta_by_seat(
        [entry["delta_by_seat"] for entry in kong_entries],
        seat_count,
    )
    fan_delta_by_seat = _fan_delta_by_seat(
        win_type=context.get("win_type"),
        winner_seat=winner_seat,
        discarder_seat=discarder_seat,
        fan_total=fan_total,
        seat_count=seat_count,
    )
    total_delta_by_seat = {
        seat: fan_delta_by_seat[seat] + kong_delta_by_seat[seat]
        for seat in range(seat_count)
    }

    score_delta = {
        "provisional": True,
        "basic_points": fan_total,
        "base_points": MCR_BASE_POINTS,
        "fan_total": fan_total,
        "minimum_qualifying_fan_total": minimum_qualifying_fan_total,
        "fan_delta_by_seat": fan_delta_by_seat,
        "kong_delta_by_seat": kong_delta_by_seat,
        "total_delta_by_seat": total_delta_by_seat,
    }

    return {
        "fan_total": fan_total,
        "minimum_qualifying_fan_total": minimum_qualifying_fan_total,
        "fan_keys": fan_keys,
        "fan_breakdown": fan_breakdown,
        "score_delta": score_delta,
        "kong_score_detail": kong_entries,
        "provisional": True,
    }


def _fan_scenarios(context: dict) -> list[dict]:
    standard_decompositions = list(context.get("standard_decompositions", []))
    non_standard_decompositions = [
        decomposition
        for decomposition in context.get("decompositions", [])
        if decomposition.get("kind") != "standard"
    ]
    scenarios: list[dict] = []

    if non_standard_decompositions:
        scenarios.append(
            {
                **context,
                "decompositions": non_standard_decompositions,
                "decomposition_kinds": [
                    decomposition.get("kind") for decomposition in non_standard_decompositions
                ],
                "standard_decompositions": [],
            }
        )

    for decomposition in standard_decompositions:
        scenarios.append(
            {
                **context,
                "decompositions": [decomposition],
                "decomposition_kinds": [decomposition.get("kind")],
                "standard_decompositions": [decomposition],
            }
        )

    if not scenarios:
        scenarios.append(context)
    return scenarios


def _evaluate_scenario(context: dict) -> dict:
    candidates = _fan_candidates(context)
    selected = _select_best_candidates(candidates)
    fan_keys = [candidate.fan_key for candidate in selected]
    fan_breakdown = [
        {"fan_key": candidate.fan_key, "fan_value": candidate.fan_value}
        for candidate in selected
    ]

    if _should_award_chicken_hand(context, fan_keys):
        fan_keys.append("chicken_hand")
        fan_breakdown.append({"fan_key": "chicken_hand", "fan_value": 8})

    fan_total = sum(entry["fan_value"] for entry in fan_breakdown)
    minimum_qualifying_fan_total = sum(
        entry["fan_value"] for entry in fan_breakdown if entry["fan_key"] != "flower_tiles"
    )
    return {
        "fan_keys": fan_keys,
        "fan_breakdown": fan_breakdown,
        "fan_total": fan_total,
        "minimum_qualifying_fan_total": minimum_qualifying_fan_total,
    }


def _fan_candidates(context: dict) -> list[_FanCandidate]:
    candidates: list[_FanCandidate] = []
    for order, rule in enumerate(get_registered_fan_rules()):
        match_count = int(rule.matcher(context) or 0)
        resolved_values = (
            rule.value_resolver(context, match_count, rule.fan_value)
            if rule.value_resolver is not None
            else [rule.fan_value] * match_count
        )
        for resolved_value in resolved_values:
            candidates.append(
                _FanCandidate(
                    fan_key=rule.fan_key,
                    fan_value=resolved_value,
                    order=order,
                    excludes=rule.excludes,
                    forbidden_with=rule.forbidden_with,
                )
            )
    return candidates


def _select_best_candidates(candidates: list[_FanCandidate]) -> list[_FanCandidate]:
    ordered = sorted(candidates, key=lambda candidate: (-candidate.fan_value, candidate.order))
    suffix_sum = [0] * (len(ordered) + 1)
    for index in range(len(ordered) - 1, -1, -1):
        suffix_sum[index] = suffix_sum[index + 1] + ordered[index].fan_value

    best_score = -1
    best_selected: list[_FanCandidate] = []

    def dfs(
        index: int,
        score: int,
        selected: list[_FanCandidate],
        selected_keys: frozenset[str],
        blocked_keys: frozenset[str],
    ) -> None:
        nonlocal best_score, best_selected
        if score + suffix_sum[index] < best_score:
            return
        if index >= len(ordered):
            if score > best_score:
                best_score = score
                best_selected = list(selected)
            return

        dfs(index + 1, score, selected, selected_keys, blocked_keys)

        candidate = ordered[index]
        candidate_conflicts = set(candidate.excludes) | set(candidate.forbidden_with)
        if candidate.fan_key in blocked_keys:
            return
        if selected_keys & candidate_conflicts:
            return

        selected.append(candidate)
        dfs(
            index + 1,
            score + candidate.fan_value,
            selected,
            selected_keys | {candidate.fan_key},
            blocked_keys | candidate_conflicts,
        )
        selected.pop()

    dfs(0, 0, [], frozenset(), frozenset())
    return sorted(best_selected, key=lambda candidate: candidate.order)


def _fan_delta_by_seat(
    *,
    win_type: str | None,
    winner_seat: int | None,
    discarder_seat: int | None,
    fan_total: int,
    seat_count: int,
) -> dict[int, int]:
    deltas = {seat: 0 for seat in range(seat_count)}
    if winner_seat is None or fan_total <= 0:
        return deltas

    if win_type == "self_draw":
        payment = fan_total + MCR_BASE_POINTS
        for seat in range(seat_count):
            if seat == winner_seat:
                continue
            deltas[seat] -= payment
            deltas[winner_seat] += payment
        return deltas

    if discarder_seat is not None:
        deltas[winner_seat] += fan_total + (MCR_BASE_POINTS * (seat_count - 1))
        for seat in range(seat_count):
            if seat == winner_seat:
                continue
            if seat == discarder_seat:
                deltas[seat] -= fan_total + MCR_BASE_POINTS
            else:
                deltas[seat] -= MCR_BASE_POINTS
    return deltas


def _normalize_kong_entries(raw_entries: list[dict], seat_count: int) -> list[dict]:
    normalized: list[dict] = []
    for entry in raw_entries:
        kong_type = entry["kong_type"]
        actor_seat = entry["actor_seat"]
        payer_seats = list(entry["payer_seats"])
        unit_score = KONG_SCORES[kong_type]
        delta_by_seat = {seat: 0 for seat in range(seat_count)}
        for payer in payer_seats:
            delta_by_seat[payer] -= unit_score
            delta_by_seat[actor_seat] += unit_score
        normalized.append(
            {
                "kong_type": kong_type,
                "actor_seat": actor_seat,
                "payer_seats": payer_seats,
                "delta_by_seat": delta_by_seat,
            }
        )
    return normalized


def _sum_delta_by_seat(delta_maps: list[dict[int, int]], seat_count: int) -> dict[int, int]:
    totals = {seat: 0 for seat in range(seat_count)}
    for delta_map in delta_maps:
        for seat in range(seat_count):
            totals[seat] += delta_map.get(seat, 0)
    return totals


def _should_award_chicken_hand(context: dict, fan_keys: list[str]) -> bool:
    if len(context.get("all_tile_keys", [])) != 14:
        return False
    non_flower_fans = [fan_key for fan_key in fan_keys if fan_key != "flower_tiles"]
    return not non_flower_fans
