use serde_json::{Value, json};
use std::collections::HashSet;

use crate::core::engine::EngineOutput;
use crate::core::engine::planner::{
    plan_claim_window_continuation_without_winner, plan_claim_window_response, plan_discard_action,
    resolve_claims, resolve_hu_claims,
};
use crate::core::event::GameEvent;
use crate::core::state::{
    ClaimResponse, DisplayMeldOrientation, DisplayMeldState, DisplayMeldTileState,
    KongTrackerEntry, LastActionContext, PendingAction, RobKongWindowAction, RoomState, RoundState,
};
use crate::core::tile::Tile;
use crate::room_scoring::RoomScoringCache;

use super::meld::{
    SelfKongCandidate, SelfKongKind, available_self_kongs_from_cache,
    claim_window_options_after_discard_in_room_state, is_valid_chow_sequence_by_keys,
    resolve_self_kong_selection, seats_with_hu_candidate_for_tile_in_room_state,
};
use super::runtime::{
    current_actor_in_room_state, is_last_live_tile_point_in_room_state,
    replacement_tile_from_tail_in_room_state, round_event_message,
    sync_pending_timeout_in_room_state,
};
use super::settlement::settle_exhaustive_draw_output_in_room_state;
use super::win::{
    apply_hu_settlement_output_in_room_state, compute_hu_settlement_for_state,
    compute_multi_hu_settlement_for_state, settlement_meets_minimum_hu_fan,
};

#[cfg(test)]
use super::meld::{
    available_self_kongs, claim_window_options_after_discard, seats_with_hu_candidate_for_tile,
};
#[cfg(test)]
use super::runtime::{
    current_actor, is_last_live_tile_point, pending_timeout_kind, player_concealed_tile,
    project_room_state, replacement_tile_from_tail,
};
#[cfg(test)]
use super::settlement::settle_exhaustive_draw_output;
#[cfg(test)]
use super::win::{apply_hu_settlement_output, compute_hu_settlement};
#[cfg(test)]
use crate::core::engine::reducer::update_room_state;
#[cfg(test)]
use crate::core::state::ClaimWindowAction;

const MAX_SEATS: usize = 4;

#[cfg(test)]
fn apply_room_state_side_effect<F>(room: &mut Value, effect: F)
where
    F: FnOnce(&mut RoomState),
{
    let _ = update_room_state(room, |state| {
        effect(state);
        Ok(())
    });
}

#[cfg(test)]
fn sync_pending_timeout_for_value_room(room: &mut Value) {
    apply_room_state_side_effect(room, sync_pending_timeout_in_room_state);
}

fn has_discard_response_lock(_round: &RoundState, _seat_index: usize) -> bool {
    false
}

fn consume_discard_response_lock(_round: &mut RoundState, _seat_index: usize) {}

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

fn ready_hand_declared_event(seat_index: usize, tile: &Tile) -> GameEvent {
    GameEvent::ReadyHandDeclared {
        seat: seat_index,
        tile: tile.clone(),
    }
}

