use crate::bot::{self, BotAction};
use crate::core::action::{GameCommand, PlayerAction};
use crate::core::engine::reducer::{LegacyRoomMutation, apply_legacy_room_mutations};
use crate::core::engine::{
    EngineContext, EngineOutput, parse_legacy_player_command,
    planner::{
        compute_pending_timeout_value, plan_advance_opening_flowers,
        plan_claim_window_continuation_without_winner, plan_claim_window_response,
        plan_discard_action, plan_flower_action, plan_round_start_payload,
        plan_settlement_to_match, resolve_claims,
    },
};
use crate::core::state::{MatchState, PendingAction, RoomState};
use crate::projection::SeatProjectionSupport;
use crate::projection::bot_view::{BotClaimOption, build_bot_context_view};
use crate::room_scoring::RoomScoringCache;
use crate::rules::standard::{
    meld::{
        SelfKongCandidate, SelfKongKind, available_self_kongs, available_self_kongs_from_cache,
        claim_tile_id_options, claim_window_options_after_discard, is_valid_chow_sequence_by_keys,
        resolve_self_kong_selection, seats_with_hu_candidate_for_tile,
    },
    win::{
        can_declare_hu as standard_can_declare_hu,
        can_declare_hu_with_cache as standard_can_declare_hu_with_cache,
        compute_hu_settlement as standard_compute_hu_settlement,
    },
};
use chrono::{SecondsFormat, Utc};
use rand::Rng;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashSet};

const MAX_SEATS: usize = 4;
const ACTIVE_TURN_TIMEOUT_SECONDS: i64 = 30;
const CONTINUE_ACTION_AUTO_ADVANCE_SECONDS: i64 = 30;
const WIND_ORDER: [&str; 4] = ["east", "south", "west", "north"];

pub fn room_messages(room: &Value, local_seat: usize) -> Vec<Value> {
    let mut messages = vec![room_snapshot(room, local_seat)];
    if let Some(result) = match_result_message(room) {
        messages.push(result);
    }
    messages
}

pub fn action_prompt(room: &Value, local_seat: usize) -> Option<Value> {
    let state = project_room_state(room).ok()?;
    crate::projection::prompt::action_prompt_message(
        &state,
        local_seat,
        &seat_projection_support(room, local_seat),
    )
}

pub fn room_ready_to_start(room: &Value) -> bool {
    let seats = room
        .get("seats")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    seats.len() == MAX_SEATS
        && seats.iter().all(|seat| {
            seat.get("ready").and_then(Value::as_bool).unwrap_or(false)
                && (seat
                    .get("connected")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                    || seat.get("is_bot").and_then(Value::as_bool).unwrap_or(false))
        })
}

pub fn next_bot_action(room: &Value) -> Option<BotAction> {
    let state = project_room_state(room).ok()?;
    if state.phase != "playing" {
        return None;
    }
    let pending_timeout = state.pending_timeout.as_ref()?;
    let round = state.round_state.as_ref()?;
    match pending_timeout.kind.as_str() {
        "opening_flowers" => {
            let seat_index = round.current_actor;
            if !state
                .seats
                .iter()
                .find(|seat| seat.seat_index == seat_index)
                .map(|seat| seat.is_bot)
                .unwrap_or(false)
            {
                return None;
            }
            let cache = RoomScoringCache::from_state(&state);
            let tile_ids = player_first_flower_tile_id_from_cache(&cache, seat_index)
                .map(|value| vec![value])
                .unwrap_or_default();
            Some(BotAction {
                seat_index,
                action_type: if tile_ids.is_empty() {
                    "pass".to_string()
                } else {
                    "flower".to_string()
                },
                tile_ids,
            })
        }
        "active_turn" => {
            let seat_index = round.current_actor;
            if !state
                .seats
                .iter()
                .find(|seat| seat.seat_index == seat_index)
                .map(|seat| seat.is_bot)
                .unwrap_or(false)
            {
                return None;
            }
            let cache = RoomScoringCache::from_state(&state);
            if standard_can_declare_hu_with_cache(room, &cache, seat_index, None, None) {
                return Some(BotAction {
                    seat_index,
                    action_type: "hu".to_string(),
                    tile_ids: vec![],
                });
            }
            if let Some(tile_id) = player_first_flower_tile_id_from_cache(&cache, seat_index) {
                return Some(BotAction {
                    seat_index,
                    action_type: "flower".to_string(),
                    tile_ids: vec![tile_id],
                });
            }
            choose_bot_active_turn_action_with_cache(room, &cache, seat_index)
        }
        "claim_window" => match round.pending_action.as_ref()? {
            PendingAction::RobKongWindow(rob) => {
                let seat_index = rob.offered_hu_seats.iter().copied().find(|seat| {
                    state
                        .seats
                        .iter()
                        .find(|seat_state| seat_state.seat_index == *seat)
                        .map(|seat_state| seat_state.is_bot)
                        .unwrap_or(false)
                        && !rob.responded_seats.contains(seat)
                })?;
                Some(BotAction {
                    seat_index,
                    action_type: "hu".to_string(),
                    tile_ids: vec![],
                })
            }
            PendingAction::ClaimWindow(claim) => {
                let cache = RoomScoringCache::from_state(&state);
                let seat_index = claim
                    .claim_window
                    .iter()
                    .enumerate()
                    .find(|(seat, claims)| {
                        state
                            .seats
                            .iter()
                            .find(|seat_state| seat_state.seat_index == *seat)
                            .map(|seat_state| seat_state.is_bot)
                            .unwrap_or(false)
                            && !claims.is_empty()
                            && !claim.responded_seats.contains(seat)
                    })
                    .map(|(seat, _)| seat)?;
                choose_bot_claim_action_with_cache(room, &cache, seat_index)
            }
            _ => None,
        },
        _ => None,
    }
}

pub fn add_bot_seats_for_test(room: &mut Value) {
    let Some(obj) = room.as_object_mut() else {
        return;
    };
    let seats = obj
        .get_mut("seats")
        .and_then(Value::as_array_mut)
        .expect("room seats should exist");
    let occupied: std::collections::HashSet<usize> = seats
        .iter()
        .filter_map(|seat| {
            seat.get("seat_index")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
        })
        .collect();
    for seat_index in 0..MAX_SEATS {
        if occupied.contains(&seat_index) {
            continue;
        }
        seats.push(json!({
            "seat_index": seat_index,
            "nickname": format!("Bot {seat_index}"),
            "reconnect_token": Value::Null,
            "player_session_id": -((seat_index as i64) + 1),
            "connected": true,
            "ready": true,
            "is_bot": true,
            "seat_type": "bot",
            "bot_persona": Value::Null,
            "bot_aggression": Value::Null,
            "disconnect_deadline_at": Value::Null,
        }));
    }
    seats.sort_by_key(|seat| seat.get("seat_index").and_then(Value::as_u64).unwrap_or(99));
}

pub fn start_match(room: &mut Value, dealer_seat: usize, seed: u64) {
    let enforce_minimum_eight_fan = room
        .get("enforce_minimum_eight_fan")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    start_round(
        room,
        dealer_seat,
        "east",
        format!("east-1-dealer-{dealer_seat}-{seed}"),
        enforce_minimum_eight_fan,
        seed,
    );

    let mut cumulative_scores = BTreeMap::new();
    for seat in 0..MAX_SEATS {
        cumulative_scores.insert(seat, 0);
    }
    let match_state = MatchState {
        prevailing_wind: "east".to_string(),
        hand_number: 1,
        dealer_seat,
        cumulative_scores,
        match_finished: false,
        last_completed_round_id: None,
    };
    let _ = apply_legacy_room_mutations(
        room,
        &[LegacyRoomMutation::SetRoomField {
            key: "match_state".to_string(),
            value: serde_json::to_value(&match_state).unwrap_or(Value::Null),
        }],
    );
}

fn start_round(
    room: &mut Value,
    dealer_seat: usize,
    round_wind: &str,
    round_id: String,
    enforce_minimum_eight_fan: bool,
    seed: u64,
) {
    let (round_state, pending_timeout) = plan_round_start_payload(
        dealer_seat,
        round_wind,
        round_id,
        enforce_minimum_eight_fan,
        seed,
    );
    let round_state = serde_json::to_value(&round_state).unwrap_or(Value::Null);
    let pending_timeout = serde_json::to_value(&pending_timeout).unwrap_or(Value::Null);
    let _ = apply_legacy_room_mutations(
        room,
        &[
            LegacyRoomMutation::SetRoomField {
                key: "phase".to_string(),
                value: Value::String("playing".to_string()),
            },
            LegacyRoomMutation::SetRoomField {
                key: "round_state".to_string(),
                value: round_state,
            },
            LegacyRoomMutation::SetRoomField {
                key: "pending_timeout".to_string(),
                value: pending_timeout,
            },
            LegacyRoomMutation::SetRoomField {
                key: "start_next_round_confirmed_seats".to_string(),
                value: Value::Array(vec![]),
            },
            LegacyRoomMutation::SetRoomField {
                key: "restart_match_confirmed_seats".to_string(),
                value: Value::Array(vec![]),
            },
            LegacyRoomMutation::SetRoomField {
                key: "continue_action_auto_advance_deadline_at".to_string(),
                value: Value::Null,
            },
        ],
    );
}

pub fn try_handle_action(
    room: &mut Value,
    seat_index: usize,
    action_type: &str,
    tile_ids: &[String],
) -> Option<Result<Vec<Value>, String>> {
    let command = match parse_legacy_player_command(seat_index, action_type, tile_ids)? {
        Ok(command) => command,
        Err(reason) => return Some(Err(reason)),
    };
    try_handle_command(room, command).map(|result| result.map(|output| output.emitted_messages))
}

pub fn try_handle_command(
    room: &mut Value,
    command: GameCommand,
) -> Option<Result<EngineOutput, String>> {
    let context = EngineContext::from_legacy_room(room).ok()?;
    match command {
        GameCommand::PlayerAction { actor, action } => {
            try_handle_player_action_command(room, &context, actor, action)
        }
        _ => None,
    }
}

fn try_handle_player_action_command(
    room: &mut Value,
    _context: &EngineContext,
    seat_index: usize,
    action: PlayerAction,
) -> Option<Result<EngineOutput, String>> {
    match action {
        PlayerAction::Hu => {
            Some(apply_hu_action(room, seat_index).map(EngineOutput::from_emitted_messages))
        }
        PlayerAction::Flower { tile_ids } => Some(
            apply_flower_action(room, seat_index, &tile_ids)
                .map(EngineOutput::from_emitted_messages),
        ),
        PlayerAction::Discard { tile_id } => {
            if can_resolve_discard_locally(room, seat_index, &tile_id) {
                Some(
                    apply_discard_action(room, seat_index, &tile_id)
                        .map(EngineOutput::from_emitted_messages),
                )
            } else {
                None
            }
        }
        PlayerAction::Kong { tile_ids } => {
            if claim_window_supported_locally(room) {
                Some(
                    apply_claim_window_action(room, seat_index, "kong", &tile_ids)
                        .map(EngineOutput::from_emitted_messages),
                )
            } else if rob_kong_window_supported_locally(room) {
                None
            } else if is_self_kong_turn(room, seat_index) {
                try_handle_self_kong_action(room, seat_index, &tile_ids)
                    .map(|result| result.map(EngineOutput::from_emitted_messages))
            } else {
                None
            }
        }
        PlayerAction::Pass => {
            if claim_window_supported_locally(room) {
                Some(
                    apply_claim_window_action(room, seat_index, "pass", &[])
                        .map(EngineOutput::from_emitted_messages),
                )
            } else if rob_kong_window_supported_locally(room) {
                Some(apply_rob_kong_pass(room, seat_index).map(EngineOutput::from_emitted_messages))
            } else if pending_timeout_kind(room) == Some("opening_flowers") {
                Some(
                    apply_opening_flowers_pass(room, seat_index)
                        .map(EngineOutput::from_emitted_messages),
                )
            } else {
                None
            }
        }
        PlayerAction::Chow { tile_ids } => {
            if claim_window_supported_locally(room) {
                Some(
                    apply_claim_window_action(room, seat_index, "chow", &tile_ids)
                        .map(EngineOutput::from_emitted_messages),
                )
            } else {
                None
            }
        }
        PlayerAction::Pung { tile_ids } => {
            if claim_window_supported_locally(room) {
                Some(
                    apply_claim_window_action(room, seat_index, "pung", &tile_ids)
                        .map(EngineOutput::from_emitted_messages),
                )
            } else {
                None
            }
        }
        PlayerAction::ActivateSkill { .. } => Some(Err("unsupported_action".to_string())),
    }
}

