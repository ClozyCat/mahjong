use super::context::*;
use super::neural::{
    NeuralDecisionScores, RankedTileScore, rank_masked_claims, rank_masked_discards,
};
use super::search::{
    BotDiscardPlan, STAGE_ONE_DEPTH, SearchEngine, claim_action_bonus, claim_meld_tile_keys,
    simulated_tiles_after_removal,
};
use crate::bot::arena::{ArenaBotPolicyConfig, ArenaPolicyMode};
use std::{env, time::Instant};

const HYBRID_TOP_SEARCH_CANDIDATES: usize = 5;
const HYBRID_CLOSE_SEARCH_GAP: i64 = 90;
const HYBRID_NEURAL_OVERRIDE_GAP: i64 = 180;
const DEFAULT_NEURAL_PRIOR_WEIGHT: i64 = 35;
const POLICY_ENV: &str = "MAHJONG_BOT_POLICY";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BotPolicyMode {
    Heuristic,
    Hybrid,
    Neural,
}

pub fn choose_active_turn_action(context: &BotContext) -> Option<BotAction> {
    choose_active_turn_action_with_config(context, &bot_policy_config_from_env())
}

pub fn choose_active_turn_action_with_config(
    context: &BotContext,
    config: &ArenaBotPolicyConfig,
) -> Option<BotAction> {
    let policy_mode = bot_policy_mode_from_config(config);
    let neural_weight = config.neural_weight.max(0);
    choose_active_turn_action_inner(context, policy_mode, neural_weight, config)
}

fn choose_active_turn_action_inner(
    context: &BotContext,
    policy_mode: BotPolicyMode,
    neural_weight: i64,
    config: &ArenaBotPolicyConfig,
) -> Option<BotAction> {
    if policy_mode == BotPolicyMode::Neural {
        if let Some(scores) = neural_decision_scores_for_policy(context, config) {
            if let Some(action) = select_neural_only_active_turn_action(context, &scores) {
                return Some(action);
            }
        }
    }

    let decision_started = Instant::now();
    let mut engine = SearchEngine::new(context);
    let search_plans = engine.rank_best_discard_plans(
        context,
        &context.player.concealed_tiles,
        &context.player.concealed_tile_counts,
        &context.player.meld_tile_key_groups,
        &[],
        context.restricted_discard_tile_key.as_deref(),
        context.drawn_tile_id.as_deref(),
    );
    let baseline = search_plans.first()?.clone();

    let mut best_kong = None;
    for candidate in &context.self_kong_candidates {
        if candidate.kind == BotSelfKongKind::Add
            && context.add_kong_risk_tiles.contains(&candidate.tile_key)
        {
            continue;
        }

        let concealed_counts_after = tile_counts_after_removal(
            &context.player.concealed_tiles,
            &context.player.concealed_tile_counts,
            &candidate.tile_ids,
        );
        let mut meld_groups_after = context.player.meld_tile_key_groups.clone();
        let mut appended_open_flags = Vec::new();
        match candidate.kind {
            BotSelfKongKind::Concealed => {
                meld_groups_after.push(vec![candidate.tile_key.clone(); 4]);
                appended_open_flags.push(false);
            }
            BotSelfKongKind::Add => {
                let meld_index = candidate.meld_index?;
                if let Some(meld) = meld_groups_after.get_mut(meld_index) {
                    *meld = vec![candidate.tile_key.clone(); 4];
                }
            }
        }

        let expected_score = engine.expected_score_after_forced_draw(
            context,
            &concealed_counts_after,
            &meld_groups_after,
            &appended_open_flags,
            Some(candidate.tile_key.as_str()),
            STAGE_ONE_DEPTH,
        )?;
        let kong_bonus = match candidate.kind {
            BotSelfKongKind::Concealed => 220,
            BotSelfKongKind::Add => 120,
        };
        let total_score = expected_score + kong_bonus;
        let replace = best_kong
            .as_ref()
            .map(|(_, score): &(BotAction, i64)| total_score > *score)
            .unwrap_or(true);
        if replace {
            best_kong = Some((
                BotAction {
                    seat_index: context.seat_index,
                    action_type: "kong".to_string(),
                    tile_ids: candidate.tile_ids.clone(),
                },
                total_score,
            ));
        }
    }

    trace_discard_decision_if_enabled(context, &engine, &baseline, decision_started.elapsed());

    let neural_scores = if policy_mode == BotPolicyMode::Hybrid {
        neural_decision_scores_for_policy(context, config)
    } else {
        None
    };
    let selected_discard = neural_scores
        .as_ref()
        .and_then(|scores| {
            let neural_discard_scores = rank_masked_discards(context, &scores.discard_logits);
            select_discard_plan_for_policy(
                policy_mode,
                &search_plans,
                &neural_discard_scores,
                Some(scores),
                neural_weight,
            )
        })
        .unwrap_or_else(|| baseline.clone());

    if let Some(action) = neural_scores.as_ref().and_then(|scores| {
        select_neural_v2_self_kong(
            context,
            scores,
            best_kong.as_ref(),
            baseline.score,
            engine.kong_margin(),
        )
    }) {
        return Some(action);
    }

    if let Some((action, score)) = best_kong {
        if score > baseline.score + engine.kong_margin() {
            return Some(action);
        }
    }

    Some(BotAction {
        seat_index: context.seat_index,
        action_type: "discard".to_string(),
        tile_ids: vec![selected_discard.tile_id],
    })
}

