use serde_json::{Value, json};
use std::collections::HashSet;

use crate::core::engine::EngineOutput;
use crate::core::engine::planner::{
    plan_claim_window_continuation_without_winner, plan_claim_window_response, plan_discard_action,
    resolve_claims,
};
use crate::core::engine::reducer::{LegacyRoomMutation, apply_legacy_room_mutations};
use crate::core::event::GameEvent;
use crate::core::state::{
    ClaimWindowAction, LastActionContext, PendingAction, RobKongWindowAction, RoomState, RoundState,
};
use crate::core::tile::Tile;
use crate::rules::skills::{
    note_tracker_claimed_discard, note_tracker_discard, note_tracker_draw,
    sync_round_skill_trackers,
};

use super::meld::{
    SelfKongCandidate, SelfKongKind, available_self_kongs, claim_window_options_after_discard,
    is_valid_chow_sequence_by_keys, resolve_self_kong_selection, seats_with_hu_candidate_for_tile,
};
use super::runtime::{
    current_actor, is_last_live_tile_point, pending_timeout_kind, player_concealed_tile,
    project_room_state, replacement_tile_from_tail, round_event_message, sync_pending_timeout,
};
use super::settlement::settle_exhaustive_draw;
use super::win::{apply_hu_settlement, compute_hu_settlement};

const MAX_SEATS: usize = 4;

fn tile_discarded_event(seat_index: usize, tile: &Tile) -> GameEvent {
    GameEvent::TileDiscarded {
        seat: seat_index,
        tile: tile.clone(),
    }
}

fn tile_discarded_message(seat_index: usize, tile: &Tile) -> Value {
    round_event_message(
        "tile_discarded",
        json!({
            "type": "tile_discarded",
            "seat": seat_index,
            "tile_id": tile.tile_id,
            "tile_key": tile.tile_key,
        }),
    )
}

fn replacement_draw_event(seat_index: usize, tile: &Tile) -> GameEvent {
    GameEvent::TileDrawn {
        seat: seat_index,
        tile: tile.clone(),
        source: "replacement_draw".to_string(),
    }
}

fn replacement_draw_message(seat_index: usize, tile: &Tile) -> Value {
    round_event_message(
        "replacement_draw",
        json!({
            "type": "replacement_draw",
            "seat": seat_index,
            "tile_id": tile.tile_id,
            "tile_key": tile.tile_key,
        }),
    )
}

fn meld_claimed_event(seat_index: usize, meld: &[String], discarder_seat: usize) -> GameEvent {
    GameEvent::MeldClaimed {
        seat: seat_index,
        meld: meld.to_vec(),
        from: discarder_seat,
    }
}

fn claim_made_message(
    seat_index: usize,
    discarder_seat: usize,
    action_type: &str,
    last_discard_tile: &Tile,
    meld: &[String],
) -> Value {
    round_event_message(
        "claim_made",
        json!({
            "type": "claim_made",
            "seat": seat_index,
            "from": discarder_seat,
            "claim_type": action_type,
            "tile_id": last_discard_tile.tile_id,
            "tile_key": last_discard_tile.tile_key,
            "meld": meld,
        }),
    )
}

