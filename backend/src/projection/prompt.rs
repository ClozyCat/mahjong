use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::core::ids::Seat;
use crate::core::state::RoomState;
use crate::projection::SeatProjectionSupport;
use crate::projection::room_snapshot::{PendingActionView, build_pending_action_view};

#[derive(Debug, Clone, Serialize)]
struct ActionPromptMessage {
    #[serde(rename = "type")]
    kind: &'static str,
    payload: ActionPromptPayload,
}

#[derive(Debug, Clone, Serialize)]
struct ActionPromptPayload {
    server_now: String,
    seat_index: Seat,
    options: Vec<String>,
    deadline_at: Option<String>,
    remaining_extra_time: Option<i64>,
}

pub fn action_prompt_message(
    state: &RoomState,
    local_seat: Seat,
    support: &SeatProjectionSupport,
) -> Option<Value> {
    let pending = build_pending_action_view(state, local_seat, support)?;
    let options = pending.options();
    if options.is_empty() {
        return None;
    }
    serde_json::to_value(ActionPromptMessage {
        kind: "action_prompt",
        payload: ActionPromptPayload {
            server_now: now_iso(),
            seat_index: pending.seat_index().unwrap_or(local_seat),
            options,
            deadline_at: pending.deadline_at(),
            remaining_extra_time: pending.remaining_extra_time(),
        },
    })
    .ok()
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true)
}

impl PendingActionView {
    pub fn options(&self) -> Vec<String> {
        match self {
            Self::ActiveTurn { options, .. }
            | Self::ClaimWindow { options, .. }
            | Self::RobKongWindow { options, .. }
            | Self::PlayerMultiplierSelection { options, .. } => options.clone(),
        }
    }

    pub fn deadline_at(&self) -> Option<String> {
        match self {
            Self::ActiveTurn { deadline_at, .. }
            | Self::ClaimWindow { deadline_at, .. }
            | Self::RobKongWindow { deadline_at, .. }
            | Self::PlayerMultiplierSelection { deadline_at, .. } => deadline_at.clone(),
        }
    }

    pub fn seat_index(&self) -> Option<Seat> {
        match self {
            Self::ActiveTurn { seat_index, .. } => Some(*seat_index),
            Self::ClaimWindow { .. }
            | Self::RobKongWindow { .. }
            | Self::PlayerMultiplierSelection { .. } => None,
        }
    }

    pub fn remaining_extra_time(&self) -> Option<i64> {
        match self {
            Self::ActiveTurn {
                remaining_extra_time,
                ..
            }
            | Self::ClaimWindow {
                remaining_extra_time,
                ..
            }
            | Self::RobKongWindow {
                remaining_extra_time,
                ..
            } => *remaining_extra_time,
            Self::PlayerMultiplierSelection { .. } => None,
        }
    }
}
