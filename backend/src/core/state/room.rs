use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::error::EngineError;
use crate::core::ids::TableCode;
use crate::core::state::pending::ContinueActionState;

use super::{MatchState, PendingTimeout, RoundState, SeatState, array, bool_or};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
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
        serde_json::from_value(value.clone()).map_err(Into::into)
    }

    pub fn from_room_str(raw: &str) -> Result<Self, EngineError> {
        let value: Value = serde_json::from_str(raw)?;
        Self::from_room_value(&value)
    }

    pub fn to_room_value(&self) -> Result<Value, serde_json::Error> {
        serde_json::to_value(self)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::RoomState;
    use crate::core::state::{ContinueActionState, SeatState};

    #[test]
    fn parses_waiting_room_shape() {
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
            "continue_action": null
        });

        let parsed = RoomState::from_room_value(&room).expect("room should parse");
        assert_eq!(parsed.table_code, "ABCD");
        assert_eq!(parsed.phase, "waiting");
        assert_eq!(parsed.seats.len(), 1);
        assert!(parsed.round_state.is_none());
        assert!(parsed.continue_action.is_none());
    }

    #[test]
    fn parses_playing_room_shape() {
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
            "continue_action": null
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
    fn parses_settlement_continue_action_shape() {
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
            "continue_action": {
                "action_id": "start_next_round",
                "confirmed_seats": [0],
                "required_seats": [0],
                "online_seats": [0],
                "auto_advance_deadline_at": "2026-04-07T10:10:00Z"
            }
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
                skill_loadout: Default::default(),
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