pub fn try_handle_self_kong_action(
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

pub fn apply_claim_window_action(
    room: &mut Value,
    seat_index: usize,
    action_type: &str,
    tile_ids: &[String],
) -> Result<EngineOutput, String> {
    if matches!(action_type, "chow" | "pung" | "kong") {
        validate_claim_selection(room, seat_index, action_type, tile_ids)?;
    }
    let state = project_room_state(room)?;
    let plan = plan_claim_window_response(&state, seat_index, action_type, tile_ids)?;
    apply_legacy_room_mutations(room, &plan.mutations)?;
    if !plan.unresolved_seats.is_empty() {
        sync_round_skill_trackers(room);
        sync_pending_timeout(room);
        return Ok(EngineOutput::default());
    }
    resolve_recorded_claims_local_output(room)
}

pub fn apply_discard_action(
    room: &mut Value,
    seat_index: usize,
    tile_id: &str,
) -> Result<Vec<Value>, String> {
    apply_discard_action_output(room, seat_index, tile_id).map(|output| output.emitted_messages)
}

pub fn apply_discard_action_output(
    room: &mut Value,
    seat_index: usize,
    tile_id: &str,
) -> Result<EngineOutput, String> {
    let state = project_room_state(room)?;
    if state.phase != "playing" {
        return Err("round_not_ready".to_string());
    }
    let round = round_state_ref(&state)?;
    if round.current_actor != seat_index {
        return Err("not_your_turn".to_string());
    }
    if round.pending_action.is_some() {
        return Err("invalid_action".to_string());
    }

    let discarded_tile = round
        .players
        .get(seat_index)
        .and_then(|player| {
            player
                .concealed_tiles
                .iter()
                .find(|tile| tile.tile_id == tile_id)
        })
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;
    if let Some(restricted) = round.restricted_discard_tile_key.as_deref() {
        if discarded_tile.tile_key == restricted {
            return Err("invalid_action".to_string());
        }
    }
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
            tile: Some(discarded_tile.clone()),
        },
    ];
    let mut simulated = room.clone();
    apply_legacy_room_mutations(&mut simulated, &discard_mutations)?;

    let previous_was_last_live_tile = round.last_action_context.was_last_live_tile;
    let claim_window =
        claim_window_options_after_discard(&simulated, seat_index, &discarded_tile.tile_key);
    let plan = plan_discard_action(
        &state,
        seat_index,
        tile_id,
        claim_window,
        previous_was_last_live_tile,
    )?;
    apply_legacy_room_mutations(room, &plan.discard_mutations(seat_index))?;
    note_tracker_discard(room, seat_index, &plan.discarded_tile.tile_key);
    if plan.continuation.needs_exhaustive_draw {
        sync_round_skill_trackers(room);
        let discard_message = tile_discarded_message(seat_index, &plan.discarded_tile);
        let mut settlement_messages = settle_exhaustive_draw(room);
        let settlement_output = EngineOutput::from_emitted_messages(settlement_messages.clone());
        let mut events = vec![tile_discarded_event(seat_index, &plan.discarded_tile)];
        events.extend(settlement_output.events);
        let mut emitted_messages = vec![discard_message];
        emitted_messages.append(&mut settlement_messages);
        return Ok(EngineOutput::new(events, emitted_messages));
    }
    apply_legacy_room_mutations(room, &plan.followup_mutations())?;
    let updated_state = project_room_state(room)?;
    let updated_round = round_state_ref(&updated_state)?;
    let next_actor = updated_round.current_actor;
    let drawn_tile_key = updated_round
        .last_action_context
        .tile_id
        .as_deref()
        .and_then(|tile_id| {
            updated_round
                .players
                .get(next_actor)
                .and_then(|player| {
                    player
                        .concealed_tiles
                        .iter()
                        .find(|tile| tile.tile_id == tile_id)
                })
                .map(|tile| tile.tile_key.clone())
        });
    if let Some(tile_key) = drawn_tile_key.as_deref() {
        note_tracker_draw(room, next_actor, tile_key);
    }
    sync_round_skill_trackers(room);
    sync_pending_timeout(room);
    Ok(EngineOutput::new(
        vec![tile_discarded_event(seat_index, &plan.discarded_tile)],
        vec![tile_discarded_message(seat_index, &plan.discarded_tile)],
    ))
}

