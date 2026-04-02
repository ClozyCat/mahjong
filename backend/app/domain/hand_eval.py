from collections import Counter
from itertools import permutations

SUIT_KEYS = ("w", "t", "b")
HONOR_KEYS = {"east", "south", "west", "north", "red", "green", "white"}
KNITTED_GROUPS = ((1, 4, 7), (2, 5, 8), (3, 6, 9))


def _parse_suit(tile_key: str) -> tuple[str, int] | None:
    if not tile_key:
        return None
    prefix = tile_key[0]
    if prefix not in {"w", "t", "b"}:
        return None
    try:
        rank = int(tile_key[1:])
    except ValueError:
        return None
    if rank < 1 or rank > 9:
        return None
    return prefix, rank


def _is_seven_pairs(counts: Counter[str]) -> bool:
    if sum(counts.values()) != 14:
        return False
    pair_count = 0
    for count in counts.values():
        if count not in {2, 4}:
            return False
        pair_count += count // 2
    return pair_count == 7


def _seven_pairs_pair_tiles(counts: Counter[str]) -> list[str]:
    pair_tiles: list[str] = []
    for tile_key in sorted(counts):
        pair_tiles.extend([tile_key] * (counts[tile_key] // 2))
    return pair_tiles


def _is_thirteen_orphans(counts: Counter[str]) -> bool:
    required = {
        "w1",
        "w9",
        "t1",
        "t9",
        "b1",
        "b9",
        "east",
        "south",
        "west",
        "north",
        "red",
        "green",
        "white",
    }
    if any(tile_key not in required for tile_key in counts):
        return False
    if any(count == 0 for count in (counts.get(key, 0) for key in required)):
        return False
    duplicate_count = sum(1 for key in required if counts.get(key, 0) == 2)
    return duplicate_count == 1 and sum(counts.values()) == 14


def _knitted_patterns() -> tuple[frozenset[str], ...]:
    return tuple(
        frozenset(
            f"{suit}{rank}"
            for suit, ranks in zip(order, KNITTED_GROUPS, strict=True)
            for rank in ranks
        )
        for order in permutations(SUIT_KEYS)
    )


KNITTED_PATTERNS = _knitted_patterns()


def _five_tile_completion(counts: Counter[str]) -> dict | None:
    if sum(counts.values()) != 5:
        return None

    for pair_tile, count in list(counts.items()):
        if count < 2:
            continue
        next_counts = counts.copy()
        next_counts[pair_tile] -= 2
        if next_counts[pair_tile] == 0:
            del next_counts[pair_tile]

        if len(next_counts) == 1:
            meld_tile, meld_count = next(iter(next_counts.items()))
            if meld_count == 3:
                return {
                    "pair": pair_tile,
                    "meld": [meld_tile, meld_tile, meld_tile],
                    "completion_kind": "pung_and_pair",
                }

        melds = _extract_all_melds(next_counts)
        if melds:
            meld = melds[0][0]
            return {
                "pair": pair_tile,
                "meld": meld,
                "completion_kind": "chow_and_pair" if len(set(meld)) == 3 else "pung_and_pair",
            }
    return None


def _special_knitted_decompositions(counts: Counter[str]) -> list[dict]:
    decompositions: list[dict] = []
    seen: set[tuple] = set()
    is_all_singletons = all(count == 1 for count in counts.values())
    honor_tiles = sorted(tile_key for tile_key in counts if tile_key in HONOR_KEYS)

    for pattern in KNITTED_PATTERNS:
        if pattern.issubset(counts):
            remaining = counts.copy()
            for tile_key in pattern:
                remaining[tile_key] -= 1
                if remaining[tile_key] == 0:
                    del remaining[tile_key]

            if not remaining:
                continue

            if set(remaining).issubset(HONOR_KEYS) and len(remaining) == 5 and all(
                count == 1 for count in remaining.values()
            ):
                signature = ("knitted_straight", tuple(sorted(pattern)), tuple(sorted(remaining)))
                if signature not in seen:
                    seen.add(signature)
                    decompositions.append(
                        {
                            "kind": "knitted_straight",
                            "pattern_tiles": sorted(pattern),
                            "honor_tiles": sorted(remaining),
                            "completion_kind": "honours",
                        }
                    )

            completion = _five_tile_completion(remaining)
            if completion is not None:
                signature = (
                    "knitted_straight",
                    tuple(sorted(pattern)),
                    completion["pair"],
                    tuple(completion["meld"]),
                )
                if signature not in seen:
                    seen.add(signature)
                    decompositions.append(
                        {
                            "kind": "knitted_straight",
                            "pattern_tiles": sorted(pattern),
                            "pair": completion["pair"],
                            "meld": completion["meld"],
                            "completion_kind": completion["completion_kind"],
                        }
                    )

        if not is_all_singletons:
            continue
        suit_tiles = [tile_key for tile_key in counts if tile_key not in HONOR_KEYS]
        if not set(suit_tiles).issubset(pattern):
            continue

        lesser_signature = (
            "lesser_honours_and_knitted_tiles",
            tuple(sorted(suit_tiles)),
            tuple(honor_tiles),
        )
        if len(honor_tiles) >= 5 and lesser_signature not in seen:
            seen.add(lesser_signature)
            decompositions.append(
                {
                    "kind": "lesser_honours_and_knitted_tiles",
                    "pattern_tiles": sorted(suit_tiles),
                    "honor_tiles": honor_tiles,
                }
            )

        if len(honor_tiles) == 7 and set(honor_tiles) == HONOR_KEYS:
            greater_signature = (
                "greater_honours_and_knitted_tiles",
                tuple(sorted(suit_tiles)),
            )
            if greater_signature not in seen:
                seen.add(greater_signature)
                decompositions.append(
                    {
                        "kind": "greater_honours_and_knitted_tiles",
                        "pattern_tiles": sorted(suit_tiles),
                        "honor_tiles": honor_tiles,
                    }
                )

    return decompositions


def _can_form_melds(counts: Counter[str]) -> bool:
    if not counts:
        return True
    tile_key = next(iter(counts))
    count = counts[tile_key]
    if count <= 0:
        counts.pop(tile_key, None)
        return _can_form_melds(counts)
    if count >= 3:
        next_counts = counts.copy()
        next_counts[tile_key] -= 3
        if next_counts[tile_key] == 0:
            del next_counts[tile_key]
        if _can_form_melds(next_counts):
            return True
    parsed = _parse_suit(tile_key)
    if parsed is not None:
        prefix, rank = parsed
        if rank <= 7:
            second = f"{prefix}{rank + 1}"
            third = f"{prefix}{rank + 2}"
            if counts.get(second, 0) > 0 and counts.get(third, 0) > 0:
                next_counts = counts.copy()
                next_counts[tile_key] -= 1
                next_counts[second] -= 1
                next_counts[third] -= 1
                for key in (tile_key, second, third):
                    if next_counts.get(key, 0) == 0:
                        next_counts.pop(key, None)
                if _can_form_melds(next_counts):
                    return True
    return False


def _is_standard_hand(counts: Counter[str]) -> bool:
    return any(
        decomposition["kind"] == "standard"
        for decomposition in _standard_decompositions_from_counts(counts)
    )


def decompose_winning_hand(tile_keys: list[str]) -> list[dict]:
    if len(tile_keys) != 14:
        return []

    counts = Counter(tile_keys)
    decompositions: list[dict] = []
    if _is_seven_pairs(counts):
        decompositions.append(
            {
                "kind": "seven_pairs",
                "pairs": _seven_pairs_pair_tiles(counts),
            }
        )
    if _is_thirteen_orphans(counts):
        pair_tile = next(tile_key for tile_key, count in counts.items() if count == 2)
        decompositions.append(
            {
                "kind": "thirteen_orphans",
                "pair": pair_tile,
                "orphans": sorted(counts.keys()),
            }
        )
    decompositions.extend(_special_knitted_decompositions(counts))
    decompositions.extend(_standard_decompositions_from_counts(counts))
    return decompositions


def decompose_winning_hand_with_melds(
    concealed_tile_keys: list[str],
    meld_tile_key_groups: list[list[str]],
) -> list[dict]:
    if not meld_tile_key_groups:
        return decompose_winning_hand(concealed_tile_keys)

    normalized_melds = [_normalize_meld_tile_key_group(meld_group) for meld_group in meld_tile_key_groups]
    if any(meld is None for meld in normalized_melds):
        return []

    remaining_meld_count = 4 - len(normalized_melds)
    if remaining_meld_count < 0:
        return []
    if len(concealed_tile_keys) != remaining_meld_count * 3 + 2:
        return []

    base_decompositions = _standard_decompositions_from_counts(Counter(concealed_tile_keys))
    fixed_melds = [list(meld) for meld in normalized_melds if meld is not None]
    return [
        {
            "kind": "standard",
            "pair": decomposition["pair"],
            "melds": [*fixed_melds, *decomposition["melds"]],
        }
        for decomposition in base_decompositions
    ]


def is_winning_hand_with_melds(
    concealed_tile_keys: list[str],
    meld_tile_key_groups: list[list[str]],
) -> bool:
    return bool(decompose_winning_hand_with_melds(concealed_tile_keys, meld_tile_key_groups))


def _standard_decompositions_from_counts(counts: Counter[str]) -> list[dict]:
    decompositions: list[dict] = []
    seen: set[tuple[str, tuple[tuple[str, ...], ...]]] = set()
    for tile_key, count in list(counts.items()):
        if count < 2:
            continue
        next_counts = counts.copy()
        next_counts[tile_key] -= 2
        if next_counts[tile_key] == 0:
            del next_counts[tile_key]
        for melds in _extract_all_melds(next_counts):
            canonical_melds = tuple(sorted(tuple(meld) for meld in melds))
            signature = (tile_key, canonical_melds)
            if signature in seen:
                continue
            seen.add(signature)
            decompositions.append(
                {
                    "kind": "standard",
                    "pair": tile_key,
                    "melds": [list(meld) for meld in canonical_melds],
                }
            )
    return decompositions


def _normalize_meld_tile_key_group(meld_tile_keys: list[str]) -> list[str] | None:
    if len(meld_tile_keys) == 3:
        return list(meld_tile_keys)
    if len(meld_tile_keys) == 4 and len(set(meld_tile_keys)) == 1:
        return list(meld_tile_keys[:3])
    return None


def _extract_all_melds(counts: Counter[str]) -> list[list[list[str]]]:
    if not counts:
        return [[]]

    tile_key = next(iter(sorted(counts)))
    count = counts[tile_key]
    if count <= 0:
        next_counts = counts.copy()
        next_counts.pop(tile_key, None)
        return _extract_all_melds(next_counts)

    results: list[list[list[str]]] = []
    if count >= 3:
        next_counts = counts.copy()
        next_counts[tile_key] -= 3
        if next_counts[tile_key] == 0:
            del next_counts[tile_key]
        for melds in _extract_all_melds(next_counts):
            results.append([[tile_key, tile_key, tile_key], *melds])

    parsed = _parse_suit(tile_key)
    if parsed is not None:
        prefix, rank = parsed
        if rank <= 7:
            second = f"{prefix}{rank + 1}"
            third = f"{prefix}{rank + 2}"
            if counts.get(second, 0) > 0 and counts.get(third, 0) > 0:
                next_counts = counts.copy()
                next_counts[tile_key] -= 1
                next_counts[second] -= 1
                next_counts[third] -= 1
                for key in (tile_key, second, third):
                    if next_counts.get(key, 0) == 0:
                        next_counts.pop(key, None)
                for melds in _extract_all_melds(next_counts):
                    results.append([[tile_key, second, third], *melds])
    return results


def is_winning_hand(tile_keys: list[str]) -> bool:
    return bool(decompose_winning_hand(tile_keys))
