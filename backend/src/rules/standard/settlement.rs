use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::core::engine::EngineOutput;
use crate::core::engine::planner::plan_settlement_to_match;
use crate::core::engine::reducer::update_room_state;
use crate::core::event::GameEvent;
use crate::core::ids::Seat;
use crate::core::state::settlement::zero_score_map;
use crate::core::state::{
    KongTrackerEntry, RoomState, RoundSettlement, SettlementKongScoreDetailEntry,
    SettlementScoreDelta,
};
use crate::rules::skills::{
    apply_draw_settlement_hooks, sync_match_skill_trackers_after_settlement_in_room_state,
};

use super::runtime::{project_room_state, round_event_message};

const MAX_SEATS: usize = 4;

pub fn settle_exhaustive_draw(room: &mut Value) -> Vec<Value> {
    settle_exhaustive_draw_output(room).emitted_messages
}

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
        sync_match_skill_trackers_after_settlement_in_room_state(state);
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
    let _ = apply_draw_settlement_hooks(room, &mut settlement);
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
    sync_match_skill_trackers_after_settlement_in_room_state(room);
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::Value;

    use super::settle_exhaustive_draw_output;
    use crate::core::state::{
        LastActionContext, LianHuanJiTracker, MatchSkillTrackers, MatchState, PlayerRoundState,
        RoomState, RoundState, SeatState, ZouWeiShangJiTracker,
    };

    #[test]
    fn value_draw_settlement_output_syncs_match_skill_trackers_via_typed_helper() {
        let mut room = test_room_value_with_match_trackers();

        let _ = settle_exhaustive_draw_output(&mut room);

        let parsed = RoomState::from_room_value(&room).expect("room should remain typed");
        let trackers = &parsed
            .match_state
            .as_ref()
            .expect("match state")
            .skill_trackers;

        assert_eq!(trackers.lian_huan_ji.streaks.get(&0), Some(&0));
        assert_eq!(trackers.lian_huan_ji.streaks.get(&1), Some(&0));
        assert_eq!(trackers.lian_huan_ji.streaks.get(&2), Some(&0));
        assert_eq!(trackers.lian_huan_ji.streaks.get(&3), Some(&0));
        assert_eq!(
            trackers.zou_wei_shang_ji.pending_win_penalty.get(&2),
            Some(&4)
        );
    }

    fn test_room_value_with_match_trackers() -> Value {
        let state = RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            test_mode: false,
            enforce_minimum_eight_fan: true,
            seats: (0..4)
                .map(|seat_index| SeatState {
                    seat_index,
                    ..Default::default()
                })
                .collect(),
            match_state: Some(MatchState {
                prevailing_wind: "east".to_string(),
                hand_number: 1,
                dealer_seat: 0,
                cumulative_scores: (0..4).map(|seat| (seat, 0)).collect(),
                match_finished: false,
                last_completed_round_id: None,
                statistics: Default::default(),
                skill_trackers: MatchSkillTrackers {
                    lian_huan_ji: LianHuanJiTracker {
                        streaks: BTreeMap::from([(0, 2), (1, 5), (2, 1), (3, 3)]),
                    },
                    zou_wei_shang_ji: ZouWeiShangJiTracker {
                        pending_win_penalty: BTreeMap::from([(2, 4)]),
                    },
                },
            }),
            round_state: Some(RoundState {
                round_id: "east-1-dealer-0".to_string(),
                dealer_seat: 0,
                round_wind: "east".to_string(),
                current_actor: 1,
                phase: "playing".to_string(),
                players: (0..4)
                    .map(|seat| PlayerRoundState {
                        seat,
                        ..Default::default()
                    })
                    .collect(),
                last_action_context: LastActionContext {
                    kind: "draw".to_string(),
                    seat: 1,
                    tile_id: Some("b1#0".to_string()),
                    ..Default::default()
                },
                ..Default::default()
            }),
            pending_timeout: None,
            continue_action: None,
        };

        state.to_room_value().expect("room value")
    }
}
