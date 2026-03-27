from __future__ import annotations

from collections import Counter


def extract_hand_features(
    *,
    concealed_tile_keys: list[str],
    meld_tile_key_groups: list[list[str]],
    meld_open_flags: list[bool] | None = None,
    incoming_tile: str | None,
    seat_wind_key: str | None = None,
    round_wind_key: str | None = None,
    decompositions: list[dict] | None = None,
) -> dict:
    effective_concealed = list(concealed_tile_keys)
    if incoming_tile is not None:
        effective_concealed.append(incoming_tile)

    all_tile_keys = list(effective_concealed)
    for meld_group in meld_tile_key_groups:
        all_tile_keys.extend(meld_group)

    sequence_groups = _extract_sequences(effective_concealed, decompositions=decompositions)
    triplet_keys = _extract_triplet_keys(
        effective_concealed,
        meld_tile_key_groups,
        decompositions=decompositions,
    )
    has_open_meld = (
        any(meld_open_flags)
        if meld_open_flags is not None
        else bool(meld_tile_key_groups)
    )

    return {
        "concealed_hand": not has_open_meld,
        "thirteen_orphans": _is_thirteen_orphans(effective_concealed, meld_tile_key_groups),
        "seven_pairs": _is_seven_pairs(effective_concealed, meld_tile_key_groups),
        "pung_hand": _is_pung_hand(effective_concealed, meld_tile_key_groups),
        "mixed_one_suit": _is_mixed_one_suit(all_tile_keys),
        "pure_one_suit": _is_pure_one_suit(all_tile_keys),
        "ping_hu": _is_ping_hu(
            effective_concealed,
            meld_tile_key_groups,
            decompositions=decompositions,
        ),
        "yi_ban_gao": _has_yi_ban_gao(sequence_groups),
        "duan_yao": _is_duan_yao(all_tile_keys),
        "hun_yao_jiu": _is_hun_yao_jiu(all_tile_keys),
        "qing_yao_jiu": _is_qing_yao_jiu(all_tile_keys),
        "triplet_keys": triplet_keys,
        "seat_wind_triplet": seat_wind_key in triplet_keys if seat_wind_key else False,
        "round_wind_triplet": round_wind_key in triplet_keys if round_wind_key else False,
        "dragon_triplet_count": sum(
            1 for tile_key in triplet_keys if tile_key in {"red", "green", "white"}
        ),
        "terminal_triplet_count": sum(
            1 for tile_key in triplet_keys if _is_terminal_suit_tile(tile_key)
        ),
        "non_seat_non_round_wind_triplet_count": sum(
            1
            for tile_key in triplet_keys
            if tile_key in {"east", "south", "west", "north"}
            and tile_key not in {seat_wind_key, round_wind_key}
        ),
    }


def _is_seven_pairs(
    tile_keys: list[str],
    meld_tile_key_groups: list[list[str]],
) -> bool:
    if meld_tile_key_groups or len(tile_keys) != 14:
        return False
    counts = Counter(tile_keys)
    return len(counts) == 7 and all(count == 2 for count in counts.values())


def _is_thirteen_orphans(
    tile_keys: list[str],
    meld_tile_key_groups: list[list[str]],
) -> bool:
    if meld_tile_key_groups or len(tile_keys) != 14:
        return False
    counts = Counter(tile_keys)
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
    if set(counts) != required:
        return False
    duplicate_count = sum(1 for tile_key in required if counts.get(tile_key, 0) == 2)
    return duplicate_count == 1 and all(counts.get(tile_key, 0) >= 1 for tile_key in required)


def _is_pung_hand(
    tile_keys: list[str],
    meld_tile_key_groups: list[list[str]],
) -> bool:
    if any(_meld_is_sequence(meld) for meld in meld_tile_key_groups):
        return False
    return _can_form_all_pungs(Counter(tile_keys))


def _is_ping_hu(
    tile_keys: list[str],
    meld_tile_key_groups: list[list[str]],
    *,
    decompositions: list[dict] | None = None,
) -> bool:
    if meld_tile_key_groups or len(tile_keys) != 14:
        return False
    decomposition = _standard_decomposition(tile_keys, decompositions=decompositions)
    if decomposition is None:
        return False
    return all(_meld_is_sequence(list(meld)) for meld in decomposition["melds"])


def _has_yi_ban_gao(sequence_groups: list[tuple[str, str, str]]) -> bool:
    counts = Counter(sequence_groups)
    return any(count >= 2 for count in counts.values())


def _is_duan_yao(tile_keys: list[str]) -> bool:
    return all(_is_simple_tile(tile_key) for tile_key in tile_keys)


def _is_hun_yao_jiu(tile_keys: list[str]) -> bool:
    has_honors = any(_parse_suit(tile_key) is None for tile_key in tile_keys)
    has_terminals = any(_is_terminal_suit_tile(tile_key) for tile_key in tile_keys)
    return has_honors and has_terminals and all(
        _is_terminal_or_honor(tile_key) for tile_key in tile_keys
    )


