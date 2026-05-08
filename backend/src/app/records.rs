use std::collections::HashMap;

use anyhow::Result;
use serde::Serialize;
use serde_json::{Value, json};

use super::persistence::{
    ArchiveRoundInput, ArchiveRoundOutcome, ArchivedFanStatInput, ArchivedRoundPlayerInput,
    GameRecordDetail, GameSummaryRecord, RoundPlayerResultRecord, TableParticipantRecord,
    UserFanStatRecord, UserGamePlayerSummaryRecord,
};
use super::users::{display_label, title_for_points};
use super::{AppContext, notify_user_connections};
use crate::core::state::{RoomState, RoundSettlement};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct UserBriefView {
    pub(crate) user_id: i64,
    pub(crate) display_name: String,
    pub(crate) points: i64,
    pub(crate) title: String,
    pub(crate) display_label: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct GameSummaryView {
    pub(crate) game_id: i64,
    pub(crate) table_code: String,
    pub(crate) owner: UserBriefView,
    pub(crate) multiplier: i64,
    pub(crate) started_at: String,
    pub(crate) ended_at: Option<String>,
    pub(crate) round_count: i64,
    pub(crate) opponent_names: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) player_summary: Option<UserGamePlayerSummaryView>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct UserGamePlayerSummaryView {
    pub(crate) round_count: i64,
    pub(crate) win_count: i64,
    pub(crate) self_draw_win_count: i64,
    pub(crate) discard_win_count: i64,
    pub(crate) deal_in_count: i64,
    pub(crate) total_score_delta: i64,
    pub(crate) average_cumulative_score: i64,
    pub(crate) high_score_round_count: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct RoundPlayerResultView {
    pub(crate) user_id: i64,
    pub(crate) seat_index: usize,
    pub(crate) score_delta: i64,
    pub(crate) point_delta: i64,
    pub(crate) cumulative_score: i64,
    pub(crate) is_winner: bool,
    pub(crate) win_type: Option<String>,
    pub(crate) nickname_snapshot: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct RoundRecordView {
    pub(crate) round_record_id: i64,
    pub(crate) round_id: String,
    pub(crate) ended_at: String,
    pub(crate) settlement: Value,
    pub(crate) player_results: Vec<RoundPlayerResultView>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct GameDetailView {
    pub(crate) game_id: i64,
    pub(crate) table_code: String,
    pub(crate) owner: UserBriefView,
    pub(crate) multiplier: i64,
    pub(crate) started_at: String,
    pub(crate) ended_at: Option<String>,
    pub(crate) round_count: i64,
    pub(crate) final_room: Option<Value>,
    pub(crate) rounds: Vec<RoundRecordView>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct FanStatView {
    pub(crate) user_id: i64,
    pub(crate) fan_key: String,
    pub(crate) fan_label: String,
    pub(crate) count: i64,
    pub(crate) last_seen_at: String,
}

fn user_brief_view(user_id: i64, display_name: &str, points: i64) -> UserBriefView {
    UserBriefView {
        user_id,
        display_name: display_name.to_string(),
        points,
        title: title_for_points(points).to_string(),
        display_label: display_label(display_name, points),
    }
}

pub(crate) fn game_summary_view(summary: &GameSummaryRecord) -> GameSummaryView {
    GameSummaryView {
        game_id: summary.game_id,
        table_code: summary.table_code.clone(),
        owner: user_brief_view(
            summary.owner_user_id,
            &summary.owner_display_name,
            summary.owner_points,
        ),
        multiplier: summary.multiplier,
        started_at: summary.started_at.clone(),
        ended_at: summary.ended_at.clone(),
        round_count: summary.round_count,
        opponent_names: summary.opponent_names.clone(),
        player_summary: summary
            .player_summary
            .as_ref()
            .map(user_game_player_summary_view),
    }
}

fn user_game_player_summary_view(
    summary: &UserGamePlayerSummaryRecord,
) -> UserGamePlayerSummaryView {
    UserGamePlayerSummaryView {
        round_count: summary.round_count,
        win_count: summary.win_count,
        self_draw_win_count: summary.self_draw_win_count,
        discard_win_count: summary.discard_win_count,
        deal_in_count: summary.deal_in_count,
        total_score_delta: summary.total_score_delta,
        average_cumulative_score: summary.average_cumulative_score,
        high_score_round_count: summary.high_score_round_count,
    }
}

fn round_player_result_view(result: &RoundPlayerResultRecord) -> RoundPlayerResultView {
    RoundPlayerResultView {
        user_id: result.user_id,
        seat_index: result.seat_index,
        score_delta: result.score_delta,
        point_delta: result.point_delta,
        cumulative_score: result.cumulative_score,
        is_winner: result.is_winner,
        win_type: result.win_type.clone(),
        nickname_snapshot: result.nickname_snapshot.clone(),
    }
}

pub(crate) fn game_detail_view(detail: &GameRecordDetail) -> Result<GameDetailView> {
    Ok(GameDetailView {
        game_id: detail.summary.game_id,
        table_code: detail.summary.table_code.clone(),
        owner: user_brief_view(
            detail.summary.owner_user_id,
            &detail.summary.owner_display_name,
            detail.summary.owner_points,
        ),
        multiplier: detail.summary.multiplier,
        started_at: detail.summary.started_at.clone(),
        ended_at: detail.summary.ended_at.clone(),
        round_count: detail.summary.round_count,
        final_room: detail
            .final_room_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?,
        rounds: detail
            .rounds
            .iter()
            .map(|round| {
                Ok(RoundRecordView {
                    round_record_id: round.round_record_id,
                    round_id: round.round_id.clone(),
                    ended_at: round.ended_at.clone(),
                    settlement: serde_json::from_str(&round.settlement_json)?,
                    player_results: round
                        .player_results
                        .iter()
                        .map(round_player_result_view)
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>>>()?,
    })
}

pub(crate) fn fan_stat_view(record: &UserFanStatRecord) -> FanStatView {
    FanStatView {
        user_id: record.user_id,
        fan_key: record.fan_key.clone(),
        fan_label: record.fan_label.clone(),
        count: record.count,
        last_seen_at: record.last_seen_at.clone(),
    }
}

fn pure_bot_seat_present(room: &RoomState) -> bool {
    room.seats.iter().any(|seat| seat.seat_type == "bot")
}

fn fan_keys_by_winner(settlement: &RoundSettlement) -> HashMap<usize, Vec<String>> {
    let mut by_seat = HashMap::new();
    if !settlement.winning_details.is_empty() {
        for detail in &settlement.winning_details {
            by_seat.insert(detail.winner_seat, detail.fan_keys.clone());
        }
        return by_seat;
    }
    if let Some(winner_seat) = settlement.winner_seat {
        by_seat.insert(winner_seat, settlement.fan_keys.clone());
    }
    by_seat
}

fn archive_input_from_room(
    room: &RoomState,
    table_created_at: &str,
    archived_at: &str,
    participants: &[TableParticipantRecord],
) -> Result<Option<ArchiveRoundInput>> {
    let Some(owner_user_id) = room.owner_user_id else {
        return Ok(None);
    };
    let Some(round) = room.round_state.as_ref() else {
        return Ok(None);
    };
    if round.phase != "settlement" {
        return Ok(None);
    }
    let Some(settlement) = round.settlement.as_ref() else {
        return Ok(None);
    };

    let participants_by_seat = participants
        .iter()
        .map(|participant| (participant.seat_index, participant))
        .collect::<HashMap<_, _>>();
    let points_enabled = !pure_bot_seat_present(room);
    let winner_fans = fan_keys_by_winner(settlement);
    let winning_seats = settlement
        .winning_seats()
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let cumulative_scores = room
        .match_state
        .as_ref()
        .map(|state| state.cumulative_scores.clone())
        .unwrap_or_default();

    let mut player_results = Vec::new();
    for participant in participants {
        let seat_index = participant.seat_index;
        let score_delta = settlement
            .score_delta
            .total_delta_by_seat
            .get(&seat_index)
            .copied()
            .unwrap_or(0);
        let is_winner = winning_seats.contains(&seat_index);
        let point_delta = if points_enabled { score_delta } else { 0 };
        player_results.push(ArchivedRoundPlayerInput {
            user_id: participant.user_id,
            seat_index,
            score_delta,
            point_delta,
            cumulative_score: cumulative_scores.get(&seat_index).copied().unwrap_or(0),
            is_winner,
            win_type: if is_winner {
                Some(settlement.win_type.clone())
            } else {
                None
            },
            nickname_snapshot: participant.nickname_snapshot.clone(),
        });
    }

    let mut fan_counts = HashMap::<(i64, String), i64>::new();
    for (seat_index, keys) in winner_fans {
        let Some(participant) = participants_by_seat.get(&seat_index) else {
            continue;
        };
        for fan_key in keys {
            *fan_counts
                .entry((participant.user_id, fan_key))
                .or_insert(0) += 1;
        }
    }
    let fan_stats = fan_counts
        .into_iter()
        .map(|((user_id, fan_key), count)| ArchivedFanStatInput {
            user_id,
            fan_label: fan_key.clone(),
            fan_key,
            count,
            last_seen_at: archived_at.to_string(),
        })
        .collect::<Vec<_>>();

    Ok(Some(ArchiveRoundInput {
        table_code: room.table_code.clone(),
        owner_user_id,
        multiplier: room.multiplier,
        started_at: table_created_at.to_string(),
        ended_at: archived_at.to_string(),
        round_id: round.round_id.clone(),
        settlement_json: serde_json::to_string(settlement)?,
        points_enabled,
        player_results,
        fan_stats,
    }))
}

pub(crate) async fn archive_current_round_if_needed(
    state: &AppContext,
    room: &RoomState,
    table_created_at: &str,
    archived_at: &str,
) -> Result<Option<ArchiveRoundOutcome>> {
    let participants = state
        .inner
        .db
        .list_active_table_participants_for_table(&room.table_code)
        .await?;
    let Some(input) = archive_input_from_room(room, table_created_at, archived_at, &participants)?
    else {
        return Ok(None);
    };

    let outcome = state.inner.db.archive_round(input).await?;
    if outcome.inserted {
        let recipient_user_ids = participants
            .iter()
            .map(|participant| participant.user_id)
            .collect::<std::collections::BTreeSet<_>>();
        for update in &outcome.point_updates {
            let previous_points = update.points - update.delta;
            let participant = participants
                .iter()
                .find(|participant| participant.user_id == update.user_id);
            let display_name = participant
                .map(|participant| participant.nickname_snapshot.clone())
                .unwrap_or_else(|| format!("用户 #{}", update.user_id));
            let payload = json!({
                "type": "user_points_updated",
                "payload": {
                    "user_id": update.user_id,
                    "delta": update.delta,
                    "old_points": previous_points,
                    "points": update.points,
                    "old_title": title_for_points(previous_points),
                    "title": title_for_points(update.points),
                    "display_name": display_name,
                    "reason": "round_settlement",
                    "source_table_code": room.table_code,
                    "source_round_id": room.round_state.as_ref().map(|round| round.round_id.clone()),
                }
            });
            for recipient_user_id in &recipient_user_ids {
                notify_user_connections(state, *recipient_user_id, payload.clone()).await;
            }
        }
    }
    Ok(Some(outcome))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::app::auth::{generate_session_token, hash_password, hash_session_token};
    use crate::app::persistence::{DbWorker, in_memory_database};
    use crate::app::server;
    use crate::app::{Settings, serialize_room_state};
    use crate::core::state::{MatchState, RoundState, SeatState, SettlementScoreDelta};
    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{Method, Request, StatusCode, header};
    use std::collections::BTreeMap;
    use tower::ServiceExt;

    const INITIAL_USER_POINTS: i64 = 600;

    fn test_settings() -> Settings {
        Settings {
            bind_addr: "127.0.0.1:0".to_string(),
            database_path: ":memory:".to_string(),
            cors_origins: vec![],
            frontend_dir: None,
        }
    }

    async fn test_state() -> Result<(AppContext, DbWorker)> {
        let db = in_memory_database("")?;
        db.initialize()?;
        let worker = DbWorker::start(db)?;
        Ok((AppContext::new(worker.clone()), worker))
    }

    async fn test_app() -> Result<(Router, AppContext, DbWorker)> {
        let (state, worker) = test_state().await?;
        Ok((
            server::build_app(state.clone(), &test_settings()),
            state,
            worker,
        ))
    }

    async fn register_user(
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

    fn json_response(
        response: axum::response::Response,
    ) -> impl std::future::Future<Output = Value> {
        async move {
            let bytes = to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("response body should be readable");
            serde_json::from_slice(&bytes).expect("response body should be valid json")
        }
    }

    fn json_request(method: Method, uri: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .expect("request should build")
    }

    fn base_room(
        table_code: &str,
        multiplier: i64,
        seats: Vec<SeatState>,
        score_delta_by_seat: &[(usize, i64)],
        cumulative_scores: &[(usize, i64)],
        winning_seat: usize,
        fan_keys: &[&str],
    ) -> RoomState {
        RoomState {
            table_code: table_code.to_string(),
            phase: "settlement".to_string(),
            mode: "normal".to_string(),
            owner_user_id: Some(1),
            multiplier,
            seats,
            match_state: Some(MatchState {
                cumulative_scores: cumulative_scores
                    .iter()
                    .copied()
                    .collect::<BTreeMap<_, _>>(),
                last_completed_round_id: Some("round-1".to_string()),
                ..MatchState::default()
            }),
            round_state: Some(RoundState {
                round_id: "round-1".to_string(),
                phase: "settlement".to_string(),
                settlement: Some(RoundSettlement {
                    provisional: true,
                    win_type: "discard".to_string(),
                    winner_seat: Some(winning_seat),
                    discarder_seat: Some((winning_seat + 1) % 2),
                    fan_keys: fan_keys.iter().map(|item| (*item).to_string()).collect(),
                    score_delta: SettlementScoreDelta {
                        total_delta_by_seat: score_delta_by_seat
                            .iter()
                            .copied()
                            .collect::<BTreeMap<_, _>>(),
                        ..crate::core::state::SettlementScoreDelta::default()
                    },
                    ..RoundSettlement::default()
                }),
                ..RoundState::default()
            }),
            pending_timeout: None,
            continue_action: None,
        }
    }

    fn seat(
        seat_index: usize,
        nickname: &str,
        reconnect_token: Option<&str>,
        is_bot: bool,
    ) -> SeatState {
        SeatState {
            seat_index,
            user_id: None,
            nickname: Some(nickname.to_string()),
            points: None,
            title: None,
            reconnect_token: reconnect_token.map(ToString::to_string),
            player_session_id: Some((seat_index as i64) + 1),
            connected: true,
            ready: true,
            is_bot,
            seat_type: if is_bot { "bot" } else { "human" }.to_string(),
            bot_persona: None,
            bot_aggression: None,
            disconnect_deadline_at: None,
        }
    }

    fn bot_takeover_seat(seat_index: usize, nickname: &str, reconnect_token: &str) -> SeatState {
        let mut seat = seat(seat_index, nickname, Some(reconnect_token), true);
        seat.seat_type = "human".to_string();
        seat
    }

    async fn persist_participant(
        worker: &DbWorker,
        room: &RoomState,
        table_created_at: &str,
        user_id: i64,
        seat_index: usize,
        nickname: &str,
    ) -> Result<()> {
        let room_json = serialize_room_state(room)?;
        worker
            .save_table_and_store_reconnect_token_and_upsert_participant(
                &room.table_code,
                table_created_at,
                &room_json,
                &format!("token-{seat_index}"),
                seat_index,
                seat_index as i64 + 10,
                user_id,
                nickname,
                table_created_at,
            )
            .await
    }

    async fn archived_detail(worker: &DbWorker) -> Result<GameRecordDetail> {
        let summaries = worker.list_game_summaries(10).await?;
        let game_id = summaries
            .first()
            .expect("one archived game should exist")
            .game_id;
        worker
            .get_game_detail(game_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("game detail should exist"))
    }

    #[tokio::test(flavor = "current_thread")]
    async fn records_archive_round_creates_results_without_multiplier_points() -> Result<()> {
        let (state, worker) = test_state().await?;
        let owner_user_id = register_user(&worker, "INVITE300001", "Owner").await?;
        let guest_user_id = register_user(&worker, "INVITE300002", "Guest").await?;
        let room = base_room(
            "ROOMREC1",
            3,
            vec![
                seat(0, "Owner", Some("owner-token"), false),
                seat(1, "Guest", Some("guest-token"), false),
            ],
            &[(0, 8), (1, -8)],
            &[(0, 108), (1, 92)],
            0,
            &["all_pungs"],
        );
        persist_participant(
            &worker,
            &room,
            "2026-05-06T00:00:00Z",
            owner_user_id,
            0,
            "Owner",
        )
        .await?;
        persist_participant(
            &worker,
            &room,
            "2026-05-06T00:00:00Z",
            guest_user_id,
            1,
            "Guest",
        )
        .await?;

        let outcome = archive_current_round_if_needed(
            &state,
            &room,
            "2026-05-06T00:00:00Z",
            "2026-05-06T01:00:00Z",
        )
        .await?
        .expect("settlement should archive");
        assert!(outcome.inserted);
        assert_eq!(outcome.point_updates.len(), 2);

        let detail = archived_detail(&worker).await?;
        assert_eq!(detail.summary.round_count, 1);
        let player_results = &detail.rounds[0].player_results;
        assert_eq!(player_results.len(), 2);
        assert_eq!(player_results[0].point_delta, 8);
        assert_eq!(player_results[1].point_delta, -8);

        let owner = worker
            .get_user_by_id(owner_user_id)
            .await?
            .expect("owner should exist");
        let guest = worker
            .get_user_by_id(guest_user_id)
            .await?
            .expect("guest should exist");
        assert_eq!(owner.points, INITIAL_USER_POINTS + 8);
        assert_eq!(guest.points, INITIAL_USER_POINTS - 8);

        let fan_stats = worker.list_user_fan_stats(owner_user_id).await?;
        assert_eq!(fan_stats.len(), 1);
        assert_eq!(fan_stats[0].fan_key, "all_pungs");
        assert_eq!(fan_stats[0].count, 1);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn point_events_archive_is_idempotent_and_fan_stats_increment_once() -> Result<()> {
        let (state, worker) = test_state().await?;
        let owner_user_id = register_user(&worker, "INVITE300003", "Owner").await?;
        let guest_user_id = register_user(&worker, "INVITE300004", "Guest").await?;
        let room = base_room(
            "ROOMREC2",
            2,
            vec![
                seat(0, "Owner", Some("owner-token"), false),
                seat(1, "Guest", Some("guest-token"), false),
            ],
            &[(0, 6), (1, -6)],
            &[(0, 106), (1, 94)],
            0,
            &["pure_straight"],
        );
        persist_participant(
            &worker,
            &room,
            "2026-05-06T00:00:00Z",
            owner_user_id,
            0,
            "Owner",
        )
        .await?;
        persist_participant(
            &worker,
            &room,
            "2026-05-06T00:00:00Z",
            guest_user_id,
            1,
            "Guest",
        )
        .await?;

        archive_current_round_if_needed(
            &state,
            &room,
            "2026-05-06T00:00:00Z",
            "2026-05-06T01:00:00Z",
        )
        .await?;
        let second = archive_current_round_if_needed(
            &state,
            &room,
            "2026-05-06T00:00:00Z",
            "2026-05-06T01:00:01Z",
        )
        .await?
        .expect("duplicate archive still returns outcome");
        assert!(!second.inserted);

        let detail = archived_detail(&worker).await?;
        assert_eq!(detail.rounds.len(), 1);

        let owner = worker
            .get_user_by_id(owner_user_id)
            .await?
            .expect("owner should exist");
        let guest = worker
            .get_user_by_id(guest_user_id)
            .await?
            .expect("guest should exist");
        assert_eq!(owner.points, INITIAL_USER_POINTS + 6);
        assert_eq!(guest.points, INITIAL_USER_POINTS - 6);

        let fan_stats = worker.list_user_fan_stats(owner_user_id).await?;
        assert_eq!(fan_stats[0].count, 1);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn point_events_skip_points_when_independent_bot_seat_exists() -> Result<()> {
        let (state, worker) = test_state().await?;
        let owner_user_id = register_user(&worker, "INVITE300005", "Owner").await?;
        let guest_user_id = register_user(&worker, "INVITE300006", "Guest").await?;
        let room = base_room(
            "ROOMREC3",
            3,
            vec![
                seat(0, "Owner", Some("owner-token"), false),
                seat(1, "Guest", Some("guest-token"), false),
                seat(2, "Bot 2", None, true),
            ],
            &[(0, 8), (1, -8), (2, 0)],
            &[(0, 108), (1, 92), (2, 100)],
            0,
            &["mixed_one_suit"],
        );
        persist_participant(
            &worker,
            &room,
            "2026-05-06T00:00:00Z",
            owner_user_id,
            0,
            "Owner",
        )
        .await?;
        persist_participant(
            &worker,
            &room,
            "2026-05-06T00:00:00Z",
            guest_user_id,
            1,
            "Guest",
        )
        .await?;

        archive_current_round_if_needed(
            &state,
            &room,
            "2026-05-06T00:00:00Z",
            "2026-05-06T01:00:00Z",
        )
        .await?;

        let detail = archived_detail(&worker).await?;
        let player_results = &detail.rounds[0].player_results;
        assert_eq!(player_results[0].point_delta, 0);
        assert_eq!(player_results[1].point_delta, 0);

        let owner = worker
            .get_user_by_id(owner_user_id)
            .await?
            .expect("owner should exist");
        let guest = worker
            .get_user_by_id(guest_user_id)
            .await?
            .expect("guest should exist");
        assert_eq!(owner.points, INITIAL_USER_POINTS);
        assert_eq!(guest.points, INITIAL_USER_POINTS);

        let fan_stats = worker.list_user_fan_stats(owner_user_id).await?;
        assert_eq!(fan_stats[0].count, 1);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn point_events_skip_points_when_bot_seat_has_participant_record() -> Result<()> {
        let (state, worker) = test_state().await?;
        let owner_user_id = register_user(&worker, "INVITE300011", "Owner").await?;
        let guest_user_id = register_user(&worker, "INVITE300012", "Guest").await?;
        let bot_user_id = register_user(&worker, "INVITE300013", "Bot User").await?;
        let room = base_room(
            "ROOMREC6",
            2,
            vec![
                seat(0, "Owner", Some("owner-token"), false),
                seat(1, "Guest", Some("guest-token"), false),
                seat(2, "Bot 2", None, true),
            ],
            &[(0, 7), (1, -7), (2, 0)],
            &[(0, 107), (1, 93), (2, 100)],
            0,
            &["all_sequences"],
        );
        persist_participant(
            &worker,
            &room,
            "2026-05-06T00:00:00Z",
            owner_user_id,
            0,
            "Owner",
        )
        .await?;
        persist_participant(
            &worker,
            &room,
            "2026-05-06T00:00:00Z",
            guest_user_id,
            1,
            "Guest",
        )
        .await?;
        persist_participant(
            &worker,
            &room,
            "2026-05-06T00:00:00Z",
            bot_user_id,
            2,
            "Bot User",
        )
        .await?;

        archive_current_round_if_needed(
            &state,
            &room,
            "2026-05-06T00:00:00Z",
            "2026-05-06T01:00:00Z",
        )
        .await?;

        let detail = archived_detail(&worker).await?;
        let player_results = &detail.rounds[0].player_results;
        assert!(player_results.iter().all(|result| result.point_delta == 0));

        let owner = worker
            .get_user_by_id(owner_user_id)
            .await?
            .expect("owner should exist");
        let guest = worker
            .get_user_by_id(guest_user_id)
            .await?
            .expect("guest should exist");
        assert_eq!(owner.points, INITIAL_USER_POINTS);
        assert_eq!(guest.points, INITIAL_USER_POINTS);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn point_events_treat_bot_takeover_human_seat_as_human() -> Result<()> {
        let (state, worker) = test_state().await?;
        let owner_user_id = register_user(&worker, "INVITE300007", "Owner").await?;
        let guest_user_id = register_user(&worker, "INVITE300008", "Guest").await?;
        let room = base_room(
            "ROOMREC4",
            2,
            vec![
                bot_takeover_seat(0, "Owner", "owner-token"),
                seat(1, "Guest", Some("guest-token"), false),
            ],
            &[(0, 9), (1, -9)],
            &[(0, 109), (1, 91)],
            0,
            &["all_sequences"],
        );
        persist_participant(
            &worker,
            &room,
            "2026-05-06T00:00:00Z",
            owner_user_id,
            0,
            "Owner",
        )
        .await?;
        persist_participant(
            &worker,
            &room,
            "2026-05-06T00:00:00Z",
            guest_user_id,
            1,
            "Guest",
        )
        .await?;

        archive_current_round_if_needed(
            &state,
            &room,
            "2026-05-06T00:00:00Z",
            "2026-05-06T01:00:00Z",
        )
        .await?;

        let detail = archived_detail(&worker).await?;
        let player_results = &detail.rounds[0].player_results;
        assert_eq!(player_results[0].point_delta, 9);
        assert_eq!(player_results[1].point_delta, -9);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn records_public_apis_return_games_fans_and_leaderboard() -> Result<()> {
        let (app, state, worker) = test_app().await?;
        let owner_user_id = register_user(&worker, "INVITE300009", "Owner").await?;
        let guest_user_id = register_user(&worker, "INVITE300010", "Guest").await?;
        let mut room = base_room(
            "ROOMREC5",
            1,
            vec![
                seat(0, "Owner", Some("owner-token"), false),
                seat(1, "Guest", Some("guest-token"), false),
            ],
            &[(0, 10), (1, -10)],
            &[(0, 110), (1, 90)],
            0,
            &["all_pungs"],
        );
        room.phase = "finished".to_string();
        if let Some(match_state) = &mut room.match_state {
            match_state.match_finished = true;
        }
        persist_participant(
            &worker,
            &room,
            "2026-05-06T00:00:00Z",
            owner_user_id,
            0,
            "Owner",
        )
        .await?;
        persist_participant(
            &worker,
            &room,
            "2026-05-06T00:00:00Z",
            guest_user_id,
            1,
            "Guest",
        )
        .await?;
        let archive = archive_current_round_if_needed(
            &state,
            &room,
            "2026-05-06T00:00:00Z",
            "2026-05-06T01:00:00Z",
        )
        .await?
        .expect("archive should produce a game");
        let game_id = archive.game_id;
        worker
            .delete_table(&room.table_code, "2026-05-06T01:05:00Z")
            .await?;

        let games_response = app
            .clone()
            .oneshot(json_request(Method::GET, "/api/games"))
            .await?;
        assert_eq!(games_response.status(), StatusCode::OK);
        let games_body = json_response(games_response).await;
        assert_eq!(games_body.as_array().map(Vec::len), Some(1));

        let detail_response = app
            .clone()
            .oneshot(json_request(Method::GET, &format!("/api/games/{game_id}")))
            .await?;
        assert_eq!(detail_response.status(), StatusCode::OK);
        let detail_body = json_response(detail_response).await;
        assert_eq!(detail_body["rounds"][0]["round_id"], "round-1");

        let user_games_response = app
            .clone()
            .oneshot(json_request(
                Method::GET,
                &format!("/api/users/{owner_user_id}/games"),
            ))
            .await?;
        assert_eq!(user_games_response.status(), StatusCode::OK);
        let user_games_body = json_response(user_games_response).await;
        assert_eq!(user_games_body[0]["game_id"], game_id);
        assert_eq!(user_games_body[0]["opponent_names"][0], "Guest");
        assert_eq!(user_games_body[0]["player_summary"]["round_count"], 1);
        assert_eq!(user_games_body[0]["player_summary"]["win_count"], 1);
        assert_eq!(user_games_body[0]["player_summary"]["deal_in_count"], 0);
        assert_eq!(
            user_games_body[0]["player_summary"]["high_score_round_count"],
            1
        );

        let guest_games_response = app
            .clone()
            .oneshot(json_request(
                Method::GET,
                &format!("/api/users/{guest_user_id}/games"),
            ))
            .await?;
        assert_eq!(guest_games_response.status(), StatusCode::OK);
        let guest_games_body = json_response(guest_games_response).await;
        assert_eq!(guest_games_body[0]["player_summary"]["deal_in_count"], 1);

        let fans_response = app
            .clone()
            .oneshot(json_request(
                Method::GET,
                &format!("/api/users/{owner_user_id}/fans"),
            ))
            .await?;
        assert_eq!(fans_response.status(), StatusCode::OK);
        let fans_body = json_response(fans_response).await;
        assert_eq!(fans_body[0]["fan_key"], "all_pungs");

        let leaderboard_response = app
            .oneshot(json_request(Method::GET, "/api/leaderboard"))
            .await?;
        assert_eq!(leaderboard_response.status(), StatusCode::OK);
        let leaderboard_body = json_response(leaderboard_response).await;
        assert_eq!(leaderboard_body[0]["user_id"], owner_user_id);
        assert_eq!(leaderboard_body[1]["user_id"], guest_user_id);
        Ok(())
    }
}
