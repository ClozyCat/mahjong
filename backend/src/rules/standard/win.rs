use serde_json::{Value, json};

use crate::core::engine::EngineOutput;
use crate::core::engine::planner::plan_settlement_to_match;
use crate::core::event::GameEvent;
use crate::core::state::settlement::{SettlementWinningDetailEntry, zero_score_map};
use crate::core::state::{
    PendingAction, RoomState, RoundSettlement, SettlementFanBreakdownEntry,
    SettlementKongScoreDetailEntry, SettlementScoreDelta,
};
use crate::room_scoring::RoomScoringCache;
use crate::rules::scoring::{
    Decomposition as ScoringDecomposition, EvaluationInput as ScoringEvaluationInput,
    KongEntry as ScoringKongEntry, TimingFeatures as ScoringTimingFeatures,
    decompose_winning_hand_with_melds as scoring_decompose_winning_hand_with_melds,
    evaluate_fans as scoring_evaluate_fans, extract_hand_features as scoring_extract_hand_features,
};

use super::runtime::round_event_message;

#[cfg(test)]
use super::runtime::{current_actor, project_room_state};
#[cfg(test)]
use crate::core::engine::reducer::update_room_state;

const MAX_SEATS: usize = 4;
const WIND_ORDER: [&str; 4] = ["east", "south", "west", "north"];
const MULTI_HU_WIN_LABEL: &str = "一炮多响";
pub(crate) const MINIMUM_HU_FAN: i64 = 8;
pub(crate) const BOT_MINIMUM_HU_FAN: i64 = MINIMUM_HU_FAN;

struct PreparedWinEvaluation {
    concealed_tile_keys: Vec<String>,
    meld_tile_key_groups: Vec<Vec<String>>,
    open_meld_tile_key_groups: Vec<Vec<String>>,
    meld_open_flags: Vec<bool>,
    decompositions: Vec<ScoringDecomposition>,
    kong_entries: Vec<ScoringKongEntry>,
}

struct EvaluatedWinResult {
    fan_result: crate::rules::scoring::FanResult,
}

fn winning_detail_entry(
    winner_seat: usize,
    display_win_label: Option<String>,
    flower_count: usize,
    fan_result: &crate::rules::scoring::FanResult,
) -> SettlementWinningDetailEntry {
    SettlementWinningDetailEntry {
        winner_seat,
        display_win_label,
        fan_total: fan_result.fan_total,
        fan_keys: fan_result.fan_keys.clone(),
        fan_breakdown: fan_result
            .fan_breakdown
            .iter()
            .map(|entry| SettlementFanBreakdownEntry {
                fan_key: entry.fan_key.clone(),
                fan_value: entry.fan_value,
            })
            .collect(),
        flower_count,
    }
}

pub(crate) fn compute_multi_hu_settlement_for_state(
    state: &RoomState,
    winner_seats: &[usize],
) -> Result<RoundSettlement, String> {
    let Some((&primary_winner_seat, _)) = winner_seats.split_first() else {
        return Err("invalid_action".to_string());
    };
    if winner_seats.len() == 1 {
        let settlement = compute_hu_settlement_for_state(state, primary_winner_seat, "discard")?;
        if !settlement_meets_minimum_hu_fan(&settlement) {
            return Err("invalid_action".to_string());
        }
        return Ok(settlement);
    }

    let settlements = winner_seats
        .iter()
        .copied()
        .map(|winner_seat| compute_hu_settlement_for_state(state, winner_seat, "discard"))
        .collect::<Result<Vec<_>, _>>()?;
    if settlements
        .iter()
        .any(|settlement| !settlement_meets_minimum_hu_fan(settlement))
    {
        return Err("invalid_action".to_string());
    }
    let seat_count = state
        .round_state
        .as_ref()
        .map(|round| round.players.len())
        .unwrap_or(MAX_SEATS);
    let mut fan_delta_by_seat = zero_score_map(seat_count);
    for settlement in &settlements {
        for seat in 0..seat_count {
            *fan_delta_by_seat.entry(seat).or_default() += settlement
                .score_delta
                .fan_delta_by_seat
                .get(&seat)
                .copied()
                .unwrap_or(0);
        }
    }

    let primary_settlement = settlements
        .first()
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;
    let kong_delta_by_seat = primary_settlement.score_delta.kong_delta_by_seat.clone();
    let total_delta_by_seat = (0..seat_count)
        .map(|seat| {
            (
                seat,
                fan_delta_by_seat.get(&seat).copied().unwrap_or(0)
                    + kong_delta_by_seat.get(&seat).copied().unwrap_or(0),
            )
        })
        .collect();

    let mut winning_details = primary_settlement.winning_details.clone();
    for settlement in settlements.iter().skip(1) {
        winning_details.extend(settlement.winning_details.clone());
    }

    let mut aggregated = primary_settlement;
    aggregated.display_win_label = Some(MULTI_HU_WIN_LABEL.to_string());
    aggregated.winning_details = winning_details;
    aggregated.score_delta.fan_delta_by_seat = fan_delta_by_seat;
    aggregated.score_delta.kong_delta_by_seat = kong_delta_by_seat;
    aggregated.score_delta.total_delta_by_seat = total_delta_by_seat;

    // Preserve deterministic primary-winner fields for legacy consumers.
    aggregated.winner_seat = Some(primary_winner_seat);

    Ok(aggregated)
}

