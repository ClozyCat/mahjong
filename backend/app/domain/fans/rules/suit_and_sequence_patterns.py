from __future__ import annotations

from collections import Counter

from app.domain.fans.models import FanRule


def get_suit_and_sequence_pattern_rules() -> tuple[FanRule, ...]:
    return (
        FanRule(
            fan_key="pure_straight",
            fan_value=16,
            category="suit_and_sequence_patterns",
            matcher=lambda context: int(_has_pure_straight(context)),
            excludes=("short_straight", "two_terminal_chows"),
        ),
        FanRule(
            fan_key="mixed_triple_chow",
            fan_value=8,
            category="suit_and_sequence_patterns",
            matcher=lambda context: int(_has_mixed_triple_chow(context)),
            excludes=("mixed_double_chow",),
        ),
        FanRule(
            fan_key="pure_double_chow",
            fan_value=1,
            category="suit_and_sequence_patterns",
            matcher=lambda context: int(_has_pure_double_chow(context)),
        ),
        FanRule(
            fan_key="mixed_double_chow",
            fan_value=1,
            category="suit_and_sequence_patterns",
            matcher=lambda context: int(_has_mixed_double_chow(context)),
        ),
        FanRule(
            fan_key="mixed_straight",
            fan_value=8,
            category="suit_and_sequence_patterns",
            matcher=lambda context: int(_has_mixed_straight(context)),
        ),
        FanRule(
            fan_key="mixed_shifted_chows",
            fan_value=6,
            category="suit_and_sequence_patterns",
            matcher=lambda context: int(_has_mixed_shifted_chows(context)),
        ),
        FanRule(
            fan_key="pure_shifted_chows",
            fan_value=16,
            category="suit_and_sequence_patterns",
            matcher=lambda context: int(_has_pure_shifted_chows(context)),
        ),
        FanRule(
            fan_key="four_pure_shifted_chows",
            fan_value=32,
            category="suit_and_sequence_patterns",
            matcher=lambda context: int(_has_four_pure_shifted_chows(context)),
            excludes=("pure_shifted_chows", "short_straight", "two_terminal_chows"),
        ),
        FanRule(
            fan_key="pure_triple_chow",
            fan_value=24,
            category="suit_and_sequence_patterns",
            matcher=lambda context: int(_has_pure_triple_chow(context)),
            excludes=("pure_double_chow",),
        ),
        FanRule(
            fan_key="quadruple_chow",
            fan_value=48,
            category="suit_and_sequence_patterns",
            matcher=lambda context: int(_has_quadruple_chows(context)),
            excludes=("pure_double_chow", "pure_triple_chow", "tile_hog"),
        ),
        FanRule(
            fan_key="short_straight",
            fan_value=1,
            category="suit_and_sequence_patterns",
            matcher=lambda context: int(_has_short_straight(context)),
        ),
        FanRule(
            fan_key="two_terminal_chows",
            fan_value=1,
            category="suit_and_sequence_patterns",
            matcher=lambda context: int(_has_two_terminal_chows(context)),
        ),
        FanRule(
            fan_key="three_suited_terminal_chows",
            fan_value=16,
            category="suit_and_sequence_patterns",
            matcher=lambda context: int(_has_three_suited_terminal_chows(context)),
            excludes=("all_chows", "mixed_double_chow", "two_terminal_chows", "no_honours"),
        ),
        FanRule(
            fan_key="pure_terminal_chows",
            fan_value=64,
            category="suit_and_sequence_patterns",
            matcher=lambda context: int(_has_pure_terminal_chows(context)),
            excludes=("all_chows", "pure_double_chow", "two_terminal_chows", "full_flush", "one_voided_suit", "no_honours"),
        ),
        FanRule(
            fan_key="one_voided_suit",
            fan_value=1,
            category="suit_and_sequence_patterns",
            matcher=lambda context: int(_has_one_voided_suit(context)),
        ),
        FanRule(
            fan_key="no_honours",
            fan_value=1,
            category="suit_and_sequence_patterns",
            matcher=lambda context: int(_has_no_honors(context)),
        ),
    )


def _has_pure_straight(context: dict) -> bool:
    for sequences in _sequence_groups_by_suit(context).values():
        starts = {start for start, _ in sequences}
        if {1, 4, 7}.issubset(starts):
            return True
    return False


def _has_mixed_triple_chow(context: dict) -> bool:
    grouped: dict[int, set[str]] = {}
    for suit, sequences in _sequence_groups_by_suit(context).items():
        for start, _ in sequences:
            grouped.setdefault(start, set()).add(suit)
    return any(suits == {"w", "t", "b"} for suits in grouped.values())


def _has_pure_double_chow(context: dict) -> bool:
    for sequences in _sequence_groups_by_suit(context).values():
        counts = Counter(sequence for _, sequence in sequences)
        if any(count >= 2 for count in counts.values()):
            return True
    return False


def _has_mixed_double_chow(context: dict) -> bool:
    grouped: dict[int, set[str]] = {}
    for suit, sequences in _sequence_groups_by_suit(context).items():
        for start, _ in sequences:
            grouped.setdefault(start, set()).add(suit)
    return any(len(suits) >= 2 for suits in grouped.values())