pub fn choose_claim_action(context: &BotContext) -> Option<BotAction> {
    choose_claim_action_with_config(context, &bot_policy_config_from_env())
}

pub fn choose_claim_action_with_config(
    context: &BotContext,
    config: &ArenaBotPolicyConfig,
) -> Option<BotAction> {
    let policy_mode = bot_policy_mode_from_config(config);
    let neural_weight = config.neural_weight.max(0);
    choose_claim_action_inner(context, policy_mode, neural_weight, config)
}

fn choose_claim_action_inner(
    context: &BotContext,
    policy_mode: BotPolicyMode,
    neural_weight: i64,
    config: &ArenaBotPolicyConfig,
) -> Option<BotAction> {
    if policy_mode == BotPolicyMode::Neural {
        if let Some(scores) = neural_decision_scores_for_policy(context, config) {
            if let Some(action) = select_neural_only_claim(context, &scores) {
                return Some(action);
            }
        }
    }

    let mut engine = SearchEngine::new(context);
    let pass_score = engine.score_13_tile_hand(
        context,
        &context.player.concealed_tile_counts,
        &context.player.meld_tile_key_groups,
        &[],
        STAGE_ONE_DEPTH,
    );
    let pass_signals = engine.strategic_signals_for_state(
        context,
        &context.player.concealed_tile_counts,
        &context.player.meld_tile_key_groups,
        &[],
    );
    let discard_tile_key = context.last_discard_tile_key.as_deref()?;

    let mut claim_plans = Vec::new();
    for option in &context.claim_options {
        let concealed_counts_after = tile_counts_after_removal(
            &context.player.concealed_tiles,
            &context.player.concealed_tile_counts,
            &option.tile_ids,
        );
        let mut meld_groups_after = context.player.meld_tile_key_groups.clone();
        let claim_meld = claim_meld_tile_keys(
            &option.action_type,
            discard_tile_key,
            &option.tile_ids,
            &context.player.concealed_tiles,
        );
        let appended_open_flags = vec![true];
        meld_groups_after.push(claim_meld.clone());

        let total_score = if option.action_type == "kong" {
            engine.expected_score_after_forced_draw(
                context,
                &concealed_counts_after,
                &meld_groups_after,
                &appended_open_flags,
                Some(discard_tile_key),
                STAGE_ONE_DEPTH,
            )? + 140
        } else {
            let concealed_after =
                simulated_tiles_after_removal(&context.player.concealed_tiles, &option.tile_ids);
            let plan = engine.best_discard_plan(
                context,
                &concealed_after,
                &concealed_counts_after,
                &meld_groups_after,
                &appended_open_flags,
                Some(discard_tile_key),
                None,
            )?;
            let signals = engine.strategic_signals_for_state(
                context,
                &concealed_counts_after,
                &meld_groups_after,
                &appended_open_flags,
            );
            let action_bonus = claim_action_bonus(
                context,
                &option.action_type,
                &claim_meld,
                pass_signals,
                signals,
            );
            plan.score + action_bonus
        };

        claim_plans.push((
            BotAction {
                seat_index: context.seat_index,
                action_type: option.action_type.clone(),
                tile_ids: option.tile_ids.clone(),
            },
            total_score,
        ));
    }

    if policy_mode == BotPolicyMode::Hybrid {
        if let Some(scores) = neural_decision_scores_for_policy(context, config) {
            if let Some(action) = select_hybrid_claim(
                context,
                &scores,
                pass_score,
                &claim_plans,
                engine.claim_margin(),
                neural_weight,
            ) {
                return Some(action);
            }
        }
    }

    if let Some((action, score)) = best_claim_plan(&claim_plans) {
        if *score > pass_score + engine.claim_margin() {
            return Some(action.clone());
        }
    }

    Some(BotAction {
        seat_index: context.seat_index,
        action_type: "pass".to_string(),
        tile_ids: vec![],
    })
}

pub(crate) fn bot_policy_config_from_env() -> ArenaBotPolicyConfig {
    let mode = match bot_policy_mode() {
        BotPolicyMode::Heuristic => ArenaPolicyMode::Heuristic,
        BotPolicyMode::Hybrid => ArenaPolicyMode::Hybrid,
        BotPolicyMode::Neural => ArenaPolicyMode::Neural,
    };
    ArenaBotPolicyConfig {
        id: match mode {
            ArenaPolicyMode::Heuristic => "env-heuristic",
            ArenaPolicyMode::Hybrid => "env-hybrid",
            ArenaPolicyMode::Neural => "env-neural",
        }
        .to_string(),
        mode,
        neural_weight: neural_prior_weight(),
        model_path: env::var("MAHJONG_BOT_MODEL_PATH").ok(),
    }
}

fn bot_policy_mode() -> BotPolicyMode {
    match env::var(POLICY_ENV).ok().as_deref() {
        Some(value) if value.eq_ignore_ascii_case("neural") => BotPolicyMode::Neural,
        Some(value) if value.eq_ignore_ascii_case("hybrid") => BotPolicyMode::Hybrid,
        _ => BotPolicyMode::Heuristic,
    }
}

