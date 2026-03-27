from __future__ import annotations

from app.domain.fans.decomposition import decompose_tile_keys
from app.domain.hand_eval import is_winning_hand

STANDARD_WIN_TILE_KEYS = [
    *(f"{prefix}{rank}" for prefix in ("w", "t", "b") for rank in range(1, 10)),
    "east",
    "south",
    "west",
    "north",
    "red",
    "green",
    "white",
]

def build_fan_context(**context) -> dict:
    normalized = dict(context)
    tile_keys = list(normalized.get("tile_keys", []))
    decompositions = normalized.get("decompositions")
    if decompositions is None and tile_keys:
        decompositions = decompose_tile_keys(tile_keys)
    normalized["decompositions"] = list(decompositions or [])
    normalized["decomposition_kinds"] = [
        decomposition.get("kind") for decomposition in normalized["decompositions"]
    ]
    normalized["standard_decompositions"] = [
        decomposition
        for decomposition in normalized["decompositions"]
        if decomposition.get("kind") == "standard"
    ]
    normalized["all_tile_keys"] = list(tile_keys)
    normalized["visible_tile_keys"] = list(normalized.get("visible_tile_keys", []))
    normalized["open_meld_tile_key_groups"] = list(
        normalized.get("open_meld_tile_key_groups", normalized.get("meld_tile_key_groups", []))
    )
    normalized["wait_types"] = _resolve_wait_types(
        normalized["standard_decompositions"],
        normalized.get("incoming_tile"),
        normalized["all_tile_keys"],
    )
    normalized["winning_tile"] = normalized.get("incoming_tile")
    normalized["is_self_draw"] = normalized.get("win_type") == "self_draw"
    normalized["is_discard_win"] = normalized.get("win_type") == "discard"
    normalized["is_concealed"] = len(normalized["open_meld_tile_key_groups"]) == 0
    return normalized


def _resolve_wait_types(
    standard_decompositions: list[dict],
    incoming_tile: str | None,
    all_tile_keys: list[str],
) -> list[str]:
    if incoming_tile is None:
        return []
    if _winning_tile_options(all_tile_keys, incoming_tile) != [incoming_tile]:
        return []

    wait_types: list[str] = []
    for decomposition in standard_decompositions:
        if decomposition.get("pair") == incoming_tile:
            wait_types.append("single_wait")
            continue

        for meld in decomposition.get("melds", []):
            if incoming_tile not in meld:
                continue
            if not all(_is_suit_tile(tile_key) for tile_key in meld):
                continue
            ranks = sorted(int(tile_key[1:]) for tile_key in meld)
            incoming_rank = int(incoming_tile[1:])
            if ranks == [1, 2, 3] and incoming_rank == 3:
                wait_types.append("edge_wait")
            elif ranks == [7, 8, 9] and incoming_rank == 7:
                wait_types.append("edge_wait")
            elif incoming_rank == ranks[1]:
                wait_types.append("closed_wait")

    deduped: list[str] = []
    for wait_type in wait_types:
        if wait_type not in deduped:
            deduped.append(wait_type)
    return deduped if len(deduped) == 1 else []


def _winning_tile_options(all_tile_keys: list[str], incoming_tile: str) -> list[str]:
    if len(all_tile_keys) != 14 or incoming_tile not in all_tile_keys:
        return []

    base_tile_keys = list(all_tile_keys)
    base_tile_keys.remove(incoming_tile)

    winning_tiles: list[str] = []
    for tile_key in STANDARD_WIN_TILE_KEYS:
        if base_tile_keys.count(tile_key) >= 4:
            continue
        if is_winning_hand(base_tile_keys + [tile_key]):
            winning_tiles.append(tile_key)
    return winning_tiles


def _is_suit_tile(tile_key: str) -> bool:
    return (
        len(tile_key) == 2
        and tile_key[0] in {"w", "t", "b"}
        and tile_key[1].isdigit()
    )
