use super::context::*;
use crate::rules::scoring::{
    EvaluationInput as ScoringEvaluationInput, TimingFeatures as ScoringTimingFeatures,
    decompose_winning_hand_with_melds as scoring_decompose_winning_hand_with_melds,
    evaluate_fans as scoring_evaluate_fans, extract_hand_features as scoring_extract_hand_features,
};
use crate::rules::standard::win::BOT_MINIMUM_HU_FAN;
use rand::{Rng, SeedableRng, rngs::StdRng};
use std::collections::{HashMap, HashSet, hash_map::DefaultHasher};
use std::env;
use std::hash::{Hash, Hasher};

pub(crate) const STAGE_ONE_DEPTH: u8 = 0;
const STAGE_TWO_DEPTH: u8 = 1;
const STAGE_TWO_CANDIDATES: usize = 3;
const STAGE_TWO_CANDIDATES_PRESSURE: usize = 4;
const STAGE_ONE_EARLY_RETURN_MARGIN: i64 = 150;
const KONG_SCORE_MARGIN: i64 = 80;
const CLAIM_SCORE_MARGIN: i64 = 100;
const BASE_DRAW_SCAN_LIMIT: usize = 18;
const EXPECTIMAX_DRAW_LIMIT: usize = 14;
const MONTE_CARLO_SAMPLE_COUNT_EARLY: usize = 15;
const MONTE_CARLO_SAMPLE_COUNT_MID: usize = 10;
const MONTE_CARLO_SAMPLE_COUNT_LATE: usize = 8;
const MONTE_CARLO_HORIZON_EARLY: usize = 2;
const MONTE_CARLO_HORIZON_MID: usize = 1;
const MONTE_CARLO_HORIZON_LATE: usize = 1;
const MONTE_CARLO_SCORE_GAP_LIMIT: i64 = 110;
const ORPHAN_INDICES: [usize; 13] = [0, 8, 9, 17, 18, 26, 27, 28, 29, 30, 31, 32, 33];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BotStrength {
    Normal,
    Strong,
}

