use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::thread;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use backend::bot::neural::{NeuralOutputSnapshot, inspect_bot_context_neural_scores};
use backend::bot::policy::BotPolicyConfig;
use backend::core::engine::try_handle_player_action_in_room_state;
use backend::core::state::{RoomState, SeatState};
use backend::evaluation::apply_evaluation_rules;
use backend::projection::bot_view::{BotClaimOption, build_bot_context_view};
use backend::room_scoring::RoomScoringCache;
use backend::rules::standard::automation::next_bot_action_in_room_state_with_policy_resolver;
use backend::rules::standard::flow::start_single_round_in_room_state;
use backend::rules::standard::win::legal_hu_available_for_seat_in_room_state;

const DEFAULT_MODEL_PATH: &str = "backend/assets/sft/sft.onnx";
const MAX_ACTIONS_PER_HAND: usize = 600;
const WINDS: [&str; 4] = ["east", "south", "west", "north"];

#[derive(Clone, Debug)]
struct ArenaArgs {
    candidate: String,
    baseline: Option<String>,
    candidate_model: Option<String>,
    baseline_model: Option<String>,
    candidate_risk_weight: Option<f32>,
    candidate_value_scale: Option<f32>,
    deals: usize,
    seed: u64,
    min_fan: i64,
    threads: usize,
    diagnostics: bool,
    json_out: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, Serialize)]
struct SideStats {
    hands: u64,
    total_score_delta: i64,
    wins: u64,
    self_draw_wins: u64,
    discard_wins: u64,
    deal_ins: u64,
    draws: u64,
    total_win_fan: i64,
    fan_8_15: u64,
    fan_16_23: u64,
    fan_24_47: u64,
    fan_48_plus: u64,
    open_meld_hands: u64,
    open_meld_wins: u64,
    open_meld_draws: u64,
    concealed_hands: u64,
    concealed_wins: u64,
    legal_self_draw_hu_ops: u64,
    missed_self_draw_hu: u64,
    legal_discard_hu_ops: u64,
    missed_discard_hu: u64,
    aborted_hands: u64,
}

impl SideStats {
    fn merge(&mut self, other: &Self) {
        self.hands += other.hands;
        self.total_score_delta += other.total_score_delta;
        self.wins += other.wins;
        self.self_draw_wins += other.self_draw_wins;
        self.discard_wins += other.discard_wins;
        self.deal_ins += other.deal_ins;
        self.draws += other.draws;
        self.total_win_fan += other.total_win_fan;
        self.fan_8_15 += other.fan_8_15;
        self.fan_16_23 += other.fan_16_23;
        self.fan_24_47 += other.fan_24_47;
        self.fan_48_plus += other.fan_48_plus;
        self.open_meld_hands += other.open_meld_hands;
        self.open_meld_wins += other.open_meld_wins;
        self.open_meld_draws += other.open_meld_draws;
        self.concealed_hands += other.concealed_hands;
        self.concealed_wins += other.concealed_wins;
        self.legal_self_draw_hu_ops += other.legal_self_draw_hu_ops;
        self.missed_self_draw_hu += other.missed_self_draw_hu;
        self.legal_discard_hu_ops += other.legal_discard_hu_ops;
        self.missed_discard_hu += other.missed_discard_hu;
        self.aborted_hands += other.aborted_hands;
    }

    fn avg_score(&self) -> f64 {
        self.total_score_delta as f64 / self.hands.max(1) as f64
    }

    fn rate(numerator: u64, denominator: u64) -> f64 {
        100.0 * numerator as f64 / denominator.max(1) as f64
    }

    fn avg_win_fan(&self) -> f64 {
        if self.wins == 0 {
            0.0
        } else {
            self.total_win_fan as f64 / self.wins as f64
        }
    }
}

#[derive(Clone, Debug, Default)]
struct DiagnosticSamples {
    value_for_risk: Vec<f32>,
    qualifying_fan_value: Vec<f32>,
    max_tenpai_prob: Vec<f32>,
    max_cond_risk_prob: Vec<f32>,
    max_joint_risk_prob: Vec<f32>,
    self_draw_hu_pass_minus_hu: Vec<f32>,
    self_draw_qualifying_fan: Vec<f32>,
}