pub fn can_resolve_discard_locally(room: &Value, seat_index: usize, tile_id: &str) -> bool {
    let Ok(state) = project_room_state(room) else {
        return false;
    };
    if state.phase != "playing" {
        return false;
    }
    let Some(round) = state.round_state.as_ref() else {
        return false;
    };
    if round.current_actor != seat_index {
        return false;
    }
    if round.pending_action.is_some() {
        return false;
    }
    if round.wall.live_tiles_remaining() <= 0 {
        return false;
    }

    let Some(discarded_tile) = round.players.get(seat_index).and_then(|player| {
        player
            .concealed_tiles
            .iter()
            .find(|tile| tile.tile_id == tile_id)
    }) else {
        return false;
    };
    if round.restricted_discard_tile_key.as_deref() == Some(discarded_tile.tile_key.as_str()) {
        return false;
    }

    !discarded_tile.tile_id.is_empty()
}

pub fn claim_window_supported_locally(room: &Value) -> bool {
    project_room_state(room)
        .ok()
        .and_then(|state| state.round_state)
        .and_then(|round| round.pending_action)
        .is_some_and(|pending| matches!(pending, PendingAction::ClaimWindow(_)))
}

pub fn rob_kong_window_supported_locally(room: &Value) -> bool {
    project_room_state(room)
        .ok()
        .and_then(|state| state.round_state)
        .and_then(|round| round.pending_action)
        .is_some_and(|pending| matches!(pending, PendingAction::RobKongWindow(_)))
}

pub fn can_resolve_claim_window_timeout_locally(room: &Value) -> bool {
    pending_timeout_kind(room) == Some("claim_window") && claim_window_supported_locally(room)
}

