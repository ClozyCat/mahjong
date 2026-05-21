use serde::{Deserialize, Serialize};

use crate::app::{generate_short_hex, initial_room_state_with_owner};
use crate::core::state::{RoomState, SeatState};

#[derive(Debug, Deserialize)]
pub(crate) struct CreateEvaluationRequest {
    #[serde(default)]
    pub(crate) subject_user_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct EvaluationSubjectResponse {
    pub(crate) subject_id: String,
    pub(crate) user_id: Option<i64>,
    pub(crate) display_name: String,
    pub(crate) kind: String,
    pub(crate) table_code: String,
    pub(crate) phase: String,
    pub(crate) completed: bool,
    pub(crate) final_score: Option<i64>,
    pub(crate) deal_in_count: Option<u64>,
    pub(crate) win_count: Option<u64>,
    pub(crate) completed_round_count: Option<u64>,
    pub(crate) ready_hand_win_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct EvaluationSessionResponse {
    pub(crate) evaluation_id: String,
    pub(crate) seed: u64,
    pub(crate) subjects: Vec<EvaluationSubjectResponse>,
}

pub(crate) fn evaluation_table_code(prefix: &str, index: usize) -> String {
    format!("{}{:02}", prefix, index)
}

pub(crate) fn build_evaluation_room(
    table_code: &str,
    owner_user_id: i64,
    subject_user_id: Option<i64>,
    subject_name: &str,
    subject_is_bot: bool,
) -> RoomState {
    let mut room = initial_room_state_with_owner(table_code, Some(owner_user_id), 1);
    crate::evaluation::apply_evaluation_rules(&mut room);
    room.seats.push(SeatState {
        seat_index: 0,
        user_id: subject_user_id,
        nickname: Some(subject_name.to_string()),
        points: Some(600),
        title: None,
        connected: subject_is_bot,
        is_bot: subject_is_bot,
        seat_type: if subject_is_bot { "bot" } else { "human" }.to_string(),
        ..SeatState::default()
    });
    for seat_index in 1..4 {
        room.seats.push(SeatState {
            seat_index,
            nickname: Some(format!("sft_bot_{seat_index}")),
            points: Some(600),
            connected: true,
            is_bot: true,
            seat_type: "bot".to_string(),
            ..SeatState::default()
        });
    }
    room
}

pub(crate) fn new_evaluation_id() -> String {
    format!("eval-{}", generate_short_hex(6))
}

pub(crate) fn apply_room_result_to_evaluation_subject(
    subject: &mut EvaluationSubjectResponse,
    room: &RoomState,
) {
    subject.phase = room.phase.clone();
    let Some(match_state) = room.match_state.as_ref() else {
        return;
    };
    subject.completed = match_state.match_finished
        || match_state.statistics.completed_round_count as usize
            >= crate::evaluation::EVALUATION_HAND_COUNT;

    // Find the subject's current seat index by user_id, fallback to seat 0
    let subject_seat_index = room.seats.iter()
        .find(|seat| seat.user_id == subject.user_id)
        .map(|seat| seat.seat_index)
        .unwrap_or(0);

    subject.final_score = match_state.cumulative_scores.get(&subject_seat_index).copied();
    if let Some(stats) = match_state.statistics.seat_stats_by_seat.get(&subject_seat_index) {
        subject.deal_in_count = Some(u64::from(stats.deal_in_count));
        subject.win_count = Some(u64::from(stats.win_count));
        subject.ready_hand_win_count = Some(u64::from(stats.ready_hand_win_count));
    }
    subject.completed_round_count = Some(u64::from(match_state.statistics.completed_round_count));
}

pub(crate) fn apply_room_result_to_evaluation_session(
    session: &mut EvaluationSessionResponse,
    room: &RoomState,
) {
    for subject in &mut session.subjects {
        if subject.table_code == room.table_code {
            apply_room_result_to_evaluation_subject(subject, room);
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::state::MatchState;

    #[test]
    fn create_evaluation_response_serializes_subject_results() {
        let response = EvaluationSessionResponse {
            evaluation_id: "eval-test".to_string(),
            seed: 7,
            subjects: vec![EvaluationSubjectResponse {
                subject_id: "user:1".to_string(),
                user_id: Some(1),
                display_name: "Alice".to_string(),
                kind: "human".to_string(),
                table_code: "EVAL1".to_string(),
                phase: "waiting".to_string(),
                completed: false,
                final_score: None,
                deal_in_count: None,
                win_count: None,
                completed_round_count: None,
                ready_hand_win_count: None,
            }],
        };

        let value = serde_json::to_value(response).expect("response");

        assert_eq!(value["evaluation_id"], "eval-test");
        assert_eq!(
            value["subjects"][0]["deal_in_count"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn evaluation_room_uses_fixed_rules_and_default_sft_opponents() {
        let room = build_evaluation_room("EVAL01", 1, Some(1), "Alice", false);

        assert_eq!(room.mode, crate::evaluation::EVALUATION_ROOM_MODE);
        assert_eq!(room.minimum_hu_fan, 8);
        assert!(!room.dealer_repeat_enabled);
        assert!(!room.dealer_double_enabled);
        assert_eq!(room.seats.len(), 4);
        assert_eq!(room.seats[0].seat_type, "human");
        assert!(room.seats[1..].iter().all(|seat| seat.is_bot));
    }

    #[test]
    fn room_result_updates_subject_score_and_counts() {
        let mut room = RoomState {
            phase: "finished".to_string(),
            match_state: Some(MatchState::default()),
            ..RoomState::default()
        };
        let match_state = room.match_state.as_mut().expect("match state");
        match_state.match_finished = true;
        match_state.cumulative_scores.insert(0, 42);
        let stats = match_state
            .statistics
            .seat_stats_by_seat
            .entry(0)
            .or_default();
        stats.deal_in_count = 2;
        stats.win_count = 3;
        stats.ready_hand_win_count = 1;
        match_state.statistics.completed_round_count = 4;
        let mut subject = EvaluationSubjectResponse {
            subject_id: "user:1".to_string(),
            user_id: Some(1),
            display_name: "Alice".to_string(),
            kind: "human".to_string(),
            table_code: "EVAL01".to_string(),
            phase: "playing".to_string(),
            completed: false,
            final_score: None,
            deal_in_count: None,
            win_count: None,
            completed_round_count: None,
            ready_hand_win_count: None,
        };

        apply_room_result_to_evaluation_subject(&mut subject, &room);

        assert!(subject.completed);
        assert_eq!(subject.phase, "finished");
        assert_eq!(subject.final_score, Some(42));
        assert_eq!(subject.deal_in_count, Some(2));
        assert_eq!(subject.win_count, Some(3));
        assert_eq!(subject.completed_round_count, Some(4));
        assert_eq!(subject.ready_hand_win_count, Some(1));
    }

    #[test]
    fn room_result_uses_subject_current_seat_after_wind_rotation() {
        let mut room = RoomState {
            phase: "finished".to_string(),
            seats: vec![
                SeatState {
                    seat_index: 0,
                    nickname: Some("sft_bot_1".to_string()),
                    is_bot: true,
                    seat_type: "bot".to_string(),
                    ..SeatState::default()
                },
                SeatState {
                    seat_index: 1,
                    user_id: Some(1),
                    nickname: Some("Alice".to_string()),
                    seat_type: "human".to_string(),
                    ..SeatState::default()
                },
            ],
            match_state: Some(MatchState::default()),
            ..RoomState::default()
        };
        let match_state = room.match_state.as_mut().expect("match state");
        match_state.match_finished = true;
        match_state.cumulative_scores.insert(0, 11);
        match_state.cumulative_scores.insert(1, 42);
        match_state
            .statistics
            .seat_stats_by_seat
            .entry(0)
            .or_default()
            .win_count = 9;
        let subject_stats = match_state
            .statistics
            .seat_stats_by_seat
            .entry(1)
            .or_default();
        subject_stats.deal_in_count = 2;
        subject_stats.win_count = 3;
        subject_stats.ready_hand_win_count = 1;
        match_state.statistics.completed_round_count = 4;
        let mut subject = EvaluationSubjectResponse {
            subject_id: "user:1".to_string(),
            user_id: Some(1),
            display_name: "Alice".to_string(),
            kind: "human".to_string(),
            table_code: "EVAL01".to_string(),
            phase: "playing".to_string(),
            completed: false,
            final_score: None,
            deal_in_count: None,
            win_count: None,
            completed_round_count: None,
            ready_hand_win_count: None,
        };

        apply_room_result_to_evaluation_subject(&mut subject, &room);

        assert_eq!(subject.final_score, Some(42));
        assert_eq!(subject.deal_in_count, Some(2));
        assert_eq!(subject.win_count, Some(3));
        assert_eq!(subject.ready_hand_win_count, Some(1));
    }

    #[test]
    fn evaluation_completes_after_configured_hand_count_even_on_final_settlement() {
        let mut room = RoomState {
            phase: "settlement".to_string(),
            match_state: Some(MatchState::default()),
            ..RoomState::default()
        };
        let match_state = room.match_state.as_mut().expect("match state");
        match_state.match_finished = false;
        match_state.statistics.completed_round_count =
            crate::evaluation::EVALUATION_HAND_COUNT as u32;
        let mut subject = EvaluationSubjectResponse {
            subject_id: "user:1".to_string(),
            user_id: Some(1),
            display_name: "Bot".to_string(),
            kind: "bot".to_string(),
            table_code: "EVAL01".to_string(),
            phase: "playing".to_string(),
            completed: false,
            final_score: None,
            deal_in_count: None,
            win_count: None,
            completed_round_count: None,
            ready_hand_win_count: None,
        };

        apply_room_result_to_evaluation_subject(&mut subject, &room);

        assert!(subject.completed);
        assert_eq!(subject.phase, "settlement");
        assert_eq!(
            subject.completed_round_count,
            Some(crate::evaluation::EVALUATION_HAND_COUNT as u64)
        );
    }

    #[test]
    fn room_result_updates_matching_subject_in_session() {
        let mut room = RoomState {
            table_code: "EVAL02".to_string(),
            phase: "finished".to_string(),
            match_state: Some(MatchState::default()),
            ..RoomState::default()
        };
        let match_state = room.match_state.as_mut().expect("match state");
        match_state.match_finished = true;
        match_state.cumulative_scores.insert(0, 88);
        match_state.statistics.completed_round_count = 16;
        match_state
            .statistics
            .seat_stats_by_seat
            .entry(0)
            .or_default()
            .win_count = 5;
        let mut session = EvaluationSessionResponse {
            evaluation_id: "eval-test".to_string(),
            seed: 7,
            subjects: vec![
                EvaluationSubjectResponse {
                    subject_id: "user:1".to_string(),
                    user_id: Some(1),
                    display_name: "Alice".to_string(),
                    kind: "human".to_string(),
                    table_code: "EVAL01".to_string(),
                    phase: "waiting".to_string(),
                    completed: false,
                    final_score: None,
                    deal_in_count: None,
                    win_count: None,
                    completed_round_count: None,
                    ready_hand_win_count: None,
                },
                EvaluationSubjectResponse {
                    subject_id: "user:2".to_string(),
                    user_id: Some(2),
                    display_name: "Bot".to_string(),
                    kind: "bot".to_string(),
                    table_code: "EVAL02".to_string(),
                    phase: "playing".to_string(),
                    completed: false,
                    final_score: None,
                    deal_in_count: None,
                    win_count: None,
                    completed_round_count: None,
                    ready_hand_win_count: None,
                },
            ],
        };

        apply_room_result_to_evaluation_session(&mut session, &room);

        assert!(!session.subjects[0].completed);
        assert!(session.subjects[1].completed);
        assert_eq!(session.subjects[1].final_score, Some(88));
        assert_eq!(session.subjects[1].win_count, Some(5));
        assert_eq!(session.subjects[1].completed_round_count, Some(16));
    }
}
