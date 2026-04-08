use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::error::EngineError;
use crate::core::ids::{Seat, TileKey};
use crate::core::tile::Tile;

use super::effect::EffectState;
use super::pending::{LastActionContext, PendingAction};
use super::settlement::RoundSettlement;
use super::skill_trackers::RoundSkillTrackers;
use super::{PlayerRoundState, WallState, array, bool_or, seat_vec, string_opt, usize_or};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
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
    pub score_trackers: RoundScoreTrackers,
    pub last_action_context: LastActionContext,
    pub rule_state: RuleRuntimeState,
    pub effect_state: EffectState,
    pub restricted_discard_tile_key: Option<TileKey>,
    #[serde(default)]
    pub skill_trackers: RoundSkillTrackers,
}

impl RoundState {
    pub(crate) fn from_value(value: &Value) -> Result<Self, EngineError> {
        let players = value
            .get("players")
            .map(|players| {
                array(players, "round_state.players").and_then(|players| {
                    players
                        .iter()
                        .map(PlayerRoundState::from_value)
                        .collect::<Result<Vec<_>, _>>()
                })
            })
            .transpose()?
            .unwrap_or_default();
        let last_discard = value
            .get("last_discard")
            .filter(|discard| !discard.is_null())
            .map(|discard| Tile::from_value(discard, "round_state.last_discard"))
            .transpose()?;
        let pending_action = value
            .get("pending_action")
            .filter(|pending| !pending.is_null())
            .and_then(PendingAction::from_value);
        let score_trackers = RoundScoreTrackers::from_value(value.get("score_trackers"));
        Ok(Self {
            round_id: value
                .get("round_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            dealer_seat: usize_or(value, "dealer_seat", 0),
            round_wind: value
                .get("round_wind")
                .and_then(Value::as_str)
                .unwrap_or("east")
                .to_string(),
            current_actor: usize_or(value, "current_actor", 0),
            phase: value
                .get("phase")
                .and_then(Value::as_str)
                .unwrap_or("playing")
                .to_string(),
            wall: value
                .get("wall")
                .map(WallState::from_value)
                .transpose()?
                .unwrap_or_default(),
            players,
            last_discard,
            pending_action,
            settlement: value
                .get("settlement")
                .filter(|settlement| !settlement.is_null())
                .map(RoundSettlement::from_value),
            version: value
                .get("version")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
            score_trackers,
            last_action_context: LastActionContext::from_value(value.get("last_action_context")),
            rule_state: RuleRuntimeState {
                enforce_minimum_eight_fan: bool_or(value, "enforce_minimum_eight_fan", true),
            },
            effect_state: EffectState::from_value(value.get("effect_state"))?,
            restricted_discard_tile_key: string_opt(value, "restricted_discard_tile_key"),
            skill_trackers: RoundSkillTrackers::from_value(value.get("skill_trackers")),
        })
    }

    pub(crate) fn to_value(&self) -> Result<Value, serde_json::Error> {
        let mut value = serde_json::to_value(self)?;
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "pending_action".to_string(),
                self.pending_action
                    .as_ref()
                    .map(PendingAction::to_value)
                    .unwrap_or(Value::Null),
            );
            object.insert(
                "settlement".to_string(),
                self.settlement
                    .as_ref()
                    .map(RoundSettlement::to_value)
                    .unwrap_or(Value::Null),
            );
            object.insert(
                "enforce_minimum_eight_fan".to_string(),
                Value::Bool(self.rule_state.enforce_minimum_eight_fan),
            );
            object.insert(
                "skill_trackers".to_string(),
                self.skill_trackers.to_value(),
            );
            object.remove("rule_state");
        }
        Ok(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RoundScoreTrackers {
    pub kong_entries: Vec<KongTrackerEntry>,
    pub opening_flowers_completed: bool,
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
            opening_flowers_completed: value
                .get("opening_flowers_completed")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
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
pub struct RuleRuntimeState {
    pub enforce_minimum_eight_fan: bool,
}