impl BotStrength {
    fn from_env() -> Self {
        env::var("MAHJONG_BOT_STRENGTH")
            .ok()
            .and_then(|value| Self::parse(&value))
            .unwrap_or(Self::Normal)
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "normal" | "default" | "balanced" => Some(Self::Normal),
            "strong" | "enhanced" => Some(Self::Strong),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BotSearchBudget {
    strength: BotStrength,
}

impl BotSearchBudget {
    fn for_strength(strength: BotStrength) -> Self {
        Self { strength }
    }

    fn from_env() -> Self {
        Self::for_strength(BotStrength::from_env())
    }

    fn stage_two_candidate_limit(self, critical: bool) -> usize {
        match (self.strength, critical) {
            (BotStrength::Strong, true) => 5,
            (_, true) => STAGE_TWO_CANDIDATES_PRESSURE,
            _ => STAGE_TWO_CANDIDATES,
        }
    }

    fn expectimax_draw_limit(self, critical: bool) -> usize {
        match (self.strength, critical) {
            (BotStrength::Strong, true) => 16,
            _ => EXPECTIMAX_DRAW_LIMIT,
        }
    }

    fn monte_carlo_sample_limit(self, base: usize, critical: bool) -> usize {
        match (self.strength, critical) {
            (BotStrength::Strong, true) => base + 2,
            _ => base,
        }
    }
}

#[derive(Clone)]
pub(crate) struct BotDiscardPlan {
    pub(crate) tile_id: String,
    pub(crate) tile_key: String,
    pub(crate) score: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DiscardDecisionTelemetry {
    pub(crate) stage_one_candidates: usize,
    pub(crate) finalist_gap: Option<i64>,
    pub(crate) stage_two_candidates: usize,
    pub(crate) ran_stage_two: bool,
    pub(crate) ran_monte_carlo: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BotMode {
    Balanced,
    LeadingConservative,
    TrailingAggressive,
    LateDefense,
}

#[derive(Clone, Copy)]
struct ModeProfile {
    shanten_weight: i64,
    improving_weight: i64,
    winning_weight: i64,
    fan_weight: i64,
    danger_weight: i64,
    kong_margin: i64,
    claim_margin: i64,
}

#[derive(Clone, Copy, Default)]
struct OpponentThreat {
    pressure: i64,
    tenpai_likelihood: i64,
    high_tenpai_probability: bool,
    flush_suit: Option<usize>,
    honor_focus: bool,
    dragon_focus: bool,
    central_wait_bias: bool,
    edge_wait_bias: bool,
    hand_value: i64,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct StrategicSignals {
    pub(crate) route_score: i64,
    pub(crate) fan_estimate: i64,
    pub(crate) closed_route_score: i64,
    pub(crate) sequence_route_score: i64,
    pub(crate) triplet_route_score: i64,
}

#[derive(Clone, Copy, Default)]
struct RoutePatternProgress {
    fan_estimate: i64,
    route_bonus: i64,
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
struct StaticStateKey {
    counts: TileCounts,
    melds: Vec<CompactBotMeld>,
}

#[derive(Clone)]
struct CachedStateAnalysis {
    meld_open_flags: Vec<bool>,
    visible_counts: [i32; TILE_KIND_COUNT],
    concealed_tile_keys: Vec<String>,
    open_meld_tile_key_groups: Vec<Vec<String>>,
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

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct ShantenAfterDrawKey {
    counts: TileCounts,
    draw_tile_index: u8,
    open_meld_count: u8,
    restricted_discard_tile_index: Option<u8>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct DiscardStateKey {
    counts: TileCounts,
    tile_index: u8,
}

pub(crate) struct SearchEngine {
    profile: ModeProfile,
    budget: BotSearchBudget,
    monte_carlo_safety_weight: i64,
    threat_profiles: Vec<OpponentThreat>,
    base_visible_counts: [i32; TILE_KIND_COUNT],
    base_hand_score_cache: HashMap<StaticStateKey, i64>,
    hand_score_cache: HashMap<HandStateKey, i64>,
    expected_draw_cache: HashMap<ExpectedDrawKey, Option<i64>>,
    best_discard_cache: HashMap<BestDiscardKey, Option<i64>>,
    state_analysis_cache: HashMap<StaticStateKey, CachedStateAnalysis>,
    strategic_signals_cache: HashMap<StaticStateKey, StrategicSignals>,
    winning_fan_cache: HashMap<WinningKey, Option<i64>>,
    shanten_cache: HashMap<ShantenKey, i32>,
    shanten_after_draw_cache: HashMap<ShantenAfterDrawKey, i32>,
    discard_danger_cache: HashMap<DiscardStateKey, i64>,
    deal_in_ev_cache: HashMap<DiscardStateKey, i64>,
    discard_preference_cache: HashMap<DiscardStateKey, i64>,
    strongest_threat_seat: Option<usize>,
    last_discard_telemetry: Option<DiscardDecisionTelemetry>,
}

fn select_bot_mode(context: &BotContext) -> BotMode {
    let late_round = context.wall_tiles_remaining > 0 && context.wall_tiles_remaining <= 18;
    if late_round {
        return BotMode::LateDefense;
    }

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
    let worst_other = context
        .cumulative_scores
        .iter()
        .enumerate()
        .filter(|(seat, _)| *seat != context.seat_index)
        .map(|(_, score)| *score)
        .min()
        .unwrap_or(my_score);

    if my_score - best_other >= 24 {
        return BotMode::LeadingConservative;
    }
    if worst_other - my_score >= 16 || best_other - my_score >= 24 {
        return BotMode::TrailingAggressive;
    }
    BotMode::Balanced
}

fn mode_profile(mode: BotMode) -> ModeProfile {
    match mode {
        BotMode::Balanced => ModeProfile {
            shanten_weight: 1800,
            improving_weight: 120,
            winning_weight: 320,
            fan_weight: 36,
            danger_weight: 100,
            kong_margin: KONG_SCORE_MARGIN,
            claim_margin: CLAIM_SCORE_MARGIN,
        },
        BotMode::LeadingConservative => ModeProfile {
            shanten_weight: 1650,
            improving_weight: 110,
            winning_weight: 280,
            fan_weight: 26,
            danger_weight: 155,
            kong_margin: KONG_SCORE_MARGIN + 60,
            claim_margin: CLAIM_SCORE_MARGIN + 50,
        },
        BotMode::TrailingAggressive => ModeProfile {
            shanten_weight: 1750,
            improving_weight: 135,
            winning_weight: 360,
            fan_weight: 58,
            danger_weight: 72,
            kong_margin: (KONG_SCORE_MARGIN - 20).max(20),
            claim_margin: (CLAIM_SCORE_MARGIN - 25).max(20),
        },
        BotMode::LateDefense => ModeProfile {
            shanten_weight: 1200,
            improving_weight: 80,
            winning_weight: 220,
            fan_weight: 20,
            danger_weight: 210,
            kong_margin: KONG_SCORE_MARGIN + 120,
            claim_margin: CLAIM_SCORE_MARGIN + 100,
        },
    }
}

fn profile_for_context(context: &BotContext) -> ModeProfile {
    let mut profile = mode_profile(select_bot_mode(context));
    let (offense_pressure, defense_pressure) = placement_pressure(context);

    profile.shanten_weight =
        (profile.shanten_weight + offense_pressure * 8 - defense_pressure * 6).clamp(1150, 1950);
    profile.improving_weight =
        (profile.improving_weight + offense_pressure - defense_pressure / 2).clamp(70, 150);
    profile.winning_weight =
        (profile.winning_weight + offense_pressure * 5 - defense_pressure * 2).clamp(220, 460);
    profile.fan_weight =
        (profile.fan_weight + offense_pressure * 2 - defense_pressure).clamp(18, 72);
    profile.danger_weight =
        (profile.danger_weight + defense_pressure * 3 - offense_pressure).clamp(70, 240);
    profile.kong_margin =
        (profile.kong_margin - offense_pressure * 3 + defense_pressure * 4).clamp(20, 260);
    profile.claim_margin =
        (profile.claim_margin - offense_pressure * 3 + defense_pressure * 4).clamp(20, 260);
    profile
}

fn placement_pressure(context: &BotContext) -> (i64, i64) {
    let my_score = context
        .cumulative_scores
        .get(context.seat_index)
        .copied()
        .unwrap_or(0);
    let mut standings = context
        .cumulative_scores
        .iter()
        .copied()
        .enumerate()
        .collect::<Vec<_>>();
    standings.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    let Some(my_rank) = standings
        .iter()
        .position(|(seat_index, _)| *seat_index == context.seat_index)
    else {
        return (0, 0);
    };

    let gap_to_above = my_rank
        .checked_sub(1)
        .and_then(|index| standings.get(index))
        .map(|(_, score)| score - my_score)
        .unwrap_or(0);
    let gap_to_below = standings
        .get(my_rank + 1)
        .map(|(_, score)| my_score - score)
        .unwrap_or(i64::MAX / 4);
    let top_gap = standings
        .first()
        .map(|(_, score)| score - my_score)
        .unwrap_or(0)
        .max(0);

    let mut offense_pressure = 0_i64;
    if my_rank > 0 && gap_to_above > 0 {
        offense_pressure += match gap_to_above {
            i64::MIN..=4 => 24,
            5..=8 => 18,
            9..=14 => 10,
            _ => 0,
        };
        offense_pressure += match top_gap {
            32.. => 14,
            20..=31 => 8,
            _ => 0,
        };
        if my_rank + 1 == standings.len() {
            offense_pressure += 8;
        }
    }

    let mut defense_pressure = 0_i64;
    if my_rank + 1 < standings.len() && gap_to_below > 0 {
        defense_pressure += match gap_to_below {
            i64::MIN..=3 => 26,
            4..=6 => 18,
            7..=10 => 10,
            _ => 0,
        };
        if context.wall_tiles_remaining <= 24 && gap_to_below <= 10 {
            defense_pressure += 10;
        }
    }
    if my_rank == 0 && gap_to_below > 0 {
        defense_pressure += match gap_to_below {
            16.. => 10,
            10..=15 => 6,
            _ => 0,
        };
    }

    (offense_pressure, defense_pressure)
}

fn is_budget_critical_state(context: &BotContext, shanten: i32, strongest_threat: i64) -> bool {
    shanten <= 1 || context.wall_tiles_remaining <= 14 || strongest_threat >= 90
}

impl SearchEngine {
    pub(crate) fn new(context: &BotContext) -> Self {
        let profile = profile_for_context(context);
        let threat_profiles = (0..context.seat_count)
            .map(|seat| {
                if seat == context.seat_index {
                    OpponentThreat::default()
                } else {
                    opponent_threat_profile(context, seat)
                }
            })
            .collect::<Vec<_>>();
        let strongest_threat_seat = threat_profiles
            .iter()
            .enumerate()
            .filter(|(seat, _)| *seat != context.seat_index)
            .map(|(seat, threat)| (seat, threat.pressure + threat.tenpai_likelihood))
            .max_by_key(|(_, score)| *score)
            .and_then(|(seat, score)| (score > 0).then_some(seat));

        Self {
            profile,
            budget: BotSearchBudget::from_env(),
            monte_carlo_safety_weight: monte_carlo_safety_weight_from_threats(
                context,
                &threat_profiles,
            ),
            threat_profiles,
            base_visible_counts: known_visible_tile_counts(context),
            base_hand_score_cache: HashMap::new(),
            hand_score_cache: HashMap::new(),
            expected_draw_cache: HashMap::new(),
            best_discard_cache: HashMap::new(),
            state_analysis_cache: HashMap::new(),
            strategic_signals_cache: HashMap::new(),
            winning_fan_cache: HashMap::new(),
            shanten_cache: HashMap::new(),
            shanten_after_draw_cache: HashMap::new(),
            discard_danger_cache: HashMap::new(),
            deal_in_ev_cache: HashMap::new(),
            discard_preference_cache: HashMap::new(),
            strongest_threat_seat,
            last_discard_telemetry: None,
        }
    }

    pub(crate) fn kong_margin(&self) -> i64 {
        self.profile.kong_margin
    }

    pub(crate) fn claim_margin(&self) -> i64 {
        self.profile.claim_margin
    }

    fn deal_in_ev_cost(
        &mut self,
        context: &BotContext,
        concealed_counts: &TileCounts,
        discard_tile_index: usize,
    ) -> i64 {
        let key = DiscardStateKey {
            counts: *concealed_counts,
            tile_index: discard_tile_index as u8,
        };
        if let Some(score) = self.deal_in_ev_cache.get(&key).copied() {
            return score;
        }

        let known_count = |index: usize| {
            i64::from(self.base_visible_counts[index]) + i64::from(concealed_counts[index])
        };
        let tile_key = tile_key_for_index(discard_tile_index);
        let mut total_cost = 0_i64;
        for (seat, discards) in context.opponent_discards_by_seat.iter().enumerate() {
            if seat == context.seat_index {
                continue;
            }
            let threat = self.threat_profiles.get(seat).copied().unwrap_or_default();
            let seat_risk = seat_discard_danger_score(
                &threat,
                discard_tile_index,
                tile_key,
                discards,
                &known_count,
            );
            if seat_risk <= 0 || threat.hand_value <= 0 {
                continue;
            }
            total_cost += seat_risk * threat.hand_value / 120;
        }

        let result = total_cost.max(0);
        self.deal_in_ev_cache.insert(key, result);
        result
    }

    pub(crate) fn strongest_threat_opponent(&self, context: &BotContext) -> Option<(usize, i64)> {
        self.threat_profiles
            .iter()
            .enumerate()
            .filter(|(seat, _)| *seat != context.seat_index)
            .map(|(seat, threat)| (seat, threat.pressure + threat.tenpai_likelihood))
            .max_by_key(|(_, score)| *score)
    }

    pub(crate) fn last_discard_telemetry(&self) -> Option<DiscardDecisionTelemetry> {
        self.last_discard_telemetry
    }

    fn state_analysis(
        &mut self,
        context: &BotContext,
        concealed_counts: &TileCounts,
        meld_tile_key_groups: &[Vec<String>],
        appended_open_flags: &[bool],
    ) -> (StaticStateKey, CachedStateAnalysis) {
        let meld_open_flags =
            meld_open_flags_for_state(context, meld_tile_key_groups, appended_open_flags);
        let key = StaticStateKey {
            counts: *concealed_counts,
            melds: compact_melds(meld_tile_key_groups, &meld_open_flags),
        };
        if let Some(analysis) = self.state_analysis_cache.get(&key).cloned() {
            return (key, analysis);
        }

        let visible_counts = visible_tile_counts_for_state(context, meld_tile_key_groups);
        let concealed_tile_keys = tile_keys_from_counts(concealed_counts);
        let open_meld_tile_key_groups = meld_tile_key_groups
            .iter()
            .zip(meld_open_flags.iter())
            .filter_map(|(meld, is_open)| (*is_open).then_some(meld.clone()))
            .collect::<Vec<_>>();
        let analysis = CachedStateAnalysis {
            meld_open_flags,
            visible_counts,
            concealed_tile_keys,
            open_meld_tile_key_groups,
        };
        self.state_analysis_cache
            .insert(key.clone(), analysis.clone());
        (key, analysis)
    }

    pub(crate) fn strategic_signals_for_state(
        &mut self,
        context: &BotContext,
        concealed_counts: &TileCounts,
        meld_tile_key_groups: &[Vec<String>],
        appended_open_flags: &[bool],
    ) -> StrategicSignals {
        let (key, analysis) = self.state_analysis(
            context,
            concealed_counts,
            meld_tile_key_groups,
            appended_open_flags,
        );
        if let Some(signals) = self.strategic_signals_cache.get(&key).copied() {
            return signals;
        }

        let signals = strategic_signals(
            context,
            concealed_counts,
            meld_tile_key_groups,
            &analysis.meld_open_flags,
        );
        self.strategic_signals_cache.insert(key, signals);
        signals
    }
}

impl SearchEngine {
    pub(crate) fn best_discard_plan(
        &mut self,
        context: &BotContext,
        concealed_tiles: &[BotTileView],
        concealed_counts: &TileCounts,
        meld_tile_key_groups: &[Vec<String>],
        appended_open_flags: &[bool],
        restricted_discard_tile_key: Option<&str>,
        drawn_tile_id: Option<&str>,
    ) -> Option<BotDiscardPlan> {
        let stage_one = self.rank_discard_plans_at_depth(
            context,
            concealed_tiles,
            concealed_counts,
            meld_tile_key_groups,
            appended_open_flags,
            restricted_discard_tile_key,
            drawn_tile_id,
            STAGE_ONE_DEPTH,
        );
        let mut telemetry = DiscardDecisionTelemetry {
            stage_one_candidates: stage_one.len(),
            ..Default::default()
        };
        if stage_one.is_empty() {
            self.last_discard_telemetry = Some(telemetry);
            return None;
        }
        if stage_one.len() == 1 {
            self.last_discard_telemetry = Some(telemetry);
            return stage_one.into_iter().next();
        }
        let finalist_gap = stage_one[0].score - stage_one[1].score;
        telemetry.finalist_gap = Some(finalist_gap);
        if finalist_gap >= STAGE_ONE_EARLY_RETURN_MARGIN {
            self.last_discard_telemetry = Some(telemetry);
            return stage_one.into_iter().next();
        }

        let stage_two_candidates = self
            .stage_two_candidate_count(
                context,
                concealed_counts,
                meld_tile_key_groups,
                finalist_gap,
            )
            .min(stage_one.len());
        let run_monte_carlo = self.should_run_monte_carlo(
            context,
            concealed_counts,
            meld_tile_key_groups,
            finalist_gap,
        );
        telemetry.ran_stage_two = true;
        telemetry.stage_two_candidates = stage_two_candidates;
        telemetry.ran_monte_carlo = run_monte_carlo;
        let mut finalists = stage_one
            .into_iter()
            .take(stage_two_candidates)
            .collect::<Vec<_>>();
        for finalist in &mut finalists {
            if let Some(tile) = concealed_tiles
                .iter()
                .find(|tile| tile.tile_id == finalist.tile_id)
                .cloned()
            {
                let stage_two_score = self.score_discard_candidate(
                    context,
                    concealed_counts,
                    meld_tile_key_groups,
                    appended_open_flags,
                    restricted_discard_tile_key,
                    drawn_tile_id,
                    &tile,
                    STAGE_TWO_DEPTH,
                )?;
                finalist.score = if run_monte_carlo {
                    if let Some(monte_carlo_score) = self.monte_carlo_discard_score(
                        context,
                        concealed_counts,
                        meld_tile_key_groups,
                        appended_open_flags,
                        restricted_discard_tile_key,
                        &tile,
                    ) {
                        (stage_two_score * 5 + monte_carlo_score * 4) / 9
                    } else {
                        stage_two_score
                    }
                } else {
                    stage_two_score
                };
            }
        }
        finalists.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| right.tile_key.cmp(&left.tile_key))
        });
        self.last_discard_telemetry = Some(telemetry);
        finalists.into_iter().next()
    }

    fn stage_two_candidate_count(
        &mut self,
        context: &BotContext,
        concealed_counts: &TileCounts,
        meld_tile_key_groups: &[Vec<String>],
        finalist_gap: i64,
    ) -> usize {
        if finalist_gap > MONTE_CARLO_SCORE_GAP_LIMIT / 2 {
            return STAGE_TWO_CANDIDATES;
        }

        let shanten = self.bot_min_shanten(concealed_counts, meld_tile_key_groups.len());
        let strongest_threat = self
            .strongest_threat_opponent(context)
            .map(|(_, score)| score)
            .unwrap_or(0);
        self.budget
            .stage_two_candidate_limit(is_budget_critical_state(context, shanten, strongest_threat))
    }

    fn should_run_monte_carlo(
        &mut self,
        context: &BotContext,
        concealed_counts: &TileCounts,
        meld_tile_key_groups: &[Vec<String>],
        finalist_gap: i64,
    ) -> bool {
        if finalist_gap > MONTE_CARLO_SCORE_GAP_LIMIT {
            return false;
        }
        if context.wall_tiles_remaining <= 14 {
            return true;
        }
        self.bot_min_shanten(concealed_counts, meld_tile_key_groups.len()) <= 1
    }

    fn rank_discard_plans_at_depth(
        &mut self,
        context: &BotContext,
        concealed_tiles: &[BotTileView],
        concealed_counts: &TileCounts,
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
                concealed_counts,
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
        concealed_counts: &TileCounts,
        meld_tile_key_groups: &[Vec<String>],
        appended_open_flags: &[bool],
        restricted_discard_tile_key: Option<&str>,
        drawn_tile_id: Option<&str>,
        tile: &BotTileView,
        depth: u8,
    ) -> Option<i64> {
        let discard_tile_index = tile_index(&tile.tile_key)?;
        if Some(tile.tile_key.as_str()) == restricted_discard_tile_key {
            return None;
        }
        let mut next_counts = *concealed_counts;
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
            self.discard_danger_penalty(context, concealed_counts, discard_tile_index)
                * self.profile.danger_weight
                / 100;
        Some(
            state_score
                + self.discard_tile_preference_score(
                    concealed_counts,
                    discard_tile_index,
                    prefer_drawn_copy,
                )
                - danger_penalty,
        )
    }

    fn monte_carlo_discard_score(
        &mut self,
        context: &BotContext,
        concealed_counts: &TileCounts,
        meld_tile_key_groups: &[Vec<String>],
        appended_open_flags: &[bool],
        restricted_discard_tile_key: Option<&str>,
        tile: &BotTileView,
    ) -> Option<i64> {
        if context.wall_tiles_remaining <= 0 {
            return None;
        }
        let discard_tile_index = tile_index(&tile.tile_key)?;
        if concealed_counts[discard_tile_index] == 0
            || Some(tile.tile_key.as_str()) == restricted_discard_tile_key
        {
            return None;
        }

        let mut next_counts = *concealed_counts;
        next_counts[discard_tile_index] -= 1;
        let initial_risk =
            self.discard_danger_penalty(context, concealed_counts, discard_tile_index)
                * self.monte_carlo_safety_weight
                / 100;
        let initial_deal_in_cost =
            self.deal_in_ev_cost(context, concealed_counts, discard_tile_index)
                * self.monte_carlo_safety_weight
                / 160;
        let opening_score = self.base_hand_score(
            context,
            &next_counts,
            meld_tile_key_groups,
            appended_open_flags,
        ) - initial_risk
            - initial_deal_in_cost;

        let visible_counts = visible_tile_counts_for_state(context, meld_tile_key_groups);
        let hidden_pool = hidden_tile_pool(&next_counts, &visible_counts);
        if hidden_pool.is_empty() {
            return Some(opening_score);
        }

        let sample_count = self.monte_carlo_sample_count_for_state(
            context,
            &next_counts,
            meld_tile_key_groups,
            hidden_pool.len(),
        );
        let horizon =
            self.monte_carlo_horizon_for_state(context, &next_counts, meld_tile_key_groups);
        let seed = monte_carlo_seed(
            context,
            &next_counts,
            meld_tile_key_groups,
            appended_open_flags,
            discard_tile_index,
        );

        let mut rollout_total = 0_i64;
        let mut completed_rollouts = 0_i64;
        for sample in 0..sample_count {
            let mut rng = StdRng::seed_from_u64(seed ^ monte_carlo_mix(sample as u64 + 1));
            let Some(score) = self.simulate_rollout_after_discard(
                context,
                &next_counts,
                meld_tile_key_groups,
                appended_open_flags,
                restricted_discard_tile_key,
                &hidden_pool,
                horizon,
                self.monte_carlo_safety_weight,
                &mut rng,
            ) else {
                continue;
            };
            rollout_total += score;
            completed_rollouts += 1;
        }

        if completed_rollouts == 0 {
            return Some(opening_score);
        }
        let average_rollout_score = rollout_total / completed_rollouts;
        Some((opening_score * 3 + average_rollout_score * 4) / 7)
    }

    fn monte_carlo_sample_count_for_state(
        &mut self,
        context: &BotContext,
        concealed_counts: &TileCounts,
        meld_tile_key_groups: &[Vec<String>],
        hidden_tile_count: usize,
    ) -> usize {
        let base = monte_carlo_sample_count(context, hidden_tile_count);
        let shanten = self.bot_min_shanten(concealed_counts, meld_tile_key_groups.len());
        let strongest_threat = self
            .strongest_threat_opponent(context)
            .map(|(_, score)| score)
            .unwrap_or(0);
        let bonus = usize::from(shanten <= 1) * 2
            + usize::from(strongest_threat >= 90)
            + usize::from(context.wall_tiles_remaining <= 14);
        self.budget
            .monte_carlo_sample_limit(
                base + bonus,
                is_budget_critical_state(context, shanten, strongest_threat),
            )
            .min(hidden_tile_count.max(1))
    }

    fn monte_carlo_horizon_for_state(
        &mut self,
        context: &BotContext,
        concealed_counts: &TileCounts,
        meld_tile_key_groups: &[Vec<String>],
    ) -> usize {
        let base = monte_carlo_horizon(context);
        let shanten = self.bot_min_shanten(concealed_counts, meld_tile_key_groups.len());
        if base == 1 && shanten <= 1 && context.wall_tiles_remaining >= 16 {
            2
        } else {
            base
        }
    }

    fn simulate_rollout_after_discard(
        &mut self,
        context: &BotContext,
        concealed_counts: &TileCounts,
        meld_tile_key_groups: &[Vec<String>],
        appended_open_flags: &[bool],
        restricted_discard_tile_key: Option<&str>,
        hidden_pool: &[usize],
        horizon: usize,
        safety_weight: i64,
        rng: &mut StdRng,
    ) -> Option<i64> {
        let mut counts = *concealed_counts;
        let mut pool = hidden_pool.to_vec();
        let mut score =
            self.base_hand_score(context, &counts, meld_tile_key_groups, appended_open_flags);

        for step in 0..horizon {
            burn_hidden_tiles(&mut pool, context.seat_count.saturating_sub(1), rng);
            let Some(draw_tile_index) = remove_random_hidden_tile(&mut pool, rng) else {
                break;
            };

            if let Some(fan_total) = self.hypothetical_self_draw_fan_total(
                context,
                &counts,
                meld_tile_key_groups,
                appended_open_flags,
                draw_tile_index,
            ) {
                let win_bonus = fan_total * 280 + 2200 - step as i64 * 120;
                return Some(score.max(win_bonus));
            }

            counts[draw_tile_index] = counts[draw_tile_index].saturating_add(1);
            let (discard_tile_index, rollout_score) = self.best_rollout_discard_from_counts(
                context,
                &counts,
                meld_tile_key_groups,
                appended_open_flags,
                restricted_discard_tile_key,
                safety_weight,
            )?;
            counts[discard_tile_index] -= 1;
            score = (score * 2 + rollout_score) / 3;
        }

        Some(score)
    }

    fn best_rollout_discard_from_counts(
        &mut self,
        context: &BotContext,
        concealed_counts_before_discard: &TileCounts,
        meld_tile_key_groups: &[Vec<String>],
        appended_open_flags: &[bool],
        restricted_discard_tile_key: Option<&str>,
        safety_weight: i64,
    ) -> Option<(usize, i64)> {
        let restricted_discard_tile_index = restricted_discard_tile_key.and_then(tile_index);
        let mut best = None;

        for discard_tile_index in 0..TILE_KIND_COUNT {
            if concealed_counts_before_discard[discard_tile_index] == 0 {
                continue;
            }
            if Some(discard_tile_index) == restricted_discard_tile_index {
                continue;
            }

            let mut next_counts = *concealed_counts_before_discard;
            next_counts[discard_tile_index] -= 1;
            let base_score = self.base_hand_score(
                context,
                &next_counts,
                meld_tile_key_groups,
                appended_open_flags,
            );
            let preference_score = self.discard_tile_preference_score(
                concealed_counts_before_discard,
                discard_tile_index,
                false,
            ) / 2;
            let safety_penalty = self.discard_danger_penalty(
                context,
                concealed_counts_before_discard,
                discard_tile_index,
            ) * safety_weight
                / 100;
            let deal_in_cost =
                self.deal_in_ev_cost(context, concealed_counts_before_discard, discard_tile_index)
                    * safety_weight
                    / 160;
            let total_score = base_score + preference_score - safety_penalty - deal_in_cost;
            let replace = best
                .as_ref()
                .map(|(_, best_score): &(usize, i64)| total_score > *best_score)
                .unwrap_or(true);
            if replace {
                best = Some((discard_tile_index, total_score));
            }
        }

        best
    }

    pub(crate) fn score_13_tile_hand(
        &mut self,
        context: &BotContext,
        concealed_counts: &TileCounts,
        meld_tile_key_groups: &[Vec<String>],
        appended_open_flags: &[bool],
        depth: u8,
    ) -> i64 {
        let (state_key, _) = self.state_analysis(
            context,
            concealed_counts,
            meld_tile_key_groups,
            appended_open_flags,
        );
        let key = HandStateKey {
            counts: *concealed_counts,
            melds: state_key.melds.clone(),
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
        let (state_key, analysis) = self.state_analysis(
            context,
            concealed_counts,
            meld_tile_key_groups,
            appended_open_flags,
        );
        if let Some(score) = self.base_hand_score_cache.get(&state_key).copied() {
            return score;
        }

        let open_meld_count = meld_tile_key_groups.len();
        let shanten = self.bot_min_shanten(concealed_counts, open_meld_count);
        let strategic = self
            .strategic_signals_cache
            .get(&state_key)
            .copied()
            .unwrap_or_else(|| {
                let signals = strategic_signals(
                    context,
                    concealed_counts,
                    meld_tile_key_groups,
                    &analysis.meld_open_flags,
                );
                self.strategic_signals_cache
                    .insert(state_key.clone(), signals);
                signals
            });
        let mut improving_outs = 0_i64;
        let mut winning_outs = 0_i64;
        let mut fan_potential = 0_i64;

        for (draw_tile_index, remaining) in prioritized_draw_candidates(
            concealed_counts,
            &analysis.visible_counts,
            BASE_DRAW_SCAN_LIMIT,
        ) {
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
        let score = -i64::from(shanten) * self.profile.shanten_weight
            + improving_outs * self.profile.improving_weight
            + winning_outs * self.profile.winning_weight
            + fan_potential * self.profile.fan_weight
            + bot_shape_score(concealed_counts)
            + suit_focus_score(concealed_counts, meld_tile_key_groups)
            + honor_value_score(context, concealed_counts)
            + strategic.route_score
            + tenpai_bonus;
        self.base_hand_score_cache.insert(state_key, score);
        score
    }

    pub(crate) fn expected_score_after_forced_draw(
        &mut self,
        context: &BotContext,
        concealed_counts_before_draw: &TileCounts,
        meld_tile_key_groups: &[Vec<String>],
        appended_open_flags: &[bool],
        restricted_discard_tile_key: Option<&str>,
        depth: u8,
    ) -> Option<i64> {
        let (state_key, analysis) = self.state_analysis(
            context,
            concealed_counts_before_draw,
            meld_tile_key_groups,
            appended_open_flags,
        );
        let key = ExpectedDrawKey {
            counts: *concealed_counts_before_draw,
            melds: state_key.melds.clone(),
            restricted_discard_tile_index: restricted_discard_tile_key
                .and_then(tile_index)
                .map(|index| index as u8),
            depth,
        };
        if let Some(score) = self.expected_draw_cache.get(&key).cloned() {
            return score;
        }

        let mut weighted_score = 0_i64;
        let mut total_weight = 0_i64;
        let shanten =
            self.bot_min_shanten(concealed_counts_before_draw, meld_tile_key_groups.len());
        let strongest_threat = self
            .strongest_threat_opponent(context)
            .map(|(_, score)| score)
            .unwrap_or(0);
        let draw_limit = self.budget.expectimax_draw_limit(is_budget_critical_state(
            context,
            shanten,
            strongest_threat,
        ));
        for (draw_tile_index, remaining) in prioritized_draw_candidates(
            concealed_counts_before_draw,
            &analysis.visible_counts,
            draw_limit,
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
        let (state_key, _) = self.state_analysis(
            context,
            concealed_counts_before_discard,
            meld_tile_key_groups,
            appended_open_flags,
        );
        let key = BestDiscardKey {
            counts: *concealed_counts_before_discard,
            melds: state_key.melds.clone(),
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
        let key = WinningKey {
            counts: *concealed_counts,
            melds: compact_melds(meld_tile_key_groups, &meld_open_flags),
            draw_tile_index: draw_tile_index as u8,
        };
        if let Some(score) = self.winning_fan_cache.get(&key).cloned() {
            return score;
        }

        if !draw_completes_hand_precheck(
            concealed_counts,
            draw_tile_index,
            meld_tile_key_groups.len(),
        ) {
            self.winning_fan_cache.insert(key, None);
            return None;
        }

        let (state_key, analysis) = self.state_analysis(
            context,
            concealed_counts,
            meld_tile_key_groups,
            appended_open_flags,
        );
        debug_assert_eq!(state_key.melds, key.melds);

        let incoming_tile = tile_key_for_index(draw_tile_index);
        let concealed_tile_keys = analysis.concealed_tile_keys.clone();
        let mut effective_concealed = concealed_tile_keys.clone();
        effective_concealed.push(incoming_tile.to_string());
        let decompositions =
            scoring_decompose_winning_hand_with_melds(&effective_concealed, meld_tile_key_groups);
        if decompositions.is_empty() {
            self.winning_fan_cache.insert(key, None);
            return None;
        }

        let features = scoring_extract_hand_features(
            &concealed_tile_keys,
            meld_tile_key_groups,
            Some(&analysis.meld_open_flags),
            Some(incoming_tile),
            Some(&seat_wind_key(context.seat_index, context.dealer_seat)),
            context.round_wind.as_deref(),
            Some(&decompositions),
        );
        let result = scoring_evaluate_fans(ScoringEvaluationInput {
            win_type: "self_draw".to_string(),
            winner_seat: Some(context.seat_index),
            discarder_seat: None,
            ready_hand_declared: false,
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
            open_meld_tile_key_groups: analysis.open_meld_tile_key_groups.clone(),
            incoming_tile: Some(incoming_tile.to_string()),
            decompositions,
        });
        let score =
            (result.minimum_qualifying_fan_total >= BOT_MINIMUM_HU_FAN).then_some(result.fan_total);
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

    fn discard_danger_penalty(
        &mut self,
        context: &BotContext,
        concealed_counts: &TileCounts,
        discard_tile_index: usize,
    ) -> i64 {
        let key = DiscardStateKey {
            counts: *concealed_counts,
            tile_index: discard_tile_index as u8,
        };
        if let Some(score) = self.discard_danger_cache.get(&key).copied() {
            return score;
        }

        let known_count = |index: usize| {
            i64::from(self.base_visible_counts[index]) + i64::from(concealed_counts[index])
        };
        let unseen_copies = i64::from((4 - self.base_visible_counts[discard_tile_index]).max(0));
        let round_progress = context
            .opponent_discards_by_seat
            .iter()
            .map(|items| items.len() as i64)
            .sum::<i64>();
        let late_factor = 1 + round_progress / 14;
        let mut penalty = unseen_copies * 10 * late_factor;

        let absolute_visible = known_count(discard_tile_index);
        if absolute_visible >= 4 {
            self.discard_danger_cache.insert(key, 0);
            return 0;
        }
        if absolute_visible == 3 {
            penalty -= 80;
        }

        let tile_key = tile_key_for_index(discard_tile_index);
        for (seat, discards) in context.opponent_discards_by_seat.iter().enumerate() {
            if seat == context.seat_index {
                continue;
            }
            let threat = self.threat_profiles.get(seat).copied().unwrap_or_default();
            penalty += weighted_threat_score(
                threat.pressure + threat.tenpai_likelihood,
                Some(seat) == self.strongest_threat_seat,
            ) + seat_discard_danger_score(
                &threat,
                discard_tile_index,
                tile_key,
                discards,
                &known_count,
            );
        }

        if concealed_counts[discard_tile_index] >= 2 {
            penalty -= 18;
        }
        let result = penalty.max(0);
        self.discard_danger_cache.insert(key, result);
        result
    }

    fn discard_tile_preference_score(
        &mut self,
        concealed_counts: &TileCounts,
        tile_index: usize,
        prefer_drawn_copy: bool,
    ) -> i64 {
        let key = DiscardStateKey {
            counts: *concealed_counts,
            tile_index: tile_index as u8,
        };
        let base = if let Some(score) = self.discard_preference_cache.get(&key).copied() {
            score
        } else {
            let visible = i64::from(self.base_visible_counts[tile_index]);
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
            self.discard_preference_cache.insert(key, score);
            score
        };
        base + i64::from(prefer_drawn_copy) * 12
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

fn tile_keys_from_counts(counts: &TileCounts) -> Vec<String> {
    let mut tile_keys = Vec::new();
    for (tile_index, count) in counts.iter().enumerate() {
        for _ in 0..usize::from(*count) {
            tile_keys.push(tile_key_for_index(tile_index).to_string());
        }
    }
    tile_keys
}

pub(crate) fn meld_open_flags_for_state(
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
        flags.push(
            appended_open_flags
                .get(appended_index)
                .copied()
                .unwrap_or(true),
        );
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

fn visible_tile_counts(tile_keys: &[String]) -> [i32; TILE_KIND_COUNT] {
    let mut counts = [0_i32; TILE_KIND_COUNT];
    for tile_key in tile_keys {
        if let Some(tile_index) = tile_index(tile_key) {
            counts[tile_index] += 1;
        }
    }
    counts
}

fn visible_tile_counts_for_state(
    context: &BotContext,
    meld_tile_key_groups: &[Vec<String>],
) -> [i32; TILE_KIND_COUNT] {
    let mut counts = known_visible_tile_counts(context);

    for meld in &context.player.meld_tile_key_groups {
        apply_visible_meld_delta(&mut counts, meld, -1);
    }
    for meld in meld_tile_key_groups {
        apply_visible_meld_delta(&mut counts, meld, 1);
    }
    counts
}

fn known_visible_tile_counts(context: &BotContext) -> [i32; TILE_KIND_COUNT] {
    let mut known_tile_keys = Vec::with_capacity(context.visible_tile_keys.len());
    known_tile_keys.extend(context.visible_tile_keys.iter().cloned());
    visible_tile_counts(&known_tile_keys)
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
        let remaining =
            estimated_remaining_tile_count(visible_counts, concealed_counts, tile_index);
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

fn hidden_tile_pool(
    concealed_counts: &TileCounts,
    visible_counts: &[i32; TILE_KIND_COUNT],
) -> Vec<usize> {
    let mut pool = Vec::new();
    for tile_index in 0..TILE_KIND_COUNT {
        let remaining =
            estimated_remaining_tile_count(visible_counts, concealed_counts, tile_index);
        for _ in 0..remaining {
            pool.push(tile_index);
        }
    }
    pool
}

fn remove_random_hidden_tile(pool: &mut Vec<usize>, rng: &mut StdRng) -> Option<usize> {
    if pool.is_empty() {
        return None;
    }
    let index = rng.random_range(0..pool.len());
    Some(pool.swap_remove(index))
}

fn burn_hidden_tiles(pool: &mut Vec<usize>, count: usize, rng: &mut StdRng) {
    let burn_count = count.min(pool.len());
    for _ in 0..burn_count {
        let _ = remove_random_hidden_tile(pool, rng);
    }
}

fn monte_carlo_sample_count(context: &BotContext, hidden_tile_count: usize) -> usize {
    let base = if context.wall_tiles_remaining <= 12 {
        MONTE_CARLO_SAMPLE_COUNT_LATE
    } else if context.wall_tiles_remaining <= 28 {
        MONTE_CARLO_SAMPLE_COUNT_MID
    } else {
        MONTE_CARLO_SAMPLE_COUNT_EARLY
    };
    base.min(hidden_tile_count.max(1))
}

fn monte_carlo_horizon(context: &BotContext) -> usize {
    if context.wall_tiles_remaining <= 12 {
        MONTE_CARLO_HORIZON_LATE
    } else if context.wall_tiles_remaining <= 28 {
        MONTE_CARLO_HORIZON_MID
    } else {
        MONTE_CARLO_HORIZON_EARLY
    }
}

fn monte_carlo_safety_weight_from_threats(
    context: &BotContext,
    threat_profiles: &[OpponentThreat],
) -> i64 {
    let mut total_threat = 0_i64;
    let mut max_threat = 0_i64;

    for (seat, threat) in threat_profiles.iter().enumerate() {
        if seat == context.seat_index {
            continue;
        }
        let seat_threat = threat.pressure + threat.tenpai_likelihood;
        total_threat += seat_threat;
        max_threat = max_threat.max(seat_threat);
    }

    let late_bonus = if context.wall_tiles_remaining <= 12 {
        55
    } else if context.wall_tiles_remaining <= 24 {
        30
    } else {
        0
    };
    (95 + total_threat / 8 + max_threat / 5 + late_bonus).clamp(85, 235)
}

fn monte_carlo_seed(
    context: &BotContext,
    concealed_counts: &TileCounts,
    meld_tile_key_groups: &[Vec<String>],
    appended_open_flags: &[bool],
    discard_tile_index: usize,
) -> u64 {
    let meld_open_flags =
        meld_open_flags_for_state(context, meld_tile_key_groups, appended_open_flags);
    let compact = compact_melds(meld_tile_key_groups, &meld_open_flags);
    let mut hasher = DefaultHasher::new();
    context.seat_index.hash(&mut hasher);
    context.wall_tiles_remaining.hash(&mut hasher);
    context.dealer_seat.hash(&mut hasher);
    context.round_wind.hash(&mut hasher);
    concealed_counts.hash(&mut hasher);
    compact.hash(&mut hasher);
    discard_tile_index.hash(&mut hasher);
    hasher.finish()
}

fn monte_carlo_mix(value: u64) -> u64 {
    value.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(17)
}

#[cfg(test)]
fn discard_danger_penalty(
    context: &BotContext,
    concealed_counts: &TileCounts,
    tile_key: &str,
) -> i64 {
    let Some(discard_index) = tile_index(tile_key) else {
        return 0;
    };
    SearchEngine::new(context).discard_danger_penalty(context, concealed_counts, discard_index)
}

fn suji_safety_bonus(tile_key: &str, discards: &[String]) -> i32 {
    let Some(tile_index) = tile_index(tile_key) else {
        return 0;
    };
    if tile_index >= HONOR_TILE_START {
        return 0;
    }
    let rank = (tile_index % 9) + 1;
    let mut bonus = 0;
    if rank >= 4 {
        let lower_suji = tile_key_for_index(tile_index - 3);
        if discards.iter().any(|key| key == lower_suji) {
            bonus += 34;
        }
    }
    if rank <= 6 {
        let upper_suji = tile_key_for_index(tile_index + 3);
        if discards.iter().any(|key| key == upper_suji) {
            bonus += 34;
        }
    }
    bonus
}

fn kabe_safety_bonus<F>(tile_index: usize, known_count: &F) -> i32
where
    F: Fn(usize) -> i64,
{
    if tile_index >= HONOR_TILE_START {
        return 0;
    }
    let rank = tile_index % 9;
    let mut bonus = 0;
    let neighbor_indices = [
        rank.checked_sub(1).map(|_| tile_index - 1),
        (rank <= 7).then_some(tile_index + 1),
        rank.checked_sub(2).map(|_| tile_index - 2),
        (rank <= 6).then_some(tile_index + 2),
    ];
    for (pos, maybe_index) in neighbor_indices.into_iter().enumerate() {
        let Some(index) = maybe_index else {
            continue;
        };
        let known = known_count(index);
        if known >= 4 {
            bonus += if pos < 2 { 26 } else { 14 };
        } else if known == 3 {
            bonus += if pos < 2 { 10 } else { 6 };
        }
    }
    bonus
}

fn opponent_threat_profile(context: &BotContext, seat: usize) -> OpponentThreat {
    let Some(melds) = context.opponent_melds_by_seat.get(seat) else {
        return OpponentThreat::default();
    };
    let discards = context
        .opponent_discards_by_seat
        .get(seat)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let meld_count = melds.len() as i64;
    let tenpai_likelihood = infer_tenpai_likelihood(context, &melds, &discards);
    if meld_count == 0 && tenpai_likelihood == 0 {
        return OpponentThreat::default();
    }
    let mut threat = OpponentThreat {
        pressure: meld_count * 26
            + if meld_count == 0 {
                match tenpai_likelihood {
                    90.. => 18,
                    70.. => 8,
                    _ => 0,
                }
            } else {
                0
            },
        tenpai_likelihood,
        high_tenpai_probability: tenpai_likelihood >= 90,
        hand_value: 120,
        ..Default::default()
    };
    if meld_count == 0
        && tenpai_likelihood >= 70
        && let Some(flush_suit) = infer_concealed_flush_suit(context, discards)
    {
        threat.flush_suit = Some(flush_suit);
        threat.pressure += 24;
    }
    let mut suit_counts = [0_i64; 3];
    let mut honor_count = 0_i64;
    let mut dragon_melds = 0_i64;

    for meld in melds {
        let mut meld_has_honor = false;
        let mut same_tile = true;
        let first = meld.first().cloned().unwrap_or_default();
        for tile_key in meld {
            if tile_key != &first {
                same_tile = false;
            }
            if let Some(index) = tile_index(tile_key) {
                if index >= HONOR_TILE_START {
                    meld_has_honor = true;
                    honor_count += 1;
                    if index >= 31 {
                        dragon_melds += i64::from(same_tile);
                    }
                } else {
                    suit_counts[index / 9] += 1;
                }
            }
        }
        if meld_has_honor {
            threat.pressure += 18;
        }
        if same_tile && meld.len() >= 3 {
            threat.pressure += 14;
        }
    }

    let dominant_suit = suit_counts
        .iter()
        .enumerate()
        .max_by_key(|(_, count)| **count)
        .map(|(index, count)| (index, *count))
        .unwrap_or((0, 0));
    let off_suit_discards = discards
        .iter()
        .filter_map(|tile_key| tile_index(tile_key))
        .filter(|index| *index < HONOR_TILE_START && *index / 9 != dominant_suit.0)
        .count() as i64;
    if dominant_suit.1 >= 6 && off_suit_discards >= 4 {
        threat.flush_suit = Some(dominant_suit.0);
        threat.pressure += 36;
    }
    if honor_count >= 3 {
        threat.honor_focus = true;
        threat.pressure += 22;
    }
    if dragon_melds > 0 {
        threat.dragon_focus = true;
        threat.pressure += dragon_melds * 26;
    }
    let recent_discards = discards.iter().rev().take(8).collect::<Vec<_>>();
    let terminal_honor_recent = recent_discards
        .iter()
        .filter(|tile_key| {
            tile_index(tile_key)
                .is_some_and(|index| index >= HONOR_TILE_START || matches!(index % 9, 0 | 8))
        })
        .count() as i64;
    let middle_recent = recent_discards
        .iter()
        .filter(|tile_key| {
            tile_index(tile_key)
                .is_some_and(|index| index < HONOR_TILE_START && (3..=5).contains(&(index % 9)))
        })
        .count() as i64;
    threat.central_wait_bias = terminal_honor_recent >= 4 && middle_recent <= 1;
    threat.edge_wait_bias = middle_recent >= 3 && terminal_honor_recent == 0;
    if threat.central_wait_bias {
        threat.pressure += 12;
    }
    if threat.edge_wait_bias {
        threat.pressure += 8;
    }
    threat.hand_value =
        estimate_threat_hand_value(&threat, meld_count, dominant_suit.1, dragon_melds);
    threat
}

fn estimate_threat_hand_value(
    threat: &OpponentThreat,
    meld_count: i64,
    dominant_suit_tiles: i64,
    dragon_melds: i64,
) -> i64 {
    let mut value = 120_i64 + meld_count * 24 + threat.tenpai_likelihood / 3;
    if threat.flush_suit.is_some() && dominant_suit_tiles >= 6 {
        value += 110;
    }
    if threat.honor_focus {
        value += 40;
    }
    if threat.dragon_focus {
        value += dragon_melds * 60;
    }
    if threat.high_tenpai_probability {
        value += 26;
    }
    value.clamp(80, 360)
}

fn infer_tenpai_likelihood(
    context: &BotContext,
    melds: &[Vec<String>],
    discards: &[String],
) -> i64 {
    let meld_count = melds.len() as i64;
    let late_round = context.wall_tiles_remaining > 0 && context.wall_tiles_remaining <= 20;
    let very_late_round = context.wall_tiles_remaining > 0 && context.wall_tiles_remaining <= 12;
    let recent_discards = discards.iter().rev().take(6).collect::<Vec<_>>();
    let honor_or_terminal_recent = recent_discards
        .iter()
        .filter(|tile_key| {
            tile_index(tile_key)
                .is_some_and(|index| index >= HONOR_TILE_START || matches!(index % 9, 0 | 8))
        })
        .count() as i64;
    let mut duplicate_counts = [0_u8; TILE_KIND_COUNT];
    let mut seen_suits = [false; 3];
    for tile_key in &recent_discards {
        if let Some(index) = tile_index(tile_key) {
            duplicate_counts[index] = duplicate_counts[index].saturating_add(1);
            if index < HONOR_TILE_START {
                seen_suits[index / 9] = true;
            }
        }
    }
    let duplicate_recent = duplicate_counts.iter().filter(|count| **count >= 2).count() as i64;
    let suit_span = seen_suits.into_iter().filter(|seen| *seen).count() as i64;
    let stagnation_signal =
        honor_or_terminal_recent * 8 + duplicate_recent * 16 + i64::from(suit_span <= 1) * 12;

    let mut likelihood = meld_count * 26 + stagnation_signal;
    if late_round {
        likelihood += 20;
    }
    if very_late_round {
        likelihood += 16;
    }
    if meld_count >= 3 {
        likelihood += 18;
    }
    if discards.len() >= 9 {
        likelihood += 10;
    }
    if meld_count == 0 {
        if discards.len() >= 10 {
            likelihood += 12;
        }
        if discards.len() >= 12 {
            likelihood += 10;
        }
        if honor_or_terminal_recent >= 4 {
            likelihood += 10;
        }
        if duplicate_recent >= 2 {
            likelihood += 10;
        }
    }
    likelihood.clamp(0, 140)
}

fn infer_concealed_flush_suit(context: &BotContext, discards: &[String]) -> Option<usize> {
    if !(context.wall_tiles_remaining > 0 && context.wall_tiles_remaining <= 16) {
        return None;
    }

    let mut suit_discards = [0_i64; 3];
    let mut honor_discards = 0_i64;
    for tile_key in discards {
        let Some(index) = tile_index(tile_key) else {
            continue;
        };
        if index >= HONOR_TILE_START {
            honor_discards += 1;
        } else {
            suit_discards[index / 9] += 1;
        }
    }

    let total_suit_discards = suit_discards.iter().sum::<i64>();
    let (candidate_suit, candidate_count) = suit_discards
        .iter()
        .enumerate()
        .min_by_key(|(_, count)| **count)?;
    let off_suit_discards = total_suit_discards - candidate_count;

    (total_suit_discards >= 6
        && off_suit_discards >= 6
        && *candidate_count <= 1
        && honor_discards >= 1)
        .then_some(candidate_suit)
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

pub(crate) fn simulated_tiles_after_removal(
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

pub(crate) fn claim_meld_tile_keys(
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
            dfs(counts, tile_index, melds, taatsu, 1, open_meld_count, best);
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

            if tile_index < HONOR_TILE_START && tile_index % 9 <= 7 && counts[tile_index + 1] > 0 {
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

            if tile_index < HONOR_TILE_START && tile_index % 9 <= 6 && counts[tile_index + 2] > 0 {
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
    dfs(&mut working, 0, 0, 0, 0, open_meld_count as i32, &mut best);
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

fn draw_completes_hand_precheck(
    concealed_counts: &TileCounts,
    draw_tile_index: usize,
    open_meld_count: usize,
) -> bool {
    let mut counts_after_draw = *concealed_counts;
    counts_after_draw[draw_tile_index] = counts_after_draw[draw_tile_index].saturating_add(1);
    standard_shanten_with_open_melds(&counts_after_draw, open_meld_count)
        .min(seven_pairs_shanten(&counts_after_draw, open_meld_count))
        .min(thirteen_orphans_shanten(
            &counts_after_draw,
            open_meld_count,
        ))
        <= -1
}

fn best_shanten_after_draw(
    engine: &mut SearchEngine,
    concealed_counts: &TileCounts,
    draw_tile_index: usize,
    open_meld_count: usize,
    restricted_discard_tile_index: Option<usize>,
) -> i32 {
    let key = ShantenAfterDrawKey {
        counts: *concealed_counts,
        draw_tile_index: draw_tile_index as u8,
        open_meld_count: open_meld_count as u8,
        restricted_discard_tile_index: restricted_discard_tile_index.map(|index| index as u8),
    };
    if let Some(best_shanten) = engine.shanten_after_draw_cache.get(&key).copied() {
        return best_shanten;
    }

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
        best_shanten =
            best_shanten.min(engine.bot_min_shanten(&counts_after_draw, open_meld_count));
        counts_after_draw[discard_tile_index] += 1;
    }
    engine.shanten_after_draw_cache.insert(key, best_shanten);
    best_shanten
}

fn weighted_threat_score(threat_score: i64, is_primary: bool) -> i64 {
    if threat_score <= 0 {
        return 0;
    }
    if is_primary {
        threat_score + 18
    } else {
        threat_score * 3 / 5
    }
}

fn seat_discard_danger_score<F>(
    threat: &OpponentThreat,
    discard_tile_index: usize,
    tile_key: &str,
    discards: &[String],
    known_count: &F,
) -> i64
where
    F: Fn(usize) -> i64,
{
    let same_tile_discards = discards
        .iter()
        .filter(|key| key.as_str() == tile_key)
        .count() as i64;
    if same_tile_discards > 0 {
        return -150 * same_tile_discards;
    }

    let mut penalty = 0_i64;
    let mut seat_recent = discards.iter().rev().take(6);
    if discard_tile_index >= HONOR_TILE_START {
        penalty += 36;
        if threat.honor_focus {
            penalty += 42;
        }
        if threat.dragon_focus && discard_tile_index >= 31 {
            penalty += 56;
        }
        if threat.high_tenpai_probability {
            penalty += 34;
        }
        return penalty;
    }

    let rank = (discard_tile_index % 9) + 1;
    let suit_index = discard_tile_index / 9;
    if rank == 1 || rank == 9 {
        penalty += 22;
    }
    if threat.flush_suit == Some(suit_index) {
        penalty += 52;
    }
    if threat.high_tenpai_probability {
        penalty += if rank == 1 || rank == 9 { 18 } else { 28 };
    }
    penalty += wait_shape_risk(threat, rank);
    penalty -= suji_safety_bonus(tile_key, discards) as i64;
    penalty -= kabe_safety_bonus(discard_tile_index, known_count) as i64;

    let suit = tile_key.as_bytes()[0];
    let same_suit_recent = seat_recent
        .clone()
        .filter(|key| key.as_bytes().first().copied() == Some(suit))
        .count() as i64;
    if same_suit_recent == 0 {
        penalty += 28;
    } else {
        penalty -= 8 * same_suit_recent.min(2);
    }

    let adjacent_recent = seat_recent.any(|key| {
        let Some(other_index) = tile_index(key) else {
            return false;
        };
        other_index < HONOR_TILE_START
            && other_index / 9 == discard_tile_index / 9
            && other_index.abs_diff(discard_tile_index) <= 2
    });
    if adjacent_recent {
        penalty -= 18;
    } else {
        penalty += 20;
    }
    penalty
}

fn wait_shape_risk(threat: &OpponentThreat, rank: usize) -> i64 {
    let mut penalty = 0_i64;
    if threat.central_wait_bias {
        penalty += match rank {
            4..=6 => 32,
            3 | 7 => 18,
            2 | 8 => 4,
            _ => -10,
        };
    }
    if threat.edge_wait_bias {
        penalty += match rank {
            3 | 7 => 18,
            2 | 8 => 10,
            4..=6 => -12,
            _ => 0,
        };
    }
    penalty
}

pub(crate) fn strategic_signals(
    context: &BotContext,
    concealed_counts: &TileCounts,
    meld_tile_key_groups: &[Vec<String>],
    meld_open_flags: &[bool],
) -> StrategicSignals {
    let open_meld_count = meld_open_flags.iter().filter(|is_open| **is_open).count();
    let mut full_counts = *concealed_counts;
    let mut open_chow_count = 0_i64;

    for (meld_index, meld) in meld_tile_key_groups.iter().enumerate() {
        let same_tile = meld
            .first()
            .is_some_and(|first| meld.iter().all(|tile| tile == first));
        if meld_open_flags.get(meld_index).copied().unwrap_or(true) && meld.len() == 3 && !same_tile
        {
            open_chow_count += 1;
        }
        for tile_key in meld {
            if let Some(tile_index) = tile_index(tile_key) {
                full_counts[tile_index] = full_counts[tile_index].saturating_add(1);
            }
        }
    }

    let mut suit_counts = [0_i64; 3];
    let mut honor_count = 0_i64;
    for (tile_index, count) in full_counts.iter().enumerate() {
        let count = i64::from(*count);
        if tile_index >= HONOR_TILE_START {
            honor_count += count;
        } else {
            suit_counts[tile_index / 9] += count;
        }
    }

    let (dominant_suit, dominant_suit_count) = suit_counts
        .iter()
        .enumerate()
        .max_by_key(|(_, count)| **count)
        .map(|(index, count)| (index, *count))
        .unwrap_or((0, 0));
    let off_suit_count = suit_counts
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != dominant_suit)
        .map(|(_, count)| *count)
        .sum::<i64>();

    let concealed_pair_count = concealed_counts
        .iter()
        .map(|count| match *count {
            2 => 1_i64,
            4 => 2_i64,
            _ => 0_i64,
        })
        .sum::<i64>();
    let concealed_singleton_count =
        concealed_counts.iter().filter(|count| **count == 1).count() as i64;
    let pair_count = full_counts.iter().filter(|count| **count >= 2).count() as i64;
    let triplet_count = full_counts.iter().filter(|count| **count >= 3).count() as i64;
    let quad_count = full_counts.iter().filter(|count| **count >= 4).count() as i64;
    let sequence_density = sequence_density_score(concealed_counts, meld_tile_key_groups);
    let (value_honor_pair_count, value_honor_triplet_fan, value_honor_route_bonus) =
        value_honor_progress(context, &full_counts);
    let dominant_suit_remaining =
        remaining_tiles_for_suit(context, &full_counts, dominant_suit) as i64;
    let pure_one_suit_conversion_need = off_suit_count + honor_count;
    let mixed_one_suit_conversion_need = off_suit_count;
    let pure_one_suit_supply_penalty =
        route_supply_penalty(dominant_suit_remaining, pure_one_suit_conversion_need);
    let mixed_one_suit_supply_penalty =
        route_supply_penalty(dominant_suit_remaining, mixed_one_suit_conversion_need);

    let pure_one_suit_route = if dominant_suit_count > 0 {
        dominant_suit_count * 28
            - off_suit_count * 64
            - honor_count * 30
            - pure_one_suit_supply_penalty
    } else {
        0
    };
    let mixed_one_suit_route = if dominant_suit_count > 0 && honor_count > 0 {
        dominant_suit_count * 24 + honor_count * 16
            - off_suit_count * 66
            - mixed_one_suit_supply_penalty
    } else {
        dominant_suit_count * 14 - off_suit_count * 50 - mixed_one_suit_supply_penalty
    };
    let seven_pairs_route = if open_meld_count == 0 {
        concealed_pair_count * 92 - concealed_singleton_count * 12 + quad_count * 18
            - triplet_count * 72
            - sequence_density * 10
    } else {
        -260
    };
    let thirteen_orphans_progress = thirteen_orphans_progress(&full_counts, open_meld_count);
    let terminal_honor_progress =
        terminal_honor_progress(&full_counts, sequence_density, open_chow_count);
    let all_pungs_route = triplet_count * 82 + pair_count * 18 + value_honor_pair_count * 20
        - sequence_density * 12
        - open_chow_count * 78;

    let pure_one_suit_fan_estimate =
        estimate_pure_one_suit_fan(dominant_suit_count, off_suit_count, honor_count);
    let mixed_one_suit_fan_estimate =
        estimate_mixed_one_suit_fan(dominant_suit_count, off_suit_count, honor_count);
    let pure_straight_progress = pure_straight_progress(&full_counts);
    let mixed_triple_chow_progress = mixed_triple_chow_progress(&full_counts);
    let seven_pairs_fan_estimate = if open_meld_count == 0 {
        match concealed_pair_count {
            6.. => 24,
            5 => 16,
            4 => 10,
            _ => 0,
        }
    } else {
        0
    };
    let all_pungs_fan_estimate = match triplet_count {
        4.. => 6,
        3 => 4,
        2 => 2,
        _ => 0,
    } + value_honor_triplet_fan;
    let pure_all_pungs_fan_estimate =
        composite_fan_estimate(pure_one_suit_fan_estimate, all_pungs_fan_estimate);
    let mixed_all_pungs_fan_estimate =
        composite_fan_estimate(mixed_one_suit_fan_estimate, all_pungs_fan_estimate);
    let pure_seven_pairs_fan_estimate =
        composite_fan_estimate(pure_one_suit_fan_estimate, seven_pairs_fan_estimate);
    let mixed_seven_pairs_fan_estimate =
        composite_fan_estimate(mixed_one_suit_fan_estimate, seven_pairs_fan_estimate);
    let pure_straight_mixed_one_suit_fan_estimate = composite_fan_estimate(
        mixed_one_suit_fan_estimate,
        pure_straight_progress.fan_estimate,
    );
    let pure_straight_pure_one_suit_fan_estimate = composite_fan_estimate(
        pure_one_suit_fan_estimate,
        pure_straight_progress.fan_estimate,
    );
    let fan_estimate = pure_one_suit_fan_estimate
        .max(mixed_one_suit_fan_estimate)
        .max(pure_straight_progress.fan_estimate)
        .max(mixed_triple_chow_progress.fan_estimate)
        .max(seven_pairs_fan_estimate)
        .max(thirteen_orphans_progress.fan_estimate)
        .max(terminal_honor_progress.fan_estimate)
        .max(all_pungs_fan_estimate)
        .max(pure_all_pungs_fan_estimate)
        .max(mixed_all_pungs_fan_estimate)
        .max(pure_seven_pairs_fan_estimate)
        .max(mixed_seven_pairs_fan_estimate)
        .max(pure_straight_mixed_one_suit_fan_estimate)
        .max(pure_straight_pure_one_suit_fan_estimate);

    let mut route_score = pure_one_suit_route
        .max(mixed_one_suit_route)
        .max(seven_pairs_route)
        .max(thirteen_orphans_progress.route_bonus)
        .max(terminal_honor_progress.route_bonus)
        .max(all_pungs_route)
        + value_honor_route_bonus;
    route_score += pure_straight_progress.route_bonus + mixed_triple_chow_progress.route_bonus;

    route_score += if fan_estimate >= 8 {
        fan_estimate * 18 + 80
    } else {
        fan_estimate * 8
    };

    StrategicSignals {
        route_score,
        fan_estimate,
        closed_route_score: seven_pairs_route.max(thirteen_orphans_progress.route_bonus),
        sequence_route_score: pure_straight_progress.route_bonus
            + mixed_triple_chow_progress.route_bonus
            + pure_one_suit_route.max(mixed_one_suit_route) / 3,
        triplet_route_score: all_pungs_route.max(terminal_honor_progress.route_bonus)
            + value_honor_route_bonus,
    }
}

fn estimate_pure_one_suit_fan(
    dominant_suit_count: i64,
    off_suit_count: i64,
    honor_count: i64,
) -> i64 {
    if honor_count > 0 {
        return 0;
    }
    match (dominant_suit_count, off_suit_count) {
        (10.., 0) => 24,
        (9.., 1) => 16,
        (9.., 2) => 10,
        (8.., 1) => 10,
        (8.., 2) => 8,
        _ => 0,
    }
}

fn estimate_mixed_one_suit_fan(
    dominant_suit_count: i64,
    off_suit_count: i64,
    honor_count: i64,
) -> i64 {
    if honor_count == 0 {
        return 0;
    }
    match (dominant_suit_count, off_suit_count) {
        (10.., 0) => 6,
        (9.., 1) => 6,
        (8.., 1) => 5,
        (8.., 2) => 3,
        _ => 0,
    }
}

fn pure_straight_progress(full_counts: &TileCounts) -> RoutePatternProgress {
    let mut best = RoutePatternProgress::default();

    for suit_index in 0..3 {
        let mut complete_segments = 0_i64;
        let mut near_segments = 0_i64;
        let mut present_tiles = 0_i64;

        for start in [0, 3, 6] {
            let support = sequence_support(full_counts, suit_index, start);
            present_tiles += i64::from(support.present_tiles);
            if support.complete {
                complete_segments += 1;
            } else if support.present_tiles >= 2 {
                near_segments += 1;
            }
        }

        let progress = RoutePatternProgress {
            fan_estimate: i64::from(complete_segments == 3) * 2,
            route_bonus: complete_segments * 42 + near_segments * 18 + present_tiles * 4,
        };
        if progress.fan_estimate > best.fan_estimate || progress.route_bonus > best.route_bonus {
            best = progress;
        }
    }

    best
}

fn mixed_triple_chow_progress(full_counts: &TileCounts) -> RoutePatternProgress {
    let mut best = RoutePatternProgress::default();

    for start in 0..=6 {
        let mut complete_suits = 0_i64;
        let mut near_suits = 0_i64;
        let mut present_tiles = 0_i64;

        for suit_index in 0..3 {
            let support = sequence_support(full_counts, suit_index, start);
            present_tiles += i64::from(support.present_tiles);
            if support.complete {
                complete_suits += 1;
            } else if support.present_tiles >= 2 {
                near_suits += 1;
            }
        }

        let progress = RoutePatternProgress {
            fan_estimate: i64::from(complete_suits == 3) * 8,
            route_bonus: complete_suits * 54 + near_suits * 22 + present_tiles * 4,
        };
        if progress.fan_estimate > best.fan_estimate || progress.route_bonus > best.route_bonus {
            best = progress;
        }
    }

    best
}

fn thirteen_orphans_progress(
    full_counts: &TileCounts,
    open_meld_count: usize,
) -> RoutePatternProgress {
    if open_meld_count > 0 {
        return RoutePatternProgress::default();
    }

    let unique_orphans = ORPHAN_INDICES
        .iter()
        .filter(|index| full_counts[**index] > 0)
        .count() as i64;
    let orphan_pair = ORPHAN_INDICES.iter().any(|index| full_counts[*index] >= 2) as i64;
    let non_orphan_tiles = full_counts
        .iter()
        .enumerate()
        .filter(|(index, _)| !ORPHAN_INDICES.contains(index))
        .map(|(_, count)| i64::from(*count))
        .sum::<i64>();

    RoutePatternProgress {
        fan_estimate: match (unique_orphans, orphan_pair) {
            (13, 1..) => 88,
            (12.., 1..) => 64,
            (11.., _) => 32,
            (10, _) => 16,
            _ => 0,
        },
        route_bonus: unique_orphans * 118 + orphan_pair * 46 - non_orphan_tiles * 90,
    }
}

fn terminal_honor_progress(
    full_counts: &TileCounts,
    sequence_density: i64,
    open_chow_count: i64,
) -> RoutePatternProgress {
    let mut terminal_or_honor_tiles = 0_i64;
    let mut middle_tiles = 0_i64;
    let mut terminal_or_honor_triplets = 0_i64;
    let mut honor_tiles = 0_i64;
    let mut terminal_or_honor_pairs = 0_i64;

    for (tile_index, count) in full_counts.iter().enumerate() {
        let count = i64::from(*count);
        if count == 0 {
            continue;
        }
        let is_terminal_or_honor =
            tile_index >= HONOR_TILE_START || matches!(tile_index % 9, 0 | 8);
        if is_terminal_or_honor {
            terminal_or_honor_tiles += count;
            if count >= 2 {
                terminal_or_honor_pairs += 1;
            }
            if count >= 3 {
                terminal_or_honor_triplets += 1;
            }
            if tile_index >= HONOR_TILE_START {
                honor_tiles += count;
            }
        } else {
            middle_tiles += count;
        }
    }

    RoutePatternProgress {
        fan_estimate: if middle_tiles == 0 {
            match (terminal_or_honor_triplets, honor_tiles > 0) {
                (4.., false) => 64,
                (4.., true) => 32,
                (3, false) => 24,
                (3, true) => 16,
                (2, true) => 8,
                _ => 0,
            }
        } else if middle_tiles <= 2 && terminal_or_honor_triplets >= 3 {
            8
        } else {
            0
        },
        route_bonus: terminal_or_honor_tiles * 34
            + terminal_or_honor_triplets * 88
            + terminal_or_honor_pairs * 24
            - middle_tiles * 96
            - sequence_density * 14
            - open_chow_count * 86,
    }
}

fn composite_fan_estimate(primary: i64, secondary: i64) -> i64 {
    if primary > 0 && secondary > 0 {
        primary + secondary
    } else {
        0
    }
}

fn route_supply_penalty(remaining_tiles: i64, conversion_need: i64) -> i64 {
    if conversion_need <= 0 {
        return 0;
    }
    let shortage = (conversion_need * 2 - remaining_tiles).max(0);
    shortage * 28
}

fn remaining_tiles_for_suit(
    context: &BotContext,
    full_counts: &TileCounts,
    suit_index: usize,
) -> i32 {
    let visible_counts = known_visible_tile_counts(context);
    let suit_start = suit_index * 9;
    let mut remaining = 0_i32;
    for tile_index in suit_start..(suit_start + 9) {
        remaining += (4 - visible_counts[tile_index] - i32::from(full_counts[tile_index])).max(0);
    }
    remaining
}

fn value_honor_progress(context: &BotContext, counts: &TileCounts) -> (i64, i64, i64) {
    let seat_wind = seat_wind_key(context.seat_index, context.dealer_seat);
    let round_wind = context.round_wind.as_deref();
    let mut value_honor_pair_count = 0_i64;
    let mut value_honor_triplet_fan = 0_i64;
    let mut value_honor_route_bonus = 0_i64;

    for tile_key in ["east", "south", "west", "north", "red", "green", "white"] {
        let Some(tile_index) = tile_index(tile_key) else {
            continue;
        };
        let count = counts[tile_index];
        if count < 2 {
            continue;
        }

        let per_set_fan = if tile_key == seat_wind { 2 } else { 0 }
            + if Some(tile_key) == round_wind { 2 } else { 0 }
            + if tile_index >= 31 { 2 } else { 0 };

        if per_set_fan > 0 {
            value_honor_pair_count += 1;
            value_honor_route_bonus += 26 + i64::from(count) * 8;
            if count >= 3 {
                value_honor_triplet_fan += per_set_fan;
                value_honor_route_bonus += 42 + i64::from(per_set_fan) * 18;
            }
        }
    }

    (
        value_honor_pair_count,
        value_honor_triplet_fan,
        value_honor_route_bonus,
    )
}

fn sequence_density_score(
    concealed_counts: &TileCounts,
    meld_tile_key_groups: &[Vec<String>],
) -> i64 {
    let mut density = 0_i64;

    for suit_start in [0, 9, 18] {
        for offset in 0..8 {
            density += i64::from(
                concealed_counts[suit_start + offset]
                    .min(concealed_counts[suit_start + offset + 1]),
            );
        }
        for offset in 0..7 {
            density += i64::from(
                concealed_counts[suit_start + offset]
                    .min(concealed_counts[suit_start + offset + 2]),
            );
        }
    }

    for meld in meld_tile_key_groups {
        if meld.len() == 3
            && meld
                .first()
                .is_some_and(|first| meld.iter().any(|tile| tile != first))
        {
            density += 3;
        }
    }

    density
}

#[derive(Clone, Copy, Default)]
struct SequenceSupport {
    present_tiles: u8,
    complete: bool,
}

fn sequence_support(full_counts: &TileCounts, suit_index: usize, start: usize) -> SequenceSupport {
    let base_index = suit_index * 9 + start;
    let present_tiles = u8::from(full_counts[base_index] > 0)
        + u8::from(full_counts[base_index + 1] > 0)
        + u8::from(full_counts[base_index + 2] > 0);
    SequenceSupport {
        present_tiles,
        complete: full_counts[base_index] > 0
            && full_counts[base_index + 1] > 0
            && full_counts[base_index + 2] > 0,
    }
}

pub(crate) fn claim_action_bonus(
    context: &BotContext,
    action_type: &str,
    claim_meld: &[String],
    previous_signals: StrategicSignals,
    next_signals: StrategicSignals,
) -> i64 {
    let mut bonus = match action_type {
        "pung" => 24,
        "chow" => -40,
        _ => 0,
    };

    if meld_is_value_honor_set(context, claim_meld) {
        bonus += 120;
    } else if meld_is_terminal_or_honor_set(claim_meld) && action_type == "pung" {
        bonus += 26;
    }

    let route_gain = next_signals.route_score - previous_signals.route_score;
    bonus += route_gain / 4;

    if action_type == "chow" && meld_has_single_suit(claim_meld) && next_signals.fan_estimate >= 6 {
        bonus += 56;
    }

    if next_signals.fan_estimate >= 8 && previous_signals.fan_estimate < 8 {
        bonus += 96;
    }
    if previous_signals.closed_route_score > next_signals.closed_route_score {
        let closed_route_loss =
            previous_signals.closed_route_score - next_signals.closed_route_score;
        bonus -= closed_route_loss / 2;
    }
    if action_type == "chow" {
        bonus += (next_signals.sequence_route_score - previous_signals.sequence_route_score) / 3;
        if previous_signals.triplet_route_score > next_signals.triplet_route_score + 100 {
            bonus -= 48;
        }
    }
    if action_type == "pung" {
        bonus += (next_signals.triplet_route_score - previous_signals.triplet_route_score) / 3;
        if previous_signals.sequence_route_score > next_signals.sequence_route_score + 100 {
            bonus -= 52;
        }
    }
    if previous_signals.fan_estimate > next_signals.fan_estimate {
        bonus -= (previous_signals.fan_estimate - next_signals.fan_estimate) * 32;
    }

    bonus
}

pub(crate) fn meld_is_value_honor_set(context: &BotContext, claim_meld: &[String]) -> bool {
    if claim_meld.len() < 3 {
        return false;
    }
    let Some(tile_key) = claim_meld.first() else {
        return false;
    };
    if !claim_meld.iter().all(|tile| tile == tile_key) {
        return false;
    }
    let seat_wind = seat_wind_key(context.seat_index, context.dealer_seat);
    Some(tile_key.as_str()) == context.round_wind.as_deref()
        || tile_key == &seat_wind
        || matches!(tile_key.as_str(), "red" | "green" | "white")
}

fn meld_is_terminal_or_honor_set(claim_meld: &[String]) -> bool {
    let Some(tile_key) = claim_meld.first() else {
        return false;
    };
    if !claim_meld.iter().all(|tile| tile == tile_key) {
        return false;
    }
    tile_index(tile_key)
        .is_some_and(|index| index >= HONOR_TILE_START || matches!(index % 9, 0 | 8))
}

fn meld_has_single_suit(claim_meld: &[String]) -> bool {
    let mut suit = None;
    for tile_key in claim_meld {
        let Some(tile_index) = tile_index(tile_key) else {
            return false;
        };
        if tile_index >= HONOR_TILE_START {
            return false;
        }
        let current_suit = tile_index / 9;
        if let Some(existing) = suit {
            if existing != current_suit {
                return false;
            }
        } else {
            suit = Some(current_suit);
        }
    }
    suit.is_some()
}

fn tile_is_isolated(counts: &TileCounts, tile_index: usize) -> bool {
    if tile_index >= HONOR_TILE_START {
        return counts[tile_index] == 1;
    }
    let rank = tile_index % 9;
    let left_two = if rank >= 2 {
        Some(tile_index - 2)
    } else {
        None
    };
    let left_one = if rank >= 1 {
        Some(tile_index - 1)
    } else {
        None
    };
    let right_one = if rank <= 7 {
        Some(tile_index + 1)
    } else {
        None
    };
    let right_two = if rank <= 6 {
        Some(tile_index + 2)
    } else {
        None
    };
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

#[cfg(test)]
mod tests {
    use super::*;

    fn base_context() -> BotContext {
        BotContext {
            seat_index: 0,
            seat_count: 4,
            dealer_seat: 0,
            round_wind: Some("east".to_string()),
            cumulative_scores: vec![0, 0, 0, 0],
            wall_tiles_remaining: 40,
            visible_tile_keys: Vec::new(),
            opponent_discards_by_seat: vec![vec![], vec![], vec![], vec![]],
            opponent_melds_by_seat: vec![vec![], vec![], vec![], vec![]],
            kong_entries: Vec::new(),
            player: BotPlayerContext {
                concealed_tiles: vec![],
                concealed_tile_counts: [0; TILE_KIND_COUNT],
                meld_tile_key_groups: Vec::new(),
                flower_count: 0,
            },
            restricted_discard_tile_key: None,
            drawn_tile_id: None,
            self_kong_candidates: Vec::new(),
            claim_options: Vec::new(),
            last_discard_tile_key: None,
            add_kong_risk_tiles: HashSet::new(),
        }
    }

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

    #[test]
    fn selects_leading_conservative_mode() {
        let mut context = base_context();
        context.cumulative_scores = vec![48, 0, -16, -32];
        assert_eq!(select_bot_mode(&context), BotMode::LeadingConservative);
    }

    #[test]
    fn selects_trailing_aggressive_mode() {
        let mut context = base_context();
        context.cumulative_scores = vec![-24, 24, 8, -8];
        assert_eq!(select_bot_mode(&context), BotMode::TrailingAggressive);
    }

    #[test]
    fn selects_late_defense_before_other_modes() {
        let mut context = base_context();
        context.cumulative_scores = vec![-40, 20, 10, 10];
        context.wall_tiles_remaining = 12;
        assert_eq!(select_bot_mode(&context), BotMode::LateDefense);
    }

    #[test]
    fn late_defense_penalizes_danger_more_than_balanced() {
        let mut context = base_context();
        context.opponent_discards_by_seat = vec![
            vec![],
            vec!["w9".to_string(), "w9".to_string()],
            vec!["b1".to_string()],
            vec!["t9".to_string()],
        ];
        let mut counts = [0_u8; TILE_KIND_COUNT];
        counts[tile_index("w5").expect("tile index")] = 1;
        let balanced_penalty = discard_danger_penalty(&context, &counts, "w5");

        context.wall_tiles_remaining = 10;
        let late_penalty = discard_danger_penalty(&context, &counts, "w5")
            * mode_profile(select_bot_mode(&context)).danger_weight
            / 100;

        assert!(late_penalty > balanced_penalty);
    }

    #[test]
    fn exposed_dragon_meld_raises_honor_danger() {
        let mut context = base_context();
        context.opponent_melds_by_seat[1] = vec![vec![
            "red".to_string(),
            "red".to_string(),
            "red".to_string(),
        ]];
        let counts = [0_u8; TILE_KIND_COUNT];
        let dragon_penalty = discard_danger_penalty(&context, &counts, "green");
        let suit_penalty = discard_danger_penalty(&context, &counts, "w5");
        assert!(dragon_penalty > suit_penalty);
    }

    #[test]
    fn flush_like_open_melds_raise_same_suit_danger() {
        let mut context = base_context();
        context.opponent_melds_by_seat[2] = vec![
            vec!["w3".to_string(), "w4".to_string(), "w5".to_string()],
            vec!["w7".to_string(), "w8".to_string(), "w9".to_string()],
        ];
        context.opponent_discards_by_seat[2] = vec![
            "b1".to_string(),
            "b4".to_string(),
            "t2".to_string(),
            "t8".to_string(),
            "b9".to_string(),
        ];
        let counts = [0_u8; TILE_KIND_COUNT];
        let same_suit_penalty = discard_danger_penalty(&context, &counts, "w6");
        let off_suit_penalty = discard_danger_penalty(&context, &counts, "b6");
        assert!(same_suit_penalty > off_suit_penalty);
    }

    #[test]
    fn genbutsu_is_safer_than_non_genbutsu() {
        let mut context = base_context();
        context.opponent_discards_by_seat[1] = vec!["w5".to_string()];
        let counts = [0_u8; TILE_KIND_COUNT];
        let genbutsu_penalty = discard_danger_penalty(&context, &counts, "w5");
        let other_penalty = discard_danger_penalty(&context, &counts, "w4");
        assert!(genbutsu_penalty < other_penalty);
    }

    #[test]
    fn suji_discards_reduce_danger_without_riichi_specific_assumptions() {
        let mut context = base_context();
        context.opponent_discards_by_seat[1] = vec!["w1".to_string(), "w7".to_string()];
        let counts = [0_u8; TILE_KIND_COUNT];
        let suji_penalty = discard_danger_penalty(&context, &counts, "w4");
        let non_suji_penalty = discard_danger_penalty(&context, &counts, "w5");
        assert!(suji_penalty < non_suji_penalty);
    }

    #[test]
    fn absolute_kabe_and_zetsu_reduce_danger() {
        let context = base_context();
        let mut counts = [0_u8; TILE_KIND_COUNT];
        counts[tile_index("w5").expect("tile index")] = 1;
        counts[tile_index("w4").expect("tile index")] = 3;
        let kabe_penalty = discard_danger_penalty(&context, &counts, "w5");

        let mut zetsu_counts = [0_u8; TILE_KIND_COUNT];
        zetsu_counts[tile_index("w5").expect("tile index")] = 4;
        let zetsu_penalty = discard_danger_penalty(&context, &zetsu_counts, "w5");

        assert!(zetsu_penalty <= kabe_penalty);
    }

    #[test]
    fn multiple_open_melds_and_late_round_mark_high_tenpai_probability() {
        let mut context = base_context();
        context.wall_tiles_remaining = 10;
        context.opponent_melds_by_seat[1] = vec![
            vec!["w3".to_string(), "w4".to_string(), "w5".to_string()],
            vec!["t7".to_string(), "t8".to_string(), "t9".to_string()],
            vec!["red".to_string(), "red".to_string(), "red".to_string()],
        ];
        context.opponent_discards_by_seat[1] = vec![
            "white".to_string(),
            "north".to_string(),
            "w1".to_string(),
            "w1".to_string(),
            "b9".to_string(),
            "green".to_string(),
        ];
        let threat = opponent_threat_profile(&context, 1);
        assert!(threat.high_tenpai_probability);
        assert!(threat.tenpai_likelihood >= 90);
    }

    #[test]
    fn late_closed_hand_can_still_register_tenpai_threat() {
        let mut context = base_context();
        context.wall_tiles_remaining = 8;
        context.opponent_discards_by_seat[1] = vec![
            "east".to_string(),
            "south".to_string(),
            "west".to_string(),
            "north".to_string(),
            "white".to_string(),
            "green".to_string(),
            "red".to_string(),
            "b9".to_string(),
            "b9".to_string(),
            "t9".to_string(),
            "t9".to_string(),
            "w1".to_string(),
        ];

        let threat = opponent_threat_profile(&context, 1);
        assert!(threat.tenpai_likelihood >= 90);
        assert!(threat.high_tenpai_probability);
    }

    #[test]
    fn closed_hand_off_suit_discards_can_hint_flush_pressure() {
        let mut context = base_context();
        context.wall_tiles_remaining = 9;
        context.opponent_discards_by_seat[1] = vec![
            "b1".to_string(),
            "b2".to_string(),
            "b4".to_string(),
            "b7".to_string(),
            "t1".to_string(),
            "t3".to_string(),
            "t5".to_string(),
            "t8".to_string(),
            "east".to_string(),
            "red".to_string(),
        ];

        let threat = opponent_threat_profile(&context, 1);
        assert_eq!(threat.flush_suit, Some(0));
    }

    #[test]
    fn explicit_tenpai_inference_raises_overall_discard_danger() {
        let counts = [0_u8; TILE_KIND_COUNT];

        let mut quiet = base_context();
        quiet.opponent_melds_by_seat[1] =
            vec![vec!["w3".to_string(), "w4".to_string(), "w5".to_string()]];
        quiet.opponent_discards_by_seat[1] = vec!["w1".to_string(), "t1".to_string()];
        let quiet_penalty = discard_danger_penalty(&quiet, &counts, "w6");

        let mut loud = base_context();
        loud.wall_tiles_remaining = 9;
        loud.opponent_melds_by_seat[1] = vec![
            vec!["w3".to_string(), "w4".to_string(), "w5".to_string()],
            vec!["t7".to_string(), "t8".to_string(), "t9".to_string()],
        ];
        loud.opponent_discards_by_seat[1] = vec![
            "east".to_string(),
            "east".to_string(),
            "white".to_string(),
            "b9".to_string(),
            "north".to_string(),
            "north".to_string(),
        ];
        let loud_penalty = discard_danger_penalty(&loud, &counts, "w6");

        assert!(loud_penalty > quiet_penalty);
    }

    #[test]
    fn cached_strategic_signals_match_direct_evaluation() {
        let mut context = base_context();
        context.player.meld_tile_key_groups = vec![vec![
            "east".to_string(),
            "east".to_string(),
            "east".to_string(),
        ]];
        let concealed_tiles = tiles(&[
            "w2", "w3", "w4", "w5", "w6", "w7", "t2", "t3", "t4", "red", "red",
        ]);
        context.player.concealed_tile_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));
        context.player.concealed_tiles = concealed_tiles;

        let meld_open_flags =
            meld_open_flags_for_state(&context, &context.player.meld_tile_key_groups, &[]);
        let direct = strategic_signals(
            &context,
            &context.player.concealed_tile_counts,
            &context.player.meld_tile_key_groups,
            &meld_open_flags,
        );

        let mut engine = SearchEngine::new(&context);
        let cached = engine.strategic_signals_for_state(
            &context,
            &context.player.concealed_tile_counts,
            &context.player.meld_tile_key_groups,
            &[],
        );

        assert_eq!(cached.route_score, direct.route_score);
        assert_eq!(cached.fan_estimate, direct.fan_estimate);
    }

    #[test]
    fn cached_state_analysis_matches_direct_derivations() {
        let mut context = base_context();
        context.visible_tile_keys = vec!["w9".to_string(), "red".to_string()];
        context.player.meld_tile_key_groups = vec![
            vec!["east".to_string(), "east".to_string(), "east".to_string()],
            vec![
                "w5".to_string(),
                "w5".to_string(),
                "w5".to_string(),
                "w5".to_string(),
            ],
        ];
        let concealed_tiles = tiles(&["w2", "w3", "w4", "t2", "t3", "t4", "red", "red"]);
        context.player.concealed_tile_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));
        context.player.concealed_tiles = concealed_tiles;

        let mut engine = SearchEngine::new(&context);
        let (_, analysis) = engine.state_analysis(
            &context,
            &context.player.concealed_tile_counts,
            &context.player.meld_tile_key_groups,
            &[],
        );

        assert_eq!(
            analysis.meld_open_flags,
            meld_open_flags_for_state(&context, &context.player.meld_tile_key_groups, &[])
        );
        assert_eq!(
            analysis.visible_counts,
            visible_tile_counts_for_state(&context, &context.player.meld_tile_key_groups)
        );
        assert_eq!(
            analysis.concealed_tile_keys,
            tile_keys_from_counts(&context.player.concealed_tile_counts)
        );
        assert_eq!(analysis.open_meld_tile_key_groups.len(), 2);
    }

    #[test]
    fn composite_routes_can_exceed_single_route_fan_ceiling() {
        let context = base_context();
        let concealed_tiles = tiles(&[
            "w1", "w1", "w1", "w2", "w2", "w2", "w3", "w3", "w3", "w4", "w4", "w4", "w5", "w5",
        ]);
        let concealed_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));

        let signals = strategic_signals(&context, &concealed_counts, &[], &[]);
        assert!(signals.fan_estimate > 24);
    }

    #[test]
    fn mixed_triple_chow_route_reaches_eight_fan_estimate() {
        let context = base_context();
        let concealed_tiles = tiles(&[
            "w1", "w2", "w3", "t1", "t2", "t3", "b1", "b2", "b3", "east", "east", "red", "red",
            "w5",
        ]);
        let concealed_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));

        let signals = strategic_signals(&context, &concealed_counts, &[], &[]);
        assert!(signals.fan_estimate >= 8);
    }

    #[test]
    fn pure_straight_can_complete_mixed_one_suit_route_to_eight_fan() {
        let context = base_context();
        let concealed_tiles = tiles(&[
            "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "east", "east", "red", "red",
            "w5",
        ]);
        let concealed_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));

        let signals = strategic_signals(&context, &concealed_counts, &[], &[]);
        assert!(signals.fan_estimate >= 8);
    }

    #[test]
    fn triplets_do_not_count_as_free_seven_pairs_progress() {
        let context = base_context();
        let concealed_tiles = tiles(&[
            "w1", "w1", "w1", "t2", "t2", "t2", "b3", "b3", "east", "east", "red", "red", "white",
        ]);
        let concealed_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));

        let signals = strategic_signals(&context, &concealed_counts, &[], &[]);
        assert!(signals.fan_estimate < 16);
    }

    #[test]
    fn dead_dominant_suit_tiles_reduce_route_confidence() {
        let live_context = base_context();
        let concealed_tiles = tiles(&[
            "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "w9", "b1", "b2", "b3",
        ]);
        let concealed_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));

        let live_signals = strategic_signals(&live_context, &concealed_counts, &[], &[]);

        let mut dead_context = base_context();
        dead_context.visible_tile_keys = vec![
            "w1".to_string(),
            "w1".to_string(),
            "w1".to_string(),
            "w2".to_string(),
            "w2".to_string(),
            "w2".to_string(),
            "w3".to_string(),
            "w3".to_string(),
            "w3".to_string(),
            "w4".to_string(),
            "w4".to_string(),
            "w4".to_string(),
            "w5".to_string(),
            "w5".to_string(),
            "w5".to_string(),
            "w6".to_string(),
            "w6".to_string(),
            "w6".to_string(),
            "w7".to_string(),
            "w7".to_string(),
            "w7".to_string(),
            "w8".to_string(),
            "w8".to_string(),
            "w8".to_string(),
            "w9".to_string(),
            "w9".to_string(),
        ];
        let dead_signals = strategic_signals(&dead_context, &concealed_counts, &[], &[]);

        assert!(dead_signals.route_score < live_signals.route_score);
    }

    #[test]
    fn monte_carlo_budget_skips_when_finalists_are_already_separated() {
        let mut context = base_context();
        context.wall_tiles_remaining = 12;
        let concealed_tiles = tiles(&[
            "w1", "w2", "w3", "w4", "w5", "w6", "t2", "t3", "t4", "b2", "b3", "red", "green",
        ]);
        context.player.concealed_tile_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));
        context.player.concealed_tiles = concealed_tiles;

        let mut engine = SearchEngine::new(&context);
        assert!(engine.should_run_monte_carlo(
            &context,
            &context.player.concealed_tile_counts,
            &context.player.meld_tile_key_groups,
            40,
        ));
        assert!(!engine.should_run_monte_carlo(
            &context,
            &context.player.concealed_tile_counts,
            &context.player.meld_tile_key_groups,
            120,
        ));
    }

    #[test]
    fn self_draw_precheck_accepts_standard_hand_completion() {
        let concealed_counts = tile_counts34(
            [
                "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "t2", "t3", "t4", "red",
            ]
            .into_iter(),
        );

        assert!(draw_completes_hand_precheck(
            &concealed_counts,
            tile_index("red").expect("tile index"),
            0,
        ));
    }

    #[test]
    fn self_draw_precheck_rejects_non_winning_draw() {
        let concealed_counts = tile_counts34(
            [
                "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "t1", "t2", "t3", "red", "green",
            ]
            .into_iter(),
        );

        assert!(!draw_completes_hand_precheck(
            &concealed_counts,
            tile_index("white").expect("tile index"),
            0,
        ));
    }

    #[test]
    fn precheck_failure_is_cached_in_winning_fan_cache() {
        let context = base_context();
        let concealed_counts = tile_counts34(
            [
                "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "t1", "t2", "t3", "red", "green",
            ]
            .into_iter(),
        );
        let mut engine = SearchEngine::new(&context);
        let draw_tile_index = tile_index("white").expect("tile index");
        let key = WinningKey {
            counts: concealed_counts,
            melds: Vec::new(),
            draw_tile_index: draw_tile_index as u8,
        };

        assert_eq!(
            engine.hypothetical_self_draw_fan_total(
                &context,
                &concealed_counts,
                &[],
                &[],
                draw_tile_index
            ),
            None
        );
        assert!(engine.winning_fan_cache.contains_key(&key));
        assert_eq!(engine.winning_fan_cache.get(&key), Some(&None));
    }

    #[test]
    fn low_fan_self_draw_is_not_scored_as_bot_win() {
        let context = base_context();
        let concealed_counts = tile_counts34(
            [
                "w1", "w2", "w3", "t4", "t5", "t6", "b3", "b4", "b5", "w6", "w7", "w8", "red",
            ]
            .into_iter(),
        );
        let mut engine = SearchEngine::new(&context);

        let fan_total = engine.hypothetical_self_draw_fan_total(
            &context,
            &concealed_counts,
            &[],
            &[],
            tile_index("red").expect("tile index"),
        );

        assert_eq!(fan_total, None);
    }

    #[test]
    fn flower_tiles_do_not_satisfy_bot_minimum_self_draw_fan() {
        let mut context = base_context();
        context.player.flower_count = 7;
        let concealed_counts = tile_counts34(
            [
                "w1", "w2", "w3", "t4", "t5", "t6", "b3", "b4", "b5", "w6", "w7", "w8", "red",
            ]
            .into_iter(),
        );
        let mut engine = SearchEngine::new(&context);

        let fan_total = engine.hypothetical_self_draw_fan_total(
            &context,
            &concealed_counts,
            &[],
            &[],
            tile_index("red").expect("tile index"),
        );

        assert_eq!(fan_total, None);
    }

    #[test]
    fn records_discard_telemetry_for_close_decision() {
        let mut context = base_context();
        context.wall_tiles_remaining = 12;
        let concealed_tiles = tiles(&[
            "w1", "w2", "w3", "w4", "w5", "w6", "t2", "t3", "t4", "b2", "b3", "red", "green",
        ]);
        context.player.concealed_tile_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));
        context.player.concealed_tiles = concealed_tiles.clone();

        let mut engine = SearchEngine::new(&context);
        let _ = engine.best_discard_plan(
            &context,
            &concealed_tiles,
            &context.player.concealed_tile_counts,
            &context.player.meld_tile_key_groups,
            &[],
            None,
            None,
        );

        let telemetry = engine.last_discard_telemetry().expect("telemetry");
        assert!(telemetry.stage_one_candidates >= 2);
        assert!(telemetry.ran_stage_two);
        assert!(telemetry.finalist_gap.is_some());
    }