#[cfg(test)]
pub fn compute_hu_settlement(
    room: &Value,
    winner_seat: usize,
    hu_context: &str,
) -> Result<RoundSettlement, String> {
    let state = project_room_state(room)?;
    if state.phase != "playing" {
        return Err("round_not_ready".to_string());
    }
    let round = state
        .round_state
        .as_ref()
        .ok_or_else(|| "round_not_ready".to_string())?;

    let discarder_seat = if hu_context == "self_draw" {
        if round.current_actor != winner_seat {
            return Err("invalid_action".to_string());
        }
        None
    } else {
        match round.pending_action.as_ref() {
            Some(PendingAction::ClaimWindow(claim)) => {
                if !claim
                    .claim_window
                    .get(winner_seat)
                    .is_some_and(|claims| claims.iter().any(|claim_type| claim_type == "hu"))
                {
                    return Err("invalid_action".to_string());
                }
                Some(claim.discarder_seat)
            }
            Some(PendingAction::RobKongWindow(rob)) => {
                if !rob.offered_hu_seats.contains(&winner_seat) {
                    return Err("invalid_action".to_string());
                }
                Some(rob.actor_seat)
            }
            _ => return Err("invalid_action".to_string()),
        }
    };

    let incoming_tile = if hu_context == "self_draw" {
        None
    } else {
        round
            .last_discard
            .as_ref()
            .map(|tile| tile.tile_key.as_str())
    };

    let evaluated = fan_result_for_win(room, winner_seat, incoming_tile, discarder_seat)?;
    let fan_result = &evaluated.fan_result;
    let flower_count = round
        .players
        .get(winner_seat)
        .map(|player| player.flowers.len())
        .unwrap_or(0);
    let display_win_label = None;

    Ok(RoundSettlement {
        provisional: true,
        win_type: hu_context.to_string(),
        winner_seat: Some(winner_seat),
        discarder_seat,
        display_win_label: display_win_label.clone(),
        fan_total: fan_result.fan_total,
        fan_keys: fan_result.fan_keys.clone(),
        fan_breakdown: fan_result
            .fan_breakdown
            .iter()
            .map(|entry| SettlementFanBreakdownEntry {
                fan_key: entry.fan_key.clone(),
                fan_value: entry.fan_value,
            })
            .collect(),
        winning_details: vec![winning_detail_entry(
            winner_seat,
            display_win_label,
            flower_count,
            fan_result,
        )],
        score_delta: SettlementScoreDelta {
            provisional: fan_result.score_delta.provisional,
            basic_points: fan_result.score_delta.basic_points,
            base_points: fan_result.score_delta.base_points,
            fan_total: fan_result.score_delta.fan_total,
            minimum_qualifying_fan_total: fan_result.score_delta.minimum_qualifying_fan_total,
            fan_delta_by_seat: fan_result
                .score_delta
                .fan_delta_by_seat
                .iter()
                .enumerate()
                .map(|(seat, delta)| (seat, *delta))
                .collect(),
            kong_delta_by_seat: fan_result
                .score_delta
                .kong_delta_by_seat
                .iter()
                .enumerate()
                .map(|(seat, delta)| (seat, *delta))
                .collect(),
            total_delta_by_seat: fan_result
                .score_delta
                .total_delta_by_seat
                .iter()
                .enumerate()
                .map(|(seat, delta)| (seat, *delta))
                .collect(),
        },
        flower_count,
        draw_type: None,
        kong_score_detail: fan_result
            .kong_score_detail
            .iter()
            .map(|entry| SettlementKongScoreDetailEntry {
                kong_type: entry.kong_type.clone(),
                actor_seat: entry.actor_seat,
                payer_seats: entry.payer_seats.clone(),
                delta_by_seat: entry
                    .delta_by_seat
                    .iter()
                    .enumerate()
                    .map(|(seat, delta)| (seat, *delta))
                    .collect(),
            })
            .collect(),
    })
}

#[cfg(test)]
#[allow(dead_code)]
pub fn apply_hu_settlement(
    room: &mut Value,
    winner_seat: usize,
    hu_context: &str,
    settlement: RoundSettlement,
) -> Result<Vec<Value>, String> {
    apply_hu_settlement_output(room, winner_seat, hu_context, settlement)
        .map(|output| output.emitted_messages)
}

#[cfg(test)]
pub fn apply_hu_settlement_output(
    room: &mut Value,
    winner_seat: usize,
    hu_context: &str,
    settlement: RoundSettlement,
) -> Result<EngineOutput, String> {
    let settlement_value = settlement.to_value();
    let round_id = room
        .get("round_state")
        .and_then(|round| round.get("round_id"))
        .cloned()
        .unwrap_or(Value::Null);
    let winning_tile_id = room
        .get("round_state")
        .and_then(|round| round.get("last_action_context"))
        .and_then(|context| context.get("tile_id"))
        .cloned()
        .unwrap_or(Value::Null);
    let discarded_tile = room
        .get("round_state")
        .and_then(|round| round.get("last_discard"))
        .cloned()
        .unwrap_or(Value::Null);
    let settlement_for_write = settlement.clone();
    let settlement_match_plan = project_room_state(room)
        .ok()
        .and_then(|state| plan_settlement_to_match(&state, &settlement_for_write));
    update_room_state(room, |state| {
        state.phase = "settlement".to_string();
        state.pending_timeout = None;
        if let Some(round) = state.round_state.as_mut() {
            round.phase = "settlement".to_string();
            round.pending_action = None;
            round.settlement = Some(settlement_for_write.clone());
            round.current_actor = winner_seat;
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
    })?;

    let winning_seats = settlement.winning_seats();
    let first_events = if hu_context == "self_draw" {
        vec![round_event_message(
            "self_hu_declared",
            json!({
                "type": "self_hu_declared",
                "seat": winner_seat,
                "tile_id": winning_tile_id,
            }),
        )]
    } else {
        winning_seats
            .iter()
            .map(|winning_seat| {
                round_event_message(
                    "claim_made",
                    json!({
                        "type": "claim_made",
                        "seat": winning_seat,
                        "from": settlement.discarder_seat,
                        "claim_type": "hu",
                        "tile_id": discarded_tile.get("tile_id").cloned().unwrap_or(Value::Null),
                    }),
                )
            })
            .collect()
    };
    let settlement_message = round_event_message(
        "settlement_ready",
        json!({
            "type": "settlement_ready",
            "round_id": round_id,
            "settlement": settlement_value,
        }),
    );
    let mut events = winning_seats
        .iter()
        .map(|winning_seat| GameEvent::HuDeclared {
            winner: *winning_seat,
            source: if hu_context == "self_draw" {
                "self_draw".to_string()
            } else {
                "discard".to_string()
            },
        })
        .collect::<Vec<_>>();
    events.push(GameEvent::SettlementPrepared {
        settlement: settlement.clone(),
    });
    let mut emitted_messages = first_events;
    emitted_messages.push(settlement_message);
    Ok(EngineOutput::new(events, emitted_messages))
}

pub fn hu_action_hint_in_room_state(room: &RoomState, seat_index: usize) -> Option<&'static str> {
    if room.phase != "playing" {
        return None;
    }
    let round = room.round_state.as_ref()?;
    let Some(pending_action) = round.pending_action.as_ref() else {
        return (round.current_actor == seat_index).then_some("self_draw");
    };
    match pending_action {
        PendingAction::ClaimWindow(claim)
            if claim_window_action_offers_claim(claim, seat_index, "hu") =>
        {
            Some("discard")
        }
        PendingAction::RobKongWindow(rob) if rob_kong_action_offers_seat(rob, seat_index) => {
            Some("discard")
        }
        PendingAction::ClaimWindow(_) | PendingAction::RobKongWindow(_) => None,
    }
}

