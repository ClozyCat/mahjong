from __future__ import annotations

from collections import Counter

from app.domain.fans.models import FanRule


def get_triplet_pattern_rules() -> tuple[FanRule, ...]:
    return (
        FanRule(
            fan_key="all_pungs",
            fan_value=6,
            category="triplet_patterns",
            matcher=lambda context: int(
                bool(context.get("features", {}).get("pung_hand"))
                and not bool(context.get("features", {}).get("seven_pairs"))
            ),
        ),
        FanRule(
            fan_key="seat_wind",
            fan_value=2,
            category="triplet_patterns",
            matcher=lambda context: int(bool(context.get("features", {}).get("seat_wind_triplet"))),
        ),
        FanRule(
            fan_key="prevalent_wind",
            fan_value=2,
            category="triplet_patterns",
            matcher=lambda context: int(bool(context.get("features", {}).get("round_wind_triplet"))),
        ),
        FanRule(
            fan_key="dragon_pung",
            fan_value=2,
            category="triplet_patterns",
            matcher=lambda context: int(context.get("features", {}).get("dragon_triplet_count", 0) or 0),
        ),
        FanRule(
            fan_key="triple_pung",
            fan_value=16,
            category="triplet_patterns",
            matcher=lambda context: int(_has_triple_pung(context)),
            excludes=("double_pung",),
        ),
        FanRule(
            fan_key="double_pung",
            fan_value=2,
            category="triplet_patterns",
            matcher=lambda context: int(_has_double_pung(context)),
        ),
        FanRule(
            fan_key="mixed_shifted_pungs",
            fan_value=8,
            category="triplet_patterns",
            matcher=lambda context: int(_has_mixed_shifted_pungs(context)),
        ),
        FanRule(
            fan_key="pure_shifted_pungs",
            fan_value=24,
            category="triplet_patterns",
            matcher=lambda context: int(_has_pure_shifted_pungs(context)),
        ),
        FanRule(
            fan_key="pung_of_terminals_or_honours",
            fan_value=1,
            category="triplet_patterns",
            matcher=lambda context: int(_pung_of_terminals_or_honours_count(context)),
        ),
        FanRule(
            fan_key="two_concealed_pungs",
            fan_value=2,
            category="triplet_patterns",
            matcher=lambda context: int(_concealed_pung_count(context) >= 2),
        ),
        FanRule(
            fan_key="three_concealed_pungs",
            fan_value=16,
            category="triplet_patterns",
            matcher=lambda context: int(_concealed_pung_count(context) >= 3),
        ),
        FanRule(
            fan_key="four_pure_shifted_pungs",
            fan_value=48,
            category="triplet_patterns",
            matcher=lambda context: int(_has_four_pure_shifted_pungs(context)),
            excludes=("all_pungs", "pure_shifted_pungs"),
        ),
        FanRule(
            fan_key="four_concealed_pungs",
            fan_value=64,
            category="triplet_patterns",
            matcher=lambda context: int(_concealed_pung_count(context) >= 4),
            excludes=("all_pungs", "concealed_hand"),
        ),
    )


def _has_triple_pung(context: dict) -> bool:
    grouped: dict[int, set[str]] = {}
    for suit, rank in _suited_triplets(context):
        grouped.setdefault(rank, set()).add(suit)
    return any(suits == {"w", "t", "b"} for suits in grouped.values())


def _has_double_pung(context: dict) -> bool:
    grouped: dict[int, set[str]] = {}
    for suit, rank in _suited_triplets(context):
        grouped.setdefault(rank, set()).add(suit)
    return any(len(suits) >= 2 for suits in grouped.values())


def _has_mixed_shifted_pungs(context: dict) -> bool:
    grouped: dict[int, set[str]] = {}
    for suit, rank in _suited_triplets(context):
        grouped.setdefault(rank, set()).add(suit)
    for rank in range(1, 8):
        if not all(index in grouped for index in (rank, rank + 1, rank + 2)):
            continue
        if any(
            len({suit1, suit2, suit3}) == 3
            for suit1 in grouped[rank]
            for suit2 in grouped[rank + 1]
            for suit3 in grouped[rank + 2]
        ):
            return True
    return False


def _has_pure_shifted_pungs(context: dict) -> bool:
    grouped: dict[str, list[int]] = {}
    for suit, rank in _suited_triplets(context):
        grouped.setdefault(suit, []).append(rank)
    for ranks in grouped.values():
        counts = Counter(ranks)
        unique_ranks = sorted(counts)
        for step in (1, 2):
            for rank in unique_ranks:
                if all(counts.get(rank + offset * step, 0) >= 1 for offset in range(3)):
                    return True
    return False


def _has_four_pure_shifted_pungs(context: dict) -> bool:
    grouped: dict[str, set[int]] = {}
    for suit, rank in _suited_triplets(context):
        grouped.setdefault(suit, set()).add(rank)
    return any(all(rank + offset in ranks for offset in range(4)) for ranks in grouped.values() for rank in ranks)


def _concealed_pung_count(context: dict) -> int:
    concealed_tile_keys = list(context.get("concealed_tile_keys", []))
    concealed_kongs = sum(
        1
        for entry in context.get("kong_entries", [])
        if entry.get("actor_seat") == context.get("winner_seat")
        and entry.get("kong_type") == "concealed_kong"
    )
    best_standard = 0
    for decomposition in context.get("standard_decompositions", []):
        concealed_triplets = 0
        for meld in decomposition.get("melds", []):
            if len(meld) == 3 and len(set(meld)) == 1 and concealed_tile_keys.count(meld[0]) >= 3:
                concealed_triplets += 1
        best_standard = max(best_standard, concealed_triplets)
    return best_standard + concealed_kongs


def _pung_of_terminals_or_honours_count(context: dict) -> int:
    features = context.get("features", {})
    return int(features.get("terminal_triplet_count", 0) or 0) + int(
        features.get("non_seat_non_round_wind_triplet_count", 0) or 0
    )


def _suited_triplets(context: dict) -> list[tuple[str, int]]:
    triplets: list[tuple[str, int]] = []
    for decomposition in context.get("standard_decompositions", []):
        for meld in decomposition.get("melds", []):
            if len(meld) != 3:
                continue
            if len(set(meld)) != 1:
                continue
            tile_key = meld[0]
            if len(tile_key) != 2 or tile_key[0] not in {"w", "t", "b"} or not tile_key[1].isdigit():
                continue
            triplets.append((tile_key[0], int(tile_key[1])))
    return triplets
