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
    seat_index: Seat,
    options: Vec<String>,
    deadline_at: Option<String>,
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
            seat_index: pending.seat_index().unwrap_or(local_seat),
            options,
            deadline_at: pending.deadline_at(),
        },
    })
    .ok()
}

impl PendingActionView {
    pub fn options(&self) -> Vec<String> {
        match self {
            Self::OpeningFlowers { options, .. }
            | Self::ActiveTurn { options, .. }
            | Self::ClaimWindow { options, .. }
            | Self::RobKongWindow { options, .. }
            | Self::SkillDraft { options, .. } => options.clone(),
        }
    }

    pub fn deadline_at(&self) -> Option<String> {
        match self {
            Self::OpeningFlowers { deadline_at, .. }
            | Self::ActiveTurn { deadline_at, .. }
            | Self::ClaimWindow { deadline_at, .. }
            | Self::RobKongWindow { deadline_at, .. }
            | Self::SkillDraft { deadline_at, .. } => deadline_at.clone(),
        }
    }

    pub fn seat_index(&self) -> Option<Seat> {
        match self {
            Self::OpeningFlowers { seat_index, .. }
            | Self::ActiveTurn { seat_index, .. }
            | Self::SkillDraft { seat_index, .. } => {
                Some(*seat_index)
            }
            Self::ClaimWindow { .. } | Self::RobKongWindow { .. } => None,
        }
    }
}
