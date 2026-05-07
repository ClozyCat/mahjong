use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};

use super::auth::{
    AuthenticatedUser, bearer_token, generate_session_token, hash_password, hash_session_token,
    verify_password,
};
use super::invites::{InviteAvailability, invite_availability, invite_expires_at};
use super::persistence::{Database, DbWorker};
use super::protocol::{create_table_response, detail_response};
use super::records::{fan_stat_view, game_detail_view, game_summary_view};
use super::room_runtime::{RoomHandle, RoomRuntime, close_room_handle, restore_persisted_rooms};
use super::social_ws::social_websocket_handler;
use super::users::{PublicUserView, public_user_view, public_user_view_with_active_table};
use super::ws::websocket_handler;
use super::{
    AppContext, CreateTableRequest, Settings, initial_room_state_with_owner, is_valid_table_code,
    normalize_table_code, notify_all_user_connections, notify_user_connections, now_iso,
    parse_room_json, room_phase, serialize_room_state, user_active_table_updated_message,
};
use crate::core::state::RoomState;

#[derive(Debug)]
enum CreateTableError {
    Conflict,
    Internal(anyhow::Error),
}

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    invite_code: String,
    display_name: String,
    password: String,
    #[serde(default)]
    username: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    identifier: String,
    password: String,
}

#[derive(Debug, Deserialize, Default)]
struct UpdateMeRequest {
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    bio: Option<String>,
    #[serde(default)]
    avatar: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateMultiplierRequest {
    multiplier: i64,
}

#[derive(Debug, Deserialize)]
struct CreateTableInviteRequest {
    invitee_user_id: i64,
}

#[derive(Debug, Serialize)]
struct AuthResponse {
    session_token: String,
    user: PublicUserView,
}

#[derive(Debug, Serialize)]
struct ActiveTableResponse {
    table_code: String,
    seat_index: usize,
    role: String,
}

#[derive(Debug, Clone, Serialize)]
struct TableInviteResponse {
    id: i64,
    table_code: String,
    inviter_user_id: i64,
    invitee_user_id: i64,
    status: String,
    created_at: String,
    expires_at: String,
    accepted_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct AcceptInviteResponse {
    invite_id: i64,
    table_code: String,
    seat_index: usize,
    status: String,
}

#[derive(Debug, Clone, Serialize)]
struct SpectatorRequestResponse {
    id: i64,
    table_code: String,
    requester_user_id: i64,
    owner_user_id: i64,
    status: String,
    created_at: String,
    decided_at: Option<String>,
}

pub(crate) async fn run() -> Result<()> {
    let settings = Settings::from_env()?;
    let db = DbWorker::start(Database::open(&settings.database_path)?)?;
    let app_state = AppContext::new(db);
    restore_persisted_rooms(&app_state).await;

    let app = build_app(app_state, &settings);

    let listener = tokio::net::TcpListener::bind(&settings.bind_addr)
        .await
        .with_context(|| format!("failed to bind to {}", settings.bind_addr))?;
    axum::serve(listener, app).await?;
    Ok(())
}

pub(crate) fn build_app(app_state: AppContext, settings: &Settings) -> Router {
    let app = Router::new()
        .route("/api/health", get(healthcheck))
        .route("/api/auth/register", post(register))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/me", get(get_me).patch(update_me))
        .route("/api/me/active-table", get(get_my_active_table))
        .route("/api/games", get(list_games))
        .route("/api/games/{game_id}", get(get_game))
        .route("/api/users/{user_id}/games", get(list_user_games))
        .route("/api/users/{user_id}/fans", get(list_user_fans))
        .route("/api/leaderboard", get(get_leaderboard))
        .route("/api/me/invites", get(get_my_invites))
        .route("/api/me/spectator-requests", get(get_my_spectator_requests))
        .route("/api/tables", post(create_table))
        .route(
            "/api/tables/{table_code}/invites",
            post(create_table_invite),
        )
        .route(
            "/api/tables/{table_code}/spectator-requests",
            post(create_spectator_request),
        )
        .route(
            "/api/tables/{table_code}/multiplier",
            axum::routing::patch(update_table_multiplier),
        )
        .route("/api/invites/{invite_id}/accept", post(accept_table_invite))
        .route("/api/invites/{invite_id}/reject", post(reject_table_invite))
        .route(
            "/api/spectator-requests/{request_id}/approve",
            post(approve_spectator_request),
        )
        .route(
            "/api/spectator-requests/{request_id}/reject",
            post(reject_spectator_request),
        )
        .route("/ws/me", get(social_websocket_handler))
        .route("/ws/{table_code}", get(websocket_handler))
        .with_state(app_state)
        .layer(build_cors_layer(&settings.cors_origins));
    attach_frontend(app, settings.frontend_dir.as_deref())
}

fn attach_frontend(app: Router, frontend_dir: Option<&str>) -> Router {
    let Some(frontend_dir) = frontend_dir else {
        return app;
    };

    let frontend_dir = PathBuf::from(frontend_dir);
    if !frontend_dir.join("index.html").is_file() {
        eprintln!(
            "frontend directory {:?} does not contain index.html; skipping static file hosting",
            frontend_dir
        );
        return app;
    }

    app.fallback_service(
        ServeDir::new(&frontend_dir)
            .append_index_html_on_directories(true)
            .not_found_service(ServeFile::new(frontend_dir.join("index.html"))),
    )
}

fn build_cors_layer(origins: &[String]) -> CorsLayer {
    let mut layer = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::OPTIONS])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT]);

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

