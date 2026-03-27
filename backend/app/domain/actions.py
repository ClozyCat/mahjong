from app.domain.models import Tile


def tile_drawn_event(seat: int, tile: Tile) -> dict:
    return {"type": "tile_drawn", "seat": seat, "tile_id": tile.tile_id}


def flower_exposed_event(seat: int, tile: Tile) -> dict:
    return {"type": "flower_exposed", "seat": seat, "tile_id": tile.tile_id}


def replacement_draw_event(seat: int, tile: Tile) -> dict:
    return {"type": "replacement_draw", "seat": seat, "tile_id": tile.tile_id}


def tile_discarded_event(seat: int, tile: Tile) -> dict:
    return {"type": "tile_discarded", "seat": seat, "tile_id": tile.tile_id}


def claim_made_event(seat: int, claim_type: str, tile: Tile) -> dict:
    return {
        "type": "claim_made",
        "seat": seat,
        "claim_type": claim_type,
        "tile_id": tile.tile_id,
    }


def self_kong_declared_event(
    seat: int,
    kong_type: str,
    tile_key: str,
    tile_ids: list[str],
) -> dict:
    return {
        "type": "self_kong_declared",
        "seat": seat,
        "kong_type": kong_type,
        "tile_key": tile_key,
        "tile_ids": tile_ids,
    }
