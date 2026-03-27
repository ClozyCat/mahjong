from __future__ import annotations

from typing import Iterable

from app.domain.models import Tile


SUIT_DEFINITIONS: tuple[tuple[str, str, str], ...] = (
    ("characters", "Character", "w"),
    ("bamboos", "Bamboo", "t"),
    ("dots", "Dot", "b"),
)

WIND_DEFINITIONS: tuple[tuple[str, str], ...] = (
    ("east", "East Wind"),
    ("south", "South Wind"),
    ("west", "West Wind"),
    ("north", "North Wind"),
)

DRAGON_DEFINITIONS: tuple[tuple[str, str], ...] = (
    ("red", "Red Dragon"),
    ("green", "Green Dragon"),
    ("white", "White Dragon"),
)

FLOWER_DEFINITIONS: tuple[tuple[str, str], ...] = (
    ("f1", "Spring Flower"),
    ("f2", "Summer Flower"),
    ("f3", "Autumn Flower"),
    ("f4", "Winter Flower"),
    ("f5", "Plum Flower"),
    ("f6", "Orchid Flower"),
    ("f7", "Chrysanthemum Flower"),
    ("f8", "Bamboo Flower"),
)


def build_full_tile_set() -> list[Tile]:
    tiles: list[Tile] = []
    _extend_suits(tiles)
    _extend_honors(tiles)
    _extend_flowers(tiles)
    return tiles


def _extend_suits(tiles: list[Tile]) -> None:
    for suit_key, suit_name, prefix in SUIT_DEFINITIONS:
        for rank in range(1, 10):
            base_tile_id = f"{prefix}{rank}"
            for copy_index in range(4):
                tiles.append(
                    Tile(
                        tile_id=f"{base_tile_id}#{copy_index}",
                        tile_key=base_tile_id,
                        kind="suit",
                        suit=suit_key,
                        rank=rank,
                        name=f"{suit_name} {rank}",
                    )
                )


def _extend_honors(tiles: list[Tile]) -> None:
    _extend_honor_group(tiles, WIND_DEFINITIONS, kind="wind")
    _extend_honor_group(tiles, DRAGON_DEFINITIONS, kind="dragon")


def _extend_honor_group(
    tiles: list[Tile], definitions: Iterable[tuple[str, str]], kind: str
) -> None:
    for tile_key, tile_name in definitions:
        for copy_index in range(4):
            tiles.append(
                Tile(
                    tile_id=f"{tile_key}#{copy_index}",
                    tile_key=tile_key,
                    kind=kind,
                    suit=None,
                    rank=None,
                    name=tile_name,
                )
            )


def _extend_flowers(tiles: list[Tile]) -> None:
    for tile_key, tile_name in FLOWER_DEFINITIONS:
        tiles.append(
            Tile(
                tile_id=f"{tile_key}#0",
                tile_key=tile_key,
                kind="flower",
                suit=None,
                rank=None,
                name=tile_name,
            )
        )
