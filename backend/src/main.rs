mod app;
mod bot;
mod core;
#[cfg(test)]
mod mahjong;
mod projection;
mod room_scoring;
mod rules;
mod scoring;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    app::server::run().await
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use anyhow::Result;
    use chrono::Utc;
    use serde_json::{Value, json};
    use tokio::sync::{Notify, mpsc};

    use crate::app::persistence::{DbWorker, in_memory_database};
    use crate::app::room_runtime::{
        RoomRuntime, close_room_handle, mark_restored_room_disconnected, replace_connection,
        restore_persisted_rooms, room_handle, room_has_only_bots,
    };
    use crate::app::{
        AppContext, ConnectionHandle, Settings, add_bot_to_waiting_room, initial_room_state,
        maybe_start_test_match, now_iso, occupied_seats, parse_datetime,
        remove_bot_from_waiting_room, room_has_round_state, seat_matches_reconnect_credentials,
        send_outbound,
    };
    use crate::core::state::{RoomState, SeatState};

    fn room_state(value: Value) -> RoomState {
        RoomState::from_room_value(&value).expect("room state should parse")
    }

    fn test_app_context(db: DbWorker) -> AppContext {
        AppContext::new(
            Settings {
                bind_addr: "127.0.0.1:0".to_string(),
                database_path: ":memory:".to_string(),
                default_test_mode: false,
                cors_origins: vec![],
                frontend_dir: None,
            },
            db,
        )
    }

    fn test_connection_handle(capacity: usize) -> (ConnectionHandle, mpsc::Receiver<String>) {
        let (sender, receiver) = mpsc::channel(capacity);
        (
            ConnectionHandle {
                id: 1,
                sender,
                close_requested: Arc::new(AtomicBool::new(false)),
                close_notify: Arc::new(Notify::new()),
            },
            receiver,
        )
    }

    #[test]
    fn maybe_start_test_match_starts_when_round_state_is_null() {
        let mut room = room_state(json!({
            "table_code": "ROOM42",
            "phase": "waiting",
            "mode": "test",
            "test_mode": true,
            "enforce_minimum_eight_fan": true,
            "continue_action": null,
            "seats": [{
            "seat_index": 0,
            "nickname": "Solo",
            "reconnect_token": "token-1",
            "player_session_id": 1,
            "connected": true,
            "ready": false,
            "is_bot": false,
            "seat_type": "human",
            "bot_persona": Value::Null,
            "bot_aggression": Value::Null,
            "disconnect_deadline_at": Value::Null,
        }],
            "match_state": null,
            "round_state": null,
            "pending_timeout": null
        }));

        maybe_start_test_match(&mut room);

        assert_eq!(room.phase, "playing");
        assert_eq!(room.mode, "test");
        assert_eq!(room.seats.len(), 4);
        assert!(room_has_round_state(&room));
        assert_eq!(
            room.match_state.as_ref().map(|state| state.dealer_seat),
            Some(0)
        );
    }

    #[test]
    fn maybe_start_test_match_keeps_human_seat_and_future_timeout() {
        let mut room = room_state(json!({
            "table_code": "ROOM43",
            "phase": "waiting",
            "mode": "test",
            "test_mode": true,
            "enforce_minimum_eight_fan": true,
            "continue_action": null,
            "seats": [{
                "seat_index": 0,
                "nickname": "Solo",
                "reconnect_token": "token-1",
                "player_session_id": 1,
                "connected": true,
                "ready": false,
                "is_bot": false,
                "seat_type": "human",
                "bot_persona": Value::Null,
                "bot_aggression": Value::Null,
                "disconnect_deadline_at": Value::Null
            }],
            "match_state": null,
            "round_state": null,
            "pending_timeout": null
        }));

        maybe_start_test_match(&mut room);

        assert!(
            room.seats
                .iter()
                .find(|seat| seat.seat_index == 0)
                .is_some_and(|seat| !seat.is_bot)
        );
        let deadline = room
            .pending_timeout
            .as_ref()
            .and_then(|timeout| timeout.deadline_at.as_deref())
            .and_then(parse_datetime)
            .expect("test match should schedule a timeout");
        assert!(deadline > Utc::now());
    }

    #[test]
    fn send_outbound_requests_close_when_channel_is_full() {
        let (handle, _receiver) = test_connection_handle(1);

        send_outbound(vec![
            handle.outbound(json!({ "type": "first" })),
            handle.outbound(json!({ "type": "second" })),
        ]);

        assert!(handle.should_close());
    }

    #[test]
    fn replace_connection_closes_previous_socket() {
        let (previous, _receiver) = test_connection_handle(1);
        let replacement = ConnectionHandle {
            id: 2,
            sender: previous.sender.clone(),
            close_requested: Arc::new(AtomicBool::new(false)),
            close_notify: Arc::new(Notify::new()),
        };
        let mut runtime = RoomRuntime::new(now_iso(), initial_room_state("ROOM42", "normal", true));
        runtime.connections = HashMap::from([(0, previous.clone())]);

        replace_connection(&mut runtime, 0, &replacement);

        assert!(previous.should_close());
        assert_eq!(runtime.connections.get(&0).map(|handle| handle.id), Some(2));
    }

    #[test]
    fn restored_human_seats_receive_disconnect_deadline() {
        let mut room = room_state(json!({
            "table_code": "ROOM42",
            "phase": "waiting",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "continue_action": null,
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
                "bot_persona": Value::Null,
                "bot_aggression": Value::Null,
                "disconnect_deadline_at": Value::Null
            },
            {
                "seat_index": 1,
                "nickname": "Bot 1",
                "reconnect_token": Value::Null,
                "player_session_id": -2,
                "connected": true,
                "ready": true,
                "is_bot": true,
                "seat_type": "bot",
                "bot_persona": Value::Null,
                "bot_aggression": Value::Null,
                "disconnect_deadline_at": Value::Null
            }
        ],
            "match_state": null,
            "round_state": null,
            "pending_timeout": null
        }));

        mark_restored_room_disconnected(&mut room);

        let human_deadline = room.seats[0]
            .disconnect_deadline_at
            .as_deref()
            .and_then(parse_datetime);
        assert!(human_deadline.is_some());
        assert!(!room.seats[0].connected);
        assert!(room.seats[1].disconnect_deadline_at.is_none());
    }

    #[test]
    fn add_bot_to_waiting_room_fills_first_empty_seat_and_marks_ready() {
        let mut room = room_state(json!({
            "table_code": "ROOM42",
            "phase": "waiting",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "continue_action": null,
            "seats": [
            {
                "seat_index": 0,
                "nickname": "Alice",
                "reconnect_token": "token-1",
                "player_session_id": 1,
                "connected": true,
                "ready": false,
                "is_bot": false,
                "seat_type": "human",
                "bot_persona": Value::Null,
                "bot_aggression": Value::Null,
                "disconnect_deadline_at": Value::Null
            },
            {
                "seat_index": 2,
                "nickname": "Carol",
                "reconnect_token": "token-2",
                "player_session_id": 2,
                "connected": true,
                "ready": true,
                "is_bot": false,
                "seat_type": "human",
                "bot_persona": Value::Null,
                "bot_aggression": Value::Null,
                "disconnect_deadline_at": Value::Null
            }
        ],
            "match_state": null,
            "round_state": null,
            "pending_timeout": null
        }));

        let inserted_seat = add_bot_to_waiting_room(&mut room).expect("bot seat should be added");

        assert_eq!(inserted_seat, 1);
        assert_eq!(room.seats.len(), 3);
        assert_eq!(room.seats[1].seat_index, 1);
        assert_eq!(room.seats[1].nickname.as_deref(), Some("Bot 1"));
        assert!(room.seats[1].ready);
        assert!(room.seats[1].connected);
        assert!(room.seats[1].is_bot);
    }

    #[test]
    fn remove_bot_from_waiting_room_removes_highest_index_bot() {
        let mut room = room_state(json!({
            "table_code": "ROOM42",
            "phase": "waiting",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "continue_action": null,
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
                "bot_persona": Value::Null,
                "bot_aggression": Value::Null,
                "disconnect_deadline_at": Value::Null
            },
            {
                "seat_index": 1,
                "nickname": "Bot 1",
                "reconnect_token": Value::Null,
                "player_session_id": -2,
                "connected": true,
                "ready": true,
                "is_bot": true,
                "seat_type": "bot",
                "bot_persona": Value::Null,
                "bot_aggression": Value::Null,
                "disconnect_deadline_at": Value::Null
            },
            {
                "seat_index": 3,
                "nickname": "Bot 3",
                "reconnect_token": Value::Null,
                "player_session_id": -4,
                "connected": true,
                "ready": true,
                "is_bot": true,
                "seat_type": "bot",
                "bot_persona": Value::Null,
                "bot_aggression": Value::Null,
                "disconnect_deadline_at": Value::Null
            }
        ],
            "match_state": null,
            "round_state": null,
            "pending_timeout": null
        }));

        let removed_seat =
            remove_bot_from_waiting_room(&mut room).expect("bot seat should be removed");

        assert_eq!(removed_seat, 3);
        assert_eq!(room.seats.len(), 2);
        assert_eq!(occupied_seats(&room), HashSet::from([0, 1]));
    }

    #[test]
    fn room_has_only_bots_requires_non_empty_bot_only_room() {
        let mut empty_room = initial_room_state("ROOM42", "normal", true);
        assert!(!room_has_only_bots(&empty_room));

        empty_room = room_state(json!({
            "table_code": "ROOM42",
            "phase": "waiting",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "continue_action": null,
            "seats": [{
            "seat_index": 0,
            "nickname": "Bot 1",
            "reconnect_token": Value::Null,
            "player_session_id": -1,
            "connected": true,
            "ready": true,
            "is_bot": true,
            "seat_type": "bot",
            "bot_persona": Value::Null,
            "bot_aggression": Value::Null,
            "disconnect_deadline_at": Value::Null
        }],
            "match_state": null,
            "round_state": null,
            "pending_timeout": null
        }));
        assert!(room_has_only_bots(&empty_room));

        empty_room = room_state(json!({
            "table_code": "ROOM42",
            "phase": "waiting",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "continue_action": null,
            "seats": [
            {
                "seat_index": 0,
                "nickname": "Bot 1",
                "reconnect_token": Value::Null,
                "player_session_id": -1,
                "connected": true,
                "ready": true,
                "is_bot": true,
                "seat_type": "bot",
                "bot_persona": Value::Null,
                "bot_aggression": Value::Null,
                "disconnect_deadline_at": Value::Null
            },
            {
                "seat_index": 1,
                "nickname": "Alice",
                "reconnect_token": "token-1",
                "player_session_id": 1,
                "connected": false,
                "ready": true,
                "is_bot": false,
                "seat_type": "human",
                "bot_persona": Value::Null,
                "bot_aggression": Value::Null,
                "disconnect_deadline_at": Value::Null
            }
        ],
            "match_state": null,
            "round_state": null,
            "pending_timeout": null
        }));
        assert!(!room_has_only_bots(&empty_room));
    }

    #[test]
    fn seat_matches_reconnect_credentials_requires_current_room_token() {
        let room = room_state(json!({
            "table_code": "ROOM42",
            "phase": "waiting",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "continue_action": null,
            "seats": [{
            "seat_index": 0,
            "nickname": "Alice",
            "reconnect_token": "token-new",
            "player_session_id": 42,
            "connected": false,
            "ready": true,
            "is_bot": false,
            "seat_type": "human",
            "bot_persona": Value::Null,
            "bot_aggression": Value::Null,
            "disconnect_deadline_at": Value::Null,
        }],
            "match_state": null,
            "round_state": null,
            "pending_timeout": null
        }));

        assert!(seat_matches_reconnect_credentials(
            &room,
            0,
            42,
            "token-new"
        ));
        assert!(!seat_matches_reconnect_credentials(
            &room,
            0,
            42,
            "token-old"
        ));
        assert!(!seat_matches_reconnect_credentials(
            &room,
            0,
            7,
            "token-new"
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn restore_persisted_rooms_rehydrates_disconnect_tasks() -> Result<()> {
        let db = in_memory_database("")?;
        db.initialize()?;
        let worker = DbWorker::start(db)?;
        let state = test_app_context(worker.clone());

        let mut room = initial_room_state("ROOM42", "normal", true);
        room.seats.push(SeatState {
            seat_index: 0,
            nickname: Some("Alice".to_string()),
            reconnect_token: Some("token-1".to_string()),
            player_session_id: Some(42),
            connected: true,
            ready: true,
            is_bot: false,
            seat_type: "human".to_string(),
            bot_persona: None,
            bot_aggression: None,
            disconnect_deadline_at: None,
            skill_loadout: Default::default(),
        });
        let room_json = crate::app::serialize_room_state(&room)?;
        worker
            .save_table("ROOM42", "2026-04-07T00:00:00Z", &room_json)
            .await?;

        restore_persisted_rooms(&state).await;

        let room_handle = room_handle(&state, "ROOM42")
            .await
            .expect("restored room should be loaded");
        let runtime = room_handle.runtime.lock().await;
        assert!(!runtime.room.seats[0].connected);
        assert!(
            runtime.room.seats[0]
                .disconnect_deadline_at
                .as_deref()
                .and_then(parse_datetime)
                .is_some()
        );
        assert!(runtime.disconnect_task.is_some());
        drop(runtime);
        close_room_handle(&room_handle).await;
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn restore_persisted_rooms_deletes_all_bot_rooms() -> Result<()> {
        let db = in_memory_database("")?;
        db.initialize()?;
        let worker = DbWorker::start(db)?;
        let state = test_app_context(worker.clone());

        let mut room = initial_room_state("ROOMBOT", "normal", true);
        room.seats.push(SeatState {
            seat_index: 0,
            nickname: Some("Bot 1".to_string()),
            reconnect_token: None,
            player_session_id: Some(-1),
            connected: true,
            ready: true,
            is_bot: true,
            seat_type: "bot".to_string(),
            bot_persona: None,
            bot_aggression: None,
            disconnect_deadline_at: None,
            skill_loadout: Default::default(),
        });
        let room_json = crate::app::serialize_room_state(&room)?;
        worker
            .save_table("ROOMBOT", "2026-04-07T00:00:00Z", &room_json)
            .await?;

        restore_persisted_rooms(&state).await;

        assert!(room_handle(&state, "ROOMBOT").await.is_none());
        assert!(worker.get_table("ROOMBOT").await?.is_none());
        Ok(())
    }
}