def _has_mixed_straight(context: dict) -> bool:
    grouped: dict[int, set[str]] = {}
    for suit, sequences in _sequence_groups_by_suit(context).items():
        for start, _ in sequences:
            grouped.setdefault(start, set()).add(suit)
    return (
        1 in grouped
        and 4 in grouped
        and 7 in grouped
        and all(grouped[start] for start in (1, 4, 7))
        and any(
            suit1 != suit2 != suit3
            for suit1 in grouped[1]
            for suit2 in grouped[4]
            for suit3 in grouped[7]
            if len({suit1, suit2, suit3}) == 3
        )
    )


def _has_mixed_shifted_chows(context: dict) -> bool:
    grouped: dict[int, set[str]] = {}
    for suit, sequences in _sequence_groups_by_suit(context).items():
        for start, _ in sequences:
            grouped.setdefault(start, set()).add(suit)
    for start in range(1, 6):
        if start not in grouped or start + 1 not in grouped or start + 2 not in grouped:
            continue
        if any(
            len({suit1, suit2, suit3}) == 3
            for suit1 in grouped[start]
            for suit2 in grouped[start + 1]
            for suit3 in grouped[start + 2]
        ):
            return True
    return False


def _has_pure_shifted_chows(context: dict) -> bool:
    for sequences in _sequence_groups_by_suit(context).values():
        starts = Counter(start for start, _ in sequences)
        unique_starts = sorted(starts)
        for step in (1, 2):
            for start in unique_starts:
                if all(starts.get(start + offset * step, 0) >= 1 for offset in range(3)):
                    return True
    return False


def _has_four_pure_shifted_chows(context: dict) -> bool:
    for sequences in _sequence_groups_by_suit(context).values():
        starts = Counter(start for start, _ in sequences)
        unique_starts = sorted(starts)
        for step in (1, 2):
            for start in unique_starts:
                if all(starts.get(start + offset * step, 0) >= 1 for offset in range(4)):
                    return True
    return False


def _has_pure_triple_chow(context: dict) -> bool:
    for sequences in _sequence_groups_by_suit(context).values():
        counts = Counter(sequence for _, sequence in sequences)
        if any(count >= 3 for count in counts.values()):
            return True
    return False


def _has_quadruple_chows(context: dict) -> bool:
    for sequences in _sequence_groups_by_suit(context).values():
        counts = Counter(sequence for _, sequence in sequences)
        if any(count >= 4 for count in counts.values()):
            return True
    return False


def _has_short_straight(context: dict) -> bool:
    for sequences in _sequence_groups_by_suit(context).values():
        starts = {start for start, _ in sequences}
        if {1, 4}.issubset(starts) or {4, 7}.issubset(starts):
            return True
    return False


def _has_two_terminal_chows(context: dict) -> bool:
    for sequences in _sequence_groups_by_suit(context).values():
        starts = {start for start, _ in sequences}
        if {1, 7}.issubset(starts):
            return True
    return False


def _has_three_suited_terminal_chows(context: dict) -> bool:
    terminal_suits: set[str] = set()
    for suit, sequences in _sequence_groups_by_suit(context).items():
        starts = {start for start, _ in sequences}
        if {1, 7}.issubset(starts):
            terminal_suits.add(suit)
    return len(terminal_suits) >= 2


def _has_pure_terminal_chows(context: dict) -> bool:
    for suit, sequences in _sequence_groups_by_suit(context).items():
        sequence_counts = Counter(sequence for _, sequence in sequences)
        starts = {start for start, _ in sequences}
        if (
            starts.issuperset({1, 7})
            and sequence_counts.get((f"{suit}1", f"{suit}2", f"{suit}3"), 0) >= 2
            and sequence_counts.get((f"{suit}7", f"{suit}8", f"{suit}9"), 0) >= 2
        ):
            return True
    return False


def _has_one_voided_suit(context: dict) -> bool:
    suits = {
        tile_key[0]
        for tile_key in context.get("all_tile_keys", [])
        if len(tile_key) >= 2 and tile_key[0] in {"w", "t", "b"}
    }
    return len(suits) == 2


def _has_no_honors(context: dict) -> bool:
    tile_keys = context.get("all_tile_keys", [])
    return bool(tile_keys) and all(
        len(tile_key) >= 2 and tile_key[0] in {"w", "t", "b"} for tile_key in tile_keys
    )


def _sequence_groups_by_suit(context: dict) -> dict[str, list[tuple[int, tuple[str, str, str]]]]:
    grouped: dict[str, list[tuple[int, tuple[str, str, str]]]] = {}
    for decomposition in context.get("standard_decompositions", []):
        for meld in decomposition.get("melds", []):
            if len(meld) != 3:
                continue
            if not all(_is_suit_tile(tile_key) for tile_key in meld):
                continue
            suit = meld[0][0]
            ranks = sorted(int(tile_key[1:]) for tile_key in meld)
            if all(tile_key[0] == suit for tile_key in meld) and ranks == [ranks[0], ranks[0] + 1, ranks[0] + 2]:
                grouped.setdefault(suit, []).append((ranks[0], tuple(meld)))
    return grouped


def _is_suit_tile(tile_key: str) -> bool:
    return (
        len(tile_key) == 2
        and tile_key[0] in {"w", "t", "b"}
        and tile_key[1].isdigit()
    )
