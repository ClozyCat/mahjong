use std::time::Duration;

use chrono::Utc;

use crate::app::evaluation::apply_room_result_to_evaluation_session;
use crate::app::room_runtime::{
    abort_join_handle, close_runtime, remap_connections_to_current_seats, restore_room_snapshot,
    room_handle, room_has_only_bots, snapshot_connections, unregister_room_handle,
};
use crate::app::{
    AppContext, bot_action_delay_ms, broadcast_to_handles,
    collect_snapshot_and_prompt_outbound_from_snapshot, continue_action_deadline,
    notify_all_user_connections, pending_timeout_deadline, record_timeout_auto_responses,
    records::{apply_point_updates_to_room, archive_current_round_if_needed},
    room_has_round_state, room_seats, send_outbound, serialize_room, sleep_until,
    timeout_auto_response_seats, user_active_table_updated_message,
};
use crate::core::engine::try_handle_player_action_in_room_state;
use crate::rules::standard::actions::apply_discard_action_output_in_room_state;
use crate::rules::standard::automation::{
    next_bot_action_in_room_state_with_policy_resolver as standard_next_bot_action,
    try_process_due_timeout_in_room_state as standard_try_process_due_timeout,
};
use crate::rules::standard::flow::{
    process_due_continue_action_in_room_state as standard_process_due_continue_action,
    reconcile_continue_action_state_in_room_state as reconcile_standard_continue_action_state,
    room_ready_to_start as standard_room_ready_to_start,
    start_match_in_room_state as standard_start_match,
};
use crate::rules::standard::win::apply_hu_action_output_in_room_state;
use crate::special_bots;

const UNATTENDED_ROOM_CLEANUP_DELAY: Duration = Duration::from_secs(180);

fn player_is_ready_hand(room: &crate::core::state::RoomState, seat_index: usize) -> bool {
    room.round_state
        .as_ref()
        .and_then(|round| round.players.get(seat_index))
        .is_some_and(|player| player.is_ready_hand)
}

fn room_has_online_players(runtime: &crate::app::room_runtime::RoomRuntime) -> bool {
    if runtime.room.mode == crate::evaluation::EVALUATION_ROOM_MODE
        && runtime
            .room
            .seats
            .iter()
            .all(|seat| seat.is_bot || seat.seat_type != "human")
    {
        return true;
    }

    runtime.room.seats.iter().any(|seat| {
        seat.seat_type == "human"
            && seat.connected
            && runtime
                .connections
                .get(&seat.seat_index)
                .is_some_and(|group| !group.connections.is_empty())
    })
}

fn evaluation_room_has_only_ai_players(room: &crate::core::state::RoomState) -> bool {
    room.mode == crate::evaluation::EVALUATION_ROOM_MODE
        && !room.seats.is_empty()
        && room
            .seats
            .iter()
            .all(|seat| seat.is_bot || seat.seat_type != "human")
}

fn finished_evaluation_room_has_human_subject(room: &crate::core::state::RoomState) -> bool {
    room.mode == crate::evaluation::EVALUATION_ROOM_MODE
        && room_match_finished(room)
        && room
            .seats
            .iter()
            .any(|seat| !seat.is_bot && seat.seat_type == "human" && seat.user_id.is_some())
}

fn room_match_finished(room: &crate::core::state::RoomState) -> bool {
    room.match_state
        .as_ref()
        .is_some_and(|match_state| match_state.match_finished)
}

async fn freeze_evaluation_result(state: &AppContext, room: &crate::core::state::RoomState) {
    let mut sessions = state.inner.evaluation_sessions.write().await;
    for session in sessions.values_mut() {
        apply_room_result_to_evaluation_session(session, room);
    }
}