impl DiagnosticSamples {
    fn merge(&mut self, mut other: Self) {
        self.value_for_risk.append(&mut other.value_for_risk);
        self.qualifying_fan_value
            .append(&mut other.qualifying_fan_value);
        self.max_tenpai_prob.append(&mut other.max_tenpai_prob);
        self.max_cond_risk_prob
            .append(&mut other.max_cond_risk_prob);
        self.max_joint_risk_prob
            .append(&mut other.max_joint_risk_prob);
        self.self_draw_hu_pass_minus_hu
            .append(&mut other.self_draw_hu_pass_minus_hu);
        self.self_draw_qualifying_fan
            .append(&mut other.self_draw_qualifying_fan);
    }

    fn record_turn(&mut self, snap: &NeuralOutputSnapshot) {
        if self.value_for_risk.len() >= 2000 {
            return;
        }
        self.value_for_risk.push(snap.value_for_risk);
        self.qualifying_fan_value.push(snap.qualifying_fan_value);
        let max_tenpai = snap
            .opponent_tenpai_logits
            .iter()
            .map(|&l| sigmoid(l))
            .fold(0.0_f32, f32::max);
        self.max_tenpai_prob.push(max_tenpai);
        self.max_cond_risk_prob.push(snap.max_conditional_risk_prob);
        self.max_joint_risk_prob.push(snap.max_joint_deal_in_prob);
    }

    fn record_self_draw_hu(&mut self, snap: &NeuralOutputSnapshot) {
        self.self_draw_hu_pass_minus_hu
            .push(snap.hu_logits[0] - snap.hu_logits[1]);
        self.self_draw_qualifying_fan
            .push(snap.qualifying_fan_value);
    }
}

#[derive(Clone, Debug, Serialize)]
struct DistributionSummary {
    count: usize,
    min: f32,
    p10: f32,
    median: f32,
    mean: f32,
    p90: f32,
    max: f32,
}

#[derive(Clone, Debug, Serialize)]
struct PairedDuplicateSummary {
    deals: usize,
    candidate_hands: u64,
    mean_score_per_hand: f64,
    std_per_deal: f64,
    ci95_half_width: f64,
    t_stat: f64,
    deals_won: usize,
    deals_tied: usize,
    deals_lost: usize,
}

#[derive(Clone, Debug, Serialize)]
struct ArenaReport {
    mode: String,
    deals: usize,
    seed: u64,
    min_fan: i64,
    elapsed_secs: f64,
    candidate_config: BotPolicyConfig,
    baseline_config: Option<BotPolicyConfig>,
    candidate: SideStats,
    baseline: Option<SideStats>,
    paired_duplicate: Option<PairedDuplicateSummary>,
}

