use serde_json::{Value, json};

use crate::core::engine::EngineOutput;
use crate::core::engine::planner::plan_settlement_to_match;
use crate::core::event::GameEvent;
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
const LOW_FAN_WIN_LABEL: &str = "屁和";

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
    required_minimum_fan_total: i64,
}

fn low_fan_display_win_label(
    enforce_minimum_eight_fan: bool,
    fan_result: &crate::rules::scoring::FanResult,
) -> Option<String> {
    if !enforce_minimum_eight_fan && fan_result.minimum_qualifying_fan_total < 8 {
        return Some(LOW_FAN_WIN_LABEL.to_string());
    }

    None
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
    let enforce_minimum_eight_fan = round.rule_state.enforce_minimum_eight_fan;

    Ok(RoundSettlement {
        provisional: true,
        win_type: hu_context.to_string(),
        winner_seat: Some(winner_seat),
        discarder_seat,
        display_win_label: low_fan_display_win_label(enforce_minimum_eight_fan, fan_result),
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

    let first_event = if hu_context == "self_draw" {
        round_event_message(
            "self_hu_declared",
            json!({
                "type": "self_hu_declared",
                "seat": winner_seat,
                "tile_id": winning_tile_id,
            }),
        )
    } else {
        round_event_message(
            "claim_made",
            json!({
                "type": "claim_made",
                "seat": winner_seat,
                "claim_type": "hu",
                "tile_id": discarded_tile.get("tile_id").cloned().unwrap_or(Value::Null),
            }),
        )
    };
    let settlement_message = round_event_message(
        "settlement_ready",
        json!({
            "type": "settlement_ready",
            "round_id": round_id,
            "settlement": settlement_value,
        }),
    );
    Ok(EngineOutput::new(
        vec![
            GameEvent::HuDeclared {
                winner: winner_seat,
                source: if hu_context == "self_draw" {
                    "self_draw".to_string()
                } else {
                    "discard".to_string()
                },
            },
            GameEvent::SettlementPrepared {
                settlement: settlement.clone(),
            },
        ],
        vec![first_event, settlement_message],
    ))
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
        _ => None,
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
    apply_hu_settlement_output_in_room_state(room, seat_index, hu_context, settlement)
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
    let enforce_minimum_eight_fan = round.rule_state.enforce_minimum_eight_fan;

    Ok(RoundSettlement {
        provisional: true,
        win_type: hu_context.to_string(),
        winner_seat: Some(winner_seat),
        discarder_seat,
        display_win_label: low_fan_display_win_label(enforce_minimum_eight_fan, fan_result),
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

    let first_event = if hu_context == "self_draw" {
        round_event_message(
            "self_hu_declared",
            json!({
                "type": "self_hu_declared",
                "seat": winner_seat,
                "tile_id": winning_tile_id,
            }),
        )
    } else {
        round_event_message(
            "claim_made",
            json!({
                "type": "claim_made",
                "seat": winner_seat,
                "claim_type": "hu",
                "tile_id": discarded_tile
                    .as_ref()
                    .map(|tile| Value::String(tile.tile_id.clone()))
                    .unwrap_or(Value::Null),
            }),
        )
    };
    let settlement_message = round_event_message(
        "settlement_ready",
        json!({
            "type": "settlement_ready",
            "round_id": round_id,
            "settlement": settlement.to_value(),
        }),
    );
    Ok(EngineOutput::new(
        vec![
            GameEvent::HuDeclared {
                winner: winner_seat,
                source: if hu_context == "self_draw" {
                    "self_draw".to_string()
                } else {
                    "discard".to_string()
                },
            },
            GameEvent::SettlementPrepared {
                settlement: settlement.clone(),
            },
        ],
        vec![first_event, settlement_message],
    ))
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
        _ => None,
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
    if state
        .round_state
        .as_ref()
        .and_then(|round| round.pending_action.as_ref())
        .is_some_and(|pending| matches!(pending, PendingAction::OpeningFlowers(_)))
    {
        return false;
    }

    fan_result_for_win_with_state(state, cache, seat_index, incoming_tile, discarder_seat)
        .map(|evaluated| {
            evaluated.fan_result.minimum_qualifying_fan_total
                >= evaluated.required_minimum_fan_total
        })
        .unwrap_or(false)
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

    let enforce_minimum_eight_fan = state
        .round_state
        .as_ref()
        .map(|round| round.rule_state.enforce_minimum_eight_fan)
        .unwrap_or(state.enforce_minimum_eight_fan);
    let evaluation = ScoringEvaluationInput {
        win_type: win_type.clone(),
        winner_seat: Some(winner_seat),
        discarder_seat,
        flower_count: cache
            .player(winner_seat)
            .map(|player| player.flower_count)
            .unwrap_or(0),
        seat_count: cache.seat_count,
        features,
        timing: timing_features_for_win_state(state, incoming_tile.is_none()),
        kong_entries,
        tile_keys: player_tile_keys,
        visible_tile_keys: cache.visible_tile_keys.clone(),
        concealed_tile_keys,
        meld_tile_key_groups,
        open_meld_tile_key_groups,
        incoming_tile: incoming_tile.map(ToString::to_string),
        decompositions,
    };
    let result = scoring_evaluate_fans(evaluation);
    Ok(EvaluatedWinResult {
        fan_result: result,
        required_minimum_fan_total: if enforce_minimum_eight_fan { 8 } else { 0 },
    })
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

fn seat_wind_key(seat_index: usize, dealer_seat: usize) -> String {
    WIND_ORDER[(seat_index + MAX_SEATS - dealer_seat) % MAX_SEATS].to_string()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::Value;

    use super::{
        can_declare_hu_with_cache_for_state, compute_hu_settlement_for_state,
    };
    use crate::core::state::{
        LastActionContext, MatchState, PlayerRoundState, RoomState, RoundState, SeatState,
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
    fn labels_low_fan_wins_as_pi_he_when_eight_fan_requirement_is_disabled() {
        let tile_keys = [
            "w1", "w2", "w3", "t4", "t5", "t6", "b3", "b4", "b5", "w6", "w7", "w8", "red", "red",
        ];
        let mut state = test_room_state_with_concealed_tiles(&tile_keys);
        state.enforce_minimum_eight_fan = false;
        let round = state.round_state.as_mut().expect("round state");
        round.rule_state.enforce_minimum_eight_fan = false;

        let settlement =
            compute_hu_settlement_for_state(&state, 0, "self_draw").expect("settlement");

        assert!(settlement.fan_total < 8);
        assert_eq!(settlement.display_win_label.as_deref(), Some("屁和"));
    }

    fn test_room_state_with_concealed_tiles(tile_keys: &[&str]) -> RoomState {
        RoomState {
            table_code: "ROOM7P".to_string(),
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
                        ..Default::default()
                    },
                    PlayerRoundState {
                        seat: 2,
                        ..Default::default()
                    },
                    PlayerRoundState {
                        seat: 3,
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
