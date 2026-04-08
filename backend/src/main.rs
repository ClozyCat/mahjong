mod app;
mod bot;
mod core;
mod mahjong;
mod projection;
mod room_scoring;
mod rules;
mod scoring;

use std::collections::{HashMap, HashSet};
use std::env;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxumPath, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, SecondsFormat, Utc};
use futures_util::{SinkExt, StreamExt};
use mahjong::{
    action_prompt as build_action_prompt, add_bot_seats_for_test as rust_add_bot_seats_for_test,
    reconcile_continue_action_state as rust_reconcile_continue_action_state,
    record_continue_action as rust_record_continue_action, room_messages as build_room_messages,
    room_ready_to_start as rust_room_ready_to_start, start_match as rust_start_match,
    try_handle_action as try_rust_action,
};
use rand::Rng;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{Notify, RwLock, mpsc};
use tower_http::cors::{Any, CorsLayer};

use crate::app::persistence::{Database, DbWorker};
use crate::app::room_runtime::{
    RoomHandle, RoomRuntime, close_room_handle, close_runtime, ensure_room_loaded,
    mark_restored_room_disconnected, replace_connection, restore_persisted_rooms,
    restore_room_snapshot, room_handle, room_has_only_bots, should_terminate_unattended,
    snapshot_connections, unregister_room_handle,
};
use crate::app::scheduler::schedule_room_tasks_detached;

const MAX_SEATS: usize = 4;
const DISCONNECT_GRACE_SECONDS: i64 = 120;
const BOT_ACTION_DELAY_TEST_MS: u64 = 0;
const BOT_ACTION_DELAY_NORMAL_MS: u64 = 600;
const OUTBOUND_CHANNEL_CAPACITY: usize = 128;

#[derive(Clone)]
struct Settings {
    bind_addr: String,
    database_path: String,
    default_test_mode: bool,
    cors_origins: Vec<String>,
}

impl Settings {
    fn from_env() -> Result<Self> {
        Ok(Self {
            bind_addr: env::var("MAHJONG_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8000".to_string()),
            database_path: resolve_database_path(
                &env::var("MAHJONG_DATABASE_URL")
                    .unwrap_or_else(|_| "sqlite+pysqlite:////data/mahjong.db".to_string()),
            ),
            default_test_mode: parse_bool_env("MAHJONG_TEST_MODE"),
            cors_origins: dev_cors_origins(),
        })
    }
}

#[derive(Clone)]
struct AppContext {
    settings: Settings,
    next_connection_id: Arc<AtomicU64>,
    inner: Arc<AppState>,
}

struct AppState {
    db: DbWorker,
    rooms: RwLock<HashMap<String, Arc<RoomHandle>>>,
}

#[derive(Clone)]
struct ConnectionHandle {
    id: u64,
    sender: mpsc::Sender<String>,
    close_requested: Arc<AtomicBool>,
    close_notify: Arc<Notify>,
}

impl ConnectionHandle {
    fn outbound(&self, payload: Value) -> OutboundMessage {
        OutboundMessage {
            connection: self.clone(),
            payload,
        }
    }

    fn try_send(&self, message: String) -> Result<(), mpsc::error::TrySendError<String>> {
        self.sender.try_send(message)
    }

    fn request_close(&self) {
        self.close_requested.store(true, Ordering::Relaxed);
        self.close_notify.notify_waiters();
    }

    fn should_close(&self) -> bool {
        self.close_requested.load(Ordering::Relaxed)
    }
}

struct OutboundMessage {
    connection: ConnectionHandle,
    payload: Value,
}

#[derive(Debug, Deserialize)]
struct CreateTableRequest {
    table_code: Option<String>,
    mode: Option<String>,
    test_mode: Option<bool>,
    enforce_minimum_eight_fan: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ClientEnvelope {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    payload: Value,
}

struct MessageOutcome {
    outbound: Vec<OutboundMessage>,
    owned_seat: Option<usize>,
    clear_owned_seat: bool,
    close_socket: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let settings = Settings::from_env()?;
    let db = DbWorker::start(Database::open(&settings.database_path)?)?;

    let app_state = AppContext {
        settings: settings.clone(),
        next_connection_id: Arc::new(AtomicU64::new(1)),
        inner: Arc::new(AppState {
            db,
            rooms: RwLock::new(HashMap::new()),
        }),
    };
    restore_persisted_rooms(&app_state).await;