pub fn resolve_claim_window_timeout(room: &mut Value) -> Result<Vec<Value>, String> {
    let state = project_room_state(room)?;
    let round = state
        .round_state
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let claim = match round.pending_action.as_ref() {
        Some(PendingAction::ClaimWindow(claim)) => claim,
        _ => return Err("invalid_action".to_string()),
    };
    let discarder_seat = claim.discarder_seat;
    let mut responded_seats = claim.responded_seats.clone();
    let unresolved_seats = claim
        .claim_window
        .iter()
        .enumerate()
        .filter(|(_, claims)| !claims.is_empty())
        .filter(|(seat_index, _)| !responded_seats.contains(seat_index))
        .map(|(seat_index, _)| seat_index)
        .collect::<Vec<_>>();
    responded_seats.extend(unresolved_seats.iter().copied());
    let unresolved: Vec<Value> = unresolved_seats
        .iter()
        .map(|seat_index| Value::Number((*seat_index as u64).into()))
        .collect();
    apply_legacy_room_mutations(
        room,
        &[
            LegacyRoomMutation::SetRoundPendingAction {
                pending_action: Some(PendingAction::ClaimWindow(ClaimWindowAction {
                    discarder_seat,
                    claim_window: claim.claim_window.clone(),
                    responded_seats,
                    claim_responses: claim.claim_responses.clone(),
                })),
            },
            LegacyRoomMutation::IncrementRoundVersion,
        ],
    )?;
    sync_round_skill_trackers(room);

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

pub fn can_resolve_rob_kong_timeout_locally(room: &Value) -> bool {
    pending_timeout_kind(room) == Some("claim_window") && rob_kong_window_supported_locally(room)
}

pub fn apply_rob_kong_pass(room: &mut Value, seat_index: usize) -> Result<EngineOutput, String> {
    let state = project_room_state(room)?;
    let round = state
        .round_state
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let rob = match round.pending_action.as_ref() {
        Some(PendingAction::RobKongWindow(rob)) => rob,
        _ => return Err("invalid_action".to_string()),
    };
    if !rob.offered_hu_seats.contains(&seat_index) {
        return Err("invalid_action".to_string());
    }
    if rob.responded_seats.contains(&seat_index) {
        return Err("invalid_action".to_string());
    }

    let mut next_responded = rob.responded_seats.clone();
    next_responded.push(seat_index);
    apply_legacy_room_mutations(
        room,
        &[
            LegacyRoomMutation::SetRoundPendingAction {
                pending_action: Some(PendingAction::RobKongWindow(RobKongWindowAction {
                    actor_seat: rob.actor_seat,
                    tile_id: rob.tile_id.clone(),
                    tile_key: rob.tile_key.clone(),
                    meld_index: rob.meld_index,
                    offered_hu_seats: rob.offered_hu_seats.clone(),
                    responded_seats: next_responded.clone(),
                })),
            },
            LegacyRoomMutation::IncrementRoundVersion,
        ],
    )?;

    let unresolved = rob
        .offered_hu_seats
        .iter()
        .copied()
        .filter(|offered_seat| !next_responded.contains(offered_seat))
        .collect::<Vec<_>>();
    if !unresolved.is_empty() {
        sync_round_skill_trackers(room);
        sync_pending_timeout(room);
        return Ok(EngineOutput::default());
    }
    let messages = complete_add_kong_after_passes(room)?;
    Ok(EngineOutput::from_emitted_messages(messages))
}

pub fn resolve_rob_kong_timeout(room: &mut Value) -> Result<Vec<Value>, String> {
    let state = project_room_state(room)?;
    let round = state
        .round_state
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let rob = match round.pending_action.as_ref() {
        Some(PendingAction::RobKongWindow(rob)) => rob,
        _ => return Err("invalid_action".to_string()),
    };
    let actor_seat = rob.actor_seat;
    let unresolved_seats: Vec<usize> = rob
        .offered_hu_seats
        .iter()
        .copied()
        .filter(|seat| !rob.responded_seats.contains(seat))
        .collect();
    let unresolved: Vec<Value> = unresolved_seats
        .iter()
        .map(|seat| Value::Number((*seat as u64).into()))
        .collect();
    let mut next_responded = rob.responded_seats.clone();
    next_responded.extend(unresolved_seats);
    apply_legacy_room_mutations(
        room,
        &[
            LegacyRoomMutation::SetRoundPendingAction {
                pending_action: Some(PendingAction::RobKongWindow(RobKongWindowAction {
                    actor_seat,
                    tile_id: rob.tile_id.clone(),
                    tile_key: rob.tile_key.clone(),
                    meld_index: rob.meld_index,
                    offered_hu_seats: rob.offered_hu_seats.clone(),
                    responded_seats: next_responded,
                })),
            },
            LegacyRoomMutation::IncrementRoundVersion,
        ],
    )?;
    sync_round_skill_trackers(room);
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
    let replacement_tile = tile_from_value(&replacement_tile)?;
    let plan =
        plan_self_kong_completion(room, seat_index, selection, replacement_tile.clone(), false)?;
    apply_legacy_room_mutations(room, &plan.mutations)?;
    note_tracker_draw(room, seat_index, &replacement_tile.tile_key);
    sync_round_skill_trackers(room);
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
    let selected_tile = tile_from_value(&selected_tile)?;
    apply_legacy_room_mutations(
        room,
        &[
            LegacyRoomMutation::SetRoundLastDiscard {
                tile: Some(selected_tile.clone()),
            },
            LegacyRoomMutation::SetRoundPendingAction {
                pending_action: Some(PendingAction::RobKongWindow(RobKongWindowAction {
                    actor_seat: seat_index,
                    tile_id: Some(selected_tile.tile_id.clone()),
                    tile_key: Some(selected_tile.tile_key.clone()),
                    meld_index: selection.meld_index,
                    offered_hu_seats,
                    responded_seats: vec![],
                })),
            },
            LegacyRoomMutation::IncrementRoundVersion,
        ],
    )?;
    sync_round_skill_trackers(room);
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
    replacement_tile: Tile,
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
                meld: vec![
                    selection.tile_key.clone(),
                    selection.tile_key.clone(),
                    selection.tile_key.clone(),
                    selection.tile_key.clone(),
                ],
            });
        }
        SelfKongKind::Add => {
            let meld_index = selection
                .meld_index
                .ok_or_else(|| "invalid_action".to_string())?;
            mutations.push(LegacyRoomMutation::AppendTileToPlayerMeld {
                seat_index,
                meld_index,
                tile_key: selection.tile_key.clone(),
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
        tile_key: Some(selection.tile_key.clone()),
    });
    mutations.push(LegacyRoomMutation::SetRoundLastActionContext {
        context: LastActionContext {
            kind: "replacement_draw".to_string(),
            seat: seat_index,
            tile_id: Some(replacement_tile.tile_id.clone()),
            from_kong_replacement: true,
            was_last_live_tile: false,
            was_last_discard: false,
        },
    });
    if clear_pending_action {
        mutations.push(LegacyRoomMutation::SetRoundPendingAction {
            pending_action: None,
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
            replacement_draw_message(seat_index, &replacement_tile),
        ],
    })
}

