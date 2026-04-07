use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub enum LegacyRoomMutation {
    RemovePlayerConcealedTileById {
        seat_index: usize,
        tile_id: String,
    },
    PushPlayerDiscard {
        seat_index: usize,
        tile: Value,
    },
    PushPlayerConcealedTile {
        seat_index: usize,
        tile: Value,
    },
    PushPlayerMeld {
        seat_index: usize,
        meld: Value,
    },
    PushPlayerFlower {
        seat_index: usize,
        tile: Value,
    },
    AppendTileToPlayerMeld {
        seat_index: usize,
        meld_index: usize,
        tile: Value,
    },
    PopPlayerDiscardLast {
        seat_index: usize,
    },
    AdvanceWallHead,
    RetreatWallTail,
    SetRoundLastDiscard {
        tile: Value,
    },
    SetRoundPendingAction {
        pending_action: Value,
    },
    SetRoundRestrictedDiscardTileKey {
        tile_key: Value,
    },
    SetRoundLastActionContext {
        context: Value,
    },
    SetRoundCurrentActor {
        seat_index: usize,
    },
    SetRoundField {
        key: String,
        value: Value,
    },
    IncrementRoundVersion,
    AppendRoundKongEntry {
        kong_type: String,
        actor_seat: usize,
        payer_seats: Vec<usize>,
        tile_key: Value,
    },
    SetRoomField {
        key: String,
        value: Value,
    },
    SetMatchField {
        key: String,
        value: Value,
    },
    PushUniqueSeatToRoomArray {
        key: String,
        seat_index: usize,
    },
}

pub fn apply_legacy_room_mutations(
    room: &mut Value,
    mutations: &[LegacyRoomMutation],
) -> Result<(), String> {
    for mutation in mutations {
        apply_legacy_room_mutation(room, mutation)?;
    }
    Ok(())
}

fn apply_legacy_room_mutation(
    room: &mut Value,
    mutation: &LegacyRoomMutation,
) -> Result<(), String> {
    match mutation {
        LegacyRoomMutation::RemovePlayerConcealedTileById {
            seat_index,
            tile_id,
        } => {
            let concealed_tiles = player_zone_mut(room, *seat_index, "concealed_tiles")?;
            let tile_index = concealed_tiles
                .iter()
                .position(|tile| {
                    tile.get("tile_id").and_then(Value::as_str) == Some(tile_id.as_str())
                })
                .ok_or_else(|| "invalid_action".to_string())?;
            concealed_tiles.remove(tile_index);
            Ok(())
        }
        LegacyRoomMutation::PushPlayerDiscard { seat_index, tile } => {
            player_zone_mut(room, *seat_index, "discards")?.push(tile.clone());
            Ok(())
        }
        LegacyRoomMutation::PushPlayerConcealedTile { seat_index, tile } => {
            player_zone_mut(room, *seat_index, "concealed_tiles")?.push(tile.clone());
            Ok(())
        }
        LegacyRoomMutation::PushPlayerMeld { seat_index, meld } => {
            player_zone_mut(room, *seat_index, "melds")?.push(meld.clone());
            Ok(())
        }
        LegacyRoomMutation::PushPlayerFlower { seat_index, tile } => {
            player_zone_mut(room, *seat_index, "flowers")?.push(tile.clone());
            Ok(())
        }
        LegacyRoomMutation::AppendTileToPlayerMeld {
            seat_index,
            meld_index,
            tile,
        } => {
            let melds = player_zone_mut(room, *seat_index, "melds")?;
            let meld = melds
                .get_mut(*meld_index)
                .and_then(Value::as_array_mut)
                .ok_or_else(|| "invalid_action".to_string())?;
            meld.push(tile.clone());
            Ok(())
        }
        LegacyRoomMutation::PopPlayerDiscardLast { seat_index } => {
            player_zone_mut(room, *seat_index, "discards")?.pop();
            Ok(())
        }
        LegacyRoomMutation::AdvanceWallHead => {
            let wall = room
                .get_mut("round_state")
                .and_then(Value::as_object_mut)
                .and_then(|round_state| round_state.get_mut("wall"))
                .and_then(Value::as_object_mut)
                .ok_or_else(|| "invalid_action".to_string())?;
            let head_index = wall
                .get("head_index")
                .and_then(Value::as_u64)
                .ok_or_else(|| "invalid_action".to_string())?;
            wall.insert("head_index".to_string(), json!(head_index + 1));
            Ok(())
        }
        LegacyRoomMutation::RetreatWallTail => {
            let wall = room
                .get_mut("round_state")
                .and_then(Value::as_object_mut)
                .and_then(|round_state| round_state.get_mut("wall"))
                .and_then(Value::as_object_mut)
                .ok_or_else(|| "invalid_action".to_string())?;
            let tail_index = wall
                .get("tail_index")
                .and_then(Value::as_i64)
                .ok_or_else(|| "invalid_action".to_string())?;
            wall.insert("tail_index".to_string(), json!(tail_index - 1));
            Ok(())
        }
        LegacyRoomMutation::SetRoundLastDiscard { tile } => {
            round_state_insert(room, "last_discard", tile.clone())
        }
        LegacyRoomMutation::SetRoundPendingAction { pending_action } => {
            round_state_insert(room, "pending_action", pending_action.clone())
        }
        LegacyRoomMutation::SetRoundRestrictedDiscardTileKey { tile_key } => {
            round_state_insert(room, "restricted_discard_tile_key", tile_key.clone())
        }
        LegacyRoomMutation::SetRoundLastActionContext { context } => {
            round_state_insert(room, "last_action_context", context.clone())
        }
        LegacyRoomMutation::SetRoundCurrentActor { seat_index } => {
            round_state_insert(room, "current_actor", json!(seat_index))
        }
        LegacyRoomMutation::SetRoundField { key, value } => {
            round_state_insert(room, key, value.clone())
        }
        LegacyRoomMutation::IncrementRoundVersion => {
            let round_state = room
                .get_mut("round_state")
                .and_then(Value::as_object_mut)
                .ok_or_else(|| "invalid_action".to_string())?;
            let version = round_state
                .get("version")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                + 1;
            round_state.insert("version".to_string(), json!(version));
            Ok(())
        }
        LegacyRoomMutation::AppendRoundKongEntry {
            kong_type,
            actor_seat,
            payer_seats,
            tile_key,
        } => {
            let round_state = room
                .get_mut("round_state")
                .and_then(Value::as_object_mut)
                .ok_or_else(|| "invalid_action".to_string())?;
            let score_trackers = round_state
                .entry("score_trackers".to_string())
                .or_insert_with(|| json!({}));
            let trackers = score_trackers
                .as_object_mut()
                .ok_or_else(|| "invalid_action".to_string())?;
            let kong_entries = trackers
                .entry("kong_entries".to_string())
                .or_insert_with(|| Value::Array(vec![]));
            let entries = kong_entries
                .as_array_mut()
                .ok_or_else(|| "invalid_action".to_string())?;
            entries.push(json!({
                "kong_type": kong_type,
                "actor_seat": actor_seat,
                "payer_seats": payer_seats,
                "tile_key": tile_key,
            }));
            Ok(())
        }
        LegacyRoomMutation::SetRoomField { key, value } => {
            let object = room
                .as_object_mut()
                .ok_or_else(|| "invalid_action".to_string())?;
            object.insert(key.clone(), value.clone());
            Ok(())
        }
        LegacyRoomMutation::SetMatchField { key, value } => {
            let match_state = room
                .get_mut("match_state")
                .and_then(Value::as_object_mut)
                .ok_or_else(|| "invalid_action".to_string())?;
            match_state.insert(key.clone(), value.clone());
            Ok(())
        }
        LegacyRoomMutation::PushUniqueSeatToRoomArray { key, seat_index } => {
            let array = room
                .get_mut(key)
                .and_then(Value::as_array_mut)
                .ok_or_else(|| "invalid_action".to_string())?;
            if !array.iter().any(|value| {
                value
                    .as_u64()
                    .map(|seat| seat as usize == *seat_index)
                    .unwrap_or(false)
            }) {
                array.push(json!(seat_index));
            }
            Ok(())
        }
    }
}