pub fn try_process_due_timeout(room: &mut Value) -> Option<Vec<Value>> {
    match pending_timeout_kind(room) {
        Some("active_turn") => {
            let seat_index = current_actor(room)?;
            let tile_id = room
                .get("pending_timeout")
                .and_then(|timeout| timeout.get("drawn_tile_id"))
                .and_then(Value::as_str)
                .map(ToString::to_string)
                .or_else(|| last_concealed_tile_id(room, seat_index))?;
            if !can_resolve_discard_locally(room, seat_index, &tile_id) {
                return None;
            }
            apply_discard_action(room, seat_index, &tile_id).ok()
        }
        Some("opening_flowers") => {
            let seat_index = current_actor(room)?;
            let drawn_tile_id = room
                .get("pending_timeout")
                .and_then(|timeout| timeout.get("drawn_tile_id"))
                .and_then(Value::as_str)
                .map(ToString::to_string);
            let result = if let Some(tile_id) = drawn_tile_id {
                apply_flower_action(room, seat_index, &[tile_id])
            } else {
                apply_opening_flowers_pass(room, seat_index)
            };
            result.ok()
        }
        Some("claim_window") => {
            if claim_window_supported_locally(room)
                && can_resolve_claim_window_timeout_locally(room)
            {
                resolve_claim_window_timeout(room).ok()
            } else if rob_kong_window_supported_locally(room)
                && can_resolve_rob_kong_timeout_locally(room)
            {
                resolve_rob_kong_timeout(room).ok()
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn hu_action_hint(room: &Value, seat_index: usize) -> Option<&'static str> {
    let phase = room.get("phase").and_then(Value::as_str)?;
    if phase != "playing" {
        return None;
    }
    let round_state = room.get("round_state")?;
    let pending = round_state.get("pending_action");
    if pending.is_none() || pending.is_some_and(Value::is_null) {
        if current_actor(room) == Some(seat_index) {
            return Some("self_draw");
        }
        return None;
    }
    let pending_action = pending?;
    match pending_action.get("type").and_then(Value::as_str) {
        Some("claim_window") if claim_window_offers_claim(pending_action, seat_index, "hu") => {
            Some("discard")
        }
        Some("rob_kong_window") if rob_kong_window_offers_seat(pending_action, seat_index) => {
            Some("discard")
        }
        Some("claim_window") | Some("rob_kong_window") => None,
        _ => None,
    }
}

fn claim_window_offers_claim(pending_action: &Value, seat_index: usize, claim_type: &str) -> bool {
    json_array_contains_str(
        pending_action
            .get("claim_window")
            .and_then(Value::as_array)
            .and_then(|claim_window| claim_window.get(seat_index))
            .and_then(Value::as_array),
        claim_type,
    )
}

fn rob_kong_window_offers_seat(pending_action: &Value, seat_index: usize) -> bool {
    json_array_contains_seat(
        pending_action
            .get("offered_hu_seats")
            .and_then(Value::as_array),
        seat_index,
    )
}

fn json_array_contains_seat(values: Option<&Vec<Value>>, seat_index: usize) -> bool {
    values.is_some_and(|items| {
        items.iter().any(|value| {
            value
                .as_u64()
                .map(|seat| seat as usize == seat_index)
                .unwrap_or(false)
        })
    })
}

fn json_array_contains_str(values: Option<&Vec<Value>>, needle: &str) -> bool {
    values.is_some_and(|items| items.iter().any(|value| value.as_str() == Some(needle)))
}

pub fn record_continue_action(
    room: &mut Value,
    seat_index: usize,
    action_id: &str,
) -> Result<(), String> {
    let current_action =
        current_continue_action_id(room).ok_or_else(|| "invalid_action".to_string())?;
    if current_action != action_id {
        return Err(match action_id {
            "start_next_round" => "round_not_ready".to_string(),
            "restart_match" => "match_not_finished".to_string(),
            _ => "invalid_action".to_string(),
        });
    }
    let field = if action_id == "start_next_round" {
        "start_next_round_confirmed_seats"
    } else {
        "restart_match_confirmed_seats"
    };
    apply_legacy_room_mutations(
        room,
        &[LegacyRoomMutation::PushUniqueSeatToRoomArray {
            key: field.to_string(),
            seat_index,
        }],
    )?;
    reconcile_continue_action(room)?;
    Ok(())
}

pub fn process_due_continue_action(room: &mut Value) -> Result<bool, String> {
    let action_id = current_continue_action_id(room).ok_or_else(|| "invalid_action".to_string())?;
    let deadline = room
        .get("continue_action_auto_advance_deadline_at")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    if deadline.is_none() {
        return Ok(false);
    }
    complete_continue_action(room, action_id)?;
    Ok(true)
}

pub fn reconcile_continue_action_state(room: &mut Value) -> Result<(), String> {
    reconcile_continue_action(room)
}

pub fn apply_hu_settlement(
    room: &mut Value,
    winner_seat: usize,
    hu_context: &str,
    settlement: Value,
) -> Result<Vec<Value>, String> {
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
        LegacyRoomMutation::SetRoundCurrentActor {
            seat_index: winner_seat,
        },
        LegacyRoomMutation::IncrementRoundVersion,
    ];
    if let Ok(state) = project_room_state(room) {
        mutations.extend(plan_settlement_to_match(&state, &settlement));
    }
    apply_legacy_room_mutations(room, &mutations)?;

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
    Ok(vec![
        first_event,
        round_event_message(
            "settlement_ready",
            json!({
                "type": "settlement_ready",
                "round_id": round_id,
            }),
        ),
    ])
}

fn apply_hu_action(room: &mut Value, seat_index: usize) -> Result<Vec<Value>, String> {
    let Some(hu_context) = hu_action_hint(room, seat_index) else {
        return Err("invalid_action".to_string());
    };
    let settlement = standard_compute_hu_settlement(room, seat_index, hu_context)?;
    apply_hu_settlement(room, seat_index, hu_context, settlement)
}

/*

fn compute_hu_settlement_unused(
    room: &Value,
    winner_seat: usize,
    hu_context: &str,
) -> Result<Value, String> {
    if room.get("phase").and_then(Value::as_str) != Some("playing") {
        return Err("round_not_ready".to_string());
    }

    let discarder_seat = if hu_context == "self_draw" {
        if current_actor(room) != Some(winner_seat) {
            return Err("invalid_action".to_string());
        }
        None
    } else {
        let Some(pending_action) = room
            .get("round_state")
            .and_then(|round| round.get("pending_action"))
        else {
            return Err("invalid_action".to_string());
        };
        match pending_action.get("type").and_then(Value::as_str) {
            Some("claim_window") => {
                if !claim_window_offers_claim(pending_action, winner_seat, "hu") {
                    return Err("invalid_action".to_string());
                }
                pending_action
                    .get("discarder_seat")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize)
            }
            Some("rob_kong_window") => {
                if !rob_kong_window_offers_seat(pending_action, winner_seat) {
                    return Err("invalid_action".to_string());
                }
                pending_action
                    .get("actor_seat")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize)
            }
            _ => return Err("invalid_action".to_string()),
        }
    };

    let incoming_tile = if hu_context == "self_draw" {
        None
    } else {
        room.get("round_state")
            .and_then(|round| round.get("last_discard"))
            .and_then(|tile| tile.get("tile_key"))
            .and_then(Value::as_str)
    };

    let fan_result = fan_result_for_win(room, winner_seat, incoming_tile, discarder_seat)?;
    let flower_count = player_flower_count(room, winner_seat);
    let enforce_minimum_eight_fan = room
        .get("round_state")
        .and_then(|round| round.get("enforce_minimum_eight_fan"))
        .and_then(Value::as_bool)
        .or_else(|| {
            room.get("enforce_minimum_eight_fan")
                .and_then(Value::as_bool)
        })
        .unwrap_or(true);

    Ok(json!({
        "provisional": true,
        "win_type": hu_context,
        "winner_seat": winner_seat,
        "discarder_seat": discarder_seat,
        "display_win_label": if !enforce_minimum_eight_fan && fan_result.fan_total < 8 { Value::String("屁和".to_string()) } else { Value::Null },
        "fan_total": fan_result.fan_total,
        "fan_keys": fan_result.fan_keys,
        "fan_breakdown": Value::Array(
            fan_result
                .fan_breakdown
                .iter()
                .map(|entry| json!({ "fan_key": entry.fan_key, "fan_value": entry.fan_value }))
                .collect()
        ),
        "score_delta": fan_result.score_delta_json(),
        "flower_count": flower_count,
        "kong_score_detail": fan_result.kong_score_detail_json(),
    }))
}

fn can_declare_hu(
    room: &Value,
    seat_index: usize,
    incoming_tile: Option<&str>,
    discarder_seat: Option<usize>,
) -> bool {
    standard_can_declare_hu(room, seat_index, incoming_tile, discarder_seat)
}

fn can_declare_hu_with_cache(
    room: &Value,
    cache: &RoomScoringCache,
    seat_index: usize,
    incoming_tile: Option<&str>,
    discarder_seat: Option<usize>,
) -> bool {
    standard_can_declare_hu_with_cache(room, cache, seat_index, incoming_tile, discarder_seat)
}

fn fan_result_for_win(
    room: &Value,
    winner_seat: usize,
    incoming_tile: Option<&str>,
    discarder_seat: Option<usize>,
) -> Result<crate::scoring::FanResult, String> {
    let cache = RoomScoringCache::from_room(room);
    fan_result_for_win_with_cache(room, &cache, winner_seat, incoming_tile, discarder_seat)
}

fn fan_result_for_win_with_cache(
    _room: &Value,
    cache: &RoomScoringCache,
    winner_seat: usize,
    incoming_tile: Option<&str>,
    discarder_seat: Option<usize>,
) -> Result<crate::scoring::FanResult, String> {
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

    Ok(scoring_evaluate_fans(ScoringEvaluationInput {
        win_type,
        winner_seat: Some(winner_seat),
        discarder_seat,
        flower_count: cache
            .player(winner_seat)
            .map(|player| player.flower_count)
            .unwrap_or(0),
        seat_count: cache.seat_count,
        features,
        timing: timing_features_for_win(_room, incoming_tile.is_none()),
        kong_entries,
        tile_keys: player_tile_keys,
        visible_tile_keys: cache.visible_tile_keys.clone(),
        concealed_tile_keys,
        meld_tile_key_groups,
        open_meld_tile_key_groups,
        incoming_tile: incoming_tile.map(ToString::to_string),
        decompositions,
    }))
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

fn timing_features_for_win(room: &Value, self_draw: bool) -> ScoringTimingFeatures {
    let context = room
        .get("round_state")
        .and_then(|round| round.get("last_action_context"))
        .cloned()
        .unwrap_or(Value::Null);
    if self_draw {
        let is_replacement = context
            .get("from_kong_replacement")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        return ScoringTimingFeatures {
            gang_shang_hua: is_replacement,
            hai_di_lao_yue: !is_replacement
                && context.get("kind").and_then(Value::as_str) == Some("draw")
                && context
                    .get("was_last_live_tile")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            he_di_lao_yu: false,
            robbing_the_kong: false,
        };
    }

    ScoringTimingFeatures {
        gang_shang_hua: false,
        hai_di_lao_yue: false,
        he_di_lao_yu: context.get("kind").and_then(Value::as_str) == Some("discard")
            && context
                .get("was_last_discard")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        robbing_the_kong: room
            .get("round_state")
            .and_then(|round| round.get("pending_action"))
            .and_then(|pending| pending.get("type"))
            .and_then(Value::as_str)
            == Some("rob_kong_window"),
    }
}

fn player_flower_count(room: &Value, seat_index: usize) -> usize {
    room.get("round_state")
        .and_then(|round| round.get("players"))
        .and_then(Value::as_array)
        .and_then(|players| players.get(seat_index))
        .and_then(|player| player.get("flowers"))
        .and_then(Value::as_array)
        .map(|flowers| flowers.len())
        .unwrap_or(0)
}

*/

fn room_snapshot(room: &Value, local_seat: usize) -> Value {
    let Ok(state) = project_room_state(room) else {
        return json!({
            "type": "room_snapshot",
            "payload": {
                "table_code": room.get("table_code").cloned().unwrap_or(Value::Null),
                "phase": room.get("phase").cloned().unwrap_or(Value::String("waiting".to_string())),
                "mode": room.get("mode").cloned().unwrap_or(Value::String("normal".to_string())),
                "seats": Value::Array(vec![]),
                "local_seat": local_seat,
                "reconnect_token": Value::Null,
                "match_state": Value::Null,
                "private_state": Value::Null,
                "continue_action": Value::Null,
            }
        });
    };
    crate::projection::room_snapshot::room_snapshot_message(
        &state,
        local_seat,
        &seat_projection_support(room, local_seat),
    )
}

fn apply_opening_flowers_pass(room: &mut Value, seat_index: usize) -> Result<Vec<Value>, String> {
    let round_state = room
        .get("round_state")
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| "round_not_ready".to_string())?;
    let pending_action = round_state
        .get("pending_action")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if pending_action.get("type").and_then(Value::as_str) != Some("opening_flowers") {
        return Err("invalid_action".to_string());
    }
    if current_actor(room) != Some(seat_index) {
        return Err("not_your_turn".to_string());
    }
    if player_has_concealed_flower(&Value::Object(round_state.clone()), seat_index) {
        return Err("invalid_action".to_string());
    }

    let state = project_room_state(room)?;
    let mutations = plan_advance_opening_flowers(&state, seat_index);
    apply_legacy_room_mutations(room, &mutations)?;
    sync_pending_timeout(room);
    Ok(vec![])
}

fn apply_flower_action(
    room: &mut Value,
    seat_index: usize,
    tile_ids: &[String],
) -> Result<Vec<Value>, String> {
    if room.get("phase").and_then(Value::as_str) != Some("playing") {
        return Err("round_not_ready".to_string());
    }
    if current_actor(room) != Some(seat_index) {
        return Err("not_your_turn".to_string());
    }
    if tile_ids.len() != 1 {
        return Err("invalid_action".to_string());
    }
    if is_last_live_tile_point(room) {
        return Err("invalid_action".to_string());
    }

    let pending_type = room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))
        .and_then(|pending| pending.get("type"))
        .and_then(Value::as_str);
    if pending_type.is_some() && pending_type != Some("opening_flowers") {
        return Err("invalid_action".to_string());
    }

    let tile_id = &tile_ids[0];
    let state = project_room_state(room)?;
    let plan = plan_flower_action(&state, seat_index, tile_id)?;
    apply_legacy_room_mutations(room, &plan.mutations)?;
    sync_pending_timeout(room);

    Ok(vec![
        round_event_message(
            "flower_exposed",
            json!({
                "type": "flower_exposed",
                "seat": seat_index,
                "tile_id": plan.flower_tile.tile_id,
            }),
        ),
        round_event_message(
            "replacement_draw",
            json!({
                "type": "replacement_draw",
                "seat": seat_index,
                "tile_id": plan.replacement_tile.tile_id,
            }),
        ),
    ])
}

fn try_handle_self_kong_action(
    room: &mut Value,
    seat_index: usize,
    tile_ids: &[String],
) -> Option<Result<Vec<Value>, String>> {
    let candidates = available_self_kongs(room, seat_index);
    if candidates.is_empty() {
        return Some(Err("invalid_action".to_string()));
    }
    let selection = resolve_self_kong_selection(&candidates, tile_ids);
    let Some(selection) = selection else {
        return Some(Err("invalid_action".to_string()));
    };
    replacement_tile_from_tail(room)?;
    if selection.kind == SelfKongKind::Add {
        let offered_hu_seats =
            seats_with_hu_candidate_for_tile(room, seat_index, &selection.tile_key);
        if !offered_hu_seats.is_empty() {
            return Some(start_rob_kong_window(
                room,
                seat_index,
                &selection,
                offered_hu_seats,
            ));
        }
    }
    Some(apply_self_kong_action(room, seat_index, &selection))
}

fn apply_self_kong_action(
    room: &mut Value,
    seat_index: usize,
    selection: &SelfKongCandidate,
) -> Result<Vec<Value>, String> {
    if room.get("phase").and_then(Value::as_str) != Some("playing") {
        return Err("round_not_ready".to_string());
    }
    if current_actor(room) != Some(seat_index) {
        return Err("not_your_turn".to_string());
    }
    if room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))
        .filter(|value| !value.is_null())
        .is_some()
    {
        return Err("invalid_action".to_string());
    }
    if is_last_live_tile_point(room) {
        return Err("invalid_action".to_string());
    }

    let replacement_tile =
        replacement_tile_from_tail(room).ok_or_else(|| "invalid_action".to_string())?;
    complete_self_kong(room, seat_index, selection, replacement_tile)
}

fn complete_self_kong(
    room: &mut Value,
    seat_index: usize,
    selection: &SelfKongCandidate,
    replacement_tile: Value,
) -> Result<Vec<Value>, String> {
    let plan =
        plan_self_kong_completion(room, seat_index, selection, replacement_tile.clone(), false)?;
    apply_legacy_room_mutations(room, &plan.mutations)?;
    sync_pending_timeout(room);
    Ok(plan.events)
}

fn start_rob_kong_window(
    room: &mut Value,
    seat_index: usize,
    selection: &SelfKongCandidate,
    offered_hu_seats: Vec<usize>,
) -> Result<Vec<Value>, String> {
    let selected_tile = player_concealed_tile(
        room,
        seat_index,
        selection
            .tile_ids
            .first()
            .map(String::as_str)
            .unwrap_or_default(),
    )
    .cloned()
    .ok_or_else(|| "invalid_action".to_string())?;
    apply_legacy_room_mutations(
        room,
        &[
            LegacyRoomMutation::SetRoundLastDiscard {
                tile: selected_tile.clone(),
            },
            LegacyRoomMutation::SetRoundPendingAction {
                pending_action: json!({
                    "type": "rob_kong_window",
                    "actor_seat": seat_index,
                    "tile_id": selected_tile.get("tile_id").cloned().unwrap_or(Value::Null),
                    "tile_key": selected_tile.get("tile_key").cloned().unwrap_or(Value::Null),
                    "meld_index": selection.meld_index,
                    "offered_hu_seats": offered_hu_seats,
                    "responded_seats": [],
                }),
            },
            LegacyRoomMutation::IncrementRoundVersion,
        ],
    )?;
    sync_pending_timeout(room);
    Ok(vec![round_event_message(
        "self_kong_declared",
        json!({
            "type": "self_kong_declared",
            "seat": seat_index,
            "kong_type": "add_kong",
            "tile_key": selection.tile_key,
            "tile_ids": selection.tile_ids,
        }),
    )])
}

