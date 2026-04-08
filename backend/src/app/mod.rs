pub(crate) mod persistence;
pub(crate) mod room_runtime;
pub(crate) mod scheduler;
pub(crate) mod server;
pub(crate) mod ws;

use std::collections::{HashMap, HashSet};
use std::env;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, SecondsFormat, Utc};
use rand::Rng;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{Notify, RwLock, mpsc};

use self::persistence::DbWorker;
use self::room_runtime::RoomHandle;
use crate::mahjong::{
    action_prompt as build_action_prompt, add_bot_seats_for_test as rust_add_bot_seats_for_test,
    room_messages as build_room_messages, start_match as rust_start_match,
};

pub(crate) const MAX_SEATS: usize = 4;
pub(crate) const DISCONNECT_GRACE_SECONDS: i64 = 120;
pub(crate) const BOT_ACTION_DELAY_TEST_MS: u64 = 0;
pub(crate) const BOT_ACTION_DELAY_NORMAL_MS: u64 = 600;
pub(crate) const OUTBOUND_CHANNEL_CAPACITY: usize = 128;

#[derive(Clone)]
pub(crate) struct Settings {
    pub(crate) bind_addr: String,
    pub(crate) database_path: String,
    pub(crate) default_test_mode: bool,
    pub(crate) cors_origins: Vec<String>,
}

