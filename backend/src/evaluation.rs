use serde::{Deserialize, Serialize};

use crate::bot::policy::BotPolicyConfig;
use crate::core::state::RoomState;

pub const EVALUATION_ROOM_MODE: &str = "evaluation";
pub const EVALUATION_HAND_COUNT: usize = 16;
pub const EVALUATION_INITIAL_SUBJECT_SEAT: usize = 0;
pub const EVALUATION_MINIMUM_HU_FAN: i64 = 8;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EvaluationSubjectPolicyConfig {
    pub display_name: String,
    #[serde(flatten)]
    pub policy: BotPolicyConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EvaluationArenaConfig {
    pub matches: usize,
    pub seed: u64,
    #[serde(default = "default_max_actions_per_match")]
    pub max_actions_per_match: usize,
    #[serde(default)]
    pub report_trajectories: bool,
    pub subjects: Vec<EvaluationSubjectPolicyConfig>,
    pub opponents: Vec<BotPolicyConfig>,
    #[serde(default)]
    pub expert_source: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct EvaluationSubjectReport {
    pub subject_id: String,
    pub display_name: String,
    pub kind: String,
    pub completed: bool,
    pub final_score: i64,
    pub deal_in_count: u64,
    pub win_count: u64,
}

fn default_max_actions_per_match() -> usize {
    2400
}

impl EvaluationArenaConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.subjects.is_empty() {
            return Err("evaluation requires at least one subject".to_string());
        }
        if self.opponents.len() != 3 {
            return Err("evaluation requires exactly three opponents".to_string());
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn new_for_test(
        matches: usize,
        seed: u64,
        subjects: Vec<BotPolicyConfig>,
        opponents: Vec<BotPolicyConfig>,
    ) -> Result<Self, String> {
        let config = Self {
            matches,
            seed,
            max_actions_per_match: default_max_actions_per_match(),
            report_trajectories: false,
            subjects: subjects
                .into_iter()
                .map(|policy| EvaluationSubjectPolicyConfig {
                    display_name: policy.id.clone(),
                    policy,
                })
                .collect(),
            opponents,
            expert_source: None,
        };
        config.validate()?;
        Ok(config)
    }
}

pub fn apply_evaluation_rules(room: &mut RoomState) {
    room.mode = EVALUATION_ROOM_MODE.to_string();
    room.minimum_hu_fan = EVALUATION_MINIMUM_HU_FAN;
    room.dealer_repeat_enabled = false;
    room.dealer_double_enabled = false;
    room.player_multiplier_selection_enabled = false;
    room.ready_hand_enabled = false;
}

pub fn evaluation_match_seeds(seed: u64, matches: usize) -> Vec<u64> {
    (0..matches)
        .map(|match_index| seed.wrapping_add(match_index as u64))
        .collect()
}

pub fn default_sft_opponents() -> Vec<BotPolicyConfig> {
    (0..3)
        .map(|index| BotPolicyConfig {
            id: format!("sft-opponent-{}", index + 1),
            model_path: Some(crate::bot_config::SFT_MODEL_PATH.to_string()),
            sample_actions: false,
            temperature: 1.0,
            temperature_range: None,
            discard_base_risk_weight: 0.90,
            discard_value_risk_range: 0.55,
            discard_min_risk_weight: 0.25,
            discard_max_risk_weight: 1.45,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::state::RoomState;

    #[test]
    fn evaluation_room_rules_are_fixed() {
        let mut room = RoomState::default();
        room.minimum_hu_fan = 0;
        room.dealer_repeat_enabled = true;
        room.dealer_double_enabled = true;

        apply_evaluation_rules(&mut room);

        assert_eq!(room.mode, EVALUATION_ROOM_MODE);
        assert_eq!(room.minimum_hu_fan, 8);
        assert!(!room.dealer_repeat_enabled);
        assert!(!room.dealer_double_enabled);
    }

    #[test]
    fn evaluation_requires_exactly_three_opponents() {
        let subject = test_policy("candidate");
        let opponents = vec![test_policy("a"), test_policy("b")];

        let result = EvaluationArenaConfig::new_for_test(1, 7, vec![subject], opponents);

        assert!(result.is_err());
    }

    #[test]
    fn replicated_match_seeds_are_stable_per_subject() {
        let seeds = evaluation_match_seeds(100, 3);

        assert_eq!(seeds, vec![100, 101, 102]);
    }

    fn test_policy(id: &str) -> BotPolicyConfig {
        BotPolicyConfig {
            id: id.to_string(),
            model_path: Some(crate::bot_config::SFT_MODEL_PATH.to_string()),
            sample_actions: false,
            temperature: 1.0,
            temperature_range: None,
            discard_base_risk_weight: 0.90,
            discard_value_risk_range: 0.55,
            discard_min_risk_weight: 0.25,
            discard_max_risk_weight: 1.45,
        }
    }
}
