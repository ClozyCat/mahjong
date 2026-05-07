use anyhow::Result;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use tower::ServiceExt;

use super::persistence::{DbWorker, in_memory_database};
use super::{AppContext, Settings, server};

fn test_settings() -> Settings {
    Settings {
        bind_addr: "127.0.0.1:0".to_string(),
        database_path: ":memory:".to_string(),
        cors_origins: vec![],
        frontend_dir: None,
    }
}

async fn test_app() -> Result<(Router, DbWorker)> {
    let db = in_memory_database("")?;
    db.initialize()?;
    let worker = DbWorker::start(db)?;
    let state = AppContext::new(worker.clone());
    Ok((server::build_app(state, &test_settings()), worker))
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

#[tokio::test(flavor = "current_thread")]
async fn register_creates_user_and_session_and_me_can_update_display_name() -> Result<()> {
    let (app, worker) = test_app().await?;
    worker
        .create_invite_code("INVITE000001", "2026-05-06T00:00:00Z", None)
        .await?;

    let register_response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/auth/register",
            json!({
                "invite_code": "INVITE000001",
                "display_name": "Alice",
                "password": "secret-123",
            }),
        ))
        .await?;
    assert_eq!(register_response.status(), StatusCode::CREATED);
    let register_body = json_response(register_response).await;
    let session_token = register_body["session_token"]
        .as_str()
        .expect("register should return a session token");
    assert_eq!(register_body["user"]["display_name"], "Alice");
    assert_eq!(register_body["user"]["points"], 600);

    let me_response = app
        .clone()
        .oneshot(authed_json_request(
            Method::GET,
            "/api/me",
            session_token,
            json!({}),
        ))
        .await?;
    assert_eq!(me_response.status(), StatusCode::OK);

    let update_response = app
        .clone()
        .oneshot(authed_json_request(
            Method::PATCH,
            "/api/me",
            session_token,
            json!({
                "display_name": "Alicia",
            }),
        ))
        .await?;
    assert_eq!(update_response.status(), StatusCode::OK);
    let update_body = json_response(update_response).await;
    assert_eq!(update_body["display_name"], "Alicia");

    let logout_response = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/auth/logout",
            session_token,
            json!({}),
        ))
        .await?;
    assert_eq!(logout_response.status(), StatusCode::NO_CONTENT);

    let rejected_me = app
        .oneshot(authed_json_request(
            Method::GET,
            "/api/me",
            session_token,
            json!({}),
        ))
        .await?;
    assert_eq!(rejected_me.status(), StatusCode::UNAUTHORIZED);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn register_rejects_reused_invite_code() -> Result<()> {
    let (app, worker) = test_app().await?;
    worker
        .create_invite_code("INVITE000002", "2026-05-06T00:00:00Z", None)
        .await?;

    let first = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/auth/register",
            json!({
                "invite_code": "INVITE000002",
                "display_name": "Bob",
                "password": "secret-123",
            }),
        ))
        .await?;
    assert_eq!(first.status(), StatusCode::CREATED);

    let second = app
        .oneshot(json_request(
            Method::POST,
            "/api/auth/register",
            json!({
                "invite_code": "INVITE000002",
                "display_name": "Carol",
                "password": "secret-456",
            }),
        ))
        .await?;
    assert_eq!(second.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let second_body = json_response(second).await;
    assert_eq!(second_body["detail"], "invite_code_invalid");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn login_does_not_award_daily_points() -> Result<()> {
    let (app, worker) = test_app().await?;
    worker
        .create_invite_code("INVITE000003", "2026-05-06T00:00:00Z", None)
        .await?;

    let register_response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/auth/register",
            json!({
                "invite_code": "INVITE000003",
                "display_name": "Dora",
                "password": "secret-123",
            }),
        ))
        .await?;
    assert_eq!(register_response.status(), StatusCode::CREATED);

    let first_login = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/auth/login",
            json!({
                "identifier": "Dora",
                "password": "secret-123",
            }),
        ))
        .await?;
    assert_eq!(first_login.status(), StatusCode::OK);
    let first_body = json_response(first_login).await;
    assert_eq!(first_body["user"]["points"], 600);

    let second_login = app
        .oneshot(json_request(
            Method::POST,
            "/api/auth/login",
            json!({
                "identifier": "Dora",
                "password": "secret-123",
            }),
        ))
        .await?;
    assert_eq!(second_login.status(), StatusCode::OK);
    let second_body = json_response(second_login).await;
    assert_eq!(second_body["user"]["points"], 600);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn get_my_active_table_returns_latest_active_participant() -> Result<()> {
    let (app, worker) = test_app().await?;
    worker
        .create_invite_code("INVITE000004", "2026-05-06T00:00:00Z", None)
        .await?;

    let register_response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/auth/register",
            json!({
                "invite_code": "INVITE000004",
                "display_name": "Erin",
                "password": "secret-123",
            }),
        ))
        .await?;
    assert_eq!(register_response.status(), StatusCode::CREATED);
    let register_body = json_response(register_response).await;
    let session_token = register_body["session_token"]
        .as_str()
        .expect("register should return a session token");
    let user_id = register_body["user"]["user_id"]
        .as_i64()
        .expect("register should return a user id");

    let no_active_response = app
        .clone()
        .oneshot(authed_json_request(
            Method::GET,
            "/api/me/active-table",
            session_token,
            json!({}),
        ))
        .await?;
    assert_eq!(no_active_response.status(), StatusCode::OK);
    let no_active_body = json_response(no_active_response).await;
    assert!(no_active_body.is_null());

    let create_response = app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/tables",
            session_token,
            json!({ "table_code": "LIVE99" }),
        ))
        .await?;
    assert_eq!(create_response.status(), StatusCode::CREATED);
    let table = worker
        .get_table("LIVE99")
        .await?
        .expect("created table should be persisted");
    worker
        .save_table_and_store_reconnect_token_and_upsert_participant(
            "LIVE99",
            &table.created_at,
            &table.room_json,
            "token-live-99",
            2,
            42,
            user_id,
            "Erin",
            "2026-05-06T12:00:00Z",
        )
        .await?;

    let active_response = app
        .oneshot(authed_json_request(
            Method::GET,
            "/api/me/active-table",
            session_token,
            json!({}),
        ))
        .await?;
    assert_eq!(active_response.status(), StatusCode::OK);
    let active_body = json_response(active_response).await;
    assert_eq!(active_body["table_code"], "LIVE99");
    assert_eq!(active_body["seat_index"], 2);
    assert_eq!(active_body["role"], "player");
    Ok(())
}
