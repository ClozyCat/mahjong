use chrono::{SecondsFormat, TimeDelta, Utc};
use rand::Rng;
use serde_json::{Value, json};
use std::collections::BTreeMap;

use crate::core::engine::EngineOutput;
use crate::core::engine::planner::{plan_flower_action, plan_round_start_payload};
use crate::core::event::GameEvent;
use crate::core::state::{ContinueActionState, MatchState, RoomState, SeatState};
use crate::core::tile::Tile;

use super::runtime::{
    current_actor_in_room_state, is_last_live_tile_point_in_room_state, round_event_message,
    sync_pending_timeout_in_room_state,
};
use super::settlement::apply_settlement_to_match_in_room_state;

#[cfg(test)]
use super::runtime::{current_actor, is_last_live_tile_point, project_room_state};
#[cfg(test)]
use super::settlement::apply_settlement_to_match;
#[cfg(test)]
use crate::core::engine::reducer::update_room_state;

const MAX_SEATS: usize = 4;
const CONTINUE_ACTION_AUTO_ADVANCE_SECONDS: i64 = 30;
const WIND_ORDER: [&str; 4] = ["east", "south", "west", "north"];

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

pub fn room_ready_to_start(room: &RoomState) -> bool {
    room.seats.len() == MAX_SEATS
        && room
            .seats
            .iter()
            .all(|seat| seat.ready && (seat.connected || seat.is_bot))
}

pub fn add_bot_seats_for_test(room: &mut RoomState) {
    let occupied = room
        .seats
        .iter()
        .map(|seat| seat.seat_index)
        .collect::<std::collections::HashSet<_>>();
    for seat_index in 0..MAX_SEATS {
        if occupied.contains(&seat_index) {
            continue;
        }
        room.seats.push(SeatState {
            seat_index,
            nickname: Some(format!("Bot {seat_index}")),
            reconnect_token: None,
            player_session_id: Some(-((seat_index as i64) + 1)),
            connected: true,
            ready: true,
            is_bot: true,
            seat_type: "bot".to_string(),
            bot_persona: None,
            bot_aggression: None,
            disconnect_deadline_at: None,
        });
    }
    room.seats.sort_by_key(|seat| seat.seat_index);
}

pub fn start_match_in_room_state(
    room: &mut RoomState,
    dealer_seat: usize,
    seed: u64,
) -> Result<(), String> {
    let mut cumulative_scores = BTreeMap::new();
    for seat in 0..MAX_SEATS {
        cumulative_scores.insert(seat, 0);
    }
    let mut match_state = MatchState {
        prevailing_wind: "east".to_string(),
        hand_number: 1,
        dealer_seat,
        cumulative_scores,
        match_finished: false,
        last_completed_round_id: None,
        statistics: Default::default(),
    };
    match_state.sync_statistics_to_cumulative_scores();
    room.match_state = Some(match_state);
    let enforce_minimum_eight_fan = room.enforce_minimum_eight_fan;
    start_round_in_room_state(
        room,
        dealer_seat,
        "east",
        format!("east-1-dealer-{dealer_seat}-{seed}"),
        enforce_minimum_eight_fan,
        seed,
    );
    Ok(())
}

pub fn record_continue_action_in_room_state(
    room: &mut RoomState,
    seat_index: usize,
    action_id: &str,
) -> Result<(), String> {
    let current_action = current_continue_action_id_in_room_state(room)
        .ok_or_else(|| "invalid_action".to_string())?;
    if current_action != action_id {
        return Err(match action_id {
            "start_next_round" => "round_not_ready".to_string(),
            "restart_match" => "match_not_finished".to_string(),
            _ => "invalid_action".to_string(),
        });
    }
    let action = room
        .continue_action
        .get_or_insert_with(|| ContinueActionState {
            action_id: action_id.to_string(),
            confirmed_seats: Vec::new(),
            required_seats: Vec::new(),
            online_seats: Vec::new(),
            auto_advance_deadline_at: None,
        });
    action.action_id = action_id.to_string();
    if !action.confirmed_seats.contains(&seat_index) {
        action.confirmed_seats.push(seat_index);
    }
    reconcile_continue_action_in_room_state(room)?;
    Ok(())
}

