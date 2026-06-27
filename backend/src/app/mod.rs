pub(crate) mod auth;
pub(crate) mod evaluation;
pub(crate) mod invites;
pub(crate) mod persistence;
pub(crate) mod protocol;
pub(crate) mod records;
pub(crate) mod room_runtime;
pub(crate) mod scheduler;
pub(crate) mod server;
#[cfg(test)]
mod server_auth_tests;
#[cfg(test)]
mod server_table_tests;
pub(crate) mod social_ws;
pub(crate) mod users;
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
use serde_json::{Value, json};
use tokio::sync::{Notify, RwLock, mpsc};

use self::persistence::DbWorker;
use self::protocol::player_presence_message;
use self::room_runtime::RoomHandle;
use crate::core::state::{PendingAction, RoomState, SeatState};
use crate::projection::match_result::match_result_message;
use crate::projection::prompt::action_prompt_message;
use crate::projection::room_snapshot::room_snapshot_message;
use crate::projection::support::build_seat_projection_support_for_state;

pub(crate) const MAX_SEATS: usize = 4;
pub(crate) const DISCONNECT_GRACE_SECONDS: i64 = 2;
pub(crate) const BOT_ACTION_DELAY_MS: u64 = 150;
pub(crate) const TIMEOUT_AUTO_RESPONSE_BOT_TAKEOVER_THRESHOLD: u8 = 3;
pub(crate) const OUTBOUND_CHANNEL_CAPACITY: usize = 128;

pub(crate) fn bot_action_delay_ms(room: &RoomState) -> u64 {
    if room.mode == crate::evaluation::EVALUATION_ROOM_MODE
        && room
            .seats
            .iter()
            .all(|seat| seat.is_bot || seat.seat_type != "human")
    {
        return 0;
    }
    BOT_ACTION_DELAY_MS
}

#[derive(Clone)]
pub(crate) struct Settings {
    pub(crate) bind_addr: String,
    pub(crate) database_path: String,
    pub(crate) cors_origins: Vec<String>,
    pub(crate) frontend_dir: Option<String>,
    pub(crate) dev_seed_user: Option<DevSeedUserSettings>,
}

#[derive(Clone)]
pub(crate) struct DevSeedUserSettings {
    pub(crate) username: String,
    pub(crate) display_name: String,
    pub(crate) password: String,
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
            dev_seed_user: dev_seed_user_settings(),
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
    pub(crate) user_connections: RwLock<HashMap<i64, HashMap<u64, ConnectionHandle>>>,
    pub(crate) special_bot_user_ids: RwLock<HashSet<i64>>,
    pub(crate) evaluation_sessions:
        RwLock<HashMap<String, self::evaluation::EvaluationSessionResponse>>,
}

