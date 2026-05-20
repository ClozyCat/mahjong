use anyhow::Result;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::{Notify, mpsc};
use tower::ServiceExt;

use super::persistence::{DbWorker, in_memory_database};
use super::room_runtime::room_handle;
use super::{
    AppContext, ConnectionHandle, Settings, add_bot_to_waiting_room, parse_room_json,
    register_user_connection, serialize_room_state, server,
};
use crate::core::state::SeatState;

fn test_settings() -> Settings {
    Settings {
        bind_addr: "127.0.0.1:0".to_string(),
        database_path: ":memory:".to_string(),
        cors_origins: vec![],
        frontend_dir: None,
        dev_seed_user: None,
    }
}

async fn test_app() -> Result<(Router, DbWorker, AppContext)> {
    let db = in_memory_database("")?;
    db.initialize()?;
    let worker = DbWorker::start(db)?;
    let state = AppContext::new(worker.clone());
    Ok((
        server::build_app(state.clone(), &test_settings()),
        worker,
        state,
    ))
}

async fn json_response(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&bytes).expect("response body should be valid json")
}

fn json_request(method: Method, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("request should build")
}

fn authed_json_request(method: Method, uri: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .expect("request should build")
}

async fn register_user(
    app: &Router,
    worker: &DbWorker,
    invite_code: &str,
    display_name: &str,
) -> Result<(String, i64)> {
    worker
        .create_invite_code(invite_code, "2026-05-06T00:00:00Z", None)
        .await?;
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/auth/register",
            json!({
                "invite_code": invite_code,
                "display_name": display_name,
                "password": "secret-123",
            }),
        ))
        .await?;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_response(response).await;
    let token = body["session_token"]
        .as_str()
        .expect("session token should exist")
        .to_string();
    let user_id = body["user"]["user_id"]
        .as_i64()
        .expect("user id should exist");
    Ok((token, user_id))
}

async fn create_table(app: &Router, token: &str, multiplier: i64) -> Result<String> {
    let response = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/tables",
            token,
            json!({ "multiplier": multiplier }),
        ))
        .await?;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_response(response).await;
    Ok(body["table_code"]
        .as_str()
        .expect("table code should exist")
        .to_string())
}

async fn create_evaluation(app: &Router, token: &str, subject_user_ids: Vec<i64>) -> Result<Value> {
    let response = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/evaluations",
            token,
            json!({ "subject_user_ids": subject_user_ids }),
        ))
        .await?;
    assert_eq!(response.status(), StatusCode::CREATED);
    Ok(json_response(response).await)
}

async fn add_bots_to_table(
    state: &AppContext,
    worker: &DbWorker,
    table_code: &str,
    count: usize,
) -> Result<()> {
    let handle = room_handle(state, table_code)
        .await
        .expect("room should be loaded");
    let _persist_guard = handle.persist.lock().await;
    let mut runtime = handle.runtime.lock().await;
    for _ in 0..count {
        add_bot_to_waiting_room(&mut runtime.room).expect("bot seat should be added");
    }
    let created_at = runtime.created_at.clone();
    let room_json = serialize_room_state(&runtime.room)?;
    drop(runtime);
    worker
        .save_table(table_code, &created_at, &room_json)
        .await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn create_evaluation_includes_owner_and_creates_evaluation_room() -> Result<()> {
    let (app, worker, state) = test_app().await?;
    let (token, user_id) = register_user(&app, &worker, "INVITEEVAL01", "Alice").await?;

    let body = create_evaluation(&app, &token, vec![]).await?;
    let evaluation_id = body["evaluation_id"]
        .as_str()
        .expect("evaluation id should exist");
    let subject = &body["subjects"][0];
    let table_code = subject["table_code"]
        .as_str()
        .expect("table code should exist");

    assert!(evaluation_id.starts_with("eval-"));
    assert_eq!(subject["user_id"], user_id);
    assert_eq!(subject["kind"], "human");
    assert_eq!(subject["phase"], "waiting");

    let handle = room_handle(&state, table_code)
        .await
        .expect("evaluation room should be loaded");
    let runtime = handle.runtime.lock().await;
    assert_eq!(runtime.room.mode, crate::evaluation::EVALUATION_ROOM_MODE);
    assert_eq!(runtime.room.seats.len(), 4);
    assert_eq!(runtime.room.seats[0].user_id, Some(user_id));
    assert!(runtime.room.seats[1..].iter().all(|seat| seat.is_bot));
    drop(runtime);

    let response = app
        .clone()
        .oneshot(authed_json_request(
            Method::GET,
            &format!("/api/evaluations/{evaluation_id}"),
            &token,
            json!({}),
        ))
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let fetched = json_response(response).await;
    assert_eq!(fetched["evaluation_id"], evaluation_id);
    assert_eq!(fetched["subjects"][0]["table_code"], table_code);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn create_evaluation_rejects_more_than_four_subjects() -> Result<()> {
    let (app, worker, _state) = test_app().await?;
    let (token, _user_id) = register_user(&app, &worker, "INVITEEVAL02", "Alice").await?;

    let response = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/evaluations",
            &token,
            json!({ "subject_user_ids": [2, 3, 4, 5] }),
        ))
        .await?;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json_response(response).await;
    assert_eq!(body["detail"], "too_many_evaluation_subjects");
    Ok(())
}

