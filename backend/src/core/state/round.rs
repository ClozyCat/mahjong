use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::error::EngineError;
use crate::core::ids::{Seat, TileKey};
use crate::core::tile::Tile;

use super::pending::{LastActionContext, PendingAction};
use super::settlement::RoundSettlement;
use super::{PlayerRoundState, WallState, array, bool_or, seat_vec, string_opt, usize_or};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RoundState {
    pub round_id: String,
    pub dealer_seat: Seat,
    pub round_wind: String,
    pub current_actor: Seat,
    pub phase: String,
    pub wall: WallState,
    pub players: Vec<PlayerRoundState>,
    pub last_discard: Option<Tile>,
    pub pending_action: Option<PendingAction>,
    pub settlement: Option<RoundSettlement>,
    pub version: u64,
    #[serde(default, deserialize_with = "super::null_default")]
    pub score_trackers: RoundScoreTrackers,
    pub last_action_context: LastActionContext,
    #[serde(flatten)]
    pub rule_state: RuleRuntimeState,
    pub restricted_discard_tile_key: Option<TileKey>,
}

impl RoundState {
    pub(crate) fn from_value(value: &Value) -> Result<Self, EngineError> {
        serde_json::from_value(value.clone()).map_err(Into::into)
    }

    pub(crate) fn to_value(&self) -> Result<Value, serde_json::Error> {
        serde_json::to_value(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RoundScoreTrackers {
    pub kong_entries: Vec<KongTrackerEntry>,
}

impl RoundScoreTrackers {
    pub(crate) fn from_value(value: Option<&Value>) -> Self {
        let Some(value) = value else {
            return Self::default();
        };
        let kong_entries = value
            .get("kong_entries")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .map(KongTrackerEntry::from_value)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Self {
            kong_entries,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct KongTrackerEntry {
    pub kong_type: String,
    pub actor_seat: Seat,
    pub payer_seats: Vec<Seat>,
    pub tile_key: Option<TileKey>,
}

impl KongTrackerEntry {
    fn from_value(value: &Value) -> Self {
        Self {
            kong_type: value
                .get("kong_type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            actor_seat: value
                .get("actor_seat")
                .and_then(Value::as_u64)
                .map(|value| value as Seat)
                .unwrap_or(0),
            payer_seats: seat_vec(value.get("payer_seats")),
            tile_key: string_opt(value, "tile_key"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RuleRuntimeState {
    pub enforce_minimum_eight_fan: bool,
}
