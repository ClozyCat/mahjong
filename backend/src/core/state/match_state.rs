use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::error::EngineError;
use crate::core::ids::{RoundId, Seat};

use super::RoundSettlement;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MatchSeatStatistics {
    pub score_history: Vec<i64>,
    pub win_count: u32,
    pub deal_in_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MatchStatistics {
    pub completed_round_count: u32,
    pub seat_stats_by_seat: BTreeMap<Seat, MatchSeatStatistics>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MatchState {
    pub prevailing_wind: String,
    pub hand_number: u32,
    pub dealer_seat: Seat,
    pub dealer_repeat_count: u32,
    pub cumulative_scores: BTreeMap<Seat, i64>,
    pub match_finished: bool,
    pub last_completed_round_id: Option<RoundId>,
    #[serde(default, deserialize_with = "super::null_default")]
    pub statistics: MatchStatistics,
    /// 每位玩家在一整局(16小局)中的额外思考时长(秒)
    #[serde(default)]
    pub extra_time_pool: BTreeMap<Seat, i64>,
}

impl MatchState {
    /// 初始化额外时间池，每位玩家获得90秒
    pub fn initialize_extra_time_pool(&mut self) {
        for seat in 0..4 {
            self.extra_time_pool.insert(seat, 90);
        }
    }
}

impl MatchState {
    pub(crate) fn from_value(value: &Value) -> Result<Self, EngineError> {
        serde_json::from_value(value.clone()).map_err(Into::into)
    }

    pub fn sync_statistics_to_cumulative_scores(&mut self) {
        let inferred_completed_round_count = self
            .statistics
            .seat_stats_by_seat
            .values()
            .map(|stats| stats.score_history.len().saturating_sub(1) as u32)
            .max()
            .unwrap_or(0);
        self.statistics.completed_round_count = self
            .statistics
            .completed_round_count
            .max(inferred_completed_round_count);

        for (&seat, &score) in &self.cumulative_scores {
            let seat_stats = self.statistics.seat_stats_by_seat.entry(seat).or_default();
            if seat_stats.score_history.is_empty() {
                seat_stats.score_history.push(score);
                continue;
            }

            if let Some(last_score) = seat_stats.score_history.last_mut() {
                *last_score = score;
            }
        }
    }

    pub fn apply_completed_round(
        &mut self,
        round_id: RoundId,
        cumulative_scores: BTreeMap<Seat, i64>,
        settlement: &RoundSettlement,
    ) {
        if self.last_completed_round_id.as_deref() == Some(round_id.as_str()) {
            self.cumulative_scores = cumulative_scores;
            self.sync_statistics_to_cumulative_scores();
            return;
        }

        self.sync_statistics_to_cumulative_scores();

        for (&seat, &score) in &cumulative_scores {
            let seat_stats = self.statistics.seat_stats_by_seat.entry(seat).or_default();
            seat_stats.score_history.push(score);
        }

        let winning_seats = settlement.winning_seats();

        for &winner_seat in &winning_seats {
            let winner_stats = self
                .statistics
                .seat_stats_by_seat
                .entry(winner_seat)
                .or_default();
            winner_stats.win_count += 1;
        }

        if settlement.win_type == "discard" {
            if let Some(discarder_seat) = settlement.discarder_seat {
                let discarder_stats = self
                    .statistics
                    .seat_stats_by_seat
                    .entry(discarder_seat)
                    .or_default();
                discarder_stats.deal_in_count += winning_seats.len() as u32;
            }
        }

        self.statistics.completed_round_count += 1;
        self.cumulative_scores = cumulative_scores;
        self.last_completed_round_id = Some(round_id);
    }
}
