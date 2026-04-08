use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Error;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxumPath, State};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{Notify, mpsc};

use super::room_runtime::{
    close_runtime, ensure_room_loaded, replace_connection, restore_room_snapshot, room_handle,
    room_has_only_bots, should_terminate_unattended, snapshot_connections, unregister_room_handle,
};
use super::scheduler::schedule_room_tasks_detached;
use super::{
    AppContext, ConnectionHandle, MAX_SEATS, OUTBOUND_CHANNEL_CAPACITY, OutboundMessage,
    add_bot_to_waiting_room, collect_join_outbound_from_snapshot,
    collect_snapshot_and_prompt_outbound_from_snapshot, convert_seat_to_bot,
    generate_player_session_id, generate_reconnect_token, generate_short_hex,
    maybe_start_test_match, normalize_table_code, occupied_seats,
    presence_and_snapshot_for_all_from_snapshot, remove_bot_from_waiting_room,
    remove_seat_from_room, room_has_round_state, room_phase, room_player_session_id, room_seats,
    seat_exists, seat_matches_reconnect_credentials, send_outbound, serialize_room,
    set_seat_connected,
};
use crate::mahjong::{
    reconcile_continue_action_state as rust_reconcile_continue_action_state,
    record_continue_action as rust_record_continue_action,
    room_ready_to_start as rust_room_ready_to_start, start_match as rust_start_match,
    try_handle_action as try_rust_action,
};

#[derive(Debug, Deserialize)]
pub(crate) struct ClientEnvelope {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    payload: Value,
}

pub(crate) struct MessageOutcome {
    pub(crate) outbound: Vec<OutboundMessage>,
    pub(crate) owned_seat: Option<usize>,
    pub(crate) clear_owned_seat: bool,
    pub(crate) close_socket: bool,
}

pub(crate) async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppContext>,
    AxumPath(table_code): AxumPath<String>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| websocket_session(state, socket, normalize_table_code(&table_code)))
}

async fn websocket_session(state: AppContext, socket: WebSocket, table_code: String) {
    let connection_id = state.next_connection_id.fetch_add(1, Ordering::Relaxed);
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<String>(OUTBOUND_CHANNEL_CAPACITY);
    let close_requested = Arc::new(AtomicBool::new(false));
    let close_notify = Arc::new(Notify::new());
    let handle = ConnectionHandle {
        id: connection_id,
        sender: outgoing_tx.clone(),
        close_requested: close_requested.clone(),
        close_notify: close_notify.clone(),
    };

    let writer_close_requested = close_requested.clone();
    let writer_close_notify = close_notify.clone();
    let writer = tokio::spawn(async move {
        loop {
            tokio::select! {
                maybe_message = outgoing_rx.recv() => {
                    let Some(message) = maybe_message else {
                        break;
                    };
                    if ws_sender.send(Message::Text(message.into())).await.is_err() {
                        break;
                    }
                }
                _ = writer_close_notify.notified() => {
                    if writer_close_requested.load(Ordering::Relaxed) {
                        break;
                    }
                }
            }
        }
        let _ = ws_sender.close().await;
    });

    let mut owned_seat: Option<usize> = None;
    let mut close_socket = false;

    loop {
        if handle.should_close() {
            break;
        }
        let next = tokio::select! {
            next = ws_receiver.next() => next,
            _ = close_notify.notified() => {
                if handle.should_close() {
                    break;
                }
                continue;
            }
        };
        let Some(next) = next else {
            break;
        };
        let Ok(message) = next else {
            break;
        };
        let Message::Text(text) = message else {
            continue;
        };
        let envelope: ClientEnvelope = match serde_json::from_str(text.as_str()) {
            Ok(value) => value,
            Err(_) => {
                send_outbound(vec![handle.outbound(json!({
                    "type": "action_rejected",
                    "payload": { "reason": "unsupported_message" }
                }))]);
                continue;
            }
        };

        let outcome =
            handle_client_message(state.clone(), &table_code, &handle, owned_seat, &envelope).await;

        if let Some(new_seat) = outcome.owned_seat {
            owned_seat = Some(new_seat);
        }
        if outcome.clear_owned_seat {
            owned_seat = None;
        }
        send_outbound(outcome.outbound);
        if handle.should_close() {
            break;
        }
        if outcome.close_socket {
            close_socket = true;
            break;
        }
    }

    if !close_socket {
        handle_disconnect(state, &table_code, owned_seat, connection_id).await;
    }
    handle.request_close();
    drop(outgoing_tx);
    let _ = writer.await;
}

