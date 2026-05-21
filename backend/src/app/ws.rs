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
use super::evaluation::apply_room_result_to_evaluation_session;
use super::protocol::{
    HeartbeatPayload, action_rejected_message, dealer_selection_started_message, heartbeat_message,
    leave_table_accepted_message, quick_chat_message,
};
use super::records::{apply_point_updates_to_room, archive_current_round_if_needed};
use super::room_runtime::{
    PendingStartMatch, add_seat_connection, broadcast_to_seat_group, close_runtime,
    connection_current_seat, ensure_room_loaded, remap_connections_to_current_seats,
    remove_all_seat_connections, remove_seat_connection, restore_room_snapshot, room_handle,
    room_has_only_bots, seat_group_contains_connection, should_terminate_unattended,
    snapshot_connections, unregister_room_handle,
};
use super::scheduler::schedule_room_tasks_detached;
use super::users::title_for_points;
use super::{
    AppContext, ConnectionHandle, OUTBOUND_CHANNEL_CAPACITY, OutboundMessage,
    add_bot_to_waiting_room, collect_join_outbound_from_snapshot,
    collect_snapshot_and_prompt_outbound_from_snapshot, convert_seat_to_bot, generate_short_hex,
    normalize_table_code, notify_all_user_connections, occupied_seats,
    presence_and_snapshot_for_all_from_snapshot, random_open_seat_index,
    remove_bot_from_waiting_room, remove_seat_from_room, reset_timeout_auto_response_count,
    room_has_round_state, room_phase, room_seats, seat_exists, send_outbound, serialize_room,
    set_seat_bot_takeover, set_seat_connected, user_active_table_updated_message,
};
use crate::core::engine::try_handle_player_action_in_room_state;
use crate::core::state::{RoomState, SeatState};
use crate::rules::standard::flow::{
    reconcile_continue_action_state_in_room_state as reconcile_standard_continue_action_state,
    record_continue_action_in_room_state as record_standard_continue_action,
    room_ready_to_start as room_ready_to_start_in_state,
};

const DEALER_SELECTION_DURATION_MS: u64 = 4_200;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
enum ClientMessage {
    JoinTable(JoinTableRequest),
    AdjustBots(AdjustBotsRequest),
    SetMinimumHuFan(SetMinimumHuFanRequest),
    SetDealerRepeat(SetRuleToggleRequest),
    SetDealerDouble(SetRuleToggleRequest),
    SetBotTakeover(SetBotTakeoverRequest),
    StartMatch,
    StartNextRound,
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
    chat_kind: Option<String>,
    #[serde(default)]
    emoji: String,
}

#[derive(Debug, Default, Deserialize)]
struct AdjustBotsRequest {
    #[serde(default)]
    delta: i64,
}

#[derive(Debug, Default, Deserialize)]
struct SetMinimumHuFanRequest {
    #[serde(default)]
    minimum_hu_fan: i64,
}

#[derive(Debug, Default, Deserialize)]
struct SetRuleToggleRequest {
    #[serde(default)]
    enabled: bool,
}

#[derive(Debug, Default, Deserialize)]
struct SetBotTakeoverRequest {
    #[serde(default)]
    enabled: bool,
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
}

impl ConnectionRole {
    fn owned_seat(self) -> Option<usize> {
        match self {
            Self::Player { seat_index } => Some(seat_index),
            Self::Unbound => None,
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
        ClientMessage::JoinTable(request) => {
            if !matches!(role, ConnectionRole::Unbound) {
                return reject_to(connection, "seat_already_owned");
            }
            handle_join_table(state, table_code, connection, request).await
        }
        ClientMessage::AdjustBots(request) => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, role.owned_seat()).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_adjust_bots(state, table_code, connection, seat_index, request).await
        }
        ClientMessage::SetMinimumHuFan(request) => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, role.owned_seat()).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_set_minimum_hu_fan(state, table_code, connection, seat_index, request).await
        }
        ClientMessage::SetDealerRepeat(request) => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, role.owned_seat()).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_set_dealer_rule_toggle(
                state,
                table_code,
                connection,
                seat_index,
                request,
                DealerRuleToggle::Repeat,
            )
            .await
        }
        ClientMessage::SetDealerDouble(request) => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, role.owned_seat()).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_set_dealer_rule_toggle(
                state,
                table_code,
                connection,
                seat_index,
                request,
                DealerRuleToggle::Double,
            )
            .await
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
            let ConnectionRole::Player { seat_index } = role else {
                return reject_to(connection, "seat_not_owned");
            };
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, Some(seat_index)).await
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

fn normalize_minimum_hu_fan(value: i64) -> Option<i64> {
    [0, 2, 4, 6, 8].contains(&value).then_some(value)
}

fn room_is_evaluation(room: &RoomState) -> bool {
    room.mode == crate::evaluation::EVALUATION_ROOM_MODE
}

