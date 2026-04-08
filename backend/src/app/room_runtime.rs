use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use serde_json::Value;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::AppContext;
use crate::ConnectionHandle;
use crate::app::scheduler::schedule_room_tasks;
use crate::disconnect_deadline_iso;
use crate::mahjong::reconcile_continue_action_state as rust_reconcile_continue_action_state;
use crate::serialize_room;

pub(crate) type SeatConnections = Vec<(usize, ConnectionHandle)>;

pub(crate) struct RoomHandle {
    closed: AtomicBool,
    pub(crate) persist: Mutex<()>,
    pub(crate) runtime: Mutex<RoomRuntime>,
}

pub(crate) type RoomRef = Arc<RoomHandle>;

pub(crate) struct RoomRuntime {
    pub(crate) created_at: String,
    pub(crate) room: Value,
    pub(crate) connections: HashMap<usize, ConnectionHandle>,
    pub(crate) timeout_nonce: u64,
    pub(crate) continue_nonce: u64,
    pub(crate) disconnect_nonce: u64,
    pub(crate) bot_nonce: u64,
    pub(crate) timeout_task: Option<JoinHandle<()>>,
    pub(crate) continue_task: Option<JoinHandle<()>>,
    pub(crate) disconnect_task: Option<JoinHandle<()>>,
    pub(crate) bot_task: Option<JoinHandle<()>>,
}

impl RoomRuntime {
    pub(crate) fn new(created_at: String, room: Value) -> Self {
        Self {
            created_at,
            room,
            connections: HashMap::new(),
            timeout_nonce: 0,
            continue_nonce: 0,
            disconnect_nonce: 0,
            bot_nonce: 0,
            timeout_task: None,
            continue_task: None,
            disconnect_task: None,
            bot_task: None,
        }
    }
}

impl RoomHandle {
    pub(crate) fn new(runtime: RoomRuntime) -> Self {
        Self {
            closed: AtomicBool::new(false),
            persist: Mutex::new(()),
            runtime: Mutex::new(runtime),
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    pub(crate) fn mark_closed(&self) {
        self.closed.store(true, Ordering::Relaxed);
    }
}

pub(crate) fn abort_join_handle(handle: &mut Option<JoinHandle<()>>) {
    if let Some(handle) = handle.take() {
        handle.abort();
    }
}

pub(crate) fn abort_room_tasks(runtime: &mut RoomRuntime) {
    abort_join_handle(&mut runtime.timeout_task);
    abort_join_handle(&mut runtime.continue_task);
    abort_join_handle(&mut runtime.disconnect_task);
    abort_join_handle(&mut runtime.bot_task);
}

pub(crate) fn close_runtime(runtime: &mut RoomRuntime) {
    for connection in runtime.connections.values() {
        connection.request_close();
    }
    runtime.connections.clear();
    abort_room_tasks(runtime);
}

pub(crate) async fn room_handle(state: &AppContext, table_code: &str) -> Option<RoomRef> {
    let rooms = state.inner.rooms.read().await;
    rooms.get(table_code).cloned()
}

pub(crate) async fn ensure_room_loaded(
    state: &AppContext,
    table_code: &str,
) -> Result<Option<RoomRef>> {
    if let Some(handle) = room_handle(state, table_code).await {
        return Ok(Some(handle));
    }

    let record = state.inner.db.get_table(table_code).await?;
    let Some(record) = record else {
        return Ok(None);
    };

    let mut room: Value = serde_json::from_str(&record.room_json)?;
    mark_restored_room_disconnected(&mut room);
    let _ = rust_reconcile_continue_action_state(&mut room);
    state
        .inner
        .db
        .save_table(table_code, &record.created_at, &serialize_room(&room)?)
        .await?;

    let handle = Arc::new(RoomHandle::new(RoomRuntime::new(record.created_at, room)));

    let mut rooms = state.inner.rooms.write().await;
    if let Some(existing) = rooms.get(table_code).cloned() {
        return Ok(Some(existing));
    }
    rooms.insert(table_code.to_string(), handle.clone());
    Ok(Some(handle))
}

pub(crate) async fn restore_persisted_rooms(state: &AppContext) {
    let table_codes = match state.inner.db.list_table_codes().await {
        Ok(table_codes) => table_codes,
        Err(error) => {
            eprintln!("failed to list persisted rooms during startup restore: {error:#}");
            return;
        }
    };

    for table_code in table_codes {
        match ensure_room_loaded(state, &table_code).await {
            Ok(Some(_)) => schedule_room_tasks(state.clone(), table_code).await,
            Ok(None) => {}
            Err(error) => {
                eprintln!("failed to restore persisted room during startup: {error:#}");
            }
        }
    }
}

pub(crate) async fn close_room_handle(room_handle: &RoomHandle) {
    room_handle.mark_closed();
    let _persist_guard = room_handle.persist.lock().await;
    let mut runtime = room_handle.runtime.lock().await;
    close_runtime(&mut runtime);
}

pub(crate) async fn restore_room_snapshot(room_handle: &RoomHandle, room: Value) {
    let mut runtime = room_handle.runtime.lock().await;
    runtime.room = room;
}

pub(crate) async fn unregister_room_handle(
    state: &AppContext,
    table_code: &str,
    room_handle: &RoomRef,
) {
    let mut rooms = state.inner.rooms.write().await;
    if rooms
        .get(table_code)
        .is_some_and(|current| Arc::ptr_eq(current, room_handle))
    {
        rooms.remove(table_code);
    }
}

pub(crate) fn mark_restored_room_disconnected(room: &mut Value) {
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
            object.insert(
                "disconnect_deadline_at".to_string(),
                Value::String(disconnect_deadline_iso()),
            );
        }
    }
}

pub(crate) fn replace_connection(
    runtime: &mut RoomRuntime,
    seat_index: usize,
    connection: &ConnectionHandle,
) {
    if let Some(previous) = runtime.connections.insert(seat_index, connection.clone()) {
        if previous.id != connection.id {
            previous.request_close();
        }
    }
}

pub(crate) fn snapshot_connections(runtime: &RoomRuntime) -> SeatConnections {
    runtime
        .connections
        .iter()
        .map(|(seat, handle)| (*seat, handle.clone()))
        .collect()
}

pub(crate) fn room_has_only_bots(room: &Value) -> bool {
    let seats = crate::room_seats(room);
    !seats.is_empty()
        && seats
            .into_iter()
            .all(|seat| seat.get("is_bot").and_then(Value::as_bool).unwrap_or(false))
}

pub(crate) fn should_terminate_unattended(runtime: &RoomRuntime) -> bool {
    if !runtime.connections.is_empty() {
        return false;
    }
    crate::room_seats(&runtime.room).into_iter().all(|seat| {
        seat.get("is_bot").and_then(Value::as_bool).unwrap_or(false)
            || seat
                .get("reconnect_token")
                .map(Value::is_null)
                .unwrap_or(true)
    })
}
