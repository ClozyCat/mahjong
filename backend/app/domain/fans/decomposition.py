from __future__ import annotations

from app.domain.hand_eval import decompose_winning_hand


def decompose_tile_keys(tile_keys: list[str]) -> list[dict]:
    return decompose_winning_hand(tile_keys)
