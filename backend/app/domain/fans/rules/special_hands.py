from __future__ import annotations

from app.domain.fans.models import FanRule


def get_special_hand_rules() -> tuple[FanRule, ...]:
    return (
        FanRule(
            fan_key="thirteen_orphans",
            fan_value=88,
            category="special_hands",
            matcher=lambda context: int(bool(context.get("features", {}).get("thirteen_orphans"))),
            excludes=("all_types", "concealed_hand", "single_wait"),
        ),
        FanRule(
            fan_key="seven_pairs",
            fan_value=24,
            category="special_hands",
            matcher=lambda context: int(bool(context.get("features", {}).get("seven_pairs"))),
            excludes=("concealed_hand", "single_wait"),
        ),
        FanRule(
            fan_key="seven_shifted_pairs",
            fan_value=88,
            category="special_hands",
            matcher=lambda context: int(_is_seven_shifted_pairs(context)),
            excludes=("seven_pairs", "full_flush", "concealed_hand", "one_voided_suit", "no_honours", "single_wait"),
        ),
        FanRule(
            fan_key="nine_gates",
            fan_value=88,
            category="special_hands",
            matcher=lambda context: int(_is_nine_gates(context)),
            excludes=("pung_of_terminals_or_honours", "full_flush", "concealed_hand", "one_voided_suit", "no_honours"),
        ),
        FanRule(
            fan_key="knitted_straight",
            fan_value=12,
            category="special_hands",
            matcher=lambda context: int(_has_decomposition_kind(context, "knitted_straight")),
        ),
        FanRule(
            fan_key="lesser_honours_and_knitted_tiles",
            fan_value=12,
            category="special_hands",
            matcher=lambda context: int(
                _has_decomposition_kind(context, "lesser_honours_and_knitted_tiles")
            ),
            excludes=("all_types", "concealed_hand"),
        ),
        FanRule(
            fan_key="greater_honours_and_knitted_tiles",
            fan_value=24,
            category="special_hands",
            matcher=lambda context: int(
                _has_decomposition_kind(context, "greater_honours_and_knitted_tiles")
            ),
            excludes=("all_types", "concealed_hand", "lesser_honours_and_knitted_tiles"),
        ),
    )


def _is_seven_shifted_pairs(context: dict) -> bool:
    pair_decomposition = next(
        (
            decomposition
            for decomposition in context.get("decompositions", [])
            if decomposition.get("kind") == "seven_pairs"
        ),
        None,
    )
    if pair_decomposition is None:
        return False
    pairs = pair_decomposition.get("pairs", [])
    if len(pairs) != 7:
        return False
    if not all(len(tile_key) == 2 and tile_key[0] in {"w", "t", "b"} for tile_key in pairs):
        return False
    suits = {tile_key[0] for tile_key in pairs}
    if len(suits) != 1:
        return False
    ranks = sorted(int(tile_key[1]) for tile_key in pairs)
    return ranks == list(range(ranks[0], ranks[0] + 7))


def _is_nine_gates(context: dict) -> bool:
    tile_keys = context.get("all_tile_keys", [])
    if len(tile_keys) != 14:
        return False
    if not all(len(tile_key) == 2 and tile_key[0] in {"w", "t", "b"} for tile_key in tile_keys):
        return False
    suits = {tile_key[0] for tile_key in tile_keys}
    if len(suits) != 1:
        return False
    counts: dict[str, int] = {}
    for tile_key in tile_keys:
        counts[tile_key] = counts.get(tile_key, 0) + 1
    suit = next(iter(suits))
    base = {
        f"{suit}1": 3,
        f"{suit}9": 3,
        f"{suit}2": 1,
        f"{suit}3": 1,
        f"{suit}4": 1,
        f"{suit}5": 1,
        f"{suit}6": 1,
        f"{suit}7": 1,
        f"{suit}8": 1,
    }
    remaining = dict(counts)
    for tile_key, needed in base.items():
        if remaining.get(tile_key, 0) < needed:
            return False
        remaining[tile_key] -= needed
        if remaining[tile_key] == 0:
            remaining.pop(tile_key)
    return sum(remaining.values()) == 1


def _has_decomposition_kind(context: dict, kind: str) -> bool:
    return any(decomposition.get("kind") == kind for decomposition in context.get("decompositions", []))
