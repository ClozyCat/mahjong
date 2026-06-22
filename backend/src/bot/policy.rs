use super::context::*;
use super::features::BotFeaturesV2;
use super::neural::{NeuralDecisionScores, RankedClaimScore, RankedTileScore};
use crate::bot::action_space::CLAIM_ACTION_COUNT;
use crate::bot::arena::ArenaBotPolicyConfig;
use rand::{Rng, rngs::StdRng};
use std::cell::Cell;
use std::env;

thread_local! {
    static TIMING_ENCODE_NS: Cell<u128> = Cell::new(0);
    static TIMING_INFERENCE_NS: Cell<u128> = Cell::new(0);
    static TIMING_SAMPLE_NS: Cell<u128> = Cell::new(0);
    static TIMING_COUNT: Cell<u64> = Cell::new(0);
}

pub(crate) fn reset_timing_detail() {
    TIMING_ENCODE_NS.set(0);
    TIMING_INFERENCE_NS.set(0);
    TIMING_SAMPLE_NS.set(0);
    TIMING_COUNT.set(0);
}

pub(crate) fn print_timing_detail() {
    let count = TIMING_COUNT.get();
    if count == 0 {
        return;
    }
    let enc = TIMING_ENCODE_NS.get() as f64 / count as f64 / 1_000_000.0;
    let inf = TIMING_INFERENCE_NS.get() as f64 / count as f64 / 1_000_000.0;
    let sam = TIMING_SAMPLE_NS.get() as f64 / count as f64 / 1_000_000.0;
    eprintln!(
        "[timing_detail] encode={enc:.3}ms inference={inf:.3}ms sample={sam:.3}ms  (per-action, n={count})"
    );
}

const NEURAL_DISCARD_VALUE_SCALE: f32 = 8.0;
const NEURAL_HU_PASS_MARGIN: f32 = 3.0;

#[derive(Clone, Copy, Debug)]
pub(crate) struct RiskConfig {
    pub base_risk_weight: f32,
    pub value_risk_range: f32,
    pub min_risk_weight: f32,
    pub max_risk_weight: f32,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            base_risk_weight: 0.90,
            value_risk_range: 0.55,
            min_risk_weight: 0.25,
            max_risk_weight: 1.45,
        }
    }
}