pub fn apply_hu_action_output_in_room_state(
    room: &mut RoomState,
    seat_index: usize,
) -> Result<EngineOutput, String> {
    let Some(hu_context) = hu_action_hint_in_room_state(room, seat_index) else {
        return Err("invalid_action".to_string());
    };
    let settlement = compute_hu_settlement_for_state(room, seat_index, hu_context)?;
    if !settlement_meets_minimum_hu_fan(&settlement) {
        return Err("invalid_action".to_string());
    }
    apply_hu_settlement_output_in_room_state(room, seat_index, hu_context, settlement)
}

pub(crate) fn hu_meets_bot_minimum_fan_for_state(
    state: &RoomState,
    winner_seat: usize,
    hu_context: &str,
) -> bool {
    hu_meets_minimum_fan_for_state(state, winner_seat, hu_context)
}

pub(crate) fn hu_meets_minimum_fan_for_state(
    state: &RoomState,
    winner_seat: usize,
    hu_context: &str,
) -> bool {
    compute_hu_settlement_for_state(state, winner_seat, hu_context)
        .is_ok_and(|settlement| settlement_meets_minimum_hu_fan(&settlement))
}

pub(crate) fn settlement_meets_minimum_hu_fan(settlement: &RoundSettlement) -> bool {
    settlement.score_delta.minimum_qualifying_fan_total >= MINIMUM_HU_FAN
}

pub(crate) fn compute_hu_settlement_for_state(
    state: &RoomState,
    winner_seat: usize,
    hu_context: &str,
) -> Result<RoundSettlement, String> {
    if state.phase != "playing" {
        return Err("round_not_ready".to_string());
    }
    let round = state
        .round_state
        .as_ref()
        .ok_or_else(|| "round_not_ready".to_string())?;

    let discarder_seat = if hu_context == "self_draw" {
        if round.current_actor != winner_seat {
            return Err("invalid_action".to_string());
        }
        None
    } else {
        match round.pending_action.as_ref() {
            Some(PendingAction::ClaimWindow(claim)) => {
                if !claim
                    .claim_window
                    .get(winner_seat)
                    .is_some_and(|claims| claims.iter().any(|claim_type| claim_type == "hu"))
                {
                    return Err("invalid_action".to_string());
                }
                Some(claim.discarder_seat)
            }
            Some(PendingAction::RobKongWindow(rob)) => {
                if !rob.offered_hu_seats.contains(&winner_seat) {
                    return Err("invalid_action".to_string());
                }
                Some(rob.actor_seat)
            }
            _ => return Err("invalid_action".to_string()),
        }
    };

    let incoming_tile = if hu_context == "self_draw" {
        None
    } else {
        round
            .last_discard
            .as_ref()
            .map(|tile| tile.tile_key.as_str())
    };

    let cache = RoomScoringCache::from_state(state);
    let evaluated =
        fan_result_for_win_with_state(state, &cache, winner_seat, incoming_tile, discarder_seat)?;
    let fan_result = &evaluated.fan_result;
    let flower_count = round
        .players
        .get(winner_seat)
        .map(|player| player.flowers.len())
        .unwrap_or(0);
    let display_win_label = None;

    Ok(RoundSettlement {
        provisional: true,
        win_type: hu_context.to_string(),
        winner_seat: Some(winner_seat),
        discarder_seat,
        display_win_label: display_win_label.clone(),
        fan_total: fan_result.fan_total,
        fan_keys: fan_result.fan_keys.clone(),
        fan_breakdown: fan_result
            .fan_breakdown
            .iter()
            .map(|entry| SettlementFanBreakdownEntry {
                fan_key: entry.fan_key.clone(),
                fan_value: entry.fan_value,
            })
            .collect(),
        winning_details: vec![winning_detail_entry(
            winner_seat,
            display_win_label,
            flower_count,
            fan_result,
        )],
        score_delta: SettlementScoreDelta {
            provisional: fan_result.score_delta.provisional,
            basic_points: fan_result.score_delta.basic_points,
            base_points: fan_result.score_delta.base_points,
            fan_total: fan_result.score_delta.fan_total,
            minimum_qualifying_fan_total: fan_result.score_delta.minimum_qualifying_fan_total,
            fan_delta_by_seat: fan_result
                .score_delta
                .fan_delta_by_seat
                .iter()
                .enumerate()
                .map(|(seat, delta)| (seat, *delta))
                .collect(),
            kong_delta_by_seat: fan_result
                .score_delta
                .kong_delta_by_seat
                .iter()
                .enumerate()
                .map(|(seat, delta)| (seat, *delta))
                .collect(),
            total_delta_by_seat: fan_result
                .score_delta
                .total_delta_by_seat
                .iter()
                .enumerate()
                .map(|(seat, delta)| (seat, *delta))
                .collect(),
        },
        flower_count,
        draw_type: None,
        kong_score_detail: fan_result
            .kong_score_detail
            .iter()
            .map(|entry| SettlementKongScoreDetailEntry {
                kong_type: entry.kong_type.clone(),
                actor_seat: entry.actor_seat,
                payer_seats: entry.payer_seats.clone(),
                delta_by_seat: entry
                    .delta_by_seat
                    .iter()
                    .enumerate()
                    .map(|(seat, delta)| (seat, *delta))
                    .collect(),
            })
            .collect(),
    })
}