fn main() -> Result<()> {
    let args = parse_args()?;
    let mut candidate_config = resolve_policy(&args.candidate, args.candidate_model.as_deref())?;
    if let Some(risk_weight) = args.candidate_risk_weight {
        candidate_config.discard_base_risk_weight = risk_weight;
    }
    if let Some(value_scale) = args.candidate_value_scale {
        candidate_config.discard_value_scale = value_scale;
    }
    let baseline_config = match args.baseline.as_deref() {
        Some(spec) => Some(resolve_policy(spec, args.baseline_model.as_deref())?),
        None => None,
    };

    let started = Instant::now();
    let deals = args.deals;
    let threads = args.threads.max(1).min(deals.max(1));
    let next_deal = Arc::new(AtomicUsize::new(0));
    let candidate_arc = Arc::new(candidate_config.clone());
    let baseline_arc = Arc::new(baseline_config.clone());

    let mut handles = Vec::with_capacity(threads);
    for _ in 0..threads {
        let next_deal = Arc::clone(&next_deal);
        let candidate = Arc::clone(&candidate_arc);
        let baseline = Arc::clone(&baseline_arc);
        let base_seed = args.seed;
        let min_fan = args.min_fan;
        let diagnostics = args.diagnostics;

        handles.push(thread::spawn(move || {
            let mut cand_stats = SideStats::default();
            let mut base_stats = SideStats::default();
            let mut deal_means = Vec::<f64>::new();
            let mut diag = DiagnosticSamples::default();

            loop {
                let deal_index = next_deal.fetch_add(1, Ordering::Relaxed);
                if deal_index >= deals {
                    break;
                }
                let deal_seed = base_seed.wrapping_add(deal_index as u64);
                let round_wind = WINDS[(deal_index / 4) % 4];
                let dealer_seat = deal_index % 4;

                if let Some(base) = baseline.as_ref() {
                    let mut deal_delta_sum = 0_i64;
                    for candidate_seat in 0..4 {
                        let outcome = run_single_hand(
                            deal_seed,
                            dealer_seat,
                            round_wind,
                            min_fan,
                            |seat| {
                                if seat == candidate_seat {
                                    (*candidate).clone()
                                } else {
                                    base.clone()
                                }
                            },
                            diagnostics && candidate_seat == 0,
                            &mut diag,
                        );
                        deal_delta_sum += outcome.seat_deltas[candidate_seat];
                        cand_stats.merge(&outcome.seat_stats[candidate_seat]);
                        for seat in 0..4 {
                            if seat != candidate_seat {
                                base_stats.merge(&outcome.seat_stats[seat]);
                            }
                        }
                    }
                    deal_means.push(deal_delta_sum as f64 / 4.0);
                } else {
                    let outcome = run_single_hand(
                        deal_seed,
                        dealer_seat,
                        round_wind,
                        min_fan,
                        |_| (*candidate).clone(),
                        diagnostics,
                        &mut diag,
                    );
                    for seat in 0..4 {
                        cand_stats.merge(&outcome.seat_stats[seat]);
                    }
                }
            }

            (cand_stats, base_stats, deal_means, diag)
        }));
    }

    let mut total_cand = SideStats::default();
    let mut total_base = SideStats::default();
    let mut all_deal_means = Vec::with_capacity(deals);
    let mut total_diag = DiagnosticSamples::default();

    for handle in handles {
        let (cand, base, mut means, diag) = handle
            .join()
            .map_err(|_| anyhow::anyhow!("arena worker panicked"))?;
        total_cand.merge(&cand);
        total_base.merge(&base);
        all_deal_means.append(&mut means);
        total_diag.merge(diag);
    }

    let elapsed = started.elapsed().as_secs_f64();
    let paired_duplicate = if baseline_config.is_some() {
        Some(summarize_paired_deals(&all_deal_means, total_cand.hands))
    } else {
        None
    };

    let report = ArenaReport {
        mode: if baseline_config.is_some() {
            "duplicate_4seat_ab".to_string()
        } else {
            "self_play_4seat".to_string()
        },
        deals,
        seed: args.seed,
        min_fan: args.min_fan,
        elapsed_secs: elapsed,
        candidate_config,
        baseline_config: baseline_config.clone(),
        candidate: total_cand,
        baseline: baseline_config.map(|_| total_base),
        paired_duplicate,
    };

    print_report(&report, args.diagnostics.then_some(&total_diag));

    if let Some(json_out) = args.json_out {
        if let Some(parent) = json_out.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&json_out, serde_json::to_string_pretty(&report)?)?;
        println!("Saved JSON report to {}", json_out.display());
    }

    Ok(())
}

struct HandOutcome {
    seat_deltas: [i64; 4],
    seat_stats: [SideStats; 4],
}

