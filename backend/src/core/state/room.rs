use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::error::EngineError;
use crate::core::ids::TableCode;
use crate::core::state::pending::ContinueActionState;

use super::{MatchState, PendingTimeout, RoundState, SeatState, array, bool_or};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RoomState {
    pub table_code: TableCode,
    pub phase: String,
    pub mode: String,
    pub test_mode: bool,
    pub enforce_minimum_eight_fan: bool,
    pub seats: Vec<SeatState>,
    pub match_state: Option<MatchState>,
    pub round_state: Option<RoundState>,
    pub pending_timeout: Option<PendingTimeout>,
    pub continue_action: Option<ContinueActionState>,
}

impl RoomState {
    pub fn from_room_value(value: &Value) -> Result<Self, EngineError> {
        let seats = value
            .get("seats")
            .map(|seats| {
                array(seats, "room.seats").map(|seats| {
                    seats
                        .iter()
                        .map(SeatState::from_value)
                        .collect::<Vec<_>>()
                })
            })
            .transpose()?
            .unwrap_or_default();
        let match_state = value
            .get("match_state")
            .filter(|match_state| !match_state.is_null())
            .map(MatchState::from_value)
            .transpose()?;
        let round_state = value
            .get("round_state")
            .filter(|round_state| !round_state.is_null())
            .map(RoundState::from_value)
            .transpose()?;
        let pending_timeout = value
            .get("pending_timeout")
            .filter(|pending| !pending.is_null())
            .map(PendingTimeout::from_value);
        Ok(Self {
            table_code: value
                .get("table_code")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            phase: value
                .get("phase")
                .and_then(Value::as_str)
                .unwrap_or("waiting")
                .to_string(),
            mode: value
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or("normal")
                .to_string(),
            test_mode: bool_or(value, "test_mode", false),
            enforce_minimum_eight_fan: bool_or(value, "enforce_minimum_eight_fan", true),
            seats,
            match_state,
            round_state,
            pending_timeout,
            continue_action: parse_continue_action(value),
        })
    }

    pub fn from_room_str(raw: &str) -> Result<Self, EngineError> {
        let value: Value = serde_json::from_str(raw)?;
        Self::from_room_value(&value)
    }

    pub fn to_room_value(&self) -> Result<Value, serde_json::Error> {
        let mut value = serde_json::to_value(self)?;
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "round_state".to_string(),
                match &self.round_state {
                    Some(round) => round.to_value()?,
                    None => Value::Null,
                },
            );
        }
        Ok(value)
    }
}

