use chrono::{SecondsFormat, Utc};
use serde_json::{Value, json};

use crate::core::engine::planner::compute_pending_timeout_value;
use crate::core::state::{RoomState, pending_action_response_seat};
use crate::core::tile::Tile;

const PENDING_TIMEOUT_SECONDS: i64 = 15;

#[cfg(test)]
pub fn project_room_state(room: &Value) -> Result<RoomState, String> {
    RoomState::from_room_value(room).map_err(|error| error.to_string())
}

pub fn sync_pending_timeout_in_room_state(room: &mut RoomState) {
    refund_unused_extra_time(room);
    room.pending_timeout = compute_pending_timeout_value(room, deadline_iso());
}

/// 在重新计算倒计时前，将当前回合未用完的额外思考时间退还到时间池
fn refund_unused_extra_time(room: &mut RoomState) {
    let pending_timeout = match room.pending_timeout.as_ref() {
        Some(t) => t.clone(),
        None => return,
    };
    let deadline_str = match pending_timeout.deadline_at.as_ref() {
        Some(s) => s.clone(),
        None => return,
    };
    let deadline: chrono::DateTime<chrono::Utc> =
        match chrono::DateTime::parse_from_rfc3339(&deadline_str) {
            Ok(dt) => dt.into(),
            Err(_) => return,
        };
    let seat = pending_timeout
        .extra_time_seat
        .or_else(|| pending_timeout_extra_time_seat(room))
        .unwrap_or(pending_timeout.seat_index);
    let match_state = match room.match_state.as_mut() {
        Some(s) => s,
        None => return,
    };

    let now = Utc::now();
    if deadline > now {
        let remaining = (deadline - now).num_seconds().max(0);
        // 只有被 extra_time 延长的倒计时才需要退还，普通 15s 倒计时的剩余不算额外时间
        if pending_timeout.extended_with_extra {
            *match_state.extra_time_pool.entry(seat).or_insert(0) += remaining;
        }
    }
}

fn pending_timeout_extra_time_seat(room: &RoomState) -> Option<usize> {
    let pending_timeout = room.pending_timeout.as_ref()?;
    match pending_timeout.kind.as_str() {
        "active_turn" => room.round_state.as_ref().map(|round| round.current_actor),
        "claim_window" => room
            .round_state
            .as_ref()
            .and_then(|round| round.pending_action.as_ref())
            .and_then(pending_action_response_seat),
        _ => None,
    }
}

pub fn round_event_message(event_type: &str, event: Value) -> Value {
    json!({
        "type": "round_event",
        "payload": {
            "event_type": event_type,
            "event": event,
        }
    })
}

#[cfg(test)]
pub fn current_actor(room: &Value) -> Option<usize> {
    room.get("round_state")
        .and_then(|round| round.get("current_actor"))
        .and_then(Value::as_u64)
        .map(|value| value as usize)
}

pub fn current_actor_in_room_state(room: &RoomState) -> Option<usize> {
    room.round_state.as_ref().map(|round| round.current_actor)
}

#[cfg(test)]
pub fn pending_timeout_kind(room: &Value) -> Option<&str> {
    room.get("pending_timeout")
        .and_then(|timeout| timeout.get("kind"))
        .and_then(Value::as_str)
}

