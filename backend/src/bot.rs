use crate::scoring::{
    EvaluationInput as ScoringEvaluationInput, KongEntry as ScoringKongEntry,
    TimingFeatures as ScoringTimingFeatures,
    decompose_winning_hand_with_melds as scoring_decompose_winning_hand_with_melds,
    evaluate_fans as scoring_evaluate_fans, extract_hand_features as scoring_extract_hand_features,
};
use std::collections::{HashMap, HashSet};

const TILE_KIND_COUNT: usize = 34;
const HONOR_TILE_START: usize = 27;
const STANDARD_TILE_KEYS: [&str; 34] = [
    "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "t1", "t2", "t3", "t4", "t5", "t6",
    "t7", "t8", "t9", "b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9", "east", "south",
    "west", "north", "red", "green", "white",
];
const WIND_ORDER: [&str; 4] = ["east", "south", "west", "north"];
const STAGE_ONE_DEPTH: u8 = 0;
const STAGE_TWO_DEPTH: u8 = 1;
const STAGE_TWO_CANDIDATES: usize = 3;
const KONG_SCORE_MARGIN: i64 = 80;
const CLAIM_SCORE_MARGIN: i64 = 100;
const BASE_DRAW_SCAN_LIMIT: usize = 18;
const EXPECTIMAX_DRAW_LIMIT: usize = 12;

pub type TileCounts = [u8; TILE_KIND_COUNT];

#[derive(Clone)]
pub struct BotAction {
    pub seat_index: usize,
    pub action_type: String,
    pub tile_ids: Vec<String>,
}

