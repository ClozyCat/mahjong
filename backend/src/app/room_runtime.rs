use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use serde::Serialize;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::app::scheduler::schedule_room_tasks;
use crate::app::{
    AppContext, ConnectionHandle, OutboundMessage, parse_room_json, serialize_room_state,
};
use crate::core::state::RoomState;
use crate::rules::standard::flow::reconcile_continue_action_state_in_room_state as reconcile_standard_continue_action_state;

pub(crate) type SeatConnections = Vec<(usize, ConnectionHandle)>;
pub(crate) type SpectatorConnections = Vec<(u64, ConnectionHandle)>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpectatorIdentity {
    pub(crate) user_id: i64,
    pub(crate) display_name: String,
}

#[derive(Clone)]
pub(crate) struct SpectatorConnection {
    pub(crate) user_id: i64,
    pub(crate) display_name: String,
    pub(crate) connection: ConnectionHandle,
}

pub(crate) struct RoomHandle {
    closed: AtomicBool,
    pub(crate) persist: Mutex<()>,
    pub(crate) runtime: Mutex<RoomRuntime>,
}

pub(crate) type RoomRef = Arc<RoomHandle>;

#[derive(Clone)]
pub(crate) struct SeatConnectionGroup {
    pub(crate) user_id: Option<i64>,
    pub(crate) connections: HashMap<u64, ConnectionHandle>,
}