pub fn apply_hu_settlement_output_in_room_state(
    room: &mut RoomState,
    winner_seat: usize,
    hu_context: &str,
    settlement: RoundSettlement,
) -> Result<EngineOutput, String> {
    let round = room
        .round_state
        .as_ref()
        .ok_or_else(|| "round_not_ready".to_string())?;
    let round_id = round.round_id.clone();
    let winning_tile_id = round.last_action_context.tile_id.clone();
    let discarded_tile = round.last_discard.clone();

    let settlement_for_write = settlement.clone();
    let settlement_match_plan = plan_settlement_to_match(room, &settlement_for_write);
    room.phase = "settlement".to_string();
    room.pending_timeout = None;
    if let Some(round) = room.round_state.as_mut() {
        round.phase = "settlement".to_string();
        round.pending_action = None;
        round.settlement = Some(settlement_for_write.clone());
        round.current_actor = winner_seat;
        round.version += 1;
    }
    if let Some(plan) = settlement_match_plan {
        if let Some(match_state) = room.match_state.as_mut() {
            match_state.apply_completed_round(plan.round_id, plan.cumulative_scores, &settlement);
        }
    }

    let winning_seats = settlement.winning_seats();
    let first_events = if hu_context == "self_draw" {
        vec![round_event_message(
            "self_hu_declared",
            json!({
                "type": "self_hu_declared",
                "seat": winner_seat,
                "tile_id": winning_tile_id,
            }),
        )]
    } else {
        winning_seats
            .iter()
            .map(|winning_seat| {
                round_event_message(
                    "claim_made",
                    json!({
                        "type": "claim_made",
                        "seat": winning_seat,
                        "from": settlement.discarder_seat,
                        "claim_type": "hu",
                        "tile_id": discarded_tile
                            .as_ref()
                            .map(|tile| Value::String(tile.tile_id.clone()))
                            .unwrap_or(Value::Null),
                    }),
                )
            })
            .collect()
    };
    let settlement_message = round_event_message(
        "settlement_ready",
        json!({
            "type": "settlement_ready",
            "round_id": round_id,
            "settlement": settlement.to_value(),
        }),
    );
    let mut events = winning_seats
        .iter()
        .map(|winning_seat| GameEvent::HuDeclared {
            winner: *winning_seat,
            source: if hu_context == "self_draw" {
                "self_draw".to_string()
            } else {
                "discard".to_string()
            },
        })
        .collect::<Vec<_>>();
    events.push(GameEvent::SettlementPrepared {
        settlement: settlement.clone(),
    });
    let mut emitted_messages = first_events;
    emitted_messages.push(settlement_message);
    Ok(EngineOutput::new(events, emitted_messages))
}

#[cfg(test)]
#[allow(dead_code)]
pub fn hu_action_hint(room: &Value, seat_index: usize) -> Option<&'static str> {
    let state = project_room_state(room).ok()?;
    if state.phase != "playing" {
        return None;
    }
    let round = state.round_state.as_ref()?;
    let Some(pending_action) = round.pending_action.as_ref() else {
        return (current_actor(room) == Some(seat_index)).then_some("self_draw");
    };
    match pending_action {
        PendingAction::ClaimWindow(claim)
            if claim_window_action_offers_claim(claim, seat_index, "hu") =>
        {
            Some("discard")
        }
        PendingAction::RobKongWindow(rob) if rob_kong_action_offers_seat(rob, seat_index) => {
            Some("discard")
        }
        PendingAction::ClaimWindow(_) | PendingAction::RobKongWindow(_) => None,
    }
}

fn claim_window_action_offers_claim(
    pending_action: &crate::core::state::ClaimWindowAction,
    seat_index: usize,
    claim_type: &str,
) -> bool {
    pending_action
        .claim_window
        .get(seat_index)
        .is_some_and(|claims| claims.iter().any(|claim| claim == claim_type))
}

fn rob_kong_action_offers_seat(
    pending_action: &crate::core::state::RobKongWindowAction,
    seat_index: usize,
) -> bool {
    pending_action.offered_hu_seats.contains(&seat_index)
}

#[cfg(test)]
#[allow(dead_code)]
pub fn claim_window_offers_claim(
    pending_action: &Value,
    seat_index: usize,
    claim_type: &str,
) -> bool {
    json_array_contains_str(
        pending_action
            .get("claim_window")
            .and_then(Value::as_array)
            .and_then(|claim_window| claim_window.get(seat_index))
            .and_then(Value::as_array),
        claim_type,
    )
}

#[cfg(test)]
pub fn can_declare_hu_with_cache(
    room: &Value,
    cache: &RoomScoringCache,
    seat_index: usize,
    incoming_tile: Option<&str>,
    discarder_seat: Option<usize>,
) -> bool {
    let Ok(state) = project_room_state(room) else {
        return false;
    };
    can_declare_hu_with_cache_for_state(&state, cache, seat_index, incoming_tile, discarder_seat)
}

