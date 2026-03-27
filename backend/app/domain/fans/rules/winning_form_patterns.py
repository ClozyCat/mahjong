from __future__ import annotations

from app.domain.fans.models import FanRule


def get_winning_form_pattern_rules() -> tuple[FanRule, ...]:
    return (
        FanRule(
            fan_key="all_chows",
            fan_value=2,
            category="winning_form_patterns",
            matcher=lambda context: int(_is_all_chows(context)),
            excludes=("no_honours",),
        ),
        FanRule(
            fan_key="outside_hand",
            fan_value=4,
            category="winning_form_patterns",
            matcher=lambda context: int(_is_outside_hand(context)),
        ),
        FanRule(
            fan_key="edge_wait",
            fan_value=1,
            category="winning_form_patterns",
            matcher=lambda context: int("edge_wait" in context.get("wait_types", [])),
            excludes=("closed_wait", "single_wait"),
        ),
        FanRule(
            fan_key="closed_wait",
            fan_value=1,
            category="winning_form_patterns",
            matcher=lambda context: int("closed_wait" in context.get("wait_types", [])),
            excludes=("edge_wait", "single_wait"),
        ),
        FanRule(
            fan_key="single_wait",
            fan_value=1,
            category="winning_form_patterns",
            matcher=lambda context: int("single_wait" in context.get("wait_types", [])),
            excludes=("edge_wait", "closed_wait"),
        ),
    )


def _is_all_chows(context: dict) -> bool:
    standard_decompositions = context.get("standard_decompositions", [])
    if not standard_decompositions:
        return False
    if not context.get("all_tile_keys"):
        return False
    if any(tile_key in {"east", "south", "west", "north", "red", "green", "white"} for tile_key in context.get("all_tile_keys", [])):
        return False
    for decomposition in standard_decompositions:
        melds = decomposition.get("melds", [])
        if melds and all(_is_sequence(meld) for meld in melds):
            return True
    return False


def _is_outside_hand(context: dict) -> bool:
    standard_decompositions = context.get("standard_decompositions", [])
    if not standard_decompositions:
        return False
    for decomposition in standard_decompositions:
        pair_tile = decomposition.get("pair")
        if not _is_terminal_or_honor(pair_tile):
            continue
        melds = decomposition.get("melds", [])
        if melds and all(_meld_has_terminal_or_honor(meld) for meld in melds):
            return True
    return False


def _is_sequence(meld: list[str]) -> bool:
    if len(meld) != 3:
        return False
    if not all(_is_suit_tile(tile_key) for tile_key in meld):
        return False
    suit = meld[0][0]
    ranks = sorted(int(tile_key[1:]) for tile_key in meld)
    return all(tile_key[0] == suit for tile_key in meld) and ranks == [ranks[0], ranks[0] + 1, ranks[0] + 2]


def _meld_has_terminal_or_honor(meld: list[str]) -> bool:
    if len(meld) != 3:
        return False
    if len(set(meld)) == 1:
        return _is_terminal_or_honor(meld[0])
    return any(_is_terminal(tile_key) for tile_key in meld)


def _is_terminal_or_honor(tile_key: str | None) -> bool:
    if tile_key is None:
        return False
    return not _is_suit_tile(tile_key) or _is_terminal(tile_key)


def _is_terminal(tile_key: str) -> bool:
    return _is_suit_tile(tile_key) and tile_key[1] in {"1", "9"}


def _is_suit_tile(tile_key: str) -> bool:
    return (
        len(tile_key) == 2
        and tile_key[0] in {"w", "t", "b"}
        and tile_key[1].isdigit()
    )