    let app = Router::new()
        .route("/api/health", get(healthcheck))
        .route("/api/tables", post(create_table))
        .route("/ws/{table_code}", get(websocket_handler))
        .with_state(app_state)
        .layer(build_cors_layer(&settings.cors_origins));

    let listener = tokio::net::TcpListener::bind(&settings.bind_addr)
        .await
        .with_context(|| format!("failed to bind to {}", settings.bind_addr))?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn build_cors_layer(origins: &[String]) -> CorsLayer {
    let mut layer = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::ACCEPT]);

    let header_values: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|origin| HeaderValue::from_str(origin).ok())
        .collect();
    if header_values.is_empty() {
        layer = layer.allow_origin(Any);
    } else {
        layer = layer.allow_origin(header_values);
    }
    layer
}

async fn healthcheck() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn create_table(
    State(state): State<AppContext>,
    payload: Option<Json<CreateTableRequest>>,
) -> Response {
    let payload = payload.map(|value| value.0);
    let requested_mode = if let Some(ref body) = payload {
        if let Some(mode) = &body.mode {
            Some(mode.clone())
        } else {
            body.test_mode
                .map(|value| if value { "test" } else { "normal" }.to_string())
        }
    } else {
        None
    };
    let resolved_mode = requested_mode.unwrap_or_else(|| {
        if state.settings.default_test_mode {
            "test".to_string()
        } else {
            "normal".to_string()
        }
    });

    if resolved_mode != "normal" && resolved_mode != "test" {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "detail": "unsupported_mode" })),
        )
            .into_response();
    }

    let requested_code = match payload
        .as_ref()
        .and_then(|body| body.table_code.clone())
        .map(|value| normalize_table_code(&value))
    {
        Some(code) if !is_valid_table_code(&code) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "detail": "invalid_table_code" })),
            )
                .into_response();
        }
        value => value,
    };

    let enforce_minimum_eight_fan = payload
        .as_ref()
        .and_then(|body| body.enforce_minimum_eight_fan)
        .unwrap_or(true);

    let result = create_or_replace_table(
        &state,
        requested_code,
        &resolved_mode,
        enforce_minimum_eight_fan,
    )
    .await;

    match result {
        Ok((table_code, created_at, room)) => (
            StatusCode::CREATED,
            Json(json!({
                "table_code": table_code,
                "phase": "waiting",
                "mode": resolved_mode,
                "created_at": created_at,
                "seats": room.get("seats").cloned().unwrap_or_else(|| Value::Array(vec![])),
            })),
        )
            .into_response(),
        Err(CreateTableError::Conflict) => (
            StatusCode::CONFLICT,
            Json(json!({ "detail": "table_code_exists" })),
        )
            .into_response(),
        Err(CreateTableError::Internal(error)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "detail": error.to_string() })),
        )
            .into_response(),
    }
}

enum CreateTableError {
    Conflict,
    Internal(anyhow::Error),
}

async fn create_or_replace_table(
    state: &AppContext,
    requested_code: Option<String>,
    mode: &str,
    enforce_minimum_eight_fan: bool,
) -> std::result::Result<(String, String, Value), CreateTableError> {
    let mut rooms = state.inner.rooms.write().await;
    let runtime_codes: HashSet<String> = rooms.keys().cloned().collect();
    let table_code = if let Some(code) = requested_code {
        code
    } else {
        state
            .inner
            .db
            .generate_table_code(runtime_codes)
            .await
            .map_err(CreateTableError::Internal)?
    };

    let existing_record = state
        .inner
        .db
        .get_table(&table_code)
        .await
        .map_err(CreateTableError::Internal)?;
    if let Some(record) = existing_record {
        let existing_room: Value = serde_json::from_str(&record.room_json)
            .map_err(|error| CreateTableError::Internal(error.into()))?;
        let occupied = existing_room
            .get("seats")
            .and_then(Value::as_array)
            .map(|seats| !seats.is_empty())
            .unwrap_or(false);
        if occupied {
            return Err(CreateTableError::Conflict);
        }
    }

    let replaced = rooms.remove(&table_code);
    if let Some(room_handle) = &replaced {
        room_handle.mark_closed();
    }
    drop(rooms);
    let created_at = now_iso();
    let room = initial_room_payload(&table_code, mode, enforce_minimum_eight_fan);
    let room_json = serialize_room(&room).map_err(CreateTableError::Internal)?;
    state
        .inner
        .db
        .save_table(&table_code, &created_at, &room_json)
        .await
        .map_err(CreateTableError::Internal)?;
    let room_handle = Arc::new(RoomHandle::new(RoomRuntime::new(
        created_at.clone(),
        room.clone(),
    )));
    let replaced_after_insert = {
        let mut rooms = state.inner.rooms.write().await;
        rooms.insert(table_code.clone(), room_handle)
    };
    if let Some(old_room) = replaced.or(replaced_after_insert) {
        close_room_handle(&old_room).await;
    }
    Ok((table_code, created_at, room))
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppContext>,
    AxumPath(table_code): AxumPath<String>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| websocket_session(state, socket, normalize_table_code(&table_code)))
}