struct SelfKongPlan {
    mutations: Vec<LegacyRoomMutation>,
    events: Vec<Value>,
}

fn plan_self_kong_completion(
    room: &Value,
    seat_index: usize,
    selection: &SelfKongCandidate,
    replacement_tile: Value,
    clear_pending_action: bool,
) -> Result<SelfKongPlan, String> {
    for tile_id in &selection.tile_ids {
        player_concealed_tile(room, seat_index, tile_id)
            .ok_or_else(|| "invalid_action".to_string())?;
    }

    let mut mutations = selection
        .tile_ids
        .iter()
        .cloned()
        .map(
            |tile_id| LegacyRoomMutation::RemovePlayerConcealedTileById {
                seat_index,
                tile_id,
            },
        )
        .collect::<Vec<_>>();

    match selection.kind {
        SelfKongKind::Concealed => {
            mutations.push(LegacyRoomMutation::PushPlayerMeld {
                seat_index,
                meld: Value::Array(vec![
                    Value::String(selection.tile_key.clone()),
                    Value::String(selection.tile_key.clone()),
                    Value::String(selection.tile_key.clone()),
                    Value::String(selection.tile_key.clone()),
                ]),
            });
        }
        SelfKongKind::Add => {
            let meld_index = selection
                .meld_index
                .ok_or_else(|| "invalid_action".to_string())?;
            mutations.push(LegacyRoomMutation::AppendTileToPlayerMeld {
                seat_index,
                meld_index,
                tile: Value::String(selection.tile_key.clone()),
            });
        }
    }

    mutations.push(LegacyRoomMutation::PushPlayerConcealedTile {
        seat_index,
        tile: replacement_tile.clone(),
    });
    mutations.push(LegacyRoomMutation::RetreatWallTail);
    mutations.push(LegacyRoomMutation::AppendRoundKongEntry {
        kong_type: match selection.kind {
            SelfKongKind::Concealed => "concealed_kong".to_string(),
            SelfKongKind::Add => "add_kong".to_string(),
        },
        actor_seat: seat_index,
        payer_seats: (0..MAX_SEATS)
            .filter(|other| *other != seat_index)
            .collect(),
        tile_key: Value::String(selection.tile_key.clone()),
    });
    mutations.push(LegacyRoomMutation::SetRoundLastActionContext {
        context: json!({
            "kind": "replacement_draw",
            "seat": seat_index,
            "tile_id": replacement_tile.get("tile_id").cloned().unwrap_or(Value::Null),
            "from_kong_replacement": true,
            "was_last_live_tile": false,
            "was_last_discard": false,
        }),
    });
    if clear_pending_action {
        mutations.push(LegacyRoomMutation::SetRoundPendingAction {
            pending_action: Value::Null,
        });
    }
    mutations.push(LegacyRoomMutation::IncrementRoundVersion);

    Ok(SelfKongPlan {
        mutations,
        events: vec![
            round_event_message(
                "self_kong_declared",
                json!({
                    "type": "self_kong_declared",
                    "seat": seat_index,
                    "kong_type": match selection.kind {
                        SelfKongKind::Concealed => "concealed_kong",
                        SelfKongKind::Add => "add_kong",
                    },
                    "tile_key": selection.tile_key,
                    "tile_ids": selection.tile_ids,
                }),
            ),
            round_event_message(
                "replacement_draw",
                json!({
                    "type": "replacement_draw",
                    "seat": seat_index,
                    "tile_id": replacement_tile.get("tile_id").cloned().unwrap_or(Value::Null),
                }),
            ),
        ],
    })
}

fn apply_discard_action(
    room: &mut Value,
    seat_index: usize,
    tile_id: &str,
) -> Result<Vec<Value>, String> {
    if room.get("phase").and_then(Value::as_str) != Some("playing") {
        return Err("round_not_ready".to_string());
    }
    if current_actor(room) != Some(seat_index) {
        return Err("not_your_turn".to_string());
    }
    if room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))
        .filter(|value| !value.is_null())
        .is_some()
    {
        return Err("invalid_action".to_string());
    }

    let restricted_discard_tile_key = room
        .get("round_state")
        .and_then(|round| round.get("restricted_discard_tile_key"))
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let discarded_tile = player_concealed_tile(room, seat_index, tile_id)
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;
    if let Some(restricted) = restricted_discard_tile_key.as_deref() {
        if discarded_tile.get("tile_key").and_then(Value::as_str) == Some(restricted) {
            return Err("invalid_action".to_string());
        }
    }
    let state = project_room_state(room)?;
    let discard_mutations = vec![
        LegacyRoomMutation::RemovePlayerConcealedTileById {
            seat_index,
            tile_id: tile_id.to_string(),
        },
        LegacyRoomMutation::PushPlayerDiscard {
            seat_index,
            tile: discarded_tile.clone(),
        },
        LegacyRoomMutation::SetRoundLastDiscard {
            tile: discarded_tile.clone(),
        },
    ];
    let mut simulated = room.clone();
    apply_legacy_room_mutations(&mut simulated, &discard_mutations)?;

    let previous_was_last_live_tile = room
        .get("round_state")
        .and_then(|round| round.get("last_action_context"))
        .and_then(|context| context.get("was_last_live_tile"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let claim_window = claim_window_options_after_discard(
        &simulated,
        seat_index,
        discarded_tile
            .get("tile_key")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    let plan = plan_discard_action(
        &state,
        seat_index,
        tile_id,
        claim_window,
        previous_was_last_live_tile,
    )?;
    apply_legacy_room_mutations(room, &plan.discard_mutations)?;
    if plan.needs_exhaustive_draw {
        let mut messages = vec![round_event_message(
            "tile_discarded",
            json!({
                "type": "tile_discarded",
                "seat": seat_index,
                "tile_id": plan.discarded_tile.tile_id,
            }),
        )];
        messages.extend(settle_exhaustive_draw_local(room));
        return Ok(messages);
    }
    apply_legacy_room_mutations(room, &plan.followup_mutations)?;
    sync_pending_timeout(room);
    Ok(vec![round_event_message(
        "tile_discarded",
        json!({
            "type": "tile_discarded",
            "seat": seat_index,
            "tile_id": plan.discarded_tile.tile_id,
        }),
    )])
}

fn apply_claim_window_action(
    room: &mut Value,
    seat_index: usize,
    action_type: &str,
    tile_ids: &[String],
) -> Result<Vec<Value>, String> {
    if matches!(action_type, "chow" | "pung" | "kong") {
        validate_claim_selection(room, seat_index, action_type, tile_ids)?;
    }
    let state = project_room_state(room)?;
    let plan = plan_claim_window_response(&state, seat_index, action_type, tile_ids)?;
    apply_legacy_room_mutations(room, &plan.mutations)?;
    if !plan.unresolved_seats.is_empty() {
        sync_pending_timeout(room);
        return Ok(vec![]);
    }
    resolve_recorded_claims_local(room)
}

fn is_self_kong_turn(room: &Value, seat_index: usize) -> bool {
    pending_timeout_kind(room) == Some("active_turn") && current_actor(room) == Some(seat_index)
}

fn resolve_recorded_claims_local(room: &mut Value) -> Result<Vec<Value>, String> {
    let pending_action = room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;
    if pending_action.get("type").and_then(Value::as_str) != Some("claim_window") {
        return Err("invalid_action".to_string());
    }
    let discarder_seat = pending_action
        .get("discarder_seat")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| "invalid_action".to_string())?;
    let claim_responses = pending_action
        .get("claim_responses")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    if let Some(winner) = resolve_claims(&claim_responses, discarder_seat) {
        let winner_seat = winner
            .get("seat")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .ok_or_else(|| "invalid_action".to_string())?;
        let claim_type = winner
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| "invalid_action".to_string())?;
        let tiles = winner
            .get("tiles")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| value.as_str().map(ToString::to_string))
            .collect::<Vec<_>>();
        return apply_selected_claim(room, winner_seat, claim_type, &tiles);
    }

    let state = project_room_state(room)?;
    let plan = plan_claim_window_continuation_without_winner(&state, discarder_seat)?;
    if plan.needs_exhaustive_draw {
        return Ok(settle_exhaustive_draw_local(room));
    }
    apply_legacy_room_mutations(room, &plan.mutations)?;
    sync_pending_timeout(room);
    Ok(vec![])
}

fn validate_claim_selection(
    room: &Value,
    seat_index: usize,
    action_type: &str,
    tile_ids: &[String],
) -> Result<(), String> {
    let last_discard = room
        .get("round_state")
        .and_then(|round| round.get("last_discard"))
        .ok_or_else(|| "invalid_action".to_string())?;
    let last_discard_tile_key = last_discard
        .get("tile_key")
        .and_then(Value::as_str)
        .ok_or_else(|| "invalid_action".to_string())?;
    let expected = match action_type {
        "chow" | "pung" => 2,
        "kong" => 3,
        _ => return Err("invalid_action".to_string()),
    };
    if tile_ids.len() != expected {
        return Err("invalid_action".to_string());
    }
    let player_tiles = player_concealed_tiles_slice(room, seat_index)
        .ok_or_else(|| "invalid_action".to_string())?;
    let mut claimed_tile_keys = Vec::with_capacity(tile_ids.len());
    let mut used_indices = HashSet::with_capacity(tile_ids.len());
    for tile_id in tile_ids {
        let Some((index, tile)) = player_tiles.iter().enumerate().find(|(index, tile)| {
            !used_indices.contains(index)
                && tile.get("tile_id").and_then(Value::as_str) == Some(tile_id.as_str())
        }) else {
            return Err("invalid_action".to_string());
        };
        used_indices.insert(index);
        claimed_tile_keys.push(
            tile.get("tile_key")
                .and_then(Value::as_str)
                .ok_or_else(|| "invalid_action".to_string())?,
        );
    }

    if (action_type == "pung" || action_type == "kong")
        && claimed_tile_keys
            .iter()
            .any(|tile_key| *tile_key != last_discard_tile_key)
    {
        return Err("invalid_action".to_string());
    }
    if action_type == "chow"
        && !is_valid_chow_sequence_by_keys(last_discard_tile_key, &claimed_tile_keys)
    {
        return Err("invalid_action".to_string());
    }
    Ok(())
}

/*

fn is_valid_chow_sequence_by_keys(discard_tile_key: &str, tiles: &[&str]) -> bool {
    if tiles.len() != 2 {
        return false;
    }
    let Some(discard_index) = tile_index(discard_tile_key) else {
        return false;
    };
    let Some((discard_suit, discard_rank)) = suited_tile_components(discard_index) else {
        return false;
    };
    let mut ranks = vec![discard_rank as i32];
    for tile_key in tiles {
        let Some(tile_index) = tile_index(tile_key) else {
            return false;
        };
        let Some((tile_suit, rank)) = suited_tile_components(tile_index) else {
            return false;
        };
        if tile_suit != discard_suit {
            return false;
        }
        ranks.push(rank as i32);
    }
    ranks.sort_unstable();
    ranks[0] + 1 == ranks[1] && ranks[1] + 1 == ranks[2]
}

*/

fn discarder_latest_discard_matches(
    room: &Value,
    discarder_seat: usize,
    last_discard: &Value,
) -> bool {
    room.get("round_state")
        .and_then(|round| round.get("players"))
        .and_then(Value::as_array)
        .and_then(|players| players.get(discarder_seat))
        .and_then(|player| player.get("discards"))
        .and_then(Value::as_array)
        .and_then(|discards| discards.last())
        .map(|tile| tile.get("tile_id") == last_discard.get("tile_id"))
        .unwrap_or(false)
}

fn apply_selected_claim(
    room: &mut Value,
    seat_index: usize,
    action_type: &str,
    tile_ids: &[String],
) -> Result<Vec<Value>, String> {
    if action_type == "hu" {
        let settlement = standard_compute_hu_settlement(room, seat_index, "discard")?;
        return apply_hu_settlement(room, seat_index, "discard", settlement);
    }
    let plan = plan_selected_claim(room, seat_index, action_type, tile_ids)?;
    apply_legacy_room_mutations(room, &plan.mutations)?;
    sync_pending_timeout(room);
    Ok(plan.events)
}

struct SelectedClaimPlan {
    mutations: Vec<LegacyRoomMutation>,
    events: Vec<Value>,
}

fn plan_selected_claim(
    room: &Value,
    seat_index: usize,
    action_type: &str,
    tile_ids: &[String],
) -> Result<SelectedClaimPlan, String> {
    if action_type != "chow" && action_type != "pung" && action_type != "kong" {
        return Err("invalid_action".to_string());
    }
    validate_claim_selection(room, seat_index, action_type, tile_ids)?;
    let discarder_seat = room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))
        .and_then(|pending| pending.get("discarder_seat"))
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| "invalid_action".to_string())?;
    let last_discard = room
        .get("round_state")
        .and_then(|round| round.get("last_discard"))
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;
    let restricted_tile_key = last_discard.get("tile_key").cloned().unwrap_or(Value::Null);

    if !discarder_latest_discard_matches(room, discarder_seat, &last_discard) {
        return Err("invalid_action".to_string());
    }
    let claimed_tiles = selected_player_tiles(room, seat_index, tile_ids)?;
    let meld = claim_meld_value(action_type, &last_discard, &claimed_tiles);

    let mut mutations = tile_ids
        .iter()
        .cloned()
        .map(
            |tile_id| LegacyRoomMutation::RemovePlayerConcealedTileById {
                seat_index,
                tile_id,
            },
        )
        .collect::<Vec<_>>();
    mutations.push(LegacyRoomMutation::PopPlayerDiscardLast {
        seat_index: discarder_seat,
    });
    mutations.push(LegacyRoomMutation::PushPlayerMeld { seat_index, meld });

    let mut events = vec![round_event_message(
        "claim_made",
        json!({
            "type": "claim_made",
            "seat": seat_index,
            "claim_type": action_type,
            "tile_id": last_discard.get("tile_id").cloned().unwrap_or(Value::Null),
        }),
    )];

    if action_type == "kong" {
        let replacement_tile =
            replacement_tile_from_tail(room).ok_or_else(|| "invalid_action".to_string())?;
        mutations.push(LegacyRoomMutation::RetreatWallTail);
        mutations.push(LegacyRoomMutation::PushPlayerConcealedTile {
            seat_index,
            tile: replacement_tile.clone(),
        });
        mutations.push(LegacyRoomMutation::AppendRoundKongEntry {
            kong_type: "exposed_kong".to_string(),
            actor_seat: seat_index,
            payer_seats: vec![discarder_seat],
            tile_key: last_discard.get("tile_key").cloned().unwrap_or(Value::Null),
        });
        mutations.push(LegacyRoomMutation::SetRoundLastActionContext {
            context: json!({
                "kind": "replacement_draw",
                "seat": seat_index,
                "tile_id": replacement_tile.get("tile_id").cloned().unwrap_or(Value::Null),
                "from_kong_replacement": true,
                "was_last_live_tile": false,
                "was_last_discard": false,
            }),
        });
        events.push(round_event_message(
            "replacement_draw",
            json!({
                "type": "replacement_draw",
                "seat": seat_index,
                "tile_id": replacement_tile.get("tile_id").cloned().unwrap_or(Value::Null),
            }),
        ));
    }

    mutations.push(LegacyRoomMutation::SetRoundCurrentActor { seat_index });
    mutations.push(LegacyRoomMutation::SetRoundLastDiscard { tile: Value::Null });
    mutations.push(LegacyRoomMutation::SetRoundPendingAction {
        pending_action: Value::Null,
    });
    mutations.push(LegacyRoomMutation::SetRoundRestrictedDiscardTileKey {
        tile_key: restricted_tile_key,
    });
    mutations.push(LegacyRoomMutation::IncrementRoundVersion);

    Ok(SelectedClaimPlan { mutations, events })
}