fn ready_hand_declared_message(seat_index: usize, tile: &Tile) -> Value {
    round_event_message(
        "ready_hand_declared",
        json!({
            "type": "ready_hand_declared",
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

fn create_normal_display_tile(code: &str) -> DisplayMeldTileState {
    DisplayMeldTileState {
        code: code.to_string(),
        orientation: DisplayMeldOrientation::Normal,
    }
}

fn create_display_meld_from_codes(codes: &[String]) -> DisplayMeldState {
    DisplayMeldState {
        tiles: codes
            .iter()
            .map(|code| create_normal_display_tile(code))
            .collect(),
    }
}

fn create_repeated_display_meld(
    code: &str,
    count: usize,
    special_index: usize,
    orientation: DisplayMeldOrientation,
) -> DisplayMeldState {
    DisplayMeldState {
        tiles: (0..count)
            .map(|index| DisplayMeldTileState {
                code: code.to_string(),
                orientation: if index == special_index {
                    orientation.clone()
                } else {
                    DisplayMeldOrientation::Normal
                },
            })
            .collect(),
    }
}

fn create_claim_display_meld(
    action_type: &str,
    actor_seat: usize,
    source_seat: usize,
    meld: &[String],
    claim_tile_key: &str,
) -> DisplayMeldState {
    if action_type == "pung" || action_type == "kong" {
        let relative_source_seat = (source_seat + MAX_SEATS - actor_seat) % MAX_SEATS;
        let claimed_index = if relative_source_seat == 1 && meld.len() > 1 {
            meld.len() - 1
        } else {
            0
        };
        let claimed_orientation = if relative_source_seat == 2 {
            DisplayMeldOrientation::UpsideDown
        } else {
            DisplayMeldOrientation::Rotated
        };

        return create_repeated_display_meld(
            claim_tile_key,
            meld.len(),
            claimed_index,
            claimed_orientation,
        );
    }

    let mut display_meld = create_display_meld_from_codes(meld);
    if let Some(claimed_index) = meld.iter().position(|code| code == claim_tile_key) {
        if let Some(tile) = display_meld.tiles.get_mut(claimed_index) {
            tile.orientation = DisplayMeldOrientation::Rotated;
        }
    }
    display_meld
}

fn create_concealed_kong_display_meld(tile_key: &str) -> DisplayMeldState {
    DisplayMeldState {
        tiles: (0..4)
            .map(|index| DisplayMeldTileState {
                code: tile_key.to_string(),
                orientation: if index == 0 || index == 3 {
                    DisplayMeldOrientation::FaceDown
                } else {
                    DisplayMeldOrientation::Normal
                },
            })
            .collect(),
    }
}

fn upgrade_add_kong_display_meld(
    existing_display_meld: Option<&DisplayMeldState>,
    existing_meld: Option<&Vec<String>>,
    tile_key: &str,
) -> DisplayMeldState {
    let base_tiles = existing_display_meld
        .map(|meld| meld.tiles.clone())
        .or_else(|| {
            existing_meld.map(|meld| {
                create_display_meld_from_codes(meld)
                    .tiles
                    .into_iter()
                    .collect::<Vec<_>>()
            })
        })
        .unwrap_or_else(|| {
            vec![
                create_normal_display_tile(tile_key),
                create_normal_display_tile(tile_key),
                create_normal_display_tile(tile_key),
            ]
        });

    let mut tiles = Vec::with_capacity(base_tiles.len() + 1);
    tiles.push(DisplayMeldTileState {
        code: tile_key.to_string(),
        orientation: DisplayMeldOrientation::FaceDown,
    });
    tiles.extend(base_tiles);

    DisplayMeldState { tiles }
}

fn round_state_mut(state: &mut RoomState) -> Result<&mut RoundState, String> {
    state
        .round_state
        .as_mut()
        .ok_or_else(|| "invalid_action".to_string())
}

fn round_player_mut(
    round: &mut RoundState,
    seat_index: usize,
) -> Result<&mut crate::core::state::PlayerRoundState, String> {
    round
        .players
        .get_mut(seat_index)
        .ok_or_else(|| "invalid_action".to_string())
}

fn empty_claim_window() -> Vec<Vec<String>> {
    vec![Vec::new(); MAX_SEATS]
}

fn remove_player_concealed_tile(
    round: &mut RoundState,
    seat_index: usize,
    tile_id: &str,
) -> Result<Tile, String> {
    let player = round_player_mut(round, seat_index)?;
    let tile_index = player
        .concealed_tiles
        .iter()
        .position(|tile| tile.tile_id == tile_id)
        .ok_or_else(|| "invalid_action".to_string())?;
    Ok(player.concealed_tiles.remove(tile_index))
}

fn apply_discard_to_round(
    round: &mut RoundState,
    seat_index: usize,
    discarded_tile: &Tile,
) -> Result<(), String> {
    let removed_tile = remove_player_concealed_tile(round, seat_index, &discarded_tile.tile_id)?;
    if removed_tile.tile_id != discarded_tile.tile_id {
        return Err("invalid_action".to_string());
    }
    round_player_mut(round, seat_index)?
        .discards
        .push(discarded_tile.clone());
    round.last_discard = Some(discarded_tile.clone());
    Ok(())
}

fn apply_discard_continuation_to_round(
    round: &mut RoundState,
    continuation: &crate::core::engine::planner::PlannedDiscardContinuation,
) -> Result<(), String> {
    if let Some(tile) = continuation.drawn_tile.as_ref() {
        round.wall.head_index += 1;
        round_player_mut(round, continuation.current_actor)?
            .concealed_tiles
            .push(tile.clone());
    }
    round.pending_action = continuation.pending_action.clone();
    round.restricted_discard_tile_key = None;
    round.last_action_context = continuation.last_action_context.clone();
    round.version += 1;
    round.current_actor = continuation.current_actor;
    Ok(())
}

fn push_kong_entry(round: &mut RoundState, entry: &KongTrackerEntry) {
    round.score_trackers.kong_entries.push(entry.clone());
}

#[cfg(test)]
#[allow(dead_code)]
pub fn try_handle_self_kong_action_output(
    room: &mut Value,
    seat_index: usize,
    tile_ids: &[String],
) -> Option<Result<EngineOutput, String>> {
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
            return Some(start_rob_kong_window_output(
                room,
                seat_index,
                &selection,
                offered_hu_seats,
            ));
        }
    }
    Some(apply_self_kong_action_output(room, seat_index, &selection))
}

pub fn try_handle_self_kong_action_output_in_room_state(
    room: &mut RoomState,
    seat_index: usize,
    tile_ids: &[String],
) -> Result<Option<Result<EngineOutput, String>>, String> {
    let cache = RoomScoringCache::from_state(room);
    let candidates = available_self_kongs_from_cache(&cache, seat_index);
    if candidates.is_empty() {
        return Ok(Some(Err("invalid_action".to_string())));
    }
    let Some(selection) = resolve_self_kong_selection(&candidates, tile_ids) else {
        return Ok(Some(Err("invalid_action".to_string())));
    };
    replacement_tile_from_tail_in_room_state(room).ok_or_else(|| "invalid_action".to_string())?;
    if selection.kind == SelfKongKind::Add {
        let offered_hu_seats =
            seats_with_hu_candidate_for_tile_in_room_state(room, seat_index, &selection.tile_key);
        if !offered_hu_seats.is_empty() {
            return Ok(Some(start_rob_kong_window_output_in_room_state(
                room,
                seat_index,
                &selection,
                offered_hu_seats,
            )));
        }
    }
    Ok(Some(complete_self_kong_output_in_room_state(
        room, seat_index, &selection,
    )))
}

#[cfg(test)]
#[allow(dead_code)]
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
    update_room_state(room, |state| {
        let round = state
            .round_state
            .as_mut()
            .ok_or_else(|| "invalid_action".to_string())?;
        round.pending_action = Some(plan.pending_action.clone());
        round.version += 1;
        Ok(())
    })?;
    if !plan.unresolved_seats.is_empty() {
        sync_pending_timeout_for_value_room(room);
        return Ok(EngineOutput::default());
    }
    resolve_recorded_claims_local_output(room)
}

pub fn apply_claim_window_action_in_room_state(
    room: &mut RoomState,
    seat_index: usize,
    action_type: &str,
    tile_ids: &[String],
) -> Result<EngineOutput, String> {
    if matches!(action_type, "chow" | "pung" | "kong") {
        validate_claim_selection_in_room_state(room, seat_index, action_type, tile_ids)?;
    }
    let plan = plan_claim_window_response(room, seat_index, action_type, tile_ids)?;
    {
        let round = room
            .round_state
            .as_mut()
            .ok_or_else(|| "invalid_action".to_string())?;
        round.pending_action = Some(plan.pending_action.clone());
        round.version += 1;
    }
    if !plan.unresolved_seats.is_empty() {
        sync_pending_timeout_in_room_state(room);
        return Ok(EngineOutput::default());
    }
    resolve_recorded_claims_local_output_in_room_state(room)
}

#[cfg(test)]
pub fn apply_discard_action(
    room: &mut Value,
    seat_index: usize,
    tile_id: &str,
) -> Result<Vec<Value>, String> {
    apply_discard_action_output(room, seat_index, tile_id).map(|output| output.emitted_messages)
}

#[cfg(test)]
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
    let mut simulated = room.clone();
    update_room_state(&mut simulated, |state| {
        let round = round_state_mut(state)?;
        apply_discard_to_round(round, seat_index, &discarded_tile)
    })?;

    let previous_was_last_live_tile = round.last_action_context.was_last_live_tile;
    let discard_response_locked = has_discard_response_lock(round, seat_index);
    let claim_window = if discard_response_locked {
        empty_claim_window()
    } else {
        claim_window_options_after_discard(&simulated, seat_index, &discarded_tile.tile_key)
    };
    let plan = plan_discard_action(
        &state,
        seat_index,
        tile_id,
        claim_window,
        previous_was_last_live_tile,
    )?;
    update_room_state(room, |state| {
        let round = round_state_mut(state)?;
        apply_discard_to_round(round, seat_index, &plan.discarded_tile)?;
        if discard_response_locked {
            consume_discard_response_lock(round, seat_index);
        }
        Ok(())
    })?;
    if plan.continuation.needs_exhaustive_draw {
        sync_pending_timeout_for_value_room(room);
        let discard_message = tile_discarded_message(seat_index, &plan.discarded_tile);
        let settlement_output = settle_exhaustive_draw_output(room);
        let mut events = vec![tile_discarded_event(seat_index, &plan.discarded_tile)];
        events.extend(settlement_output.events);
        let mut emitted_messages = vec![discard_message];
        emitted_messages.extend(settlement_output.emitted_messages);
        return Ok(EngineOutput::new(events, emitted_messages));
    }
    update_room_state(room, |state| {
        let round = round_state_mut(state)?;
        apply_discard_continuation_to_round(round, &plan.continuation)
    })?;
    let updated_state = project_room_state(room)?;
    let updated_round = round_state_ref(&updated_state)?;
    let next_actor = updated_round.current_actor;
    let _drawn_tile_key = updated_round
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
    sync_pending_timeout_for_value_room(room);
    Ok(EngineOutput::new(
        vec![tile_discarded_event(seat_index, &plan.discarded_tile)],
        vec![tile_discarded_message(seat_index, &plan.discarded_tile)],
    ))
}

pub fn apply_discard_action_output_in_room_state(
    room: &mut RoomState,
    seat_index: usize,
    tile_id: &str,
) -> Result<EngineOutput, String> {
    if room.phase != "playing" {
        return Err("round_not_ready".to_string());
    }
    let round = round_state_ref(room)?;
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
    let mut simulated = room.clone();
    {
        let round = round_state_mut(&mut simulated)?;
        apply_discard_to_round(round, seat_index, &discarded_tile)?;
    }

    let previous_was_last_live_tile = round.last_action_context.was_last_live_tile;
    let discard_response_locked = has_discard_response_lock(round, seat_index);
    let claim_window = if discard_response_locked {
        empty_claim_window()
    } else {
        claim_window_options_after_discard_in_room_state(
            &simulated,
            seat_index,
            &discarded_tile.tile_key,
        )
    };
    let plan = plan_discard_action(
        room,
        seat_index,
        tile_id,
        claim_window,
        previous_was_last_live_tile,
    )?;
    {
        let round = round_state_mut(room)?;
        apply_discard_to_round(round, seat_index, &plan.discarded_tile)?;
        if discard_response_locked {
            consume_discard_response_lock(round, seat_index);
        }
    }
    if plan.continuation.needs_exhaustive_draw {
        let discard_message = tile_discarded_message(seat_index, &plan.discarded_tile);
        let settlement_output = settle_exhaustive_draw_output_in_room_state(room);
        let mut events = vec![tile_discarded_event(seat_index, &plan.discarded_tile)];
        events.extend(settlement_output.events);
        let mut emitted_messages = vec![discard_message];
        emitted_messages.extend(settlement_output.emitted_messages);
        return Ok(EngineOutput::new(events, emitted_messages));
    }
    {
        let round = round_state_mut(room)?;
        apply_discard_continuation_to_round(round, &plan.continuation)?;
    }
    let updated_round = round_state_ref(room)?;
    let next_actor = updated_round.current_actor;
    let _drawn_tile_key = updated_round
        .last_action_context
        .tile_id
        .as_deref()
        .and_then(|drawn_tile_id| {
            updated_round
                .players
                .get(next_actor)
                .and_then(|player| {
                    player
                        .concealed_tiles
                        .iter()
                        .find(|tile| tile.tile_id == drawn_tile_id)
                })
                .map(|tile| tile.tile_key.clone())
        });
    sync_pending_timeout_in_room_state(room);
    Ok(EngineOutput::new(
        vec![tile_discarded_event(seat_index, &plan.discarded_tile)],
        vec![tile_discarded_message(seat_index, &plan.discarded_tile)],
    ))
}

