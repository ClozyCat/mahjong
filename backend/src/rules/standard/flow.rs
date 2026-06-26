use chrono::{SecondsFormat, TimeDelta, Utc};
use serde_json::{Value, json};
use std::collections::BTreeMap;

use crate::core::engine::EngineOutput;
use crate::core::engine::planner::{plan_flower_action, plan_round_start_payload};
use crate::core::event::GameEvent;
use crate::core::state::{ContinueActionState, MatchState, RoomState, SeatState};
use crate::core::tile::Tile;

use super::runtime::{
    current_actor_in_room_state, is_last_live_tile_point_in_room_state,
    replacement_tile_from_tail_in_room_state, round_event_message,
    sync_pending_timeout_in_room_state,
};
use super::settlement::{
    apply_settlement_to_match_in_room_state, settle_exhaustive_draw_output_in_room_state,
};

#[cfg(test)]
use super::runtime::{
    current_actor, is_last_live_tile_point, project_room_state, replacement_tile_from_tail,
};
#[cfg(test)]
use super::settlement::settle_exhaustive_draw_output;
#[cfg(test)]
use crate::core::engine::reducer::update_room_state;

const MAX_SEATS: usize = 4;
const CONTINUE_ACTION_AUTO_ADVANCE_SECONDS: i64 = 15;
const BOT_TAKEOVER_AUTO_CONTINUE_SECONDS: i64 = 3;
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
    room.seats.len() == MAX_SEATS && room.seats.iter().all(|seat| seat.connected || seat.is_bot)
}