async fn handle_client_message(
    state: AppContext,
    table_code: &str,
    connection: &ConnectionHandle,
    owned_seat: Option<usize>,
    envelope: &ClientEnvelope,
) -> MessageOutcome {
    match envelope.kind.as_str() {
        "join_table" => {
            if owned_seat.is_some() {
                return reject_to(connection, "seat_already_owned");
            }
            handle_join_table(state, table_code, connection, envelope).await
        }
        "reconnect" => {
            if owned_seat.is_some() {
                return reject_to(connection, "seat_already_owned");
            }
            handle_reconnect(state, table_code, connection, envelope).await
        }
        "ready" => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, owned_seat).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_ready(state, table_code, connection, seat_index, envelope).await
        }
        "adjust_bots" => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, owned_seat).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_adjust_bots(state, table_code, connection, seat_index, envelope).await
        }
        "start_match" => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, owned_seat).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_start_match(state, table_code, connection, seat_index).await
        }
        "start_next_round" => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, owned_seat).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_continue_action(
                state,
                table_code,
                connection,
                seat_index,
                "start_next_round",
            )
            .await
        }
        "restart_match" => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, owned_seat).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_continue_action(state, table_code, connection, seat_index, "restart_match").await
        }
        "leave_table" => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, owned_seat).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_leave_table(state, table_code, connection, seat_index).await
        }
        "action_request" => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, owned_seat).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_action_request(state, table_code, connection, seat_index, envelope).await
        }
        "quick_chat" => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, owned_seat).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_quick_chat(state, table_code, connection, seat_index, envelope).await
        }
        "heartbeat" => MessageOutcome {
            outbound: vec![connection.outbound(json!({
                "type": "heartbeat",
                "payload": envelope.payload.clone(),
            }))],
            owned_seat: None,
            clear_owned_seat: false,
            close_socket: false,
        },
        _ => reject_to(connection, "unsupported_message"),
    }
}

async fn assert_active_owned_seat(
    state: &AppContext,
    table_code: &str,
    connection: &ConnectionHandle,
    owned_seat: Option<usize>,
) -> Option<usize> {
    let seat_index = owned_seat?;
    let room_handle = room_handle(state, table_code).await?;
    if room_handle.is_closed() {
        return None;
    }
    let runtime = room_handle.runtime.lock().await;
    let current = runtime.connections.get(&seat_index)?;
    if current.id == connection.id {
        Some(seat_index)
    } else {
        None
    }
}