    #[test]
    fn elevates_stage_two_budget_for_close_high_pressure_spot() {
        let mut context = base_context();
        context.wall_tiles_remaining = 12;
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
        let concealed_counts = tile_counts34(
            [
                "w1", "w2", "w3", "t1", "t2", "t3", "b1", "b2", "b3", "east", "east", "green", "w9",
            ]
            .into_iter(),
        );
        let mut engine = SearchEngine::new(&context);

        assert_eq!(
            engine.stage_two_candidate_count(&context, &concealed_counts, &[], 20),
            STAGE_TWO_CANDIDATES_PRESSURE
        );
    }

    #[test]
    fn normal_budget_keeps_current_search_limits() {
        let budget = BotSearchBudget::for_strength(BotStrength::Normal);

        assert_eq!(
            budget.stage_two_candidate_limit(false),
            STAGE_TWO_CANDIDATES
        );
        assert_eq!(
            budget.stage_two_candidate_limit(true),
            STAGE_TWO_CANDIDATES_PRESSURE
        );
        assert_eq!(budget.expectimax_draw_limit(false), EXPECTIMAX_DRAW_LIMIT);
        assert_eq!(budget.expectimax_draw_limit(true), EXPECTIMAX_DRAW_LIMIT);
        assert_eq!(
            budget.monte_carlo_sample_limit(MONTE_CARLO_SAMPLE_COUNT_MID, true),
            MONTE_CARLO_SAMPLE_COUNT_MID
        );
    }

