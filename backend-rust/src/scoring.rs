use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
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

#[derive(Clone, Debug, Default)]
pub struct HandFeatures {
    pub concealed_hand: bool,
    pub thirteen_orphans: bool,
    pub seven_pairs: bool,
    pub pung_hand: bool,
    pub mixed_one_suit: bool,
    pub pure_one_suit: bool,
    pub ping_hu: bool,
    pub yi_ban_gao: bool,
    pub duan_yao: bool,
    pub hun_yao_jiu: bool,
    pub qing_yao_jiu: bool,
    pub triplet_keys: Vec<String>,
    pub seat_wind_triplet: bool,
    pub round_wind_triplet: bool,
    pub dragon_triplet_count: usize,
    pub terminal_triplet_count: usize,
    pub non_seat_non_round_wind_triplet_count: usize,
}

#[derive(Clone, Debug, Default)]
pub struct TimingFeatures {
    pub gang_shang_hua: bool,
    pub hai_di_lao_yue: bool,
    pub he_di_lao_yu: bool,
    pub robbing_the_kong: bool,
}

#[derive(Clone, Debug, Default)]
pub struct KongEntry {
    pub kong_type: String,
    pub actor_seat: usize,
    pub payer_seats: Vec<usize>,
    pub tile_key: Option<String>,
}

#[derive(Clone, Debug)]
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
    pub seat_wind_key: Option<String>,
    pub round_wind_key: Option<String>,
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

impl FanResult {
    pub fn score_delta_json(&self) -> Value {
        json!({
            "provisional": self.score_delta.provisional,
            "basic_points": self.score_delta.basic_points,
            "base_points": self.score_delta.base_points,
            "fan_total": self.score_delta.fan_total,
            "minimum_qualifying_fan_total": self.score_delta.minimum_qualifying_fan_total,
            "fan_delta_by_seat": score_map_value(&self.score_delta.fan_delta_by_seat),
            "kong_delta_by_seat": score_map_value(&self.score_delta.kong_delta_by_seat),
            "total_delta_by_seat": score_map_value(&self.score_delta.total_delta_by_seat),
        })
    }

    pub fn kong_score_detail_json(&self) -> Value {
        Value::Array(
            self.kong_score_detail
                .iter()
                .map(|entry| {
                    json!({
                        "kong_type": entry.kong_type,
                        "actor_seat": entry.actor_seat,
                        "payer_seats": entry.payer_seats,
                        "delta_by_seat": score_map_value(&entry.delta_by_seat),
                    })
                })
                .collect(),
        )
    }
}

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
    meld_tile_key_groups: Vec<Vec<String>>,
    open_meld_tile_key_groups: Vec<Vec<String>>,
    decompositions: Vec<Decomposition>,
    seat_wind_key: Option<String>,
    round_wind_key: Option<String>,
    standard_decompositions: Vec<Decomposition>,
    all_tile_keys: Vec<String>,
    wait_types: Vec<String>,
    winning_tile: Option<String>,
}

impl FanContext {
    fn from_input(input: EvaluationInput) -> Self {
        let decompositions = if input.decompositions.is_empty() && !input.tile_keys.is_empty() {
            decompose_winning_hand(&input.tile_keys)
        } else {
            input.decompositions.clone()
        };
        let standard_decompositions = decompositions
            .iter()
            .filter(|decomposition| decomposition.kind == "standard")
            .cloned()
            .collect::<Vec<_>>();
        let wait_types = resolve_wait_types(
            &standard_decompositions,
            input.incoming_tile.as_deref(),
            &input.tile_keys,
        );
        Self {
            win_type: input.win_type,
            winner_seat: input.winner_seat,
            discarder_seat: input.discarder_seat,
            flower_count: input.flower_count,
            seat_count: input.seat_count.max(1),
            features: input.features,
            timing: input.timing,
            kong_entries: input.kong_entries,
            visible_tile_keys: input.visible_tile_keys,
            concealed_tile_keys: input.concealed_tile_keys,
            meld_tile_key_groups: input.meld_tile_key_groups.clone(),
            open_meld_tile_key_groups: if input.open_meld_tile_key_groups.is_empty() {
                input.meld_tile_key_groups
            } else {
                input.open_meld_tile_key_groups
            },
            decompositions,
            seat_wind_key: input.seat_wind_key,
            round_wind_key: input.round_wind_key,
            standard_decompositions,
            all_tile_keys: input.tile_keys,
            wait_types,
            winning_tile: input.incoming_tile,
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
        next
    }
}

#[derive(Clone)]
struct FanRule {
    fan_key: &'static str,
    fan_value: i64,
    matcher: fn(&FanContext) -> usize,
    value_resolver: Option<fn(&FanContext, usize, i64) -> Vec<i64>>,
    excludes: &'static [&'static str],
    forbidden_with: &'static [&'static str],
}

#[derive(Clone)]
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