async fn handle_join_table(
    state: AppContext,
    table_code: &str,
    connection: &ConnectionHandle,
    envelope: &ClientEnvelope,
) -> MessageOutcome {
    let nickname = envelope
        .payload
        .get("nickname")
        .and_then(Value::as_str)
        .unwrap_or("Player")
        .to_string();

    let Some(room_handle) = ensure_room_loaded(&state, table_code).await.ok().flatten() else {
        return reject_to(connection, "table_not_found");
    };
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }
    let _persist_guard = room_handle.persist.lock().await;
    let mut runtime = room_handle.runtime.lock().await;
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }
    let previous_room = runtime.room.clone();
    if runtime
        .connections
        .values()
        .any(|handle| handle.id == connection.id)
    {
        return reject_to(connection, "seat_already_owned");
    }
    let occupied = occupied_seats(&runtime.room);
    let Some(seat_index) = (0..MAX_SEATS).find(|seat| !occupied.contains(seat)) else {
        return reject_to(connection, "table_full");
    };

    let player_session_id = generate_player_session_id();
    let reconnect_token = generate_reconnect_token();
    {
        let seats = runtime
            .room
            .get_mut("seats")
            .and_then(Value::as_array_mut)
            .expect("room seats should exist");
        seats.push(json!({
            "seat_index": seat_index,
            "nickname": nickname,
            "reconnect_token": reconnect_token,
            "player_session_id": player_session_id,
            "connected": true,
            "ready": false,
            "is_bot": false,
            "seat_type": "human",
            "bot_persona": Value::Null,
            "bot_aggression": Value::Null,
            "disconnect_deadline_at": Value::Null,
        }));
        seats.sort_by_key(|seat| seat.get("seat_index").and_then(Value::as_u64).unwrap_or(99));
    }
    maybe_start_test_match(&mut runtime.room);
    let created_at = runtime.created_at.clone();
    let room = runtime.room.clone();
    let connections = snapshot_connections(&runtime);
    drop(runtime);
    let room_json = match serialize_room(&room) {
        Ok(value) => value,
        Err(error) => return internal_error_to(connection, error),
    };
    if let Err(error) = state
        .inner
        .db
        .save_table_and_store_reconnect_token(
            table_code,
            &created_at,
            &room_json,
            &reconnect_token,
            seat_index,
            player_session_id,
        )
        .await
    {
        restore_room_snapshot(&room_handle, previous_room).await;
        return internal_error_to(connection, error);
    }
    let outbound = collect_join_outbound_from_snapshot(
        &room,
        &connections,
        table_code,
        connection,
        seat_index,
        true,
    );
    let mut runtime = room_handle.runtime.lock().await;
    replace_connection(&mut runtime, seat_index, connection);
    drop(runtime);
    schedule_room_tasks_detached(state, table_code.to_string());
    MessageOutcome {
        outbound,
        owned_seat: Some(seat_index),
        clear_owned_seat: false,
        close_socket: false,
    }
}