fn parse_bool_env(key: &str) -> bool {
    matches!(
        env::var(key).ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON")
    )
}

fn dev_cors_origins() -> Vec<String> {
    let mut origins = vec![
        "http://localhost:5173".to_string(),
        "http://127.0.0.1:5173".to_string(),
    ];
    if let Ok(extra) = env::var("MAHJONG_DEV_CORS_ORIGINS") {
        for item in extra.split(',') {
            let candidate = item.trim();
            if !candidate.is_empty() && !origins.iter().any(|origin| origin == candidate) {
                origins.push(candidate.to_string());
            }
        }
    }
    origins
}

fn resolve_database_path(database_url: &str) -> String {
    for prefix in ["sqlite+pysqlite:///", "sqlite:///"] {
        if let Some(rest) = database_url.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    database_url.to_string()
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
    let mut outbound = broadcast_to_handles(&broadcast_handles, Some(&rust_handled_messages));
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
            "sent_at": now_iso(),
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
    } else {
        if room_has_only_bots(&runtime.room) || should_terminate_unattended(&runtime) {
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
        Some(disconnect_deadline_iso()),
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

fn find_seat_mut(
    room: &mut Value,
    seat_index: usize,
) -> Option<&mut serde_json::Map<String, Value>> {
    room.get_mut("seats")
        .and_then(Value::as_array_mut)
        .and_then(|seats| {
            seats.iter_mut().find(|seat| {
                seat.get("seat_index")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize == seat_index)
                    .unwrap_or(false)
            })
        })
        .and_then(Value::as_object_mut)
}

fn remove_seat_from_room(room: &mut Value, seat_index: usize) {
    if let Some(seats) = room.get_mut("seats").and_then(Value::as_array_mut) {
        if let Some(index) = seats.iter().position(|seat| {
            seat.get("seat_index")
                .and_then(Value::as_u64)
                .map(|value| value as usize == seat_index)
                .unwrap_or(false)
        }) {
            seats.remove(index);
        }
    }
}

fn seat_exists(room: &Value, seat_index: usize) -> bool {
    room.get("seats")
        .and_then(Value::as_array)
        .is_some_and(|seats| {
            seats.iter().any(|seat| {
                seat.get("seat_index")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize == seat_index)
                    .unwrap_or(false)
            })
        })
}

fn first_open_seat_index(room: &Value) -> Option<usize> {
    let occupied = occupied_seats(room);
    (0..MAX_SEATS).find(|seat_index| !occupied.contains(seat_index))
}

fn add_bot_to_waiting_room(room: &mut Value) -> Result<usize, &'static str> {
    if room_phase(room) != "waiting" || room_has_round_state(room) {
        return Err("room_already_started");
    }

    let Some(seat_index) = first_open_seat_index(room) else {
        return Err("room_full");
    };
    let Some(seats) = room.get_mut("seats").and_then(Value::as_array_mut) else {
        return Err("invalid_room_state");
    };

    seats.push(json!({
        "seat_index": seat_index,
        "nickname": format!("Bot {seat_index}"),
        "reconnect_token": Value::Null,
        "player_session_id": -((seat_index as i64) + 1),
        "connected": true,
        "ready": true,
        "is_bot": true,
        "seat_type": "bot",
        "bot_persona": Value::Null,
        "bot_aggression": Value::Null,
        "disconnect_deadline_at": Value::Null,
    }));
    seats.sort_by_key(|seat| seat.get("seat_index").and_then(Value::as_u64).unwrap_or(99));
    Ok(seat_index)
}

