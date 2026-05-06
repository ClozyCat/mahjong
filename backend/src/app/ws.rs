use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Error;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxumPath, State};
use axum::response::IntoResponse;
use chrono::{SecondsFormat, TimeDelta, Utc};
use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::{Notify, mpsc};

use super::auth::hash_session_token;
use super::collect_observer_outbound_from_snapshot;
use super::protocol::{
    HeartbeatPayload, action_rejected_message, dealer_selection_started_message, heartbeat_message,
    leave_table_accepted_message, quick_chat_message,
};
use super::records::archive_current_round_if_needed;
use super::room_runtime::{
    PendingStartMatch, add_seat_connection, broadcast_to_seat_group, close_runtime,
    ensure_room_loaded, remove_all_seat_connections, remove_seat_connection, restore_room_snapshot,
    room_handle, room_has_only_bots, seat_group_contains_connection, should_terminate_unattended,
    snapshot_connections, unregister_room_handle,
};
use super::room_runtime::{
    remove_spectator_connection, replace_spectator_connection, snapshot_spectator_connections,
};
use super::scheduler::schedule_room_tasks_detached;
use super::{
    AppContext, ConnectionHandle, OUTBOUND_CHANNEL_CAPACITY, OutboundMessage,
    add_bot_to_waiting_room, collect_join_outbound_from_snapshot,
    collect_snapshot_and_prompt_outbound_from_snapshot, convert_seat_to_bot,
    generate_player_session_id, generate_reconnect_token, generate_short_hex, normalize_table_code,
    occupied_seats, presence_and_snapshot_for_all_from_snapshot, random_open_seat_index,
    remove_bot_from_waiting_room, remove_seat_from_room, room_has_round_state, room_phase,
    room_player_session_id, room_seats, seat_exists, seat_matches_reconnect_credentials,
    send_outbound, serialize_room, set_seat_bot_takeover, set_seat_connected,
};
use crate::core::engine::try_handle_player_action_in_room_state;
use crate::core::state::SeatState;
use crate::rules::standard::flow::{
    reconcile_continue_action_state_in_room_state as reconcile_standard_continue_action_state,
    record_continue_action_in_room_state as record_standard_continue_action,
    room_ready_to_start as room_ready_to_start_in_state,
};