pub fn can_declare_hu_with_cache_for_state(
    state: &RoomState,
    cache: &RoomScoringCache,
    seat_index: usize,
    incoming_tile: Option<&str>,
    discarder_seat: Option<usize>,
) -> bool {
    fan_result_for_win_with_state(state, cache, seat_index, incoming_tile, discarder_seat)
        .is_ok_and(|evaluated| evaluated.fan_result.minimum_qualifying_fan_total >= MINIMUM_HU_FAN)
}

#[cfg(test)]
#[allow(dead_code)]
fn json_array_contains_str(values: Option<&Vec<Value>>, needle: &str) -> bool {
    values.is_some_and(|items| items.iter().any(|value| value.as_str() == Some(needle)))
}

#[cfg(test)]
fn fan_result_for_win(
    room: &Value,
    winner_seat: usize,
    incoming_tile: Option<&str>,
    discarder_seat: Option<usize>,
) -> Result<EvaluatedWinResult, String> {
    let cache = RoomScoringCache::from_room(room);
    fan_result_for_win_with_cache(room, &cache, winner_seat, incoming_tile, discarder_seat)
}

#[cfg(test)]
fn fan_result_for_win_with_cache(
    room: &Value,
    cache: &RoomScoringCache,
    winner_seat: usize,
    incoming_tile: Option<&str>,
    discarder_seat: Option<usize>,
) -> Result<EvaluatedWinResult, String> {
    let state = project_room_state(room)?;
    fan_result_for_win_with_state(&state, cache, winner_seat, incoming_tile, discarder_seat)
}

fn fan_result_for_win_with_state(
    state: &RoomState,
    cache: &RoomScoringCache,
    winner_seat: usize,
    incoming_tile: Option<&str>,
    discarder_seat: Option<usize>,
) -> Result<EvaluatedWinResult, String> {
    let PreparedWinEvaluation {
        concealed_tile_keys,
        meld_tile_key_groups,
        open_meld_tile_key_groups,
        meld_open_flags,
        decompositions,
        kong_entries,
    } = prepare_win_evaluation(cache, winner_seat, incoming_tile)?;

    let win_type = if incoming_tile.is_none() {
        "self_draw"
    } else {
        "discard"
    }
    .to_string();
    let features = scoring_extract_hand_features(
        &concealed_tile_keys,
        &meld_tile_key_groups,
        Some(&meld_open_flags),
        incoming_tile,
        Some(&seat_wind_key(winner_seat, cache.dealer_seat)),
        cache.round_wind.as_deref(),
        Some(&decompositions),
    );

    let player_tile_keys =
        player_tile_keys_from_parts(&concealed_tile_keys, &meld_tile_key_groups, incoming_tile);
    let winning_tile = winning_tile_for_win_state(state, winner_seat, incoming_tile);
    let visible_tile_keys = visible_tile_keys_for_win_state(state, cache, incoming_tile);

    let evaluation = ScoringEvaluationInput {
        win_type: win_type.clone(),
        winner_seat: Some(winner_seat),
        discarder_seat,
        ready_hand_declared: state
            .round_state
            .as_ref()
            .and_then(|round| round.players.get(winner_seat))
            .is_some_and(|player| player.is_ready_hand),
        flower_count: cache
            .player(winner_seat)
            .map(|player| player.flower_count)
            .unwrap_or(0),
        seat_count: cache.seat_count,
        features,
        timing: timing_features_for_win_state(state, incoming_tile.is_none()),
        kong_entries,
        tile_keys: player_tile_keys,
        visible_tile_keys,
        concealed_tile_keys,
        meld_tile_key_groups,
        open_meld_tile_key_groups,
        incoming_tile: incoming_tile.map(ToString::to_string),
        winning_tile,
        decompositions,
    };
    let result = scoring_evaluate_fans(evaluation);
    Ok(EvaluatedWinResult { fan_result: result })
}

fn visible_tile_keys_for_win_state(
    state: &RoomState,
    cache: &RoomScoringCache,
    incoming_tile: Option<&str>,
) -> Vec<String> {
    let mut visible_tile_keys = cache.visible_tile_keys.clone();
    let Some(incoming_tile) = incoming_tile else {
        return visible_tile_keys;
    };
    let Some(round) = state.round_state.as_ref() else {
        return visible_tile_keys;
    };
    if round
        .last_discard
        .as_ref()
        .is_some_and(|tile| tile.tile_key == incoming_tile)
    {
        if let Some(index) = visible_tile_keys
            .iter()
            .position(|tile_key| tile_key == incoming_tile)
        {
            visible_tile_keys.remove(index);
        }
    }
    visible_tile_keys
}

fn prepare_win_evaluation(
    cache: &RoomScoringCache,
    winner_seat: usize,
    incoming_tile: Option<&str>,
) -> Result<PreparedWinEvaluation, String> {
    let player = cache
        .player(winner_seat)
        .ok_or_else(|| "invalid_action".to_string())?;
    let concealed_tile_keys = player.concealed_tile_keys.clone();
    let meld_tile_key_groups = player.meld_tile_key_groups.clone();

    let mut effective_concealed_tile_keys =
        Vec::with_capacity(concealed_tile_keys.len() + usize::from(incoming_tile.is_some()));
    effective_concealed_tile_keys.extend(concealed_tile_keys.iter().cloned());
    if let Some(tile_key) = incoming_tile {
        effective_concealed_tile_keys.push(tile_key.to_string());
    }

    let decompositions = scoring_decompose_winning_hand_with_melds(
        &effective_concealed_tile_keys,
        &meld_tile_key_groups,
    );
    if decompositions.is_empty() {
        return Err("invalid_action".to_string());
    }

    let kong_entries = cache.kong_entries.clone();
    let (open_meld_tile_key_groups, meld_open_flags) =
        classify_meld_groups(winner_seat, &meld_tile_key_groups, &kong_entries);

    Ok(PreparedWinEvaluation {
        concealed_tile_keys,
        meld_tile_key_groups,
        open_meld_tile_key_groups,
        meld_open_flags,
        decompositions,
        kong_entries,
    })
}

