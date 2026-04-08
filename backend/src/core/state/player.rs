use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::error::EngineError;
use crate::core::ids::{Seat, TileKey};
use crate::core::tile::Tile;

use super::{SkillLoadout, array, bool_or, i64_opt, string_opt, usize_or};

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
    #[serde(default, deserialize_with = "super::null_default")]
    pub skill_loadout: SkillLoadout,
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
    pub concealed_tiles: Vec<Tile>,
    pub melds: Vec<Vec<TileKey>>,
    pub flowers: Vec<Tile>,
    pub discards: Vec<Tile>,
    #[serde(default, deserialize_with = "super::null_default")]
    pub skill_loadout: SkillLoadout,
}

impl PlayerRoundState {
    pub(crate) fn from_value(value: &Value) -> Result<Self, EngineError> {
        serde_json::from_value(value.clone()).map_err(Into::into)
    }
}