pub fn apply_ready_hand_action_output_in_room_state(
    room: &mut RoomState,
    seat_index: usize,
    tile_id: &str,
) -> Result<EngineOutput, String> {
    if !crate::rules::standard::ready_hand::can_declare_ready_hand_with_tile_id(
        room, seat_index, tile_id,
    ) {
        return Err("invalid_action".to_string());
    }

    let discard_output = apply_discard_action_output_in_room_state(room, seat_index, tile_id)?;
    let discarded_tile = room
        .round_state
        .as_ref()
        .and_then(|round| round.players.get(seat_index))
        .and_then(|player| player.discards.last())
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;

    room.round_state
        .as_mut()
        .and_then(|round| round.players.get_mut(seat_index))
        .ok_or_else(|| "invalid_action".to_string())?
        .is_ready_hand = true;

    let mut events = discard_output.events;
    events.push(ready_hand_declared_event(seat_index, &discarded_tile));
    let mut emitted_messages = discard_output.emitted_messages;
    emitted_messages.push(ready_hand_declared_message(seat_index, &discarded_tile));
    Ok(EngineOutput::new(events, emitted_messages))
}

#[cfg(test)]
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

#[cfg(test)]
pub fn claim_window_supported_locally(room: &Value) -> bool {
    project_room_state(room)
        .ok()
        .and_then(|state| state.round_state)
        .and_then(|round| round.pending_action)
        .is_some_and(|pending| matches!(pending, PendingAction::ClaimWindow(_)))
}

#[cfg(test)]
pub fn rob_kong_window_supported_locally(room: &Value) -> bool {
    project_room_state(room)
        .ok()
        .and_then(|state| state.round_state)
        .and_then(|round| round.pending_action)
        .is_some_and(|pending| matches!(pending, PendingAction::RobKongWindow(_)))
}

#[cfg(test)]
pub fn can_resolve_claim_window_timeout_locally(room: &Value) -> bool {
    pending_timeout_kind(room) == Some("claim_window") && claim_window_supported_locally(room)
}

#[cfg(test)]
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
    update_room_state(room, |state| {
        let round = state
            .round_state
            .as_mut()
            .ok_or_else(|| "invalid_action".to_string())?;
        round.pending_action = Some(PendingAction::ClaimWindow(ClaimWindowAction {
            discarder_seat,
            claim_window: claim.claim_window.clone(),
            responded_seats,
            claim_responses: claim.claim_responses.clone(),
        }));
        round.version += 1;
        Ok(())
    })?;
    sync_pending_timeout_for_value_room(room);

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

#[cfg(test)]
pub fn can_resolve_rob_kong_timeout_locally(room: &Value) -> bool {
    pending_timeout_kind(room) == Some("claim_window") && rob_kong_window_supported_locally(room)
}

#[cfg(test)]
#[allow(dead_code)]
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
    update_room_state(room, |state| {
        let round = state
            .round_state
            .as_mut()
            .ok_or_else(|| "invalid_action".to_string())?;
        round.pending_action = Some(PendingAction::RobKongWindow(RobKongWindowAction {
            actor_seat: rob.actor_seat,
            tile_id: rob.tile_id.clone(),
            tile_key: rob.tile_key.clone(),
            meld_index: rob.meld_index,
            offered_hu_seats: rob.offered_hu_seats.clone(),
            responded_seats: next_responded.clone(),
            claim_responses: rob.claim_responses.clone(),
        }));
        round.version += 1;
        Ok(())
    })?;

    let unresolved = rob
        .offered_hu_seats
        .iter()
        .copied()
        .filter(|offered_seat| !next_responded.contains(offered_seat))
        .collect::<Vec<_>>();
    if !unresolved.is_empty() {
        sync_pending_timeout_for_value_room(room);
        return Ok(EngineOutput::default());
    }
    let winner_seats = resolve_hu_claims(&rob.claim_responses, rob.actor_seat)
        .into_iter()
        .map(|response| response.seat)
        .collect::<Vec<_>>();
    if !winner_seats.is_empty() {
        let state = project_room_state(room)?;
        let settlement = compute_multi_hu_settlement_for_state(&state, &winner_seats)?;
        return apply_hu_settlement_output(room, winner_seats[0], "discard", settlement);
    }
    complete_add_kong_after_passes_output(room)
}

#[cfg(test)]
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
    let claim_responses = rob.claim_responses.clone();
    let unresolved: Vec<Value> = unresolved_seats
        .iter()
        .map(|seat| Value::Number((*seat as u64).into()))
        .collect();
    let mut next_responded = rob.responded_seats.clone();
    next_responded.extend(unresolved_seats);
    update_room_state(room, |state| {
        let round = state
            .round_state
            .as_mut()
            .ok_or_else(|| "invalid_action".to_string())?;
        round.pending_action = Some(PendingAction::RobKongWindow(RobKongWindowAction {
            actor_seat,
            tile_id: rob.tile_id.clone(),
            tile_key: rob.tile_key.clone(),
            meld_index: rob.meld_index,
            offered_hu_seats: rob.offered_hu_seats.clone(),
            responded_seats: next_responded,
            claim_responses: rob.claim_responses.clone(),
        }));
        round.version += 1;
        Ok(())
    })?;
    sync_pending_timeout_for_value_room(room);
    let mut messages = vec![round_event_message(
        "rob_kong_auto_passed",
        json!({
            "type": "rob_kong_auto_passed",
            "actor_seat": actor_seat,
            "seats": unresolved,
        }),
    )];
    let winner_seats = resolve_hu_claims(&claim_responses, actor_seat)
        .into_iter()
        .map(|response| response.seat)
        .collect::<Vec<_>>();
    if !winner_seats.is_empty() {
        let state = project_room_state(room)?;
        let settlement = compute_multi_hu_settlement_for_state(&state, &winner_seats)?;
        let output = apply_hu_settlement_output(room, winner_seats[0], "discard", settlement)?;
        messages.extend(output.emitted_messages);
        return Ok(messages);
    }
    messages.extend(complete_add_kong_after_passes(room)?);
    Ok(messages)
}

#[cfg(test)]
#[allow(dead_code)]
fn apply_self_kong_action_output(
    room: &mut Value,
    seat_index: usize,
    selection: &SelfKongCandidate,
) -> Result<EngineOutput, String> {
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
    complete_self_kong_output(room, seat_index, selection, replacement_tile)
}

