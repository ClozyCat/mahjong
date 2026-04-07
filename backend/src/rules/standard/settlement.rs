use serde_json::{Map, Value, json};

use crate::core::engine::planner::plan_settlement_to_match;
use crate::core::engine::reducer::{LegacyRoomMutation, apply_legacy_room_mutations};

use super::runtime::{project_room_state, round_event_message};

const MAX_SEATS: usize = 4;

pub fn settle_exhaustive_draw(room: &mut Value) -> Vec<Value> {
    let seat_count = room
        .get("round_state")
        .and_then(|round| round.get("players"))
        .and_then(Value::as_array)
        .map(|players| players.len())
        .unwrap_or(MAX_SEATS);
    let kong_delta = kong_delta_by_seat_from_room(room);
    let settlement = json!({
        "provisional": true,
        "win_type": "draw",
        "winner_seat": Value::Null,
        "discarder_seat": Value::Null,
        "fan_total": 0,
        "fan_keys": [],
        "fan_breakdown": [],
        "score_delta": {
            "provisional": true,
            "fan_total": 0,
            "fan_delta_by_seat": zero_score_map(seat_count),
            "kong_delta_by_seat": kong_delta.clone(),
            "total_delta_by_seat": kong_delta,
        },
        "flower_count": 0,
        "draw_type": "exhaustive",
        "kong_score_detail": room
            .get("round_state")
            .and_then(|round| round.get("score_trackers"))
            .and_then(|trackers| trackers.get("kong_entries"))
            .cloned()
            .unwrap_or_else(|| Value::Array(vec![])),
    });
    let mut mutations = vec![
        LegacyRoomMutation::SetRoomField {
            key: "phase".to_string(),
            value: Value::String("settlement".to_string()),
        },
        LegacyRoomMutation::SetRoomField {
            key: "pending_timeout".to_string(),
            value: Value::Null,
        },
        LegacyRoomMutation::SetRoundField {
            key: "phase".to_string(),
            value: Value::String("settlement".to_string()),
        },
        LegacyRoomMutation::SetRoundPendingAction {
            pending_action: Value::Null,
        },
        LegacyRoomMutation::SetRoundField {
            key: "settlement".to_string(),
            value: settlement.clone(),
        },
        LegacyRoomMutation::IncrementRoundVersion,
    ];
    if let Ok(state) = project_room_state(room) {
        mutations.extend(plan_settlement_to_match(&state, &settlement));
    }
    let _ = apply_legacy_room_mutations(room, &mutations);
    apply_settlement_to_match(room);
    vec![round_event_message(
        "round_drawn",
        json!({
            "type": "round_drawn",
            "round_id": room
                .get("round_state")
                .and_then(|round| round.get("round_id"))
                .cloned()
                .unwrap_or(Value::Null),
        }),
    )]
}

pub fn apply_settlement_to_match(room: &mut Value) {
    let mutations = room
        .get("round_state")
        .and_then(|round| round.get("settlement"))
        .cloned()
        .and_then(|settlement| {
            project_room_state(room)
                .ok()
                .map(|state| plan_settlement_to_match(&state, &settlement))
        })
        .unwrap_or_default();
    let _ = apply_legacy_room_mutations(room, &mutations);
}

fn kong_delta_by_seat_from_room(room: &Value) -> Value {
    let seat_count = room
        .get("round_state")
        .and_then(|round| round.get("players"))
        .and_then(Value::as_array)
        .map(|players| players.len())
        .unwrap_or(MAX_SEATS);
    let mut deltas = vec![0_i64; seat_count];
    let kong_entries = room
        .get("round_state")
        .and_then(|round| round.get("score_trackers"))
        .and_then(|trackers| trackers.get("kong_entries"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for entry in kong_entries {
        let actor_seat = entry
            .get("actor_seat")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(0);
        let payer_seats = entry
            .get("payer_seats")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for payer in payer_seats {
            let payer_seat = payer.as_u64().map(|value| value as usize).unwrap_or(0);
            if payer_seat < deltas.len() {
                deltas[payer_seat] -= 1;
            }
            if actor_seat < deltas.len() {
                deltas[actor_seat] += 1;
            }
        }
    }
    let mut map = Map::new();
    for (seat_index, delta) in deltas.into_iter().enumerate() {
        map.insert(seat_index.to_string(), Value::Number(delta.into()));
    }
    Value::Object(map)
}

fn zero_score_map(seat_count: usize) -> Value {
    let mut map = Map::new();
    for seat in 0..seat_count {
        map.insert(seat.to_string(), Value::Number(0.into()));
    }
    Value::Object(map)
}
