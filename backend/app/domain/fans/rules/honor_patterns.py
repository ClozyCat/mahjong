from __future__ import annotations

from collections import Counter

from app.domain.fans.models import FanRule

WIND_KEYS = {"east", "south", "west", "north"}
DRAGON_KEYS = {"red", "green", "white"}
HONOR_KEYS = WIND_KEYS | DRAGON_KEYS
ALL_GREEN_KEYS = {"t2", "t3", "t4", "t6", "t8", "green"}


def get_honor_pattern_rules() -> tuple[FanRule, ...]:
    return (
        FanRule(
            fan_key="big_three_winds",
            fan_value=12,
            category="honor_patterns",
            matcher=lambda context: int(_is_big_three_winds(context)),
            excludes=("pung_of_terminals_or_honours",),
        ),
        FanRule(
            fan_key="big_three_dragons",
            fan_value=88,
            category="honor_patterns",
            matcher=lambda context: int(_is_big_three_dragons(context)),
            excludes=("dragon_pung", "two_dragon_pungs"),
        ),
        FanRule(
            fan_key="two_dragon_pungs",
            fan_value=6,
            category="honor_patterns",
            matcher=lambda context: int(_is_two_dragon_pungs(context)),
            excludes=("dragon_pung",),
        ),
        FanRule(
            fan_key="little_three_dragons",
            fan_value=64,
            category="honor_patterns",
            matcher=lambda context: int(_is_little_three_dragons(context)),
            excludes=("dragon_pung", "two_dragon_pungs"),
        ),
        FanRule(
            fan_key="big_four_winds",
            fan_value=88,
            category="honor_patterns",
            matcher=lambda context: int(_is_big_four_winds(context)),
            excludes=("pung_of_terminals_or_honours", "prevalent_wind", "seat_wind", "big_three_winds", "all_pungs"),
        ),
        FanRule(
            fan_key="little_four_winds",
            fan_value=64,
            category="honor_patterns",
            matcher=lambda context: int(_is_little_four_winds(context)),
            excludes=("pung_of_terminals_or_honours", "big_three_winds"),
        ),
        FanRule(
            fan_key="all_honours",
            fan_value=64,
            category="honor_patterns",
            matcher=lambda context: int(_is_all_honors(context)),
            excludes=("pung_of_terminals_or_honours", "outside_hand", "all_pungs"),
        ),
        FanRule(
            fan_key="all_terminals_and_honours",
            fan_value=32,
            category="honor_patterns",
            matcher=lambda context: int(_is_all_terminals_and_honors(context)),
            excludes=("pung_of_terminals_or_honours", "outside_hand", "all_pungs"),
        ),
        FanRule(
            fan_key="all_terminals",
            fan_value=88,
            category="honor_patterns",
            matcher=lambda context: int(_is_all_terminals(context)),
            excludes=("pung_of_terminals_or_honours", "outside_hand", "all_pungs", "no_honours"),
        ),
        FanRule(
            fan_key="all_even_pungs",
            fan_value=24,
            category="honor_patterns",
            matcher=lambda context: int(_is_all_even_pungs(context)),
            excludes=("all_pungs", "no_honours", "all_simples"),
        ),
        FanRule(
            fan_key="all_green",
            fan_value=88,
            category="honor_patterns",
            matcher=lambda context: int(_is_all_green(context)),
        ),
    )


def _is_big_three_winds(context: dict) -> bool:
    wind_triplet_count = len(_triplet_keys(context) & WIND_KEYS)
    return wind_triplet_count >= 3


def _is_big_three_dragons(context: dict) -> bool:
    return _triplet_keys(context).issuperset(DRAGON_KEYS)


def _is_two_dragon_pungs(context: dict) -> bool:
    return len(_triplet_keys(context) & DRAGON_KEYS) >= 2


def _is_little_three_dragons(context: dict) -> bool:
    pair_tile = _pair_tile(context)
    return (
        len(_triplet_keys(context) & DRAGON_KEYS) == 2
        and pair_tile in DRAGON_KEYS
    )


def _is_big_four_winds(context: dict) -> bool:
    return _triplet_keys(context).issuperset(WIND_KEYS)


def _is_little_four_winds(context: dict) -> bool:
    pair_tile = _pair_tile(context)
    return (
        len(_triplet_keys(context) & WIND_KEYS) == 3
        and pair_tile in WIND_KEYS
    )


def _is_all_honors(context: dict) -> bool:
    tile_keys = context.get("all_tile_keys", [])
    return bool(tile_keys) and all(tile_key in HONOR_KEYS for tile_key in tile_keys)


def _is_all_terminals_and_honors(context: dict) -> bool:
    tile_keys = context.get("all_tile_keys", [])
    has_honors = any(tile_key in HONOR_KEYS for tile_key in tile_keys)
    has_terminals = any(_is_terminal(tile_key) for tile_key in tile_keys)
    return bool(tile_keys) and has_honors and has_terminals and all(
        tile_key in HONOR_KEYS or _is_terminal(tile_key) for tile_key in tile_keys
    )


def _is_all_terminals(context: dict) -> bool:
    tile_keys = context.get("all_tile_keys", [])
    return bool(tile_keys) and all(_is_terminal(tile_key) for tile_key in tile_keys)


def _is_all_even_pungs(context: dict) -> bool:
    if not bool(context.get("features", {}).get("pung_hand")):
        return False
    tile_keys = context.get("all_tile_keys", [])
    return bool(tile_keys) and all(_is_even_tile(tile_key) for tile_key in tile_keys)


def _is_all_green(context: dict) -> bool:
    tile_keys = context.get("all_tile_keys", [])
    return bool(tile_keys) and all(tile_key in ALL_GREEN_KEYS for tile_key in tile_keys)


def _triplet_keys(context: dict) -> set[str]:
    triplets: set[str] = set()
    for decomposition in context.get("standard_decompositions", []):
        for meld in decomposition.get("melds", []):
            counts = Counter(meld)
            if len(meld) == 3 and len(counts) == 1:
                triplets.add(meld[0])
    return triplets


def _pair_tile(context: dict) -> str | None:
    for decomposition in context.get("standard_decompositions", []):
        pair_tile = decomposition.get("pair")
        if isinstance(pair_tile, str):
            return pair_tile
    return None


def _is_terminal(tile_key: str) -> bool:
    return len(tile_key) >= 2 and tile_key[0] in {"w", "t", "b"} and tile_key[1:] in {"1", "9"}


def _is_even_tile(tile_key: str) -> bool:
    return len(tile_key) >= 2 and tile_key[0] in {"w", "t", "b"} and tile_key[1:] in {"2", "4", "6", "8"}
