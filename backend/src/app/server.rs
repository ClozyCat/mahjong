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
use super::persistence::{Database, DbWorker, UserRecord};
use super::protocol::{create_table_response, detail_response};
use super::records::{game_detail_view, game_summary_view};
use super::room_runtime::{
    RoomHandle, RoomRuntime, close_room_handle, ensure_room_loaded, restore_persisted_rooms,
    restore_room_snapshot, snapshot_connections,
};
use super::scheduler::schedule_room_tasks_detached;
use super::social_ws::social_websocket_handler;
use super::users::{
    PublicUserView, public_user_view, public_user_view_with_active_table, title_for_points,
};
use super::ws::websocket_handler;
use super::{
    AppContext, CreateTableRequest, Settings, collect_snapshot_and_prompt_outbound_from_snapshot,
    initial_room_state_with_owner, is_valid_table_code, normalize_table_code,
    notify_all_user_connections, notify_user_connections, now_iso, parse_room_json, room_phase,
    send_outbound, serialize_room_state, user_active_table_updated_message,
};
use crate::core::state::{RoomState, SeatState};
use crate::rules::standard::flow::start_match_in_room_state;
use crate::special_bots::{self, SPECIAL_BOT_SEAT_TYPE};

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

pub(crate) async fn run() -> Result<()> {
    let settings = Settings::from_env()?;
    let db = DbWorker::start(Database::open(&settings.database_path)?)?;
    let app_state = AppContext::new(db);
    seed_dev_user(&app_state, &settings).await?;
    seed_special_bot_users(&app_state).await?;
    restore_persisted_rooms(&app_state).await;

    let app = build_app(app_state, &settings);

    let listener = tokio::net::TcpListener::bind(&settings.bind_addr)
        .await
        .with_context(|| format!("failed to bind to {}", settings.bind_addr))?;
    axum::serve(listener, app).await?;
    Ok(())
}

pub(crate) async fn seed_dev_user(app_state: &AppContext, settings: &Settings) -> Result<()> {
    let Some(seed_user) = settings.dev_seed_user.as_ref() else {
        return Ok(());
    };
    let password_hash = hash_password(&seed_user.password)?;
    app_state
        .inner
        .db
        .upsert_dev_user(
            &seed_user.username,
            &seed_user.display_name,
            &password_hash,
            &now_iso(),
        )
        .await?;
    eprintln!(
        "dev login account ready: username={} password={}",
        seed_user.username, seed_user.password
    );
    Ok(())
}

pub(crate) async fn seed_special_bot_users(app_state: &AppContext) -> Result<()> {
    let mut user_ids = HashSet::new();
    for bot in special_bots::definitions() {
        let password_hash = hash_password(&special_bot_password(bot.username))?;
        let user = app_state
            .inner
            .db
            .upsert_special_bot_user(bot.username, bot.display_name, &password_hash, &now_iso())
            .await?;
        user_ids.insert(user.user_id);
    }
    *app_state.inner.special_bot_user_ids.write().await = user_ids;
    Ok(())
}