fn bot_policy_mode_from_config(config: &ArenaBotPolicyConfig) -> BotPolicyMode {
    match config.mode {
        ArenaPolicyMode::Heuristic => BotPolicyMode::Heuristic,
        ArenaPolicyMode::Hybrid => BotPolicyMode::Hybrid,
        ArenaPolicyMode::Neural => BotPolicyMode::Neural,
    }
}

fn neural_decision_scores_for_policy(
    context: &BotContext,
    config: &ArenaBotPolicyConfig,
) -> Option<NeuralDecisionScores> {
    let path = config.model_path.as_deref().map(std::path::Path::new);
    super::neural::neural_decision_scores_for_model_path(context, path)
}

fn trace_discard_decision_if_enabled(
    context: &BotContext,
    engine: &SearchEngine,
    baseline: &super::search::BotDiscardPlan,
    elapsed: std::time::Duration,
) {
    if env::var_os("MAHJONG_BOT_TRACE").is_none() {
        return;
    }
    let Some(telemetry) = engine.last_discard_telemetry() else {
        return;
    };
    eprintln!(
        "bot-discard seat={} tile={} score={} elapsed_ms={} stage1={} gap={:?} stage2={} ran_stage2={} ran_mc={}",
        context.seat_index,
        baseline.tile_key,
        baseline.score,
        elapsed.as_millis(),
        telemetry.stage_one_candidates,
        telemetry.finalist_gap,
        telemetry.stage_two_candidates,
        telemetry.ran_stage_two,
        telemetry.ran_monte_carlo,
    );
}

fn tile_counts_after_removal(
    concealed_tiles: &[BotTileView],
    concealed_counts: &TileCounts,
    removed_tile_ids: &[String],
) -> TileCounts {
    let mut counts = *concealed_counts;
    for removed_tile_id in removed_tile_ids {
        let Some(tile) = concealed_tiles
            .iter()
            .find(|tile| tile.tile_id == *removed_tile_id)
        else {
            continue;
        };
        let Some(tile_index) = tile_index(&tile.tile_key) else {
            continue;
        };
        counts[tile_index] = counts[tile_index].saturating_sub(1);
    }
    counts
}

fn select_neural_only_active_turn_action(
    context: &BotContext,
    scores: &NeuralDecisionScores,
) -> Option<BotAction> {
    if let Some(action) = select_neural_only_self_kong(context, scores) {
        return Some(action);
    }
    select_neural_only_discard_plan(&rank_masked_discards(context, &scores.discard_logits)).map(
        |plan| BotAction {
            seat_index: context.seat_index,
            action_type: "discard".to_string(),
            tile_ids: vec![plan.tile_id],
        },
    )
}

fn neural_prior_weight() -> i64 {
    env::var("MAHJONG_BOT_NEURAL_WEIGHT")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(DEFAULT_NEURAL_PRIOR_WEIGHT)
        .max(0)
}

fn select_discard_plan_for_policy(
    policy_mode: BotPolicyMode,
    search_plans: &[BotDiscardPlan],
    neural_scores: &[RankedTileScore],
    decision_scores: Option<&NeuralDecisionScores>,
    neural_weight: i64,
) -> Option<BotDiscardPlan> {
    match policy_mode {
        BotPolicyMode::Neural => select_neural_only_discard_plan(neural_scores),
        BotPolicyMode::Hybrid => decision_scores
            .and_then(|scores| {
                select_hybrid_discard_plan(
                    search_plans,
                    neural_scores,
                    &scores.risk_logits,
                    scores.value,
                    neural_weight,
                )
            })
            .or_else(|| search_plans.first().cloned()),
        BotPolicyMode::Heuristic => search_plans.first().cloned(),
    }
}

fn select_neural_only_discard_plan(neural_scores: &[RankedTileScore]) -> Option<BotDiscardPlan> {
    neural_scores.first().map(|score| BotDiscardPlan {
        tile_id: score.tile_id.clone(),
        tile_key: score.tile_key.clone(),
        score: 0,
    })
}