fn claim_meld_value(action_type: &str, last_discard: &Value, claimed_tiles: &[Value]) -> Value {
    if action_type == "chow" {
        let mut tiles = claimed_tiles
            .iter()
            .map(|tile| tile.get("tile_key").cloned().unwrap_or(Value::Null))
            .collect::<Vec<_>>();
        tiles.push(last_discard.get("tile_key").cloned().unwrap_or(Value::Null));
        tiles.sort_by(|left, right| {
            left.as_str()
                .unwrap_or("")
                .cmp(right.as_str().unwrap_or(""))
        });
        return Value::Array(tiles);
    }
    let mut tiles = vec![last_discard.get("tile_key").cloned().unwrap_or(Value::Null)];
    tiles.extend(
        claimed_tiles
            .iter()
            .map(|tile| tile.get("tile_key").cloned().unwrap_or(Value::Null)),
    );
    Value::Array(tiles)
}

fn selected_player_tiles(
    room: &Value,
    seat_index: usize,
    tile_ids: &[String],
) -> Result<Vec<Value>, String> {
    let player_tiles = player_concealed_tiles_slice(room, seat_index)
        .ok_or_else(|| "invalid_action".to_string())?;
    let mut selected = Vec::with_capacity(tile_ids.len());
    let mut used_indices = HashSet::with_capacity(tile_ids.len());
    for tile_id in tile_ids {
        let Some((index, tile)) = player_tiles.iter().enumerate().find(|(index, tile)| {
            !used_indices.contains(index)
                && tile.get("tile_id").and_then(Value::as_str) == Some(tile_id.as_str())
        }) else {
            return Err("invalid_action".to_string());
        };
        used_indices.insert(index);
        selected.push(tile.clone());
    }
    Ok(selected)
}

fn can_resolve_claim_window_timeout_locally(room: &Value) -> bool {
    pending_timeout_kind(room) == Some("claim_window") && claim_window_supported_locally(room)
}

/*

fn available_self_kongs(room: &Value, seat_index: usize) -> Vec<SelfKongCandidate> {
    let cache = RoomScoringCache::from_room(room);
    available_self_kongs_from_cache(&cache, seat_index)
}

fn available_self_kongs_from_cache(
    cache: &RoomScoringCache,
    seat_index: usize,
) -> Vec<SelfKongCandidate> {
    let Some(player) = cache.player(seat_index) else {
        return Vec::new();
    };
    let mut by_key: HashMap<String, Vec<String>> = HashMap::new();
    for tile in &player.concealed_tiles {
        by_key
            .entry(tile.tile_key.clone())
            .or_default()
            .push(tile.tile_id.clone());
    }

    let mut candidates = Vec::new();
    for (tile_key, tile_ids) in &by_key {
        if tile_ids.len() >= 4 {
            candidates.push(SelfKongCandidate {
                kind: SelfKongKind::Concealed,
                tile_ids: tile_ids[0..4].to_vec(),
                tile_key: tile_key.clone(),
                meld_index: None,
            });
        }
    }

    for (meld_index, meld) in player.meld_tile_key_groups.iter().enumerate() {
        if meld.len() == 3 && meld.iter().all(|tile_key| tile_key == &meld[0]) {
            if let Some(tile_ids) = by_key.get(&meld[0]) {
                if let Some(tile_id) = tile_ids.first() {
                    candidates.push(SelfKongCandidate {
                        kind: SelfKongKind::Add,
                        tile_ids: vec![tile_id.clone()],
                        tile_key: meld[0].clone(),
                        meld_index: Some(meld_index),
                    });
                }
            }
        }
    }
    candidates
}

fn resolve_self_kong_selection(
    candidates: &[SelfKongCandidate],
    tile_ids: &[String],
) -> Option<SelfKongCandidate> {
    if tile_ids.is_empty() {
        return None;
    }
    if tile_ids
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len()
        != tile_ids.len()
    {
        return None;
    }
    let mut normalized = tile_ids.to_vec();
    normalized.sort();
    candidates.iter().find_map(|candidate| {
        let mut candidate_ids = candidate.tile_ids.clone();
        candidate_ids.sort();
        if candidate_ids == normalized {
            Some(candidate.clone())
        } else {
            None
        }
    })
}

*/

fn replacement_tile_from_tail(room: &Value) -> Option<Value> {
    let wall = room.get("round_state")?.get("wall")?;
    let head_index = wall.get("head_index")?.as_i64()?;
    let tail_index = wall.get("tail_index")?.as_i64()?;
    if head_index > tail_index {
        return None;
    }
    wall.get("tiles")
        .and_then(Value::as_array)
        .and_then(|tiles| tiles.get(tail_index as usize))
        .cloned()
}

/*

fn seats_with_hu_candidate_for_tile(room: &Value, actor_seat: usize, tile_key: &str) -> Vec<usize> {
    let cache = RoomScoringCache::from_room(room);
    (0..MAX_SEATS)
        .filter(|seat_index| *seat_index != actor_seat)
        .filter(|seat_index| {
            standard_can_declare_hu_with_cache(room, &cache, *seat_index, Some(tile_key), None)
        })
        .collect()
}

*/

fn claim_window_supported_locally(room: &Value) -> bool {
    room.get("round_state")
        .and_then(|round| round.get("pending_action"))
        .and_then(|pending| pending.get("type"))
        .and_then(Value::as_str)
        == Some("claim_window")
}

fn rob_kong_window_supported_locally(room: &Value) -> bool {
    room.get("round_state")
        .and_then(|round| round.get("pending_action"))
        .and_then(|pending| pending.get("type"))
        .and_then(Value::as_str)
        == Some("rob_kong_window")
}

fn resolve_claim_window_timeout(room: &mut Value) -> Result<Vec<Value>, String> {
    let pending_action = room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))
        .cloned()
        .unwrap_or(Value::Null);
    let discarder_seat = pending_action
        .get("discarder_seat")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| "invalid_action".to_string())?;
    let claim_window = pending_action
        .get("claim_window")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut responded = pending_action
        .get("responded_seats")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let unresolved: Vec<Value> = claim_window
        .iter()
        .enumerate()
        .filter(|(_, claims)| {
            claims
                .as_array()
                .map(|items| !items.is_empty())
                .unwrap_or(false)
        })
        .filter(|(seat_index, _)| {
            !responded.iter().any(|value| {
                value
                    .as_u64()
                    .map(|seat| seat as usize == *seat_index)
                    .unwrap_or(false)
            })
        })
        .map(|(seat_index, _)| Value::Number((seat_index as u64).into()))
        .collect();
    responded.extend(unresolved.iter().cloned());
    apply_legacy_room_mutations(
        room,
        &[
            LegacyRoomMutation::SetRoundPendingAction {
                pending_action: json!({
                    "type": "claim_window",
                    "discarder_seat": discarder_seat,
                    "claim_window": claim_window,
                    "responded_seats": responded,
                    "claim_responses": pending_action
                        .get("claim_responses")
                        .cloned()
                        .unwrap_or_else(|| Value::Array(vec![])),
                }),
            },
            LegacyRoomMutation::IncrementRoundVersion,
        ],
    )?;

    let mut messages = vec![round_event_message(
        "claim_auto_passed",
        json!({
            "type": "claim_auto_passed",
            "discarder_seat": discarder_seat,
            "seats": unresolved,
        }),
    )];
    messages.extend(resolve_recorded_claims_local(room)?);
    Ok(messages)
}

fn can_resolve_rob_kong_timeout_locally(room: &Value) -> bool {
    pending_timeout_kind(room) == Some("claim_window") && rob_kong_window_supported_locally(room)
}

fn apply_rob_kong_pass(room: &mut Value, seat_index: usize) -> Result<Vec<Value>, String> {
    let pending = room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;
    if pending.get("type").and_then(Value::as_str) != Some("rob_kong_window") {
        return Err("invalid_action".to_string());
    }
    let offered = pending
        .get("offered_hu_seats")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !offered.iter().any(|value| {
        value
            .as_u64()
            .map(|seat| seat as usize == seat_index)
            .unwrap_or(false)
    }) {
        return Err("invalid_action".to_string());
    }
    let responded = pending
        .get("responded_seats")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if responded.iter().any(|value| {
        value
            .as_u64()
            .map(|seat| seat as usize == seat_index)
            .unwrap_or(false)
    }) {
        return Err("invalid_action".to_string());
    }

    let mut next_responded = responded;
    next_responded.push(Value::Number((seat_index as u64).into()));
    apply_legacy_room_mutations(
        room,
        &[
            LegacyRoomMutation::SetRoundPendingAction {
                pending_action: json!({
                    "type": "rob_kong_window",
                    "actor_seat": pending.get("actor_seat").cloned().unwrap_or(Value::Null),
                    "tile_id": pending.get("tile_id").cloned().unwrap_or(Value::Null),
                    "tile_key": pending.get("tile_key").cloned().unwrap_or(Value::Null),
                    "meld_index": pending.get("meld_index").cloned().unwrap_or(Value::Null),
                    "offered_hu_seats": offered,
                    "responded_seats": next_responded.clone(),
                }),
            },
            LegacyRoomMutation::IncrementRoundVersion,
        ],
    )?;

    let unresolved = offered
        .iter()
        .filter_map(|value| value.as_u64().map(|seat| seat as usize))
        .filter(|offered_seat| {
            !next_responded.iter().any(|value| {
                value
                    .as_u64()
                    .map(|seat| seat as usize == *offered_seat)
                    .unwrap_or(false)
            })
        })
        .collect::<Vec<_>>();
    if !unresolved.is_empty() {
        sync_pending_timeout(room);
        return Ok(vec![]);
    }
    complete_add_kong_after_passes(room)
}

fn resolve_rob_kong_timeout(room: &mut Value) -> Result<Vec<Value>, String> {
    let pending = room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;
    if pending.get("type").and_then(Value::as_str) != Some("rob_kong_window") {
        return Err("invalid_action".to_string());
    }
    let actor_seat = pending
        .get("actor_seat")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| "invalid_action".to_string())?;
    let offered = pending
        .get("offered_hu_seats")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let responded = pending
        .get("responded_seats")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let unresolved: Vec<Value> = offered
        .iter()
        .filter(|seat| !responded.iter().any(|value| value == *seat))
        .cloned()
        .collect();
    let mut next_responded = responded;
    next_responded.extend(unresolved.iter().cloned());
    apply_legacy_room_mutations(
        room,
        &[
            LegacyRoomMutation::SetRoundPendingAction {
                pending_action: json!({
                    "type": "rob_kong_window",
                    "actor_seat": actor_seat,
                    "tile_id": pending.get("tile_id").cloned().unwrap_or(Value::Null),
                    "tile_key": pending.get("tile_key").cloned().unwrap_or(Value::Null),
                    "meld_index": pending.get("meld_index").cloned().unwrap_or(Value::Null),
                    "offered_hu_seats": offered,
                    "responded_seats": next_responded,
                }),
            },
            LegacyRoomMutation::IncrementRoundVersion,
        ],
    )?;
    let mut messages = vec![round_event_message(
        "rob_kong_auto_passed",
        json!({
            "type": "rob_kong_auto_passed",
            "actor_seat": actor_seat,
            "seats": unresolved,
        }),
    )];
    messages.extend(complete_add_kong_after_passes(room)?);
    Ok(messages)
}

fn settle_exhaustive_draw_local(room: &mut Value) -> Vec<Value> {
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
            "round_id": room.get("round_state").and_then(|round| round.get("round_id")).cloned().unwrap_or(Value::Null),
        }),
    )]
}

fn apply_settlement_to_match(room: &mut Value) {
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

fn current_continue_action_id(room: &Value) -> Option<&'static str> {
    match room.get("phase").and_then(Value::as_str) {
        Some("settlement") => Some("start_next_round"),
        Some("finished") => Some("restart_match"),
        _ => None,
    }
}

fn continue_required_human_seats(room: &Value) -> Vec<usize> {
    room.get("seats")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|seat| !seat.get("is_bot").and_then(Value::as_bool).unwrap_or(false))
        .filter_map(|seat| {
            seat.get("seat_index")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
        })
        .collect()
}

fn continue_online_human_seats(room: &Value) -> Vec<usize> {
    room.get("seats")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|seat| {
            seat.get("connected")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .filter(|seat| !seat.get("is_bot").and_then(Value::as_bool).unwrap_or(false))
        .filter_map(|seat| {
            seat.get("seat_index")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
        })
        .collect()
}

fn current_confirmed_continue_seats(room: &Value, action_id: &str) -> Vec<usize> {
    let field = if action_id == "start_next_round" {
        "start_next_round_confirmed_seats"
    } else {
        "restart_match_confirmed_seats"
    };
    room.get(field)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|seat| seat.as_u64().map(|value| value as usize))
        .collect()
}

fn continue_all_occupied_seats(room: &Value) -> Vec<usize> {
    room.get("seats")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|seat| {
            seat.get("seat_index")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
        })
        .collect()
}

fn reconcile_continue_action(room: &mut Value) -> Result<(), String> {
    let Some(action_id) = current_continue_action_id(room) else {
        apply_legacy_room_mutations(
            room,
            &[LegacyRoomMutation::SetRoomField {
                key: "continue_action_auto_advance_deadline_at".to_string(),
                value: Value::Null,
            }],
        )?;
        return Ok(());
    };
    let required = continue_required_human_seats(room);
    let confirmed = current_confirmed_continue_seats(room, action_id);
    let online = continue_online_human_seats(room);

    if required.iter().all(|seat| confirmed.contains(seat)) {
        complete_continue_action(room, action_id)?;
        return Ok(());
    }

    let online_unconfirmed = online
        .iter()
        .filter(|seat| !confirmed.contains(seat))
        .copied()
        .collect::<Vec<_>>();
    if !online_unconfirmed.is_empty() {
        apply_legacy_room_mutations(
            room,
            &[LegacyRoomMutation::SetRoomField {
                key: "continue_action_auto_advance_deadline_at".to_string(),
                value: Value::Null,
            }],
        )?;
        return Ok(());
    }

    let offline_unconfirmed = required
        .iter()
        .filter(|seat| !online.contains(seat) && !confirmed.contains(seat))
        .copied()
        .collect::<Vec<_>>();
    if offline_unconfirmed.is_empty() {
        complete_continue_action(room, action_id)?;
        return Ok(());
    }

    if room
        .get("continue_action_auto_advance_deadline_at")
        .is_none()
        || room
            .get("continue_action_auto_advance_deadline_at")
            .is_some_and(Value::is_null)
    {
        apply_legacy_room_mutations(
            room,
            &[LegacyRoomMutation::SetRoomField {
                key: "continue_action_auto_advance_deadline_at".to_string(),
                value: Value::String(
                    (Utc::now() + chrono::TimeDelta::seconds(CONTINUE_ACTION_AUTO_ADVANCE_SECONDS))
                        .to_rfc3339_opts(SecondsFormat::Micros, true),
                ),
            }],
        )?;
    }
    Ok(())
}