fn special_bot_password(username: &str) -> String {
    format!(
        "special-bot-login-disabled::{username}::{}::{}",
        generate_session_token(),
        generate_session_token()
    )
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
        .route("/api/leaderboard", get(get_leaderboard))
        .route("/api/me/invites", get(get_my_invites))
        .route("/api/tables", post(create_table))
        .route("/api/evaluations", post(create_evaluation))
        .route("/api/evaluations/{evaluation_id}", get(get_evaluation))
        .route(
            "/api/tables/{table_code}/invites",
            post(create_table_invite),
        )
        .route(
            "/api/tables/{table_code}/multiplier",
            axum::routing::patch(update_table_multiplier),
        )
        .route("/api/invites/{invite_id}/accept", post(accept_table_invite))
        .route("/api/invites/{invite_id}/reject", post(reject_table_invite))
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
        Ok(Some(user))
            if !special_bots::is_special_bot_username(&user.username)
                && verify_password(&password, &user.password_hash) =>
        {
            user
        }
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
    if state
        .inner
        .special_bot_user_ids
        .read()
        .await
        .contains(&authenticated_user.user_id)
    {
        return json_error(StatusCode::FORBIDDEN, "special_bot_profile_locked");
    }
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
            let mut active_table_phases = HashMap::new();
            for table_code in active_tables.values() {
                if active_table_phases.contains_key(table_code) {
                    continue;
                }
                let phase = match state.inner.db.get_table(table_code).await {
                    Ok(Some(record)) => match parse_room_json(&record.room_json) {
                        Ok(room) => Some(room.phase),
                        Err(error) => {
                            return json_error(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                &error.to_string(),
                            );
                        }
                    },
                    Ok(None) => None,
                    Err(error) => {
                        return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
                    }
                };
                active_table_phases.insert(table_code.clone(), phase);
            }
            let special_bot_user_ids = state.inner.special_bot_user_ids.read().await.clone();
            let mut views = Vec::new();
            for user in &users {
                let active_table_code = active_tables.get(&user.user_id).cloned();
                let active_table_phase = active_table_code
                    .as_ref()
                    .and_then(|table_code| active_table_phases.get(table_code).cloned().flatten());
                views.push(public_user_view_with_active_table(
                    user,
                    active_table_code,
                    active_table_phase,
                    special_bot_user_ids.contains(&user.user_id),
                ));
            }
            Json(views).into_response()
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

async fn create_evaluation(
    State(state): State<AppContext>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<super::evaluation::CreateEvaluationRequest>,
) -> Response {
    let authenticated_user = match require_authenticated_user(&state, &headers).await {
        Ok(user) => user,
        Err(response) => return response,
    };

    let mut subject_user_ids = vec![authenticated_user.user_id];
    for user_id in payload.subject_user_ids {
        if !subject_user_ids.contains(&user_id) {
            subject_user_ids.push(user_id);
        }
    }
    if subject_user_ids.len() > 4 {
        return json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "too_many_evaluation_subjects",
        );
    }

    let mut subjects = Vec::new();
    for user_id in subject_user_ids {
        let user = match state.inner.db.get_user_by_id(user_id).await {
            Ok(Some(user)) => user,
            Ok(None) => return json_error(StatusCode::NOT_FOUND, "evaluation_subject_not_found"),
            Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
        };
        subjects.push(user);
    }

    let evaluation_id = super::evaluation::new_evaluation_id();
    let table_prefix = evaluation_table_prefix(&evaluation_id);
    let seed = rand::random::<u64>();
    let special_bot_user_ids = state.inner.special_bot_user_ids.read().await.clone();
    let mut response = super::evaluation::EvaluationSessionResponse {
        evaluation_id: evaluation_id.clone(),
        seed,
        subjects: Vec::new(),
    };

    for (index, subject) in subjects.iter().enumerate() {
        let table_code = super::evaluation::evaluation_table_code(&table_prefix, index);
        let subject_is_bot = special_bot_user_ids.contains(&subject.user_id);
        let room = super::evaluation::build_evaluation_room(
            &table_code,
            authenticated_user.user_id,
            Some(subject.user_id),
            &subject.display_name,
            subject_is_bot,
        );
        let mut room = room;
        if subject_is_bot && let Err(error) = start_match_in_room_state(&mut room, 0, seed) {
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error);
        }
        let created_at = now_iso();
        let room_json = match serialize_room_state(&room) {
            Ok(room_json) => room_json,
            Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
        };

        let save_result = if subject_is_bot {
            state
                .inner
                .db
                .save_table(&table_code, &created_at, &room_json)
                .await
        } else {
            state
                .inner
                .db
                .save_table_and_upsert_participant(
                    &table_code,
                    &created_at,
                    &room_json,
                    0,
                    subject.user_id,
                    &subject.display_name,
                    &created_at,
                )
                .await
        };
        if let Err(error) = save_result {
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
        }

        let room_handle = Arc::new(RoomHandle::new(RoomRuntime::new(created_at, room.clone())));
        let old_room = state
            .inner
            .rooms
            .write()
            .await
            .insert(table_code.clone(), room_handle);
        if let Some(old_room) = old_room {
            close_room_handle(&old_room).await;
        }
        schedule_room_tasks_detached(state.clone(), table_code.clone());

        response
            .subjects
            .push(super::evaluation::EvaluationSubjectResponse {
                subject_id: format!("user:{}", subject.user_id),
                user_id: Some(subject.user_id),
                display_name: subject.display_name.clone(),
                kind: if subject_is_bot { "bot" } else { "human" }.to_string(),
                table_code,
                phase: room.phase,
                completed: false,
                final_score: None,
                deal_in_count: None,
                win_count: None,
                completed_round_count: None,
                ready_hand_win_count: None,
            });
    }

    state
        .inner
        .evaluation_sessions
        .write()
        .await
        .insert(evaluation_id, response.clone());
    (StatusCode::CREATED, Json(response)).into_response()
}