#[allow(dead_code)]
#[cfg(test)]
fn complete_self_kong_output(
    room: &mut Value,
    seat_index: usize,
    selection: &SelfKongCandidate,
    replacement_tile: Value,
) -> Result<EngineOutput, String> {
    let replacement_tile = tile_from_value(&replacement_tile)?;
    let plan =
        plan_self_kong_completion(room, seat_index, selection, replacement_tile.clone(), false)?;
    update_room_state(room, |state| {
        let round = round_state_mut(state)?;
        apply_self_kong_plan_to_round(round, seat_index, &plan)
    })?;
    sync_pending_timeout_for_value_room(room);
    Ok(EngineOutput::new(
        vec![
            self_kong_declared_event(
                seat_index,
                self_kong_kind_name(selection.kind),
                &selection.tile_key,
                &selection.tile_ids,
            ),
            replacement_draw_event(seat_index, &replacement_tile),
        ],
        plan.emitted_messages,
    ))
}

fn complete_self_kong_output_in_room_state(
    room: &mut RoomState,
    seat_index: usize,
    selection: &SelfKongCandidate,
) -> Result<EngineOutput, String> {
    if room.phase != "playing" {
        return Err("round_not_ready".to_string());
    }
    if current_actor_in_room_state(room) != Some(seat_index) {
        return Err("not_your_turn".to_string());
    }
    if room
        .round_state
        .as_ref()
        .and_then(|round| round.pending_action.as_ref())
        .is_some()
    {
        return Err("invalid_action".to_string());
    }
    if is_last_live_tile_point_in_room_state(room) {
        return Err("invalid_action".to_string());
    }

    let replacement_tile = replacement_tile_from_tail_in_room_state(room)
        .ok_or_else(|| "invalid_action".to_string())?;
    let plan = plan_self_kong_completion_in_room_state(
        room,
        seat_index,
        selection,
        replacement_tile.clone(),
        false,
    )?;
    {
        let round = round_state_mut(room)?;
        apply_self_kong_plan_to_round(round, seat_index, &plan)?;
    }
    sync_pending_timeout_in_room_state(room);
    Ok(EngineOutput::new(
        vec![
            self_kong_declared_event(
                seat_index,
                self_kong_kind_name(selection.kind),
                &selection.tile_key,
                &selection.tile_ids,
            ),
            replacement_draw_event(seat_index, &replacement_tile),
        ],
        plan.emitted_messages,
    ))
}

#[allow(dead_code)]
#[cfg(test)]
fn start_rob_kong_window_output(
    room: &mut Value,
    seat_index: usize,
    selection: &SelfKongCandidate,
    offered_hu_seats: Vec<usize>,
) -> Result<EngineOutput, String> {
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
    update_room_state(room, |state| {
        let round = round_state_mut(state)?;
        round.last_discard = Some(selected_tile.clone());
        round.pending_action = Some(PendingAction::RobKongWindow(RobKongWindowAction {
            actor_seat: seat_index,
            tile_id: Some(selected_tile.tile_id.clone()),
            tile_key: Some(selected_tile.tile_key.clone()),
            meld_index: selection.meld_index,
            offered_hu_seats: offered_hu_seats.clone(),
            responded_seats: vec![],
            claim_responses: vec![],
        }));
        round.version += 1;
        Ok(())
    })?;
    sync_pending_timeout_for_value_room(room);
    let kong_type = "add_kong";
    let event = self_kong_declared_payload(
        seat_index,
        kong_type,
        &selection.tile_key,
        &selection.tile_ids,
    );
    Ok(EngineOutput::new(
        vec![GameEvent::SelfKongDeclared {
            seat: seat_index,
            kong_type: kong_type.to_string(),
            tile_key: selection.tile_key.clone(),
            tile_ids: selection.tile_ids.clone(),
        }],
        vec![round_event_message("self_kong_declared", event)],
    ))
}

fn start_rob_kong_window_output_in_room_state(
    room: &mut RoomState,
    seat_index: usize,
    selection: &SelfKongCandidate,
    offered_hu_seats: Vec<usize>,
) -> Result<EngineOutput, String> {
    let selected_tile = room
        .round_state
        .as_ref()
        .and_then(|round| round.players.get(seat_index))
        .and_then(|player| {
            player.concealed_tiles.iter().find(|tile| {
                Some(tile.tile_id.as_str()) == selection.tile_ids.first().map(String::as_str)
            })
        })
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;
    {
        let round = round_state_mut(room)?;
        round.last_discard = Some(selected_tile.clone());
        round.pending_action = Some(PendingAction::RobKongWindow(RobKongWindowAction {
            actor_seat: seat_index,
            tile_id: Some(selected_tile.tile_id.clone()),
            tile_key: Some(selected_tile.tile_key.clone()),
            meld_index: selection.meld_index,
            offered_hu_seats: offered_hu_seats.clone(),
            responded_seats: vec![],
            claim_responses: vec![],
        }));
        round.version += 1;
    }
    sync_pending_timeout_in_room_state(room);
    let kong_type = "add_kong";
    let event = self_kong_declared_payload(
        seat_index,
        kong_type,
        &selection.tile_key,
        &selection.tile_ids,
    );
    Ok(EngineOutput::new(
        vec![GameEvent::SelfKongDeclared {
            seat: seat_index,
            kong_type: kong_type.to_string(),
            tile_key: selection.tile_key.clone(),
            tile_ids: selection.tile_ids.clone(),
        }],
        vec![round_event_message("self_kong_declared", event)],
    ))
}

struct SelfKongPlan {
    removed_tile_ids: Vec<String>,
    meld_update: SelfKongMeldUpdate,
    display_meld_update: SelfKongDisplayMeldUpdate,
    replacement_tile: Tile,
    kong_entry: KongTrackerEntry,
    last_action_context: LastActionContext,
    clear_pending_action: bool,
    emitted_messages: Vec<Value>,
}

enum SelfKongMeldUpdate {
    Push(Vec<String>),
    Append { meld_index: usize, tile_key: String },
}

enum SelfKongDisplayMeldUpdate {
    Push(DisplayMeldState),
    AppendFaceDown { meld_index: usize, tile_key: String },
}

#[cfg(test)]
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

    let meld_update = match selection.kind {
        SelfKongKind::Concealed => SelfKongMeldUpdate::Push(vec![
            selection.tile_key.clone(),
            selection.tile_key.clone(),
            selection.tile_key.clone(),
            selection.tile_key.clone(),
        ]),
        SelfKongKind::Add => SelfKongMeldUpdate::Append {
            meld_index: selection
                .meld_index
                .ok_or_else(|| "invalid_action".to_string())?,
            tile_key: selection.tile_key.clone(),
        },
    };
    let display_meld_update = match selection.kind {
        SelfKongKind::Concealed => {
            SelfKongDisplayMeldUpdate::Push(create_concealed_kong_display_meld(&selection.tile_key))
        }
        SelfKongKind::Add => SelfKongDisplayMeldUpdate::AppendFaceDown {
            meld_index: selection
                .meld_index
                .ok_or_else(|| "invalid_action".to_string())?,
            tile_key: selection.tile_key.clone(),
        },
    };

    let kong_entry = KongTrackerEntry {
        kong_type: match selection.kind {
            SelfKongKind::Concealed => "concealed_kong".to_string(),
            SelfKongKind::Add => "add_kong".to_string(),
        },
        actor_seat: seat_index,
        payer_seats: (0..MAX_SEATS)
            .filter(|other| *other != seat_index)
            .collect(),
        tile_key: Some(selection.tile_key.clone()),
    };
    let last_action_context = LastActionContext {
        kind: "replacement_draw".to_string(),
        seat: seat_index,
        tile_id: Some(replacement_tile.tile_id.clone()),
        from_kong_replacement: true,
        was_last_live_tile: false,
        was_last_discard: false,
    };

    Ok(SelfKongPlan {
        removed_tile_ids: selection.tile_ids.clone(),
        meld_update,
        display_meld_update,
        replacement_tile: replacement_tile.clone(),
        kong_entry,
        last_action_context,
        clear_pending_action,
        emitted_messages: vec![
            round_event_message(
                "self_kong_declared",
                self_kong_declared_payload(
                    seat_index,
                    self_kong_kind_name(selection.kind),
                    &selection.tile_key,
                    &selection.tile_ids,
                ),
            ),
            replacement_draw_message(seat_index, &replacement_tile),
        ],
    })
}

