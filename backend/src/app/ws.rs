use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Error;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxumPath, State};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::{Notify, mpsc};

use super::protocol::{
    HeartbeatPayload, action_rejected_message, heartbeat_message, leave_table_accepted_message,
    quick_chat_message,
};
use super::room_runtime::{
    close_runtime, ensure_room_loaded, replace_connection, restore_room_snapshot, room_handle,
    room_has_only_bots, should_terminate_unattended, snapshot_connections, unregister_room_handle,
};
use super::scheduler::schedule_room_tasks_detached;
use super::{
    AppContext, ConnectionHandle, OUTBOUND_CHANNEL_CAPACITY, OutboundMessage,
    add_bot_to_waiting_room, collect_join_outbound_from_snapshot,
    collect_snapshot_and_prompt_outbound_from_snapshot, convert_seat_to_bot,
    generate_player_session_id, generate_reconnect_token, generate_short_hex,
    maybe_start_test_match, normalize_table_code, occupied_seats,
    presence_and_snapshot_for_all_from_snapshot, random_open_seat_index,
    remove_bot_from_waiting_room, remove_seat_from_room, room_has_round_state, room_phase,
    room_player_session_id, room_seats, seat_exists, seat_matches_reconnect_credentials,
    send_outbound, serialize_room, set_seat_connected,
};
use crate::core::engine::try_handle_player_action_in_room_state;
use crate::core::state::SeatState;
use crate::rules::standard::flow::{
    reconcile_continue_action_state_in_room_state as reconcile_standard_continue_action_state,
    record_continue_action_in_room_state as record_standard_continue_action,
    room_ready_to_start as room_ready_to_start_in_state,
    start_match_in_room_state as start_standard_match,
};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
enum ClientMessage {
    JoinTable(JoinTableRequest),
    Reconnect(ReconnectRequest),
    Ready(ReadyRequest),
    AdjustBots(AdjustBotsRequest),
    StartMatch,
    StartNextRound,
    RestartMatch,
    LeaveTable,
    ActionRequest(ActionRequest),
    QuickChat(QuickChatRequest),
    Heartbeat(HeartbeatPayload),
}

fn has_empty_payload(value: &Value) -> bool {
    value.get("payload").is_none_or(|payload| {
        payload.is_null() || payload.as_object().is_some_and(|object| object.is_empty())
    })
}

fn parse_client_message(raw: &str) -> Result<ClientMessage, serde_json::Error> {
    let value: Value = serde_json::from_str(raw)?;
    match value.get("type").and_then(Value::as_str) {
        Some("start_match") if has_empty_payload(&value) => Ok(ClientMessage::StartMatch),
        Some("start_next_round") if has_empty_payload(&value) => Ok(ClientMessage::StartNextRound),
        Some("restart_match") if has_empty_payload(&value) => Ok(ClientMessage::RestartMatch),
        Some("leave_table") if has_empty_payload(&value) => Ok(ClientMessage::LeaveTable),
        _ => serde_json::from_value(value),
    }
}

#[derive(Debug, Default, Deserialize)]
struct JoinTableRequest {
    #[serde(default)]
    nickname: String,
}

#[derive(Debug, Default, Deserialize)]
struct ReconnectRequest {
    #[serde(default)]
    reconnect_token: String,
}

#[derive(Debug, Deserialize)]
struct ReadyRequest {
    #[serde(default = "default_true")]
    ready: bool,
}

impl Default for ReadyRequest {
    fn default() -> Self {
        Self { ready: true }
    }
}

#[derive(Debug, Default, Deserialize)]
struct ActionRequest {
    #[serde(default)]
    action_type: String,
    #[serde(default)]
    tile_ids: Vec<String>,
}

impl ActionRequest {
    fn tile_id_strings(&self) -> Vec<String> {
        self.tile_ids.clone()
    }
}

#[derive(Debug, Default, Deserialize)]
struct QuickChatRequest {
    target_seat: Option<usize>,
    #[serde(default)]
    emoji: String,
}

#[derive(Debug, Default, Deserialize)]
struct AdjustBotsRequest {
    #[serde(default)]
    delta: i64,
}