fn select_hybrid_discard_plan(
    search_plans: &[BotDiscardPlan],
    neural_scores: &[RankedTileScore],
    risk_logits: &[f32; TILE_KIND_COUNT],
    model_value: f32,
    weight: i64,
) -> Option<BotDiscardPlan> {
    let best_search = search_plans.first()?.clone();
    let candidate_plans = hybrid_discard_candidates(search_plans, neural_scores, &best_search);
    if candidate_plans.len() <= 1 {
        return Some(best_search);
    }
    let mut best = None;
    let candidate_neural_scores = neural_scores
        .iter()
        .filter(|score| {
            candidate_plans
                .iter()
                .any(|plan| plan.tile_key == score.tile_key)
        })
        .collect::<Vec<_>>();
    let min_logit = candidate_neural_scores
        .iter()
        .map(|score| score.logit)
        .fold(f32::INFINITY, f32::min);
    let max_logit = candidate_neural_scores
        .iter()
        .map(|score| score.logit)
        .fold(f32::NEG_INFINITY, f32::max);
    let logit_range = max_logit - min_logit;
    let candidate_risks = candidate_plans
        .iter()
        .filter_map(|plan| tile_index(&plan.tile_key).map(|index| risk_logits[index]))
        .collect::<Vec<_>>();
    let min_risk = candidate_risks
        .iter()
        .copied()
        .fold(f32::INFINITY, f32::min);
    let max_risk = candidate_risks
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);
    let risk_range = max_risk - min_risk;
    let authority_weight = neural_authority_weight(weight, model_value);
    let risk_weight = neural_risk_weight(weight);

    for plan in &candidate_plans {
        let neural_bonus = candidate_neural_scores
            .iter()
            .find(|score| score.tile_key == plan.tile_key)
            .map(|score| {
                normalized_model_bonus(score.logit, min_logit, logit_range, authority_weight)
            })
            .unwrap_or(0);
        let risk_penalty = tile_index(&plan.tile_key)
            .map(|index| {
                normalized_model_bonus(risk_logits[index], min_risk, risk_range, risk_weight)
            })
            .unwrap_or(0);
        let final_score = plan.score + neural_bonus - risk_penalty;
        let replace = best
            .as_ref()
            .map(|(selected, selected_score): &(BotDiscardPlan, i64)| {
                final_score > *selected_score
                    || (final_score == *selected_score && plan.tile_key > selected.tile_key)
            })
            .unwrap_or(true);
        if replace {
            best = Some((plan.clone(), final_score));
        }
    }

    best.map(|(plan, _)| plan)
}

fn hybrid_discard_candidates(
    search_plans: &[BotDiscardPlan],
    neural_scores: &[RankedTileScore],
    best_search: &BotDiscardPlan,
) -> Vec<BotDiscardPlan> {
    let mut candidates = search_plans
        .iter()
        .take(HYBRID_TOP_SEARCH_CANDIDATES)
        .take_while(|plan| best_search.score - plan.score <= HYBRID_CLOSE_SEARCH_GAP)
        .cloned()
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        candidates.push(best_search.clone());
    }
    if let Some(top_neural) = neural_scores.first() {
        if let Some(model_plan) = search_plans
            .iter()
            .find(|plan| plan.tile_key == top_neural.tile_key)
        {
            let already_included = candidates
                .iter()
                .any(|plan| plan.tile_key == model_plan.tile_key);
            if best_search.score - model_plan.score <= HYBRID_NEURAL_OVERRIDE_GAP
                && !already_included
            {
                candidates.push(model_plan.clone());
            }
        }
    }
    candidates
}

fn neural_authority_weight(base_weight: i64, model_value: f32) -> i64 {
    let value_adjustment = (model_value.clamp(-4.0, 4.0) * 4.0).round() as i64;
    (base_weight + value_adjustment).max(0)
}

fn neural_risk_weight(base_weight: i64) -> i64 {
    if base_weight == 0 {
        0
    } else {
        ((base_weight * 3) / 4).max(10)
    }
}

fn neural_claim_weight(base_weight: i64, model_value: f32) -> i64 {
    let value_adjustment = (model_value.clamp(-4.0, 4.0) * 6.0).round() as i64;
    (base_weight * 2 + value_adjustment).max(0)
}

fn normalized_model_bonus(value: f32, min_value: f32, value_range: f32, weight: i64) -> i64 {
    if weight == 0
        || value_range.abs() < f32::EPSILON
        || !value.is_finite()
        || !min_value.is_finite()
        || !value_range.is_finite()
    {
        return 0;
    }
    let normalized = ((value - min_value) / value_range) * 2.0 - 1.0;
    (normalized * weight as f32).round() as i64
}

fn select_neural_only_claim(
    context: &BotContext,
    scores: &NeuralDecisionScores,
) -> Option<BotAction> {
    let ranked = rank_masked_claims(context, &scores.claim_logits);
    let best = ranked.first()?;
    if best.action_name == "pass" {
        return Some(pass_action(context.seat_index));
    }
    let option = claim_option_for_ranked_action(context, best.action_name)?;
    Some(BotAction {
        seat_index: context.seat_index,
        action_type: option.action_type.clone(),
        tile_ids: option.tile_ids.clone(),
    })
}

