use super::fan_table::StandardFanTable;

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::sync::{OnceLock, RwLock};

const SUIT_KEYS: [char; 3] = ['w', 't', 'b'];
const HONOR_KEYS: [&str; 7] = ["east", "south", "west", "north", "red", "green", "white"];
const WIND_KEYS: [&str; 4] = ["east", "south", "west", "north"];
const DRAGON_KEYS: [&str; 3] = ["red", "green", "white"];
const ALL_GREEN_KEYS: [&str; 6] = ["t2", "t3", "t4", "t6", "t8", "green"];
const REVERSIBLE_TILE_KEYS: [&str; 14] = [
    "b2", "b4", "b5", "b6", "b8", "b9", "t1", "t2", "t3", "t4", "t5", "t8", "t9", "white",
];
const STANDARD_WIN_TILE_KEYS: [&str; 34] = [
    "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "t1", "t2", "t3", "t4", "t5", "t6", "t7",
    "t8", "t9", "b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9", "east", "south", "west",
    "north", "red", "green", "white",
];
const KNITTED_PATTERNS: [[&str; 9]; 6] = [
    ["w1", "w4", "w7", "t2", "t5", "t8", "b3", "b6", "b9"],
    ["w1", "w4", "w7", "b2", "b5", "b8", "t3", "t6", "t9"],
    ["t1", "t4", "t7", "w2", "w5", "w8", "b3", "b6", "b9"],
    ["t1", "t4", "t7", "b2", "b5", "b8", "w3", "w6", "w9"],
    ["b1", "b4", "b7", "w2", "w5", "w8", "t3", "t6", "t9"],
    ["b1", "b4", "b7", "t2", "t5", "t8", "w3", "w6", "w9"],
];
const MCR_BASE_POINTS: i64 = 8;
const TILE_KIND_COUNT: usize = 34;
const HONOR_TILE_START: usize = 27;
const THIRTEEN_ORPHAN_INDICES: [usize; 13] = [0, 8, 9, 17, 18, 26, 27, 28, 29, 30, 31, 32, 33];
const DECOMPOSITION_CACHE_LIMIT: usize = 4096;
const HAND_FEATURE_CACHE_LIMIT: usize = 4096;
const FAN_RESULT_CACHE_LIMIT: usize = 2048;