fn parse_continue_action(value: &Value) -> Option<ContinueActionState> {
    if let Some(action) = value
        .get("continue_action")
        .filter(|action| !action.is_null())
        .cloned()
    {
        return serde_json::from_value(action).ok();
    }
    let action_id = match value.get("phase").and_then(Value::as_str) {
        Some("settlement") => Some("start_next_round"),
        Some("finished") => Some("restart_match"),
        _ => None,
    }?;
    let confirmed_field = if action_id == "start_next_round" {
        "start_next_round_confirmed_seats"
    } else {
        "restart_match_confirmed_seats"
    };
    let confirmed_seats = super::seat_vec(value.get(confirmed_field));
    let required_seats = value
        .get("seats")
        .and_then(Value::as_array)
        .map(|seats| {
            seats
                .iter()
                .filter(|seat| !seat.get("is_bot").and_then(Value::as_bool).unwrap_or(false))
                .filter_map(|seat| seat.get("seat_index").and_then(Value::as_u64))
                .map(|seat| seat as usize)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let online_seats = value
        .get("seats")
        .and_then(Value::as_array)
        .map(|seats| {
            seats
                .iter()
                .filter(|seat| {
                    seat.get("connected")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                })
                .filter(|seat| !seat.get("is_bot").and_then(Value::as_bool).unwrap_or(false))
                .filter_map(|seat| seat.get("seat_index").and_then(Value::as_u64))
                .map(|seat| seat as usize)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let auto_advance_deadline_at = value
        .get("continue_action_auto_advance_deadline_at")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    Some(ContinueActionState {
        action_id: action_id.to_string(),
        confirmed_seats,
        required_seats,
        online_seats,
        auto_advance_deadline_at,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::RoomState;
    use crate::core::state::{ContinueActionState, SeatState};

    #[test]
    fn parses_waiting_room_legacy_shape() {
        let room = json!({
            "table_code": "ABCD",
            "phase": "waiting",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "seats": [{
                "seat_index": 0,
                "nickname": "Alice",
                "reconnect_token": "token-1",
                "player_session_id": 1,
                "connected": true,
                "ready": false,
                "is_bot": false,
                "seat_type": "human",
                "bot_persona": null,
                "bot_aggression": null,
                "disconnect_deadline_at": null
            }],
            "match_state": null,
            "round_state": null,
            "pending_timeout": null,
            "start_next_round_confirmed_seats": [],
            "restart_match_confirmed_seats": [],
            "continue_action_auto_advance_deadline_at": null
        });

        let parsed = RoomState::from_room_value(&room).expect("room should parse");
        assert_eq!(parsed.table_code, "ABCD");
        assert_eq!(parsed.phase, "waiting");
        assert_eq!(parsed.seats.len(), 1);
        assert!(parsed.round_state.is_none());
        assert!(parsed.continue_action.is_none());
    }

    #[test]
    fn parses_playing_room_legacy_shape() {
        let room = json!({
            "table_code": "WXYZ",
            "phase": "playing",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "seats": [
                {
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
                },
                {
                    "seat_index": 1,
                    "nickname": "Bot 1",
                    "reconnect_token": null,
                    "player_session_id": -2,
                    "connected": true,
                    "ready": true,
                    "is_bot": true,
                    "seat_type": "bot",
                    "bot_persona": null,
                    "bot_aggression": null,
                    "disconnect_deadline_at": null
                }
            ],
            "match_state": {
                "prevailing_wind": "east",
                "hand_number": 1,
                "dealer_seat": 0,
                "cumulative_scores": {
                    "0": 0,
                    "1": 0
                },
                "match_finished": false,
                "last_completed_round_id": null
            },
            "round_state": {
                "round_id": "east-1-dealer-0-seed",
                "dealer_seat": 0,
                "current_actor": 0,
                "wall": {
                    "tiles": [
                        {
                            "tile_id": "w1#0",
                            "tile_key": "w1",
                            "kind": "suit",
                            "suit": "characters",
                            "rank": 1,
                            "name": "Character 1"
                        }
                    ],
                    "head_index": 1,
                    "tail_index": 143
                },
                "players": [
                    {
                        "seat": 0,
                        "concealed_tiles": [
                            {
                                "tile_id": "w2#0",
                                "tile_key": "w2",
                                "kind": "suit",
                                "suit": "characters",
                                "rank": 2,
                                "name": "Character 2"
                            }
                        ],
                        "melds": [],
                        "flowers": [],
                        "discards": []
                    },
                    {
                        "seat": 1,
                        "concealed_tiles": [],
                        "melds": [["w3", "w4", "w5"]],
                        "flowers": [],
                        "discards": [
                            {
                                "tile_id": "east#0",
                                "tile_key": "east",
                                "kind": "wind",
                                "suit": null,
                                "rank": null,
                                "name": "East Wind"
                            }
                        ]
                    }
                ],
                "last_discard": {
                    "tile_id": "east#0",
                    "tile_key": "east",
                    "kind": "wind",
                    "suit": null,
                    "rank": null,
                    "name": "East Wind"
                },
                "pending_action": {
                    "type": "claim_window",
                    "discarder_seat": 0,
                    "claim_window": [[], ["pung", "hu"]],
                    "responded_seats": []
                },
                "phase": "playing",
                "settlement": null,
                "version": 2,
                "score_trackers": {
                    "kong_entries": [{
                        "kong_type": "melded_kong",
                        "actor_seat": 1,
                        "payer_seats": [0],
                        "tile_key": "east"
                    }],
                    "opening_flowers_completed": true
                },
                "last_action_context": {
                    "kind": "discard",
                    "seat": 0,
                    "tile_id": "east#0",
                    "from_kong_replacement": false,
                    "was_last_live_tile": false,
                    "was_last_discard": true
                },
                "round_wind": "east",
                "enforce_minimum_eight_fan": true,
                "restricted_discard_tile_key": null
            },
            "pending_timeout": {
                "kind": "claim_window",
                "seat_index": 0,
                "deadline_at": "2026-04-07T10:00:00Z",
                "drawn_tile_id": null
            },
            "start_next_round_confirmed_seats": [],
            "restart_match_confirmed_seats": [],
            "continue_action_auto_advance_deadline_at": null
        });

        let parsed = RoomState::from_room_value(&room).expect("room should parse");
        assert_eq!(parsed.seats.len(), 2);
        let round = parsed.round_state.expect("round state");
        assert_eq!(round.players.len(), 2);
        assert_eq!(round.wall.live_tiles_remaining(), 143);
        assert_eq!(
            round
                .last_discard
                .as_ref()
                .map(|tile| tile.tile_key.as_str()),
            Some("east")
        );
        assert_eq!(
            round
                .pending_action
                .as_ref()
                .map(|action| action.action_type()),
            Some("claim_window")
        );
        assert_eq!(
            parsed
                .pending_timeout
                .as_ref()
                .map(|timeout| timeout.kind.as_str()),
            Some("claim_window")
        );
    }

    #[test]
    fn parses_settlement_continue_action_from_legacy_shape() {
        let room = json!({
            "table_code": "DONE",
            "phase": "settlement",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "seats": [
                {
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
                },
                {
                    "seat_index": 1,
                    "nickname": "Bot 1",
                    "reconnect_token": null,
                    "player_session_id": -2,
                    "connected": true,
                    "ready": true,
                    "is_bot": true,
                    "seat_type": "bot",
                    "bot_persona": null,
                    "bot_aggression": null,
                    "disconnect_deadline_at": null
                }
            ],
            "match_state": null,
            "round_state": null,
            "pending_timeout": null,
            "start_next_round_confirmed_seats": [0],
            "restart_match_confirmed_seats": [],
            "continue_action_auto_advance_deadline_at": "2026-04-07T10:10:00Z"
        });

        let parsed = RoomState::from_room_value(&room).expect("room should parse");
        let continue_action = parsed.continue_action.expect("continue action");
        assert_eq!(continue_action.action_id, "start_next_round");
        assert_eq!(continue_action.confirmed_seats, vec![0]);
        assert_eq!(continue_action.required_seats, vec![0]);
        assert_eq!(continue_action.online_seats, vec![0]);
    }

    #[test]
    fn serializes_continue_action_in_typed_shape() {
        let room = RoomState {
            table_code: "DONE".to_string(),
            phase: "settlement".to_string(),
            mode: "normal".to_string(),
            test_mode: false,
            enforce_minimum_eight_fan: true,
            seats: vec![SeatState {
                seat_index: 0,
                nickname: Some("Alice".to_string()),
                reconnect_token: Some("token-1".to_string()),
                player_session_id: Some(1),
                connected: true,
                ready: true,
                is_bot: false,
                seat_type: "human".to_string(),
                bot_persona: None,
                bot_aggression: None,
                disconnect_deadline_at: None,
            }],
            match_state: None,
            round_state: None,
            pending_timeout: None,
            continue_action: Some(ContinueActionState {
                action_id: "start_next_round".to_string(),
                confirmed_seats: vec![0],
                required_seats: vec![0],
                online_seats: vec![0],
                auto_advance_deadline_at: Some("2026-04-07T10:10:00Z".to_string()),
            }),
        };

        let value = room.to_room_value().expect("room should serialize");
        assert_eq!(value["continue_action"]["action_id"], "start_next_round");
        assert_eq!(value["continue_action"]["confirmed_seats"], json!([0]));
        assert_eq!(
            value["continue_action"]["auto_advance_deadline_at"],
            "2026-04-07T10:10:00Z"
        );
        assert!(value.get("start_next_round_confirmed_seats").is_none());
    }
}