async fn handle_reconnect(
    state: AppContext,
    table_code: &str,
    connection: &ConnectionHandle,
    envelope: &ClientEnvelope,
) -> MessageOutcome {
    let reconnect_token = envelope
        .payload
        .get("reconnect_token")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let token_record = match state.inner.db.get_reconnect_token(&reconnect_token).await {
        Ok(Some(token_record)) => token_record,
        Ok(None) | Err(_) => return reject_to(connection, "invalid_reconnect_token"),
    };
    if token_record.table_code != table_code {
        return reject_to(connection, "table_not_found");
    }

    let Some(room_handle) = ensure_room_loaded(&state, table_code).await.ok().flatten() else {
        return reject_to(connection, "table_not_found");
    };
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }
    let _persist_guard = room_handle.persist.lock().await;
    let mut runtime = room_handle.runtime.lock().await;
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }
    let previous_room = runtime.room.clone();
    if runtime
        .connections
        .values()
        .any(|handle| handle.id == connection.id)
    {
        return reject_to(connection, "seat_already_owned");
    }
    let Some(current_session_id) = room_player_session_id(&runtime.room, token_record.seat_index)
    else {
        return reject_to(connection, "invalid_reconnect_token");
    };
    if !seat_matches_reconnect_credentials(
        &runtime.room,
        token_record.seat_index,
        token_record.player_session_id,
        &reconnect_token,
    ) || current_session_id != token_record.player_session_id
    {
        return reject_to(connection, "invalid_reconnect_token");
    }

    let new_token = generate_reconnect_token();
    if let Some(seats) = runtime.room.get_mut("seats").and_then(Value::as_array_mut) {
        if let Some(seat) = seats.iter_mut().find(|seat| {
            seat.get("seat_index")
                .and_then(Value::as_u64)
                .map(|value| value as usize == token_record.seat_index)
                .unwrap_or(false)
        }) {
            if let Some(object) = seat.as_object_mut() {
                object.insert(
                    "reconnect_token".to_string(),
                    Value::String(new_token.clone()),
                );
                object.insert("connected".to_string(), Value::Bool(true));
                object.insert("disconnect_deadline_at".to_string(), Value::Null);
            }
        }
    }
    let _ = rust_reconcile_continue_action_state(&mut runtime.room);
    let created_at = runtime.created_at.clone();
    let room = runtime.room.clone();
    let connections = snapshot_connections(&runtime);
    drop(runtime);
    let room_json = match serialize_room(&room) {
        Ok(value) => value,
        Err(error) => return internal_error_to(connection, error),
    };
    if let Err(error) = state
        .inner
        .db
        .rotate_reconnect_token(
            table_code,
            &created_at,
            &room_json,
            &reconnect_token,
            &new_token,
            token_record.seat_index,
            token_record.player_session_id,
        )
        .await
    {
        restore_room_snapshot(&room_handle, previous_room).await;
        if error.to_string().contains("stale reconnect token") {
            return reject_to(connection, "invalid_reconnect_token");
        }
        return internal_error_to(connection, error);
    }

    let outbound = collect_join_outbound_from_snapshot(
        &room,
        &connections,
        table_code,
        connection,
        token_record.seat_index,
        true,
    );
    let mut runtime = room_handle.runtime.lock().await;
    replace_connection(&mut runtime, token_record.seat_index, connection);
    drop(runtime);
    schedule_room_tasks_detached(state, table_code.to_string());
    MessageOutcome {
        outbound,
        owned_seat: Some(token_record.seat_index),
        clear_owned_seat: false,
        close_socket: false,
    }
}

async fn handle_ready(
    state: AppContext,
    table_code: &str,
    connection: &ConnectionHandle,
    seat_index: usize,
    envelope: &ClientEnvelope,
) -> MessageOutcome {
    let ready = envelope
        .payload
        .get("ready")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let Some(room_handle) = ensure_room_loaded(&state, table_code).await.ok().flatten() else {
        return reject_to(connection, "table_not_found");
    };
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }
    let _persist_guard = room_handle.persist.lock().await;
    let mut runtime = room_handle.runtime.lock().await;
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }
    let previous_room = runtime.room.clone();
    if room_has_round_state(&runtime.room) {
        return reject_to(connection, "room_already_started");
    }
    if let Some(seats) = runtime.room.get_mut("seats").and_then(Value::as_array_mut) {
        if let Some(seat) = seats.iter_mut().find(|seat| {
            seat.get("seat_index")
                .and_then(Value::as_u64)
                .map(|value| value as usize == seat_index)
                .unwrap_or(false)
        }) {
            if let Some(object) = seat.as_object_mut() {
                object.insert("ready".to_string(), Value::Bool(ready));
            }
        }
    }
    let created_at = runtime.created_at.clone();
    let room = runtime.room.clone();
    let connections = snapshot_connections(&runtime);
    drop(runtime);
    let room_json = match serialize_room(&room) {
        Ok(value) => value,
        Err(error) => return internal_error_to(connection, error),
    };
    let outbound = collect_snapshot_and_prompt_outbound_from_snapshot(&room, &connections);
    if let Err(error) = state
        .inner
        .db
        .save_table(table_code, &created_at, &room_json)
        .await
    {
        restore_room_snapshot(&room_handle, previous_room).await;
        return internal_error_to(connection, error);
    }
    let outcome = MessageOutcome {
        outbound,
        owned_seat: None,
        clear_owned_seat: false,
        close_socket: false,
    };
    schedule_room_tasks_detached(state, table_code.to_string());
    outcome
}

