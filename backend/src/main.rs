#![cfg_attr(test, allow(dead_code, unused_imports))]

#[cfg(test)]
mod app;
#[cfg(test)]
mod bot;
#[cfg(test)]
mod core;
#[cfg(test)]
mod mahjong;
#[cfg(test)]
mod projection;
#[cfg(test)]
mod room_scoring;
#[cfg(test)]
mod rules;
#[cfg(test)]
mod scoring;
#[cfg(test)]
mod special_bots;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    backend::run_from_env().await
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use anyhow::Result;
    use rand::{SeedableRng, rngs::StdRng};
    use serde_json::{Value, json};
    use tokio::sync::{Notify, mpsc};

    use crate::app::persistence::{DbWorker, in_memory_database};
    use crate::app::room_runtime::{
        RoomRuntime, add_seat_connection, close_room_handle, mark_restored_room_disconnected,
        restore_persisted_rooms, room_handle, room_has_only_bots,
    };
    use crate::app::{
        AppContext, ConnectionHandle, add_bot_to_waiting_room, initial_room_state, now_iso,
        occupied_seats, random_open_seat_index_with_rng, remove_bot_from_waiting_room,
        send_outbound,
    };
    use crate::core::state::{RoomState, SeatState};

    fn room_state(value: Value) -> RoomState {
        RoomState::from_room_value(&value).expect("room state should parse")
    }

    fn test_app_context(db: DbWorker) -> AppContext {
        AppContext::new(db)
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
    fn send_outbound_requests_close_when_channel_is_full() {
        let (handle, _receiver) = test_connection_handle(1);

        send_outbound(vec![
            handle.outbound(json!({ "type": "first" })),
            handle.outbound(json!({ "type": "second" })),
        ]);

        assert!(handle.should_close());
    }

    #[test]
    fn add_seat_connection_keeps_existing_socket_for_same_seat() {
        let (previous, _receiver) = test_connection_handle(1);
        let replacement = ConnectionHandle {
            id: 2,
            sender: previous.sender.clone(),
            close_requested: Arc::new(AtomicBool::new(false)),
            close_notify: Arc::new(Notify::new()),
        };
        let mut runtime = RoomRuntime::new(now_iso(), initial_room_state("ROOM42"));
        add_seat_connection(&mut runtime, 0, Some(11), &previous);
        add_seat_connection(&mut runtime, 0, Some(11), &replacement);

        assert!(!previous.should_close());
        assert_eq!(
            runtime
                .connections
                .get(&0)
                .map(|group| group.connections.len()),
            Some(2)
        );
    }

    #[test]
    fn restored_human_seats_are_marked_disconnected_without_takeover_deadline() {
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
                "connected": true,
                "is_bot": false,
                "seat_type": "human",
                "bot_persona": Value::Null,
                "bot_aggression": Value::Null,
                "disconnect_deadline_at": Value::Null
            },
            {
                "seat_index": 1,
                "nickname": "舒伯特",
                "connected": true,
                "is_bot": true,
                "seat_type": "special_bot",
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

        assert!(!room.seats[0].connected);
        assert!(room.seats[0].disconnect_deadline_at.is_none());
        assert!(room.seats[1].connected);
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
                "connected": true,
                "is_bot": false,
                "seat_type": "human",
                "bot_persona": Value::Null,
                "bot_aggression": Value::Null,
                "disconnect_deadline_at": Value::Null
            },
            {
                "seat_index": 2,
                "nickname": "Carol",
                "connected": true,
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
        assert_eq!(room.seats[1].nickname.as_deref(), Some("bot_1"));
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
                "connected": true,
                "is_bot": false,
                "seat_type": "human",
                "bot_persona": Value::Null,
                "bot_aggression": Value::Null,
                "disconnect_deadline_at": Value::Null
            },
            {
                "seat_index": 1,
                "nickname": "Bot 1",
                "connected": true,
                "is_bot": true,
                "seat_type": "bot",
                "bot_persona": Value::Null,
                "bot_aggression": Value::Null,
                "disconnect_deadline_at": Value::Null
            },
            {
                "seat_index": 3,
                "nickname": "Bot 3",
                "connected": true,
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
        let mut empty_room = initial_room_state("ROOM42");
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
            "connected": true,
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
            "seats": [{
            "seat_index": 0,
            "nickname": "舒伯特",
            "connected": true,
            "is_bot": true,
            "seat_type": "special_bot",
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
                "connected": true,
                "is_bot": true,
                "seat_type": "bot",
                "bot_persona": Value::Null,
                "bot_aggression": Value::Null,
                "disconnect_deadline_at": Value::Null
            },
            {
                "seat_index": 1,
                "nickname": "Alice",
                "connected": false,
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
    fn random_open_seat_index_can_pick_different_open_seats() {
        let room = room_state(json!({
            "table_code": "ROOM42",
            "phase": "waiting",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "continue_action": null,
            "seats": [
                {
                    "seat_index": 1,
                    "nickname": "Alice",
                    "connected": true,
                    "is_bot": false,
                    "seat_type": "human",
                    "bot_persona": Value::Null,
                    "bot_aggression": Value::Null,
                    "disconnect_deadline_at": Value::Null
                },
                {
                    "seat_index": 3,
                    "nickname": "Bob",
                    "connected": true,
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

        let mut rng = StdRng::seed_from_u64(7);
        let picks: HashSet<_> = (0..16)
            .map(|_| {
                random_open_seat_index_with_rng(&room, &mut rng)
                    .expect("room should still have open seats")
            })
            .collect();

        assert_eq!(picks, HashSet::from([0, 2]));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn restore_persisted_rooms_marks_humans_disconnected_without_takeover_task() -> Result<()>
    {
        let db = in_memory_database("")?;
        db.initialize()?;
        let worker = DbWorker::start(db)?;
        let state = test_app_context(worker.clone());

        let mut room = initial_room_state("ROOM42");
        room.seats.push(SeatState {
            seat_index: 0,
            user_id: None,
            nickname: Some("Alice".to_string()),
            points: None,
            title: None,
            connected: true,
            is_bot: false,
            seat_type: "human".to_string(),
            bot_persona: None,
            bot_aggression: None,
            disconnect_deadline_at: None,
            consecutive_timeout_auto_response_count: 0,
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
        assert!(runtime.room.seats[0].disconnect_deadline_at.is_none());
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

        let mut room = initial_room_state("ROOMBOT");
        room.seats.push(SeatState {
            seat_index: 0,
            user_id: None,
            nickname: Some("Bot 1".to_string()),
            points: None,
            title: None,
            connected: true,
            is_bot: true,
            seat_type: "bot".to_string(),
            bot_persona: None,
            bot_aggression: None,
            disconnect_deadline_at: None,
            consecutive_timeout_auto_response_count: 0,
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