const DEALER_SELECTION_DURATION_MS: u64 = 4_200;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
enum ClientMessage {
    WatchTable(WatchTableRequest),
    JoinTable(JoinTableRequest),
    Reconnect(ReconnectRequest),
    Ready(ReadyRequest),
    AdjustBots(AdjustBotsRequest),
    SetBotTakeover(SetBotTakeoverRequest),
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
    session_token: String,
}
#[derive(Debug, Default, Deserialize)]
struct WatchTableRequest {
    #[serde(default)]
    session_token: String,
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

#[derive(Debug, Default, Deserialize)]
struct SetBotTakeoverRequest {
    #[serde(default)]
    enabled: bool,
}

fn default_true() -> bool {
    true
}

fn dealer_selection_reveal_at() -> String {
    (Utc::now() + TimeDelta::milliseconds(DEALER_SELECTION_DURATION_MS as i64))
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub(crate) struct MessageOutcome {
    pub(crate) outbound: Vec<OutboundMessage>,
    pub(crate) role: Option<ConnectionRole>,
    pub(crate) clear_role: bool,
    pub(crate) close_socket: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionRole {
    Unbound,
    Player { seat_index: usize },
    Spectator { spectator_id: u64 },
}

impl ConnectionRole {
    fn owned_seat(self) -> Option<usize> {
        match self {
            Self::Player { seat_index } => Some(seat_index),
            Self::Unbound => None,
            Self::Spectator { .. } => None,
        }
    }
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

    let mut role = ConnectionRole::Unbound;
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
            handle_client_message(state.clone(), &table_code, &handle, role, message).await;

        if let Some(new_role) = outcome.role {
            role = new_role;
        }
        if outcome.clear_role {
            role = ConnectionRole::Unbound;
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
        match role {
            ConnectionRole::Player { seat_index } => {
                handle_disconnect(state, &table_code, Some(seat_index), connection_id).await;
            }
            ConnectionRole::Spectator { spectator_id } => {
                handle_spectator_disconnect(state, &table_code, spectator_id, connection_id).await;
            }
            ConnectionRole::Unbound => {}
        }
    }
    handle.request_close();
    drop(outgoing_tx);
    let _ = writer.await;
}

async fn handle_client_message(
    state: AppContext,
    table_code: &str,
    connection: &ConnectionHandle,
    role: ConnectionRole,
    message: ClientMessage,
) -> MessageOutcome {
    match message {
        ClientMessage::WatchTable(request) => {
            if !matches!(role, ConnectionRole::Unbound) {
                return reject_to(connection, "seat_already_owned");
            }
            handle_watch_table(state, table_code, connection, request).await
        }
        ClientMessage::JoinTable(request) => {
            if !matches!(role, ConnectionRole::Unbound) {
                return reject_to(connection, "seat_already_owned");
            }
            handle_join_table(state, table_code, connection, request).await
        }
        ClientMessage::Reconnect(request) => {
            if !matches!(role, ConnectionRole::Unbound) {
                return reject_to(connection, "seat_already_owned");
            }
            handle_reconnect(state, table_code, connection, request).await
        }
        ClientMessage::Ready(request) => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, role.owned_seat()).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_ready(state, table_code, connection, seat_index, request).await
        }
        ClientMessage::AdjustBots(request) => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, role.owned_seat()).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_adjust_bots(state, table_code, connection, seat_index, request).await
        }
        ClientMessage::SetBotTakeover(request) => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, role.owned_seat()).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_set_bot_takeover(state, table_code, connection, seat_index, request).await
        }
        ClientMessage::StartMatch => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, role.owned_seat()).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_start_match(state, table_code, connection, seat_index).await
        }
        ClientMessage::StartNextRound => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, role.owned_seat()).await
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
                assert_active_owned_seat(&state, table_code, connection, role.owned_seat()).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_continue_action(state, table_code, connection, seat_index, "restart_match").await
        }
        ClientMessage::LeaveTable => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, role.owned_seat()).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_leave_table(state, table_code, connection, seat_index).await
        }
        ClientMessage::ActionRequest(request) => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, role.owned_seat()).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_action_request(state, table_code, connection, seat_index, request).await
        }
        ClientMessage::QuickChat(request) => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, role.owned_seat()).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_quick_chat(state, table_code, connection, seat_index, request).await
        }
        ClientMessage::Heartbeat(payload) => MessageOutcome {
            outbound: vec![connection.outbound(heartbeat_message(payload))],
            role: None,
            clear_role: false,
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
    if seat_group_contains_connection(&runtime, seat_index, connection.id) {
        Some(seat_index)
    } else {
        None
    }
}
async fn handle_watch_table(
    state: AppContext,
    table_code: &str,
    connection: &ConnectionHandle,
    request: WatchTableRequest,
) -> MessageOutcome {
    let session_token = request.session_token.trim().to_string();
    if session_token.is_empty() {
        return reject_to(connection, "auth_required");
    }
    let authenticated_user = match state
        .inner
        .db
        .get_authenticated_user(&hash_session_token(&session_token), &super::now_iso())
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) | Err(_) => return reject_to(connection, "auth_required"),
    };
    let _nickname = request.nickname.trim();
    let Some(room_handle) = ensure_room_loaded(&state, table_code).await.ok().flatten() else {
        return reject_to(connection, "table_not_found");
    };
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }

    let spectator_id = connection.id;
    let runtime = room_handle.runtime.lock().await;
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }
    if runtime.room.owner_user_id == Some(authenticated_user.user_id) {
        return reject_to(connection, "player_cannot_watch_own_table");
    }
    let table_code = runtime.room.table_code.clone();
    let room = runtime.room.clone();
    drop(runtime);

    match state
        .inner
        .db
        .get_active_table_participant(&table_code, authenticated_user.user_id)
        .await
    {
        Ok(Some(_)) => return reject_to(connection, "player_cannot_watch_own_table"),
        Ok(None) => {}
        Err(error) => return internal_error_to(connection, error),
    }
    match state
        .inner
        .db
        .has_approved_spectator_request(&table_code, authenticated_user.user_id)
        .await
    {
        Ok(true) => {}
        Ok(false) => return reject_to(connection, "spectator_requires_owner_approval"),
        Err(error) => return internal_error_to(connection, error),
    }

    let mut runtime = room_handle.runtime.lock().await;
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }
    replace_spectator_connection(&mut runtime, spectator_id, connection);
    drop(runtime);

    MessageOutcome {
        outbound: collect_observer_outbound_from_snapshot(
            &room,
            &[(spectator_id, connection.clone())],
        ),
        role: Some(ConnectionRole::Spectator { spectator_id }),
        clear_role: false,
        close_socket: false,
    }
}