impl Settings {
    pub(crate) fn from_env() -> Result<Self> {
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
pub(crate) struct AppContext {
    pub(crate) settings: Settings,
    pub(crate) next_connection_id: Arc<AtomicU64>,
    pub(crate) inner: Arc<AppState>,
}

pub(crate) struct AppState {
    pub(crate) db: DbWorker,
    pub(crate) rooms: RwLock<HashMap<String, Arc<RoomHandle>>>,
}

impl AppContext {
    pub(crate) fn new(settings: Settings, db: DbWorker) -> Self {
        Self {
            settings,
            next_connection_id: Arc::new(AtomicU64::new(1)),
            inner: Arc::new(AppState {
                db,
                rooms: RwLock::new(HashMap::new()),
            }),
        }
    }
}

#[derive(Clone)]
pub(crate) struct ConnectionHandle {
    pub(crate) id: u64,
    pub(crate) sender: mpsc::Sender<String>,
    pub(crate) close_requested: Arc<AtomicBool>,
    pub(crate) close_notify: Arc<Notify>,
}

impl ConnectionHandle {
    pub(crate) fn outbound(&self, payload: Value) -> OutboundMessage {
        OutboundMessage {
            connection: self.clone(),
            payload,
        }
    }

    pub(crate) fn try_send(
        &self,
        message: String,
    ) -> Result<(), mpsc::error::TrySendError<String>> {
        self.sender.try_send(message)
    }

    pub(crate) fn request_close(&self) {
        self.close_requested.store(true, Ordering::Relaxed);
        self.close_notify.notify_waiters();
    }

    pub(crate) fn should_close(&self) -> bool {
        self.close_requested.load(Ordering::Relaxed)
    }
}

pub(crate) struct OutboundMessage {
    pub(crate) connection: ConnectionHandle,
    pub(crate) payload: Value,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateTableRequest {
    pub(crate) table_code: Option<String>,
    pub(crate) mode: Option<String>,
    pub(crate) test_mode: Option<bool>,
    pub(crate) enforce_minimum_eight_fan: Option<bool>,
}

pub(crate) fn parse_bool_env(key: &str) -> bool {
    matches!(
        env::var(key).ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON")
    )
}

pub(crate) fn dev_cors_origins() -> Vec<String> {
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

pub(crate) fn resolve_database_path(database_url: &str) -> String {
    for prefix in ["sqlite+pysqlite:///", "sqlite:///"] {
        if let Some(rest) = database_url.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    database_url.to_string()
}

pub(crate) fn serialize_room(room: &Value) -> Result<String> {
    serde_json::to_string(room).map_err(Into::into)
}

pub(crate) fn serialize_payload(payload: &Value) -> String {
    serde_json::to_string(payload).unwrap_or_else(|_| {
        "{\"type\":\"action_rejected\",\"payload\":{\"reason\":\"serialization_error\"}}"
            .to_string()
    })
}

pub(crate) fn initial_room_payload(
    table_code: &str,
    mode: &str,
    enforce_minimum_eight_fan: bool,
) -> Value {
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

pub(crate) fn normalize_table_code(table_code: &str) -> String {
    table_code.trim().to_ascii_uppercase()
}

pub(crate) fn is_valid_table_code(table_code: &str) -> bool {
    !table_code.is_empty()
        && table_code.len() <= 12
        && table_code
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
}

pub(crate) fn room_mode(room: &Value) -> String {
    room.get("mode")
        .and_then(Value::as_str)
        .unwrap_or("normal")
        .to_string()
}

pub(crate) fn room_phase(room: &Value) -> String {
    room.get("phase")
        .and_then(Value::as_str)
        .unwrap_or("waiting")
        .to_string()
}

pub(crate) fn room_has_round_state(room: &Value) -> bool {
    room.get("round_state")
        .is_some_and(|state| !state.is_null())
}

pub(crate) fn room_seats(room: &Value) -> Vec<Value> {
    room.get("seats")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn occupied_seats(room: &Value) -> HashSet<usize> {
    room_seats(room)
        .into_iter()
        .filter_map(|seat| {
            seat.get("seat_index")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
        })
        .collect()
}

pub(crate) fn room_player_session_id(room: &Value, seat_index: usize) -> Option<i64> {
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

pub(crate) fn room_reconnect_token(room: &Value, seat_index: usize) -> Option<&str> {
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

pub(crate) fn seat_matches_reconnect_credentials(
    room: &Value,
    seat_index: usize,
    player_session_id: i64,
    reconnect_token: &str,
) -> bool {
    room_player_session_id(room, seat_index) == Some(player_session_id)
        && room_reconnect_token(room, seat_index) == Some(reconnect_token)
}

pub(crate) fn pending_timeout_deadline(room: &Value) -> Option<DateTime<Utc>> {
    room.get("pending_timeout")
        .and_then(|value| value.get("deadline_at"))
        .and_then(Value::as_str)
        .and_then(parse_datetime)
}

pub(crate) fn continue_action_deadline(room: &Value) -> Option<DateTime<Utc>> {
    room.get("continue_action_auto_advance_deadline_at")
        .and_then(Value::as_str)
        .and_then(parse_datetime)
}

pub(crate) fn disconnect_deadline_for_seat(
    room: &Value,
    seat_index: usize,
) -> Option<DateTime<Utc>> {
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

pub(crate) fn next_disconnect_deadline(room: &Value) -> Option<(usize, DateTime<Utc>)> {
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

pub(crate) fn parse_datetime(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
                .map(|value| DateTime::<Utc>::from_naive_utc_and_offset(value, Utc))
        })
        .ok()
}

pub(crate) async fn sleep_until(deadline: DateTime<Utc>) {
    let now = Utc::now();
    let duration = if deadline > now {
        (deadline - now).to_std().unwrap_or(Duration::from_secs(0))
    } else {
        Duration::from_secs(0)
    };
    tokio::time::sleep(duration).await;
}

pub(crate) fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true)
}

pub(crate) fn disconnect_deadline_iso() -> String {
    (Utc::now() + chrono::TimeDelta::seconds(DISCONNECT_GRACE_SECONDS))
        .to_rfc3339_opts(SecondsFormat::Micros, true)
}

pub(crate) fn generate_player_session_id() -> i64 {
    let mut rng = rand::rng();
    rng.random_range(1_i64..i64::MAX)
}

pub(crate) fn generate_reconnect_token() -> String {
    generate_short_hex(32)
}

pub(crate) fn generate_short_hex(bytes: usize) -> String {
    let mut rng = rand::rng();
    let mut data = vec![0_u8; bytes];
    rng.fill(data.as_mut_slice());
    data.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn find_seat_mut(
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

pub(crate) fn remove_seat_from_room(room: &mut Value, seat_index: usize) {
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

pub(crate) fn seat_exists(room: &Value, seat_index: usize) -> bool {
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

pub(crate) fn first_open_seat_index(room: &Value) -> Option<usize> {
    let occupied = occupied_seats(room);
    (0..MAX_SEATS).find(|seat_index| !occupied.contains(seat_index))
}

pub(crate) fn add_bot_to_waiting_room(room: &mut Value) -> Result<usize, &'static str> {
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

pub(crate) fn remove_bot_from_waiting_room(room: &mut Value) -> Result<usize, &'static str> {
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

pub(crate) fn convert_seat_to_bot(room: &mut Value, seat_index: usize) {
    if let Some(seat) = find_seat_mut(room, seat_index) {
        seat.insert("connected".to_string(), Value::Bool(true));
        seat.insert("ready".to_string(), Value::Bool(true));
        seat.insert("is_bot".to_string(), Value::Bool(true));
        seat.insert("seat_type".to_string(), Value::String("bot".to_string()));
        seat.insert("reconnect_token".to_string(), Value::Null);
        seat.insert("disconnect_deadline_at".to_string(), Value::Null);
    }
}

pub(crate) fn set_seat_connected(
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

pub(crate) fn collect_join_outbound_from_snapshot(
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

pub(crate) fn presence_and_snapshot_for_all_from_snapshot(
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

pub(crate) fn collect_snapshot_and_prompt_outbound_from_snapshot(
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

pub(crate) fn build_room_messages_for_seat(
    room: &Value,
    local_seat: usize,
    connection: &ConnectionHandle,
) -> Vec<OutboundMessage> {
    build_room_messages(room, local_seat)
        .into_iter()
        .map(|payload| connection.outbound(payload))
        .collect()
}

pub(crate) fn build_prompt_for_seat(room: &Value, local_seat: usize) -> Option<Value> {
    build_action_prompt(room, local_seat)
}

pub(crate) fn broadcast_to_handles(
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

pub(crate) fn send_outbound(outbound: Vec<OutboundMessage>) {
    for message in outbound {
        let payload = serialize_payload(&message.payload);
        if let Err(error) = message.connection.try_send(payload) {
            if matches!(error, mpsc::error::TrySendError::Full(_)) {
                message.connection.request_close();
            }
        }
    }
}

pub(crate) fn maybe_start_test_match(room: &mut Value) {
    if room_mode(room) != "test" || room_has_round_state(room) {
        return;
    }

    rust_add_bot_seats_for_test(room);
    rust_start_match(room, 0, rand::random::<u64>());
}
