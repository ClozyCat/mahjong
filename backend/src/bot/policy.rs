use super::context::*;
use super::search::{
    STAGE_ONE_DEPTH, SearchEngine, claim_action_bonus, claim_meld_tile_keys,
    meld_is_value_honor_set, meld_open_flags_for_state, simulated_tiles_after_removal,
    strategic_signals,
};

pub fn choose_active_turn_action(context: &BotContext) -> Option<BotAction> {
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

        let concealed_after =
            simulated_tiles_after_removal(&context.player.concealed_tiles, &candidate.tile_ids);
        let concealed_counts_after =
            tile_counts34(concealed_after.iter().map(|tile| tile.tile_key.as_str()));
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
    let discard_tile_key = context.last_discard_tile_key.as_deref()?;

    let mut best_claim = None;
    for option in &context.claim_options {
        let concealed_after =
            simulated_tiles_after_removal(&context.player.concealed_tiles, &option.tile_ids);
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
            let concealed_counts_after =
                tile_counts34(concealed_after.iter().map(|tile| tile.tile_key.as_str()));
            engine.expected_score_after_forced_draw(
                context,
                &concealed_counts_after,
                &meld_groups_after,
                &appended_open_flags,
                Some(discard_tile_key),
                STAGE_ONE_DEPTH,
            )? + 140
        } else {
            let concealed_counts_after =
                tile_counts34(concealed_after.iter().map(|tile| tile.tile_key.as_str()));
            let plan = engine.best_discard_plan(
                context,
                &concealed_after,
                &concealed_counts_after,
                &meld_groups_after,
                &appended_open_flags,
                Some(discard_tile_key),
                None,
            )?;
            let meld_open_flags =
                meld_open_flags_for_state(context, &meld_groups_after, &appended_open_flags);
            let signals = strategic_signals(
                context,
                &concealed_counts_after,
                &meld_groups_after,
                &meld_open_flags,
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
                claim_action_bonus(context, &option.action_type, &claim_meld, signals);
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