#[cfg(test)]
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
            user_id: None,
            nickname: Some(format!("Bot {seat_index}")),
            points: None,
            title: None,
            connected: true,
            is_bot: true,
            seat_type: "bot".to_string(),
            bot_persona: None,
            bot_aggression: None,
            disconnect_deadline_at: None,
            consecutive_timeout_auto_response_count: 0,
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
        seed,
        prevailing_wind: "east".to_string(),
        hand_number: 1,
        dealer_seat,
        dealer_repeat_count: 0,
        cumulative_scores,
        match_finished: false,
        last_completed_round_id: None,
        statistics: Default::default(),
        extra_time_pool: Default::default(),
    };
    match_state.sync_statistics_to_cumulative_scores();
    match_state.initialize_extra_time_pool();
    room.match_state = Some(match_state);
    start_round_in_room_state(
        room,
        dealer_seat,
        "east",
        format!("east-1-dealer-{dealer_seat}-{seed}"),
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
    start_round(
        room,
        dealer_seat,
        "east",
        format!("east-1-dealer-{dealer_seat}-{seed}"),
        seed,
    );

    let mut cumulative_scores = BTreeMap::new();
    for seat in 0..MAX_SEATS {
        cumulative_scores.insert(seat, 0);
    }
    let mut match_state = MatchState {
        seed,
        prevailing_wind: "east".to_string(),
        hand_number: 1,
        dealer_seat,
        dealer_repeat_count: 0,
        cumulative_scores,
        match_finished: false,
        last_completed_round_id: None,
        statistics: Default::default(),
        extra_time_pool: Default::default(),
    };
    match_state.sync_statistics_to_cumulative_scores();
    let _ = update_room_state(room, |state| {
        state.match_state = Some(match_state);
        Ok(())
    });
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
    let pending_type = room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))
        .and_then(|pending| pending.get("type"))
        .and_then(Value::as_str);
    if pending_type.is_some() {
        return Err("invalid_action".to_string());
    }

    if is_last_live_tile_point(room) || replacement_tile_from_tail(room).is_none() {
        return Ok(settle_exhaustive_draw_output(room));
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
    let pending_type = room
        .round_state
        .as_ref()
        .and_then(|round| round.pending_action.as_ref())
        .map(|pending| pending.action_type());
    if pending_type.is_some() {
        return Err("invalid_action".to_string());
    }

    if is_last_live_tile_point_in_room_state(room)
        || replacement_tile_from_tail_in_room_state(room).is_none()
    {
        return Ok(settle_exhaustive_draw_output_in_room_state(room));
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
    seed: u64,
) {
    let (round_state, pending_timeout) =
        plan_round_start_payload(dealer_seat, round_wind, round_id, seed);
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
    seed: u64,
) {
    let (round_state, pending_timeout) =
        plan_round_start_payload(dealer_seat, round_wind, round_id, seed);
    room.phase = "playing".to_string();
    room.round_state = Some(round_state);
    room.pending_timeout = Some(pending_timeout);
    room.continue_action = None;
}

fn seat_rotation_for_completed_wind(prevailing_wind: &str) -> Option<[usize; MAX_SEATS]> {
    match prevailing_wind {
        "east" | "west" => Some([1, 0, 3, 2]),
        "south" => Some([2, 3, 1, 0]),
        _ => None,
    }
}

fn rotate_seats_after_wind_end(room: &mut RoomState, prevailing_wind: &str) {
    let Some(old_to_new_seat) = seat_rotation_for_completed_wind(prevailing_wind) else {
        return;
    };
    rotate_match_scores_after_wind_end(room, &old_to_new_seat);
    for seat in &mut room.seats {
        if let Some(&next_seat) = old_to_new_seat.get(seat.seat_index) {
            seat.seat_index = next_seat;
        }
    }
    room.seats.sort_by_key(|seat| seat.seat_index);
}

fn rotate_match_scores_after_wind_end(room: &mut RoomState, old_to_new_seat: &[usize; MAX_SEATS]) {
    let Some(match_state) = room.match_state.as_mut() else {
        return;
    };

    remap_seat_keyed_map(&mut match_state.cumulative_scores, old_to_new_seat);
    remap_seat_keyed_map(
        &mut match_state.statistics.seat_stats_by_seat,
        old_to_new_seat,
    );
    remap_seat_keyed_map(&mut match_state.extra_time_pool, old_to_new_seat);
    match_state.sync_statistics_to_cumulative_scores();
}

fn remap_seat_keyed_map<T>(
    values_by_seat: &mut BTreeMap<usize, T>,
    old_to_new_seat: &[usize; MAX_SEATS],
) {
    let previous_values = std::mem::take(values_by_seat);
    for (old_seat, value) in previous_values {
        let next_seat = old_to_new_seat.get(old_seat).copied().unwrap_or(old_seat);
        values_by_seat.insert(next_seat, value);
    }
}

fn wind_seed_component(wind: &str) -> u64 {
    match wind {
        "east" => 0xE451,
        "south" => 0x50A7,
        "west" => 0xCE57,
        "north" => 0xA047,
        _ => 0,
    }
}

fn derive_round_seed(
    base_seed: u64,
    round_wind: &str,
    hand_number: usize,
    dealer_seat: usize,
    dealer_repeat_count: u32,
) -> u64 {
    base_seed
        ^ wind_seed_component(round_wind).rotate_left(7)
        ^ (hand_number as u64).rotate_left(17)
        ^ (dealer_seat as u64).rotate_left(29)
        ^ (dealer_repeat_count as u64).rotate_left(41)
}

fn settlement_is_final_hand_in_room_state(room: &RoomState) -> bool {
    room.phase == "settlement"
        && room
            .match_state
            .as_ref()
            .map(|match_state| {
                match_state.prevailing_wind == "north"
                    && match_state.hand_number >= 4
                    && !settlement_repeats_dealer_in_room_state(room, match_state.dealer_seat)
            })
            .unwrap_or(false)
}

fn settlement_repeats_dealer_in_room_state(room: &RoomState, dealer_seat: usize) -> bool {
    room.dealer_repeat_enabled && settlement_keeps_dealer(room, dealer_seat)
}

fn current_continue_action_id_in_room_state(room: &RoomState) -> Option<&'static str> {
    match room.phase.as_str() {
        "settlement" if !settlement_is_final_hand_in_room_state(room) => Some("start_next_round"),
        _ => None,
    }
}

fn finish_final_settlement_in_room_state(room: &mut RoomState) {
    apply_settlement_to_match_in_room_state(room);
    room.pending_timeout = None;
    room.continue_action = None;
}

fn is_human_controlled_seat(seat: &SeatState) -> bool {
    seat.seat_type == "human" || (seat.seat_type.is_empty() && !seat.is_bot)
}

fn continue_required_human_seats_in_room_state(room: &RoomState) -> Vec<usize> {
    room.seats
        .iter()
        .filter(|seat| is_human_controlled_seat(seat))
        .map(|seat| seat.seat_index)
        .collect()
}

