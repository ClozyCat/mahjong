use std::time::Duration;

use chrono::Utc;

use crate::app::room_runtime::{
    abort_join_handle, close_runtime, restore_room_snapshot, room_handle, room_has_only_bots,
    should_terminate_unattended, snapshot_connections, snapshot_spectator_connections,
    snapshot_spectator_identities, unregister_room_handle,
};
use crate::app::{
    records::archive_current_round_if_needed,
    AppContext, BOT_ACTION_DELAY_MS, broadcast_to_handles,
    collect_observer_outbound_from_snapshot, collect_snapshot_and_prompt_outbound_from_snapshot,
    continue_action_deadline, convert_seat_to_bot, disconnect_deadline_for_seat,
    next_disconnect_deadline, pending_timeout_deadline, remove_seat_from_room,
    room_has_round_state, room_seats, send_outbound, serialize_room, sleep_until,
};
use crate::core::engine::try_handle_player_action_in_room_state;
use crate::rules::standard::actions::apply_discard_action_output_in_room_state;
use crate::rules::standard::automation::{
    next_bot_action_in_room_state as standard_next_bot_action,
    try_process_due_timeout_in_room_state as standard_try_process_due_timeout,
};
use crate::rules::standard::flow::{
    process_due_continue_action_in_room_state as standard_process_due_continue_action,
    reconcile_continue_action_state_in_room_state as reconcile_standard_continue_action_state,
    room_ready_to_start as standard_room_ready_to_start,
    start_match_in_room_state as standard_start_match,
};
use crate::rules::standard::win::apply_hu_action_output_in_room_state;

