#![allow(dead_code)]

use crate::bot::BotAction;
use crate::core::action::GameCommand;
use crate::core::engine::{
    EngineContext, EngineOutput, discard_supported_locally, parse_player_command,
    try_handle_command_in_room_state,
};
use crate::core::state::{RoomState, RoundSettlement};
use crate::projection::support::build_seat_projection_support_for_state;
use crate::rules::standard::{
    actions::apply_discard_action_output_in_room_state,
    automation::{
        next_bot_action_in_room_state, try_process_due_timeout as standard_try_process_due_timeout,
    },
    flow::{
        add_bot_seats_for_test as add_standard_test_bots,
        process_due_continue_action_in_room_state, reconcile_continue_action_state_in_room_state,
        record_continue_action_in_room_state,
        room_ready_to_start as room_ready_to_start_in_room_state, start_match_in_room_state,
    },
    runtime::project_room_state as standard_project_room_state,
    win::apply_hu_settlement_output_in_room_state,
};
use chrono::{SecondsFormat, Utc};
use serde_json::{Value, json};
const ACTIVE_TURN_TIMEOUT_SECONDS: i64 = 30;

#[cfg(test)]
fn action_prompt(room: &Value, local_seat: usize) -> Option<Value> {
    let state = project_room_state(room).ok()?;
    crate::projection::prompt::action_prompt_message(
        &state,
        local_seat,
        &build_seat_projection_support_for_state(&state, local_seat),
    )
}

pub fn room_ready_to_start(room: &Value) -> bool {
    project_room_state(room)
        .map(|state| room_ready_to_start_in_room_state(&state))
        .unwrap_or(false)
}

pub fn next_bot_action(room: &Value) -> Option<BotAction> {
    project_room_state(room)
        .ok()
        .and_then(|state| next_bot_action_in_room_state(&state).ok().flatten())
}

pub fn add_bot_seats_for_test(room: &mut Value) {
    let _ = with_room_state(room, |state| {
        add_standard_test_bots(state);
        Ok(())
    });
}

pub fn start_match(room: &mut Value, dealer_seat: usize, seed: u64) {
    let _ = with_room_state(room, |state| {
        start_match_in_room_state(state, dealer_seat, seed)
    });
}

pub fn try_handle_action(
    room: &mut Value,
    seat_index: usize,
    action_type: &str,
    tile_ids: &[String],
) -> Option<Result<Vec<Value>, String>> {
    let command = match parse_player_command(seat_index, action_type, tile_ids)? {
        Ok(command) => command,
        Err(reason) => return Some(Err(reason)),
    };
    try_handle_command(room, command).map(|result| result.map(|output| output.emitted_messages))
}

pub fn try_handle_command(
    room: &mut Value,
    command: GameCommand,
) -> Option<Result<EngineOutput, String>> {
    let mut room_state = RoomState::from_room_value(room)
        .ok()
        .map(EngineContext::from_room_state)?
        .room;
    let result = try_handle_command_in_room_state(&mut room_state, command).ok()?;
    *room = room_state.to_room_value().ok()?;
    result
}

pub fn try_process_due_timeout(room: &mut Value) -> Option<Vec<Value>> {
    standard_try_process_due_timeout(room)
}

pub fn record_continue_action(
    room: &mut Value,
    seat_index: usize,
    action_id: &str,
) -> Result<(), String> {
    with_room_state(room, |state| {
        record_continue_action_in_room_state(state, seat_index, action_id)
    })
}

pub fn process_due_continue_action(room: &mut Value) -> Result<bool, String> {
    with_room_state(room, process_due_continue_action_in_room_state)
}

pub fn reconcile_continue_action_state(room: &mut Value) -> Result<(), String> {
    with_room_state(room, |state| {
        reconcile_continue_action_state_in_room_state(state)
    })
}

pub fn apply_hu_settlement(
    room: &mut Value,
    winner_seat: usize,
    hu_context: &str,
    settlement: Value,
) -> Result<Vec<Value>, String> {
    with_room_state(room, |state| {
        apply_hu_settlement_output_in_room_state(
            state,
            winner_seat,
            hu_context,
            RoundSettlement::from_value(&settlement),
        )
        .map(|output| output.emitted_messages)
    })
}

#[cfg(test)]
fn room_snapshot(room: &Value, local_seat: usize) -> Value {
    let (state, support) = projected_room_message_context(room, local_seat);
    crate::projection::room_snapshot::room_snapshot_message(&state, local_seat, &support)
}

