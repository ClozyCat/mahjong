use serde::{Deserialize, Serialize};

use super::{
    action_space::{claim_action_index, self_kong_action_index, tile_index},
    context::{BotAction, BotContext, BotSelfKongKind},
    features::encode_bot_context_v2,
};
use crate::core::{
    engine::try_handle_player_action_in_room_state,
    state::{RoomState, SeatState},
};
use crate::rules::standard::{
    automation::{
        next_bot_action_in_room_state_with_policy_resolver,
        next_bot_decision_trace_in_room_state_with_policy_resolver,
    },
    flow::start_match_in_room_state,
};

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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ArenaTrajectoryRow {
    pub schema_version: u32,
    pub match_id: String,
    pub decision_index: u64,
    pub seat_index: usize,
    pub policy_id: String,
    pub decision_kind: String,
    pub tile_planes: Vec<f32>,
    pub scalar_features: Vec<f32>,
    pub discard_mask: Vec<bool>,
    pub claim_mask: Vec<bool>,
    pub self_kong_mask: Vec<bool>,
    pub hu_mask: Vec<bool>,
    pub action_head: String,
    pub action_index: i64,
    pub action_semantic: String,
    pub log_prob: f32,
    pub value: f32,
    pub reward: f32,
    pub done: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ArenaRunOutput {
    pub reports: Vec<ArenaMatchReport>,
    pub trajectories: Vec<ArenaTrajectoryRow>,
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

pub fn run_arena(
    config: &ArenaConfig,
    include_trajectories: bool,
) -> Result<ArenaRunOutput, String> {
    if config.policies.is_empty() {
        return Err("arena config requires at least one policy".to_string());
    }

    let mut output = ArenaRunOutput::default();
    for match_index in 0..config.matches {
        let seed = config.seed.wrapping_add(match_index as u64);
        let match_id = format!("arena-{seed}-{match_index}");
        let mut room = arena_room(&format!("ARENA{match_index:04}"));
        start_match_in_room_state(&mut room, 0, seed)?;
        let mut accumulator = ArenaMatchAccumulator::new(config);
        let mut action_count = 0_usize;
        let mut match_trajectories = Vec::new();

        while room.phase == "playing" && action_count < config.max_actions_per_match {
            let started = std::time::Instant::now();
            let trace = include_trajectories
                .then(|| {
                    next_bot_decision_trace_in_room_state_with_policy_resolver(&room, &|seat| {
                        policy_for_seat(config, seat)
                    })
                })
                .transpose()?
                .flatten();
            let action = if let Some(trace) = trace.as_ref() {
                trace.action.clone()
            } else {
                let Some(action) =
                    next_bot_action_in_room_state_with_policy_resolver(&room, &|seat| {
                        policy_for_seat(config, seat)
                    })?
                else {
                    break;
                };
                action
            };
            let elapsed_ms = started.elapsed().as_millis();
            let action_seat = action.seat_index;
            let action_type = action.action_type.clone();
            let handled = try_handle_player_action_in_room_state(
                &mut room,
                action.seat_index,
                &action.action_type,
                &action.tile_ids,
            )?;
            match handled {
                Some(Ok(_)) => {
                    accumulator.record_decision(action_seat, &action_type, elapsed_ms);
                    if let Some(trace) = trace.as_ref() {
                        let policy = policy_for_seat(config, action_seat);
                        if let Some(row) = trajectory_row_from_trace(
                            &match_id,
                            match_trajectories.len() as u64,
                            &policy.id,
                            trace,
                        ) {
                            match_trajectories.push(row);
                        }
                    }
                    action_count += 1;
                }
                Some(Err(reason)) => return Err(format!("arena action was rejected: {reason}")),
                None => {
                    return Err(format!(
                        "arena action was not handled: seat={} action={}",
                        action_seat, action_type
                    ));
                }
            }
        }

        let report = build_match_report(
            match_index,
            seed,
            &room,
            accumulator,
            action_count,
            action_count < config.max_actions_per_match,
        );
        assign_terminal_rewards(&mut match_trajectories, &report);
        output.trajectories.extend(match_trajectories);
        output.reports.push(report);
    }
    Ok(output)
}

fn policy_for_seat(config: &ArenaConfig, seat_index: usize) -> ArenaBotPolicyConfig {
    config
        .policies
        .get(seat_index % config.policies.len())
        .cloned()
        .unwrap_or_else(ArenaBotPolicyConfig::heuristic)
}

fn trajectory_row_from_trace(
    match_id: &str,
    decision_index: u64,
    policy_id: &str,
    trace: &crate::rules::standard::automation::BotDecisionTrace,
) -> Option<ArenaTrajectoryRow> {
    let features = encode_bot_context_v2(&trace.context);
    let (action_head, action_index, action_semantic) =
        encode_action_for_trajectory(&trace.decision_kind, &trace.context, &trace.action)?;
    Some(ArenaTrajectoryRow {
        schema_version: 1,
        match_id: match_id.to_string(),
        decision_index,
        seat_index: trace.action.seat_index,
        policy_id: policy_id.to_string(),
        decision_kind: trace.decision_kind.clone(),
        tile_planes: features.tile_planes,
        scalar_features: features.scalar_features,
        discard_mask: features.discard_mask.to_vec(),
        claim_mask: features.claim_mask.to_vec(),
        self_kong_mask: features.self_kong_mask.to_vec(),
        hu_mask: features.hu_mask.to_vec(),
        action_head,
        action_index,
        action_semantic,
        log_prob: 0.0,
        value: 0.0,
        reward: 0.0,
        done: false,
    })
}

fn encode_action_for_trajectory(
    decision_kind: &str,
    context: &BotContext,
    action: &BotAction,
) -> Option<(String, i64, String)> {
    match decision_kind {
        "active_turn" => encode_active_turn_action(context, action),
        "claim_window" => encode_claim_action(context, action),
        _ => None,
    }
}

fn encode_active_turn_action(
    context: &BotContext,
    action: &BotAction,
) -> Option<(String, i64, String)> {
    match action.action_type.as_str() {
        "discard" => {
            let tile_id = action.tile_ids.first()?;
            let tile_key = context
                .player
                .concealed_tiles
                .iter()
                .find(|tile| &tile.tile_id == tile_id)
                .map(|tile| tile.tile_key.as_str())?;
            let index = tile_index(tile_key)? as i64;
            Some((
                "discard".to_string(),
                index,
                action_semantic("discard", Some(tile_key)),
            ))
        }
        "kong" => {
            let candidate = context
                .self_kong_candidates
                .iter()
                .find(|candidate| candidate.tile_ids == action.tile_ids)?;
            let action_name = match candidate.kind {
                BotSelfKongKind::Concealed => "concealed_kong",
                BotSelfKongKind::Add => "add_kong",
            };
            let index = self_kong_action_index(action_name)? as i64;
            Some((
                "self_kong".to_string(),
                index,
                format!("self_kong:{action_name}:{}", candidate.tile_key),
            ))
        }
        _ => None,
    }
}

fn encode_claim_action(context: &BotContext, action: &BotAction) -> Option<(String, i64, String)> {
    let action_name = match action.action_type.as_str() {
        "pass" => "pass",
        "hu" => "hu",
        "pung" => "pung",
        "kong" => "kong",
        "chow" => claim_chow_action_name(context, action)?,
        _ => return None,
    };
    let index = claim_action_index(action_name)? as i64;
    Some((
        "claim".to_string(),
        index,
        action_semantic(
            &action.action_type,
            context.last_discard_tile_key.as_deref(),
        ),
    ))
}

fn claim_chow_action_name(context: &BotContext, action: &BotAction) -> Option<&'static str> {
    let last_discard = context.last_discard_tile_key.as_deref()?;
    let discard_index = tile_index(last_discard)?;
    if discard_index >= 27 {
        return Some("chow_mid");
    }

    let mut keys = vec![last_discard.to_string()];
    for tile_id in &action.tile_ids {
        let tile = context
            .player
            .concealed_tiles
            .iter()
            .find(|tile| &tile.tile_id == tile_id)?;
        keys.push(tile.tile_key.clone());
    }

    keys.sort_by_key(|key| tile_index(key).unwrap_or(usize::MAX));
    let middle_index = tile_index(keys.get(1)?)?;
    if middle_index >= 27 || middle_index / 9 != discard_index / 9 {
        return Some("chow_mid");
    }
    if discard_index == middle_index - 1 {
        return Some("chow_left");
    }
    if discard_index == middle_index + 1 {
        return Some("chow_right");
    }
    Some("chow_mid")
}

pub fn action_semantic(action_type: &str, tile_key: Option<&str>) -> String {
    match tile_key {
        Some(tile_key) if action_type == "discard" => format!("discard:{tile_key}"),
        Some(tile_key) if action_type == "chow" => format!("claim:chow:{tile_key}"),
        Some(tile_key) if action_type == "pung" => format!("claim:pung:{tile_key}"),
        Some(tile_key) if action_type == "kong" => format!("claim:kong:{tile_key}"),
        _ => action_type.to_string(),
    }
}

fn assign_terminal_rewards(rows: &mut [ArenaTrajectoryRow], report: &ArenaMatchReport) {
    for row in rows.iter_mut() {
        if let Some(seat) = report
            .seats
            .iter()
            .find(|seat| seat.seat_index == row.seat_index)
        {
            let score_reward = seat.score_delta as f32 / 100.0;
            let win_bonus = if seat.wins > 0 { 1.0 } else { 0.0 };
            let deal_in_penalty = if seat.dealt_in > 0 { -1.5 } else { 0.0 };
            row.reward = score_reward + win_bonus + deal_in_penalty;
        }
    }
    if let Some(last) = rows.last_mut() {
        last.done = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::action_space::{CLAIM_ACTION_COUNT, SELF_KONG_ACTION_COUNT, TILE_KIND_COUNT};

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

    #[test]
    fn trajectory_row_serializes_action_metadata() {
        let row = ArenaTrajectoryRow {
            schema_version: 1,
            match_id: "arena-1".to_string(),
            decision_index: 0,
            seat_index: 0,
            policy_id: "heuristic".to_string(),
            decision_kind: "active_turn".to_string(),
            tile_planes: vec![0.0; 340],
            scalar_features: vec![0.0; 10],
            discard_mask: vec![true; TILE_KIND_COUNT],
            claim_mask: vec![true; CLAIM_ACTION_COUNT],
            self_kong_mask: vec![true; SELF_KONG_ACTION_COUNT],
            hu_mask: vec![true, false],
            action_head: "discard".to_string(),
            action_index: 0,
            action_semantic: "discard:w1".to_string(),
            log_prob: 0.0,
            value: 0.0,
            reward: 0.0,
            done: false,
        };

        let value = serde_json::to_value(row).expect("row");

        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["action_head"], "discard");
        assert_eq!(
            value["discard_mask"].as_array().expect("mask").len(),
            TILE_KIND_COUNT
        );
    }
}
