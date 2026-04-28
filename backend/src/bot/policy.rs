use super::context::*;
use super::neural::{
    NeuralDecisionScores, RankedTileScore, neural_decision_scores, rank_masked_claims,
    rank_masked_discards,
};
use super::search::{
    BotDiscardPlan, STAGE_ONE_DEPTH, SearchEngine, claim_action_bonus, claim_meld_tile_keys,
    simulated_tiles_after_removal,
};
use std::{env, time::Instant};

const HYBRID_TOP_SEARCH_CANDIDATES: usize = 3;
const HYBRID_CLOSE_SEARCH_GAP: i64 = 30;
const DEFAULT_NEURAL_PRIOR_WEIGHT: i64 = 15;

pub fn choose_active_turn_action(context: &BotContext) -> Option<BotAction> {
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

    let neural_scores = neural_decision_scores(context);
    let selected_discard = neural_scores
        .as_ref()
        .and_then(|scores| select_neural_v2_discard(context, &search_plans, scores))
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

    let mut best_claim = None;
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

        let replace = best_claim
            .as_ref()
            .map(|(_, score): &(BotAction, i64)| total_score > *score)
            .unwrap_or(true);
        if replace {
            best_claim = Some((
                BotAction {
                    seat_index: context.seat_index,
                    action_type: option.action_type.clone(),
                    tile_ids: option.tile_ids.clone(),
                },
                total_score,
            ));
        }
    }

    if let Some(action) = select_neural_v2_claim(
        context,
        pass_score,
        best_claim.as_ref(),
        engine.claim_margin(),
    ) {
        return Some(action);
    }

    if let Some((action, score)) = best_claim {
        if score > pass_score + engine.claim_margin() {
            return Some(action);
        }
    }

    Some(BotAction {
        seat_index: context.seat_index,
        action_type: "pass".to_string(),
        tile_ids: vec![],
    })
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

fn select_neural_v2_discard(
    context: &BotContext,
    search_plans: &[BotDiscardPlan],
    scores: &NeuralDecisionScores,
) -> Option<BotDiscardPlan> {
    let neural_scores = rank_masked_discards(context, &scores.discard_logits);
    select_hybrid_discard_plan(search_plans, &neural_scores, neural_prior_weight())
}

fn neural_prior_weight() -> i64 {
    env::var("MAHJONG_BOT_NEURAL_WEIGHT")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(DEFAULT_NEURAL_PRIOR_WEIGHT)
        .max(0)
}

fn select_hybrid_discard_plan(
    search_plans: &[BotDiscardPlan],
    neural_scores: &[RankedTileScore],
    weight: i64,
) -> Option<BotDiscardPlan> {
    let best_search = search_plans.first()?.clone();
    let candidate_count = search_plans
        .iter()
        .take(HYBRID_TOP_SEARCH_CANDIDATES)
        .take_while(|plan| best_search.score - plan.score <= HYBRID_CLOSE_SEARCH_GAP)
        .count();
    if candidate_count <= 1 {
        return Some(best_search);
    }
    let candidate_plans = &search_plans[..candidate_count];
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

    for plan in candidate_plans {
        let neural_bonus = candidate_neural_scores
            .iter()
            .find(|score| score.tile_key == plan.tile_key)
            .map(|score| {
                if weight == 0 || logit_range.abs() < f32::EPSILON {
                    0
                } else {
                    let normalized = ((score.logit - min_logit) / logit_range) * 2.0 - 1.0;
                    (normalized * weight as f32).round() as i64
                }
            })
            .unwrap_or(0);
        let final_score = plan.score + neural_bonus;
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

fn select_neural_v2_claim(
    context: &BotContext,
    pass_score: i64,
    best_search_claim: Option<&(BotAction, i64)>,
    claim_margin: i64,
) -> Option<BotAction> {
    let scores = neural_decision_scores(context)?;
    let ranked = rank_masked_claims(context, &scores.claim_logits);
    let best = ranked.first()?;
    if best.action_name == "pass" {
        return Some(pass_action(context.seat_index));
    }
    let option = context
        .claim_options
        .iter()
        .find(|option| claim_option_matches_ranked_action(&option.action_type, best.action_name))?;
    if best.action_name == "hu" {
        return Some(BotAction {
            seat_index: context.seat_index,
            action_type: option.action_type.clone(),
            tile_ids: option.tile_ids.clone(),
        });
    }
    if let Some((search_action, search_score)) = best_search_claim {
        if claim_action_matches_ranked_action(search_action, option, best.action_name)
            && *search_score >= pass_score - claim_margin
        {
            return Some(BotAction {
                seat_index: context.seat_index,
                action_type: option.action_type.clone(),
                tile_ids: option.tile_ids.clone(),
            });
        }
    }
    None
}

fn select_neural_v2_self_kong(
    context: &BotContext,
    scores: &NeuralDecisionScores,
    best_search_kong: Option<&(BotAction, i64)>,
    baseline_score: i64,
    kong_margin: i64,
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
    if logit <= pass_logit {
        return None;
    }
    if let Some((search_action, search_score)) = best_search_kong {
        if search_action.tile_ids == action.tile_ids
            && *search_score >= baseline_score - kong_margin
        {
            return Some(action);
        }
    }
    None
}

fn claim_option_matches_ranked_action(option_action_type: &str, ranked_action_name: &str) -> bool {
    match ranked_action_name {
        "chow_left" | "chow_mid" | "chow_right" => option_action_type == "chow",
        other => option_action_type == other,
    }
}

fn claim_action_matches_ranked_action(
    action: &BotAction,
    option: &crate::projection::bot_view::BotClaimOption,
    ranked_action_name: &str,
) -> bool {
    claim_option_matches_ranked_action(&action.action_type, ranked_action_name)
        && action.tile_ids == option.tile_ids
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

        let selected = select_hybrid_discard_plan(&search_plans, &neural_scores, 80)
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
                score: 880,
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

        let selected = select_hybrid_discard_plan(&search_plans, &neural_scores, 80)
            .expect("hybrid selection");

        assert_eq!(selected.tile_key, "w1");
    }

    #[test]
    fn neural_prior_only_breaks_top_three_search_candidates() {
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

        let selected = select_hybrid_discard_plan(&search_plans, &neural_scores, 80)
            .expect("hybrid selection");

        assert_ne!(selected.tile_key, "red");
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

        let selected =
            select_hybrid_discard_plan(&search_plans, &neural_scores, 0).expect("hybrid selection");

        assert_eq!(selected.tile_key, "w1");
    }
}