type TileCounts = [u8; TILE_KIND_COUNT];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct CompactMeld([u8; 3]);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct StandardDecompositionSignature {
    pair_index: u8,
    melds: Vec<CompactMeld>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Decomposition {
    pub kind: String,
    pub pair: Option<String>,
    pub melds: Vec<Vec<String>>,
    pub pairs: Vec<String>,
    pub pattern_tiles: Vec<String>,
    pub honor_tiles: Vec<String>,
    pub meld: Vec<String>,
    pub completion_kind: Option<String>,
    pub orphans: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct HandFeatures {
    pub concealed_hand: bool,
    pub thirteen_orphans: bool,
    pub seven_pairs: bool,
    pub pung_hand: bool,
    pub mixed_one_suit: bool,
    pub pure_one_suit: bool,
    pub duan_yao: bool,
    pub hun_yao_jiu: bool,
    pub qing_yao_jiu: bool,
    pub seat_wind_triplet: bool,
    pub round_wind_triplet: bool,
    pub dragon_triplet_count: usize,
    pub terminal_triplet_count: usize,
    pub non_seat_non_round_wind_triplet_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct TimingFeatures {
    pub gang_shang_hua: bool,
    pub hai_di_lao_yue: bool,
    pub he_di_lao_yu: bool,
    pub robbing_the_kong: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct KongEntry {
    pub kong_type: String,
    pub actor_seat: usize,
    pub payer_seats: Vec<usize>,
    pub tile_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EvaluationInput {
    pub win_type: String,
    pub winner_seat: Option<usize>,
    pub discarder_seat: Option<usize>,
    pub flower_count: usize,
    pub seat_count: usize,
    pub features: HandFeatures,
    pub timing: TimingFeatures,
    pub kong_entries: Vec<KongEntry>,
    pub tile_keys: Vec<String>,
    pub visible_tile_keys: Vec<String>,
    pub concealed_tile_keys: Vec<String>,
    pub meld_tile_key_groups: Vec<Vec<String>>,
    pub open_meld_tile_key_groups: Vec<Vec<String>>,
    pub incoming_tile: Option<String>,
    pub decompositions: Vec<Decomposition>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FanBreakdownEntry {
    pub fan_key: String,
    pub fan_value: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScoreDelta {
    pub provisional: bool,
    pub basic_points: i64,
    pub base_points: i64,
    pub fan_total: i64,
    pub minimum_qualifying_fan_total: i64,
    pub fan_delta_by_seat: Vec<i64>,
    pub kong_delta_by_seat: Vec<i64>,
    pub total_delta_by_seat: Vec<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KongScoreDetailEntry {
    pub kong_type: String,
    pub actor_seat: usize,
    pub payer_seats: Vec<usize>,
    pub delta_by_seat: Vec<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FanResult {
    pub fan_total: i64,
    pub minimum_qualifying_fan_total: i64,
    pub fan_keys: Vec<String>,
    pub fan_breakdown: Vec<FanBreakdownEntry>,
    pub score_delta: ScoreDelta,
    pub kong_score_detail: Vec<KongScoreDetailEntry>,
    pub provisional: bool,
}

impl FanResult {}

#[derive(Clone, Debug)]
struct FanContext {
    win_type: String,
    winner_seat: Option<usize>,
    discarder_seat: Option<usize>,
    flower_count: usize,
    seat_count: usize,
    features: HandFeatures,
    timing: TimingFeatures,
    kong_entries: Vec<KongEntry>,
    visible_tile_keys: Vec<String>,
    concealed_tile_keys: Vec<String>,
    open_meld_tile_key_groups: Vec<Vec<String>>,
    decompositions: Vec<Decomposition>,
    standard_decompositions: Vec<Decomposition>,
    all_tile_keys: Vec<String>,
    wait_types: Vec<String>,
    winning_tile: Option<String>,
    standard_derived: StandardDerivedData,
    all_tile_derived: AllTileDerivedData,
}

impl FanContext {
    fn from_input(input: EvaluationInput) -> Self {
        let EvaluationInput {
            win_type,
            winner_seat,
            discarder_seat,
            flower_count,
            seat_count,
            features,
            timing,
            kong_entries,
            tile_keys,
            visible_tile_keys,
            concealed_tile_keys,
            meld_tile_key_groups: _,
            open_meld_tile_key_groups,
            incoming_tile,
            decompositions: input_decompositions,
        } = input;

        let decompositions = if input_decompositions.is_empty() && !tile_keys.is_empty() {
            decompose_winning_hand(&tile_keys)
        } else {
            input_decompositions
        };
        let standard_decompositions = decompositions
            .iter()
            .filter(|decomposition| decomposition.kind == "standard")
            .cloned()
            .collect::<Vec<_>>();
        let wait_types = resolve_wait_types(
            &standard_decompositions,
            incoming_tile.as_deref(),
            &tile_keys,
        );
        let standard_derived = derive_standard_data(
            &standard_decompositions,
            &concealed_tile_keys,
            &kong_entries,
            winner_seat,
        );
        let all_tile_derived = derive_all_tile_data(&tile_keys);
        Self {
            win_type,
            winner_seat,
            discarder_seat,
            flower_count,
            seat_count: seat_count.max(1),
            features,
            timing,
            kong_entries,
            visible_tile_keys,
            concealed_tile_keys,
            open_meld_tile_key_groups,
            decompositions,
            standard_decompositions,
            all_tile_keys: tile_keys,
            wait_types,
            winning_tile: incoming_tile,
            standard_derived,
            all_tile_derived,
        }
    }

    fn with_scenario(
        &self,
        decompositions: Vec<Decomposition>,
        standard_decompositions: Vec<Decomposition>,
    ) -> Self {
        let mut next = self.clone();
        next.decompositions = decompositions;
        next.standard_decompositions = standard_decompositions;
        next.standard_derived = derive_standard_data(
            &next.standard_decompositions,
            &next.concealed_tile_keys,
            &next.kong_entries,
            next.winner_seat,
        );
        next
    }
}

type FanValueResolver = fn(&FanContext, usize, i64) -> Vec<i64>;

#[derive(Clone, Copy)]
pub(crate) struct FanRule {
    fan_key: &'static str,
    fan_value: i64,
    matcher: fn(&FanContext) -> usize,
    value_resolver: Option<FanValueResolver>,
    excludes: &'static [&'static str],
    forbidden_with: &'static [&'static str],
}

#[derive(Clone, Copy)]
struct FanCandidate {
    fan_key: &'static str,
    fan_value: i64,
    order: usize,
    excludes: &'static [&'static str],
    forbidden_with: &'static [&'static str],
}

#[derive(Clone)]
struct ScenarioResult {
    fan_keys: Vec<String>,
    fan_breakdown: Vec<FanBreakdownEntry>,
    fan_total: i64,
    minimum_qualifying_fan_total: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct FiveTileCompletion {
    pair_index: u8,
    meld: CompactMeld,
    completion_kind: &'static str,
}

#[derive(Clone, Debug, Default)]
struct StandardDerivedData {
    triplet_keys: HashSet<String>,
    pair_tile: Option<String>,
    triplet_suits_by_rank: HashMap<i32, HashSet<char>>,
    triplet_rank_counts_by_suit: HashMap<char, HashMap<i32, usize>>,
    triplet_rank_sets_by_suit: HashMap<char, HashSet<i32>>,
    sequence_suits_by_start: HashMap<i32, HashSet<char>>,
    sequence_start_counts_by_suit: HashMap<char, HashMap<i32, usize>>,
    concealed_pung_count: usize,
}

#[derive(Clone, Debug, Default)]
struct AllTileDerivedData {
    counts: Option<TileCounts>,
    suited_suits: HashSet<char>,
    has_honours: bool,
    has_wind: bool,
    has_dragon: bool,
    all_honours: bool,
    has_terminals: bool,
    all_terminal_or_honour: bool,
    all_terminals: bool,
    all_even: bool,
    all_green: bool,
    upper_four: bool,
    upper_tiles: bool,
    lower_four: bool,
    lower_tiles: bool,
    middle_tiles: bool,
    reversible_tiles: bool,
    tile_hog: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct DecompositionCacheKey {
    concealed_tile_keys: Vec<String>,
    meld_tile_key_groups: Vec<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct HandFeatureCacheKey {
    concealed_tile_keys: Vec<String>,
    meld_tile_key_groups: Vec<Vec<String>>,
    meld_open_flags: Option<Vec<bool>>,
    incoming_tile: Option<String>,
    seat_wind_key: Option<String>,
    round_wind_key: Option<String>,
    decompositions: Option<Vec<Decomposition>>,
}

fn cached_clone<K, V, F>(
    cache: &'static OnceLock<RwLock<HashMap<K, V>>>,
    limit: usize,
    key: K,
    compute: F,
) -> V
where
    K: Clone + Eq + Hash,
    V: Clone,
    F: FnOnce() -> V,
{
    let cache = cache.get_or_init(|| RwLock::new(HashMap::new()));
    if let Some(value) = cache
        .read()
        .expect("cache rwlock poisoned")
        .get(&key)
        .cloned()
    {
        return value;
    }

    let value = compute();
    let mut cache_guard = cache.write().expect("cache rwlock poisoned");
    if let Some(existing) = cache_guard.get(&key).cloned() {
        return existing;
    }
    if cache_guard.len() >= limit && !cache_guard.contains_key(&key) {
        if let Some(evicted_key) = cache_guard.keys().next().cloned() {
            cache_guard.remove(&evicted_key);
        }
    }
    cache_guard.insert(key, value.clone());
    value
}

fn decomposition_cache()
-> &'static OnceLock<RwLock<HashMap<DecompositionCacheKey, Vec<Decomposition>>>> {
    static CACHE: OnceLock<RwLock<HashMap<DecompositionCacheKey, Vec<Decomposition>>>> =
        OnceLock::new();
    &CACHE
}

fn hand_feature_cache() -> &'static OnceLock<RwLock<HashMap<HandFeatureCacheKey, HandFeatures>>> {
    static CACHE: OnceLock<RwLock<HashMap<HandFeatureCacheKey, HandFeatures>>> = OnceLock::new();
    &CACHE
}

fn fan_result_cache() -> &'static OnceLock<RwLock<HashMap<EvaluationInput, FanResult>>> {
    static CACHE: OnceLock<RwLock<HashMap<EvaluationInput, FanResult>>> = OnceLock::new();
    &CACHE
}

fn canonicalize_tile_keys(mut tile_keys: Vec<String>) -> Vec<String> {
    tile_keys.sort_unstable();
    tile_keys
}

fn canonicalize_tile_key_groups(mut groups: Vec<Vec<String>>) -> Vec<Vec<String>> {
    for group in &mut groups {
        group.sort_unstable();
    }
    groups.sort_unstable();
    groups
}

fn canonicalize_decomposition(mut decomposition: Decomposition) -> Decomposition {
    decomposition.melds = canonicalize_tile_key_groups(decomposition.melds);
    decomposition.pairs = canonicalize_tile_keys(decomposition.pairs);
    decomposition.pattern_tiles = canonicalize_tile_keys(decomposition.pattern_tiles);
    decomposition.honor_tiles = canonicalize_tile_keys(decomposition.honor_tiles);
    decomposition.meld = canonicalize_tile_keys(decomposition.meld);
    decomposition.orphans = canonicalize_tile_keys(decomposition.orphans);
    decomposition
}

fn decomposition_sort_key(
    decomposition: &Decomposition,
) -> (
    &str,
    Option<&str>,
    Vec<Vec<&str>>,
    Vec<&str>,
    Vec<&str>,
    Vec<&str>,
    Vec<&str>,
    Option<&str>,
    Vec<&str>,
) {
    (
        decomposition.kind.as_str(),
        decomposition.pair.as_deref(),
        decomposition
            .melds
            .iter()
            .map(|meld| meld.iter().map(String::as_str).collect::<Vec<_>>())
            .collect(),
        decomposition.pairs.iter().map(String::as_str).collect(),
        decomposition
            .pattern_tiles
            .iter()
            .map(String::as_str)
            .collect(),
        decomposition
            .honor_tiles
            .iter()
            .map(String::as_str)
            .collect(),
        decomposition.meld.iter().map(String::as_str).collect(),
        decomposition.completion_kind.as_deref(),
        decomposition.orphans.iter().map(String::as_str).collect(),
    )
}

fn canonicalize_decompositions(mut decompositions: Vec<Decomposition>) -> Vec<Decomposition> {
    for decomposition in &mut decompositions {
        *decomposition = canonicalize_decomposition(std::mem::take(decomposition));
    }
    decompositions
        .sort_by(|left, right| decomposition_sort_key(left).cmp(&decomposition_sort_key(right)));
    decompositions
}

fn canonicalize_evaluation_input(mut input: EvaluationInput) -> EvaluationInput {
    input.kong_entries = {
        let mut kong_entries = input.kong_entries;
        for entry in &mut kong_entries {
            entry.payer_seats.sort_unstable();
        }
        kong_entries.sort_by(|left, right| {
            (
                left.actor_seat,
                left.payer_seats.as_slice(),
                left.tile_key.as_deref(),
                left.kong_type.as_str(),
            )
                .cmp(&(
                    right.actor_seat,
                    right.payer_seats.as_slice(),
                    right.tile_key.as_deref(),
                    right.kong_type.as_str(),
                ))
        });
        kong_entries
    };
    input.tile_keys = canonicalize_tile_keys(input.tile_keys);
    input.visible_tile_keys = canonicalize_tile_keys(input.visible_tile_keys);
    input.concealed_tile_keys = canonicalize_tile_keys(input.concealed_tile_keys);
    input.meld_tile_key_groups = canonicalize_tile_key_groups(input.meld_tile_key_groups);
    input.open_meld_tile_key_groups = canonicalize_tile_key_groups(input.open_meld_tile_key_groups);
    input.decompositions = canonicalize_decompositions(input.decompositions);
    input
}

#[derive(Debug, Default, Clone, Copy)]
#[allow(dead_code)]
pub struct StandardScoreEvaluator;

impl StandardScoreEvaluator {
    #[allow(dead_code)]
    pub fn evaluate(&self, input: EvaluationInput) -> FanResult {
        evaluate_fans(input)
    }
}

pub fn evaluate_fans(input: EvaluationInput) -> FanResult {
    let cache_key = canonicalize_evaluation_input(input.clone());
    cached_clone(
        fan_result_cache(),
        FAN_RESULT_CACHE_LIMIT,
        cache_key,
        || evaluate_fans_uncached(input),
    )
}

pub fn recompute_score_delta(
    result: &mut FanResult,
    win_type: &str,
    winner_seat: Option<usize>,
    discarder_seat: Option<usize>,
    seat_count: usize,
) {
    let fan_delta_by_seat = fan_delta_by_seat(
        win_type,
        winner_seat,
        discarder_seat,
        result.fan_total,
        seat_count,
    );
    let total_delta_by_seat = fan_delta_by_seat
        .iter()
        .enumerate()
        .map(|(seat, fan_delta)| {
            fan_delta
                + result
                    .score_delta
                    .kong_delta_by_seat
                    .get(seat)
                    .copied()
                    .unwrap_or(0)
        })
        .collect::<Vec<_>>();
    result.score_delta.basic_points = result.fan_total;
    result.score_delta.fan_total = result.fan_total;
    result.score_delta.minimum_qualifying_fan_total = result.minimum_qualifying_fan_total;
    result.score_delta.fan_delta_by_seat = fan_delta_by_seat;
    result.score_delta.total_delta_by_seat = total_delta_by_seat;
}
fn evaluate_fans_uncached(input: EvaluationInput) -> FanResult {
    let context = FanContext::from_input(input);
    let best_result = fan_scenarios(&context)
        .into_iter()
        .map(|scenario| evaluate_scenario(&scenario))
        .max_by_key(|result| {
            (
                result.minimum_qualifying_fan_total,
                result.fan_total,
                result.fan_breakdown.len(),
            )
        })
        .unwrap_or_else(|| ScenarioResult {
            fan_keys: vec![],
            fan_breakdown: vec![],
            fan_total: 0,
            minimum_qualifying_fan_total: 0,
        });

    let kong_entries = normalize_kong_entries(&context.kong_entries, context.seat_count);
    let kong_delta_by_seat = sum_delta_by_seat(&kong_entries, context.seat_count);
    let fan_delta_by_seat = fan_delta_by_seat(
        &context.win_type,
        context.winner_seat,
        context.discarder_seat,
        best_result.fan_total,
        context.seat_count,
    );
    let total_delta_by_seat = fan_delta_by_seat
        .iter()
        .enumerate()
        .map(|(seat, fan_delta)| fan_delta + kong_delta_by_seat.get(seat).copied().unwrap_or(0))
        .collect::<Vec<_>>();

    FanResult {
        fan_total: best_result.fan_total,
        minimum_qualifying_fan_total: best_result.minimum_qualifying_fan_total,
        fan_keys: best_result.fan_keys,
        fan_breakdown: best_result.fan_breakdown,
        score_delta: ScoreDelta {
            provisional: true,
            basic_points: best_result.fan_total,
            base_points: MCR_BASE_POINTS,
            fan_total: best_result.fan_total,
            minimum_qualifying_fan_total: best_result.minimum_qualifying_fan_total,
            fan_delta_by_seat,
            kong_delta_by_seat,
            total_delta_by_seat,
        },
        kong_score_detail: kong_entries,
        provisional: true,
    }
}

pub fn extract_hand_features(
    concealed_tile_keys: &[String],
    meld_tile_key_groups: &[Vec<String>],
    meld_open_flags: Option<&[bool]>,
    incoming_tile: Option<&str>,
    seat_wind_key: Option<&str>,
    round_wind_key: Option<&str>,
    decompositions: Option<&[Decomposition]>,
) -> HandFeatures {
    let cache_key = HandFeatureCacheKey {
        concealed_tile_keys: canonicalize_tile_keys(concealed_tile_keys.to_vec()),
        meld_tile_key_groups: canonicalize_tile_key_groups(meld_tile_key_groups.to_vec()),
        meld_open_flags: meld_open_flags.map(|flags| flags.to_vec()),
        incoming_tile: incoming_tile.map(ToString::to_string),
        seat_wind_key: seat_wind_key.map(ToString::to_string),
        round_wind_key: round_wind_key.map(ToString::to_string),
        decompositions: decompositions.map(|items| canonicalize_decompositions(items.to_vec())),
    };
    cached_clone(
        hand_feature_cache(),
        HAND_FEATURE_CACHE_LIMIT,
        cache_key,
        || {
            extract_hand_features_uncached(
                concealed_tile_keys,
                meld_tile_key_groups,
                meld_open_flags,
                incoming_tile,
                seat_wind_key,
                round_wind_key,
                decompositions,
            )
        },
    )
}

fn extract_hand_features_uncached(
    concealed_tile_keys: &[String],
    meld_tile_key_groups: &[Vec<String>],
    meld_open_flags: Option<&[bool]>,
    incoming_tile: Option<&str>,
    seat_wind_key: Option<&str>,
    round_wind_key: Option<&str>,
    decompositions: Option<&[Decomposition]>,
) -> HandFeatures {
    let mut effective_concealed =
        Vec::with_capacity(concealed_tile_keys.len() + usize::from(incoming_tile.is_some()));
    effective_concealed.extend(concealed_tile_keys.iter().cloned());
    if let Some(tile) = incoming_tile {
        effective_concealed.push(tile.to_string());
    }

    let meld_tile_count = meld_tile_key_groups.iter().map(Vec::len).sum::<usize>();
    let mut all_tile_keys = Vec::with_capacity(effective_concealed.len() + meld_tile_count);
    all_tile_keys.extend(effective_concealed.iter().cloned());
    for meld_group in meld_tile_key_groups {
        all_tile_keys.extend(meld_group.iter().cloned());
    }

    let triplet_keys =
        extract_triplet_keys(&effective_concealed, meld_tile_key_groups, decompositions);
    let has_open_meld = meld_open_flags
        .map(|flags| flags.iter().any(|flag| *flag))
        .unwrap_or(!meld_tile_key_groups.is_empty());

    let seat_wind_triplet = seat_wind_key
        .map(|key| triplet_keys.iter().any(|tile_key| tile_key == key))
        .unwrap_or(false);
    let round_wind_triplet = round_wind_key
        .map(|key| triplet_keys.iter().any(|tile_key| tile_key == key))
        .unwrap_or(false);

    HandFeatures {
        concealed_hand: !has_open_meld,
        thirteen_orphans: features_is_thirteen_orphans(&effective_concealed, meld_tile_key_groups),
        seven_pairs: features_is_seven_pairs(&effective_concealed, meld_tile_key_groups),
        pung_hand: features_is_pung_hand(&effective_concealed, meld_tile_key_groups),
        mixed_one_suit: is_mixed_one_suit(&all_tile_keys),
        pure_one_suit: is_pure_one_suit(&all_tile_keys),
        duan_yao: is_duan_yao(&all_tile_keys),
        hun_yao_jiu: is_hun_yao_jiu(&all_tile_keys),
        qing_yao_jiu: is_qing_yao_jiu(&all_tile_keys),
        dragon_triplet_count: triplet_keys
            .iter()
            .filter(|tile_key| DRAGON_KEYS.contains(&tile_key.as_str()))
            .count(),
        terminal_triplet_count: triplet_keys
            .iter()
            .filter(|tile_key| is_terminal_suit_tile(tile_key))
            .count(),
        non_seat_non_round_wind_triplet_count: triplet_keys
            .iter()
            .filter(|tile_key| WIND_KEYS.contains(&tile_key.as_str()))
            .filter(|tile_key| {
                seat_wind_key != Some(tile_key.as_str())
                    && round_wind_key != Some(tile_key.as_str())
            })
            .count(),
        seat_wind_triplet,
        round_wind_triplet,
    }
}

pub fn decompose_winning_hand(tile_keys: &[String]) -> Vec<Decomposition> {
    let cache_key = DecompositionCacheKey {
        concealed_tile_keys: canonicalize_tile_keys(tile_keys.to_vec()),
        meld_tile_key_groups: vec![],
    };
    cached_clone(
        decomposition_cache(),
        DECOMPOSITION_CACHE_LIMIT,
        cache_key,
        || decompose_winning_hand_uncached(tile_keys),
    )
}

fn decompose_winning_hand_uncached(tile_keys: &[String]) -> Vec<Decomposition> {
    if tile_keys.len() != 14 {
        return vec![];
    }
    let Some(counts) = tile_counts_array(tile_keys.iter().map(String::as_str)) else {
        return vec![];
    };
    let mut decompositions = Vec::new();
    if is_seven_pairs(&counts) {
        decompositions.push(Decomposition {
            kind: "seven_pairs".to_string(),
            pairs: seven_pairs_pair_tiles(&counts),
            ..Default::default()
        });
    }
    if is_thirteen_orphans(&counts) {
        let pair_tile = THIRTEEN_ORPHAN_INDICES
            .iter()
            .copied()
            .find(|index| counts[*index] == 2)
            .map(tile_key_for_index)
            .unwrap_or_default()
            .to_string();
        decompositions.push(Decomposition {
            kind: "thirteen_orphans".to_string(),
            pair: Some(pair_tile),
            orphans: THIRTEEN_ORPHAN_INDICES
                .iter()
                .map(|index| tile_key_for_index(*index).to_string())
                .collect(),
            ..Default::default()
        });
    }
    decompositions.extend(special_knitted_decompositions(&counts));
    decompositions.extend(standard_decompositions_from_counts(&counts));
    decompositions
}

pub fn decompose_winning_hand_with_melds(
    concealed_tile_keys: &[String],
    meld_tile_key_groups: &[Vec<String>],
) -> Vec<Decomposition> {
    let cache_key = DecompositionCacheKey {
        concealed_tile_keys: canonicalize_tile_keys(concealed_tile_keys.to_vec()),
        meld_tile_key_groups: canonicalize_tile_key_groups(meld_tile_key_groups.to_vec()),
    };
    cached_clone(
        decomposition_cache(),
        DECOMPOSITION_CACHE_LIMIT,
        cache_key,
        || decompose_winning_hand_with_melds_uncached(concealed_tile_keys, meld_tile_key_groups),
    )
}

fn decompose_winning_hand_with_melds_uncached(
    concealed_tile_keys: &[String],
    meld_tile_key_groups: &[Vec<String>],
) -> Vec<Decomposition> {
    if meld_tile_key_groups.is_empty() {
        return decompose_winning_hand(concealed_tile_keys);
    }

    let normalized = meld_tile_key_groups
        .iter()
        .map(|meld| normalize_meld_tile_key_group(meld))
        .collect::<Option<Vec<_>>>();
    let Some(normalized) = normalized else {
        return vec![];
    };

    let remaining_meld_count = 4_i32 - normalized.len() as i32;
    if remaining_meld_count < 0 {
        return vec![];
    }
    if concealed_tile_keys.len() != remaining_meld_count as usize * 3 + 2 {
        return vec![];
    }

    let Some(counts) = tile_counts_array(concealed_tile_keys.iter().map(String::as_str)) else {
        return vec![];
    };
    let base = standard_decompositions_from_counts(&counts);
    base.into_iter()
        .map(|mut decomposition| {
            let mut melds = normalized.clone();
            melds.extend(decomposition.melds.clone());
            decomposition.melds = melds;
            decomposition
        })
        .collect()
}

#[allow(dead_code)]
pub fn is_winning_hand(tile_keys: &[String]) -> bool {
    if tile_keys.len() != 14 {
        return false;
    }
    let Some(counts) = tile_counts_array(tile_keys.iter().map(String::as_str)) else {
        return false;
    };
    is_winning_hand_from_counts(&counts)
}

fn fan_scenarios(context: &FanContext) -> Vec<FanContext> {
    let standard_decompositions = context.standard_decompositions.clone();
    let non_standard = context
        .decompositions
        .iter()
        .filter(|decomposition| decomposition.kind != "standard")
        .cloned()
        .collect::<Vec<_>>();

    let mut scenarios = Vec::new();
    if !non_standard.is_empty() {
        scenarios.push(context.with_scenario(non_standard, vec![]));
    }
    for decomposition in standard_decompositions {
        scenarios.push(context.with_scenario(vec![decomposition.clone()], vec![decomposition]));
    }
    if scenarios.is_empty() {
        scenarios.push(context.clone());
    }
    scenarios
}

fn evaluate_scenario(context: &FanContext) -> ScenarioResult {
    let candidates = fan_candidates(context);
    let selected = select_best_candidates(&candidates);
    let mut fan_keys = selected
        .iter()
        .map(|candidate| candidate.fan_key.to_string())
        .collect::<Vec<_>>();
    let mut fan_breakdown = selected
        .iter()
        .map(|candidate| FanBreakdownEntry {
            fan_key: candidate.fan_key.to_string(),
            fan_value: candidate.fan_value,
        })
        .collect::<Vec<_>>();

    if should_award_chicken_hand(context, &fan_keys) {
        fan_keys.push("chicken_hand".to_string());
        fan_breakdown.push(FanBreakdownEntry {
            fan_key: "chicken_hand".to_string(),
            fan_value: 8,
        });
    }

    let fan_total = fan_breakdown
        .iter()
        .map(|entry| entry.fan_value)
        .sum::<i64>();
    let minimum_qualifying_fan_total = fan_breakdown
        .iter()
        .filter(|entry| entry.fan_key != "flower_tiles")
        .map(|entry| entry.fan_value)
        .sum::<i64>();

    ScenarioResult {
        fan_keys,
        fan_breakdown,
        fan_total,
        minimum_qualifying_fan_total,
    }
}
fn fan_candidates(context: &FanContext) -> Vec<FanCandidate> {
    let rules = StandardFanTable::rules();
    let mut candidates = Vec::with_capacity(rules.len());
    for (order, rule) in rules.iter().copied().enumerate() {
        let match_count = (rule.matcher)(context);
        let resolved_values = if let Some(resolver) = rule.value_resolver {
            resolver(context, match_count, rule.fan_value)
        } else {
            vec![rule.fan_value; match_count]
        };
        for resolved_value in resolved_values {
            candidates.push(FanCandidate {
                fan_key: rule.fan_key,
                fan_value: resolved_value,
                order,
                excludes: rule.excludes,
                forbidden_with: rule.forbidden_with,
            });
        }
    }
    candidates
}

fn select_best_candidates(candidates: &[FanCandidate]) -> Vec<FanCandidate> {
    let mut ordered = candidates.to_vec();
    ordered.sort_by(|left, right| {
        right
            .fan_value
            .cmp(&left.fan_value)
            .then_with(|| left.order.cmp(&right.order))
    });

    let mut suffix_sum = vec![0_i64; ordered.len() + 1];
    for index in (0..ordered.len()).rev() {
        suffix_sum[index] = suffix_sum[index + 1] + ordered[index].fan_value;
    }

    let mut best_score = -1_i64;
    let mut best_selected = Vec::<FanCandidate>::new();

    #[allow(clippy::too_many_arguments)]
    fn dfs(
        index: usize,
        score: i64,
        selected: &mut Vec<FanCandidate>,
        selected_keys: &mut HashSet<&'static str>,
        blocked_keys: &mut HashSet<&'static str>,
        ordered: &[FanCandidate],
        suffix_sum: &[i64],
        best_score: &mut i64,
        best_selected: &mut Vec<FanCandidate>,
    ) {
        if score + suffix_sum[index] < *best_score {
            return;
        }
        if index >= ordered.len() {
            if score > *best_score {
                *best_score = score;
                *best_selected = selected.clone();
            }
            return;
        }

        dfs(
            index + 1,
            score,
            selected,
            selected_keys,
            blocked_keys,
            ordered,
            suffix_sum,
            best_score,
            best_selected,
        );

        let candidate = ordered[index];
        if blocked_keys.contains(candidate.fan_key) {
            return;
        }
        if candidate
            .excludes
            .iter()
            .chain(candidate.forbidden_with.iter())
            .any(|conflict| selected_keys.contains(conflict))
        {
            return;
        }

        selected.push(candidate);
        selected_keys.insert(candidate.fan_key);
        let mut inserted_blocked =
            Vec::with_capacity(candidate.excludes.len() + candidate.forbidden_with.len());
        for conflict in candidate
            .excludes
            .iter()
            .chain(candidate.forbidden_with.iter())
            .copied()
        {
            if blocked_keys.insert(conflict) {
                inserted_blocked.push(conflict);
            }
        }
        dfs(
            index + 1,
            score + candidate.fan_value,
            selected,
            selected_keys,
            blocked_keys,
            ordered,
            suffix_sum,
            best_score,
            best_selected,
        );
        selected.pop();
        selected_keys.remove(candidate.fan_key);
        for conflict in inserted_blocked {
            blocked_keys.remove(conflict);
        }
    }

    dfs(
        0,
        0,
        &mut Vec::new(),
        &mut HashSet::new(),
        &mut HashSet::new(),
        &ordered,
        &suffix_sum,
        &mut best_score,
        &mut best_selected,
    );
    best_selected.sort_by_key(|candidate| candidate.order);
    best_selected
}

fn normalize_kong_entries(
    raw_entries: &[KongEntry],
    seat_count: usize,
) -> Vec<KongScoreDetailEntry> {
    raw_entries
        .iter()
        .map(|entry| {
            let unit_score = match entry.kong_type.as_str() {
                "exposed_kong" | "concealed_kong" | "add_kong" => 1_i64,
                _ => 0,
            };
            let mut delta_by_seat = vec![0_i64; seat_count];
            for payer in &entry.payer_seats {
                if *payer < seat_count {
                    delta_by_seat[*payer] -= unit_score;
                }
                if entry.actor_seat < seat_count {
                    delta_by_seat[entry.actor_seat] += unit_score;
                }
            }
            KongScoreDetailEntry {
                kong_type: entry.kong_type.clone(),
                actor_seat: entry.actor_seat,
                payer_seats: entry.payer_seats.clone(),
                delta_by_seat,
            }
        })
        .collect()
}

fn sum_delta_by_seat(entries: &[KongScoreDetailEntry], seat_count: usize) -> Vec<i64> {
    let mut totals = vec![0_i64; seat_count];
    for entry in entries {
        for (seat, total) in totals.iter_mut().enumerate().take(seat_count) {
            *total += entry.delta_by_seat.get(seat).copied().unwrap_or(0);
        }
    }
    totals
}

fn fan_delta_by_seat(
    win_type: &str,
    winner_seat: Option<usize>,
    discarder_seat: Option<usize>,
    fan_total: i64,
    seat_count: usize,
) -> Vec<i64> {
    let mut deltas = vec![0_i64; seat_count];
    let Some(winner_seat) = winner_seat else {
        return deltas;
    };
    if fan_total <= 0 || winner_seat >= seat_count {
        return deltas;
    }

    if win_type == "self_draw" {
        let payment = fan_total + MCR_BASE_POINTS;
        let mut winner_gain = 0_i64;
        for (seat, delta) in deltas.iter_mut().enumerate().take(seat_count) {
            if seat == winner_seat {
                continue;
            }
            *delta -= payment;
            winner_gain += payment;
        }
        deltas[winner_seat] += winner_gain;
        return deltas;
    }

    if let Some(discarder_seat) = discarder_seat {
        deltas[winner_seat] +=
            fan_total + (MCR_BASE_POINTS * (seat_count.saturating_sub(1) as i64));
        for (seat, delta) in deltas.iter_mut().enumerate().take(seat_count) {
            if seat == winner_seat {
                continue;
            }
            if seat == discarder_seat {
                *delta -= fan_total + MCR_BASE_POINTS;
            } else {
                *delta -= MCR_BASE_POINTS;
            }
        }
    }
    deltas
}

fn derive_standard_data(
    standard_decompositions: &[Decomposition],
    concealed_tile_keys: &[String],
    kong_entries: &[KongEntry],
    winner_seat: Option<usize>,
) -> StandardDerivedData {
    let concealed_counts = tile_counts_array(concealed_tile_keys.iter().map(String::as_str));
    let concealed_kongs = kong_entries
        .iter()
        .filter(|entry| Some(entry.actor_seat) == winner_seat)
        .filter(|entry| entry.kong_type == "concealed_kong")
        .count();

    let mut derived = StandardDerivedData {
        concealed_pung_count: concealed_kongs,
        ..Default::default()
    };
    let mut best_standard_concealed_pungs = 0usize;

    for decomposition in standard_decompositions {
        if derived.pair_tile.is_none() {
            derived.pair_tile = decomposition.pair.clone();
        }

        let mut decomposition_concealed_pungs = 0usize;
        for meld in &decomposition.melds {
            if meld.len() != 3 {
                continue;
            }

            if meld.iter().all(|tile_key| tile_key == &meld[0]) {
                let triplet_tile = &meld[0];
                derived.triplet_keys.insert(triplet_tile.clone());
                if let Some((suit, rank)) = parse_suit(triplet_tile) {
                    derived
                        .triplet_suits_by_rank
                        .entry(rank)
                        .or_default()
                        .insert(suit);
                    *derived
                        .triplet_rank_counts_by_suit
                        .entry(suit)
                        .or_default()
                        .entry(rank)
                        .or_insert(0) += 1;
                    derived
                        .triplet_rank_sets_by_suit
                        .entry(suit)
                        .or_default()
                        .insert(rank);
                }

                if concealed_counts.as_ref().is_some_and(|counts| {
                    tile_index(triplet_tile)
                        .map(|index| counts[index] >= 3)
                        .unwrap_or(false)
                }) {
                    decomposition_concealed_pungs += 1;
                }
                continue;
            }

            if let Some((suit, start)) = sequence_meld_start(meld) {
                derived
                    .sequence_suits_by_start
                    .entry(start)
                    .or_default()
                    .insert(suit);
                *derived
                    .sequence_start_counts_by_suit
                    .entry(suit)
                    .or_default()
                    .entry(start)
                    .or_insert(0) += 1;
            }
        }

        best_standard_concealed_pungs =
            best_standard_concealed_pungs.max(decomposition_concealed_pungs);
    }

    derived.concealed_pung_count += best_standard_concealed_pungs;
    derived
}

fn derive_all_tile_data(all_tile_keys: &[String]) -> AllTileDerivedData {
    let counts = tile_counts_array(all_tile_keys.iter().map(String::as_str));
    let has_tiles = !all_tile_keys.is_empty();
    let mut derived = AllTileDerivedData {
        counts,
        all_honours: has_tiles,
        all_terminal_or_honour: has_tiles,
        all_terminals: has_tiles,
        all_even: has_tiles,
        all_green: has_tiles,
        upper_four: has_tiles,
        upper_tiles: has_tiles,
        lower_four: has_tiles,
        lower_tiles: has_tiles,
        middle_tiles: has_tiles,
        reversible_tiles: has_tiles,
        ..Default::default()
    };

    for tile_key in all_tile_keys {
        if let Some((suit, rank)) = parse_suit(tile_key) {
            derived.suited_suits.insert(suit);
            derived.all_honours = false;
            derived.has_terminals |= matches!(rank, 1 | 9);
            derived.all_terminal_or_honour &= matches!(rank, 1 | 9);
            derived.all_terminals &= matches!(rank, 1 | 9);
            derived.all_even &= matches!(rank, 2 | 4 | 6 | 8);
            derived.upper_four &= rank >= 6;
            derived.upper_tiles &= matches!(rank, 7..=9);
            derived.lower_four &= rank <= 4;
            derived.lower_tiles &= matches!(rank, 1..=3);
            derived.middle_tiles &= matches!(rank, 4..=6);
        } else {
            derived.has_honours = true;
            derived.has_wind |= WIND_KEYS.contains(&tile_key.as_str());
            derived.has_dragon |= DRAGON_KEYS.contains(&tile_key.as_str());
            derived.all_terminals = false;
            derived.all_even = false;
            derived.upper_four = false;
            derived.upper_tiles = false;
            derived.lower_four = false;
            derived.lower_tiles = false;
            derived.middle_tiles = false;
        }
        derived.all_green &= ALL_GREEN_KEYS.contains(&tile_key.as_str());
        derived.reversible_tiles &= REVERSIBLE_TILE_KEYS.contains(&tile_key.as_str());
    }

    if let Some(counts) = derived.counts.as_ref() {
        derived.tile_hog = counts.iter().any(|count| *count >= 4);
    }

    derived
}

fn should_award_chicken_hand(context: &FanContext, fan_keys: &[String]) -> bool {
    if context.all_tile_keys.len() != 14 {
        return false;
    }
    !fan_keys.iter().any(|fan_key| fan_key != "flower_tiles")
}

pub(crate) fn registered_fan_rules() -> &'static [FanRule] {
    static FAN_RULES: &[FanRule] = &[
        FanRule {
            fan_key: "self_drawn",
            fan_value: 1,
            matcher: match_self_drawn,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "out_with_replacement_tile",
            fan_value: 8,
            matcher: match_out_with_replacement_tile,
            value_resolver: None,
            excludes: &["self_drawn"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "last_tile_draw",
            fan_value: 8,
            matcher: match_last_tile_draw,
            value_resolver: None,
            excludes: &["self_drawn"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "last_tile_claim",
            fan_value: 8,
            matcher: match_last_tile_claim,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "robbing_the_kong",
            fan_value: 8,
            matcher: match_robbing_the_kong,
            value_resolver: None,
            excludes: &["last_tile"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "all_simples",
            fan_value: 2,
            matcher: match_all_simples,
            value_resolver: None,
            excludes: &["no_honours"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "full_flush",
            fan_value: 24,
            matcher: match_full_flush,
            value_resolver: None,
            excludes: &["one_voided_suit", "no_honours"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "half_flush",
            fan_value: 6,
            matcher: match_half_flush,
            value_resolver: None,
            excludes: &["one_voided_suit"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "thirteen_orphans",
            fan_value: 88,
            matcher: match_thirteen_orphans,
            value_resolver: None,
            excludes: &["all_types", "concealed_hand", "single_wait"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "seven_pairs",
            fan_value: 24,
            matcher: match_seven_pairs,
            value_resolver: None,
            excludes: &["concealed_hand", "single_wait"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "seven_shifted_pairs",
            fan_value: 88,
            matcher: match_seven_shifted_pairs,
            value_resolver: None,
            excludes: &[
                "seven_pairs",
                "full_flush",
                "concealed_hand",
                "one_voided_suit",
                "no_honours",
                "single_wait",
            ],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "nine_gates",
            fan_value: 88,
            matcher: match_nine_gates,
            value_resolver: None,
            excludes: &[
                "pung_of_terminals_or_honours",
                "full_flush",
                "concealed_hand",
                "one_voided_suit",
                "no_honours",
            ],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "knitted_straight",
            fan_value: 12,
            matcher: match_knitted_straight,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "lesser_honours_and_knitted_tiles",
            fan_value: 12,
            matcher: match_lesser_honours_knitted,
            value_resolver: None,
            excludes: &["all_types", "concealed_hand"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "greater_honours_and_knitted_tiles",
            fan_value: 24,
            matcher: match_greater_honours_knitted,
            value_resolver: None,
            excludes: &[
                "all_types",
                "concealed_hand",
                "lesser_honours_and_knitted_tiles",
            ],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "big_three_winds",
            fan_value: 12,
            matcher: match_big_three_winds,
            value_resolver: None,
            excludes: &["pung_of_terminals_or_honours"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "big_three_dragons",
            fan_value: 88,
            matcher: match_big_three_dragons,
            value_resolver: None,
            excludes: &["dragon_pung", "two_dragon_pungs"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "two_dragon_pungs",
            fan_value: 6,
            matcher: match_two_dragon_pungs,
            value_resolver: None,
            excludes: &["dragon_pung"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "little_three_dragons",
            fan_value: 64,
            matcher: match_little_three_dragons,
            value_resolver: None,
            excludes: &["dragon_pung", "two_dragon_pungs"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "big_four_winds",
            fan_value: 88,
            matcher: match_big_four_winds,
            value_resolver: None,
            excludes: &[
                "pung_of_terminals_or_honours",
                "prevalent_wind",
                "seat_wind",
                "big_three_winds",
                "all_pungs",
            ],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "little_four_winds",
            fan_value: 64,
            matcher: match_little_four_winds,
            value_resolver: None,
            excludes: &["pung_of_terminals_or_honours", "big_three_winds"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "all_honours",
            fan_value: 64,
            matcher: match_all_honours,
            value_resolver: None,
            excludes: &["pung_of_terminals_or_honours", "outside_hand", "all_pungs"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "all_terminals_and_honours",
            fan_value: 32,
            matcher: match_all_terminals_and_honours,
            value_resolver: None,
            excludes: &["pung_of_terminals_or_honours", "outside_hand", "all_pungs"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "all_terminals",
            fan_value: 88,
            matcher: match_all_terminals,
            value_resolver: None,
            excludes: &[
                "pung_of_terminals_or_honours",
                "outside_hand",
                "all_pungs",
                "no_honours",
            ],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "all_even_pungs",
            fan_value: 24,
            matcher: match_all_even_pungs,
            value_resolver: None,
            excludes: &["all_pungs", "no_honours", "all_simples"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "all_green",
            fan_value: 88,
            matcher: match_all_green,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "all_pungs",
            fan_value: 6,
            matcher: match_all_pungs,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "seat_wind",
            fan_value: 2,
            matcher: match_seat_wind,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "prevalent_wind",
            fan_value: 2,
            matcher: match_prevalent_wind,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "dragon_pung",
            fan_value: 2,
            matcher: match_dragon_pung,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "triple_pung",
            fan_value: 16,
            matcher: match_triple_pung,
            value_resolver: None,
            excludes: &["double_pung"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "double_pung",
            fan_value: 2,
            matcher: match_double_pung,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "mixed_shifted_pungs",
            fan_value: 8,
            matcher: match_mixed_shifted_pungs,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "pure_shifted_pungs",
            fan_value: 24,
            matcher: match_pure_shifted_pungs,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "pung_of_terminals_or_honours",
            fan_value: 1,
            matcher: match_pung_of_terminals_or_honours,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "two_concealed_pungs",
            fan_value: 2,
            matcher: match_two_concealed_pungs,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "three_concealed_pungs",
            fan_value: 16,
            matcher: match_three_concealed_pungs,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "four_pure_shifted_pungs",
            fan_value: 48,
            matcher: match_four_pure_shifted_pungs,
            value_resolver: None,
            excludes: &["all_pungs", "pure_shifted_pungs"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "four_concealed_pungs",
            fan_value: 64,
            matcher: match_four_concealed_pungs,
            value_resolver: None,
            excludes: &["all_pungs", "concealed_hand"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "all_types",
            fan_value: 6,
            matcher: match_all_types,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "all_fives",
            fan_value: 16,
            matcher: match_all_fives,
            value_resolver: None,
            excludes: &["no_honours", "all_simples"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "upper_four",
            fan_value: 12,
            matcher: match_upper_four,
            value_resolver: None,
            excludes: &["no_honours"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "upper_tiles",
            fan_value: 24,
            matcher: match_upper_tiles,
            value_resolver: None,
            excludes: &["no_honours", "upper_four"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "lower_four",
            fan_value: 12,
            matcher: match_lower_four,
            value_resolver: None,
            excludes: &["no_honours"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "lower_tiles",
            fan_value: 24,
            matcher: match_lower_tiles,
            value_resolver: None,
            excludes: &["no_honours", "lower_four"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "middle_tiles",
            fan_value: 24,
            matcher: match_middle_tiles,
            value_resolver: None,
            excludes: &["no_honours", "all_simples"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "tile_hog",
            fan_value: 2,
            matcher: match_tile_hog,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "reversible_tiles",
            fan_value: 8,
            matcher: match_reversible_tiles,
            value_resolver: None,
            excludes: &["one_voided_suit"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "pure_straight",
            fan_value: 16,
            matcher: match_pure_straight,
            value_resolver: None,
            excludes: &["short_straight", "two_terminal_chows"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "mixed_triple_chow",
            fan_value: 8,
            matcher: match_mixed_triple_chow,
            value_resolver: None,
            excludes: &["mixed_double_chow"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "pure_double_chow",
            fan_value: 1,
            matcher: match_pure_double_chow,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "mixed_double_chow",
            fan_value: 1,
            matcher: match_mixed_double_chow,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "mixed_straight",
            fan_value: 8,
            matcher: match_mixed_straight,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "mixed_shifted_chows",
            fan_value: 6,
            matcher: match_mixed_shifted_chows,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "pure_shifted_chows",
            fan_value: 16,
            matcher: match_pure_shifted_chows,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "four_pure_shifted_chows",
            fan_value: 32,
            matcher: match_four_pure_shifted_chows,
            value_resolver: None,
            excludes: &["pure_shifted_chows", "short_straight", "two_terminal_chows"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "pure_triple_chow",
            fan_value: 24,
            matcher: match_pure_triple_chow,
            value_resolver: None,
            excludes: &["pure_double_chow"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "quadruple_chow",
            fan_value: 48,
            matcher: match_quadruple_chow,
            value_resolver: None,
            excludes: &["pure_double_chow", "pure_triple_chow", "tile_hog"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "short_straight",
            fan_value: 1,
            matcher: match_short_straight,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "two_terminal_chows",
            fan_value: 1,
            matcher: match_two_terminal_chows,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "three_suited_terminal_chows",
            fan_value: 16,
            matcher: match_three_suited_terminal_chows,
            value_resolver: None,
            excludes: &[
                "all_chows",
                "mixed_double_chow",
                "two_terminal_chows",
                "no_honours",
            ],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "pure_terminal_chows",
            fan_value: 64,
            matcher: match_pure_terminal_chows,
            value_resolver: None,
            excludes: &[
                "all_chows",
                "pure_double_chow",
                "two_terminal_chows",
                "full_flush",
                "one_voided_suit",
                "no_honours",
            ],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "one_voided_suit",
            fan_value: 1,
            matcher: match_one_voided_suit,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "no_honours",
            fan_value: 1,
            matcher: match_no_honours,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "concealed_kong",
            fan_value: 2,
            matcher: match_concealed_kong,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "two_concealed_kongs",
            fan_value: 8,
            matcher: match_two_concealed_kongs,
            value_resolver: None,
            excludes: &["concealed_kong"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "two_melded_kongs",
            fan_value: 4,
            matcher: match_two_melded_kongs,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "melded_kong",
            fan_value: 1,
            matcher: match_melded_kong,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "three_kongs",
            fan_value: 32,
            matcher: match_three_kongs,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "four_kongs",
            fan_value: 88,
            matcher: match_four_kongs,
            value_resolver: None,
            excludes: &["all_pungs", "single_wait"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "all_chows",
            fan_value: 2,
            matcher: match_all_chows,
            value_resolver: None,
            excludes: &["no_honours"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "outside_hand",
            fan_value: 4,
            matcher: match_outside_hand,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "edge_wait",
            fan_value: 1,
            matcher: match_edge_wait,
            value_resolver: None,
            excludes: &["closed_wait", "single_wait"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "closed_wait",
            fan_value: 1,
            matcher: match_closed_wait,
            value_resolver: None,
            excludes: &["edge_wait", "single_wait"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "single_wait",
            fan_value: 1,
            matcher: match_single_wait,
            value_resolver: None,
            excludes: &["edge_wait", "closed_wait"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "concealed_hand",
            fan_value: 2,
            matcher: match_concealed_hand,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "fully_concealed_hand",
            fan_value: 4,
            matcher: match_fully_concealed_hand,
            value_resolver: None,
            excludes: &["self_drawn", "concealed_hand"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "melded_hand",
            fan_value: 6,
            matcher: match_melded_hand,
            value_resolver: None,
            excludes: &["single_wait"],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "flower_tiles",
            fan_value: 1,
            matcher: match_flower_tiles,
            value_resolver: Some(resolve_flower_tiles),
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "last_tile",
            fan_value: 4,
            matcher: match_last_tile,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
        FanRule {
            fan_key: "chicken_hand",
            fan_value: 8,
            matcher: |_context| 0,
            value_resolver: None,
            excludes: &[],
            forbidden_with: &[],
        },
    ];
    FAN_RULES
}
fn match_self_drawn(context: &FanContext) -> usize {
    usize::from(context.win_type == "self_draw")
}
fn match_out_with_replacement_tile(context: &FanContext) -> usize {
    usize::from(context.timing.gang_shang_hua)
}
fn match_last_tile_draw(context: &FanContext) -> usize {
    usize::from(context.timing.hai_di_lao_yue && !context.timing.gang_shang_hua)
}
fn match_last_tile_claim(context: &FanContext) -> usize {
    usize::from(
        context.timing.he_di_lao_yu
            && !context.timing.gang_shang_hua
            && !context.timing.hai_di_lao_yue,
    )
}
fn match_robbing_the_kong(context: &FanContext) -> usize {
    usize::from(context.timing.robbing_the_kong)
}
fn match_all_simples(context: &FanContext) -> usize {
    usize::from(
        context.features.duan_yao
            && !context.features.hun_yao_jiu
            && !context.features.qing_yao_jiu,
    )
}
fn match_full_flush(context: &FanContext) -> usize {
    usize::from(context.features.pure_one_suit)
}
fn match_half_flush(context: &FanContext) -> usize {
    usize::from(context.features.mixed_one_suit && !context.features.pure_one_suit)
}
fn match_thirteen_orphans(context: &FanContext) -> usize {
    usize::from(context.features.thirteen_orphans)
}
fn match_seven_pairs(context: &FanContext) -> usize {
    usize::from(context.features.seven_pairs)
}
fn match_seven_shifted_pairs(context: &FanContext) -> usize {
    usize::from(is_seven_shifted_pairs_pattern(context))
}
fn match_nine_gates(context: &FanContext) -> usize {
    usize::from(is_nine_gates(context))
}
fn match_knitted_straight(context: &FanContext) -> usize {
    usize::from(has_decomposition_kind(context, "knitted_straight"))
}
fn match_lesser_honours_knitted(context: &FanContext) -> usize {
    usize::from(has_decomposition_kind(
        context,
        "lesser_honours_and_knitted_tiles",
    ))
}
fn match_greater_honours_knitted(context: &FanContext) -> usize {
    usize::from(has_decomposition_kind(
        context,
        "greater_honours_and_knitted_tiles",
    ))
}
fn match_big_three_winds(context: &FanContext) -> usize {
    let triplets = triplet_keys_set(context);
    usize::from(
        triplets
            .iter()
            .filter(|tile_key| WIND_KEYS.contains(&tile_key.as_str()))
            .count()
            >= 3,
    )
}
fn match_big_three_dragons(context: &FanContext) -> usize {
    let triplets = triplet_keys_set(context);
    usize::from(
        DRAGON_KEYS
            .iter()
            .all(|tile| triplets.iter().any(|value| value == tile)),
    )
}
fn match_two_dragon_pungs(context: &FanContext) -> usize {
    let triplets = triplet_keys_set(context);
    usize::from(
        triplets
            .iter()
            .filter(|tile_key| DRAGON_KEYS.contains(&tile_key.as_str()))
            .count()
            >= 2,
    )
}
fn match_little_three_dragons(context: &FanContext) -> usize {
    let triplets = triplet_keys_set(context);
    let pair_tile = pair_tile(context);
    usize::from(
        triplets
            .iter()
            .filter(|tile_key| DRAGON_KEYS.contains(&tile_key.as_str()))
            .count()
            == 2
            && pair_tile
                .map(|tile| DRAGON_KEYS.contains(&tile))
                .unwrap_or(false),
    )
}
fn match_big_four_winds(context: &FanContext) -> usize {
    let triplets = triplet_keys_set(context);
    usize::from(
        WIND_KEYS
            .iter()
            .all(|tile| triplets.iter().any(|value| value == tile)),
    )
}
fn match_little_four_winds(context: &FanContext) -> usize {
    let triplets = triplet_keys_set(context);
    let pair_tile = pair_tile(context);
    usize::from(
        triplets
            .iter()
            .filter(|tile_key| WIND_KEYS.contains(&tile_key.as_str()))
            .count()
            == 3
            && pair_tile
                .map(|tile| WIND_KEYS.contains(&tile))
                .unwrap_or(false),
    )
}
fn match_all_honours(context: &FanContext) -> usize {
    usize::from(context.all_tile_derived.all_honours)
}
fn match_all_terminals_and_honours(context: &FanContext) -> usize {
    usize::from(
        context.all_tile_derived.has_honours
            && context.all_tile_derived.has_terminals
            && context.all_tile_derived.all_terminal_or_honour,
    )
}
fn match_all_terminals(context: &FanContext) -> usize {
    usize::from(context.all_tile_derived.all_terminals)
}
fn match_all_even_pungs(context: &FanContext) -> usize {
    usize::from(context.features.pung_hand && context.all_tile_derived.all_even)
}
fn match_all_green(context: &FanContext) -> usize {
    usize::from(context.all_tile_derived.all_green)
}
fn match_all_pungs(context: &FanContext) -> usize {
    usize::from(context.features.pung_hand && !context.features.seven_pairs)
}
fn match_seat_wind(context: &FanContext) -> usize {
    usize::from(context.features.seat_wind_triplet)
}
fn match_prevalent_wind(context: &FanContext) -> usize {
    usize::from(context.features.round_wind_triplet)
}
fn match_dragon_pung(context: &FanContext) -> usize {
    context.features.dragon_triplet_count
}
fn match_triple_pung(context: &FanContext) -> usize {
    usize::from(has_triple_pung(context))
}
fn match_double_pung(context: &FanContext) -> usize {
    usize::from(has_double_pung(context))
}
fn match_mixed_shifted_pungs(context: &FanContext) -> usize {
    usize::from(has_mixed_shifted_pungs(context))
}
fn match_pure_shifted_pungs(context: &FanContext) -> usize {
    usize::from(has_pure_shifted_pungs(context))
}
fn match_pung_of_terminals_or_honours(context: &FanContext) -> usize {
    context.features.terminal_triplet_count + context.features.non_seat_non_round_wind_triplet_count
}
fn match_two_concealed_pungs(context: &FanContext) -> usize {
    usize::from(concealed_pung_count(context) >= 2)
}
fn match_three_concealed_pungs(context: &FanContext) -> usize {
    usize::from(concealed_pung_count(context) >= 3)
}
fn match_four_pure_shifted_pungs(context: &FanContext) -> usize {
    usize::from(has_four_pure_shifted_pungs(context))
}
fn match_four_concealed_pungs(context: &FanContext) -> usize {
    usize::from(concealed_pung_count(context) >= 4)
}
fn match_all_types(context: &FanContext) -> usize {
    usize::from(has_all_types(context))
}
fn match_all_fives(context: &FanContext) -> usize {
    usize::from(has_all_fives(context))
}
fn match_upper_four(context: &FanContext) -> usize {
    usize::from(is_upper_four(context))
}
fn match_upper_tiles(context: &FanContext) -> usize {
    usize::from(is_upper_tiles(context))
}
fn match_lower_four(context: &FanContext) -> usize {
    usize::from(is_lower_four(context))
}
fn match_lower_tiles(context: &FanContext) -> usize {
    usize::from(is_lower_tiles(context))
}
fn match_middle_tiles(context: &FanContext) -> usize {
    usize::from(is_middle_tiles(context))
}
fn match_tile_hog(context: &FanContext) -> usize {
    usize::from(has_tile_hog(context))
}
fn match_reversible_tiles(context: &FanContext) -> usize {
    usize::from(has_reversible_tiles(context))
}
fn match_pure_straight(context: &FanContext) -> usize {
    usize::from(has_pure_straight(context))
}
fn match_mixed_triple_chow(context: &FanContext) -> usize {
    usize::from(has_mixed_triple_chow(context))
}
fn match_pure_double_chow(context: &FanContext) -> usize {
    usize::from(has_pure_double_chow(context))
}
fn match_mixed_double_chow(context: &FanContext) -> usize {
    usize::from(has_mixed_double_chow(context))
}
fn match_mixed_straight(context: &FanContext) -> usize {
    usize::from(has_mixed_straight(context))
}
fn match_mixed_shifted_chows(context: &FanContext) -> usize {
    usize::from(has_mixed_shifted_chows(context))
}
fn match_pure_shifted_chows(context: &FanContext) -> usize {
    usize::from(has_pure_shifted_chows(context))
}
fn match_four_pure_shifted_chows(context: &FanContext) -> usize {
    usize::from(has_four_pure_shifted_chows(context))
}
fn match_pure_triple_chow(context: &FanContext) -> usize {
    usize::from(has_pure_triple_chow(context))
}
fn match_quadruple_chow(context: &FanContext) -> usize {
    usize::from(has_quadruple_chow(context))
}
fn match_short_straight(context: &FanContext) -> usize {
    usize::from(has_short_straight(context))
}
fn match_two_terminal_chows(context: &FanContext) -> usize {
    usize::from(has_two_terminal_chows(context))
}
fn match_three_suited_terminal_chows(context: &FanContext) -> usize {
    usize::from(has_three_suited_terminal_chows(context))
}
fn match_pure_terminal_chows(context: &FanContext) -> usize {
    usize::from(has_pure_terminal_chows(context))
}
fn match_one_voided_suit(context: &FanContext) -> usize {
    usize::from(has_one_voided_suit(context))
}
fn match_no_honours(context: &FanContext) -> usize {
    usize::from(has_no_honours(context))
}
fn match_concealed_kong(context: &FanContext) -> usize {
    usize::from(concealed_kong_count(context) >= 1)
}
fn match_two_concealed_kongs(context: &FanContext) -> usize {
    usize::from(concealed_kong_count(context) >= 2)
}
fn match_two_melded_kongs(context: &FanContext) -> usize {
    usize::from(melded_kong_count(context) >= 2)
}
fn match_melded_kong(context: &FanContext) -> usize {
    usize::from(melded_kong_count(context) >= 1)
}
fn match_three_kongs(context: &FanContext) -> usize {
    usize::from(total_kong_count(context) >= 3)
}
fn match_four_kongs(context: &FanContext) -> usize {
    usize::from(total_kong_count(context) >= 4)
}
fn match_all_chows(context: &FanContext) -> usize {
    usize::from(is_all_chows(context))
}
fn match_outside_hand(context: &FanContext) -> usize {
    usize::from(is_outside_hand(context))
}
fn match_edge_wait(context: &FanContext) -> usize {
    usize::from(context.wait_types.iter().any(|wait| wait == "edge_wait"))
}
fn match_closed_wait(context: &FanContext) -> usize {
    usize::from(context.wait_types.iter().any(|wait| wait == "closed_wait"))
}
fn match_single_wait(context: &FanContext) -> usize {
    usize::from(context.wait_types.iter().any(|wait| wait == "single_wait"))
}
fn match_concealed_hand(context: &FanContext) -> usize {
    usize::from(context.features.concealed_hand)
}
fn match_fully_concealed_hand(context: &FanContext) -> usize {
    usize::from(context.features.concealed_hand && context.win_type == "self_draw")
}
fn match_melded_hand(context: &FanContext) -> usize {
    usize::from(
        context.win_type == "discard"
            && context.open_meld_tile_key_groups.len() == 4
            && context.concealed_tile_keys.len() == 2,
    )
}
fn match_flower_tiles(context: &FanContext) -> usize {
    context.flower_count
}
fn resolve_flower_tiles(_context: &FanContext, match_count: usize, fan_value: i64) -> Vec<i64> {
    if match_count > 0 {
        vec![match_count as i64 * fan_value]
    } else {
        vec![]
    }
}
fn match_last_tile(context: &FanContext) -> usize {
    let Some(winning_tile) = context.winning_tile.as_deref() else {
        return 0;
    };
    usize::from(
        context
            .visible_tile_keys
            .iter()
            .filter(|tile_key| tile_key.as_str() == winning_tile)
            .count()
            >= 3,
    )
}

fn features_is_seven_pairs(tile_keys: &[String], meld_tile_key_groups: &[Vec<String>]) -> bool {
    if !meld_tile_key_groups.is_empty() || tile_keys.len() != 14 {
        return false;
    }
    tile_counts_array(tile_keys.iter().map(String::as_str))
        .is_some_and(|counts| is_seven_pairs(&counts))
}

fn features_is_thirteen_orphans(
    tile_keys: &[String],
    meld_tile_key_groups: &[Vec<String>],
) -> bool {
    if !meld_tile_key_groups.is_empty() || tile_keys.len() != 14 {
        return false;
    }
    tile_counts_array(tile_keys.iter().map(String::as_str))
        .is_some_and(|counts| is_thirteen_orphans(&counts))
}

fn features_is_pung_hand(tile_keys: &[String], meld_tile_key_groups: &[Vec<String>]) -> bool {
    if meld_tile_key_groups
        .iter()
        .any(|meld| meld_is_sequence(meld))
    {
        return false;
    }
    tile_counts_array(tile_keys.iter().map(String::as_str))
        .is_some_and(|counts| can_form_all_pungs(&counts))
}

fn is_duan_yao(tile_keys: &[String]) -> bool {
    tile_keys.iter().all(|tile_key| is_simple_tile(tile_key))
}
fn is_hun_yao_jiu(tile_keys: &[String]) -> bool {
    let has_honours = tile_keys
        .iter()
        .any(|tile_key| parse_suit(tile_key).is_none());
    let has_terminals = tile_keys
        .iter()
        .any(|tile_key| is_terminal_suit_tile(tile_key));
    has_honours
        && has_terminals
        && tile_keys
            .iter()
            .all(|tile_key| is_terminal_or_honour(tile_key))
}
fn is_qing_yao_jiu(tile_keys: &[String]) -> bool {
    !tile_keys.is_empty()
        && tile_keys
            .iter()
            .all(|tile_key| is_terminal_suit_tile(tile_key))
}

fn extract_triplet_keys(
    tile_keys: &[String],
    meld_tile_key_groups: &[Vec<String>],
    decompositions: Option<&[Decomposition]>,
) -> Vec<String> {
    let mut triplet_keys = Vec::new();
    if let Some(decomposition) = standard_decomposition(tile_keys, decompositions) {
        triplet_keys.extend(
            decomposition
                .melds
                .iter()
                .filter(|meld| meld.len() >= 3 && meld.iter().all(|tile_key| tile_key == &meld[0]))
                .map(|meld| meld[0].clone()),
        );
        if decompositions.is_some() {
            return triplet_keys;
        }
    }
    triplet_keys.extend(
        meld_tile_key_groups
            .iter()
            .filter(|meld| meld.len() >= 3 && meld.iter().all(|tile_key| tile_key == &meld[0]))
            .map(|meld| meld[0].clone()),
    );
    triplet_keys
}

fn standard_decomposition(
    tile_keys: &[String],
    decompositions: Option<&[Decomposition]>,
) -> Option<Decomposition> {
    if let Some(decompositions) = decompositions {
        return decompositions
            .iter()
            .find(|decomposition| decomposition.kind == "standard")
            .cloned();
    }
    decompose_standard_hand(tile_keys)
}

fn decompose_standard_hand(tile_keys: &[String]) -> Option<Decomposition> {
    tile_counts_array(tile_keys.iter().map(String::as_str))
        .and_then(|counts| first_standard_decomposition_from_counts(&counts))
}

fn can_form_all_pungs(counts: &TileCounts) -> bool {
    if total_tile_count(counts) % 3 != 2 {
        return false;
    }
    counts.iter().enumerate().any(|(tile_index, count)| {
        if *count < 2_u8 {
            return false;
        }
        let mut next = *counts;
        next[tile_index] -= 2;
        next.iter().all(|value| value % 3 == 0)
    })
}
fn is_mixed_one_suit(tile_keys: &[String]) -> bool {
    let suits = tile_keys
        .iter()
        .filter_map(|tile_key| parse_suit(tile_key).map(|(prefix, _)| prefix))
        .collect::<HashSet<_>>();
    let has_honours = tile_keys
        .iter()
        .any(|tile_key| parse_suit(tile_key).is_none());
    suits.len() == 1 && has_honours
}

fn is_pure_one_suit(tile_keys: &[String]) -> bool {
    let suits = tile_keys
        .iter()
        .filter_map(|tile_key| parse_suit(tile_key).map(|(prefix, _)| prefix))
        .collect::<HashSet<_>>();
    let has_honours = tile_keys
        .iter()
        .any(|tile_key| parse_suit(tile_key).is_none());
    suits.len() == 1 && !has_honours
}

fn has_all_types(context: &FanContext) -> bool {
    context.all_tile_derived.suited_suits == HashSet::from(['w', 't', 'b'])
        && context.all_tile_derived.has_wind
        && context.all_tile_derived.has_dragon
}

fn has_all_fives(context: &FanContext) -> bool {
    let Some(decomposition) = context.standard_decompositions.first() else {
        return false;
    };
    let Some(pair) = decomposition.pair.as_deref() else {
        return false;
    };
    if !matches!(pair, "w5" | "t5" | "b5") {
        return false;
    }
    !decomposition.melds.is_empty()
        && decomposition.melds.iter().all(|meld| {
            meld.iter()
                .any(|tile_key| matches!(tile_key.as_str(), "w5" | "t5" | "b5"))
        })
}

fn is_upper_four(context: &FanContext) -> bool {
    context.all_tile_derived.upper_four
}
fn is_upper_tiles(context: &FanContext) -> bool {
    context.all_tile_derived.upper_tiles
}
fn is_lower_four(context: &FanContext) -> bool {
    context.all_tile_derived.lower_four
}
fn is_lower_tiles(context: &FanContext) -> bool {
    context.all_tile_derived.lower_tiles
}
fn is_middle_tiles(context: &FanContext) -> bool {
    context.all_tile_derived.middle_tiles
}
fn has_tile_hog(context: &FanContext) -> bool {
    context.all_tile_derived.tile_hog
}
fn has_reversible_tiles(context: &FanContext) -> bool {
    context.all_tile_derived.reversible_tiles
}

fn has_pure_straight(context: &FanContext) -> bool {
    sequence_start_counts_by_suit(context)
        .values()
        .any(|starts| starts.contains_key(&1) && starts.contains_key(&4) && starts.contains_key(&7))
}

fn has_mixed_triple_chow(context: &FanContext) -> bool {
    sequence_suits_by_start(context)
        .values()
        .any(|suits| suits == &HashSet::from(['w', 't', 'b']))
}

fn has_pure_double_chow(context: &FanContext) -> bool {
    sequence_start_counts_by_suit(context)
        .values()
        .any(|counts| counts.values().any(|count| *count >= 2))
}

fn has_mixed_double_chow(context: &FanContext) -> bool {
    sequence_suits_by_start(context)
        .values()
        .any(|suits| suits.len() >= 2)
}

fn has_mixed_straight(context: &FanContext) -> bool {
    let grouped = sequence_suits_by_start(context);
    if !(grouped.contains_key(&1) && grouped.contains_key(&4) && grouped.contains_key(&7)) {
        return false;
    }
    for suit1 in grouped.get(&1).into_iter().flatten() {
        for suit2 in grouped.get(&4).into_iter().flatten() {
            for suit3 in grouped.get(&7).into_iter().flatten() {
                if HashSet::from([*suit1, *suit2, *suit3]).len() == 3 {
                    return true;
                }
            }
        }
    }
    false
}

fn has_mixed_shifted_chows(context: &FanContext) -> bool {
    let grouped = sequence_suits_by_start(context);
    for start in 1..=5 {
        if !(grouped.contains_key(&start)
            && grouped.contains_key(&(start + 1))
            && grouped.contains_key(&(start + 2)))
        {
            continue;
        }
        for suit1 in grouped.get(&start).into_iter().flatten() {
            for suit2 in grouped.get(&(start + 1)).into_iter().flatten() {
                for suit3 in grouped.get(&(start + 2)).into_iter().flatten() {
                    if HashSet::from([*suit1, *suit2, *suit3]).len() == 3 {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn has_pure_shifted_chows(context: &FanContext) -> bool {
    sequence_start_counts_by_suit(context)
        .values()
        .any(|starts| {
            let unique = starts.keys().copied().collect::<Vec<_>>();
            for step in [1, 2] {
                for start in &unique {
                    if (0..3).all(|offset| {
                        starts.get(&(start + offset * step)).copied().unwrap_or(0) >= 1
                    }) {
                        return true;
                    }
                }
            }
            false
        })
}

fn has_four_pure_shifted_chows(context: &FanContext) -> bool {
    sequence_start_counts_by_suit(context)
        .values()
        .any(|starts| {
            let unique = starts.keys().copied().collect::<Vec<_>>();
            for step in [1, 2] {
                for start in &unique {
                    if (0..4).all(|offset| {
                        starts.get(&(start + offset * step)).copied().unwrap_or(0) >= 1
                    }) {
                        return true;
                    }
                }
            }
            false
        })
}

fn has_pure_triple_chow(context: &FanContext) -> bool {
    sequence_start_counts_by_suit(context)
        .values()
        .any(|counts| counts.values().any(|count| *count >= 3))
}

fn has_quadruple_chow(context: &FanContext) -> bool {
    sequence_start_counts_by_suit(context)
        .values()
        .any(|counts| counts.values().any(|count| *count >= 4))
}

fn has_short_straight(context: &FanContext) -> bool {
    sequence_start_counts_by_suit(context)
        .values()
        .any(|starts| {
            (starts.contains_key(&1) && starts.contains_key(&4))
                || (starts.contains_key(&4) && starts.contains_key(&7))
        })
}
fn has_two_terminal_chows(context: &FanContext) -> bool {
    sequence_start_counts_by_suit(context)
        .values()
        .any(|starts| starts.contains_key(&1) && starts.contains_key(&7))
}
fn has_three_suited_terminal_chows(context: &FanContext) -> bool {
    let mut terminal_suits = HashSet::new();
    for (suit, starts) in sequence_start_counts_by_suit(context) {
        if starts.contains_key(&1) && starts.contains_key(&7) {
            terminal_suits.insert(suit);
        }
    }
    terminal_suits.len() >= 2
}
fn has_pure_terminal_chows(context: &FanContext) -> bool {
    for starts in sequence_start_counts_by_suit(context).values() {
        if starts.contains_key(&1)
            && starts.contains_key(&7)
            && starts.get(&1).copied().unwrap_or(0) >= 2
            && starts.get(&7).copied().unwrap_or(0) >= 2
        {
            return true;
        }
    }
    false
}
fn has_one_voided_suit(context: &FanContext) -> bool {
    context.all_tile_derived.suited_suits.len() == 2
}
fn has_no_honours(context: &FanContext) -> bool {
    !context.all_tile_derived.has_honours && !context.all_tile_keys.is_empty()
}
fn concealed_kong_count(context: &FanContext) -> usize {
    context
        .kong_entries
        .iter()
        .filter(|entry| {
            context.winner_seat.is_none() || Some(entry.actor_seat) == context.winner_seat
        })
        .filter(|entry| entry.kong_type == "concealed_kong")
        .count()
}
fn melded_kong_count(context: &FanContext) -> usize {
    context
        .kong_entries
        .iter()
        .filter(|entry| {
            context.winner_seat.is_none() || Some(entry.actor_seat) == context.winner_seat
        })
        .filter(|entry| matches!(entry.kong_type.as_str(), "exposed_kong" | "add_kong"))
        .count()
}
fn total_kong_count(context: &FanContext) -> usize {
    context
        .kong_entries
        .iter()
        .filter(|entry| {
            context.winner_seat.is_none() || Some(entry.actor_seat) == context.winner_seat
        })
        .count()
}
fn is_all_chows(context: &FanContext) -> bool {
    !context.standard_decompositions.is_empty()
        && !context.all_tile_keys.is_empty()
        && !context
            .all_tile_keys
            .iter()
            .any(|tile_key| HONOR_KEYS.contains(&tile_key.as_str()))
        && context.standard_decompositions.iter().any(|decomposition| {
            !decomposition.melds.is_empty()
                && decomposition
                    .melds
                    .iter()
                    .all(|meld| meld_is_sequence(meld))
        })
}
fn is_outside_hand(context: &FanContext) -> bool {
    context.standard_decompositions.iter().any(|decomposition| {
        decomposition
            .pair
            .as_deref()
            .map(is_terminal_or_honour)
            .unwrap_or(false)
            && !decomposition.melds.is_empty()
            && decomposition
                .melds
                .iter()
                .all(|meld| meld_has_terminal_or_honour(meld))
    })
}
fn is_seven_shifted_pairs_pattern(context: &FanContext) -> bool {
    let Some(pair_decomposition) = context
        .decompositions
        .iter()
        .find(|decomposition| decomposition.kind == "seven_pairs")
    else {
        return false;
    };
    if pair_decomposition.pairs.len() != 7 {
        return false;
    }
    if !pair_decomposition
        .pairs
        .iter()
        .all(|tile_key| parse_suit(tile_key).is_some())
    {
        return false;
    }
    let suits = pair_decomposition
        .pairs
        .iter()
        .filter_map(|tile_key| parse_suit(tile_key).map(|(prefix, _)| prefix))
        .collect::<HashSet<_>>();
    if suits.len() != 1 {
        return false;
    }
    let mut ranks = pair_decomposition
        .pairs
        .iter()
        .filter_map(|tile_key| parse_suit(tile_key).map(|(_, rank)| rank))
        .collect::<Vec<_>>();
    ranks.sort_unstable();
    ranks == (ranks[0]..ranks[0] + 7).collect::<Vec<_>>()
}
fn is_nine_gates(context: &FanContext) -> bool {
    if context.all_tile_keys.len() != 14 || context.all_tile_derived.has_honours {
        return false;
    }
    let Some(counts) = context.all_tile_derived.counts else {
        return false;
    };
    if context.all_tile_derived.suited_suits.len() != 1 {
        return false;
    }
    let suit_offset = match context.all_tile_derived.suited_suits.iter().next().copied() {
        Some('w') => 0,
        Some('t') => 9,
        Some('b') => 18,
        _ => return false,
    };
    let mut remaining = counts;
    for (offset, needed) in [
        (0_usize, 3_u8),
        (8, 3),
        (1, 1),
        (2, 1),
        (3, 1),
        (4, 1),
        (5, 1),
        (6, 1),
        (7, 1),
    ] {
        let index = suit_offset + offset;
        if remaining[index] < needed {
            return false;
        }
        remaining[index] -= needed;
    }
    total_tile_count(&remaining) == 1
}
fn has_decomposition_kind(context: &FanContext, kind: &str) -> bool {
    context
        .decompositions
        .iter()
        .any(|decomposition| decomposition.kind == kind)
}
fn triplet_keys_set(context: &FanContext) -> &HashSet<String> {
    &context.standard_derived.triplet_keys
}
fn pair_tile(context: &FanContext) -> Option<&str> {
    context.standard_derived.pair_tile.as_deref()
}
fn triplet_suits_by_rank(context: &FanContext) -> &HashMap<i32, HashSet<char>> {
    &context.standard_derived.triplet_suits_by_rank
}
fn triplet_rank_counts_by_suit(context: &FanContext) -> &HashMap<char, HashMap<i32, usize>> {
    &context.standard_derived.triplet_rank_counts_by_suit
}
fn triplet_rank_sets_by_suit(context: &FanContext) -> &HashMap<char, HashSet<i32>> {
    &context.standard_derived.triplet_rank_sets_by_suit
}
fn sequence_suits_by_start(context: &FanContext) -> &HashMap<i32, HashSet<char>> {
    &context.standard_derived.sequence_suits_by_start
}
fn sequence_start_counts_by_suit(context: &FanContext) -> &HashMap<char, HashMap<i32, usize>> {
    &context.standard_derived.sequence_start_counts_by_suit
}
fn has_triple_pung(context: &FanContext) -> bool {
    triplet_suits_by_rank(context)
        .values()
        .any(|suits| suits == &HashSet::from(['w', 't', 'b']))
}
fn has_double_pung(context: &FanContext) -> bool {
    triplet_suits_by_rank(context)
        .values()
        .any(|suits| suits.len() >= 2)
}
fn has_mixed_shifted_pungs(context: &FanContext) -> bool {
    let grouped = triplet_suits_by_rank(context);
    for rank in 1..=7 {
        if !(grouped.contains_key(&rank)
            && grouped.contains_key(&(rank + 1))
            && grouped.contains_key(&(rank + 2)))
        {
            continue;
        }
        for suit1 in grouped.get(&rank).into_iter().flatten() {
            for suit2 in grouped.get(&(rank + 1)).into_iter().flatten() {
                for suit3 in grouped.get(&(rank + 2)).into_iter().flatten() {
                    if HashSet::from([*suit1, *suit2, *suit3]).len() == 3 {
                        return true;
                    }
                }
            }
        }
    }
    false
}
fn has_pure_shifted_pungs(context: &FanContext) -> bool {
    triplet_rank_counts_by_suit(context).values().any(|counts| {
        let unique = counts.keys().copied().collect::<Vec<_>>();
        for step in [1, 2] {
            for rank in &unique {
                if (0..3)
                    .all(|offset| counts.get(&(rank + offset * step)).copied().unwrap_or(0) >= 1)
                {
                    return true;
                }
            }
        }
        false
    })
}
fn has_four_pure_shifted_pungs(context: &FanContext) -> bool {
    triplet_rank_sets_by_suit(context).values().any(|ranks| {
        ranks
            .iter()
            .any(|rank| (0..4).all(|offset| ranks.contains(&(rank + offset))))
    })
}
fn concealed_pung_count(context: &FanContext) -> usize {
    context.standard_derived.concealed_pung_count
}
fn resolve_wait_types(
    standard_decompositions: &[Decomposition],
    incoming_tile: Option<&str>,
    all_tile_keys: &[String],
) -> Vec<String> {
    let Some(incoming_tile) = incoming_tile else {
        return vec![];
    };
    if winning_tile_options(all_tile_keys, incoming_tile) != vec![incoming_tile.to_string()] {
        return vec![];
    }

    let mut wait_types = Vec::new();
    for decomposition in standard_decompositions {
        if decomposition.pair.as_deref() == Some(incoming_tile) {
            if !wait_types.iter().any(|wait| wait == "single_wait") {
                wait_types.push("single_wait".to_string());
            }
            continue;
        }
        for meld in &decomposition.melds {
            if !meld.iter().any(|tile_key| tile_key == incoming_tile) {
                continue;
            }
            if !meld.iter().all(|tile_key| parse_suit(tile_key).is_some()) {
                continue;
            }
            let mut ranks = meld
                .iter()
                .filter_map(|tile_key| parse_suit(tile_key).map(|(_, rank)| rank))
                .collect::<Vec<_>>();
            ranks.sort_unstable();
            let incoming_rank = parse_suit(incoming_tile).map(|(_, rank)| rank).unwrap_or(0);
            let next = if (ranks == [1, 2, 3] && incoming_rank == 3)
                || (ranks == [7, 8, 9] && incoming_rank == 7)
            {
                Some("edge_wait")
            } else if incoming_rank == ranks[1] {
                Some("closed_wait")
            } else {
                None
            };
            if let Some(wait_type) = next {
                if !wait_types.iter().any(|value| value == wait_type) {
                    wait_types.push(wait_type.to_string());
                }
            }
        }
    }
    if wait_types.len() == 1 {
        wait_types
    } else {
        vec![]
    }
}

fn winning_tile_options(all_tile_keys: &[String], incoming_tile: &str) -> Vec<String> {
    if all_tile_keys.len() != 14 {
        return vec![];
    }
    let Some(mut counts) = tile_counts_array(all_tile_keys.iter().map(String::as_str)) else {
        return vec![];
    };
    let Some(incoming_index) = tile_index(incoming_tile) else {
        return vec![];
    };
    if counts[incoming_index] == 0 {
        return vec![];
    }
    counts[incoming_index] -= 1;

    let mut winning_tiles = Vec::new();
    for tile_index in 0..TILE_KIND_COUNT {
        if counts[tile_index] >= 4 {
            continue;
        }
        counts[tile_index] += 1;
        if is_winning_hand_from_counts(&counts) {
            winning_tiles.push(tile_key_for_index(tile_index).to_string());
        }
        counts[tile_index] -= 1;
    }
    winning_tiles
}

fn standard_decompositions_from_counts(counts: &TileCounts) -> Vec<Decomposition> {
    let mut decompositions = Vec::new();
    let mut seen = HashSet::new();
    for pair_index in 0..TILE_KIND_COUNT {
        if counts[pair_index] < 2 {
            continue;
        }
        let mut next_counts = *counts;
        next_counts[pair_index] -= 2;
        let mut compact_results = Vec::new();
        let mut current = Vec::with_capacity(4);
        extract_all_melds(&mut next_counts, &mut current, &mut compact_results);
        for mut melds in compact_results {
            melds.sort_unstable();
            let signature = StandardDecompositionSignature {
                pair_index: pair_index as u8,
                melds: melds.clone(),
            };
            if !seen.insert(signature) {
                continue;
            }
            decompositions.push(Decomposition {
                kind: "standard".to_string(),
                pair: Some(tile_key_for_index(pair_index).to_string()),
                melds: compact_melds_to_tile_key_groups(&melds),
                ..Default::default()
            });
        }
    }
    decompositions
}

fn special_knitted_decompositions(counts: &TileCounts) -> Vec<Decomposition> {
    let mut decompositions = Vec::new();
    let mut seen = HashSet::new();
    let is_all_singletons = counts.iter().all(|count| *count <= 1);
    let honor_tiles = nonzero_tile_indices(counts)
        .filter(|index| is_honor_tile(*index))
        .map(|index| tile_key_for_index(index).to_string())
        .collect::<Vec<_>>();

    for pattern in KNITTED_PATTERNS {
        let pattern_indices = knitted_pattern_indices(&pattern);
        if pattern_indices.iter().all(|index| counts[*index] > 0) {
            let mut remaining = *counts;
            for index in pattern_indices {
                remaining[index] -= 1;
            }
            let remaining_honors = nonzero_tile_indices(&remaining)
                .filter(|index| is_honor_tile(*index))
                .map(|index| tile_key_for_index(index).to_string())
                .collect::<Vec<_>>();
            if total_tile_count(&remaining) == 5
                && nonzero_tile_indices(&remaining).all(is_honor_tile)
                && remaining_honors.len() == 5
                && nonzero_tile_indices(&remaining).all(|index| remaining[index] == 1)
            {
                let signature = format!(
                    "knitted_straight|{}|{}",
                    pattern.join(","),
                    remaining_honors.join(",")
                );
                if seen.insert(signature) {
                    decompositions.push(Decomposition {
                        kind: "knitted_straight".to_string(),
                        pattern_tiles: pattern
                            .iter()
                            .map(|tile_key| (*tile_key).to_string())
                            .collect(),
                        honor_tiles: remaining_honors,
                        completion_kind: Some("honours".to_string()),
                        ..Default::default()
                    });
                }
            }
            if let Some(completion) = five_tile_completion_detail(&remaining) {
                let meld_tile_keys = compact_meld_to_tile_keys(completion.meld);
                let signature = format!(
                    "knitted_straight|{}|{}|{}",
                    pattern.join(","),
                    tile_key_for_index(completion.pair_index as usize),
                    meld_tile_keys.join(",")
                );
                if seen.insert(signature) {
                    decompositions.push(Decomposition {
                        kind: "knitted_straight".to_string(),
                        pattern_tiles: pattern
                            .iter()
                            .map(|tile_key| (*tile_key).to_string())
                            .collect(),
                        pair: Some(tile_key_for_index(completion.pair_index as usize).to_string()),
                        meld: meld_tile_keys,
                        completion_kind: Some(completion.completion_kind.to_string()),
                        ..Default::default()
                    });
                }
            }
        }

        if !is_all_singletons {
            continue;
        }
        let suit_tiles = nonzero_tile_indices(counts)
            .filter(|index| !is_honor_tile(*index))
            .map(|index| tile_key_for_index(index).to_string())
            .collect::<Vec<_>>();
        if !suit_tiles
            .iter()
            .all(|tile_key| pattern.contains(&tile_key.as_str()))
        {
            continue;
        }

        let lesser_signature = format!("lesser|{}|{}", suit_tiles.join(","), honor_tiles.join(","));
        if honor_tiles.len() >= 5 && seen.insert(lesser_signature) {
            decompositions.push(Decomposition {
                kind: "lesser_honours_and_knitted_tiles".to_string(),
                pattern_tiles: suit_tiles.clone(),
                honor_tiles: honor_tiles.clone(),
                ..Default::default()
            });
        }
        if honor_tiles.len() == 7
            && HONOR_KEYS
                .iter()
                .all(|tile_key| honor_tiles.iter().any(|value| value == tile_key))
        {
            let signature = format!("greater|{}", suit_tiles.join(","));
            if seen.insert(signature) {
                decompositions.push(Decomposition {
                    kind: "greater_honours_and_knitted_tiles".to_string(),
                    pattern_tiles: suit_tiles.clone(),
                    honor_tiles: honor_tiles.clone(),
                    ..Default::default()
                });
            }
        }
    }
    decompositions
}

fn five_tile_completion_detail(counts: &TileCounts) -> Option<FiveTileCompletion> {
    if total_tile_count(counts) != 5 {
        return None;
    }
    for pair_index in 0..TILE_KIND_COUNT {
        if counts[pair_index] < 2 {
            continue;
        }
        let mut next_counts = *counts;
        next_counts[pair_index] -= 2;
        if let Some(meld) = single_meld(&next_counts) {
            return Some(FiveTileCompletion {
                pair_index: pair_index as u8,
                completion_kind: if is_triplet_meld(meld) {
                    "pung_and_pair"
                } else {
                    "chow_and_pair"
                },
                meld,
            });
        }
    }
    None
}

fn extract_all_melds(
    counts: &mut TileCounts,
    current: &mut Vec<CompactMeld>,
    results: &mut Vec<Vec<CompactMeld>>,
) {
    let Some(tile_index) = first_nonzero_tile_index(counts) else {
        results.push(current.clone());
        return;
    };
    if counts[tile_index] >= 3 {
        counts[tile_index] -= 3;
        current.push(triplet_meld(tile_index));
        extract_all_melds(counts, current, results);
        current.pop();
        counts[tile_index] += 3;
    }
    if let Some((second, third)) = sequence_tile_indices(tile_index) {
        if counts[second] > 0 && counts[third] > 0 {
            counts[tile_index] -= 1;
            counts[second] -= 1;
            counts[third] -= 1;
            current.push(sequence_meld(tile_index));
            extract_all_melds(counts, current, results);
            current.pop();
            counts[tile_index] += 1;
            counts[second] += 1;
            counts[third] += 1;
        }
    }
}

fn extract_first_melds(counts: &mut TileCounts, melds: &mut Vec<CompactMeld>) -> bool {
    let Some(tile_index) = first_nonzero_tile_index(counts) else {
        return true;
    };
    if counts[tile_index] >= 3 {
        counts[tile_index] -= 3;
        melds.push(triplet_meld(tile_index));
        if extract_first_melds(counts, melds) {
            return true;
        }
        melds.pop();
        counts[tile_index] += 3;
    }
    if let Some((second, third)) = sequence_tile_indices(tile_index) {
        if counts[second] > 0 && counts[third] > 0 {
            counts[tile_index] -= 1;
            counts[second] -= 1;
            counts[third] -= 1;
            melds.push(sequence_meld(tile_index));
            if extract_first_melds(counts, melds) {
                return true;
            }
            melds.pop();
            counts[tile_index] += 1;
            counts[second] += 1;
            counts[third] += 1;
        }
    }
    false
}

fn is_seven_pairs(counts: &TileCounts) -> bool {
    if total_tile_count(counts) != 14 {
        return false;
    }
    let mut pair_count = 0usize;
    for count in counts {
        if *count == 0 {
            continue;
        }
        if !matches!(*count, 2 | 4) {
            return false;
        }
        pair_count += usize::from(*count / 2);
    }
    pair_count == 7
}

fn seven_pairs_pair_tiles(counts: &TileCounts) -> Vec<String> {
    let mut pair_tiles = Vec::new();
    for (tile_index, count) in counts.iter().enumerate() {
        for _ in 0..usize::from(*count / 2) {
            pair_tiles.push(tile_key_for_index(tile_index).to_string());
        }
    }
    pair_tiles
}

fn is_thirteen_orphans(counts: &TileCounts) -> bool {
    if total_tile_count(counts) != 14 {
        return false;
    }
    let mut pair_count = 0usize;
    for (tile_index, count) in counts.iter().copied().enumerate().take(TILE_KIND_COUNT) {
        if count == 0 {
            continue;
        }
        if !THIRTEEN_ORPHAN_INDICES.contains(&tile_index) {
            return false;
        }
    }
    for tile_index in THIRTEEN_ORPHAN_INDICES {
        match counts[tile_index] {
            1 => {}
            2 => pair_count += 1,
            _ => return false,
        }
    }
    pair_count == 1
}

fn normalize_meld_tile_key_group(meld_tile_keys: &[String]) -> Option<Vec<String>> {
    if meld_tile_keys.len() == 3 {
        return Some(meld_tile_keys.to_vec());
    }
    if meld_tile_keys.len() == 4
        && meld_tile_keys
            .iter()
            .all(|tile_key| tile_key == &meld_tile_keys[0])
    {
        return Some(meld_tile_keys[0..3].to_vec());
    }
    None
}

fn tile_counts_array<'a>(tile_keys: impl Iterator<Item = &'a str>) -> Option<TileCounts> {
    let mut counts = [0_u8; TILE_KIND_COUNT];
    for tile_key in tile_keys {
        let tile_index = tile_index(tile_key)?;
        counts[tile_index] = counts[tile_index].saturating_add(1);
    }
    Some(counts)
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
    STANDARD_WIN_TILE_KEYS
        .get(tile_index)
        .copied()
        .unwrap_or_default()
}

fn total_tile_count(counts: &TileCounts) -> usize {
    counts.iter().map(|count| usize::from(*count)).sum()
}

fn first_nonzero_tile_index(counts: &TileCounts) -> Option<usize> {
    counts.iter().position(|count| *count > 0)
}

fn nonzero_tile_indices(counts: &TileCounts) -> impl Iterator<Item = usize> + '_ {
    counts
        .iter()
        .enumerate()
        .filter(|(_, count)| **count > 0)
        .map(|(index, _)| index)
}

fn is_honor_tile(tile_index: usize) -> bool {
    tile_index >= HONOR_TILE_START
}

fn sequence_tile_indices(tile_index: usize) -> Option<(usize, usize)> {
    if tile_index >= HONOR_TILE_START || tile_index % 9 >= 7 {
        return None;
    }
    Some((tile_index + 1, tile_index + 2))
}

fn triplet_meld(tile_index: usize) -> CompactMeld {
    CompactMeld([tile_index as u8, tile_index as u8, tile_index as u8])
}

fn sequence_meld(tile_index: usize) -> CompactMeld {
    CompactMeld([
        tile_index as u8,
        (tile_index + 1) as u8,
        (tile_index + 2) as u8,
    ])
}

fn is_triplet_meld(meld: CompactMeld) -> bool {
    meld.0[0] == meld.0[1]
}

fn compact_meld_to_tile_keys(meld: CompactMeld) -> Vec<String> {
    meld.0
        .iter()
        .map(|tile_index| tile_key_for_index(usize::from(*tile_index)).to_string())
        .collect()
}

fn compact_melds_to_tile_key_groups(melds: &[CompactMeld]) -> Vec<Vec<String>> {
    melds
        .iter()
        .copied()
        .map(compact_meld_to_tile_keys)
        .collect()
}

fn first_standard_decomposition_from_counts(counts: &TileCounts) -> Option<Decomposition> {
    for pair_index in 0..TILE_KIND_COUNT {
        if counts[pair_index] < 2 {
            continue;
        }
        let mut next_counts = *counts;
        next_counts[pair_index] -= 2;
        let mut melds = Vec::with_capacity(4);
        if extract_first_melds(&mut next_counts, &mut melds) {
            return Some(Decomposition {
                kind: "standard".to_string(),
                pair: Some(tile_key_for_index(pair_index).to_string()),
                melds: compact_melds_to_tile_key_groups(&melds),
                ..Default::default()
            });
        }
    }
    None
}

fn has_standard_winning_hand(counts: &TileCounts) -> bool {
    for pair_index in 0..TILE_KIND_COUNT {
        if counts[pair_index] < 2 {
            continue;
        }
        let mut next_counts = *counts;
        next_counts[pair_index] -= 2;
        let mut melds = Vec::with_capacity(4);
        if extract_first_melds(&mut next_counts, &mut melds) {
            return true;
        }
    }
    false
}

fn has_special_knitted_winning(counts: &TileCounts) -> bool {
    let is_all_singletons = counts.iter().all(|count| *count <= 1);
    let honour_count = nonzero_tile_indices(counts)
        .filter(|index| is_honor_tile(*index))
        .count();
    let suited_indices = nonzero_tile_indices(counts)
        .filter(|index| !is_honor_tile(*index))
        .collect::<Vec<_>>();

    for pattern in KNITTED_PATTERNS {
        let pattern_indices = knitted_pattern_indices(&pattern);
        if pattern_indices.iter().all(|index| counts[*index] > 0) {
            let mut remaining = *counts;
            for index in pattern_indices {
                remaining[index] -= 1;
            }
            let remaining_nonzero = nonzero_tile_indices(&remaining).collect::<Vec<_>>();
            if total_tile_count(&remaining) == 5
                && remaining_nonzero.len() == 5
                && remaining_nonzero
                    .iter()
                    .all(|index| is_honor_tile(*index) && remaining[*index] == 1)
            {
                return true;
            }
            if five_tile_completion_detail(&remaining).is_some() {
                return true;
            }
        }

        if !is_all_singletons {
            continue;
        }
        if suited_indices
            .iter()
            .all(|index| pattern_indices.contains(index))
        {
            if honour_count >= 5 {
                return true;
            }
            if honour_count == 7
                && (HONOR_TILE_START..TILE_KIND_COUNT).all(|index| counts[index] == 1)
            {
                return true;
            }
        }
    }
    false
}

fn is_winning_hand_from_counts(counts: &TileCounts) -> bool {
    total_tile_count(counts) == 14
        && (is_seven_pairs(counts)
            || is_thirteen_orphans(counts)
            || has_special_knitted_winning(counts)
            || has_standard_winning_hand(counts))
}

fn knitted_pattern_indices(pattern: &[&str; 9]) -> [usize; 9] {
    let mut indices = [0_usize; 9];
    for (slot, tile_key) in pattern.iter().enumerate() {
        indices[slot] = tile_index(tile_key).expect("knitted pattern tile keys should be valid");
    }
    indices
}

fn single_meld(counts: &TileCounts) -> Option<CompactMeld> {
    if total_tile_count(counts) != 3 {
        return None;
    }
    let tile_index = first_nonzero_tile_index(counts)?;
    if counts[tile_index] == 3 {
        return Some(triplet_meld(tile_index));
    }
    if let Some((second, third)) = sequence_tile_indices(tile_index) {
        if counts[tile_index] == 1 && counts[second] == 1 && counts[third] == 1 {
            return Some(sequence_meld(tile_index));
        }
    }
    None
}

fn sequence_meld_start(meld_tile_keys: &[String]) -> Option<(char, i32)> {
    if meld_tile_keys.len() != 3 {
        return None;
    }
    let mut parsed = meld_tile_keys
        .iter()
        .filter_map(|tile_key| parse_suit(tile_key))
        .collect::<Vec<_>>();
    if parsed.len() != 3 {
        return None;
    }
    parsed.sort_by(|left, right| left.1.cmp(&right.1));
    let suit = parsed[0].0;
    if parsed.iter().all(|(current_suit, _)| *current_suit == suit)
        && parsed[0].1 + 1 == parsed[1].1
        && parsed[1].1 + 1 == parsed[2].1
    {
        return Some((suit, parsed[0].1));
    }
    None
}

fn meld_is_sequence(meld_tile_keys: &[String]) -> bool {
    if meld_tile_keys.len() < 3 {
        return false;
    }
    if meld_tile_keys
        .iter()
        .any(|tile_key| parse_suit(tile_key).is_none())
    {
        return false;
    }
    let mut parsed = meld_tile_keys
        .iter()
        .take(3)
        .filter_map(|tile_key| parse_suit(tile_key))
        .collect::<Vec<_>>();
    parsed.sort_by(|left, right| left.1.cmp(&right.1));
    parsed[0].0 == parsed[1].0
        && parsed[1].0 == parsed[2].0
        && parsed[0].1 + 1 == parsed[1].1
        && parsed[1].1 + 1 == parsed[2].1
}

fn meld_has_terminal_or_honour(meld: &[String]) -> bool {
    if meld.len() != 3 {
        return false;
    }
    if meld.iter().all(|tile_key| tile_key == &meld[0]) {
        return is_terminal_or_honour(&meld[0]);
    }
    meld.iter().any(|tile_key| is_terminal(tile_key))
}

fn is_simple_tile(tile_key: &str) -> bool {
    parse_suit(tile_key)
        .map(|(_, rank)| (2..=8).contains(&rank))
        .unwrap_or(false)
}
fn is_terminal_suit_tile(tile_key: &str) -> bool {
    parse_suit(tile_key)
        .map(|(_, rank)| matches!(rank, 1 | 9))
        .unwrap_or(false)
}
fn is_terminal_or_honour(tile_key: &str) -> bool {
    is_terminal_suit_tile(tile_key) || parse_suit(tile_key).is_none()
}
fn is_terminal(tile_key: &str) -> bool {
    parse_suit(tile_key)
        .map(|(_, rank)| matches!(rank, 1 | 9))
        .unwrap_or(false)
}

fn parse_suit(tile_key: &str) -> Option<(char, i32)> {
    let mut chars = tile_key.chars();
    let prefix = chars.next()?;
    if !SUIT_KEYS.contains(&prefix) {
        return None;
    }
    let rank = tile_key.get(1..)?.parse::<i32>().ok()?;
    if !(1..=9).contains(&rank) {
        return None;
    }
    Some((prefix, rank))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_big_three_dragons() {
        let tile_keys = vec![
            "red", "red", "red", "green", "green", "green", "white", "white", "white", "w1", "w1",
            "w1", "w9", "w9",
        ]
        .into_iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
        let decompositions = vec![Decomposition {
            kind: "standard".to_string(),
            pair: Some("w9".to_string()),
            melds: vec![
                vec!["red".to_string(), "red".to_string(), "red".to_string()],
                vec![
                    "green".to_string(),
                    "green".to_string(),
                    "green".to_string(),
                ],
                vec![
                    "white".to_string(),
                    "white".to_string(),
                    "white".to_string(),
                ],
                vec!["w1".to_string(), "w1".to_string(), "w1".to_string()],
            ],
            ..Default::default()
        }];
        let features = extract_hand_features(
            &tile_keys,
            &[],
            None,
            None,
            Some("east"),
            Some("east"),
            Some(&decompositions),
        );
        let result = evaluate_fans(EvaluationInput {
            win_type: "discard".to_string(),
            winner_seat: Some(0),
            discarder_seat: Some(1),
            flower_count: 0,
            seat_count: 4,
            features,
            timing: TimingFeatures::default(),
            kong_entries: vec![],
            tile_keys,
            visible_tile_keys: vec![],
            concealed_tile_keys: vec![],
            meld_tile_key_groups: vec![],
            open_meld_tile_key_groups: vec![],
            incoming_tile: None,
            decompositions,
        });

        assert!(result.fan_keys.iter().any(|fan| fan == "big_three_dragons"));
    }

    #[test]
    fn scores_nine_gates() {
        let tile_keys = vec![
            "w1", "w1", "w1", "w2", "w3", "w4", "w5", "w5", "w6", "w7", "w8", "w9", "w9", "w9",
        ]
        .into_iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
        let decompositions = decompose_winning_hand(&tile_keys);
        let features = extract_hand_features(
            &tile_keys,
            &[],
            None,
            None,
            Some("east"),
            Some("east"),
            Some(&decompositions),
        );
        let result = evaluate_fans(EvaluationInput {
            win_type: "self_draw".to_string(),
            winner_seat: Some(0),
            discarder_seat: None,
            flower_count: 0,
            seat_count: 4,
            features,
            timing: TimingFeatures::default(),
            kong_entries: vec![],
            tile_keys,
            visible_tile_keys: vec![],
            concealed_tile_keys: vec![],
            meld_tile_key_groups: vec![],
            open_meld_tile_key_groups: vec![],
            incoming_tile: None,
            decompositions,
        });

        assert!(result.fan_keys.iter().any(|fan| fan == "nine_gates"));
    }

    #[test]
    fn awards_chicken_hand_when_no_other_non_flower_fan_exists() {
        let tile_keys = vec![
            "w1", "w2", "w3", "t4", "t5", "t6", "b3", "b4", "b5", "w6", "w7", "w8", "red", "red",
        ]
        .into_iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
        let decompositions = vec![Decomposition {
            kind: "standard".to_string(),
            pair: Some("red".to_string()),
            melds: vec![
                vec!["w1".to_string(), "w2".to_string(), "w3".to_string()],
                vec!["t4".to_string(), "t5".to_string(), "t6".to_string()],
                vec!["b3".to_string(), "b4".to_string(), "b5".to_string()],
                vec!["w6".to_string(), "w7".to_string(), "w8".to_string()],
            ],
            ..Default::default()
        }];
        let result = evaluate_fans(EvaluationInput {
            win_type: "discard".to_string(),
            winner_seat: Some(0),
            discarder_seat: Some(1),
            flower_count: 0,
            seat_count: 4,
            features: HandFeatures::default(),
            timing: TimingFeatures::default(),
            kong_entries: vec![],
            tile_keys,
            visible_tile_keys: vec![],
            concealed_tile_keys: vec![],
            meld_tile_key_groups: vec![],
            open_meld_tile_key_groups: vec![],
            incoming_tile: None,
            decompositions,
        });

        assert!(result.fan_keys.iter().any(|fan| fan == "chicken_hand"));
    }

    #[test]
    fn repeated_precheck_calls_return_identical_results() {
        let tile_keys = vec![
            "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "red", "red", "red", "green",
            "green",
        ]
        .into_iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();

        let first_decompositions = decompose_winning_hand(&tile_keys);
        let second_decompositions = decompose_winning_hand(&tile_keys);
        assert_eq!(first_decompositions, second_decompositions);

        let first_features = extract_hand_features(
            &tile_keys,
            &[],
            None,
            None,
            Some("east"),
            Some("east"),
            Some(&first_decompositions),
        );
        let second_features = extract_hand_features(
            &tile_keys,
            &[],
            None,
            None,
            Some("east"),
            Some("east"),
            Some(&second_decompositions),
        );
        assert_eq!(first_features, second_features);

        let input = EvaluationInput {
            win_type: "discard".to_string(),
            winner_seat: Some(0),
            discarder_seat: Some(1),
            flower_count: 0,
            seat_count: 4,
            features: first_features,
            timing: TimingFeatures::default(),
            kong_entries: vec![],
            tile_keys,
            visible_tile_keys: vec![],
            concealed_tile_keys: vec![],
            meld_tile_key_groups: vec![],
            open_meld_tile_key_groups: vec![],
            incoming_tile: None,
            decompositions: first_decompositions,
        };

        let first_result = evaluate_fans(input.clone());
        let second_result = evaluate_fans(input);
        assert_eq!(first_result, second_result);
    }

    #[test]
    fn evaluate_fans_is_stable_for_reordered_equivalent_inputs() {
        let ordered_tile_keys = vec![
            "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "red", "red", "red", "green",
            "green",
        ]
        .into_iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
        let shuffled_tile_keys = vec![
            "green", "w9", "red", "w3", "w5", "red", "w7", "green", "w2", "w6", "w8", "w1", "red",
            "w4",
        ]
        .into_iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();

        let ordered_decompositions = decompose_winning_hand(&ordered_tile_keys);
        let shuffled_decompositions = decompose_winning_hand(&shuffled_tile_keys);
        let ordered_features = extract_hand_features(
            &ordered_tile_keys,
            &[],
            None,
            None,
            Some("east"),
            Some("east"),
            Some(&ordered_decompositions),
        );
        let shuffled_features = extract_hand_features(
            &shuffled_tile_keys,
            &[],
            None,
            None,
            Some("east"),
            Some("east"),
            Some(&shuffled_decompositions),
        );

        let ordered_result = evaluate_fans(EvaluationInput {
            win_type: "discard".to_string(),
            winner_seat: Some(0),
            discarder_seat: Some(1),
            flower_count: 0,
            seat_count: 4,
            features: ordered_features,
            timing: TimingFeatures::default(),
            kong_entries: vec![],
            tile_keys: ordered_tile_keys,
            visible_tile_keys: vec![],
            concealed_tile_keys: vec![],
            meld_tile_key_groups: vec![],
            open_meld_tile_key_groups: vec![],
            incoming_tile: None,
            decompositions: ordered_decompositions,
        });
        let shuffled_result = evaluate_fans(EvaluationInput {
            win_type: "discard".to_string(),
            winner_seat: Some(0),
            discarder_seat: Some(1),
            flower_count: 0,
            seat_count: 4,
            features: shuffled_features,
            timing: TimingFeatures::default(),
            kong_entries: vec![],
            tile_keys: shuffled_tile_keys,
            visible_tile_keys: vec![],
            concealed_tile_keys: vec![],
            meld_tile_key_groups: vec![],
            open_meld_tile_key_groups: vec![],
            incoming_tile: None,
            decompositions: shuffled_decompositions,
        });

        assert_eq!(ordered_result, shuffled_result);
    }

    #[test]
    fn does_not_treat_all_melds_as_open_when_open_groups_are_empty() {
        let concealed_tile_keys = vec!["red", "red"]
            .into_iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let meld_tile_key_groups = vec![
            vec!["w1", "w2", "w3"],
            vec!["w4", "w5", "w6"],
            vec!["t1", "t2", "t3"],
            vec!["b4", "b5", "b6"],
        ]
        .into_iter()
        .map(|meld| {
            meld.into_iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
        let tile_keys = concealed_tile_keys
            .iter()
            .cloned()
            .chain(
                meld_tile_key_groups
                    .iter()
                    .flat_map(|meld| meld.iter().cloned()),
            )
            .collect::<Vec<_>>();
        let decompositions = vec![Decomposition {
            kind: "standard".to_string(),
            pair: Some("red".to_string()),
            melds: meld_tile_key_groups.clone(),
            ..Default::default()
        }];

        let result = evaluate_fans(EvaluationInput {
            win_type: "discard".to_string(),
            winner_seat: Some(0),
            discarder_seat: Some(1),
            flower_count: 0,
            seat_count: 4,
            features: HandFeatures::default(),
            timing: TimingFeatures::default(),
            kong_entries: vec![],
            tile_keys,
            visible_tile_keys: vec![],
            concealed_tile_keys,
            meld_tile_key_groups,
            open_meld_tile_key_groups: vec![],
            incoming_tile: Some("red".to_string()),
            decompositions,
        });

        assert!(!result.fan_keys.iter().any(|fan| fan == "melded_hand"));
    }
}
