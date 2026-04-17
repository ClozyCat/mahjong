use super::context::*;
use super::search::{
    STAGE_ONE_DEPTH, SearchEngine, claim_action_bonus, claim_meld_tile_keys,
    meld_is_value_honor_set, simulated_tiles_after_removal,
};
use std::{env, time::Instant};

pub fn choose_active_turn_action(context: &BotContext) -> Option<BotAction> {
    let decision_started = Instant::now();
    let mut engine = SearchEngine::new(context);
    let baseline = engine.best_discard_plan(
        context,
        &context.player.concealed_tiles,
        &context.player.concealed_tile_counts,
        &context.player.meld_tile_key_groups,
        &[],
        context.restricted_discard_tile_key.as_deref(),
        context.drawn_tile_id.as_deref(),
    )?;

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
            if context.enforce_minimum_eight_fan {
                let should_skip = match option.action_type.as_str() {
                    "chow" => signals.fan_estimate < 6,
                    "pung" => {
                        signals.fan_estimate < 4 && !meld_is_value_honor_set(context, &claim_meld)
                    }
                    _ => false,
                };
                if should_skip {
                    continue;
                }
            }
            let action_bonus =
                claim_action_bonus(context, &option.action_type, &claim_meld, pass_signals, signals);
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

#[cfg(test)]
mod tests {
    use super::*;

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
            enforce_minimum_eight_fan: true,
            self_kong_candidates: Vec::new(),
            claim_options: Vec::new(),
            last_discard_tile_key: None,
            add_kong_risk_tiles: std::collections::HashSet::new(),
            private_knowledge_tile_keys: Vec::new(),
        }
    }

    #[test]
    fn bot_never_uses_active_skill_even_if_available() {
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
        assert_ne!(action.action_type, "skill:jin_chan_tuo_qiao");
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
}