fn run_single_hand<F>(
    seed: u64,
    dealer_seat: usize,
    round_wind: &str,
    min_fan: i64,
    policy_for_seat: F,
    collect_diagnostics: bool,
    diag: &mut DiagnosticSamples,
) -> HandOutcome
where
    F: Fn(usize) -> BotPolicyConfig,
{
    let mut room = build_bot_room(min_fan);
    start_single_round_in_room_state(&mut room, dealer_seat, round_wind, seed)
        .expect("start single round");

    let mut seat_stats: [SideStats; 4] = std::array::from_fn(|_| SideStats::default());
    let mut actions_taken = 0_usize;

    while room.phase == "playing" && actions_taken < MAX_ACTIONS_PER_HAND {
        let Some(action) =
            next_bot_action_in_room_state_with_policy_resolver(&room, &policy_for_seat)
                .expect("next bot action")
        else {
            break;
        };

        let seat = action.seat_index;
        let legal_hu = legal_hu_available_for_seat_in_room_state(&room, seat);
        match legal_hu {
            Some("self_draw") => {
                seat_stats[seat].legal_self_draw_hu_ops += 1;
                if action.action_type != "hu" {
                    seat_stats[seat].missed_self_draw_hu += 1;
                }
                if collect_diagnostics {
                    if let Some(snap) =
                        inspect_current_seat(&room, seat, &policy_for_seat(seat), true)
                    {
                        diag.record_self_draw_hu(&snap);
                    }
                }
            }
            Some("discard") | Some("rob_kong") => {
                seat_stats[seat].legal_discard_hu_ops += 1;
                if action.action_type != "hu" {
                    seat_stats[seat].missed_discard_hu += 1;
                }
            }
            _ => {}
        }

        if collect_diagnostics && action.action_type == "discard" && actions_taken % 4 == 0 {
            if let Some(snap) = inspect_current_seat(&room, seat, &policy_for_seat(seat), false) {
                diag.record_turn(&snap);
            }
        }

        let step = try_handle_player_action_in_room_state(
            &mut room,
            action.seat_index,
            &action.action_type,
            &action.tile_ids,
        );
        match step {
            Ok(Some(Ok(_))) => {
                actions_taken += 1;
            }
            _ => {
                break;
            }
        }
    }

    let mut seat_deltas = [0_i64; 4];
    for seat in 0..4 {
        seat_stats[seat].hands = 1;
    }

    let Some(round) = room.round_state.as_ref() else {
        for stats in &mut seat_stats {
            stats.aborted_hands = 1;
        }
        return HandOutcome {
            seat_deltas,
            seat_stats,
        };
    };

    let Some(settlement) = round.settlement.as_ref() else {
        for stats in &mut seat_stats {
            stats.aborted_hands = 1;
        }
        return HandOutcome {
            seat_deltas,
            seat_stats,
        };
    };

    let winning_seats = settlement.winning_seats();
    let is_draw = settlement.win_type == "draw";

    for seat in 0..4 {
        let delta = settlement
            .score_delta
            .total_delta_by_seat
            .get(&seat)
            .copied()
            .unwrap_or(0);
        seat_deltas[seat] = delta;
        seat_stats[seat].total_score_delta = delta;

        let has_open_meld = round
            .players
            .get(seat)
            .is_some_and(|player| !player.melds.is_empty());
        if has_open_meld {
            seat_stats[seat].open_meld_hands = 1;
        } else {
            seat_stats[seat].concealed_hands = 1;
        }

        if is_draw {
            seat_stats[seat].draws = 1;
            if has_open_meld {
                seat_stats[seat].open_meld_draws = 1;
            }
            continue;
        }

        if winning_seats.contains(&seat) {
            seat_stats[seat].wins = 1;
            if settlement.win_type == "self_draw" {
                seat_stats[seat].self_draw_wins = 1;
            } else {
                seat_stats[seat].discard_wins = 1;
            }
            if has_open_meld {
                seat_stats[seat].open_meld_wins = 1;
            } else {
                seat_stats[seat].concealed_wins = 1;
            }
            let fan = settlement
                .winning_details
                .iter()
                .find(|detail| detail.winner_seat == seat)
                .map(|detail| detail.fan_total)
                .unwrap_or(settlement.fan_total);
            seat_stats[seat].total_win_fan = fan;
            match fan {
                ..=15 => seat_stats[seat].fan_8_15 = 1,
                16..=23 => seat_stats[seat].fan_16_23 = 1,
                24..=47 => seat_stats[seat].fan_24_47 = 1,
                _ => seat_stats[seat].fan_48_plus = 1,
            }
        }

        if settlement.win_type == "discard" && settlement.discarder_seat == Some(seat) {
            seat_stats[seat].deal_ins = 1;
        }
    }

    HandOutcome {
        seat_deltas,
        seat_stats,
    }
}

