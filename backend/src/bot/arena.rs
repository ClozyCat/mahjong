use serde::{Deserialize, Serialize};

use crate::core::state::{RoomState, SeatState};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArenaPolicyMode {
    Heuristic,
    Hybrid,
    Neural,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ArenaBotPolicyConfig {
    pub id: String,
    pub mode: ArenaPolicyMode,
    pub neural_weight: i64,
    pub model_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ArenaConfig {
    pub matches: usize,
    pub seed: u64,
    #[serde(default = "default_max_actions_per_match")]
    pub max_actions_per_match: usize,
    #[serde(default)]
    pub report_trajectories: bool,
    pub policies: Vec<ArenaBotPolicyConfig>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct ArenaSeatMetrics {
    pub seat_index: usize,
    pub policy_id: String,
    pub score_delta: i64,
    pub wins: u64,
    pub dealt_in: u64,
    pub first_tenpai_turn: Option<u64>,
    pub final_tenpai: bool,
    pub claim_count: u64,
    pub discard_count: u64,
    pub decision_count: u64,
    pub decision_latency_ms_sum: u128,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ArenaMatchReport {
    pub match_index: usize,
    pub seed: u64,
    pub completed: bool,
    pub action_count: usize,
    pub seats: Vec<ArenaSeatMetrics>,
}

fn default_max_actions_per_match() -> usize {
    2400
}

impl ArenaBotPolicyConfig {
    pub fn heuristic() -> Self {
        Self {
            id: "heuristic".to_string(),
            mode: ArenaPolicyMode::Heuristic,
            neural_weight: 0,
            model_path: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ArenaMatchAccumulator {
    pub seats: Vec<ArenaSeatMetrics>,
}

impl ArenaMatchAccumulator {
    pub fn new(config: &ArenaConfig) -> Self {
        Self {
            seats: (0..4)
                .map(|seat_index| ArenaSeatMetrics {
                    seat_index,
                    policy_id: config
                        .policies
                        .get(seat_index % config.policies.len())
                        .map(|policy| policy.id.clone())
                        .unwrap_or_else(|| "heuristic".to_string()),
                    ..ArenaSeatMetrics::default()
                })
                .collect(),
        }
    }

    pub fn record_decision(&mut self, seat_index: usize, action_type: &str, latency_ms: u128) {
        if let Some(metrics) = self.seats.get_mut(seat_index) {
            metrics.decision_count += 1;
            metrics.decision_latency_ms_sum += latency_ms;
            match action_type {
                "discard" => {
                    metrics.discard_count += 1;
                }
                "chow" | "pung" | "kong" => metrics.claim_count += 1,
                _ => {}
            }
        }
    }
}

pub fn arena_room(table_code: &str) -> RoomState {
    RoomState {
        table_code: table_code.to_string(),
        phase: "waiting".to_string(),
        mode: "normal".to_string(),
        seats: (0..4)
            .map(|seat_index| SeatState {
                seat_index,
                nickname: Some(format!("Arena Bot {seat_index}")),
                reconnect_token: None,
                player_session_id: Some(-((seat_index as i64) + 1)),
                connected: true,
                ready: true,
                is_bot: true,
                seat_type: "bot".to_string(),
                bot_persona: None,
                bot_aggression: None,
                disconnect_deadline_at: None,
            })
            .collect(),
        match_state: None,
        round_state: None,
        pending_timeout: None,
        continue_action: None,
    }
}

pub fn build_match_report(
    match_index: usize,
    seed: u64,
    room: &RoomState,
    mut accumulator: ArenaMatchAccumulator,
    action_count: usize,
    completed: bool,
) -> ArenaMatchReport {
    if let Some(match_state) = &room.match_state {
        for metrics in &mut accumulator.seats {
            metrics.score_delta = match_state
                .cumulative_scores
                .get(&metrics.seat_index)
                .copied()
                .unwrap_or_default();
            if let Some(stats) = match_state
                .statistics
                .seat_stats_by_seat
                .get(&metrics.seat_index)
            {
                metrics.wins = stats.win_count as u64;
                metrics.dealt_in = stats.deal_in_count as u64;
            }
        }
    }
    if let Some(round) = &room.round_state {
        for player in &round.players {
            if let Some(metrics) = accumulator.seats.get_mut(player.seat) {
                metrics.final_tenpai = player.is_ready_hand;
            }
        }
    }
    ArenaMatchReport {
        match_index,
        seed,
        completed,
        action_count,
        seats: accumulator.seats,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_config_parses_policy_ids() {
        let raw = r#"{
            "matches": 2,
            "seed": 20260429,
            "policies": [
                {"id":"heuristic","mode":"heuristic","neural_weight":0,"model_path":null},
                {"id":"hybrid30","mode":"hybrid","neural_weight":30,"model_path":"backend/assets/models/mahjong_policy_net.onnx"}
            ]
        }"#;

        let config: ArenaConfig = serde_json::from_str(raw).expect("config");

        assert_eq!(config.matches, 2);
        assert_eq!(config.seed, 20260429);
        assert_eq!(config.max_actions_per_match, 2400);
        assert!(!config.report_trajectories);
        assert_eq!(config.policies[1].id, "hybrid30");
        assert_eq!(config.policies[1].mode, ArenaPolicyMode::Hybrid);
    }

    #[test]
    fn heuristic_policy_config_has_stable_defaults() {
        let config = ArenaBotPolicyConfig::heuristic();

        assert_eq!(config.id, "heuristic");
        assert_eq!(config.mode, ArenaPolicyMode::Heuristic);
        assert_eq!(config.neural_weight, 0);
        assert_eq!(config.model_path, None);
    }

    #[test]
    fn arena_room_creates_four_ready_bot_seats() {
        let room = arena_room("AR01");

        assert_eq!(room.table_code, "AR01");
        assert_eq!(room.phase, "waiting");
        assert_eq!(room.seats.len(), 4);
        assert!(room.seats.iter().all(|seat| seat.is_bot && seat.ready));
    }

    #[test]
    fn accumulator_records_decision_counts() {
        let config = ArenaConfig {
            matches: 1,
            seed: 7,
            max_actions_per_match: 10,
            report_trajectories: false,
            policies: vec![ArenaBotPolicyConfig::heuristic()],
        };
        let mut accumulator = ArenaMatchAccumulator::new(&config);

        accumulator.record_decision(0, "discard", 3);
        accumulator.record_decision(0, "pung", 2);

        assert_eq!(accumulator.seats[0].decision_count, 2);
        assert_eq!(accumulator.seats[0].discard_count, 1);
        assert_eq!(accumulator.seats[0].claim_count, 1);
        assert_eq!(accumulator.seats[0].decision_latency_ms_sum, 5);
    }
}