async fn handle_adjust_bots(
    state: AppContext,
    table_code: &str,
    connection: &ConnectionHandle,
    seat_index: usize,
    envelope: &ClientEnvelope,
) -> MessageOutcome {
    let delta = envelope
        .payload
        .get("delta")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let Some(room_handle) = ensure_room_loaded(&state, table_code).await.ok().flatten() else {
        return reject_to(connection, "table_not_found");
    };
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }
    let _persist_guard = room_handle.persist.lock().await;
    let mut runtime = room_handle.runtime.lock().await;
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }
    let previous_room = runtime.room.clone();
    if !seat_exists(&runtime.room, seat_index) {
        return reject_to(connection, "seat_not_owned");
    }

    let update_result = match delta {
        1 => add_bot_to_waiting_room(&mut runtime.room).map(|_| ()),
        -1 => remove_bot_from_waiting_room(&mut runtime.room).map(|_| ()),
        _ => Err("invalid_bot_adjustment"),
    };
    if let Err(reason) = update_result {
        return reject_to(connection, reason);
    }

    let created_at = runtime.created_at.clone();
    let room = runtime.room.clone();
    let connections = snapshot_connections(&runtime);
    drop(runtime);
    let room_json = match serialize_room(&room) {
        Ok(value) => value,
        Err(error) => return internal_error_to(connection, error),
    };
    let outbound = collect_snapshot_and_prompt_outbound_from_snapshot(&room, &connections);
    if let Err(error) = state
        .inner
        .db
        .save_table(table_code, &created_at, &room_json)
        .await
    {
        restore_room_snapshot(&room_handle, previous_room).await;
        return internal_error_to(connection, error);
    }
    let outcome = MessageOutcome {
        outbound,
        owned_seat: None,
        clear_owned_seat: false,
        close_socket: false,
    };
    schedule_room_tasks_detached(state, table_code.to_string());
    outcome
}

fn reject_to(connection: &ConnectionHandle, reason: &str) -> MessageOutcome {
    MessageOutcome {
        outbound: vec![connection.outbound(json!({
            "type": "action_rejected",
            "payload": { "reason": reason }
        }))],
        owned_seat: None,
        clear_owned_seat: false,
        close_socket: false,
    }
}

fn internal_error_to(connection: &ConnectionHandle, error: Error) -> MessageOutcome {
    reject_to(connection, &format!("internal_error:{error}"))
}

async fn handle_start_match(
    state: AppContext,
    table_code: &str,
    connection: &ConnectionHandle,
    _seat_index: usize,
) -> MessageOutcome {
    let Some(room_handle) = ensure_room_loaded(&state, table_code).await.ok().flatten() else {
        return reject_to(connection, "table_not_found");
    };
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }
    let _persist_guard = room_handle.persist.lock().await;
    let mut runtime = room_handle.runtime.lock().await;
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }
    let previous_room = runtime.room.clone();
    let already_started = room_has_round_state(&runtime.room);
    let ready_to_start = rust_room_ready_to_start(&runtime.room);
    let occupied = occupied_seats(&runtime.room);
    if already_started {
        return reject_to(connection, "room_already_started");
    }
    if !ready_to_start {
        return reject_to(connection, "room_not_ready");
    }
    let dealer_seat = {
        let occupied: Vec<usize> = occupied.into_iter().collect();
        let mut rng = rand::rng();
        occupied[rng.random_range(0..occupied.len())]
    };
    rust_start_match(&mut runtime.room, dealer_seat, rand::random::<u64>());
    let created_at = runtime.created_at.clone();
    let room = runtime.room.clone();
    let connections = snapshot_connections(&runtime);
    drop(runtime);
    let room_json = match serialize_room(&room) {
        Ok(value) => value,
        Err(error) => return internal_error_to(connection, error),
    };
    let outbound = collect_snapshot_and_prompt_outbound_from_snapshot(&room, &connections);
    if let Err(error) = state
        .inner
        .db
        .save_table(table_code, &created_at, &room_json)
        .await
    {
        restore_room_snapshot(&room_handle, previous_room).await;
        return internal_error_to(connection, error);
    }
    let outcome = MessageOutcome {
        outbound,
        owned_seat: None,
        clear_owned_seat: false,
        close_socket: false,
    };
    schedule_room_tasks_detached(state, table_code.to_string());
    outcome
}

