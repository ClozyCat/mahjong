use crate::bot::policy::BotPolicyConfig;
use crate::core::state::{RoomState, SeatState};
use std::sync::OnceLock;

pub(crate) const SPECIAL_BOT_SEAT_TYPE: &str = "special_bot";
pub(crate) const SFT_MODEL_PATH: &str = "backend/assets/sft/sft.onnx";

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SpecialBotDefinition {
    pub(crate) username: &'static str,
    pub(crate) display_name: &'static str,
    pub(crate) model_path_env: &'static str,
    pub(crate) temperature: f32,
}

const SPECIAL_BOT_DEFINITIONS: [SpecialBotDefinition; 4] = [
    SpecialBotDefinition {
        username: "bot_schubert",
        display_name: "舒伯特",
        model_path_env: "MAHJONG_BOT_SCHUBERT_MODEL_PATH",
        temperature: 1.0,
    },
    SpecialBotDefinition {
        username: "bot_andersen",
        display_name: "爱伦坡",
        model_path_env: "MAHJONG_BOT_ANDERSEN_MODEL_PATH",
        temperature: 0.3,
    },
    SpecialBotDefinition {
        username: "bot_balzac",
        display_name: "巴尔扎克",
        model_path_env: "MAHJONG_BOT_BALZAC_MODEL_PATH",
        temperature: 1.0,
    },
    SpecialBotDefinition {
        username: "bot_dickens",
        display_name: "狄更斯",
        model_path_env: "MAHJONG_BOT_DICKENS_MODEL_PATH",
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

pub(crate) fn definition_for_display_name(
    display_name: &str,
) -> Option<&'static SpecialBotDefinition> {
    SPECIAL_BOT_DEFINITIONS
        .iter()
        .find(|bot| bot.display_name == display_name)
}

fn default_model_path() -> &'static str {
    static DEFAULT_PATH: OnceLock<&str> = OnceLock::new();
    DEFAULT_PATH.get_or_init(|| {
        if let Ok(path) = std::env::var("MAHJONG_BOT_MODEL_PATH") {
            Box::leak(path.into_boxed_str())
        } else {
            SFT_MODEL_PATH
        }
    })
}

fn resolve_bot_model_path(env_name: &str) -> String {
    std::env::var(env_name).unwrap_or_else(|_| default_model_path().to_string())
}

#[cfg(test)]
pub(crate) fn model_path_for_display_name(display_name: &str) -> Option<String> {
    definition_for_display_name(display_name).map(|bot| resolve_bot_model_path(bot.model_path_env))
}

#[cfg(test)]
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

pub(crate) fn policy_config_for_seat(room: &RoomState, seat_index: usize) -> BotPolicyConfig {
    let (policy_id, model_path, temperature, sample_actions) = room
        .seats
        .iter()
        .find(|seat| seat.seat_index == seat_index)
        .and_then(|seat| {
            seat.nickname
                .as_deref()
                .and_then(definition_for_display_name)
                .map(|bot| {
                    (
                        "special",
                        resolve_bot_model_path(bot.model_path_env),
                        bot.temperature,
                        true,
                    )
                })
        })
        .unwrap_or(("sft", default_model_path().to_string(), 1.0, false));
    let is_evaluation = room.mode == crate::evaluation::EVALUATION_ROOM_MODE;

    BotPolicyConfig {
        id: format!("{policy_id}-seat-{seat_index}"),
        model_path: Some(model_path),
        sample_actions: sample_actions && !is_evaluation,
        temperature: if is_evaluation { 1.0 } else { temperature },
        temperature_range: None,
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
        // When env vars are not set, falls back to MAHJONG_BOT_MODEL_PATH default
        let _ = model_path_for_display_name("舒伯特");
        let _ = model_path_for_display_name("爱伦坡");
        let _ = model_path_for_display_name("巴尔扎克");
        let _ = model_path_for_display_name("狄更斯");
        assert!(model_path_for_display_name("爱因斯坦").is_none());
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
            player_multiplier_selection_enabled: false,
            ready_hand_enabled: true,
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

        let seat0_path = policy_config_for_seat(&room, 0).model_path;
        let seat1_path = policy_config_for_seat(&room, 1).model_path;
        assert!(seat0_path.is_some());
        assert!(seat1_path.is_some());
    }

    #[test]
    fn policy_config_samples_only_for_named_special_bots() {
        let room = RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            minimum_hu_fan: crate::core::state::room::default_minimum_hu_fan(),
            dealer_repeat_enabled: false,
            dealer_double_enabled: false,
            player_multiplier_selection_enabled: false,
            ready_hand_enabled: true,
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

        assert!(policy_config_for_seat(&room, 0).sample_actions);
        assert!(!policy_config_for_seat(&room, 1).sample_actions);
    }

    #[test]
    fn evaluation_special_bot_policy_uses_deterministic_arena_style() {
        let room = RoomState {
            table_code: "EVAL42".to_string(),
            phase: "playing".to_string(),
            mode: crate::evaluation::EVALUATION_ROOM_MODE.to_string(),
            owner_user_id: None,
            multiplier: 1,
            minimum_hu_fan: crate::evaluation::EVALUATION_MINIMUM_HU_FAN,
            dealer_repeat_enabled: false,
            dealer_double_enabled: false,
            player_multiplier_selection_enabled: false,
            ready_hand_enabled: false,
            seats: vec![SeatState {
                seat_index: 0,
                nickname: Some("巴尔扎克".to_string()),
                is_bot: true,
                seat_type: SPECIAL_BOT_SEAT_TYPE.to_string(),
                ..SeatState::default()
            }],
            match_state: None,
            round_state: None,
            pending_timeout: None,
            continue_action: None,
        };

        let policy = policy_config_for_seat(&room, 0);

        assert!(policy.model_path.is_some());
        assert!(!policy.sample_actions);
        assert_eq!(policy.temperature, 1.0);
    }
}