fn select_hybrid_claim(
    context: &BotContext,
    scores: &NeuralDecisionScores,
    pass_score: i64,
    search_claims: &[(BotAction, i64)],
    claim_margin: i64,
    neural_weight: i64,
) -> Option<BotAction> {
    let ranked = rank_masked_claims(context, &scores.claim_logits);
    let best = ranked.first()?;
    if best.action_name == "pass" {
        return Some(pass_action(context.seat_index));
    }
    if let Some(action) = select_neural_hu_claim(context, scores, best.action_name) {
        return Some(action);
    }

    let min_logit = ranked
        .iter()
        .map(|score| score.logit)
        .fold(f32::INFINITY, f32::min);
    let max_logit = ranked
        .iter()
        .map(|score| score.logit)
        .fold(f32::NEG_INFINITY, f32::max);
    let logit_range = max_logit - min_logit;
    let claim_weight = neural_claim_weight(neural_weight, scores.value);
    let mut best_scored_claim = None;

    for (search_action, search_score) in search_claims {
        let Some(option) = context
            .claim_options
            .iter()
            .find(|option| claim_action_matches_option(search_action, option))
        else {
            continue;
        };
        let action_name = if option.action_type == "chow" {
            claim_chow_action_name(context, option)
        } else {
            option.action_type.as_str()
        };
        let Some(logit) = ranked
            .iter()
            .find(|score| score.action_name == action_name)
            .map(|score| score.logit)
        else {
            continue;
        };
        let neural_bonus = normalized_model_bonus(logit, min_logit, logit_range, claim_weight);
        let final_score = *search_score + neural_bonus;
        let replace = best_scored_claim
            .as_ref()
            .map(|(_, selected_score): &(BotAction, i64)| final_score > *selected_score)
            .unwrap_or(true);
        if replace {
            best_scored_claim = Some((search_action.clone(), final_score));
        }
    }

    best_scored_claim
        .filter(|(_, score)| *score >= pass_score - claim_margin)
        .map(|(action, _)| action)
}

fn select_neural_hu_claim(
    context: &BotContext,
    scores: &NeuralDecisionScores,
    best_claim_action: &str,
) -> Option<BotAction> {
    let option = claim_option_for_ranked_action(context, "hu")?;
    if best_claim_action == "hu" || scores.hu_logits[1] > scores.hu_logits[0] {
        return Some(BotAction {
            seat_index: context.seat_index,
            action_type: option.action_type.clone(),
            tile_ids: option.tile_ids.clone(),
        });
    }
    None
}

fn select_neural_only_self_kong(
    context: &BotContext,
    scores: &NeuralDecisionScores,
) -> Option<BotAction> {
    if context.self_kong_candidates.is_empty() {
        return None;
    }
    let pass_logit = scores.self_kong_logits[0];
    let mut best = None;
    for candidate in &context.self_kong_candidates {
        if candidate.kind == BotSelfKongKind::Add
            && context.add_kong_risk_tiles.contains(&candidate.tile_key)
        {
            continue;
        }
        let index = match candidate.kind {
            BotSelfKongKind::Concealed => 1,
            BotSelfKongKind::Add => 2,
        };
        let logit = scores.self_kong_logits[index];
        let replace = best
            .as_ref()
            .map(|(_, selected_logit): &(BotAction, f32)| logit > *selected_logit)
            .unwrap_or(true);
        if replace {
            best = Some((
                BotAction {
                    seat_index: context.seat_index,
                    action_type: "kong".to_string(),
                    tile_ids: candidate.tile_ids.clone(),
                },
                logit,
            ));
        }
    }
    let (action, logit) = best?;
    (logit > pass_logit).then_some(action)
}

fn select_neural_v2_self_kong(
    context: &BotContext,
    scores: &NeuralDecisionScores,
    best_search_kong: Option<&(BotAction, i64)>,
    baseline_score: i64,
    kong_margin: i64,
) -> Option<BotAction> {
    let action = select_neural_only_self_kong(context, scores)?;
    if let Some((search_action, search_score)) = best_search_kong {
        if search_action.tile_ids == action.tile_ids
            && *search_score >= baseline_score - kong_margin
        {
            return Some(action);
        }
    }
    None
}

fn claim_option_for_ranked_action<'a>(
    context: &'a BotContext,
    ranked_action_name: &str,
) -> Option<&'a crate::projection::bot_view::BotClaimOption> {
    context.claim_options.iter().find(|option| {
        if option.action_type == "chow" {
            claim_chow_action_name(context, option) == ranked_action_name
        } else {
            option.action_type == ranked_action_name
        }
    })
}

fn claim_chow_action_name(
    context: &BotContext,
    option: &crate::projection::bot_view::BotClaimOption,
) -> &'static str {
    let Some(last_discard) = context.last_discard_tile_key.as_deref() else {
        return "chow_mid";
    };
    let Some(discard_index) = tile_index(last_discard) else {
        return "chow_mid";
    };
    if discard_index >= HONOR_TILE_START {
        return "chow_mid";
    }

    let mut keys = vec![last_discard.to_string()];
    for tile_id in &option.tile_ids {
        let Some(tile) = context
            .player
            .concealed_tiles
            .iter()
            .find(|tile| &tile.tile_id == tile_id)
        else {
            return "chow_mid";
        };
        keys.push(tile.tile_key.clone());
    }

    keys.sort_by_key(|key| tile_index(key).unwrap_or(usize::MAX));
    let Some(middle_index) = keys.get(1).and_then(|key| tile_index(key)) else {
        return "chow_mid";
    };
    if middle_index >= HONOR_TILE_START || middle_index / 9 != discard_index / 9 {
        return "chow_mid";
    }
    if discard_index == middle_index - 1 {
        return "chow_left";
    }
    if discard_index == middle_index + 1 {
        return "chow_right";
    }
    "chow_mid"
}

fn claim_action_matches_option(
    action: &BotAction,
    option: &crate::projection::bot_view::BotClaimOption,
) -> bool {
    action.action_type == option.action_type && action.tile_ids == option.tile_ids
}

