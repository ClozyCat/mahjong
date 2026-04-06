mod mahjong;
mod scoring;

use std::collections::{HashMap, HashSet};
use std::env;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
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
    next_bot_action as rust_next_bot_action,
    process_due_continue_action as rust_process_due_continue_action,
    reconcile_continue_action_state as rust_reconcile_continue_action_state,
    record_continue_action as rust_record_continue_action, room_messages as build_room_messages,
    room_ready_to_start as rust_room_ready_to_start, start_match as rust_start_match,
    try_handle_action as try_rust_action, try_process_due_timeout as try_rust_process_due_timeout,
};
use rand::Rng;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc};
use tower_http::cors::{Any, CorsLayer};

const MAX_SEATS: usize = 4;
const DISCONNECT_GRACE_SECONDS: i64 = 120;
const BOT_ACTION_DELAY_TEST_MS: u64 = 0;
const BOT_ACTION_DELAY_NORMAL_MS: u64 = 800;

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
    inner: Arc<Mutex<AppState>>,
}

struct AppState {
    db: Database,
    rooms: HashMap<String, RoomRuntime>,
}

struct RoomRuntime {
    room: Value,
    connections: HashMap<usize, ConnectionHandle>,
    timeout_nonce: u64,
    continue_nonce: u64,
    disconnect_nonce: u64,
    bot_nonce: u64,
}

#[derive(Clone)]
struct ConnectionHandle {
    id: u64,
    sender: mpsc::UnboundedSender<String>,
}

struct OutboundMessage {
    sender: mpsc::UnboundedSender<String>,
    payload: Value,
}

struct Database {
    conn: Connection,
}

struct TableRecord {
    created_at: String,
    room_json: String,
}

struct ReconnectTokenRecord {
    table_code: String,
    seat_index: usize,
    player_session_id: i64,
}

struct SqliteColumn {
    name: String,
    not_null: bool,
    primary_key: bool,
}