    #[test]
    fn strong_budget_spends_more_only_in_critical_spots() {
        let budget = BotSearchBudget::for_strength(BotStrength::Strong);

        assert_eq!(
            budget.stage_two_candidate_limit(false),
            STAGE_TWO_CANDIDATES
        );
        assert_eq!(budget.stage_two_candidate_limit(true), 5);
        assert_eq!(budget.expectimax_draw_limit(false), EXPECTIMAX_DRAW_LIMIT);
        assert_eq!(budget.expectimax_draw_limit(true), 16);
        assert_eq!(
            budget.monte_carlo_sample_limit(MONTE_CARLO_SAMPLE_COUNT_MID, false),
            MONTE_CARLO_SAMPLE_COUNT_MID
        );
        assert_eq!(
            budget.monte_carlo_sample_limit(MONTE_CARLO_SAMPLE_COUNT_MID, true),
            12
        );
    }

    #[test]
    fn caches_best_shanten_after_draw_results() {
        let context = base_context();
        let concealed_counts = tile_counts34(
            [
                "w1", "w2", "w3", "w4", "w5", "w6", "t2", "t3", "t4", "b7", "b8", "east", "east",
            ]
            .into_iter(),
        );
        let draw_tile_index = tile_index("b9").expect("tile index");
        let mut engine = SearchEngine::new(&context);
        let key = ShantenAfterDrawKey {
            counts: concealed_counts,
            draw_tile_index: draw_tile_index as u8,
            open_meld_count: 0,
            restricted_discard_tile_index: None,
        };

        let best =
            best_shanten_after_draw(&mut engine, &concealed_counts, draw_tile_index, 0, None);
        assert_eq!(engine.shanten_after_draw_cache.get(&key), Some(&best));
    }

