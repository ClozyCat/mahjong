use chrono::{SecondsFormat, Utc};
use serde_json::{Value, json};

use crate::core::engine::planner::compute_pending_timeout_value;
use crate::core::engine::reducer::update_room_state;
use crate::core::state::RoomState;
use crate::core::tile::Tile;

pub fn project_room_state(room: &Value) -> Result<RoomState, String> {
    RoomState::from_room_value(room).map_err(|error| error.to_string())
}

pub fn sync_pending_timeout(room: &mut Value) {
    let pending_timeout = project_room_state(room)
        .ok()
        .and_then(|state| compute_pending_timeout_value(&state, deadline_iso()));
    let _ = update_room_state(room, |state| {
        state.pending_timeout = pending_timeout;
        Ok(())
    });
}

pub fn sync_pending_timeout_in_room_state(room: &mut RoomState) {
    room.pending_timeout = compute_pending_timeout_value(room, deadline_iso());
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

pub fn current_actor(room: &Value) -> Option<usize> {
    room.get("round_state")
        .and_then(|round| round.get("current_actor"))
        .and_then(Value::as_u64)
        .map(|value| value as usize)
}

pub fn current_actor_in_room_state(room: &RoomState) -> Option<usize> {
    room.round_state.as_ref().map(|round| round.current_actor)
}

pub fn pending_timeout_kind(room: &Value) -> Option<&str> {
    room.get("pending_timeout")
        .and_then(|timeout| timeout.get("kind"))
        .and_then(Value::as_str)
}

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

pub fn player_concealed_tiles_slice(room: &Value, seat_index: usize) -> Option<&[Value]> {
    room.get("round_state")
        .and_then(|round| round.get("players"))
        .and_then(Value::as_array)
        .and_then(|players| players.get(seat_index))
        .and_then(|player| player.get("concealed_tiles"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
}

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
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