fn player_is_ready_hand(room: &crate::core::state::RoomState, seat_index: usize) -> bool {
    room.round_state
        .as_ref()
        .and_then(|round| round.players.get(seat_index))
        .is_some_and(|player| player.is_ready_hand)
}

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
    let rust_messages = match standard_try_process_due_timeout(&mut runtime.room) {
        Ok(messages) => messages,
        Err(_) => return,
    };
    let Some(rust_messages) = rust_messages else {
        return;
    };
    let created_at = runtime.created_at.clone();
    let room = runtime.room.clone();
    let room_json = match serialize_room(&room) {
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
    if let Err(error) =
        archive_current_round_if_needed(&state, &room, &created_at, &crate::app::now_iso()).await
    {
        eprintln!("failed to archive timeout settlement for table {table_code}: {error:#}");
    }
    let runtime = room_handle.runtime.lock().await;
    let connections = snapshot_connections(&runtime);
    let spectator_connections = snapshot_spectator_connections(&runtime);
    let spectator_identities = snapshot_spectator_identities(&runtime);
    let broadcast_handles = connections
        .iter()
        .map(|(_, handle)| handle.clone())
        .collect::<Vec<_>>();
    let mut broadcast_handles = broadcast_handles;
    broadcast_handles.extend(
        spectator_connections
            .iter()
            .map(|(_, handle)| handle.clone()),
    );
    let mut snapshot_outbound = collect_snapshot_and_prompt_outbound_from_snapshot(
        &runtime.room,
        &spectator_identities,
        &connections,
    );
    snapshot_outbound.extend(collect_observer_outbound_from_snapshot(
        &runtime.room,
        &spectator_identities,
        &spectator_connections,
    ));
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
    let processed = match standard_process_due_continue_action(&mut runtime.room) {
        Ok(result) => result,
        Err(_) => return,
    };
    if !processed {
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
    let spectator_connections = snapshot_spectator_connections(&runtime);
    let spectator_identities = snapshot_spectator_identities(&runtime);
    let mut outbound = collect_snapshot_and_prompt_outbound_from_snapshot(
        &runtime.room,
        &spectator_identities,
        &connections,
    );
    outbound.extend(collect_observer_outbound_from_snapshot(
        &runtime.room,
        &spectator_identities,
        &spectator_connections,
    ));
    drop(runtime);
    send_outbound(outbound);
    schedule_room_tasks_detached(state, table_code);
}

async fn process_due_start_match(state: AppContext, table_code: String, expected_nonce: u64) {
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
    if runtime.start_match_nonce != expected_nonce {
        return;
    }
    let Some(pending_start) = runtime.pending_start_match.clone() else {
        return;
    };
    let Some(deadline) = super::parse_datetime(&pending_start.reveal_at) else {
        runtime.pending_start_match = None;
        return;
    };
    if deadline > Utc::now() {
        return;
    }
    if runtime.room.phase != "waiting"
        || runtime.room.round_state.is_some()
        || !standard_room_ready_to_start(&runtime.room)
        || !runtime
            .room
            .seats
            .iter()
            .any(|seat| seat.seat_index == pending_start.dealer_seat)
    {
        runtime.pending_start_match = None;
        let room = runtime.room.clone();
        let connections = snapshot_connections(&runtime);
        let spectator_connections = snapshot_spectator_connections(&runtime);
        let spectator_identities = snapshot_spectator_identities(&runtime);
        drop(runtime);
        let mut outbound =
            collect_snapshot_and_prompt_outbound_from_snapshot(&room, &spectator_identities, &connections);
        outbound.extend(collect_observer_outbound_from_snapshot(
            &room,
            &spectator_identities,
            &spectator_connections,
        ));
        send_outbound(outbound);
        schedule_room_tasks_detached(state, table_code);
        return;
    }

    if standard_start_match(
        &mut runtime.room,
        pending_start.dealer_seat,
        rand::random::<u64>(),
    )
    .is_err()
    {
        runtime.pending_start_match = None;
        return;
    }
    runtime.pending_start_match = None;

    let created_at = runtime.created_at.clone();
    let room = runtime.room.clone();
    let connections = snapshot_connections(&runtime);
    let spectator_connections = snapshot_spectator_connections(&runtime);
    let spectator_identities = snapshot_spectator_identities(&runtime);
    drop(runtime);
    let room_json = match serialize_room(&room) {
        Ok(value) => value,
        Err(_) => return,
    };
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
    let mut outbound =
        collect_snapshot_and_prompt_outbound_from_snapshot(&room, &spectator_identities, &connections);
    outbound.extend(collect_observer_outbound_from_snapshot(
        &room,
        &spectator_identities,
        &spectator_connections,
    ));
    send_outbound(outbound);
    schedule_room_tasks_detached(state, table_code);
}

async fn process_due_disconnect_timeout(
    state: AppContext,
    table_code: String,
    seat_index: usize,
    expected_nonce: u64,
) {
    let left_at = super::now_iso();
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
        let _ = reconcile_standard_continue_action_state(&mut runtime.room);
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
        state.inner.db.delete_table(&table_code, &left_at).await.ok();
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
        .save_table_and_delete_tokens_for_seat(
            &table_code,
            &created_at,
            &room_json,
            seat_index,
            &left_at,
        )
        .await
        .is_err()
    {
        restore_room_snapshot(&room_handle, previous_room).await;
        return;
    }
    let mut runtime = room_handle.runtime.lock().await;
    let connections = snapshot_connections(&runtime);
    let spectator_connections = snapshot_spectator_connections(&runtime);
    let spectator_identities = snapshot_spectator_identities(&runtime);
    let mut outbound = collect_snapshot_and_prompt_outbound_from_snapshot(
        &runtime.room,
        &spectator_identities,
        &connections,
    );
    outbound.extend(collect_observer_outbound_from_snapshot(
        &runtime.room,
        &spectator_identities,
        &spectator_connections,
    ));
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

    let action = match standard_next_bot_action(&runtime.room) {
        Ok(action) => action,
        Err(_) => return,
    };
    let Some(action) = action else {
        return;
    };

    let action_result = if player_is_ready_hand(&runtime.room, action.seat_index) {
        match action.action_type.as_str() {
            "discard" => action
                .tile_ids
                .first()
                .and_then(|tile_id| {
                    apply_discard_action_output_in_room_state(
                        &mut runtime.room,
                        action.seat_index,
                        tile_id,
                    )
                    .ok()
                })
                .map(Ok),
            "hu" => apply_hu_action_output_in_room_state(&mut runtime.room, action.seat_index)
                .ok()
                .map(Ok),
            _ => match try_handle_player_action_in_room_state(
                &mut runtime.room,
                action.seat_index,
                &action.action_type,
                &action.tile_ids,
            ) {
                Ok(result) => result,
                Err(_) => return,
            },
        }
    } else {
        match try_handle_player_action_in_room_state(
            &mut runtime.room,
            action.seat_index,
            &action.action_type,
            &action.tile_ids,
        ) {
            Ok(result) => result,
            Err(_) => return,
        }
    };
    let messages = match action_result {
        Some(Ok(output)) => output.emitted_messages,
        Some(Err(_)) | None => return,
    };

    let created_at = runtime.created_at.clone();
    let room = runtime.room.clone();
    let connections = snapshot_connections(&runtime);
    let spectator_connections = snapshot_spectator_connections(&runtime);
    let spectator_identities = snapshot_spectator_identities(&runtime);
    let broadcast_handles = connections
        .iter()
        .map(|(_, handle)| handle.clone())
        .collect::<Vec<_>>();
    let mut broadcast_handles = broadcast_handles;
    broadcast_handles.extend(
        spectator_connections
            .iter()
            .map(|(_, handle)| handle.clone()),
    );
    drop(runtime);
    let room_json = match serialize_room(&room) {
        Ok(value) => value,
        Err(_) => return,
    };
    let mut snapshot_outbound =
        collect_snapshot_and_prompt_outbound_from_snapshot(&room, &spectator_identities, &connections);
    snapshot_outbound.extend(collect_observer_outbound_from_snapshot(
        &room,
        &spectator_identities,
        &spectator_connections,
    ));
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
    if let Err(error) =
        archive_current_round_if_needed(&state, &room, &created_at, &crate::app::now_iso()).await
    {
        eprintln!("failed to archive bot settlement for table {table_code}: {error:#}");
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
    if room_seats(&runtime.room).is_empty()
        || (room_has_only_bots(&runtime.room) && runtime.connections.is_empty())
    {
        room_handle.mark_closed();
        close_runtime(&mut runtime);
        drop(runtime);
        unregister_room_handle(&state, &table_code, &room_handle).await;
        state.inner.db.delete_table(&table_code, &super::now_iso()).await.ok();
        return;
    }
    abort_join_handle(&mut runtime.timeout_task);
    abort_join_handle(&mut runtime.continue_task);
    abort_join_handle(&mut runtime.start_match_task);
    abort_join_handle(&mut runtime.disconnect_task);
    abort_join_handle(&mut runtime.bot_task);
    runtime.timeout_nonce = runtime.timeout_nonce.wrapping_add(1);
    runtime.continue_nonce = runtime.continue_nonce.wrapping_add(1);
    runtime.start_match_nonce = runtime.start_match_nonce.wrapping_add(1);
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

    if let Some(pending_start) = runtime.pending_start_match.clone()
        && let Some(deadline) = super::parse_datetime(&pending_start.reveal_at)
    {
        let state_clone = state.clone();
        let table_clone = table_code.clone();
        let nonce = runtime.start_match_nonce;
        runtime.start_match_task = Some(tokio::spawn(async move {
            sleep_until(deadline).await;
            process_due_start_match(state_clone, table_clone, nonce).await;
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

    if standard_next_bot_action(&runtime.room)
        .ok()
        .flatten()
        .is_some()
    {
        let state_clone = state.clone();
        let table_clone = table_code.clone();
        let nonce = runtime.bot_nonce;
        runtime.bot_task = Some(tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(BOT_ACTION_DELAY_MS)).await;
            process_due_bot_action(state_clone, table_clone, nonce).await;
        }));
    }
}

pub(crate) fn schedule_room_tasks_detached(state: AppContext, table_code: String) {
    tokio::spawn(async move {
        schedule_room_tasks(state, table_code).await;
    });
}