def _is_qing_yao_jiu(tile_keys: list[str]) -> bool:
    return tile_keys != [] and all(_is_terminal_suit_tile(tile_key) for tile_key in tile_keys)


def _extract_sequences(
    tile_keys: list[str],
    *,
    decompositions: list[dict] | None = None,
) -> list[tuple[str, str, str]]:
    decomposition = _standard_decomposition(tile_keys, decompositions=decompositions)
    if decomposition is None:
        return []
    return [
        tuple(meld)
        for meld in decomposition["melds"]
        if _meld_is_sequence(list(meld))
    ]


def _extract_triplet_keys(
    tile_keys: list[str],
    meld_tile_key_groups: list[list[str]],
    *,
    decompositions: list[dict] | None = None,
) -> list[str]:
    triplet_keys: list[str] = []
    decomposition = _standard_decomposition(tile_keys, decompositions=decompositions)
    if decomposition is not None:
        for meld in decomposition["melds"]:
            if len(set(meld)) == 1:
                triplet_keys.append(meld[0])

    for meld_group in meld_tile_key_groups:
        if len(meld_group) >= 3 and len(set(meld_group)) == 1:
            triplet_keys.append(meld_group[0])
    return triplet_keys


def _standard_decomposition(
    tile_keys: list[str],
    *,
    decompositions: list[dict] | None = None,
) -> dict | None:
    if decompositions is not None:
        return next(
            (decomposition for decomposition in decompositions if decomposition.get("kind") == "standard"),
            None,
        )
    return _decompose_standard_hand(tile_keys)


def _decompose_standard_hand(tile_keys: list[str]) -> dict | None:
    counts = Counter(tile_keys)
    for tile_key, count in list(counts.items()):
        if count < 2:
            continue
        next_counts = counts.copy()
        next_counts[tile_key] -= 2
        if next_counts[tile_key] == 0:
            del next_counts[tile_key]
        melds = _extract_melds(next_counts)
        if melds is not None:
            return {"pair": tile_key, "melds": melds}
    return None


def _extract_melds(counts: Counter[str]) -> list[list[str]] | None:
    if not counts:
        return []
    tile_key = next(iter(counts))
    count = counts[tile_key]
    if count <= 0:
        next_counts = counts.copy()
        next_counts.pop(tile_key, None)
        return _extract_melds(next_counts)

    if count >= 3:
        next_counts = counts.copy()
        next_counts[tile_key] -= 3
        if next_counts[tile_key] == 0:
            del next_counts[tile_key]
        melds = _extract_melds(next_counts)
        if melds is not None:
            return [[tile_key, tile_key, tile_key]] + melds

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
                melds = _extract_melds(next_counts)
                if melds is not None:
                    return [[tile_key, second, third]] + melds
    return None


def _can_form_all_pungs(counts: Counter[str]) -> bool:
    if sum(counts.values()) % 3 != 2:
        return False

    for tile_key, count in list(counts.items()):
        if count < 2:
            continue
        next_counts = counts.copy()
        next_counts[tile_key] -= 2
        if next_counts[tile_key] == 0:
            del next_counts[tile_key]
        if all(value % 3 == 0 for value in next_counts.values()):
            return True
    return False


def _is_mixed_one_suit(tile_keys: list[str]) -> bool:
    suits = {parsed[0] for tile_key in tile_keys if (parsed := _parse_suit(tile_key))}
    has_honors = any(_parse_suit(tile_key) is None for tile_key in tile_keys)
    return len(suits) == 1 and has_honors


def _is_pure_one_suit(tile_keys: list[str]) -> bool:
    suits = {parsed[0] for tile_key in tile_keys if (parsed := _parse_suit(tile_key))}
    has_honors = any(_parse_suit(tile_key) is None for tile_key in tile_keys)
    return len(suits) == 1 and not has_honors


def _meld_is_sequence(meld_tile_keys: list[str]) -> bool:
    if len(meld_tile_keys) < 3:
        return False
    if any(_parse_suit(tile_key) is None for tile_key in meld_tile_keys):
        return False
    parsed = sorted(_parse_suit(tile_key) for tile_key in meld_tile_keys[:3])
    assert parsed[0] is not None and parsed[1] is not None and parsed[2] is not None
    same_suit = parsed[0][0] == parsed[1][0] == parsed[2][0]
    consecutive = parsed[0][1] + 1 == parsed[1][1] and parsed[1][1] + 1 == parsed[2][1]
    return same_suit and consecutive


def _is_simple_tile(tile_key: str) -> bool:
    parsed = _parse_suit(tile_key)
    return parsed is not None and 2 <= parsed[1] <= 8


def _is_terminal_suit_tile(tile_key: str) -> bool:
    parsed = _parse_suit(tile_key)
    return parsed is not None and parsed[1] in {1, 9}


def _is_terminal_or_honor(tile_key: str) -> bool:
    return _is_terminal_suit_tile(tile_key) or _parse_suit(tile_key) is None


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
