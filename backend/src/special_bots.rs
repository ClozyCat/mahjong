use crate::bot::arena::{ArenaBotPolicyConfig, ArenaPolicyMode};
use crate::core::state::{RoomState, SeatState};

pub(crate) const SPECIAL_BOT_SEAT_TYPE: &str = "special_bot";
pub(crate) const SFT_MODEL_PATH: &str = "backend/assets/sft/sft.onnx";
pub(crate) const PPO_MODEL_PATH: &str = "backend/assets/ppo/ppo.onnx";

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SpecialBotDefinition {
    pub(crate) username: &'static str,
    pub(crate) display_name: &'static str,
    pub(crate) model_path: &'static str,
    pub(crate) temperature: f32,
}

const SPECIAL_BOT_DEFINITIONS: [SpecialBotDefinition; 4] = [
    SpecialBotDefinition {
        username: "bot_schubert",
        display_name: "舒伯特",
        model_path: SFT_MODEL_PATH,
        temperature: 1.0,
    },
    SpecialBotDefinition {
        username: "bot_andersen",
        display_name: "爱伦坡",
        model_path: PPO_MODEL_PATH,
        temperature: 0.3,
    },
    SpecialBotDefinition {
        username: "bot_balzac",
        display_name: "巴尔扎克",
        model_path: PPO_MODEL_PATH,
        temperature: 1.0,
    },
    SpecialBotDefinition {
        username: "bot_dickens",
        display_name: "狄更斯",
        model_path: PPO_MODEL_PATH,
        temperature: 2.0,
    },
];

pub(crate) fn definitions() -> &'static [SpecialBotDefinition] {
    &SPECIAL_BOT_DEFINITIONS
}

pub(crate) fn is_special_bot_username(username: &str) -> bool {
    SPECIAL_BOT_DEFINITIONS
        .iter()
        .any(|bot| bot.username == username)
}

pub(crate) fn definition_for_display_name(display_name: &str) -> Option<&'static SpecialBotDefinition> {
    SPECIAL_BOT_DEFINITIONS
        .iter()
        .find(|bot| bot.display_name == display_name)
}

pub(crate) fn model_path_for_display_name(display_name: &str) -> Option<&'static str> {
    definition_for_display_name(display_name)
        .map(|bot| bot.model_path)
}

pub(crate) fn temperature_for_display_name(display_name: &str) -> Option<f32> {
    definition_for_display_name(display_name).map(|bot| bot.temperature)
}

pub(crate) fn is_special_bot_seat(seat: &SeatState) -> bool {
    seat.seat_type == SPECIAL_BOT_SEAT_TYPE
}

pub(crate) fn seat_blocks_public_records(seat: &SeatState) -> bool {
    is_independent_bot_seat(seat)
}

pub(crate) fn is_independent_bot_seat(seat: &SeatState) -> bool {
    seat.seat_type == "bot" || (seat.seat_type.is_empty() && seat.is_bot)
}

pub(crate) fn policy_config_for_seat(room: &RoomState, seat_index: usize) -> ArenaBotPolicyConfig {
    let (policy_id, model_path, temperature) = room
        .seats
        .iter()
        .find(|seat| seat.seat_index == seat_index)
        .and_then(|seat| {
            seat.nickname
                .as_deref()
                .and_then(definition_for_display_name)
                .map(|bot| ("special", bot.model_path, bot.temperature))
        })
        .unwrap_or(("sft", SFT_MODEL_PATH, 1.0));

    ArenaBotPolicyConfig {
        id: format!("{policy_id}-seat-{seat_index}"),
        mode: ArenaPolicyMode::Neural,
        model_path: Some(model_path.to_string()),
        sample_actions: false,
        temperature,
        record_heuristic_comparison: false,
        discard_base_risk_weight: 0.90,
        discard_value_risk_range: 0.55,
        discard_min_risk_weight: 0.25,
        discard_max_risk_weight: 1.45,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_special_bot_names_to_model_paths() {
        assert_eq!(
            model_path_for_display_name("舒伯特"),
            Some(SFT_MODEL_PATH)
        );
        assert_eq!(
            model_path_for_display_name("爱伦坡"),
            Some(PPO_MODEL_PATH)
        );
        assert_eq!(
            model_path_for_display_name("巴尔扎克"),
            Some(PPO_MODEL_PATH)
        );
        assert_eq!(
            model_path_for_display_name("狄更斯"),
            Some(PPO_MODEL_PATH)
        );
        assert_eq!(model_path_for_display_name("爱因斯坦"), None);
        assert_eq!(model_path_for_display_name("伯努利"), None);
        assert_eq!(model_path_for_display_name("达尔文"), None);
        assert_eq!(model_path_for_display_name("莎士比亚"), None);
        assert_eq!(model_path_for_display_name("安徒生"), None);
    }

    #[test]
    fn maps_special_bot_names_to_temperatures() {
        assert_eq!(temperature_for_display_name("爱伦坡"), Some(0.3));
        assert_eq!(temperature_for_display_name("巴尔扎克"), Some(1.0));
        assert_eq!(temperature_for_display_name("狄更斯"), Some(2.0));
        assert_eq!(temperature_for_display_name("爱因斯坦"), None);
    }

    #[test]
    fn policy_config_uses_special_model_only_for_special_bot_seats() {
        let room = RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            minimum_hu_fan: crate::core::state::room::default_minimum_hu_fan(),
            dealer_repeat_enabled: false,
            dealer_double_enabled: false,
            seats: vec![
                SeatState {
                    seat_index: 0,
                    nickname: Some("舒伯特".to_string()),
                    is_bot: true,
                    seat_type: SPECIAL_BOT_SEAT_TYPE.to_string(),
                    ..SeatState::default()
                },
                SeatState {
                    seat_index: 1,
                    nickname: Some("bot_1".to_string()),
                    is_bot: true,
                    seat_type: "bot".to_string(),
                    ..SeatState::default()
                },
            ],
            match_state: None,
            round_state: None,
            pending_timeout: None,
            continue_action: None,
        };

        assert_eq!(
            policy_config_for_seat(&room, 0).model_path.as_deref(),
            Some(SFT_MODEL_PATH)
        );
        assert_eq!(
            policy_config_for_seat(&room, 1).model_path.as_deref(),
            Some(SFT_MODEL_PATH)
        );
    }
}