fn plan_self_kong_completion_in_room_state(
    room: &RoomState,
    seat_index: usize,
    selection: &SelfKongCandidate,
    replacement_tile: Tile,
    clear_pending_action: bool,
) -> Result<SelfKongPlan, String> {
    let round = room
        .round_state
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let player = round
        .players
        .get(seat_index)
        .ok_or_else(|| "invalid_action".to_string())?;
    for tile_id in &selection.tile_ids {
        if !player
            .concealed_tiles
            .iter()
            .any(|tile| tile.tile_id == tile_id.as_str())
        {
            return Err("invalid_action".to_string());
        }
    }

    let meld_update = match selection.kind {
        SelfKongKind::Concealed => SelfKongMeldUpdate::Push(vec![
            selection.tile_key.clone(),
            selection.tile_key.clone(),
            selection.tile_key.clone(),
            selection.tile_key.clone(),
        ]),
        SelfKongKind::Add => SelfKongMeldUpdate::Append {
            meld_index: selection
                .meld_index
                .ok_or_else(|| "invalid_action".to_string())?,
            tile_key: selection.tile_key.clone(),
        },
    };
    let display_meld_update = match selection.kind {
        SelfKongKind::Concealed => {
            SelfKongDisplayMeldUpdate::Push(create_concealed_kong_display_meld(&selection.tile_key))
        }
        SelfKongKind::Add => SelfKongDisplayMeldUpdate::AppendFaceDown {
            meld_index: selection
                .meld_index
                .ok_or_else(|| "invalid_action".to_string())?,
            tile_key: selection.tile_key.clone(),
        },
    };

    let kong_entry = KongTrackerEntry {
        kong_type: match selection.kind {
            SelfKongKind::Concealed => "concealed_kong".to_string(),
            SelfKongKind::Add => "add_kong".to_string(),
        },
        actor_seat: seat_index,
        payer_seats: (0..MAX_SEATS)
            .filter(|other| *other != seat_index)
            .collect(),
        tile_key: Some(selection.tile_key.clone()),
    };
    let last_action_context = LastActionContext {
        kind: "replacement_draw".to_string(),
        seat: seat_index,
        tile_id: Some(replacement_tile.tile_id.clone()),
        from_kong_replacement: true,
        was_last_live_tile: false,
        was_last_discard: false,
    };

    Ok(SelfKongPlan {
        removed_tile_ids: selection.tile_ids.clone(),
        meld_update,
        display_meld_update,
        replacement_tile: replacement_tile.clone(),
        kong_entry,
        last_action_context,
        clear_pending_action,
        emitted_messages: vec![
            round_event_message(
                "self_kong_declared",
                self_kong_declared_payload(
                    seat_index,
                    self_kong_kind_name(selection.kind),
                    &selection.tile_key,
                    &selection.tile_ids,
                ),
            ),
            replacement_draw_message(seat_index, &replacement_tile),
        ],
    })
}

fn apply_self_kong_plan_to_round(
    round: &mut RoundState,
    seat_index: usize,
    plan: &SelfKongPlan,
) -> Result<(), String> {
    for tile_id in &plan.removed_tile_ids {
        remove_player_concealed_tile(round, seat_index, tile_id)?;
    }
    {
        let player = round_player_mut(round, seat_index)?;
        let append_display_meld = match &plan.display_meld_update {
            SelfKongDisplayMeldUpdate::AppendFaceDown {
                meld_index,
                tile_key,
            } => Some((
                *meld_index,
                upgrade_add_kong_display_meld(
                    player.display_melds.get(*meld_index),
                    player.melds.get(*meld_index),
                    tile_key,
                ),
            )),
            SelfKongDisplayMeldUpdate::Push(_) => None,
        };
        match &plan.meld_update {
            SelfKongMeldUpdate::Push(meld) => player.melds.push(meld.clone()),
            SelfKongMeldUpdate::Append {
                meld_index,
                tile_key,
            } => {
                let meld = player
                    .melds
                    .get_mut(*meld_index)
                    .ok_or_else(|| "invalid_action".to_string())?;
                meld.push(tile_key.clone());
            }
        }
        match &plan.display_meld_update {
            SelfKongDisplayMeldUpdate::Push(display_meld) => {
                player.display_melds.push(display_meld.clone());
            }
            SelfKongDisplayMeldUpdate::AppendFaceDown { meld_index, .. } => {
                let updated_display_meld = append_display_meld
                    .as_ref()
                    .map(|(_, meld)| meld.clone())
                    .ok_or_else(|| "invalid_action".to_string())?;
                if let Some(display_meld) = player.display_melds.get_mut(*meld_index) {
                    *display_meld = updated_display_meld;
                } else {
                    while player.display_melds.len() < *meld_index {
                        let fallback_index = player.display_melds.len();
                        let fallback_display_meld = player
                            .melds
                            .get(fallback_index)
                            .map(|meld| create_display_meld_from_codes(meld))
                            .unwrap_or_default();
                        player.display_melds.push(fallback_display_meld);
                    }
                    player.display_melds.push(updated_display_meld);
                }
            }
        }
        player.concealed_tiles.push(plan.replacement_tile.clone());
    }
    round.wall.tail_index = round.wall.tail_index.saturating_sub(1);
    push_kong_entry(round, &plan.kong_entry);
    round.last_action_context = plan.last_action_context.clone();
    if plan.clear_pending_action {
        round.pending_action = None;
    }
    round.version += 1;
    Ok(())
}

#[cfg(test)]
fn resolve_recorded_claims_local(room: &mut Value) -> Result<Vec<Value>, String> {
    resolve_recorded_claims_local_output(room).map(|output| output.emitted_messages)
}

#[cfg(test)]
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
    let hu_winners = resolve_hu_claims(&claim_responses, discarder_seat)
        .into_iter()
        .map(|response| response.seat)
        .collect::<Vec<_>>();

    if !hu_winners.is_empty() {
        let settlement = compute_multi_hu_settlement_for_state(&state, &hu_winners)?;
        return apply_hu_settlement_output(room, hu_winners[0], "discard", settlement);
    }

    if let Some(winner) = resolve_claims(&claim_responses, discarder_seat) {
        return apply_selected_claim(room, winner.seat, &winner.action_type, &winner.tiles);
    }

    let state = project_room_state(room)?;
    let plan = plan_claim_window_continuation_without_winner(&state, discarder_seat)?;
    if plan.needs_exhaustive_draw() {
        sync_pending_timeout_for_value_room(room);
        return Ok(settle_exhaustive_draw_output(room));
    }
    update_room_state(room, |state| {
        let round = state
            .round_state
            .as_mut()
            .ok_or_else(|| "invalid_action".to_string())?;
        match &plan.outcome {
            crate::core::engine::planner::PlannedClaimWindowOutcome::ExhaustiveDraw => {}
            crate::core::engine::planner::PlannedClaimWindowOutcome::AdvanceTurn {
                current_actor,
                drawn_tile,
                last_action_context,
            } => {
                round.wall.head_index += 1;
                let player = round
                    .players
                    .get_mut(*current_actor)
                    .ok_or_else(|| "invalid_action".to_string())?;
                player.concealed_tiles.push(drawn_tile.clone());
                round.pending_action = None;
                round.current_actor = *current_actor;
                round.last_action_context = last_action_context.clone();
                round.version += 1;
            }
        }
        Ok(())
    })?;
    let updated_state = project_room_state(room)?;
    let updated_round = round_state_ref(&updated_state)?;
    let seat = updated_round.current_actor;
    let _drawn_tile_key = updated_round
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
    sync_pending_timeout_for_value_room(room);
    Ok(EngineOutput::default())
}