async fn register(
    State(state): State<AppContext>,
    Json(payload): Json<RegisterRequest>,
) -> Response {
    let Some(invite_code) = normalized_required(&payload.invite_code) else {
        return json_error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_registration");
    };
    let Some(display_name) = normalized_required(&payload.display_name) else {
        return json_error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_registration");
    };
    let Some(password) = normalized_required(&payload.password) else {
        return json_error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_registration");
    };
    let username = payload
        .username
        .as_deref()
        .and_then(normalized_optional)
        .unwrap_or_else(|| display_name.clone());

    let password_hash = match hash_password(&password) {
        Ok(password_hash) => password_hash,
        Err(error) => {
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
        }
    };
    let session_token = generate_session_token();
    let token_hash = hash_session_token(&session_token);
    let created_at = now_iso();

    match state
        .inner
        .db
        .register_user(
            &username,
            &display_name,
            &password_hash,
            &invite_code,
            &token_hash,
            &created_at,
        )
        .await
    {
        Ok(user) => (
            StatusCode::CREATED,
            Json(AuthResponse {
                session_token,
                user: public_user_view(&user),
            }),
        )
            .into_response(),
        Err(error) if error_matches(&error, "invite_code_invalid") => {
            json_error(StatusCode::UNPROCESSABLE_ENTITY, "invite_code_invalid")
        }
        Err(error) if error_matches(&error, "username_taken") => {
            json_error(StatusCode::CONFLICT, "username_taken")
        }
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn login(State(state): State<AppContext>, Json(payload): Json<LoginRequest>) -> Response {
    let Some(identifier) = normalized_required(&payload.identifier) else {
        return json_error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_credentials");
    };
    let Some(password) = normalized_required(&payload.password) else {
        return json_error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_credentials");
    };

    let user = match state.inner.db.find_user_by_identifier(&identifier).await {
        Ok(Some(user)) if verify_password(&password, &user.password_hash) => user,
        Ok(_) => return json_error(StatusCode::UNAUTHORIZED, "invalid_credentials"),
        Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    };

    let created_at = now_iso();
    let session_token = generate_session_token();
    let token_hash = hash_session_token(&session_token);
    if let Err(error) = state
        .inner
        .db
        .create_auth_session(&token_hash, user.user_id, &created_at)
        .await
    {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
    }
    match state.inner.db.get_user_by_id(user.user_id).await {
        Ok(Some(user)) => (
            StatusCode::OK,
            Json(AuthResponse {
                session_token,
                user: public_user_view(&user),
            }),
        )
            .into_response(),
        Ok(None) => json_error(StatusCode::UNAUTHORIZED, "auth_required"),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn logout(State(state): State<AppContext>, headers: axum::http::HeaderMap) -> Response {
    let Some(token) = bearer_token(&headers) else {
        return json_error(StatusCode::UNAUTHORIZED, "auth_required");
    };
    let token_hash = hash_session_token(&token);
    match state
        .inner
        .db
        .revoke_auth_session(&token_hash, &now_iso())
        .await
    {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => json_error(StatusCode::UNAUTHORIZED, "auth_required"),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn get_me(State(state): State<AppContext>, headers: axum::http::HeaderMap) -> Response {
    let authenticated_user = match require_authenticated_user(&state, &headers).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    match state
        .inner
        .db
        .get_user_by_id(authenticated_user.user_id)
        .await
    {
        Ok(Some(user)) => Json(public_user_view(&user)).into_response(),
        Ok(None) => json_error(StatusCode::UNAUTHORIZED, "auth_required"),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn get_my_active_table(
    State(state): State<AppContext>,
    headers: axum::http::HeaderMap,
) -> Response {
    let authenticated_user = match require_authenticated_user(&state, &headers).await {
        Ok(user) => user,
        Err(response) => return response,
    };

    match state
        .inner
        .db
        .list_active_table_participants_for_user(authenticated_user.user_id)
        .await
    {
        Ok(participants) => {
            Json(
                participants
                    .into_iter()
                    .last()
                    .map(|participant| ActiveTableResponse {
                        table_code: participant.table_code,
                        seat_index: participant.seat_index,
                        role: participant.role,
                    }),
            )
            .into_response()
        }
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn update_me(
    State(state): State<AppContext>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<UpdateMeRequest>,
) -> Response {
    let authenticated_user = match require_authenticated_user(&state, &headers).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    let Some(display_name) = normalized_patch_field(payload.display_name) else {
        return json_error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_profile_update");
    };
    let Some(bio) = normalized_patch_field(payload.bio) else {
        return json_error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_profile_update");
    };
    let avatar = payload.avatar.and_then(|value| normalized_optional(&value));

    match state
        .inner
        .db
        .update_user_profile(
            authenticated_user.user_id,
            display_name,
            bio,
            avatar,
            &now_iso(),
        )
        .await
    {
        Ok(Some(user)) => Json(public_user_view(&user)).into_response(),
        Ok(None) => json_error(StatusCode::UNAUTHORIZED, "auth_required"),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn list_games(State(state): State<AppContext>) -> Response {
    match state.inner.db.list_game_summaries(50).await {
        Ok(games) => Json(games.iter().map(game_summary_view).collect::<Vec<_>>()).into_response(),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn get_game(
    State(state): State<AppContext>,
    axum::extract::Path(game_id): axum::extract::Path<i64>,
) -> Response {
    match state.inner.db.get_game_detail(game_id).await {
        Ok(Some(game)) => match game_detail_view(&game) {
            Ok(view) => Json(view).into_response(),
            Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
        },
        Ok(None) => json_error(StatusCode::NOT_FOUND, "game_not_found"),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn list_user_games(
    State(state): State<AppContext>,
    axum::extract::Path(user_id): axum::extract::Path<i64>,
) -> Response {
    if state
        .inner
        .db
        .get_user_by_id(user_id)
        .await
        .ok()
        .flatten()
        .is_none()
    {
        return json_error(StatusCode::NOT_FOUND, "user_not_found");
    }
    match state.inner.db.list_games_for_user(user_id, 50).await {
        Ok(games) => Json(games.iter().map(game_summary_view).collect::<Vec<_>>()).into_response(),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn list_user_fans(
    State(state): State<AppContext>,
    axum::extract::Path(user_id): axum::extract::Path<i64>,
) -> Response {
    if state
        .inner
        .db
        .get_user_by_id(user_id)
        .await
        .ok()
        .flatten()
        .is_none()
    {
        return json_error(StatusCode::NOT_FOUND, "user_not_found");
    }
    match state.inner.db.list_user_fan_stats(user_id).await {
        Ok(fans) => Json(fans.iter().map(fan_stat_view).collect::<Vec<_>>()).into_response(),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn get_leaderboard(State(state): State<AppContext>) -> Response {
    match state.inner.db.list_users_by_points(100).await {
        Ok(users) => {
            let active_tables = match state.inner.db.list_active_table_participants().await {
                Ok(participants) => {
                    participants
                        .into_iter()
                        .fold(HashMap::new(), |mut by_user, participant| {
                            by_user.insert(participant.user_id, participant.table_code);
                            by_user
                        })
                }
                Err(error) => {
                    return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
                }
            };
            Json(
                users
                    .iter()
                    .map(|user| {
                        public_user_view_with_active_table(
                            user,
                            active_tables.get(&user.user_id).cloned(),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
            .into_response()
        }
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn create_table(
    State(state): State<AppContext>,
    headers: axum::http::HeaderMap,
    payload: Option<Json<CreateTableRequest>>,
) -> Response {
    let authenticated_user = match require_authenticated_user(&state, &headers).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    let payload = payload.map(|value| value.0);
    let requested_code = match payload
        .as_ref()
        .and_then(|body| body.table_code.clone())
        .map(|value| normalize_table_code(&value))
    {
        Some(code) if !is_valid_table_code(&code) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(detail_response("invalid_table_code")),
            )
                .into_response();
        }
        value => value,
    };
    let result =
        create_or_replace_table(&state, requested_code, authenticated_user.user_id, 1).await;

    match result {
        Ok((table_code, created_at, room)) => (
            StatusCode::CREATED,
            Json(create_table_response(
                &table_code,
                "normal",
                room.owner_user_id,
                room.multiplier,
                &created_at,
                room.seats,
            )),
        )
            .into_response(),
        Err(CreateTableError::Conflict) => (
            StatusCode::CONFLICT,
            Json(detail_response("table_code_exists")),
        )
            .into_response(),
        Err(CreateTableError::Internal(error)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(detail_response(&error.to_string())),
        )
            .into_response(),
    }
}

async fn create_or_replace_table(
    state: &AppContext,
    requested_code: Option<String>,
    owner_user_id: i64,
    multiplier: i64,
) -> std::result::Result<(String, String, RoomState), CreateTableError> {
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
        let existing_room =
            parse_room_json(&record.room_json).map_err(CreateTableError::Internal)?;
        let occupied = !existing_room.seats.is_empty();
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
    let room = initial_room_state_with_owner(&table_code, Some(owner_user_id), multiplier);
    let room_json = serialize_room_state(&room).map_err(CreateTableError::Internal)?;
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

async fn update_table_multiplier(
    State(state): State<AppContext>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(table_code): axum::extract::Path<String>,
    Json(payload): Json<UpdateMultiplierRequest>,
) -> Response {
    let authenticated_user = match require_authenticated_user(&state, &headers).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    if payload.multiplier != 1 {
        return json_error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_multiplier");
    }

    let table_code = normalize_table_code(&table_code);
    let Some(room_handle) = crate::app::room_runtime::ensure_room_loaded(&state, &table_code)
        .await
        .ok()
        .flatten()
    else {
        return json_error(StatusCode::NOT_FOUND, "table_not_found");
    };
    if room_handle.is_closed() {
        return json_error(StatusCode::NOT_FOUND, "table_not_found");
    }

    let _persist_guard = room_handle.persist.lock().await;
    let mut runtime = room_handle.runtime.lock().await;
    if runtime.room.owner_user_id != Some(authenticated_user.user_id) {
        return json_error(StatusCode::FORBIDDEN, "only_owner_can_change_multiplier");
    }
    if room_phase(&runtime.room) != "waiting" || runtime.room.round_state.is_some() {
        return json_error(StatusCode::CONFLICT, "table_multiplier_locked");
    }

    runtime.room.multiplier = 1;
    let room = runtime.room.clone();
    let created_at = runtime.created_at.clone();
    drop(runtime);

    let room_json = match serialize_room_state(&room) {
        Ok(room_json) => room_json,
        Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    };
    if let Err(error) = state
        .inner
        .db
        .save_table(&table_code, &created_at, &room_json)
        .await
    {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
    }

    Json(json!({
        "table_code": table_code,
        "owner_user_id": room.owner_user_id,
        "multiplier": room.multiplier,
        "phase": room.phase,
    }))
    .into_response()
}

async fn create_table_invite(
    State(state): State<AppContext>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(table_code): axum::extract::Path<String>,
    Json(payload): Json<CreateTableInviteRequest>,
) -> Response {
    let authenticated_user = match require_authenticated_user(&state, &headers).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    let table_code = normalize_table_code(&table_code);
    let Some(room_handle) = crate::app::room_runtime::ensure_room_loaded(&state, &table_code)
        .await
        .ok()
        .flatten()
    else {
        return json_error(StatusCode::NOT_FOUND, "table_not_found");
    };
    if room_handle.is_closed() {
        return json_error(StatusCode::NOT_FOUND, "table_not_found");
    }

    if state
        .inner
        .db
        .get_user_by_id(payload.invitee_user_id)
        .await
        .ok()
        .flatten()
        .is_none()
    {
        return json_error(StatusCode::NOT_FOUND, "user_not_found");
    }

    let _persist_guard = room_handle.persist.lock().await;
    let runtime = room_handle.runtime.lock().await;
    if runtime.room.owner_user_id != Some(authenticated_user.user_id) {
        return json_error(StatusCode::FORBIDDEN, "only_owner_can_invite");
    }
    if inviteable_seat_index(&runtime.room).is_none() {
        return json_error(StatusCode::CONFLICT, "table_full");
    }
    drop(runtime);

    match invite_availability(&state, payload.invitee_user_id, &table_code).await {
        Ok(InviteAvailability::TargetAlreadyInTable) => {
            return json_error(StatusCode::CONFLICT, "target_already_in_table");
        }
        Ok(InviteAvailability::TargetPlayerBusy) => {
            return json_error(StatusCode::CONFLICT, "target_player_busy");
        }
        Ok(InviteAvailability::Available) => {}
        Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }

    let created_at = now_iso();
    let expires_at = invite_expires_at();
    match state
        .inner
        .db
        .create_table_invite(
            &table_code,
            authenticated_user.user_id,
            payload.invitee_user_id,
            &created_at,
            &expires_at,
        )
        .await
    {
        Ok(invite) => {
            let invite_response = table_invite_response(invite.clone());
            notify_user_connections(
                &state,
                invite.invitee_user_id,
                json!({
                    "type": "table_invite_created",
                    "payload": invite_response.clone(),
                }),
            )
            .await;
            (StatusCode::CREATED, Json(invite_response)).into_response()
        }
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn get_my_invites(
    State(state): State<AppContext>,
    headers: axum::http::HeaderMap,
) -> Response {
    let authenticated_user = match require_authenticated_user(&state, &headers).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    match state
        .inner
        .db
        .list_available_table_invites_for_user(authenticated_user.user_id, &now_iso())
        .await
    {
        Ok(invites) => {
            let mut available_invites = Vec::new();
            for invite in invites {
                match state.inner.db.get_table(&invite.table_code).await {
                    Ok(Some(_)) => {}
                    Ok(None) => continue,
                    Err(error) => {
                        return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
                    }
                }

                match crate::app::room_runtime::ensure_room_loaded(&state, &invite.table_code).await
                {
                    Ok(Some(room_handle)) if !room_handle.is_closed() => {
                        available_invites.push(invite);
                    }
                    Ok(_) => {}
                    Err(error) => {
                        return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
                    }
                }
            }

            Json(
                available_invites
                    .into_iter()
                    .map(table_invite_response)
                    .collect::<Vec<_>>(),
            )
            .into_response()
        }
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn create_spectator_request(
    State(state): State<AppContext>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(table_code): axum::extract::Path<String>,
) -> Response {
    let authenticated_user = match require_authenticated_user(&state, &headers).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    let table_code = normalize_table_code(&table_code);
    let Some(room_handle) = crate::app::room_runtime::ensure_room_loaded(&state, &table_code)
        .await
        .ok()
        .flatten()
    else {
        return json_error(StatusCode::NOT_FOUND, "table_not_found");
    };
    if room_handle.is_closed() {
        return json_error(StatusCode::NOT_FOUND, "table_not_found");
    }

    let runtime = room_handle.runtime.lock().await;
    if runtime.room.owner_user_id == Some(authenticated_user.user_id) {
        return json_error(StatusCode::CONFLICT, "player_cannot_watch_own_table");
    }
    drop(runtime);

    match state
        .inner
        .db
        .get_active_table_participant(&table_code, authenticated_user.user_id)
        .await
    {
        Ok(Some(_)) => return json_error(StatusCode::CONFLICT, "player_cannot_watch_own_table"),
        Ok(None) => {}
        Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }

    let owner_user_id = match state.inner.db.get_table(&table_code).await {
        Ok(Some(record)) => match parse_room_json(&record.room_json) {
            Ok(room) => match room.owner_user_id {
                Some(owner_user_id) => owner_user_id,
                None => {
                    return json_error(StatusCode::CONFLICT, "spectator_requires_owner_approval");
                }
            },
            Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
        },
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "table_not_found"),
        Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    };

    match state
        .inner
        .db
        .create_spectator_request(
            &table_code,
            authenticated_user.user_id,
            owner_user_id,
            &now_iso(),
        )
        .await
    {
        Ok(request) => {
            let payload = spectator_request_response(request.clone());
            notify_user_connections(
                &state,
                owner_user_id,
                json!({
                    "type": "spectator_request_created",
                    "payload": payload.clone(),
                }),
            )
            .await;
            (StatusCode::CREATED, Json(payload)).into_response()
        }
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn get_my_spectator_requests(
    State(state): State<AppContext>,
    headers: axum::http::HeaderMap,
) -> Response {
    let authenticated_user = match require_authenticated_user(&state, &headers).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    match state
        .inner
        .db
        .list_pending_spectator_requests_for_owner(authenticated_user.user_id)
        .await
    {
        Ok(requests) => Json(
            requests
                .into_iter()
                .map(spectator_request_response)
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn accept_table_invite(
    State(state): State<AppContext>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(invite_id): axum::extract::Path<i64>,
) -> Response {
    let authenticated_user = match require_authenticated_user(&state, &headers).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    let invite = match state.inner.db.get_table_invite(invite_id).await {
        Ok(Some(invite)) => invite,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "table_invite_invalid"),
        Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    };
    if invite.invitee_user_id != authenticated_user.user_id {
        return json_error(StatusCode::FORBIDDEN, "table_invite_invalid");
    }

    match invite_availability(&state, authenticated_user.user_id, &invite.table_code).await {
        Ok(InviteAvailability::TargetAlreadyInTable) => {
            return json_error(StatusCode::CONFLICT, "target_already_in_table");
        }
        Ok(InviteAvailability::TargetPlayerBusy) => {
            return json_error(StatusCode::CONFLICT, "target_player_busy");
        }
        Ok(InviteAvailability::Available) => {}
        Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }

    let user = match state
        .inner
        .db
        .get_user_by_id(authenticated_user.user_id)
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) => return json_error(StatusCode::UNAUTHORIZED, "auth_required"),
        Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    };
    let Some(room_handle) =
        crate::app::room_runtime::ensure_room_loaded(&state, &invite.table_code)
            .await
            .ok()
            .flatten()
    else {
        return json_error(StatusCode::NOT_FOUND, "table_not_found");
    };
    if room_handle.is_closed() {
        return json_error(StatusCode::NOT_FOUND, "table_not_found");
    }

    let _persist_guard = room_handle.persist.lock().await;
    let mut runtime = room_handle.runtime.lock().await;
    let Some((seat_index, replaces_existing_seat)) = inviteable_seat_index(&runtime.room) else {
        return json_error(StatusCode::CONFLICT, "table_full");
    };

    let player_session_id = crate::app::generate_player_session_id();
    let reconnect_token = crate::app::generate_reconnect_token();
    if replaces_existing_seat {
        if let Some(seat) = runtime
            .room
            .seats
            .iter_mut()
            .find(|seat| seat.seat_index == seat_index)
        {
            seat.nickname = Some(user.display_name.clone());
            seat.reconnect_token = Some(reconnect_token.clone());
            seat.player_session_id = Some(player_session_id);
            seat.connected = false;
            seat.ready = false;
            seat.is_bot = false;
            seat.seat_type = "human".to_string();
            seat.bot_persona = None;
            seat.bot_aggression = None;
            seat.disconnect_deadline_at = None;
        }
    } else {
        runtime.room.seats.push(crate::core::state::SeatState {
            seat_index,
            nickname: Some(user.display_name.clone()),
            reconnect_token: Some(reconnect_token.clone()),
            player_session_id: Some(player_session_id),
            connected: false,
            ready: false,
            is_bot: false,
            seat_type: "human".to_string(),
            bot_persona: None,
            bot_aggression: None,
            disconnect_deadline_at: None,
        });
        runtime.room.seats.sort_by_key(|seat| seat.seat_index);
    }
    let room = runtime.room.clone();
    let created_at = runtime.created_at.clone();
    drop(runtime);

    let room_json = match serialize_room_state(&room) {
        Ok(room_json) => room_json,
        Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    };
    match state
        .inner
        .db
        .accept_table_invite_and_reserve_seat(
            invite_id,
            authenticated_user.user_id,
            &now_iso(),
            &invite.table_code,
            &room_json,
            &created_at,
            &reconnect_token,
            seat_index,
            player_session_id,
            &user.display_name,
        )
        .await
    {
        Ok(invite) => {
            notify_all_user_connections(
                &state,
                user_active_table_updated_message(
                    authenticated_user.user_id,
                    Some(&invite.table_code),
                ),
            )
            .await;
            (
                StatusCode::OK,
                Json(AcceptInviteResponse {
                    invite_id: invite.id,
                    table_code: invite.table_code,
                    seat_index,
                    status: invite.status,
                }),
            )
                .into_response()
        }
        Err(error) if error_matches(&error, "table_invite_invalid") => {
            json_error(StatusCode::UNPROCESSABLE_ENTITY, "table_invite_invalid")
        }
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn approve_spectator_request(
    State(state): State<AppContext>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(request_id): axum::extract::Path<i64>,
) -> Response {
    decide_spectator_request(state, headers, request_id, true).await
}

async fn reject_spectator_request(
    State(state): State<AppContext>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(request_id): axum::extract::Path<i64>,
) -> Response {
    decide_spectator_request(state, headers, request_id, false).await
}

async fn reject_table_invite(
    State(state): State<AppContext>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(invite_id): axum::extract::Path<i64>,
) -> Response {
    let authenticated_user = match require_authenticated_user(&state, &headers).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    match state
        .inner
        .db
        .reject_table_invite(invite_id, authenticated_user.user_id, &now_iso())
        .await
    {
        Ok(Some(invite)) => {
            let payload = table_invite_response(invite.clone());
            notify_user_connections(
                &state,
                invite.inviter_user_id,
                json!({
                    "type": "table_invite_decided",
                    "payload": payload.clone(),
                }),
            )
            .await;
            Json(payload).into_response()
        }
        Ok(None) => json_error(StatusCode::NOT_FOUND, "table_invite_invalid"),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn decide_spectator_request(
    state: AppContext,
    headers: axum::http::HeaderMap,
    request_id: i64,
    approved: bool,
) -> Response {
    let authenticated_user = match require_authenticated_user(&state, &headers).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    match state
        .inner
        .db
        .decide_spectator_request(request_id, authenticated_user.user_id, approved, &now_iso())
        .await
    {
        Ok(Some(request)) => {
            let payload = spectator_request_response(request.clone());
            notify_user_connections(
                &state,
                request.requester_user_id,
                json!({
                    "type": "spectator_request_decided",
                    "payload": payload.clone(),
                }),
            )
            .await;
            Json(payload).into_response()
        }
        Ok(None) => json_error(StatusCode::NOT_FOUND, "spectator_request_not_found"),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn require_authenticated_user(
    state: &AppContext,
    headers: &axum::http::HeaderMap,
) -> std::result::Result<AuthenticatedUser, Response> {
    let Some(token) = bearer_token(headers) else {
        return Err(json_error(StatusCode::UNAUTHORIZED, "auth_required"));
    };
    let token_hash = hash_session_token(&token);
    match state
        .inner
        .db
        .get_authenticated_user(&token_hash, &now_iso())
        .await
    {
        Ok(Some(user)) => Ok(user),
        Ok(None) => Err(json_error(StatusCode::UNAUTHORIZED, "auth_required")),
        Err(error) => Err(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &error.to_string(),
        )),
    }
}

fn normalized_required(value: &str) -> Option<String> {
    normalized_optional(value)
}

fn normalized_optional(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalized_patch_field(value: Option<String>) -> Option<Option<String>> {
    match value {
        Some(value) => normalized_optional(&value).map(Some),
        None => Some(None),
    }
}

fn json_error(status: StatusCode, detail: &str) -> Response {
    (status, Json(detail_response(detail))).into_response()
}

fn error_matches(error: &anyhow::Error, expected: &str) -> bool {
    format!("{error:#}").contains(expected)
}

fn inviteable_seat_index(room: &crate::core::state::RoomState) -> Option<(usize, bool)> {
    if let Some(seat_index) = crate::app::random_bot_seat_index(room) {
        return Some((seat_index, true));
    }

    if room_phase(room) == "waiting" && room.round_state.is_none() {
        return crate::app::random_open_seat_index(room).map(|seat_index| (seat_index, false));
    }

    None
}

fn table_invite_response(invite: super::persistence::TableInviteRecord) -> TableInviteResponse {
    TableInviteResponse {
        id: invite.id,
        table_code: invite.table_code,
        inviter_user_id: invite.inviter_user_id,
        invitee_user_id: invite.invitee_user_id,
        status: invite.status,
        created_at: invite.created_at,
        expires_at: invite.expires_at,
        accepted_at: invite.accepted_at,
    }
}

fn spectator_request_response(
    request: super::persistence::SpectatorRequestRecord,
) -> SpectatorRequestResponse {
    SpectatorRequestResponse {
        id: request.id,
        table_code: request.table_code,
        requester_user_id: request.requester_user_id,
        owner_user_id: request.owner_user_id,
        status: request.status,
        created_at: request.created_at,
        decided_at: request.decided_at,
    }
}