fn inspect_current_seat(
    room: &RoomState,
    seat_index: usize,
    policy: &BotPolicyConfig,
    can_hu: bool,
) -> Option<NeuralOutputSnapshot> {
    let cache = RoomScoringCache::from_state(room);
    let claim_options = if can_hu {
        vec![BotClaimOption {
            action_type: "hu".to_string(),
            tile_ids: Vec::new(),
        }]
    } else {
        Vec::new()
    };
    let context = build_bot_context_view(
        &cache,
        room,
        seat_index,
        claim_options,
        Vec::new(),
        HashSet::new(),
    )?;
    let path = policy.model_path.as_deref().map(Path::new);
    inspect_bot_context_neural_scores(path, &context)
}

fn build_bot_room(min_fan: i64) -> RoomState {
    let mut room = RoomState {
        table_code: "ARENA".to_string(),
        phase: "waiting".to_string(),
        mode: "evaluation".to_string(),
        multiplier: 1,
        minimum_hu_fan: min_fan,
        ..RoomState::default()
    };
    apply_evaluation_rules(&mut room);
    room.minimum_hu_fan = min_fan;
    for seat_index in 0..4 {
        room.seats.push(SeatState {
            seat_index,
            user_id: None,
            nickname: Some(format!("ArenaBot{seat_index}")),
            points: Some(0),
            title: None,
            connected: true,
            is_bot: true,
            seat_type: "bot".to_string(),
            bot_persona: None,
            bot_aggression: None,
            disconnect_deadline_at: None,
            consecutive_timeout_auto_response_count: 0,
        });
    }
    room
}

fn summarize_paired_deals(deal_means: &[f64], candidate_hands: u64) -> PairedDuplicateSummary {
    let n = deal_means.len();
    if n == 0 {
        return PairedDuplicateSummary {
            deals: 0,
            candidate_hands,
            mean_score_per_hand: 0.0,
            std_per_deal: 0.0,
            ci95_half_width: 0.0,
            t_stat: 0.0,
            deals_won: 0,
            deals_tied: 0,
            deals_lost: 0,
        };
    }
    let mean = deal_means.iter().sum::<f64>() / n as f64;
    let var = if n > 1 {
        deal_means
            .iter()
            .map(|value| {
                let diff = *value - mean;
                diff * diff
            })
            .sum::<f64>()
            / (n - 1) as f64
    } else {
        0.0
    };
    let std = var.sqrt();
    let se = std / (n as f64).sqrt();
    let ci95 = 1.96 * se;
    let t_stat = if se > 1e-9 { mean / se } else { 0.0 };
    let deals_won = deal_means.iter().filter(|&&v| v > 1e-9).count();
    let deals_lost = deal_means.iter().filter(|&&v| v < -1e-9).count();
    let deals_tied = n - deals_won - deals_lost;

    PairedDuplicateSummary {
        deals: n,
        candidate_hands,
        mean_score_per_hand: mean,
        std_per_deal: std,
        ci95_half_width: ci95,
        t_stat,
        deals_won,
        deals_tied,
        deals_lost,
    }
}

