use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::error::EngineError;
use crate::core::ids::{TileId, TileKey};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Tile {
    pub tile_id: TileId,
    pub tile_key: TileKey,
    pub kind: String,
    pub suit: Option<String>,
    pub rank: Option<u8>,
    pub name: Option<String>,
}

impl Tile {
    pub fn from_value(value: &Value, context: &str) -> Result<Self, EngineError> {
        let object = value
            .as_object()
            .ok_or_else(|| EngineError::decode(format!("{context} should be a tile object")))?;
        let tile_id = object
            .get("tile_id")
            .and_then(Value::as_str)
            .ok_or_else(|| EngineError::decode(format!("{context}.tile_id missing")))?;
        let tile_key = object
            .get("tile_key")
            .and_then(Value::as_str)
            .ok_or_else(|| EngineError::decode(format!("{context}.tile_key missing")))?;
        let kind = object
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let suit = object
            .get("suit")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let rank = object
            .get("rank")
            .and_then(Value::as_u64)
            .map(|value| value as u8);
        let name = object
            .get("name")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        Ok(Self {
            tile_id: tile_id.to_string(),
            tile_key: tile_key.to_string(),
            kind: kind.to_string(),
            suit,
            rank,
            name,
        })
    }
    pub fn tile_key_only(tile_key: &str) -> Self {
        Self {
            tile_key: tile_key.to_string(),
            ..Self::default()
        }
    }
}