async fn handle_continue_action(
    state: AppContext,
    table_code: &str,
    connection: &ConnectionHandle,
    seat_index: usize,
    action_id: &str,
) -> MessageOutcome {
    let Some(room_handle) = ensure_room_loaded(&state, table_code).await.ok().flatten() else {
        return reject_to(connection, "table_not_found");
    };
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }
    let _persist_guard = room_handle.persist.lock().await;
    let mut runtime = room_handle.runtime.lock().await;
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }
    let previous_room = runtime.room.clone();
    if let Err(reason) = rust_record_continue_action(&mut runtime.room, seat_index, action_id) {
        return reject_to(connection, &reason);
    }
    let created_at = runtime.created_at.clone();
    let room = runtime.room.clone();
    let connections = snapshot_connections(&runtime);
    drop(runtime);
    let room_json = match serialize_room(&room) {
        Ok(value) => value,
        Err(error) => return internal_error_to(connection, error),
    };
    let outbound = collect_snapshot_and_prompt_outbound_from_snapshot(&room, &connections);
    if let Err(error) = state
        .inner
        .db
        .save_table(table_code, &created_at, &room_json)
        .await
    {
        restore_room_snapshot(&room_handle, previous_room).await;
        return internal_error_to(connection, error);
    }
    let outcome = MessageOutcome {
        outbound,
        owned_seat: None,
        clear_owned_seat: false,
        close_socket: false,
    };
    schedule_room_tasks_detached(state, table_code.to_string());
    outcome
}

async fn handle_action_request(
    state: AppContext,
    table_code: &str,
    connection: &ConnectionHandle,
    seat_index: usize,
    envelope: &ClientEnvelope,
) -> MessageOutcome {
    let tile_ids = envelope
        .payload
        .get("tile_ids")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let tile_id_strings = tile_ids
        .iter()
        .filter_map(Value::as_str)
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let action_type = envelope
        .payload
        .get("action_type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let Some(room_handle) = ensure_room_loaded(&state, table_code).await.ok().flatten() else {
        return reject_to(connection, "round_not_ready");
    };
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }
    let _persist_guard = room_handle.persist.lock().await;
    let mut runtime = room_handle.runtime.lock().await;
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }
    let previous_room = runtime.room.clone();
    let rust_handled_messages = match try_rust_action(
        &mut runtime.room,
        seat_index,
        &action_type,
        &tile_id_strings,
    ) {
        Some(Ok(messages)) => messages,
        Some(Err(reason)) => return reject_to(connection, &reason),
        None => return reject_to(connection, "invalid_action"),
    };

    let created_at = runtime.created_at.clone();
    let room_json = match serialize_room(&runtime.room) {
        Ok(value) => value,
        Err(error) => return internal_error_to(connection, error),
    };
    drop(runtime);
    if let Err(error) = state
        .inner
        .db
        .save_table(table_code, &created_at, &room_json)
        .await
    {
        restore_room_snapshot(&room_handle, previous_room).await;
        return internal_error_to(connection, error);
    }
    let runtime = room_handle.runtime.lock().await;
    let connections = snapshot_connections(&runtime);
    let broadcast_handles = connections
        .iter()
        .map(|(_, handle)| handle.clone())
        .collect::<Vec<_>>();
    let snapshot_outbound =
        collect_snapshot_and_prompt_outbound_from_snapshot(&runtime.room, &connections);
    drop(runtime);
    let mut outbound =
        super::broadcast_to_handles(&broadcast_handles, Some(&rust_handled_messages));
    outbound.extend(snapshot_outbound);
    schedule_room_tasks_detached(state, table_code.to_string());
    MessageOutcome {
        outbound,
        owned_seat: None,
        clear_owned_seat: false,
        close_socket: false,
    }
}