impl Database {
    fn open(path: &str) -> Result<Self> {
        if let Some(parent) = Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create database directory for {path}"))?;
            }
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open sqlite database at {path}"))?;
        let db = Self { conn };
        db.initialize()?;
        Ok(db)
    }

    fn initialize(&self) -> Result<()> {
        self.conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        self.ensure_tables_schema()?;
        self.ensure_reconnect_tokens_schema()?;
        Ok(())
    }

    fn ensure_tables_schema(&self) -> Result<()> {
        let columns = self.table_columns("tables")?;
        if columns.is_empty() {
            self.create_tables_table()?;
            return Ok(());
        }

        if self.tables_schema_is_current(&columns) {
            return Ok(());
        }

        let room_source = if columns.iter().any(|column| column.name == "room_json") {
            Some("room_json")
        } else if columns.iter().any(|column| column.name == "state_json") {
            Some("state_json")
        } else {
            None
        };

        eprintln!("detected legacy sqlite schema for `tables`; rebuilding it");
        if room_source.is_none() {
            eprintln!(
                "legacy `tables` rows do not contain a room snapshot column; starting with an empty table store"
            );
        }

        self.with_schema_rebuild("migrate `tables` schema", |db| {
            db.conn.execute_batch(
                "
                DROP TABLE IF EXISTS tables_legacy;
                ALTER TABLE tables RENAME TO tables_legacy;
                ",
            )?;
            db.create_tables_table()?;
            if let Some(room_source) = room_source {
                let copy_sql = format!(
                    "
                    INSERT INTO tables (table_code, created_at, room_json)
                    SELECT table_code, created_at, {room_source}
                    FROM tables_legacy
                    WHERE table_code IS NOT NULL
                      AND created_at IS NOT NULL
                      AND {room_source} IS NOT NULL
                    "
                );
                db.conn.execute_batch(&copy_sql)?;
            }
            db.conn.execute_batch(
                "
                DROP TABLE IF EXISTS player_sessions;
                DROP TABLE IF EXISTS table_seats;
                DROP TABLE IF EXISTS room_snapshots;
                DROP TABLE IF EXISTS round_snapshots;
                DROP TABLE IF EXISTS settlements;
                DROP TABLE IF EXISTS round_events;
                DROP TABLE IF EXISTS alembic_version;
                DROP TABLE tables_legacy;
                ",
            )?;
            Ok(())
        })
    }

    fn ensure_reconnect_tokens_schema(&self) -> Result<()> {
        let columns = self.table_columns("reconnect_tokens")?;
        if columns.is_empty() {
            self.create_reconnect_tokens_table()?;
            return Ok(());
        }

        if self.reconnect_tokens_schema_is_current(&columns) {
            return Ok(());
        }

        let can_copy_rows = columns.iter().any(|column| column.name == "token")
            && columns.iter().any(|column| column.name == "table_code")
            && columns.iter().any(|column| column.name == "seat_index")
            && columns
                .iter()
                .any(|column| column.name == "player_session_id");

        eprintln!("detected legacy sqlite schema for `reconnect_tokens`; rebuilding it");
        if !can_copy_rows {
            eprintln!("legacy reconnect tokens cannot be migrated and will be reset");
        }

        self.with_schema_rebuild("migrate `reconnect_tokens` schema", |db| {
            db.conn.execute_batch(
                "
                DROP TABLE IF EXISTS reconnect_tokens_legacy;
                ALTER TABLE reconnect_tokens RENAME TO reconnect_tokens_legacy;
                ",
            )?;
            db.create_reconnect_tokens_table()?;
            if can_copy_rows {
                db.conn.execute_batch(
                    "
                    INSERT INTO reconnect_tokens (token, table_code, seat_index, player_session_id)
                    SELECT token, table_code, seat_index, player_session_id
                    FROM reconnect_tokens_legacy
                    WHERE token IS NOT NULL
                      AND table_code IS NOT NULL
                    ",
                )?;
            }
            db.conn
                .execute_batch("DROP TABLE reconnect_tokens_legacy;")?;
            Ok(())
        })
    }

    fn create_tables_table(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS tables (
                table_code TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                room_json TEXT NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    fn create_reconnect_tokens_table(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS reconnect_tokens (
                token TEXT PRIMARY KEY,
                table_code TEXT NOT NULL,
                seat_index INTEGER NOT NULL,
                player_session_id INTEGER NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    fn tables_schema_is_current(&self, columns: &[SqliteColumn]) -> bool {
        columns.len() == 3
            && columns
                .iter()
                .any(|column| column.name == "table_code" && column.primary_key)
            && columns
                .iter()
                .any(|column| column.name == "created_at" && column.not_null)
            && columns
                .iter()
                .any(|column| column.name == "room_json" && column.not_null)
    }

    fn reconnect_tokens_schema_is_current(&self, columns: &[SqliteColumn]) -> bool {
        columns.len() == 4
            && columns
                .iter()
                .any(|column| column.name == "token" && column.primary_key)
            && columns
                .iter()
                .any(|column| column.name == "table_code" && column.not_null)
            && columns
                .iter()
                .any(|column| column.name == "seat_index" && column.not_null)
            && columns
                .iter()
                .any(|column| column.name == "player_session_id" && column.not_null)
    }

    fn table_columns(&self, table_name: &str) -> Result<Vec<SqliteColumn>> {
        let pragma = match table_name {
            "tables" => "PRAGMA table_info(tables)",
            "reconnect_tokens" => "PRAGMA table_info(reconnect_tokens)",
            _ => return Err(anyhow!("unsupported table inspection target: {table_name}")),
        };

        let mut statement = self.conn.prepare(pragma)?;
        let columns = statement
            .query_map([], |row| {
                Ok(SqliteColumn {
                    name: row.get(1)?,
                    not_null: row.get::<_, i64>(3)? != 0,
                    primary_key: row.get::<_, i64>(5)? != 0,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(columns)
    }

    fn with_schema_rebuild<F>(&self, context: &str, work: F) -> Result<()>
    where
        F: FnOnce(&Database) -> Result<()>,
    {
        self.conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
        if let Err(error) = self.conn.execute_batch("BEGIN IMMEDIATE;") {
            let _ = self.conn.execute_batch("PRAGMA foreign_keys = ON;");
            return Err(error)
                .with_context(|| format!("failed to start sqlite transaction for {context}"));
        }

        let result = work(self);
        match result {
            Ok(()) => {
                if let Err(error) = self.conn.execute_batch("COMMIT;") {
                    let _ = self.conn.execute_batch("ROLLBACK;");
                    let _ = self.conn.execute_batch("PRAGMA foreign_keys = ON;");
                    return Err(error).with_context(|| {
                        format!("failed to commit sqlite transaction for {context}")
                    });
                }
                self.conn.execute_batch("PRAGMA foreign_keys = ON;")?;
                Ok(())
            }
            Err(error) => {
                let _ = self.conn.execute_batch("ROLLBACK;");
                let _ = self.conn.execute_batch("PRAGMA foreign_keys = ON;");
                Err(error).with_context(|| context.to_string())
            }
        }
    }

    fn get_table(&self, table_code: &str) -> Result<Option<TableRecord>> {
        self.conn
            .query_row(
                "SELECT created_at, room_json FROM tables WHERE table_code = ?1",
                params![table_code],
                |row| {
                    Ok(TableRecord {
                        created_at: row.get(0)?,
                        room_json: row.get(1)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    fn save_table(&self, table_code: &str, created_at: &str, room_json: &str) -> Result<()> {
        self.conn.execute(
            "
            INSERT INTO tables (table_code, created_at, room_json)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(table_code) DO UPDATE
            SET created_at = excluded.created_at,
                room_json = excluded.room_json
            ",
            params![table_code, created_at, room_json],
        )?;
        Ok(())
    }

    fn delete_table(&self, table_code: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM reconnect_tokens WHERE table_code = ?1",
            params![table_code],
        )?;
        self.conn.execute(
            "DELETE FROM tables WHERE table_code = ?1",
            params![table_code],
        )?;
        Ok(())
    }

    fn store_reconnect_token(
        &self,
        token: &str,
        table_code: &str,
        seat_index: usize,
        player_session_id: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO reconnect_tokens (token, table_code, seat_index, player_session_id) VALUES (?1, ?2, ?3, ?4)",
            params![token, table_code, seat_index as i64, player_session_id],
        )?;
        Ok(())
    }

    fn get_reconnect_token(&self, token: &str) -> Result<Option<ReconnectTokenRecord>> {
        self.conn
            .query_row(
                "SELECT table_code, seat_index, player_session_id FROM reconnect_tokens WHERE token = ?1",
                params![token],
                |row| {
                    Ok(ReconnectTokenRecord {
                        table_code: row.get(0)?,
                        seat_index: row.get::<_, i64>(1)? as usize,
                        player_session_id: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    fn delete_reconnect_token(&self, token: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM reconnect_tokens WHERE token = ?1",
            params![token],
        )?;
        Ok(())
    }

    fn delete_tokens_for_seat(&self, table_code: &str, seat_index: usize) -> Result<()> {
        self.conn.execute(
            "DELETE FROM reconnect_tokens WHERE table_code = ?1 AND seat_index = ?2",
            params![table_code, seat_index as i64],
        )?;
        Ok(())
    }
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
    let db = Database::open(&settings.database_path)?;

    let app_state = AppContext {
        settings: settings.clone(),
        next_connection_id: Arc::new(AtomicU64::new(1)),
        inner: Arc::new(Mutex::new(AppState {
            db,
            rooms: HashMap::new(),
        })),
    };

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

    let mut inner = state.inner.lock().await;
    let result = create_or_replace_table_locked(
        &mut inner,
        requested_code,
        &resolved_mode,
        enforce_minimum_eight_fan,
    );
    drop(inner);

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

fn create_or_replace_table_locked(
    inner: &mut AppState,
    requested_code: Option<String>,
    mode: &str,
    enforce_minimum_eight_fan: bool,
) -> std::result::Result<(String, String, Value), CreateTableError> {
    let runtime_codes: HashSet<String> = inner.rooms.keys().cloned().collect();
    let table_code = if let Some(code) = requested_code {
        code
    } else {
        generate_table_code(&runtime_codes, &inner.db).map_err(CreateTableError::Internal)?
    };

    if let Some(record) = inner
        .db
        .get_table(&table_code)
        .map_err(CreateTableError::Internal)?
    {
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

    inner.rooms.remove(&table_code);
    let created_at = now_iso();
    let room = initial_room_payload(&table_code, mode, enforce_minimum_eight_fan);
    inner
        .db
        .save_table(
            &table_code,
            &created_at,
            &serde_json::to_string(&room)
                .map_err(|error| CreateTableError::Internal(error.into()))?,
        )
        .map_err(CreateTableError::Internal)?;
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
    let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded_channel::<String>();
    let handle = ConnectionHandle {
        id: connection_id,
        sender: outgoing_tx.clone(),
    };

    let writer = tokio::spawn(async move {
        while let Some(message) = outgoing_rx.recv().await {
            if ws_sender.send(Message::Text(message.into())).await.is_err() {
                break;
            }
        }
    });

    let mut owned_seat: Option<usize> = None;
    let mut close_socket = false;

    while let Some(next) = ws_receiver.next().await {
        let Ok(message) = next else {
            break;
        };
        let Message::Text(text) = message else {
            continue;
        };
        let envelope: ClientEnvelope = match serde_json::from_str(text.as_str()) {
            Ok(value) => value,
            Err(_) => {
                let _ = outgoing_tx.send(serialize_payload(&json!({
                    "type": "action_rejected",
                    "payload": { "reason": "unsupported_message" }
                })));
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
        if outcome.close_socket {
            close_socket = true;
            break;
        }
    }

    if !close_socket {
        handle_disconnect(state, &table_code, owned_seat, connection_id).await;
    }
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
        "join_table" => handle_join_table(state, table_code, connection, envelope).await,
        "reconnect" => handle_reconnect(state, table_code, connection, envelope).await,
        "ready" => {
            let Some(seat_index) =
                assert_active_owned_seat(&state, table_code, connection, owned_seat).await
            else {
                return reject_to(connection, "seat_not_owned");
            };
            handle_ready(state, table_code, connection, seat_index, envelope).await
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
            outbound: vec![OutboundMessage {
                sender: connection.sender.clone(),
                payload: json!({
                    "type": "heartbeat",
                    "payload": envelope.payload.clone(),
                }),
            }],
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
    let inner = state.inner.lock().await;
    let runtime = inner.rooms.get(table_code)?;
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

    let mut inner = state.inner.lock().await;
    let Some(created_at) = ensure_room_loaded_locked(&mut inner, table_code)
        .ok()
        .flatten()
    else {
        return reject_to(connection, "table_not_found");
    };
    let current_room = inner
        .rooms
        .get(table_code)
        .map(|runtime| runtime.room.clone())
        .unwrap_or_else(|| initial_room_payload(table_code, "normal", true));
    let occupied = occupied_seats(&current_room);
    let Some(seat_index) = (0..MAX_SEATS).find(|seat| !occupied.contains(seat)) else {
        return reject_to(connection, "table_full");
    };

    let player_session_id = generate_player_session_id();
    let reconnect_token = generate_reconnect_token();
    let Some(runtime) = inner.rooms.get_mut(table_code) else {
        return reject_to(connection, "table_not_found");
    };
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
    if room_mode(&runtime.room) == "test" && runtime.room.get("round_state").is_none() {
        rust_add_bot_seats_for_test(&mut runtime.room);
        rust_start_match(&mut runtime.room, 0, rand::random::<u64>());
    }
    runtime.connections.insert(seat_index, connection.clone());
    let room_to_persist = runtime.room.clone();
    if let Err(error) = persist_room_locked(&inner.db, table_code, &created_at, &room_to_persist) {
        return internal_error_to(connection, error);
    }
    if let Err(error) =
        inner
            .db
            .store_reconnect_token(&reconnect_token, table_code, seat_index, player_session_id)
    {
        return internal_error_to(connection, error);
    }

    let outbound =
        collect_join_outbound_locked(&mut inner, table_code, connection, seat_index, true);
    drop(inner);
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

    let mut inner = state.inner.lock().await;
    let Some(token_record) = inner
        .db
        .get_reconnect_token(&reconnect_token)
        .ok()
        .flatten()
    else {
        return reject_to(connection, "invalid_reconnect_token");
    };
    if token_record.table_code != table_code {
        return reject_to(connection, "table_not_found");
    }

    let Some(created_at) = ensure_room_loaded_locked(&mut inner, table_code)
        .ok()
        .flatten()
    else {
        return reject_to(connection, "table_not_found");
    };
    let current_room = inner
        .rooms
        .get(table_code)
        .map(|runtime| runtime.room.clone())
        .unwrap();
    let Some(current_session_id) = room_player_session_id(&current_room, token_record.seat_index)
    else {
        return reject_to(connection, "invalid_reconnect_token");
    };
    if current_session_id != token_record.player_session_id {
        return reject_to(connection, "invalid_reconnect_token");
    }

    let new_token = generate_reconnect_token();
    let Some(runtime) = inner.rooms.get_mut(table_code) else {
        return reject_to(connection, "table_not_found");
    };
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
    runtime
        .connections
        .insert(token_record.seat_index, connection.clone());
    let room_to_persist = runtime.room.clone();
    if let Err(error) = persist_room_locked(&inner.db, table_code, &created_at, &room_to_persist) {
        return internal_error_to(connection, error);
    }
    if let Err(error) = inner.db.delete_reconnect_token(&reconnect_token) {
        return internal_error_to(connection, error);
    }
    if let Err(error) = inner.db.store_reconnect_token(
        &new_token,
        table_code,
        token_record.seat_index,
        token_record.player_session_id,
    ) {
        return internal_error_to(connection, error);
    }

    let outbound = collect_join_outbound_locked(
        &mut inner,
        table_code,
        connection,
        token_record.seat_index,
        true,
    );
    drop(inner);
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
    let mut inner = state.inner.lock().await;
    let Some(created_at) = ensure_room_loaded_locked(&mut inner, table_code)
        .ok()
        .flatten()
    else {
        return reject_to(connection, "table_not_found");
    };
    let Some(runtime) = inner.rooms.get_mut(table_code) else {
        return reject_to(connection, "table_not_found");
    };
    if runtime.room.get("round_state").is_some()
        && !runtime.room.get("round_state").is_some_and(Value::is_null)
    {
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
    let room_to_persist = runtime.room.clone();
    if let Err(error) = persist_room_locked(&inner.db, table_code, &created_at, &room_to_persist) {
        return internal_error_to(connection, error);
    }
    let outbound = collect_snapshot_and_prompt_outbound_locked(&mut inner, table_code);
    drop(inner);
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
    let mut inner = state.inner.lock().await;
    let Some(created_at) = ensure_room_loaded_locked(&mut inner, table_code)
        .ok()
        .flatten()
    else {
        return reject_to(connection, "table_not_found");
    };
    let current_room = inner
        .rooms
        .get(table_code)
        .map(|runtime| runtime.room.clone())
        .unwrap();
    if current_room.get("round_state").is_some()
        && !current_room.get("round_state").is_some_and(Value::is_null)
    {
        return reject_to(connection, "room_already_started");
    }
    if !rust_room_ready_to_start(&current_room) {
        return reject_to(connection, "room_not_ready");
    }
    let dealer_seat = {
        let occupied: Vec<usize> = occupied_seats(&current_room).into_iter().collect();
        let mut rng = rand::rng();
        occupied[rng.random_range(0..occupied.len())]
    };
    if let Some(runtime) = inner.rooms.get_mut(table_code) {
        rust_start_match(&mut runtime.room, dealer_seat, rand::random::<u64>());
    }
    let room_to_persist = inner
        .rooms
        .get(table_code)
        .map(|runtime| runtime.room.clone())
        .unwrap();
    if let Err(error) = persist_room_locked(&inner.db, table_code, &created_at, &room_to_persist) {
        return internal_error_to(connection, error);
    }
    let outbound = collect_snapshot_and_prompt_outbound_locked(&mut inner, table_code);
    drop(inner);
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
    let mut inner = state.inner.lock().await;
    let Some(created_at) = ensure_room_loaded_locked(&mut inner, table_code)
        .ok()
        .flatten()
    else {
        return reject_to(connection, "table_not_found");
    };
    let Some(runtime) = inner.rooms.get_mut(table_code) else {
        return reject_to(connection, "table_not_found");
    };
    if let Err(reason) = rust_record_continue_action(&mut runtime.room, seat_index, action_id) {
        return reject_to(connection, &reason);
    }
    let room_to_persist = runtime.room.clone();
    if let Err(error) = persist_room_locked(&inner.db, table_code, &created_at, &room_to_persist) {
        return internal_error_to(connection, error);
    }
    let outbound = collect_snapshot_and_prompt_outbound_locked(&mut inner, table_code);
    drop(inner);
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

    let mut inner = state.inner.lock().await;
    let Some(created_at) = ensure_room_loaded_locked(&mut inner, table_code)
        .ok()
        .flatten()
    else {
        return reject_to(connection, "round_not_ready");
    };
    let mut rust_handled_messages: Option<Vec<Value>> = None;
    {
        let Some(runtime) = inner.rooms.get_mut(table_code) else {
            return reject_to(connection, "table_not_found");
        };
        if let Some(result) = try_rust_action(
            &mut runtime.room,
            seat_index,
            &action_type,
            &tile_id_strings,
        ) {
            match result {
                Ok(messages) => {
                    rust_handled_messages = Some(messages);
                }
                Err(reason) => {
                    return reject_to(connection, &reason);
                }
            }
        }
    }

    if rust_handled_messages.is_none() {
        return reject_to(connection, "invalid_action");
    }

    let (room_to_persist, connections, messages) = {
        let Some(runtime) = inner.rooms.get_mut(table_code) else {
            return reject_to(connection, "table_not_found");
        };
        let messages = rust_handled_messages.unwrap_or_default();
        (
            runtime.room.clone(),
            runtime.connections.values().cloned().collect::<Vec<_>>(),
            messages,
        )
    };
    if let Err(error) = persist_room_locked(&inner.db, table_code, &created_at, &room_to_persist) {
        return internal_error_to(connection, error);
    }
    let mut outbound = broadcast_to_handles(&connections, Some(&messages));
    outbound.extend(collect_snapshot_and_prompt_outbound_locked(
        &mut inner, table_code,
    ));
    drop(inner);
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

    let inner = state.inner.lock().await;
    let Some(runtime) = inner.rooms.get(table_code) else {
        return reject_to(connection, "table_not_found");
    };
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
    let outbound = runtime
        .connections
        .values()
        .map(|handle| OutboundMessage {
            sender: handle.sender.clone(),
            payload: payload.clone(),
        })
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
    let mut inner = state.inner.lock().await;
    let Some(created_at) = ensure_room_loaded_locked(&mut inner, table_code)
        .ok()
        .flatten()
    else {
        return reject_to(connection, "table_not_found");
    };
    inner.db.delete_tokens_for_seat(table_code, seat_index).ok();
    let current_room = inner
        .rooms
        .get(table_code)
        .map(|runtime| runtime.room.clone())
        .unwrap();
    let phase = room_phase(&current_room);

    let Some(runtime) = inner.rooms.get_mut(table_code) else {
        return reject_to(connection, "table_not_found");
    };
    runtime.connections.remove(&seat_index);
    if phase == "waiting" {
        remove_seat_from_room(&mut runtime.room, seat_index);
    } else {
        convert_seat_to_bot(&mut runtime.room, seat_index);
        let _ = rust_reconcile_continue_action_state(&mut runtime.room);
    }

    let mut outbound = vec![OutboundMessage {
        sender: connection.sender.clone(),
        payload: json!({
            "type": "leave_table_accepted",
            "payload": {
                "table_code": table_code,
                "seat_index": seat_index,
            }
        }),
    }];

    if phase == "waiting" {
        if room_seats(&runtime.room).is_empty() {
            inner.rooms.remove(table_code);
            inner.db.delete_table(table_code).ok();
        } else {
            let room_to_persist = runtime.room.clone();
            if let Err(error) =
                persist_room_locked(&inner.db, table_code, &created_at, &room_to_persist)
            {
                return internal_error_to(connection, error);
            }
            outbound.extend(presence_and_snapshot_for_all_locked(
                &mut inner, table_code, seat_index, false,
            ));
        }
    } else {
        if should_terminate_unattended(runtime) {
            inner.rooms.remove(table_code);
            inner.db.delete_table(table_code).ok();
        } else {
            let room_to_persist = runtime.room.clone();
            if let Err(error) =
                persist_room_locked(&inner.db, table_code, &created_at, &room_to_persist)
            {
                return internal_error_to(connection, error);
            }
            outbound.extend(collect_snapshot_and_prompt_outbound_locked(
                &mut inner, table_code,
            ));
        }
    }

    drop(inner);
    schedule_room_tasks(state, table_code.to_string()).await;
    MessageOutcome {
        outbound,
        owned_seat: None,
        clear_owned_seat: true,
        close_socket: true,
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

    let mut inner = state.inner.lock().await;
    let Some(created_at) = ensure_room_loaded_locked(&mut inner, table_code)
        .ok()
        .flatten()
    else {
        return;
    };
    let Some(current_handle) = inner
        .rooms
        .get(table_code)
        .and_then(|runtime| runtime.connections.get(&seat_index))
        .cloned()
    else {
        return;
    };
    if current_handle.id != connection_id {
        return;
    }
    if let Some(runtime) = inner.rooms.get_mut(table_code) {
        runtime.connections.remove(&seat_index);
        let disconnect_deadline_at = (Utc::now()
            + chrono::TimeDelta::seconds(DISCONNECT_GRACE_SECONDS))
        .to_rfc3339_opts(SecondsFormat::Micros, true);
        set_seat_connected(
            &mut runtime.room,
            seat_index,
            false,
            Some(disconnect_deadline_at),
        );
        let _ = rust_reconcile_continue_action_state(&mut runtime.room);
    }

    let Some(runtime) = inner.rooms.get_mut(table_code) else {
        return;
    };
    let room_to_persist = runtime.room.clone();
    if persist_room_locked(&inner.db, table_code, &created_at, &room_to_persist).is_err() {
        return;
    }
    let outbound = presence_and_snapshot_for_all_locked(&mut inner, table_code, seat_index, false);
    drop(inner);
    send_outbound(outbound);
    schedule_room_tasks(state, table_code.to_string()).await;
}

async fn process_due_pending_timeout(state: AppContext, table_code: String, expected_nonce: u64) {
    let mut inner = state.inner.lock().await;
    let Some(created_at) = ensure_room_loaded_locked(&mut inner, &table_code)
        .ok()
        .flatten()
    else {
        return;
    };
    let current_room = inner
        .rooms
        .get(&table_code)
        .map(|runtime| runtime.room.clone())
        .unwrap();
    let current_nonce = inner
        .rooms
        .get(&table_code)
        .map(|runtime| runtime.timeout_nonce)
        .unwrap_or_default();
    if current_nonce != expected_nonce {
        return;
    }
    let Some(deadline) = pending_timeout_deadline(&current_room) else {
        return;
    };
    if deadline > Utc::now() {
        return;
    }
    let rust_messages = {
        let Some(runtime) = inner.rooms.get_mut(&table_code) else {
            return;
        };
        try_rust_process_due_timeout(&mut runtime.room)
    };
    let Some(rust_messages) = rust_messages else {
        return;
    };
    let (room_to_persist, connections, messages) = {
        let Some(runtime) = inner.rooms.get_mut(&table_code) else {
            return;
        };
        (
            runtime.room.clone(),
            runtime.connections.values().cloned().collect::<Vec<_>>(),
            rust_messages,
        )
    };
    if persist_room_locked(&inner.db, &table_code, &created_at, &room_to_persist).is_err() {
        return;
    }
    let mut outbound = broadcast_to_handles(&connections, Some(&messages));
    outbound.extend(collect_snapshot_and_prompt_outbound_locked(
        &mut inner,
        &table_code,
    ));
    drop(inner);
    send_outbound(outbound);
    schedule_room_tasks_detached(state, table_code);
}

async fn process_due_continue_action(state: AppContext, table_code: String, expected_nonce: u64) {
    let mut inner = state.inner.lock().await;
    let Some(created_at) = ensure_room_loaded_locked(&mut inner, &table_code)
        .ok()
        .flatten()
    else {
        return;
    };
    let current_room = inner
        .rooms
        .get(&table_code)
        .map(|runtime| runtime.room.clone())
        .unwrap();
    let current_nonce = inner
        .rooms
        .get(&table_code)
        .map(|runtime| runtime.continue_nonce)
        .unwrap_or_default();
    if current_nonce != expected_nonce {
        return;
    }
    let Some(deadline) = continue_action_deadline(&current_room) else {
        return;
    };
    if deadline > Utc::now() {
        return;
    }
    let Some(runtime) = inner.rooms.get_mut(&table_code) else {
        return;
    };
    if rust_process_due_continue_action(&mut runtime.room).ok() != Some(true) {
        return;
    }
    let room_to_persist = runtime.room.clone();
    if persist_room_locked(&inner.db, &table_code, &created_at, &room_to_persist).is_err() {
        return;
    }
    let outbound = collect_snapshot_and_prompt_outbound_locked(&mut inner, &table_code);
    drop(inner);
    send_outbound(outbound);
    schedule_room_tasks_detached(state, table_code);
}

async fn process_due_disconnect_timeout(
    state: AppContext,
    table_code: String,
    seat_index: usize,
    expected_nonce: u64,
) {
    let mut inner = state.inner.lock().await;
    let Some(created_at) = ensure_room_loaded_locked(&mut inner, &table_code)
        .ok()
        .flatten()
    else {
        return;
    };
    let current_room = inner
        .rooms
        .get(&table_code)
        .map(|runtime| runtime.room.clone())
        .unwrap();
    let current_nonce = inner
        .rooms
        .get(&table_code)
        .map(|runtime| runtime.disconnect_nonce)
        .unwrap_or_default();
    if current_nonce != expected_nonce {
        return;
    }
    let Some(deadline) = disconnect_deadline_for_seat(&current_room, seat_index) else {
        return;
    };
    if deadline > Utc::now() {
        return;
    }

    inner
        .db
        .delete_tokens_for_seat(&table_code, seat_index)
        .ok();
    let (room_to_persist, should_close) = {
        let Some(runtime) = inner.rooms.get_mut(&table_code) else {
            return;
        };
        runtime.connections.remove(&seat_index);
        if runtime.room.get("round_state").is_some()
            && !runtime.room.get("round_state").is_some_and(Value::is_null)
        {
            convert_seat_to_bot(&mut runtime.room, seat_index);
            let _ = rust_reconcile_continue_action_state(&mut runtime.room);
        } else {
            remove_seat_from_room(&mut runtime.room, seat_index);
        }
        let close = room_seats(&runtime.room).is_empty() || should_terminate_unattended(runtime);
        (runtime.room.clone(), close)
    };

    if should_close {
        inner.rooms.remove(&table_code);
        inner.db.delete_table(&table_code).ok();
        return;
    }

    if persist_room_locked(&inner.db, &table_code, &created_at, &room_to_persist).is_err() {
        return;
    }
    let mut outbound = Vec::new();
    outbound.extend(collect_snapshot_and_prompt_outbound_locked(
        &mut inner,
        &table_code,
    ));
    drop(inner);
    send_outbound(outbound);
    schedule_room_tasks_detached(state, table_code);
}

async fn process_due_bot_action(state: AppContext, table_code: String, expected_nonce: u64) {
    let mut inner = state.inner.lock().await;
    let Some(created_at) = ensure_room_loaded_locked(&mut inner, &table_code)
        .ok()
        .flatten()
    else {
        return;
    };
    let current_nonce = inner
        .rooms
        .get(&table_code)
        .map(|runtime| runtime.bot_nonce)
        .unwrap_or_default();
    if current_nonce != expected_nonce {
        return;
    }

    let action = inner
        .rooms
        .get(&table_code)
        .and_then(|runtime| rust_next_bot_action(&runtime.room));
    let Some(action) = action else {
        return;
    };

    let messages = {
        let Some(runtime) = inner.rooms.get_mut(&table_code) else {
            return;
        };
        match try_rust_action(
            &mut runtime.room,
            action.seat_index,
            &action.action_type,
            &action.tile_ids,
        ) {
            Some(Ok(messages)) => messages,
            Some(Err(_)) | None => return,
        }
    };

    let (room_to_persist, connections) = {
        let Some(runtime) = inner.rooms.get_mut(&table_code) else {
            return;
        };
        (
            runtime.room.clone(),
            runtime.connections.values().cloned().collect::<Vec<_>>(),
        )
    };
    if persist_room_locked(&inner.db, &table_code, &created_at, &room_to_persist).is_err() {
        return;
    }
    let mut outbound = broadcast_to_handles(&connections, Some(&messages));
    outbound.extend(collect_snapshot_and_prompt_outbound_locked(
        &mut inner,
        &table_code,
    ));
    drop(inner);
    send_outbound(outbound);
    schedule_room_tasks_detached(state, table_code);
}

async fn schedule_room_tasks(state: AppContext, table_code: String) {
    let mut inner = state.inner.lock().await;
    let Some(runtime) = inner.rooms.get_mut(&table_code) else {
        return;
    };
    runtime.timeout_nonce = runtime.timeout_nonce.wrapping_add(1);
    runtime.continue_nonce = runtime.continue_nonce.wrapping_add(1);
    runtime.disconnect_nonce = runtime.disconnect_nonce.wrapping_add(1);
    runtime.bot_nonce = runtime.bot_nonce.wrapping_add(1);

    if let Some(deadline) = pending_timeout_deadline(&runtime.room) {
        let state_clone = state.clone();
        let table_clone = table_code.clone();
        let nonce = runtime.timeout_nonce;
        tokio::spawn(async move {
            sleep_until(deadline).await;
            process_due_pending_timeout(state_clone, table_clone, nonce).await;
        });
    }

    if let Some(deadline) = continue_action_deadline(&runtime.room) {
        let state_clone = state.clone();
        let table_clone = table_code.clone();
        let nonce = runtime.continue_nonce;
        tokio::spawn(async move {
            sleep_until(deadline).await;
            process_due_continue_action(state_clone, table_clone, nonce).await;
        });
    }

    if let Some((seat_index, deadline)) = next_disconnect_deadline(&runtime.room) {
        let state_clone = state.clone();
        let table_clone = table_code.clone();
        let nonce = runtime.disconnect_nonce;
        tokio::spawn(async move {
            sleep_until(deadline).await;
            process_due_disconnect_timeout(state_clone, table_clone, seat_index, nonce).await;
        });
    }

    if rust_next_bot_action(&runtime.room).is_some() {
        let state_clone = state.clone();
        let table_clone = table_code.clone();
        let nonce = runtime.bot_nonce;
        let delay_ms = if room_mode(&runtime.room) == "test" {
            BOT_ACTION_DELAY_TEST_MS
        } else {
            BOT_ACTION_DELAY_NORMAL_MS
        };
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            process_due_bot_action(state_clone, table_clone, nonce).await;
        });
    }
}

fn schedule_room_tasks_detached(state: AppContext, table_code: String) {
    tokio::spawn(async move {
        schedule_room_tasks(state, table_code).await;
    });
}

fn ensure_room_loaded_locked(inner: &mut AppState, table_code: &str) -> Result<Option<String>> {
    if !inner.rooms.contains_key(table_code) {
        let Some(record) = inner.db.get_table(table_code)? else {
            return Ok(None);
        };
        let mut room: Value = serde_json::from_str(&record.room_json)?;
        mark_restored_room_disconnected(&mut room);
        inner.db.save_table(
            table_code,
            &record.created_at,
            &serde_json::to_string(&room)?,
        )?;
        inner.rooms.insert(
            table_code.to_string(),
            RoomRuntime {
                room,
                connections: HashMap::new(),
                timeout_nonce: 0,
                continue_nonce: 0,
                disconnect_nonce: 0,
                bot_nonce: 0,
            },
        );
    }

    let created_at = inner
        .db
        .get_table(table_code)?
        .ok_or_else(|| anyhow!("table disappeared during restore"))?
        .created_at;
    Ok(Some(created_at))
}

fn mark_restored_room_disconnected(room: &mut Value) {
    let Some(seats) = room.get_mut("seats").and_then(Value::as_array_mut) else {
        return;
    };
    for seat in seats {
        let is_bot = seat.get("is_bot").and_then(Value::as_bool).unwrap_or(false);
        if is_bot {
            continue;
        }
        if let Some(object) = seat.as_object_mut() {
            object.insert("connected".to_string(), Value::Bool(false));
            object.insert("disconnect_deadline_at".to_string(), Value::Null);
        }
    }
}

fn find_seat_mut<'a>(
    room: &'a mut Value,
    seat_index: usize,
) -> Option<&'a mut serde_json::Map<String, Value>> {
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

fn collect_join_outbound_locked(
    inner: &mut AppState,
    table_code: &str,
    connection: &ConnectionHandle,
    seat_index: usize,
    connected: bool,
) -> Vec<OutboundMessage> {
    let mut outbound = Vec::new();
    let Some(runtime) = inner.rooms.get(table_code) else {
        return outbound;
    };
    let room = runtime.room.clone();
    let connections: Vec<(usize, ConnectionHandle)> = runtime
        .connections
        .iter()
        .map(|(seat, handle)| (*seat, handle.clone()))
        .collect();
    outbound.extend(build_room_messages_for_seat_locked(
        inner,
        &room,
        seat_index,
        connection.sender.clone(),
    ));
    if let Some(prompt) = build_prompt_for_seat_locked(inner, &room, seat_index) {
        outbound.push(OutboundMessage {
            sender: connection.sender.clone(),
            payload: prompt,
        });
    }

    let presence = json!({
        "type": "player_presence",
        "payload": {
            "table_code": table_code,
            "seat_index": seat_index,
            "connected": connected,
        }
    });
    for (other_seat, handle) in &connections {
        if *other_seat == seat_index {
            continue;
        }
        outbound.push(OutboundMessage {
            sender: handle.sender.clone(),
            payload: presence.clone(),
        });
        outbound.extend(build_room_messages_for_seat_locked(
            inner,
            &room,
            *other_seat,
            handle.sender.clone(),
        ));
    }
    for (other_seat, handle) in &connections {
        if *other_seat == seat_index {
            continue;
        }
        if let Some(prompt) = build_prompt_for_seat_locked(inner, &room, *other_seat) {
            outbound.push(OutboundMessage {
                sender: handle.sender.clone(),
                payload: prompt,
            });
        }
    }
    outbound
}

fn presence_and_snapshot_for_all_locked(
    inner: &mut AppState,
    table_code: &str,
    seat_index: usize,
    connected: bool,
) -> Vec<OutboundMessage> {
    let mut outbound = Vec::new();
    let Some(runtime) = inner.rooms.get(table_code) else {
        return outbound;
    };
    let room = runtime.room.clone();
    let connections: Vec<(usize, ConnectionHandle)> = runtime
        .connections
        .iter()
        .map(|(seat, handle)| (*seat, handle.clone()))
        .collect();
    let presence = json!({
        "type": "player_presence",
        "payload": {
            "table_code": table_code,
            "seat_index": seat_index,
            "connected": connected,
        }
    });
    for (target_seat, handle) in &connections {
        outbound.push(OutboundMessage {
            sender: handle.sender.clone(),
            payload: presence.clone(),
        });
        outbound.extend(build_room_messages_for_seat_locked(
            inner,
            &room,
            *target_seat,
            handle.sender.clone(),
        ));
        if let Some(prompt) = build_prompt_for_seat_locked(inner, &room, *target_seat) {
            outbound.push(OutboundMessage {
                sender: handle.sender.clone(),
                payload: prompt,
            });
        }
    }
    outbound
}

fn collect_snapshot_and_prompt_outbound_locked(
    inner: &mut AppState,
    table_code: &str,
) -> Vec<OutboundMessage> {
    let mut outbound = Vec::new();
    let Some(runtime) = inner.rooms.get(table_code) else {
        return outbound;
    };
    let room = runtime.room.clone();
    let connections: Vec<(usize, ConnectionHandle)> = runtime
        .connections
        .iter()
        .map(|(seat, handle)| (*seat, handle.clone()))
        .collect();
    for (seat_index, handle) in &connections {
        outbound.extend(build_room_messages_for_seat_locked(
            inner,
            &room,
            *seat_index,
            handle.sender.clone(),
        ));
    }
    for (seat_index, handle) in &connections {
        if let Some(prompt) = build_prompt_for_seat_locked(inner, &room, *seat_index) {
            outbound.push(OutboundMessage {
                sender: handle.sender.clone(),
                payload: prompt,
            });
        }
    }
    outbound
}

fn build_room_messages_for_seat_locked(
    _inner: &mut AppState,
    room: &Value,
    local_seat: usize,
    sender: mpsc::UnboundedSender<String>,
) -> Vec<OutboundMessage> {
    build_room_messages(room, local_seat)
        .into_iter()
        .map(|payload| OutboundMessage {
            sender: sender.clone(),
            payload,
        })
        .collect()
}

fn build_prompt_for_seat_locked(
    _inner: &mut AppState,
    room: &Value,
    local_seat: usize,
) -> Option<Value> {
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
            outbound.push(OutboundMessage {
                sender: handle.sender.clone(),
                payload: payload.clone(),
            });
        }
    }
    outbound
}

fn persist_room_locked(
    db: &Database,
    table_code: &str,
    created_at: &str,
    room: &Value,
) -> Result<()> {
    db.save_table(table_code, created_at, &serde_json::to_string(room)?)?;
    Ok(())
}

fn reject_to(connection: &ConnectionHandle, reason: &str) -> MessageOutcome {
    MessageOutcome {
        outbound: vec![OutboundMessage {
            sender: connection.sender.clone(),
            payload: json!({
                "type": "action_rejected",
                "payload": { "reason": reason }
            }),
        }],
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
        let _ = message.sender.send(serialize_payload(&message.payload));
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

fn should_terminate_unattended(runtime: &RoomRuntime) -> bool {
    if !runtime.connections.is_empty() {
        return false;
    }
    room_seats(&runtime.room).into_iter().all(|seat| {
        seat.get("is_bot").and_then(Value::as_bool).unwrap_or(false)
            || seat
                .get("reconnect_token")
                .map(Value::is_null)
                .unwrap_or(true)
    })
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

fn generate_table_code(existing_runtime_codes: &HashSet<String>, db: &Database) -> Result<String> {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::rng();
    for _ in 0..16 {
        let code: String = (0..6)
            .map(|_| {
                let index = rng.random_range(0..alphabet.len());
                alphabet[index] as char
            })
            .collect();
        if existing_runtime_codes.contains(&code) {
            continue;
        }
        if db.get_table(&code)?.is_none() {
            return Ok(code);
        }
    }
    Err(anyhow!("unable to generate a unique table code"))
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

    fn in_memory_database(schema_sql: &str) -> Result<Database> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(schema_sql)?;
        Ok(Database { conn })
    }

    #[test]
    fn initialize_migrates_legacy_tables_with_state_json() -> Result<()> {
        let db = in_memory_database(
            "
            CREATE TABLE tables (
                table_code TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                state_json TEXT NOT NULL
            );

            INSERT INTO tables (table_code, created_at, state_json)
            VALUES ('ROOM42', '2026-04-06T00:00:00Z', '{\"table_code\":\"ROOM42\",\"seats\":[]}');
            ",
        )?;

        db.initialize()?;

        let record = db.get_table("ROOM42")?.expect("room should be migrated");
        assert_eq!(record.created_at, "2026-04-06T00:00:00Z");
        assert_eq!(record.room_json, "{\"table_code\":\"ROOM42\",\"seats\":[]}");
        Ok(())
    }

    #[test]
    fn initialize_resets_incompatible_python_schema() -> Result<()> {
        let db = in_memory_database(
            "
            CREATE TABLE tables (
                id INTEGER PRIMARY KEY,
                table_code TEXT NOT NULL,
                phase TEXT NOT NULL,
                current_round_id TEXT,
                created_at TEXT NOT NULL
            );
            CREATE UNIQUE INDEX ix_tables_table_code ON tables(table_code);
            INSERT INTO tables (table_code, phase, current_round_id, created_at)
            VALUES ('ROOM42', 'waiting', NULL, '2026-04-06T00:00:00Z');

            CREATE TABLE player_sessions (
                id INTEGER PRIMARY KEY,
                table_id INTEGER NOT NULL,
                nickname TEXT NOT NULL,
                connected INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY(table_id) REFERENCES tables(id) ON DELETE CASCADE
            );

            CREATE TABLE reconnect_tokens (
                id INTEGER PRIMARY KEY,
                table_id INTEGER NOT NULL,
                seat_index INTEGER NOT NULL,
                player_session_id INTEGER NOT NULL,
                token TEXT NOT NULL,
                issued_at TEXT NOT NULL,
                consumed_at TEXT,
                FOREIGN KEY(table_id) REFERENCES tables(id) ON DELETE CASCADE
            );

            CREATE TABLE alembic_version (
                version_num TEXT NOT NULL
            );
            INSERT INTO alembic_version (version_num) VALUES ('0001_initial_schema');
            ",
        )?;

        db.initialize()?;

        assert!(db.get_table("ROOM42")?.is_none());

        let room_json = serde_json::to_string(&json!({
            "table_code": "ROOM99",
            "seats": []
        }))?;
        db.save_table("ROOM99", "2026-04-06T01:00:00Z", &room_json)?;
        db.store_reconnect_token("token-1", "ROOM99", 1, 42)?;

        let table = db
            .get_table("ROOM99")?
            .expect("new room should be stored after reset");
        assert_eq!(table.room_json, room_json);

        let reconnect = db
            .get_reconnect_token("token-1")?
            .expect("new reconnect token should be stored after reset");
        assert_eq!(reconnect.table_code, "ROOM99");
        assert_eq!(reconnect.seat_index, 1);
        assert_eq!(reconnect.player_session_id, 42);

        let player_sessions_exists = db
            .conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'player_sessions'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let alembic_version_exists = db
            .conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'alembic_version'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        assert!(player_sessions_exists.is_none());
        assert!(alembic_version_exists.is_none());
        Ok(())
    }
}