fn remove_bot_from_waiting_room(room: &mut Value) -> Result<usize, &'static str> {
    if room_phase(room) != "waiting" || room_has_round_state(room) {
        return Err("room_already_started");
    }

    let seat_index = room
        .get("seats")
        .and_then(Value::as_array)
        .and_then(|seats| {
            seats
                .iter()
                .filter(|seat| seat.get("is_bot").and_then(Value::as_bool).unwrap_or(false))
                .filter_map(|seat| seat.get("seat_index").and_then(Value::as_u64))
                .map(|value| value as usize)
                .max()
        })
        .ok_or("bot_not_found")?;
    remove_seat_from_room(room, seat_index);
    Ok(seat_index)
}

fn convert_seat_to_bot(room: &mut Value, seat_index: usize) {
    if let Some(seat) = find_seat_mut(room, seat_index) {
        seat.insert("connected".to_string(), Value::Bool(true));
        seat.insert("ready".to_string(), Value::Bool(true));
        seat.insert("is_bot".to_string(), Value::Bool(true));
        seat.insert("seat_type".to_string(), Value::String("bot".to_string()));
        seat.insert("reconnect_token".to_string(), Value::Null);
        seat.insert("disconnect_deadline_at".to_string(), Value::Null);
    }
}

fn set_seat_connected(
    room: &mut Value,
    seat_index: usize,
    connected: bool,
    deadline_at: Option<String>,
) {
    if let Some(seat) = find_seat_mut(room, seat_index) {
        seat.insert("connected".to_string(), Value::Bool(connected));
        seat.insert(
            "disconnect_deadline_at".to_string(),
            deadline_at.map(Value::String).unwrap_or(Value::Null),
        );
    }
}

fn collect_join_outbound_from_snapshot(
    room: &Value,
    connections: &[(usize, ConnectionHandle)],
    table_code: &str,
    connection: &ConnectionHandle,
    seat_index: usize,
    connected: bool,
) -> Vec<OutboundMessage> {
    let mut outbound = Vec::new();
    outbound.extend(build_room_messages_for_seat(room, seat_index, connection));
    if let Some(prompt) = build_prompt_for_seat(room, seat_index) {
        outbound.push(connection.outbound(prompt));
    }

    let presence = json!({
        "type": "player_presence",
        "payload": {
            "table_code": table_code,
            "seat_index": seat_index,
            "connected": connected,
        }
    });
    for (other_seat, handle) in connections {
        if *other_seat == seat_index {
            continue;
        }
        outbound.push(handle.outbound(presence.clone()));
        outbound.extend(build_room_messages_for_seat(room, *other_seat, handle));
    }
    for (other_seat, handle) in connections {
        if *other_seat == seat_index {
            continue;
        }
        if let Some(prompt) = build_prompt_for_seat(room, *other_seat) {
            outbound.push(handle.outbound(prompt));
        }
    }
    outbound
}

fn presence_and_snapshot_for_all_from_snapshot(
    room: &Value,
    connections: &[(usize, ConnectionHandle)],
    table_code: &str,
    seat_index: usize,
    connected: bool,
) -> Vec<OutboundMessage> {
    let mut outbound = Vec::new();
    let presence = json!({
        "type": "player_presence",
        "payload": {
            "table_code": table_code,
            "seat_index": seat_index,
            "connected": connected,
        }
    });
    for (target_seat, handle) in connections {
        outbound.push(handle.outbound(presence.clone()));
        outbound.extend(build_room_messages_for_seat(room, *target_seat, handle));
        if let Some(prompt) = build_prompt_for_seat(room, *target_seat) {
            outbound.push(handle.outbound(prompt));
        }
    }
    outbound
}

fn collect_snapshot_and_prompt_outbound_from_snapshot(
    room: &Value,
    connections: &[(usize, ConnectionHandle)],
) -> Vec<OutboundMessage> {
    let mut outbound = Vec::new();
    for (seat_index, handle) in connections {
        outbound.extend(build_room_messages_for_seat(room, *seat_index, handle));
    }
    for (seat_index, handle) in connections {
        if let Some(prompt) = build_prompt_for_seat(room, *seat_index) {
            outbound.push(handle.outbound(prompt));
        }
    }
    outbound
}

