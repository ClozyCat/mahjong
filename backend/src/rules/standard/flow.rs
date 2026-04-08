use chrono::{SecondsFormat, TimeDelta, Utc};
use rand::Rng;
use serde_json::{Value, json};
use std::collections::BTreeMap;

use crate::core::engine::planner::{
    plan_advance_opening_flowers, plan_flower_action, plan_round_start_payload,
};
use crate::core::engine::reducer::{LegacyRoomMutation, apply_legacy_room_mutations};
use crate::core::state::{MatchState, SkillLoadout};
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
    let _ = apply_legacy_room_mutations(
        room,
        &[LegacyRoomMutation::SetRoomMatchState {
            match_state: Some(match_state),
        }],
    );
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
        LegacyRoomMutation::AddStartNextRoundConfirmedSeat { seat_index }
    } else {
        LegacyRoomMutation::AddRestartMatchConfirmedSeat { seat_index }
    };
    apply_legacy_room_mutations(room, &[field])?;
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

pub fn apply_opening_flowers_pass(
    room: &mut Value,
    seat_index: usize,
) -> Result<Vec<Value>, String> {
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
    let mutations = plan_advance_opening_flowers(&state, seat_index);
    apply_legacy_room_mutations(room, &mutations)?;
    sync_round_skill_trackers(room);
    sync_pending_timeout(room);
    Ok(vec![])
}

pub fn apply_flower_action(
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
    note_tracker_draw(room, seat_index, &plan.replacement_tile.tile_key);
    sync_round_skill_trackers(room);
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
    let _ = apply_legacy_room_mutations(
        room,
        &[
            LegacyRoomMutation::SetRoomPhase {
                phase: "playing".to_string(),
            },
            LegacyRoomMutation::SetRoomRoundState {
                round_state: Some(round_state),
            },
            LegacyRoomMutation::SetRoomPendingTimeout {
                pending_timeout: Some(pending_timeout),
            },
            LegacyRoomMutation::SetStartNextRoundConfirmedSeats { seats: vec![] },
            LegacyRoomMutation::SetRestartMatchConfirmedSeats { seats: vec![] },
            LegacyRoomMutation::SetContinueActionAutoAdvanceDeadline { deadline_at: None },
        ],
    );
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
            SkillLoadout::from_legacy_value(seat_state.get("skill_loadout")).ok()
        })
        .filter(|loadout| !loadout.equipped.is_empty())
        .or_else(|| {
            room.get("round_state")
                .and_then(|round| round.get("players"))
                .and_then(Value::as_array)
                .and_then(|players| players.get(seat))
                .and_then(|player| {
                    SkillLoadout::from_legacy_value(player.get("skill_loadout")).ok()
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
            &[LegacyRoomMutation::SetContinueActionAutoAdvanceDeadline { deadline_at: None }],
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
            &[LegacyRoomMutation::SetContinueActionAutoAdvanceDeadline { deadline_at: None }],
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
            &[LegacyRoomMutation::SetContinueActionAutoAdvanceDeadline {
                deadline_at: Some(
                    (Utc::now() + TimeDelta::seconds(CONTINUE_ACTION_AUTO_ADVANCE_SECONDS))
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
            LegacyRoomMutation::SetContinueActionAutoAdvanceDeadline { deadline_at: None },
            LegacyRoomMutation::SetStartNextRoundConfirmedSeats { seats: vec![] },
            LegacyRoomMutation::SetRestartMatchConfirmedSeats { seats: vec![] },
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
            LegacyRoomMutation::SetMatchPrevailingWind {
                prevailing_wind: next_wind.clone(),
            },
            LegacyRoomMutation::SetMatchHandNumber {
                hand_number: if match_finished {
                    hand_number as u32
                } else {
                    next_hand_number as u32
                },
            },
            LegacyRoomMutation::SetMatchDealerSeat {
                dealer_seat: if match_finished {
                    dealer_seat
                } else {
                    next_dealer
                },
            },
            LegacyRoomMutation::SetMatchFinished { match_finished },
        ],
    )?;

    if match_finished {
        apply_legacy_room_mutations(
            room,
            &[
                LegacyRoomMutation::SetRoomPhase {
                    phase: "finished".to_string(),
                },
                LegacyRoomMutation::SetRoomPendingTimeout {
                    pending_timeout: None,
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
