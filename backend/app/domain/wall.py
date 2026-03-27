from __future__ import annotations

import random

from dataclasses import dataclass

from app.domain.models import Tile
from app.domain.tiles import build_full_tile_set


@dataclass(frozen=True)
class WallState:
    tiles: tuple[Tile, ...]
    head_index: int
    tail_index: int


def build_wall(seed: int | None) -> WallState:
    tiles = build_full_tile_set()
    rng = random.Random(seed)
    rng.shuffle(tiles)
    return WallState(
        tiles=tuple(tiles),
        head_index=0,
        tail_index=len(tiles) - 1,
    )


def draw_live_tile(wall: WallState) -> tuple[Tile, WallState]:
    if wall.head_index > wall.tail_index:
        raise IndexError("No more live tiles available")
    tile = wall.tiles[wall.head_index]
    return tile, WallState(wall.tiles, wall.head_index + 1, wall.tail_index)


def draw_replacement_tile(wall: WallState) -> tuple[Tile, WallState]:
    if wall.head_index > wall.tail_index:
        raise IndexError("No replacement tiles available")
    tile = wall.tiles[wall.tail_index]
    return tile, WallState(wall.tiles, wall.head_index, wall.tail_index - 1)