async fn add_human_to_table(
    state: &AppContext,
    worker: &DbWorker,
    table_code: &str,
    user_id: i64,
    display_name: &str,
    seat_index: usize,
) -> Result<()> {
    let handle = room_handle(state, table_code)
        .await
        .expect("room should be loaded");
    let _persist_guard = handle.persist.lock().await;
    let mut runtime = handle.runtime.lock().await;
    runtime.room.seats.push(SeatState {
        seat_index,
        user_id: Some(user_id),
        nickname: Some(display_name.to_string()),
        points: Some(600),
        title: Some("平民".to_string()),
        connected: true,
        is_bot: false,
        seat_type: "human".to_string(),
        bot_persona: None,
        bot_aggression: None,
        disconnect_deadline_at: None,
        consecutive_timeout_auto_response_count: 0,
    });
    runtime.room.seats.sort_by_key(|seat| seat.seat_index);
    let created_at = runtime.created_at.clone();
    let room_json = serialize_room_state(&runtime.room)?;
    drop(runtime);
    worker
        .save_table_and_upsert_participant(
            table_code,
            &created_at,
            &room_json,
            seat_index,
            user_id,
            display_name,
            &created_at,
        )
        .await?;
    Ok(())
}

async fn add_bot_takeover_human_to_table(
    state: &AppContext,
    worker: &DbWorker,
    table_code: &str,
) -> Result<()> {
    let handle = room_handle(state, table_code)
        .await
        .expect("room should be loaded");
    let _persist_guard = handle.persist.lock().await;
    let mut runtime = handle.runtime.lock().await;
    runtime.room.seats.push(SeatState {
        seat_index: 0,
        user_id: None,
        nickname: Some("Hosted Player".to_string()),
        points: None,
        title: None,
        connected: true,
        is_bot: true,
        seat_type: "human".to_string(),
        bot_persona: None,
        bot_aggression: None,
        disconnect_deadline_at: None,
        consecutive_timeout_auto_response_count: 0,
    });
    let created_at = runtime.created_at.clone();
    let room_json = serialize_room_state(&runtime.room)?;
    drop(runtime);
    worker
        .save_table(table_code, &created_at, &room_json)
        .await?;
    Ok(())
}

async fn set_table_phase(
    state: &AppContext,
    worker: &DbWorker,
    table_code: &str,
    phase: &str,
) -> Result<()> {
    let handle = room_handle(state, table_code)
        .await
        .expect("room should be loaded");
    let _persist_guard = handle.persist.lock().await;
    let mut runtime = handle.runtime.lock().await;
    runtime.room.phase = phase.to_string();
    let created_at = runtime.created_at.clone();
    let room_json = serialize_room_state(&runtime.room)?;
    drop(runtime);
    worker
        .save_table(table_code, &created_at, &room_json)
        .await?;
    Ok(())
}

fn test_connection(id: u64) -> (ConnectionHandle, mpsc::Receiver<String>) {
    let (sender, receiver) = mpsc::channel(8);
    (
        ConnectionHandle {
            id,
            sender,
            close_requested: Arc::new(AtomicBool::new(false)),
            close_notify: Arc::new(Notify::new()),
        },
        receiver,
    )
}