async fn handle_join_table(
    state: AppContext,
    table_code: &str,
    connection: &ConnectionHandle,
    request: JoinTableRequest,
) -> MessageOutcome {
    let session_token = request.session_token.trim().to_string();
    if session_token.is_empty() {
        return reject_to(connection, "table_invite_required");
    }
    let authenticated_user = match state
        .inner
        .db
        .get_authenticated_user(&hash_session_token(&session_token), &super::now_iso())
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) | Err(_) => return reject_to(connection, "table_invite_required"),
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
        .any(|group| group.connections.contains_key(&connection.id))
    {
        return reject_to(connection, "seat_already_owned");
    }

    let existing_participant = match state
        .inner
        .db
        .get_active_table_participant(table_code, authenticated_user.user_id)
        .await
    {
        Ok(participant) => participant,
        Err(error) => return internal_error_to(connection, error),
    };

    let (seat_index, persisted_with_new_participant) =
        if let Some(participant) = existing_participant {
            let Some(seat) = runtime
                .room
                .seats
                .iter_mut()
                .find(|seat| seat.seat_index == participant.seat_index)
            else {
                return reject_to(connection, "table_invite_required");
            };
            seat.connected = true;
            seat.disconnect_deadline_at = None;
            (participant.seat_index, false)
        } else if runtime.room.owner_user_id == Some(authenticated_user.user_id)
            && room_phase(&runtime.room) == "waiting"
            && !room_has_round_state(&runtime.room)
        {
            let Some(user) = state
                .inner
                .db
                .get_user_by_id(authenticated_user.user_id)
                .await
                .ok()
                .flatten()
            else {
                return reject_to(connection, "table_invite_required");
            };
            let Some(seat_index) = random_open_seat_index(&runtime.room) else {
                return reject_to(connection, "table_full");
            };
            let player_session_id = generate_player_session_id();
            let reconnect_token = generate_reconnect_token();
            runtime.room.seats.push(SeatState {
                seat_index,
                nickname: Some(user.display_name.clone()),
                reconnect_token: Some(reconnect_token.clone()),
                player_session_id: Some(player_session_id),
                connected: true,
                ready: false,
                is_bot: false,
                seat_type: "human".to_string(),
                bot_persona: None,
                bot_aggression: None,
                disconnect_deadline_at: None,
            });
            runtime.room.seats.sort_by_key(|seat| seat.seat_index);
            let created_at = runtime.created_at.clone();
            let room = runtime.room.clone();
            let connections = snapshot_connections(&runtime);
            let spectator_connections = snapshot_spectator_connections(&runtime);
            drop(runtime);
            let room_json = match serialize_room(&room) {
                Ok(value) => value,
                Err(error) => return internal_error_to(connection, error),
            };
            if let Err(error) = state
                .inner
                .db
                .save_table_and_store_reconnect_token_and_upsert_participant(
                    table_code,
                    &created_at,
                    &room_json,
                    &reconnect_token,
                    seat_index,
                    player_session_id,
                    authenticated_user.user_id,
                    &user.display_name,
                    &created_at,
                )
                .await
            {
                restore_room_snapshot(&room_handle, previous_room).await;
                return internal_error_to(connection, error);
            }
            let mut outbound = collect_join_outbound_from_snapshot(
                &room,
                &connections,
                table_code,
                connection,
                seat_index,
                true,
            );
            outbound.extend(collect_observer_outbound_from_snapshot(
                &room,
                &spectator_connections,
            ));
            let mut runtime = room_handle.runtime.lock().await;
            add_seat_connection(
                &mut runtime,
                seat_index,
                Some(authenticated_user.user_id),
                connection,
            );
            drop(runtime);
            schedule_room_tasks_detached(state, table_code.to_string());
            return MessageOutcome {
                outbound,
                role: Some(ConnectionRole::Player { seat_index }),
                clear_role: false,
                close_socket: false,
            };
        } else {
            return reject_to(connection, "table_invite_required");
        };

    let created_at = runtime.created_at.clone();
    let room = runtime.room.clone();
    let connections = snapshot_connections(&runtime);
    let spectator_connections = snapshot_spectator_connections(&runtime);
    drop(runtime);
    if !persisted_with_new_participant {
        let room_json = match serialize_room(&room) {
            Ok(value) => value,
            Err(error) => return internal_error_to(connection, error),
        };
        if let Err(error) = state
            .inner
            .db
            .save_table(table_code, &created_at, &room_json)
            .await
        {
            restore_room_snapshot(&room_handle, previous_room).await;
            return internal_error_to(connection, error);
        }
    }
    let mut outbound = collect_join_outbound_from_snapshot(
        &room,
        &connections,
        table_code,
        connection,
        seat_index,
        true,
    );
    outbound.extend(collect_observer_outbound_from_snapshot(
        &room,
        &spectator_connections,
    ));
    let mut runtime = room_handle.runtime.lock().await;
    add_seat_connection(
        &mut runtime,
        seat_index,
        Some(authenticated_user.user_id),
        connection,
    );
    drop(runtime);
    schedule_room_tasks_detached(state, table_code.to_string());
    MessageOutcome {
        outbound,
        role: Some(ConnectionRole::Player { seat_index }),
        clear_role: false,
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
        .any(|group| group.connections.contains_key(&connection.id))
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
    let spectator_connections = snapshot_spectator_connections(&runtime);
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
    let mut outbound = collect_join_outbound_from_snapshot(
        &room,
        &connections,
        table_code,
        connection,
        token_record.seat_index,
        true,
    );
    outbound.extend(collect_observer_outbound_from_snapshot(
        &room,
        &spectator_connections,
    ));
    let mut runtime = room_handle.runtime.lock().await;
    add_seat_connection(&mut runtime, token_record.seat_index, None, connection);
    drop(runtime);
    schedule_room_tasks_detached(state, table_code.to_string());
    MessageOutcome {
        outbound,
        role: Some(ConnectionRole::Player {
            seat_index: token_record.seat_index,
        }),
        clear_role: false,
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
    let spectator_connections = snapshot_spectator_connections(&runtime);
    drop(runtime);
    let room_json = match serialize_room(&room) {
        Ok(value) => value,
        Err(error) => return internal_error_to(connection, error),
    };
    let mut outbound = collect_snapshot_and_prompt_outbound_from_snapshot(&room, &connections);
    outbound.extend(collect_observer_outbound_from_snapshot(
        &room,
        &spectator_connections,
    ));
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
        role: None,
        clear_role: false,
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
    let spectator_connections = snapshot_spectator_connections(&runtime);
    drop(runtime);
    let room_json = match serialize_room(&room) {
        Ok(value) => value,
        Err(error) => return internal_error_to(connection, error),
    };
    let mut outbound = collect_snapshot_and_prompt_outbound_from_snapshot(&room, &connections);
    outbound.extend(collect_observer_outbound_from_snapshot(
        &room,
        &spectator_connections,
    ));
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
        role: None,
        clear_role: false,
        close_socket: false,
    };
    schedule_room_tasks_detached(state, table_code.to_string());
    outcome
}