fn winning_tile_for_win_state(
    state: &RoomState,
    winner_seat: usize,
    incoming_tile: Option<&str>,
) -> Option<String> {
    if let Some(tile_key) = incoming_tile {
        return Some(tile_key.to_string());
    }

    let round = state.round_state.as_ref()?;
    let drawn_tile_id = round.last_action_context.tile_id.as_deref()?;
    round
        .players
        .get(winner_seat)?
        .concealed_tiles
        .iter()
        .find(|tile| tile.tile_id == drawn_tile_id)
        .map(|tile| tile.tile_key.clone())
}

fn seat_wind_key(seat_index: usize, dealer_seat: usize) -> String {
    WIND_ORDER[(seat_index + MAX_SEATS - dealer_seat) % MAX_SEATS].to_string()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        apply_hu_action_output_in_room_state, can_declare_hu_with_cache_for_state,
        compute_hu_settlement_for_state, compute_multi_hu_settlement_for_state,
    };
    use crate::core::state::{
        ClaimWindowAction, LastActionContext, MatchState, PendingAction, PlayerRoundState,
        RoomState, RoundState, SeatState,
    };
    use crate::core::tile::Tile;
    use crate::room_scoring::RoomScoringCache;
    use crate::rules::scoring::decompose_winning_hand;

    #[test]
    fn can_declare_hu_for_standard_seven_pairs_hand() {
        let tile_keys = [
            "w1", "w1", "w2", "w2", "w3", "w3", "t4", "t4", "t5", "t5", "b6", "b6", "red", "red",
        ];
        let decompositions = decompose_winning_hand(
            &tile_keys
                .iter()
                .map(|tile_key| (*tile_key).to_string())
                .collect::<Vec<_>>(),
        );
        assert!(
            decompositions
                .iter()
                .any(|decomposition| decomposition.kind == "seven_pairs")
        );

        let state = test_room_state_with_concealed_tiles(&tile_keys);
        let cache = RoomScoringCache::from_state(&state);

        assert!(can_declare_hu_with_cache_for_state(
            &state, &cache, 0, None, None
        ));
    }

    #[test]
    fn can_declare_hu_for_seven_pairs_hand_with_four_of_a_kind_counted_as_two_pairs() {
        let tile_keys = [
            "w1", "w1", "w1", "w1", "w2", "w2", "w3", "w3", "t4", "t4", "t5", "t5", "red", "red",
        ];
        let decompositions = decompose_winning_hand(
            &tile_keys
                .iter()
                .map(|tile_key| (*tile_key).to_string())
                .collect::<Vec<_>>(),
        );
        assert!(
            decompositions
                .iter()
                .any(|decomposition| decomposition.kind == "seven_pairs")
        );

        let state = test_room_state_with_concealed_tiles(&tile_keys);
        let cache = RoomScoringCache::from_state(&state);

        assert!(can_declare_hu_with_cache_for_state(
            &state, &cache, 0, None, None
        ));
    }

    #[test]
    fn low_fan_self_draw_keeps_standard_label_and_still_settles() {
        let tile_keys = [
            "w1", "w2", "w3", "t4", "t5", "t6", "b3", "b4", "b5", "w6", "w7", "w8", "red", "red",
        ];
        let state = test_room_state_with_concealed_tiles(&tile_keys);

        let settlement =
            compute_hu_settlement_for_state(&state, 0, "self_draw").expect("settlement");

        assert!(settlement.fan_total < 8);
        assert_eq!(settlement.display_win_label, None);
    }

    #[test]
    fn self_draw_on_fourth_visible_copy_counts_last_tile() {
        let tile_keys = [
            "w2", "w3", "w4", "w5", "w6", "t1", "t2", "t3", "b1", "b2", "b3", "red", "red", "w1",
        ];
        let mut state = test_room_state_with_concealed_tiles(&tile_keys);
        let opponent = state
            .round_state
            .as_mut()
            .and_then(|round| round.players.get_mut(1))
            .expect("opponent should exist");
        opponent.discards = (0..3)
            .map(|index| Tile {
                tile_id: format!("w1#discard-{index}"),
                tile_key: "w1".to_string(),
                ..Default::default()
            })
            .collect();

        let settlement =
            compute_hu_settlement_for_state(&state, 0, "self_draw").expect("settlement");

        assert!(
            settlement.fan_keys.iter().any(|fan| fan == "last_tile"),
            "fan keys should include last_tile, got {:?}",
            settlement.fan_keys
        );
    }

    #[test]
    fn discard_win_does_not_count_claimed_discard_as_prior_visible_last_tile() {
        let tile_keys = [
            "w2", "w3", "w4", "w5", "w6", "t1", "t2", "t3", "b1", "b2", "b3", "red", "red",
        ];
        let mut state = test_room_state_with_concealed_tiles(&tile_keys);
        let last_discard = Tile {
            tile_id: "w1#discard-win".to_string(),
            tile_key: "w1".to_string(),
            ..Default::default()
        };
        let round = state.round_state.as_mut().expect("round should exist");
        round.current_actor = 1;
        round.last_discard = Some(last_discard.clone());
        round.players[1].discards = vec![
            Tile {
                tile_id: "w1#discard-0".to_string(),
                tile_key: "w1".to_string(),
                ..Default::default()
            },
            Tile {
                tile_id: "w1#discard-1".to_string(),
                tile_key: "w1".to_string(),
                ..Default::default()
            },
            last_discard,
        ];
        round.pending_action = Some(PendingAction::ClaimWindow(ClaimWindowAction {
            discarder_seat: 1,
            claim_window: vec![vec!["hu".to_string()], vec![], vec![], vec![]],
            responded_seats: vec![],
            claim_responses: vec![],
        }));

        let settlement = compute_hu_settlement_for_state(&state, 0, "discard").expect("settlement");

        assert!(
            !settlement.fan_keys.iter().any(|fan| fan == "last_tile"),
            "claimed discard should not count as a prior visible copy, got {:?}",
            settlement.fan_keys
        );
    }

    #[test]
    fn bot_low_fan_self_draw_is_rejected_at_hu_execution() {
        let tile_keys = [
            "w1", "w2", "w3", "t4", "t5", "t6", "b3", "b4", "b5", "w6", "w7", "w8", "red", "red",
        ];
        let mut state = test_room_state_with_concealed_tiles(&tile_keys);
        state.seats.get_mut(0).expect("seat should exist").is_bot = true;

        let result = apply_hu_action_output_in_room_state(&mut state, 0);

        assert_eq!(
            result.expect_err("low fan bot hu should fail"),
            "invalid_action"
        );
    }

    #[test]
    fn human_low_fan_self_draw_is_rejected_at_hu_execution() {
        let tile_keys = [
            "w1", "w2", "w3", "t4", "t5", "t6", "b3", "b4", "b5", "w6", "w7", "w8", "red", "red",
        ];
        let mut state = test_room_state_with_concealed_tiles(&tile_keys);

        let result = apply_hu_action_output_in_room_state(&mut state, 0);

        assert_eq!(
            result.expect_err("low fan human hu should fail"),
            "invalid_action"
        );
    }

    #[test]
    fn bot_minimum_hu_fan_excludes_flower_tiles() {
        let tile_keys = [
            "w1", "w2", "w3", "t4", "t5", "t6", "b3", "b4", "b5", "w6", "w7", "w8", "red", "red",
        ];
        let mut state = test_room_state_with_concealed_tiles(&tile_keys);
        state.seats.get_mut(0).expect("seat should exist").is_bot = true;
        let player = state
            .round_state
            .as_mut()
            .and_then(|round| round.players.get_mut(0))
            .expect("player should exist");
        player.flowers = (0..7)
            .map(|index| Tile {
                tile_id: format!("flower#{index}"),
                tile_key: format!("f{}", index + 1),
                kind: "flower".to_string(),
                ..Default::default()
            })
            .collect();

        let settlement =
            compute_hu_settlement_for_state(&state, 0, "self_draw").expect("settlement");
        assert!(settlement.fan_total >= 8);
        assert!(settlement.score_delta.minimum_qualifying_fan_total < 8);

        let result = apply_hu_action_output_in_room_state(&mut state, 0);

        assert_eq!(
            result.expect_err("flower-only minimum fan should fail"),
            "invalid_action"
        );
    }

    #[test]
    fn can_declare_hu_requires_eight_non_flower_fan() {
        let tile_keys = [
            "w1", "w2", "w3", "t4", "t5", "t6", "b3", "b4", "b5", "w6", "w7", "w8", "red", "red",
        ];
        let mut state = test_room_state_with_concealed_tiles(&tile_keys);
        let player = state
            .round_state
            .as_mut()
            .and_then(|round| round.players.get_mut(0))
            .expect("player should exist");
        player.flowers = (0..7)
            .map(|index| Tile {
                tile_id: format!("flower#{index}"),
                tile_key: format!("f{}", index + 1),
                kind: "flower".to_string(),
                ..Default::default()
            })
            .collect();
        let cache = RoomScoringCache::from_state(&state);

        assert!(!can_declare_hu_with_cache_for_state(
            &state, &cache, 0, None, None
        ));
    }

    #[test]
    fn single_discard_hu_claim_requires_eight_non_flower_fan() {
        let mut state = test_room_state_with_concealed_tiles(&[]);
        let low_fan_waiting_tiles = [
            "w2", "w3", "t4", "t5", "t6", "b3", "b4", "b5", "w6", "w7", "w8", "red", "red",
        ];
        let round = state.round_state.as_mut().expect("round should exist");
        round.current_actor = 0;
        round.players[1].concealed_tiles = low_fan_waiting_tiles
            .iter()
            .enumerate()
            .map(|(index, tile_key)| Tile {
                tile_id: format!("{tile_key}#{index}"),
                tile_key: (*tile_key).to_string(),
                ..Default::default()
            })
            .collect();
        round.last_discard = Some(Tile {
            tile_id: "w1#discard".to_string(),
            tile_key: "w1".to_string(),
            ..Default::default()
        });
        round.pending_action = Some(PendingAction::ClaimWindow(ClaimWindowAction {
            discarder_seat: 0,
            claim_window: vec![vec![], vec!["hu".to_string()], vec![], vec![]],
            responded_seats: vec![],
            claim_responses: vec![],
        }));

        let settlement = compute_hu_settlement_for_state(&state, 1, "discard").expect("settlement");
        assert!(
            settlement.score_delta.minimum_qualifying_fan_total < 8,
            "test hand should be below eight non-flower fan, got {:?}",
            settlement.fan_breakdown
        );

        assert_eq!(
            compute_multi_hu_settlement_for_state(&state, &[1])
                .expect_err("single low-fan discard hu should fail"),
            "invalid_action"
        );
    }

    #[test]
    fn settlement_includes_ready_hand_win_for_ready_hand_winner() {
        let tile_keys = [
            "w1", "w2", "w3", "w4", "w5", "w6", "t1", "t2", "t3", "b1", "b2", "b3", "red", "red",
        ];
        let base_state = test_room_state_with_concealed_tiles(&tile_keys);
        let base_settlement =
            compute_hu_settlement_for_state(&base_state, 0, "self_draw").expect("base settlement");

        let mut ready_hand_room = base_state.to_room_value().expect("state should serialize");
        ready_hand_room["round_state"]["players"][0]["is_ready_hand"] = json!(true);
        let ready_hand_state =
            RoomState::from_room_value(&ready_hand_room).expect("room should parse");

        let ready_hand_settlement =
            compute_hu_settlement_for_state(&ready_hand_state, 0, "self_draw")
                .expect("ready-hand settlement");

        assert!(
            ready_hand_settlement
                .fan_keys
                .iter()
                .any(|fan| fan == "ready_hand_win")
        );
        assert_eq!(
            ready_hand_settlement.fan_total,
            base_settlement.fan_total + 2
        );
    }

    fn test_room_state_with_concealed_tiles(tile_keys: &[&str]) -> RoomState {
        RoomState {
            table_code: "ROOM7P".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
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
            }),
            round_state: Some(RoundState {
                round_id: "east-1-dealer-0".to_string(),
                dealer_seat: 0,
                round_wind: "east".to_string(),
                current_actor: 0,
                phase: "playing".to_string(),
                players: vec![
                    PlayerRoundState {
                        seat: 0,
                        is_ready_hand: false,
                        concealed_tiles: tile_keys
                            .iter()
                            .enumerate()
                            .map(|(index, tile_key)| Tile {
                                tile_id: format!("{tile_key}#{index}"),
                                tile_key: (*tile_key).to_string(),
                                ..Default::default()
                            })
                            .collect(),
                        ..Default::default()
                    },
                    PlayerRoundState {
                        seat: 1,
                        is_ready_hand: false,
                        ..Default::default()
                    },
                    PlayerRoundState {
                        seat: 2,
                        is_ready_hand: false,
                        ..Default::default()
                    },
                    PlayerRoundState {
                        seat: 3,
                        is_ready_hand: false,
                        ..Default::default()
                    },
                ],
                last_action_context: LastActionContext {
                    kind: "draw".to_string(),
                    seat: 0,
                    tile_id: tile_keys
                        .last()
                        .map(|tile_key| format!("{tile_key}#{}", tile_keys.len() - 1)),
                    ..Default::default()
                },
                ..Default::default()
            }),
            pending_timeout: None,
            continue_action: None,
        }
    }
}

