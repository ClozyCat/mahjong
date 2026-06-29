use crate::bot::policy::BotPolicyConfig;
use crate::core::state::{RoomState, SeatState};
use std::sync::OnceLock;

pub(crate) const SFT_MODEL_PATH: &str = "backend/assets/sft/sft.onnx";

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

pub(crate) fn is_independent_bot_seat(seat: &SeatState) -> bool {
    seat.seat_type == "bot" || (seat.seat_type.is_empty() && seat.is_bot)
}

pub(crate) fn seat_blocks_public_records(seat: &SeatState) -> bool {
    is_independent_bot_seat(seat)
}

pub(crate) fn policy_config_for_seat(_room: &RoomState, seat_index: usize) -> BotPolicyConfig {
    BotPolicyConfig {
        id: format!("sft-seat-{seat_index}"),
        model_path: Some(default_model_path().to_string()),
        sample_actions: false,
        temperature: 1.0,
        temperature_range: None,
        discard_base_risk_weight: 0.90,
        discard_value_risk_range: 0.55,
        discard_min_risk_weight: 0.25,
        discard_max_risk_weight: 1.45,
    }
}