fn apply_discard_action(
    room: &mut Value,
    seat_index: usize,
    tile_id: &str,
) -> Result<Vec<Value>, String> {
    with_room_state(room, |state| {
        apply_discard_action_output_in_room_state(state, seat_index, tile_id)
            .map(|output| output.emitted_messages)
    })
}

fn can_resolve_discard_locally(room: &Value, seat_index: usize, tile_id: &str) -> bool {
    project_room_state(room)
        .map(EngineContext::from_room_state)
        .map(|context| discard_supported_locally(&context, seat_index, tile_id))
        .unwrap_or(false)
}

fn project_room_state(room: &Value) -> Result<RoomState, String> {
    standard_project_room_state(room)
}

fn with_room_state<T, F>(room: &mut Value, mutate: F) -> Result<T, String>
where
    F: FnOnce(&mut RoomState) -> Result<T, String>,
{
    let mut room_state = RoomState::from_room_value(room).map_err(|error| error.to_string())?;
    let result = mutate(&mut room_state)?;
    *room = room_state
        .to_room_value()
        .map_err(|error| error.to_string())?;
    Ok(result)
}

fn projected_room_message_context(
    room: &Value,
    local_seat: usize,
) -> (RoomState, crate::projection::SeatProjectionSupport) {
    match project_room_state(room) {
        Ok(state) => {
            let support = build_seat_projection_support_for_state(&state, local_seat);
            (state, support)
        }
        Err(_) => (fallback_room_state(room), Default::default()),
    }
}

fn fallback_room_state(room: &Value) -> RoomState {
    RoomState {
        table_code: room
            .get("table_code")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        phase: room
            .get("phase")
            .and_then(Value::as_str)
            .unwrap_or("waiting")
            .to_string(),
        mode: room
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("normal")
            .to_string(),
        ..Default::default()
    }
}