fn complete_continue_action(room: &mut Value, action_id: &str) -> Result<(), String> {
    apply_legacy_room_mutations(
        room,
        &[
            LegacyRoomMutation::SetRoomField {
                key: "continue_action_auto_advance_deadline_at".to_string(),
                value: Value::Null,
            },
            LegacyRoomMutation::SetRoomField {
                key: "start_next_round_confirmed_seats".to_string(),
                value: Value::Array(vec![]),
            },
            LegacyRoomMutation::SetRoomField {
                key: "restart_match_confirmed_seats".to_string(),
                value: Value::Array(vec![]),
            },
        ],
    )?;
    match action_id {
        "start_next_round" => complete_start_next_round(room),
        "restart_match" => {
            let occupied = continue_all_occupied_seats(room);
            if occupied.is_empty() {
                return Err("invalid_action".to_string());
            }
            let mut rng = rand::rng();
            let dealer_index = rng.random_range(0..occupied.len());
            start_match(room, occupied[dealer_index], rand::random::<u64>());
            Ok(())
        }
        _ => Err("invalid_action".to_string()),
    }
}

fn complete_start_next_round(room: &mut Value) -> Result<(), String> {
    apply_settlement_to_match(room);
    let match_state = room
        .get("match_state")
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;
    let prevailing_wind = match_state
        .get("prevailing_wind")
        .and_then(Value::as_str)
        .unwrap_or("east");
    let hand_number = match_state
        .get("hand_number")
        .and_then(Value::as_i64)
        .unwrap_or(1) as usize;
    let dealer_seat = match_state
        .get("dealer_seat")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let current_wind_index = WIND_ORDER
        .iter()
        .position(|wind| *wind == prevailing_wind)
        .unwrap_or(0);
    let next_dealer = (dealer_seat + 1) % MAX_SEATS;
    let mut next_hand_number = hand_number + 1;
    let mut next_wind = prevailing_wind.to_string();
    let mut match_finished = false;
    if next_hand_number > MAX_SEATS {
        next_hand_number = 1;
        if current_wind_index == WIND_ORDER.len() - 1 {
            match_finished = true;
        } else {
            next_wind = WIND_ORDER[current_wind_index + 1].to_string();
        }
    }

    apply_legacy_room_mutations(
        room,
        &[
            LegacyRoomMutation::SetMatchField {
                key: "prevailing_wind".to_string(),
                value: Value::String(next_wind.clone()),
            },
            LegacyRoomMutation::SetMatchField {
                key: "hand_number".to_string(),
                value: Value::Number(
                    (if match_finished {
                        hand_number
                    } else {
                        next_hand_number
                    } as u64)
                        .into(),
                ),
            },
            LegacyRoomMutation::SetMatchField {
                key: "dealer_seat".to_string(),
                value: Value::Number(
                    (if match_finished {
                        dealer_seat
                    } else {
                        next_dealer
                    } as u64)
                        .into(),
                ),
            },
            LegacyRoomMutation::SetMatchField {
                key: "match_finished".to_string(),
                value: Value::Bool(match_finished),
            },
        ],
    )?;

    if match_finished {
        apply_legacy_room_mutations(
            room,
            &[
                LegacyRoomMutation::SetRoomField {
                    key: "phase".to_string(),
                    value: Value::String("finished".to_string()),
                },
                LegacyRoomMutation::SetRoomField {
                    key: "pending_timeout".to_string(),
                    value: Value::Null,
                },
            ],
        )?;
        return Ok(());
    }
    let enforce = room
        .get("enforce_minimum_eight_fan")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let round_id = format!(
        "{next_wind}-{next_hand_number}-dealer-{next_dealer}-{}",
        rand::random::<u64>()
    );
    start_round(
        room,
        next_dealer,
        &next_wind,
        round_id,
        enforce,
        rand::random::<u64>(),
    );
    Ok(())
}

fn complete_add_kong_after_passes(room: &mut Value) -> Result<Vec<Value>, String> {
    let pending = room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;
    let actor_seat = pending
        .get("actor_seat")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| "invalid_action".to_string())?;
    let tile_id = pending
        .get("tile_id")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| "invalid_action".to_string())?;
    let tile_key = pending
        .get("tile_key")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| "invalid_action".to_string())?;
    let meld_index = pending
        .get("meld_index")
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    let replacement_tile =
        replacement_tile_from_tail(room).ok_or_else(|| "invalid_action".to_string())?;
    let selection = SelfKongCandidate {
        kind: SelfKongKind::Add,
        tile_ids: vec![tile_id],
        tile_key,
        meld_index,
    };
    let plan = plan_self_kong_completion(room, actor_seat, &selection, replacement_tile, true)?;
    apply_legacy_room_mutations(room, &plan.mutations)?;
    sync_pending_timeout(room);
    Ok(plan.events)
}

fn match_result_message(room: &Value) -> Option<Value> {
    let round_state = room.get("round_state")?;
    if round_state.get("phase").and_then(Value::as_str) != Some("settlement") {
        return None;
    }
    let settlement = round_state.get("settlement")?.clone();
    let mut payload = settlement.as_object().cloned().unwrap_or_default();
    payload.insert(
        "table_code".to_string(),
        room.get("table_code").cloned().unwrap_or(Value::Null),
    );
    payload.insert(
        "round_id".to_string(),
        round_state.get("round_id").cloned().unwrap_or(Value::Null),
    );
    payload.insert("phase".to_string(), Value::String("settlement".to_string()));
    Some(json!({
        "type": "match_result",
        "payload": Value::Object(payload),
    }))
}

fn sync_pending_timeout(room: &mut Value) {
    let pending_timeout = project_room_state(room)
        .ok()
        .and_then(|state| compute_pending_timeout_value(&state, deadline_iso()))
        .and_then(|timeout| serde_json::to_value(timeout).ok())
        .unwrap_or(Value::Null);
    let _ = apply_legacy_room_mutations(
        room,
        &[LegacyRoomMutation::SetRoomField {
            key: "pending_timeout".to_string(),
            value: pending_timeout,
        }],
    );
}

fn round_event_message(event_type: &str, event: Value) -> Value {
    json!({
        "type": "round_event",
        "payload": {
            "event_type": event_type,
            "event": event,
        }
    })
}

fn current_actor(room: &Value) -> Option<usize> {
    room.get("round_state")
        .and_then(|round| round.get("current_actor"))
        .and_then(Value::as_u64)
        .map(|value| value as usize)
}

fn pending_timeout_kind(room: &Value) -> Option<&str> {
    room.get("pending_timeout")
        .and_then(|timeout| timeout.get("kind"))
        .and_then(Value::as_str)
}

fn restricted_same_turn_discard_tile_ids(room: &Value, seat_index: usize) -> Vec<Value> {
    let Some(restricted_tile_key) = room
        .get("round_state")
        .and_then(|round| round.get("restricted_discard_tile_key"))
        .and_then(Value::as_str)
    else {
        return Vec::new();
    };

    player_concealed_tiles_slice(room, seat_index)
        .into_iter()
        .flatten()
        .filter(|tile| tile.get("tile_key").and_then(Value::as_str) == Some(restricted_tile_key))
        .filter_map(|tile| tile.get("tile_id").and_then(Value::as_str))
        .map(|tile_id| Value::String(tile_id.to_string()))
        .collect()
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

fn player_first_flower_tile_id_from_cache(
    cache: &RoomScoringCache,
    seat_index: usize,
) -> Option<String> {
    cache
        .player(seat_index)?
        .concealed_tiles
        .iter()
        .find(|tile| tile.kind == "flower")
        .map(|tile| tile.tile_id.clone())
}

fn is_last_live_tile_point(room: &Value) -> bool {
    room.get("round_state")
        .and_then(|round| round.get("last_action_context"))
        .map(|context| {
            context.get("kind").and_then(Value::as_str) == Some("draw")
                && context
                    .get("was_last_live_tile")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                && !context
                    .get("from_kong_replacement")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
        })
        .unwrap_or(false)
}

fn can_resolve_discard_locally(room: &Value, seat_index: usize, tile_id: &str) -> bool {
    if room.get("phase").and_then(Value::as_str) != Some("playing") {
        return false;
    }
    if current_actor(room) != Some(seat_index) {
        return false;
    }
    if room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))
        .filter(|value| !value.is_null())
        .is_some()
    {
        return false;
    }
    if wall_live_tiles_remaining(room) <= 0 {
        return false;
    }

    let Some(discarded_tile) = player_concealed_tile(room, seat_index, tile_id) else {
        return false;
    };
    if let Some(restricted) = room
        .get("round_state")
        .and_then(|round| round.get("restricted_discard_tile_key"))
        .and_then(Value::as_str)
    {
        if discarded_tile.get("tile_key").and_then(Value::as_str) == Some(restricted) {
            return false;
        }
    }

    discarded_tile.get("tile_id").is_some()
}

fn wall_live_tiles_remaining(room: &Value) -> i64 {
    room.get("round_state")
        .and_then(|round| round.get("wall"))
        .and_then(|wall| {
            let head = wall.get("head_index")?.as_i64()?;
            let tail = wall.get("tail_index")?.as_i64()?;
            Some((tail - head + 1).max(0))
        })
        .unwrap_or(0)
}

fn player_concealed_tiles_slice(room: &Value, seat_index: usize) -> Option<&[Value]> {
    room.get("round_state")
        .and_then(|round| round.get("players"))
        .and_then(Value::as_array)
        .and_then(|players| players.get(seat_index))
        .and_then(|player| player.get("concealed_tiles"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
}

fn player_concealed_tile<'a>(
    room: &'a Value,
    seat_index: usize,
    tile_id: &str,
) -> Option<&'a Value> {
    player_concealed_tiles_slice(room, seat_index)?
        .iter()
        .find(|tile| tile.get("tile_id").and_then(Value::as_str) == Some(tile_id))
}

fn last_concealed_tile_id(room: &Value, seat_index: usize) -> Option<String> {
    let cache = RoomScoringCache::from_room(room);
    last_concealed_tile_id_from_cache(&cache, seat_index)
}

fn last_concealed_tile_id_from_cache(
    cache: &RoomScoringCache,
    seat_index: usize,
) -> Option<String> {
    cache
        .player(seat_index)?
        .concealed_tiles
        .last()
        .map(|tile| tile.tile_id.clone())
}

/*

fn tile_index(tile_key: &str) -> Option<usize> {
    match tile_key {
        "east" => Some(27),
        "south" => Some(28),
        "west" => Some(29),
        "north" => Some(30),
        "red" => Some(31),
        "green" => Some(32),
        "white" => Some(33),
        _ => {
            let bytes = tile_key.as_bytes();
            if bytes.len() != 2 {
                return None;
            }
            let suit_offset = match bytes[0] {
                b'w' => 0,
                b't' => 9,
                b'b' => 18,
                _ => return None,
            };
            let rank = usize::from(bytes[1].checked_sub(b'0')?);
            if !(1..=9).contains(&rank) {
                return None;
            }
            Some(suit_offset + rank - 1)
        }
    }
}

fn suited_tile_components(tile_index: usize) -> Option<(usize, usize)> {
    if tile_index >= HONOR_TILE_START {
        return None;
    }
    Some((tile_index / 9, (tile_index % 9) + 1))
}

fn chow_required_tile_pairs(tile_index: usize) -> Vec<(usize, usize)> {
    let Some((_, rank)) = suited_tile_components(tile_index) else {
        return Vec::new();
    };
    let mut pairs = Vec::with_capacity(3);
    if rank >= 3 {
        pairs.push((tile_index - 2, tile_index - 1));
    }
    if (2..=8).contains(&rank) {
        pairs.push((tile_index - 1, tile_index + 1));
    }
    if rank <= 7 {
        pairs.push((tile_index + 1, tile_index + 2));
    }
    pairs
}

fn compute_claim_window_without_hu(
    room: &Value,
    discarder_seat: usize,
    discarded_tile: &Value,
) -> Vec<Value> {
    let discarded_tile_key = discarded_tile
        .get("tile_key")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let ltw_after_discard = is_last_tile_wall_point_after_discard(room);
    let next_player = (discarder_seat + 1) % MAX_SEATS;
    let scoring_cache = RoomScoringCache::from_room(room);

    (0..MAX_SEATS)
        .map(|seat_index| {
            if seat_index == discarder_seat {
                return Value::Array(vec![]);
            }
            let counts = scoring_cache
                .player(seat_index)
                .map(|player| player.concealed_tile_counts)
                .unwrap_or([0; TILE_KIND_COUNT]);
            let mut claims = Vec::new();
            if !ltw_after_discard {
                let same_tile_count = tile_index(&discarded_tile_key)
                    .map(|tile_index| counts[tile_index])
                    .unwrap_or(0);
                if same_tile_count >= 2 {
                    claims.push(Value::String("pung".to_string()));
                }
                if same_tile_count >= 3 {
                    claims.push(Value::String("kong".to_string()));
                }
                if seat_index == next_player && can_chow(&discarded_tile_key, &counts) {
                    claims.push(Value::String("chow".to_string()));
                }
            }
            if standard_can_declare_hu_with_cache(
                room,
                &scoring_cache,
                seat_index,
                Some(&discarded_tile_key),
                None,
            ) {
                claims.push(Value::String("hu".to_string()));
            }
            Value::Array(claims)
        })
        .collect()
}

fn can_chow(discarded_tile_key: &str, counts: &TileCounts) -> bool {
    let Some(discard_index) = tile_index(discarded_tile_key) else {
        return false;
    };
    for (left_index, right_index) in chow_required_tile_pairs(discard_index) {
        if counts[left_index] > 0 && counts[right_index] > 0 {
            return true;
        }
    }
    false
}

fn is_last_tile_wall_point_after_discard(room: &Value) -> bool {
    room.get("round_state")
        .and_then(|round| round.get("last_action_context"))
        .map(|context| {
            context.get("kind").and_then(Value::as_str) == Some("discard")
                && context
                    .get("was_last_discard")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
        })
        .unwrap_or(false)
}

*/

fn choose_bot_active_turn_action_with_cache(
    room: &Value,
    cache: &RoomScoringCache,
    seat_index: usize,
) -> Option<BotAction> {
    let self_kong_candidates = available_self_kongs_from_cache(cache, seat_index);
    let add_kong_risk_tiles = self_kong_candidates
        .iter()
        .filter(|candidate| candidate.kind == SelfKongKind::Add)
        .filter(|candidate| {
            !seats_with_hu_candidate_for_tile(room, seat_index, &candidate.tile_key).is_empty()
        })
        .map(|candidate| candidate.tile_key.clone())
        .collect::<HashSet<_>>();
    let bot_context = build_bot_context_view(
        cache,
        seat_index,
        Vec::new(),
        self_kong_candidates,
        add_kong_risk_tiles,
    )?;
    bot::choose_active_turn_action(&bot_context)
}

