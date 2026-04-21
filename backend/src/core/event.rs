use serde::{Deserialize, Serialize};

use crate::core::ids::Seat;
use crate::core::state::RoundSettlement;
use crate::core::tile::Tile;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameEvent {
    MatchStarted {
        dealer: Seat,
    },
    RoundStarted {
        round_id: String,
    },
    TileDrawn {
        seat: Seat,
        tile: Tile,
        source: String,
    },
    TileDiscarded {
        seat: Seat,
        tile: Tile,
    },
    ReadyHandDeclared {
        seat: Seat,
        tile: Tile,
    },
    MeldClaimed {
        seat: Seat,
        meld: Vec<String>,
        from: Seat,
    },
    FlowerExposed {
        seat: Seat,
        tile_id: String,
    },
    SelfKongDeclared {
        seat: Seat,
        kong_type: String,
        tile_key: String,
        tile_ids: Vec<String>,
    },
    ClaimAutoPassed {
        discarder_seat: Seat,
        seats: Vec<Seat>,
    },
    HuDeclared {
        winner: Seat,
        source: String,
    },
    SettlementPrepared {
        settlement: RoundSettlement,
    },
}
