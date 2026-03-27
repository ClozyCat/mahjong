from app.domain.wall import build_wall, draw_live_tile, draw_replacement_tile


def test_replacement_draw_consumes_tail_pointer():
    wall = build_wall(seed=7)
    live_tile, wall = draw_live_tile(wall)
    replacement_tile, wall = draw_replacement_tile(wall)
    assert live_tile.tile_id != replacement_tile.tile_id
    assert wall.head_index == 1
    assert wall.tail_index == 142