fn default_true() -> bool {
    true
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
        let message: ClientMessage = match parse_client_message(text.as_str()) {
            Ok(value) => value,
            Err(_) => {
                send_outbound(vec![
                    handle.outbound(action_rejected_message("unsupported_message")),
                ]);
                continue;
            }
        };

        let outcome =
            handle_client_message(state.clone(), &table_code, &handle, owned_seat, message).await;

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
    message: ClientMessage,
) -> MessageOutcome {
    match message {
        ClientMessage::JoinTable(request) => {
            if owned_seat.is_some() {
                return reject_to(connection, "seat_already_owned");
            }
            handle_join_table(state, table_code, connection, request).await
        }
        ClientMessage::Reconnect(request) => {
            if owned_seat.is_some() {
                return reject_to(connection, "seat_already_owned");
            }
            handle_reconnect(state, table_code, connection, request).await
        }
        ClientMessage::Ready(request) => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, owned_seat).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_ready(state, table_code, connection, seat_index, request).await
        }
        ClientMessage::AdjustBots(request) => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, owned_seat).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_adjust_bots(state, table_code, connection, seat_index, request).await
        }
        ClientMessage::StartMatch => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, owned_seat).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_start_match(state, table_code, connection, seat_index).await
        }
        ClientMessage::StartNextRound => {
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
        ClientMessage::RestartMatch => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, owned_seat).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_continue_action(state, table_code, connection, seat_index, "restart_match").await
        }
        ClientMessage::LeaveTable => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, owned_seat).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_leave_table(state, table_code, connection, seat_index).await
        }
        ClientMessage::ActionRequest(request) => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, owned_seat).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_action_request(state, table_code, connection, seat_index, request).await
        }
        ClientMessage::QuickChat(request) => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, owned_seat).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_quick_chat(state, table_code, connection, seat_index, request).await
        }
        ClientMessage::Heartbeat(payload) => MessageOutcome {
            outbound: vec![connection.outbound(heartbeat_message(payload))],
            owned_seat: None,
            clear_owned_seat: false,
            close_socket: false,
        },
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
    request: JoinTableRequest,
) -> MessageOutcome {
    let nickname = if request.nickname.trim().is_empty() {
        "Player".to_string()
    } else {
        request.nickname
    };

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
    let Some(seat_index) = random_open_seat_index(&runtime.room) else {
        return reject_to(connection, "table_full");
    };

    let player_session_id = generate_player_session_id();
    let reconnect_token = generate_reconnect_token();
    runtime.room.seats.push(SeatState {
        seat_index,
        nickname: Some(nickname.clone()),
        reconnect_token: Some(reconnect_token.clone()),
        player_session_id: Some(player_session_id),
        connected: true,
        ready: false,
        is_bot: false,
        seat_type: "human".to_string(),
        bot_persona: None,
        bot_aggression: None,
        disconnect_deadline_at: None,
        skill_loadout: Default::default(),
    });
    runtime.room.seats.sort_by_key(|seat| seat.seat_index);
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
    request: ReconnectRequest,
) -> MessageOutcome {
    let reconnect_token = request.reconnect_token;

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
    if let Some(seat) = runtime
        .room
        .seats
        .iter_mut()
        .find(|seat| seat.seat_index == token_record.seat_index)
    {
        seat.reconnect_token = Some(new_token.clone());
        seat.connected = true;
        seat.disconnect_deadline_at = None;
    }
    let _ = reconcile_standard_continue_action_state(&mut runtime.room);
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
    request: ReadyRequest,
) -> MessageOutcome {
    let ready = request.ready;
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
    if let Some(seat) = runtime
        .room
        .seats
        .iter_mut()
        .find(|seat| seat.seat_index == seat_index)
    {
        seat.ready = ready;
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
    request: AdjustBotsRequest,
) -> MessageOutcome {
    let delta = request.delta;
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
        outbound: vec![connection.outbound(action_rejected_message(reason))],
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
    let ready_to_start = room_ready_to_start_in_state(&runtime.room);
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
    if let Err(error) = start_standard_match(&mut runtime.room, dealer_seat, rand::random::<u64>())
    {
        return internal_error_to(connection, Error::msg(error));
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
    let continue_result = record_standard_continue_action(&mut runtime.room, seat_index, action_id);
    if let Err(reason) = continue_result {
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
    request: ActionRequest,
) -> MessageOutcome {
    let tile_id_strings = request.tile_id_strings();
    let action_type = request.action_type;

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
    let action_result = match try_handle_player_action_in_room_state(
        &mut runtime.room,
        seat_index,
        &action_type,
        &tile_id_strings,
    ) {
        Ok(result) => result,
        Err(error) => return internal_error_to(connection, Error::msg(error)),
    };
    let rust_handled_messages = match action_result {
        Some(Ok(output)) => output.emitted_messages,
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
    request: QuickChatRequest,
) -> MessageOutcome {
    let target_seat = request.target_seat;
    let emoji = request.emoji.trim().to_string();
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

    let payload = quick_chat_message(
        generate_short_hex(8),
        seat_index,
        target_seat,
        emoji,
        super::now_iso(),
    );
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
        let _ = reconcile_standard_continue_action_state(&mut runtime.room);
    }

    let mut outbound =
        vec![connection.outbound(leave_table_accepted_message(table_code, seat_index))];

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
    let _ = reconcile_standard_continue_action_state(&mut runtime.room);
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

#[cfg(test)]
mod tests {
    use super::{ClientMessage, parse_client_message};

    #[test]
    fn parse_payloadless_commands_with_empty_payload_object() {
        let cases = [
            (
                r#"{"type":"start_match","payload":{}}"#,
                ClientMessage::StartMatch,
            ),
            (
                r#"{"type":"start_next_round","payload":{}}"#,
                ClientMessage::StartNextRound,
            ),
            (
                r#"{"type":"restart_match","payload":{}}"#,
                ClientMessage::RestartMatch,
            ),
            (
                r#"{"type":"leave_table","payload":{}}"#,
                ClientMessage::LeaveTable,
            ),
        ];

        for (raw, expected) in cases {
            let parsed = parse_client_message(raw).expect("message should parse");
            assert_eq!(
                std::mem::discriminant(&parsed),
                std::mem::discriminant(&expected)
            );
        }
    }

    #[test]
    fn parse_payloadless_commands_without_payload() {
        let parsed =
            parse_client_message(r#"{"type":"start_next_round"}"#).expect("message should parse");
        assert!(matches!(parsed, ClientMessage::StartNextRound));
    }

    #[test]
    fn reject_non_empty_payload_for_payloadless_commands() {
        let result = parse_client_message(r#"{"type":"leave_table","payload":{"force":true}}"#);
        assert!(result.is_err());
    }
}