impl RiskConfig {
    pub fn from_arena_config(config: &ArenaBotPolicyConfig) -> Self {
        Self {
            base_risk_weight: config.discard_base_risk_weight,
            value_risk_range: config.discard_value_risk_range,
            min_risk_weight: config.discard_min_risk_weight,
            max_risk_weight: config.discard_max_risk_weight,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct BotPolicyDecisionTelemetry {
    pub(crate) model_loaded: bool,
    pub(crate) used_neural_action: bool,
}

#[derive(Clone)]
pub(crate) struct BotPolicyDecision {
    pub(crate) action: BotAction,
    pub(crate) telemetry: BotPolicyDecisionTelemetry,
    pub(crate) features: Option<BotFeaturesV2>,
    pub(crate) neural_scores: Option<NeuralDecisionScores>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NeuralHuChoice {
    Pass,
    Hu,
}

pub fn choose_active_turn_action(context: &BotContext) -> Option<BotAction> {
    choose_active_turn_action_with_config(context, &bot_policy_config_from_env())
}

pub fn choose_active_turn_action_with_config(
    context: &BotContext,
    config: &ArenaBotPolicyConfig,
) -> Option<BotAction> {
    choose_active_turn_decision_with_config_and_rng(context, config, None)
        .map(|decision| decision.action)
}

pub(crate) fn choose_active_turn_decision_with_config_and_rng(
    context: &BotContext,
    config: &ArenaBotPolicyConfig,
    mut rng: Option<&mut StdRng>,
) -> Option<BotPolicyDecision> {
    let mut telemetry = BotPolicyDecisionTelemetry::default();
    let risk_config = RiskConfig::from_arena_config(config);
    let _t0 = std::time::Instant::now();
    let features = crate::bot::features::encode_bot_context_v2(context);
    TIMING_ENCODE_NS.with(|t| t.set(t.get() + _t0.elapsed().as_nanos()));
    let _t1 = std::time::Instant::now();
    if let Some(scores) = neural_decision_scores_for_policy_features(&features, config) {
        TIMING_INFERENCE_NS.with(|t| t.set(t.get() + _t1.elapsed().as_nanos()));
        telemetry.model_loaded = true;
        let _t2 = std::time::Instant::now();
        let action = if config.sample_actions {
            if let Some(rng) = rng.as_deref_mut() {
                let temperature = sample_temperature(config, rng);
                sample_neural_active_turn_action(
                    context,
                    &features,
                    &scores,
                    temperature,
                    rng,
                    Some(&risk_config),
                )
            } else {
                let mut live_rng = rand::rng();
                let temperature = sample_temperature(config, &mut live_rng);
                sample_neural_active_turn_action(
                    context,
                    &features,
                    &scores,
                    temperature,
                    &mut live_rng,
                    Some(&risk_config),
                )
            }
        } else {
            select_neural_only_active_turn_action(context, &features, &scores, Some(&risk_config))
        };
        TIMING_SAMPLE_NS.with(|t| t.set(t.get() + _t2.elapsed().as_nanos()));
        TIMING_COUNT.with(|c| c.set(c.get() + 1));
        if let Some(action) = action {
            telemetry.used_neural_action = true;
            return Some(BotPolicyDecision {
                action,
                telemetry,
                features: Some(features),
                neural_scores: Some(scores),
            });
        }
    }

    let action = random_active_turn_action(context, rng)?;
    Some(BotPolicyDecision {
        action,
        telemetry,
        features: None,
        neural_scores: None,
    })
}

pub fn choose_claim_action(context: &BotContext) -> Option<BotAction> {
    choose_claim_action_with_config(context, &bot_policy_config_from_env())
}

pub fn choose_claim_action_with_config(
    context: &BotContext,
    config: &ArenaBotPolicyConfig,
) -> Option<BotAction> {
    choose_claim_decision_with_config_and_rng(context, config, None).map(|decision| decision.action)
}

pub(crate) fn choose_claim_decision_with_config_and_rng(
    context: &BotContext,
    config: &ArenaBotPolicyConfig,
    mut rng: Option<&mut StdRng>,
) -> Option<BotPolicyDecision> {
    let mut telemetry = BotPolicyDecisionTelemetry::default();
    let features = crate::bot::features::encode_bot_context_v2(context);
    if let Some(scores) = neural_decision_scores_for_policy_features(&features, config) {
        telemetry.model_loaded = true;
        let action = if config.sample_actions {
            if let Some(rng) = rng.as_deref_mut() {
                let temperature = sample_temperature(config, rng);
                sample_neural_claim_action(context, &features, &scores, temperature, rng)
            } else {
                let mut live_rng = rand::rng();
                let temperature = sample_temperature(config, &mut live_rng);
                sample_neural_claim_action(context, &features, &scores, temperature, &mut live_rng)
            }
        } else {
            select_neural_only_claim(context, &features, &scores)
        };
        if let Some(action) = action {
            telemetry.used_neural_action = true;
            return Some(BotPolicyDecision {
                action,
                telemetry,
                features: Some(features),
                neural_scores: Some(scores),
            });
        }
    }

    let action = random_claim_action(context, rng)?;
    Some(BotPolicyDecision {
        action,
        telemetry,
        features: None,
        neural_scores: None,
    })
}

pub(crate) fn choose_neural_hu_decision_with_config_and_rng(
    context: &BotContext,
    config: &ArenaBotPolicyConfig,
    mut rng: Option<&mut StdRng>,
) -> Option<BotPolicyDecision> {
    let features = crate::bot::features::encode_bot_context_v2(context);
    let maybe_scores = neural_decision_scores_for_policy_features(&features, config);
    let choice = if let Some(scores) = maybe_scores.as_ref() {
        if config.sample_actions {
            if let Some(rng) = rng.as_deref_mut() {
                let temperature = sample_temperature(config, rng);
                sample_neural_hu_choice(&features, scores, temperature, rng)
            } else {
                let mut live_rng = rand::rng();
                let temperature = sample_temperature(config, &mut live_rng);
                sample_neural_hu_choice(&features, scores, temperature, &mut live_rng)
            }
            .or_else(|| select_neural_hu_choice(&features, scores, context))?
        } else {
            select_neural_hu_choice(&features, scores, context)?
        }
    } else {
        random_hu_choice(&features)?
    };
    Some(BotPolicyDecision {
        action: bot_action_for_hu_choice(context.seat_index, choice),
        telemetry: BotPolicyDecisionTelemetry {
            model_loaded: maybe_scores.is_some(),
            used_neural_action: maybe_scores.is_some(),
        },
        features: Some(features),
        neural_scores: maybe_scores,
    })
}

pub(crate) fn choose_neural_claim_decision_with_config_and_rng(
    context: &BotContext,
    config: &ArenaBotPolicyConfig,
    mut rng: Option<&mut StdRng>,
) -> Option<BotPolicyDecision> {
    let features = crate::bot::features::encode_bot_context_v2(context);
    let scores = neural_decision_scores_for_policy_features(&features, config)?;
    let action = if config.sample_actions {
        if let Some(rng) = rng.as_deref_mut() {
            let temperature = sample_temperature(config, rng);
            sample_neural_claim_action(context, &features, &scores, temperature, rng)
        } else {
            let mut live_rng = rand::rng();
            let temperature = sample_temperature(config, &mut live_rng);
            sample_neural_claim_action(context, &features, &scores, temperature, &mut live_rng)
        }
        .or_else(|| select_neural_only_claim(context, &features, &scores))?
    } else {
        select_neural_only_claim(context, &features, &scores)?
    };
    Some(BotPolicyDecision {
        telemetry: BotPolicyDecisionTelemetry {
            model_loaded: true,
            used_neural_action: true,
        },
        action,
        features: Some(features),
        neural_scores: Some(scores),
    })
}

pub(crate) fn bot_policy_config_from_env() -> ArenaBotPolicyConfig {
    let model_path = env::var("MAHJONG_BOT_MODEL_PATH")
        .unwrap_or_else(|_| crate::special_bots::SFT_MODEL_PATH.to_string());
    ArenaBotPolicyConfig {
        id: "env-neural".to_string(),
        model_path: Some(model_path),
        sample_actions: false,
        temperature: 1.0,
        temperature_range: None,
        discard_base_risk_weight: 0.90,
        discard_value_risk_range: 0.55,
        discard_min_risk_weight: 0.25,
        discard_max_risk_weight: 1.45,
    }
}

fn neural_decision_scores_for_policy_features(
    features: &BotFeaturesV2,
    config: &ArenaBotPolicyConfig,
) -> Option<NeuralDecisionScores> {
    let path = config.model_path.as_deref().map(std::path::Path::new);
    super::neural::neural_decision_scores_for_features(path, features.clone())
}

fn sample_temperature(config: &ArenaBotPolicyConfig, rng: &mut impl Rng) -> f32 {
    match config.temperature_range {
        Some([min, max]) if min.is_finite() && max.is_finite() && max > min => {
            rng.random_range(min..max)
        }
        _ => config.temperature,
    }
}

fn random_active_turn_action(context: &BotContext, rng: Option<&mut StdRng>) -> Option<BotAction> {
    let mut actions = context
        .self_kong_candidates
        .iter()
        .filter(|candidate| {
            !(candidate.kind == BotSelfKongKind::Add
                && context.add_kong_risk_tiles.contains(&candidate.tile_key))
        })
        .map(|candidate| BotAction {
            seat_index: context.seat_index,
            action_type: "kong".to_string(),
            tile_ids: candidate.tile_ids.clone(),
        })
        .collect::<Vec<_>>();
    actions.extend(
        context
            .player
            .concealed_tiles
            .iter()
            .filter(|tile| {
                !tile.is_flower
                    && Some(tile.tile_key.as_str())
                        != context.restricted_discard_tile_key.as_deref()
            })
            .map(|tile| BotAction {
                seat_index: context.seat_index,
                action_type: "discard".to_string(),
                tile_ids: vec![tile.tile_id.clone()],
            }),
    );
    choose_random_action(actions, rng)
}

fn random_claim_action(context: &BotContext, rng: Option<&mut StdRng>) -> Option<BotAction> {
    if context
        .claim_options
        .iter()
        .any(|option| option.action_type == "hu")
    {
        return Some(BotAction {
            seat_index: context.seat_index,
            action_type: "hu".to_string(),
            tile_ids: Vec::new(),
        });
    }
    let mut actions = context
        .claim_options
        .iter()
        .map(|option| BotAction {
            seat_index: context.seat_index,
            action_type: option.action_type.clone(),
            tile_ids: option.tile_ids.clone(),
        })
        .collect::<Vec<_>>();
    actions.push(pass_action(context.seat_index));
    choose_random_action(actions, rng)
}

fn random_hu_choice(features: &BotFeaturesV2) -> Option<NeuralHuChoice> {
    if features.hu_mask[1] {
        Some(NeuralHuChoice::Hu)
    } else if features.hu_mask[0] {
        Some(NeuralHuChoice::Pass)
    } else {
        None
    }
}

fn choose_random_action(actions: Vec<BotAction>, rng: Option<&mut StdRng>) -> Option<BotAction> {
    if actions.is_empty() {
        return None;
    }
    let index = if let Some(rng) = rng {
        rng.random_range(0..actions.len())
    } else {
        rand::rng().random_range(0..actions.len())
    };
    actions.into_iter().nth(index)
}

fn sample_masked_index<const N: usize, R: Rng + ?Sized>(
    logits: &[f32; N],
    mask: &[bool; N],
    temperature: f32,
    rng: &mut R,
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
    features: &BotFeaturesV2,
    scores: &NeuralDecisionScores,
    temperature: f32,
    rng: &mut impl Rng,
    risk_config: Option<&RiskConfig>,
) -> Option<BotAction> {
    if context.self_kong_candidates.is_empty() {
        return sample_neural_discard_action(
            context,
            features,
            scores,
            temperature,
            rng,
            risk_config,
        );
    }

    let selected = sample_masked_index(
        &scores.self_kong_logits,
        &features.self_kong_mask,
        temperature,
        rng,
    )?;
    match selected {
        0 => sample_neural_discard_action(context, features, scores, temperature, rng, risk_config),
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
                .or_else(|| {
                    sample_neural_discard_action(
                        context,
                        features,
                        scores,
                        temperature,
                        rng,
                        risk_config,
                    )
                })
        }
        _ => sample_neural_discard_action(context, features, scores, temperature, rng, risk_config),
    }
}

fn sample_neural_discard_action(
    context: &BotContext,
    features: &BotFeaturesV2,
    scores: &NeuralDecisionScores,
    temperature: f32,
    rng: &mut impl Rng,
    risk_config: Option<&RiskConfig>,
) -> Option<BotAction> {
    let discard_logits = risk_adjusted_discard_logits(scores, risk_config);
    let tile_index =
        sample_masked_index(&discard_logits, &features.discard_mask, temperature, rng)?;
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
    features: &BotFeaturesV2,
    scores: &NeuralDecisionScores,
    temperature: f32,
    rng: &mut impl Rng,
) -> Option<BotAction> {
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

fn sample_neural_hu_choice(
    features: &BotFeaturesV2,
    scores: &NeuralDecisionScores,
    temperature: f32,
    rng: &mut impl Rng,
) -> Option<NeuralHuChoice> {
    if should_take_legal_hu(features, scores)? {
        return Some(NeuralHuChoice::Hu);
    }
    let selected = sample_masked_index(&scores.hu_logits, &features.hu_mask, temperature, rng)?;
    neural_hu_choice_for_index(selected)
}

fn select_neural_hu_choice(
    features: &BotFeaturesV2,
    scores: &NeuralDecisionScores,
    context: &BotContext,
) -> Option<NeuralHuChoice> {
    if !features.hu_mask[1] {
        return None;
    }
    let min_fan = context.minimum_hu_fan as f32;
    if min_fan > 0.0 && scores.qualifying_fan_value < min_fan {
        return Some(NeuralHuChoice::Pass);
    }
    let pass_logit = scores.hu_logits[0];
    let hu_logit = scores.hu_logits[1];
    if !pass_logit.is_finite() || !hu_logit.is_finite() {
        return None;
    }
    Some(if pass_logit - hu_logit >= NEURAL_HU_PASS_MARGIN {
        NeuralHuChoice::Pass
    } else {
        NeuralHuChoice::Hu
    })
}

fn should_take_legal_hu(features: &BotFeaturesV2, scores: &NeuralDecisionScores) -> Option<bool> {
    if !features.hu_mask[1] {
        return Some(false);
    }
    let pass_logit = scores.hu_logits[0];
    let hu_logit = scores.hu_logits[1];
    if !pass_logit.is_finite() || !hu_logit.is_finite() {
        return None;
    }
    Some(pass_logit - hu_logit < NEURAL_HU_PASS_MARGIN)
}

fn neural_hu_choice_for_index(index: usize) -> Option<NeuralHuChoice> {
    match index {
        0 => Some(NeuralHuChoice::Pass),
        1 => Some(NeuralHuChoice::Hu),
        _ => None,
    }
}

fn bot_action_for_hu_choice(seat_index: usize, choice: NeuralHuChoice) -> BotAction {
    BotAction {
        seat_index,
        action_type: match choice {
            NeuralHuChoice::Pass => "pass",
            NeuralHuChoice::Hu => "hu",
        }
        .to_string(),
        tile_ids: Vec::new(),
    }
}

fn select_neural_only_active_turn_action(
    context: &BotContext,
    features: &BotFeaturesV2,
    scores: &NeuralDecisionScores,
    risk_config: Option<&RiskConfig>,
) -> Option<BotAction> {
    if let Some(action) = select_neural_only_self_kong(context, scores) {
        return Some(action);
    }
    let discard_logits = risk_adjusted_discard_logits(scores, risk_config);
    select_neural_only_discard_action(
        context,
        &rank_masked_discards_with_features(context, features, &discard_logits),
    )
}

fn select_neural_only_discard_action(
    context: &BotContext,
    neural_scores: &[RankedTileScore],
) -> Option<BotAction> {
    neural_scores.first().map(|score| BotAction {
        seat_index: context.seat_index,
        action_type: "discard".to_string(),
        tile_ids: vec![score.tile_id.clone()],
    })
}

fn rank_masked_discards_with_features(
    context: &BotContext,
    features: &BotFeaturesV2,
    logits: &[f32; TILE_KIND_COUNT],
) -> Vec<RankedTileScore> {
    let mut scores = Vec::new();
    let mut visited_tile_keys = std::collections::HashSet::new();
    for tile in &context.player.concealed_tiles {
        if tile.is_flower || !visited_tile_keys.insert(tile.tile_key.clone()) {
            continue;
        }
        let Some(index) = tile_index(&tile.tile_key) else {
            continue;
        };
        if !features.discard_mask[index] {
            continue;
        }
        let Some(tile_id) = context
            .player
            .concealed_tiles
            .iter()
            .find(|candidate| !candidate.is_flower && candidate.tile_key == tile.tile_key)
            .map(|candidate| candidate.tile_id.clone())
        else {
            continue;
        };
        scores.push(RankedTileScore {
            tile_id,
            tile_key: tile.tile_key.clone(),
            logit: logits[index],
        });
    }
    scores.sort_by(|left, right| {
        right
            .logit
            .total_cmp(&left.logit)
            .then_with(|| right.tile_key.cmp(&left.tile_key))
    });
    scores
}

pub(crate) fn risk_adjusted_discard_logits(
    scores: &NeuralDecisionScores,
    risk_config: Option<&RiskConfig>,
) -> [f32; TILE_KIND_COUNT] {
    let risk_config = risk_config.copied().unwrap_or_default();
    let risk_weight = neural_discard_risk_weight(scores.value_for_risk, &risk_config);
    let mut adjusted = scores.discard_logits;
    for (index, logit) in adjusted.iter_mut().enumerate() {
        let policy_logit = scores.discard_logits[index];
        if !policy_logit.is_finite() {
            continue;
        }
        let Some(risk_probability) = sigmoid_probability(scores.risk_logits[index]) else {
            continue;
        };
        let adjusted_logit = policy_logit - risk_weight * risk_probability;
        if adjusted_logit.is_finite() {
            *logit = adjusted_logit;
        }
    }
    adjusted
}

fn neural_discard_risk_weight(value: f32, risk_config: &RiskConfig) -> f32 {
    let normalized_value = if value.is_finite() {
        (value / NEURAL_DISCARD_VALUE_SCALE).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    (risk_config.base_risk_weight - risk_config.value_risk_range * normalized_value)
        .clamp(risk_config.min_risk_weight, risk_config.max_risk_weight)
}

fn sigmoid_probability(logit: f32) -> Option<f32> {
    if !logit.is_finite() {
        return None;
    }
    if logit >= 0.0 {
        let z = (-logit).exp();
        Some(1.0 / (1.0 + z))
    } else {
        let z = logit.exp();
        Some(z / (1.0 + z))
    }
}

fn select_neural_only_claim(
    context: &BotContext,
    features: &BotFeaturesV2,
    scores: &NeuralDecisionScores,
) -> Option<BotAction> {
    let ranked = rank_masked_claims_with_features(context, features, &scores.claim_logits);
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

fn rank_masked_claims_with_features(
    _context: &BotContext,
    features: &BotFeaturesV2,
    logits: &[f32; CLAIM_ACTION_COUNT],
) -> Vec<RankedClaimScore> {
    let mut scores = crate::bot::action_space::CLAIM_ACTIONS
        .iter()
        .enumerate()
        .filter_map(|(index, action_name)| {
            features.claim_mask[index].then_some(RankedClaimScore {
                action_name,
                logit: logits[index],
            })
        })
        .collect::<Vec<_>>();
    scores.sort_by(|left, right| {
        right
            .logit
            .total_cmp(&left.logit)
            .then_with(|| right.action_name.cmp(left.action_name))
    });
    scores
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

fn pass_action(seat_index: usize) -> BotAction {
    BotAction {
        seat_index,
        action_type: "pass".to_string(),
        tile_ids: vec![],
    }
}

#[cfg(test)]
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
    use crate::bot::action_space::{CLAIM_ACTION_COUNT, SELF_KONG_ACTION_COUNT};
    use crate::bot::neural;
    use rand::SeedableRng;

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
            minimum_hu_fan: crate::core::state::room::default_minimum_hu_fan(),
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

    fn neural_scores_for_discards(
        discard_logits: [f32; TILE_KIND_COUNT],
        risk_logits: [f32; TILE_KIND_COUNT],
        value_for_risk: f32,
    ) -> neural::NeuralDecisionScores {
        neural::NeuralDecisionScores {
            discard_logits,
            claim_logits: [0.0; CLAIM_ACTION_COUNT],
            self_kong_logits: [0.0; SELF_KONG_ACTION_COUNT],
            hu_logits: [0.0; 2],
            value_for_risk,
            qualifying_fan_value: 0.0,
            risk_logits,
        }
    }

    #[test]
    fn missing_neural_model_falls_back_to_random_legal_discard() {
        let mut context = base_context();
        let concealed_tiles = tiles(&["w1", "w2", "w3"]);
        context.player.concealed_tile_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));
        context.player.concealed_tiles = concealed_tiles.clone();
        let config = ArenaBotPolicyConfig {
            id: "missing".to_string(),
            model_path: Some("missing-model.onnx".to_string()),
            sample_actions: false,
            temperature: 1.0,
            temperature_range: None,
            discard_base_risk_weight: 0.90,
            discard_value_risk_range: 0.55,
            discard_min_risk_weight: 0.25,
            discard_max_risk_weight: 1.45,
        };
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);

        let decision =
            choose_active_turn_decision_with_config_and_rng(&context, &config, Some(&mut rng))
                .expect("random action");

        assert_eq!(decision.action.action_type, "discard");
        assert_eq!(decision.action.tile_ids.len(), 1);
        assert!(
            concealed_tiles
                .iter()
                .any(|tile| Some(&tile.tile_id) == decision.action.tile_ids.first())
        );
        assert!(!decision.telemetry.model_loaded);
        assert!(!decision.telemetry.used_neural_action);
    }

    #[test]
    fn sample_temperature_uses_configured_range() {
        let config = ArenaBotPolicyConfig {
            id: "explorer".to_string(),
            model_path: None,
            sample_actions: true,
            temperature: 1.0,
            temperature_range: Some([1.5, 2.5]),
            discard_base_risk_weight: 0.90,
            discard_value_risk_range: 0.55,
            discard_min_risk_weight: 0.25,
            discard_max_risk_weight: 1.45,
        };
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);

        for _ in 0..16 {
            let temperature = sample_temperature(&config, &mut rng);
            assert!((1.5..2.5).contains(&temperature));
        }
    }

    #[test]
    fn missing_neural_model_falls_back_to_hu_when_hu_is_available() {
        let mut context = base_context();
        context.claim_options = vec![BotClaimOption {
            action_type: "hu".to_string(),
            tile_ids: Vec::new(),
        }];
        let config = ArenaBotPolicyConfig {
            id: "missing".to_string(),
            model_path: Some("missing-model.onnx".to_string()),
            sample_actions: false,
            temperature: 1.0,
            temperature_range: None,
            discard_base_risk_weight: 0.90,
            discard_value_risk_range: 0.55,
            discard_min_risk_weight: 0.25,
            discard_max_risk_weight: 1.45,
        };

        let decision = choose_neural_hu_decision_with_config_and_rng(&context, &config, None)
            .expect("hu action");

        assert_eq!(decision.action.action_type, "hu");
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

        let selected = select_neural_only_discard_action(&base_context(), &neural_scores)
            .expect("neural selection");

        assert_eq!(selected.tile_ids, vec!["t1#0"]);
    }

    #[test]
    fn neural_policy_avoids_slightly_better_logit_when_discard_risk_is_high() {
        let mut context = base_context();
        let concealed_tiles = tiles(&["w1", "t1"]);
        context.player.concealed_tile_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));
        context.player.concealed_tiles = concealed_tiles;
        let mut discard_logits = [0.0_f32; TILE_KIND_COUNT];
        discard_logits[tile_index("w1").expect("w1")] = 1.10;
        discard_logits[tile_index("t1").expect("t1")] = 1.00;
        let mut risk_logits = [-5.0_f32; TILE_KIND_COUNT];
        risk_logits[tile_index("w1").expect("w1")] = 5.0;
        risk_logits[tile_index("t1").expect("t1")] = -5.0;
        let scores = neural_scores_for_discards(discard_logits, risk_logits, -8.0);

        let features = crate::bot::features::encode_bot_context_v2(&context);
        let action = select_neural_only_active_turn_action(&context, &features, &scores, None)
            .expect("neural action");

        assert_eq!(action.action_type, "discard");
        assert_eq!(normalized_action_tiles(&context, &action), vec!["t1"]);
    }