fn build_room_messages_for_seat(
    room: &Value,
    local_seat: usize,
    connection: &ConnectionHandle,
) -> Vec<OutboundMessage> {
    build_room_messages(room, local_seat)
        .into_iter()
        .map(|payload| connection.outbound(payload))
        .collect()
}

fn build_prompt_for_seat(room: &Value, local_seat: usize) -> Option<Value> {
    build_action_prompt(room, local_seat)
}

fn broadcast_to_handles(
    handles: &[ConnectionHandle],
    messages: Option<&Vec<Value>>,
) -> Vec<OutboundMessage> {
    let mut outbound = Vec::new();
    let Some(messages) = messages else {
        return outbound;
    };
    for handle in handles {
        for payload in messages {
            outbound.push(handle.outbound(payload.clone()));
        }
    }
    outbound
}

fn serialize_room(room: &Value) -> Result<String> {
    serde_json::to_string(room).map_err(Into::into)
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

fn internal_error_to(connection: &ConnectionHandle, error: anyhow::Error) -> MessageOutcome {
    reject_to(connection, &format!("internal_error:{error}"))
}

fn send_outbound(outbound: Vec<OutboundMessage>) {
    for message in outbound {
        let payload = serialize_payload(&message.payload);
        if let Err(error) = message.connection.try_send(payload) {
            if matches!(error, mpsc::error::TrySendError::Full(_)) {
                message.connection.request_close();
            }
        }
    }
}

fn serialize_payload(payload: &Value) -> String {
    serde_json::to_string(payload).unwrap_or_else(|_| {
        "{\"type\":\"action_rejected\",\"payload\":{\"reason\":\"serialization_error\"}}"
            .to_string()
    })
}

fn initial_room_payload(table_code: &str, mode: &str, enforce_minimum_eight_fan: bool) -> Value {
    json!({
        "table_code": table_code,
        "phase": "waiting",
        "mode": mode,
        "test_mode": mode == "test",
        "enforce_minimum_eight_fan": enforce_minimum_eight_fan,
        "start_next_round_confirmed_seats": [],
        "restart_match_confirmed_seats": [],
        "continue_action_auto_advance_deadline_at": null,
        "seats": [],
        "match_state": null,
        "round_state": null,
        "pending_timeout": null,
    })
}

fn normalize_table_code(table_code: &str) -> String {
    table_code.trim().to_ascii_uppercase()
}

fn is_valid_table_code(table_code: &str) -> bool {
    !table_code.is_empty()
        && table_code.len() <= 12
        && table_code
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
}

fn room_mode(room: &Value) -> String {
    room.get("mode")
        .and_then(Value::as_str)
        .unwrap_or("normal")
        .to_string()
}

fn room_has_round_state(room: &Value) -> bool {
    room.get("round_state")
        .is_some_and(|state| !state.is_null())
}

fn maybe_start_test_match(room: &mut Value) {
    if room_mode(room) != "test" || room_has_round_state(room) {
        return;
    }

    rust_add_bot_seats_for_test(room);
    rust_start_match(room, 0, rand::random::<u64>());
}

fn room_phase(room: &Value) -> String {
    room.get("phase")
        .and_then(Value::as_str)
        .unwrap_or("waiting")
        .to_string()
}

fn room_seats(room: &Value) -> Vec<Value> {
    room.get("seats")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn occupied_seats(room: &Value) -> HashSet<usize> {
    room_seats(room)
        .into_iter()
        .filter_map(|seat| {
            seat.get("seat_index")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
        })
        .collect()
}

fn room_player_session_id(room: &Value, seat_index: usize) -> Option<i64> {
    room.get("seats")
        .and_then(Value::as_array)
        .and_then(|seats| {
            seats.iter().find(|seat| {
                seat.get("seat_index")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize == seat_index)
                    .unwrap_or(false)
            })
        })
        .and_then(|seat| seat.get("player_session_id").and_then(Value::as_i64))
}

fn room_reconnect_token(room: &Value, seat_index: usize) -> Option<&str> {
    room.get("seats")
        .and_then(Value::as_array)
        .and_then(|seats| {
            seats.iter().find(|seat| {
                seat.get("seat_index")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize == seat_index)
                    .unwrap_or(false)
            })
        })
        .and_then(|seat| seat.get("reconnect_token").and_then(Value::as_str))
}

