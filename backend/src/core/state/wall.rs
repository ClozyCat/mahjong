use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::error::EngineError;
use crate::core::tile::Tile;

use super::{array, usize_or};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WallState {
    pub tiles: Vec<Tile>,
    pub head_index: usize,
    pub tail_index: usize,
}

impl WallState {
    pub(crate) fn from_legacy_value(value: &Value) -> Result<Self, EngineError> {
        let tiles = value
            .get("tiles")
            .map(|tiles| {
                array(tiles, "round_state.wall.tiles").and_then(|tiles| {
                    tiles
                        .iter()
                        .enumerate()
                        .map(|(index, tile)| {
                            Tile::from_legacy_value(
                                tile,
                                &format!("round_state.wall.tiles[{index}]"),
                            )
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
            })
            .transpose()?
            .unwrap_or_default();
        Ok(Self {
            tiles,
            head_index: usize_or(value, "head_index", 0),
            tail_index: usize_or(value, "tail_index", 0),
        })
    }

    pub fn live_tiles_remaining(&self) -> usize {
        self.tail_index
            .checked_sub(self.head_index)
            .map(|distance| distance + 1)
            .unwrap_or(0)
    }
}