async fn handle_quick_chat(
    state: AppContext,
    table_code: &str,
    connection: &ConnectionHandle,
    seat_index: usize,
    envelope: &ClientEnvelope,
) -> MessageOutcome {
    let target_seat = envelope
        .payload
        .get("target_seat")
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    let emoji = envelope
        .payload
        .get("emoji")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if emoji.is_empty() {
        return reject_to(connection, "invalid_action");
    }

    let Some(room_handle) = room_handle(&state, table_code).await else {
        return reject_to(connection, "table_not_found");
    };
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }
    let runtime = room_handle.runtime.lock().await;
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }
    let Some(target_seat) = target_seat else {
        return reject_to(connection, "invalid_action");
    };
    if !occupied_seats(&runtime.room).contains(&target_seat) {
        return reject_to(connection, "invalid_action");
    }

    let payload = json!({
        "type": "quick_chat",
        "payload": {
            "message_id": generate_short_hex(8),
            "actor_seat": seat_index,
            "target_seat": target_seat,
            "emoji": emoji,
            "sent_at": super::now_iso(),
        }
    });
    let connections = snapshot_connections(&runtime);
    drop(runtime);
    let outbound = connections
        .into_iter()
        .map(|(_, handle)| handle.outbound(payload.clone()))
        .collect();
    MessageOutcome {
        outbound,
        owned_seat: None,
        clear_owned_seat: false,
        close_socket: false,
    }
}

async fn handle_leave_table(
    state: AppContext,
    table_code: &str,
    connection: &ConnectionHandle,
    seat_index: usize,
) -> MessageOutcome {
    let Some(room_handle) = ensure_room_loaded(&state, table_code).await.ok().flatten() else {
        return reject_to(connection, "table_not_found");
    };
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }
    let _persist_guard = room_handle.persist.lock().await;
    let mut runtime = room_handle.runtime.lock().await;
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }
    let previous_room = runtime.room.clone();
    let created_at = runtime.created_at.clone();
    let phase = room_phase(&runtime.room);
    if phase == "waiting" {
        remove_seat_from_room(&mut runtime.room, seat_index);
    } else {
        convert_seat_to_bot(&mut runtime.room, seat_index);
        let _ = rust_reconcile_continue_action_state(&mut runtime.room);
    }

    let mut outbound = vec![connection.outbound(json!({
        "type": "leave_table_accepted",
        "payload": {
            "table_code": table_code,
            "seat_index": seat_index,
        }
    }))];

    if phase == "waiting" {
        if room_seats(&runtime.room).is_empty() || room_has_only_bots(&runtime.room) {
            room_handle.mark_closed();
            close_runtime(&mut runtime);
            drop(runtime);
            unregister_room_handle(&state, table_code, &room_handle).await;
            state.inner.db.delete_table(table_code).await.ok();
            schedule_room_tasks_detached(state, table_code.to_string());
            MessageOutcome {
                outbound,
                owned_seat: None,
                clear_owned_seat: true,
                close_socket: true,
            }
        } else {
            let room = runtime.room.clone();
            let connections = snapshot_connections(&runtime)
                .into_iter()
                .filter(|(other_seat, _)| *other_seat != seat_index)
                .collect::<Vec<_>>();
            drop(runtime);
            let room_json = match serialize_room(&room) {
                Ok(value) => value,
                Err(error) => return internal_error_to(connection, error),
            };
            outbound.extend(presence_and_snapshot_for_all_from_snapshot(
                &room,
                &connections,
                table_code,
                seat_index,
                false,
            ));
            if let Err(error) = state
                .inner
                .db
                .save_table_and_delete_tokens_for_seat(
                    table_code,
                    &created_at,
                    &room_json,
                    seat_index,
                )
                .await
            {
                restore_room_snapshot(&room_handle, previous_room).await;
                return internal_error_to(connection, error);
            }
            let mut runtime = room_handle.runtime.lock().await;
            runtime.connections.remove(&seat_index);
            drop(runtime);
            schedule_room_tasks_detached(state, table_code.to_string());
            MessageOutcome {
                outbound,
                owned_seat: None,
                clear_owned_seat: true,
                close_socket: true,
            }
        }
    } else if room_has_only_bots(&runtime.room) || should_terminate_unattended(&runtime) {
        room_handle.mark_closed();
        close_runtime(&mut runtime);
        drop(runtime);
        unregister_room_handle(&state, table_code, &room_handle).await;
        state.inner.db.delete_table(table_code).await.ok();
        schedule_room_tasks_detached(state, table_code.to_string());
        MessageOutcome {
            outbound,
            owned_seat: None,
            clear_owned_seat: true,
            close_socket: true,
        }
    } else {
        let room = runtime.room.clone();
        let connections = snapshot_connections(&runtime)
            .into_iter()
            .filter(|(other_seat, _)| *other_seat != seat_index)
            .collect::<Vec<_>>();
        drop(runtime);
        let room_json = match serialize_room(&room) {
            Ok(value) => value,
            Err(error) => return internal_error_to(connection, error),
        };
        outbound.extend(collect_snapshot_and_prompt_outbound_from_snapshot(
            &room,
            &connections,
        ));
        if let Err(error) = state
            .inner
            .db
            .save_table_and_delete_tokens_for_seat(table_code, &created_at, &room_json, seat_index)
            .await
        {
            restore_room_snapshot(&room_handle, previous_room).await;
            return internal_error_to(connection, error);
        }
        let mut runtime = room_handle.runtime.lock().await;
        runtime.connections.remove(&seat_index);
        drop(runtime);
        schedule_room_tasks_detached(state, table_code.to_string());
        MessageOutcome {
            outbound,
            owned_seat: None,
            clear_owned_seat: true,
            close_socket: true,
        }
    }
}