fn resolve_recorded_claims_local_output_in_room_state(
    room: &mut RoomState,
) -> Result<EngineOutput, String> {
    let round = room
        .round_state
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let claim = match round.pending_action.as_ref() {
        Some(PendingAction::ClaimWindow(claim)) => claim,
        _ => return Err("invalid_action".to_string()),
    };
    let discarder_seat = claim.discarder_seat;
    let claim_responses = claim.claim_responses.clone();
    let hu_winners = resolve_hu_claims(&claim_responses, discarder_seat)
        .into_iter()
        .map(|response| response.seat)
        .collect::<Vec<_>>();

    if !hu_winners.is_empty() {
        let settlement = compute_multi_hu_settlement_for_state(room, &hu_winners)?;
        return apply_hu_settlement_output_in_room_state(
            room,
            hu_winners[0],
            "discard",
            settlement,
        );
    }

    if let Some(winner) = resolve_claims(&claim_responses, discarder_seat) {
        return apply_selected_claim_in_room_state(
            room,
            winner.seat,
            &winner.action_type,
            &winner.tiles,
        );
    }

    let plan = plan_claim_window_continuation_without_winner(room, discarder_seat)?;
    if plan.needs_exhaustive_draw() {
        return Ok(settle_exhaustive_draw_output_in_room_state(room));
    }
    {
        let round = room
            .round_state
            .as_mut()
            .ok_or_else(|| "invalid_action".to_string())?;
        match &plan.outcome {
            crate::core::engine::planner::PlannedClaimWindowOutcome::ExhaustiveDraw => {}
            crate::core::engine::planner::PlannedClaimWindowOutcome::AdvanceTurn {
                current_actor,
                drawn_tile,
                last_action_context,
            } => {
                round.wall.head_index += 1;
                let player = round
                    .players
                    .get_mut(*current_actor)
                    .ok_or_else(|| "invalid_action".to_string())?;
                player.concealed_tiles.push(drawn_tile.clone());
                round.pending_action = None;
                round.current_actor = *current_actor;
                round.last_action_context = last_action_context.clone();
                round.version += 1;
            }
        }
    }
    let updated_round = round_state_ref(room)?;
    let seat = updated_round.current_actor;
    let _drawn_tile_key = updated_round
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
    sync_pending_timeout_in_room_state(room);
    Ok(EngineOutput::default())
}

#[cfg(test)]
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

