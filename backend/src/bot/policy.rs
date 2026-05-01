use super::context::*;
use super::neural::{
    NeuralDecisionScores, RankedTileScore, rank_masked_claims, rank_masked_discards,
};
use super::search::{
    BotDiscardPlan, STAGE_ONE_DEPTH, SearchEngine, claim_action_bonus, claim_meld_tile_keys,
    simulated_tiles_after_removal,
};
use crate::bot::arena::{ArenaBotPolicyConfig, ArenaPolicyMode};
use rand::{Rng, rngs::StdRng};
use std::{env, time::Instant};

const POLICY_ENV: &str = "MAHJONG_BOT_POLICY";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BotPolicyMode {
    Heuristic,
    Neural,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct BotPolicyDecisionTelemetry {
    pub(crate) model_loaded: bool,
    pub(crate) used_neural_action: bool,
    pub(crate) used_fallback: bool,
    pub(crate) same_as_heuristic: Option<bool>,
}

#[derive(Clone)]
pub(crate) struct BotPolicyDecision {
    pub(crate) action: BotAction,
    pub(crate) telemetry: BotPolicyDecisionTelemetry,
}

pub fn choose_active_turn_action(context: &BotContext) -> Option<BotAction> {
    choose_active_turn_action_with_config(context, &bot_policy_config_from_env())
}

pub fn choose_active_turn_action_with_config(
    context: &BotContext,
    config: &ArenaBotPolicyConfig,
) -> Option<BotAction> {
    let policy_mode = bot_policy_mode_from_config(config);
    choose_active_turn_action_inner(context, policy_mode, config)
}

fn choose_active_turn_action_inner(
    context: &BotContext,
    policy_mode: BotPolicyMode,
    config: &ArenaBotPolicyConfig,
) -> Option<BotAction> {
    if policy_mode == BotPolicyMode::Neural {
        if let Some(scores) = neural_decision_scores_for_policy(context, config) {
            if let Some(action) = select_neural_only_active_turn_action(context, &scores) {
                return Some(action);
            }
        }
    }

    choose_heuristic_active_turn_action(context)
}

fn choose_heuristic_active_turn_action(context: &BotContext) -> Option<BotAction> {
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

    if let Some((action, score)) = best_kong {
        if score > baseline.score + engine.kong_margin() {
            return Some(action);
        }
    }

    Some(BotAction {
        seat_index: context.seat_index,
        action_type: "discard".to_string(),
        tile_ids: vec![baseline.tile_id],
    })
}

pub(crate) fn choose_active_turn_decision_with_config_and_rng(
    context: &BotContext,
    config: &ArenaBotPolicyConfig,
    mut rng: Option<&mut StdRng>,
) -> Option<BotPolicyDecision> {
    let mut telemetry = BotPolicyDecisionTelemetry::default();
    if matches!(config.mode, ArenaPolicyMode::Neural) {
        if config.sample_actions {
            if let Some(rng) = rng.as_deref_mut() {
                if let Some(scores) = neural_decision_scores_for_policy(context, config) {
                    telemetry.model_loaded = true;
                    if let Some(action) =
                        sample_neural_active_turn_action(context, &scores, config.temperature, rng)
                    {
                        telemetry.used_neural_action = true;
                        telemetry.same_as_heuristic =
                            same_as_heuristic_active_turn_action(context, &action);
                        return Some(BotPolicyDecision { action, telemetry });
                    }
                }
                telemetry.used_fallback = true;
                let action = choose_heuristic_active_turn_action(context)?;
                return Some(BotPolicyDecision { action, telemetry });
            }
        }

        if let Some(scores) = neural_decision_scores_for_policy(context, config) {
            telemetry.model_loaded = true;
            if let Some(action) = select_neural_only_active_turn_action(context, &scores) {
                telemetry.used_neural_action = true;
                telemetry.same_as_heuristic =
                    same_as_heuristic_active_turn_action(context, &action);
                return Some(BotPolicyDecision { action, telemetry });
            }
        }
        telemetry.used_fallback = true;
    }

    let action = choose_heuristic_active_turn_action(context)?;
    Some(BotPolicyDecision { action, telemetry })
}

pub fn choose_claim_action(context: &BotContext) -> Option<BotAction> {
    choose_claim_action_with_config(context, &bot_policy_config_from_env())
}

pub fn choose_claim_action_with_config(
    context: &BotContext,
    config: &ArenaBotPolicyConfig,
) -> Option<BotAction> {
    let policy_mode = bot_policy_mode_from_config(config);
    choose_claim_action_inner(context, policy_mode, config)
}

fn choose_claim_action_inner(
    context: &BotContext,
    policy_mode: BotPolicyMode,
    config: &ArenaBotPolicyConfig,
) -> Option<BotAction> {
    if policy_mode == BotPolicyMode::Neural {
        if let Some(scores) = neural_decision_scores_for_policy(context, config) {
            if let Some(action) = select_neural_only_claim(context, &scores) {
                return Some(action);
            }
        }
    }

    choose_heuristic_claim_action(context)
}

fn choose_heuristic_claim_action(context: &BotContext) -> Option<BotAction> {
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

pub(crate) fn choose_claim_decision_with_config_and_rng(
    context: &BotContext,
    config: &ArenaBotPolicyConfig,
    mut rng: Option<&mut StdRng>,
) -> Option<BotPolicyDecision> {
    let mut telemetry = BotPolicyDecisionTelemetry::default();
    if matches!(config.mode, ArenaPolicyMode::Neural) {
        if config.sample_actions {
            if let Some(rng) = rng.as_deref_mut() {
                if let Some(scores) = neural_decision_scores_for_policy(context, config) {
                    telemetry.model_loaded = true;
                    if let Some(action) =
                        sample_neural_claim_action(context, &scores, config.temperature, rng)
                    {
                        telemetry.used_neural_action = true;
                        telemetry.same_as_heuristic =
                            same_as_heuristic_claim_action(context, &action);
                        return Some(BotPolicyDecision { action, telemetry });
                    }
                }
                telemetry.used_fallback = true;
                let action = choose_heuristic_claim_action(context)?;
                return Some(BotPolicyDecision { action, telemetry });
            }
        }

        if let Some(scores) = neural_decision_scores_for_policy(context, config) {
            telemetry.model_loaded = true;
            if let Some(action) = select_neural_only_claim(context, &scores) {
                telemetry.used_neural_action = true;
                telemetry.same_as_heuristic = same_as_heuristic_claim_action(context, &action);
                return Some(BotPolicyDecision { action, telemetry });
            }
        }
        telemetry.used_fallback = true;
    }

    let action = choose_heuristic_claim_action(context)?;
    Some(BotPolicyDecision { action, telemetry })
}

pub(crate) fn bot_policy_config_from_env() -> ArenaBotPolicyConfig {
    let mode = match bot_policy_mode() {
        BotPolicyMode::Heuristic => ArenaPolicyMode::Heuristic,
        BotPolicyMode::Neural => ArenaPolicyMode::Neural,
    };
    ArenaBotPolicyConfig {
        id: match mode {
            ArenaPolicyMode::Heuristic => "env-heuristic",
            ArenaPolicyMode::Neural => "env-neural",
        }
        .to_string(),
        mode,
        model_path: env::var("MAHJONG_BOT_MODEL_PATH").ok(),
        sample_actions: false,
        temperature: 1.0,
    }
}

fn bot_policy_mode() -> BotPolicyMode {
    match env::var(POLICY_ENV).ok().as_deref() {
        Some(value) if value.eq_ignore_ascii_case("neural") => BotPolicyMode::Neural,
        _ => BotPolicyMode::Heuristic,
    }
}

fn bot_policy_mode_from_config(config: &ArenaBotPolicyConfig) -> BotPolicyMode {
    match config.mode {
        ArenaPolicyMode::Heuristic => BotPolicyMode::Heuristic,
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

fn sample_masked_index<const N: usize>(
    logits: &[f32; N],
    mask: &[bool; N],
    temperature: f32,
    rng: &mut StdRng,
) -> Option<usize> {
    let temperature = temperature.clamp(0.05, 5.0);
    let max_logit = logits
        .iter()
        .zip(mask.iter())
        .filter_map(|(logit, allowed)| (*allowed && logit.is_finite()).then_some(*logit))
        .max_by(f32::total_cmp)?;
    let mut weights = [0.0_f32; N];
    let mut total = 0.0_f32;
    for (index, (logit, allowed)) in logits.iter().zip(mask.iter()).enumerate() {
        if !*allowed || !logit.is_finite() {
            continue;
        }
        let weight = ((*logit - max_logit) / temperature).exp();
        if weight.is_finite() && weight > 0.0 {
            weights[index] = weight;
            total += weight;
        }
    }
    if total <= 0.0 || !total.is_finite() {
        return None;
    }
    let mut threshold = rng.random_range(0.0..total);
    for (index, weight) in weights.iter().enumerate() {
        threshold -= *weight;
        if threshold <= 0.0 {
            return Some(index);
        }
    }
    weights.iter().rposition(|weight| *weight > 0.0)
}

fn sample_neural_active_turn_action(
    context: &BotContext,
    scores: &NeuralDecisionScores,
    temperature: f32,
    rng: &mut StdRng,
) -> Option<BotAction> {
    let features = crate::bot::features::encode_bot_context_v2(context);
    if context.self_kong_candidates.is_empty() {
        return sample_neural_discard_action(context, scores, temperature, rng);
    }

    let selected = sample_masked_index(
        &scores.self_kong_logits,
        &features.self_kong_mask,
        temperature,
        rng,
    )?;
    match selected {
        0 => sample_neural_discard_action(context, scores, temperature, rng),
        1 | 2 => {
            let expected_kind = if selected == 1 {
                BotSelfKongKind::Concealed
            } else {
                BotSelfKongKind::Add
            };
            context
                .self_kong_candidates
                .iter()
                .find(|candidate| {
                    candidate.kind == expected_kind
                        && !(candidate.kind == BotSelfKongKind::Add
                            && context.add_kong_risk_tiles.contains(&candidate.tile_key))
                })
                .map(|candidate| BotAction {
                    seat_index: context.seat_index,
                    action_type: "kong".to_string(),
                    tile_ids: candidate.tile_ids.clone(),
                })
                .or_else(|| sample_neural_discard_action(context, scores, temperature, rng))
        }
        _ => sample_neural_discard_action(context, scores, temperature, rng),
    }
}

fn sample_neural_discard_action(
    context: &BotContext,
    scores: &NeuralDecisionScores,
    temperature: f32,
    rng: &mut StdRng,
) -> Option<BotAction> {
    let features = crate::bot::features::encode_bot_context_v2(context);
    let tile_index = sample_masked_index(
        &scores.discard_logits,
        &features.discard_mask,
        temperature,
        rng,
    )?;
    let tile_key = tile_key_for_index(tile_index);
    let tile_id = context
        .player
        .concealed_tiles
        .iter()
        .find(|tile| !tile.is_flower && tile.tile_key == tile_key)
        .map(|tile| tile.tile_id.clone())?;
    Some(BotAction {
        seat_index: context.seat_index,
        action_type: "discard".to_string(),
        tile_ids: vec![tile_id],
    })
}

fn sample_neural_claim_action(
    context: &BotContext,
    scores: &NeuralDecisionScores,
    temperature: f32,
    rng: &mut StdRng,
) -> Option<BotAction> {
    let features = crate::bot::features::encode_bot_context_v2(context);
    let selected =
        sample_masked_index(&scores.claim_logits, &features.claim_mask, temperature, rng)?;
    let action_name = crate::bot::action_space::CLAIM_ACTIONS.get(selected)?;
    if *action_name == "pass" {
        return Some(pass_action(context.seat_index));
    }
    let option = claim_option_for_ranked_action(context, action_name)?;
    Some(BotAction {
        seat_index: context.seat_index,
        action_type: option.action_type.clone(),
        tile_ids: option.tile_ids.clone(),
    })
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

fn select_neural_only_discard_plan(neural_scores: &[RankedTileScore]) -> Option<BotDiscardPlan> {
    neural_scores.first().map(|score| BotDiscardPlan {
        tile_id: score.tile_id.clone(),
        tile_key: score.tile_key.clone(),
        score: 0,
    })
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

fn same_as_heuristic_active_turn_action(context: &BotContext, action: &BotAction) -> Option<bool> {
    choose_heuristic_active_turn_action(context)
        .map(|heuristic| bot_actions_are_equivalent(context, action, &heuristic))
}

fn same_as_heuristic_claim_action(context: &BotContext, action: &BotAction) -> Option<bool> {
    choose_heuristic_claim_action(context)
        .map(|heuristic| bot_actions_are_equivalent(context, action, &heuristic))
}

fn bot_actions_are_equivalent(context: &BotContext, left: &BotAction, right: &BotAction) -> bool {
    if left.seat_index != right.seat_index || left.action_type != right.action_type {
        return false;
    }
    normalized_action_tiles(context, left) == normalized_action_tiles(context, right)
}

fn normalized_action_tiles(context: &BotContext, action: &BotAction) -> Vec<String> {
    let mut tile_keys = action
        .tile_ids
        .iter()
        .map(|tile_id| {
            context
                .player
                .concealed_tiles
                .iter()
                .find(|tile| &tile.tile_id == tile_id)
                .map(|tile| tile.tile_key.clone())
                .unwrap_or_else(|| tile_id.clone())
        })
        .collect::<Vec<_>>();
    tile_keys.sort();
    tile_keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::neural;

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
            discard_history: Vec::new(),
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
    fn missing_neural_model_reports_fallback_telemetry() {
        let mut context = base_context();
        let concealed_tiles = tiles(&[
            "w1", "w2", "w3", "t1", "t2", "t3", "b1", "b2", "b3", "east", "east", "green", "w9",
            "w6",
        ]);
        context.player.concealed_tile_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));
        context.player.concealed_tiles = concealed_tiles;
        let config = ArenaBotPolicyConfig {
            id: "missing".to_string(),
            mode: ArenaPolicyMode::Neural,
            model_path: Some("missing-model.onnx".to_string()),
            sample_actions: false,
            temperature: 1.0,
        };

        let decision = choose_active_turn_decision_with_config_and_rng(&context, &config, None)
            .expect("fallback action");

        assert_eq!(decision.action.action_type, "discard");
        assert!(!decision.telemetry.model_loaded);
        assert!(decision.telemetry.used_fallback);
        assert!(!decision.telemetry.used_neural_action);
        assert_eq!(decision.telemetry.same_as_heuristic, None);
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
    fn neural_policy_uses_top_neural_discard() {
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

        let selected = select_neural_only_discard_plan(&neural_scores).expect("neural selection");

        assert_eq!(selected.tile_key, "t1");
    }

    #[test]
    fn sample_masked_index_never_selects_illegal_action() {
        use rand::SeedableRng;

        let logits = [100.0_f32, 1.0, 2.0];
        let mask = [false, true, true];
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);

        for _ in 0..64 {
            let selected =
                sample_masked_index(&logits, &mask, 1.0, &mut rng).expect("sample should exist");
            assert_ne!(selected, 0);
        }
    }
}