fn print_report(report: &ArenaReport, diag: Option<&DiagnosticSamples>) {
    println!("============================================================");
    println!(
        "Guobiao Mahjong Bot Arena | mode={} | deals={} | min_fan={} | elapsed={:.2}s",
        report.mode, report.deals, report.min_fan, report.elapsed_secs
    );
    println!("============================================================");
    print_side("Candidate", &report.candidate_config.id, &report.candidate);
    if let (Some(base_cfg), Some(base_stats)) = (&report.baseline_config, &report.baseline) {
        print_side("Baseline ", &base_cfg.id, base_stats);
    }
    if let Some(paired) = &report.paired_duplicate {
        println!("------------------------------------------------------------");
        println!(
            "4-Seat Duplicate Paired Delta (Candidate - Table Avg): {:+.3} ± {:.3} pts/hand (95% CI) | t = {:+.2}",
            paired.mean_score_per_hand, paired.ci95_half_width, paired.t_stat
        );
        println!(
            "Paired Deals (n={}): Won={} | Tied={} | Lost={}",
            paired.deals, paired.deals_won, paired.deals_tied, paired.deals_lost
        );
    }
    if let Some(diag) = diag {
        println!("------------------------------------------------------------");
        println!("Neural Output Diagnostics (sft.onnx live gameplay):");
        print_dist("  value_for_risk         ", &diag.value_for_risk);
        print_dist("  qualifying_fan_value   ", &diag.qualifying_fan_value);
        print_dist("  max_opp_tenpai_prob    ", &diag.max_tenpai_prob);
        print_dist("  max_cond_risk_prob     ", &diag.max_cond_risk_prob);
        print_dist("  max_joint_deal_in_prob ", &diag.max_joint_risk_prob);
        print_dist(
            "  self_draw_hu(pass-hu)  ",
            &diag.self_draw_hu_pass_minus_hu,
        );
        print_dist("  self_draw_qfan_value   ", &diag.self_draw_qualifying_fan);
    }
    println!("============================================================");
}

fn print_side(label: &str, id: &str, s: &SideStats) {
    println!("{label} [{id}] (hands={}, aborted={}):", s.hands, s.aborted_hands);
    println!(
        "  AvgScore={:+.3}/hand | Win={:.2}% (SelfDraw={:.2}%, DiscardWin={:.2}%) | DealIn={:.2}% | Draw={:.2}%",
        s.avg_score(),
        SideStats::rate(s.wins, s.hands),
        SideStats::rate(s.self_draw_wins, s.hands),
        SideStats::rate(s.discard_wins, s.hands),
        SideStats::rate(s.deal_ins, s.hands),
        SideStats::rate(s.draws, s.hands),
    );
    println!(
        "  AvgWinFan={:.2} | Buckets: 8-15={} | 16-23={} | 24-47={} | 48+={}",
        s.avg_win_fan(),
        s.fan_8_15,
        s.fan_16_23,
        s.fan_24_47,
        s.fan_48_plus,
    );
    println!(
        "  OpenMeld={:.1}% (WinRate={:.2}%, DrawRate={:.2}%) | ConcealedWinRate={:.2}%",
        SideStats::rate(s.open_meld_hands, s.hands),
        SideStats::rate(s.open_meld_wins, s.open_meld_hands),
        SideStats::rate(s.open_meld_draws, s.open_meld_hands),
        SideStats::rate(s.concealed_wins, s.concealed_hands),
    );
    println!(
        "  LegalHu Missed: SelfDraw={}/{} ({:.1}%) | Discard={}/{} ({:.1}%)",
        s.missed_self_draw_hu,
        s.legal_self_draw_hu_ops,
        SideStats::rate(s.missed_self_draw_hu, s.legal_self_draw_hu_ops),
        s.missed_discard_hu,
        s.legal_discard_hu_ops,
        SideStats::rate(s.missed_discard_hu, s.legal_discard_hu_ops),
    );
}

fn summarize_dist(values: &[f32]) -> Option<DistributionSummary> {
    if values.is_empty() {
        return None;
    }
    let mut sorted: Vec<f32> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(f32::total_cmp);
    let n = sorted.len();
    let mean = sorted.iter().map(|&v| v as f64).sum::<f64>() / n as f64;
    let pct = |p: f64| sorted[((n - 1) as f64 * p).round() as usize];
    Some(DistributionSummary {
        count: n,
        min: sorted[0],
        p10: pct(0.10),
        median: pct(0.50),
        mean: mean as f32,
        p90: pct(0.90),
        max: sorted[n - 1],
    })
}

fn print_dist(label: &str, values: &[f32]) {
    if let Some(d) = summarize_dist(values) {
        println!(
            "{label} (n={:4}): min={:+.4} p10={:+.4} med={:+.4} mean={:+.4} p90={:+.4} max={:+.4}",
            d.count, d.min, d.p10, d.median, d.mean, d.p90, d.max
        );
    } else {
        println!("{label} (n=   0): <no samples>");
    }
}

