use std::time::Duration;

use chrono::Utc;

use crate::AppContext;
use crate::BOT_ACTION_DELAY_NORMAL_MS;
use crate::BOT_ACTION_DELAY_TEST_MS;
use crate::app::room_runtime::{
    abort_join_handle, close_runtime, restore_room_snapshot, room_handle, room_has_only_bots,
    should_terminate_unattended, snapshot_connections, unregister_room_handle,
};
use crate::broadcast_to_handles;
use crate::collect_snapshot_and_prompt_outbound_from_snapshot;
use crate::continue_action_deadline;
use crate::convert_seat_to_bot;
use crate::disconnect_deadline_for_seat;
use crate::next_disconnect_deadline;
use crate::pending_timeout_deadline;
use crate::remove_seat_from_room;
use crate::room_has_round_state;
use crate::room_mode;
use crate::room_seats;
use crate::send_outbound;
use crate::serialize_room;
use crate::sleep_until;
use crate::try_rust_action;
use crate::{
    mahjong::next_bot_action as rust_next_bot_action,
    mahjong::process_due_continue_action as rust_process_due_continue_action,
    mahjong::reconcile_continue_action_state as rust_reconcile_continue_action_state,
    mahjong::try_process_due_timeout as try_rust_process_due_timeout,
};

