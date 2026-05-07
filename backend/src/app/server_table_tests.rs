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
    AppContext, ConnectionHandle, Settings, parse_room_json, register_user_connection, server,
};

fn test_settings() -> Settings {
    Settings {
        bind_addr: "127.0.0.1:0".to_string(),
        database_path: ":memory:".to_string(),
        cors_origins: vec![],
        frontend_dir: None,
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
    let (app, worker, _state) = test_app().await?;
    let (owner_token, _owner_id) = register_user(&app, &worker, "INVITE100006", "Owner").await?;
    let (_guest_token, guest_id) = register_user(&app, &worker, "INVITE100007", "Guest").await?;

    let table_code = create_table(&app, &owner_token, 1).await?;
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
async fn invite_only_user_in_self_plus_bots_table_can_still_be_invited() -> Result<()> {
    let (app, worker, _state) = test_app().await?;
    let (owner_a_token, _owner_a_id) =
        register_user(&app, &worker, "INVITE100009", "OwnerA").await?;
    let (owner_b_token, _owner_b_id) =
        register_user(&app, &worker, "INVITE100010", "OwnerB").await?;
    let (_guest_token, guest_id) = register_user(&app, &worker, "INVITE100011", "Guest").await?;

    let table_a = create_table(&app, &owner_a_token, 1).await?;
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
async fn invite_only_accepted_invites_are_omitted_from_me_invites() -> Result<()> {
    let (app, worker, _state) = test_app().await?;
    let (owner_token, _owner_id) = register_user(&app, &worker, "INVITE100012", "Owner").await?;
    let (guest_token, guest_id) = register_user(&app, &worker, "INVITE100013", "Guest").await?;

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
    let (app, worker, _state) = test_app().await?;
    let (owner_a_token, _owner_a_id) =
        register_user(&app, &worker, "INVITE100014", "OwnerA").await?;
    let (owner_b_token, _owner_b_id) =
        register_user(&app, &worker, "INVITE100015", "OwnerB").await?;
    let (_guest_token, guest_id) = register_user(&app, &worker, "INVITE100016", "Guest").await?;
    let (_third_token, third_id) = register_user(&app, &worker, "INVITE100017", "Third").await?;

    let table_a = create_table(&app, &owner_a_token, 1).await?;
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
    let (app, worker, _state) = test_app().await?;
    let (owner_token, _owner_id) = register_user(&app, &worker, "INVITE100016", "Owner").await?;
    let (_guest_token, guest_id) = register_user(&app, &worker, "INVITE100017", "Guest").await?;

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

    let participant = worker
        .get_active_table_participant(&table_code, guest_id)
        .await?
        .expect("participant should exist after accepting invite");
    assert_eq!(participant.table_code, table_code);
    assert_eq!(participant.user_id, guest_id);
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
    let (app, worker, _state) = test_app().await?;
    let (owner_token, _owner_id) = register_user(&app, &worker, "INVITE100040", "Owner").await?;
    let (guest_token, guest_id) = register_user(&app, &worker, "INVITE100041", "Guest").await?;

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

#[tokio::test(flavor = "current_thread")]
async fn spectator_non_player_request_creates_pending_request_and_owner_can_approve() -> Result<()>
{
    let (app, worker, _state) = test_app().await?;
    let (owner_token, owner_id) = register_user(&app, &worker, "INVITE100020", "Owner").await?;
    let (viewer_token, viewer_id) = register_user(&app, &worker, "INVITE100021", "Viewer").await?;

    let table_code = create_table(&app, &owner_token, 1).await?;
    let request_response = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_code}/spectator-requests"),
            &viewer_token,
            json!({}),
        ))
        .await?;
    assert_eq!(request_response.status(), StatusCode::CREATED);
    let request_body = json_response(request_response).await;
    let request_id = request_body["id"]
        .as_i64()
        .expect("spectator request id should exist");
    assert_eq!(request_body["requester_user_id"], viewer_id);
    assert_eq!(request_body["owner_user_id"], owner_id);

    let owner_requests = app
        .clone()
        .oneshot(authed_json_request(
            Method::GET,
            "/api/me/spectator-requests",
            &owner_token,
            json!({}),
        ))
        .await?;
    assert_eq!(owner_requests.status(), StatusCode::OK);
    let owner_requests_body = json_response(owner_requests).await;
    assert_eq!(owner_requests_body.as_array().map(Vec::len), Some(1));
    assert_eq!(owner_requests_body[0]["id"], request_id);

    let approve_response = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/spectator-requests/{request_id}/approve"),
            &owner_token,
            json!({}),
        ))
        .await?;
    assert_eq!(approve_response.status(), StatusCode::OK);
    assert!(
        worker
            .has_approved_spectator_request(&table_code, viewer_id)
            .await?
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn spectator_player_cannot_request_to_watch_same_table() -> Result<()> {
    let (app, worker, _state) = test_app().await?;
    let (owner_token, _owner_id) = register_user(&app, &worker, "INVITE100022", "Owner").await?;
    let (_guest_token, guest_id) = register_user(&app, &worker, "INVITE100023", "Guest").await?;

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

    let request_response = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/tables/{table_code}/spectator-requests"),
            &guest_token,
            json!({}),
        ))
        .await?;
    assert_eq!(request_response.status(), StatusCode::CONFLICT);
    let request_body = json_response(request_response).await;
    assert_eq!(request_body["detail"], "player_cannot_watch_own_table");
    Ok(())
}
