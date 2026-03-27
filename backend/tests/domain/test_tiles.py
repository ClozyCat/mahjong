from collections import Counter

from app.domain.tiles import build_full_tile_set


def test_build_full_tile_set_returns_144_tiles():
    tiles = build_full_tile_set()
    assert len(tiles) == 144
    assert sum(tile.kind == "flower" for tile in tiles) == 8
    counts = Counter(tile.tile_key for tile in tiles)
    assert counts["w1"] == 4
    assert counts["east"] == 4
    assert counts["f1"] == 1
