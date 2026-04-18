use serde_json::Value;

use crate::core::state::{ContinueActionState, RoomState};

pub fn update_room_state<F>(room: &mut Value, update: F) -> Result<(), String>
where
    F: FnOnce(&mut RoomState) -> Result<(), String>,
{
    let mut state = RoomState::from_room_value(room).map_err(|error| error.to_string())?;
    update(&mut state)?;
    refresh_continue_action_state(&mut state);
    *room = state.to_room_value().map_err(|error| error.to_string())?;
    Ok(())
}

fn ensure_continue_action<'a>(
    room: &'a mut RoomState,
    action_id: &str,
) -> &'a mut ContinueActionState {
    let required_seats = room
        .seats
        .iter()
        .filter(|seat| !seat.is_bot)
        .map(|seat| seat.seat_index)
        .collect::<Vec<_>>();
    let online_seats = room
        .seats
        .iter()
        .filter(|seat| !seat.is_bot && seat.connected)
        .map(|seat| seat.seat_index)
        .collect::<Vec<_>>();
    room.continue_action
        .get_or_insert_with(|| ContinueActionState {
            action_id: action_id.to_string(),
            confirmed_seats: Vec::new(),
            required_seats: required_seats.clone(),
            online_seats: online_seats.clone(),
            auto_advance_deadline_at: None,
        });
    let action = room
        .continue_action
        .as_mut()
        .expect("continue action inserted");
    action.action_id = action_id.to_string();
    action.required_seats = required_seats;
    action.online_seats = online_seats;
    action
}

fn refresh_continue_action_state(room: &mut RoomState) {
    let action_id = match room.phase.as_str() {
        "settlement" => Some("start_next_round"),
        "finished" => Some("restart_match"),
        _ => None,
    };
    let Some(action_id) = action_id else {
        room.continue_action = None;
        return;
    };
    let confirmed = room
        .continue_action
        .as_ref()
        .filter(|action| action.action_id == action_id)
        .map(|action| action.confirmed_seats.clone())
        .unwrap_or_default();
    let deadline = room
        .continue_action
        .as_ref()
        .filter(|action| action.action_id == action_id)
        .and_then(|action| action.auto_advance_deadline_at.clone());
    let action = ensure_continue_action(room, action_id);
    action.confirmed_seats = confirmed;
    action.auto_advance_deadline_at = deadline;
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::update_room_state;
    use crate::core::state::ContinueActionState;

    #[test]
    fn applies_basic_discard_update_and_preserves_room_shape() {
        let mut room = json!({
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
                "round_wind": "east",
                "current_actor": 0,
                "phase": "playing",
                "wall": {
                    "tiles": [],
                    "head_index": 0,
                    "tail_index": 0
                },
                "players": [{
                    "seat": 0,
                    "concealed_tiles": [
                        {"tile_id": "w1#0", "tile_key": "w1", "kind": "suit", "suit": "characters", "rank": 1, "name": "Character 1"},
                        {"tile_id": "w2#0", "tile_key": "w2", "kind": "suit", "suit": "characters", "rank": 2, "name": "Character 2"}
                    ],
                    "melds": [],
                    "flowers": [],
                    "discards": []
                }],
                "last_discard": null,
                "pending_action": null,
                "settlement": null,
                "version": 1,
                "score_trackers": {
                    "kong_entries": [],
                    "opening_flowers_completed": true
                },
                "last_action_context": {
                    "kind": "draw",
                    "seat": 0,
                    "tile_id": null,
                    "from_kong_replacement": false,
                    "was_last_live_tile": false,
                    "was_last_discard": false
                },
                "restricted_discard_tile_key": null,
                "enforce_minimum_eight_fan": true
            },
            "pending_timeout": null,
            "continue_action": null
        });

        update_room_state(&mut room, |state| {
            let round = state
                .round_state
                .as_mut()
                .ok_or_else(|| "invalid_action".to_string())?;
            let player = round
                .players
                .get_mut(0)
                .ok_or_else(|| "invalid_action".to_string())?;
            let tile_index = player
                .concealed_tiles
                .iter()
                .position(|tile| tile.tile_id == "w1#0")
                .ok_or_else(|| "invalid_action".to_string())?;
            let discarded_tile = player.concealed_tiles.remove(tile_index);
            player.discards.push(discarded_tile.clone());
            round.last_discard = Some(discarded_tile);
            round.version += 1;
            Ok(())
        })
        .expect("typed update should apply");

        assert_eq!(
            room["round_state"]["players"][0]["concealed_tiles"],
            json!([{
                "tile_id":"w2#0",
                "tile_key":"w2",
                "kind":"suit",
                "suit":"characters",
                "rank":2,
                "name":"Character 2"
            }])
        );
        assert_eq!(
            room["round_state"]["players"][0]["discards"][0]["tile_id"],
            "w1#0"
        );
        assert_eq!(room["round_state"]["last_discard"]["tile_key"], "w1");
        assert_eq!(room["round_state"]["version"], 2);
    }

    #[test]
    fn refreshes_continue_action_shape_from_typed_state() {
        let mut room = json!({
            "table_code": "ROOM42",
            "phase": "settlement",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "seats": [{
                "seat_index": 0,
                "nickname": "Alice",
                "reconnect_token": "token-1",
                "player_session_id": 1,
                "connected": true,
                "ready": true,
                "is_bot": false,
                "seat_type": "human",
                "bot_persona": null,
                "bot_aggression": null,
                "disconnect_deadline_at": null
            }],
            "match_state": null,
            "round_state": null,
            "pending_timeout": null,
            "continue_action": null
        });

        update_room_state(&mut room, |state| {
            state.continue_action = Some(ContinueActionState {
                action_id: "start_next_round".to_string(),
                confirmed_seats: vec![0],
                required_seats: Vec::new(),
                online_seats: Vec::new(),
                auto_advance_deadline_at: Some("2026-04-08T00:00:00Z".to_string()),
            });
            Ok(())
        })
        .expect("typed update should apply");

        assert_eq!(room["continue_action"]["confirmed_seats"], json!([0]));
        assert_eq!(
            room["continue_action"]["auto_advance_deadline_at"],
            json!("2026-04-08T00:00:00Z")
        );
    }
}
