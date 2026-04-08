use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::error::EngineError;
use crate::core::tile::Tile;

use super::{array, usize_or};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct WallState {
    pub tiles: Vec<Tile>,
    pub head_index: usize,
    pub tail_index: usize,
}

impl WallState {
    pub(crate) fn from_value(value: &Value) -> Result<Self, EngineError> {
        serde_json::from_value(value.clone()).map_err(Into::into)
    }
    pub fn live_tiles_remaining(&self) -> usize {
        self.tail_index
            .checked_sub(self.head_index)
            .map(|distance| distance + 1)
            .unwrap_or(0)
    }
}