async fn handle_set_bot_takeover(
    state: AppContext,
    table_code: &str,
    connection: &ConnectionHandle,
    seat_index: usize,
    request: SetBotTakeoverRequest,
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
    if let Err(reason) = set_seat_bot_takeover(&mut runtime.room, seat_index, request.enabled) {
        return reject_to(connection, reason);
    }
    let _ = reconcile_standard_continue_action_state(&mut runtime.room);

    let created_at = runtime.created_at.clone();
    let room = runtime.room.clone();
    let connections = snapshot_connections(&runtime);
    let spectator_connections = snapshot_spectator_connections(&runtime);
    drop(runtime);
    let room_json = match serialize_room(&room) {
        Ok(value) => value,
        Err(error) => return internal_error_to(connection, error),
    };
    let mut outbound = collect_snapshot_and_prompt_outbound_from_snapshot(&room, &connections);
    outbound.extend(collect_observer_outbound_from_snapshot(
        &room,
        &spectator_connections,
    ));
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
        role: None,
        clear_role: false,
        close_socket: false,
    };
    schedule_room_tasks_detached(state, table_code.to_string());
    outcome
}

fn reject_to(connection: &ConnectionHandle, reason: &str) -> MessageOutcome {
    MessageOutcome {
        outbound: vec![connection.outbound(action_rejected_message(reason))],
        role: None,
        clear_role: false,
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
    if runtime.pending_start_match.is_some() {
        return reject_to(connection, "room_already_started");
    }
    let already_started = room_has_round_state(&runtime.room);
    let ready_to_start = room_ready_to_start_in_state(&runtime.room);
    let occupied = occupied_seats(&runtime.room);
    if already_started || room_phase(&runtime.room) != "waiting" {
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
    let started_at = super::now_iso();
    let reveal_at = dealer_selection_reveal_at();
    runtime.pending_start_match = Some(PendingStartMatch {
        dealer_seat,
        reveal_at: reveal_at.clone(),
    });
    let connections = snapshot_connections(&runtime);
    let spectator_connections = snapshot_spectator_connections(&runtime);
    drop(runtime);
    let selection_message = dealer_selection_started_message(
        dealer_seat,
        started_at,
        reveal_at,
        DEALER_SELECTION_DURATION_MS,
    );
    let mut outbound = connections
        .into_iter()
        .map(|(_, handle)| handle.outbound(selection_message.clone()))
        .collect::<Vec<_>>();
    outbound.extend(
        spectator_connections
            .into_iter()
            .map(|(_, handle)| handle.outbound(selection_message.clone())),
    );
    let outcome = MessageOutcome {
        outbound,
        role: None,
        clear_role: false,
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
    let spectator_connections = snapshot_spectator_connections(&runtime);
    drop(runtime);
    let room_json = match serialize_room(&room) {
        Ok(value) => value,
        Err(error) => return internal_error_to(connection, error),
    };
    let mut outbound = collect_snapshot_and_prompt_outbound_from_snapshot(&room, &connections);
    outbound.extend(collect_observer_outbound_from_snapshot(
        &room,
        &spectator_connections,
    ));
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
        role: None,
        clear_role: false,
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
    if runtime
        .room
        .seats
        .iter()
        .any(|seat| seat.seat_index == seat_index && seat.is_bot)
    {
        return reject_to(connection, "bot_takeover_enabled");
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
    let room = runtime.room.clone();
    let room_json = match serialize_room(&room) {
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
    if let Err(error) =
        archive_current_round_if_needed(&state, &room, &created_at, &super::now_iso()).await
    {
        eprintln!("failed to archive round for table {table_code}: {error:#}");
    }
    let runtime = room_handle.runtime.lock().await;
    let connections = snapshot_connections(&runtime);
    let spectator_connections = snapshot_spectator_connections(&runtime);
    let mut broadcast_handles = connections
        .iter()
        .map(|(_, handle)| handle.clone())
        .collect::<Vec<_>>();
    broadcast_handles.extend(
        spectator_connections
            .iter()
            .map(|(_, handle)| handle.clone()),
    );
    let room = runtime.room.clone();
    let mut snapshot_outbound =
        collect_snapshot_and_prompt_outbound_from_snapshot(&room, &connections);
    snapshot_outbound.extend(collect_observer_outbound_from_snapshot(
        &room,
        &spectator_connections,
    ));
    drop(runtime);
    let mut outbound =
        super::broadcast_to_handles(&broadcast_handles, Some(&rust_handled_messages));
    outbound.extend(snapshot_outbound);
    schedule_room_tasks_detached(state, table_code.to_string());
    MessageOutcome {
        outbound,
        role: None,
        clear_role: false,
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
        role: None,
        clear_role: false,
        close_socket: false,
    }
}

async fn handle_leave_table(
    state: AppContext,
    table_code: &str,
    connection: &ConnectionHandle,
    seat_index: usize,
) -> MessageOutcome {
    let left_at = super::now_iso();
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

    let leave_payload = leave_table_accepted_message(table_code, seat_index);
    let mut outbound = broadcast_to_seat_group(&runtime, seat_index, leave_payload.clone());
    if outbound.is_empty() {
        outbound.push(connection.outbound(leave_payload));
    }

    if phase == "waiting" {
        if room_seats(&runtime.room).is_empty() || room_has_only_bots(&runtime.room) {
            room_handle.mark_closed();
            close_runtime(&mut runtime);
            drop(runtime);
            unregister_room_handle(&state, table_code, &room_handle).await;
            state.inner.db.delete_table(table_code, &left_at).await.ok();
            schedule_room_tasks_detached(state, table_code.to_string());
            MessageOutcome {
                outbound,
                role: None,
                clear_role: true,
                close_socket: true,
            }
        } else {
            let room = runtime.room.clone();
            let connections = snapshot_connections(&runtime)
                .into_iter()
                .filter(|(other_seat, _)| *other_seat != seat_index)
                .collect::<Vec<_>>();
            let spectator_connections = snapshot_spectator_connections(&runtime);
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
            outbound.extend(collect_observer_outbound_from_snapshot(
                &room,
                &spectator_connections,
            ));
            if let Err(error) = state
                .inner
                .db
                .save_table_and_delete_tokens_for_seat(
                    table_code,
                    &created_at,
                    &room_json,
                    seat_index,
                    &left_at,
                )
                .await
            {
                restore_room_snapshot(&room_handle, previous_room).await;
                return internal_error_to(connection, error);
            }
            let mut runtime = room_handle.runtime.lock().await;
            for handle in remove_all_seat_connections(&mut runtime, seat_index) {
                handle.request_close();
            }
            drop(runtime);
            schedule_room_tasks_detached(state, table_code.to_string());
            MessageOutcome {
                outbound,
                role: None,
                clear_role: true,
                close_socket: true,
            }
        }
    } else if (room_has_only_bots(&runtime.room)
        && runtime
            .connections
            .keys()
            .all(|connected_seat| *connected_seat == seat_index))
        || should_terminate_unattended(&runtime)
    {
        room_handle.mark_closed();
        close_runtime(&mut runtime);
        drop(runtime);
        unregister_room_handle(&state, table_code, &room_handle).await;
        state.inner.db.delete_table(table_code, &left_at).await.ok();
        schedule_room_tasks_detached(state, table_code.to_string());
        MessageOutcome {
            outbound,
            role: None,
            clear_role: true,
            close_socket: true,
        }
    } else {
        let room = runtime.room.clone();
        let connections = snapshot_connections(&runtime)
            .into_iter()
            .filter(|(other_seat, _)| *other_seat != seat_index)
            .collect::<Vec<_>>();
        let spectator_connections = snapshot_spectator_connections(&runtime);
        drop(runtime);
        let room_json = match serialize_room(&room) {
            Ok(value) => value,
            Err(error) => return internal_error_to(connection, error),
        };
        outbound.extend(collect_snapshot_and_prompt_outbound_from_snapshot(
            &room,
            &connections,
        ));
        outbound.extend(collect_observer_outbound_from_snapshot(
            &room,
            &spectator_connections,
        ));
        if let Err(error) = state
            .inner
            .db
            .save_table_and_delete_tokens_for_seat(
                table_code,
                &created_at,
                &room_json,
                seat_index,
                &left_at,
            )
            .await
        {
            restore_room_snapshot(&room_handle, previous_room).await;
            return internal_error_to(connection, error);
        }
        let mut runtime = room_handle.runtime.lock().await;
        for handle in remove_all_seat_connections(&mut runtime, seat_index) {
            handle.request_close();
        }
        drop(runtime);
        schedule_room_tasks_detached(state, table_code.to_string());
        MessageOutcome {
            outbound,
            role: None,
            clear_role: true,
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
    if !seat_group_contains_connection(&runtime, seat_index, connection_id) {
        return;
    }
    if remove_seat_connection(&mut runtime, seat_index, connection_id) {
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
    let spectator_connections = snapshot_spectator_connections(&runtime);
    drop(runtime);
    let room_json = match serialize_room(&room) {
        Ok(value) => value,
        Err(_) => return,
    };
    let mut outbound = presence_and_snapshot_for_all_from_snapshot(
        &room,
        &connections,
        table_code,
        seat_index,
        false,
    );
    outbound.extend(collect_observer_outbound_from_snapshot(
        &room,
        &spectator_connections,
    ));
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
    let runtime = room_handle.runtime.lock().await;
    drop(runtime);
    send_outbound(outbound);
    schedule_room_tasks_detached(state, table_code.to_string());
}
async fn handle_spectator_disconnect(
    state: AppContext,
    table_code: &str,
    spectator_id: u64,
    connection_id: u64,
) {
    let Some(room_handle) = room_handle(&state, table_code).await else {
        return;
    };
    if room_handle.is_closed() {
        return;
    }
    let mut runtime = room_handle.runtime.lock().await;
    remove_spectator_connection(&mut runtime, spectator_id, connection_id);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use anyhow::Result;
    use serde_json::Value;
    use tokio::sync::{Notify, mpsc};

    use super::{
        ClientMessage, ConnectionRole, JoinTableRequest, ReadyRequest, WatchTableRequest,
        handle_client_message, handle_disconnect, handle_join_table, handle_watch_table,
        parse_client_message,
    };
    use crate::app::auth::{generate_session_token, hash_password, hash_session_token};
    use crate::app::persistence::{DbWorker, in_memory_database};
    use crate::app::room_runtime::room_handle;
    use crate::app::{
        AppContext, ConnectionHandle, initial_room_state_with_owner, serialize_room_state,
    };
    use crate::core::state::SeatState;

    fn test_connection_handle(
        id: u64,
        capacity: usize,
    ) -> (ConnectionHandle, mpsc::Receiver<String>) {
        let (sender, receiver) = mpsc::channel(capacity);
        (
            ConnectionHandle {
                id,
                sender,
                close_requested: Arc::new(AtomicBool::new(false)),
                close_notify: Arc::new(Notify::new()),
            },
            receiver,
        )
    }

    async fn build_reserved_participant_state(table_code: &str) -> Result<(AppContext, String)> {
        let db = in_memory_database("")?;
        db.initialize()?;
        let worker = DbWorker::start(db)?;

        worker
            .create_invite_code("INVITE200003", "2026-05-06T00:00:00Z", None)
            .await?;
        let owner_token = generate_session_token();
        let owner = worker
            .register_user(
                "Owner",
                "Owner",
                &hash_password("secret-123")?,
                "INVITE200003",
                &hash_session_token(&owner_token),
                "2026-05-06T00:00:00Z",
            )
            .await?;

        worker
            .create_invite_code("INVITE200004", "2026-05-06T00:00:00Z", None)
            .await?;
        let guest_token = generate_session_token();
        let guest = worker
            .register_user(
                "Guest",
                "Guest",
                &hash_password("secret-123")?,
                "INVITE200004",
                &hash_session_token(&guest_token),
                "2026-05-06T00:00:00Z",
            )
            .await?;

        let mut room = initial_room_state_with_owner(table_code, Some(owner.user_id), 1);
        room.seats.push(SeatState {
            seat_index: 0,
            nickname: Some("Guest".to_string()),
            reconnect_token: Some("token-join".to_string()),
            player_session_id: Some(88),
            connected: false,
            ready: false,
            is_bot: false,
            seat_type: "human".to_string(),
            bot_persona: None,
            bot_aggression: None,
            disconnect_deadline_at: None,
        });
        let room_json = serialize_room_state(&room)?;
        worker
            .save_table_and_store_reconnect_token_and_upsert_participant(
                table_code,
                "2026-05-06T00:00:00Z",
                &room_json,
                "token-join",
                0,
                88,
                guest.user_id,
                "Guest",
                "2026-05-06T00:00:00Z",
            )
            .await?;

        Ok((AppContext::new(worker), guest_token))
    }

    async fn build_watch_state(table_code: &str) -> Result<(AppContext, i64, String, i64, String)> {
        let db = in_memory_database("")?;
        db.initialize()?;
        let worker = DbWorker::start(db)?;

        worker
            .create_invite_code("INVITE200005", "2026-05-06T00:00:00Z", None)
            .await?;
        let owner_token = generate_session_token();
        let owner = worker
            .register_user(
                "OwnerWatch",
                "OwnerWatch",
                &hash_password("secret-123")?,
                "INVITE200005",
                &hash_session_token(&owner_token),
                "2026-05-06T00:00:00Z",
            )
            .await?;

        worker
            .create_invite_code("INVITE200006", "2026-05-06T00:00:00Z", None)
            .await?;
        let guest_token = generate_session_token();
        let guest = worker
            .register_user(
                "GuestWatch",
                "GuestWatch",
                &hash_password("secret-123")?,
                "INVITE200006",
                &hash_session_token(&guest_token),
                "2026-05-06T00:00:00Z",
            )
            .await?;

        worker
            .create_invite_code("INVITE200007", "2026-05-06T00:00:00Z", None)
            .await?;
        let viewer_token = generate_session_token();
        let viewer = worker
            .register_user(
                "ViewerWatch",
                "ViewerWatch",
                &hash_password("secret-123")?,
                "INVITE200007",
                &hash_session_token(&viewer_token),
                "2026-05-06T00:00:00Z",
            )
            .await?;

        let mut room = initial_room_state_with_owner(table_code, Some(owner.user_id), 1);
        room.seats.push(SeatState {
            seat_index: 0,
            nickname: Some("GuestWatch".to_string()),
            reconnect_token: Some("watch-token".to_string()),
            player_session_id: Some(91),
            connected: true,
            ready: true,
            is_bot: false,
            seat_type: "human".to_string(),
            bot_persona: None,
            bot_aggression: None,
            disconnect_deadline_at: None,
        });
        let room_json = serialize_room_state(&room)?;
        worker
            .save_table_and_store_reconnect_token_and_upsert_participant(
                table_code,
                "2026-05-06T00:00:00Z",
                &room_json,
                "watch-token",
                0,
                91,
                guest.user_id,
                "GuestWatch",
                "2026-05-06T00:00:00Z",
            )
            .await?;

        Ok((
            AppContext::new(worker),
            guest.user_id,
            guest_token,
            viewer.user_id,
            viewer_token,
        ))
    }

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

    #[test]
    fn parse_bot_takeover_toggle() {
        let parsed =
            parse_client_message(r#"{"type":"set_bot_takeover","payload":{"enabled":true}}"#)
                .expect("set_bot_takeover should parse");

        assert!(matches!(parsed, ClientMessage::SetBotTakeover(request) if request.enabled));
    }

    #[test]
    fn parse_join_table_with_session_token() {
        let parsed = parse_client_message(
            r#"{"type":"join_table","payload":{"session_token":"token-123"}}"#,
        )
        .expect("join_table should parse");

        assert!(
            matches!(parsed, ClientMessage::JoinTable(request) if request.session_token == "token-123")
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn invite_only_join_table_rejects_uninvited_user() -> Result<()> {
        let db = in_memory_database("")?;
        db.initialize()?;
        let worker = DbWorker::start(db)?;
        worker
            .create_invite_code("INVITE200001", "2026-05-06T00:00:00Z", None)
            .await?;
        let owner_token = generate_session_token();
        let owner = worker
            .register_user(
                "Owner",
                "Owner",
                &hash_password("secret-123")?,
                "INVITE200001",
                &hash_session_token(&owner_token),
                "2026-05-06T00:00:00Z",
            )
            .await?;
        worker
            .create_invite_code("INVITE200002", "2026-05-06T00:00:00Z", None)
            .await?;
        let guest_token = generate_session_token();
        let _guest = worker
            .register_user(
                "Guest",
                "Guest",
                &hash_password("secret-123")?,
                "INVITE200002",
                &hash_session_token(&guest_token),
                "2026-05-06T00:00:00Z",
            )
            .await?;
        let state = AppContext::new(worker.clone());
        let room = initial_room_state_with_owner("ROOM42", Some(owner.user_id), 1);
        let room_json = serialize_room_state(&room)?;
        worker
            .save_table("ROOM42", "2026-05-06T00:00:00Z", &room_json)
            .await?;

        let (connection, _receiver) = test_connection_handle(1, 4);
        let outcome = handle_join_table(
            state,
            "ROOM42",
            &connection,
            JoinTableRequest {
                session_token: guest_token,
            },
        )
        .await;

        assert!(outcome.role.is_none());
        let payload: Value = serde_json::from_str(&outcome.outbound[0].payload)?;
        assert_eq!(payload["payload"]["reason"], "table_invite_required");
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn invite_only_join_table_allows_reserved_participant() -> Result<()> {
        let (state, guest_token) = build_reserved_participant_state("ROOM42").await?;
        let (connection, _receiver) = test_connection_handle(1, 8);
        let outcome = handle_join_table(
            state.clone(),
            "ROOM42",
            &connection,
            JoinTableRequest {
                session_token: guest_token,
            },
        )
        .await;

        assert!(matches!(
            outcome.role,
            Some(ConnectionRole::Player { seat_index: 0 })
        ));
        let room_handle = room_handle(&state, "ROOM42")
            .await
            .expect("room should be loaded");
        let runtime = room_handle.runtime.lock().await;
        assert!(runtime.room.seats[0].connected);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn multi_device_join_table_allows_actions_from_both_connections() -> Result<()> {
        let (state, guest_token) = build_reserved_participant_state("ROOM52").await?;
        let (first_connection, _first_receiver) = test_connection_handle(1, 8);
        let (second_connection, _second_receiver) = test_connection_handle(2, 8);

        let first_join = handle_join_table(
            state.clone(),
            "ROOM52",
            &first_connection,
            JoinTableRequest {
                session_token: guest_token.clone(),
            },
        )
        .await;
        assert!(matches!(
            first_join.role,
            Some(ConnectionRole::Player { seat_index: 0 })
        ));

        let second_join = handle_join_table(
            state.clone(),
            "ROOM52",
            &second_connection,
            JoinTableRequest {
                session_token: guest_token,
            },
        )
        .await;
        assert!(matches!(
            second_join.role,
            Some(ConnectionRole::Player { seat_index: 0 })
        ));
        assert!(
            second_join
                .outbound
                .iter()
                .any(|message| message.connection.id == 1)
        );
        assert!(
            second_join
                .outbound
                .iter()
                .any(|message| message.connection.id == 2)
        );

        let second_ready = handle_client_message(
            state.clone(),
            "ROOM52",
            &second_connection,
            ConnectionRole::Player { seat_index: 0 },
            ClientMessage::Ready(ReadyRequest { ready: true }),
        )
        .await;
        assert!(
            second_ready
                .outbound
                .iter()
                .any(|message| message.connection.id == 1)
        );
        assert!(
            second_ready
                .outbound
                .iter()
                .any(|message| message.connection.id == 2)
        );

        let first_ready = handle_client_message(
            state.clone(),
            "ROOM52",
            &first_connection,
            ConnectionRole::Player { seat_index: 0 },
            ClientMessage::Ready(ReadyRequest { ready: false }),
        )
        .await;
        assert!(
            first_ready
                .outbound
                .iter()
                .any(|message| message.connection.id == 1)
        );
        assert!(
            first_ready
                .outbound
                .iter()
                .any(|message| message.connection.id == 2)
        );

        let room_handle = room_handle(&state, "ROOM52")
            .await
            .expect("room should be loaded");
        let runtime = room_handle.runtime.lock().await;
        assert_eq!(
            runtime
                .connections
                .get(&0)
                .map(|group| group.connections.len()),
            Some(2)
        );
        assert!(!runtime.room.seats[0].ready);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn multi_device_disconnect_only_marks_seat_offline_after_last_connection() -> Result<()> {
        let (state, guest_token) = build_reserved_participant_state("ROOM62").await?;
        let (first_connection, _first_receiver) = test_connection_handle(1, 8);
        let (second_connection, _second_receiver) = test_connection_handle(2, 8);

        let _ = handle_join_table(
            state.clone(),
            "ROOM62",
            &first_connection,
            JoinTableRequest {
                session_token: guest_token.clone(),
            },
        )
        .await;
        let _ = handle_join_table(
            state.clone(),
            "ROOM62",
            &second_connection,
            JoinTableRequest {
                session_token: guest_token,
            },
        )
        .await;

        handle_disconnect(state.clone(), "ROOM62", Some(0), 1).await;
        let room_handle = room_handle(&state, "ROOM62")
            .await
            .expect("room should be loaded");
        {
            let runtime = room_handle.runtime.lock().await;
            assert!(runtime.room.seats[0].connected);
            assert_eq!(
                runtime
                    .connections
                    .get(&0)
                    .map(|group| group.connections.len()),
                Some(1)
            );
        }

        handle_disconnect(state.clone(), "ROOM62", Some(0), 2).await;
        let runtime = room_handle.runtime.lock().await;
        assert!(!runtime.room.seats[0].connected);
        assert!(runtime.connections.get(&0).is_none());
        Ok(())
    }

    #[test]
    fn parse_watch_table_message() {
        let parsed = parse_client_message(
            r#"{"type":"watch_table","payload":{"session_token":"token-456","nickname":"Viewer"}}"#,
        )
        .expect("watch_table should parse");
        assert!(matches!(
            parsed,
            ClientMessage::WatchTable(request)
                if request.session_token == "token-456" && request.nickname == "Viewer"
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn spectator_player_in_same_table_cannot_watch_own_table() -> Result<()> {
        let (state, _guest_user_id, guest_token, _viewer_user_id, _viewer_token) =
            build_watch_state("ROOM72").await?;
        let (connection, _receiver) = test_connection_handle(1, 8);

        let outcome = handle_watch_table(
            state,
            "ROOM72",
            &connection,
            WatchTableRequest {
                session_token: guest_token,
                nickname: "GuestWatch".to_string(),
            },
        )
        .await;

        let payload: Value = serde_json::from_str(&outcome.outbound[0].payload)?;
        assert_eq!(
            payload["payload"]["reason"],
            "player_cannot_watch_own_table"
        );
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn spectator_watch_table_requires_owner_approval() -> Result<()> {
        let (state, _guest_user_id, _guest_token, _viewer_user_id, viewer_token) =
            build_watch_state("ROOM82").await?;
        let (connection, _receiver) = test_connection_handle(1, 8);

        let outcome = handle_watch_table(
            state,
            "ROOM82",
            &connection,
            WatchTableRequest {
                session_token: viewer_token,
                nickname: "ViewerWatch".to_string(),
            },
        )
        .await;

        let payload: Value = serde_json::from_str(&outcome.outbound[0].payload)?;
        assert_eq!(
            payload["payload"]["reason"],
            "spectator_requires_owner_approval"
        );
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn spectator_approved_request_allows_watch_table() -> Result<()> {
        let (state, _guest_user_id, _guest_token, viewer_user_id, viewer_token) =
            build_watch_state("ROOM92").await?;
        state
            .inner
            .db
            .create_spectator_request("ROOM92", viewer_user_id, 1, "2026-05-06T00:10:00Z")
            .await?;
        state
            .inner
            .db
            .decide_spectator_request(1, 1, true, "2026-05-06T00:11:00Z")
            .await?;
        let (connection, _receiver) = test_connection_handle(1, 8);

        let outcome = handle_watch_table(
            state,
            "ROOM92",
            &connection,
            WatchTableRequest {
                session_token: viewer_token,
                nickname: "ViewerWatch".to_string(),
            },
        )
        .await;

        assert!(matches!(
            outcome.role,
            Some(ConnectionRole::Spectator { .. })
        ));
        Ok(())
    }
}
