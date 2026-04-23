use serde_json::Value;

use crate::core::action::{GameCommand, PlayerAction};
use crate::core::event::GameEvent;
use crate::core::ids::Seat;
use crate::core::state::{RoomState, RoundSettlement};
use crate::core::tile::Tile;

#[derive(Debug, Clone, Default)]
pub struct EngineOutput {
    pub events: Vec<GameEvent>,
    pub emitted_messages: Vec<Value>,
}

impl EngineOutput {
    pub fn new(events: Vec<GameEvent>, emitted_messages: Vec<Value>) -> Self {
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
    pub fn new(room: RoomState) -> Self {
        Self { room }
    }

    pub fn from_room_state(room: RoomState) -> Self {
        Self::new(room)
    }

    pub fn current_actor(&self) -> Option<Seat> {
        self.room
            .round_state
            .as_ref()
            .map(|round| round.current_actor)
    }
}

pub fn parse_player_command(
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
        "ready_hand" => {
            if tile_ids.len() != 1 {
                Err("select_tile_first".to_string())
            } else {
                Ok(PlayerAction::ReadyHand {
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

#[cfg(test)]
fn extract_events_from_messages(messages: &[Value]) -> Vec<GameEvent> {
    messages
        .iter()
        .filter_map(extract_event_from_message)
        .collect()
}

#[cfg(test)]
fn extract_event_from_message(message: &Value) -> Option<GameEvent> {
    if message.get("type").and_then(Value::as_str) != Some("round_event") {
        return None;
    }
    let payload = message.get("payload")?;
    let event_type = payload
        .get("event_type")
        .and_then(Value::as_str)?
        .to_string();
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
        "ready_hand_declared" => Some(GameEvent::ReadyHandDeclared {
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
        "claim_made" => {
            let seat = event
                .get("seat")
                .and_then(Value::as_u64)
                .map(|value| value as Seat)
                .unwrap_or(0);
            let claim_type = event.get("claim_type").and_then(Value::as_str);
            if claim_type == Some("hu") {
                Some(GameEvent::HuDeclared {
                    winner: seat,
                    source: "discard".to_string(),
                })
            } else {
                Some(GameEvent::MeldClaimed {
                    seat,
                    meld: event
                        .get("meld")
                        .and_then(Value::as_array)
                        .map(|meld| {
                            meld.iter()
                                .filter_map(|tile| tile.as_str().map(ToString::to_string))
                                .collect()
                        })
                        .unwrap_or_default(),
                    from: event
                        .get("from")
                        .and_then(Value::as_u64)
                        .map(|value| value as Seat)
                        .unwrap_or(0),
                })
            }
        }
        "flower_exposed" => Some(GameEvent::FlowerExposed {
            seat: event
                .get("seat")
                .and_then(Value::as_u64)
                .map(|value| value as Seat)
                .unwrap_or(0),
            tile_id: event
                .get("tile_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        }),
        "self_kong_declared" => Some(GameEvent::SelfKongDeclared {
            seat: event
                .get("seat")
                .and_then(Value::as_u64)
                .map(|value| value as Seat)
                .unwrap_or(0),
            kong_type: event
                .get("kong_type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            tile_key: event
                .get("tile_key")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            tile_ids: event
                .get("tile_ids")
                .and_then(Value::as_array)
                .map(|tile_ids| {
                    tile_ids
                        .iter()
                        .filter_map(|tile_id| tile_id.as_str().map(ToString::to_string))
                        .collect()
                })
                .unwrap_or_default(),
        }),
        "claim_auto_passed" => Some(GameEvent::ClaimAutoPassed {
            discarder_seat: event
                .get("discarder_seat")
                .and_then(Value::as_u64)
                .map(|value| value as Seat)
                .unwrap_or(0),
            seats: event
                .get("seats")
                .and_then(Value::as_array)
                .map(|seats| {
                    seats
                        .iter()
                        .filter_map(|seat| seat.as_u64().map(|value| value as Seat))
                        .collect()
                })
                .unwrap_or_default(),
        }),
        "settlement_ready" | "round_drawn" => event
            .get("settlement")
            .map(RoundSettlement::from_value)
            .map(|settlement| GameEvent::SettlementPrepared { settlement })
            .or(None),
        _ => None,
    }
}

#[cfg(test)]
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

    use super::{extract_events_from_messages, parse_player_command};
    use crate::core::action::{GameCommand, PlayerAction};
    use crate::core::event::GameEvent;

    #[test]
    fn parses_discard_command() {
        let command = parse_player_command(2, "discard", &[String::from("w1#0")])
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
    fn rejects_discard_without_selection() {
        let command =
            parse_player_command(0, "discard", &[]).expect("discard should be recognized");
        assert_eq!(command, Err("select_tile_first".to_string()));
    }

    #[test]
    fn parses_ready_hand_command() {
        let command = parse_player_command(2, "ready_hand", &[String::from("b9#discard")])
            .expect("ready_hand should be recognized")
            .expect("ready_hand should parse");

        assert!(matches!(
            command,
            GameCommand::PlayerAction { actor: 2, .. }
        ));
    }

    #[test]
    fn extracts_round_events() {
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
                    "event_type": "settlement_ready",
                    "event": {
                        "round_id": "east-1",
                        "settlement": {
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
                                "fan_delta_by_seat": {"0": -16, "1": 32, "2": -8, "3": -8},
                                "kong_delta_by_seat": {"0": 0, "1": 0, "2": 0, "3": 0},
                                "total_delta_by_seat": {"0": -16, "1": 32, "2": -8, "3": -8}
                            },
                            "flower_count": 0,
                            "draw_type": null,
                            "kong_score_detail": []
                        }
                    }
                }
            }),
            json!({
                "type": "round_event",
                "payload": {
                    "event_type": "claim_made",
                    "event": {
                        "seat": 2,
                        "from": 0,
                        "claim_type": "pung",
                        "meld": ["w3", "w3", "w3"]
                    }
                }
            }),
            json!({
                "type": "round_event",
                "payload": {
                    "event_type": "claim_auto_passed",
                    "event": {
                        "discarder_seat": 0,
                        "seats": [2]
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
            GameEvent::SettlementPrepared { settlement }
                if settlement.winner_seat == Some(1) && settlement.fan_total == 8
        ));
        assert!(matches!(
            &events[2],
            GameEvent::MeldClaimed { seat: 2, from: 0, meld }
                if meld == &vec!["w3".to_string(), "w3".to_string(), "w3".to_string()]
        ));
        assert!(matches!(
            &events[3],
            GameEvent::ClaimAutoPassed { discarder_seat: 0, seats } if seats == &vec![2]
        ));
    }
}