#[derive(Clone)]
pub struct BotTileView {
    pub tile_id: String,
    pub tile_key: String,
    pub is_flower: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BotSelfKongKind {
    Concealed,
    Add,
}

#[derive(Clone)]
pub struct BotSelfKongCandidate {
    pub kind: BotSelfKongKind,
    pub tile_ids: Vec<String>,
    pub tile_key: String,
    pub meld_index: Option<usize>,
}

#[derive(Clone)]
pub struct BotClaimOption {
    pub action_type: String,
    pub tile_ids: Vec<String>,
}

#[derive(Clone)]
pub struct BotPlayerContext {
    pub concealed_tiles: Vec<BotTileView>,
    pub concealed_tile_counts: TileCounts,
    pub meld_tile_key_groups: Vec<Vec<String>>,
    pub flower_count: usize,
}

#[derive(Clone)]
pub struct BotContext {
    pub seat_index: usize,
    pub seat_count: usize,
    pub dealer_seat: usize,
    pub round_wind: Option<String>,
    pub visible_tile_keys: Vec<String>,
    pub opponent_discards_by_seat: Vec<Vec<String>>,
    pub kong_entries: Vec<ScoringKongEntry>,
    pub player: BotPlayerContext,
    pub restricted_discard_tile_key: Option<String>,
    pub drawn_tile_id: Option<String>,
    pub enforce_minimum_eight_fan: bool,
    pub self_kong_candidates: Vec<BotSelfKongCandidate>,
    pub claim_options: Vec<BotClaimOption>,
    pub last_discard_tile_key: Option<String>,
    pub add_kong_risk_tiles: HashSet<String>,
}

#[derive(Clone)]
struct BotDiscardPlan {
    tile_id: String,
    tile_key: String,
    score: i64,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct CompactBotMeld {
    len: u8,
    tiles: [u8; 4],
    is_open: bool,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct HandStateKey {
    counts: TileCounts,
    melds: Vec<CompactBotMeld>,
    depth: u8,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct ExpectedDrawKey {
    counts: TileCounts,
    melds: Vec<CompactBotMeld>,
    restricted_discard_tile_index: Option<u8>,
    depth: u8,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct BestDiscardKey {
    counts: TileCounts,
    melds: Vec<CompactBotMeld>,
    restricted_discard_tile_index: Option<u8>,
    depth: u8,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct WinningKey {
    counts: TileCounts,
    melds: Vec<CompactBotMeld>,
    draw_tile_index: u8,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct ShantenKey {
    counts: TileCounts,
    open_meld_count: u8,
}

#[derive(Default)]
struct SearchEngine {
    hand_score_cache: HashMap<HandStateKey, i64>,
    expected_draw_cache: HashMap<ExpectedDrawKey, Option<i64>>,
    best_discard_cache: HashMap<BestDiscardKey, Option<i64>>,
    winning_fan_cache: HashMap<WinningKey, Option<i64>>,
    shanten_cache: HashMap<ShantenKey, i32>,
}

pub fn choose_active_turn_action(context: &BotContext) -> Option<BotAction> {
    let mut engine = SearchEngine::default();
    let baseline = engine.best_discard_plan(
        context,
        &context.player.concealed_tiles,
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
        if score > baseline.score + KONG_SCORE_MARGIN {
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
    let mut engine = SearchEngine::default();
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
        meld_groups_after.push(claim_meld);

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
            let plan = engine.best_discard_plan(
                context,
                &concealed_after,
                &meld_groups_after,
                &appended_open_flags,
                Some(discard_tile_key),
                None,
            )?;
            let action_bonus = if option.action_type == "pung" { 40 } else { -20 };
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
        if score > pass_score + CLAIM_SCORE_MARGIN {
            return Some(action);
        }
    }

    Some(BotAction {
        seat_index: context.seat_index,
        action_type: "pass".to_string(),
        tile_ids: vec![],
    })
}

impl SearchEngine {
    fn best_discard_plan(
        &mut self,
        context: &BotContext,
        concealed_tiles: &[BotTileView],
        meld_tile_key_groups: &[Vec<String>],
        appended_open_flags: &[bool],
        restricted_discard_tile_key: Option<&str>,
        drawn_tile_id: Option<&str>,
    ) -> Option<BotDiscardPlan> {
        let stage_one = self.rank_discard_plans_at_depth(
            context,
            concealed_tiles,
            meld_tile_key_groups,
            appended_open_flags,
            restricted_discard_tile_key,
            drawn_tile_id,
            STAGE_ONE_DEPTH,
        );
        if stage_one.is_empty() {
            return None;
        }
        let mut finalists = stage_one
            .into_iter()
            .take(STAGE_TWO_CANDIDATES)
            .collect::<Vec<_>>();
        for finalist in &mut finalists {
            if let Some(tile) = concealed_tiles
                .iter()
                .find(|tile| tile.tile_id == finalist.tile_id)
                .cloned()
            {
                finalist.score = self.score_discard_candidate(
                    context,
                    concealed_tiles,
                    meld_tile_key_groups,
                    appended_open_flags,
                    restricted_discard_tile_key,
                    drawn_tile_id,
                    &tile,
                    STAGE_TWO_DEPTH,
                )?;
            }
        }
        finalists.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| right.tile_key.cmp(&left.tile_key))
        });
        finalists.into_iter().next()
    }

    fn rank_discard_plans_at_depth(
        &mut self,
        context: &BotContext,
        concealed_tiles: &[BotTileView],
        meld_tile_key_groups: &[Vec<String>],
        appended_open_flags: &[bool],
        restricted_discard_tile_key: Option<&str>,
        drawn_tile_id: Option<&str>,
        depth: u8,
    ) -> Vec<BotDiscardPlan> {
        let mut plans = Vec::new();
        let mut visited_tile_keys = HashSet::new();

        for tile in concealed_tiles {
            if tile.is_flower || Some(tile.tile_key.as_str()) == restricted_discard_tile_key {
                continue;
            }
            if !visited_tile_keys.insert(tile.tile_key.clone()) {
                continue;
            }
            let Some(score) = self.score_discard_candidate(
                context,
                concealed_tiles,
                meld_tile_key_groups,
                appended_open_flags,
                restricted_discard_tile_key,
                drawn_tile_id,
                tile,
                depth,
            ) else {
                continue;
            };
            let Some(tile_id) =
                preferred_discard_tile_id_for_key(concealed_tiles, &tile.tile_key, drawn_tile_id)
            else {
                continue;
            };
            plans.push(BotDiscardPlan {
                tile_id,
                tile_key: tile.tile_key.clone(),
                score,
            });
        }

        plans.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| right.tile_key.cmp(&left.tile_key))
        });
        plans
    }

    fn score_discard_candidate(
        &mut self,
        context: &BotContext,
        concealed_tiles: &[BotTileView],
        meld_tile_key_groups: &[Vec<String>],
        appended_open_flags: &[bool],
        restricted_discard_tile_key: Option<&str>,
        drawn_tile_id: Option<&str>,
        tile: &BotTileView,
        depth: u8,
    ) -> Option<i64> {
        let concealed_counts =
            tile_counts34(concealed_tiles.iter().map(|item| item.tile_key.as_str()));
        let discard_tile_index = tile_index(&tile.tile_key)?;
        if Some(tile.tile_key.as_str()) == restricted_discard_tile_key {
            return None;
        }
        let mut next_counts = concealed_counts;
        next_counts[discard_tile_index] -= 1;
        let state_score = self.score_13_tile_hand(
            context,
            &next_counts,
            meld_tile_key_groups,
            appended_open_flags,
            depth,
        );
        let prefer_drawn_copy = drawn_tile_id == Some(tile.tile_id.as_str());
        let danger_penalty =
            discard_danger_penalty(context, &concealed_counts, &tile.tile_key);
        Some(
            state_score
                + discard_tile_preference_score(
                    context,
                    &concealed_counts,
                    &tile.tile_key,
                    prefer_drawn_copy,
                )
                - danger_penalty,
        )
    }

    fn score_13_tile_hand(
        &mut self,
        context: &BotContext,
        concealed_counts: &TileCounts,
        meld_tile_key_groups: &[Vec<String>],
        appended_open_flags: &[bool],
        depth: u8,
    ) -> i64 {
        let meld_open_flags =
            meld_open_flags_for_state(context, meld_tile_key_groups, appended_open_flags);
        let compact_melds = compact_melds(meld_tile_key_groups, &meld_open_flags);
        let key = HandStateKey {
            counts: *concealed_counts,
            melds: compact_melds,
            depth,
        };
        if let Some(score) = self.hand_score_cache.get(&key).copied() {
            return score;
        }

        let base = self.base_hand_score(
            context,
            concealed_counts,
            meld_tile_key_groups,
            appended_open_flags,
        );
        let score = if depth == 0 {
            base
        } else {
            let future = self
                .expected_score_after_forced_draw(
                    context,
                    concealed_counts,
                    meld_tile_key_groups,
                    appended_open_flags,
                    None,
                    depth - 1,
                )
                .unwrap_or(base);
            (base * 4 + future * 3) / 7
        };

        self.hand_score_cache.insert(key, score);
        score
    }

    fn base_hand_score(
        &mut self,
        context: &BotContext,
        concealed_counts: &TileCounts,
        meld_tile_key_groups: &[Vec<String>],
        appended_open_flags: &[bool],
    ) -> i64 {
        let open_meld_count = meld_tile_key_groups.len();
        let shanten = self.bot_min_shanten(concealed_counts, open_meld_count);
        let visible_counts = visible_tile_counts_for_state(context, meld_tile_key_groups);
        let mut improving_outs = 0_i64;
        let mut winning_outs = 0_i64;
        let mut fan_potential = 0_i64;

        for (draw_tile_index, remaining) in
            prioritized_draw_candidates(concealed_counts, &visible_counts, BASE_DRAW_SCAN_LIMIT)
        {
            if remaining <= 0 {
                continue;
            }
            if let Some(fan_total) = self.hypothetical_self_draw_fan_total(
                context,
                concealed_counts,
                meld_tile_key_groups,
                appended_open_flags,
                draw_tile_index,
            ) {
                winning_outs += i64::from(remaining);
                fan_potential += i64::from(remaining) * fan_total;
                continue;
            }

            let next_shanten = best_shanten_after_draw(
                self,
                concealed_counts,
                draw_tile_index,
                open_meld_count,
                None,
            );
            if next_shanten < shanten {
                improving_outs += i64::from(remaining) * i64::from(shanten - next_shanten);
            }
        }

        let tenpai_bonus = if shanten == 0 { 600 } else { 0 };
        -i64::from(shanten) * 1800
            + improving_outs * 120
            + winning_outs * 320
            + fan_potential * 36
            + bot_shape_score(concealed_counts)
            + suit_focus_score(concealed_counts, meld_tile_key_groups)
            + honor_value_score(context, concealed_counts)
            + tenpai_bonus
    }

    fn expected_score_after_forced_draw(
        &mut self,
        context: &BotContext,
        concealed_counts_before_draw: &TileCounts,
        meld_tile_key_groups: &[Vec<String>],
        appended_open_flags: &[bool],
        restricted_discard_tile_key: Option<&str>,
        depth: u8,
    ) -> Option<i64> {
        let meld_open_flags =
            meld_open_flags_for_state(context, meld_tile_key_groups, appended_open_flags);
        let compact_melds = compact_melds(meld_tile_key_groups, &meld_open_flags);
        let key = ExpectedDrawKey {
            counts: *concealed_counts_before_draw,
            melds: compact_melds,
            restricted_discard_tile_index: restricted_discard_tile_key
                .and_then(tile_index)
                .map(|index| index as u8),
            depth,
        };
        if let Some(score) = self.expected_draw_cache.get(&key).cloned() {
            return score;
        }

        let visible_counts = visible_tile_counts_for_state(context, meld_tile_key_groups);
        let mut weighted_score = 0_i64;
        let mut total_weight = 0_i64;
        for (draw_tile_index, remaining) in prioritized_draw_candidates(
            concealed_counts_before_draw,
            &visible_counts,
            EXPECTIMAX_DRAW_LIMIT,
        ) {
            if remaining <= 0 {
                continue;
            }
            let score = if let Some(fan_total) = self.hypothetical_self_draw_fan_total(
                context,
                concealed_counts_before_draw,
                meld_tile_key_groups,
                appended_open_flags,
                draw_tile_index,
            ) {
                fan_total * 240 + 1800
            } else {
                let mut counts_after_draw = *concealed_counts_before_draw;
                counts_after_draw[draw_tile_index] =
                    counts_after_draw[draw_tile_index].saturating_add(1);
                self.best_discard_score_from_counts(
                    context,
                    &counts_after_draw,
                    meld_tile_key_groups,
                    appended_open_flags,
                    restricted_discard_tile_key,
                    depth,
                )?
            };
            weighted_score += score * i64::from(remaining);
            total_weight += i64::from(remaining);
        }

        let result = (total_weight > 0).then_some(weighted_score / total_weight);
        self.expected_draw_cache.insert(key, result);
        result
    }

    fn best_discard_score_from_counts(
        &mut self,
        context: &BotContext,
        concealed_counts_before_discard: &TileCounts,
        meld_tile_key_groups: &[Vec<String>],
        appended_open_flags: &[bool],
        restricted_discard_tile_key: Option<&str>,
        depth: u8,
    ) -> Option<i64> {
        let meld_open_flags =
            meld_open_flags_for_state(context, meld_tile_key_groups, appended_open_flags);
        let compact_melds = compact_melds(meld_tile_key_groups, &meld_open_flags);
        let key = BestDiscardKey {
            counts: *concealed_counts_before_discard,
            melds: compact_melds,
            restricted_discard_tile_index: restricted_discard_tile_key
                .and_then(tile_index)
                .map(|index| index as u8),
            depth,
        };
        if let Some(score) = self.best_discard_cache.get(&key).cloned() {
            return score;
        }

        let restricted_discard_tile_index = restricted_discard_tile_key.and_then(tile_index);
        let mut best_score = None;
        for discard_tile_index in 0..TILE_KIND_COUNT {
            if concealed_counts_before_discard[discard_tile_index] == 0 {
                continue;
            }
            if Some(discard_tile_index) == restricted_discard_tile_index {
                continue;
            }
            let mut next_counts = *concealed_counts_before_discard;
            next_counts[discard_tile_index] -= 1;
            let score = self.score_13_tile_hand(
                context,
                &next_counts,
                meld_tile_key_groups,
                appended_open_flags,
                depth,
            );
            best_score = Some(best_score.map_or(score, |current: i64| current.max(score)));
        }

        self.best_discard_cache.insert(key, best_score);
        best_score
    }

    fn hypothetical_self_draw_fan_total(
        &mut self,
        context: &BotContext,
        concealed_counts: &TileCounts,
        meld_tile_key_groups: &[Vec<String>],
        appended_open_flags: &[bool],
        draw_tile_index: usize,
    ) -> Option<i64> {
        let meld_open_flags =
            meld_open_flags_for_state(context, meld_tile_key_groups, appended_open_flags);
        let compact_melds = compact_melds(meld_tile_key_groups, &meld_open_flags);
        let key = WinningKey {
            counts: *concealed_counts,
            melds: compact_melds,
            draw_tile_index: draw_tile_index as u8,
        };
        if let Some(score) = self.winning_fan_cache.get(&key).cloned() {
            return score;
        }

        let incoming_tile = tile_key_for_index(draw_tile_index);
        let concealed_tile_keys = tile_keys_from_counts(concealed_counts);
        let mut effective_concealed = concealed_tile_keys.clone();
        effective_concealed.push(incoming_tile.to_string());
        let decompositions =
            scoring_decompose_winning_hand_with_melds(&effective_concealed, meld_tile_key_groups);
        if decompositions.is_empty() {
            self.winning_fan_cache.insert(key, None);
            return None;
        }

        let open_meld_tile_key_groups = meld_tile_key_groups
            .iter()
            .zip(meld_open_flags.iter())
            .filter_map(|(meld, is_open)| (*is_open).then_some(meld.clone()))
            .collect::<Vec<_>>();
        let features = scoring_extract_hand_features(
            &concealed_tile_keys,
            meld_tile_key_groups,
            Some(&meld_open_flags),
            Some(incoming_tile),
            Some(&seat_wind_key(context.seat_index, context.dealer_seat)),
            context.round_wind.as_deref(),
            Some(&decompositions),
        );
        let result = scoring_evaluate_fans(ScoringEvaluationInput {
            win_type: "self_draw".to_string(),
            winner_seat: Some(context.seat_index),
            discarder_seat: None,
            flower_count: context.player.flower_count,
            seat_count: context.seat_count,
            features,
            timing: ScoringTimingFeatures::default(),
            kong_entries: context.kong_entries.clone(),
            tile_keys: player_tile_keys_from_parts(
                &concealed_tile_keys,
                meld_tile_key_groups,
                Some(incoming_tile),
            ),
            visible_tile_keys: context.visible_tile_keys.clone(),
            concealed_tile_keys,
            meld_tile_key_groups: meld_tile_key_groups.to_vec(),
            open_meld_tile_key_groups,
            incoming_tile: Some(incoming_tile.to_string()),
            decompositions,
        });
        let score = if context.enforce_minimum_eight_fan
            && result.minimum_qualifying_fan_total < 8
        {
            None
        } else {
            Some(result.fan_total.max(result.minimum_qualifying_fan_total))
        };
        self.winning_fan_cache.insert(key, score);
        score
    }

    fn bot_min_shanten(&mut self, concealed_counts: &TileCounts, open_meld_count: usize) -> i32 {
        let key = ShantenKey {
            counts: *concealed_counts,
            open_meld_count: open_meld_count as u8,
        };
        if let Some(shanten) = self.shanten_cache.get(&key).copied() {
            return shanten;
        }
        let shanten = standard_shanten_with_open_melds(concealed_counts, open_meld_count)
            .min(seven_pairs_shanten(concealed_counts, open_meld_count))
            .min(thirteen_orphans_shanten(concealed_counts, open_meld_count));
        self.shanten_cache.insert(key, shanten);
        shanten
    }
}

fn compact_melds(meld_tile_key_groups: &[Vec<String>], open_flags: &[bool]) -> Vec<CompactBotMeld> {
    meld_tile_key_groups
        .iter()
        .enumerate()
        .map(|(index, meld)| {
            let mut tiles = [0_u8; 4];
            for (tile_slot, tile_key) in meld.iter().take(4).enumerate() {
                tiles[tile_slot] = tile_index(tile_key).unwrap_or(0) as u8;
            }
            CompactBotMeld {
                len: meld.len() as u8,
                tiles,
                is_open: open_flags.get(index).copied().unwrap_or(true),
            }
        })
        .collect()
}

fn seat_wind_key(seat_index: usize, dealer_seat: usize) -> String {
    WIND_ORDER[(seat_index + 4 - dealer_seat) % 4].to_string()
}

fn tile_keys_from_counts(counts: &TileCounts) -> Vec<String> {
    let mut tile_keys = Vec::new();
    for (tile_index, count) in counts.iter().enumerate() {
        for _ in 0..usize::from(*count) {
            tile_keys.push(tile_key_for_index(tile_index).to_string());
        }
    }
    tile_keys
}

fn player_tile_keys_from_parts(
    concealed_tile_keys: &[String],
    meld_tile_key_groups: &[Vec<String>],
    incoming_tile: Option<&str>,
) -> Vec<String> {
    let meld_tile_count = meld_tile_key_groups
        .iter()
        .map(|meld| {
            if meld.len() == 4 && meld.iter().all(|tile_key| tile_key == &meld[0]) {
                3
            } else {
                meld.len()
            }
        })
        .sum::<usize>();
    let mut tile_keys = Vec::with_capacity(
        concealed_tile_keys.len() + meld_tile_count + usize::from(incoming_tile.is_some()),
    );
    tile_keys.extend(concealed_tile_keys.iter().cloned());
    for meld in meld_tile_key_groups {
        if meld.len() == 4 && meld.iter().all(|tile_key| tile_key == &meld[0]) {
            tile_keys.extend(meld.iter().take(3).cloned());
        } else {
            tile_keys.extend(meld.iter().cloned());
        }
    }
    if let Some(tile_key) = incoming_tile {
        tile_keys.push(tile_key.to_string());
    }
    tile_keys
}

fn meld_open_flags_for_state(
    context: &BotContext,
    meld_tile_key_groups: &[Vec<String>],
    appended_open_flags: &[bool],
) -> Vec<bool> {
    let existing_meld_count = context
        .player
        .meld_tile_key_groups
        .len()
        .min(meld_tile_key_groups.len());
    let mut flags = Vec::with_capacity(meld_tile_key_groups.len());
    for meld in meld_tile_key_groups.iter().take(existing_meld_count) {
        flags.push(meld_is_open_with_entries(
            context.seat_index,
            meld,
            &context.kong_entries,
        ));
    }
    for extra_index in existing_meld_count..meld_tile_key_groups.len() {
        let appended_index = extra_index - existing_meld_count;
        flags.push(appended_open_flags.get(appended_index).copied().unwrap_or(true));
    }
    flags
}

fn meld_is_open_with_entries(
    seat_index: usize,
    meld: &[String],
    kong_entries: &[ScoringKongEntry],
) -> bool {
    if meld.len() != 4 || !meld.iter().all(|tile_key| tile_key == &meld[0]) {
        return true;
    }

    let tile_key = meld[0].as_str();
    for entry in kong_entries.iter().rev() {
        if entry.actor_seat != seat_index {
            continue;
        }
        if entry
            .tile_key
            .as_deref()
            .is_some_and(|value| value != tile_key)
        {
            continue;
        }
        return entry.kong_type != "concealed_kong";
    }
    true
}

fn apply_visible_meld_delta(
    target: &mut [i32; TILE_KIND_COUNT],
    meld_tile_keys: &[String],
    delta: i32,
) {
    let limit = if meld_tile_keys.len() == 4
        && meld_tile_keys
            .iter()
            .all(|tile_key| tile_key == &meld_tile_keys[0])
    {
        3
    } else {
        meld_tile_keys.len()
    };
    for tile_key in meld_tile_keys.iter().take(limit) {
        if let Some(tile_index) = tile_index(tile_key) {
            target[tile_index] += delta;
        }
    }
}

fn visible_tile_counts_for_state(
    context: &BotContext,
    meld_tile_key_groups: &[Vec<String>],
) -> [i32; TILE_KIND_COUNT] {
    let mut counts = [0_i32; TILE_KIND_COUNT];
    for tile_key in &context.visible_tile_keys {
        if let Some(tile_index) = tile_index(tile_key) {
            counts[tile_index] += 1;
        }
    }

    for meld in &context.player.meld_tile_key_groups {
        apply_visible_meld_delta(&mut counts, meld, -1);
    }
    for meld in meld_tile_key_groups {
        apply_visible_meld_delta(&mut counts, meld, 1);
    }
    counts
}

fn estimated_remaining_tile_count(
    visible_counts: &[i32; TILE_KIND_COUNT],
    concealed_counts: &TileCounts,
    tile_index: usize,
) -> i32 {
    (4 - visible_counts[tile_index] - i32::from(concealed_counts[tile_index])).max(0)
}

fn prioritized_draw_candidates(
    concealed_counts: &TileCounts,
    visible_counts: &[i32; TILE_KIND_COUNT],
    limit: usize,
) -> Vec<(usize, i32)> {
    let mut candidates = Vec::new();
    for tile_index in 0..TILE_KIND_COUNT {
        let remaining = estimated_remaining_tile_count(visible_counts, concealed_counts, tile_index);
        if remaining <= 0 {
            continue;
        }
        let mut priority = remaining * 12 + i32::from(concealed_counts[tile_index]) * 24;
        if tile_index >= HONOR_TILE_START {
            if concealed_counts[tile_index] > 0 {
                priority += 36;
            }
        } else {
            let rank = (tile_index % 9) + 1;
            if (3..=7).contains(&rank) {
                priority += 12;
            }
            if rank >= 2 {
                priority += i32::from(concealed_counts[tile_index - 1]) * 10;
            }
            if rank <= 8 {
                priority += i32::from(concealed_counts[tile_index + 1]) * 10;
            }
            if rank >= 3 {
                priority += i32::from(concealed_counts[tile_index - 2]) * 6;
            }
            if rank <= 7 {
                priority += i32::from(concealed_counts[tile_index + 2]) * 6;
            }
        }
        candidates.push((tile_index, remaining, priority));
    }
    candidates.sort_by(|left, right| right.2.cmp(&left.2).then_with(|| right.1.cmp(&left.1)));
    candidates
        .into_iter()
        .take(limit)
        .map(|(tile_index, remaining, _)| (tile_index, remaining))
        .collect()
}

fn discard_danger_penalty(
    context: &BotContext,
    concealed_counts: &TileCounts,
    tile_key: &str,
) -> i64 {
    let Some(discard_index) = tile_index(tile_key) else {
        return 0;
    };
    let visible_counts = visible_tile_counts_for_state(context, &context.player.meld_tile_key_groups);
    let unseen_copies = i64::from((4 - visible_counts[discard_index]).max(0));
    let round_progress = context
        .opponent_discards_by_seat
        .iter()
        .map(|items| items.len() as i64)
        .sum::<i64>();
    let late_factor = 1 + round_progress / 14;
    let mut penalty = unseen_copies * 10 * late_factor;

    for (seat, discards) in context.opponent_discards_by_seat.iter().enumerate() {
        if seat == context.seat_index {
            continue;
        }
        let same_tile_discards = discards.iter().filter(|key| key.as_str() == tile_key).count() as i64;
        if same_tile_discards > 0 {
            penalty -= 90 * same_tile_discards;
            continue;
        }

        let seat_recent = discards.iter().rev().take(6).cloned().collect::<Vec<_>>();
        if discard_index >= HONOR_TILE_START {
            penalty += 36;
        } else {
            let rank = (discard_index % 9) + 1;
            if rank == 1 || rank == 9 {
                penalty += 22;
            }
            let suit = tile_key.as_bytes()[0] as char;
            let same_suit_recent = seat_recent
                .iter()
                .filter(|key| key.starts_with(suit))
                .count() as i64;
            if same_suit_recent == 0 {
                penalty += 28;
            } else {
                penalty -= 8 * same_suit_recent.min(2);
            }

            let adjacent_recent = seat_recent.iter().any(|key| {
                let Some(other_index) = tile_index(key) else {
                    return false;
                };
                other_index < HONOR_TILE_START
                    && other_index / 9 == discard_index / 9
                    && other_index.abs_diff(discard_index) <= 2
            });
            if adjacent_recent {
                penalty -= 18;
            } else {
                penalty += 20;
            }
        }
    }

    if concealed_counts[discard_index] >= 2 {
        penalty -= 18;
    }
    penalty.max(0)
}

fn discard_tile_preference_score(
    context: &BotContext,
    concealed_counts: &TileCounts,
    tile_key: &str,
    prefer_drawn_copy: bool,
) -> i64 {
    let Some(tile_index) = tile_index(tile_key) else {
        return 0;
    };
    let visible_counts =
        visible_tile_counts_for_state(context, &context.player.meld_tile_key_groups);
    let visible = i64::from(visible_counts[tile_index]);
    let mut score = visible * 24;
    if tile_index >= HONOR_TILE_START {
        score += 72;
    } else {
        let rank = (tile_index % 9) + 1;
        if rank == 1 || rank == 9 {
            score += 28;
        }
        if tile_is_isolated(concealed_counts, tile_index) {
            score += 52;
        }
    }
    if prefer_drawn_copy {
        score += 12;
    }
    score
}

fn preferred_discard_tile_id_for_key(
    concealed_tiles: &[BotTileView],
    tile_key: &str,
    drawn_tile_id: Option<&str>,
) -> Option<String> {
    if let Some(tile_id) = drawn_tile_id {
        if concealed_tiles
            .iter()
            .any(|tile| tile.tile_id == tile_id && tile.tile_key == tile_key)
        {
            return Some(tile_id.to_string());
        }
    }
    concealed_tiles
        .iter()
        .rev()
        .find(|tile| tile.tile_key == tile_key)
        .map(|tile| tile.tile_id.clone())
}

fn simulated_tiles_after_removal(
    concealed_tiles: &[BotTileView],
    removed_tile_ids: &[String],
) -> Vec<BotTileView> {
    let removed = removed_tile_ids.iter().collect::<HashSet<_>>();
    concealed_tiles
        .iter()
        .filter(|tile| !removed.contains(&tile.tile_id))
        .cloned()
        .collect()
}

fn claim_meld_tile_keys(
    action_type: &str,
    discard_tile_key: &str,
    tile_ids: &[String],
    concealed_tiles: &[BotTileView],
) -> Vec<String> {
    if action_type == "chow" {
        let mut meld = tile_ids
            .iter()
            .filter_map(|tile_id| {
                concealed_tiles
                    .iter()
                    .find(|tile| tile.tile_id == *tile_id)
                    .map(|tile| tile.tile_key.clone())
            })
            .collect::<Vec<_>>();
        meld.push(discard_tile_key.to_string());
        meld.sort();
        return meld;
    }
    let count = if action_type == "kong" { 4 } else { 3 };
    vec![discard_tile_key.to_string(); count]
}

fn standard_shanten_with_open_melds(counts: &TileCounts, open_meld_count: usize) -> i32 {
    fn dfs(
        counts: &mut TileCounts,
        start_index: usize,
        melds: i32,
        taatsu: i32,
        has_pair: i32,
        open_meld_count: i32,
        best: &mut i32,
    ) {
        let total_melds = melds + open_meld_count;
        if total_melds > 4 {
            return;
        }
        let available_taatsu = (4 - total_melds).max(0);
        let capped_taatsu = taatsu.min(available_taatsu);
        let shanten = 8 - total_melds * 2 - capped_taatsu - has_pair;
        if shanten < *best {
            *best = shanten;
        }
        if *best <= -1 {
            return;
        }

        let Some(tile_index) = (start_index..TILE_KIND_COUNT).find(|index| counts[*index] > 0)
        else {
            return;
        };

        if counts[tile_index] >= 3 {
            counts[tile_index] -= 3;
            dfs(
                counts,
                tile_index,
                melds + 1,
                taatsu,
                has_pair,
                open_meld_count,
                best,
            );
            counts[tile_index] += 3;
        }

        if tile_index < HONOR_TILE_START
            && tile_index % 9 <= 6
            && counts[tile_index + 1] > 0
            && counts[tile_index + 2] > 0
        {
            counts[tile_index] -= 1;
            counts[tile_index + 1] -= 1;
            counts[tile_index + 2] -= 1;
            dfs(
                counts,
                tile_index,
                melds + 1,
                taatsu,
                has_pair,
                open_meld_count,
                best,
            );
            counts[tile_index] += 1;
            counts[tile_index + 1] += 1;
            counts[tile_index + 2] += 1;
        }

        if has_pair == 0 && counts[tile_index] >= 2 {
            counts[tile_index] -= 2;
            dfs(
                counts,
                tile_index,
                melds,
                taatsu,
                1,
                open_meld_count,
                best,
            );
            counts[tile_index] += 2;
        }

        if taatsu < 4 {
            if counts[tile_index] >= 2 {
                counts[tile_index] -= 2;
                dfs(
                    counts,
                    tile_index,
                    melds,
                    taatsu + 1,
                    has_pair,
                    open_meld_count,
                    best,
                );
                counts[tile_index] += 2;
            }

            if tile_index < HONOR_TILE_START && tile_index % 9 <= 7 && counts[tile_index + 1] > 0
            {
                counts[tile_index] -= 1;
                counts[tile_index + 1] -= 1;
                dfs(
                    counts,
                    tile_index,
                    melds,
                    taatsu + 1,
                    has_pair,
                    open_meld_count,
                    best,
                );
                counts[tile_index] += 1;
                counts[tile_index + 1] += 1;
            }

            if tile_index < HONOR_TILE_START && tile_index % 9 <= 6 && counts[tile_index + 2] > 0
            {
                counts[tile_index] -= 1;
                counts[tile_index + 2] -= 1;
                dfs(
                    counts,
                    tile_index,
                    melds,
                    taatsu + 1,
                    has_pair,
                    open_meld_count,
                    best,
                );
                counts[tile_index] += 1;
                counts[tile_index + 2] += 1;
            }
        }

        counts[tile_index] -= 1;
        dfs(
            counts,
            tile_index,
            melds,
            taatsu,
            has_pair,
            open_meld_count,
            best,
        );
        counts[tile_index] += 1;
    }

    let mut best = 8;
    let mut working = *counts;
    dfs(
        &mut working,
        0,
        0,
        0,
        0,
        open_meld_count as i32,
        &mut best,
    );
    best
}

fn seven_pairs_shanten(counts: &TileCounts, open_meld_count: usize) -> i32 {
    if open_meld_count > 0 {
        return i32::MAX / 4;
    }
    let pair_count = counts.iter().filter(|count| **count >= 2).count() as i32;
    let distinct_count = counts.iter().filter(|count| **count > 0).count() as i32;
    6 - pair_count + (7 - distinct_count).max(0)
}

fn thirteen_orphans_shanten(counts: &TileCounts, open_meld_count: usize) -> i32 {
    if open_meld_count > 0 {
        return i32::MAX / 4;
    }
    const ORPHAN_INDICES: [usize; 13] = [0, 8, 9, 17, 18, 26, 27, 28, 29, 30, 31, 32, 33];
    let unique_count = ORPHAN_INDICES
        .iter()
        .filter(|index| counts[**index] > 0)
        .count() as i32;
    let has_pair = ORPHAN_INDICES.iter().any(|index| counts[*index] >= 2) as i32;
    13 - unique_count - has_pair
}

fn best_shanten_after_draw(
    engine: &mut SearchEngine,
    concealed_counts: &TileCounts,
    draw_tile_index: usize,
    open_meld_count: usize,
    restricted_discard_tile_index: Option<usize>,
) -> i32 {
    let mut counts_after_draw = *concealed_counts;
    counts_after_draw[draw_tile_index] = counts_after_draw[draw_tile_index].saturating_add(1);
    let mut best_shanten = i32::MAX;
    for discard_tile_index in 0..TILE_KIND_COUNT {
        if counts_after_draw[discard_tile_index] == 0 {
            continue;
        }
        if Some(discard_tile_index) == restricted_discard_tile_index {
            continue;
        }
        counts_after_draw[discard_tile_index] -= 1;
        best_shanten = best_shanten.min(engine.bot_min_shanten(&counts_after_draw, open_meld_count));
        counts_after_draw[discard_tile_index] += 1;
    }
    best_shanten
}

fn tile_is_isolated(counts: &TileCounts, tile_index: usize) -> bool {
    if tile_index >= HONOR_TILE_START {
        return counts[tile_index] == 1;
    }
    let rank = tile_index % 9;
    let left_two = if rank >= 2 { Some(tile_index - 2) } else { None };
    let left_one = if rank >= 1 { Some(tile_index - 1) } else { None };
    let right_one = if rank <= 7 { Some(tile_index + 1) } else { None };
    let right_two = if rank <= 6 { Some(tile_index + 2) } else { None };
    [left_two, left_one, right_one, right_two]
        .into_iter()
        .flatten()
        .all(|index| counts[index] == 0)
}

fn bot_shape_score(concealed_counts: &TileCounts) -> i64 {
    let mut score = 0_i64;
    for tile_index in 0..TILE_KIND_COUNT {
        let count = i64::from(concealed_counts[tile_index]);
        if count == 0 {
            continue;
        }
        if tile_index >= HONOR_TILE_START {
            score += match count {
                1 => -28,
                2 => 24,
                3 => 44,
                _ => 48,
            };
            continue;
        }

        let rank = (tile_index % 9) + 1;
        if (3..=7).contains(&rank) {
            score += count * 8;
        } else if rank == 1 || rank == 9 {
            score -= count * 6;
        }

        if count >= 2 {
            score += 22;
        }
        if count >= 3 {
            score += 20;
        }

        if rank <= 8 {
            score +=
                12 * i64::from(concealed_counts[tile_index].min(concealed_counts[tile_index + 1]));
        }
        if rank <= 7 {
            score +=
                8 * i64::from(concealed_counts[tile_index].min(concealed_counts[tile_index + 2]));
        }

        if tile_is_isolated(concealed_counts, tile_index) {
            score -= if rank == 1 || rank == 9 { 36 } else { 20 };
        }
    }
    score
}

fn suit_focus_score(concealed_counts: &TileCounts, meld_tile_key_groups: &[Vec<String>]) -> i64 {
    let mut suit_counts = [0_i64; 3];
    let mut honor_count = 0_i64;
    for tile_index in 0..TILE_KIND_COUNT {
        let count = i64::from(concealed_counts[tile_index]);
        if tile_index >= HONOR_TILE_START {
            honor_count += count;
        } else {
            suit_counts[tile_index / 9] += count;
        }
    }
    for meld in meld_tile_key_groups {
        for tile_key in meld {
            if let Some(tile_index) = tile_index(tile_key) {
                if tile_index >= HONOR_TILE_START {
                    honor_count += 1;
                } else {
                    suit_counts[tile_index / 9] += 1;
                }
            }
        }
    }
    let dominant = suit_counts.into_iter().max().unwrap_or(0);
    let spread_penalty = suit_counts.into_iter().filter(|count| *count > 0).count() as i64 * 18;
    dominant * 12 - honor_count * 4 - spread_penalty
}

fn honor_value_score(context: &BotContext, concealed_counts: &TileCounts) -> i64 {
    let seat_wind = seat_wind_key(context.seat_index, context.dealer_seat);
    let round_wind = context.round_wind.as_deref();
    let mut score = 0_i64;
    for (tile_key, tile_index) in [
        ("east", 27),
        ("south", 28),
        ("west", 29),
        ("north", 30),
        ("red", 31),
        ("green", 32),
        ("white", 33),
    ] {
        let count = concealed_counts[tile_index];
        if count == 0 {
            continue;
        }
        let base = if tile_key == seat_wind || Some(tile_key) == round_wind {
            42
        } else if tile_index >= 31 {
            36
        } else {
            -10
        };
        score += match count {
            1 => i64::from(base),
            2 => i64::from(base) * 2 + 18,
            3 => i64::from(base) * 3 + 42,
            _ => i64::from(base) * 3 + 48,
        };
    }
    score
}

fn tile_counts34<'a>(tile_keys: impl Iterator<Item = &'a str>) -> TileCounts {
    let mut counts = [0_u8; TILE_KIND_COUNT];
    for tile_key in tile_keys {
        if let Some(tile_index) = tile_index(tile_key) {
            counts[tile_index] = counts[tile_index].saturating_add(1);
        }
    }
    counts
}

fn tile_index(tile_key: &str) -> Option<usize> {
    match tile_key {
        "east" => Some(27),
        "south" => Some(28),
        "west" => Some(29),
        "north" => Some(30),
        "red" => Some(31),
        "green" => Some(32),
        "white" => Some(33),
        _ => {
            let bytes = tile_key.as_bytes();
            if bytes.len() != 2 {
                return None;
            }
            let suit_offset = match bytes[0] {
                b'w' => 0,
                b't' => 9,
                b'b' => 18,
                _ => return None,
            };
            let rank = usize::from(bytes[1].checked_sub(b'0')?);
            if !(1..=9).contains(&rank) {
                return None;
            }
            Some(suit_offset + rank - 1)
        }
    }
}

fn tile_key_for_index(tile_index: usize) -> &'static str {
    STANDARD_TILE_KEYS
        .get(tile_index)
        .copied()
        .unwrap_or_default()
}
