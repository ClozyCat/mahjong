use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::ids::{Seat, TileId, TileKey};

use super::{seat_vec, string_opt, usize_opt};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PendingTimeout {
    pub kind: String,
    pub seat_index: Seat,
    pub extra_time_seat: Option<Seat>,
    pub deadline_at: Option<String>,
    pub drawn_tile_id: Option<TileId>,
    #[serde(default)]
    pub extended_with_extra: bool,
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
            extra_time_seat: usize_opt(value, "extra_time_seat"),
            deadline_at: string_opt(value, "deadline_at"),
            drawn_tile_id: string_opt(value, "drawn_tile_id"),
            extended_with_extra: value
                .get("extended_with_extra")
                .and_then(Value::as_bool)
                .unwrap_or(false),
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
    ClaimWindow(ClaimWindowAction),
    RobKongWindow(RobKongWindowAction),
    PlayerMultiplierSelection(PlayerMultiplierSelectionAction),
}

impl PendingAction {
    pub(crate) fn from_value(value: &Value) -> Option<Self> {
        serde_json::from_value(value.clone()).ok()
    }
    pub fn action_type(&self) -> &str {
        match self {
            Self::ClaimWindow(_) => "claim_window",
            Self::RobKongWindow(_) => "rob_kong_window",
            Self::PlayerMultiplierSelection(_) => "player_multiplier_selection",
        }
    }

    pub(crate) fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

pub fn pending_action_response_seat(pending_action: &PendingAction) -> Option<Seat> {
    match pending_action {
        PendingAction::ClaimWindow(claim) => next_claim_window_responder_seat(claim),
        PendingAction::RobKongWindow(rob) => next_rob_kong_responder_seat(rob),
        PendingAction::PlayerMultiplierSelection(_) => None,
    }
}

fn next_claim_window_responder_seat(claim: &ClaimWindowAction) -> Option<Seat> {
    response_order_from(claim.discarder_seat).find(|seat| {
        claim
            .claim_window
            .get(*seat)
            .is_some_and(|claims| !claims.is_empty())
            && !claim.responded_seats.contains(seat)
    })
}

fn next_rob_kong_responder_seat(rob: &RobKongWindowAction) -> Option<Seat> {
    response_order_from(rob.actor_seat)
        .find(|seat| rob.offered_hu_seats.contains(seat) && !rob.responded_seats.contains(seat))
}

fn response_order_from(origin_seat: Seat) -> impl Iterator<Item = Seat> {
    (1..4).map(move |offset| (origin_seat + offset) % 4)
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
pub struct PlayerMultiplierSelectionAction {
    pub responded_seats: Vec<Seat>,
    #[serde(default, deserialize_with = "super::deserialize_seat_i64_map")]
    pub selected_multipliers: BTreeMap<Seat, i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ClaimResponse {
    pub seat: Seat,
    #[serde(rename = "type")]
    pub action_type: String,
    pub tiles: Vec<TileId>,
}