async fn process_unattended_room_cleanup(
    state: AppContext,
    table_code: String,
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
    if room_handle.is_closed() || runtime.unattended_cleanup_nonce != expected_nonce {
        return;
    }
    if room_has_online_players(&runtime) {
        return;
    }
    room_handle.mark_closed();
    close_runtime(&mut runtime);
    drop(runtime);
    unregister_room_handle(&state, &table_code, &room_handle).await;
    state
        .inner
        .db
        .delete_table(&table_code, &crate::app::now_iso())
        .await
        .ok();
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
    let timed_out_seats = timeout_auto_response_seats(&runtime.room);
    let rust_messages = match standard_try_process_due_timeout(&mut runtime.room) {
        Ok(messages) => messages,
        Err(_) => return,
    };
    let Some(rust_messages) = rust_messages else {
        return;
    };
    if !rust_messages.is_empty()
        && record_timeout_auto_responses(&mut runtime.room, &timed_out_seats)
        && room_has_round_state(&runtime.room)
    {
        let _ = reconcile_standard_continue_action_state(&mut runtime.room);
    }
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
    let archive_outcome =
        match archive_current_round_if_needed(&state, &room, &created_at, &crate::app::now_iso())
            .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                eprintln!("failed to archive timeout settlement for table {table_code}: {error:#}");
                None
            }
        };
    let mut runtime = room_handle.runtime.lock().await;
    let point_updates = archive_outcome
        .as_ref()
        .map(|outcome| outcome.point_updates.as_slice())
        .unwrap_or(&[]);
    let room_points_changed = apply_point_updates_to_room(&mut runtime.room, point_updates);
    let reconciled_continue_action = runtime.room.phase == "settlement"
        && reconcile_standard_continue_action_state(&mut runtime.room).is_ok();
    if room_points_changed || reconciled_continue_action {
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
            eprintln!("failed to persist timeout seat points for table {table_code}");
        }
        runtime = room_handle.runtime.lock().await;
    }
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
    let processed = match standard_process_due_continue_action(&mut runtime.room) {
        Ok(result) => result,
        Err(_) => return,
    };
    if !processed {
        return;
    }
    remap_connections_to_current_seats(&mut runtime, &previous_room);
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
        drop(runtime);
        let outbound = collect_snapshot_and_prompt_outbound_from_snapshot(&room, &connections);
        send_outbound(outbound);
        schedule_room_tasks_detached(state, table_code);
        return;
    }

    if standard_start_match(
        &mut runtime.room,
        pending_start.dealer_seat,
        pending_start.seed.unwrap_or_else(rand::random::<u64>),
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
    for user_id in room.seats.iter().filter_map(|seat| seat.user_id) {
        notify_all_user_connections(
            &state,
            user_active_table_updated_message(user_id, Some(&table_code), Some(&room.phase)),
        )
        .await;
    }
    let outbound = collect_snapshot_and_prompt_outbound_from_snapshot(&room, &connections);
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

    let action = match standard_next_bot_action(&runtime.room, &|seat_index| {
        special_bots::policy_config_for_seat(&runtime.room, seat_index)
    }) {
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
    let broadcast_handles = connections
        .iter()
        .map(|(_, handle)| handle.clone())
        .collect::<Vec<_>>();
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
    let archive_outcome =
        match archive_current_round_if_needed(&state, &room, &created_at, &crate::app::now_iso())
            .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                eprintln!("failed to archive bot settlement for table {table_code}: {error:#}");
                None
            }
        };
    let mut runtime = room_handle.runtime.lock().await;
    let point_updates = archive_outcome
        .as_ref()
        .map(|outcome| outcome.point_updates.as_slice())
        .unwrap_or(&[]);
    let room_points_changed = apply_point_updates_to_room(&mut runtime.room, point_updates);
    let reconciled_continue_action = runtime.room.phase == "settlement"
        && reconcile_standard_continue_action_state(&mut runtime.room).is_ok();
    if room_points_changed || reconciled_continue_action {
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
            eprintln!("failed to persist bot seat points for table {table_code}");
        }
        runtime = room_handle.runtime.lock().await;
    }
    let connections = snapshot_connections(&runtime);
    let snapshot_outbound =
        collect_snapshot_and_prompt_outbound_from_snapshot(&runtime.room, &connections);
    drop(runtime);
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
    if runtime.room.phase == "settlement" {
        let _ = reconcile_standard_continue_action_state(&mut runtime.room);
    }
    if evaluation_room_has_only_ai_players(&runtime.room) && room_match_finished(&runtime.room) {
        freeze_evaluation_result(&state, &runtime.room).await;
        room_handle.mark_closed();
        close_runtime(&mut runtime);
        drop(runtime);
        unregister_room_handle(&state, &table_code, &room_handle).await;
        state
            .inner
            .db
            .delete_table(&table_code, &super::now_iso())
            .await
            .ok();
        return;
    }
    if finished_evaluation_room_has_human_subject(&runtime.room) {
        freeze_evaluation_result(&state, &runtime.room).await;
    }
    if room_seats(&runtime.room).is_empty()
        || (room_has_only_bots(&runtime.room)
            && runtime.connections.is_empty()
            && !room_has_online_players(&runtime))
    {
        room_handle.mark_closed();
        close_runtime(&mut runtime);
        drop(runtime);
        unregister_room_handle(&state, &table_code, &room_handle).await;
        state
            .inner
            .db
            .delete_table(&table_code, &super::now_iso())
            .await
            .ok();
        return;
    }
    abort_join_handle(&mut runtime.timeout_task);
    abort_join_handle(&mut runtime.continue_task);
    abort_join_handle(&mut runtime.start_match_task);
    abort_join_handle(&mut runtime.bot_task);
    abort_join_handle(&mut runtime.unattended_cleanup_task);
    runtime.timeout_nonce = runtime.timeout_nonce.wrapping_add(1);
    runtime.continue_nonce = runtime.continue_nonce.wrapping_add(1);
    runtime.start_match_nonce = runtime.start_match_nonce.wrapping_add(1);
    runtime.bot_nonce = runtime.bot_nonce.wrapping_add(1);
    runtime.unattended_cleanup_nonce = runtime.unattended_cleanup_nonce.wrapping_add(1);

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

    if standard_next_bot_action(&runtime.room, &|seat_index| {
        special_bots::policy_config_for_seat(&runtime.room, seat_index)
    })
    .ok()
    .flatten()
    .is_some()
    {
        let state_clone = state.clone();
        let table_clone = table_code.clone();
        let nonce = runtime.bot_nonce;
        let delay_ms = bot_action_delay_ms(&runtime.room);
        runtime.bot_task = Some(tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            process_due_bot_action(state_clone, table_clone, nonce).await;
        }));
    }

    if !room_has_online_players(&runtime)
        && !finished_evaluation_room_has_human_subject(&runtime.room)
    {
        let state_clone = state.clone();
        let table_clone = table_code.clone();
        let nonce = runtime.unattended_cleanup_nonce;
        runtime.unattended_cleanup_task = Some(tokio::spawn(async move {
            tokio::time::sleep(UNATTENDED_ROOM_CLEANUP_DELAY).await;
            process_unattended_room_cleanup(state_clone, table_clone, nonce).await;
        }));
    }
}