#[tokio::test(flavor = "current_thread")]
async fn multiplier_create_table_always_stores_default_multiplier() -> Result<()> {
    let (app, worker, _state) = test_app().await?;
    let (token, user_id) = register_user(&app, &worker, "INVITE100001", "Owner").await?;

    let response = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/tables",
            &token,
            json!({ "multiplier": 3 }),
        ))
        .await?;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_response(response).await;
    let table_code = body["table_code"]
        .as_str()
        .expect("table code should exist");

    let table = worker
        .get_table(table_code)
        .await?
        .expect("table should be persisted");
    let room = parse_room_json(&table.room_json)?;
    assert_eq!(room.owner_user_id, Some(user_id));
    assert_eq!(room.multiplier, 1);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn multiplier_owner_can_only_keep_default_while_waiting() -> Result<()> {
    let (app, worker, _state) = test_app().await?;
    let (token, _user_id) = register_user(&app, &worker, "INVITE100002", "Owner").await?;

    let create = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/tables",
            &token,
            json!({ "multiplier": 1 }),
        ))
        .await?;
    let create_body = json_response(create).await;
    let table_code = create_body["table_code"]
        .as_str()
        .expect("table code should exist");

    let update = app
        .clone()
        .oneshot(authed_json_request(
            Method::PATCH,
            &format!("/api/tables/{table_code}/multiplier"),
            &token,
            json!({ "multiplier": 1 }),
        ))
        .await?;
    assert_eq!(update.status(), StatusCode::OK);

    let table = worker
        .get_table(table_code)
        .await?
        .expect("table should be persisted");
    let room = parse_room_json(&table.room_json)?;
    assert_eq!(room.multiplier, 1);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn multiplier_owner_cannot_set_non_default_after_start() -> Result<()> {
    let (app, worker, state) = test_app().await?;
    let (token, _user_id) = register_user(&app, &worker, "INVITE100003", "Owner").await?;

    let create = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/tables",
            &token,
            json!({ "multiplier": 1 }),
        ))
        .await?;
    let create_body = json_response(create).await;
    let table_code = create_body["table_code"]
        .as_str()
        .expect("table code should exist")
        .to_string();

    let room = room_handle(&state, &table_code)
        .await
        .expect("room should be loaded");
    {
        let mut runtime = room.runtime.lock().await;
        runtime.room.phase = "playing".to_string();
    }

    let update = app
        .clone()
        .oneshot(authed_json_request(
            Method::PATCH,
            &format!("/api/tables/{table_code}/multiplier"),
            &token,
            json!({ "multiplier": 3 }),
        ))
        .await?;
    assert_eq!(update.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json_response(update).await;
    assert_eq!(body["detail"], "invalid_multiplier");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn multiplier_non_owner_cannot_touch_default_table_setting() -> Result<()> {
    let (app, worker, _state) = test_app().await?;
    let (owner_token, _owner_user_id) =
        register_user(&app, &worker, "INVITE100004", "Owner").await?;
    let (guest_token, _guest_user_id) =
        register_user(&app, &worker, "INVITE100005", "Guest").await?;

    let create = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/tables",
            &owner_token,
            json!({ "multiplier": 1 }),
        ))
        .await?;
    let create_body = json_response(create).await;
    let table_code = create_body["table_code"]
        .as_str()
        .expect("table code should exist");

    let update = app
        .clone()
        .oneshot(authed_json_request(
            Method::PATCH,
            &format!("/api/tables/{table_code}/multiplier"),
            &guest_token,
            json!({ "multiplier": 1 }),
        ))
        .await?;
    assert_eq!(update.status(), StatusCode::FORBIDDEN);
    let body = json_response(update).await;
    assert_eq!(body["detail"], "only_owner_can_change_multiplier");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invite_only_idle_user_invite_succeeds_and_is_visible_in_me_invites() -> Result<()> {
    let (app, worker, state) = test_app().await?;
    let (owner_token, _owner_id) = register_user(&app, &worker, "INVITE100006", "Owner").await?;
    let (_guest_token, guest_id) = register_user(&app, &worker, "INVITE100007", "Guest").await?;

    let table_code = create_table(&app, &owner_token, 1).await?;
    add_bots_to_table(&state, &worker, &table_code, 1).await?;
    let invite_response = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_code}/invites"),
            &owner_token,
            json!({ "invitee_user_id": guest_id }),
        ))
        .await?;
    assert_eq!(invite_response.status(), StatusCode::CREATED);

    let me_invites = app
        .clone()
        .oneshot(authed_json_request(
            Method::GET,
            "/api/me/invites",
            &owner_token.replace("Owner", "Owner"),
            json!({}),
        ))
        .await?;
    assert_eq!(me_invites.status(), StatusCode::OK);

    let (_guest_token, _) = register_user(&app, &worker, "INVITE100008", "Spare").await?;
    let guest_token = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/auth/login",
            json!({
                "identifier": "Guest",
                "password": "secret-123"
            }),
        ))
        .await?;
    let guest_token_body = json_response(guest_token).await;
    let session_token = guest_token_body["session_token"]
        .as_str()
        .expect("guest login should return session")
        .to_string();

    let guest_invites = app
        .clone()
        .oneshot(authed_json_request(
            Method::GET,
            "/api/me/invites",
            &session_token,
            json!({}),
        ))
        .await?;
    assert_eq!(guest_invites.status(), StatusCode::OK);
    let invites_body = json_response(guest_invites).await;
    assert_eq!(invites_body.as_array().map(Vec::len), Some(1));
    assert_eq!(invites_body[0]["table_code"], table_code);
    assert_eq!(invites_body[0]["status"], "pending");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invite_only_latest_pending_invite_from_same_inviter_replaces_older_one() -> Result<()> {
    let (app, worker, state) = test_app().await?;
    let (owner_token, _owner_id) = register_user(&app, &worker, "INVITE100090", "Owner").await?;
    let (guest_token, guest_id) = register_user(&app, &worker, "INVITE100091", "Guest").await?;

    let table_code = create_table(&app, &owner_token, 1).await?;
    add_bots_to_table(&state, &worker, &table_code, 1).await?;

    let first_invite = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_code}/invites"),
            &owner_token,
            json!({ "invitee_user_id": guest_id }),
        ))
        .await?;
    assert_eq!(first_invite.status(), StatusCode::CREATED);
    let first_invite_body = json_response(first_invite).await;
    let first_invite_id = first_invite_body["id"]
        .as_i64()
        .expect("first invite id should exist");

    let second_invite = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_code}/invites"),
            &owner_token,
            json!({ "invitee_user_id": guest_id }),
        ))
        .await?;
    assert_eq!(second_invite.status(), StatusCode::CREATED);
    let second_invite_body = json_response(second_invite).await;
    let second_invite_id = second_invite_body["id"]
        .as_i64()
        .expect("second invite id should exist");
    assert_ne!(first_invite_id, second_invite_id);

    let guest_invites = app
        .clone()
        .oneshot(authed_json_request(
            Method::GET,
            "/api/me/invites",
            &guest_token,
            json!({}),
        ))
        .await?;
    assert_eq!(guest_invites.status(), StatusCode::OK);
    let invites_body = json_response(guest_invites).await;
    assert_eq!(invites_body.as_array().map(Vec::len), Some(1));
    assert_eq!(invites_body[0]["id"], second_invite_id);
    assert_eq!(invites_body[0]["status"], "pending");

    let stale_accept = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/invites/{first_invite_id}/accept"),
            &guest_token,
            json!({}),
        ))
        .await?;
    assert_eq!(stale_accept.status(), StatusCode::UNPROCESSABLE_ENTITY);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invite_only_create_requires_open_or_replaceable_seat() -> Result<()> {
    let (app, worker, state) = test_app().await?;
    let (owner_token, _owner_id) = register_user(&app, &worker, "INVITE100050", "Owner").await?;
    let (_guest_token, guest_id) = register_user(&app, &worker, "INVITE100051", "Guest").await?;

    let table_code = create_table(&app, &owner_token, 1).await?;
    add_bot_takeover_human_to_table(&state, &worker, &table_code).await?;
    {
        let handle = room_handle(&state, &table_code)
            .await
            .expect("room should be loaded");
        let _persist_guard = handle.persist.lock().await;
        let mut runtime = handle.runtime.lock().await;
        for seat_index in 1..4 {
            runtime.room.seats.push(SeatState {
                seat_index,
                user_id: None,
                nickname: Some(format!("Player {seat_index}")),
                points: None,
                title: None,
                connected: true,
                is_bot: false,
                seat_type: "human".to_string(),
                bot_persona: None,
                bot_aggression: None,
                disconnect_deadline_at: None,
                consecutive_timeout_auto_response_count: 0,
            });
        }
        let created_at = runtime.created_at.clone();
        let room_json = serialize_room_state(&runtime.room)?;
        drop(runtime);
        worker
            .save_table(&table_code, &created_at, &room_json)
            .await?;
    }
    let invite = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_code}/invites"),
            &owner_token,
            json!({ "invitee_user_id": guest_id }),
        ))
        .await?;

    assert_eq!(invite.status(), StatusCode::CONFLICT);
    let body = json_response(invite).await;
    assert_eq!(body["detail"], "table_full");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invite_only_create_allows_empty_waiting_seat() -> Result<()> {
    let (app, worker, _state) = test_app().await?;
    let (owner_token, _owner_id) = register_user(&app, &worker, "INVITE100054", "Owner").await?;
    let (_guest_token, guest_id) = register_user(&app, &worker, "INVITE100055", "Guest").await?;

    let table_code = create_table(&app, &owner_token, 1).await?;
    let invite = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_code}/invites"),
            &owner_token,
            json!({ "invitee_user_id": guest_id }),
        ))
        .await?;

    assert_eq!(invite.status(), StatusCode::CREATED);
    let body = json_response(invite).await;
    assert_eq!(body["table_code"], table_code);
    assert_eq!(body["status"], "pending");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invite_only_create_allows_playing_table_with_replaceable_bot() -> Result<()> {
    let (app, worker, state) = test_app().await?;
    let (owner_token, _owner_id) = register_user(&app, &worker, "INVITE100056", "Owner").await?;
    let (_guest_token, guest_id) = register_user(&app, &worker, "INVITE100057", "Guest").await?;

    let table_code = create_table(&app, &owner_token, 1).await?;
    add_bots_to_table(&state, &worker, &table_code, 1).await?;
    set_table_phase(&state, &worker, &table_code, "playing").await?;

    let invite = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_code}/invites"),
            &owner_token,
            json!({ "invitee_user_id": guest_id }),
        ))
        .await?;

    assert_eq!(invite.status(), StatusCode::CREATED);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invite_only_full_bot_takeover_human_seat_is_not_replaceable() -> Result<()> {
    let (app, worker, state) = test_app().await?;
    let (owner_token, _owner_id) = register_user(&app, &worker, "INVITE100052", "Owner").await?;
    let (_guest_token, guest_id) = register_user(&app, &worker, "INVITE100053", "Guest").await?;

    let table_code = create_table(&app, &owner_token, 1).await?;
    add_bot_takeover_human_to_table(&state, &worker, &table_code).await?;
    {
        let handle = room_handle(&state, &table_code)
            .await
            .expect("room should be loaded");
        let _persist_guard = handle.persist.lock().await;
        let mut runtime = handle.runtime.lock().await;
        for seat_index in 1..4 {
            runtime.room.seats.push(SeatState {
                seat_index,
                user_id: None,
                nickname: Some(format!("Player {seat_index}")),
                points: None,
                title: None,
                connected: true,
                is_bot: false,
                seat_type: "human".to_string(),
                bot_persona: None,
                bot_aggression: None,
                disconnect_deadline_at: None,
                consecutive_timeout_auto_response_count: 0,
            });
        }
        let created_at = runtime.created_at.clone();
        let room_json = serialize_room_state(&runtime.room)?;
        drop(runtime);
        worker
            .save_table(&table_code, &created_at, &room_json)
            .await?;
    }

    let invite = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_code}/invites"),
            &owner_token,
            json!({ "invitee_user_id": guest_id }),
        ))
        .await?;

    assert_eq!(invite.status(), StatusCode::CONFLICT);
    let body = json_response(invite).await;
    assert_eq!(body["detail"], "table_full");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invite_only_user_in_self_plus_bots_table_can_still_be_invited() -> Result<()> {
    let (app, worker, state) = test_app().await?;
    let (owner_a_token, _owner_a_id) =
        register_user(&app, &worker, "INVITE100009", "OwnerA").await?;
    let (owner_b_token, _owner_b_id) =
        register_user(&app, &worker, "INVITE100010", "OwnerB").await?;
    let (_guest_token, guest_id) = register_user(&app, &worker, "INVITE100011", "Guest").await?;

    let table_a = create_table(&app, &owner_a_token, 1).await?;
    add_bots_to_table(&state, &worker, &table_a, 1).await?;
    let invite_a = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_a}/invites"),
            &owner_a_token,
            json!({ "invitee_user_id": guest_id }),
        ))
        .await?;
    let invite_a_body = json_response(invite_a).await;
    let invite_a_id = invite_a_body["id"]
        .as_i64()
        .expect("invite id should exist");

    let guest_login = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/auth/login",
            json!({
                "identifier": "Guest",
                "password": "secret-123"
            }),
        ))
        .await?;
    let guest_body = json_response(guest_login).await;
    let guest_token = guest_body["session_token"]
        .as_str()
        .expect("guest login should return token")
        .to_string();
    let accept = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/invites/{invite_a_id}/accept"),
            &guest_token,
            json!({}),
        ))
        .await?;
    assert_eq!(accept.status(), StatusCode::OK);

    let table_b = create_table(&app, &owner_b_token, 1).await?;
    add_bots_to_table(&state, &worker, &table_b, 1).await?;
    let invite_b = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_b}/invites"),
            &owner_b_token,
            json!({ "invitee_user_id": guest_id }),
        ))
        .await?;
    assert_eq!(invite_b.status(), StatusCode::CREATED);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invite_only_accept_rejects_other_pending_invites_for_invitee() -> Result<()> {
    let (app, worker, state) = test_app().await?;
    let (owner_a_token, _owner_a_id) =
        register_user(&app, &worker, "INVITE100092", "OwnerA").await?;
    let (owner_b_token, _owner_b_id) =
        register_user(&app, &worker, "INVITE100093", "OwnerB").await?;
    let (guest_token, guest_id) = register_user(&app, &worker, "INVITE100094", "Guest").await?;

    let table_a = create_table(&app, &owner_a_token, 1).await?;
    add_bots_to_table(&state, &worker, &table_a, 1).await?;
    let invite_a = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_a}/invites"),
            &owner_a_token,
            json!({ "invitee_user_id": guest_id }),
        ))
        .await?;
    assert_eq!(invite_a.status(), StatusCode::CREATED);
    let invite_a_body = json_response(invite_a).await;
    let invite_a_id = invite_a_body["id"]
        .as_i64()
        .expect("first invite id should exist");

    let table_b = create_table(&app, &owner_b_token, 1).await?;
    add_bots_to_table(&state, &worker, &table_b, 1).await?;
    let invite_b = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_b}/invites"),
            &owner_b_token,
            json!({ "invitee_user_id": guest_id }),
        ))
        .await?;
    assert_eq!(invite_b.status(), StatusCode::CREATED);
    let invite_b_body = json_response(invite_b).await;
    let invite_b_id = invite_b_body["id"]
        .as_i64()
        .expect("second invite id should exist");

    let accept = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/invites/{invite_b_id}/accept"),
            &guest_token,
            json!({}),
        ))
        .await?;
    assert_eq!(accept.status(), StatusCode::OK);

    let accepted = worker
        .get_table_invite(invite_b_id)
        .await?
        .expect("accepted invite should remain queryable");
    assert_eq!(accepted.status, "accepted");
    let rejected = worker
        .get_table_invite(invite_a_id)
        .await?
        .expect("older invite should remain queryable");
    assert_eq!(rejected.status, "rejected");

    let me_invites = app
        .clone()
        .oneshot(authed_json_request(
            Method::GET,
            "/api/me/invites",
            &guest_token,
            json!({}),
        ))
        .await?;
    assert_eq!(me_invites.status(), StatusCode::OK);
    let invites_body = json_response(me_invites).await;
    assert_eq!(
        invites_body
            .as_array()
            .expect("invites should be an array")
            .len(),
        0
    );

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invite_only_accepted_invites_are_omitted_from_me_invites() -> Result<()> {
    let (app, worker, state) = test_app().await?;
    let (owner_token, _owner_id) = register_user(&app, &worker, "INVITE100012", "Owner").await?;
    let (guest_token, guest_id) = register_user(&app, &worker, "INVITE100013", "Guest").await?;

    let table_code = create_table(&app, &owner_token, 1).await?;
    add_bots_to_table(&state, &worker, &table_code, 1).await?;
    let invite = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_code}/invites"),
            &owner_token,
            json!({ "invitee_user_id": guest_id }),
        ))
        .await?;
    assert_eq!(invite.status(), StatusCode::CREATED);
    let invite_body = json_response(invite).await;
    let invite_id = invite_body["id"].as_i64().expect("invite id should exist");

    let accept = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/invites/{invite_id}/accept"),
            &guest_token,
            json!({}),
        ))
        .await?;
    assert_eq!(accept.status(), StatusCode::OK);

    let me_invites = app
        .clone()
        .oneshot(authed_json_request(
            Method::GET,
            "/api/me/invites",
            &guest_token,
            json!({}),
        ))
        .await?;
    assert_eq!(me_invites.status(), StatusCode::OK);
    let invites_body = json_response(me_invites).await;
    assert_eq!(invites_body.as_array().map(Vec::len), Some(0));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invite_only_user_with_other_human_in_table_is_busy() -> Result<()> {
    let (app, worker, state) = test_app().await?;
    let (owner_a_token, _owner_a_id) =
        register_user(&app, &worker, "INVITE100014", "OwnerA").await?;
    let (owner_b_token, _owner_b_id) =
        register_user(&app, &worker, "INVITE100015", "OwnerB").await?;
    let (_guest_token, guest_id) = register_user(&app, &worker, "INVITE100016", "Guest").await?;
    let (_third_token, third_id) = register_user(&app, &worker, "INVITE100017", "Third").await?;

    let table_a = create_table(&app, &owner_a_token, 1).await?;
    add_bots_to_table(&state, &worker, &table_a, 2).await?;
    let guest_invite = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_a}/invites"),
            &owner_a_token,
            json!({ "invitee_user_id": guest_id }),
        ))
        .await?;
    let guest_invite_body = json_response(guest_invite).await;
    let guest_invite_id = guest_invite_body["id"]
        .as_i64()
        .expect("invite id should exist");
    let third_invite = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_a}/invites"),
            &owner_a_token,
            json!({ "invitee_user_id": third_id }),
        ))
        .await?;
    let third_invite_body = json_response(third_invite).await;
    let third_invite_id = third_invite_body["id"]
        .as_i64()
        .expect("invite id should exist");

    let guest_login = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/auth/login",
            json!({
                "identifier": "Guest",
                "password": "secret-123"
            }),
        ))
        .await?;
    let guest_body = json_response(guest_login).await;
    let guest_token = guest_body["session_token"]
        .as_str()
        .expect("guest login should return token")
        .to_string();
    let third_login = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/auth/login",
            json!({
                "identifier": "Third",
                "password": "secret-123"
            }),
        ))
        .await?;
    let third_body = json_response(third_login).await;
    let third_token = third_body["session_token"]
        .as_str()
        .expect("third login should return token")
        .to_string();

    let accept_guest = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/invites/{guest_invite_id}/accept"),
            &guest_token,
            json!({}),
        ))
        .await?;
    assert_eq!(accept_guest.status(), StatusCode::OK);
    let accept_third = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/invites/{third_invite_id}/accept"),
            &third_token,
            json!({}),
        ))
        .await?;
    assert_eq!(accept_third.status(), StatusCode::OK);

    let table_b = create_table(&app, &owner_b_token, 1).await?;
    add_bots_to_table(&state, &worker, &table_b, 1).await?;
    let invite_b = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_b}/invites"),
            &owner_b_token,
            json!({ "invitee_user_id": guest_id }),
        ))
        .await?;
    assert_eq!(invite_b.status(), StatusCode::CONFLICT);
    let body = json_response(invite_b).await;
    assert_eq!(body["detail"], "target_player_busy");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invite_only_accept_creates_table_participant() -> Result<()> {
    let (app, worker, state) = test_app().await?;
    let (owner_token, owner_id) = register_user(&app, &worker, "INVITE100016", "Owner").await?;
    let (_guest_token, guest_id) = register_user(&app, &worker, "INVITE100017", "Guest").await?;
    let (connection, mut receiver) = test_connection(1003);

    register_user_connection(&state, owner_id, connection).await;
    let _ = receiver.recv().await;

    let table_code = create_table(&app, &owner_token, 1).await?;
    add_bots_to_table(&state, &worker, &table_code, 1).await?;
    let invite = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_code}/invites"),
            &owner_token,
            json!({ "invitee_user_id": guest_id }),
        ))
        .await?;
    let invite_body = json_response(invite).await;
    let invite_id = invite_body["id"].as_i64().expect("invite id should exist");

    let guest_login = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/auth/login",
            json!({
                "identifier": "Guest",
                "password": "secret-123"
            }),
        ))
        .await?;
    let guest_body = json_response(guest_login).await;
    let guest_token = guest_body["session_token"]
        .as_str()
        .expect("guest login should return token")
        .to_string();
    let accept = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/invites/{invite_id}/accept"),
            &guest_token,
            json!({}),
        ))
        .await?;
    assert_eq!(accept.status(), StatusCode::OK);
    let accept_body = json_response(accept).await;
    assert_eq!(accept_body["seat_index"], 0);

    let invite_notification = receiver
        .recv()
        .await
        .expect("inviter should receive accepted invite update");
    let invite_payload: Value = serde_json::from_str(&invite_notification)?;
    assert_eq!(invite_payload["type"], "table_invite_decided");
    assert_eq!(invite_payload["payload"]["id"], invite_id);
    assert_eq!(invite_payload["payload"]["status"], "accepted");

    let notification = receiver
        .recv()
        .await
        .expect("social listeners should receive active table updates");
    let payload: Value = serde_json::from_str(&notification)?;
    assert_eq!(payload["type"], "user_active_table_updated");
    assert_eq!(payload["payload"]["user_id"], guest_id);
    assert_eq!(payload["payload"]["active_table_code"], table_code);

    let participant = worker
        .get_active_table_participant(&table_code, guest_id)
        .await?
        .expect("participant should exist after accepting invite");
    assert_eq!(participant.table_code, table_code);
    assert_eq!(participant.user_id, guest_id);

    let table = worker
        .get_table(&table_code)
        .await?
        .expect("table should exist after accepting invite");
    let room = parse_room_json(&table.room_json)?;
    assert_eq!(room.seats.len(), 1);
    let seat = room
        .seats
        .iter()
        .find(|seat| seat.seat_index == 0)
        .expect("replaced seat should exist");
    assert_eq!(seat.seat_type, "human");
    assert!(!seat.is_bot);
    assert_eq!(seat.nickname.as_deref(), Some("Guest"));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invite_special_bot_auto_accepts_and_reserves_seat() -> Result<()> {
    let (app, worker, state) = test_app().await?;
    server::seed_special_bot_users(&state).await?;
    let bot = worker
        .find_user_by_identifier("bot_schubert")
        .await?
        .expect("special bot should be seeded");
    let (owner_token, owner_id) = register_user(&app, &worker, "INVITE100100", "Owner").await?;
    let (connection, mut receiver) = test_connection(1004);

    register_user_connection(&state, owner_id, connection).await;
    let _ = receiver.recv().await;

    let table_code = create_table(&app, &owner_token, 1).await?;
    add_human_to_table(&state, &worker, &table_code, owner_id, "Owner", 0).await?;
    add_bots_to_table(&state, &worker, &table_code, 1).await?;

    let invite = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_code}/invites"),
            &owner_token,
            json!({ "invitee_user_id": bot.user_id }),
        ))
        .await?;

    assert_eq!(invite.status(), StatusCode::CREATED);
    let invite_body = json_response(invite).await;
    assert_eq!(invite_body["status"], "accepted");
    assert!(invite_body["accepted_at"].as_str().is_some());

    let notification = receiver
        .recv()
        .await
        .expect("social listeners should receive active table updates");
    let payload: Value = serde_json::from_str(&notification)?;
    assert_eq!(payload["type"], "user_active_table_updated");
    assert_eq!(payload["payload"]["user_id"], bot.user_id);
    assert_eq!(payload["payload"]["active_table_code"], table_code);

    let participant = worker
        .get_active_table_participant(&table_code, bot.user_id)
        .await?
        .expect("special bot participant should exist");
    assert_eq!(participant.seat_index, 1);
    assert_eq!(participant.nickname_snapshot, "舒伯特");

    let table = worker
        .get_table(&table_code)
        .await?
        .expect("table should exist after auto accept");
    let room = parse_room_json(&table.room_json)?;
    let seat = room
        .seats
        .iter()
        .find(|seat| seat.seat_index == 1)
        .expect("special bot seat should exist");
    assert_eq!(seat.user_id, Some(bot.user_id));
    assert_eq!(seat.nickname.as_deref(), Some("舒伯特"));
    assert_eq!(seat.seat_type, crate::special_bots::SPECIAL_BOT_SEAT_TYPE);
    assert!(seat.is_bot);
    assert!(seat.connected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invite_special_bot_in_other_table_is_busy() -> Result<()> {
    let (app, worker, state) = test_app().await?;
    server::seed_special_bot_users(&state).await?;
    let bot = worker
        .find_user_by_identifier("bot_schubert")
        .await?
        .expect("special bot should be seeded");
    let (owner_a_token, owner_a_id) =
        register_user(&app, &worker, "INVITE100101", "Owner A").await?;
    let (owner_b_token, owner_b_id) =
        register_user(&app, &worker, "INVITE100102", "Owner B").await?;

    let table_a = create_table(&app, &owner_a_token, 1).await?;
    add_human_to_table(&state, &worker, &table_a, owner_a_id, "Owner A", 0).await?;
    add_bots_to_table(&state, &worker, &table_a, 1).await?;
    let first_invite = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_a}/invites"),
            &owner_a_token,
            json!({ "invitee_user_id": bot.user_id }),
        ))
        .await?;
    assert_eq!(first_invite.status(), StatusCode::CREATED);

    let table_b = create_table(&app, &owner_b_token, 1).await?;
    add_human_to_table(&state, &worker, &table_b, owner_b_id, "Owner B", 0).await?;
    add_bots_to_table(&state, &worker, &table_b, 1).await?;
    let second_invite = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_b}/invites"),
            &owner_b_token,
            json!({ "invitee_user_id": bot.user_id }),
        ))
        .await?;

    assert_eq!(second_invite.status(), StatusCode::CONFLICT);
    let body = json_response(second_invite).await;
    assert_eq!(body["detail"], "target_player_busy");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invite_only_accept_uses_empty_waiting_seat() -> Result<()> {
    let (app, worker, _state) = test_app().await?;
    let (owner_token, _owner_id) = register_user(&app, &worker, "INVITE100058", "Owner").await?;
    let (guest_token, guest_id) = register_user(&app, &worker, "INVITE100059", "Guest").await?;

    let table_code = create_table(&app, &owner_token, 1).await?;
    let invite = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_code}/invites"),
            &owner_token,
            json!({ "invitee_user_id": guest_id }),
        ))
        .await?;
    assert_eq!(invite.status(), StatusCode::CREATED);
    let invite_body = json_response(invite).await;
    let invite_id = invite_body["id"].as_i64().expect("invite id should exist");

    let accept = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/invites/{invite_id}/accept"),
            &guest_token,
            json!({}),
        ))
        .await?;

    assert_eq!(accept.status(), StatusCode::OK);
    let accept_body = json_response(accept).await;
    let seat_index = accept_body["seat_index"]
        .as_u64()
        .expect("seat index should exist") as usize;
    assert!(seat_index < 4);
    let participant = worker
        .get_active_table_participant(&table_code, guest_id)
        .await?
        .expect("participant should exist after accepting invite");
    assert_eq!(participant.seat_index, seat_index);
    let table = worker
        .get_table(&table_code)
        .await?
        .expect("table should exist after accepting invite");
    let room = parse_room_json(&table.room_json)?;
    room.seats
        .iter()
        .find(|seat| seat.user_id == Some(guest_id))
        .expect("guest seat should exist");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invite_only_accept_allows_playing_table_with_replaceable_bot() -> Result<()> {
    let (app, worker, state) = test_app().await?;
    let (owner_token, _owner_id) = register_user(&app, &worker, "INVITE100060", "Owner").await?;
    let (guest_token, guest_id) = register_user(&app, &worker, "INVITE100061", "Guest").await?;

    let table_code = create_table(&app, &owner_token, 1).await?;
    add_bots_to_table(&state, &worker, &table_code, 1).await?;
    let invite = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_code}/invites"),
            &owner_token,
            json!({ "invitee_user_id": guest_id }),
        ))
        .await?;
    assert_eq!(invite.status(), StatusCode::CREATED);
    let invite_body = json_response(invite).await;
    let invite_id = invite_body["id"].as_i64().expect("invite id should exist");
    set_table_phase(&state, &worker, &table_code, "playing").await?;

    let accept = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/invites/{invite_id}/accept"),
            &guest_token,
            json!({}),
        ))
        .await?;

    assert_eq!(accept.status(), StatusCode::OK);
    let participant = worker
        .get_active_table_participant(&table_code, guest_id)
        .await?
        .expect("participant should exist after accepting invite");
    assert_eq!(participant.seat_index, 0);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invite_only_reject_marks_invite_and_notifies_inviter() -> Result<()> {
    let (app, worker, state) = test_app().await?;
    let (owner_token, owner_id) = register_user(&app, &worker, "INVITE100062", "Owner").await?;
    let (guest_token, guest_id) = register_user(&app, &worker, "INVITE100063", "Guest").await?;
    let (connection, mut receiver) = test_connection(1002);

    register_user_connection(&state, owner_id, connection).await;
    let _ = receiver.recv().await;

    let table_code = create_table(&app, &owner_token, 1).await?;
    let invite = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_code}/invites"),
            &owner_token,
            json!({ "invitee_user_id": guest_id }),
        ))
        .await?;
    assert_eq!(invite.status(), StatusCode::CREATED);
    let invite_body = json_response(invite).await;
    let invite_id = invite_body["id"].as_i64().expect("invite id should exist");

    let reject = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/invites/{invite_id}/reject"),
            &guest_token,
            json!({}),
        ))
        .await?;

    assert_eq!(reject.status(), StatusCode::OK);
    let body = json_response(reject).await;
    assert_eq!(body["status"], "rejected");

    let notification = receiver
        .recv()
        .await
        .expect("owner should receive realtime invite decision");
    let payload: Value = serde_json::from_str(&notification)?;
    assert_eq!(payload["type"], "table_invite_decided");
    assert_eq!(payload["payload"]["id"], invite_id);
    assert_eq!(payload["payload"]["status"], "rejected");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invite_only_create_table_invite_pushes_realtime_notification() -> Result<()> {
    let (app, worker, state) = test_app().await?;
    let (owner_token, _owner_id) = register_user(&app, &worker, "INVITE100018", "Owner").await?;
    let (_guest_token, guest_id) = register_user(&app, &worker, "INVITE100019", "Guest").await?;
    let (connection, mut receiver) = test_connection(1001);

    register_user_connection(&state, guest_id, connection).await;
    let _ = receiver.recv().await;

    let table_code = create_table(&app, &owner_token, 1).await?;
    add_bots_to_table(&state, &worker, &table_code, 1).await?;
    let invite = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_code}/invites"),
            &owner_token,
            json!({ "invitee_user_id": guest_id }),
        ))
        .await?;
    assert_eq!(invite.status(), StatusCode::CREATED);

    let notification = receiver
        .recv()
        .await
        .expect("guest should receive realtime invite notification");
    let payload: Value = serde_json::from_str(&notification)?;
    assert_eq!(payload["type"], "table_invite_created");
    assert_eq!(payload["payload"]["table_code"], table_code);
    assert_eq!(payload["payload"]["invitee_user_id"], guest_id);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invite_only_my_invites_omits_deleted_tables() -> Result<()> {
    let (app, worker, state) = test_app().await?;
    let (owner_token, _owner_id) = register_user(&app, &worker, "INVITE100040", "Owner").await?;
    let (guest_token, guest_id) = register_user(&app, &worker, "INVITE100041", "Guest").await?;

    let table_code = create_table(&app, &owner_token, 1).await?;
    add_bots_to_table(&state, &worker, &table_code, 1).await?;
    let invite = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_code}/invites"),
            &owner_token,
            json!({ "invitee_user_id": guest_id }),
        ))
        .await?;
    assert_eq!(invite.status(), StatusCode::CREATED);

    worker
        .delete_table(&table_code, "2026-05-06T12:30:00Z")
        .await?;

    let my_invites = app
        .clone()
        .oneshot(authed_json_request(
            Method::GET,
            "/api/me/invites",
            &guest_token,
            json!({}),
        ))
        .await?;
    assert_eq!(my_invites.status(), StatusCode::OK);
    let body = json_response(my_invites).await;
    assert_eq!(body.as_array().map(Vec::len), Some(0));
    Ok(())
}
