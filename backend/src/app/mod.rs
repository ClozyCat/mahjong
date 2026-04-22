pub(crate) mod persistence;
pub(crate) mod protocol;
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
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{Notify, RwLock, mpsc};

use self::persistence::DbWorker;
use self::protocol::player_presence_message;
use self::room_runtime::RoomHandle;
use crate::core::state::{RoomState, SeatState};
use crate::projection::match_result::match_result_message;
use crate::projection::prompt::action_prompt_message;
use crate::projection::room_snapshot::room_snapshot_message;
use crate::projection::support::build_seat_projection_support_for_state;

pub(crate) const MAX_SEATS: usize = 4;
pub(crate) const DISCONNECT_GRACE_SECONDS: i64 = 120;
pub(crate) const BOT_ACTION_DELAY_MS: u64 = 300;
pub(crate) const OUTBOUND_CHANNEL_CAPACITY: usize = 128;

#[derive(Clone)]
pub(crate) struct Settings {
    pub(crate) bind_addr: String,
    pub(crate) database_path: String,
    pub(crate) cors_origins: Vec<String>,
    pub(crate) frontend_dir: Option<String>,
}

impl Settings {
    pub(crate) fn from_env() -> Result<Self> {
        Ok(Self {
            bind_addr: env::var("MAHJONG_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8000".to_string()),
            database_path: resolve_database_path(
                &env::var("MAHJONG_DATABASE_URL")
                    .unwrap_or_else(|_| "sqlite+pysqlite:////data/mahjong.db".to_string()),
            ),
            cors_origins: dev_cors_origins(),
            frontend_dir: optional_env_value("MAHJONG_FRONTEND_DIR"),
        })
    }
}

#[derive(Clone)]
pub(crate) struct AppContext {
    pub(crate) next_connection_id: Arc<AtomicU64>,
    pub(crate) inner: Arc<AppState>,
}

pub(crate) struct AppState {
    pub(crate) db: DbWorker,
    pub(crate) rooms: RwLock<HashMap<String, Arc<RoomHandle>>>,
}

