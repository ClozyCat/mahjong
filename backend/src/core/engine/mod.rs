pub mod planner;
pub mod reducer;

use serde_json::Value;

use crate::core::action::{GameCommand, PlayerAction};
use crate::core::event::GameEvent;
use crate::core::ids::Seat;
use crate::core::state::RoomState;
use crate::core::tile::Tile;

#[derive(Debug, Clone, Default)]
pub struct EngineOutput {
    pub events: Vec<GameEvent>,
    pub emitted_messages: Vec<Value>,
}

impl EngineOutput {
    pub fn from_emitted_messages(emitted_messages: Vec<Value>) -> Self {
        let events = extract_events_from_messages(&emitted_messages);
        Self {
            events,
            emitted_messages,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EngineContext {
    pub room: RoomState,
}

impl EngineContext {
    pub fn from_legacy_room(room: &Value) -> Result<Self, String> {
        RoomState::from_legacy_value(room)
            .map(|room| Self { room })
            .map_err(|error| error.to_string())
    }

    pub fn current_actor(&self) -> Option<Seat> {
        self.room.round_state.as_ref().map(|round| round.current_actor)
    }
}

pub fn parse_legacy_player_command(
    actor: Seat,
    action_type: &str,
    tile_ids: &[String],
) -> Option<Result<GameCommand, String>> {
    let action = match action_type {
        "hu" => Ok(PlayerAction::Hu),
        "flower" => Ok(PlayerAction::Flower {
            tile_ids: tile_ids.to_vec(),
        }),
        "discard" => {
            if tile_ids.len() != 1 {
                Err("select_tile_first".to_string())
            } else {
                Ok(PlayerAction::Discard {
                    tile_id: tile_ids[0].clone(),
                })
            }
        }
        "chow" => Ok(PlayerAction::Chow {
            tile_ids: tile_ids.to_vec(),
        }),
        "pung" => Ok(PlayerAction::Pung {
            tile_ids: tile_ids.to_vec(),
        }),
        "kong" => Ok(PlayerAction::Kong {
            tile_ids: tile_ids.to_vec(),
        }),
        "pass" => Ok(PlayerAction::Pass),
        _ => return None,
    };

    Some(action.map(|action| GameCommand::PlayerAction { actor, action }))
}

pub fn extract_events_from_messages(messages: &[Value]) -> Vec<GameEvent> {
    messages.iter().filter_map(extract_event_from_message).collect()
}

fn extract_event_from_message(message: &Value) -> Option<GameEvent> {
    if message.get("type").and_then(Value::as_str) != Some("round_event") {
        return None;
    }
    let payload = message.get("payload")?;
    let event_type = payload.get("event_type").and_then(Value::as_str)?.to_string();
    let event = payload.get("event").cloned().unwrap_or(Value::Null);

    match event_type.as_str() {
        "tile_discarded" => Some(GameEvent::TileDiscarded {
            seat: event
                .get("seat")
                .and_then(Value::as_u64)
                .map(|value| value as Seat)
                .unwrap_or(0),
            tile: event_tile(&event),
        }),
        "replacement_draw" => Some(GameEvent::TileDrawn {
            seat: event
                .get("seat")
                .and_then(Value::as_u64)
                .map(|value| value as Seat)
                .unwrap_or(0),
            tile: event_tile(&event),
            source: "replacement_draw".to_string(),
        }),
        "self_hu_declared" => Some(GameEvent::HuDeclared {
            winner: event
                .get("seat")
                .and_then(Value::as_u64)
                .map(|value| value as Seat)
                .unwrap_or(0),
            source: "self_draw".to_string(),
        }),
        "claim_made"
            if event.get("claim_type").and_then(Value::as_str) == Some("hu") =>
        {
            Some(GameEvent::HuDeclared {
                winner: event
                    .get("seat")
                    .and_then(Value::as_u64)
                    .map(|value| value as Seat)
                    .unwrap_or(0),
                source: "discard".to_string(),
            })
        }
        _ => Some(GameEvent::LegacyRoundEvent { event_type, event }),
    }
}

fn event_tile(event: &Value) -> Tile {
    let tile_id = event
        .get("tile_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let tile_key = event
        .get("tile_key")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Tile {
        tile_id,
        tile_key,
        kind: String::new(),
        suit: None,
        rank: None,
        name: None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{extract_events_from_messages, parse_legacy_player_command};
    use crate::core::action::{GameCommand, PlayerAction};
    use crate::core::event::GameEvent;

    #[test]
    fn parses_legacy_discard_command() {
        let command = parse_legacy_player_command(2, "discard", &[String::from("w1#0")])
            .expect("discard should be recognized")
            .expect("discard should parse");
        assert_eq!(
            command,
            GameCommand::PlayerAction {
                actor: 2,
                action: PlayerAction::Discard {
                    tile_id: "w1#0".to_string()
                }
            }
        );
    }

    #[test]
    fn rejects_legacy_discard_without_selection() {
        let command = parse_legacy_player_command(0, "discard", &[])
            .expect("discard should be recognized");
        assert_eq!(command, Err("select_tile_first".to_string()));
    }

    #[test]
    fn extracts_typed_and_legacy_round_events() {
        let messages = vec![
            json!({
                "type": "round_event",
                "payload": {
                    "event_type": "tile_discarded",
                    "event": {
                        "seat": 1,
                        "tile_id": "east#0"
                    }
                }
            }),
            json!({
                "type": "round_event",
                "payload": {
                    "event_type": "claim_auto_passed",
                    "event": {
                        "seat": 2
                    }
                }
            }),
        ];

        let events = extract_events_from_messages(&messages);
        assert!(matches!(
            &events[0],
            GameEvent::TileDiscarded { seat: 1, tile } if tile.tile_id == "east#0"
        ));
        assert!(matches!(
            &events[1],
            GameEvent::LegacyRoundEvent { event_type, .. } if event_type == "claim_auto_passed"
        ));
    }
}
