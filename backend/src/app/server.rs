use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};
use tower_http::cors::{Any, CorsLayer};

use super::persistence::{Database, DbWorker};
use super::room_runtime::{RoomHandle, RoomRuntime, close_room_handle, restore_persisted_rooms};
use super::ws::websocket_handler;
use super::{
    AppContext, CreateTableRequest, Settings, initial_room_payload, is_valid_table_code,
    normalize_table_code, now_iso, serialize_room,
};

#[derive(Debug)]
enum CreateTableError {
    Conflict,
    Internal(anyhow::Error),
}

pub(crate) async fn run() -> Result<()> {
    let settings = Settings::from_env()?;
    let db = DbWorker::start(Database::open(&settings.database_path)?)?;
    let app_state = AppContext::new(settings.clone(), db);
    restore_persisted_rooms(&app_state).await;

    let app = Router::new()
        .route("/api/health", get(healthcheck))
        .route("/api/tables", post(create_table))
        .route("/ws/{table_code}", get(websocket_handler))
        .with_state(app_state)
        .layer(build_cors_layer(&settings.cors_origins));

    let listener = tokio::net::TcpListener::bind(&settings.bind_addr)
        .await
        .with_context(|| format!("failed to bind to {}", settings.bind_addr))?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn build_cors_layer(origins: &[String]) -> CorsLayer {
    let mut layer = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::ACCEPT]);

    let header_values: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|origin| HeaderValue::from_str(origin).ok())
        .collect();
    if header_values.is_empty() {
        layer = layer.allow_origin(Any);
    } else {
        layer = layer.allow_origin(header_values);
    }
    layer
}

async fn healthcheck() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn create_table(
    State(state): State<AppContext>,
    payload: Option<Json<CreateTableRequest>>,
) -> Response {
    let payload = payload.map(|value| value.0);
    let requested_mode = if let Some(ref body) = payload {
        if let Some(mode) = &body.mode {
            Some(mode.clone())
        } else {
            body.test_mode
                .map(|value| if value { "test" } else { "normal" }.to_string())
        }
    } else {
        None
    };
    let resolved_mode = requested_mode.unwrap_or_else(|| {
        if state.settings.default_test_mode {
            "test".to_string()
        } else {
            "normal".to_string()
        }
    });

    if resolved_mode != "normal" && resolved_mode != "test" {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "detail": "unsupported_mode" })),
        )
            .into_response();
    }

    let requested_code = match payload
        .as_ref()
        .and_then(|body| body.table_code.clone())
        .map(|value| normalize_table_code(&value))
    {
        Some(code) if !is_valid_table_code(&code) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "detail": "invalid_table_code" })),
            )
                .into_response();
        }
        value => value,
    };

    let enforce_minimum_eight_fan = payload
        .as_ref()
        .and_then(|body| body.enforce_minimum_eight_fan)
        .unwrap_or(true);

    let result = create_or_replace_table(
        &state,
        requested_code,
        &resolved_mode,
        enforce_minimum_eight_fan,
    )
    .await;

    match result {
        Ok((table_code, created_at, room)) => (
            StatusCode::CREATED,
            Json(json!({
                "table_code": table_code,
                "phase": "waiting",
                "mode": resolved_mode,
                "created_at": created_at,
                "seats": room.get("seats").cloned().unwrap_or_else(|| Value::Array(vec![])),
            })),
        )
            .into_response(),
        Err(CreateTableError::Conflict) => (
            StatusCode::CONFLICT,
            Json(json!({ "detail": "table_code_exists" })),
        )
            .into_response(),
        Err(CreateTableError::Internal(error)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "detail": error.to_string() })),
        )
            .into_response(),
    }
}

async fn create_or_replace_table(
    state: &AppContext,
    requested_code: Option<String>,
    mode: &str,
    enforce_minimum_eight_fan: bool,
) -> std::result::Result<(String, String, Value), CreateTableError> {
    let mut rooms = state.inner.rooms.write().await;
    let runtime_codes: HashSet<String> = rooms.keys().cloned().collect();
    let table_code = if let Some(code) = requested_code {
        code
    } else {
        state
            .inner
            .db
            .generate_table_code(runtime_codes)
            .await
            .map_err(CreateTableError::Internal)?
    };

    let existing_record = state
        .inner
        .db
        .get_table(&table_code)
        .await
        .map_err(CreateTableError::Internal)?;
    if let Some(record) = existing_record {
        let existing_room: Value = serde_json::from_str(&record.room_json)
            .map_err(|error| CreateTableError::Internal(error.into()))?;
        let occupied = existing_room
            .get("seats")
            .and_then(Value::as_array)
            .map(|seats| !seats.is_empty())
            .unwrap_or(false);
        if occupied {
            return Err(CreateTableError::Conflict);
        }
    }

    let replaced = rooms.remove(&table_code);
    if let Some(room_handle) = &replaced {
        room_handle.mark_closed();
    }
    drop(rooms);

    let created_at = now_iso();
    let room = initial_room_payload(&table_code, mode, enforce_minimum_eight_fan);
    let room_json = serialize_room(&room).map_err(CreateTableError::Internal)?;
    state
        .inner
        .db
        .save_table(&table_code, &created_at, &room_json)
        .await
        .map_err(CreateTableError::Internal)?;

    let room_handle = Arc::new(RoomHandle::new(RoomRuntime::new(
        created_at.clone(),
        room.clone(),
    )));
    let replaced_after_insert = {
        let mut rooms = state.inner.rooms.write().await;
        rooms.insert(table_code.clone(), room_handle)
    };
    if let Some(old_room) = replaced.or(replaced_after_insert) {
        close_room_handle(&old_room).await;
    }

    Ok((table_code, created_at, room))
}
