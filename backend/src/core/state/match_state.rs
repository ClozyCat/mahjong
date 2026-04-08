use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::error::EngineError;
use crate::core::ids::{RoundId, Seat};

use super::{MatchSkillTrackers, i64_opt, object, string_opt, usize_or};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MatchState {
    pub prevailing_wind: String,
    pub hand_number: u32,
    pub dealer_seat: Seat,
    pub cumulative_scores: BTreeMap<Seat, i64>,
    pub match_finished: bool,
    pub last_completed_round_id: Option<RoundId>,
    #[serde(default, deserialize_with = "super::null_default")]
    pub skill_trackers: MatchSkillTrackers,
}

impl MatchState {
    pub(crate) fn from_value(value: &Value) -> Result<Self, EngineError> {
        serde_json::from_value(value.clone()).map_err(Into::into)
    }
}