fn seat_matches_reconnect_credentials(
    room: &Value,
    seat_index: usize,
    player_session_id: i64,
    reconnect_token: &str,
) -> bool {
    room_player_session_id(room, seat_index) == Some(player_session_id)
        && room_reconnect_token(room, seat_index) == Some(reconnect_token)
}

fn pending_timeout_deadline(room: &Value) -> Option<DateTime<Utc>> {
    room.get("pending_timeout")
        .and_then(|value| value.get("deadline_at"))
        .and_then(Value::as_str)
        .and_then(parse_datetime)
}

fn continue_action_deadline(room: &Value) -> Option<DateTime<Utc>> {
    room.get("continue_action_auto_advance_deadline_at")
        .and_then(Value::as_str)
        .and_then(parse_datetime)
}

fn disconnect_deadline_for_seat(room: &Value, seat_index: usize) -> Option<DateTime<Utc>> {
    room.get("seats")
        .and_then(Value::as_array)
        .and_then(|seats| {
            seats.iter().find(|seat| {
                seat.get("seat_index")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize == seat_index)
                    .unwrap_or(false)
            })
        })
        .and_then(|seat| seat.get("disconnect_deadline_at"))
        .and_then(Value::as_str)
        .and_then(parse_datetime)
}

fn next_disconnect_deadline(room: &Value) -> Option<(usize, DateTime<Utc>)> {
    room.get("seats")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|seat| {
            let seat_index = seat.get("seat_index").and_then(Value::as_u64)? as usize;
            let is_bot = seat.get("is_bot").and_then(Value::as_bool).unwrap_or(false);
            let connected = seat
                .get("connected")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if is_bot || connected {
                return None;
            }
            let deadline = seat
                .get("disconnect_deadline_at")
                .and_then(Value::as_str)
                .and_then(parse_datetime)?;
            Some((seat_index, deadline))
        })
        .min_by_key(|(_, deadline)| *deadline)
}

fn parse_datetime(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
                .map(|value| DateTime::<Utc>::from_naive_utc_and_offset(value, Utc))
        })
        .ok()
}

