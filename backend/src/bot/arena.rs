use super::{
    action_space::{claim_action_index, self_kong_action_index, tile_index},
    context::{BotAction, BotContext, BotSelfKongKind},
    features::{encode_bot_context_v2, encode_global_features_v2},
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
    flow::{record_continue_action_in_room_state, start_match_in_room_state},
    ready_hand::is_tenpai_hand_with_melds,
};
use rand::{SeedableRng, rngs::StdRng};
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};

fn bot_context_for_discards(tile_keys: &[&str]) -> BotContext {
    let concealed_tiles = tile_keys
        .iter()
        .enumerate()
        .map(|(index, tile_key)| super::context::BotTileView {
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
        minimum_hu_fan: crate::core::state::room::default_minimum_hu_fan(),
        cumulative_scores: vec![0, 0, 0, 0],
        wall_tiles_remaining: 18,
        visible_tile_keys: Vec::new(),
        opponent_discards_by_seat: vec![vec![], vec![], vec![], vec![]],
        opponent_melds_by_seat: vec![vec![], vec![], vec![], vec![]],
        discard_history: Vec::new(),
        kong_entries: Vec::new(),
        player: super::context::BotPlayerContext {
            concealed_tiles,
            concealed_tile_counts: tile_counts_for_keys(tile_keys),
            meld_tile_key_groups: Vec::new(),
            flower_count: 0,
        },
        restricted_discard_tile_key: None,
        drawn_tile_id: None,
        self_kong_candidates: Vec::new(),
        claim_options: Vec::new(),
        last_discard_tile_key: None,
        add_kong_risk_tiles: std::collections::HashSet::new(),
    }
}

fn tile_counts_for_keys(tile_keys: &[&str]) -> super::context::TileCounts {
    let mut counts = [0_u8; super::context::TILE_KIND_COUNT];
    for tile_key in tile_keys {
        if let Some(index) = tile_index(tile_key) {
            counts[index] = counts[index].saturating_add(1);
        }
    }
    counts
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ArenaBotPolicyConfig {
    pub id: String,
    pub model_path: Option<String>,
    #[serde(default)]
    pub sample_actions: bool,
    #[serde(default = "default_policy_temperature")]
    pub temperature: f32,
    #[serde(default = "default_discard_base_risk_weight")]
    pub discard_base_risk_weight: f32,
    #[serde(default = "default_discard_value_risk_range")]
    pub discard_value_risk_range: f32,
    #[serde(default = "default_discard_min_risk_weight")]
    pub discard_min_risk_weight: f32,
    #[serde(default = "default_discard_max_risk_weight")]
    pub discard_max_risk_weight: f32,
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
    pub neural_action_count: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ArenaMatchReport {
    pub match_index: usize,
    pub seed: u64,
    pub completed: bool,
    pub action_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_initial_seat: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_final_score: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_deal_in_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_win_count: Option<u64>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EvaluationReplicaSpec {
    replica_index: usize,
    subject_index: usize,
    match_index: usize,
    seed: u64,
}

fn default_policy_temperature() -> f32 {
    1.0
}

fn default_discard_base_risk_weight() -> f32 {
    0.90
}

fn default_discard_value_risk_range() -> f32 {
    0.55
}

fn default_discard_min_risk_weight() -> f32 {
    0.25
}

fn default_discard_max_risk_weight() -> f32 {
    1.45
}

#[derive(Clone, Debug, Default)]
pub struct ArenaMatchAccumulator {
    pub seats: Vec<ArenaSeatMetrics>,
}

impl ArenaMatchAccumulator {
    pub fn new_with_policies(policies_by_seat: &[ArenaBotPolicyConfig]) -> Self {
        Self {
            seats: (0..4)
                .map(|seat_index| ArenaSeatMetrics {
                    seat_index,
                    policy_id: policies_by_seat
                        .get(seat_index)
                        .map(|policy| policy.id.clone())
                        .unwrap_or_else(|| format!("seat-{seat_index}")),
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

fn arena_identity_index_for_seat(room: &RoomState, seat_index: usize) -> usize {
    let Some(nickname) = room
        .seats
        .iter()
        .find(|seat| seat.seat_index == seat_index)
        .and_then(|seat| seat.nickname.as_deref())
    else {
        return seat_index;
    };
    nickname
        .strip_prefix("Arena Bot ")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|index| *index < 4)
        .unwrap_or(seat_index)
}

fn record_evaluation_decision(
    accumulator: &mut ArenaMatchAccumulator,
    room: &RoomState,
    seat_index: usize,
    action_type: &str,
    latency_ms: u128,
    telemetry: Option<&super::policy::BotPolicyDecisionTelemetry>,
) {
    let identity_index = arena_identity_index_for_seat(room, seat_index);
    accumulator.record_decision(identity_index, action_type, latency_ms, telemetry);
}

fn record_evaluation_tenpai_metrics(accumulator: &mut ArenaMatchAccumulator, room: &RoomState) {
    let Some(round) = &room.round_state else {
        return;
    };
    for player in &round.players {
        let identity_index = arena_identity_index_for_seat(room, player.seat);
        if let Some(metrics) = accumulator.seats.get_mut(identity_index) {
            metrics.final_tenpai = player_is_tenpai(player);
            if metrics.final_tenpai && metrics.first_tenpai_turn.is_none() {
                metrics.first_tenpai_turn = Some(metrics.discard_count + 1);
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
        minimum_hu_fan: crate::core::state::room::default_minimum_hu_fan(),
        dealer_repeat_enabled: false,
        dealer_double_enabled: false,
        ready_hand_enabled: true,
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

pub fn run_evaluation_arena(
    config: &crate::evaluation::EvaluationArenaConfig,
    include_trajectories: bool,
) -> Result<ArenaRunOutput, String> {
    run_evaluation_arena_with_jobs(config, include_trajectories, 1)
}

pub fn run_evaluation_arena_with_jobs(
    config: &crate::evaluation::EvaluationArenaConfig,
    include_trajectories: bool,
    worker_count: usize,
) -> Result<ArenaRunOutput, String> {
    config.validate()?;
    validate_evaluation_policy_models(config)?;

    let replicas = evaluation_replica_specs(config);
    if replicas.is_empty() {
        return Ok(ArenaRunOutput::default());
    }
    if worker_count <= 1 || replicas.len() == 1 {
        return run_evaluation_replicas_serial(config, include_trajectories, &replicas);
    }
    run_evaluation_replicas_parallel(config, include_trajectories, &replicas, worker_count)
}

fn evaluation_replica_specs(
    config: &crate::evaluation::EvaluationArenaConfig,
) -> Vec<EvaluationReplicaSpec> {
    let mut replicas = Vec::with_capacity(config.subjects.len() * config.matches);
    for subject_index in 0..config.subjects.len() {
        for match_index in 0..config.matches {
            replicas.push(EvaluationReplicaSpec {
                replica_index: replicas.len(),
                subject_index,
                match_index,
                seed: config.seed.wrapping_add(match_index as u64),
            });
        }
    }
    replicas
}

fn run_evaluation_replicas_serial(
    config: &crate::evaluation::EvaluationArenaConfig,
    include_trajectories: bool,
    replicas: &[EvaluationReplicaSpec],
) -> Result<ArenaRunOutput, String> {
    let mut output = ArenaRunOutput::default();
    for replica in replicas {
        let subject = config
            .subjects
            .get(replica.subject_index)
            .expect("replica subject index is valid");
        let completed_match = run_evaluation_arena_match(
            config,
            subject,
            replica.replica_index,
            replica.match_index,
            replica.seed,
            include_trajectories,
        )?;
        output.trajectories.extend(completed_match.trajectories);
        output.reports.push(completed_match.report);
    }
    Ok(output)
}

fn run_evaluation_replicas_parallel(
    config: &crate::evaluation::EvaluationArenaConfig,
    include_trajectories: bool,
    replicas: &[EvaluationReplicaSpec],
    worker_count: usize,
) -> Result<ArenaRunOutput, String> {
    let config = Arc::new(config.clone());
    let replicas = Arc::new(replicas.to_vec());
    let worker_count = worker_count.max(1).min(replicas.len());
    let next_replica = Arc::new(AtomicUsize::new(0));
    let cancel = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = mpsc::channel::<Result<(usize, ArenaCompletedMatch), String>>();

    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            let config = Arc::clone(&config);
            let replicas = Arc::clone(&replicas);
            let next_replica = Arc::clone(&next_replica);
            let cancel = Arc::clone(&cancel);
            let sender = sender.clone();
            scope.spawn(move || {
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let replica_offset = next_replica.fetch_add(1, Ordering::Relaxed);
                    let Some(replica) = replicas.get(replica_offset).copied() else {
                        break;
                    };
                    let subject = config
                        .subjects
                        .get(replica.subject_index)
                        .expect("replica subject index is valid");
                    let result = run_evaluation_arena_match(
                        &config,
                        subject,
                        replica.replica_index,
                        replica.match_index,
                        replica.seed,
                        include_trajectories,
                    )
                    .map(|completed_match| (replica_offset, completed_match));
                    if result.is_err() {
                        cancel.store(true, Ordering::Relaxed);
                    }
                    if sender.send(result).is_err() {
                        break;
                    }
                }
            });
        }
        drop(sender);

        let mut completed = Vec::with_capacity(replicas.len());
        for received in receiver {
            match received {
                Ok(row) => completed.push(row),
                Err(reason) => {
                    cancel.store(true, Ordering::Relaxed);
                    return Err(reason);
                }
            }
            if completed.len() == replicas.len() {
                break;
            }
        }
        completed.sort_by_key(|(replica_offset, _)| *replica_offset);
        Ok(ArenaRunOutput {
            reports: completed
                .iter()
                .map(|(_, completed_match)| completed_match.report.clone())
                .collect(),
            trajectories: completed
                .into_iter()
                .flat_map(|(_, completed_match)| completed_match.trajectories)
                .collect(),
        })
    })
}

fn run_evaluation_arena_match(
    config: &crate::evaluation::EvaluationArenaConfig,
    subject: &crate::evaluation::EvaluationSubjectPolicyConfig,
    replica_index: usize,
    match_index: usize,
    seed: u64,
    include_trajectories: bool,
) -> Result<ArenaCompletedMatch, String> {
    let match_id = format!("evaluation-{seed}-{replica_index}");
    let mut room = arena_room(&format!("EVAL{replica_index:04}"));
    crate::evaluation::apply_evaluation_rules(&mut room);
    start_match_in_room_state(
        &mut room,
        crate::evaluation::EVALUATION_INITIAL_SUBJECT_SEAT,
        seed,
    )?;
    let initial_policies = evaluation_policies_by_current_seat(subject, &config.opponents);
    let mut accumulator = ArenaMatchAccumulator::new_with_policies(&initial_policies);
    let mut action_count = 0_usize;
    let mut trajectories = Vec::new();
    let mut rollout_rng = StdRng::seed_from_u64(seed ^ 0xA17E_5EED);
    let mut timing_inference_ns = 0_u128;
    let mut timing_game_ns = 0_u128;
    let mut timing_trajectory_ns = 0_u128;

    crate::bot::policy::reset_timing_detail();

    while action_count < config.max_actions_per_match {
        if room.phase == "settlement" {
            let confirmed_seat = crate::evaluation::EVALUATION_INITIAL_SUBJECT_SEAT;
            if record_continue_action_in_room_state(&mut room, confirmed_seat, "start_next_round")
                .is_err()
            {
                break;
            }
            continue;
        }
        if room.phase != "playing" {
            break;
        }

        let started = std::time::Instant::now();
        let _inference_start = std::time::Instant::now();
        let trace = next_bot_decision_trace_in_room_state_with_policy_resolver(
            &room,
            &|seat| evaluation_policy_for_current_seat(&room, subject, &config.opponents, seat),
            Some(&mut rollout_rng),
        )?;
        timing_inference_ns += _inference_start.elapsed().as_nanos();
        let action = if let Some(trace) = trace.as_ref() {
            trace.action.clone()
        } else {
            let Some(action) =
                next_bot_action_in_room_state_with_policy_resolver(&room, &|seat| {
                    evaluation_policy_for_current_seat(&room, subject, &config.opponents, seat)
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
        let _game_start = std::time::Instant::now();
        let handled = try_handle_player_action_in_room_state(
            &mut room,
            action.seat_index,
            &action.action_type,
            &action.tile_ids,
        )?;
        timing_game_ns += _game_start.elapsed().as_nanos();
        match handled {
            Some(Ok(_)) => {
                let telemetry = trace.as_ref().map(|trace| &trace.telemetry);
                record_evaluation_decision(
                    &mut accumulator,
                    &room,
                    action_seat,
                    &action_type,
                    elapsed_ms,
                    telemetry,
                );
                record_evaluation_tenpai_metrics(&mut accumulator, &room);
                let _traj_start = std::time::Instant::now();
                if let (true, Some(trace)) = (include_trajectories, trace.as_ref()) {
                    let policy = evaluation_policy_for_current_seat(
                        &room,
                        subject,
                        &config.opponents,
                        action_seat,
                    );
                    let reward_after = reward_snapshot_from_room(&room, action_seat);
                    if let Some(mut row) = trajectory_row_from_trace_with_state(
                        &match_id,
                        trajectories.len() as u64,
                        &policy,
                        trace,
                        &room,
                    ) {
                        apply_shaping_reward(&mut row, reward_before, reward_after);
                        trajectories.push(row);
                    }
                }
                timing_trajectory_ns += _traj_start.elapsed().as_nanos();
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

    if action_count > 0 {
        let total_ns = timing_inference_ns + timing_game_ns + timing_trajectory_ns;
        eprintln!(
            "[timing] match={match_index} actions={action_count}  \
             inference={:.1}ms ({:.0}%)  game={:.1}ms ({:.0}%)  trajectory={:.1}ms ({:.0}%)",
            timing_inference_ns as f64 / 1_000_000.0,
            timing_inference_ns as f64 / total_ns as f64 * 100.0,
            timing_game_ns as f64 / 1_000_000.0,
            timing_game_ns as f64 / total_ns as f64 * 100.0,
            timing_trajectory_ns as f64 / 1_000_000.0,
            timing_trajectory_ns as f64 / total_ns as f64 * 100.0,
        );
        crate::bot::policy::print_timing_detail();
    }

    let report = build_evaluation_match_report(
        match_index,
        seed,
        &room,
        accumulator,
        action_count,
        action_count < config.max_actions_per_match,
        subject,
    );
    assign_terminal_rewards(&mut trajectories, &report);
    Ok(ArenaCompletedMatch {
        report,
        trajectories,
    })
}

fn evaluation_policies_by_current_seat(
    subject: &crate::evaluation::EvaluationSubjectPolicyConfig,
    opponents: &[ArenaBotPolicyConfig],
) -> Vec<ArenaBotPolicyConfig> {
    let mut policies = Vec::with_capacity(4);
    policies.push(subject.policy.clone());
    policies.extend(opponents.iter().take(3).cloned());
    policies
}

fn evaluation_policy_for_current_seat(
    room: &RoomState,
    subject: &crate::evaluation::EvaluationSubjectPolicyConfig,
    opponents: &[ArenaBotPolicyConfig],
    seat_index: usize,
) -> ArenaBotPolicyConfig {
    let seat_name = room
        .seats
        .iter()
        .find(|seat| seat.seat_index == seat_index)
        .and_then(|seat| seat.nickname.as_deref())
        .unwrap_or_default();
    if seat_name == "Arena Bot 0" {
        return subject.policy.clone();
    }
    for opponent_index in 1..4 {
        if seat_name == format!("Arena Bot {opponent_index}") {
            return opponents
                .get(opponent_index - 1)
                .cloned()
                .expect("evaluation has exactly three opponents");
        }
    }
    evaluation_policies_by_current_seat(subject, opponents)
        .get(seat_index)
        .cloned()
        .unwrap_or_else(|| subject.policy.clone())
}

fn apply_subject_fields_to_report(
    report: &mut ArenaMatchReport,
    _room: &RoomState,
    subject: &crate::evaluation::EvaluationSubjectPolicyConfig,
) {
    let subject_metrics = report
        .seats
        .iter()
        .find(|seat| seat.policy_id == subject.policy.id)
        .cloned();
    report.subject_id = Some(subject.policy.id.clone());
    report.subject_display_name = Some(subject.display_name.clone());
    report.subject_initial_seat = Some(crate::evaluation::EVALUATION_INITIAL_SUBJECT_SEAT);
    report.subject_final_score = subject_metrics.as_ref().map(|metrics| metrics.score_delta);
    report.subject_deal_in_count = subject_metrics.as_ref().map(|metrics| metrics.dealt_in);
    report.subject_win_count = subject_metrics.as_ref().map(|metrics| metrics.wins);
}

fn build_evaluation_match_report(
    match_index: usize,
    seed: u64,
    room: &RoomState,
    mut accumulator: ArenaMatchAccumulator,
    action_count: usize,
    completed: bool,
    subject: &crate::evaluation::EvaluationSubjectPolicyConfig,
) -> ArenaMatchReport {
    if let Some(match_state) = &room.match_state {
        for current_seat in 0..4 {
            let identity_index = arena_identity_index_for_seat(room, current_seat);
            if let Some(metrics) = accumulator.seats.get_mut(identity_index) {
                metrics.score_delta = match_state
                    .cumulative_scores
                    .get(&current_seat)
                    .copied()
                    .unwrap_or_default();
                if let Some(stats) = match_state.statistics.seat_stats_by_seat.get(&current_seat) {
                    metrics.wins = stats.win_count as u64;
                    metrics.dealt_in = stats.deal_in_count as u64;
                }
            }
        }
    }
    record_evaluation_tenpai_metrics(&mut accumulator, room);
    let mut report = ArenaMatchReport {
        match_index,
        seed,
        completed,
        action_count,
        subject_id: None,
        subject_display_name: None,
        subject_initial_seat: None,
        subject_final_score: None,
        subject_deal_in_count: None,
        subject_win_count: None,
        seats: accumulator.seats,
    };
    apply_subject_fields_to_report(&mut report, room, subject);
    report
}

fn validate_evaluation_policy_models(
    config: &crate::evaluation::EvaluationArenaConfig,
) -> Result<(), String> {
    let context = validation_bot_context();
    for policy in config
        .subjects
        .iter()
        .map(|subject| &subject.policy)
        .chain(config.opponents.iter())
    {
        if neural_decision_scores_for_model_path(
            &context,
            policy.model_path.as_deref().map(std::path::Path::new),
        )
        .is_none()
        {
            return Err(format!(
                "failed to load neural model for policy '{}'",
                policy.id
            ));
        }
    }
    Ok(())
}

fn validation_bot_context() -> BotContext {
    bot_context_for_discards(&[
        "w1", "w2", "w3", "t1", "t2", "t3", "b1", "b2", "b3", "east", "east", "green", "w9", "w6",
    ])
}

fn trajectory_row_from_trace_with_state(
    match_id: &str,
    decision_index: u64,
    policy: &ArenaBotPolicyConfig,
    trace: &crate::rules::standard::automation::BotDecisionTrace,
    state: &RoomState,
) -> Option<ArenaTrajectoryRow> {
    let features = trace
        .features
        .clone()
        .unwrap_or_else(|| encode_bot_context_v2(&trace.context));

    let (global_tile_planes, global_scalar_features) = {
        use crate::room_scoring::RoomScoringCache;

        let cache = RoomScoringCache::from_state(state);
        let (tile_planes, scalar_features) =
            encode_global_features_v2(&cache, trace.action.seat_index);
        (Some(tile_planes), Some(scalar_features))
    };

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
        global_tile_planes,
        global_scalar_features,
        done: false,
    })
}

#[cfg(test)]
fn trajectory_row_from_trace(
    match_id: &str,
    decision_index: u64,
    policy: &ArenaBotPolicyConfig,
    trace: &crate::rules::standard::automation::BotDecisionTrace,
) -> Option<ArenaTrajectoryRow> {
    let features = trace
        .features
        .clone()
        .unwrap_or_else(|| encode_bot_context_v2(&trace.context));

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
    let computed_scores;
    let scores = match trace_scores {
        Some(scores) => scores,
        None => {
            let model_path = policy.model_path.as_deref().map(std::path::Path::new);
            computed_scores = neural_decision_scores_for_model_path(context, model_path)?;
            &computed_scores
        }
    };
    let risk_config = super::policy::RiskConfig::from_arena_config(policy);
    let log_prob = match action_head {
        "discard" => {
            let discard_logits = risk_adjusted_discard_logits(scores, Some(&risk_config));
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
    use crate::bot::neural::NeuralDecisionScores;
    use crate::core::state::{PlayerRoundState, RoundState};
    use crate::core::tile::Tile;
    use crate::rules::standard::automation::BotDecisionTrace;

    fn test_policy(id: &str) -> ArenaBotPolicyConfig {
        ArenaBotPolicyConfig {
            id: id.to_string(),
            model_path: Some("backend/assets/sft/sft.onnx".to_string()),
            sample_actions: false,
            temperature: 1.0,
            discard_base_risk_weight: default_discard_base_risk_weight(),
            discard_value_risk_range: default_discard_value_risk_range(),
            discard_min_risk_weight: default_discard_min_risk_weight(),
            discard_max_risk_weight: default_discard_max_risk_weight(),
        }
    }

    #[test]
    fn evaluation_arena_errors_when_policy_model_cannot_load() {
        let mut policy = test_policy("missing-model");
        policy.model_path = Some("backend/assets/sft/does-not-exist.onnx".to_string());
        let config = crate::evaluation::EvaluationArenaConfig {
            matches: 1,
            seed: 7,
            max_actions_per_match: 10,
            report_trajectories: false,
            subjects: vec![crate::evaluation::EvaluationSubjectPolicyConfig {
                display_name: "Missing".to_string(),
                policy,
            }],
            opponents: vec![
                test_policy("opponent-1"),
                test_policy("opponent-2"),
                test_policy("opponent-3"),
            ],
        };

        let error = run_evaluation_arena(&config, false).expect_err("missing model must fail");

        assert!(error.contains("failed to load neural model"));
        assert!(error.contains("missing-model"));
    }

    #[test]
    fn arena_policy_config_parses_stochastic_neural_rollout_policy() {
        let raw = r#"{
            "id":"learner",
            "model_path":"backend/assets/models/mahjong_policy_net.onnx",
            "sample_actions":true,
            "temperature":0.8
        }"#;

        let config: ArenaBotPolicyConfig = serde_json::from_str(raw).expect("config");

        assert_eq!(config.id, "learner");
        assert!(config.sample_actions);
        assert_eq!(config.temperature, 0.8);
    }

    #[test]
    fn arena_policy_config_rejects_removed_policy_mode() {
        let raw = r#"{
            "id":"removed_policy",
            "mode":"neural",
            "model_path":"backend/assets/models/mahjong_policy_net.onnx"
        }"#;

        let parsed = serde_json::from_str::<ArenaBotPolicyConfig>(raw);

        assert!(parsed.is_err());
    }

    #[test]
    fn arena_policy_config_rejects_removed_policy_weight_field() {
        let removed_field = concat!("neural", "_weight");
        let raw = format!(
            r#"{{
            "id":"neural",
            "{removed_field}":0,
            "model_path":"backend/assets/models/mahjong_policy_net.onnx"
        }}"#
        );

        let parsed = serde_json::from_str::<ArenaBotPolicyConfig>(&raw);

        assert!(parsed.is_err());
    }

    #[test]
    fn arena_config_parses_subject_replica_shape() {
        let raw = r#"{
            "matches": 1,
            "seed": 20260520,
            "subjects": [
                {"id":"candidate","display_name":"Candidate","model_path":"backend/assets/sft/sft.onnx"}
            ],
            "opponents": [
                {"id":"sft-a","model_path":"backend/assets/sft/sft.onnx"},
                {"id":"sft-b","model_path":"backend/assets/sft/sft.onnx"},
                {"id":"sft-c","model_path":"backend/assets/sft/sft.onnx"}
            ]
        }"#;

        let config: crate::evaluation::EvaluationArenaConfig =
            serde_json::from_str(raw).expect("evaluation arena config");

        assert_eq!(config.subjects.len(), 1);
        assert_eq!(config.opponents.len(), 3);
    }

    #[test]
    fn arena_config_rejects_hard_seat_rotation_fields() {
        let raw = r#"{
            "matches": 1,
            "seed": 20260520,
            "seat_rotation": "cyclic",
            "subjects": [
                {"id":"candidate","display_name":"Candidate","model_path":"backend/assets/sft/sft.onnx"}
            ],
            "opponents": [
                {"id":"sft-a","model_path":"backend/assets/sft/sft.onnx"},
                {"id":"sft-b","model_path":"backend/assets/sft/sft.onnx"},
                {"id":"sft-c","model_path":"backend/assets/sft/sft.onnx"}
            ]
        }"#;

        let parsed = serde_json::from_str::<crate::evaluation::EvaluationArenaConfig>(raw);

        assert!(parsed.is_err());
    }

    #[test]
    fn evaluation_replica_specs_keep_subject_major_order_and_shared_seeds() {
        let config = crate::evaluation::EvaluationArenaConfig {
            matches: 2,
            seed: 100,
            max_actions_per_match: 10,
            report_trajectories: false,
            subjects: vec![
                crate::evaluation::EvaluationSubjectPolicyConfig {
                    display_name: "Baseline".to_string(),
                    policy: test_policy("baseline"),
                },
                crate::evaluation::EvaluationSubjectPolicyConfig {
                    display_name: "Candidate".to_string(),
                    policy: test_policy("candidate"),
                },
            ],
            opponents: vec![
                test_policy("opponent-1"),
                test_policy("opponent-2"),
                test_policy("opponent-3"),
            ],
        };

        let specs = evaluation_replica_specs(&config);

        assert_eq!(
            specs,
            vec![
                EvaluationReplicaSpec {
                    replica_index: 0,
                    subject_index: 0,
                    match_index: 0,
                    seed: 100,
                },
                EvaluationReplicaSpec {
                    replica_index: 1,
                    subject_index: 0,
                    match_index: 1,
                    seed: 101,
                },
                EvaluationReplicaSpec {
                    replica_index: 2,
                    subject_index: 1,
                    match_index: 0,
                    seed: 100,
                },
                EvaluationReplicaSpec {
                    replica_index: 3,
                    subject_index: 1,
                    match_index: 1,
                    seed: 101,
                },
            ]
        );
    }

    #[test]
    fn evaluation_subject_replicas_start_from_same_wall_for_same_match_seed() {
        let config = crate::evaluation::EvaluationArenaConfig {
            matches: 1,
            seed: 100,
            max_actions_per_match: 10,
            report_trajectories: false,
            subjects: vec![
                crate::evaluation::EvaluationSubjectPolicyConfig {
                    display_name: "Baseline".to_string(),
                    policy: test_policy("baseline"),
                },
                crate::evaluation::EvaluationSubjectPolicyConfig {
                    display_name: "Candidate".to_string(),
                    policy: test_policy("candidate"),
                },
            ],
            opponents: vec![
                test_policy("opponent-1"),
                test_policy("opponent-2"),
                test_policy("opponent-3"),
            ],
        };
        let specs = evaluation_replica_specs(&config);

        let first_seed = specs[0].seed;
        let second_seed = specs[1].seed;
        let mut first_room = arena_room("EVALA");
        let mut second_room = arena_room("EVALB");
        crate::evaluation::apply_evaluation_rules(&mut first_room);
        crate::evaluation::apply_evaluation_rules(&mut second_room);
        start_match_in_room_state(
            &mut first_room,
            crate::evaluation::EVALUATION_INITIAL_SUBJECT_SEAT,
            first_seed,
        )
        .expect("first replica should start");
        start_match_in_room_state(
            &mut second_room,
            crate::evaluation::EVALUATION_INITIAL_SUBJECT_SEAT,
            second_seed,
        )
        .expect("second replica should start");

        let first_wall = first_room
            .round_state
            .as_ref()
            .expect("first round")
            .wall
            .tiles
            .iter()
            .map(|tile| tile.tile_id.clone())
            .collect::<Vec<_>>();
        let second_wall = second_room
            .round_state
            .as_ref()
            .expect("second round")
            .wall
            .tiles
            .iter()
            .map(|tile| tile.tile_id.clone())
            .collect::<Vec<_>>();
        assert_eq!(first_seed, second_seed);
        assert_eq!(first_wall, second_wall);
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
        let mut accumulator = ArenaMatchAccumulator::new_with_policies(&[test_policy("neural")]);

        accumulator.record_decision(0, "discard", 3, None);
        accumulator.record_decision(0, "pung", 2, None);

        assert_eq!(accumulator.seats[0].decision_count, 2);
        assert_eq!(accumulator.seats[0].discard_count, 1);
        assert_eq!(accumulator.seats[0].claim_count, 1);
        assert_eq!(accumulator.seats[0].decision_latency_ms_sum, 5);
    }

    #[test]
    fn accumulator_records_policy_telemetry() {
        let mut accumulator = ArenaMatchAccumulator::new_with_policies(&[test_policy("neural")]);

        let neural = crate::bot::policy::BotPolicyDecisionTelemetry {
            model_loaded: true,
            used_neural_action: true,
        };
        accumulator.record_decision(0, "discard", 3, Some(&neural));

        assert!(accumulator.seats[0].model_loaded);
        assert_eq!(accumulator.seats[0].neural_action_count, 1);
    }

    #[test]
    fn match_report_final_tenpai_uses_hand_shape_not_ready_hand_flag() {
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

        let subject = crate::evaluation::EvaluationSubjectPolicyConfig {
            display_name: "Neural".to_string(),
            policy: test_policy("neural"),
        };
        let report = build_evaluation_match_report(
            0,
            7,
            &room,
            ArenaMatchAccumulator::new_with_policies(&[test_policy("neural")]),
            1,
            true,
            &subject,
        );
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
        let report = build_evaluation_match_report(
            0,
            7,
            &room,
            ArenaMatchAccumulator::new_with_policies(&[test_policy("neural")]),
            1,
            true,
            &subject,
        );
        assert!(!report.seats[0].final_tenpai);
    }

    #[test]
    fn match_report_records_first_tenpai_turn_from_actual_hand_shape() {
        let mut accumulator = ArenaMatchAccumulator::new_with_policies(&[test_policy("neural")]);
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

        let subject = crate::evaluation::EvaluationSubjectPolicyConfig {
            display_name: "Neural".to_string(),
            policy: test_policy("neural"),
        };
        let report = build_evaluation_match_report(0, 7, &room, accumulator, 1, true, &subject);

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
            policy_id: "neural".to_string(),
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
            features: None,
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
            model_path: Some("missing-model-for-trajectory-test.onnx".to_string()),
            sample_actions: true,
            temperature: 1.0,
            discard_base_risk_weight: default_discard_base_risk_weight(),
            discard_value_risk_range: default_discard_value_risk_range(),
            discard_min_risk_weight: default_discard_min_risk_weight(),
            discard_max_risk_weight: default_discard_max_risk_weight(),
        };

        let row = trajectory_row_from_trace("arena-test", 0, &policy, &trace).expect("row");

        let expected = -(1.0_f32 + (-2.0_f32).exp()).ln();
        assert!((row.log_prob - expected).abs() < 0.0001);
        assert_eq!(row.value, 0.75);
    }

    #[test]
    fn trajectory_row_reuses_trace_features() {
        let context = bot_context_for_discards(&["w1", "w2"]);
        let mut features = encode_bot_context_v2(&context);
        let marker = 0.42_f32;
        features.tile_planes[0] = marker;
        features.scalar_features[0] = marker;
        features.discard_sequence[0] = marker;
        let trace = BotDecisionTrace {
            decision_kind: "active_turn".to_string(),
            action: BotAction {
                seat_index: 0,
                action_type: "discard".to_string(),
                tile_ids: vec!["w1-0".to_string()],
            },
            context,
            features: Some(features),
            telemetry: crate::bot::policy::BotPolicyDecisionTelemetry::default(),
            neural_scores: None,
        };
        let policy = ArenaBotPolicyConfig {
            id: "learner".to_string(),
            model_path: None,
            sample_actions: false,
            temperature: 1.0,
            discard_base_risk_weight: default_discard_base_risk_weight(),
            discard_value_risk_range: default_discard_value_risk_range(),
            discard_min_risk_weight: default_discard_min_risk_weight(),
            discard_max_risk_weight: default_discard_max_risk_weight(),
        };

        let row = trajectory_row_from_trace("arena-test", 0, &policy, &trace).expect("row");

        assert_eq!(row.tile_planes[0], marker);
        assert_eq!(row.scalar_features[0], marker);
        assert_eq!(row.discard_sequence[0], marker);
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
            features: None,
            telemetry: crate::bot::policy::BotPolicyDecisionTelemetry::default(),
            neural_scores: Some(scores.clone()),
        };
        let policy = ArenaBotPolicyConfig {
            id: "learner".to_string(),
            model_path: Some("missing-model-for-trajectory-test.onnx".to_string()),
            sample_actions: true,
            temperature: 1.0,
            discard_base_risk_weight: default_discard_base_risk_weight(),
            discard_value_risk_range: default_discard_value_risk_range(),
            discard_min_risk_weight: default_discard_min_risk_weight(),
            discard_max_risk_weight: default_discard_max_risk_weight(),
        };

        let row = trajectory_row_from_trace("arena-test", 0, &policy, &trace).expect("row");
        let features = encode_bot_context_v2(&trace.context);
        let adjusted = risk_adjusted_discard_logits(&scores, None);
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
            subject_id: None,
            subject_display_name: None,
            subject_initial_seat: None,
            subject_final_score: None,
            subject_deal_in_count: None,
            subject_win_count: None,
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
            qualifying_fan_potential: 0,
        };
        let after = RewardSnapshot {
            shanten: 1,
            qualifying_fan_potential: 0,
        };

        apply_shaping_reward(&mut row, Some(before), Some(after));

        assert!(row.step_reward > 0.0);
        assert_eq!(row.reward, row.step_reward);
        assert_eq!(row.shanten_before, Some(2));
        assert_eq!(row.shanten_after, Some(1));

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
            policy_id: "neural".to_string(),
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
            global_tile_planes: None,
            global_scalar_features: None,
            done: false,
        }
    }

    fn tile_key_only_vec(tile_keys: &[&str]) -> Vec<Tile> {
        tile_keys
            .iter()
            .map(|tile_key| Tile::tile_key_only(tile_key))
            .collect()
    }
}