    #[test]
    fn primary_threat_gets_more_weight_than_secondary_threat() {
        assert!(weighted_threat_score(120, true) > weighted_threat_score(120, false));
    }

    #[test]
    fn passes_on_value_honor_pung_when_seven_pairs_route_is_mature() {
        let mut context = base_context();
        let concealed_tiles = tiles(&[
            "red", "red", "east", "east", "green", "green", "white", "white", "w1", "w1", "t9",
            "t9", "b5",
        ]);
        context.player.concealed_tile_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));
        context.player.concealed_tiles = concealed_tiles.clone();
        context.claim_options = vec![BotClaimOption {
            action_type: "pung".to_string(),
            tile_ids: vec![
                concealed_tiles[0].tile_id.clone(),
                concealed_tiles[1].tile_id.clone(),
            ],
        }];
        context.last_discard_tile_key = Some("red".to_string());

        let action = crate::bot::choose_claim_action(&context).expect("claim action");
        assert_eq!(action.action_type, "pass");
    }

    #[test]
    fn takes_chow_when_it_improves_the_best_route() {
        let mut context = base_context();
        let concealed_tiles = tiles(&[
            "w2", "w3", "t2", "t3", "t4", "b3", "b4", "b5", "w7", "w8", "red", "green", "white",
        ]);
        context.player.concealed_tile_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));
        context.player.concealed_tiles = concealed_tiles.clone();
        context.claim_options = vec![BotClaimOption {
            action_type: "chow".to_string(),
            tile_ids: vec![
                concealed_tiles[0].tile_id.clone(),
                concealed_tiles[1].tile_id.clone(),
            ],
        }];
        context.last_discard_tile_key = Some("w1".to_string());

        let action = crate::bot::choose_claim_action(&context).expect("claim action");
        assert_eq!(action.action_type, "chow");
    }

    #[test]
    fn takes_value_honor_pung_when_it_improves_the_hand() {
        let mut context = base_context();
        let concealed_tiles = tiles(&[
            "red", "red", "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w8", "east", "east",
        ]);
        context.player.concealed_tile_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));
        context.player.concealed_tiles = concealed_tiles.clone();
        context.claim_options = vec![BotClaimOption {
            action_type: "pung".to_string(),
            tile_ids: vec![
                concealed_tiles[0].tile_id.clone(),
                concealed_tiles[1].tile_id.clone(),
            ],
        }];
        context.last_discard_tile_key = Some("red".to_string());

        let action = crate::bot::choose_claim_action(&context).expect("claim action");
        assert_eq!(action.action_type, "pung");
    }

    #[test]
    fn thirteen_orphans_route_gets_high_fan_estimate_near_completion() {
        let context = base_context();
        let concealed_tiles = tiles(&[
            "w1", "w9", "t1", "t9", "b1", "b9", "east", "south", "west", "north", "red", "green",
            "white", "white",
        ]);
        let concealed_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));

        let signals = strategic_signals(&context, &concealed_counts, &[], &[]);
        assert!(signals.fan_estimate >= 32);
    }

    #[test]
    fn terminal_triplet_route_exceeds_plain_all_pungs_value() {
        let context = base_context();
        let concealed_tiles = tiles(&[
            "w1", "w1", "w1", "w9", "w9", "w9", "t1", "t1", "t1", "t9", "t9", "t9", "east", "east",
        ]);
        let concealed_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));

        let signals = strategic_signals(&context, &concealed_counts, &[], &[]);
        assert!(signals.fan_estimate >= 16);
    }

    #[test]
    fn central_wait_bias_makes_middle_tiles_more_dangerous_than_terminals() {
        let mut context = base_context();
        context.wall_tiles_remaining = 10;
        context.opponent_discards_by_seat[1] = vec![
            "east".to_string(),
            "south".to_string(),
            "west".to_string(),
            "north".to_string(),
            "red".to_string(),
            "green".to_string(),
            "white".to_string(),
            "w1".to_string(),
            "w9".to_string(),
            "b1".to_string(),
            "b9".to_string(),
        ];
        let counts = [0_u8; TILE_KIND_COUNT];

        let middle_penalty = discard_danger_penalty(&context, &counts, "w5");
        let terminal_penalty = discard_danger_penalty(&context, &counts, "w1");
        assert!(middle_penalty > terminal_penalty);
    }

    #[test]
    fn expensive_open_hand_increases_explicit_deal_in_cost() {
        let counts = [0_u8; TILE_KIND_COUNT];
        let discard_tile_index = tile_index("w5").expect("tile index");

        let mut cheap = base_context();
        cheap.opponent_melds_by_seat[1] = vec![
            vec!["w3".to_string(), "w4".to_string(), "w5".to_string()],
            vec!["t3".to_string(), "t4".to_string(), "t5".to_string()],
        ];
        cheap.opponent_discards_by_seat[1] = vec![
            "east".to_string(),
            "north".to_string(),
            "b9".to_string(),
            "white".to_string(),
        ];

        let mut expensive = cheap.clone();
        expensive.opponent_melds_by_seat[1] = vec![
            vec!["red".to_string(), "red".to_string(), "red".to_string()],
            vec!["w3".to_string(), "w4".to_string(), "w5".to_string()],
            vec!["w7".to_string(), "w8".to_string(), "w9".to_string()],
        ];

        let cheap_cost =
            SearchEngine::new(&cheap).deal_in_ev_cost(&cheap, &counts, discard_tile_index);
        let expensive_cost =
            SearchEngine::new(&expensive).deal_in_ev_cost(&expensive, &counts, discard_tile_index);
        assert!(expensive_cost > cheap_cost);
    }

    #[test]
    fn near_top_gap_reduces_claim_margin_below_balanced_profile() {
        let balanced = profile_for_context(&base_context());

        let mut context = base_context();
        context.wall_tiles_remaining = 24;
        context.seat_index = 1;
        context.cumulative_scores = vec![30, 26, 8, -64];

        let pressured = profile_for_context(&context);
        assert!(pressured.claim_margin < balanced.claim_margin);
        assert!(pressured.winning_weight > balanced.winning_weight);
    }

    #[test]
    fn protecting_position_raises_danger_weight_without_late_round_mode() {
        let balanced = profile_for_context(&base_context());

        let mut context = base_context();
        context.wall_tiles_remaining = 26;
        context.seat_index = 2;
        context.cumulative_scores = vec![28, 12, -4, -6];

        let pressured = profile_for_context(&context);
        assert!(pressured.danger_weight > balanced.danger_weight);
    }

    #[test]
    fn monte_carlo_rollout_prefers_safer_honor_discard_under_threat() {
        let mut context = base_context();
        context.wall_tiles_remaining = 16;
        context.opponent_melds_by_seat[1] = vec![
            vec!["red".to_string(), "red".to_string(), "red".to_string()],
            vec!["w3".to_string(), "w4".to_string(), "w5".to_string()],
        ];
        context.opponent_discards_by_seat[1] = vec![
            "white".to_string(),
            "w9".to_string(),
            "b9".to_string(),
            "north".to_string(),
        ];

        let concealed_tiles = tiles(&[
            "w1", "w2", "w3", "t1", "t2", "t3", "b1", "b2", "b3", "east", "east", "white", "green",
            "w9",
        ]);
        context.player.concealed_tile_counts =
            tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));
        context.player.concealed_tiles = concealed_tiles.clone();

        let white_tile = concealed_tiles
            .iter()
            .find(|tile| tile.tile_key == "white")
            .cloned()
            .expect("white tile");
        let green_tile = concealed_tiles
            .iter()
            .find(|tile| tile.tile_key == "green")
            .cloned()
            .expect("green tile");

        let white_score = SearchEngine::new(&context)
            .monte_carlo_discard_score(
                &context,
                &context.player.concealed_tile_counts,
                &context.player.meld_tile_key_groups,
                &[],
                None,
                &white_tile,
            )
            .expect("white score");
        let green_score = SearchEngine::new(&context)
            .monte_carlo_discard_score(
                &context,
                &context.player.concealed_tile_counts,
                &context.player.meld_tile_key_groups,
                &[],
                None,
                &green_tile,
            )
            .expect("green score");

        assert!(white_score > green_score);
    }
}