pub fn process_due_continue_action_in_room_state(room: &mut RoomState) -> Result<bool, String> {
    let action_id = current_continue_action_id_in_room_state(room)
        .ok_or_else(|| "invalid_action".to_string())?;
    let deadline = room
        .continue_action
        .as_ref()
        .and_then(|action| action.auto_advance_deadline_at.clone());
    if deadline.is_none() {
        return Ok(false);
    }
    complete_continue_action_in_room_state(room, action_id)?;
    Ok(true)
}

pub fn reconcile_continue_action_state_in_room_state(room: &mut RoomState) -> Result<(), String> {
    reconcile_continue_action_in_room_state(room)
}

#[cfg(test)]
#[allow(dead_code)]
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
    let mut match_state = MatchState {
        prevailing_wind: "east".to_string(),
        hand_number: 1,
        dealer_seat,
        cumulative_scores,
        match_finished: false,
        last_completed_round_id: None,
        statistics: Default::default(),
    };
    match_state.sync_statistics_to_cumulative_scores();
    let _ = update_room_state(room, |state| {
        state.match_state = Some(match_state);
        Ok(())
    });
}

#[cfg(test)]
#[allow(dead_code)]
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
    update_room_state(room, |state| {
        let action = state
            .continue_action
            .get_or_insert_with(|| ContinueActionState {
                action_id: action_id.to_string(),
                confirmed_seats: Vec::new(),
                required_seats: Vec::new(),
                online_seats: Vec::new(),
                auto_advance_deadline_at: None,
            });
        action.action_id = action_id.to_string();
        if !action.confirmed_seats.contains(&seat_index) {
            action.confirmed_seats.push(seat_index);
        }
        Ok(())
    })?;
    reconcile_continue_action(room)?;
    Ok(())
}

#[cfg(test)]
#[allow(dead_code)]
pub fn process_due_continue_action(room: &mut Value) -> Result<bool, String> {
    let action_id = current_continue_action_id(room).ok_or_else(|| "invalid_action".to_string())?;
    let deadline = project_room_state(room)?
        .continue_action
        .and_then(|action| action.auto_advance_deadline_at);
    if deadline.is_none() {
        return Ok(false);
    }
    complete_continue_action(room, action_id)?;
    Ok(true)
}

#[cfg(test)]
#[allow(dead_code)]
pub fn reconcile_continue_action_state(room: &mut Value) -> Result<(), String> {
    reconcile_continue_action(room)
}

#[cfg(test)]
pub fn apply_flower_action_output(
    room: &mut Value,
    seat_index: usize,
    tile_ids: &[String],
) -> Result<EngineOutput, String> {
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
    if pending_type.is_some() {
        return Err("invalid_action".to_string());
    }

    let tile_id = &tile_ids[0];
    let state = project_room_state(room)?;
    let plan = plan_flower_action(&state, seat_index, tile_id)?;
    update_room_state(room, |state| {
        let round = state
            .round_state
            .as_mut()
            .ok_or_else(|| "round_not_ready".to_string())?;
        let player = round
            .players
            .get_mut(seat_index)
            .ok_or_else(|| "invalid_action".to_string())?;
        let concealed_index = player
            .concealed_tiles
            .iter()
            .position(|tile| tile.tile_id == plan.flower_tile.tile_id)
            .ok_or_else(|| "invalid_action".to_string())?;
        player.concealed_tiles.remove(concealed_index);
        player.flowers.push(plan.flower_tile.clone());
        player.concealed_tiles.push(plan.replacement_tile.clone());
        round.wall.tail_index = round.wall.tail_index.saturating_sub(1);
        round.last_action_context = plan.last_action_context.clone();
        round.version += 1;
        Ok(())
    })?;
    sync_pending_timeout_for_value_room(room);

    let flower_event = json!({
        "type": "flower_exposed",
        "seat": seat_index,
        "tile_id": plan.flower_tile.tile_id,
    });
    Ok(EngineOutput::new(
        vec![
            GameEvent::FlowerExposed {
                seat: seat_index,
                tile_id: plan.flower_tile.tile_id.clone(),
            },
            GameEvent::TileDrawn {
                seat: seat_index,
                tile: plan.replacement_tile.clone(),
                source: "replacement_draw".to_string(),
            },
        ],
        vec![
            round_event_message("flower_exposed", flower_event),
            replacement_draw_message(seat_index, &plan.replacement_tile),
        ],
    ))
}