pub(crate) struct RoomRuntime {
    pub(crate) created_at: String,
    pub(crate) room: RoomState,
    pub(crate) connections: HashMap<usize, SeatConnectionGroup>,
    pub(crate) spectator_connections: HashMap<u64, SpectatorConnection>,
    pub(crate) timeout_nonce: u64,
    pub(crate) continue_nonce: u64,
    pub(crate) start_match_nonce: u64,
    pub(crate) bot_nonce: u64,
    pub(crate) unattended_cleanup_nonce: u64,
    pub(crate) timeout_task: Option<JoinHandle<()>>,
    pub(crate) continue_task: Option<JoinHandle<()>>,
    pub(crate) start_match_task: Option<JoinHandle<()>>,
    pub(crate) bot_task: Option<JoinHandle<()>>,
    pub(crate) unattended_cleanup_task: Option<JoinHandle<()>>,
    pub(crate) pending_start_match: Option<PendingStartMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingStartMatch {
    pub(crate) dealer_seat: usize,
    pub(crate) reveal_at: String,
}

impl RoomRuntime {
    pub(crate) fn new(created_at: String, room: RoomState) -> Self {
        Self {
            created_at,
            room,
            connections: HashMap::new(),
            spectator_connections: HashMap::new(),
            timeout_nonce: 0,
            continue_nonce: 0,
            start_match_nonce: 0,
            bot_nonce: 0,
            unattended_cleanup_nonce: 0,
            timeout_task: None,
            continue_task: None,
            start_match_task: None,
            bot_task: None,
            unattended_cleanup_task: None,
            pending_start_match: None,
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
    abort_join_handle(&mut runtime.start_match_task);
    abort_join_handle(&mut runtime.bot_task);
    abort_join_handle(&mut runtime.unattended_cleanup_task);
}

pub(crate) fn close_runtime(runtime: &mut RoomRuntime) {
    for group in runtime.connections.values() {
        for connection in group.connections.values() {
            connection.request_close();
        }
    }
    runtime.connections.clear();
    {
        for spectator in runtime.spectator_connections.values() {
            spectator.connection.request_close();
        }
        runtime.spectator_connections.clear();
    }
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

    let mut room = parse_room_json(&record.room_json)?;
    mark_restored_room_disconnected(&mut room);
    let _ = reconcile_standard_continue_action_state(&mut room);
    state
        .inner
        .db
        .save_table(
            table_code,
            &record.created_at,
            &serialize_room_state(&room)?,
        )
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

pub(crate) async fn restore_room_snapshot(room_handle: &RoomHandle, room: RoomState) {
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

pub(crate) fn mark_restored_room_disconnected(room: &mut RoomState) {
    for seat in &mut room.seats {
        if seat.seat_type == "bot" {
            continue;
        }
        seat.connected = false;
        seat.disconnect_deadline_at = None;
    }
}

pub(crate) fn add_seat_connection(
    runtime: &mut RoomRuntime,
    seat_index: usize,
    user_id: Option<i64>,
    connection: &ConnectionHandle,
) {
    let group = runtime
        .connections
        .entry(seat_index)
        .or_insert_with(|| SeatConnectionGroup {
            user_id,
            connections: HashMap::new(),
        });
    if group.user_id.is_none() && user_id.is_some() {
        group.user_id = user_id;
    }
    group.connections.insert(connection.id, connection.clone());
}

pub(crate) fn remove_seat_connection(
    runtime: &mut RoomRuntime,
    seat_index: usize,
    connection_id: u64,
) -> bool {
    let Some(group) = runtime.connections.get_mut(&seat_index) else {
        return false;
    };
    group.connections.remove(&connection_id);
    let still_live = !group.connections.is_empty();
    if !still_live {
        runtime.connections.remove(&seat_index);
    }
    still_live
}

pub(crate) fn remove_all_seat_connections(
    runtime: &mut RoomRuntime,
    seat_index: usize,
) -> Vec<ConnectionHandle> {
    runtime
        .connections
        .remove(&seat_index)
        .map(|group| group.connections.into_values().collect())
        .unwrap_or_default()
}

pub(crate) fn snapshot_seat_connections(
    runtime: &RoomRuntime,
    seat_index: usize,
) -> Vec<ConnectionHandle> {
    runtime
        .connections
        .get(&seat_index)
        .map(|group| group.connections.values().cloned().collect())
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) fn seat_has_live_connections(runtime: &RoomRuntime, seat_index: usize) -> bool {
    runtime
        .connections
        .get(&seat_index)
        .is_some_and(|group| !group.connections.is_empty())
}

pub(crate) fn seat_group_contains_connection(
    runtime: &RoomRuntime,
    seat_index: usize,
    connection_id: u64,
) -> bool {
    runtime
        .connections
        .get(&seat_index)
        .is_some_and(|group| group.connections.contains_key(&connection_id))
}

pub(crate) fn connection_current_seat(runtime: &RoomRuntime, connection_id: u64) -> Option<usize> {
    runtime.connections.iter().find_map(|(seat_index, group)| {
        group
            .connections
            .contains_key(&connection_id)
            .then_some(*seat_index)
    })
}

pub(crate) fn remap_connections_to_current_seats(
    runtime: &mut RoomRuntime,
    previous_room: &RoomState,
) {
    let user_to_current_seat = runtime
        .room
        .seats
        .iter()
        .filter_map(|seat| seat.user_id.map(|user_id| (user_id, seat.seat_index)))
        .collect::<HashMap<_, _>>();
    let previous_seats = previous_room
        .seats
        .iter()
        .map(|seat| (seat.seat_index, seat))
        .collect::<HashMap<_, _>>();

    let mut next_connections: HashMap<usize, SeatConnectionGroup> = HashMap::new();
    for (previous_seat, group) in runtime.connections.drain() {
        let next_seat = previous_seats
            .get(&previous_seat)
            .and_then(|seat| {
                seat.user_id
                    .and_then(|user_id| user_to_current_seat.get(&user_id).copied())
            })
            .unwrap_or(previous_seat);
        let target = next_connections
            .entry(next_seat)
            .or_insert_with(|| SeatConnectionGroup {
                user_id: group.user_id,
                connections: HashMap::new(),
            });
        if target.user_id.is_none() {
            target.user_id = group.user_id;
        }
        target.connections.extend(group.connections);
    }
    runtime.connections = next_connections;
}

pub(crate) fn broadcast_to_seat_group<T: Serialize + Clone>(
    runtime: &RoomRuntime,
    seat_index: usize,
    payload: T,
) -> Vec<OutboundMessage> {
    snapshot_seat_connections(runtime, seat_index)
        .into_iter()
        .map(|handle| handle.outbound(payload.clone()))
        .collect()
}

pub(crate) fn snapshot_connections(runtime: &RoomRuntime) -> SeatConnections {
    runtime
        .connections
        .iter()
        .flat_map(|(seat, group)| {
            group
                .connections
                .values()
                .cloned()
                .map(|handle| (*seat, handle))
                .collect::<Vec<_>>()
        })
        .collect()
}
pub(crate) fn replace_spectator_connection(
    runtime: &mut RoomRuntime,
    spectator_id: u64,
    user_id: i64,
    display_name: String,
    connection: &ConnectionHandle,
) {
    if let Some(previous) = runtime.spectator_connections.insert(
        spectator_id,
        SpectatorConnection {
            user_id,
            display_name,
            connection: connection.clone(),
        },
    ) {
        if previous.connection.id != connection.id {
            previous.connection.request_close();
        }
    }
}
pub(crate) fn snapshot_spectator_connections(runtime: &RoomRuntime) -> SpectatorConnections {
    runtime
        .spectator_connections
        .iter()
        .map(|(spectator_id, spectator)| (*spectator_id, spectator.connection.clone()))
        .collect()
}
pub(crate) fn snapshot_spectator_identities(runtime: &RoomRuntime) -> Vec<SpectatorIdentity> {
    let mut identities = BTreeMap::new();
    for spectator in runtime.spectator_connections.values() {
        identities
            .entry(spectator.user_id)
            .or_insert_with(|| spectator.display_name.clone());
    }
    identities
        .into_iter()
        .map(|(user_id, display_name)| SpectatorIdentity {
            user_id,
            display_name,
        })
        .collect()
}
pub(crate) fn remove_spectator_connection(
    runtime: &mut RoomRuntime,
    spectator_id: u64,
    connection_id: u64,
) {
    if runtime
        .spectator_connections
        .get(&spectator_id)
        .is_some_and(|spectator| spectator.connection.id == connection_id)
    {
        runtime.spectator_connections.remove(&spectator_id);
    }
}

pub(crate) fn room_has_only_bots(room: &RoomState) -> bool {
    !room.seats.is_empty() && room.seats.iter().all(|seat| seat.seat_type == "bot")
}

pub(crate) fn should_terminate_unattended(runtime: &RoomRuntime) -> bool {
    if !runtime.connections.is_empty() {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use tokio::sync::{Notify, mpsc};

    use super::{
        RoomRuntime, add_seat_connection, remap_connections_to_current_seats,
        remove_seat_connection, seat_group_contains_connection, seat_has_live_connections,
        snapshot_connections, snapshot_seat_connections,
    };
    use crate::app::ConnectionHandle;
    use crate::app::initial_room_state;
    use crate::core::state::SeatState;

    fn test_connection(id: u64) -> ConnectionHandle {
        let (sender, _receiver) = mpsc::channel(4);
        ConnectionHandle {
            id,
            sender,
            close_requested: Arc::new(AtomicBool::new(false)),
            close_notify: Arc::new(Notify::new()),
        }
    }

    #[test]
    fn multi_device_seat_group_tracks_multiple_connections() {
        let mut runtime = RoomRuntime::new(
            "2026-05-06T00:00:00Z".to_string(),
            initial_room_state("ROOM42"),
        );
        let first = test_connection(1);
        let second = test_connection(2);

        add_seat_connection(&mut runtime, 0, Some(11), &first);
        add_seat_connection(&mut runtime, 0, Some(11), &second);

        assert!(seat_has_live_connections(&runtime, 0));
        assert!(seat_group_contains_connection(&runtime, 0, 1));
        assert!(seat_group_contains_connection(&runtime, 0, 2));
        assert_eq!(snapshot_seat_connections(&runtime, 0).len(), 2);
        assert_eq!(snapshot_connections(&runtime).len(), 2);

        assert!(remove_seat_connection(&mut runtime, 0, 1));
        assert!(seat_has_live_connections(&runtime, 0));
        assert_eq!(snapshot_seat_connections(&runtime, 0).len(), 1);

        assert!(!remove_seat_connection(&mut runtime, 0, 2));
        assert!(!seat_has_live_connections(&runtime, 0));
    }

    #[test]
    fn remaps_live_connections_when_players_change_seats() {
        let mut previous_room = initial_room_state("ROOM42");
        previous_room.seats = (0..4)
            .map(|seat_index| SeatState {
                seat_index,
                user_id: Some((seat_index as i64 + 1) * 100),
                connected: true,
                ready: true,
                seat_type: "human".to_string(),
                ..Default::default()
            })
            .collect();
        let mut runtime =
            RoomRuntime::new("2026-05-07T00:00:00Z".to_string(), previous_room.clone());
        let first = test_connection(1);
        let third = test_connection(3);
        add_seat_connection(&mut runtime, 0, Some(100), &first);
        add_seat_connection(&mut runtime, 2, Some(300), &third);

        runtime.room.seats = vec![
            SeatState {
                seat_index: 0,
                ..previous_room.seats[1].clone()
            },
            SeatState {
                seat_index: 1,
                ..previous_room.seats[0].clone()
            },
            SeatState {
                seat_index: 2,
                ..previous_room.seats[3].clone()
            },
            SeatState {
                seat_index: 3,
                ..previous_room.seats[2].clone()
            },
        ];

        remap_connections_to_current_seats(&mut runtime, &previous_room);

        assert!(seat_group_contains_connection(&runtime, 1, first.id));
        assert!(seat_group_contains_connection(&runtime, 3, third.id));
        assert!(!seat_group_contains_connection(&runtime, 0, first.id));
        assert!(!seat_group_contains_connection(&runtime, 2, third.id));
    }
}