fn player_tile_keys_from_parts(
    concealed_tile_keys: &[String],
    meld_tile_key_groups: &[Vec<String>],
    incoming_tile: Option<&str>,
) -> Vec<String> {
    let meld_tile_count = meld_tile_key_groups
        .iter()
        .map(|meld| {
            if meld.len() == 4 && meld.iter().all(|tile_key| tile_key == &meld[0]) {
                3
            } else {
                meld.len()
            }
        })
        .sum::<usize>();
    let mut tile_keys = Vec::with_capacity(
        concealed_tile_keys.len() + meld_tile_count + usize::from(incoming_tile.is_some()),
    );
    tile_keys.extend(concealed_tile_keys.iter().cloned());
    for meld in meld_tile_key_groups {
        if meld.len() == 4 && meld.iter().all(|tile_key| tile_key == &meld[0]) {
            tile_keys.extend(meld.iter().take(3).cloned());
        } else {
            tile_keys.extend(meld.iter().cloned());
        }
    }
    if let Some(tile_key) = incoming_tile {
        tile_keys.push(tile_key.to_string());
    }
    tile_keys
}

fn classify_meld_groups(
    seat_index: usize,
    meld_tile_key_groups: &[Vec<String>],
    kong_entries: &[ScoringKongEntry],
) -> (Vec<Vec<String>>, Vec<bool>) {
    let mut open_meld_tile_key_groups = Vec::new();
    let mut meld_open_flags = Vec::with_capacity(meld_tile_key_groups.len());
    for meld in meld_tile_key_groups {
        let is_open = meld_is_open_with_entries(seat_index, meld, kong_entries);
        meld_open_flags.push(is_open);
        if is_open {
            open_meld_tile_key_groups.push(meld.clone());
        }
    }
    (open_meld_tile_key_groups, meld_open_flags)
}

