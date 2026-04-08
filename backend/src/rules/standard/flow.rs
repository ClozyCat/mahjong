use chrono::{SecondsFormat, TimeDelta, Utc};
use rand::Rng;
use serde_json::{Value, json};
use std::collections::BTreeMap;

use crate::core::engine::EngineOutput;
use crate::core::event::GameEvent;
use crate::core::engine::planner::{
    plan_advance_opening_flowers, plan_flower_action, plan_round_start_payload,
};
use crate::core::engine::reducer::update_room_state;
use crate::core::tile::Tile;
use crate::core::state::{ContinueActionState, MatchState, SkillLoadout};
use crate::rules::skills::{note_tracker_draw, sync_round_skill_trackers};

use super::runtime::{
    current_actor, is_last_live_tile_point, project_room_state, round_event_message,
    sync_pending_timeout,
};
use super::settlement::apply_settlement_to_match;

const MAX_SEATS: usize = 4;
const CONTINUE_ACTION_AUTO_ADVANCE_SECONDS: i64 = 30;
const WIND_ORDER: [&str; 4] = ["east", "south", "west", "north"];

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
        skill_trackers: Default::default(),
    };
    let _ = update_room_state(room, |state| {
        state.match_state = Some(match_state);
        Ok(())
    });
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

pub fn reconcile_continue_action_state(room: &mut Value) -> Result<(), String> {
    reconcile_continue_action(room)
}

pub fn apply_opening_flowers_pass(
    room: &mut Value,
    seat_index: usize,
) -> Result<Vec<Value>, String> {
    apply_opening_flowers_pass_output(room, seat_index).map(|output| output.emitted_messages)
}

pub fn apply_opening_flowers_pass_output(
    room: &mut Value,
    seat_index: usize,
) -> Result<EngineOutput, String> {
    let round_state = room
        .get("round_state")
        .ok_or_else(|| "round_not_ready".to_string())?;
    let pending_action = round_state.get("pending_action").unwrap_or(&Value::Null);
    if pending_action.get("type").and_then(Value::as_str) != Some("opening_flowers") {
        return Err("invalid_action".to_string());
    }
    if current_actor(room) != Some(seat_index) {
        return Err("not_your_turn".to_string());
    }
    if player_has_concealed_flower(round_state, seat_index) {
        return Err("invalid_action".to_string());
    }

    let state = project_room_state(room)?;
    let plan = plan_advance_opening_flowers(&state, seat_index);
    update_room_state(room, |state| {
        let round = state
            .round_state
            .as_mut()
            .ok_or_else(|| "round_not_ready".to_string())?;
        round.current_actor = plan.current_actor;
        round.pending_action = plan.pending_action.clone();
        if let Some(score_trackers) = plan.score_trackers.as_ref() {
            round.score_trackers = score_trackers.clone();
        }
        Ok(())
    })?;
    sync_round_skill_trackers(room);
    sync_pending_timeout(room);
    Ok(EngineOutput::default())
}

pub fn apply_flower_action(
    room: &mut Value,
    seat_index: usize,
    tile_ids: &[String],
) -> Result<Vec<Value>, String> {
    apply_flower_action_output(room, seat_index, tile_ids).map(|output| output.emitted_messages)
}

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
    if pending_type.is_some() && pending_type != Some("opening_flowers") {
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
        if let Some(advance) = plan.opening_flowers_advance.as_ref() {
            round.current_actor = advance.current_actor;
            round.pending_action = advance.pending_action.clone();
            if let Some(score_trackers) = advance.score_trackers.as_ref() {
                round.score_trackers = score_trackers.clone();
            }
        }
        Ok(())
    })?;
    note_tracker_draw(room, seat_index, &plan.replacement_tile.tile_key);
    sync_round_skill_trackers(room);
    sync_pending_timeout(room);

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

fn start_round(
    room: &mut Value,
    dealer_seat: usize,
    round_wind: &str,
    round_id: String,
    enforce_minimum_eight_fan: bool,
    seed: u64,
) {
    let (mut round_state, pending_timeout) = plan_round_start_payload(
        dealer_seat,
        round_wind,
        round_id,
        enforce_minimum_eight_fan,
        seed,
    );
    seed_round_skill_loadouts(room, &mut round_state);
    let _ = update_room_state(room, |state| {
        state.phase = "playing".to_string();
        state.round_state = Some(round_state);
        state.pending_timeout = Some(pending_timeout);
        state.continue_action = None;
        Ok(())
    });
    sync_round_skill_trackers(room);
}

fn seed_round_skill_loadouts(room: &Value, round_state: &mut crate::core::state::RoundState) {
    for player in &mut round_state.players {
        player.skill_loadout = skill_loadout_for_seat(room, player.seat);
    }
}

fn skill_loadout_for_seat(room: &Value, seat: usize) -> SkillLoadout {
    room.get("seats")
        .and_then(Value::as_array)
        .and_then(|seats| {
            seats.iter().find(|entry| {
                entry
                    .get("seat_index")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize)
                    == Some(seat)
            })
        })
        .and_then(|seat_state| {
            SkillLoadout::from_value(seat_state.get("skill_loadout")).ok()
        })
        .filter(|loadout| !loadout.equipped.is_empty())
        .or_else(|| {
            room.get("round_state")
                .and_then(|round| round.get("players"))
                .and_then(Value::as_array)
                .and_then(|players| players.get(seat))
                .and_then(|player| {
                    SkillLoadout::from_value(player.get("skill_loadout")).ok()
                })
                .filter(|loadout| !loadout.equipped.is_empty())
        })
        .unwrap_or_default()
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
    project_room_state(room)
        .ok()
        .and_then(|state| state.continue_action)
        .filter(|action| action.action_id == action_id)
        .map(|action| action.confirmed_seats)
        .unwrap_or_default()
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