fn sigmoid(x: f32) -> f32 {
    if x >= 0.0 {
        1.0 / (1.0 + (-x).exp())
    } else {
        let e = x.exp();
        e / (1.0 + e)
    }
}

fn resolve_policy(spec: &str, model_override: Option<&str>) -> Result<BotPolicyConfig> {
    let model_path = model_override
        .map(ToString::to_string)
        .or_else(|| Some(DEFAULT_MODEL_PATH.to_string()));
    let mut config = match spec {
        "fixed" | "default" | "fixed-auto-hu" => {
            BotPolicyConfig::default_with_model(spec, model_path.clone())
        }
        "fixed-neural-hu" => {
            let mut cfg = BotPolicyConfig::default_with_model("fixed-neural-hu", model_path.clone());
            cfg.always_take_legal_hu = false;
            cfg
        }
        "fixed-no-joint-risk" => {
            let mut cfg =
                BotPolicyConfig::default_with_model("fixed-no-joint-risk", model_path.clone());
            cfg.use_joint_tenpai_risk = false;
            cfg
        }
        "legacy" => BotPolicyConfig::legacy_with_model("legacy", model_path.clone()),
        path_str => {
            let raw = fs::read_to_string(path_str)
                .with_context(|| format!("reading policy json {path_str}"))?;
            serde_json::from_str::<BotPolicyConfig>(&raw)
                .with_context(|| format!("parsing policy json {path_str}"))?
        }
    };
    if let Some(override_path) = model_override {
        config.model_path = Some(override_path.to_string());
    }
    Ok(config)
}

fn parse_args() -> Result<ArenaArgs> {
    let mut candidate = "fixed".to_string();
    let mut baseline: Option<String> = None;
    let mut candidate_model: Option<String> = None;
    let mut baseline_model: Option<String> = None;
    let mut candidate_risk_weight: Option<f32> = None;
    let mut candidate_value_scale: Option<f32> = None;
    let mut deals = 100_usize;
    let mut seed = 20_260_601_u64;
    let mut min_fan = 8_i64;
    let mut threads = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let mut diagnostics = false;
    let mut json_out: Option<PathBuf> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--candidate" => candidate = next_arg(&mut args, "--candidate")?,
            "--baseline" => baseline = Some(next_arg(&mut args, "--baseline")?),
            "--candidate-model" => candidate_model = Some(next_arg(&mut args, "--candidate-model")?),
            "--baseline-model" => baseline_model = Some(next_arg(&mut args, "--baseline-model")?),
            "--candidate-risk-weight" => {
                candidate_risk_weight = Some(next_arg(&mut args, "--candidate-risk-weight")?.parse()?)
            }
            "--candidate-value-scale" => {
                candidate_value_scale = Some(next_arg(&mut args, "--candidate-value-scale")?.parse()?)
            }
            "--deals" => deals = next_arg(&mut args, "--deals")?.parse()?,
            "--seed" => seed = next_arg(&mut args, "--seed")?.parse()?,
            "--min-fan" => min_fan = next_arg(&mut args, "--min-fan")?.parse()?,
            "--threads" => threads = next_arg(&mut args, "--threads")?.parse()?,
            "--diagnostics" => diagnostics = true,
            "--json-out" => json_out = Some(PathBuf::from(next_arg(&mut args, "--json-out")?)),
            "--help" | "-h" => {
                println!(
                    "Usage: bot_arena [--candidate fixed|fixed-neural-hu|legacy|<json>] \
                     [--baseline <spec>] [--candidate-model <onnx>] [--baseline-model <onnx>] \
                     [--deals 100] [--seed 20260601] [--min-fan 8] [--threads 4] \
                     [--diagnostics] [--json-out <path>]"
                );
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}"),
        }
    }

    Ok(ArenaArgs {
        candidate,
        baseline,
        candidate_model,
        baseline_model,
        candidate_risk_weight,
        candidate_value_scale,
        deals,
        seed,
        min_fan,
        threads,
        diagnostics,
        json_out,
    })
}

fn next_arg(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next()
        .with_context(|| format!("{flag} requires a value"))
}
