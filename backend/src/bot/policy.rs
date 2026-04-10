use super::context::*;
use super::search::{
    STAGE_ONE_DEPTH, SearchEngine, claim_action_bonus, claim_meld_tile_keys,
    meld_is_value_honor_set, simulated_tiles_after_removal,
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

    if let Some((action, score)) = best_kong {
        if score > baseline.score + engine.kong_margin() {
            return Some(action);
        }
    }

    if let Some(action) = choose_active_turn_skill_action(context, &mut engine, &baseline) {
        return Some(action);
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

fn choose_active_turn_skill_action(
    context: &BotContext,
    engine: &mut SearchEngine,
    baseline: &super::search::BotDiscardPlan,
) -> Option<BotAction> {
    if !context.is_skill_mode || context.available_skills.is_empty() {
        return None;
    }

    let open_meld_count = context.player.meld_tile_key_groups.len();
    let shanten = engine.min_shanten(&context.player.concealed_tile_counts, open_meld_count);
    let discard_danger = engine.discard_tile_danger(
        context,
        &context.player.concealed_tile_counts,
        &baseline.tile_key,
    );
    let strongest_threat = engine.strongest_threat_opponent(context);

    if skill_ready(context, "jin_chan_tuo_qiao")
        && !context
            .visible_effect_types
            .iter()
            .any(|effect| effect == "jin_chan_tuo_qiao_guard")
    {
        let threat_score = strongest_threat.map(|(_, score)| score).unwrap_or(0);
        if discard_danger >= 140 && (threat_score >= 90 || shanten <= 2) {
            return Some(skill_action(context, "jin_chan_tuo_qiao", Vec::new()));
        }
    }

    if skill_ready(context, "zou_wei_shang_ji")
        && context.wall_tiles_remaining >= 8
        && shanten <= 1
        && !is_leading_conservative(context)
    {
        return Some(skill_action(context, "zou_wei_shang_ji", Vec::new()));
    }

    if skill_ready(context, "an_du_chen_cang") && context.private_knowledge_tile_keys.is_empty() {
        if let Some((target, threat_score)) = strongest_threat {
            if threat_score >= 70 {
                return Some(skill_action(
                    context,
                    "an_du_chen_cang",
                    vec![format!("seat:{target}")],
                ));
            }
        }
    }

    if skill_ready(context, "wu_zhong_sheng_you")
        && context.wall_tiles_remaining >= 18
        && shanten >= 1
        && !is_leading_conservative(context)
        && should_replace_dead_tile(&context.player.concealed_tile_counts, &baseline.tile_key)
    {
        return Some(skill_action(
            context,
            "wu_zhong_sheng_you",
            vec![baseline.tile_id.clone()],
        ));
    }

    if skill_ready(context, "sheng_dong_ji_xi")
        && context.private_knowledge_tile_keys.is_empty()
        && context.wall_tiles_remaining >= 16
        && shanten >= 2
    {
        return Some(skill_action(context, "sheng_dong_ji_xi", Vec::new()));
    }

    if skill_ready(context, "tou_liang_huan_zhu")
        && context.enforce_minimum_eight_fan
        && open_meld_count > 0
        && shanten <= 2
    {
        let signals = engine.strategic_signals_for_state(
            context,
            &context.player.concealed_tile_counts,
            &context.player.meld_tile_key_groups,
            &[],
        );
        if signals.fan_estimate < 8 {
            if let Some((meld_index, _)) = context
                .player
                .meld_tile_key_groups
                .iter()
                .enumerate()
                .find(|(_, meld)| is_sequence_meld(meld))
            {
                return Some(skill_action(
                    context,
                    "tou_liang_huan_zhu",
                    vec![format!("meld:{meld_index}")],
                ));
            }
        }
    }

    None
}

fn skill_ready(context: &BotContext, skill_id: &str) -> bool {
    context
        .available_skills
        .iter()
        .any(|skill| skill.skill_id == skill_id && skill.charges > 0)
}

fn skill_action(context: &BotContext, skill_id: &str, tile_ids: Vec<String>) -> BotAction {
    BotAction {
        seat_index: context.seat_index,
        action_type: format!("skill:{skill_id}"),
        tile_ids,
    }
}

fn should_replace_dead_tile(concealed_counts: &TileCounts, tile_key: &str) -> bool {
    let Some(tile_index) = tile_index(tile_key) else {
        return false;
    };
    if concealed_counts[tile_index] != 1 {
        return false;
    }
    if tile_index >= HONOR_TILE_START {
        return true;
    }

    let rank = tile_index % 9;
    let neighbor_indices = [
        rank.checked_sub(2).map(|_| tile_index - 2),
        rank.checked_sub(1).map(|_| tile_index - 1),
        (rank <= 7).then_some(tile_index + 1),
        (rank <= 6).then_some(tile_index + 2),
    ];
    neighbor_indices
        .into_iter()
        .flatten()
        .all(|index| concealed_counts[index] == 0)
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

fn is_sequence_meld(meld: &[String]) -> bool {
    meld.len() == 3
        && meld
            .first()
            .is_some_and(|first| meld.iter().any(|tile_key| tile_key != first))
}

fn is_leading_conservative(context: &BotContext) -> bool {
    let my_score = context
        .cumulative_scores
        .get(context.seat_index)
        .copied()
        .unwrap_or(0);
    let best_other = context
        .cumulative_scores
        .iter()
        .enumerate()
        .filter(|(seat, _)| *seat != context.seat_index)
        .map(|(_, score)| *score)
        .max()
        .unwrap_or(my_score);
    my_score - best_other >= 24
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
            is_skill_mode: true,
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
            visible_effect_types: Vec::new(),
            private_knowledge_tile_keys: Vec::new(),
            available_skills: Vec::new(),
        }
    }

    #[test]
    fn uses_escape_skill_before_risky_discard() {
        let mut context = base_context();
        context.available_skills.push(BotSkillView {
            skill_id: "jin_chan_tuo_qiao".to_string(),
            charges: 1,
        });
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
        assert_eq!(action.action_type, "skill:jin_chan_tuo_qiao");
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
