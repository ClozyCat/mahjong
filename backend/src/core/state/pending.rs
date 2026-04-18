use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::ids::{Seat, TileId, TileKey};

use super::{seat_vec, string_opt, usize_opt};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PendingTimeout {
    pub kind: String,
    pub seat_index: Seat,
    pub deadline_at: Option<String>,
    pub drawn_tile_id: Option<TileId>,
}

impl PendingTimeout {
    pub(crate) fn from_value(value: &Value) -> Self {
        Self {
            kind: value
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            seat_index: value
                .get("seat_index")
                .and_then(Value::as_u64)
                .map(|value| value as Seat)
                .unwrap_or(0),
            deadline_at: string_opt(value, "deadline_at"),
            drawn_tile_id: string_opt(value, "drawn_tile_id"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ContinueActionState {
    pub action_id: String,
    pub confirmed_seats: Vec<Seat>,
    pub required_seats: Vec<Seat>,
    pub online_seats: Vec<Seat>,
    pub auto_advance_deadline_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LastActionContext {
    pub kind: String,
    pub seat: Seat,
    pub tile_id: Option<TileId>,
    pub from_kong_replacement: bool,
    pub was_last_live_tile: bool,
    pub was_last_discard: bool,
}

impl LastActionContext {
    pub(crate) fn from_value(value: Option<&Value>) -> Self {
        let Some(value) = value else {
            return Self::default();
        };
        Self {
            kind: value
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            seat: value
                .get("seat")
                .and_then(Value::as_u64)
                .map(|value| value as Seat)
                .unwrap_or(0),
            tile_id: string_opt(value, "tile_id"),
            from_kong_replacement: value
                .get("from_kong_replacement")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            was_last_live_tile: value
                .get("was_last_live_tile")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            was_last_discard: value
                .get("was_last_discard")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PendingAction {
    OpeningFlowers(OpeningFlowersAction),
    ClaimWindow(ClaimWindowAction),
    RobKongWindow(RobKongWindowAction),
}

impl PendingAction {
    pub(crate) fn from_value(value: &Value) -> Option<Self> {
        serde_json::from_value(value.clone()).ok()
    }
    pub fn action_type(&self) -> &str {
        match self {
            Self::OpeningFlowers(_) => "opening_flowers",
            Self::ClaimWindow(_) => "claim_window",
            Self::RobKongWindow(_) => "rob_kong_window",
        }
    }

    pub(crate) fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct OpeningFlowersAction {
    pub dealer_seat: Seat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ClaimWindowAction {
    pub discarder_seat: Seat,
    pub claim_window: Vec<Vec<String>>,
    pub responded_seats: Vec<Seat>,
    pub claim_responses: Vec<ClaimResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RobKongWindowAction {
    pub actor_seat: Seat,
    pub tile_id: Option<TileId>,
    pub tile_key: Option<TileKey>,
    pub meld_index: Option<usize>,
    pub offered_hu_seats: Vec<Seat>,
    pub responded_seats: Vec<Seat>,
    pub claim_responses: Vec<ClaimResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ClaimResponse {
    pub seat: Seat,
    #[serde(rename = "type")]
    pub action_type: String,
    pub tiles: Vec<TileId>,
}