pub(crate) fn classify_meld_groups_for_projection(
    seat_index: usize,
    meld_tile_key_groups: &[Vec<String>],
    kong_entries: &[ScoringKongEntry],
) -> (Vec<Vec<String>>, Vec<bool>) {
    classify_meld_groups(seat_index, meld_tile_key_groups, kong_entries)
}

fn meld_is_open_with_entries(
    seat_index: usize,
    meld: &[String],
    kong_entries: &[ScoringKongEntry],
) -> bool {
    if meld.len() != 4 || !meld.iter().all(|tile_key| tile_key == &meld[0]) {
        return true;
    }

    let tile_key = meld[0].as_str();
    for entry in kong_entries.iter().rev() {
        if entry.actor_seat != seat_index {
            continue;
        }
        if entry
            .tile_key
            .as_deref()
            .is_some_and(|value| value != tile_key)
        {
            continue;
        }
        return entry.kong_type != "concealed_kong";
    }
    true
}

fn timing_features_for_win_state(state: &RoomState, self_draw: bool) -> ScoringTimingFeatures {
    let context = state
        .round_state
        .as_ref()
        .map(|round| round.last_action_context.clone())
        .unwrap_or_default();
    if self_draw {
        let is_replacement = context.from_kong_replacement;
        return ScoringTimingFeatures {
            gang_shang_hua: is_replacement,
            hai_di_lao_yue: !is_replacement && context.kind == "draw" && context.was_last_live_tile,
            he_di_lao_yu: false,
            robbing_the_kong: false,
        };
    }

    ScoringTimingFeatures {
        gang_shang_hua: false,
        hai_di_lao_yue: false,
        he_di_lao_yu: context.kind == "discard" && context.was_last_discard,
        robbing_the_kong: state
            .round_state
            .as_ref()
            .and_then(|round| round.pending_action.as_ref())
            .is_some_and(|pending| matches!(pending, PendingAction::RobKongWindow(_))),
    }
}