fn validate_claim_selection_in_room_state(
    room: &RoomState,
    seat_index: usize,
    action_type: &str,
    tile_ids: &[String],
) -> Result<(), String> {
    let round = round_state_ref(room)?;
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

#[cfg(test)]
fn apply_selected_claim(
    room: &mut Value,
    seat_index: usize,
    action_type: &str,
    tile_ids: &[String],
) -> Result<EngineOutput, String> {
    if action_type == "hu" {
        let settlement = compute_hu_settlement(room, seat_index, "discard")?;
        if !settlement_meets_minimum_hu_fan(&settlement) {
            return Err("invalid_action".to_string());
        }
        return apply_hu_settlement_output(room, seat_index, "discard", settlement);
    }
    let plan = plan_selected_claim(room, seat_index, action_type, tile_ids)?;
    update_room_state(room, |state| {
        let round = round_state_mut(state)?;
        apply_selected_claim_plan_to_round(round, seat_index, &plan)
    })?;
    sync_pending_timeout_for_value_room(room);
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

fn apply_selected_claim_in_room_state(
    room: &mut RoomState,
    seat_index: usize,
    action_type: &str,
    tile_ids: &[String],
) -> Result<EngineOutput, String> {
    if action_type == "hu" {
        let settlement = compute_hu_settlement_for_state(room, seat_index, "discard")?;
        if !settlement_meets_minimum_hu_fan(&settlement) {
            return Err("invalid_action".to_string());
        }
        return apply_hu_settlement_output_in_room_state(room, seat_index, "discard", settlement);
    }
    let plan = plan_selected_claim_in_room_state(room, seat_index, action_type, tile_ids)?;
    {
        let round = round_state_mut(room)?;
        apply_selected_claim_plan_to_round(round, seat_index, &plan)?;
    }
    sync_pending_timeout_in_room_state(room);
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
    display_meld: DisplayMeldState,
    replacement_tile: Option<Tile>,
    consumed_tile_ids: Vec<String>,
    restricted_tile_key: Option<String>,
    kong_entry: Option<KongTrackerEntry>,
    last_action_context: Option<LastActionContext>,
    emitted_messages: Vec<Value>,
}

#[cfg(test)]
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
    let display_meld = create_claim_display_meld(
        action_type,
        seat_index,
        discarder_seat,
        &meld,
        &last_discard_tile.tile_key,
    );

    let mut emitted_messages = vec![claim_made_message(
        seat_index,
        discarder_seat,
        action_type,
        &last_discard_tile,
        &meld,
    )];

    let mut replacement_tile_for_output = None;
    let mut kong_entry = None;
    let mut last_action_context = None;
    if action_type == "kong" {
        let replacement_tile = tile_from_value(
            &replacement_tile_from_tail(room).ok_or_else(|| "invalid_action".to_string())?,
        )?;
        replacement_tile_for_output = Some(replacement_tile.clone());
        kong_entry = Some(KongTrackerEntry {
            kong_type: "exposed_kong".to_string(),
            actor_seat: seat_index,
            payer_seats: vec![discarder_seat],
            tile_key: Some(last_discard_tile.tile_key.clone()),
        });
        last_action_context = Some(LastActionContext {
            kind: "replacement_draw".to_string(),
            seat: seat_index,
            tile_id: Some(replacement_tile.tile_id.clone()),
            from_kong_replacement: true,
            was_last_live_tile: false,
            was_last_discard: false,
        });
        emitted_messages.push(replacement_draw_message(seat_index, &replacement_tile));
    }

    Ok(SelectedClaimPlan {
        discarder_seat,
        meld,
        display_meld,
        replacement_tile: replacement_tile_for_output,
        consumed_tile_ids: tile_ids.to_vec(),
        restricted_tile_key,
        kong_entry,
        last_action_context,
        emitted_messages,
    })
}

fn plan_selected_claim_in_room_state(
    room: &RoomState,
    seat_index: usize,
    action_type: &str,
    tile_ids: &[String],
) -> Result<SelectedClaimPlan, String> {
    if action_type != "chow" && action_type != "pung" && action_type != "kong" {
        return Err("invalid_action".to_string());
    }
    validate_claim_selection_in_room_state(room, seat_index, action_type, tile_ids)?;
    let round = round_state_ref(room)?;
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
    let display_meld = create_claim_display_meld(
        action_type,
        seat_index,
        discarder_seat,
        &meld,
        &last_discard_tile.tile_key,
    );

    let mut emitted_messages = vec![claim_made_message(
        seat_index,
        discarder_seat,
        action_type,
        &last_discard_tile,
        &meld,
    )];

    let mut replacement_tile_for_output = None;
    let mut kong_entry = None;
    let mut last_action_context = None;
    if action_type == "kong" {
        let replacement_tile = replacement_tile_from_tail_in_room_state(room)
            .ok_or_else(|| "invalid_action".to_string())?;
        replacement_tile_for_output = Some(replacement_tile.clone());
        kong_entry = Some(KongTrackerEntry {
            kong_type: "exposed_kong".to_string(),
            actor_seat: seat_index,
            payer_seats: vec![discarder_seat],
            tile_key: Some(last_discard_tile.tile_key.clone()),
        });
        last_action_context = Some(LastActionContext {
            kind: "replacement_draw".to_string(),
            seat: seat_index,
            tile_id: Some(replacement_tile.tile_id.clone()),
            from_kong_replacement: true,
            was_last_live_tile: false,
            was_last_discard: false,
        });
        emitted_messages.push(replacement_draw_message(seat_index, &replacement_tile));
    }

    Ok(SelectedClaimPlan {
        discarder_seat,
        meld,
        display_meld,
        replacement_tile: replacement_tile_for_output,
        consumed_tile_ids: tile_ids.to_vec(),
        restricted_tile_key,
        kong_entry,
        last_action_context,
        emitted_messages,
    })
}

fn apply_selected_claim_plan_to_round(
    round: &mut RoundState,
    seat_index: usize,
    plan: &SelectedClaimPlan,
) -> Result<(), String> {
    for tile_id in &plan.consumed_tile_ids {
        remove_player_concealed_tile(round, seat_index, tile_id)?;
    }
    round_player_mut(round, plan.discarder_seat)?.discards.pop();
    {
        let player = round_player_mut(round, seat_index)?;
        player.melds.push(plan.meld.clone());
        player.display_melds.push(plan.display_meld.clone());
        if let Some(replacement_tile) = plan.replacement_tile.as_ref() {
            player.concealed_tiles.push(replacement_tile.clone());
        }
    }
    if plan.replacement_tile.is_some() {
        round.wall.tail_index = round.wall.tail_index.saturating_sub(1);
    }
    if let Some(kong_entry) = plan.kong_entry.as_ref() {
        push_kong_entry(round, kong_entry);
    }
    if let Some(last_action_context) = plan.last_action_context.as_ref() {
        round.last_action_context = last_action_context.clone();
    }
    round.current_actor = seat_index;
    round.last_discard = None;
    round.pending_action = None;
    round.restricted_discard_tile_key = plan.restricted_tile_key.clone();
    round.version += 1;
    Ok(())
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

#[cfg(test)]
fn tile_from_value(tile: &Value) -> Result<Tile, String> {
    Tile::from_value(tile, "standard_action.tile").map_err(|error| error.to_string())
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

#[cfg(test)]
fn complete_add_kong_after_passes(room: &mut Value) -> Result<Vec<Value>, String> {
    complete_add_kong_after_passes_output(room).map(|output| output.emitted_messages)
}

#[cfg(test)]
fn complete_add_kong_after_passes_output(room: &mut Value) -> Result<EngineOutput, String> {
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
    let output_replacement_tile = replacement_tile.clone();
    let plan = plan_self_kong_completion(room, actor_seat, &selection, replacement_tile, true)?;
    update_room_state(room, |state| {
        let round = round_state_mut(state)?;
        apply_self_kong_plan_to_round(round, actor_seat, &plan)
    })?;
    sync_pending_timeout_for_value_room(room);
    Ok(EngineOutput::new(
        vec![
            self_kong_declared_event(
                actor_seat,
                "add_kong",
                &selection.tile_key,
                &selection.tile_ids,
            ),
            replacement_draw_event(actor_seat, &output_replacement_tile),
        ],
        plan.emitted_messages,
    ))
}

fn complete_add_kong_after_passes_output_in_room_state(
    room: &mut RoomState,
) -> Result<EngineOutput, String> {
    let round = room
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
    let replacement_tile = replacement_tile_from_tail_in_room_state(room)
        .ok_or_else(|| "invalid_action".to_string())?;
    let selection = SelfKongCandidate {
        kind: SelfKongKind::Add,
        tile_ids: vec![tile_id],
        tile_key,
        meld_index,
    };
    let output_replacement_tile = replacement_tile.clone();
    let plan = plan_self_kong_completion_in_room_state(
        room,
        actor_seat,
        &selection,
        replacement_tile,
        true,
    )?;
    {
        let round = round_state_mut(room)?;
        apply_self_kong_plan_to_round(round, actor_seat, &plan)?;
    }
    sync_pending_timeout_in_room_state(room);
    Ok(EngineOutput::new(
        vec![
            self_kong_declared_event(
                actor_seat,
                "add_kong",
                &selection.tile_key,
                &selection.tile_ids,
            ),
            replacement_draw_event(actor_seat, &output_replacement_tile),
        ],
        plan.emitted_messages,
    ))
}

fn self_kong_declared_payload(
    seat_index: usize,
    kong_type: &str,
    tile_key: &str,
    tile_ids: &[String],
) -> Value {
    json!({
        "type": "self_kong_declared",
        "seat": seat_index,
        "kong_type": kong_type,
        "tile_key": tile_key,
        "tile_ids": tile_ids,
    })
}

fn self_kong_declared_event(
    seat_index: usize,
    kong_type: &str,
    tile_key: &str,
    tile_ids: &[String],
) -> GameEvent {
    GameEvent::SelfKongDeclared {
        seat: seat_index,
        kong_type: kong_type.to_string(),
        tile_key: tile_key.to_string(),
        tile_ids: tile_ids.to_vec(),
    }
}

pub fn apply_rob_kong_pass_in_room_state(
    room: &mut RoomState,
    seat_index: usize,
) -> Result<EngineOutput, String> {
    let round = room
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

    let actor_seat = rob.actor_seat;
    let tile_id = rob.tile_id.clone();
    let tile_key = rob.tile_key.clone();
    let meld_index = rob.meld_index;
    let offered_hu_seats = rob.offered_hu_seats.clone();
    let claim_responses = rob.claim_responses.clone();
    let mut next_responded = rob.responded_seats.clone();
    next_responded.push(seat_index);
    {
        let round = room
            .round_state
            .as_mut()
            .ok_or_else(|| "invalid_action".to_string())?;
        round.pending_action = Some(PendingAction::RobKongWindow(RobKongWindowAction {
            actor_seat,
            tile_id,
            tile_key,
            meld_index,
            offered_hu_seats: offered_hu_seats.clone(),
            responded_seats: next_responded.clone(),
            claim_responses: claim_responses.clone(),
        }));
        round.version += 1;
    }

    let unresolved = offered_hu_seats
        .iter()
        .copied()
        .filter(|offered_seat| !next_responded.contains(offered_seat))
        .collect::<Vec<_>>();
    if !unresolved.is_empty() {
        sync_pending_timeout_in_room_state(room);
        return Ok(EngineOutput::default());
    }
    let winner_seats = resolve_hu_claims(&claim_responses, actor_seat)
        .into_iter()
        .map(|response| response.seat)
        .collect::<Vec<_>>();
    if !winner_seats.is_empty() {
        let settlement = compute_multi_hu_settlement_for_state(room, &winner_seats)?;
        return apply_hu_settlement_output_in_room_state(
            room,
            winner_seats[0],
            "discard",
            settlement,
        );
    }
    complete_add_kong_after_passes_output_in_room_state(room)
}

pub fn apply_rob_kong_hu_in_room_state(
    room: &mut RoomState,
    seat_index: usize,
) -> Result<EngineOutput, String> {
    let round = room
        .round_state
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let rob = match round.pending_action.as_ref() {
        Some(PendingAction::RobKongWindow(rob)) => rob,
        _ => return Err("invalid_action".to_string()),
    };
    if !rob.offered_hu_seats.contains(&seat_index) || rob.responded_seats.contains(&seat_index) {
        return Err("invalid_action".to_string());
    }

    let actor_seat = rob.actor_seat;
    let tile_id = rob.tile_id.clone();
    let tile_key = rob.tile_key.clone();
    let meld_index = rob.meld_index;
    let offered_hu_seats = rob.offered_hu_seats.clone();
    let mut next_responded = rob.responded_seats.clone();
    next_responded.push(seat_index);
    let mut claim_responses = rob.claim_responses.clone();
    claim_responses.push(ClaimResponse {
        seat: seat_index,
        action_type: "hu".to_string(),
        tiles: vec![],
    });
    {
        let round = room
            .round_state
            .as_mut()
            .ok_or_else(|| "invalid_action".to_string())?;
        round.pending_action = Some(PendingAction::RobKongWindow(RobKongWindowAction {
            actor_seat,
            tile_id,
            tile_key,
            meld_index,
            offered_hu_seats: offered_hu_seats.clone(),
            responded_seats: next_responded.clone(),
            claim_responses: claim_responses.clone(),
        }));
        round.version += 1;
    }

    let unresolved = offered_hu_seats
        .iter()
        .copied()
        .filter(|offered_seat| !next_responded.contains(offered_seat))
        .collect::<Vec<_>>();
    if !unresolved.is_empty() {
        sync_pending_timeout_in_room_state(room);
        return Ok(EngineOutput::default());
    }

    let winner_seats = resolve_hu_claims(&claim_responses, actor_seat)
        .into_iter()
        .map(|response| response.seat)
        .collect::<Vec<_>>();
    let settlement = compute_multi_hu_settlement_for_state(room, &winner_seats)?;
    apply_hu_settlement_output_in_room_state(room, winner_seats[0], "discard", settlement)
}

fn self_kong_kind_name(kind: SelfKongKind) -> &'static str {
    match kind {
        SelfKongKind::Concealed => "concealed_kong",
        SelfKongKind::Add => "add_kong",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::state::{
        LastActionContext, PendingTimeout, PlayerRoundState, RoundScoreTrackers, RoundState,
        RuleRuntimeState, SeatState, WallState,
    };

    fn suit(tile_key: &str, tile_id: &str) -> Tile {
        Tile {
            tile_id: tile_id.to_string(),
            tile_key: tile_key.to_string(),
            kind: "suit".to_string(),
            suit: Some(
                if tile_key.starts_with('w') {
                    "characters"
                } else if tile_key.starts_with('t') {
                    "bamboos"
                } else {
                    "dots"
                }
                .to_string(),
            ),
            rank: tile_key[1..].parse().ok(),
            name: Some(tile_key.to_string()),
        }
    }

    fn wind(tile_key: &str, tile_id: &str) -> Tile {
        Tile {
            tile_id: tile_id.to_string(),
            tile_key: tile_key.to_string(),
            kind: "wind".to_string(),
            suit: None,
            rank: None,
            name: Some(tile_key.to_string()),
        }
    }

    fn seat_state(seat_index: usize) -> SeatState {
        SeatState {
            seat_index,
            nickname: Some(format!("P{seat_index}")),
            reconnect_token: Some(format!("token-{seat_index}")),
            player_session_id: Some(seat_index as i64 + 1),
            connected: true,
            ready: true,
            is_bot: false,
            seat_type: "human".to_string(),
            bot_persona: None,
            bot_aggression: None,
            disconnect_deadline_at: None,
        }
    }

    fn discard_action_room() -> RoomState {
        RoomState {
            table_code: "ROOM99".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            seats: (0..4).map(seat_state).collect(),
            match_state: None,
            round_state: Some(RoundState {
                round_id: "east-1".to_string(),
                dealer_seat: 0,
                round_wind: "east".to_string(),
                current_actor: 0,
                phase: "playing".to_string(),
                wall: WallState {
                    tiles: vec![
                        suit("w9", "w9#draw"),
                        suit("t9", "t9#wall"),
                        suit("b9", "b9#wall"),
                    ],
                    head_index: 0,
                    tail_index: 2,
                },
                players: vec![
                    PlayerRoundState {
                        seat: 0,
                        is_ready_hand: false,
                        concealed_tiles: vec![
                            wind("east", "east#discard"),
                            suit("w1", "w1#0"),
                            suit("w2", "w2#0"),
                        ],
                        melds: vec![],
                        display_melds: vec![],
                        flowers: vec![],
                        discards: vec![],
                    },
                    PlayerRoundState {
                        seat: 1,
                        is_ready_hand: false,
                        concealed_tiles: vec![suit("t1", "t1#1"), suit("t2", "t2#1")],
                        melds: vec![],
                        display_melds: vec![],
                        flowers: vec![],
                        discards: vec![],
                    },
                    PlayerRoundState {
                        seat: 2,
                        is_ready_hand: false,
                        concealed_tiles: vec![suit("b1", "b1#2"), suit("b2", "b2#2")],
                        melds: vec![],
                        display_melds: vec![],
                        flowers: vec![],
                        discards: vec![],
                    },
                    PlayerRoundState {
                        seat: 3,
                        is_ready_hand: false,
                        concealed_tiles: vec![suit("w5", "w5#3"), suit("w6", "w6#3")],
                        melds: vec![],
                        display_melds: vec![],
                        flowers: vec![],
                        discards: vec![],
                    },
                ],
                last_discard: None,
                pending_action: None,
                settlement: None,
                version: 1,
                score_trackers: RoundScoreTrackers::default(),
                last_action_context: LastActionContext {
                    kind: "draw".to_string(),
                    seat: 0,
                    tile_id: Some("east#discard".to_string()),
                    from_kong_replacement: false,
                    was_last_live_tile: false,
                    was_last_discard: false,
                },
                rule_state: RuleRuntimeState {},
                restricted_discard_tile_key: None,
            }),
            pending_timeout: Some(PendingTimeout {
                kind: "active_turn".to_string(),
                seat_index: 0,
                deadline_at: None,
                drawn_tile_id: Some("east#discard".to_string()),
            }),
            continue_action: None,
        }
    }

    fn normalize_deadlines(mut room: RoomState) -> RoomState {
        if let Some(timeout) = room.pending_timeout.as_mut() {
            timeout.deadline_at = None;
        }
        room
    }

    #[test]
    fn claim_display_meld_marks_opposite_pung_tile_as_upside_down() {
        let display_meld = create_claim_display_meld(
            "pung",
            0,
            2,
            &["w3".to_string(), "w3".to_string(), "w3".to_string()],
            "w3",
        );

        assert_eq!(display_meld.tiles[0].code, "w3");
        assert_eq!(
            display_meld.tiles[0].orientation,
            DisplayMeldOrientation::UpsideDown
        );
    }

    #[test]
    fn claim_display_meld_places_right_source_kong_tile_in_last_slot() {
        let display_meld = create_claim_display_meld(
            "kong",
            0,
            1,
            &[
                "w3".to_string(),
                "w3".to_string(),
                "w3".to_string(),
                "w3".to_string(),
            ],
            "w3",
        );

        assert_eq!(display_meld.tiles[3].code, "w3");
        assert_eq!(
            display_meld.tiles[3].orientation,
            DisplayMeldOrientation::Rotated
        );
        assert!(
            display_meld
                .tiles
                .iter()
                .take(3)
                .all(|tile| tile.orientation == DisplayMeldOrientation::Normal)
        );
    }

    #[test]
    fn discard_action_value_wrapper_matches_room_state_variant() {
        let mut value_room = discard_action_room()
            .to_room_value()
            .expect("room should serialize");
        let value_output = apply_discard_action_output(&mut value_room, 0, "east#discard")
            .expect("value wrapper should succeed");

        let mut typed_room = discard_action_room();
        let typed_output =
            apply_discard_action_output_in_room_state(&mut typed_room, 0, "east#discard")
                .expect("typed action should succeed");

        assert_eq!(value_output.events, typed_output.events);
        assert_eq!(value_output.emitted_messages, typed_output.emitted_messages);

        let actual_room =
            RoomState::from_room_value(&value_room).expect("value room should remain valid");
        assert_eq!(
            normalize_deadlines(actual_room),
            normalize_deadlines(typed_room)
        );
    }

    #[test]
    fn typed_discard_after_last_live_tile_drawn_settles_even_with_dead_wall_tiles_present() {
        let mut room = discard_action_room();
        let round = room.round_state.as_mut().expect("round should exist");
        round.wall.head_index = 1;
        round.wall.tail_index = 0;
        round.last_action_context.was_last_live_tile = true;

        let output = apply_discard_action_output_in_room_state(&mut room, 0, "east#discard")
            .expect("discard action should settle exhaustive draw");

        assert_eq!(output.emitted_messages.len(), 2);
        assert_eq!(
            output.emitted_messages[0]["payload"]["event_type"],
            "tile_discarded"
        );
        assert_eq!(
            output.emitted_messages[1]["payload"]["event_type"],
            "round_drawn"
        );
        assert_eq!(room.phase, "settlement");
        assert_eq!(
            room.round_state.as_ref().map(|round| round.phase.as_str()),
            Some("settlement")
        );
        assert!(room.pending_timeout.is_none());
    }
}