fn choose_bot_claim_action_with_cache(
    room: &Value,
    cache: &RoomScoringCache,
    seat_index: usize,
) -> Option<BotAction> {
    let pending_action = room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))?;
    if claim_window_offers_claim(pending_action, seat_index, "hu") {
        return Some(BotAction {
            seat_index,
            action_type: "hu".to_string(),
            tile_ids: vec![],
        });
    }
    let claim_options = ["kong", "pung", "chow"]
        .into_iter()
        .filter(|claim_type| claim_window_offers_claim(pending_action, seat_index, claim_type))
        .flat_map(|claim_type| {
            claim_tile_id_options(cache, seat_index, claim_type)
                .into_iter()
                .map(move |tile_ids| BotClaimOption {
                    action_type: claim_type.to_string(),
                    tile_ids,
                })
        })
        .collect::<Vec<_>>();
    let bot_context =
        build_bot_context_view(cache, seat_index, claim_options, Vec::new(), HashSet::new())?;
    bot::choose_claim_action(&bot_context)
}

fn project_room_state(room: &Value) -> Result<RoomState, String> {
    RoomState::from_legacy_value(room).map_err(|error| error.to_string())
}

fn seat_projection_support(room: &Value, local_seat: usize) -> SeatProjectionSupport {
    let has_concealed_flower = room
        .get("round_state")
        .map(|round_state| player_has_concealed_flower(round_state, local_seat))
        .unwrap_or(false);
    let has_self_kong =
        room.get("round_state").is_some() && !available_self_kongs(room, local_seat).is_empty();
    let can_hu =
        room.get("round_state").is_some() && standard_can_declare_hu(room, local_seat, None, None);
    let restricted_discard_tile_ids = restricted_same_turn_discard_tile_ids(room, local_seat)
        .into_iter()
        .filter_map(|value| value.as_str().map(ToString::to_string))
        .collect();
    SeatProjectionSupport {
        has_concealed_flower,
        has_self_kong,
        can_hu,
        restricted_discard_tile_ids,
    }
}

fn player_has_concealed_flower(round_state: &Value, seat_index: usize) -> bool {
    round_state
        .get("players")
        .and_then(Value::as_array)
        .and_then(|players| players.get(seat_index))
        .and_then(|player| player.get("concealed_tiles"))
        .and_then(Value::as_array)
        .map(|tiles| {
            tiles
                .iter()
                .any(|tile| tile.get("kind").and_then(Value::as_str) == Some("flower"))
        })
        .unwrap_or(false)
}