fn deadline_iso() -> String {
    (Utc::now() + chrono::TimeDelta::seconds(ACTIVE_TURN_TIMEOUT_SECONDS))
        .to_rfc3339_opts(SecondsFormat::Micros, true)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

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

    fn pending_timeout_deadline(room: &Value) -> chrono::DateTime<Utc> {
        room["pending_timeout"]["deadline_at"]
            .as_str()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&Utc))
            .expect("pending timeout should carry an RFC3339 deadline")
    }

    fn suit(tile_key: &str, tile_id: &str) -> Value {
        tile(tile_key, tile_id, "suit")
    }

    fn wind(tile_key: &str, tile_id: &str) -> Value {
        tile(tile_key, tile_id, "wind")
    }

    fn dragon(tile_key: &str, tile_id: &str) -> Value {
        tile(tile_key, tile_id, "dragon")
    }

    fn room_for_local_discard() -> Value {
        json!({
            "table_code": "ROOM1",
            "phase": "playing",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "continue_action": null,
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
            "continue_action": null,
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

    fn room_for_local_self_hu() -> Value {
        let mut room = room_for_local_discard();
        room["round_state"]["players"][0]["concealed_tiles"] = json!([
            dragon("red", "red#0"),
            dragon("red", "red#1"),
            dragon("red", "red#2"),
            dragon("green", "green#0"),
            dragon("green", "green#1"),
            dragon("green", "green#2"),
            dragon("white", "white#0"),
            dragon("white", "white#1"),
            dragon("white", "white#2"),
            suit("w1", "w1#0"),
            suit("w1", "w1#1"),
            suit("w1", "w1#2"),
            suit("w9", "w9#0"),
            suit("w9", "w9#1")
        ]);
        room["pending_timeout"]["drawn_tile_id"] = json!("w9#1");
        room["round_state"]["last_action_context"]["tile_id"] = json!("w9#1");
        room
    }

    fn room_for_bot_active_turn() -> Value {
        let mut room = room_for_local_discard();
        room["seats"][0]["is_bot"] = json!(true);
        room["seats"][0]["seat_type"] = json!("bot");
        room
    }

    fn room_for_last_live_tile_active_turn() -> Value {
        let mut room = room_for_local_discard();
        room["round_state"]["wall"]["tiles"] = json!([suit("w1", "w1#spent")]);
        room["round_state"]["wall"]["head_index"] = json!(1);
        room["round_state"]["wall"]["tail_index"] = json!(0);
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
            "continue_action": null,
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
            "continue_action": null,
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
                "wall": {"tiles": [], "head_index": 1, "tail_index": 0},
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
    fn local_discard_after_last_live_tile_drawn_settles_exhaustive_draw() {
        let mut room = room_for_last_live_tile_active_turn();
        assert!(can_resolve_discard_locally(&room, 0, "east#discard"));

        let result = try_handle_action(&mut room, 0, "discard", &[String::from("east#discard")])
            .expect("discard should be handled locally")
            .expect("discard should settle exhaustive draw");

        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["payload"]["event_type"], "tile_discarded");
        assert_eq!(result[1]["payload"]["event_type"], "round_drawn");
        assert_eq!(room["phase"], "settlement");
        assert_eq!(room["round_state"]["phase"], "settlement");
        assert!(room["pending_timeout"].is_null());
        assert_eq!(room["round_state"]["settlement"]["win_type"], "draw");
        assert_eq!(room["round_state"]["settlement"]["draw_type"], "exhaustive");
    }

    #[test]
    fn due_timeout_after_last_live_tile_drawn_settles_exhaustive_draw() {
        let mut room = room_for_last_live_tile_active_turn();

        let result = try_process_due_timeout(&mut room).expect("timeout should auto-resolve");

        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["payload"]["event_type"], "tile_discarded");
        assert_eq!(result[1]["payload"]["event_type"], "round_drawn");
        assert_eq!(room["phase"], "settlement");
        assert_eq!(room["round_state"]["phase"], "settlement");
        assert!(room["pending_timeout"].is_null());
        assert_eq!(room["round_state"]["settlement"]["win_type"], "draw");
        assert_eq!(room["round_state"]["settlement"]["draw_type"], "exhaustive");
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
    fn try_handle_command_emits_typed_claim_event() {
        let mut room = room_for_local_claim_window();
        let _ = try_handle_command(
            &mut room,
            GameCommand::PlayerAction {
                actor: 0,
                action: PlayerAction::Discard {
                    tile_id: "w3#discard".to_string(),
                },
            },
        )
        .expect("discard command should be handled locally")
        .expect("discard command should succeed");

        let output = try_handle_command(
            &mut room,
            GameCommand::PlayerAction {
                actor: 2,
                action: PlayerAction::Pung {
                    tile_ids: vec!["w3#2a".to_string(), "w3#2b".to_string()],
                },
            },
        )
        .expect("pung command should be handled locally")
        .expect("pung command should succeed");

        assert_eq!(output.emitted_messages.len(), 1);
        assert!(matches!(
            output.events.first(),
            Some(GameEvent::MeldClaimed { seat: 2, from: 0, meld })
                if meld == &vec!["w3".to_string(), "w3".to_string(), "w3".to_string()]
        ));
    }

    #[test]
    fn action_prompt_exposes_equipped_skill_option() {
        let mut room = room_for_local_discard();
        room["mode"] = json!("skill");
        room["round_state"]["players"][0]["skill_loadout"] = json!({
            "equipped": [{
                "skill_id": "score_boost",
                "owner": 0,
                "level": 1,
                "cooldown": 0,
                "charges": 1,
                "config": {"amount": 2}
            }]
        });

        let prompt = action_prompt(&room, 0).expect("prompt should exist");
        let options = prompt["payload"]["options"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert!(
            options
                .iter()
                .any(|option| option.as_str() == Some("skill:score_boost"))
        );
    }

    #[test]
    fn local_skill_action_persists_effects_and_consumes_charge() {
        let mut room = room_for_local_discard();
        room["mode"] = json!("skill");
        room["round_state"]["players"][0]["skill_loadout"] = json!({
            "equipped": [{
                "skill_id": "score_boost",
                "owner": 0,
                "level": 1,
                "cooldown": 0,
                "charges": 1,
                "config": {"amount": 2}
            }]
        });

        let result = try_handle_action(&mut room, 0, "skill:score_boost", &[])
            .expect("skill should be handled locally")
            .expect("skill should succeed");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["payload"]["event_type"], "skill_activated");
        assert_eq!(
            room["round_state"]["players"][0]["skill_loadout"]["equipped"][0]["charges"],
            0
        );
        assert_eq!(
            room["round_state"]["effect_state"]["rule_overrides"][0]["rule_key"],
            "bonus_fan"
        );
        assert_eq!(
            room["round_state"]["effect_state"]["ongoing"][0]["effect_type"],
            "score_boost"
        );
    }

    #[test]
    fn score_boost_skill_increases_settlement_fan_total() {
        let mut baseline_room = room_for_local_self_hu();
        let _ = try_handle_action(&mut baseline_room, 0, "hu", &[])
            .expect("baseline hu should be handled locally")
            .expect("baseline hu should succeed");
        let baseline_fan_total = baseline_room["round_state"]["settlement"]["fan_total"]
            .as_i64()
            .expect("baseline fan total");

        let mut boosted_room = room_for_local_self_hu();
        boosted_room["mode"] = json!("skill");
        boosted_room["round_state"]["players"][0]["skill_loadout"] = json!({
            "equipped": [{
                "skill_id": "score_boost",
                "owner": 0,
                "level": 1,
                "cooldown": 0,
                "charges": 1,
                "config": {"amount": 2}
            }]
        });

        let _ = try_handle_action(&mut boosted_room, 0, "skill:score_boost", &[])
            .expect("skill should be handled locally")
            .expect("skill should succeed");
        let _ = try_handle_action(&mut boosted_room, 0, "hu", &[])
            .expect("boosted hu should be handled locally")
            .expect("boosted hu should succeed");

        let boosted_fan_total = boosted_room["round_state"]["settlement"]["fan_total"]
            .as_i64()
            .expect("boosted fan total");
        assert_eq!(boosted_fan_total, baseline_fan_total + 2);
        assert!(
            boosted_room["round_state"]["settlement"]["fan_keys"]
                .as_array()
                .is_some_and(|keys| keys
                    .iter()
                    .any(|key| key.as_str() == Some("skill_bonus:score_boost")))
        );
    }

    #[test]
    fn peek_skill_populates_private_knowledge_in_snapshot() {
        let mut room = room_for_local_discard();
        room["mode"] = json!("skill");
        room["round_state"]["players"][0]["skill_loadout"] = json!({
            "equipped": [{
                "skill_id": "peek_opponent_tile",
                "owner": 0,
                "level": 1,
                "cooldown": 0,
                "charges": 1,
                "config": {}
            }]
        });

        let _ = try_handle_action(
            &mut room,
            0,
            "skill:peek_opponent_tile",
            &[String::from("seat:1")],
        )
        .expect("peek skill should be handled locally")
        .expect("peek skill should succeed");

        let snapshot = room_snapshot(&room, 0);
        assert_eq!(
            snapshot["payload"]["private_state"]["visible_effects"][0]["effect_type"],
            "peek_opponent_tile"
        );
        assert_eq!(
            snapshot["payload"]["private_state"]["private_knowledge"][0]["target_seat"],
            1
        );
        assert_eq!(
            snapshot["payload"]["private_state"]["private_knowledge"][0]["tile_keys"][0],
            "w1"
        );
    }

    #[test]
    fn pass_on_hu_window_triggers_decline_hu_skill_effect() {
        let mut room = room_for_local_add_kong_with_robber();
        room["round_state"]["players"][1]["skill_loadout"] = json!({
            "equipped": [{
                "skill_id": "yu_qin_gu_zong",
                "owner": 1,
                "level": 1,
                "cooldown": 0,
                "charges": 1,
                "config": {}
            }]
        });

        let _ = try_handle_action(&mut room, 0, "kong", &[String::from("w3#add")])
            .expect("add kong should be handled locally")
            .expect("add kong should open rob kong window");

        let _ = try_handle_action(&mut room, 1, "pass", &[])
            .expect("rob kong pass should be handled locally")
            .expect("pass should succeed");

        assert_eq!(
            room["round_state"]["effect_state"]["ongoing"][0]["effect_type"],
            "yu_qin_gu_zong_window"
        );
        assert_eq!(
            room["round_state"]["effect_state"]["ongoing"][0]["owner"],
            1
        );
    }

    #[test]
    fn zou_wei_shang_ji_forces_draw_without_next_round_penalty() {
        let mut room = room_for_local_discard();
        room["mode"] = json!("skill");
        room["round_state"]["players"][0]["skill_loadout"] = json!({
            "equipped": [{
                "skill_id": "zou_wei_shang_ji",
                "owner": 0,
                "level": 1,
                "cooldown": 0,
                "charges": 1,
                "config": {}
            }]
        });

        let result = try_handle_action(&mut room, 0, "skill:zou_wei_shang_ji", &[])
            .expect("skill should be handled locally")
            .expect("skill should succeed");

        assert!(
            result
                .iter()
                .any(|message| message["payload"]["event_type"] == "round_drawn")
        );
        assert!(
            result
                .iter()
                .any(|message| message["payload"]["event_type"] == "skill_force_draw")
        );
        assert_eq!(room["phase"], "settlement");
        assert_eq!(
            room["round_state"]["settlement"]["draw_type"],
            "skill_forced"
        );
        assert!(
            room["match_state"]["skill_trackers"]["zou_wei_shang_ji"]["pending_win_penalty"]
                .as_object()
                .map(|penalties| penalties.is_empty())
                .unwrap_or(true)
        );
    }

    #[test]
    fn jin_chan_tuo_qiao_blocks_claim_window_for_next_discard() {
        let mut room = room_for_local_claim_window();
        room["mode"] = json!("skill");
        room["round_state"]["players"][0]["skill_loadout"] = json!({
            "equipped": [{
                "skill_id": "jin_chan_tuo_qiao",
                "owner": 0,
                "level": 1,
                "cooldown": 0,
                "charges": 1,
                "charges_per_round": 1,
                "config": {}
            }]
        });

        let _ = try_handle_action(&mut room, 0, "skill:jin_chan_tuo_qiao", &[])
            .expect("skill should be handled locally")
            .expect("skill should succeed");

        let result = try_handle_action(&mut room, 0, "discard", &[String::from("w3#discard")])
            .expect("discard should be handled locally")
            .expect("discard should succeed");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["payload"]["event_type"], "tile_discarded");
        assert_eq!(room["round_state"]["current_actor"], 1);
        assert_eq!(room["pending_timeout"]["kind"], "active_turn");
        assert!(room["round_state"]["pending_action"].is_null());
        assert!(
            room["round_state"]["effect_state"]["ongoing"]
                .as_array()
                .map(|effects| effects.is_empty())
                .unwrap_or(true)
        );
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
    fn local_chow_claim_keeps_turn_timeout_in_future() {
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

        assert_eq!(room["pending_timeout"]["kind"], "active_turn");
        assert!(pending_timeout_deadline(&room) > Utc::now());
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
        assert_eq!(room["match_state"]["statistics"]["completed_round_count"], 1);
        assert_eq!(
            room["match_state"]["statistics"]["seat_stats_by_seat"]["1"]["score_history"],
            json!([0, 24])
        );
        assert_eq!(
            room["match_state"]["statistics"]["seat_stats_by_seat"]["1"]["win_count"],
            1
        );
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
        assert_eq!(room["continue_action"], Value::Null);
    }

    #[test]
    fn local_start_next_round_preserves_skill_loadout_from_previous_round() {
        let mut room = room_for_local_continue_action();
        room["round_state"]["players"][0]["skill_loadout"] = json!({
            "equipped": [{
                "skill_id": "sheng_dong_ji_xi",
                "owner": 0,
                "level": 1,
                "cooldown": 0,
                "charges": 1,
                "config": {}
            }]
        });

        record_continue_action(&mut room, 0, "start_next_round")
            .expect("continue action should succeed");

        assert_eq!(
            room["round_state"]["players"][0]["skill_loadout"]["equipped"][0]["skill_id"],
            "sheng_dong_ji_xi"
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
        room["match_state"]["statistics"] = json!({
            "completed_round_count": 3,
            "seat_stats_by_seat": {
                "0": {"score_history": [0, 12, 20], "win_count": 2},
                "1": {"score_history": [0, -4, -10], "win_count": 0},
                "2": {"score_history": [0, -4, -5], "win_count": 1},
                "3": {"score_history": [0, -4, -5], "win_count": 0}
            }
        });

        record_continue_action(&mut room, 0, "restart_match").expect("restart should succeed");

        assert_eq!(room["phase"], "playing");
        assert_eq!(room["match_state"]["prevailing_wind"], "east");
        assert_eq!(room["match_state"]["hand_number"], 1);
        assert_eq!(
            room["match_state"]["cumulative_scores"],
            json!({"0": 0, "1": 0, "2": 0, "3": 0})
        );
        assert_eq!(room["match_state"]["statistics"]["completed_round_count"], 0);
        assert_eq!(
            room["match_state"]["statistics"]["seat_stats_by_seat"]["0"]["score_history"],
            json!([0])
        );
    }
}