pub fn evaluate_fans(input: EvaluationInput) -> FanResult {
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
    let mut effective_concealed = concealed_tile_keys.to_vec();
    if let Some(tile) = incoming_tile {
        effective_concealed.push(tile.to_string());
    }

    let mut all_tile_keys = effective_concealed.clone();
    for meld_group in meld_tile_key_groups {
        all_tile_keys.extend(meld_group.clone());
    }

    let sequence_groups = extract_sequences(&effective_concealed, decompositions);
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
        ping_hu: is_ping_hu(&effective_concealed, meld_tile_key_groups, decompositions),
        yi_ban_gao: has_yi_ban_gao(&sequence_groups),
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
        triplet_keys,
        seat_wind_triplet,
        round_wind_triplet,
    }
}

pub fn decompose_winning_hand(tile_keys: &[String]) -> Vec<Decomposition> {
    if tile_keys.len() != 14 {
        return vec![];
    }
    let counts = tile_counts(tile_keys.iter().map(String::as_str));
    let mut decompositions = Vec::new();
    if is_seven_pairs(&counts) {
        decompositions.push(Decomposition {
            kind: "seven_pairs".to_string(),
            pairs: seven_pairs_pair_tiles(&counts),
            ..Default::default()
        });
    }
    if is_thirteen_orphans(&counts) {
        let pair_tile = counts
            .iter()
            .find(|(_, count)| **count == 2)
            .map(|(tile_key, _)| tile_key.clone())
            .unwrap_or_default();
        decompositions.push(Decomposition {
            kind: "thirteen_orphans".to_string(),
            pair: Some(pair_tile),
            orphans: counts.keys().cloned().collect(),
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

    let base = standard_decompositions_from_counts(&tile_counts(
        concealed_tile_keys.iter().map(String::as_str),
    ));
    base.into_iter()
        .map(|mut decomposition| {
            let mut melds = normalized.clone();
            melds.extend(decomposition.melds.clone());
            decomposition.melds = melds;
            decomposition
        })
        .collect()
}

pub fn is_winning_hand(tile_keys: &[String]) -> bool {
    !decompose_winning_hand(tile_keys).is_empty()
}

pub fn is_winning_hand_with_melds(
    concealed_tile_keys: &[String],
    meld_tile_key_groups: &[Vec<String>],
) -> bool {
    !decompose_winning_hand_with_melds(concealed_tile_keys, meld_tile_key_groups).is_empty()
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
    let mut candidates = Vec::new();
    for (order, rule) in registered_fan_rules().into_iter().enumerate() {
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

        let candidate = &ordered[index];
        if blocked_keys.contains(candidate.fan_key) {
            return;
        }
        let conflicts = candidate
            .excludes
            .iter()
            .chain(candidate.forbidden_with.iter())
            .copied()
            .collect::<Vec<_>>();
        if conflicts
            .iter()
            .any(|conflict| selected_keys.contains(conflict))
        {
            return;
        }

        selected.push(candidate.clone());
        selected_keys.insert(candidate.fan_key);
        let inserted_blocked = conflicts
            .iter()
            .filter(|conflict| blocked_keys.insert(**conflict))
            .copied()
            .collect::<Vec<_>>();
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
        for seat in 0..seat_count {
            totals[seat] += entry.delta_by_seat.get(seat).copied().unwrap_or(0);
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
        for seat in 0..seat_count {
            if seat == winner_seat {
                continue;
            }
            deltas[seat] -= payment;
            deltas[winner_seat] += payment;
        }
        return deltas;
    }

    if let Some(discarder_seat) = discarder_seat {
        deltas[winner_seat] +=
            fan_total + (MCR_BASE_POINTS * (seat_count.saturating_sub(1) as i64));
        for seat in 0..seat_count {
            if seat == winner_seat {
                continue;
            }
            if seat == discarder_seat {
                deltas[seat] -= fan_total + MCR_BASE_POINTS;
            } else {
                deltas[seat] -= MCR_BASE_POINTS;
            }
        }
    }
    deltas
}

fn should_award_chicken_hand(context: &FanContext, fan_keys: &[String]) -> bool {
    if context.all_tile_keys.len() != 14 {
        return false;
    }
    !fan_keys.iter().any(|fan_key| fan_key != "flower_tiles")
}

fn registered_fan_rules() -> Vec<FanRule> {
    vec![
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
    ]
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
                .as_deref()
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
                .as_deref()
                .map(|tile| WIND_KEYS.contains(&tile))
                .unwrap_or(false),
    )
}
fn match_all_honours(context: &FanContext) -> usize {
    usize::from(
        !context.all_tile_keys.is_empty()
            && context
                .all_tile_keys
                .iter()
                .all(|tile| HONOR_KEYS.contains(&tile.as_str())),
    )
}
fn match_all_terminals_and_honours(context: &FanContext) -> usize {
    let has_honours = context
        .all_tile_keys
        .iter()
        .any(|tile| HONOR_KEYS.contains(&tile.as_str()));
    let has_terminals = context.all_tile_keys.iter().any(|tile| is_terminal(tile));
    usize::from(
        !context.all_tile_keys.is_empty()
            && has_honours
            && has_terminals
            && context
                .all_tile_keys
                .iter()
                .all(|tile| HONOR_KEYS.contains(&tile.as_str()) || is_terminal(tile)),
    )
}
fn match_all_terminals(context: &FanContext) -> usize {
    usize::from(
        !context.all_tile_keys.is_empty()
            && context.all_tile_keys.iter().all(|tile| is_terminal(tile)),
    )
}
fn match_all_even_pungs(context: &FanContext) -> usize {
    usize::from(
        context.features.pung_hand
            && !context.all_tile_keys.is_empty()
            && context.all_tile_keys.iter().all(|tile| is_even_tile(tile)),
    )
}
fn match_all_green(context: &FanContext) -> usize {
    usize::from(
        !context.all_tile_keys.is_empty()
            && context
                .all_tile_keys
                .iter()
                .all(|tile| ALL_GREEN_KEYS.contains(&tile.as_str())),
    )
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
    is_seven_pairs(&tile_counts(tile_keys.iter().map(String::as_str)))
}

fn features_is_thirteen_orphans(
    tile_keys: &[String],
    meld_tile_key_groups: &[Vec<String>],
) -> bool {
    if !meld_tile_key_groups.is_empty() || tile_keys.len() != 14 {
        return false;
    }
    is_thirteen_orphans(&tile_counts(tile_keys.iter().map(String::as_str)))
}

fn features_is_pung_hand(tile_keys: &[String], meld_tile_key_groups: &[Vec<String>]) -> bool {
    if meld_tile_key_groups
        .iter()
        .any(|meld| meld_is_sequence(meld))
    {
        return false;
    }
    can_form_all_pungs(&tile_counts(tile_keys.iter().map(String::as_str)))
}

fn is_ping_hu(
    tile_keys: &[String],
    meld_tile_key_groups: &[Vec<String>],
    decompositions: Option<&[Decomposition]>,
) -> bool {
    if !meld_tile_key_groups.is_empty() || tile_keys.len() != 14 {
        return false;
    }
    standard_decomposition(tile_keys, decompositions)
        .map(|decomposition| {
            decomposition
                .melds
                .iter()
                .all(|meld| meld_is_sequence(meld))
        })
        .unwrap_or(false)
}

fn has_yi_ban_gao(sequence_groups: &[Vec<String>]) -> bool {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for sequence in sequence_groups {
        *counts.entry(sequence.join(",")).or_insert(0) += 1;
    }
    counts.values().any(|count| *count >= 2)
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

fn extract_sequences(
    tile_keys: &[String],
    decompositions: Option<&[Decomposition]>,
) -> Vec<Vec<String>> {
    standard_decomposition(tile_keys, decompositions)
        .map(|decomposition| {
            decomposition
                .melds
                .iter()
                .filter(|meld| meld_is_sequence(meld))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
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
    let counts = tile_counts(tile_keys.iter().map(String::as_str));
    counts
        .keys()
        .filter(|tile_key| counts.get(*tile_key).copied().unwrap_or(0) >= 2)
        .find_map(|tile_key| {
            let mut next_counts = counts.clone();
            decrement_count(&mut next_counts, tile_key, 2);
            extract_first_melds(&next_counts).map(|melds| Decomposition {
                kind: "standard".to_string(),
                pair: Some(tile_key.clone()),
                melds,
                ..Default::default()
            })
        })
}

fn extract_first_melds(counts: &BTreeMap<String, usize>) -> Option<Vec<Vec<String>>> {
    if counts.is_empty() {
        return Some(vec![]);
    }
    let tile_key = counts.keys().next()?.clone();
    let count = counts.get(&tile_key).copied().unwrap_or(0);
    if count == 0 {
        let mut next = counts.clone();
        next.remove(&tile_key);
        return extract_first_melds(&next);
    }
    if count >= 3 {
        let mut next = counts.clone();
        decrement_count(&mut next, &tile_key, 3);
        if let Some(mut melds) = extract_first_melds(&next) {
            let mut result = vec![vec![tile_key.clone(), tile_key.clone(), tile_key.clone()]];
            result.append(&mut melds);
            return Some(result);
        }
    }
    if let Some((prefix, rank)) = parse_suit(&tile_key) {
        if rank <= 7 {
            let second = format!("{prefix}{}", rank + 1);
            let third = format!("{prefix}{}", rank + 2);
            if counts.get(&second).copied().unwrap_or(0) > 0
                && counts.get(&third).copied().unwrap_or(0) > 0
            {
                let mut next = counts.clone();
                decrement_count(&mut next, &tile_key, 1);
                decrement_count(&mut next, &second, 1);
                decrement_count(&mut next, &third, 1);
                if let Some(mut melds) = extract_first_melds(&next) {
                    let mut result = vec![vec![tile_key, second, third]];
                    result.append(&mut melds);
                    return Some(result);
                }
            }
        }
    }
    None
}

fn can_form_all_pungs(counts: &BTreeMap<String, usize>) -> bool {
    if counts.values().sum::<usize>() % 3 != 2 {
        return false;
    }
    counts.iter().any(|(tile_key, count)| {
        if *count < 2 {
            return false;
        }
        let mut next = counts.clone();
        decrement_count(&mut next, tile_key, 2);
        next.values().all(|value| value % 3 == 0)
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
    let suits = context
        .all_tile_keys
        .iter()
        .filter_map(|tile_key| parse_suit(tile_key).map(|(prefix, _)| prefix))
        .collect::<HashSet<_>>();
    let has_wind = context
        .all_tile_keys
        .iter()
        .any(|tile_key| WIND_KEYS.contains(&tile_key.as_str()));
    let has_dragon = context
        .all_tile_keys
        .iter()
        .any(|tile_key| DRAGON_KEYS.contains(&tile_key.as_str()));
    suits == HashSet::from(['w', 't', 'b']) && has_wind && has_dragon
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
    !context.all_tile_keys.is_empty()
        && context.all_tile_keys.iter().all(|tile_key| {
            parse_suit(tile_key)
                .map(|(_, rank)| rank >= 6)
                .unwrap_or(false)
        })
}
fn is_upper_tiles(context: &FanContext) -> bool {
    !context.all_tile_keys.is_empty()
        && context.all_tile_keys.iter().all(|tile_key| {
            parse_suit(tile_key)
                .map(|(_, rank)| matches!(rank, 7..=9))
                .unwrap_or(false)
        })
}
fn is_lower_four(context: &FanContext) -> bool {
    !context.all_tile_keys.is_empty()
        && context.all_tile_keys.iter().all(|tile_key| {
            parse_suit(tile_key)
                .map(|(_, rank)| rank <= 4)
                .unwrap_or(false)
        })
}
fn is_lower_tiles(context: &FanContext) -> bool {
    !context.all_tile_keys.is_empty()
        && context.all_tile_keys.iter().all(|tile_key| {
            parse_suit(tile_key)
                .map(|(_, rank)| matches!(rank, 1..=3))
                .unwrap_or(false)
        })
}
fn is_middle_tiles(context: &FanContext) -> bool {
    !context.all_tile_keys.is_empty()
        && context.all_tile_keys.iter().all(|tile_key| {
            parse_suit(tile_key)
                .map(|(_, rank)| matches!(rank, 4..=6))
                .unwrap_or(false)
        })
}
fn has_tile_hog(context: &FanContext) -> bool {
    tile_counts(context.all_tile_keys.iter().map(String::as_str))
        .values()
        .any(|count| *count >= 4)
}
fn has_reversible_tiles(context: &FanContext) -> bool {
    !context.all_tile_keys.is_empty()
        && context
            .all_tile_keys
            .iter()
            .all(|tile_key| REVERSIBLE_TILE_KEYS.contains(&tile_key.as_str()))
}

fn has_pure_straight(context: &FanContext) -> bool {
    sequence_groups_by_suit(context).values().any(|sequences| {
        let starts = sequences
            .iter()
            .map(|(start, _)| *start)
            .collect::<HashSet<_>>();
        starts.contains(&1) && starts.contains(&4) && starts.contains(&7)
    })
}

fn has_mixed_triple_chow(context: &FanContext) -> bool {
    let mut grouped: HashMap<i32, HashSet<char>> = HashMap::new();
    for (suit, sequences) in sequence_groups_by_suit(context) {
        for (start, _) in sequences {
            grouped.entry(start).or_default().insert(suit);
        }
    }
    grouped
        .values()
        .any(|suits| suits == &HashSet::from(['w', 't', 'b']))
}

fn has_pure_double_chow(context: &FanContext) -> bool {
    sequence_groups_by_suit(context).values().any(|sequences| {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for (_, sequence) in sequences {
            *counts.entry(sequence.join(",")).or_insert(0) += 1;
        }
        counts.values().any(|count| *count >= 2)
    })
}

fn has_mixed_double_chow(context: &FanContext) -> bool {
    let mut grouped: HashMap<i32, HashSet<char>> = HashMap::new();
    for (suit, sequences) in sequence_groups_by_suit(context) {
        for (start, _) in sequences {
            grouped.entry(start).or_default().insert(suit);
        }
    }
    grouped.values().any(|suits| suits.len() >= 2)
}

fn has_mixed_straight(context: &FanContext) -> bool {
    let mut grouped: HashMap<i32, HashSet<char>> = HashMap::new();
    for (suit, sequences) in sequence_groups_by_suit(context) {
        for (start, _) in sequences {
            grouped.entry(start).or_default().insert(suit);
        }
    }
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
    let mut grouped: HashMap<i32, HashSet<char>> = HashMap::new();
    for (suit, sequences) in sequence_groups_by_suit(context) {
        for (start, _) in sequences {
            grouped.entry(start).or_default().insert(suit);
        }
    }
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
    sequence_groups_by_suit(context).values().any(|sequences| {
        let mut starts: HashMap<i32, usize> = HashMap::new();
        for (start, _) in sequences {
            *starts.entry(*start).or_insert(0) += 1;
        }
        let unique = starts.keys().copied().collect::<Vec<_>>();
        for step in [1, 2] {
            for start in &unique {
                if (0..3)
                    .all(|offset| starts.get(&(start + offset * step)).copied().unwrap_or(0) >= 1)
                {
                    return true;
                }
            }
        }
        false
    })
}

fn has_four_pure_shifted_chows(context: &FanContext) -> bool {
    sequence_groups_by_suit(context).values().any(|sequences| {
        let mut starts: HashMap<i32, usize> = HashMap::new();
        for (start, _) in sequences {
            *starts.entry(*start).or_insert(0) += 1;
        }
        let unique = starts.keys().copied().collect::<Vec<_>>();
        for step in [1, 2] {
            for start in &unique {
                if (0..4)
                    .all(|offset| starts.get(&(start + offset * step)).copied().unwrap_or(0) >= 1)
                {
                    return true;
                }
            }
        }
        false
    })
}

fn has_pure_triple_chow(context: &FanContext) -> bool {
    sequence_groups_by_suit(context).values().any(|sequences| {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for (_, sequence) in sequences {
            *counts.entry(sequence.join(",")).or_insert(0) += 1;
        }
        counts.values().any(|count| *count >= 3)
    })
}

fn has_quadruple_chow(context: &FanContext) -> bool {
    sequence_groups_by_suit(context).values().any(|sequences| {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for (_, sequence) in sequences {
            *counts.entry(sequence.join(",")).or_insert(0) += 1;
        }
        counts.values().any(|count| *count >= 4)
    })
}

fn has_short_straight(context: &FanContext) -> bool {
    sequence_groups_by_suit(context).values().any(|sequences| {
        let starts = sequences
            .iter()
            .map(|(start, _)| *start)
            .collect::<HashSet<_>>();
        (starts.contains(&1) && starts.contains(&4)) || (starts.contains(&4) && starts.contains(&7))
    })
}
fn has_two_terminal_chows(context: &FanContext) -> bool {
    sequence_groups_by_suit(context).values().any(|sequences| {
        let starts = sequences
            .iter()
            .map(|(start, _)| *start)
            .collect::<HashSet<_>>();
        starts.contains(&1) && starts.contains(&7)
    })
}
fn has_three_suited_terminal_chows(context: &FanContext) -> bool {
    let mut terminal_suits = HashSet::new();
    for (suit, sequences) in sequence_groups_by_suit(context) {
        let starts = sequences
            .iter()
            .map(|(start, _)| *start)
            .collect::<HashSet<_>>();
        if starts.contains(&1) && starts.contains(&7) {
            terminal_suits.insert(suit);
        }
    }
    terminal_suits.len() >= 2
}
fn has_pure_terminal_chows(context: &FanContext) -> bool {
    for (suit, sequences) in sequence_groups_by_suit(context) {
        let starts = sequences
            .iter()
            .map(|(start, _)| *start)
            .collect::<HashSet<_>>();
        let mut counts: HashMap<String, usize> = HashMap::new();
        for (_, sequence) in sequences {
            *counts.entry(sequence.join(",")).or_insert(0) += 1;
        }
        if starts.contains(&1)
            && starts.contains(&7)
            && counts
                .get(&format!("{suit}1,{suit}2,{suit}3"))
                .copied()
                .unwrap_or(0)
                >= 2
            && counts
                .get(&format!("{suit}7,{suit}8,{suit}9"))
                .copied()
                .unwrap_or(0)
                >= 2
        {
            return true;
        }
    }
    false
}
fn has_one_voided_suit(context: &FanContext) -> bool {
    let suits = context
        .all_tile_keys
        .iter()
        .filter_map(|tile_key| parse_suit(tile_key).map(|(prefix, _)| prefix))
        .collect::<HashSet<_>>();
    suits.len() == 2
}
fn has_no_honours(context: &FanContext) -> bool {
    !context.all_tile_keys.is_empty()
        && context
            .all_tile_keys
            .iter()
            .all(|tile_key| parse_suit(tile_key).is_some())
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
    if context.all_tile_keys.len() != 14
        || !context
            .all_tile_keys
            .iter()
            .all(|tile_key| parse_suit(tile_key).is_some())
    {
        return false;
    }
    let suits = context
        .all_tile_keys
        .iter()
        .filter_map(|tile_key| parse_suit(tile_key).map(|(prefix, _)| prefix))
        .collect::<HashSet<_>>();
    if suits.len() != 1 {
        return false;
    }
    let suit = *suits.iter().next().unwrap_or(&'w');
    let counts = tile_counts(context.all_tile_keys.iter().map(String::as_str));
    let mut remaining = counts.clone();
    let base = vec![
        (format!("{suit}1"), 3),
        (format!("{suit}9"), 3),
        (format!("{suit}2"), 1),
        (format!("{suit}3"), 1),
        (format!("{suit}4"), 1),
        (format!("{suit}5"), 1),
        (format!("{suit}6"), 1),
        (format!("{suit}7"), 1),
        (format!("{suit}8"), 1),
    ];
    for (tile_key, needed) in base {
        if remaining.get(&tile_key).copied().unwrap_or(0) < needed {
            return false;
        }
        decrement_count(&mut remaining, &tile_key, needed);
    }
    remaining.values().sum::<usize>() == 1
}
fn has_decomposition_kind(context: &FanContext, kind: &str) -> bool {
    context
        .decompositions
        .iter()
        .any(|decomposition| decomposition.kind == kind)
}
fn triplet_keys_set(context: &FanContext) -> HashSet<String> {
    context
        .standard_decompositions
        .iter()
        .flat_map(|decomposition| decomposition.melds.iter())
        .filter(|meld| meld.len() == 3 && meld.iter().all(|tile_key| tile_key == &meld[0]))
        .map(|meld| meld[0].clone())
        .collect()
}
fn pair_tile(context: &FanContext) -> Option<String> {
    context
        .standard_decompositions
        .iter()
        .find_map(|decomposition| decomposition.pair.clone())
}
fn suited_triplets(context: &FanContext) -> Vec<(char, i32)> {
    context
        .standard_decompositions
        .iter()
        .flat_map(|decomposition| decomposition.melds.iter())
        .filter(|meld| meld.len() == 3 && meld.iter().all(|tile_key| tile_key == &meld[0]))
        .filter_map(|meld| parse_suit(&meld[0]))
        .collect()
}
fn has_triple_pung(context: &FanContext) -> bool {
    let mut grouped: HashMap<i32, HashSet<char>> = HashMap::new();
    for (suit, rank) in suited_triplets(context) {
        grouped.entry(rank).or_default().insert(suit);
    }
    grouped
        .values()
        .any(|suits| suits == &HashSet::from(['w', 't', 'b']))
}
fn has_double_pung(context: &FanContext) -> bool {
    let mut grouped: HashMap<i32, HashSet<char>> = HashMap::new();
    for (suit, rank) in suited_triplets(context) {
        grouped.entry(rank).or_default().insert(suit);
    }
    grouped.values().any(|suits| suits.len() >= 2)
}
fn has_mixed_shifted_pungs(context: &FanContext) -> bool {
    let mut grouped: HashMap<i32, HashSet<char>> = HashMap::new();
    for (suit, rank) in suited_triplets(context) {
        grouped.entry(rank).or_default().insert(suit);
    }
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
    let mut grouped: HashMap<char, Vec<i32>> = HashMap::new();
    for (suit, rank) in suited_triplets(context) {
        grouped.entry(suit).or_default().push(rank);
    }
    grouped.values().any(|ranks| {
        let mut counts: HashMap<i32, usize> = HashMap::new();
        for rank in ranks {
            *counts.entry(*rank).or_insert(0) += 1;
        }
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
    let mut grouped: HashMap<char, HashSet<i32>> = HashMap::new();
    for (suit, rank) in suited_triplets(context) {
        grouped.entry(suit).or_default().insert(rank);
    }
    grouped.values().any(|ranks| {
        ranks
            .iter()
            .any(|rank| (0..4).all(|offset| ranks.contains(&(rank + offset))))
    })
}
fn concealed_pung_count(context: &FanContext) -> usize {
    let concealed_kongs = context
        .kong_entries
        .iter()
        .filter(|entry| Some(entry.actor_seat) == context.winner_seat)
        .filter(|entry| entry.kong_type == "concealed_kong")
        .count();
    let mut best_standard = 0usize;
    for decomposition in &context.standard_decompositions {
        let concealed_triplets = decomposition
            .melds
            .iter()
            .filter(|meld| meld.len() == 3 && meld.iter().all(|tile_key| tile_key == &meld[0]))
            .filter(|meld| {
                context
                    .concealed_tile_keys
                    .iter()
                    .filter(|tile_key| *tile_key == &meld[0])
                    .count()
                    >= 3
            })
            .count();
        best_standard = best_standard.max(concealed_triplets);
    }
    best_standard + concealed_kongs
}
fn sequence_groups_by_suit(context: &FanContext) -> HashMap<char, Vec<(i32, Vec<String>)>> {
    let mut grouped: HashMap<char, Vec<(i32, Vec<String>)>> = HashMap::new();
    for decomposition in &context.standard_decompositions {
        for meld in &decomposition.melds {
            if meld.len() != 3 || !meld.iter().all(|tile_key| parse_suit(tile_key).is_some()) {
                continue;
            }
            let Some((suit, _)) = parse_suit(&meld[0]) else {
                continue;
            };
            let mut ranks = meld
                .iter()
                .filter_map(|tile_key| parse_suit(tile_key).map(|(_, rank)| rank))
                .collect::<Vec<_>>();
            ranks.sort_unstable();
            if meld.iter().all(|tile_key| tile_key.starts_with(suit))
                && ranks == vec![ranks[0], ranks[0] + 1, ranks[0] + 2]
            {
                grouped
                    .entry(suit)
                    .or_default()
                    .push((ranks[0], meld.clone()));
            }
        }
    }
    grouped
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
            let next = if ranks == vec![1, 2, 3] && incoming_rank == 3 {
                Some("edge_wait")
            } else if ranks == vec![7, 8, 9] && incoming_rank == 7 {
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
    if all_tile_keys.len() != 14
        || !all_tile_keys
            .iter()
            .any(|tile_key| tile_key == incoming_tile)
    {
        return vec![];
    }
    let mut base_tile_keys = all_tile_keys.to_vec();
    let Some(index) = base_tile_keys
        .iter()
        .position(|tile_key| tile_key == incoming_tile)
    else {
        return vec![];
    };
    base_tile_keys.remove(index);

    let mut winning_tiles = Vec::new();
    for tile_key in STANDARD_WIN_TILE_KEYS {
        if base_tile_keys
            .iter()
            .filter(|current| current.as_str() == tile_key)
            .count()
            >= 4
        {
            continue;
        }
        let mut candidate = base_tile_keys.clone();
        candidate.push(tile_key.to_string());
        if is_winning_hand(&candidate) {
            winning_tiles.push(tile_key.to_string());
        }
    }
    winning_tiles
}

fn standard_decompositions_from_counts(counts: &BTreeMap<String, usize>) -> Vec<Decomposition> {
    let mut decompositions = Vec::new();
    let mut seen = HashSet::new();
    for (tile_key, count) in counts {
        if *count < 2 {
            continue;
        }
        let mut next_counts = counts.clone();
        decrement_count(&mut next_counts, tile_key, 2);
        for melds in extract_all_melds(&next_counts) {
            let mut canonical_melds = melds;
            canonical_melds.sort();
            let signature = format!(
                "{tile_key}|{}",
                canonical_melds
                    .iter()
                    .map(|meld| meld.join(","))
                    .collect::<Vec<_>>()
                    .join("|")
            );
            if seen.insert(signature) {
                decompositions.push(Decomposition {
                    kind: "standard".to_string(),
                    pair: Some(tile_key.clone()),
                    melds: canonical_melds,
                    ..Default::default()
                });
            }
        }
    }
    decompositions
}

fn special_knitted_decompositions(counts: &BTreeMap<String, usize>) -> Vec<Decomposition> {
    let mut decompositions = Vec::new();
    let mut seen = HashSet::new();
    let is_all_singletons = counts.values().all(|count| *count == 1);
    let honor_tiles = counts
        .keys()
        .filter(|tile_key| HONOR_KEYS.contains(&tile_key.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    for pattern in KNITTED_PATTERNS {
        if pattern
            .iter()
            .all(|tile_key| counts.get(*tile_key).copied().unwrap_or(0) >= 1)
        {
            let mut remaining = counts.clone();
            for tile_key in pattern {
                decrement_count(&mut remaining, tile_key, 1);
            }
            if !remaining.is_empty() {
                let remaining_honors = remaining
                    .keys()
                    .filter(|tile_key| HONOR_KEYS.contains(&tile_key.as_str()))
                    .cloned()
                    .collect::<Vec<_>>();
                if remaining
                    .keys()
                    .all(|tile_key| HONOR_KEYS.contains(&tile_key.as_str()))
                    && remaining.len() == 5
                    && remaining.values().all(|count| *count == 1)
                {
                    let signature = format!(
                        "knitted_straight|{}|{}",
                        pattern.join(","),
                        remaining.keys().cloned().collect::<Vec<_>>().join(",")
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
                    let signature = format!(
                        "knitted_straight|{}|{}|{}",
                        pattern.join(","),
                        completion.pair,
                        completion.meld.join(",")
                    );
                    if seen.insert(signature) {
                        decompositions.push(Decomposition {
                            kind: "knitted_straight".to_string(),
                            pattern_tiles: pattern
                                .iter()
                                .map(|tile_key| (*tile_key).to_string())
                                .collect(),
                            pair: Some(completion.pair),
                            meld: completion.meld,
                            completion_kind: Some(completion.completion_kind),
                            ..Default::default()
                        });
                    }
                }
            }
        }

        if !is_all_singletons {
            continue;
        }
        let suit_tiles = counts
            .keys()
            .filter(|tile_key| !HONOR_KEYS.contains(&tile_key.as_str()))
            .cloned()
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

struct FiveTileCompletion {
    pair: String,
    meld: Vec<String>,
    completion_kind: String,
}

fn five_tile_completion_detail(counts: &BTreeMap<String, usize>) -> Option<FiveTileCompletion> {
    if counts.values().sum::<usize>() != 5 {
        return None;
    }
    for (pair_tile, count) in counts {
        if *count < 2 {
            continue;
        }
        let mut next_counts = counts.clone();
        decrement_count(&mut next_counts, pair_tile, 2);
        if next_counts.len() == 1 {
            let (meld_tile, meld_count) = next_counts.iter().next()?;
            if *meld_count == 3 {
                return Some(FiveTileCompletion {
                    pair: pair_tile.clone(),
                    meld: vec![meld_tile.clone(), meld_tile.clone(), meld_tile.clone()],
                    completion_kind: "pung_and_pair".to_string(),
                });
            }
        }
        let melds = extract_all_melds(&next_counts);
        if let Some(meld) = melds.first().and_then(|entry| entry.first()) {
            return Some(FiveTileCompletion {
                pair: pair_tile.clone(),
                completion_kind: if meld.iter().collect::<HashSet<_>>().len() == 3 {
                    "chow_and_pair".to_string()
                } else {
                    "pung_and_pair".to_string()
                },
                meld: meld.clone(),
            });
        }
    }
    None
}

fn extract_all_melds(counts: &BTreeMap<String, usize>) -> Vec<Vec<Vec<String>>> {
    if counts.is_empty() {
        return vec![vec![]];
    }
    let tile_key = counts.keys().next().cloned().unwrap_or_default();
    let count = counts.get(&tile_key).copied().unwrap_or(0);
    if count == 0 {
        let mut next = counts.clone();
        next.remove(&tile_key);
        return extract_all_melds(&next);
    }
    let mut results = Vec::new();
    if count >= 3 {
        let mut next = counts.clone();
        decrement_count(&mut next, &tile_key, 3);
        for melds in extract_all_melds(&next) {
            let mut current = vec![vec![tile_key.clone(), tile_key.clone(), tile_key.clone()]];
            current.extend(melds);
            results.push(current);
        }
    }
    if let Some((prefix, rank)) = parse_suit(&tile_key) {
        if rank <= 7 {
            let second = format!("{prefix}{}", rank + 1);
            let third = format!("{prefix}{}", rank + 2);
            if counts.get(&second).copied().unwrap_or(0) > 0
                && counts.get(&third).copied().unwrap_or(0) > 0
            {
                let mut next = counts.clone();
                decrement_count(&mut next, &tile_key, 1);
                decrement_count(&mut next, &second, 1);
                decrement_count(&mut next, &third, 1);
                for melds in extract_all_melds(&next) {
                    let mut current = vec![vec![tile_key.clone(), second.clone(), third.clone()]];
                    current.extend(melds);
                    results.push(current);
                }
            }
        }
    }
    results
}

fn is_seven_pairs(counts: &BTreeMap<String, usize>) -> bool {
    if counts.values().sum::<usize>() != 14 {
        return false;
    }
    let mut pair_count = 0usize;
    for count in counts.values() {
        if !matches!(*count, 2 | 4) {
            return false;
        }
        pair_count += count / 2;
    }
    pair_count == 7
}

fn seven_pairs_pair_tiles(counts: &BTreeMap<String, usize>) -> Vec<String> {
    let mut pair_tiles = Vec::new();
    for (tile_key, count) in counts {
        for _ in 0..(count / 2) {
            pair_tiles.push(tile_key.clone());
        }
    }
    pair_tiles
}

fn is_thirteen_orphans(counts: &BTreeMap<String, usize>) -> bool {
    let required = [
        "w1", "w9", "t1", "t9", "b1", "b9", "east", "south", "west", "north", "red", "green",
        "white",
    ];
    if counts
        .keys()
        .any(|tile_key| !required.contains(&tile_key.as_str()))
    {
        return false;
    }
    if required
        .iter()
        .any(|tile_key| counts.get(*tile_key).copied().unwrap_or(0) == 0)
    {
        return false;
    }
    counts.values().sum::<usize>() == 14
        && required
            .iter()
            .filter(|tile_key| counts.get(**tile_key).copied().unwrap_or(0) == 2)
            .count()
            == 1
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

fn tile_counts<'a>(tile_keys: impl Iterator<Item = &'a str>) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for tile_key in tile_keys {
        *counts.entry(tile_key.to_string()).or_insert(0) += 1;
    }
    counts
}

fn decrement_count(counts: &mut BTreeMap<String, usize>, tile_key: &str, amount: usize) {
    if let Some(count) = counts.get_mut(tile_key) {
        if *count <= amount {
            counts.remove(tile_key);
        } else {
            *count -= amount;
        }
    }
}

fn score_map_value(values: &[i64]) -> Value {
    let mut map = Map::new();
    for (seat, value) in values.iter().enumerate() {
        map.insert(seat.to_string(), Value::Number((*value).into()));
    }
    Value::Object(map)
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
fn is_even_tile(tile_key: &str) -> bool {
    parse_suit(tile_key)
        .map(|(_, rank)| matches!(rank, 2 | 4 | 6 | 8))
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
            seat_wind_key: Some("east".to_string()),
            round_wind_key: Some("east".to_string()),
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
            seat_wind_key: Some("east".to_string()),
            round_wind_key: Some("east".to_string()),
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
            seat_wind_key: Some("east".to_string()),
            round_wind_key: Some("east".to_string()),
        });

        assert!(result.fan_keys.iter().any(|fan| fan == "chicken_hand"));
    }
}
