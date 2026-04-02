from __future__ import annotations

from dataclasses import dataclass
from typing import Literal, TYPE_CHECKING

TileKind = Literal["suit", "wind", "dragon", "flower"]
SuitKey = Literal["characters", "bamboos", "dots"]
RoundPhase = Literal["playing", "settlement"]


@dataclass(frozen=True)
class Tile:
    tile_id: str
    tile_key: str
    kind: TileKind
    suit: SuitKey | None
    rank: int | None
    name: str


if TYPE_CHECKING:
    from app.domain.wall import WallState


@dataclass(frozen=True)
class PlayerState:
    seat: int
    concealed_tiles: tuple[Tile, ...]
    melds: tuple[tuple[Tile, ...], ...]
    flowers: tuple[Tile, ...]
    discards: tuple[Tile, ...]


@dataclass(frozen=True)
class RoundState:
    round_id: str
    dealer_seat: int
    current_actor: int
    wall: WallState
    players: tuple[PlayerState, ...]
    last_discard: Tile | None
    pending_action: dict | None
    phase: RoundPhase
    settlement: dict | None
    version: int
    score_trackers: dict | None = None
    last_action_context: dict | None = None
    round_wind: str = "east"
    enforce_minimum_eight_fan: bool = True
    restricted_discard_tile_key: str | None = None