fn best_claim_plan(claim_plans: &[(BotAction, i64)]) -> Option<&(BotAction, i64)> {
    claim_plans
        .iter()
        .max_by(|(_, left), (_, right)| left.cmp(right))
}

fn pass_action(seat_index: usize) -> BotAction {
    BotAction {
        seat_index,
        action_type: "pass".to_string(),
        tile_ids: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::{neural, search};

    fn tiles(keys: &[&str]) -> Vec<BotTileView> {
        keys.iter()
            .enumerate()
            .map(|(index, key)| BotTileView {
                tile_id: format!("{key}-{index}"),
                tile_key: (*key).to_string(),
                is_flower: false,
            })
            .collect()
    }

    fn base_context() -> BotContext {
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
            kong_entries: Vec::new(),
            player: BotPlayerContext {
                concealed_tiles: Vec::new(),
                concealed_tile_counts: [0; TILE_KIND_COUNT],
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

    fn neutral_risk_logits() -> [f32; TILE_KIND_COUNT] {
        [0.0; TILE_KIND_COUNT]
    }

    #[test]
    fn bot_still_prefers_standard_actions_when_discarding() {
        let mut context = base_context();
        context.wall_tiles_remaining = 14;
        context.opponent_melds_by_seat[1] = vec![
            vec!["w3".to_string(), "w4".to_string(), "w5".to_string()],
            vec!["red".to_string(), "red".to_string(), "red".to_string()],
        ];
        context.opponent_discards_by_seat[1] = vec![
            "white".to_string(),
            "north".to_string(),
            "b9".to_string(),
            "w9".to_string(),
        ];
        let concealed_tiles = tiles(&[
            "w1", "w2", "w3", "t1", "t2", "t3", "b1", "b2", "b3", "east", "east", "green", "w9",
            "w6",
        ]);
        context.player.concealed_tile_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));
        context.player.concealed_tiles = concealed_tiles;

        let action = choose_active_turn_action(&context).expect("action");
        assert_eq!(action.action_type, "discard");
    }

    #[test]
    fn explicit_heuristic_config_uses_existing_search_path() {
        let mut context = base_context();
        let concealed_tiles = tiles(&[
            "w1", "w2", "w3", "t1", "t2", "t3", "b1", "b2", "b3", "east", "east", "green", "w9",
            "w6",
        ]);
        context.player.concealed_tile_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));
        context.player.concealed_tiles = concealed_tiles;
        let config = crate::bot::arena::ArenaBotPolicyConfig::heuristic();

        let action = choose_active_turn_action_with_config(&context, &config).expect("action");

        assert_eq!(action.seat_index, 0);
        assert_eq!(action.action_type, "discard");
        assert_eq!(action.tile_ids.len(), 1);
    }

    #[test]
    fn tile_counts_after_removal_subtracts_requested_tiles_only() {
        let concealed_tiles = tiles(&["w1", "w1", "east", "red"]);
        let concealed_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));

        let counts = tile_counts_after_removal(
            &concealed_tiles,
            &concealed_counts,
            &[
                concealed_tiles[1].tile_id.clone(),
                concealed_tiles[3].tile_id.clone(),
            ],
        );

        assert_eq!(counts[tile_index("w1").expect("tile index")], 1);
        assert_eq!(counts[tile_index("east").expect("tile index")], 1);
        assert_eq!(counts[tile_index("red").expect("tile index")], 0);
    }

    #[test]
    fn ranked_chow_action_matches_the_same_chow_shape() {
        let mut context = base_context();
        let concealed_tiles = tiles(&["w2", "w4", "w4", "w5"]);
        context.player.concealed_tile_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));
        context.player.concealed_tiles = concealed_tiles.clone();
        context.last_discard_tile_key = Some("w3".to_string());
        context.claim_options = vec![
            BotClaimOption {
                action_type: "chow".to_string(),
                tile_ids: vec![
                    concealed_tiles[0].tile_id.clone(),
                    concealed_tiles[1].tile_id.clone(),
                ],
            },
            BotClaimOption {
                action_type: "chow".to_string(),
                tile_ids: vec![
                    concealed_tiles[2].tile_id.clone(),
                    concealed_tiles[3].tile_id.clone(),
                ],
            },
        ];

        let selected = claim_option_for_ranked_action(&context, "chow_left").expect("chow option");

        assert_eq!(
            selected.tile_ids,
            vec![
                concealed_tiles[2].tile_id.clone(),
                concealed_tiles[3].tile_id.clone()
            ]
        );
    }

    #[test]
    fn hybrid_claim_can_choose_model_favored_non_search_best_claim() {
        let mut context = base_context();
        let concealed_tiles = tiles(&["w2", "w4", "w3", "w3"]);
        context.player.concealed_tile_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));
        context.player.concealed_tiles = concealed_tiles.clone();
        context.last_discard_tile_key = Some("w3".to_string());
        context.claim_options = vec![
            BotClaimOption {
                action_type: "chow".to_string(),
                tile_ids: vec![
                    concealed_tiles[0].tile_id.clone(),
                    concealed_tiles[1].tile_id.clone(),
                ],
            },
            BotClaimOption {
                action_type: "pung".to_string(),
                tile_ids: vec![
                    concealed_tiles[2].tile_id.clone(),
                    concealed_tiles[3].tile_id.clone(),
                ],
            },
        ];
        let mut claim_logits = [0.0; crate::bot::action_space::CLAIM_ACTION_COUNT];
        claim_logits[crate::bot::action_space::claim_action_index("pass").expect("pass index")] =
            -1.0;
        claim_logits
            [crate::bot::action_space::claim_action_index("chow_mid").expect("chow index")] = 9.0;
        claim_logits[crate::bot::action_space::claim_action_index("pung").expect("pung index")] =
            0.0;
        let scores = NeuralDecisionScores {
            discard_logits: [0.0; TILE_KIND_COUNT],
            claim_logits,
            self_kong_logits: [0.0; crate::bot::action_space::SELF_KONG_ACTION_COUNT],
            hu_logits: [0.0, 0.0],
            value: 0.0,
            risk_logits: [0.0; TILE_KIND_COUNT],
        };
        let search_pung = BotAction {
            seat_index: context.seat_index,
            action_type: "pung".to_string(),
            tile_ids: context.claim_options[1].tile_ids.clone(),
        };
        let search_chow = BotAction {
            seat_index: context.seat_index,
            action_type: "chow".to_string(),
            tile_ids: context.claim_options[0].tile_ids.clone(),
        };
        let search_claims = vec![(search_pung, 1000), (search_chow, 970)];

        let selected = select_hybrid_claim(&context, &scores, 960, &search_claims, 100, 40)
            .expect("hybrid claim");

        assert_eq!(selected.action_type, "chow");
        assert_eq!(selected.tile_ids, context.claim_options[0].tile_ids);
    }

    #[test]
    fn neural_prior_can_break_close_search_score_tie() {
        let search_plans = vec![
            search::BotDiscardPlan {
                tile_id: "w1#0".to_string(),
                tile_key: "w1".to_string(),
                score: 1000,
            },
            search::BotDiscardPlan {
                tile_id: "t1#0".to_string(),
                tile_key: "t1".to_string(),
                score: 980,
            },
        ];
        let neural_scores = vec![
            neural::RankedTileScore {
                tile_id: "w1#0".to_string(),
                tile_key: "w1".to_string(),
                logit: 0.0,
            },
            neural::RankedTileScore {
                tile_id: "t1#0".to_string(),
                tile_key: "t1".to_string(),
                logit: 4.0,
            },
        ];

        let selected = select_hybrid_discard_plan(
            &search_plans,
            &neural_scores,
            &neutral_risk_logits(),
            0.0,
            80,
        )
        .expect("hybrid selection");

        assert_eq!(selected.tile_key, "t1");
    }

    #[test]
    fn neural_prior_can_override_moderate_search_gap_in_hybrid() {
        let search_plans = vec![
            search::BotDiscardPlan {
                tile_id: "w1#0".to_string(),
                tile_key: "w1".to_string(),
                score: 1000,
            },
            search::BotDiscardPlan {
                tile_id: "t1#0".to_string(),
                tile_key: "t1".to_string(),
                score: 940,
            },
        ];
        let neural_scores = vec![
            neural::RankedTileScore {
                tile_id: "t1#0".to_string(),
                tile_key: "t1".to_string(),
                logit: 8.0,
            },
            neural::RankedTileScore {
                tile_id: "w1#0".to_string(),
                tile_key: "w1".to_string(),
                logit: 0.0,
            },
        ];

        let selected = select_hybrid_discard_plan(
            &search_plans,
            &neural_scores,
            &neutral_risk_logits(),
            0.0,
            80,
        )
        .expect("hybrid selection");

        assert_eq!(selected.tile_key, "t1");
    }

    #[test]
    fn hybrid_discard_uses_risk_head_to_avoid_dangerous_tile() {
        let search_plans = vec![
            search::BotDiscardPlan {
                tile_id: "w1#0".to_string(),
                tile_key: "w1".to_string(),
                score: 1000,
            },
            search::BotDiscardPlan {
                tile_id: "t1#0".to_string(),
                tile_key: "t1".to_string(),
                score: 995,
            },
        ];
        let neural_scores = vec![
            neural::RankedTileScore {
                tile_id: "w1#0".to_string(),
                tile_key: "w1".to_string(),
                logit: 4.0,
            },
            neural::RankedTileScore {
                tile_id: "t1#0".to_string(),
                tile_key: "t1".to_string(),
                logit: 4.0,
            },
        ];
        let mut risk_logits = [0.0; TILE_KIND_COUNT];
        risk_logits[tile_index("w1").expect("tile index")] = 8.0;
        risk_logits[tile_index("t1").expect("tile index")] = -2.0;

        let selected =
            select_hybrid_discard_plan(&search_plans, &neural_scores, &risk_logits, 0.0, 40)
                .expect("hybrid selection");

        assert_eq!(selected.tile_key, "t1");
    }

    #[test]
    fn neural_prior_cannot_override_clear_search_lead() {
        let search_plans = vec![
            search::BotDiscardPlan {
                tile_id: "w1#0".to_string(),
                tile_key: "w1".to_string(),
                score: 1000,
            },
            search::BotDiscardPlan {
                tile_id: "t1#0".to_string(),
                tile_key: "t1".to_string(),
                score: 700,
            },
        ];
        let neural_scores = vec![
            neural::RankedTileScore {
                tile_id: "w1#0".to_string(),
                tile_key: "w1".to_string(),
                logit: 0.0,
            },
            neural::RankedTileScore {
                tile_id: "t1#0".to_string(),
                tile_key: "t1".to_string(),
                logit: 4.0,
            },
        ];

        let selected = select_hybrid_discard_plan(
            &search_plans,
            &neural_scores,
            &neutral_risk_logits(),
            0.0,
            80,
        )
        .expect("hybrid selection");

        assert_eq!(selected.tile_key, "w1");
    }

    #[test]
    fn neural_prior_can_choose_fourth_search_candidate_when_close() {
        let search_plans = vec![
            search::BotDiscardPlan {
                tile_id: "w1#0".to_string(),
                tile_key: "w1".to_string(),
                score: 1000,
            },
            search::BotDiscardPlan {
                tile_id: "t1#0".to_string(),
                tile_key: "t1".to_string(),
                score: 998,
            },
            search::BotDiscardPlan {
                tile_id: "b1#0".to_string(),
                tile_key: "b1".to_string(),
                score: 997,
            },
            search::BotDiscardPlan {
                tile_id: "red#0".to_string(),
                tile_key: "red".to_string(),
                score: 996,
            },
        ];
        let neural_scores = vec![
            neural::RankedTileScore {
                tile_id: "w1#0".to_string(),
                tile_key: "w1".to_string(),
                logit: 0.0,
            },
            neural::RankedTileScore {
                tile_id: "t1#0".to_string(),
                tile_key: "t1".to_string(),
                logit: 1.0,
            },
            neural::RankedTileScore {
                tile_id: "b1#0".to_string(),
                tile_key: "b1".to_string(),
                logit: 2.0,
            },
            neural::RankedTileScore {
                tile_id: "red#0".to_string(),
                tile_key: "red".to_string(),
                logit: 100.0,
            },
        ];

        let selected = select_hybrid_discard_plan(
            &search_plans,
            &neural_scores,
            &neutral_risk_logits(),
            0.0,
            80,
        )
        .expect("hybrid selection");

        assert_eq!(selected.tile_key, "red");
    }

    #[test]
    fn neural_prior_weight_zero_keeps_search_ranking() {
        let search_plans = vec![
            search::BotDiscardPlan {
                tile_id: "w1#0".to_string(),
                tile_key: "w1".to_string(),
                score: 1000,
            },
            search::BotDiscardPlan {
                tile_id: "t1#0".to_string(),
                tile_key: "t1".to_string(),
                score: 980,
            },
        ];
        let neural_scores = vec![neural::RankedTileScore {
            tile_id: "t1#0".to_string(),
            tile_key: "t1".to_string(),
            logit: 99.0,
        }];

        let selected = select_hybrid_discard_plan(
            &search_plans,
            &neural_scores,
            &neutral_risk_logits(),
            0.0,
            0,
        )
        .expect("hybrid selection");

        assert_eq!(selected.tile_key, "w1");
    }

    #[test]
    fn neural_policy_uses_top_neural_discard_even_when_search_disagrees() {
        let search_plans = vec![
            search::BotDiscardPlan {
                tile_id: "w1#0".to_string(),
                tile_key: "w1".to_string(),
                score: 1000,
            },
            search::BotDiscardPlan {
                tile_id: "t1#0".to_string(),
                tile_key: "t1".to_string(),
                score: 700,
            },
        ];
        let neural_scores = vec![
            neural::RankedTileScore {
                tile_id: "t1#0".to_string(),
                tile_key: "t1".to_string(),
                logit: 9.0,
            },
            neural::RankedTileScore {
                tile_id: "w1#0".to_string(),
                tile_key: "w1".to_string(),
                logit: 0.0,
            },
        ];

        let selected = select_discard_plan_for_policy(
            BotPolicyMode::Neural,
            &search_plans,
            &neural_scores,
            None,
            80,
        )
        .expect("neural selection");

        assert_eq!(selected.tile_key, "t1");
    }

    #[test]
    fn hybrid_policy_keeps_search_lead_when_neural_disagrees() {
        let search_plans = vec![
            search::BotDiscardPlan {
                tile_id: "w1#0".to_string(),
                tile_key: "w1".to_string(),
                score: 1000,
            },
            search::BotDiscardPlan {
                tile_id: "t1#0".to_string(),
                tile_key: "t1".to_string(),
                score: 700,
            },
        ];
        let neural_scores = vec![
            neural::RankedTileScore {
                tile_id: "t1#0".to_string(),
                tile_key: "t1".to_string(),
                logit: 9.0,
            },
            neural::RankedTileScore {
                tile_id: "w1#0".to_string(),
                tile_key: "w1".to_string(),
                logit: 0.0,
            },
        ];

        let selected = select_discard_plan_for_policy(
            BotPolicyMode::Hybrid,
            &search_plans,
            &neural_scores,
            None,
            80,
        )
        .expect("hybrid selection");

        assert_eq!(selected.tile_key, "w1");
    }
}
