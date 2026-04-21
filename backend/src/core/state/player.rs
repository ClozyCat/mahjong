use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::error::EngineError;
use crate::core::ids::{Seat, TileKey};
use crate::core::tile::Tile;

use super::{array, bool_or, i64_opt, string_opt, usize_or};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SeatState {
    pub seat_index: Seat,
    pub nickname: Option<String>,
    pub reconnect_token: Option<String>,
    pub player_session_id: Option<i64>,
    pub connected: bool,
    pub ready: bool,
    pub is_bot: bool,
    pub seat_type: String,
    pub bot_persona: Option<String>,
    pub bot_aggression: Option<i64>,
    pub disconnect_deadline_at: Option<String>,
}

impl SeatState {
    pub(crate) fn from_value(value: &Value) -> Self {
        serde_json::from_value(value.clone()).unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PlayerRoundState {
    pub seat: Seat,
    pub is_ready_hand: bool,
    pub concealed_tiles: Vec<Tile>,
    pub melds: Vec<Vec<TileKey>>,
    pub display_melds: Vec<DisplayMeldState>,
    pub flowers: Vec<Tile>,
    pub discards: Vec<Tile>,
}

impl PlayerRoundState {
    pub(crate) fn from_value(value: &Value) -> Result<Self, EngineError> {
        serde_json::from_value(value.clone()).map_err(Into::into)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayMeldOrientation {
    Normal,
    Rotated,
    UpsideDown,
    FaceDown,
}

impl Default for DisplayMeldOrientation {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DisplayMeldTileState {
    pub code: TileKey,
    pub orientation: DisplayMeldOrientation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DisplayMeldState {
    pub tiles: Vec<DisplayMeldTileState>,
}