impl AppContext {
    pub(crate) fn new(db: DbWorker) -> Self {
        Self {
            next_connection_id: Arc::new(AtomicU64::new(1)),
            inner: Arc::new(AppState {
                db,
                rooms: RwLock::new(HashMap::new()),
                user_connections: RwLock::new(HashMap::new()),
                special_bot_user_ids: RwLock::new(HashSet::new()),
                evaluation_sessions: RwLock::new(HashMap::new()),
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

fn dev_seed_user_settings() -> Option<DevSeedUserSettings> {
    Some(DevSeedUserSettings {
        username: optional_env_value("MAHJONG_DEV_DEFAULT_USERNAME")?,
        display_name: optional_env_value("MAHJONG_DEV_DEFAULT_DISPLAY_NAME")
            .unwrap_or_else(|| "调试账号".to_string()),
        password: optional_env_value("MAHJONG_DEV_DEFAULT_PASSWORD")?,
    })
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

#[cfg(test)]
pub(crate) fn initial_room_state(table_code: &str) -> RoomState {
    initial_room_state_with_owner(table_code, None, 1)
}

pub(crate) fn initial_room_state_with_owner(
    table_code: &str,
    owner_user_id: Option<i64>,
    multiplier: i64,
) -> RoomState {
    RoomState {
        table_code: table_code.to_string(),
        phase: "waiting".to_string(),
        mode: "normal".to_string(),
        owner_user_id,
        multiplier,
        minimum_hu_fan: crate::core::state::room::default_minimum_hu_fan(),
        dealer_repeat_enabled: false,
        dealer_double_enabled: false,
        player_multiplier_selection_enabled: false,
        ready_hand_enabled: true,
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

pub(crate) fn random_bot_seat_index(room: &RoomState) -> Option<usize> {
    let mut rng = rand::rng();
    random_bot_seat_index_with_rng(room, &mut rng)
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

pub(crate) fn random_bot_seat_index_with_rng<R: Rng + ?Sized>(
    room: &RoomState,
    rng: &mut R,
) -> Option<usize> {
    let bot_seats: Vec<_> = room
        .seats
        .iter()
        .filter(|seat| is_standalone_bot_seat(seat))
        .map(|seat| seat.seat_index)
        .collect();
    if bot_seats.is_empty() {
        return None;
    }
    Some(bot_seats[rng.random_range(0..bot_seats.len())])
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
        user_id: None,
        nickname: Some(format!("bot_{seat_index}")),
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
    room.seats.sort_by_key(|seat| seat.seat_index);
    Ok(seat_index)
}

fn is_standalone_bot_seat(seat: &SeatState) -> bool {
    crate::special_bots::is_independent_bot_seat(seat)
}

pub(crate) fn remove_bot_from_waiting_room(room: &mut RoomState) -> Result<usize, &'static str> {
    if room_phase(room) != "waiting" || room_has_round_state(room) {
        return Err("room_already_started");
    }

    let seat_index = room
        .seats
        .iter()
        .filter(|seat| is_standalone_bot_seat(seat))
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
        seat.user_id = None;
        seat.nickname = Some(format!("bot_{seat_index}"));
        seat.points = None;
        seat.title = None;
        seat.connected = true;
        seat.is_bot = true;
        seat.seat_type = "bot".to_string();
        seat.disconnect_deadline_at = None;
        seat.consecutive_timeout_auto_response_count = 0;
    }
}

pub(crate) fn mark_room_finished_if_no_human_players(room: &mut RoomState) -> bool {
    if room
        .seats
        .iter()
        .any(|seat| !seat.is_bot && seat.seat_type == "human" && seat.user_id.is_some())
    {
        return false;
    }

    if room.mode == crate::evaluation::EVALUATION_ROOM_MODE && !room_result_can_be_frozen(room) {
        return false;
    }

    room.phase = "finished".to_string();
    room.pending_timeout = None;
    room.continue_action = None;
    if let Some(match_state) = room.match_state.as_mut() {
        match_state.match_finished = true;
    }
    true
}

fn room_result_can_be_frozen(room: &RoomState) -> bool {
    room.match_state.as_ref().is_some_and(|match_state| {
        match_state.match_finished
            || match_state.statistics.completed_round_count as usize
                >= crate::evaluation::EVALUATION_HAND_COUNT
    })
}

pub(crate) fn set_seat_bot_takeover(
    room: &mut RoomState,
    seat_index: usize,
    enabled: bool,
) -> Result<(), &'static str> {
    let Some(seat) = room
        .seats
        .iter_mut()
        .find(|seat| seat.seat_index == seat_index)
    else {
        return Err("seat_not_owned");
    };

    seat.connected = true;
    seat.disconnect_deadline_at = None;
    seat.is_bot = enabled;
    seat.seat_type = "human".to_string();
    seat.consecutive_timeout_auto_response_count = 0;
    Ok(())
}

pub(crate) fn reset_timeout_auto_response_count(room: &mut RoomState, seat_index: usize) {
    if let Some(seat) = room
        .seats
        .iter_mut()
        .find(|seat| seat.seat_index == seat_index)
    {
        seat.consecutive_timeout_auto_response_count = 0;
    }
}

pub(crate) fn timeout_auto_response_seats(room: &RoomState) -> Vec<usize> {
    let Some(timeout) = room.pending_timeout.as_ref() else {
        return Vec::new();
    };
    let Some(round) = room.round_state.as_ref() else {
        return Vec::new();
    };

    match timeout.kind.as_str() {
        "active_turn" => vec![round.current_actor],
        "claim_window" => match round.pending_action.as_ref() {
            Some(PendingAction::ClaimWindow(claim)) => (1..MAX_SEATS)
                .map(|offset| (claim.discarder_seat + offset) % MAX_SEATS)
                .filter(|seat| {
                    claim
                        .claim_window
                        .get(*seat)
                        .is_some_and(|claims| !claims.is_empty())
                        && !claim.responded_seats.contains(seat)
                })
                .collect(),
            Some(PendingAction::RobKongWindow(rob)) => (1..MAX_SEATS)
                .map(|offset| (rob.actor_seat + offset) % MAX_SEATS)
                .filter(|seat| {
                    rob.offered_hu_seats.contains(seat) && !rob.responded_seats.contains(seat)
                })
                .collect(),
            Some(PendingAction::PlayerMultiplierSelection(selection)) => (0..MAX_SEATS)
                .filter(|seat| !selection.responded_seats.contains(seat))
                .collect(),
            None => Vec::new(),
        },
        _ => Vec::new(),
    }
}

pub(crate) fn record_timeout_auto_responses(room: &mut RoomState, seat_indexes: &[usize]) -> bool {
    let mut enabled_takeover = false;
    for seat_index in seat_indexes {
        let should_enable = room
            .seats
            .iter_mut()
            .find(|seat| seat.seat_index == *seat_index)
            .is_some_and(|seat| {
                if seat.seat_type != "human" || seat.is_bot {
                    return false;
                }
                seat.consecutive_timeout_auto_response_count = seat
                    .consecutive_timeout_auto_response_count
                    .saturating_add(1);
                seat.consecutive_timeout_auto_response_count
                    >= TIMEOUT_AUTO_RESPONSE_BOT_TAKEOVER_THRESHOLD
            });
        if should_enable && set_seat_bot_takeover(room, *seat_index, true).is_ok() {
            enabled_takeover = true;
        }
    }
    enabled_takeover
}

pub(crate) fn set_seat_connected(room: &mut RoomState, seat_index: usize, connected: bool) {
    if let Some(seat) = room
        .seats
        .iter_mut()
        .find(|seat| seat.seat_index == seat_index)
    {
        seat.connected = connected;
        seat.disconnect_deadline_at = None;
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

    let seat_profile = room.seats.iter().find(|seat| seat.seat_index == seat_index);
    let presence = player_presence_message(
        table_code,
        seat_index,
        connected,
        seat_profile.and_then(|seat| seat.user_id),
        seat_profile.and_then(|seat| seat.nickname.as_deref()),
        seat_profile.and_then(|seat| seat.points),
        seat_profile.and_then(|seat| seat.title.as_deref()),
    );
    outbound.push(connection.outbound(presence.clone()));
    for (other_seat, handle) in connections {
        if *other_seat == seat_index && handle.id == connection.id {
            continue;
        }
        outbound.push(handle.outbound(presence.clone()));
        outbound.extend(build_room_messages_for_seat(room, *other_seat, handle));
    }
    for (other_seat, handle) in connections {
        if *other_seat == seat_index && handle.id == connection.id {
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
    let seat_profile = room.seats.iter().find(|seat| seat.seat_index == seat_index);
    let presence = player_presence_message(
        table_code,
        seat_index,
        connected,
        seat_profile.and_then(|seat| seat.user_id),
        seat_profile.and_then(|seat| seat.nickname.as_deref()),
        seat_profile.and_then(|seat| seat.points),
        seat_profile.and_then(|seat| seat.title.as_deref()),
    );
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

fn snapshot_user_connections_registry(
    registry: &HashMap<i64, HashMap<u64, ConnectionHandle>>,
) -> (Vec<i64>, Vec<ConnectionHandle>) {
    let online_user_ids = registry.keys().copied().collect::<Vec<_>>();
    let handles = registry
        .values()
        .flat_map(|connections| connections.values().cloned())
        .collect::<Vec<_>>();
    (online_user_ids, handles)
}

fn user_presence_updated_message(mut online_user_ids: Vec<i64>) -> Value {
    online_user_ids.sort_unstable();
    online_user_ids.dedup();

    json!({
        "type": "user_presence_updated",
        "payload": {
            "online_user_ids": online_user_ids
        }
    })
}

pub(crate) fn user_active_table_updated_message(
    user_id: i64,
    active_table_code: Option<&str>,
    active_table_phase: Option<&str>,
) -> Value {
    json!({
        "type": "user_active_table_updated",
        "payload": {
            "user_id": user_id,
            "active_table_code": active_table_code,
            "active_table_phase": active_table_phase
        }
    })
}

pub(crate) async fn register_user_connection(
    state: &AppContext,
    user_id: i64,
    connection: ConnectionHandle,
) {
    let (mut online_user_ids, handles) = {
        let mut registry = state.inner.user_connections.write().await;
        registry
            .entry(user_id)
            .or_default()
            .insert(connection.id, connection);
        snapshot_user_connections_registry(&registry)
    };
    online_user_ids.extend(
        state
            .inner
            .special_bot_user_ids
            .read()
            .await
            .iter()
            .copied(),
    );
    let payload = user_presence_updated_message(online_user_ids);
    send_outbound(
        handles
            .into_iter()
            .map(|handle| handle.outbound(payload.clone()))
            .collect(),
    );
}

pub(crate) async fn unregister_user_connection(
    state: &AppContext,
    user_id: i64,
    connection_id: u64,
) {
    let (mut online_user_ids, handles) = {
        let mut registry = state.inner.user_connections.write().await;
        if let Some(connections) = registry.get_mut(&user_id) {
            connections.remove(&connection_id);
            if connections.is_empty() {
                registry.remove(&user_id);
            }
        }
        snapshot_user_connections_registry(&registry)
    };
    online_user_ids.extend(
        state
            .inner
            .special_bot_user_ids
            .read()
            .await
            .iter()
            .copied(),
    );
    let payload = user_presence_updated_message(online_user_ids);
    send_outbound(
        handles
            .into_iter()
            .map(|handle| handle.outbound(payload.clone()))
            .collect(),
    );
}

pub(crate) async fn notify_user_connections<T>(state: &AppContext, user_id: i64, payload: T)
where
    T: Serialize + Clone,
{
    let handles = {
        let registry = state.inner.user_connections.read().await;
        registry
            .get(&user_id)
            .map(|connections| connections.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    };
    send_outbound(
        handles
            .into_iter()
            .map(|handle| handle.outbound(payload.clone()))
            .collect(),
    );
}

pub(crate) async fn notify_all_user_connections<T>(state: &AppContext, payload: T)
where
    T: Serialize + Clone,
{
    let handles = {
        let registry = state.inner.user_connections.read().await;
        registry
            .values()
            .flat_map(|connections| connections.values().cloned())
            .collect::<Vec<_>>()
    };
    send_outbound(
        handles
            .into_iter()
            .map(|handle| handle.outbound(payload.clone()))
            .collect(),
    );
}

#[cfg(test)]
pub(crate) async fn online_user_ids(state: &AppContext) -> Vec<i64> {
    let registry = state.inner.user_connections.read().await;
    let mut user_ids = registry.keys().copied().collect::<Vec<_>>();
    user_ids.extend(
        state
            .inner
            .special_bot_user_ids
            .read()
            .await
            .iter()
            .copied(),
    );
    user_ids.sort_unstable();
    user_ids.dedup();
    user_ids
}

#[cfg(test)]
mod tests {
    use super::{
        BOT_ACTION_DELAY_MS, bot_action_delay_ms, convert_seat_to_bot, optional_env_value,
        record_timeout_auto_responses, remove_bot_from_waiting_room,
        reset_timeout_auto_response_count, resolve_database_path, set_seat_bot_takeover,
        timeout_auto_response_seats,
    };
    use crate::core::state::{RoomState, SeatState};
    use serde_json::json;

    #[test]
    fn bot_action_delay_defaults_to_150ms() {
        assert_eq!(BOT_ACTION_DELAY_MS, 150);
    }

    #[test]
    fn evaluation_room_with_only_bots_has_zero_bot_delay() {
        let mut room = RoomState {
            mode: crate::evaluation::EVALUATION_ROOM_MODE.to_string(),
            seats: vec![SeatState {
                seat_index: 0,
                connected: true,
                is_bot: true,
                seat_type: "bot".to_string(),
                ..Default::default()
            }],
            ..RoomState::default()
        };

        assert_eq!(bot_action_delay_ms(&room), 0);

        room.seats[0].seat_type = "human".to_string();
        room.seats[0].is_bot = false;
        assert_eq!(bot_action_delay_ms(&room), BOT_ACTION_DELAY_MS);
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

    #[test]
    fn bot_takeover_preserves_human_seat_identity() {
        let mut room = RoomState {
            table_code: "ABCD".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            minimum_hu_fan: crate::core::state::room::default_minimum_hu_fan(),
            dealer_repeat_enabled: false,
            dealer_double_enabled: false,
            player_multiplier_selection_enabled: false,
            ready_hand_enabled: true,
            seats: vec![SeatState {
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
            }],
            match_state: None,
            round_state: None,
            pending_timeout: None,
            continue_action: None,
        };

        set_seat_bot_takeover(&mut room, 0, true).expect("takeover should turn on");
        let seat = room.seats.first().expect("seat should remain");
        assert!(seat.is_bot);
        assert_eq!(seat.seat_type, "human");
        assert_eq!(seat.nickname.as_deref(), Some("Alice"));

        set_seat_bot_takeover(&mut room, 0, false).expect("takeover should turn off");
        let seat = room.seats.first().expect("seat should remain");
        assert!(!seat.is_bot);
        assert_eq!(seat.seat_type, "human");
        assert_eq!(seat.nickname.as_deref(), Some("Alice"));
        assert!(seat.connected);
    }

    #[test]
    fn convert_seat_to_bot_replaces_human_name_with_bot_name() {
        let mut room = RoomState {
            table_code: "ABCD".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            minimum_hu_fan: crate::core::state::room::default_minimum_hu_fan(),
            dealer_repeat_enabled: false,
            dealer_double_enabled: false,
            player_multiplier_selection_enabled: false,
            ready_hand_enabled: true,
            seats: vec![SeatState {
                seat_index: 2,
                user_id: Some(7),
                nickname: Some("Alice".to_string()),
                points: Some(650),
                title: Some("概率论博导".to_string()),
                connected: true,
                is_bot: false,
                seat_type: "human".to_string(),
                bot_persona: None,
                bot_aggression: None,
                disconnect_deadline_at: None,
                consecutive_timeout_auto_response_count: 0,
            }],
            match_state: None,
            round_state: None,
            pending_timeout: None,
            continue_action: None,
        };

        convert_seat_to_bot(&mut room, 2);

        let seat = room.seats.first().expect("seat should remain");
        assert_eq!(seat.nickname.as_deref(), Some("bot_2"));
        assert!(seat.is_bot);
        assert_eq!(seat.seat_type, "bot");
        assert!(seat.connected);
    }

    #[test]
    fn remove_bot_from_waiting_room_ignores_human_bot_takeover_seat() {
        let mut room = RoomState {
            table_code: "ABCD".to_string(),
            phase: "waiting".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            minimum_hu_fan: crate::core::state::room::default_minimum_hu_fan(),
            dealer_repeat_enabled: false,
            dealer_double_enabled: false,
            player_multiplier_selection_enabled: false,
            ready_hand_enabled: true,
            seats: vec![
                SeatState {
                    seat_index: 0,
                    user_id: None,
                    nickname: Some("Alice".to_string()),
                    points: None,
                    title: None,
                    connected: true,
                    is_bot: true,
                    seat_type: "human".to_string(),
                    bot_persona: None,
                    bot_aggression: None,
                    disconnect_deadline_at: None,
                    consecutive_timeout_auto_response_count: 0,
                },
                SeatState {
                    seat_index: 1,
                    user_id: None,
                    nickname: Some("bot_1".to_string()),
                    points: None,
                    title: None,
                    connected: true,
                    is_bot: true,
                    seat_type: "bot".to_string(),
                    bot_persona: None,
                    bot_aggression: None,
                    disconnect_deadline_at: None,
                    consecutive_timeout_auto_response_count: 0,
                },
            ],
            match_state: None,
            round_state: None,
            pending_timeout: None,
            continue_action: None,
        };

        let removed =
            remove_bot_from_waiting_room(&mut room).expect("standalone bot should be removed");

        assert_eq!(removed, 1);
        assert_eq!(room.seats.len(), 1);
        assert_eq!(room.seats[0].seat_index, 0);
        assert_eq!(room.seats[0].seat_type, "human");
    }

    #[test]
    fn record_timeout_auto_responses_enables_takeover_on_third_human_timeout() {
        let mut room = RoomState {
            table_code: "ABCD".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            minimum_hu_fan: crate::core::state::room::default_minimum_hu_fan(),
            dealer_repeat_enabled: false,
            dealer_double_enabled: false,
            player_multiplier_selection_enabled: false,
            ready_hand_enabled: true,
            seats: vec![SeatState {
                seat_index: 0,
                nickname: Some("Alice".to_string()),
                connected: true,
                seat_type: "human".to_string(),
                ..Default::default()
            }],
            match_state: None,
            round_state: None,
            pending_timeout: None,
            continue_action: None,
        };

        assert!(!record_timeout_auto_responses(&mut room, &[0]));
        assert!(!room.seats[0].is_bot);
        assert_eq!(room.seats[0].consecutive_timeout_auto_response_count, 1);

        assert!(!record_timeout_auto_responses(&mut room, &[0]));
        assert!(!room.seats[0].is_bot);
        assert_eq!(room.seats[0].consecutive_timeout_auto_response_count, 2);

        assert!(record_timeout_auto_responses(&mut room, &[0]));
        assert!(room.seats[0].is_bot);
        assert_eq!(room.seats[0].seat_type, "human");
        assert_eq!(room.seats[0].consecutive_timeout_auto_response_count, 0);
    }

    #[test]
    fn reset_timeout_auto_response_count_clears_human_action_streak() {
        let mut room = RoomState {
            table_code: "ABCD".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            minimum_hu_fan: crate::core::state::room::default_minimum_hu_fan(),
            dealer_repeat_enabled: false,
            dealer_double_enabled: false,
            player_multiplier_selection_enabled: false,
            ready_hand_enabled: true,
            seats: vec![SeatState {
                seat_index: 0,
                nickname: Some("Alice".to_string()),
                connected: true,
                seat_type: "human".to_string(),
                consecutive_timeout_auto_response_count: 1,
                ..Default::default()
            }],
            match_state: None,
            round_state: None,
            pending_timeout: None,
            continue_action: None,
        };

        reset_timeout_auto_response_count(&mut room, 0);

        assert_eq!(room.seats[0].consecutive_timeout_auto_response_count, 0);
    }

    #[test]
    fn timeout_auto_response_seats_include_unanswered_claim_window_seats() {
        let room = RoomState::from_room_value(&json!({
            "table_code": "ABCD",
            "phase": "playing",
            "mode": "normal",
            "seats": [],
            "round_state": {
                "current_actor": 0,
                "pending_action": {
                    "type": "claim_window",
                    "discarder_seat": 0,
                    "claim_window": [[], ["pung"], ["kong", "hu"], ["chow"]],
                    "responded_seats": [2],
                    "claim_responses": []
                }
            },
            "pending_timeout": {
                "kind": "claim_window",
                "seat_index": 0,
                "deadline_at": "2026-05-08T00:00:00Z",
                "drawn_tile_id": null
            }
        }))
        .expect("room should parse");

        assert_eq!(timeout_auto_response_seats(&room), vec![1, 3]);
    }
}
