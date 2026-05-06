use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::Json;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::{Notify, mpsc};

use super::auth::{AuthenticatedUser, hash_session_token};
use super::protocol::detail_response;
use super::{
    AppContext, ConnectionHandle, OUTBOUND_CHANNEL_CAPACITY, now_iso,
    register_user_connection, unregister_user_connection,
};

#[derive(Debug, Default, Deserialize)]
pub(crate) struct SocialWsQuery {
    #[serde(default)]
    session_token: String,
}

pub(crate) async fn social_websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppContext>,
    Query(query): Query<SocialWsQuery>,
) -> Response {
    let Some(user) = authenticate_session_token(&state, &query.session_token).await else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(detail_response("auth_required")),
        )
            .into_response();
    };

    ws.on_upgrade(move |socket| social_websocket_session(state, socket, user.user_id))
        .into_response()
}

pub(crate) async fn authenticate_session_token(
    state: &AppContext,
    session_token: &str,
) -> Option<AuthenticatedUser> {
    let session_token = session_token.trim();
    if session_token.is_empty() {
        return None;
    }
    state
        .inner
        .db
        .get_authenticated_user(&hash_session_token(session_token), &now_iso())
        .await
        .ok()
        .flatten()
}

async fn social_websocket_session(state: AppContext, socket: WebSocket, user_id: i64) {
    let connection_id = state.next_connection_id.fetch_add(1, Ordering::Relaxed);
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<String>(OUTBOUND_CHANNEL_CAPACITY);
    let close_requested = Arc::new(AtomicBool::new(false));
    let close_notify = Arc::new(Notify::new());
    let handle = ConnectionHandle {
        id: connection_id,
        sender: outgoing_tx,
        close_requested: close_requested.clone(),
        close_notify: close_notify.clone(),
    };

    let writer_close_requested = close_requested.clone();
    let writer_close_notify = close_notify.clone();
    let writer = tokio::spawn(async move {
        loop {
            tokio::select! {
                maybe_message = outgoing_rx.recv() => {
                    let Some(message) = maybe_message else {
                        break;
                    };
                    if ws_sender.send(Message::Text(message.into())).await.is_err() {
                        break;
                    }
                }
                _ = writer_close_notify.notified() => {
                    if writer_close_requested.load(Ordering::Relaxed) {
                        break;
                    }
                }
            }
        }
        let _ = ws_sender.close().await;
    });

    register_user_connection(&state, user_id, handle.clone()).await;

    loop {
        if handle.should_close() {
            break;
        }
        let Some(next) = ws_receiver.next().await else {
            break;
        };
        let Ok(message) = next else {
            break;
        };
        if matches!(message, Message::Close(_)) {
            break;
        }
    }

    handle.request_close();
    unregister_user_connection(&state, user_id, connection_id).await;
    let _ = writer.await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use anyhow::Result;
    use serde_json::{Value, json};
    use tokio::sync::{Notify, mpsc};

    use crate::app::auth::{generate_session_token, hash_password, hash_session_token};
    use crate::app::persistence::{DbWorker, in_memory_database};
    use crate::app::{
        AppContext, notify_user_connections, online_user_ids, register_user_connection,
        unregister_user_connection,
    };

    async fn test_state() -> Result<(AppContext, DbWorker)> {
        let db = in_memory_database("")?;
        db.initialize()?;
        let worker = DbWorker::start(db)?;
        Ok((AppContext::new(worker.clone()), worker))
    }

    async fn register_test_user(
        worker: &DbWorker,
        invite_code: &str,
        display_name: &str,
    ) -> Result<(String, i64)> {
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
        Ok((session_token, user.user_id))
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

    fn parse_online_ids(message: &str) -> Vec<i64> {
        let value: Value = serde_json::from_str(message).expect("presence message should be json");
        value["payload"]["online_user_ids"]
            .as_array()
            .expect("online user ids should be an array")
            .iter()
            .map(|item| item.as_i64().expect("user id should be i64"))
            .collect()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn authenticate_session_token_returns_user_for_valid_token() -> Result<()> {
        let (state, worker) = test_state().await?;
        let (session_token, user_id) = register_test_user(&worker, "INVITE200001", "Alice").await?;

        let authenticated = authenticate_session_token(&state, &session_token)
            .await
            .expect("session token should authenticate");

        assert_eq!(authenticated.user_id, user_id);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn user_connection_registry_broadcasts_presence_updates() -> Result<()> {
        let (state, _worker) = test_state().await?;
        let (alice_conn, mut alice_rx) = test_connection(1);
        let (bob_conn, mut bob_rx) = test_connection(2);

        register_user_connection(&state, 11, alice_conn).await;
        let first_presence = alice_rx
            .recv()
            .await
            .expect("alice should receive initial presence");
        assert_eq!(parse_online_ids(&first_presence), vec![11]);

        register_user_connection(&state, 22, bob_conn).await;
        let alice_second = alice_rx
            .recv()
            .await
            .expect("alice should receive updated presence");
        let bob_first = bob_rx
            .recv()
            .await
            .expect("bob should receive initial presence");
        assert_eq!(parse_online_ids(&alice_second), vec![11, 22]);
        assert_eq!(parse_online_ids(&bob_first), vec![11, 22]);

        unregister_user_connection(&state, 22, 2).await;
        let alice_third = alice_rx
            .recv()
            .await
            .expect("alice should receive offline update");
        assert_eq!(parse_online_ids(&alice_third), vec![11]);
        assert_eq!(online_user_ids(&state).await, vec![11]);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn notify_user_connections_targets_all_devices_for_the_same_user() -> Result<()> {
        let (state, _worker) = test_state().await?;
        let (first_conn, mut first_rx) = test_connection(1);
        let (second_conn, mut second_rx) = test_connection(2);
        let (other_conn, mut other_rx) = test_connection(3);

        register_user_connection(&state, 11, first_conn).await;
        register_user_connection(&state, 11, second_conn).await;
        register_user_connection(&state, 22, other_conn).await;

        while first_rx.try_recv().is_ok() {}
        while second_rx.try_recv().is_ok() {}
        while other_rx.try_recv().is_ok() {}

        notify_user_connections(
            &state,
            11,
            json!({
                "type": "table_invite_created",
                "payload": {
                    "table_code": "ROOM42"
                }
            }),
        )
        .await;

        let first_message = first_rx.recv().await.expect("first device should receive invite");
        let second_message = second_rx
            .recv()
            .await
            .expect("second device should receive invite");
        assert_eq!(
            serde_json::from_str::<Value>(&first_message)?["type"],
            "table_invite_created"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&second_message)?["payload"]["table_code"],
            "ROOM42"
        );
        assert!(other_rx.try_recv().is_err());
        Ok(())
    }
}