fn player_zone_mut<'a>(
    room: &'a mut Value,
    seat_index: usize,
    zone: &str,
) -> Result<&'a mut Vec<Value>, String> {
    room.get_mut("round_state")
        .and_then(Value::as_object_mut)
        .and_then(|round_state| round_state.get_mut("players"))
        .and_then(Value::as_array_mut)
        .and_then(|players| players.get_mut(seat_index))
        .and_then(Value::as_object_mut)
        .and_then(|player| player.get_mut(zone))
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "invalid_action".to_string())
}

fn round_state_insert(room: &mut Value, key: &str, value: Value) -> Result<(), String> {
    let round_state = room
        .get_mut("round_state")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "invalid_action".to_string())?;
    round_state.insert(key.to_string(), value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{LegacyRoomMutation, apply_legacy_room_mutations};

    #[test]
    fn applies_basic_discard_mutations() {
        let mut room = json!({
            "round_state": {
                "version": 1,
                "head_index": 0,
                "current_actor": 0,
                "wall": {
                    "head_index": 0
                },
                "players": [
                    {
                        "concealed_tiles": [
                            {"tile_id": "w1#0", "tile_key": "w1"},
                            {"tile_id": "w2#0", "tile_key": "w2"}
                        ],
                        "discards": []
                    }
                ]
            }
        });

        let mutations = vec![
            LegacyRoomMutation::RemovePlayerConcealedTileById {
                seat_index: 0,
                tile_id: "w1#0".to_string(),
            },
            LegacyRoomMutation::PushPlayerDiscard {
                seat_index: 0,
                tile: json!({"tile_id": "w1#0", "tile_key": "w1"}),
            },
            LegacyRoomMutation::SetRoundLastDiscard {
                tile: json!({"tile_id": "w1#0", "tile_key": "w1"}),
            },
            LegacyRoomMutation::IncrementRoundVersion,
        ];

        apply_legacy_room_mutations(&mut room, &mutations).expect("mutations should apply");
        assert_eq!(
            room["round_state"]["players"][0]["concealed_tiles"],
            json!([{"tile_id":"w2#0","tile_key":"w2"}])
        );
        assert_eq!(
            room["round_state"]["players"][0]["discards"],
            json!([{"tile_id":"w1#0","tile_key":"w1"}])
        );
        assert_eq!(room["round_state"]["last_discard"]["tile_key"], "w1");
        assert_eq!(room["round_state"]["version"], 2);
    }
}