pub(crate) fn schedule_room_tasks_detached(state: AppContext, table_code: String) {
    tokio::spawn(async move {
        schedule_room_tasks(state, table_code).await;
    });
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::schedule_room_tasks;
    use crate::app::auth::{generate_session_token, hash_password, hash_session_token};
    use crate::app::evaluation::build_evaluation_room;
    use crate::app::persistence::{DbWorker, in_memory_database};
    use crate::app::room_runtime::{RoomRuntime, room_handle};
    use crate::app::{
        AppContext, initial_room_state, initial_room_state_with_owner, serialize_room_state,
    };
    use crate::core::state::{RoundSettlement, SeatState};
    use crate::rules::standard::flow::start_match_in_room_state;

    fn offline_human_room(table_code: &str) -> crate::core::state::RoomState {
        let mut room = initial_room_state_with_owner(table_code, Some(101), 1);
        room.seats.push(SeatState {
            seat_index: 0,
            user_id: Some(101),
            nickname: Some("OfflinePlayer".to_string()),
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
        room
    }

    async fn register_scheduler_test_user(
        worker: &DbWorker,
        invite_code: &str,
        display_name: &str,
    ) -> Result<i64> {
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
        Ok(user.user_id)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn closes_room_when_unattended_cleanup_is_due() -> Result<()> {
        let db = in_memory_database("")?;
        db.initialize()?;
        let worker = DbWorker::start(db)?;
        let room = offline_human_room("ROOMIDLE");
        let room_json = serialize_room_state(&room)?;
        worker
            .save_table("ROOMIDLE", "2026-05-06T00:00:00Z", &room_json)
            .await?;
        let state = AppContext::new(worker.clone());
        let handle = std::sync::Arc::new(crate::app::room_runtime::RoomHandle::new(
            RoomRuntime::new("2026-05-06T00:00:00Z".to_string(), room),
        ));
        state
            .inner
            .rooms
            .write()
            .await
            .insert("ROOMIDLE".to_string(), handle);

        super::process_unattended_room_cleanup(state.clone(), "ROOMIDLE".to_string(), 0).await;

        assert!(worker.get_table("ROOMIDLE").await?.is_none());
        assert!(room_handle(&state, "ROOMIDLE").await.is_none());
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn schedules_cleanup_when_room_has_no_online_players() -> Result<()> {
        let db = in_memory_database("")?;
        db.initialize()?;
        let worker = DbWorker::start(db)?;
        let room = offline_human_room("ROOMIDLE2");
        let room_json = serialize_room_state(&room)?;
        worker
            .save_table("ROOMIDLE2", "2026-05-06T00:00:00Z", &room_json)
            .await?;
        let state = AppContext::new(worker);
        let handle = std::sync::Arc::new(crate::app::room_runtime::RoomHandle::new(
            RoomRuntime::new("2026-05-06T00:00:00Z".to_string(), room),
        ));
        state
            .inner
            .rooms
            .write()
            .await
            .insert("ROOMIDLE2".to_string(), handle.clone());

        schedule_room_tasks(state, "ROOMIDLE2".to_string()).await;

        let runtime = handle.runtime.lock().await;
        assert!(runtime.unattended_cleanup_task.is_some());
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn schedules_bot_action_for_unconnected_ai_evaluation_room() -> Result<()> {
        let db = in_memory_database("")?;
        db.initialize()?;
        let worker = DbWorker::start(db)?;
        let mut room = build_evaluation_room("EVALBOT", 101, Some(202), "Bot Subject", true);
        start_match_in_room_state(&mut room, 0, 7).expect("evaluation bot room should start");
        let room_json = serialize_room_state(&room)?;
        worker
            .save_table("EVALBOT", "2026-05-06T00:00:00Z", &room_json)
            .await?;
        let state = AppContext::new(worker);
        let handle = std::sync::Arc::new(crate::app::room_runtime::RoomHandle::new(
            RoomRuntime::new("2026-05-06T00:00:00Z".to_string(), room),
        ));
        state
            .inner
            .rooms
            .write()
            .await
            .insert("EVALBOT".to_string(), handle.clone());

        schedule_room_tasks(state, "EVALBOT".to_string()).await;

        let runtime = handle.runtime.lock().await;
        assert!(!handle.is_closed());
        assert!(runtime.bot_task.is_some());
        assert!(runtime.unattended_cleanup_task.is_none());
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pending_timeout_extra_time_extension_does_not_count_as_auto_response() -> Result<()> {
        let db = in_memory_database("")?;
        db.initialize()?;
        let worker = DbWorker::start(db)?;
        let mut room = initial_room_state("EXTRATME");
        for seat_index in 0..4 {
            room.seats.push(SeatState {
                seat_index,
                nickname: Some(format!("P{seat_index}")),
                connected: true,
                is_bot: false,
                seat_type: "human".to_string(),
                consecutive_timeout_auto_response_count: if seat_index == 0 { 2 } else { 0 },
                ..Default::default()
            });
        }
        start_match_in_room_state(&mut room, 0, 7).expect("room should start");
        room.pending_timeout
            .as_mut()
            .expect("started room should have a timeout")
            .deadline_at = Some("2026-01-01T00:00:00Z".to_string());
        room.match_state
            .as_mut()
            .expect("started room should have match state")
            .extra_time_pool
            .insert(0, 5);
        let room_json = serialize_room_state(&room)?;
        worker
            .save_table("EXTRATME", "2026-05-06T00:00:00Z", &room_json)
            .await?;
        let state = AppContext::new(worker);
        let handle = std::sync::Arc::new(crate::app::room_runtime::RoomHandle::new(
            RoomRuntime::new("2026-05-06T00:00:00Z".to_string(), room),
        ));
        state
            .inner
            .rooms
            .write()
            .await
            .insert("EXTRATME".to_string(), handle.clone());

        super::process_due_pending_timeout(state, "EXTRATME".to_string(), 0).await;

        let runtime = handle.runtime.lock().await;
        assert!(!runtime.room.seats[0].is_bot);
        assert_eq!(
            runtime.room.seats[0].consecutive_timeout_auto_response_count,
            2
        );
        assert_eq!(
            runtime
                .room
                .match_state
                .as_ref()
                .and_then(|match_state| match_state.extra_time_pool.get(&0))
                .copied(),
            Some(0)
        );
        assert!(
            runtime
                .room
                .pending_timeout
                .as_ref()
                .is_some_and(|timeout| timeout.extended_with_extra)
        );
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn advances_bot_only_evaluation_room_from_settlement() -> Result<()> {
        let db = in_memory_database("")?;
        db.initialize()?;
        let worker = DbWorker::start(db)?;
        let mut room = build_evaluation_room("EVALSET", 101, Some(202), "Bot Subject", true);
        start_match_in_room_state(&mut room, 0, 7).expect("evaluation bot room should start");
        room.phase = "settlement".to_string();
        room.pending_timeout = None;
        if let Some(round) = room.round_state.as_mut() {
            round.phase = "settlement".to_string();
            round.pending_action = None;
            round.settlement = Some(RoundSettlement {
                win_type: "draw".to_string(),
                ..RoundSettlement::default()
            });
        }
        let room_json = serialize_room_state(&room)?;
        worker
            .save_table("EVALSET", "2026-05-06T00:00:00Z", &room_json)
            .await?;
        let state = AppContext::new(worker);
        let handle = std::sync::Arc::new(crate::app::room_runtime::RoomHandle::new(
            RoomRuntime::new("2026-05-06T00:00:00Z".to_string(), room),
        ));
        state
            .inner
            .rooms
            .write()
            .await
            .insert("EVALSET".to_string(), handle.clone());

        schedule_room_tasks(state, "EVALSET".to_string()).await;

        let runtime = handle.runtime.lock().await;
        assert_eq!(runtime.room.phase, "playing");
        assert!(runtime.room.continue_action.is_none());
        assert!(runtime.bot_task.is_some());
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn closes_finished_ai_only_evaluation_room() -> Result<()> {
        let db = in_memory_database("")?;
        db.initialize()?;
        let worker = DbWorker::start(db)?;
        let mut room = build_evaluation_room("EVALDONE", 101, Some(202), "Bot Subject", true);
        start_match_in_room_state(&mut room, 0, 7).expect("evaluation bot room should start");
        room.phase = "finished".to_string();
        if let Some(match_state) = room.match_state.as_mut() {
            match_state.match_finished = true;
            match_state.cumulative_scores.insert(0, 32);
            match_state.statistics.completed_round_count = 16;
            let stats = match_state
                .statistics
                .seat_stats_by_seat
                .entry(0)
                .or_default();
            stats.win_count = 3;
            stats.deal_in_count = 1;
        }
        let room_json = serialize_room_state(&room)?;
        worker
            .save_table("EVALDONE", "2026-05-06T00:00:00Z", &room_json)
            .await?;
        let state = AppContext::new(worker.clone());
        let handle = std::sync::Arc::new(crate::app::room_runtime::RoomHandle::new(
            RoomRuntime::new("2026-05-06T00:00:00Z".to_string(), room),
        ));
        state
            .inner
            .rooms
            .write()
            .await
            .insert("EVALDONE".to_string(), handle.clone());
        state.inner.evaluation_sessions.write().await.insert(
            "eval-done".to_string(),
            crate::app::evaluation::EvaluationSessionResponse {
                evaluation_id: "eval-done".to_string(),
                seed: 7,
                subjects: vec![crate::app::evaluation::EvaluationSubjectResponse {
                    subject_id: "user:202".to_string(),
                    user_id: Some(202),
                    display_name: "Bot Subject".to_string(),
                    kind: "bot".to_string(),
                    table_code: "EVALDONE".to_string(),
                    phase: "playing".to_string(),
                    completed: false,
                    final_score: None,
                    deal_in_count: None,
                    win_count: None,
                    completed_round_count: None,
                    ready_hand_win_count: None,
                }],
            },
        );

        schedule_room_tasks(state.clone(), "EVALDONE".to_string()).await;

        assert!(handle.is_closed());
        assert!(room_handle(&state, "EVALDONE").await.is_none());
        assert!(worker.get_table("EVALDONE").await?.is_none());
        let sessions = state.inner.evaluation_sessions.read().await;
        let subject = &sessions
            .get("eval-done")
            .expect("evaluation session should remain")
            .subjects[0];
        assert!(subject.completed);
        assert_eq!(subject.phase, "finished");
        assert_eq!(subject.final_score, Some(32));
        assert_eq!(subject.win_count, Some(3));
        assert_eq!(subject.deal_in_count, Some(1));
        assert_eq!(subject.completed_round_count, Some(16));
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn keeps_finished_human_evaluation_room_available_for_refresh() -> Result<()> {
        let db = in_memory_database("")?;
        db.initialize()?;
        let worker = DbWorker::start(db)?;
        let owner_user_id = register_scheduler_test_user(&worker, "EVALHUMOWN", "Owner").await?;
        let subject_user_id =
            register_scheduler_test_user(&worker, "EVALHUMSUBJ", "Human Subject").await?;
        let mut room = build_evaluation_room(
            "EVALHUMDONE",
            owner_user_id,
            Some(subject_user_id),
            "Human Subject",
            false,
        );
        start_match_in_room_state(&mut room, 0, 7).expect("evaluation human room should start");
        room.phase = "finished".to_string();
        if let Some(seat) = room.seats.iter_mut().find(|seat| seat.seat_index == 0) {
            seat.connected = false;
        }
        if let Some(match_state) = room.match_state.as_mut() {
            match_state.match_finished = true;
            match_state.cumulative_scores.insert(0, 48);
            match_state.statistics.completed_round_count = 16;
        }
        let room_json = serialize_room_state(&room)?;
        worker
            .save_table_and_upsert_participant(
                "EVALHUMDONE",
                "2026-05-06T00:00:00Z",
                &room_json,
                0,
                subject_user_id,
                "Human Subject",
                "2026-05-06T00:00:00Z",
            )
            .await?;
        let state = AppContext::new(worker.clone());
        let handle = std::sync::Arc::new(crate::app::room_runtime::RoomHandle::new(
            RoomRuntime::new("2026-05-06T00:00:00Z".to_string(), room),
        ));
        state
            .inner
            .rooms
            .write()
            .await
            .insert("EVALHUMDONE".to_string(), handle.clone());

        schedule_room_tasks(state.clone(), "EVALHUMDONE".to_string()).await;

        assert!(!handle.is_closed());
        assert!(room_handle(&state, "EVALHUMDONE").await.is_some());
        assert!(worker.get_table("EVALHUMDONE").await?.is_some());
        assert!(
            worker
                .get_active_table_participant("EVALHUMDONE", subject_user_id)
                .await?
                .is_some()
        );
        let runtime = handle.runtime.lock().await;
        assert!(runtime.unattended_cleanup_task.is_none());
        Ok(())
    }
}