async fn process_due_pending_timeout(state: AppContext, table_code: String, expected_nonce: u64) {
    let Some(room_handle) = room_handle(&state, &table_code).await else {
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
    if runtime.timeout_nonce != expected_nonce {
        return;
    }
    let Some(deadline) = pending_timeout_deadline(&runtime.room) else {
        return;
    };
    if deadline > Utc::now() {
        return;
    }
    let rust_messages = try_rust_process_due_timeout(&mut runtime.room);
    let Some(rust_messages) = rust_messages else {
        return;
    };
    let created_at = runtime.created_at.clone();
    let room_json = match serialize_room(&runtime.room) {
        Ok(value) => value,
        Err(_) => return,
    };
    drop(runtime);
    if state
        .inner
        .db
        .save_table(&table_code, &created_at, &room_json)
        .await
        .is_err()
    {
        restore_room_snapshot(&room_handle, previous_room).await;
        return;
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
    let mut outbound = broadcast_to_handles(&broadcast_handles, Some(&rust_messages));
    outbound.extend(snapshot_outbound);
    send_outbound(outbound);
    schedule_room_tasks_detached(state, table_code);
}

async fn process_due_continue_action(state: AppContext, table_code: String, expected_nonce: u64) {
    let Some(room_handle) = room_handle(&state, &table_code).await else {
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
    if runtime.continue_nonce != expected_nonce {
        return;
    }
    let Some(deadline) = continue_action_deadline(&runtime.room) else {
        return;
    };
    if deadline > Utc::now() {
        return;
    }
    if rust_process_due_continue_action(&mut runtime.room).ok() != Some(true) {
        return;
    }
    let created_at = runtime.created_at.clone();
    let room_json = match serialize_room(&runtime.room) {
        Ok(value) => value,
        Err(_) => return,
    };
    drop(runtime);
    if state
        .inner
        .db
        .save_table(&table_code, &created_at, &room_json)
        .await
        .is_err()
    {
        restore_room_snapshot(&room_handle, previous_room).await;
        return;
    }
    let runtime = room_handle.runtime.lock().await;
    let connections = snapshot_connections(&runtime);
    let outbound = collect_snapshot_and_prompt_outbound_from_snapshot(&runtime.room, &connections);
    drop(runtime);
    send_outbound(outbound);
    schedule_room_tasks_detached(state, table_code);
}

async fn process_due_disconnect_timeout(
    state: AppContext,
    table_code: String,
    seat_index: usize,
    expected_nonce: u64,
) {
    let Some(room_handle) = room_handle(&state, &table_code).await else {
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
    if runtime.disconnect_nonce != expected_nonce {
        return;
    }
    let Some(deadline) = disconnect_deadline_for_seat(&runtime.room, seat_index) else {
        return;
    };
    if deadline > Utc::now() {
        return;
    }

    if room_has_round_state(&runtime.room) {
        convert_seat_to_bot(&mut runtime.room, seat_index);
        let _ = rust_reconcile_continue_action_state(&mut runtime.room);
    } else {
        remove_seat_from_room(&mut runtime.room, seat_index);
    }
    let should_close =
        room_seats(&runtime.room).is_empty() || should_terminate_unattended(&runtime);

    if should_close {
        room_handle.mark_closed();
        close_runtime(&mut runtime);
        drop(runtime);
        unregister_room_handle(&state, &table_code, &room_handle).await;
        state.inner.db.delete_table(&table_code).await.ok();
        return;
    }

    let created_at = runtime.created_at.clone();
    let room_json = match serialize_room(&runtime.room) {
        Ok(value) => value,
        Err(_) => return,
    };
    drop(runtime);
    if state
        .inner
        .db
        .save_table_and_delete_tokens_for_seat(&table_code, &created_at, &room_json, seat_index)
        .await
        .is_err()
    {
        restore_room_snapshot(&room_handle, previous_room).await;
        return;
    }
    let mut runtime = room_handle.runtime.lock().await;
    let connections = snapshot_connections(&runtime);
    let outbound = collect_snapshot_and_prompt_outbound_from_snapshot(&runtime.room, &connections);
    runtime.connections.remove(&seat_index);
    drop(runtime);
    send_outbound(outbound);
    schedule_room_tasks_detached(state, table_code);
}

async fn process_due_bot_action(state: AppContext, table_code: String, expected_nonce: u64) {
    let Some(room_handle) = room_handle(&state, &table_code).await else {
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
    if runtime.bot_nonce != expected_nonce {
        return;
    }

    let action = rust_next_bot_action(&runtime.room);
    let Some(action) = action else {
        return;
    };

    let messages = match try_rust_action(
        &mut runtime.room,
        action.seat_index,
        &action.action_type,
        &action.tile_ids,
    ) {
        Some(Ok(messages)) => messages,
        Some(Err(_)) | None => return,
    };

    let created_at = runtime.created_at.clone();
    let room = runtime.room.clone();
    let connections = snapshot_connections(&runtime);
    let broadcast_handles = connections
        .iter()
        .map(|(_, handle)| handle.clone())
        .collect::<Vec<_>>();
    drop(runtime);
    let room_json = match serialize_room(&room) {
        Ok(value) => value,
        Err(_) => return,
    };
    let snapshot_outbound = collect_snapshot_and_prompt_outbound_from_snapshot(&room, &connections);
    if state
        .inner
        .db
        .save_table(&table_code, &created_at, &room_json)
        .await
        .is_err()
    {
        restore_room_snapshot(&room_handle, previous_room).await;
        return;
    }
    let mut outbound = broadcast_to_handles(&broadcast_handles, Some(&messages));
    outbound.extend(snapshot_outbound);
    send_outbound(outbound);
    schedule_room_tasks_detached(state, table_code);
}

pub(crate) async fn schedule_room_tasks(state: AppContext, table_code: String) {
    let Some(room_handle) = room_handle(&state, &table_code).await else {
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
    if room_seats(&runtime.room).is_empty() || room_has_only_bots(&runtime.room) {
        room_handle.mark_closed();
        close_runtime(&mut runtime);
        drop(runtime);
        unregister_room_handle(&state, &table_code, &room_handle).await;
        state.inner.db.delete_table(&table_code).await.ok();
        return;
    }
    abort_join_handle(&mut runtime.timeout_task);
    abort_join_handle(&mut runtime.continue_task);
    abort_join_handle(&mut runtime.disconnect_task);
    abort_join_handle(&mut runtime.bot_task);
    runtime.timeout_nonce = runtime.timeout_nonce.wrapping_add(1);
    runtime.continue_nonce = runtime.continue_nonce.wrapping_add(1);
    runtime.disconnect_nonce = runtime.disconnect_nonce.wrapping_add(1);
    runtime.bot_nonce = runtime.bot_nonce.wrapping_add(1);

    if let Some(deadline) = pending_timeout_deadline(&runtime.room) {
        let state_clone = state.clone();
        let table_clone = table_code.clone();
        let nonce = runtime.timeout_nonce;
        runtime.timeout_task = Some(tokio::spawn(async move {
            sleep_until(deadline).await;
            process_due_pending_timeout(state_clone, table_clone, nonce).await;
        }));
    }

    if let Some(deadline) = continue_action_deadline(&runtime.room) {
        let state_clone = state.clone();
        let table_clone = table_code.clone();
        let nonce = runtime.continue_nonce;
        runtime.continue_task = Some(tokio::spawn(async move {
            sleep_until(deadline).await;
            process_due_continue_action(state_clone, table_clone, nonce).await;
        }));
    }

    if let Some((seat_index, deadline)) = next_disconnect_deadline(&runtime.room) {
        let state_clone = state.clone();
        let table_clone = table_code.clone();
        let nonce = runtime.disconnect_nonce;
        runtime.disconnect_task = Some(tokio::spawn(async move {
            sleep_until(deadline).await;
            process_due_disconnect_timeout(state_clone, table_clone, seat_index, nonce).await;
        }));
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
        runtime.bot_task = Some(tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            process_due_bot_action(state_clone, table_clone, nonce).await;
        }));
    }
}

pub(crate) fn schedule_room_tasks_detached(state: AppContext, table_code: String) {
    tokio::spawn(async move {
        schedule_room_tasks(state, table_code).await;
    });
}