async fn sleep_until(deadline: DateTime<Utc>) {
    let now = Utc::now();
    let duration = if deadline > now {
        (deadline - now).to_std().unwrap_or(Duration::from_secs(0))
    } else {
        Duration::from_secs(0)
    };
    tokio::time::sleep(duration).await;
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn disconnect_deadline_iso() -> String {
    (Utc::now() + chrono::TimeDelta::seconds(DISCONNECT_GRACE_SECONDS))
        .to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn generate_player_session_id() -> i64 {
    let mut rng = rand::rng();
    rng.random_range(1_i64..i64::MAX)
}

fn generate_reconnect_token() -> String {
    generate_short_hex(32)
}

fn generate_short_hex(bytes: usize) -> String {
    let mut rng = rand::rng();
    let mut data = vec![0_u8; bytes];
    rng.fill(data.as_mut_slice());
    data.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::persistence::in_memory_database;

    fn test_app_context(db: DbWorker) -> AppContext {
        AppContext {
            settings: Settings {
                bind_addr: "127.0.0.1:0".to_string(),
                database_path: ":memory:".to_string(),
                default_test_mode: false,
                cors_origins: vec![],
            },
            next_connection_id: Arc::new(AtomicU64::new(1)),
            inner: Arc::new(AppState {
                db,
                rooms: RwLock::new(HashMap::new()),
            }),
        }
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
        let mut room = initial_room_payload("ROOM42", "test", true);
        room["seats"] = json!([{
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
        }]);

        maybe_start_test_match(&mut room);

        assert_eq!(room["phase"], "playing");
        assert_eq!(room["mode"], "test");
        assert_eq!(room["seats"].as_array().map(Vec::len), Some(4));
        assert!(room_has_round_state(&room));
        assert_eq!(room["match_state"]["dealer_seat"], 0);
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
        let mut runtime =
            RoomRuntime::new(now_iso(), initial_room_payload("ROOM42", "normal", true));
        runtime.connections = HashMap::from([(0, previous.clone())]);

        replace_connection(&mut runtime, 0, &replacement);

        assert!(previous.should_close());
        assert_eq!(runtime.connections.get(&0).map(|handle| handle.id), Some(2));
    }

    #[test]
    fn restored_human_seats_receive_disconnect_deadline() {
        let mut room = initial_room_payload("ROOM42", "normal", true);
        room["seats"] = json!([
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
        ]);

        mark_restored_room_disconnected(&mut room);

        let human_deadline = room["seats"][0]["disconnect_deadline_at"]
            .as_str()
            .and_then(parse_datetime);
        assert!(human_deadline.is_some());
        assert_eq!(room["seats"][0]["connected"], Value::Bool(false));
        assert!(room["seats"][1]["disconnect_deadline_at"].is_null());
    }

    #[test]
    fn add_bot_to_waiting_room_fills_first_empty_seat_and_marks_ready() {
        let mut room = initial_room_payload("ROOM42", "normal", true);
        room["seats"] = json!([
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
        ]);

        let inserted_seat = add_bot_to_waiting_room(&mut room).expect("bot seat should be added");

        assert_eq!(inserted_seat, 1);
        assert_eq!(room["seats"].as_array().map(Vec::len), Some(3));
        assert_eq!(room["seats"][1]["seat_index"], Value::from(1));
        assert_eq!(room["seats"][1]["nickname"], Value::from("Bot 1"));
        assert_eq!(room["seats"][1]["ready"], Value::Bool(true));
        assert_eq!(room["seats"][1]["connected"], Value::Bool(true));
        assert_eq!(room["seats"][1]["is_bot"], Value::Bool(true));
    }

    #[test]
    fn remove_bot_from_waiting_room_removes_highest_index_bot() {
        let mut room = initial_room_payload("ROOM42", "normal", true);
        room["seats"] = json!([
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
        ]);

        let removed_seat =
            remove_bot_from_waiting_room(&mut room).expect("bot seat should be removed");

        assert_eq!(removed_seat, 3);
        assert_eq!(room["seats"].as_array().map(Vec::len), Some(2));
        assert_eq!(occupied_seats(&room), HashSet::from([0, 1]));
    }

    #[test]
    fn room_has_only_bots_requires_non_empty_bot_only_room() {
        let mut empty_room = initial_room_payload("ROOM42", "normal", true);
        assert!(!room_has_only_bots(&empty_room));

        empty_room["seats"] = json!([{
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
        }]);
        assert!(room_has_only_bots(&empty_room));

        empty_room["seats"] = json!([
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
        ]);
        assert!(!room_has_only_bots(&empty_room));
    }

    #[test]
    fn seat_matches_reconnect_credentials_requires_current_room_token() {
        let mut room = initial_room_payload("ROOM42", "normal", true);
        room["seats"] = json!([{
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
        }]);

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

        let room_json = serde_json::to_string(&json!({
            "table_code": "ROOM42",
            "mode": "normal",
            "phase": "waiting",
            "seats": [{
                "seat_index": 0,
                "nickname": "Alice",
                "reconnect_token": "token-1",
                "player_session_id": 42,
                "connected": true,
                "ready": true,
                "is_bot": false,
                "seat_type": "human",
                "bot_persona": Value::Null,
                "bot_aggression": Value::Null,
                "disconnect_deadline_at": Value::Null
            }]
        }))?;
        worker
            .save_table("ROOM42", "2026-04-07T00:00:00Z", &room_json)
            .await?;

        restore_persisted_rooms(&state).await;

        let room_handle = room_handle(&state, "ROOM42")
            .await
            .expect("restored room should be loaded");
        let runtime = room_handle.runtime.lock().await;
        assert_eq!(runtime.room["seats"][0]["connected"], Value::Bool(false));
        assert!(
            runtime.room["seats"][0]["disconnect_deadline_at"]
                .as_str()
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

        let room_json = serde_json::to_string(&json!({
            "table_code": "ROOMBOT",
            "mode": "normal",
            "phase": "waiting",
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
            }]
        }))?;
        worker
            .save_table("ROOMBOT", "2026-04-07T00:00:00Z", &room_json)
            .await?;

        restore_persisted_rooms(&state).await;

        assert!(room_handle(&state, "ROOMBOT").await.is_none());
        assert!(worker.get_table("ROOMBOT").await?.is_none());
        Ok(())
    }
}