fn resolve_recorded_claims_local(room: &mut Value) -> Result<Vec<Value>, String> {
    resolve_recorded_claims_local_output(room).map(|output| output.emitted_messages)
}

fn resolve_recorded_claims_local_output(room: &mut Value) -> Result<EngineOutput, String> {
    let state = project_room_state(room)?;
    let round = state
        .round_state
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let claim = match round.pending_action.as_ref() {
        Some(PendingAction::ClaimWindow(claim)) => claim,
        _ => return Err("invalid_action".to_string()),
    };
    let discarder_seat = claim.discarder_seat;
    let claim_responses = claim.claim_responses.clone();

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
        sync_round_skill_trackers(room);
        let emitted_messages = settle_exhaustive_draw(room);
        return Ok(EngineOutput::from_emitted_messages(emitted_messages));
    }
    apply_legacy_room_mutations(room, &plan.mutations)?;
    let updated_state = project_room_state(room)?;
    let updated_round = round_state_ref(&updated_state)?;
    let seat = updated_round.current_actor;
    let drawn_tile_key = updated_round
        .last_action_context
        .tile_id
        .as_deref()
        .and_then(|tile_id| {
            updated_round
                .players
                .get(seat)
                .and_then(|player| {
                    player
                        .concealed_tiles
                        .iter()
                        .find(|tile| tile.tile_id == tile_id)
                })
                .map(|tile| tile.tile_key.clone())
        });
    if let Some(tile_key) = drawn_tile_key.as_deref() {
        note_tracker_draw(room, seat, tile_key);
    }
    sync_round_skill_trackers(room);
    sync_pending_timeout(room);
    Ok(EngineOutput::default())
}