fn deadline_iso() -> String {
    (Utc::now() + chrono::TimeDelta::seconds(ACTIVE_TURN_TIMEOUT_SECONDS))
        .to_rfc3339_opts(SecondsFormat::Micros, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::action::{GameCommand, PlayerAction};
    use crate::core::event::GameEvent;

    fn tile(tile_key: &str, tile_id: &str, kind: &str) -> Value {
        json!({
            "tile_id": tile_id,
            "tile_key": tile_key,
            "kind": kind,
            "suit": if kind == "suit" {
                if tile_key.starts_with('w') {
                    Value::String("characters".to_string())
                } else if tile_key.starts_with('t') {
                    Value::String("bamboos".to_string())
                } else {
                    Value::String("dots".to_string())
                }
            } else {
                Value::Null
            },
            "rank": if kind == "suit" {
                tile_key[1..].parse::<i32>().ok().map(Value::from).unwrap_or(Value::Null)
            } else {
                Value::Null
            },
            "name": tile_key,
        })
    }

    fn suit(tile_key: &str, tile_id: &str) -> Value {
        tile(tile_key, tile_id, "suit")
    }

    fn wind(tile_key: &str, tile_id: &str) -> Value {
        tile(tile_key, tile_id, "wind")
    }

    fn room_for_local_discard() -> Value {
        json!({
            "table_code": "ROOM1",
            "phase": "playing",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "start_next_round_confirmed_seats": [],
            "restart_match_confirmed_seats": [],
            "continue_action_auto_advance_deadline_at": null,
            "seats": [
                {"seat_index": 0, "nickname": "P0", "reconnect_token": "t0", "player_session_id": 1, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null},
                {"seat_index": 1, "nickname": "P1", "reconnect_token": "t1", "player_session_id": 2, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null},
                {"seat_index": 2, "nickname": "P2", "reconnect_token": "t2", "player_session_id": 3, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null},
                {"seat_index": 3, "nickname": "P3", "reconnect_token": "t3", "player_session_id": 4, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null}
            ],
            "match_state": {
                "prevailing_wind": "east",
                "hand_number": 1,
                "dealer_seat": 0,
                "cumulative_scores": {"0": 0, "1": 0, "2": 0, "3": 0},
                "match_finished": false,
                "last_completed_round_id": null
            },
            "round_state": {
                "round_id": "east-1-dealer-0-test",
                "dealer_seat": 0,
                "current_actor": 0,
                "wall": {
                    "tiles": [
                        suit("w1", "w1#draw"),
                        suit("b9", "b9#tail")
                    ],
                    "head_index": 0,
                    "tail_index": 1
                },
                "players": [
                    {
                        "seat": 0,
                        "concealed_tiles": [
                            wind("east", "east#discard"),
                            suit("w2", "w2#a"), suit("w3", "w3#a"), suit("w4", "w4#a"),
                            suit("t2", "t2#a"), suit("t3", "t3#a"), suit("t4", "t4#a"),
                            suit("b2", "b2#a"), suit("b3", "b3#a"), suit("b4", "b4#a"),
                            suit("w6", "w6#a"), suit("w7", "w7#a"), suit("w8", "w8#a"), suit("b7", "b7#a")
                        ],
                        "melds": [],
                        "flowers": [],
                        "discards": []
                    },
                    {
                        "seat": 1,
                        "concealed_tiles": [
                            suit("w1", "w1#1"), suit("w2", "w2#1"), suit("w3", "w3#1"),
                            suit("t1", "t1#1"), suit("t2", "t2#1"), suit("t3", "t3#1"),
                            suit("b1", "b1#1"), suit("b2", "b2#1"), suit("b3", "b3#1"),
                            suit("w5", "w5#1"), suit("w6", "w6#1"), suit("t6", "t6#1"), suit("b6", "b6#1")
                        ],
                        "melds": [],
                        "flowers": [],
                        "discards": []
                    },
                    {
                        "seat": 2,
                        "concealed_tiles": [
                            suit("w1", "w1#2"), suit("w2", "w2#2"), suit("w5", "w5#2"),
                            suit("t1", "t1#2"), suit("t4", "t4#2"), suit("t7", "t7#2"),
                            suit("b1", "b1#2"), suit("b4", "b4#2"), suit("b7", "b7#2"),
                            suit("w9", "w9#2"), suit("t9", "t9#2"), suit("b9", "b9#2"), wind("south", "south#2")
                        ],
                        "melds": [],
                        "flowers": [],
                        "discards": []
                    },
                    {
                        "seat": 3,
                        "concealed_tiles": [
                            suit("w3", "w3#3"), suit("w5", "w5#3"), suit("w7", "w7#3"),
                            suit("t3", "t3#3"), suit("t5", "t5#3"), suit("t7", "t7#3"),
                            suit("b3", "b3#3"), suit("b5", "b5#3"), suit("b7", "b7#3"),
                            suit("w9", "w9#3"), suit("t9", "t9#3"), suit("b9", "b9#3"), wind("north", "north#3")
                        ],
                        "melds": [],
                        "flowers": [],
                        "discards": []
                    }
                ],
                "last_discard": null,
                "pending_action": null,
                "phase": "playing",
                "settlement": null,
                "version": 1,
                "score_trackers": {"kong_entries": [], "opening_flowers_completed": true},
                "last_action_context": {
                    "kind": "draw",
                    "seat": 0,
                    "tile_id": "east#discard",
                    "from_kong_replacement": false,
                    "was_last_live_tile": false,
                    "was_last_discard": false
                },
                "round_wind": "east",
                "enforce_minimum_eight_fan": true,
                "restricted_discard_tile_key": null
            },
            "pending_timeout": {
                "kind": "active_turn",
                "seat_index": 0,
                "deadline_at": deadline_iso(),
                "drawn_tile_id": "east#discard"
            }
        })
    }

    fn room_for_local_claim_window() -> Value {
        json!({
            "table_code": "ROOM2",
            "phase": "playing",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "start_next_round_confirmed_seats": [],
            "restart_match_confirmed_seats": [],
            "continue_action_auto_advance_deadline_at": null,
            "seats": [
                {"seat_index": 0, "nickname": "P0", "reconnect_token": "t0", "player_session_id": 1, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null},
                {"seat_index": 1, "nickname": "P1", "reconnect_token": "t1", "player_session_id": 2, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null},
                {"seat_index": 2, "nickname": "P2", "reconnect_token": "t2", "player_session_id": 3, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null},
                {"seat_index": 3, "nickname": "P3", "reconnect_token": "t3", "player_session_id": 4, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null}
            ],
            "match_state": {
                "prevailing_wind": "east",
                "hand_number": 1,
                "dealer_seat": 0,
                "cumulative_scores": {"0": 0, "1": 0, "2": 0, "3": 0},
                "match_finished": false,
                "last_completed_round_id": null
            },
            "round_state": {
                "round_id": "east-1-dealer-0-claim",
                "dealer_seat": 0,
                "current_actor": 0,
                "wall": {
                    "tiles": [suit("w9", "w9#draw")],
                    "head_index": 0,
                    "tail_index": 0
                },
                "players": [
                    {
                        "seat": 0,
                        "concealed_tiles": [
                            suit("w3", "w3#discard"),
                            suit("w2", "w2#a"), suit("w4", "w4#a"), suit("t2", "t2#a"),
                            suit("t3", "t3#a"), suit("t4", "t4#a"), suit("b2", "b2#a"),
                            suit("b3", "b3#a"), suit("b4", "b4#a"), suit("w6", "w6#a"),
                            suit("w7", "w7#a"), suit("w8", "w8#a"), suit("b7", "b7#a"), suit("b8", "b8#a")
                        ],
                        "melds": [],
                        "flowers": [],
                        "discards": []
                    },
                    {
                        "seat": 1,
                        "concealed_tiles": [
                            suit("w1", "w1#1"), suit("w2", "w2#1"), suit("w4", "w4#1"),
                            suit("t1", "t1#1"), suit("t2", "t2#1"), suit("t3", "t3#1"),
                            suit("b1", "b1#1"), suit("b2", "b2#1"), suit("b3", "b3#1"),
                            suit("w5", "w5#1"), suit("w6", "w6#1"), suit("t6", "t6#1"), suit("b6", "b6#1")
                        ],
                        "melds": [],
                        "flowers": [],
                        "discards": []
                    },
                    {
                        "seat": 2,
                        "concealed_tiles": [
                            suit("w3", "w3#2a"), suit("w3", "w3#2b"),
                            suit("t1", "t1#2"), suit("t4", "t4#2"), suit("t7", "t7#2"),
                            suit("b1", "b1#2"), suit("b4", "b4#2"), suit("b7", "b7#2"),
                            suit("w9", "w9#2"), suit("t9", "t9#2"), suit("b9", "b9#2"), wind("south", "south#2"), wind("north", "north#2")
                        ],
                        "melds": [],
                        "flowers": [],
                        "discards": []
                    },
                    {
                        "seat": 3,
                        "concealed_tiles": [
                            suit("w1", "w1#3"), suit("w5", "w5#3"), suit("w7", "w7#3"),
                            suit("t3", "t3#3"), suit("t5", "t5#3"), suit("t7", "t7#3"),
                            suit("b3", "b3#3"), suit("b5", "b5#3"), suit("b7", "b7#3"),
                            suit("w9", "w9#3"), suit("t9", "t9#3"), suit("b9", "b9#3"), wind("north", "north#3")
                        ],
                        "melds": [],
                        "flowers": [],
                        "discards": []
                    }
                ],
                "last_discard": null,
                "pending_action": null,
                "phase": "playing",
                "settlement": null,
                "version": 1,
                "score_trackers": {"kong_entries": [], "opening_flowers_completed": true},
                "last_action_context": {
                    "kind": "draw",
                    "seat": 0,
                    "tile_id": "w3#discard",
                    "from_kong_replacement": false,
                    "was_last_live_tile": false,
                    "was_last_discard": false
                },
                "round_wind": "east",
                "enforce_minimum_eight_fan": true,
                "restricted_discard_tile_key": null
            },
            "pending_timeout": {
                "kind": "active_turn",
                "seat_index": 0,
                "deadline_at": deadline_iso(),
                "drawn_tile_id": "w3#discard"
            }
        })
    }

    fn room_for_bot_active_turn() -> Value {
        let mut room = room_for_local_discard();
        room["seats"][0]["is_bot"] = json!(true);
        room["seats"][0]["seat_type"] = json!("bot");
        room
    }

    fn room_for_bot_shape_choice() -> Value {
        let mut room = room_for_bot_active_turn();
        room["round_state"]["players"][0]["concealed_tiles"] = json!([
            suit("w1", "w1#a"),
            suit("w2", "w2#a"),
            suit("w3", "w3#a"),
            suit("t1", "t1#a"),
            suit("t2", "t2#a"),
            suit("t3", "t3#a"),
            suit("b1", "b1#a"),
            suit("b2", "b2#a"),
            suit("b3", "b3#a"),
            suit("w5", "w5#p1"),
            suit("w5", "w5#p2"),
            suit("w6", "w6#shape"),
            wind("east", "east#isolated"),
            suit("w7", "w7#draw")
        ]);
        room["round_state"]["last_action_context"]["tile_id"] = json!("w7#draw");
        room["pending_timeout"]["drawn_tile_id"] = json!("w7#draw");
        room
    }

    fn room_for_local_kong_claim_window() -> Value {
        let mut room = room_for_local_claim_window();
        room["round_state"]["players"][2]["concealed_tiles"] = json!([
            suit("w3", "w3#2a"),
            suit("w3", "w3#2b"),
            suit("w3", "w3#2c"),
            suit("t1", "t1#2"),
            suit("t4", "t4#2"),
            suit("t7", "t7#2"),
            suit("b1", "b1#2"),
            suit("b4", "b4#2"),
            suit("b7", "b7#2"),
            suit("w9", "w9#2"),
            suit("t9", "t9#2"),
            suit("b9", "b9#2"),
            wind("south", "south#2")
        ]);
        room
    }

    fn room_for_local_concealed_self_kong() -> Value {
        json!({
            "table_code": "ROOM3",
            "phase": "playing",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "start_next_round_confirmed_seats": [],
            "restart_match_confirmed_seats": [],
            "continue_action_auto_advance_deadline_at": null,
            "seats": [
                {"seat_index": 0, "nickname": "P0", "reconnect_token": "t0", "player_session_id": 1, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null},
                {"seat_index": 1, "nickname": "P1", "reconnect_token": "t1", "player_session_id": 2, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null},
                {"seat_index": 2, "nickname": "P2", "reconnect_token": "t2", "player_session_id": 3, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null},
                {"seat_index": 3, "nickname": "P3", "reconnect_token": "t3", "player_session_id": 4, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null}
            ],
            "match_state": {
                "prevailing_wind": "east",
                "hand_number": 1,
                "dealer_seat": 0,
                "cumulative_scores": {"0": 0, "1": 0, "2": 0, "3": 0},
                "match_finished": false,
                "last_completed_round_id": null
            },
            "round_state": {
                "round_id": "east-1-dealer-0-selfkong",
                "dealer_seat": 0,
                "current_actor": 0,
                "wall": {
                    "tiles": [suit("b9", "b9#replacement")],
                    "head_index": 0,
                    "tail_index": 0
                },
                "players": [
                    {
                        "seat": 0,
                        "concealed_tiles": [
                            suit("t5", "t5#1"), suit("t5", "t5#2"), suit("t5", "t5#3"), suit("t5", "t5#4"),
                            suit("w2", "w2#a"), suit("w3", "w3#a"), suit("w4", "w4#a"),
                            suit("t2", "t2#a"), suit("t3", "t3#a"), suit("t4", "t4#a"),
                            suit("b2", "b2#a"), suit("b3", "b3#a"), suit("b4", "b4#a"), suit("w6", "w6#a")
                        ],
                        "melds": [],
                        "flowers": [],
                        "discards": []
                    },
                    {
                        "seat": 1,
                        "concealed_tiles": [suit("w1", "w1#1"), suit("w2", "w2#1"), suit("w3", "w3#1")],
                        "melds": [],
                        "flowers": [],
                        "discards": []
                    },
                    {"seat": 2, "concealed_tiles": [], "melds": [], "flowers": [], "discards": []},
                    {"seat": 3, "concealed_tiles": [], "melds": [], "flowers": [], "discards": []}
                ],
                "last_discard": null,
                "pending_action": null,
                "phase": "playing",
                "settlement": null,
                "version": 1,
                "score_trackers": {"kong_entries": [], "opening_flowers_completed": true},
                "last_action_context": {
                    "kind": "draw",
                    "seat": 0,
                    "tile_id": "w6#a",
                    "from_kong_replacement": false,
                    "was_last_live_tile": false,
                    "was_last_discard": false
                },
                "round_wind": "east",
                "enforce_minimum_eight_fan": true,
                "restricted_discard_tile_key": null
            },
            "pending_timeout": {
                "kind": "active_turn",
                "seat_index": 0,
                "deadline_at": deadline_iso(),
                "drawn_tile_id": "w6#a"
            }
        })
    }

    fn room_for_local_add_kong_without_robbers() -> Value {
        let mut room = room_for_local_concealed_self_kong();
        room["round_state"]["players"][0]["concealed_tiles"] = json!([
            suit("w3", "w3#add"),
            suit("w2", "w2#a"),
            suit("w3", "w3#a"),
            suit("w4", "w4#a"),
            suit("t2", "t2#a"),
            suit("t3", "t3#a"),
            suit("t4", "t4#a"),
            suit("b2", "b2#a"),
            suit("b3", "b3#a"),
            suit("b4", "b4#a"),
            suit("w6", "w6#a"),
            suit("w7", "w7#a"),
            suit("w8", "w8#a"),
            suit("b7", "b7#a")
        ]);
        room["round_state"]["players"][0]["melds"] = json!([["w3", "w3", "w3"]]);
        room["round_state"]["wall"]["tiles"] = json!([suit("b8", "b8#replacement")]);
        room["round_state"]["wall"]["head_index"] = json!(0);
        room["round_state"]["wall"]["tail_index"] = json!(0);
        room
    }

    fn room_for_local_add_kong_with_robber() -> Value {
        let mut room = room_for_local_add_kong_without_robbers();
        room["round_state"]["players"][1]["concealed_tiles"] = json!([
            suit("w1", "w1#1"),
            suit("w1", "w1#2"),
            suit("w1", "w1#3"),
            suit("w2", "w2#1"),
            suit("w2", "w2#2"),
            suit("w2", "w2#3"),
            suit("w3", "w3#1"),
            suit("w3", "w3#2"),
            suit("w3", "w3#3"),
            suit("w4", "w4#1"),
            suit("w4", "w4#2"),
            suit("w4", "w4#3"),
            suit("w5", "w5#pair")
        ]);
        room
    }

    fn room_for_local_settlement() -> Value {
        json!({
            "table_code": "ROOM4",
            "phase": "playing",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "start_next_round_confirmed_seats": [],
            "restart_match_confirmed_seats": [],
            "continue_action_auto_advance_deadline_at": null,
            "seats": [
                {"seat_index": 0, "nickname": "P0", "reconnect_token": "t0", "player_session_id": 1, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null},
                {"seat_index": 1, "nickname": "P1", "reconnect_token": "t1", "player_session_id": 2, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null},
                {"seat_index": 2, "nickname": "P2", "reconnect_token": "t2", "player_session_id": 3, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null},
                {"seat_index": 3, "nickname": "P3", "reconnect_token": "t3", "player_session_id": 4, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null}
            ],
            "match_state": {
                "prevailing_wind": "east",
                "hand_number": 1,
                "dealer_seat": 0,
                "cumulative_scores": {"0": 0, "1": 0, "2": 0, "3": 0},
                "match_finished": false,
                "last_completed_round_id": null
            },
            "round_state": {
                "round_id": "east-1-dealer-0-hu",
                "dealer_seat": 0,
                "current_actor": 0,
                "wall": {"tiles": [], "head_index": 0, "tail_index": -1},
                "players": [
                    {"seat": 0, "concealed_tiles": [suit("w1", "w1#0")], "melds": [], "flowers": [], "discards": []},
                    {"seat": 1, "concealed_tiles": [suit("w2", "w2#1")], "melds": [], "flowers": [], "discards": []},
                    {"seat": 2, "concealed_tiles": [], "melds": [], "flowers": [], "discards": []},
                    {"seat": 3, "concealed_tiles": [], "melds": [], "flowers": [], "discards": []}
                ],
                "last_discard": suit("w9", "w9#discard"),
                "pending_action": {"type": "claim_window", "discarder_seat": 1, "claim_window": [[], ["hu"], [], []]},
                "phase": "playing",
                "settlement": null,
                "version": 1,
                "score_trackers": {"kong_entries": [], "opening_flowers_completed": true},
                "last_action_context": {
                    "kind": "discard",
                    "seat": 1,
                    "tile_id": "w9#discard",
                    "from_kong_replacement": false,
                    "was_last_live_tile": false,
                    "was_last_discard": false
                },
                "round_wind": "east",
                "enforce_minimum_eight_fan": true,
                "restricted_discard_tile_key": null
            },
            "pending_timeout": {"kind": "claim_window", "seat_index": 1, "deadline_at": deadline_iso(), "drawn_tile_id": null}
        })
    }

    fn room_for_local_continue_action() -> Value {
        let mut room = room_for_local_settlement();
        room["seats"][1]["is_bot"] = json!(true);
        room["seats"][1]["seat_type"] = json!("bot");
        room["seats"][2]["is_bot"] = json!(true);
        room["seats"][2]["seat_type"] = json!("bot");
        room["seats"][3]["is_bot"] = json!(true);
        room["seats"][3]["seat_type"] = json!("bot");
        let settlement = json!({
            "provisional": true,
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "display_win_label": null,
            "fan_total": 8,
            "fan_keys": ["test_fan"],
            "fan_breakdown": [{"fan_key": "test_fan", "fan_value": 8}],
            "score_delta": {
                "provisional": true,
                "basic_points": 8,
                "base_points": 8,
                "fan_total": 8,
                "minimum_qualifying_fan_total": 8,
                "fan_delta_by_seat": {"0": 24, "1": -8, "2": -8, "3": -8},
                "kong_delta_by_seat": {"0": 0, "1": 0, "2": 0, "3": 0},
                "total_delta_by_seat": {"0": 24, "1": -8, "2": -8, "3": -8}
            },
            "flower_count": 0,
            "kong_score_detail": []
        });
        room["phase"] = json!("settlement");
        room["round_state"]["phase"] = json!("settlement");
        room["round_state"]["settlement"] = settlement;
        room["pending_timeout"] = Value::Null;
        room
    }

    #[test]
    fn local_discard_advances_to_next_actor_without_claim_window() {
        let mut room = room_for_local_discard();
        assert!(can_resolve_discard_locally(&room, 0, "east#discard"));

        let result = try_handle_action(&mut room, 0, "discard", &[String::from("east#discard")])
            .expect("discard should be handled locally")
            .expect("discard should succeed");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["type"], "round_event");
        assert_eq!(result[0]["payload"]["event_type"], "tile_discarded");
        assert_eq!(room["round_state"]["current_actor"], 1);
        assert_eq!(room["round_state"]["last_discard"]["tile_key"], "east");
        assert_eq!(room["pending_timeout"]["kind"], "active_turn");
        assert_eq!(room["pending_timeout"]["seat_index"], 1);
        assert_eq!(
            room["round_state"]["players"][1]["concealed_tiles"]
                .as_array()
                .map(|tiles| tiles.len()),
            Some(14)
        );
    }

    #[test]
    fn try_handle_command_emits_typed_discard_event() {
        let mut room = room_for_local_discard();
        let output = try_handle_command(
            &mut room,
            GameCommand::PlayerAction {
                actor: 0,
                action: PlayerAction::Discard {
                    tile_id: "east#discard".to_string(),
                },
            },
        )
        .expect("discard command should be handled locally")
        .expect("discard command should succeed");

        assert_eq!(output.emitted_messages.len(), 1);
        assert!(matches!(
            output.events.first(),
            Some(GameEvent::TileDiscarded { seat: 0, tile }) if tile.tile_id == "east#discard"
        ));
    }

    #[test]
    fn active_turn_timeout_can_use_local_discard_path() {
        let mut room = room_for_local_discard();
        let result = try_process_due_timeout(&mut room).expect("timeout should be handled locally");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["payload"]["event_type"], "tile_discarded");
        assert_eq!(room["round_state"]["current_actor"], 1);
        assert_eq!(room["pending_timeout"]["kind"], "active_turn");
        assert_eq!(room["pending_timeout"]["seat_index"], 1);
    }

    #[test]
    fn local_discard_can_open_claim_window_without_hu() {
        let mut room = room_for_local_claim_window();
        assert!(can_resolve_discard_locally(&room, 0, "w3#discard"));

        let result = try_handle_action(&mut room, 0, "discard", &[String::from("w3#discard")])
            .expect("discard should be handled locally")
            .expect("discard should succeed");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["payload"]["event_type"], "tile_discarded");
        assert_eq!(room["round_state"]["current_actor"], 0);
        assert_eq!(room["pending_timeout"]["kind"], "claim_window");
        assert_eq!(
            room["round_state"]["pending_action"]["type"],
            "claim_window"
        );
        assert_eq!(
            room["round_state"]["pending_action"]["claim_window"][1],
            json!(["chow"])
        );
        assert_eq!(
            room["round_state"]["pending_action"]["claim_window"][2],
            json!(["pung"])
        );
    }

    #[test]
    fn claim_window_timeout_auto_passes_and_advances_turn() {
        let mut room = room_for_local_claim_window();
        let _ = try_handle_action(&mut room, 0, "discard", &[String::from("w3#discard")])
            .expect("discard should be handled locally")
            .expect("discard should succeed");

        let result =
            try_process_due_timeout(&mut room).expect("claim timeout should be handled locally");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["payload"]["event_type"], "claim_auto_passed");
        assert_eq!(room["round_state"]["current_actor"], 1);
        assert_eq!(room["pending_timeout"]["kind"], "active_turn");
        assert_eq!(room["pending_timeout"]["seat_index"], 1);
        assert!(room["round_state"]["pending_action"].is_null());
    }

    #[test]
    fn local_claim_pass_keeps_window_open_until_all_resolved() {
        let mut room = room_for_local_claim_window();
        let _ = try_handle_action(&mut room, 0, "discard", &[String::from("w3#discard")])
            .expect("discard should be handled locally")
            .expect("discard should succeed");

        let result = try_handle_action(&mut room, 1, "pass", &[])
            .expect("pass should be handled locally")
            .expect("pass should succeed");

        assert!(result.is_empty());
        assert_eq!(
            room["round_state"]["pending_action"]["type"],
            "claim_window"
        );
        assert_eq!(
            room["round_state"]["pending_action"]["responded_seats"],
            json!([1])
        );
        assert_eq!(room["pending_timeout"]["kind"], "claim_window");
    }

    #[test]
    fn local_pung_claim_resolves_and_sets_restricted_discard() {
        let mut room = room_for_local_claim_window();
        let _ = try_handle_action(&mut room, 0, "discard", &[String::from("w3#discard")])
            .expect("discard should be handled locally")
            .expect("discard should succeed");

        let result = try_handle_action(
            &mut room,
            2,
            "pung",
            &[String::from("w3#2a"), String::from("w3#2b")],
        )
        .expect("pung should be handled locally")
        .expect("pung should succeed");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["payload"]["event_type"], "claim_made");
        assert_eq!(result[0]["payload"]["event"]["claim_type"], "pung");
        assert_eq!(room["round_state"]["current_actor"], 2);
        assert!(room["round_state"]["pending_action"].is_null());
        assert!(room["round_state"]["last_discard"].is_null());
        assert_eq!(room["round_state"]["restricted_discard_tile_key"], "w3");
        assert_eq!(room["pending_timeout"]["kind"], "active_turn");
        assert_eq!(room["pending_timeout"]["seat_index"], 2);
    }

    #[test]
    fn room_snapshot_preserves_chow_meld_tile_codes() {
        let mut room = room_for_local_claim_window();
        room["round_state"]["players"][1]["concealed_tiles"] = json!([
            suit("w3", "w3#1extra"),
            suit("w2", "w2#1"),
            suit("w4", "w4#1"),
            suit("t1", "t1#1"),
            suit("t2", "t2#1"),
            suit("t3", "t3#1"),
            suit("b1", "b1#1"),
            suit("b2", "b2#1"),
            suit("b3", "b3#1"),
            suit("w5", "w5#1"),
            suit("w6", "w6#1"),
            suit("t6", "t6#1"),
            suit("b6", "b6#1")
        ]);
        let _ = try_handle_action(&mut room, 0, "discard", &[String::from("w3#discard")])
            .expect("discard should be handled locally")
            .expect("discard should succeed");

        let _ = try_handle_action(
            &mut room,
            1,
            "chow",
            &[String::from("w2#1"), String::from("w4#1")],
        )
        .expect("chow should be handled locally")
        .expect("chow should succeed");
        let _ = try_handle_action(&mut room, 2, "pass", &[])
            .expect("pass should be handled locally")
            .expect("pass should succeed");

        let snapshot = room_snapshot(&room, 1);
        assert_eq!(
            snapshot["payload"]["private_state"]["players"][1]["melds"][0],
            json!(["w2", "w3", "w4"])
        );
        assert_eq!(
            snapshot["payload"]["private_state"]["pending_action"]["restricted_discard_tile_ids"],
            json!(["w3#1extra"])
        );
    }

    #[test]
    fn local_chow_claim_blocks_immediate_same_tile_discard() {
        let mut room = room_for_local_claim_window();
        room["round_state"]["players"][1]["concealed_tiles"] = json!([
            suit("w3", "w3#1extra"),
            suit("w2", "w2#1"),
            suit("w4", "w4#1"),
            suit("t1", "t1#1"),
            suit("t2", "t2#1"),
            suit("t3", "t3#1"),
            suit("b1", "b1#1"),
            suit("b2", "b2#1"),
            suit("b3", "b3#1"),
            suit("w5", "w5#1"),
            suit("w6", "w6#1"),
            suit("t6", "t6#1"),
            suit("b6", "b6#1")
        ]);

        let _ = try_handle_action(&mut room, 0, "discard", &[String::from("w3#discard")])
            .expect("discard should be handled locally")
            .expect("discard should succeed");

        let _ = try_handle_action(
            &mut room,
            1,
            "chow",
            &[String::from("w2#1"), String::from("w4#1")],
        )
        .expect("chow should be handled locally")
        .expect("chow should succeed");
        let _ = try_handle_action(&mut room, 2, "pass", &[])
            .expect("pass should be handled locally")
            .expect("pass should succeed");

        assert_eq!(room["round_state"]["restricted_discard_tile_key"], "w3");
        assert!(!can_resolve_discard_locally(&room, 1, "w3#1extra"));
        assert!(try_handle_action(&mut room, 1, "discard", &[String::from("w3#1extra")]).is_none());
    }

    #[test]
    fn restricted_discard_rejection_does_not_mutate_hand_state() {
        let mut room = room_for_local_claim_window();
        room["round_state"]["players"][1]["concealed_tiles"] = json!([
            suit("w3", "w3#1extra"),
            suit("w2", "w2#1"),
            suit("w4", "w4#1"),
            suit("t1", "t1#1"),
            suit("t2", "t2#1"),
            suit("t3", "t3#1"),
            suit("b1", "b1#1"),
            suit("b2", "b2#1"),
            suit("b3", "b3#1"),
            suit("w5", "w5#1"),
            suit("w6", "w6#1"),
            suit("t6", "t6#1"),
            suit("b6", "b6#1")
        ]);

        let _ = try_handle_action(&mut room, 0, "discard", &[String::from("w3#discard")])
            .expect("discard should be handled locally")
            .expect("discard should succeed");
        let _ = try_handle_action(
            &mut room,
            1,
            "chow",
            &[String::from("w2#1"), String::from("w4#1")],
        )
        .expect("chow should be handled locally")
        .expect("chow should succeed");
        let _ = try_handle_action(&mut room, 2, "pass", &[])
            .expect("pass should be handled locally")
            .expect("pass should succeed");

        let concealed_before = room["round_state"]["players"][1]["concealed_tiles"].clone();
        let discards_before = room["round_state"]["players"][1]["discards"].clone();

        let result = apply_discard_action(&mut room, 1, "w3#1extra");

        assert_eq!(result, Err("invalid_action".to_string()));
        assert_eq!(
            room["round_state"]["players"][1]["concealed_tiles"],
            concealed_before
        );
        assert_eq!(
            room["round_state"]["players"][1]["discards"],
            discards_before
        );
    }

    #[test]
    fn local_kong_claim_draws_replacement_and_tracks_kong_score() {
        let mut room = room_for_local_kong_claim_window();
        let _ = try_handle_action(&mut room, 0, "discard", &[String::from("w3#discard")])
            .expect("discard should be handled locally")
            .expect("discard should succeed");

        assert_eq!(
            room["round_state"]["pending_action"]["claim_window"][2],
            json!(["pung", "kong"])
        );

        let result = try_handle_action(
            &mut room,
            2,
            "kong",
            &[
                String::from("w3#2a"),
                String::from("w3#2b"),
                String::from("w3#2c"),
            ],
        )
        .expect("kong should be handled locally")
        .expect("kong should succeed");

        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["payload"]["event_type"], "claim_made");
        assert_eq!(result[1]["payload"]["event_type"], "replacement_draw");
        assert_eq!(room["round_state"]["current_actor"], 2);
        assert!(room["round_state"]["pending_action"].is_null());
        assert_eq!(room["round_state"]["restricted_discard_tile_key"], "w3");
        assert_eq!(room["pending_timeout"]["kind"], "active_turn");
        assert_eq!(room["pending_timeout"]["seat_index"], 2);
        assert_eq!(
            room["round_state"]["score_trackers"]["kong_entries"]
                .as_array()
                .map(|entries| entries.len()),
            Some(1)
        );
    }

    #[test]
    fn local_concealed_self_kong_draws_replacement_and_exposes_option() {
        let mut room = room_for_local_concealed_self_kong();
        let prompt = action_prompt(&room, 0).expect("prompt should exist");
        assert_eq!(prompt["payload"]["options"], json!(["discard", "kong"]));

        let result = try_handle_action(
            &mut room,
            0,
            "kong",
            &[
                String::from("t5#1"),
                String::from("t5#2"),
                String::from("t5#3"),
                String::from("t5#4"),
            ],
        )
        .expect("self kong should be handled locally")
        .expect("self kong should succeed");

        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["payload"]["event_type"], "self_kong_declared");
        assert_eq!(result[1]["payload"]["event_type"], "replacement_draw");
        assert_eq!(
            room["round_state"]["players"][0]["melds"][0],
            json!(["t5", "t5", "t5", "t5"])
        );
        assert_eq!(room["pending_timeout"]["kind"], "active_turn");
        assert_eq!(room["pending_timeout"]["seat_index"], 0);
        assert_eq!(
            room["round_state"]["score_trackers"]["kong_entries"]
                .as_array()
                .map(|entries| entries.len()),
            Some(1)
        );
    }

    #[test]
    fn next_bot_action_discards_drawn_tile_on_active_turn() {
        let room = room_for_bot_active_turn();
        let action = next_bot_action(&room).expect("bot action should exist");

        assert_eq!(action.seat_index, 0);
        assert_eq!(action.action_type, "discard");
        assert_eq!(action.tile_ids, vec!["east#discard"]);
    }

    #[test]
    fn next_bot_action_keeps_shape_and_discards_isolated_honor() {
        let room = room_for_bot_shape_choice();
        let action = next_bot_action(&room).expect("bot action should exist");

        assert_eq!(action.seat_index, 0);
        assert_eq!(action.action_type, "discard");
        assert_eq!(action.tile_ids, vec!["east#isolated"]);
    }

    #[test]
    fn local_add_kong_without_robbers_upgrades_existing_meld() {
        let mut room = room_for_local_add_kong_without_robbers();
        let prompt = action_prompt(&room, 0).expect("prompt should exist");
        assert_eq!(prompt["payload"]["options"], json!(["discard", "kong"]));

        let result = try_handle_action(&mut room, 0, "kong", &[String::from("w3#add")])
            .expect("add kong should be handled locally")
            .expect("add kong should succeed");

        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["payload"]["event_type"], "self_kong_declared");
        assert_eq!(result[0]["payload"]["event"]["kong_type"], "add_kong");
        assert_eq!(result[1]["payload"]["event_type"], "replacement_draw");
        assert_eq!(
            room["round_state"]["players"][0]["melds"][0],
            json!(["w3", "w3", "w3", "w3"])
        );
        assert_eq!(room["pending_timeout"]["kind"], "active_turn");
        assert_eq!(room["pending_timeout"]["seat_index"], 0);
    }

    #[test]
    fn local_add_kong_with_robber_opens_rob_kong_window() {
        let mut room = room_for_local_add_kong_with_robber();

        let result = try_handle_action(&mut room, 0, "kong", &[String::from("w3#add")])
            .expect("add kong should be handled locally")
            .expect("add kong should open rob kong window");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["payload"]["event_type"], "self_kong_declared");
        assert_eq!(
            room["round_state"]["pending_action"]["type"],
            "rob_kong_window"
        );
        assert_eq!(room["round_state"]["pending_action"]["actor_seat"], 0);
        assert_eq!(
            room["round_state"]["pending_action"]["offered_hu_seats"],
            json!([1])
        );
        assert_eq!(room["pending_timeout"]["kind"], "claim_window");
    }

    #[test]
    fn next_bot_action_takes_rob_kong_hu() {
        let mut room = room_for_local_add_kong_with_robber();
        room["seats"][1]["is_bot"] = json!(true);
        room["seats"][1]["seat_type"] = json!("bot");
        let _ = try_handle_action(&mut room, 0, "kong", &[String::from("w3#add")])
            .expect("add kong should be handled locally")
            .expect("add kong should open rob kong window");

        let action = next_bot_action(&room).expect("bot claim action should exist");
        assert_eq!(action.seat_index, 1);
        assert_eq!(action.action_type, "hu");
        assert!(action.tile_ids.is_empty());
    }

    #[test]
    fn next_bot_action_avoids_add_kong_when_robbers_exist() {
        let mut room = room_for_local_add_kong_with_robber();
        room["seats"][0]["is_bot"] = json!(true);
        room["seats"][0]["seat_type"] = json!("bot");

        let action = next_bot_action(&room).expect("bot action should exist");
        assert_eq!(action.seat_index, 0);
        assert_eq!(action.action_type, "discard");
    }

    #[test]
    fn local_rob_kong_pass_completes_add_kong() {
        let mut room = room_for_local_add_kong_with_robber();
        let _ = try_handle_action(&mut room, 0, "kong", &[String::from("w3#add")])
            .expect("add kong should be handled locally")
            .expect("add kong should open rob kong window");

        let result = try_handle_action(&mut room, 1, "pass", &[])
            .expect("rob kong pass should be handled locally")
            .expect("pass should succeed");

        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["payload"]["event_type"], "self_kong_declared");
        assert_eq!(result[1]["payload"]["event_type"], "replacement_draw");
        assert!(room["round_state"]["pending_action"].is_null());
        assert_eq!(
            room["round_state"]["players"][0]["melds"][0],
            json!(["w3", "w3", "w3", "w3"])
        );
        assert_eq!(room["pending_timeout"]["kind"], "active_turn");
        assert_eq!(room["pending_timeout"]["seat_index"], 0);
    }

    #[test]
    fn local_rob_kong_hu_resolves_with_rust_scoring() {
        let mut room = room_for_local_add_kong_with_robber();
        let _ = try_handle_action(&mut room, 0, "kong", &[String::from("w3#add")])
            .expect("add kong should be handled locally")
            .expect("add kong should open rob kong window");

        let result = try_handle_action(&mut room, 1, "hu", &[])
            .expect("hu should be handled locally")
            .expect("hu should succeed");

        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["payload"]["event_type"], "claim_made");
        assert_eq!(result[1]["payload"]["event_type"], "settlement_ready");
        assert_eq!(room["phase"], "settlement");
        assert_eq!(room["round_state"]["settlement"]["winner_seat"], 1);
        assert!(
            room["round_state"]["settlement"]["fan_total"]
                .as_i64()
                .unwrap_or(0)
                >= 8
        );
    }

    #[test]
    fn local_apply_hu_settlement_transitions_room_and_updates_match_state() {
        let mut room = room_for_local_settlement();
        let settlement = json!({
            "provisional": true,
            "win_type": "discard",
            "winner_seat": 1,
            "discarder_seat": 0,
            "display_win_label": null,
            "fan_total": 8,
            "fan_keys": ["test_fan"],
            "fan_breakdown": [{"fan_key": "test_fan", "fan_value": 8}],
            "score_delta": {
                "provisional": true,
                "basic_points": 8,
                "base_points": 8,
                "fan_total": 8,
                "minimum_qualifying_fan_total": 8,
                "fan_delta_by_seat": {"0": -8, "1": 24, "2": -8, "3": -8},
                "kong_delta_by_seat": {"0": 0, "1": 0, "2": 0, "3": 0},
                "total_delta_by_seat": {"0": -8, "1": 24, "2": -8, "3": -8}
            },
            "flower_count": 0,
            "kong_score_detail": []
        });

        let events = apply_hu_settlement(&mut room, 1, "discard", settlement)
            .expect("settlement should apply");

        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["payload"]["event_type"], "claim_made");
        assert_eq!(events[1]["payload"]["event_type"], "settlement_ready");
        assert_eq!(room["phase"], "settlement");
        assert_eq!(room["round_state"]["phase"], "settlement");
        assert_eq!(
            room["match_state"]["last_completed_round_id"],
            "east-1-dealer-0-hu"
        );
        assert_eq!(room["match_state"]["cumulative_scores"]["1"], 24);
    }

    #[test]
    fn hu_settlement_preserves_existing_kong_deltas_from_other_players() {
        let mut room = room_for_local_add_kong_with_robber();
        room["round_state"]["score_trackers"]["kong_entries"] = json!([
            {
                "kong_type": "exposed_kong",
                "actor_seat": 2,
                "payer_seats": [0, 1, 3],
                "tile_key": "w3"
            }
        ]);

        let _ = try_handle_action(&mut room, 0, "kong", &[String::from("w3#add")])
            .expect("add kong should be handled locally")
            .expect("add kong should open rob kong window");
        let _ = try_handle_action(&mut room, 1, "hu", &[])
            .expect("hu should be handled locally")
            .expect("hu should succeed");

        let settlement = &room["round_state"]["settlement"];
        assert_eq!(settlement["score_delta"]["kong_delta_by_seat"]["0"], -1);
        assert_eq!(settlement["score_delta"]["kong_delta_by_seat"]["1"], -1);
        assert_eq!(settlement["score_delta"]["kong_delta_by_seat"]["2"], 3);
        assert_eq!(settlement["score_delta"]["kong_delta_by_seat"]["3"], -1);
        assert_eq!(
            settlement["score_delta"]["total_delta_by_seat"]["0"].as_i64(),
            settlement["score_delta"]["fan_delta_by_seat"]["0"]
                .as_i64()
                .map(|value| value - 1)
        );
        assert_eq!(
            settlement["score_delta"]["total_delta_by_seat"]["1"].as_i64(),
            settlement["score_delta"]["fan_delta_by_seat"]["1"]
                .as_i64()
                .map(|value| value - 1)
        );
        assert_eq!(
            settlement["score_delta"]["total_delta_by_seat"]["2"].as_i64(),
            settlement["score_delta"]["fan_delta_by_seat"]["2"]
                .as_i64()
                .map(|value| value + 3)
        );
        assert_eq!(
            settlement["score_delta"]["total_delta_by_seat"]["3"].as_i64(),
            settlement["score_delta"]["fan_delta_by_seat"]["3"]
                .as_i64()
                .map(|value| value - 1)
        );
    }

    #[test]
    fn local_start_next_round_completes_when_all_required_confirmed() {
        let mut room = room_for_local_continue_action();
        record_continue_action(&mut room, 0, "start_next_round")
            .expect("continue action should succeed");

        assert_eq!(room["phase"], "playing");
        assert_eq!(room["match_state"]["hand_number"], 2);
        assert_eq!(room["match_state"]["dealer_seat"], 1);
        assert_eq!(room["match_state"]["cumulative_scores"]["0"], 24);
        assert!(room["round_state"]["phase"] == "playing");
        assert_eq!(
            room["continue_action_auto_advance_deadline_at"],
            Value::Null
        );
    }

    #[test]
    fn local_restart_match_resets_scores_and_restarts_playing() {
        let mut room = room_for_local_continue_action();
        room["phase"] = json!("finished");
        room["match_state"]["prevailing_wind"] = json!("north");
        room["match_state"]["hand_number"] = json!(4);
        room["match_state"]["dealer_seat"] = json!(3);
        room["match_state"]["match_finished"] = json!(true);
        room["match_state"]["cumulative_scores"] = json!({"0": 20, "1": -10, "2": -5, "3": -5});

        record_continue_action(&mut room, 0, "restart_match").expect("restart should succeed");

        assert_eq!(room["phase"], "playing");
        assert_eq!(room["match_state"]["prevailing_wind"], "east");
        assert_eq!(room["match_state"]["hand_number"], 1);
        assert_eq!(
            room["match_state"]["cumulative_scores"],
            json!({"0": 0, "1": 0, "2": 0, "3": 0})
        );
    }
}
