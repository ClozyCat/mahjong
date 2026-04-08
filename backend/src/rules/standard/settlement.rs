use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::core::engine::planner::plan_settlement_to_match;
use crate::core::engine::reducer::{LegacyRoomMutation, apply_legacy_room_mutations};
use crate::core::ids::Seat;
use crate::core::state::settlement::zero_score_map;
use crate::core::state::{
    KongTrackerEntry, RoundSettlement, SettlementKongScoreDetailEntry, SettlementScoreDelta,
};
use crate::rules::skills::{
    apply_draw_settlement_hooks, sync_match_skill_trackers_after_settlement,
};

use super::runtime::{project_room_state, round_event_message};

const MAX_SEATS: usize = 4;

pub fn settle_exhaustive_draw(room: &mut Value) -> Vec<Value> {
    let projected_state = project_room_state(room).ok();
    let seat_count = projected_state
        .as_ref()
        .and_then(|state| state.round_state.as_ref())
        .map(|round| round.players.len())
        .or_else(|| {
            room.get("round_state")
                .and_then(|round| round.get("players"))
                .and_then(Value::as_array)
                .map(|players| players.len())
        })
        .unwrap_or(MAX_SEATS);
    let kong_score_detail = projected_state
        .as_ref()
        .and_then(|state| state.round_state.as_ref())
        .map(|round| {
            kong_score_detail_from_trackers(&round.score_trackers.kong_entries, seat_count)
        })
        .unwrap_or_default();
    let kong_delta = total_kong_delta_by_seat(&kong_score_detail, seat_count);
    let mut settlement = RoundSettlement {
        provisional: true,
        win_type: "draw".to_string(),
        winner_seat: None,
        discarder_seat: None,
        display_win_label: None,
        fan_total: 0,
        fan_keys: vec![],
        fan_breakdown: vec![],
        score_delta: SettlementScoreDelta {
            provisional: true,
            basic_points: 0,
            base_points: 0,
            fan_total: 0,
            minimum_qualifying_fan_total: 0,
            fan_delta_by_seat: zero_score_map(seat_count),
            kong_delta_by_seat: kong_delta.clone(),
            total_delta_by_seat: kong_delta,
        },
        flower_count: 0,
        draw_type: Some("exhaustive".to_string()),
        kong_score_detail,
    };
    if let Some(state) = projected_state.as_ref() {
        let _ = apply_draw_settlement_hooks(state, &mut settlement);
    }
    let mut mutations = vec![
        LegacyRoomMutation::SetRoomPhase {
            phase: "settlement".to_string(),
        },
        LegacyRoomMutation::SetRoomPendingTimeout {
            pending_timeout: None,
        },
        LegacyRoomMutation::SetRoundPhase {
            phase: "settlement".to_string(),
        },
        LegacyRoomMutation::SetRoundPendingAction {
            pending_action: None,
        },
        LegacyRoomMutation::SetRoundSettlement {
            settlement: Some(settlement.clone()),
        },
        LegacyRoomMutation::IncrementRoundVersion,
    ];
    if let Ok(state) = project_room_state(room) {
        mutations.extend(plan_settlement_to_match(&state, &settlement));
    }
    let _ = apply_legacy_room_mutations(room, &mutations);
    sync_match_skill_trackers_after_settlement(room);
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
            "settlement": settlement.to_legacy_value(),
        }),
    )]
}

pub fn apply_settlement_to_match(room: &mut Value) {
    let mutations = project_room_state(room)
        .ok()
        .and_then(|state| {
            state.round_state.as_ref().and_then(|round| {
                round
                    .settlement
                    .as_ref()
                    .map(|settlement| plan_settlement_to_match(&state, settlement))
            })
        })
        .unwrap_or_default();
    let _ = apply_legacy_room_mutations(room, &mutations);
}

fn kong_score_detail_from_trackers(
    entries: &[KongTrackerEntry],
    seat_count: usize,
) -> Vec<SettlementKongScoreDetailEntry> {
    entries
        .iter()
        .map(|entry| SettlementKongScoreDetailEntry::from_tracker_entry(entry, seat_count))
        .collect()
}

fn total_kong_delta_by_seat(
    entries: &[SettlementKongScoreDetailEntry],
    seat_count: usize,
) -> BTreeMap<Seat, i64> {
    let mut totals = zero_score_map(seat_count);
    for entry in entries {
        for seat in 0..seat_count {
            *totals.entry(seat).or_default() +=
                entry.delta_by_seat.get(&seat).copied().unwrap_or(0);
        }
    }
    totals
}