async fn handle_disconnect(
    state: AppContext,
    table_code: &str,
    owned_seat: Option<usize>,
    connection_id: u64,
) {
    let Some(seat_index) = owned_seat else {
        return;
    };

    let Some(room_handle) = room_handle(&state, table_code).await else {
        return;
    };
    if room_handle.is_closed() {
        return;
    }
    let _persist_guard = room_handle.persist.lock().await;
    let mut runtime = room_handle.runtime.lock().await;
    if room_handle.is_closed() {
        return;
    }
    let previous_room = runtime.room.clone();
    let Some(current_handle) = runtime.connections.get(&seat_index).cloned() else {
        return;
    };
    if current_handle.id != connection_id {
        return;
    }
    set_seat_connected(
        &mut runtime.room,
        seat_index,
        false,
        Some(super::disconnect_deadline_iso()),
    );
    let _ = rust_reconcile_continue_action_state(&mut runtime.room);
    let created_at = runtime.created_at.clone();
    let room = runtime.room.clone();
    let connections = snapshot_connections(&runtime);
    drop(runtime);
    let room_json = match serialize_room(&room) {
        Ok(value) => value,
        Err(_) => return,
    };
    let outbound = presence_and_snapshot_for_all_from_snapshot(
        &room,
        &connections,
        table_code,
        seat_index,
        false,
    );
    if state
        .inner
        .db
        .save_table(table_code, &created_at, &room_json)
        .await
        .is_err()
    {
        restore_room_snapshot(&room_handle, previous_room).await;
        return;
    }
    let mut runtime = room_handle.runtime.lock().await;
    if runtime
        .connections
        .get(&seat_index)
        .is_some_and(|handle| handle.id == connection_id)
    {
        runtime.connections.remove(&seat_index);
    }
    drop(runtime);
    send_outbound(outbound);
    schedule_room_tasks_detached(state, table_code.to_string());
}