#[cfg(test)]
pub fn replacement_tile_from_tail(room: &Value) -> Option<Value> {
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

pub fn replacement_tile_from_tail_in_room_state(room: &RoomState) -> Option<Tile> {
    let round = room.round_state.as_ref()?;
    let head_index = round.wall.head_index;
    let tail_index = round.wall.tail_index;
    if head_index > tail_index {
        return None;
    }
    round.wall.tiles.get(tail_index).cloned()
}

#[cfg(test)]
pub fn is_last_live_tile_point(room: &Value) -> bool {
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

pub fn is_last_live_tile_point_in_room_state(room: &RoomState) -> bool {
    room.round_state
        .as_ref()
        .map(|round| {
            let context = &round.last_action_context;
            context.kind == "draw" && context.was_last_live_tile && !context.from_kong_replacement
        })
        .unwrap_or(false)
}

#[cfg(test)]
pub fn player_concealed_tiles_slice(room: &Value, seat_index: usize) -> Option<&[Value]> {
    room.get("round_state")
        .and_then(|round| round.get("players"))
        .and_then(Value::as_array)
        .and_then(|players| players.get(seat_index))
        .and_then(|player| player.get("concealed_tiles"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
}

#[cfg(test)]
pub fn player_concealed_tile<'a>(
    room: &'a Value,
    seat_index: usize,
    tile_id: &str,
) -> Option<&'a Value> {
    player_concealed_tiles_slice(room, seat_index)?
        .iter()
        .find(|tile| tile.get("tile_id").and_then(Value::as_str) == Some(tile_id))
}

fn deadline_iso() -> String {
    (Utc::now() + chrono::TimeDelta::seconds(PENDING_TIMEOUT_SECONDS))
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;

    use super::sync_pending_timeout_in_room_state;
    use crate::core::state::RoomState;

    #[test]
    fn sync_pending_timeout_in_room_state_sets_future_deadline() {
        let mut room = RoomState::from_room_value(&json!({
            "table_code": "ROOM42",
            "phase": "playing",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "seats": [],
            "match_state": null,
            "round_state": {
                "round_id": "round-1",
                "dealer_seat": 0,
                "current_actor": 0,
                "wall": {
                    "tiles": [{
                        "tile_id": "w1#0",
                        "tile_key": "w1",
                        "kind": "suit",
                        "suit": "characters",
                        "rank": 1,
                        "name": "w1"
                    }],
                    "head_index": 0,
                    "tail_index": 0
                },
                "players": [{
                    "seat": 0,
                    "concealed_tiles": [{
                        "tile_id": "w1#0",
                        "tile_key": "w1",
                        "kind": "suit",
                        "suit": "characters",
                        "rank": 1,
                        "name": "w1"
                    }],
                    "melds": [],
                    "flowers": [],
                    "discards": []
                }],
                "last_discard": null,
                "pending_action": null,
                "phase": "playing",
                "settlement": null,
                "version": 1,
                "score_trackers": {
                    "kong_entries": []
                },
                "last_action_context": {
                    "kind": "draw",
                    "seat": 0,
                    "tile_id": "w1#0",
                    "from_kong_replacement": false,
                    "was_last_live_tile": false,
                    "was_last_discard": false
                },
                "round_wind": "east",
                "enforce_minimum_eight_fan": true,
                "restricted_discard_tile_key": null
            },
            "pending_timeout": null,
            "continue_action": null
        }))
        .expect("room should parse");

        sync_pending_timeout_in_room_state(&mut room);

        let deadline = room
            .pending_timeout
            .as_ref()
            .and_then(|timeout| timeout.deadline_at.as_deref())
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&Utc))
            .expect("sync should set a timeout deadline");
        assert!(deadline > Utc::now());
    }

    #[test]
    fn claim_window_refunds_unused_extra_time_to_current_responder() {
        let future_deadline = (Utc::now() + chrono::TimeDelta::seconds(30))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let mut room = RoomState::from_room_value(&json!({
            "table_code": "ROOM42",
            "phase": "playing",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "seats": [],
            "match_state": {
                "prevailing_wind": "east",
                "hand_number": 1,
                "dealer_seat": 0,
                "cumulative_scores": {"0": 0, "1": 0, "2": 0, "3": 0},
                "match_finished": false,
                "last_completed_round_id": null,
                "extra_time_pool": {"0": 0, "1": 0, "2": 0, "3": 0}
            },
            "round_state": {
                "round_id": "round-1",
                "dealer_seat": 0,
                "current_actor": 1,
                "wall": {"tiles": [], "head_index": 0, "tail_index": 0},
                "players": [],
                "last_discard": null,
                "pending_action": {
                    "type": "claim_window",
                    "discarder_seat": 1,
                    "claim_window": [["chow"], [], [], []],
                    "responded_seats": [],
                    "claim_responses": []
                },
                "phase": "playing",
                "settlement": null,
                "version": 1,
                "score_trackers": {"kong_entries": []},
                "last_action_context": {
                    "kind": "discard",
                    "seat": 1,
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
                "kind": "claim_window",
                "seat_index": 1,
                "deadline_at": future_deadline,
                "drawn_tile_id": null,
                "extended_with_extra": true
            },
            "continue_action": null
        }))
        .expect("room should parse");

        sync_pending_timeout_in_room_state(&mut room);

        let extra_time_pool = &room
            .match_state
            .as_ref()
            .expect("match should exist")
            .extra_time_pool;
        assert_eq!(extra_time_pool.get(&1), Some(&0));
        assert!(extra_time_pool.get(&0).copied().unwrap_or(0) > 0);
    }

    #[test]
    fn refund_uses_timeout_owner_after_turn_advances() {
        let future_deadline = (Utc::now() + chrono::TimeDelta::seconds(30))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let mut room = RoomState::from_room_value(&json!({
            "table_code": "ROOM42",
            "phase": "playing",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "seats": [],
            "match_state": {
                "prevailing_wind": "east",
                "hand_number": 1,
                "dealer_seat": 0,
                "cumulative_scores": {"0": 0, "1": 0, "2": 0, "3": 0},
                "match_finished": false,
                "last_completed_round_id": null,
                "extra_time_pool": {"0": 0, "1": 0, "2": 0, "3": 0}
            },
            "round_state": {
                "round_id": "round-1",
                "dealer_seat": 0,
                "current_actor": 1,
                "wall": {"tiles": [], "head_index": 0, "tail_index": 0},
                "players": [],
                "last_discard": null,
                "pending_action": null,
                "phase": "playing",
                "settlement": null,
                "version": 1,
                "score_trackers": {"kong_entries": []},
                "last_action_context": {
                    "kind": "draw",
                    "seat": 1,
                    "tile_id": null,
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
                "extra_time_seat": 0,
                "deadline_at": future_deadline,
                "drawn_tile_id": null,
                "extended_with_extra": true
            },
            "continue_action": null
        }))
        .expect("room should parse");

        sync_pending_timeout_in_room_state(&mut room);

        let extra_time_pool = &room
            .match_state
            .as_ref()
            .expect("match should exist")
            .extra_time_pool;
        assert!(extra_time_pool.get(&0).copied().unwrap_or(0) > 0);
        assert_eq!(extra_time_pool.get(&1), Some(&0));
    }
}
