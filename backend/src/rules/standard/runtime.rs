use chrono::{SecondsFormat, Utc};
use serde_json::{Value, json};

use crate::core::engine::planner::compute_pending_timeout_value;
use crate::core::state::RoomState;
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
    let deadline: chrono::DateTime<chrono::Utc> = match chrono::DateTime::parse_from_rfc3339(&deadline_str) {
        Ok(dt) => dt.into(),
        Err(_) => return,
    };
    let match_state = match room.match_state.as_mut() {
        Some(s) => s,
        None => return,
    };
    let seat = pending_timeout.seat_index;

    let now = Utc::now();
    if deadline > now {
        let remaining = (deadline - now).num_seconds().max(0);
        // 截止时间 = now + 15 + extra，剩余超过15s的部分即为未使用的额外时间
        if remaining > PENDING_TIMEOUT_SECONDS {
            let extra_unused = remaining - PENDING_TIMEOUT_SECONDS;
            *match_state.extra_time_pool.entry(seat).or_insert(0) += extra_unused;
        }
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
}
