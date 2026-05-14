use serde::{Deserialize, Serialize};

use crate::core::ids::{Seat, TileId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameCommand {
    StartMatch {
        dealer: Seat,
        seed: u64,
    },
    PlayerAction {
        actor: Seat,
        action: PlayerAction,
    },
    ResolveTimeout {
        kind: String,
        nonce: u64,
    },
    ContinueAction {
        actor: Seat,
        kind: ContinueActionKind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContinueActionKind {
    StartNextRound,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerAction {
    Flower { tile_ids: Vec<TileId> },
    Discard { tile_id: TileId },
    ReadyHand { tile_id: TileId },
    Chow { tile_ids: Vec<TileId> },
    Pung { tile_ids: Vec<TileId> },
    Kong { tile_ids: Vec<TileId> },
    Hu,
    Pass,
}
