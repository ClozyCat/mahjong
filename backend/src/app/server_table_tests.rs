use anyhow::Result;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use tower::ServiceExt;

use super::persistence::{DbWorker, in_memory_database};
use super::room_runtime::room_handle;
use super::{AppContext, Settings, parse_room_json, server};

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
    Ok((server::build_app(state.clone(), &test_settings()), worker, state))
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

#[tokio::test(flavor = "current_thread")]
async fn multiplier_create_table_stores_owner_and_multiplier() -> Result<()> {
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
    assert_eq!(room.multiplier, 3);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn multiplier_owner_can_change_while_waiting() -> Result<()> {
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
            json!({ "multiplier": 2 }),
        ))
        .await?;
    assert_eq!(update.status(), StatusCode::OK);

    let table = worker
        .get_table(table_code)
        .await?
        .expect("table should be persisted");
    let room = parse_room_json(&table.room_json)?;
    assert_eq!(room.multiplier, 2);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn multiplier_owner_cannot_change_after_start() -> Result<()> {
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
    assert_eq!(update.status(), StatusCode::CONFLICT);
    let body = json_response(update).await;
    assert_eq!(body["detail"], "table_multiplier_locked");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn multiplier_non_owner_cannot_change_table_setting() -> Result<()> {
    let (app, worker, _state) = test_app().await?;
    let (owner_token, _owner_user_id) = register_user(&app, &worker, "INVITE100004", "Owner").await?;
    let (guest_token, _guest_user_id) = register_user(&app, &worker, "INVITE100005", "Guest").await?;

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
            json!({ "multiplier": 3 }),
        ))
        .await?;
    assert_eq!(update.status(), StatusCode::FORBIDDEN);
    let body = json_response(update).await;
    assert_eq!(body["detail"], "only_owner_can_change_multiplier");
    Ok(())
}