fn validate_claim_selection(
    room: &Value,
    seat_index: usize,
    action_type: &str,
    tile_ids: &[String],
) -> Result<(), String> {
    let state = project_room_state(room)?;
    let round = round_state_ref(&state)?;
    let last_discard_tile_key = round
        .last_discard
        .as_ref()
        .map(|tile| tile.tile_key.as_str())
        .ok_or_else(|| "invalid_action".to_string())?;
    let expected = match action_type {
        "chow" | "pung" => 2,
        "kong" => 3,
        _ => return Err("invalid_action".to_string()),
    };
    if tile_ids.len() != expected {
        return Err("invalid_action".to_string());
    }
    let player_tiles = round
        .players
        .get(seat_index)
        .map(|player| player.concealed_tiles.as_slice())
        .ok_or_else(|| "invalid_action".to_string())?;
    let mut claimed_tile_keys = Vec::with_capacity(tile_ids.len());
    let mut used_indices = HashSet::with_capacity(tile_ids.len());
    for tile_id in tile_ids {
        let Some((index, tile)) = player_tiles.iter().enumerate().find(|(index, tile)| {
            !used_indices.contains(index) && tile.tile_id == tile_id.as_str()
        }) else {
            return Err("invalid_action".to_string());
        };
        used_indices.insert(index);
        claimed_tile_keys.push(tile.tile_key.as_str());
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

fn apply_selected_claim(
    room: &mut Value,
    seat_index: usize,
    action_type: &str,
    tile_ids: &[String],
) -> Result<EngineOutput, String> {
    if action_type == "hu" {
        let settlement = compute_hu_settlement(room, seat_index, "discard")?;
        let emitted_messages = apply_hu_settlement(room, seat_index, "discard", settlement)?;
        return Ok(EngineOutput::from_emitted_messages(emitted_messages));
    }
    let plan = plan_selected_claim(room, seat_index, action_type, tile_ids)?;
    apply_legacy_room_mutations(room, &plan.mutations)?;
    note_tracker_claimed_discard(room, plan.discarder_seat);
    if let Some(replacement_tile) = plan.replacement_tile.as_ref() {
        note_tracker_draw(room, seat_index, &replacement_tile.tile_key);
    }
    sync_round_skill_trackers(room);
    sync_pending_timeout(room);
    let mut events = vec![meld_claimed_event(
        seat_index,
        &plan.meld,
        plan.discarder_seat,
    )];
    if let Some(replacement_tile) = plan.replacement_tile.as_ref() {
        events.push(replacement_draw_event(seat_index, replacement_tile));
    }
    Ok(EngineOutput::new(events, plan.emitted_messages))
}

struct SelectedClaimPlan {
    discarder_seat: usize,
    meld: Vec<String>,
    replacement_tile: Option<Tile>,
    mutations: Vec<LegacyRoomMutation>,
    emitted_messages: Vec<Value>,
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
    let state = project_room_state(room)?;
    let round = round_state_ref(&state)?;
    let discarder_seat = match round.pending_action.as_ref() {
        Some(PendingAction::ClaimWindow(claim)) => claim.discarder_seat,
        _ => return Err("invalid_action".to_string()),
    };
    let last_discard_tile = round
        .last_discard
        .clone()
        .ok_or_else(|| "invalid_action".to_string())?;
    let restricted_tile_key = Some(last_discard_tile.tile_key.clone());

    if !discarder_latest_discard_matches(round, discarder_seat, &last_discard_tile) {
        return Err("invalid_action".to_string());
    }
    let claimed_tiles = selected_player_tiles(round, seat_index, tile_ids)?;
    let meld = claim_meld_value(action_type, &last_discard_tile, &claimed_tiles);

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
    mutations.push(LegacyRoomMutation::PushPlayerMeld {
        seat_index,
        meld: meld.clone(),
    });

    let mut emitted_messages = vec![claim_made_message(
        seat_index,
        discarder_seat,
        action_type,
        &last_discard_tile,
        &meld,
    )];

    let mut replacement_tile_for_output = None;
    if action_type == "kong" {
        let replacement_tile = tile_from_value(
            &replacement_tile_from_tail(room).ok_or_else(|| "invalid_action".to_string())?,
        )?;
        replacement_tile_for_output = Some(replacement_tile.clone());
        mutations.push(LegacyRoomMutation::RetreatWallTail);
        mutations.push(LegacyRoomMutation::PushPlayerConcealedTile {
            seat_index,
            tile: replacement_tile.clone(),
        });
        mutations.push(LegacyRoomMutation::AppendRoundKongEntry {
            kong_type: "exposed_kong".to_string(),
            actor_seat: seat_index,
            payer_seats: vec![discarder_seat],
            tile_key: Some(last_discard_tile.tile_key.clone()),
        });
        mutations.push(LegacyRoomMutation::SetRoundLastActionContext {
            context: LastActionContext {
                kind: "replacement_draw".to_string(),
                seat: seat_index,
                tile_id: Some(replacement_tile.tile_id.clone()),
                from_kong_replacement: true,
                was_last_live_tile: false,
                was_last_discard: false,
            },
        });
        emitted_messages.push(replacement_draw_message(seat_index, &replacement_tile));
    }

    mutations.push(LegacyRoomMutation::SetRoundCurrentActor { seat_index });
    mutations.push(LegacyRoomMutation::SetRoundLastDiscard { tile: None });
    mutations.push(LegacyRoomMutation::SetRoundPendingAction {
        pending_action: None,
    });
    mutations.push(LegacyRoomMutation::SetRoundRestrictedDiscardTileKey {
        tile_key: restricted_tile_key,
    });
    mutations.push(LegacyRoomMutation::IncrementRoundVersion);

    Ok(SelectedClaimPlan {
        discarder_seat,
        meld,
        replacement_tile: replacement_tile_for_output,
        mutations,
        emitted_messages,
    })
}

fn claim_meld_value(action_type: &str, last_discard: &Tile, claimed_tiles: &[Tile]) -> Vec<String> {
    if action_type == "chow" {
        let mut tiles = claimed_tiles
            .iter()
            .map(|tile| tile.tile_key.clone())
            .collect::<Vec<_>>();
        tiles.push(last_discard.tile_key.clone());
        tiles.sort();
        return tiles;
    }
    let mut tiles = vec![last_discard.tile_key.clone()];
    tiles.extend(claimed_tiles.iter().map(|tile| tile.tile_key.clone()));
    tiles
}

fn selected_player_tiles(
    round: &RoundState,
    seat_index: usize,
    tile_ids: &[String],
) -> Result<Vec<Tile>, String> {
    let player_tiles = round
        .players
        .get(seat_index)
        .map(|player| player.concealed_tiles.as_slice())
        .ok_or_else(|| "invalid_action".to_string())?;
    let mut selected = Vec::with_capacity(tile_ids.len());
    let mut used_indices = HashSet::with_capacity(tile_ids.len());
    for tile_id in tile_ids {
        let Some((index, tile)) = player_tiles.iter().enumerate().find(|(index, tile)| {
            !used_indices.contains(index) && tile.tile_id == tile_id.as_str()
        }) else {
            return Err("invalid_action".to_string());
        };
        used_indices.insert(index);
        selected.push(tile.clone());
    }
    Ok(selected)
}

fn tile_from_value(tile: &Value) -> Result<Tile, String> {
    Tile::from_legacy_value(tile, "standard_action.tile").map_err(|error| error.to_string())
}

fn discarder_latest_discard_matches(
    round: &RoundState,
    discarder_seat: usize,
    last_discard: &Tile,
) -> bool {
    round
        .players
        .get(discarder_seat)
        .and_then(|player| player.discards.last())
        .map(|tile| tile.tile_id == last_discard.tile_id)
        .unwrap_or(false)
}

fn round_state_ref(state: &RoomState) -> Result<&RoundState, String> {
    state
        .round_state
        .as_ref()
        .ok_or_else(|| "round_not_ready".to_string())
}

fn complete_add_kong_after_passes(room: &mut Value) -> Result<Vec<Value>, String> {
    let state = project_room_state(room)?;
    let round = state
        .round_state
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let rob = match round.pending_action.as_ref() {
        Some(PendingAction::RobKongWindow(rob)) => rob,
        _ => return Err("invalid_action".to_string()),
    };
    let actor_seat = rob.actor_seat;
    let tile_id = rob
        .tile_id
        .clone()
        .ok_or_else(|| "invalid_action".to_string())?;
    let tile_key = rob
        .tile_key
        .clone()
        .ok_or_else(|| "invalid_action".to_string())?;
    let meld_index = rob.meld_index;
    let replacement_tile = tile_from_value(
        &replacement_tile_from_tail(room).ok_or_else(|| "invalid_action".to_string())?,
    )?;
    let selection = SelfKongCandidate {
        kind: SelfKongKind::Add,
        tile_ids: vec![tile_id],
        tile_key,
        meld_index,
    };
    let drawn_tile_key = Some(replacement_tile.tile_key.clone());
    let plan = plan_self_kong_completion(room, actor_seat, &selection, replacement_tile, true)?;
    apply_legacy_room_mutations(room, &plan.mutations)?;
    if let Some(tile_key) = drawn_tile_key.as_deref() {
        note_tracker_draw(room, actor_seat, tile_key);
    }
    sync_round_skill_trackers(room);
    sync_pending_timeout(room);
    Ok(plan.events)
}