#[derive(Debug, Clone, Copy)]
enum DealerRuleToggle {
    Repeat,
    Double,
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
    } else if let Some(current_seat) = connection_current_seat(&runtime, connection.id) {
        Some(current_seat)
    } else {
        None
    }
}
fn current_seat_index_for_user(
    room: &RoomState,
    user_id: i64,
    fallback_seat_index: usize,
) -> Option<usize> {
    room.seats
        .iter()
        .find(|seat| seat.user_id == Some(user_id))
        .map(|seat| seat.seat_index)
        .or_else(|| {
            room.seats
                .iter()
                .find(|seat| {
                    seat.seat_index == fallback_seat_index
                        && (seat.user_id.is_none() || seat.user_id == Some(user_id))
                })
                .map(|seat| seat.seat_index)
        })
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
        return reject_to(connection, "table_closed");
    }
    let _persist_guard = room_handle.persist.lock().await;
    let mut runtime = room_handle.runtime.lock().await;
    if room_handle.is_closed() {
        return reject_to(connection, "table_closed");
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

    let (seat_index, persisted_with_new_participant) =
        if let Some(participant) = existing_participant {
            let Some(seat_index) = current_seat_index_for_user(
                &runtime.room,
                authenticated_user.user_id,
                participant.seat_index,
            ) else {
                return reject_to(connection, "table_invite_required");
            };
            let Some(seat) = runtime
                .room
                .seats
                .iter_mut()
                .find(|seat| seat.seat_index == seat_index)
            else {
                return reject_to(connection, "table_invite_required");
            };
            seat.user_id = Some(user.user_id);
            seat.nickname = Some(user.display_name.clone());
            seat.points = Some(user.points);
            seat.title = Some(title_for_points(user.points).to_string());
            seat.connected = true;
            seat.disconnect_deadline_at = None;
            seat.is_bot = false;
            seat.seat_type = "human".to_string();
            seat.consecutive_timeout_auto_response_count = 0;
            (seat_index, false)
        } else if runtime.room.owner_user_id == Some(authenticated_user.user_id)
            && room_phase(&runtime.room) == "waiting"
            && !room_has_round_state(&runtime.room)
        {
            let Some(seat_index) = random_open_seat_index(&runtime.room) else {
                return reject_to(connection, "table_full");
            };
            runtime.room.seats.push(SeatState {
                seat_index,
                user_id: Some(user.user_id),
                nickname: Some(user.display_name.clone()),
                points: Some(user.points),
                title: Some(title_for_points(user.points).to_string()),
                connected: true,
                is_bot: false,
                seat_type: "human".to_string(),
                bot_persona: None,
                bot_aggression: None,
                disconnect_deadline_at: None,
                consecutive_timeout_auto_response_count: 0,
            });
            runtime.room.seats.sort_by_key(|seat| seat.seat_index);
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
                .save_table_and_upsert_participant(
                    table_code,
                    &created_at,
                    &room_json,
                    seat_index,
                    authenticated_user.user_id,
                    &user.display_name,
                    &created_at,
                )
                .await
            {
                restore_room_snapshot(&room_handle, previous_room).await;
                return internal_error_to(connection, error);
            }
            notify_all_user_connections(
                &state,
                user_active_table_updated_message(
                    authenticated_user.user_id,
                    Some(table_code),
                    Some(&room.phase),
                ),
            )
            .await;
            let outbound = collect_join_outbound_from_snapshot(
                &room,
                &connections,
                table_code,
                connection,
                seat_index,
                true,
            );
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
    let outbound = collect_join_outbound_from_snapshot(
        &room,
        &connections,
        table_code,
        connection,
        seat_index,
        true,
    );
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
    if room_is_evaluation(&runtime.room) {
        return reject_to(connection, "evaluation_settings_locked");
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
        role: None,
        clear_role: false,
        close_socket: false,
    };
    schedule_room_tasks_detached(state, table_code.to_string());
    outcome
}

async fn handle_set_minimum_hu_fan(
    state: AppContext,
    table_code: &str,
    connection: &ConnectionHandle,
    seat_index: usize,
    request: SetMinimumHuFanRequest,
) -> MessageOutcome {
    let Some(minimum_hu_fan) = normalize_minimum_hu_fan(request.minimum_hu_fan) else {
        return reject_to(connection, "invalid_minimum_hu_fan");
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
    if !seat_exists(&runtime.room, seat_index) {
        return reject_to(connection, "seat_not_owned");
    }
    if room_is_evaluation(&runtime.room) {
        return reject_to(connection, "evaluation_settings_locked");
    }
    if room_phase(&runtime.room) != "waiting" || room_has_round_state(&runtime.room) {
        return reject_to(connection, "room_already_started");
    }

    runtime.room.minimum_hu_fan = minimum_hu_fan;

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
        role: None,
        clear_role: false,
        close_socket: false,
    };
    schedule_room_tasks_detached(state, table_code.to_string());
    outcome
}

async fn handle_set_dealer_rule_toggle(
    state: AppContext,
    table_code: &str,
    connection: &ConnectionHandle,
    seat_index: usize,
    request: SetRuleToggleRequest,
    toggle: DealerRuleToggle,
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
    if !seat_exists(&runtime.room, seat_index) {
        return reject_to(connection, "seat_not_owned");
    }
    if room_is_evaluation(&runtime.room) {
        return reject_to(connection, "evaluation_settings_locked");
    }
    if room_phase(&runtime.room) != "waiting" || room_has_round_state(&runtime.room) {
        return reject_to(connection, "room_already_started");
    }

    match toggle {
        DealerRuleToggle::Repeat => runtime.room.dealer_repeat_enabled = request.enabled,
        DealerRuleToggle::Double => runtime.room.dealer_double_enabled = request.enabled,
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
    let evaluation_seed = evaluation_seed_for_table(&state, table_code).await;
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
    let dealer_seat = if evaluation_seed.is_some() {
        crate::evaluation::EVALUATION_INITIAL_SUBJECT_SEAT
    } else {
        let occupied: Vec<usize> = occupied.into_iter().collect();
        let mut rng = rand::rng();
        occupied[rng.random_range(0..occupied.len())]
    };
    let started_at = super::now_iso();
    let reveal_at = dealer_selection_reveal_at();
    runtime.pending_start_match = Some(PendingStartMatch {
        dealer_seat,
        reveal_at: reveal_at.clone(),
        seed: evaluation_seed,
    });
    let connections = snapshot_connections(&runtime);
    drop(runtime);
    let selection_message = dealer_selection_started_message(
        dealer_seat,
        started_at,
        reveal_at,
        DEALER_SELECTION_DURATION_MS,
    );
    let outbound = connections
        .into_iter()
        .map(|(_, handle)| handle.outbound(selection_message.clone()))
        .collect::<Vec<_>>();
    let outcome = MessageOutcome {
        outbound,
        role: None,
        clear_role: false,
        close_socket: false,
    };
    schedule_room_tasks_detached(state, table_code.to_string());
    outcome
}

async fn evaluation_seed_for_table(state: &AppContext, table_code: &str) -> Option<u64> {
    let sessions = state.inner.evaluation_sessions.read().await;
    sessions
        .values()
        .find(|session| {
            session
                .subjects
                .iter()
                .any(|subject| subject.table_code == table_code)
        })
        .map(|session| session.seed)
}

async fn freeze_evaluation_result_for_room(state: &AppContext, room: &RoomState) {
    if room.mode != crate::evaluation::EVALUATION_ROOM_MODE {
        return;
    }
    let Some(match_state) = room.match_state.as_ref() else {
        return;
    };
    if !match_state.match_finished
        && (match_state.statistics.completed_round_count as usize)
            < crate::evaluation::EVALUATION_HAND_COUNT
    {
        return;
    }

    let mut sessions = state.inner.evaluation_sessions.write().await;
    for session in sessions.values_mut() {
        apply_room_result_to_evaluation_session(session, room);
    }
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
    remap_connections_to_current_seats(&mut runtime, &previous_room);
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
    reset_timeout_auto_response_count(&mut runtime.room, seat_index);

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
    let archive_outcome = match archive_current_round_if_needed(
        &state,
        &room,
        &created_at,
        &super::now_iso(),
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(error) => {
            eprintln!("failed to archive round for table {table_code}: {error:#}");
            None
        }
    };
    let mut runtime = room_handle.runtime.lock().await;
    let point_updates = archive_outcome
        .as_ref()
        .map(|outcome| outcome.point_updates.as_slice())
        .unwrap_or(&[]);
    let room_points_changed = apply_point_updates_to_room(&mut runtime.room, point_updates);
    if room_points_changed {
        let room_json = match serialize_room(&runtime.room) {
            Ok(value) => value,
            Err(error) => {
                drop(runtime);
                return internal_error_to(connection, error);
            }
        };
        drop(runtime);
        if let Err(error) = state
            .inner
            .db
            .save_table(table_code, &created_at, &room_json)
            .await
        {
            eprintln!("failed to persist updated seat points for table {table_code}: {error:#}");
        }
        runtime = room_handle.runtime.lock().await;
    }
    let connections = snapshot_connections(&runtime);
    let broadcast_handles = connections
        .iter()
        .map(|(_, handle)| handle.clone())
        .collect::<Vec<_>>();
    let room = runtime.room.clone();
    let snapshot_outbound = collect_snapshot_and_prompt_outbound_from_snapshot(&room, &connections);
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
    let chat_kind = request.chat_kind.as_deref().map(str::trim);
    let emoji = request.emoji.trim().to_string();
    if emoji.is_empty() && chat_kind != Some("point_gesture") {
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
    if chat_kind == Some("point_gesture") {
        let actor_points = runtime
            .room
            .seats
            .iter()
            .find(|seat| seat.seat_index == seat_index)
            .and_then(|seat| seat.points);
        let target_points = runtime
            .room
            .seats
            .iter()
            .find(|seat| seat.seat_index == target_seat)
            .and_then(|seat| seat.points);
        if seat_index == target_seat
            || actor_points.is_none()
            || target_points.is_none()
            || actor_points == target_points
        {
            return reject_to(connection, "invalid_action");
        }
    }

    let payload = quick_chat_message(
        generate_short_hex(8),
        seat_index,
        target_seat,
        chat_kind,
        None,
        None,
        if chat_kind == Some("point_gesture") {
            "point_gesture".to_string()
        } else {
            emoji
        },
        super::now_iso(),
    );
    let connections = snapshot_connections(&runtime);
    drop(runtime);
    let outbound = connections
        .into_iter()
        .map(|(_, handle)| handle.outbound(payload.clone()))
        .collect::<Vec<_>>();
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
    let leaving_user_id = state
        .inner
        .db
        .list_active_table_participants_for_table(table_code)
        .await
        .ok()
        .and_then(|participants| {
            participants
                .into_iter()
                .find(|participant| participant.seat_index == seat_index)
                .map(|participant| participant.user_id)
        });
    let _persist_guard = room_handle.persist.lock().await;
    let mut runtime = room_handle.runtime.lock().await;
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }
    let previous_room = runtime.room.clone();
    let created_at = runtime.created_at.clone();
    let phase = room_phase(&runtime.room);
    if phase == "waiting" && !can_leave_waiting_table(&runtime.room, leaving_user_id) {
        return reject_to(connection, "cannot_leave_empty_table");
    }
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
            if let Some(user_id) = leaving_user_id {
                notify_all_user_connections(
                    &state,
                    user_active_table_updated_message(user_id, None, None),
                )
                .await;
            }
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
                .save_table_and_mark_participant_left(
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
            if let Some(user_id) = leaving_user_id {
                notify_all_user_connections(
                    &state,
                    user_active_table_updated_message(user_id, None, None),
                )
                .await;
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
        let room = runtime.room.clone();
        room_handle.mark_closed();
        close_runtime(&mut runtime);
        drop(runtime);
        unregister_room_handle(&state, table_code, &room_handle).await;
        freeze_evaluation_result_for_room(&state, &room).await;
        state.inner.db.delete_table(table_code, &left_at).await.ok();
        if let Some(user_id) = leaving_user_id {
            notify_all_user_connections(
                &state,
                user_active_table_updated_message(user_id, None, None),
            )
            .await;
        }
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
            .save_table_and_mark_participant_left(
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
        if let Some(user_id) = leaving_user_id {
            notify_all_user_connections(
                &state,
                user_active_table_updated_message(user_id, None, None),
            )
            .await;
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

fn can_leave_waiting_table(room: &RoomState, leaving_user_id: Option<i64>) -> bool {
    room.seats.iter().any(|seat| {
        if Some(seat.user_id.unwrap_or_default()) == leaving_user_id && seat.user_id.is_some() {
            return false;
        }

        crate::special_bots::is_independent_bot_seat(seat)
            || crate::special_bots::is_special_bot_seat(seat)
            || (seat.seat_type == "human" && seat.user_id.is_some())
    })
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
    let seat_index = connection_current_seat(&runtime, connection_id).unwrap_or(seat_index);
    if !seat_group_contains_connection(&runtime, seat_index, connection_id) {
        return;
    }
    if remove_seat_connection(&mut runtime, seat_index, connection_id) {
        return;
    }
    set_seat_connected(&mut runtime.room, seat_index, false);
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
    let runtime = room_handle.runtime.lock().await;
    drop(runtime);
    send_outbound(outbound);
    schedule_room_tasks_detached(state, table_code.to_string());
}
#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use anyhow::Result;
    use serde_json::Value;
    use tokio::sync::{Notify, mpsc};

    use super::{
        ClientMessage, ConnectionRole, JoinTableRequest, QuickChatRequest, SetMinimumHuFanRequest,
        handle_disconnect, handle_join_table, handle_leave_table, handle_quick_chat,
        handle_set_minimum_hu_fan, handle_start_match, parse_client_message, room_is_evaluation,
    };
    use crate::app::auth::{generate_session_token, hash_password, hash_session_token};
    use crate::app::evaluation::{
        EvaluationSessionResponse, EvaluationSubjectResponse, build_evaluation_room,
    };
    use crate::app::persistence::{DbWorker, in_memory_database};
    use crate::app::room_runtime::{ensure_room_loaded, room_handle};
    use crate::app::{
        AppContext, ConnectionHandle, initial_room_state_with_owner, serialize_room_state,
    };
    use crate::core::state::SeatState;
    use crate::rules::standard::flow::start_match_in_room_state;

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

    struct TestSeat<'a> {
        seat_index: usize,
        user_id: i64,
        nickname: &'a str,
        connected: bool,
    }

    struct StaleSeatRow<'a> {
        table_code: &'a str,
        room_json: &'a str,
        stale_seat_index: usize,
        user_id: i64,
        nickname: &'a str,
    }

    fn human_seat(input: TestSeat<'_>) -> SeatState {
        SeatState {
            seat_index: input.seat_index,
            user_id: Some(input.user_id),
            nickname: Some(input.nickname.to_string()),
            points: Some(600),
            title: Some("正分守门员".to_string()),
            connected: input.connected,
            is_bot: false,
            seat_type: "human".to_string(),
            bot_persona: None,
            bot_aggression: None,
            disconnect_deadline_at: None,
            consecutive_timeout_auto_response_count: 0,
        }
    }

    async fn register_ws_test_user(
        worker: &DbWorker,
        invite_code: &str,
        display_name: &str,
    ) -> Result<(i64, String)> {
        worker
            .create_invite_code(invite_code, "2026-05-06T00:00:00Z", None)
            .await?;
        let session_token = generate_session_token();
        let user = worker
            .register_user(
                display_name,
                display_name,
                &hash_password("secret-123")?,
                invite_code,
                &hash_session_token(&session_token),
                "2026-05-06T00:00:00Z",
            )
            .await?;
        Ok((user.user_id, session_token))
    }

    async fn persist_stale_rotated_seat(worker: &DbWorker, row: StaleSeatRow<'_>) -> Result<()> {
        worker
            .save_table_and_upsert_participant(
                row.table_code,
                "2026-05-06T00:00:00Z",
                row.room_json,
                row.stale_seat_index,
                row.user_id,
                row.nickname,
                "2026-05-06T00:00:00Z",
            )
            .await
    }

    async fn build_loaded_room_with_stale_seat_indexes(
        table_code: &str,
    ) -> Result<(AppContext, DbWorker, String, i64, i64)> {
        let db = in_memory_database("")?;
        db.initialize()?;
        let worker = DbWorker::start(db)?;
        let (other_user_id, _) =
            register_ws_test_user(&worker, "INVITE200008", "OtherRotated").await?;
        let (guest_user_id, guest_token) =
            register_ws_test_user(&worker, "INVITE200009", "GuestRotated").await?;

        let mut room = initial_room_state_with_owner(table_code, Some(other_user_id), 1);
        room.phase = "playing".to_string();
        room.seats.push(human_seat(TestSeat {
            seat_index: 0,
            user_id: other_user_id,
            nickname: "OtherRotated",
            connected: true,
        }));
        room.seats.push(human_seat(TestSeat {
            seat_index: 1,
            user_id: guest_user_id,
            nickname: "GuestRotated",
            connected: false,
        }));
        let room_json = serialize_room_state(&room)?;
        worker
            .save_table(table_code, "2026-05-06T00:00:00Z", &room_json)
            .await?;

        let state = AppContext::new(worker.clone());
        ensure_room_loaded(&state, table_code)
            .await?
            .expect("room should load before stale db rows are inserted");

        persist_stale_rotated_seat(
            &worker,
            StaleSeatRow {
                table_code,
                room_json: &room_json,
                stale_seat_index: 0,
                user_id: guest_user_id,
                nickname: "GuestRotated",
            },
        )
        .await?;
        persist_stale_rotated_seat(
            &worker,
            StaleSeatRow {
                table_code,
                room_json: &room_json,
                stale_seat_index: 1,
                user_id: other_user_id,
                nickname: "OtherRotated",
            },
        )
        .await?;

        Ok((state, worker, guest_token, guest_user_id, other_user_id))
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
            user_id: Some(guest.user_id),
            nickname: Some("Guest".to_string()),
            points: Some(600),
            title: Some("正分守门员".to_string()),
            connected: false,
            is_bot: false,
            seat_type: "human".to_string(),
            bot_persona: None,
            bot_aggression: None,
            disconnect_deadline_at: None,
            consecutive_timeout_auto_response_count: 0,
        });
        let room_json = serialize_room_state(&room)?;
        worker
            .save_table_and_upsert_participant(
                table_code,
                "2026-05-06T00:00:00Z",
                &room_json,
                0,
                guest.user_id,
                "Guest",
                "2026-05-06T00:00:00Z",
            )
            .await?;

        Ok((AppContext::new(worker), guest_token))
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
    fn parse_set_minimum_hu_fan() {
        let parsed =
            parse_client_message(r#"{"type":"set_minimum_hu_fan","payload":{"minimum_hu_fan":4}}"#)
                .expect("set_minimum_hu_fan should parse");

        assert!(matches!(
            parsed,
            ClientMessage::SetMinimumHuFan(request) if request.minimum_hu_fan == 4
        ));
    }

    #[test]
    fn parse_dealer_rule_toggles() {
        let dealer_repeat =
            parse_client_message(r#"{"type":"set_dealer_repeat","payload":{"enabled":true}}"#)
                .expect("set_dealer_repeat should parse");
        let dealer_double =
            parse_client_message(r#"{"type":"set_dealer_double","payload":{"enabled":false}}"#)
                .expect("set_dealer_double should parse");

        assert!(matches!(
            dealer_repeat,
            ClientMessage::SetDealerRepeat(request) if request.enabled
        ));
        assert!(matches!(
            dealer_double,
            ClientMessage::SetDealerDouble(request) if !request.enabled
        ));
    }

    #[test]
    fn evaluation_mode_is_detected_for_restricted_table_settings() {
        let mut room = crate::app::initial_room_state("EVALROOM");
        crate::evaluation::apply_evaluation_rules(&mut room);

        assert!(room_is_evaluation(&room));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn evaluation_room_rejects_minimum_hu_fan_changes() -> Result<()> {
        let db = in_memory_database("")?;
        db.initialize()?;
        let worker = DbWorker::start(db)?;
        let state = AppContext::new(worker.clone());
        let mut room = initial_room_state_with_owner("EVALLOCK", Some(1), 1);
        crate::evaluation::apply_evaluation_rules(&mut room);
        room.seats.push(human_seat(TestSeat {
            seat_index: 0,
            user_id: 1,
            nickname: "Alice",
            connected: true,
        }));
        let room_json = serialize_room_state(&room)?;
        worker
            .save_table("EVALLOCK", "2026-05-06T00:00:00Z", &room_json)
            .await?;
        ensure_room_loaded(&state, "EVALLOCK").await?;
        let (connection, _receiver) = test_connection_handle(1, 8);

        let outcome = handle_set_minimum_hu_fan(
            state,
            "EVALLOCK",
            &connection,
            0,
            SetMinimumHuFanRequest { minimum_hu_fan: 0 },
        )
        .await;

        let payload: Value = serde_json::from_str(&outcome.outbound[0].payload)?;
        assert_eq!(payload["type"], "action_rejected");
        assert_eq!(payload["payload"]["reason"], "evaluation_settings_locked");
        Ok(())
    }

    #[test]
    fn reject_deprecated_ready_message() {
        let result = parse_client_message(r#"{"type":"ready","payload":{"ready":true}}"#);

        assert!(result.is_err());
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
    async fn evaluation_start_match_uses_shared_session_seed_and_fixed_subject_seat() -> Result<()>
    {
        let db = in_memory_database("")?;
        db.initialize()?;
        let worker = DbWorker::start(db)?;
        let state = AppContext::new(worker.clone());
        let (owner_user_id, _owner_token) =
            register_ws_test_user(&worker, "INVITE200030", "EvalOwner").await?;
        let (first_user_id, first_token) =
            register_ws_test_user(&worker, "INVITE200031", "FirstSubject").await?;
        let (second_user_id, second_token) =
            register_ws_test_user(&worker, "INVITE200032", "SecondSubject").await?;
        let seed = 4242;
        let subjects = [
            ("EVALFAIR1", first_user_id, first_token, "FirstSubject"),
            ("EVALFAIR2", second_user_id, second_token, "SecondSubject"),
        ];
        state.inner.evaluation_sessions.write().await.insert(
            "eval-fair".to_string(),
            crate::app::evaluation::EvaluationSessionResponse {
                evaluation_id: "eval-fair".to_string(),
                seed,
                subjects: subjects
                    .iter()
                    .map(|(table_code, user_id, _, name)| {
                        crate::app::evaluation::EvaluationSubjectResponse {
                            subject_id: format!("user:{user_id}"),
                            user_id: Some(*user_id),
                            display_name: (*name).to_string(),
                            kind: "human".to_string(),
                            table_code: (*table_code).to_string(),
                            phase: "waiting".to_string(),
                            completed: false,
                            final_score: None,
                            deal_in_count: None,
                            win_count: None,
                            completed_round_count: None,
                        }
                    })
                    .collect(),
            },
        );
        for (table_code, user_id, _, name) in &subjects {
            let room = crate::app::evaluation::build_evaluation_room(
                table_code,
                owner_user_id,
                Some(*user_id),
                name,
                false,
            );
            let room_json = serialize_room_state(&room)?;
            worker
                .save_table_and_upsert_participant(
                    table_code,
                    "2026-05-06T00:00:00Z",
                    &room_json,
                    0,
                    *user_id,
                    name,
                    "2026-05-06T00:00:00Z",
                )
                .await?;
        }

        for (index, (table_code, _, token, _)) in subjects.iter().enumerate() {
            let (connection, _receiver) = test_connection_handle(index as u64 + 1, 8);
            let join = handle_join_table(
                state.clone(),
                table_code,
                &connection,
                JoinTableRequest {
                    session_token: token.clone(),
                },
            )
            .await;
            assert!(matches!(
                join.role,
                Some(ConnectionRole::Player { seat_index: 0 })
            ));

            let outcome = handle_start_match(state.clone(), table_code, &connection, 0).await;
            assert!(
                outcome
                    .outbound
                    .iter()
                    .any(|message| message.payload.contains("dealer_selection_started"))
            );
        }

        for (table_code, _, _, _) in subjects {
            let handle = room_handle(&state, table_code)
                .await
                .expect("evaluation room should be loaded");
            let runtime = handle.runtime.lock().await;
            let pending_start = runtime
                .pending_start_match
                .as_ref()
                .expect("evaluation start should be pending");
            assert_eq!(
                pending_start.dealer_seat,
                crate::evaluation::EVALUATION_INITIAL_SUBJECT_SEAT
            );
            assert_eq!(pending_start.seed, Some(seed));
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn leave_table_rejects_single_player_waiting_table() -> Result<()> {
        let (state, guest_token) = build_reserved_participant_state("ROOMSOLO").await?;
        let (connection, _receiver) = test_connection_handle(1, 8);
        let join = handle_join_table(
            state.clone(),
            "ROOMSOLO",
            &connection,
            JoinTableRequest {
                session_token: guest_token,
            },
        )
        .await;
        assert!(matches!(
            join.role,
            Some(ConnectionRole::Player { seat_index: 0 })
        ));

        let outcome = handle_leave_table(state, "ROOMSOLO", &connection, 0).await;
        let payload: Value = serde_json::from_str(&outcome.outbound[0].payload)?;

        assert_eq!(payload["payload"]["reason"], "cannot_leave_empty_table");
        assert!(!outcome.close_socket);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn leave_table_cleans_finished_human_evaluation_room_and_freezes_result() -> Result<()> {
        let db = in_memory_database("")?;
        db.initialize()?;
        let worker = DbWorker::start(db)?;
        let (owner_user_id, _owner_token) =
            register_ws_test_user(&worker, "INVITE200020", "EvalOwner").await?;
        let (subject_user_id, subject_token) =
            register_ws_test_user(&worker, "INVITE200021", "EvalSubject").await?;
        let mut room = build_evaluation_room(
            "EVALLEAVE",
            owner_user_id,
            Some(subject_user_id),
            "EvalSubject",
            false,
        );
        start_match_in_room_state(&mut room, 0, 7).expect("evaluation human room should start");
        room.phase = "finished".to_string();
        if let Some(match_state) = room.match_state.as_mut() {
            match_state.match_finished = true;
            match_state.cumulative_scores.insert(0, 48);
            match_state.statistics.completed_round_count = 16;
            let stats = match_state
                .statistics
                .seat_stats_by_seat
                .entry(0)
                .or_default();
            stats.win_count = 2;
            stats.deal_in_count = 1;
        }
        let room_json = serialize_room_state(&room)?;
        worker
            .save_table_and_upsert_participant(
                "EVALLEAVE",
                "2026-05-06T00:00:00Z",
                &room_json,
                0,
                subject_user_id,
                "EvalSubject",
                "2026-05-06T00:00:00Z",
            )
            .await?;
        let state = AppContext::new(worker.clone());
        state.inner.evaluation_sessions.write().await.insert(
            "eval-leave".to_string(),
            EvaluationSessionResponse {
                evaluation_id: "eval-leave".to_string(),
                seed: 7,
                subjects: vec![EvaluationSubjectResponse {
                    subject_id: format!("user:{subject_user_id}"),
                    user_id: Some(subject_user_id),
                    display_name: "EvalSubject".to_string(),
                    kind: "human".to_string(),
                    table_code: "EVALLEAVE".to_string(),
                    phase: "playing".to_string(),
                    completed: false,
                    final_score: None,
                    deal_in_count: None,
                    win_count: None,
                    completed_round_count: None,
                }],
            },
        );
        ensure_room_loaded(&state, "EVALLEAVE")
            .await?
            .expect("room should load");
        let (connection, _receiver) = test_connection_handle(1, 8);
        let _ = subject_token;

        let outcome = handle_leave_table(state.clone(), "EVALLEAVE", &connection, 0).await;

        assert!(outcome.close_socket);
        assert!(room_handle(&state, "EVALLEAVE").await.is_none());
        assert!(worker.get_table("EVALLEAVE").await?.is_none());
        assert!(
            worker
                .get_active_table_participant("EVALLEAVE", subject_user_id)
                .await?
                .is_none()
        );
        let sessions = state.inner.evaluation_sessions.read().await;
        let subject = &sessions
            .get("eval-leave")
            .expect("evaluation session should remain")
            .subjects[0];
        assert!(subject.completed);
        assert_eq!(subject.phase, "finished");
        assert_eq!(subject.final_score, Some(48));
        assert_eq!(subject.win_count, Some(2));
        assert_eq!(subject.deal_in_count, Some(1));
        assert_eq!(subject.completed_round_count, Some(16));
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn join_table_reuses_current_room_seat_after_wind_rotation() -> Result<()> {
        let (state, worker, guest_token, guest_user_id, other_user_id) =
            build_loaded_room_with_stale_seat_indexes("ROOMROTJOIN").await?;
        let (connection, _receiver) = test_connection_handle(1, 8);

        let outcome = handle_join_table(
            state.clone(),
            "ROOMROTJOIN",
            &connection,
            JoinTableRequest {
                session_token: guest_token,
            },
        )
        .await;

        assert!(matches!(
            outcome.role,
            Some(ConnectionRole::Player { seat_index: 1 })
        ));
        let room_handle = room_handle(&state, "ROOMROTJOIN")
            .await
            .expect("room should stay loaded");
        let runtime = room_handle.runtime.lock().await;
        let guest_seats = runtime
            .room
            .seats
            .iter()
            .filter(|seat| seat.user_id == Some(guest_user_id))
            .map(|seat| seat.seat_index)
            .collect::<Vec<_>>();
        assert_eq!(guest_seats, vec![1]);
        assert_eq!(
            runtime
                .room
                .seats
                .iter()
                .find(|seat| seat.seat_index == 0)
                .and_then(|seat| seat.user_id),
            Some(other_user_id)
        );
        drop(runtime);

        let participant = worker
            .get_active_table_participant("ROOMROTJOIN", guest_user_id)
            .await?
            .expect("participant should remain active");
        assert_eq!(participant.seat_index, 1);
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

    #[tokio::test(flavor = "current_thread")]
    async fn point_gesture_quick_chat_broadcasts_when_actor_points_are_higher() -> Result<()> {
        let (state, guest_token) = build_reserved_participant_state("ROOMQCGEST").await?;
        let db = state.inner.db.clone();
        let (owner_user_id, _owner_token) =
            register_ws_test_user(&db, "INVITE200010", "OwnerQCGest").await?;
        let guest_user_id = db
            .get_authenticated_user(&hash_session_token(&guest_token), "2026-05-06T00:00:00Z")
            .await?
            .map(|user| user.user_id)
            .ok_or_else(|| anyhow::anyhow!("guest should exist"))?;
        ensure_room_loaded(&state, "ROOMQCGEST")
            .await?
            .expect("room should load");

        let room_handle = room_handle(&state, "ROOMQCGEST")
            .await
            .expect("room should be loaded");
        {
            let mut runtime = room_handle.runtime.lock().await;
            runtime.room.owner_user_id = Some(owner_user_id);
            if let Some(seat) = runtime
                .room
                .seats
                .iter_mut()
                .find(|seat| seat.seat_index == 0)
            {
                seat.user_id = Some(guest_user_id);
                seat.nickname = Some("Guest".to_string());
                seat.points = Some(1100);
            }
            runtime.room.seats.push(SeatState {
                seat_index: 1,
                user_id: Some(owner_user_id),
                nickname: Some("OwnerQCGest".to_string()),
                points: Some(400),
                title: Some("正分守门员".to_string()),
                connected: true,
                is_bot: false,
                seat_type: "human".to_string(),
                bot_persona: None,
                bot_aggression: None,
                disconnect_deadline_at: None,
                consecutive_timeout_auto_response_count: 0,
            });
        }

        let (guest_connection, _guest_receiver) = test_connection_handle(1, 8);
        let guest_join = handle_join_table(
            state.clone(),
            "ROOMQCGEST",
            &guest_connection,
            JoinTableRequest {
                session_token: guest_token,
            },
        )
        .await;
        assert!(matches!(
            guest_join.role,
            Some(ConnectionRole::Player { seat_index: 0 })
        ));

        let outcome = handle_quick_chat(
            state,
            "ROOMQCGEST",
            &guest_connection,
            0,
            QuickChatRequest {
                target_seat: Some(1),
                chat_kind: Some("point_gesture".to_string()),
                emoji: "point_gesture".to_string(),
            },
        )
        .await;

        assert_eq!(outcome.outbound.len(), 1);
        let payload: Value = serde_json::from_str(&outcome.outbound[0].payload)?;
        assert_eq!(payload["type"], "quick_chat");
        assert_eq!(payload["payload"]["chat_kind"], "point_gesture");
        assert_eq!(payload["payload"]["actor_seat"], 0);
        assert_eq!(payload["payload"]["target_seat"], 1);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn point_gesture_quick_chat_rejects_equal_points() -> Result<()> {
        let (state, guest_token) = build_reserved_participant_state("ROOMQCREJECT").await?;
        let db = state.inner.db.clone();
        let (owner_user_id, owner_token) =
            register_ws_test_user(&db, "INVITE200011", "OwnerQCReject").await?;
        let guest_user_id = db
            .get_authenticated_user(&hash_session_token(&guest_token), "2026-05-06T00:00:00Z")
            .await?
            .map(|user| user.user_id)
            .ok_or_else(|| anyhow::anyhow!("guest should exist"))?;
        ensure_room_loaded(&state, "ROOMQCREJECT")
            .await?
            .expect("room should load");

        let room_handle = room_handle(&state, "ROOMQCREJECT")
            .await
            .expect("room should be loaded");
        {
            let mut runtime = room_handle.runtime.lock().await;
            runtime.room.owner_user_id = Some(owner_user_id);
            if let Some(seat) = runtime
                .room
                .seats
                .iter_mut()
                .find(|seat| seat.seat_index == 0)
            {
                seat.user_id = Some(guest_user_id);
                seat.nickname = Some("Guest".to_string());
                seat.points = Some(600);
            }
            runtime.room.seats.push(SeatState {
                seat_index: 1,
                user_id: Some(owner_user_id),
                nickname: Some("OwnerQCReject".to_string()),
                points: Some(600),
                title: Some("正分守门员".to_string()),
                connected: true,
                is_bot: false,
                seat_type: "human".to_string(),
                bot_persona: None,
                bot_aggression: None,
                disconnect_deadline_at: None,
                consecutive_timeout_auto_response_count: 0,
            });
        }

        let (guest_connection, _guest_receiver) = test_connection_handle(1, 8);
        let _ = handle_join_table(
            state.clone(),
            "ROOMQCREJECT",
            &guest_connection,
            JoinTableRequest {
                session_token: guest_token,
            },
        )
        .await;

        let (owner_connection, _owner_receiver) = test_connection_handle(2, 8);
        let _ = handle_join_table(
            state.clone(),
            "ROOMQCREJECT",
            &owner_connection,
            JoinTableRequest {
                session_token: owner_token,
            },
        )
        .await;

        let outcome = handle_quick_chat(
            state,
            "ROOMQCREJECT",
            &owner_connection,
            1,
            QuickChatRequest {
                target_seat: Some(0),
                chat_kind: Some("point_gesture".to_string()),
                emoji: "point_gesture".to_string(),
            },
        )
        .await;

        let payload: Value = serde_json::from_str(&outcome.outbound[0].payload)?;
        assert_eq!(payload["type"], "action_rejected");
        assert_eq!(payload["payload"]["reason"], "invalid_action");
        Ok(())
    }
}