    #[test]
    fn neural_discard_risk_penalty_is_weaker_when_value_is_high() {
        let mut context = base_context();
        let concealed_tiles = tiles(&["w1", "t1"]);
        context.player.concealed_tile_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));
        context.player.concealed_tiles = concealed_tiles;
        let mut discard_logits = [0.0_f32; TILE_KIND_COUNT];
        discard_logits[tile_index("w1").expect("w1")] = 1.60;
        discard_logits[tile_index("t1").expect("t1")] = 1.00;
        let mut risk_logits = [-5.0_f32; TILE_KIND_COUNT];
        risk_logits[tile_index("w1").expect("w1")] = 5.0;
        risk_logits[tile_index("t1").expect("t1")] = -5.0;
        let low_value_scores = neural_scores_for_discards(discard_logits, risk_logits, -8.0);
        let high_value_scores = neural_scores_for_discards(discard_logits, risk_logits, 8.0);

        let features = crate::bot::features::encode_bot_context_v2(&context);
        let low_value_action =
            select_neural_only_active_turn_action(&context, &features, &low_value_scores, None)
                .expect("low value neural action");
        let high_value_action =
            select_neural_only_active_turn_action(&context, &features, &high_value_scores, None)
                .expect("high value neural action");

        assert_eq!(
            normalized_action_tiles(&context, &low_value_action),
            vec!["t1"]
        );
        assert_eq!(
            normalized_action_tiles(&context, &high_value_action),
            vec!["w1"]
        );
    }

    #[test]
    fn neural_hu_requires_strong_pass_margin() {
        let mut context = base_context();
        context.claim_options = vec![BotClaimOption {
            action_type: "hu".to_string(),
            tile_ids: Vec::new(),
        }];
        let mut scores =
            neural_scores_for_discards([0.0; TILE_KIND_COUNT], [0.0; TILE_KIND_COUNT], 0.0);
        scores.hu_logits = [2.0, 1.0];
        scores.qualifying_fan_value = 8.0;

        let features = crate::bot::features::encode_bot_context_v2(&context);
        let decision = select_neural_hu_choice(&features, &scores, &context).expect("hu decision");

        assert_eq!(decision, NeuralHuChoice::Hu);
    }

    #[test]
    fn neural_hu_head_can_decline_available_hu_with_strong_pass_margin() {
        let mut context = base_context();
        context.claim_options = vec![BotClaimOption {
            action_type: "hu".to_string(),
            tile_ids: Vec::new(),
        }];
        let mut scores =
            neural_scores_for_discards([0.0; TILE_KIND_COUNT], [0.0; TILE_KIND_COUNT], 0.0);
        scores.hu_logits = [5.0, 1.0];
        scores.qualifying_fan_value = 8.0;

        let features = crate::bot::features::encode_bot_context_v2(&context);
        let decision = select_neural_hu_choice(&features, &scores, &context).expect("hu decision");

        assert_eq!(decision, NeuralHuChoice::Pass);
    }

    #[test]
    fn neural_hu_head_can_accept_available_hu() {
        let mut context = base_context();
        context.claim_options = vec![BotClaimOption {
            action_type: "hu".to_string(),
            tile_ids: Vec::new(),
        }];
        let mut scores =
            neural_scores_for_discards([0.0; TILE_KIND_COUNT], [0.0; TILE_KIND_COUNT], 0.0);
        scores.hu_logits = [1.0, 2.0];
        scores.qualifying_fan_value = 8.0;

        let features = crate::bot::features::encode_bot_context_v2(&context);
        let decision = select_neural_hu_choice(&features, &scores, &context).expect("hu decision");

        assert_eq!(decision, NeuralHuChoice::Hu);
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

    #[test]
    fn sample_masked_index_accepts_thread_rng_for_live_bot_sampling() {
        let logits = [1.0_f32, 2.0, 3.0];
        let mask = [true, true, true];
        let mut rng = rand::rng();

        let selected = sample_masked_index(&logits, &mask, 1.0, &mut rng);

        assert!(selected.is_some());
    }
}