pub fn apply_flower_action_output_in_room_state(
    room: &mut RoomState,
    seat_index: usize,
    tile_ids: &[String],
) -> Result<EngineOutput, String> {
    if room.phase != "playing" {
        return Err("round_not_ready".to_string());
    }
    if current_actor_in_room_state(room) != Some(seat_index) {
        return Err("not_your_turn".to_string());
    }
    if tile_ids.len() != 1 {
        return Err("invalid_action".to_string());
    }
    if is_last_live_tile_point_in_room_state(room) {
        return Err("invalid_action".to_string());
    }

    let pending_type = room
        .round_state
        .as_ref()
        .and_then(|round| round.pending_action.as_ref())
        .map(|pending| pending.action_type());
    if pending_type.is_some() {
        return Err("invalid_action".to_string());
    }

    let tile_id = &tile_ids[0];
    let plan = plan_flower_action(room, seat_index, tile_id)?;
    {
        let round = room
            .round_state
            .as_mut()
            .ok_or_else(|| "round_not_ready".to_string())?;
        let player = round
            .players
            .get_mut(seat_index)
            .ok_or_else(|| "invalid_action".to_string())?;
        let concealed_index = player
            .concealed_tiles
            .iter()
            .position(|tile| tile.tile_id == plan.flower_tile.tile_id)
            .ok_or_else(|| "invalid_action".to_string())?;
        player.concealed_tiles.remove(concealed_index);
        player.flowers.push(plan.flower_tile.clone());
        player.concealed_tiles.push(plan.replacement_tile.clone());
        round.wall.tail_index = round.wall.tail_index.saturating_sub(1);
        round.last_action_context = plan.last_action_context.clone();
        round.version += 1;
    }
    sync_pending_timeout_in_room_state(room);

    let flower_event = json!({
        "type": "flower_exposed",
        "seat": seat_index,
        "tile_id": plan.flower_tile.tile_id,
    });
    Ok(EngineOutput::new(
        vec![
            GameEvent::FlowerExposed {
                seat: seat_index,
                tile_id: plan.flower_tile.tile_id.clone(),
            },
            GameEvent::TileDrawn {
                seat: seat_index,
                tile: plan.replacement_tile.clone(),
                source: "replacement_draw".to_string(),
            },
        ],
        vec![
            round_event_message("flower_exposed", flower_event),
            replacement_draw_message(seat_index, &plan.replacement_tile),
        ],
    ))
}

#[cfg(test)]
#[allow(dead_code)]
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
    let _ = update_room_state(room, |state| {
        state.phase = "playing".to_string();
        state.round_state = Some(round_state);
        state.pending_timeout = Some(pending_timeout);
        state.continue_action = None;
        Ok(())
    });
}

fn start_round_in_room_state(
    room: &mut RoomState,
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
    room.phase = "playing".to_string();
    room.round_state = Some(round_state);
    room.pending_timeout = Some(pending_timeout);
    room.continue_action = None;
}

#[cfg(test)]
#[allow(dead_code)]
fn current_continue_action_id(room: &Value) -> Option<&'static str> {
    match room.get("phase").and_then(Value::as_str) {
        Some("settlement") => Some("start_next_round"),
        Some("finished") => Some("restart_match"),
        _ => None,
    }
}

fn current_continue_action_id_in_room_state(room: &RoomState) -> Option<&'static str> {
    match room.phase.as_str() {
        "settlement" => Some("start_next_round"),
        "finished" => Some("restart_match"),
        _ => None,
    }
}

#[cfg(test)]
#[allow(dead_code)]
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

