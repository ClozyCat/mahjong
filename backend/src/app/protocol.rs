use serde::Serialize;
use serde_json::Value;

use crate::core::state::SeatState;

#[derive(Debug, Clone, Serialize)]
struct PayloadEnvelope<T> {
    #[serde(rename = "type")]
    kind: &'static str,
    payload: T,
}

#[derive(Debug, Clone, Serialize)]
struct ReasonPayload {
    reason: String,
}

#[derive(Debug, Clone, Serialize)]
struct PlayerPresencePayload {
    table_code: String,
    seat_index: usize,
    connected: bool,
}

#[derive(Debug, Clone, Serialize)]
struct QuickChatPayload {
    message_id: String,
    actor_seat: usize,
    target_seat: usize,
    emoji: String,
    sent_at: String,
}

#[derive(Debug, Clone, Serialize)]
struct DealerSelectionStartedPayload {
    event_type: &'static str,
    event: DealerSelectionStartedEvent,
}

#[derive(Debug, Clone, Serialize)]
struct DealerSelectionStartedEvent {
    #[serde(rename = "type")]
    kind: &'static str,
    dealer_seat: usize,
    started_at: String,
    reveal_at: String,
    duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
struct LeaveTableAcceptedPayload {
    table_code: String,
    seat_index: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CreateTableResponsePayload {
    pub(crate) table_code: String,
    pub(crate) phase: String,
    pub(crate) mode: String,
    pub(crate) created_at: String,
    pub(crate) seats: Vec<SeatState>,
}

#[derive(Debug, Clone, Serialize)]
struct DetailPayload {
    detail: String,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, Default)]
#[serde(default)]
pub(crate) struct HeartbeatPayload {
    pub(crate) request_id: Option<String>,
    pub(crate) sent_at: Option<String>,
}

pub(crate) fn action_rejected_message(reason: &str) -> Value {
    serde_json::to_value(PayloadEnvelope {
        kind: "action_rejected",
        payload: ReasonPayload {
            reason: reason.to_string(),
        },
    })
    .unwrap_or_else(|_| {
        serde_json::json!({
            "type": "action_rejected",
            "payload": { "reason": reason }
        })
    })
}

pub(crate) fn heartbeat_message(payload: HeartbeatPayload) -> Value {
    serde_json::to_value(PayloadEnvelope {
        kind: "heartbeat",
        payload,
    })
    .unwrap_or_else(|_| {
        serde_json::json!({
            "type": "heartbeat",
            "payload": Value::Null,
        })
    })
}

pub(crate) fn player_presence_message(
    table_code: &str,
    seat_index: usize,
    connected: bool,
) -> Value {
    serde_json::to_value(PayloadEnvelope {
        kind: "player_presence",
        payload: PlayerPresencePayload {
            table_code: table_code.to_string(),
            seat_index,
            connected,
        },
    })
    .unwrap_or_else(|_| {
        serde_json::json!({
            "type": "player_presence",
            "payload": {
                "table_code": table_code,
                "seat_index": seat_index,
                "connected": connected,
            }
        })
    })
}

pub(crate) fn quick_chat_message(
    message_id: String,
    actor_seat: usize,
    target_seat: usize,
    emoji: String,
    sent_at: String,
) -> Value {
    let fallback_emoji = emoji.clone();
    serde_json::to_value(PayloadEnvelope {
        kind: "quick_chat",
        payload: QuickChatPayload {
            message_id,
            actor_seat,
            target_seat,
            emoji,
            sent_at,
        },
    })
    .unwrap_or_else(|_| {
        serde_json::json!({
            "type": "quick_chat",
            "payload": {
                "actor_seat": actor_seat,
                "target_seat": target_seat,
                "emoji": fallback_emoji,
            }
        })
    })
}

pub(crate) fn dealer_selection_started_message(
    dealer_seat: usize,
    started_at: String,
    reveal_at: String,
    duration_ms: u64,
) -> Value {
    serde_json::to_value(PayloadEnvelope {
        kind: "round_event",
        payload: DealerSelectionStartedPayload {
            event_type: "dealer_selection_started",
            event: DealerSelectionStartedEvent {
                kind: "dealer_selection_started",
                dealer_seat,
                started_at: started_at.clone(),
                reveal_at: reveal_at.clone(),
                duration_ms,
            },
        },
    })
    .unwrap_or_else(|_| {
        serde_json::json!({
            "type": "round_event",
            "payload": {
                "event_type": "dealer_selection_started",
                "event": {
                    "type": "dealer_selection_started",
                    "dealer_seat": dealer_seat,
                    "started_at": started_at,
                    "reveal_at": reveal_at,
                    "duration_ms": duration_ms,
                },
            },
        })
    })
}

pub(crate) fn leave_table_accepted_message(table_code: &str, seat_index: usize) -> Value {
    serde_json::to_value(PayloadEnvelope {
        kind: "leave_table_accepted",
        payload: LeaveTableAcceptedPayload {
            table_code: table_code.to_string(),
            seat_index,
        },
    })
    .unwrap_or_else(|_| {
        serde_json::json!({
            "type": "leave_table_accepted",
            "payload": {
                "table_code": table_code,
                "seat_index": seat_index,
            }
        })
    })
}

pub(crate) fn create_table_response(
    table_code: &str,
    mode: &str,
    created_at: &str,
    seats: Vec<SeatState>,
) -> Value {
    serde_json::to_value(CreateTableResponsePayload {
        table_code: table_code.to_string(),
        phase: "waiting".to_string(),
        mode: mode.to_string(),
        created_at: created_at.to_string(),
        seats,
    })
    .unwrap_or_else(|_| {
        serde_json::json!({
            "table_code": table_code,
            "phase": "waiting",
            "mode": mode,
            "created_at": created_at,
            "seats": [],
        })
    })
}

pub(crate) fn detail_response(detail: &str) -> Value {
    serde_json::to_value(DetailPayload {
        detail: detail.to_string(),
    })
    .unwrap_or_else(|_| serde_json::json!({ "detail": detail }))
}