impl AppContext {
    pub(crate) fn new(db: DbWorker) -> Self {
        Self {
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
    pub(crate) fn outbound<T: Serialize>(&self, payload: T) -> OutboundMessage {
        OutboundMessage {
            connection: self.clone(),
            payload: serialize_payload(&payload),
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
    pub(crate) payload: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateTableRequest {
    pub(crate) table_code: Option<String>,
}

pub(crate) fn optional_env_value(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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

pub(crate) fn serialize_room(room: &RoomState) -> Result<String> {
    serialize_room_state(room)
}

pub(crate) fn parse_room_json(room_json: &str) -> Result<RoomState> {
    serde_json::from_str(room_json).map_err(Into::into)
}

pub(crate) fn serialize_room_state(state: &RoomState) -> Result<String> {
    serde_json::to_string(state).map_err(Into::into)
}

pub(crate) fn serialize_payload<T: Serialize>(payload: &T) -> String {
    serde_json::to_string(payload).unwrap_or_else(|_| {
        serde_json::to_string(&self::protocol::action_rejected_message(
            "serialization_error",
        ))
        .unwrap_or_else(|_| {
            "{\"type\":\"action_rejected\",\"payload\":{\"reason\":\"serialization_error\"}}"
                .to_string()
        })
    })
}

pub(crate) fn initial_room_state(table_code: &str) -> RoomState {
    RoomState {
        table_code: table_code.to_string(),
        phase: "waiting".to_string(),
        mode: "normal".to_string(),
        seats: Vec::new(),
        match_state: None,
        round_state: None,
        pending_timeout: None,
        continue_action: None,
    }
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

pub(crate) fn room_phase(room: &RoomState) -> String {
    room.phase.clone()
}

pub(crate) fn room_has_round_state(room: &RoomState) -> bool {
    room.round_state.is_some()
}

pub(crate) fn room_seats(room: &RoomState) -> &[SeatState] {
    &room.seats
}

pub(crate) fn occupied_seats(room: &RoomState) -> HashSet<usize> {
    room.seats.iter().map(|seat| seat.seat_index).collect()
}

pub(crate) fn room_player_session_id(room: &RoomState, seat_index: usize) -> Option<i64> {
    room.seats
        .iter()
        .find(|seat| seat.seat_index == seat_index)
        .and_then(|seat| seat.player_session_id)
}

pub(crate) fn room_reconnect_token(room: &RoomState, seat_index: usize) -> Option<String> {
    room.seats
        .iter()
        .find(|seat| seat.seat_index == seat_index)
        .and_then(|seat| seat.reconnect_token.clone())
}

pub(crate) fn seat_matches_reconnect_credentials(
    room: &RoomState,
    seat_index: usize,
    player_session_id: i64,
    reconnect_token: &str,
) -> bool {
    room_player_session_id(room, seat_index) == Some(player_session_id)
        && room_reconnect_token(room, seat_index).as_deref() == Some(reconnect_token)
}

pub(crate) fn pending_timeout_deadline(room: &RoomState) -> Option<DateTime<Utc>> {
    room.pending_timeout
        .as_ref()
        .and_then(|timeout| timeout.deadline_at.as_deref())
        .as_deref()
        .and_then(parse_datetime)
}

pub(crate) fn continue_action_deadline(room: &RoomState) -> Option<DateTime<Utc>> {
    room.continue_action
        .as_ref()
        .and_then(|action| action.auto_advance_deadline_at.as_deref())
        .as_deref()
        .and_then(parse_datetime)
}

pub(crate) fn disconnect_deadline_for_seat(
    room: &RoomState,
    seat_index: usize,
) -> Option<DateTime<Utc>> {
    room.seats
        .iter()
        .find(|seat| seat.seat_index == seat_index)
        .and_then(|seat| seat.disconnect_deadline_at.as_deref())
        .as_deref()
        .and_then(parse_datetime)
}

pub(crate) fn next_disconnect_deadline(room: &RoomState) -> Option<(usize, DateTime<Utc>)> {
    room.seats
        .iter()
        .filter(|seat| !seat.is_bot && !seat.connected)
        .filter_map(|seat| {
            seat.disconnect_deadline_at
                .as_deref()
                .and_then(parse_datetime)
                .map(|deadline| (seat.seat_index, deadline))
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

pub(crate) fn remove_seat_from_room(room: &mut RoomState, seat_index: usize) {
    if let Some(index) = room
        .seats
        .iter()
        .position(|seat| seat.seat_index == seat_index)
    {
        room.seats.remove(index);
    }
}

pub(crate) fn seat_exists(room: &RoomState, seat_index: usize) -> bool {
    room.seats.iter().any(|seat| seat.seat_index == seat_index)
}

pub(crate) fn first_open_seat_index(room: &RoomState) -> Option<usize> {
    let occupied = occupied_seats(room);
    (0..MAX_SEATS).find(|seat_index| !occupied.contains(seat_index))
}

pub(crate) fn random_open_seat_index(room: &RoomState) -> Option<usize> {
    let mut rng = rand::rng();
    random_open_seat_index_with_rng(room, &mut rng)
}

pub(crate) fn random_open_seat_index_with_rng<R: Rng + ?Sized>(
    room: &RoomState,
    rng: &mut R,
) -> Option<usize> {
    let occupied = occupied_seats(room);
    let open_seats: Vec<_> = (0..MAX_SEATS)
        .filter(|seat_index| !occupied.contains(seat_index))
        .collect();
    if open_seats.is_empty() {
        return None;
    }
    Some(open_seats[rng.random_range(0..open_seats.len())])
}

pub(crate) fn add_bot_to_waiting_room(room: &mut RoomState) -> Result<usize, &'static str> {
    if room_phase(room) != "waiting" || room_has_round_state(room) {
        return Err("room_already_started");
    }

    let Some(seat_index) = first_open_seat_index(room) else {
        return Err("room_full");
    };
    room.seats.push(SeatState {
        seat_index,
        nickname: Some(format!("Bot {seat_index}")),
        reconnect_token: None,
        player_session_id: Some(-((seat_index as i64) + 1)),
        connected: true,
        ready: true,
        is_bot: true,
        seat_type: "bot".to_string(),
        bot_persona: None,
        bot_aggression: None,
        disconnect_deadline_at: None,
    });
    room.seats.sort_by_key(|seat| seat.seat_index);
    Ok(seat_index)
}

pub(crate) fn remove_bot_from_waiting_room(room: &mut RoomState) -> Result<usize, &'static str> {
    if room_phase(room) != "waiting" || room_has_round_state(room) {
        return Err("room_already_started");
    }

    let seat_index = room
        .seats
        .iter()
        .filter(|seat| seat.is_bot)
        .map(|seat| seat.seat_index)
        .max()
        .ok_or("bot_not_found")?;
    remove_seat_from_room(room, seat_index);
    Ok(seat_index)
}

pub(crate) fn convert_seat_to_bot(room: &mut RoomState, seat_index: usize) {
    if let Some(seat) = room
        .seats
        .iter_mut()
        .find(|seat| seat.seat_index == seat_index)
    {
        seat.connected = true;
        seat.ready = true;
        seat.is_bot = true;
        seat.seat_type = "bot".to_string();
        seat.reconnect_token = None;
        seat.disconnect_deadline_at = None;
    }
}

pub(crate) fn set_seat_connected(
    room: &mut RoomState,
    seat_index: usize,
    connected: bool,
    deadline_at: Option<String>,
) {
    if let Some(seat) = room
        .seats
        .iter_mut()
        .find(|seat| seat.seat_index == seat_index)
    {
        seat.connected = connected;
        seat.disconnect_deadline_at = deadline_at;
    }
}

pub(crate) fn collect_join_outbound_from_snapshot(
    room: &RoomState,
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

    let presence = player_presence_message(table_code, seat_index, connected);
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
    room: &RoomState,
    connections: &[(usize, ConnectionHandle)],
    table_code: &str,
    seat_index: usize,
    connected: bool,
) -> Vec<OutboundMessage> {
    let mut outbound = Vec::new();
    let presence = player_presence_message(table_code, seat_index, connected);
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
    room: &RoomState,
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
    room: &RoomState,
    local_seat: usize,
    connection: &ConnectionHandle,
) -> Vec<OutboundMessage> {
    let support = build_seat_projection_support_for_state(room, local_seat);
    let mut payloads = vec![room_snapshot_message(room, local_seat, &support)];
    if let Some(result) = match_result_message(room) {
        payloads.push(result);
    }
    payloads
        .into_iter()
        .map(|payload| connection.outbound(payload))
        .collect()
}

pub(crate) fn build_prompt_for_seat(room: &RoomState, local_seat: usize) -> Option<Value> {
    let support = build_seat_projection_support_for_state(room, local_seat);
    action_prompt_message(room, local_seat, &support)
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
        if let Err(error) = message.connection.try_send(message.payload) {
            if matches!(error, mpsc::error::TrySendError::Full(_)) {
                message.connection.request_close();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BOT_ACTION_DELAY_MS, optional_env_value, resolve_database_path};

    #[test]
    fn bot_action_delay_defaults_to_300ms() {
        assert_eq!(BOT_ACTION_DELAY_MS, 300);
    }

    #[test]
    fn optional_env_value_trims_whitespace_and_drops_empty_values() {
        unsafe {
            std::env::set_var("MAHJONG_OPTIONAL_ENV_VALUE_TEST", "  C:/mahjong/web  ");
        }
        assert_eq!(
            optional_env_value("MAHJONG_OPTIONAL_ENV_VALUE_TEST").as_deref(),
            Some("C:/mahjong/web")
        );

        unsafe {
            std::env::set_var("MAHJONG_OPTIONAL_ENV_VALUE_TEST", "   ");
        }
        assert_eq!(optional_env_value("MAHJONG_OPTIONAL_ENV_VALUE_TEST"), None);

        unsafe {
            std::env::remove_var("MAHJONG_OPTIONAL_ENV_VALUE_TEST");
        }
    }

    #[test]
    fn resolve_database_path_accepts_prefixed_or_plain_sqlite_paths() {
        assert_eq!(
            resolve_database_path("sqlite+pysqlite:////data/mahjong.db"),
            "/data/mahjong.db"
        );
        assert_eq!(
            resolve_database_path("sqlite:///C:/mahjong/data/mahjong.db"),
            "C:/mahjong/data/mahjong.db"
        );
        assert_eq!(
            resolve_database_path("C:/mahjong/data/mahjong.db"),
            "C:/mahjong/data/mahjong.db"
        );
    }
}