fn continue_online_human_seats_in_room_state(room: &RoomState) -> Vec<usize> {
    room.seats
        .iter()
        .filter(|seat| seat.connected && is_human_controlled_seat(seat))
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

fn reconcile_continue_action_in_room_state(room: &mut RoomState) -> Result<(), String> {
    if settlement_is_final_hand_in_room_state(room) {
        finish_final_settlement_in_room_state(room);
        return Ok(());
    }
    let Some(action_id) = current_continue_action_id_in_room_state(room) else {
        room.continue_action = None;
        return Ok(());
    };
    let required = continue_required_human_seats_in_room_state(room);
    let mut confirmed = current_confirmed_continue_seats_in_room_state(room, action_id);
    let online = continue_online_human_seats_in_room_state(room);

    let is_non_evaluation = room.mode != crate::evaluation::EVALUATION_ROOM_MODE;
    if is_non_evaluation {
        let bot_takeover_unconfirmed: Vec<usize> = room
            .seats
            .iter()
            .filter(|s| s.is_bot && s.seat_type == "human" && required.contains(&s.seat_index))
            .map(|s| s.seat_index)
            .filter(|idx| !confirmed.contains(idx))
            .collect();
        for idx in &bot_takeover_unconfirmed {
            confirmed.push(*idx);
        }
        confirmed.sort();
        confirmed.dedup();
        if !bot_takeover_unconfirmed.is_empty()
            && required.iter().all(|seat| confirmed.contains(seat))
        {
            let deadline = (Utc::now() + TimeDelta::seconds(BOT_TAKEOVER_AUTO_CONTINUE_SECONDS))
                .to_rfc3339_opts(SecondsFormat::Micros, true);
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
            action.auto_advance_deadline_at = Some(deadline);
            return Ok(());
        }
    }

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

fn complete_continue_action_in_room_state(
    room: &mut RoomState,
    action_id: &str,
) -> Result<(), String> {
    room.continue_action = None;
    match action_id {
        "start_next_round" => complete_start_next_round_in_room_state(room),
        _ => Err("invalid_action".to_string()),
    }
}

fn complete_start_next_round_in_room_state(room: &mut RoomState) -> Result<(), String> {
    apply_settlement_to_match_in_room_state(room);
    let match_state = room
        .match_state
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let prevailing_wind = match_state.prevailing_wind.clone();
    let prevailing_wind = prevailing_wind.as_str();
    let hand_number = match_state.hand_number as usize;
    let dealer_seat = match_state.dealer_seat;
    let dealer_repeats = room.dealer_repeat_enabled && settlement_keeps_dealer(room, dealer_seat);
    let current_wind_index = WIND_ORDER
        .iter()
        .position(|wind| *wind == prevailing_wind)
        .unwrap_or(0);
    let next_dealer = if dealer_repeats {
        dealer_seat
    } else {
        (dealer_seat + 1) % MAX_SEATS
    };
    let mut next_hand_number = if dealer_repeats {
        hand_number
    } else {
        hand_number + 1
    };
    let mut next_wind = prevailing_wind.to_string();
    let mut match_finished = false;
    if !dealer_repeats && next_hand_number > MAX_SEATS {
        next_hand_number = 1;
        if current_wind_index == WIND_ORDER.len() - 1 {
            match_finished = true;
        } else {
            next_wind = WIND_ORDER[current_wind_index + 1].to_string();
        }
    }

    let next_dealer_repeat_count = if dealer_repeats {
        room.match_state
            .as_ref()
            .map(|m| m.dealer_repeat_count + 1)
            .unwrap_or(1)
    } else {
        0
    };

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
        match_state.dealer_repeat_count = if match_finished {
            match_state.dealer_repeat_count
        } else {
            next_dealer_repeat_count
        };
        match_state.match_finished = false;
    }

    if !match_finished && !dealer_repeats && next_hand_number == 1 {
        rotate_seats_after_wind_end(room, prevailing_wind);
    }

    if match_finished {
        room.pending_timeout = None;
        room.continue_action = None;
        return Ok(());
    }

    let base_seed = room
        .match_state
        .as_ref()
        .map(|match_state| match_state.seed)
        .unwrap_or_default();
    let round_seed = derive_round_seed(
        base_seed,
        &next_wind,
        next_hand_number,
        next_dealer,
        next_dealer_repeat_count,
    );
    let round_id = format!("{next_wind}-{next_hand_number}-dealer-{next_dealer}-seed-{round_seed}");
    start_round_in_room_state(room, next_dealer, &next_wind, round_id, round_seed);
    Ok(())
}

fn settlement_keeps_dealer(room: &RoomState, dealer_seat: usize) -> bool {
    let Some(settlement) = room
        .round_state
        .as_ref()
        .and_then(|round| round.settlement.as_ref())
    else {
        return false;
    };
    settlement.win_type == "draw" || settlement.winning_seats().contains(&dealer_seat)
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
    use crate::core::state::match_state::MatchSeatStatistics;
    use crate::core::state::{
        ContinueActionState, LastActionContext, PendingTimeout, PlayerRoundState,
        RoundScoreTrackers, RoundSettlement, RoundState, RuleRuntimeState, SeatState, WallState,
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
            user_id: None,
            nickname: Some(format!("P{seat_index}")),
            points: None,
            title: None,
            connected: true,
            is_bot: false,
            seat_type: "human".to_string(),
            bot_persona: None,
            bot_aggression: None,
            disconnect_deadline_at: None,
            consecutive_timeout_auto_response_count: 0,
        }
    }

    fn ready_room_with_seats(seats: Vec<SeatState>) -> RoomState {
        RoomState {
            table_code: "ROOM42".to_string(),
            phase: "waiting".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            minimum_hu_fan: crate::core::state::room::default_minimum_hu_fan(),
            dealer_repeat_enabled: false,
            dealer_double_enabled: false,
            ready_hand_enabled: true,
            seats,
            match_state: None,
            round_state: None,
            pending_timeout: None,
            continue_action: None,
        }
    }

    fn regular_bot_seat(seat_index: usize) -> SeatState {
        SeatState {
            is_bot: true,
            seat_type: "bot".to_string(),
            nickname: Some(format!("Bot {seat_index}")),
            ..seat_state(seat_index)
        }
    }

    #[test]
    fn room_ready_to_start_requires_four_occupied_seats_even_with_bots() {
        let room = ready_room_with_seats(vec![
            seat_state(0),
            regular_bot_seat(1),
            regular_bot_seat(2),
        ]);

        assert!(!room_ready_to_start(&room));
    }

    #[test]
    fn room_ready_to_start_allows_full_mixed_room() {
        let mut special_bot = regular_bot_seat(2);
        special_bot.seat_type = crate::special_bots::SPECIAL_BOT_SEAT_TYPE.to_string();
        let room = ready_room_with_seats(vec![
            seat_state(0),
            regular_bot_seat(1),
            special_bot,
            seat_state(3),
        ]);

        assert!(room_ready_to_start(&room));
    }

    fn flower_action_room() -> RoomState {
        RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            minimum_hu_fan: crate::core::state::room::default_minimum_hu_fan(),
            dealer_repeat_enabled: false,
            dealer_double_enabled: false,
            ready_hand_enabled: true,
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
                        is_ready_hand: false,
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
                        is_ready_hand: false,
                        concealed_tiles: vec![suit("t1", "t1#1")],
                        melds: vec![],
                        display_melds: vec![],
                        flowers: vec![],
                        discards: vec![],
                    },
                    PlayerRoundState {
                        seat: 2,
                        is_ready_hand: false,
                        concealed_tiles: vec![suit("b1", "b1#2")],
                        melds: vec![],
                        display_melds: vec![],
                        flowers: vec![],
                        discards: vec![],
                    },
                    PlayerRoundState {
                        seat: 3,
                        is_ready_hand: false,
                        concealed_tiles: vec![suit("w5", "w5#3")],
                        melds: vec![],
                        display_melds: vec![],
                        flowers: vec![],
                        discards: vec![],
                    },
                ],
                discard_history: Vec::new(),
                last_discard: None,
                pending_action: None,
                settlement: None,
                version: 1,
                score_trackers: RoundScoreTrackers::default(),
                last_action_context: LastActionContext::default(),
                rule_state: RuleRuntimeState {},
                restricted_discard_tile_key: None,
            }),
            pending_timeout: Some(PendingTimeout {
                kind: "active_turn".to_string(),
                seat_index: 0,
                deadline_at: None,
                drawn_tile_id: Some("f1#0".to_string()),
                extended_with_extra: false,
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

    fn settlement_room_at_wind_end(prevailing_wind: &str) -> RoomState {
        let mut room = flower_action_room();
        room.phase = "settlement".to_string();
        room.pending_timeout = None;
        room.continue_action = None;
        room.match_state = Some(MatchState {
            seed: 0,
            prevailing_wind: prevailing_wind.to_string(),
            hand_number: 4,
            dealer_seat: 3,
            dealer_repeat_count: 0,
            cumulative_scores: BTreeMap::from([(0, 0), (1, 0), (2, 0), (3, 0)]),
            match_finished: false,
            last_completed_round_id: None,
            statistics: Default::default(),
            extra_time_pool: Default::default(),
        });
        if let Some(round) = room.round_state.as_mut() {
            round.phase = "settlement".to_string();
            round.round_wind = prevailing_wind.to_string();
            round.dealer_seat = 3;
        }
        room
    }

    fn add_draw_settlement(room: &mut RoomState) {
        if let Some(round) = room.round_state.as_mut() {
            round.settlement = Some(RoundSettlement {
                win_type: "draw".to_string(),
                ..Default::default()
            });
        }
    }

    fn nicknames_by_seat(room: &RoomState) -> Vec<String> {
        let mut names = room
            .seats
            .iter()
            .map(|seat| {
                (
                    seat.seat_index,
                    seat.nickname.clone().expect("seat should have nickname"),
                )
            })
            .collect::<Vec<_>>();
        names.sort_by_key(|(seat, _)| *seat);
        names.into_iter().map(|(_, name)| name).collect()
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
    fn typed_flower_action_after_last_live_tile_drawn_settles_exhaustive_draw() {
        let mut room = flower_action_room();
        let round = room.round_state.as_mut().expect("round should exist");
        round.wall.head_index = 1;
        round.wall.tail_index = 0;
        round.last_action_context.was_last_live_tile = true;

        let output =
            apply_flower_action_output_in_room_state(&mut room, 0, &[String::from("f1#0")])
                .expect("flower action should settle exhaustive draw");

        assert_eq!(
            output.emitted_messages[0]["payload"]["event_type"],
            "round_drawn"
        );
        assert_eq!(room.phase, "settlement");
        assert_eq!(
            room.round_state.as_ref().map(|round| round.phase.as_str()),
            Some("settlement")
        );
        assert!(room.pending_timeout.is_none());
    }

    #[test]
    fn next_round_within_same_wind_keeps_seats() {
        let mut room = settlement_room_at_wind_end("east");
        let match_state = room.match_state.as_mut().expect("match should exist");
        match_state.hand_number = 3;
        match_state.dealer_seat = 2;

        complete_start_next_round_in_room_state(&mut room).expect("next round should start");

        assert_eq!(nicknames_by_seat(&room), vec!["P0", "P1", "P2", "P3"]);
        let match_state = room.match_state.as_ref().expect("match should exist");
        assert_eq!(match_state.prevailing_wind, "east");
        assert_eq!(match_state.hand_number, 4);
    }

    #[test]
    fn next_round_seed_is_derived_from_match_seed_and_state() {
        let mut first = settlement_room_at_wind_end("east");
        let mut second = settlement_room_at_wind_end("east");
        for room in [&mut first, &mut second] {
            let match_state = room.match_state.as_mut().expect("match should exist");
            match_state.hand_number = 2;
            match_state.dealer_seat = 1;
            match_state.seed = 777;
        }

        complete_start_next_round_in_room_state(&mut first).expect("first next round");
        complete_start_next_round_in_room_state(&mut second).expect("second next round");

        let first_round = first.round_state.as_ref().expect("first round");
        let second_round = second.round_state.as_ref().expect("second round");
        assert_eq!(
            first_round
                .wall
                .tiles
                .iter()
                .map(|tile| tile.tile_id.clone())
                .collect::<Vec<_>>(),
            second_round
                .wall
                .tiles
                .iter()
                .map(|tile| tile.tile_id.clone())
                .collect::<Vec<_>>()
        );
        assert!(first_round.round_id.contains("seed-"));
    }

    #[test]
    fn dealer_repeat_keeps_dealer_and_hand_when_dealer_wins() {
        let mut room = settlement_room_at_wind_end("east");
        room.dealer_repeat_enabled = true;
        let match_state = room.match_state.as_mut().expect("match should exist");
        match_state.hand_number = 2;
        match_state.dealer_seat = 1;
        if let Some(round) = room.round_state.as_mut() {
            round.dealer_seat = 1;
            round.settlement = Some(RoundSettlement {
                win_type: "self_draw".to_string(),
                winner_seat: Some(1),
                ..Default::default()
            });
        }

        complete_start_next_round_in_room_state(&mut room).expect("next round should start");

        let match_state = room.match_state.as_ref().expect("match should exist");
        assert_eq!(match_state.prevailing_wind, "east");
        assert_eq!(match_state.hand_number, 2);
        assert_eq!(match_state.dealer_seat, 1);
        assert_eq!(match_state.dealer_repeat_count, 1);
        assert_eq!(
            room.round_state.as_ref().map(|round| round.dealer_seat),
            Some(1)
        );
    }

    #[test]
    fn dealer_repeat_on_first_hand_keeps_seats() {
        let mut room = settlement_room_at_wind_end("east");
        room.dealer_repeat_enabled = true;
        let match_state = room.match_state.as_mut().expect("match should exist");
        match_state.hand_number = 1;
        match_state.dealer_seat = 0;
        if let Some(round) = room.round_state.as_mut() {
            round.dealer_seat = 0;
            round.settlement = Some(RoundSettlement {
                win_type: "self_draw".to_string(),
                winner_seat: Some(0),
                ..Default::default()
            });
        }

        complete_start_next_round_in_room_state(&mut room).expect("next round should start");

        assert_eq!(nicknames_by_seat(&room), vec!["P0", "P1", "P2", "P3"]);
        let match_state = room.match_state.as_ref().expect("match should exist");
        assert_eq!(match_state.prevailing_wind, "east");
        assert_eq!(match_state.hand_number, 1);
        assert_eq!(match_state.dealer_seat, 0);
        assert_eq!(match_state.dealer_repeat_count, 1);
    }

    #[test]
    fn dealer_repeat_keeps_dealer_and_hand_after_draw() {
        let mut room = settlement_room_at_wind_end("east");
        room.dealer_repeat_enabled = true;
        let match_state = room.match_state.as_mut().expect("match should exist");
        match_state.hand_number = 2;
        match_state.dealer_seat = 1;
        if let Some(round) = room.round_state.as_mut() {
            round.dealer_seat = 1;
            round.settlement = Some(RoundSettlement {
                win_type: "draw".to_string(),
                ..Default::default()
            });
        }

        complete_start_next_round_in_room_state(&mut room).expect("next round should start");

        let match_state = room.match_state.as_ref().expect("match should exist");
        assert_eq!(match_state.hand_number, 2);
        assert_eq!(match_state.dealer_seat, 1);
        assert_eq!(match_state.dealer_repeat_count, 1);
    }

    #[test]
    fn dealer_repeat_changes_seed_each_time() {
        let mut room = settlement_room_at_wind_end("east");
        room.dealer_repeat_enabled = true;
        let match_state = room.match_state.as_mut().expect("match should exist");
        match_state.hand_number = 1;
        match_state.dealer_seat = 0;
        match_state.dealer_repeat_count = 0;
        if let Some(round) = room.round_state.as_mut() {
            round.dealer_seat = 0;
            round.settlement = Some(RoundSettlement {
                win_type: "self_draw".to_string(),
                winner_seat: Some(0),
                ..Default::default()
            });
        }

        let first_round_id = room.round_state.as_ref().map(|r| r.round_id.clone());

        // First dealer repeat
        complete_start_next_round_in_room_state(&mut room).expect("next round should start");
        let second_round_id = room.round_state.as_ref().map(|r| r.round_id.clone());

        // Simulate another dealer win for second repeat
        if let Some(round) = room.round_state.as_mut() {
            round.settlement = Some(RoundSettlement {
                win_type: "self_draw".to_string(),
                winner_seat: Some(0),
                ..Default::default()
            });
        }

        // Second dealer repeat
        complete_start_next_round_in_room_state(&mut room).expect("next round should start");
        let third_round_id = room.round_state.as_ref().map(|r| r.round_id.clone());

        // All three rounds should have different seeds (different round_ids)
        assert_ne!(
            first_round_id, second_round_id,
            "First and second round should have different seeds"
        );
        assert_ne!(
            second_round_id, third_round_id,
            "Second and third round should have different seeds"
        );
        assert_ne!(
            first_round_id, third_round_id,
            "First and third round should have different seeds"
        );

        // Verify dealer_repeat_count is incrementing
        let match_state = room.match_state.as_ref().expect("match should exist");
        assert_eq!(match_state.dealer_repeat_count, 2);
    }

    #[test]
    fn dealer_repeat_resets_when_non_dealer_wins() {
        let mut room = settlement_room_at_wind_end("east");
        room.dealer_repeat_enabled = true;
        let match_state = room.match_state.as_mut().expect("match should exist");
        match_state.hand_number = 2;
        match_state.dealer_seat = 1;
        match_state.dealer_repeat_count = 2;
        if let Some(round) = room.round_state.as_mut() {
            round.dealer_seat = 1;
            round.settlement = Some(RoundSettlement {
                win_type: "discard".to_string(),
                winner_seat: Some(2),
                discarder_seat: Some(1),
                ..Default::default()
            });
        }

        complete_start_next_round_in_room_state(&mut room).expect("next round should start");

        let match_state = room.match_state.as_ref().expect("match should exist");
        assert_eq!(match_state.hand_number, 3);
        assert_eq!(match_state.dealer_seat, 2);
        assert_eq!(match_state.dealer_repeat_count, 0);
    }

    #[test]
    fn first_wind_end_swaps_east_south_and_west_north_seats() {
        let mut room = settlement_room_at_wind_end("east");

        complete_start_next_round_in_room_state(&mut room).expect("next wind should start");

        assert_eq!(nicknames_by_seat(&room), vec!["P1", "P0", "P3", "P2"]);
        let match_state = room.match_state.as_ref().expect("match should exist");
        assert_eq!(match_state.prevailing_wind, "south");
        assert_eq!(match_state.hand_number, 1);
    }

    #[test]
    fn wind_end_rotation_moves_scores_and_statistics_with_players() {
        let mut room = settlement_room_at_wind_end("east");
        let match_state = room.match_state.as_mut().expect("match should exist");
        match_state.cumulative_scores = BTreeMap::from([(0, 10), (1, 20), (2, 30), (3, 40)]);
        match_state.statistics.seat_stats_by_seat = BTreeMap::from([
            (
                0,
                MatchSeatStatistics {
                    score_history: vec![0, 10],
                    win_count: 1,
                    deal_in_count: 0,
                    ready_hand_win_count: 0,
                },
            ),
            (
                1,
                MatchSeatStatistics {
                    score_history: vec![0, 20],
                    win_count: 2,
                    deal_in_count: 1,
                    ready_hand_win_count: 0,
                },
            ),
            (
                2,
                MatchSeatStatistics {
                    score_history: vec![0, 30],
                    win_count: 3,
                    deal_in_count: 2,
                    ready_hand_win_count: 0,
                },
            ),
            (
                3,
                MatchSeatStatistics {
                    score_history: vec![0, 40],
                    win_count: 4,
                    deal_in_count: 3,
                    ready_hand_win_count: 0,
                },
            ),
        ]);

        complete_start_next_round_in_room_state(&mut room).expect("next wind should start");

        assert_eq!(nicknames_by_seat(&room), vec!["P1", "P0", "P3", "P2"]);
        let match_state = room.match_state.as_ref().expect("match should exist");
        assert_eq!(
            match_state.cumulative_scores,
            BTreeMap::from([(0, 20), (1, 10), (2, 40), (3, 30)])
        );
        assert_eq!(
            match_state
                .statistics
                .seat_stats_by_seat
                .get(&0)
                .map(|stats| (
                    stats.score_history.clone(),
                    stats.win_count,
                    stats.deal_in_count
                )),
            Some((vec![0, 20], 2, 1))
        );
        assert_eq!(
            match_state
                .statistics
                .seat_stats_by_seat
                .get(&1)
                .map(|stats| (
                    stats.score_history.clone(),
                    stats.win_count,
                    stats.deal_in_count
                )),
            Some((vec![0, 10], 1, 0))
        );
    }

    #[test]
    fn wind_end_rotation_moves_extra_time_with_players() {
        let mut room = settlement_room_at_wind_end("east");
        let match_state = room.match_state.as_mut().expect("match should exist");
        match_state.extra_time_pool = BTreeMap::from([(0, 11), (1, 22), (2, 33), (3, 44)]);

        complete_start_next_round_in_room_state(&mut room).expect("next wind should start");

        assert_eq!(nicknames_by_seat(&room), vec!["P1", "P0", "P3", "P2"]);
        let match_state = room.match_state.as_ref().expect("match should exist");
        assert_eq!(
            match_state.extra_time_pool,
            BTreeMap::from([(0, 22), (1, 11), (2, 44), (3, 33)])
        );
    }

    #[test]
    fn second_wind_end_moves_east_south_opposite_west_to_south_north_to_east() {
        let mut room = settlement_room_at_wind_end("south");

        complete_start_next_round_in_room_state(&mut room).expect("next wind should start");

        assert_eq!(nicknames_by_seat(&room), vec!["P3", "P2", "P0", "P1"]);
        let match_state = room.match_state.as_ref().expect("match should exist");
        assert_eq!(match_state.prevailing_wind, "west");
        assert_eq!(match_state.hand_number, 1);
    }

    #[test]
    fn third_wind_end_swaps_east_south_and_west_north_seats() {
        let mut room = settlement_room_at_wind_end("west");

        complete_start_next_round_in_room_state(&mut room).expect("next wind should start");

        assert_eq!(nicknames_by_seat(&room), vec!["P1", "P0", "P3", "P2"]);
        let match_state = room.match_state.as_ref().expect("match should exist");
        assert_eq!(match_state.prevailing_wind, "north");
        assert_eq!(match_state.hand_number, 1);
    }

    #[test]
    fn north_four_settlement_has_no_continue_action_and_rejects_restart() {
        let mut room = settlement_room_at_wind_end("north");
        add_draw_settlement(&mut room);

        reconcile_continue_action_in_room_state(&mut room)
            .expect("final settlement should reconcile");

        assert!(room.continue_action.is_none());
        assert_eq!(
            record_continue_action_in_room_state(&mut room, 0, "restart_match"),
            Err("invalid_action".to_string())
        );
        assert_eq!(room.phase, "settlement");
        let match_state = room.match_state.as_ref().expect("match should exist");
        assert_eq!(match_state.prevailing_wind, "north");
        assert_eq!(match_state.hand_number, 4);
        assert!(!match_state.match_finished);
        assert_eq!(match_state.statistics.completed_round_count, 1);
    }

    #[test]
    fn completing_north_four_next_round_keeps_room_in_final_settlement_until_players_leave() {
        let mut room = settlement_room_at_wind_end("north");
        add_draw_settlement(&mut room);

        complete_start_next_round_in_room_state(&mut room).expect("final settlement should apply");

        assert_eq!(room.phase, "settlement");
        assert!(room.pending_timeout.is_none());
        assert!(room.continue_action.is_none());
        let match_state = room.match_state.as_ref().expect("match should exist");
        assert_eq!(match_state.prevailing_wind, "north");
        assert_eq!(match_state.hand_number, 4);
        assert!(!match_state.match_finished);
        assert_eq!(match_state.statistics.completed_round_count, 1);
    }

    #[test]
    fn north_four_dealer_repeat_draw_still_allows_next_round() {
        let mut room = settlement_room_at_wind_end("north");
        room.dealer_repeat_enabled = true;
        if let Some(round) = room.round_state.as_mut() {
            round.settlement = Some(RoundSettlement {
                win_type: "draw".to_string(),
                ..Default::default()
            });
        }

        reconcile_continue_action_in_room_state(&mut room)
            .expect("repeat settlement should reconcile");

        assert_eq!(
            room.continue_action
                .as_ref()
                .map(|action| action.action_id.as_str()),
            Some("start_next_round")
        );

        complete_start_next_round_in_room_state(&mut room).expect("repeat round should start");

        let match_state = room.match_state.as_ref().expect("match should exist");
        assert_eq!(room.phase, "playing");
        assert_eq!(match_state.prevailing_wind, "north");
        assert_eq!(match_state.hand_number, 4);
        assert_eq!(match_state.dealer_seat, 3);
        assert_eq!(match_state.dealer_repeat_count, 1);
        assert!(!match_state.match_finished);
    }

    #[test]
    fn finished_match_has_no_continue_action_and_rejects_restart() {
        let mut room = settlement_room_at_wind_end("north");
        room.phase = "finished".to_string();
        let match_state = room.match_state.as_mut().expect("match should exist");
        match_state.match_finished = true;
        room.continue_action = Some(ContinueActionState {
            action_id: "restart_match".to_string(),
            confirmed_seats: Vec::new(),
            required_seats: Vec::new(),
            online_seats: Vec::new(),
            auto_advance_deadline_at: None,
        });

        reconcile_continue_action_in_room_state(&mut room)
            .expect("finished match should reconcile");

        assert!(room.continue_action.is_none());
        assert_eq!(
            record_continue_action_in_room_state(&mut room, 0, "restart_match"),
            Err("invalid_action".to_string())
        );
        assert_eq!(room.phase, "finished");
        assert_eq!(
            room.match_state
                .as_ref()
                .expect("match should exist")
                .match_finished,
            true
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

    #[test]
    fn reconcile_continue_action_treats_bot_takeover_seats_as_human() {
        let mut room = settlement_room_at_wind_end("east");
        room.seats[0].is_bot = true;
        room.seats[0].seat_type = "human".to_string();
        room.continue_action = Some(ContinueActionState {
            action_id: "start_next_round".to_string(),
            confirmed_seats: vec![1, 2, 3],
            required_seats: Vec::new(),
            online_seats: Vec::new(),
            auto_advance_deadline_at: None,
        });

        reconcile_continue_action_in_room_state(&mut room)
            .expect("continue action should reconcile");

        let action = room
            .continue_action
            .expect("bot takeover human seat should still be required");
        assert_eq!(
            action.confirmed_seats,
            vec![0, 1, 2, 3],
            "bot takeover seat should be auto-confirmed"
        );
        assert_eq!(action.required_seats, vec![0, 1, 2, 3]);
        assert_eq!(action.online_seats, vec![0, 1, 2, 3]);
        assert!(
            action.auto_advance_deadline_at.is_some(),
            "should set 3-second auto-advance deadline"
        );
    }
}