#[cfg(test)]
#[allow(dead_code)]
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

#[cfg(test)]
#[allow(dead_code)]
fn current_confirmed_continue_seats(room: &Value, action_id: &str) -> Vec<usize> {
    project_room_state(room)
        .ok()
        .and_then(|state| state.continue_action)
        .filter(|action| action.action_id == action_id)
        .map(|action| action.confirmed_seats)
        .unwrap_or_default()
}

fn continue_required_human_seats_in_room_state(room: &RoomState) -> Vec<usize> {
    room.seats
        .iter()
        .filter(|seat| !seat.is_bot)
        .map(|seat| seat.seat_index)
        .collect()
}

fn continue_online_human_seats_in_room_state(room: &RoomState) -> Vec<usize> {
    room.seats
        .iter()
        .filter(|seat| seat.connected && !seat.is_bot)
        .map(|seat| seat.seat_index)
        .collect()
}

fn current_confirmed_continue_seats_in_room_state(room: &RoomState, action_id: &str) -> Vec<usize> {
    room.continue_action
        .as_ref()
        .filter(|action| action.action_id == action_id)
        .map(|action| action.confirmed_seats.clone())
        .unwrap_or_default()
}

#[cfg(test)]
#[allow(dead_code)]
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

fn continue_all_occupied_seats_in_room_state(room: &RoomState) -> Vec<usize> {
    room.seats.iter().map(|seat| seat.seat_index).collect()
}

