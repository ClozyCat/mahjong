use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::error::EngineError;
use crate::core::ids::{RoundId, Seat};

use super::{MatchSkillTrackers, i64_opt, object, string_opt, usize_or};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MatchState {
    pub prevailing_wind: String,
    pub hand_number: u32,
    pub dealer_seat: Seat,
    pub cumulative_scores: BTreeMap<Seat, i64>,
    pub match_finished: bool,
    pub last_completed_round_id: Option<RoundId>,
    #[serde(default)]
    pub skill_trackers: MatchSkillTrackers,
}

impl MatchState {
    pub(crate) fn from_legacy_value(value: &Value) -> Result<Self, EngineError> {
        let scores = value
            .get("cumulative_scores")
            .map(|scores| {
                object(scores, "match_state.cumulative_scores").map(|object| {
                    object
                        .iter()
                        .filter_map(|(seat, score)| seat.parse::<usize>().ok().zip(score.as_i64()))
                        .collect::<BTreeMap<Seat, i64>>()
                })
            })
            .transpose()?
            .unwrap_or_default();
        Ok(Self {
            prevailing_wind: value
                .get("prevailing_wind")
                .and_then(Value::as_str)
                .unwrap_or("east")
                .to_string(),
            hand_number: value
                .get("hand_number")
                .and_then(Value::as_u64)
                .map(|value| value as u32)
                .unwrap_or(0),
            dealer_seat: usize_or(value, "dealer_seat", 0),
            cumulative_scores: scores,
            match_finished: value
                .get("match_finished")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            last_completed_round_id: string_opt(value, "last_completed_round_id").or_else(|| {
                i64_opt(value, "last_completed_round_id").map(|value| value.to_string())
            }),
            skill_trackers: MatchSkillTrackers::from_legacy_value(value.get("skill_trackers")),
        })
    }
}
