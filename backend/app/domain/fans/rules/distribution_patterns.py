from __future__ import annotations

from app.domain.fans.models import FanRule

WIND_KEYS = {"east", "south", "west", "north"}
DRAGON_KEYS = {"red", "green", "white"}
REVERSIBLE_TILE_KEYS = {
    "b2",
    "b4",
    "b5",
    "b6",
    "b8",
    "b9",
    "t1",
    "t2",
    "t3",
    "t4",
    "t5",
    "t8",
    "t9",
    "white",
}


def get_distribution_pattern_rules() -> tuple[FanRule, ...]:
    return (
        FanRule(
            fan_key="all_types",
            fan_value=6,
            category="distribution_patterns",
            matcher=lambda context: int(_has_all_types(context)),
        ),
        FanRule(
            fan_key="all_fives",
            fan_value=16,
            category="distribution_patterns",
            matcher=lambda context: int(_has_all_fives(context)),
            excludes=("no_honours", "all_simples"),
        ),
        FanRule(
            fan_key="upper_four",
            fan_value=12,
            category="distribution_patterns",
            matcher=lambda context: int(_is_upper_four(context)),
            excludes=("no_honours",),
        ),
        FanRule(
            fan_key="upper_tiles",
            fan_value=24,
            category="distribution_patterns",
            matcher=lambda context: int(_is_upper_tiles(context)),
            excludes=("no_honours", "upper_four"),
        ),
        FanRule(
            fan_key="lower_four",
            fan_value=12,
            category="distribution_patterns",
            matcher=lambda context: int(_is_lower_four(context)),
            excludes=("no_honours",),
        ),
        FanRule(
            fan_key="lower_tiles",
            fan_value=24,
            category="distribution_patterns",
            matcher=lambda context: int(_is_lower_tiles(context)),
            excludes=("no_honours", "lower_four"),
        ),
        FanRule(
            fan_key="middle_tiles",
            fan_value=24,
            category="distribution_patterns",
            matcher=lambda context: int(_is_middle_tiles(context)),
            excludes=("no_honours", "all_simples"),
        ),
        FanRule(
            fan_key="tile_hog",
            fan_value=2,
            category="distribution_patterns",
            matcher=lambda context: int(_has_tile_hog(context)),
        ),
        FanRule(
            fan_key="reversible_tiles",
            fan_value=8,
            category="distribution_patterns",
            matcher=lambda context: int(_has_reversible_tiles(context)),
            excludes=("one_voided_suit",),
        ),
    )


def _has_all_types(context: dict) -> bool:
    tile_keys = context.get("all_tile_keys", [])
    suits = {
        tile_key[0]
        for tile_key in tile_keys
        if _is_suit_tile(tile_key)
    }
    has_wind = any(tile_key in WIND_KEYS for tile_key in tile_keys)
    has_dragon = any(tile_key in DRAGON_KEYS for tile_key in tile_keys)
    return suits == {"w", "t", "b"} and has_wind and has_dragon


def _has_all_fives(context: dict) -> bool:
    pair = None
    melds: list[list[str]] = []
    for decomposition in context.get("standard_decompositions", []):
        pair = decomposition.get("pair")
        melds = decomposition.get("melds", [])
        break
    if pair is None:
        return False
    if pair != "w5" and pair != "t5" and pair != "b5":
        return False
    for meld in melds:
        if not any(tile_key in {"w5", "t5", "b5"} for tile_key in meld):
            return False
    return bool(melds)


def _is_upper_four(context: dict) -> bool:
    tile_keys = context.get("all_tile_keys", [])
    return bool(tile_keys) and all(
        _is_suit_tile(tile_key) and int(tile_key[1]) >= 6 for tile_key in tile_keys
    )


def _is_upper_tiles(context: dict) -> bool:
    tile_keys = context.get("all_tile_keys", [])
    return bool(tile_keys) and all(
        _is_suit_tile(tile_key) and int(tile_key[1]) in {7, 8, 9}
        for tile_key in tile_keys
    )


def _is_lower_four(context: dict) -> bool:
    tile_keys = context.get("all_tile_keys", [])
    return bool(tile_keys) and all(
        _is_suit_tile(tile_key) and int(tile_key[1]) <= 4 for tile_key in tile_keys
    )


def _is_lower_tiles(context: dict) -> bool:
    tile_keys = context.get("all_tile_keys", [])
    return bool(tile_keys) and all(
        _is_suit_tile(tile_key) and int(tile_key[1]) in {1, 2, 3}
        for tile_key in tile_keys
    )


def _is_middle_tiles(context: dict) -> bool:
    tile_keys = context.get("all_tile_keys", [])
    return bool(tile_keys) and all(
        _is_suit_tile(tile_key) and int(tile_key[1]) in {4, 5, 6} for tile_key in tile_keys
    )


def _has_tile_hog(context: dict) -> bool:
    counts: dict[str, int] = {}
    for tile_key in context.get("all_tile_keys", []):
        counts[tile_key] = counts.get(tile_key, 0) + 1
    return any(count >= 4 for count in counts.values())


def _has_reversible_tiles(context: dict) -> bool:
    tile_keys = context.get("all_tile_keys", [])
    return bool(tile_keys) and all(tile_key in REVERSIBLE_TILE_KEYS for tile_key in tile_keys)


def _is_suit_tile(tile_key: str) -> bool:
    return (
        len(tile_key) == 2
        and tile_key[0] in {"w", "t", "b"}
        and tile_key[1].isdigit()
    )
