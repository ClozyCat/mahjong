pub mod effect;
pub mod match_state;
pub mod pending;
pub mod player;
pub mod room;
pub mod round;
pub mod settlement;
pub mod skill_trackers;
pub mod wall;

pub use effect::{
    EffectInstance, EffectState, KnowledgeEffect, RuleOverride, SkillInstance, SkillLoadout,
};
pub use match_state::MatchState;
pub use pending::{
    ClaimWindowAction, ContinueActionState, LastActionContext, OpeningFlowersAction, PendingAction,
    PendingTimeout, RobKongWindowAction,
};
pub use player::{PlayerRoundState, SeatState};
pub use room::RoomState;
pub use round::{KongTrackerEntry, RoundScoreTrackers, RoundState, RuleRuntimeState};
pub use settlement::{
    RoundSettlement, SettlementFanBreakdownEntry, SettlementKongScoreDetailEntry,
    SettlementScoreDelta,
};
pub use skill_trackers::{
    LianHuanJiTracker, MatchSkillTrackers, RoundSkillTrackers, ZouWeiShangJiTracker,
};
pub use wall::WallState;

use serde_json::Value;

use crate::core::error::EngineError;
use crate::core::ids::Seat;

pub(crate) fn object<'a>(
    value: &'a Value,
    context: &str,
) -> Result<&'a serde_json::Map<String, Value>, EngineError> {
    value
        .as_object()
        .ok_or_else(|| EngineError::legacy_decode(format!("{context} should be an object")))
}

pub(crate) fn array<'a>(value: &'a Value, context: &str) -> Result<&'a [Value], EngineError> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| EngineError::legacy_decode(format!("{context} should be an array")))
}

pub(crate) fn string_opt(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

pub(crate) fn bool_or(value: &Value, key: &str, default: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(default)
}

pub(crate) fn usize_or(value: &Value, key: &str, default: usize) -> usize {
    value
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(default)
}

pub(crate) fn usize_opt(value: &Value, key: &str) -> Option<usize> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
}

pub(crate) fn i64_opt(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

pub(crate) fn seat_vec(value: Option<&Value>) -> Vec<Seat> {
    value
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_u64().map(|seat| seat as Seat))
                .collect()
        })
        .unwrap_or_default()
}