#[cfg(test)]
#[allow(dead_code)]
fn reconcile_continue_action(room: &mut Value) -> Result<(), String> {
    let Some(action_id) = current_continue_action_id(room) else {
        update_room_state(room, |state| {
            state.continue_action = None;
            Ok(())
        })?;
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
        update_room_state(room, |state| {
            if let Some(action) = state.continue_action.as_mut() {
                action.auto_advance_deadline_at = None;
            }
            Ok(())
        })?;
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

    let has_deadline = project_room_state(room)?
        .continue_action
        .and_then(|action| action.auto_advance_deadline_at)
        .is_some();
    if !has_deadline {
        let deadline = (Utc::now() + TimeDelta::seconds(CONTINUE_ACTION_AUTO_ADVANCE_SECONDS))
            .to_rfc3339_opts(SecondsFormat::Micros, true);
        update_room_state(room, |state| {
            let action = state
                .continue_action
                .get_or_insert_with(|| ContinueActionState {
                    action_id: action_id.to_string(),
                    confirmed_seats: Vec::new(),
                    required_seats: Vec::new(),
                    online_seats: Vec::new(),
                    auto_advance_deadline_at: None,
                });
            action.action_id = action_id.to_string();
            action.auto_advance_deadline_at = Some(deadline);
            Ok(())
        })?;
    }
    Ok(())
}

fn reconcile_continue_action_in_room_state(room: &mut RoomState) -> Result<(), String> {
    let Some(action_id) = current_continue_action_id_in_room_state(room) else {
        room.continue_action = None;
        return Ok(());
    };
    let required = continue_required_human_seats_in_room_state(room);
    let confirmed = current_confirmed_continue_seats_in_room_state(room, action_id);
    let online = continue_online_human_seats_in_room_state(room);

    if required.iter().all(|seat| confirmed.contains(seat)) {
        complete_continue_action_in_room_state(room, action_id)?;
        return Ok(());
    }

    let action = room
        .continue_action
        .get_or_insert_with(|| ContinueActionState {
            action_id: action_id.to_string(),
            confirmed_seats: Vec::new(),
            required_seats: Vec::new(),
            online_seats: Vec::new(),
            auto_advance_deadline_at: None,
        });
    action.action_id = action_id.to_string();
    action.confirmed_seats = confirmed.clone();
    action.required_seats = required.clone();
    action.online_seats = online.clone();

    let online_unconfirmed = online
        .iter()
        .filter(|seat| !confirmed.contains(seat))
        .copied()
        .collect::<Vec<_>>();
    if !online_unconfirmed.is_empty() {
        action.auto_advance_deadline_at = None;
        return Ok(());
    }

    let offline_unconfirmed = required
        .iter()
        .filter(|seat| !online.contains(seat) && !confirmed.contains(seat))
        .copied()
        .collect::<Vec<_>>();
    if offline_unconfirmed.is_empty() {
        complete_continue_action_in_room_state(room, action_id)?;
        return Ok(());
    }

    let has_deadline = room
        .continue_action
        .as_ref()
        .and_then(|action| action.auto_advance_deadline_at.as_ref())
        .is_some();
    if !has_deadline {
        let deadline = (Utc::now() + TimeDelta::seconds(CONTINUE_ACTION_AUTO_ADVANCE_SECONDS))
            .to_rfc3339_opts(SecondsFormat::Micros, true);
        if let Some(action) = room.continue_action.as_mut() {
            action.auto_advance_deadline_at = Some(deadline);
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(dead_code)]
fn complete_continue_action(room: &mut Value, action_id: &str) -> Result<(), String> {
    update_room_state(room, |state| {
        state.continue_action = None;
        Ok(())
    })?;
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

fn complete_continue_action_in_room_state(
    room: &mut RoomState,
    action_id: &str,
) -> Result<(), String> {
    room.continue_action = None;
    match action_id {
        "start_next_round" => complete_start_next_round_in_room_state(room),
        "restart_match" => {
            let occupied = continue_all_occupied_seats_in_room_state(room);
            if occupied.is_empty() {
                return Err("invalid_action".to_string());
            }
            let mut rng = rand::rng();
            let dealer_index = rng.random_range(0..occupied.len());
            start_match_in_room_state(room, occupied[dealer_index], rand::random::<u64>())
        }
        _ => Err("invalid_action".to_string()),
    }
}

#[cfg(test)]
#[allow(dead_code)]
fn complete_start_next_round(room: &mut Value) -> Result<(), String> {
    apply_settlement_to_match(room);
    let state = project_room_state(room)?;
    let match_state = state
        .match_state
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let prevailing_wind = match_state.prevailing_wind.as_str();
    let hand_number = match_state.hand_number as usize;
    let dealer_seat = match_state.dealer_seat;
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

    update_room_state(room, |state| {
        let match_state = state
            .match_state
            .as_mut()
            .ok_or_else(|| "invalid_action".to_string())?;
        match_state.prevailing_wind = next_wind.clone();
        match_state.hand_number = if match_finished {
            hand_number as u32
        } else {
            next_hand_number as u32
        };
        match_state.dealer_seat = if match_finished {
            dealer_seat
        } else {
            next_dealer
        };
        match_state.match_finished = match_finished;
        Ok(())
    })?;

    if match_finished {
        update_room_state(room, |state| {
            state.phase = "finished".to_string();
            state.pending_timeout = None;
            Ok(())
        })?;
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

fn complete_start_next_round_in_room_state(room: &mut RoomState) -> Result<(), String> {
    apply_settlement_to_match_in_room_state(room);
    let match_state = room
        .match_state
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let prevailing_wind = match_state.prevailing_wind.as_str();
    let hand_number = match_state.hand_number as usize;
    let dealer_seat = match_state.dealer_seat;
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

    {
        let match_state = room
            .match_state
            .as_mut()
            .ok_or_else(|| "invalid_action".to_string())?;
        match_state.prevailing_wind = next_wind.clone();
        match_state.hand_number = if match_finished {
            hand_number as u32
        } else {
            next_hand_number as u32
        };
        match_state.dealer_seat = if match_finished {
            dealer_seat
        } else {
            next_dealer
        };
        match_state.match_finished = match_finished;
    }

    if match_finished {
        room.phase = "finished".to_string();
        room.pending_timeout = None;
        return Ok(());
    }

    let round_id = format!(
        "{next_wind}-{next_hand_number}-dealer-{next_dealer}-{}",
        rand::random::<u64>()
    );
    start_round_in_room_state(
        room,
        next_dealer,
        &next_wind,
        round_id,
        room.enforce_minimum_eight_fan,
        rand::random::<u64>(),
    );
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::state::{
        ContinueActionState, LastActionContext, PendingTimeout, PlayerRoundState,
        RoundScoreTrackers, RoundState, RuleRuntimeState, SeatState, WallState,
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

    fn flower(tile_key: &str, tile_id: &str) -> Tile {
        Tile {
            tile_id: tile_id.to_string(),
            tile_key: tile_key.to_string(),
            kind: "flower".to_string(),
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

    fn flower_action_room() -> RoomState {
        RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            test_mode: false,
            enforce_minimum_eight_fan: true,
            seats: (0..4).map(seat_state).collect(),
            match_state: None,
            round_state: Some(RoundState {
                round_id: "east-1".to_string(),
                dealer_seat: 0,
                round_wind: "east".to_string(),
                current_actor: 0,
                phase: "playing".to_string(),
                wall: WallState {
                    tiles: vec![suit("w9", "w9#head"), wind("east", "east#tail")],
                    head_index: 0,
                    tail_index: 1,
                },
                players: vec![
                    PlayerRoundState {
                        seat: 0,
                        concealed_tiles: vec![
                            flower("f1", "f1#0"),
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
                        concealed_tiles: vec![suit("t1", "t1#1")],
                        melds: vec![],
                        display_melds: vec![],
                        flowers: vec![],
                        discards: vec![],
                    },
                    PlayerRoundState {
                        seat: 2,
                        concealed_tiles: vec![suit("b1", "b1#2")],
                        melds: vec![],
                        display_melds: vec![],
                        flowers: vec![],
                        discards: vec![],
                    },
                    PlayerRoundState {
                        seat: 3,
                        concealed_tiles: vec![suit("w5", "w5#3")],
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
                last_action_context: LastActionContext::default(),
                rule_state: RuleRuntimeState {
                    enforce_minimum_eight_fan: true,
                },
                restricted_discard_tile_key: None,
            }),
            pending_timeout: Some(PendingTimeout {
                kind: "active_turn".to_string(),
                seat_index: 0,
                deadline_at: None,
                drawn_tile_id: Some("f1#0".to_string()),
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
    fn flower_action_value_wrapper_matches_room_state_variant() {
        let expected_room = flower_action_room();
        let mut value_room = flower_action_room()
            .to_room_value()
            .expect("room should serialize");
        let value_output = apply_flower_action_output(&mut value_room, 0, &[String::from("f1#0")])
            .expect("value wrapper should succeed");

        let mut typed_room = flower_action_room();
        let typed_output =
            apply_flower_action_output_in_room_state(&mut typed_room, 0, &[String::from("f1#0")])
                .expect("typed action should succeed");

        assert_eq!(value_output.events, typed_output.events);
        assert_eq!(value_output.emitted_messages, typed_output.emitted_messages);

        let actual_room =
            RoomState::from_room_value(&value_room).expect("value room should remain valid");
        assert_eq!(
            normalize_deadlines(actual_room),
            normalize_deadlines(typed_room)
        );
        assert_ne!(
            normalize_deadlines(expected_room).round_state,
            normalize_deadlines(
                RoomState::from_room_value(&value_room).expect("value room should parse")
            )
            .round_state
        );
    }

    #[test]
    fn reconcile_continue_action_room_state_populates_required_and_online_human_seats() {
        let mut room = flower_action_room();
        room.phase = "settlement".to_string();
        room.seats = (0..3).map(seat_state).collect();
        room.continue_action = Some(ContinueActionState {
            action_id: "start_next_round".to_string(),
            confirmed_seats: vec![0],
            required_seats: Vec::new(),
            online_seats: Vec::new(),
            auto_advance_deadline_at: None,
        });

        reconcile_continue_action_in_room_state(&mut room)
            .expect("continue action should reconcile");

        let action = room
            .continue_action
            .expect("continue action should remain pending");
        assert_eq!(action.confirmed_seats, vec![0]);
        assert_eq!(action.required_seats, vec![0, 1, 2]);
        assert_eq!(action.online_seats, vec![0, 1, 2]);
        assert!(action.auto_advance_deadline_at.is_none());
    }
}

