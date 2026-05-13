use rand::{SeedableRng, rngs::StdRng};
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};

use super::{
    action_space::{claim_action_index, self_kong_action_index, tile_index},
    context::{BotAction, BotContext, BotSelfKongKind},
    features::encode_bot_context_v2,
    neural::{NeuralDecisionScores, neural_decision_scores_for_model_path},
    policy::risk_adjusted_discard_logits,
    reward::{
        RewardSnapshot, reward_snapshot_from_context, reward_snapshot_from_room, shaping_reward,
    },
};
use crate::core::{
    engine::try_handle_player_action_in_room_state,
    state::{PlayerRoundState, RoomState, SeatState},
};
use crate::rules::standard::{
    automation::{
        next_bot_action_in_room_state_with_policy_resolver,
        next_bot_decision_trace_in_room_state_with_policy_resolver,
    },
    flow::start_match_in_room_state,
    ready_hand::is_tenpai_hand_with_melds,
};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArenaPolicyMode {
    Heuristic,
    Neural,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArenaSeatRotation {
    Fixed,
    Cyclic,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ArenaBotPolicyConfig {
    pub id: String,
    pub mode: ArenaPolicyMode,
    pub model_path: Option<String>,
    #[serde(default)]
    pub sample_actions: bool,
    #[serde(default = "default_policy_temperature")]
    pub temperature: f32,
    #[serde(default)]
    pub record_heuristic_comparison: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ArenaConfig {
    pub matches: usize,
    pub seed: u64,
    #[serde(default = "default_max_actions_per_match")]
    pub max_actions_per_match: usize,
    #[serde(default)]
    pub report_trajectories: bool,
    #[serde(default)]
    pub record_heuristic_comparison: bool,
    #[serde(default = "default_seat_rotation")]
    pub seat_rotation: ArenaSeatRotation,
    #[serde(default)]
    pub seat_rotation_offset: usize,
    pub policies: Vec<ArenaBotPolicyConfig>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
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
    pub model_loaded: bool,
    pub fallback_count: u64,
    pub neural_action_count: u64,
    pub same_as_heuristic_count: u64,
    pub heuristic_comparison_count: u64,
    pub same_as_heuristic_rate: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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
    pub discard_sequence: Vec<f32>,
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
    pub step_reward: f32,
    pub terminal_reward: f32,
    pub shanten_before: Option<i32>,
    pub shanten_after: Option<i32>,
    pub fan_potential_before: Option<i32>,
    pub fan_potential_after: Option<i32>,
    pub global_tile_planes: Option<Vec<f32>>,
    pub global_scalar_features: Option<Vec<f32>>,
    pub done: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ArenaRunOutput {
    pub reports: Vec<ArenaMatchReport>,
    pub trajectories: Vec<ArenaTrajectoryRow>,
}

struct ArenaCompletedMatch {
    report: ArenaMatchReport,
    trajectories: Vec<ArenaTrajectoryRow>,
}

fn default_max_actions_per_match() -> usize {
    2400
}

fn default_policy_temperature() -> f32 {
    1.0
}

fn default_seat_rotation() -> ArenaSeatRotation {
    ArenaSeatRotation::Fixed
}

impl ArenaBotPolicyConfig {
    pub fn heuristic() -> Self {
        Self {
            id: "heuristic".to_string(),
            mode: ArenaPolicyMode::Heuristic,
            model_path: None,
            sample_actions: false,
            temperature: 1.0,
            record_heuristic_comparison: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ArenaMatchAccumulator {
    pub seats: Vec<ArenaSeatMetrics>,
}

impl ArenaMatchAccumulator {
    pub fn new(config: &ArenaConfig, match_index: usize) -> Self {
        Self {
            seats: (0..4)
                .map(|seat_index| ArenaSeatMetrics {
                    seat_index,
                    policy_id: policy_for_match_seat(config, match_index, seat_index).id,
                    ..ArenaSeatMetrics::default()
                })
                .collect(),
        }
    }

    pub(crate) fn record_decision(
        &mut self,
        seat_index: usize,
        action_type: &str,
        latency_ms: u128,
        telemetry: Option<&super::policy::BotPolicyDecisionTelemetry>,
    ) {
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
            if let Some(telemetry) = telemetry {
                metrics.model_loaded |= telemetry.model_loaded;
                if telemetry.used_neural_action {
                    metrics.neural_action_count += 1;
                }
                if telemetry.used_fallback {
                    metrics.fallback_count += 1;
                }
                if let Some(same_as_heuristic) = telemetry.same_as_heuristic {
                    metrics.heuristic_comparison_count += 1;
                    if same_as_heuristic {
                        metrics.same_as_heuristic_count += 1;
                    }
                    metrics.same_as_heuristic_rate = metrics.same_as_heuristic_count as f64
                        / metrics.heuristic_comparison_count as f64;
                }
            }
        }
    }

    pub fn record_tenpai_metrics(&mut self, room: &RoomState) {
        let Some(round) = &room.round_state else {
            return;
        };
        for player in &round.players {
            if let Some(metrics) = self.seats.get_mut(player.seat) {
                metrics.final_tenpai = player_is_tenpai(player);
                if metrics.final_tenpai && metrics.first_tenpai_turn.is_none() {
                    metrics.first_tenpai_turn = Some(metrics.discard_count + 1);
                }
            }
        }
    }
}

fn player_is_tenpai(player: &PlayerRoundState) -> bool {
    let concealed_tile_keys = player
        .concealed_tiles
        .iter()
        .map(|tile| tile.tile_key.clone())
        .collect::<Vec<_>>();
    is_tenpai_hand_with_melds(&concealed_tile_keys, &player.melds)
}

pub fn arena_room(table_code: &str) -> RoomState {
    RoomState {
        table_code: table_code.to_string(),
        phase: "waiting".to_string(),
        mode: "normal".to_string(),
        owner_user_id: None,
        multiplier: 1,
        seats: (0..4)
            .map(|seat_index| SeatState {
                seat_index,
                user_id: None,
                nickname: Some(format!("Arena Bot {seat_index}")),
                points: None,
                title: None,
                connected: true,
                is_bot: true,
                seat_type: "bot".to_string(),
                bot_persona: None,
                bot_aggression: None,
                disconnect_deadline_at: None,
                consecutive_timeout_auto_response_count: 0,
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
    accumulator.record_tenpai_metrics(room);
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
    run_arena_with_progress(config, include_trajectories, |_| {})
}

pub fn run_arena_with_progress(
    config: &ArenaConfig,
    include_trajectories: bool,
    mut on_match_complete: impl FnMut(&ArenaMatchReport),
) -> Result<ArenaRunOutput, String> {
    if config.policies.is_empty() {
        return Err("arena config requires at least one policy".to_string());
    }

    let mut output = ArenaRunOutput::default();
    for match_index in 0..config.matches {
        let completed_match = run_arena_match(config, match_index, include_trajectories)?;
        on_match_complete(&completed_match.report);
        output.trajectories.extend(completed_match.trajectories);
        output.reports.push(completed_match.report);
    }
    Ok(output)
}

pub fn run_arena_parallel_with_progress(
    config: &ArenaConfig,
    include_trajectories: bool,
    worker_count: usize,
    mut on_match_complete: impl FnMut(&ArenaMatchReport),
) -> Result<ArenaRunOutput, String> {
    if config.policies.is_empty() {
        return Err("arena config requires at least one policy".to_string());
    }
    if config.matches == 0 {
        return Ok(ArenaRunOutput::default());
    }
    let worker_count = worker_count.max(1).min(config.matches);
    if worker_count == 1 {
        return run_arena_with_progress(config, include_trajectories, on_match_complete);
    }

    let config = Arc::new(config.clone());
    let next_match = Arc::new(AtomicUsize::new(0));
    let cancel = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = mpsc::channel::<Result<ArenaCompletedMatch, String>>();

    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            let config = Arc::clone(&config);
            let next_match = Arc::clone(&next_match);
            let cancel = Arc::clone(&cancel);
            let sender = sender.clone();
            scope.spawn(move || {
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let match_index = next_match.fetch_add(1, Ordering::Relaxed);
                    if match_index >= config.matches {
                        break;
                    }
                    match run_arena_match(&config, match_index, include_trajectories) {
                        Ok(completed_match) => {
                            if sender.send(Ok(completed_match)).is_err() {
                                break;
                            }
                        }
                        Err(reason) => {
                            cancel.store(true, Ordering::Relaxed);
                            let _ = sender.send(Err(reason));
                            break;
                        }
                    }
                }
            });
        }
        drop(sender);

        let mut reports = Vec::with_capacity(config.matches);
        let mut trajectories_by_match = Vec::with_capacity(config.matches);
        let mut completed_matches = 0_usize;
        for received in receiver {
            let completed_match = match received {
                Ok(completed_match) => completed_match,
                Err(reason) => {
                    cancel.store(true, Ordering::Relaxed);
                    return Err(reason);
                }
            };
            completed_matches += 1;
            on_match_complete(&completed_match.report);
            let match_index = completed_match.report.match_index;
            reports.push(completed_match.report);
            trajectories_by_match.push((match_index, completed_match.trajectories));
            if completed_matches == config.matches {
                break;
            }
        }

        reports.sort_by_key(|report| report.match_index);
        trajectories_by_match.sort_by_key(|(match_index, _)| *match_index);
        Ok(ArenaRunOutput {
            reports,
            trajectories: trajectories_by_match
                .into_iter()
                .flat_map(|(_, rows)| rows)
                .collect(),
        })
    })
}

fn run_arena_match(
    config: &ArenaConfig,
    match_index: usize,
    include_trajectories: bool,
) -> Result<ArenaCompletedMatch, String> {
    let seed = config.seed.wrapping_add(match_index as u64);
    let match_id = format!("arena-{seed}-{match_index}");
    let mut room = arena_room(&format!("ARENA{match_index:04}"));
    start_match_in_room_state(&mut room, 0, seed)?;
    let mut accumulator = ArenaMatchAccumulator::new(config, match_index);
    let mut action_count = 0_usize;
    let mut trajectories = Vec::new();
    let mut rollout_rng = StdRng::seed_from_u64(seed ^ 0xA17E_5EED);

    while room.phase == "playing" && action_count < config.max_actions_per_match {
        let started = std::time::Instant::now();
        let trace = {
            let rollout_rng = if include_trajectories {
                Some(&mut rollout_rng)
            } else {
                None
            };
            next_bot_decision_trace_in_room_state_with_policy_resolver(
                &room,
                &|seat| policy_for_match_seat(config, match_index, seat),
                rollout_rng,
            )?
        };
        let action = if let Some(trace) = trace.as_ref() {
            trace.action.clone()
        } else {
            let Some(action) =
                next_bot_action_in_room_state_with_policy_resolver(&room, &|seat| {
                    policy_for_match_seat(config, match_index, seat)
                })?
            else {
                break;
            };
            action
        };
        let elapsed_ms = started.elapsed().as_millis();
        let action_seat = action.seat_index;
        let action_type = action.action_type.clone();
        let reward_before = trace
            .as_ref()
            .and_then(|trace| reward_snapshot_from_context(&trace.context));
        let handled = try_handle_player_action_in_room_state(
            &mut room,
            action.seat_index,
            &action.action_type,
            &action.tile_ids,
        )?;
        match handled {
            Some(Ok(_)) => {
                let telemetry = trace.as_ref().map(|trace| &trace.telemetry);
                accumulator.record_decision(action_seat, &action_type, elapsed_ms, telemetry);
                accumulator.record_tenpai_metrics(&room);
                if let Some(trace) = trace.as_ref() {
                    let policy = policy_for_match_seat(config, match_index, action_seat);
                    let reward_after = reward_snapshot_from_room(&room, action_seat);
                    if let Some(mut row) = trajectory_row_from_trace(
                        &match_id,
                        trajectories.len() as u64,
                        &policy,
                        trace,
                    ) {
                        apply_shaping_reward(&mut row, reward_before, reward_after);
                        trajectories.push(row);
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
    assign_terminal_rewards(&mut trajectories, &report);
    Ok(ArenaCompletedMatch {
        report,
        trajectories,
    })
}

fn policy_for_match_seat(
    config: &ArenaConfig,
    match_index: usize,
    seat_index: usize,
) -> ArenaBotPolicyConfig {
    let policy_count = config.policies.len();
    if policy_count == 0 {
        return ArenaBotPolicyConfig::heuristic();
    }
    let rotation = match config.seat_rotation {
        ArenaSeatRotation::Fixed => 0,
        ArenaSeatRotation::Cyclic => config.seat_rotation_offset.wrapping_add(match_index),
    };
    let policy_index = seat_index.wrapping_add(rotation) % policy_count;
    let mut policy = config
        .policies
        .get(policy_index)
        .cloned()
        .unwrap_or_else(ArenaBotPolicyConfig::heuristic);
    policy.record_heuristic_comparison = config.record_heuristic_comparison;
    policy
}

fn trajectory_row_from_trace(
    match_id: &str,
    decision_index: u64,
    policy: &ArenaBotPolicyConfig,
    trace: &crate::rules::standard::automation::BotDecisionTrace,
) -> Option<ArenaTrajectoryRow> {
    let features = encode_bot_context_v2(&trace.context);
    let (action_head, action_index, action_semantic) =
        encode_action_for_trajectory(&trace.decision_kind, &trace.context, &trace.action)?;
    let (log_prob, value) = neural_policy_stats(
        policy,
        &trace.context,
        &features,
        &action_head,
        action_index,
        trace.neural_scores.as_ref(),
    )
    .unwrap_or((0.0, 0.0));
    Some(ArenaTrajectoryRow {
        schema_version: 1,
        match_id: match_id.to_string(),
        decision_index,
        seat_index: trace.action.seat_index,
        policy_id: policy.id.clone(),
        decision_kind: trace.decision_kind.clone(),
        tile_planes: features.tile_planes,
        scalar_features: features.scalar_features,
        discard_sequence: features.discard_sequence,
        discard_mask: features.discard_mask.to_vec(),
        claim_mask: features.claim_mask.to_vec(),
        self_kong_mask: features.self_kong_mask.to_vec(),
        hu_mask: features.hu_mask.to_vec(),
        action_head,
        action_index,
        action_semantic,
        log_prob,
        value,
        reward: 0.0,
        step_reward: 0.0,
        terminal_reward: 0.0,
        shanten_before: None,
        shanten_after: None,
        fan_potential_before: None,
        fan_potential_after: None,
        global_tile_planes: None,
        global_scalar_features: None,
        done: false,
    })
}

fn apply_shaping_reward(
    row: &mut ArenaTrajectoryRow,
    before: Option<RewardSnapshot>,
    after: Option<RewardSnapshot>,
) {
    row.shanten_before = before.map(|snapshot| snapshot.shanten);
    row.shanten_after = after.map(|snapshot| snapshot.shanten);
    row.fan_potential_before = before.map(|snapshot| snapshot.fan_potential);
    row.fan_potential_after = after.map(|snapshot| snapshot.fan_potential);
    if let (Some(before), Some(after)) = (before, after) {
        row.step_reward = shaping_reward(before, after);
        row.reward = row.step_reward;
    }
}

fn neural_policy_stats(
    policy: &ArenaBotPolicyConfig,
    context: &BotContext,
    features: &super::features::BotFeaturesV2,
    action_head: &str,
    action_index: i64,
    trace_scores: Option<&NeuralDecisionScores>,
) -> Option<(f32, f32)> {
    if !matches!(policy.mode, ArenaPolicyMode::Neural) {
        return None;
    }
    let computed_scores;
    let scores = match trace_scores {
        Some(scores) => scores,
        None => {
            let model_path = policy.model_path.as_deref().map(std::path::Path::new);
            computed_scores = neural_decision_scores_for_model_path(context, model_path)?;
            &computed_scores
        }
    };
    let log_prob = match action_head {
        "discard" => {
            let discard_logits = risk_adjusted_discard_logits(scores);
            masked_log_prob(
                &discard_logits,
                &features.discard_mask,
                action_index as usize,
            )?
        }
        "claim" => masked_log_prob(
            &scores.claim_logits,
            &features.claim_mask,
            action_index as usize,
        )?,
        "self_kong" => masked_log_prob(
            &scores.self_kong_logits,
            &features.self_kong_mask,
            action_index as usize,
        )?,
        "hu" => masked_log_prob(&scores.hu_logits, &features.hu_mask, action_index as usize)?,
        _ => return None,
    };
    Some((log_prob, scores.value))
}

fn masked_log_prob<const N: usize>(
    logits: &[f32; N],
    mask: &[bool; N],
    action_index: usize,
) -> Option<f32> {
    if action_index >= N || !mask[action_index] || !logits[action_index].is_finite() {
        return None;
    }
    let max_logit = logits
        .iter()
        .zip(mask.iter())
        .filter_map(|(logit, allowed)| allowed.then_some(*logit))
        .filter(|logit| logit.is_finite())
        .max_by(f32::total_cmp)?;
    let sum_exp = logits
        .iter()
        .zip(mask.iter())
        .filter_map(|(logit, allowed)| {
            (*allowed && logit.is_finite()).then_some((*logit - max_logit).exp())
        })
        .sum::<f32>();
    if sum_exp <= 0.0 || !sum_exp.is_finite() {
        return None;
    }
    Some(logits[action_index] - max_logit - sum_exp.ln())
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
        "hu" => Some(("hu".to_string(), 1, "hu".to_string())),
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
        row.terminal_reward = 0.0;
        row.reward = row.step_reward;
        row.done = false;
    }

    for seat in &report.seats {
        let terminal = terminal_reward_for_seat(seat);
        if let Some(row) = rows
            .iter_mut()
            .rev()
            .find(|row| row.seat_index == seat.seat_index)
        {
            row.terminal_reward = terminal;
            row.reward = row.step_reward + terminal;
            row.done = true;
        }
    }
}

fn terminal_reward_for_seat(seat: &ArenaSeatMetrics) -> f32 {
    let score_reward = seat.score_delta as f32 / 100.0;
    let win_bonus = if seat.wins > 0 { 1.0 } else { 0.0 };
    let deal_in_penalty = if seat.dealt_in > 0 { -1.5 } else { 0.0 };
    score_reward + win_bonus + deal_in_penalty
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::action_space::{CLAIM_ACTION_COUNT, SELF_KONG_ACTION_COUNT, TILE_KIND_COUNT};
    use crate::bot::context::{BotPlayerContext, BotTileView, tile_counts34};
    use crate::bot::neural::NeuralDecisionScores;
    use crate::core::state::{PlayerRoundState, RoundState};
    use crate::core::tile::Tile;
    use crate::rules::standard::automation::BotDecisionTrace;

    #[test]
    fn arena_config_parses_policy_ids() {
        let raw = r#"{
            "matches": 2,
            "seed": 20260429,
            "policies": [
                {"id":"heuristic","mode":"heuristic","model_path":null},
                {"id":"neural","mode":"neural","model_path":"backend/assets/models/mahjong_policy_net.onnx"}
            ]
        }"#;

        let config: ArenaConfig = serde_json::from_str(raw).expect("config");

        assert_eq!(config.matches, 2);
        assert_eq!(config.seed, 20260429);
        assert_eq!(config.max_actions_per_match, 2400);
        assert!(!config.report_trajectories);
        assert!(!config.record_heuristic_comparison);
        assert_eq!(config.policies[1].id, "neural");
        assert_eq!(config.policies[1].mode, ArenaPolicyMode::Neural);
        assert!(!config.policies[0].sample_actions);
        assert_eq!(config.policies[0].temperature, 1.0);
    }

    #[test]
    fn arena_config_parses_heuristic_comparison_toggle() {
        let raw = r#"{
            "matches": 1,
            "seed": 20260429,
            "record_heuristic_comparison": true,
            "policies": [
                {"id":"neural","mode":"neural","model_path":"backend/assets/models/mahjong_policy_net.onnx"}
            ]
        }"#;

        let config: ArenaConfig = serde_json::from_str(raw).expect("config");

        assert!(config.record_heuristic_comparison);
    }

    #[test]
    fn arena_config_parses_stochastic_neural_rollout_policy() {
        let raw = r#"{
            "matches": 1,
            "seed": 20260429,
            "policies": [
                {
                    "id":"learner",
                    "mode":"neural",
                    "model_path":"backend/assets/models/mahjong_policy_net.onnx",
                    "sample_actions":true,
                    "temperature":0.8
                }
            ]
        }"#;

        let config: ArenaConfig = serde_json::from_str(raw).expect("config");

        assert_eq!(config.policies[0].id, "learner");
        assert!(config.policies[0].sample_actions);
        assert_eq!(config.policies[0].temperature, 0.8);
    }

    #[test]
    fn cyclic_seat_rotation_assigns_each_policy_to_each_seat_once_per_cycle() {
        let config = ArenaConfig {
            matches: 4,
            seed: 7,
            max_actions_per_match: 10,
            report_trajectories: false,
            record_heuristic_comparison: true,
            seat_rotation: ArenaSeatRotation::Cyclic,
            seat_rotation_offset: 0,
            policies: ["a", "b", "c", "d"]
                .into_iter()
                .map(|id| ArenaBotPolicyConfig {
                    id: id.to_string(),
                    mode: ArenaPolicyMode::Heuristic,
                    model_path: None,
                    sample_actions: false,
                    temperature: 1.0,
                    record_heuristic_comparison: false,
                })
                .collect(),
        };

        for seat_index in 0..4 {
            let mut policy_ids = (0..4)
                .map(|match_index| policy_for_match_seat(&config, match_index, seat_index).id)
                .collect::<Vec<_>>();
            policy_ids.sort();

            assert_eq!(policy_ids, vec!["a", "b", "c", "d"]);
        }
        assert!(policy_for_match_seat(&config, 0, 0).record_heuristic_comparison);
    }

    #[test]
    fn cyclic_seat_rotation_offset_continues_across_chunks() {
        let config = ArenaConfig {
            matches: 2,
            seed: 7,
            max_actions_per_match: 10,
            report_trajectories: false,
            record_heuristic_comparison: false,
            seat_rotation: ArenaSeatRotation::Cyclic,
            seat_rotation_offset: 3,
            policies: ["a", "b", "c", "d"]
                .into_iter()
                .map(|id| ArenaBotPolicyConfig {
                    id: id.to_string(),
                    mode: ArenaPolicyMode::Heuristic,
                    model_path: None,
                    sample_actions: false,
                    temperature: 1.0,
                    record_heuristic_comparison: false,
                })
                .collect(),
        };

        assert_eq!(policy_for_match_seat(&config, 0, 0).id, "d");
        assert_eq!(policy_for_match_seat(&config, 1, 0).id, "a");
    }

    #[test]
    fn arena_config_rejects_removed_policy_mode() {
        let raw = r#"{
            "matches": 2,
            "seed": 20260429,
            "policies": [
                {"id":"removed_policy","mode":"removed_policy","model_path":"backend/assets/models/mahjong_policy_net.onnx"}
            ]
        }"#;

        let parsed = serde_json::from_str::<ArenaConfig>(raw);

        assert!(parsed.is_err());
    }

    #[test]
    fn heuristic_policy_config_has_stable_defaults() {
        let config = ArenaBotPolicyConfig::heuristic();

        assert_eq!(config.id, "heuristic");
        assert_eq!(config.mode, ArenaPolicyMode::Heuristic);
        assert_eq!(config.model_path, None);
        assert!(!config.sample_actions);
        assert_eq!(config.temperature, 1.0);
    }

    #[test]
    fn arena_config_rejects_removed_policy_weight_field() {
        let removed_field = concat!("neural", "_weight");
        let raw = format!(
            r#"{{
            "matches": 2,
            "seed": 20260429,
            "policies": [
                {{"id":"neural","mode":"neural","{removed_field}":0,"model_path":"backend/assets/models/mahjong_policy_net.onnx"}}
            ]
        }}"#
        );

        let parsed = serde_json::from_str::<ArenaConfig>(&raw);

        assert!(parsed.is_err());
    }

    #[test]
    fn arena_room_creates_four_bot_seats() {
        let room = arena_room("AR01");

        assert_eq!(room.table_code, "AR01");
        assert_eq!(room.phase, "waiting");
        assert_eq!(room.seats.len(), 4);
        assert!(room.seats.iter().all(|seat| seat.is_bot));
    }

    #[test]
    fn accumulator_records_decision_counts() {
        let config = ArenaConfig {
            matches: 1,
            seed: 7,
            max_actions_per_match: 10,
            report_trajectories: false,
            record_heuristic_comparison: false,
            seat_rotation: ArenaSeatRotation::Fixed,
            seat_rotation_offset: 0,
            policies: vec![ArenaBotPolicyConfig::heuristic()],
        };
        let mut accumulator = ArenaMatchAccumulator::new(&config, 0);

        accumulator.record_decision(0, "discard", 3, None);
        accumulator.record_decision(0, "pung", 2, None);

        assert_eq!(accumulator.seats[0].decision_count, 2);
        assert_eq!(accumulator.seats[0].discard_count, 1);
        assert_eq!(accumulator.seats[0].claim_count, 1);
        assert_eq!(accumulator.seats[0].decision_latency_ms_sum, 5);
    }

    #[test]
    fn accumulator_records_policy_telemetry() {
        let config = ArenaConfig {
            matches: 1,
            seed: 7,
            max_actions_per_match: 10,
            report_trajectories: false,
            record_heuristic_comparison: false,
            seat_rotation: ArenaSeatRotation::Fixed,
            seat_rotation_offset: 0,
            policies: vec![ArenaBotPolicyConfig {
                id: "neural".to_string(),
                mode: ArenaPolicyMode::Neural,
                model_path: Some("missing.onnx".to_string()),
                sample_actions: false,
                temperature: 1.0,
                record_heuristic_comparison: false,
            }],
        };
        let mut accumulator = ArenaMatchAccumulator::new(&config, 0);

        let neural = crate::bot::policy::BotPolicyDecisionTelemetry {
            model_loaded: true,
            used_neural_action: true,
            used_fallback: false,
            same_as_heuristic: Some(true),
        };
        accumulator.record_decision(0, "discard", 3, Some(&neural));
        let fallback = crate::bot::policy::BotPolicyDecisionTelemetry {
            model_loaded: false,
            used_neural_action: false,
            used_fallback: true,
            same_as_heuristic: None,
        };
        accumulator.record_decision(0, "discard", 2, Some(&fallback));

        assert!(accumulator.seats[0].model_loaded);
        assert_eq!(accumulator.seats[0].neural_action_count, 1);
        assert_eq!(accumulator.seats[0].fallback_count, 1);
        assert_eq!(accumulator.seats[0].same_as_heuristic_count, 1);
        assert_eq!(accumulator.seats[0].heuristic_comparison_count, 1);
        assert_eq!(accumulator.seats[0].same_as_heuristic_rate, 1.0);
    }

    #[test]
    fn match_report_final_tenpai_uses_hand_shape_not_ready_hand_flag() {
        let config = ArenaConfig {
            matches: 1,
            seed: 7,
            max_actions_per_match: 10,
            report_trajectories: false,
            record_heuristic_comparison: false,
            seat_rotation: ArenaSeatRotation::Fixed,
            seat_rotation_offset: 0,
            policies: vec![ArenaBotPolicyConfig::heuristic()],
        };
        let mut room = RoomState {
            phase: "playing".to_string(),
            round_state: Some(RoundState {
                players: vec![PlayerRoundState {
                    seat: 0,
                    is_ready_hand: false,
                    concealed_tiles: tile_key_only_vec(&[
                        "w1", "w2", "w3", "w4", "w5", "w6", "t1", "t2", "t3", "b1", "b2", "b3",
                        "east",
                    ]),
                    ..PlayerRoundState::default()
                }],
                ..RoundState::default()
            }),
            ..RoomState::default()
        };

        let report =
            build_match_report(0, 7, &room, ArenaMatchAccumulator::new(&config, 0), 1, true);
        assert!(report.seats[0].final_tenpai);

        let non_tenpai_tiles = tile_key_only_vec(&[
            "w1", "w1", "w4", "w7", "t2", "t5", "t8", "b3", "b6", "b9", "east", "south", "red",
        ]);
        room.round_state
            .as_mut()
            .expect("round")
            .players
            .get_mut(0)
            .expect("player")
            .concealed_tiles = non_tenpai_tiles;
        let report =
            build_match_report(0, 7, &room, ArenaMatchAccumulator::new(&config, 0), 1, true);
        assert!(!report.seats[0].final_tenpai);
    }

    #[test]
    fn match_report_records_first_tenpai_turn_from_actual_hand_shape() {
        let config = ArenaConfig {
            matches: 1,
            seed: 7,
            max_actions_per_match: 10,
            report_trajectories: false,
            record_heuristic_comparison: false,
            seat_rotation: ArenaSeatRotation::Fixed,
            seat_rotation_offset: 0,
            policies: vec![ArenaBotPolicyConfig::heuristic()],
        };
        let mut accumulator = ArenaMatchAccumulator::new(&config, 0);
        accumulator.seats[0].discard_count = 3;
        let room = RoomState {
            phase: "playing".to_string(),
            round_state: Some(RoundState {
                players: vec![PlayerRoundState {
                    seat: 0,
                    is_ready_hand: false,
                    concealed_tiles: tile_key_only_vec(&[
                        "w1", "w2", "w3", "w4", "w5", "w6", "t1", "t2", "t3", "b1", "b2", "b3",
                        "east",
                    ]),
                    ..PlayerRoundState::default()
                }],
                ..RoundState::default()
            }),
            ..RoomState::default()
        };

        let report = build_match_report(0, 7, &room, accumulator, 1, true);

        assert_eq!(report.seats[0].first_tenpai_turn, Some(4));
        assert!(report.seats[0].final_tenpai);
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
            scalar_features: vec![0.0; 12],
            discard_sequence: vec![0.0; 32 * 40],
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
            step_reward: 0.0,
            terminal_reward: 0.0,
            shanten_before: None,
            shanten_after: None,
            fan_potential_before: None,
            fan_potential_after: None,
            global_tile_planes: None,
            global_scalar_features: None,
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

    #[test]
    fn trajectory_row_reuses_trace_neural_scores_for_policy_stats() {
        let context = bot_context_for_discards(&["w1", "w2"]);
        let mut discard_logits = [0.0_f32; TILE_KIND_COUNT];
        discard_logits[tile_index("w1").expect("w1 index")] = 2.0;
        let trace = BotDecisionTrace {
            decision_kind: "active_turn".to_string(),
            action: BotAction {
                seat_index: 0,
                action_type: "discard".to_string(),
                tile_ids: vec!["w1-0".to_string()],
            },
            context,
            telemetry: crate::bot::policy::BotPolicyDecisionTelemetry::default(),
            neural_scores: Some(NeuralDecisionScores {
                discard_logits,
                claim_logits: [0.0; CLAIM_ACTION_COUNT],
                self_kong_logits: [0.0; SELF_KONG_ACTION_COUNT],
                hu_logits: [0.0; 2],
                value: 0.75,
                risk_logits: [0.0; TILE_KIND_COUNT],
            }),
        };
        let policy = ArenaBotPolicyConfig {
            id: "learner".to_string(),
            mode: ArenaPolicyMode::Neural,
            model_path: Some("missing-model-for-trajectory-test.onnx".to_string()),
            sample_actions: true,
            temperature: 1.0,
            record_heuristic_comparison: false,
        };

        let row = trajectory_row_from_trace("arena-test", 0, &policy, &trace).expect("row");

        let expected = -(1.0_f32 + (-2.0_f32).exp()).ln();
        assert!((row.log_prob - expected).abs() < 0.0001);
        assert_eq!(row.value, 0.75);
    }

    #[test]
    fn trajectory_log_prob_uses_risk_adjusted_discard_logits() {
        let context = bot_context_for_discards(&["w1", "t1"]);
        let w1_index = tile_index("w1").expect("w1 index");
        let mut risk_logits = [-5.0_f32; TILE_KIND_COUNT];
        risk_logits[w1_index] = 5.0;
        let scores = NeuralDecisionScores {
            discard_logits: [0.0; TILE_KIND_COUNT],
            claim_logits: [0.0; CLAIM_ACTION_COUNT],
            self_kong_logits: [0.0; SELF_KONG_ACTION_COUNT],
            hu_logits: [0.0; 2],
            value: -8.0,
            risk_logits,
        };
        let trace = BotDecisionTrace {
            decision_kind: "active_turn".to_string(),
            action: BotAction {
                seat_index: 0,
                action_type: "discard".to_string(),
                tile_ids: vec!["w1-0".to_string()],
            },
            context,
            telemetry: crate::bot::policy::BotPolicyDecisionTelemetry::default(),
            neural_scores: Some(scores.clone()),
        };
        let policy = ArenaBotPolicyConfig {
            id: "learner".to_string(),
            mode: ArenaPolicyMode::Neural,
            model_path: Some("missing-model-for-trajectory-test.onnx".to_string()),
            sample_actions: true,
            temperature: 1.0,
            record_heuristic_comparison: false,
        };

        let row = trajectory_row_from_trace("arena-test", 0, &policy, &trace).expect("row");
        let features = encode_bot_context_v2(&trace.context);
        let adjusted = risk_adjusted_discard_logits(&scores);
        let expected = masked_log_prob(&adjusted, &features.discard_mask, w1_index).expect("prob");
        let raw_expected =
            masked_log_prob(&scores.discard_logits, &features.discard_mask, w1_index)
                .expect("raw prob");

        assert!((row.log_prob - expected).abs() < 0.0001);
        assert!((row.log_prob - raw_expected).abs() > 0.1);
    }

    #[test]
    fn terminal_rewards_mark_each_seat_last_row_done() {
        let mut rows = vec![
            trajectory_test_row(0, 0, 0.01),
            trajectory_test_row(1, 1, 0.02),
            trajectory_test_row(2, 0, 0.03),
            trajectory_test_row(3, 1, 0.04),
        ];
        let report = ArenaMatchReport {
            match_index: 0,
            seed: 7,
            completed: true,
            action_count: 4,
            seats: vec![
                ArenaSeatMetrics {
                    seat_index: 0,
                    score_delta: 20,
                    wins: 1,
                    dealt_in: 0,
                    ..ArenaSeatMetrics::default()
                },
                ArenaSeatMetrics {
                    seat_index: 1,
                    score_delta: -20,
                    wins: 0,
                    dealt_in: 1,
                    ..ArenaSeatMetrics::default()
                },
            ],
        };

        assign_terminal_rewards(&mut rows, &report);

        assert!(!rows[0].done);
        assert!(!rows[1].done);
        assert!(rows[2].done);
        assert!(rows[3].done);
        assert_eq!(rows[0].reward, 0.01);
        assert_eq!(rows[1].reward, 0.02);
        assert_eq!(rows[2].terminal_reward, 1.2);
        assert_eq!(rows[3].terminal_reward, -1.7);
        assert_eq!(rows[2].reward, 1.23);
        assert_eq!(rows[3].reward, -1.6600001);
    }

    #[test]
    fn shaping_reward_updates_step_reward_and_diagnostics() {
        let mut row = trajectory_test_row(0, 0, 0.0);
        let before = RewardSnapshot {
            shanten: 2,
            fan_potential: 1,
        };
        let after = RewardSnapshot {
            shanten: 1,
            fan_potential: 2,
        };

        apply_shaping_reward(&mut row, Some(before), Some(after));

        assert!(row.step_reward > 0.0);
        assert_eq!(row.reward, row.step_reward);
        assert_eq!(row.shanten_before, Some(2));
        assert_eq!(row.shanten_after, Some(1));
        assert_eq!(row.fan_potential_before, Some(1));
        assert_eq!(row.fan_potential_after, Some(2));

        let mut worse_row = trajectory_test_row(1, 0, 0.0);
        apply_shaping_reward(&mut worse_row, Some(after), Some(before));

        assert!(worse_row.step_reward < 0.0);
    }

    fn trajectory_test_row(
        decision_index: u64,
        seat_index: usize,
        step_reward: f32,
    ) -> ArenaTrajectoryRow {
        ArenaTrajectoryRow {
            schema_version: 1,
            match_id: "arena-1".to_string(),
            decision_index,
            seat_index,
            policy_id: "heuristic".to_string(),
            decision_kind: "active_turn".to_string(),
            tile_planes: vec![0.0; 340],
            scalar_features: vec![0.0; 12],
            discard_sequence: vec![0.0; 32 * 40],
            discard_mask: vec![true; TILE_KIND_COUNT],
            claim_mask: vec![true; CLAIM_ACTION_COUNT],
            self_kong_mask: vec![true; SELF_KONG_ACTION_COUNT],
            hu_mask: vec![true, false],
            action_head: "discard".to_string(),
            action_index: 0,
            action_semantic: "discard:w1".to_string(),
            log_prob: 0.0,
            value: 0.0,
            reward: step_reward,
            step_reward,
            terminal_reward: 0.0,
            shanten_before: None,
            shanten_after: None,
            fan_potential_before: None,
            fan_potential_after: None,
            global_tile_planes: None,
            global_scalar_features: None,
            done: false,
        }
    }

    fn bot_context_for_discards(tile_keys: &[&str]) -> BotContext {
        let concealed_tiles = tile_keys
            .iter()
            .enumerate()
            .map(|(index, tile_key)| BotTileView {
                tile_id: format!("{tile_key}-{index}"),
                tile_key: (*tile_key).to_string(),
                is_flower: false,
            })
            .collect::<Vec<_>>();
        BotContext {
            seat_index: 0,
            seat_count: 4,
            dealer_seat: 0,
            round_wind: Some("east".to_string()),
            cumulative_scores: vec![0, 0, 0, 0],
            wall_tiles_remaining: 18,
            visible_tile_keys: Vec::new(),
            opponent_discards_by_seat: vec![vec![], vec![], vec![], vec![]],
            opponent_melds_by_seat: vec![vec![], vec![], vec![], vec![]],
            discard_history: Vec::new(),
            kong_entries: Vec::new(),
            player: BotPlayerContext {
                concealed_tiles,
                concealed_tile_counts: tile_counts34(tile_keys.iter().copied()),
                meld_tile_key_groups: Vec::new(),
                flower_count: 0,
            },
            restricted_discard_tile_key: None,
            drawn_tile_id: Some("w2-1".to_string()),
            self_kong_candidates: Vec::new(),
            claim_options: Vec::new(),
            last_discard_tile_key: None,
            add_kong_risk_tiles: std::collections::HashSet::new(),
        }
    }

    fn tile_key_only_vec(tile_keys: &[&str]) -> Vec<Tile> {
        tile_keys
            .iter()
            .map(|tile_key| Tile::tile_key_only(tile_key))
            .collect()
    }
}
