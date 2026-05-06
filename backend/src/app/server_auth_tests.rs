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

    let me_response = app
        .clone()
        .oneshot(authed_json_request(Method::GET, "/api/me", session_token, json!({})))
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
        .oneshot(authed_json_request(Method::GET, "/api/me", session_token, json!({})))
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
async fn login_daily_points_awards_only_first_login_per_beijing_date() -> Result<()> {
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
    assert_eq!(first_body["user"]["points"], 50);

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
    assert_eq!(second_body["user"]["points"], 50);
    Ok(())
}
