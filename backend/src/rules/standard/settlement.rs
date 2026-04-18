use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::core::engine::EngineOutput;
use crate::core::engine::planner::plan_settlement_to_match;
use crate::core::event::GameEvent;
use crate::core::ids::Seat;
use crate::core::state::settlement::zero_score_map;
use crate::core::state::{
    KongTrackerEntry, RoomState, RoundSettlement, SettlementKongScoreDetailEntry,
    SettlementScoreDelta,
};

use super::runtime::round_event_message;

#[cfg(test)]
use super::runtime::project_room_state;
#[cfg(test)]
use crate::core::engine::reducer::update_room_state;

const MAX_SEATS: usize = 4;

#[cfg(test)]
pub fn settle_exhaustive_draw_output(room: &mut Value) -> EngineOutput {
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
    let settlement = RoundSettlement {
        provisional: true,
        win_type: "draw".to_string(),
        winner_seat: None,
        discarder_seat: None,
        display_win_label: None,
        fan_total: 0,
        fan_keys: vec![],
        fan_breakdown: vec![],
        winning_details: vec![],
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
    let settlement_for_write = settlement.clone();
    let settlement_match_plan = projected_state
        .as_ref()
        .and_then(|state| plan_settlement_to_match(state, &settlement_for_write));
    let _ = update_room_state(room, |state| {
        state.phase = "settlement".to_string();
        state.pending_timeout = None;
        if let Some(round) = state.round_state.as_mut() {
            round.phase = "settlement".to_string();
            round.pending_action = None;
            round.settlement = Some(settlement_for_write.clone());
            round.version += 1;
        }
        if let Some(plan) = settlement_match_plan.as_ref() {
            if let Some(match_state) = state.match_state.as_mut() {
                match_state.apply_completed_round(
                    plan.round_id.clone(),
                    plan.cumulative_scores.clone(),
                    &settlement_for_write,
                );
            }
        }
        Ok(())
    });
    apply_settlement_to_match(room);
    let message = round_event_message(
        "round_drawn",
        json!({
            "type": "round_drawn",
            "round_id": room
                .get("round_state")
                .and_then(|round| round.get("round_id"))
                .cloned()
                .unwrap_or(Value::Null),
            "settlement": settlement.to_value(),
        }),
    );
    EngineOutput::new(
        vec![GameEvent::SettlementPrepared { settlement }],
        vec![message],
    )
}

pub fn settle_exhaustive_draw_output_in_room_state(room: &mut RoomState) -> EngineOutput {
    let seat_count = room
        .round_state
        .as_ref()
        .map(|round| round.players.len())
        .unwrap_or(MAX_SEATS);
    let kong_score_detail = room
        .round_state
        .as_ref()
        .map(|round| {
            kong_score_detail_from_trackers(&round.score_trackers.kong_entries, seat_count)
        })
        .unwrap_or_default();
    let kong_delta = total_kong_delta_by_seat(&kong_score_detail, seat_count);
    let settlement = RoundSettlement {
        provisional: true,
        win_type: "draw".to_string(),
        winner_seat: None,
        discarder_seat: None,
        display_win_label: None,
        fan_total: 0,
        fan_keys: vec![],
        fan_breakdown: vec![],
        winning_details: vec![],
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
    let settlement_for_write = settlement.clone();
    let settlement_match_plan = plan_settlement_to_match(room, &settlement_for_write);
    room.phase = "settlement".to_string();
    room.pending_timeout = None;
    if let Some(round) = room.round_state.as_mut() {
        round.phase = "settlement".to_string();
        round.pending_action = None;
        round.settlement = Some(settlement_for_write);
        round.version += 1;
    }
    if let Some(plan) = settlement_match_plan {
        if let Some(match_state) = room.match_state.as_mut() {
            match_state.apply_completed_round(plan.round_id, plan.cumulative_scores, &settlement);
        }
    }
    apply_settlement_to_match_in_room_state(room);
    let message = round_event_message(
        "round_drawn",
        json!({
            "type": "round_drawn",
            "round_id": room
                .round_state
                .as_ref()
                .map(|round| Value::String(round.round_id.clone()))
                .unwrap_or(Value::Null),
            "settlement": settlement.to_value(),
        }),
    );
    EngineOutput::new(
        vec![GameEvent::SettlementPrepared { settlement }],
        vec![message],
    )
}

#[cfg(test)]
pub fn apply_settlement_to_match(room: &mut Value) {
    let plan = project_room_state(room).ok().and_then(|state| {
        state.round_state.as_ref().and_then(|round| {
            round
                .settlement
                .as_ref()
                .and_then(|settlement| plan_settlement_to_match(&state, settlement))
        })
    });
    let _ = update_room_state(room, |state| {
        if let Some(plan) = plan.as_ref() {
            let settlement = state
                .round_state
                .as_ref()
                .and_then(|round| round.settlement.as_ref())
                .cloned();
            if let Some(match_state) = state.match_state.as_mut() {
                if let Some(settlement) = settlement {
                    match_state.apply_completed_round(
                        plan.round_id.clone(),
                        plan.cumulative_scores.clone(),
                        &settlement,
                    );
                }
            }
        }
        Ok(())
    });
}

pub fn apply_settlement_to_match_in_room_state(room: &mut RoomState) {
    let plan = room.round_state.as_ref().and_then(|round| {
        round
            .settlement
            .as_ref()
            .and_then(|settlement| plan_settlement_to_match(room, settlement))
    });
    if let Some(plan) = plan {
        let settlement = room
            .round_state
            .as_ref()
            .and_then(|round| round.settlement.as_ref())
            .cloned();
        if let Some(match_state) = room.match_state.as_mut() {
            if let Some(settlement) = settlement {
                match_state.apply_completed_round(
                    plan.round_id,
                    plan.cumulative_scores,
                    &settlement,
                );
            }
        }
    }
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