async fn get_evaluation(
    State(state): State<AppContext>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(evaluation_id): axum::extract::Path<String>,
) -> Response {
    if let Err(response) = require_authenticated_user(&state, &headers).await {
        return response;
    }
    let Some(mut response) = state
        .inner
        .evaluation_sessions
        .read()
        .await
        .get(&evaluation_id)
        .cloned()
    else {
        return json_error(StatusCode::NOT_FOUND, "evaluation_not_found");
    };

    for subject in &mut response.subjects {
        if let Ok(Some(room_handle)) = ensure_room_loaded(&state, &subject.table_code).await {
            let runtime = room_handle.runtime.lock().await;
            super::evaluation::apply_room_result_to_evaluation_subject(subject, &runtime.room);
        }
    }

    Json(response).into_response()
}

fn evaluation_table_prefix(evaluation_id: &str) -> String {
    let suffix = evaluation_id
        .rsplit_once('-')
        .map(|(_, value)| value)
        .unwrap_or(evaluation_id);
    let mut prefix = format!("EV{}", suffix.to_ascii_uppercase());
    prefix.truncate(10);
    prefix
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

    let invitee = match state.inner.db.get_user_by_id(payload.invitee_user_id).await {
        Ok(Some(user)) => user,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "user_not_found"),
        Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    };

    let _persist_guard = room_handle.persist.lock().await;
    let runtime = room_handle.runtime.lock().await;
    if runtime.room.owner_user_id != Some(authenticated_user.user_id) {
        return json_error(StatusCode::FORBIDDEN, "only_owner_can_invite");
    }
    if inviteable_seat_index(&runtime.room).is_none() {
        return json_error(StatusCode::CONFLICT, "table_full");
    }
    drop(runtime);
    drop(_persist_guard);

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

    if state
        .inner
        .special_bot_user_ids
        .read()
        .await
        .contains(&invitee.user_id)
    {
        return auto_accept_special_bot_invite(
            state,
            room_handle,
            table_code,
            authenticated_user.user_id,
            invitee,
        )
        .await;
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

async fn auto_accept_special_bot_invite(
    state: AppContext,
    room_handle: Arc<RoomHandle>,
    table_code: String,
    inviter_user_id: i64,
    bot_user: UserRecord,
) -> Response {
    let created_at = now_iso();
    let expires_at = invite_expires_at();
    let invite = match state
        .inner
        .db
        .create_table_invite(
            &table_code,
            inviter_user_id,
            bot_user.user_id,
            &created_at,
            &expires_at,
        )
        .await
    {
        Ok(invite) => invite,
        Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    };
    let accepted_at = now_iso();

    let _persist_guard = room_handle.persist.lock().await;
    let mut runtime = room_handle.runtime.lock().await;
    if room_handle.is_closed() {
        drop(runtime);
        let _ = state
            .inner
            .db
            .reject_table_invite(invite.id, bot_user.user_id, &accepted_at)
            .await;
        return json_error(StatusCode::NOT_FOUND, "table_not_found");
    }
    let previous_room = runtime.room.clone();
    let Some((seat_index, replaces_existing_seat)) = inviteable_seat_index(&runtime.room) else {
        drop(runtime);
        let _ = state
            .inner
            .db
            .reject_table_invite(invite.id, bot_user.user_id, &accepted_at)
            .await;
        return json_error(StatusCode::CONFLICT, "table_full");
    };
    upsert_special_bot_seat(
        &mut runtime.room,
        seat_index,
        replaces_existing_seat,
        &bot_user,
    );
    let room = runtime.room.clone();
    let room_created_at = runtime.created_at.clone();
    let connections = snapshot_connections(&runtime);
    drop(runtime);

    let room_json = match serialize_room_state(&room) {
        Ok(room_json) => room_json,
        Err(error) => {
            restore_room_snapshot(&room_handle, previous_room).await;
            let _ = state
                .inner
                .db
                .reject_table_invite(invite.id, bot_user.user_id, &accepted_at)
                .await;
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
        }
    };
    let invite = match state
        .inner
        .db
        .accept_table_invite_and_reserve_seat(
            invite.id,
            bot_user.user_id,
            &accepted_at,
            &table_code,
            &room_json,
            &room_created_at,
            seat_index,
            &bot_user.display_name,
            true,
        )
        .await
    {
        Ok(result) => result.accepted,
        Err(error) if error_matches(&error, "table_invite_invalid") => {
            restore_room_snapshot(&room_handle, previous_room).await;
            return json_error(StatusCode::UNPROCESSABLE_ENTITY, "table_invite_invalid");
        }
        Err(error) if error_matches(&error, "target_player_busy") => {
            restore_room_snapshot(&room_handle, previous_room).await;
            let _ = state
                .inner
                .db
                .reject_table_invite(invite.id, bot_user.user_id, &accepted_at)
                .await;
            return json_error(StatusCode::CONFLICT, "target_player_busy");
        }
        Err(error) => {
            restore_room_snapshot(&room_handle, previous_room).await;
            let _ = state
                .inner
                .db
                .reject_table_invite(invite.id, bot_user.user_id, &accepted_at)
                .await;
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
        }
    };

    notify_all_user_connections(
        &state,
        user_active_table_updated_message(bot_user.user_id, Some(&table_code), Some(&room.phase)),
    )
    .await;

    let outbound = collect_snapshot_and_prompt_outbound_from_snapshot(&room, &connections);
    send_outbound(outbound);
    schedule_room_tasks_detached(state, table_code);

    (StatusCode::CREATED, Json(table_invite_response(invite))).into_response()
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

    if replaces_existing_seat {
        if let Some(seat) = runtime
            .room
            .seats
            .iter_mut()
            .find(|seat| seat.seat_index == seat_index)
        {
            seat.user_id = Some(user.user_id);
            seat.nickname = Some(user.display_name.clone());
            seat.points = Some(user.points);
            seat.title = Some(title_for_points(user.points).to_string());
            seat.connected = false;
            seat.is_bot = false;
            seat.seat_type = "human".to_string();
            seat.bot_persona = None;
            seat.bot_aggression = None;
            seat.disconnect_deadline_at = None;
            seat.consecutive_timeout_auto_response_count = 0;
        }
    } else {
        runtime.room.seats.push(crate::core::state::SeatState {
            seat_index,
            user_id: Some(user.user_id),
            nickname: Some(user.display_name.clone()),
            points: Some(user.points),
            title: Some(title_for_points(user.points).to_string()),
            connected: false,
            is_bot: false,
            seat_type: "human".to_string(),
            bot_persona: None,
            bot_aggression: None,
            disconnect_deadline_at: None,
            consecutive_timeout_auto_response_count: 0,
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
            seat_index,
            &user.display_name,
            false,
        )
        .await
    {
        Ok(result) => {
            let invite = result.accepted;
            let accepted_payload = table_invite_response(invite.clone());
            notify_user_connections(
                &state,
                invite.inviter_user_id,
                json!({
                    "type": "table_invite_decided",
                    "payload": accepted_payload.clone(),
                }),
            )
            .await;
            notify_user_connections(
                &state,
                invite.invitee_user_id,
                json!({
                    "type": "table_invite_decided",
                    "payload": accepted_payload,
                }),
            )
            .await;
            for rejected_invite in result.rejected {
                let payload = table_invite_response(rejected_invite.clone());
                notify_user_connections(
                    &state,
                    rejected_invite.inviter_user_id,
                    json!({
                        "type": "table_invite_decided",
                        "payload": payload.clone(),
                    }),
                )
                .await;
                notify_user_connections(
                    &state,
                    rejected_invite.invitee_user_id,
                    json!({
                        "type": "table_invite_decided",
                        "payload": payload,
                    }),
                )
                .await;
            }
            notify_all_user_connections(
                &state,
                user_active_table_updated_message(
                    authenticated_user.user_id,
                    Some(&invite.table_code),
                    Some(&room.phase),
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

fn upsert_special_bot_seat(
    room: &mut RoomState,
    seat_index: usize,
    replaces_existing_seat: bool,
    user: &UserRecord,
) {
    let seat = SeatState {
        seat_index,
        user_id: Some(user.user_id),
        nickname: Some(user.display_name.clone()),
        points: Some(user.points),
        title: Some(title_for_points(user.points).to_string()),
        connected: true,
        is_bot: true,
        seat_type: SPECIAL_BOT_SEAT_TYPE.to_string(),
        bot_persona: Some(user.username.clone()),
        bot_aggression: None,
        disconnect_deadline_at: None,
        consecutive_timeout_auto_response_count: 0,
    };

    if replaces_existing_seat
        && let Some(existing) = room
            .seats
            .iter_mut()
            .find(|existing| existing.seat_index == seat_index)
    {
        *existing = seat;
        return;
    }

    room.seats.push(seat);
    room.seats.sort_by_key(|seat| seat.seat_index);
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
