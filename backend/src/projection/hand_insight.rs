use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;

use crate::core::ids::Seat;
use crate::core::state::{PendingTimeout, PlayerRoundState, RoomState, RoundState};
use crate::projection::SeatProjectionSupport;
use crate::room_scoring::RoomScoringCache;
use crate::rules::scoring::{
    EvaluationInput, TimingFeatures, decompose_winning_hand_with_melds, evaluate_fans,
    extract_hand_features, recommendable_fan_rules,
};
use crate::rules::standard::win::classify_meld_groups_for_projection;

const STANDARD_TILE_KEYS: [&str; 34] = [
    "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "t1", "t2", "t3", "t4", "t5", "t6", "t7",
    "t8", "t9", "b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9", "east", "south", "west",
    "north", "red", "green", "white",
];
const WIND_ORDER: [&str; 4] = ["east", "south", "west", "north"];
const GREEN_TILE_KEYS: [&str; 6] = ["t2", "t3", "t4", "t6", "t8", "green"];
const ORPHAN_TILE_KEYS: [&str; 13] = [
    "w1", "w9", "t1", "t9", "b1", "b9", "east", "south", "west", "north", "red", "green", "white",
];
const DRAGON_TILE_KEYS: [&str; 3] = ["red", "green", "white"];
const WIND_TILE_KEYS: [&str; 4] = ["east", "south", "west", "north"];

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HandInsightsView {
    current: Option<HandInsightView>,
    by_discard_tile_id: BTreeMap<String, HandInsightView>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HandInsightView {
    discard_tile_id: Option<String>,
    discard_tile_code: Option<String>,
    is_tenpai: bool,
    waits: Vec<HandInsightWaitView>,
    recommendations: Vec<HandInsightRecommendationView>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HandInsightWaitView {
    code: String,
    available_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HandInsightRecommendationView {
    fan_key: String,
    fan_value: i64,
    similarity_percent: i64,
}

pub(crate) fn build_hand_insights_view(
    state: &RoomState,
    local_seat: Seat,
    support: &SeatProjectionSupport,
) -> Option<HandInsightsView> {
    let round = state.round_state.as_ref()?;
    let local_player = round
        .players
        .iter()
        .find(|player| player.seat == local_seat)?;
    let live_tiles = non_flower_tiles(local_player);
    if live_tiles.is_empty() {
        return None;
    }

    let cache = RoomScoringCache::from_state(state);
    let public_visible_tile_keys = collect_public_visible_tile_keys(round);
    let known_tile_counts = collect_known_tile_counts(local_player, &public_visible_tile_keys);
    let by_discard_tile_id = build_discard_preview_map(
        state,
        &cache,
        round,
        local_player,
        local_seat,
        support,
        &known_tile_counts,
        &public_visible_tile_keys,
    );

    let current_concealed_tile_keys =
        current_concealed_tile_keys(local_player, local_seat, state.pending_timeout.as_ref());
    let current = Some(build_insight(
        state,
        &cache,
        local_player,
        local_seat,
        current_concealed_tile_keys,
        None,
        &known_tile_counts,
        &public_visible_tile_keys,
        Some(&by_discard_tile_id),
    ));

    Some(HandInsightsView {
        current,
        by_discard_tile_id,
    })
}

fn build_discard_preview_map(
    state: &RoomState,
    cache: &RoomScoringCache,
    _round: &RoundState,
    local_player: &PlayerRoundState,
    local_seat: Seat,
    support: &SeatProjectionSupport,
    known_tile_counts: &HashMap<String, i64>,
    public_visible_tile_keys: &[String],
) -> BTreeMap<String, HandInsightView> {
    if local_player.is_ready_hand {
        return BTreeMap::new();
    }

    non_flower_tiles(local_player)
        .into_iter()
        .filter(|tile| !support.restricted_discard_tile_ids.contains(&tile.tile_id))
        .map(|tile| {
            let concealed_tile_keys = non_flower_tiles(local_player)
                .into_iter()
                .filter(|candidate| candidate.tile_id != tile.tile_id)
                .map(|candidate| candidate.tile_key.clone())
                .collect::<Vec<_>>();
            let preview_visible_tile_keys =
                public_visible_with_extra_discard(public_visible_tile_keys, &tile.tile_key);
            let insight = build_insight(
                state,
                cache,
                local_player,
                local_seat,
                concealed_tile_keys,
                Some((&tile.tile_id, &tile.tile_key)),
                known_tile_counts,
                &preview_visible_tile_keys,
                None,
            );
            (tile.tile_id.clone(), insight)
        })
        .collect()
}

fn build_insight(
    state: &RoomState,
    cache: &RoomScoringCache,
    local_player: &PlayerRoundState,
    local_seat: Seat,
    concealed_tile_keys: Vec<String>,
    discard_tile: Option<(&str, &str)>,
    known_tile_counts: &HashMap<String, i64>,
    public_visible_tile_keys: &[String],
    preview_map: Option<&BTreeMap<String, HandInsightView>>,
) -> HandInsightView {
    let waits = build_waits(&concealed_tile_keys, &local_player.melds, known_tile_counts);
    let recommendations = if !waits.is_empty() {
        let exact = build_exact_recommendations(
            state,
            cache,
            local_player,
            local_seat,
            &concealed_tile_keys,
            discard_tile,
            &waits,
            public_visible_tile_keys,
        );
        if exact.is_empty() {
            build_heuristic_recommendations(
                cache,
                local_player,
                local_seat,
                &concealed_tile_keys,
                public_visible_tile_keys,
            )
        } else {
            exact
        }
    } else if discard_tile.is_none() {
        let aggregated = preview_map
            .map(aggregate_preview_recommendations)
            .unwrap_or_default();
        if aggregated.is_empty() {
            build_heuristic_recommendations(
                cache,
                local_player,
                local_seat,
                &concealed_tile_keys,
                public_visible_tile_keys,
            )
        } else {
            aggregated
        }
    } else {
        build_heuristic_recommendations(
            cache,
            local_player,
            local_seat,
            &concealed_tile_keys,
            public_visible_tile_keys,
        )
    };

    HandInsightView {
        discard_tile_id: discard_tile.map(|(tile_id, _)| tile_id.to_string()),
        discard_tile_code: discard_tile.map(|(_, tile_key)| tile_key.to_string()),
        is_tenpai: !waits.is_empty(),
        waits,
        recommendations,
    }
}

fn current_concealed_tile_keys(
    local_player: &PlayerRoundState,
    local_seat: Seat,
    pending_timeout: Option<&PendingTimeout>,
) -> Vec<String> {
    let locked_drawn_tile_id = if local_player.is_ready_hand {
        pending_timeout.and_then(|timeout| {
            (timeout.kind == "active_turn" && timeout.seat_index == local_seat)
                .then(|| timeout.drawn_tile_id.as_deref())
                .flatten()
        })
    } else {
        None
    };

    non_flower_tiles(local_player)
        .into_iter()
        .filter(|tile| Some(tile.tile_id.as_str()) != locked_drawn_tile_id)
        .map(|tile| tile.tile_key.clone())
        .collect()
}

fn build_waits(
    concealed_tile_keys: &[String],
    meld_tile_key_groups: &[Vec<String>],
    known_tile_counts: &HashMap<String, i64>,
) -> Vec<HandInsightWaitView> {
    let expected_concealed_count = (4_usize.saturating_sub(meld_tile_key_groups.len())) * 3 + 1;
    if concealed_tile_keys.len() != expected_concealed_count {
        return Vec::new();
    }

    let current_hand_counts = tile_counts(concealed_tile_keys, meld_tile_key_groups);
    let mut waits = STANDARD_TILE_KEYS
        .iter()
        .filter_map(|tile_key| {
            if current_hand_counts.get(*tile_key).copied().unwrap_or(0) >= 4 {
                return None;
            }

            let mut simulated = concealed_tile_keys.to_vec();
            simulated.push((*tile_key).to_string());
            if decompose_winning_hand_with_melds(&simulated, meld_tile_key_groups).is_empty() {
                return None;
            }

            Some(HandInsightWaitView {
                code: (*tile_key).to_string(),
                available_count: (4 - known_tile_counts.get(*tile_key).copied().unwrap_or(0))
                    .max(0),
            })
        })
        .collect::<Vec<_>>();
    waits.sort_by(|left, right| tile_order_key(&left.code).cmp(&tile_order_key(&right.code)));
    waits
}

fn build_exact_recommendations(
    _state: &RoomState,
    cache: &RoomScoringCache,
    local_player: &PlayerRoundState,
    local_seat: Seat,
    concealed_tile_keys: &[String],
    discard_tile: Option<(&str, &str)>,
    waits: &[HandInsightWaitView],
    public_visible_tile_keys: &[String],
) -> Vec<HandInsightRecommendationView> {
    let total_live_waits = waits
        .iter()
        .map(|wait| wait.available_count.max(0))
        .sum::<i64>();
    if total_live_waits <= 0 {
        return Vec::new();
    }

    let matched_fans_by_wait = waits
        .iter()
        .map(|wait| {
            (
                wait.code.clone(),
                evaluate_matched_fans_for_wait(
                    cache,
                    local_player,
                    local_seat,
                    concealed_tile_keys,
                    &wait.code,
                    public_visible_tile_keys,
                    discard_tile.map(|(_, tile_key)| tile_key),
                ),
            )
        })
        .collect::<HashMap<_, _>>();

    let mut recommendations = recommendable_fan_rules(4)
        .into_iter()
        .filter_map(|(fan_key, fan_value)| {
            let covered_live_waits = waits
                .iter()
                .filter(|wait| {
                    matched_fans_by_wait
                        .get(&wait.code)
                        .is_some_and(|fan_keys| fan_keys.contains(fan_key))
                })
                .map(|wait| wait.available_count.max(0))
                .sum::<i64>();
            let similarity_percent =
                ((covered_live_waits * 100) + total_live_waits / 2) / total_live_waits;
            (similarity_percent >= 20).then(|| HandInsightRecommendationView {
                fan_key: fan_key.to_string(),
                fan_value,
                similarity_percent,
            })
        })
        .collect::<Vec<_>>();

    sort_and_truncate_recommendations(&mut recommendations);
    recommendations
}

fn evaluate_matched_fans_for_wait(
    cache: &RoomScoringCache,
    local_player: &PlayerRoundState,
    local_seat: Seat,
    concealed_tile_keys: &[String],
    wait_tile_key: &str,
    public_visible_tile_keys: &[String],
    preview_discard_tile_key: Option<&str>,
) -> HashSet<String> {
    let mut matched = HashSet::new();
    for self_draw in [false, true] {
        if let Some(fan_keys) = evaluate_wait_scenario(
            cache,
            local_player,
            local_seat,
            concealed_tile_keys,
            wait_tile_key,
            public_visible_tile_keys,
            preview_discard_tile_key,
            self_draw,
        ) {
            matched.extend(fan_keys);
        }
    }
    matched
}

fn evaluate_wait_scenario(
    cache: &RoomScoringCache,
    local_player: &PlayerRoundState,
    local_seat: Seat,
    concealed_tile_keys: &[String],
    wait_tile_key: &str,
    public_visible_tile_keys: &[String],
    preview_discard_tile_key: Option<&str>,
    self_draw: bool,
) -> Option<HashSet<String>> {
    let concealed_tile_keys = if self_draw {
        let mut next = concealed_tile_keys.to_vec();
        next.push(wait_tile_key.to_string());
        next
    } else {
        concealed_tile_keys.to_vec()
    };
    let incoming_tile = if self_draw { None } else { Some(wait_tile_key) };
    let effective_concealed = if self_draw {
        concealed_tile_keys.clone()
    } else {
        let mut next = concealed_tile_keys.clone();
        next.push(wait_tile_key.to_string());
        next
    };
    let decompositions =
        decompose_winning_hand_with_melds(&effective_concealed, &local_player.melds);
    if decompositions.is_empty() {
        return None;
    }

    let (open_meld_tile_key_groups, meld_open_flags) =
        classify_meld_groups_for_projection(local_seat, &local_player.melds, &cache.kong_entries);
    let features = extract_hand_features(
        &concealed_tile_keys,
        &local_player.melds,
        Some(&meld_open_flags),
        incoming_tile,
        Some(&seat_wind_key(local_seat, cache.dealer_seat)),
        cache.round_wind.as_deref(),
        Some(&decompositions),
    );
    let visible_tile_keys = if let Some(discard_tile_key) = preview_discard_tile_key {
        public_visible_with_extra_discard(public_visible_tile_keys, discard_tile_key)
    } else {
        public_visible_tile_keys.to_vec()
    };
    let fan_result = evaluate_fans(EvaluationInput {
        win_type: if self_draw {
            "self_draw".to_string()
        } else {
            "discard".to_string()
        },
        winner_seat: Some(local_seat),
        discarder_seat: if self_draw {
            None
        } else {
            Some((local_seat + 1) % cache.seat_count.max(1))
        },
        ready_hand_declared: local_player.is_ready_hand,
        flower_count: local_player.flowers.len(),
        seat_count: cache.seat_count,
        features,
        timing: TimingFeatures::default(),
        kong_entries: cache.kong_entries.clone(),
        tile_keys: player_tile_keys_from_parts(
            &concealed_tile_keys,
            &local_player.melds,
            incoming_tile,
        ),
        visible_tile_keys,
        concealed_tile_keys,
        meld_tile_key_groups: local_player.melds.clone(),
        open_meld_tile_key_groups,
        incoming_tile: incoming_tile.map(ToString::to_string),
        decompositions,
    });

    Some(fan_result.fan_keys.into_iter().collect())
}

fn aggregate_preview_recommendations(
    preview_map: &BTreeMap<String, HandInsightView>,
) -> Vec<HandInsightRecommendationView> {
    let total_preview_count = preview_map.len() as i64;
    if total_preview_count <= 0 {
        return Vec::new();
    }

    let mut aggregated = BTreeMap::<String, (i64, Vec<i64>)>::new();
    for preview in preview_map.values() {
        for recommendation in &preview.recommendations {
            let entry = aggregated
                .entry(recommendation.fan_key.clone())
                .or_insert((recommendation.fan_value, Vec::new()));
            entry.0 = entry.0.max(recommendation.fan_value);
            entry.1.push(recommendation.similarity_percent);
        }
    }

    let mut recommendations = aggregated
        .into_iter()
        .filter_map(|(fan_key, (fan_value, mut similarity_scores))| {
            similarity_scores.sort_by(|left, right| right.cmp(left));
            let supported_branch_count = similarity_scores.len() as i64;
            let best_score = similarity_scores.first().copied().unwrap_or(0);
            let top_sample_count = similarity_scores.len().min(3) as i64;
            let top_sample_sum = similarity_scores
                .iter()
                .take(top_sample_count as usize)
                .copied()
                .sum::<i64>();
            let top_sample_average = if top_sample_count > 0 {
                (top_sample_sum + top_sample_count / 2) / top_sample_count
            } else {
                0
            };
            let branch_support_percent =
                (supported_branch_count * 100 + total_preview_count / 2) / total_preview_count;
            let blended_score = ((best_score * 60) + (top_sample_average * 40) + 50) / 100;
            let similarity_percent = ((blended_score * branch_support_percent) + 50) / 100;
            (similarity_percent >= 20).then(|| HandInsightRecommendationView {
                fan_key,
                fan_value,
                similarity_percent,
            })
        })
        .collect::<Vec<_>>();
    sort_and_truncate_recommendations(&mut recommendations);
    recommendations
}

fn build_heuristic_recommendations(
    cache: &RoomScoringCache,
    local_player: &PlayerRoundState,
    local_seat: Seat,
    concealed_tile_keys: &[String],
    public_visible_tile_keys: &[String],
) -> Vec<HandInsightRecommendationView> {
    let (open_meld_tile_key_groups, meld_open_flags) =
        classify_meld_groups_for_projection(local_seat, &local_player.melds, &cache.kong_entries);
    let route_summary = RouteSummary::from_hand(
        concealed_tile_keys,
        &local_player.melds,
        &meld_open_flags,
        &cache.kong_entries,
        local_seat,
        cache.dealer_seat,
        cache.round_wind.as_deref(),
        public_visible_tile_keys,
        cache.wall_tiles_remaining,
        &cache.opponent_melds_by_seat,
        open_meld_tile_key_groups.len(),
    );

    let mut recommendations = recommendable_fan_rules(4)
        .into_iter()
        .filter_map(|(fan_key, fan_value)| {
            let similarity_percent = route_summary.similarity_for(fan_key);
            (similarity_percent >= 20).then(|| HandInsightRecommendationView {
                fan_key: fan_key.to_string(),
                fan_value,
                similarity_percent,
            })
        })
        .collect::<Vec<_>>();
    sort_and_truncate_recommendations(&mut recommendations);
    recommendations
}

fn sort_and_truncate_recommendations(recommendations: &mut Vec<HandInsightRecommendationView>) {
    recommendations.sort_by(|left, right| {
        right
            .similarity_percent
            .cmp(&left.similarity_percent)
            .then_with(|| right.fan_value.cmp(&left.fan_value))
            .then_with(|| left.fan_key.cmp(&right.fan_key))
    });
    recommendations.truncate(6);
}

fn collect_public_visible_tile_keys(round: &RoundState) -> Vec<String> {
    let mut visible_tile_keys = Vec::new();
    for player in &round.players {
        visible_tile_keys.extend(player.discards.iter().map(|tile| tile.tile_key.clone()));
        visible_tile_keys.extend(player.flowers.iter().map(|tile| tile.tile_key.clone()));
        for meld in &player.melds {
            if meld.len() == 4 && meld.iter().all(|tile_key| tile_key == &meld[0]) {
                visible_tile_keys.extend(meld.iter().take(3).cloned());
            } else {
                visible_tile_keys.extend(meld.iter().cloned());
            }
        }
    }
    visible_tile_keys
}

fn collect_known_tile_counts(
    local_player: &PlayerRoundState,
    public_visible_tile_keys: &[String],
) -> HashMap<String, i64> {
    let mut counts = HashMap::new();
    for tile_key in public_visible_tile_keys {
        *counts.entry(tile_key.clone()).or_insert(0) += 1;
    }
    for tile in non_flower_tiles(local_player) {
        *counts.entry(tile.tile_key.clone()).or_insert(0) += 1;
    }
    counts
}

fn public_visible_with_extra_discard(
    public_visible_tile_keys: &[String],
    discard_tile_key: &str,
) -> Vec<String> {
    let mut visible_tile_keys = public_visible_tile_keys.to_vec();
    visible_tile_keys.push(discard_tile_key.to_string());
    visible_tile_keys
}

fn non_flower_tiles(local_player: &PlayerRoundState) -> Vec<&crate::core::tile::Tile> {
    local_player
        .concealed_tiles
        .iter()
        .filter(|tile| tile.kind != "flower")
        .collect()
}

fn tile_counts(
    concealed_tile_keys: &[String],
    meld_tile_key_groups: &[Vec<String>],
) -> HashMap<String, i64> {
    let mut counts = HashMap::new();
    for tile_key in concealed_tile_keys {
        *counts.entry(tile_key.clone()).or_insert(0) += 1;
    }
    for meld in meld_tile_key_groups {
        for tile_key in meld {
            *counts.entry(tile_key.clone()).or_insert(0) += 1;
        }
    }
    counts
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

fn seat_wind_key(seat_index: usize, dealer_seat: usize) -> String {
    WIND_ORDER[(seat_index + 4 - dealer_seat) % 4].to_string()
}

fn tile_order_key(tile_key: &str) -> (usize, usize, String) {
    match tile_key {
        "east" => (3, 1, tile_key.to_string()),
        "south" => (3, 2, tile_key.to_string()),
        "west" => (3, 3, tile_key.to_string()),
        "north" => (3, 4, tile_key.to_string()),
        "red" => (4, 1, tile_key.to_string()),
        "green" => (4, 2, tile_key.to_string()),
        "white" => (4, 3, tile_key.to_string()),
        _ => {
            let bytes = tile_key.as_bytes();
            if bytes.len() == 2 {
                let group = match bytes[0] {
                    b'w' => 0,
                    b't' => 1,
                    b'b' => 2,
                    _ => 9,
                };
                let order = usize::from(bytes[1].saturating_sub(b'0'));
                (group, order, tile_key.to_string())
            } else {
                (9, 9, tile_key.to_string())
            }
        }
    }
}

fn parse_suit(tile_key: &str) -> Option<(usize, i64)> {
    let bytes = tile_key.as_bytes();
    if bytes.len() != 2 {
        return None;
    }
    let suit = match bytes[0] {
        b'w' => 0,
        b't' => 1,
        b'b' => 2,
        _ => return None,
    };
    let rank = i64::from(bytes[1].checked_sub(b'0')?);
    (1..=9).contains(&rank).then_some((suit, rank))
}

#[derive(Debug, Clone)]
struct RouteSummary {
    full_counts: HashMap<String, i64>,
    suit_counts: [i64; 3],
    total_tiles: i64,
    dominant_suit_count: i64,
    off_suit_count: i64,
    honor_count: i64,
    wind_tile_count: i64,
    dragon_tile_count: i64,
    pair_count: i64,
    triplet_count: i64,
    quad_count: i64,
    open_meld_count: i64,
    open_chow_count: i64,
    open_kong_count: i64,
    concealed_tile_count: i64,
    terminal_or_honour_count: i64,
    upper_four_tile_count: i64,
    upper_tile_count: i64,
    lower_four_tile_count: i64,
    lower_tile_count: i64,
    middle_rank_tile_count: i64,
    non_green_tile_count: i64,
    dragon_pair_count: i64,
    dragon_triplet_count: i64,
    wind_pair_count: i64,
    wind_triplet_count: i64,
    five_tile_count: i64,
    unique_orphan_count: i64,
    orphan_pair_count: i64,
    single_suit_only: bool,
    best_pure_straight_score: i64,
    best_mixed_triple_chow_score: i64,
    best_mixed_straight_score: i64,
    public_visible_tile_counts: HashMap<String, i64>,
    wall_tiles_remaining: i64,
    opponent_triplet_meld_count: i64,
}

impl RouteSummary {
    fn from_hand(
        concealed_tile_keys: &[String],
        meld_tile_key_groups: &[Vec<String>],
        meld_open_flags: &[bool],
        _kong_entries: &[crate::rules::scoring::KongEntry],
        local_seat: Seat,
        dealer_seat: usize,
        round_wind: Option<&str>,
        public_visible_tile_keys: &[String],
        wall_tiles_remaining: i64,
        opponent_melds_by_seat: &[Vec<Vec<String>>],
        actual_open_meld_count: usize,
    ) -> Self {
        let full_counts = tile_counts(concealed_tile_keys, meld_tile_key_groups);
        let mut suit_counts = [0_i64; 3];
        let mut honor_count = 0_i64;
        let mut wind_tile_count = 0_i64;
        let mut dragon_tile_count = 0_i64;
        let mut terminal_or_honour_count = 0_i64;
        let mut upper_four_tile_count = 0_i64;
        let mut upper_tile_count = 0_i64;
        let mut lower_four_tile_count = 0_i64;
        let mut lower_tile_count = 0_i64;
        let mut middle_rank_tile_count = 0_i64;
        let mut non_green_tile_count = 0_i64;
        let mut five_tile_count = 0_i64;
        let mut unique_orphans = 0_i64;
        let mut orphan_pair_count = 0_i64;
        let mut dragon_pair_count = 0_i64;
        let mut dragon_triplet_count = 0_i64;
        let mut wind_pair_count = 0_i64;
        let mut wind_triplet_count = 0_i64;
        let mut pair_count = 0_i64;
        let mut triplet_count = 0_i64;
        let mut quad_count = 0_i64;

        for (tile_key, count) in &full_counts {
            if *count >= 2 {
                pair_count += 1;
            }
            if *count >= 3 {
                triplet_count += 1;
            }
            if *count >= 4 {
                quad_count += 1;
            }
            if ORPHAN_TILE_KEYS.contains(&tile_key.as_str()) {
                unique_orphans += 1;
                if *count >= 2 {
                    orphan_pair_count += 1;
                }
            }
            if DRAGON_TILE_KEYS.contains(&tile_key.as_str()) {
                if *count >= 2 {
                    dragon_pair_count += 1;
                }
                if *count >= 3 {
                    dragon_triplet_count += 1;
                }
            }
            let seat_wind = seat_wind_key(local_seat, dealer_seat);
            if WIND_TILE_KEYS.contains(&tile_key.as_str()) {
                if *count >= 2 {
                    wind_pair_count += 1;
                }
                if *count >= 3 {
                    wind_triplet_count += 1;
                }
                if tile_key == &seat_wind || round_wind == Some(tile_key.as_str()) {
                    dragon_pair_count += 0;
                }
            }
            if tile_key.ends_with('5') {
                five_tile_count += *count;
            }

            if let Some((suit_index, rank)) = parse_suit(tile_key) {
                suit_counts[suit_index] += *count;
                if matches!(rank, 1 | 9) {
                    terminal_or_honour_count += *count;
                }
                if (6..=9).contains(&rank) {
                    upper_four_tile_count += *count;
                }
                if (7..=9).contains(&rank) {
                    upper_tile_count += *count;
                }
                if (1..=4).contains(&rank) {
                    lower_four_tile_count += *count;
                }
                if (1..=3).contains(&rank) {
                    lower_tile_count += *count;
                }
                if (4..=6).contains(&rank) {
                    middle_rank_tile_count += *count;
                }
            } else {
                honor_count += *count;
                terminal_or_honour_count += *count;
                if WIND_TILE_KEYS.contains(&tile_key.as_str()) {
                    wind_tile_count += *count;
                }
                if DRAGON_TILE_KEYS.contains(&tile_key.as_str()) {
                    dragon_tile_count += *count;
                }
            }

            if !GREEN_TILE_KEYS.contains(&tile_key.as_str()) {
                non_green_tile_count += *count;
            }
        }

        let (dominant_suit_count, off_suit_count) = {
            let dominant = *suit_counts.iter().max().unwrap_or(&0);
            let off = suit_counts.iter().sum::<i64>() - dominant;
            (dominant, off)
        };
        let total_tiles = full_counts.values().sum::<i64>();
        let open_chow_count = meld_tile_key_groups
            .iter()
            .zip(meld_open_flags.iter().copied())
            .filter(|(meld, is_open)| {
                *is_open
                    && meld.len() == 3
                    && meld.first().is_some()
                    && !meld.iter().all(|tile_key| tile_key == &meld[0])
            })
            .count() as i64;
        let open_kong_count = meld_tile_key_groups
            .iter()
            .zip(meld_open_flags.iter().copied())
            .filter(|(meld, is_open)| {
                *is_open
                    && meld.len() == 4
                    && meld.first().is_some()
                    && meld.iter().all(|tile_key| tile_key == &meld[0])
            })
            .count() as i64;
        let public_visible_tile_counts = public_visible_tile_keys.iter().fold(
            HashMap::<String, i64>::new(),
            |mut counts, tile_key| {
                *counts.entry(tile_key.clone()).or_insert(0) += 1;
                counts
            },
        );
        let best_pure_straight_score = best_pure_straight_score(&full_counts);
        let best_mixed_triple_chow_score = best_mixed_triple_chow_score(&full_counts);
        let best_mixed_straight_score = best_mixed_straight_score(&full_counts);
        let opponent_triplet_meld_count = opponent_melds_by_seat
            .iter()
            .enumerate()
            .filter(|(seat, _)| *seat != local_seat)
            .flat_map(|(_, melds)| melds.iter())
            .filter(|meld| {
                meld.len() == 3
                    && meld.first().is_some()
                    && meld.iter().all(|tile_key| tile_key == &meld[0])
            })
            .count() as i64;

        Self {
            full_counts,
            suit_counts,
            total_tiles,
            dominant_suit_count,
            off_suit_count,
            honor_count,
            wind_tile_count,
            dragon_tile_count,
            pair_count,
            triplet_count,
            quad_count,
            open_meld_count: actual_open_meld_count as i64,
            open_chow_count,
            open_kong_count,
            concealed_tile_count: concealed_tile_keys.len() as i64,
            terminal_or_honour_count,
            upper_four_tile_count,
            upper_tile_count,
            lower_four_tile_count,
            lower_tile_count,
            middle_rank_tile_count,
            non_green_tile_count,
            dragon_pair_count,
            dragon_triplet_count,
            wind_pair_count,
            wind_triplet_count,
            five_tile_count,
            unique_orphan_count: unique_orphans,
            orphan_pair_count,
            single_suit_only: suit_counts.iter().filter(|count| **count > 0).count() <= 1,
            best_pure_straight_score,
            best_mixed_triple_chow_score,
            best_mixed_straight_score,
            public_visible_tile_counts,
            wall_tiles_remaining,
            opponent_triplet_meld_count,
        }
    }

    fn similarity_for(&self, fan_key: &str) -> i64 {
        let score = match fan_key {
            "full_flush" => self.full_flush_score(),
            "half_flush" => self.half_flush_score(),
            "all_pungs" => self.all_pungs_score(),
            "seven_pairs" => self.seven_pairs_score(),
            "fully_concealed_hand" => self.closed_only_score(),
            "thirteen_orphans" => self.thirteen_orphans_score(),
            "nine_gates" => self.nine_gates_score(),
            "pure_straight" => self.best_pure_straight_score,
            "mixed_triple_chow" => self.best_mixed_triple_chow_score,
            "mixed_straight" => self.best_mixed_straight_score,
            "pure_shifted_chows" => self.pure_shifted_chows_score(3),
            "four_pure_shifted_chows" => self.pure_shifted_chows_score(4),
            "pure_shifted_pungs" => self.pure_shifted_pungs_score(3),
            "four_pure_shifted_pungs" => self.pure_shifted_pungs_score(4),
            "melded_hand" => self.melded_hand_score(),
            "two_melded_kongs" => (self.open_kong_count * 48).min(100),
            "three_kongs" => ((self.quad_count * 28) + (self.open_kong_count * 14)).min(100),
            "little_three_dragons" => {
                ((self.dragon_triplet_count * 28) + (self.dragon_pair_count * 18)).min(100)
            }
            "big_three_dragons" => (self.dragon_triplet_count * 34).min(100),
            "little_four_winds" => {
                ((self.wind_triplet_count * 22) + (self.wind_pair_count * 16)).min(100)
            }
            "big_four_winds" => (self.wind_triplet_count * 26).min(100),
            "all_honours" => self.allowed_ratio_score(self.honor_count, 0),
            "all_terminals_and_honours" => {
                self.allowed_ratio_score(self.terminal_or_honour_count, 0)
            }
            "all_terminals" => {
                if self.honor_count > 0 {
                    0
                } else {
                    self.allowed_ratio_score(self.terminal_or_honour_count, 0)
                }
            }
            "all_green" => {
                self.allowed_ratio_score(self.total_tiles - self.non_green_tile_count, 0)
            }
            "all_types" => self.all_types_score(),
            "all_fives" => (self.five_tile_count * 18).min(100),
            "outside_hand" => self.allowed_ratio_score(self.terminal_or_honour_count, 8),
            "upper_four" => {
                if self.honor_count > 0 {
                    0
                } else {
                    self.allowed_ratio_score(self.upper_four_tile_count, 10)
                }
            }
            "upper_tiles" => {
                if self.honor_count > 0 {
                    0
                } else {
                    self.allowed_ratio_score(self.upper_tile_count, 12)
                }
            }
            "lower_four" => {
                if self.honor_count > 0 {
                    0
                } else {
                    self.allowed_ratio_score(self.lower_four_tile_count, 10)
                }
            }
            "lower_tiles" => {
                if self.honor_count > 0 {
                    0
                } else {
                    self.allowed_ratio_score(self.lower_tile_count, 12)
                }
            }
            "middle_tiles" => {
                if self.honor_count > 0 {
                    0
                } else {
                    self.allowed_ratio_score(self.middle_rank_tile_count, 12)
                }
            }
            "three_concealed_pungs" | "four_concealed_pungs" => {
                if self.open_meld_count > 0 {
                    0
                } else {
                    (self.triplet_count * 24).min(100)
                }
            }
            "out_with_replacement_tile" => self.replacement_tile_score(),
            "last_tile" => self.last_tile_score(),
            "last_tile_draw" | "last_tile_claim" => self.last_round_score(),
            "robbing_the_kong" => self.robbing_the_kong_score(),
            _ if fan_key.contains("chow") || fan_key.contains("straight") => {
                self.generic_chow_family_score()
            }
            _ if fan_key.contains("pung") || fan_key.contains("kong") => {
                self.generic_pung_family_score()
            }
            _ => 0,
        };
        score.clamp(0, 100)
    }

    fn full_flush_score(&self) -> i64 {
        if self.honor_count > 0 || self.dominant_suit_count == 0 {
            return (self.dominant_suit_count * 100 / self.total_tiles.max(1))
                .saturating_sub((self.off_suit_count + self.honor_count) * 18)
                .clamp(0, 100);
        }
        (self.dominant_suit_count * 100 / self.total_tiles.max(1))
            .saturating_sub(self.off_suit_count * 22)
            .clamp(0, 100)
    }

    fn half_flush_score(&self) -> i64 {
        let focus = self.dominant_suit_count + self.honor_count;
        (focus * 100 / self.total_tiles.max(1))
            .saturating_sub(self.off_suit_count * 18)
            .clamp(0, 100)
    }

    fn all_pungs_score(&self) -> i64 {
        ((self.triplet_count * 24) + (self.pair_count * 10) - (self.open_chow_count * 20))
            .clamp(0, 100)
    }

    fn seven_pairs_score(&self) -> i64 {
        if self.open_meld_count > 0 {
            return 0;
        }
        ((self.pair_count * 16) - (self.triplet_count * 12) + (self.quad_count * 8)).clamp(0, 100)
    }

    fn thirteen_orphans_score(&self) -> i64 {
        if self.open_meld_count > 0 {
            return 0;
        }
        ((self.unique_orphan_count * 7) + (self.orphan_pair_count * 12)).clamp(0, 100)
    }

    fn nine_gates_score(&self) -> i64 {
        if self.open_meld_count > 0 || !self.single_suit_only || self.honor_count > 0 {
            return 0;
        }
        ((self.dominant_suit_count * 8) - (self.off_suit_count * 16)).clamp(0, 100)
    }

    fn pure_shifted_chows_score(&self, group_size: usize) -> i64 {
        let mut best_window = 0_i64;
        for suit in ['w', 't', 'b'] {
            let max_start = 8_i64.saturating_sub(group_size as i64);
            for step in [1_i64, 2_i64] {
                if step == 2 && group_size == 4 {
                    continue;
                }
                let max_step_start = 8_i64.saturating_sub(((group_size as i64) - 1) * step);
                for start in 1..=max_start.min(max_step_start) {
                    let window_score = (0..group_size)
                        .map(|offset| {
                            self.sequence_segment_score(suit, start + (offset as i64) * step)
                        })
                        .sum::<i64>()
                        / group_size as i64;
                    best_window = best_window.max(window_score);
                }
            }
        }

        let difficulty_prior = if group_size == 4 { 58 } else { 72 };
        ((best_window * difficulty_prior) / 100)
            .saturating_sub(self.off_suit_count * 2)
            .clamp(0, 100)
    }

    fn pure_shifted_pungs_score(&self, group_size: usize) -> i64 {
        let mut best_window = 0_i64;
        for suit in ['w', 't', 'b'] {
            let steps = if group_size == 4 {
                &[1_i64][..]
            } else {
                &[1_i64, 2_i64][..]
            };
            for step in steps {
                let max_start = 9_i64.saturating_sub(((group_size as i64) - 1) * *step);
                for start in 1..=max_start {
                    let window_score = (0..group_size)
                        .map(|offset| {
                            self.triplet_slot_score(suit, start + (offset as i64) * *step)
                        })
                        .sum::<i64>()
                        / group_size as i64;
                    best_window = best_window.max(window_score);
                }
            }
        }

        let focus_percent = self.dominant_suit_count * 100 / self.total_tiles.max(1);
        let focus_adjusted = best_window * (55 + focus_percent / 2) / 100;
        let difficulty_prior = if group_size == 4 { 60 } else { 75 };
        ((focus_adjusted * difficulty_prior) / 100)
            .saturating_sub(self.open_chow_count * 18)
            .clamp(0, 100)
    }

    fn closed_only_score(&self) -> i64 {
        if self.open_meld_count > 0 { 0 } else { 86 }
    }

    fn melded_hand_score(&self) -> i64 {
        if self.open_meld_count >= 4 && self.concealed_tile_count <= 2 {
            return 96;
        }
        ((self.open_meld_count * 22) - (self.concealed_tile_count * 3)).clamp(0, 100)
    }

    fn replacement_tile_score(&self) -> i64 {
        if self.quad_count > 0 {
            48
        } else if self.triplet_count > 0 {
            24
        } else {
            0
        }
    }

    fn last_tile_score(&self) -> i64 {
        let best_seen = self
            .public_visible_tile_counts
            .values()
            .copied()
            .max()
            .unwrap_or(0);
        if best_seen >= 3 {
            64
        } else if best_seen == 2 {
            34
        } else {
            0
        }
    }

    fn last_round_score(&self) -> i64 {
        if self.wall_tiles_remaining <= 4 {
            58
        } else if self.wall_tiles_remaining <= 10 {
            32
        } else {
            0
        }
    }

    fn robbing_the_kong_score(&self) -> i64 {
        if self.opponent_triplet_meld_count > 0 {
            28
        } else {
            0
        }
    }

    fn all_types_score(&self) -> i64 {
        let supports = [
            self.suit_counts[0],
            self.suit_counts[1],
            self.suit_counts[2],
            self.wind_tile_count,
            self.dragon_tile_count,
        ];
        let present_categories = supports.iter().filter(|count| **count > 0).count() as i64;
        if present_categories < 5 {
            return (present_categories * 10).min(100);
        }

        let support_score = supports
            .into_iter()
            .map(category_support_score)
            .sum::<i64>()
            / 5;
        let structure_bonus = ((self.pair_count * 3) + (self.triplet_count * 5)).min(15);
        ((support_score * 55) / 100 + structure_bonus).clamp(0, 100)
    }

    fn generic_chow_family_score(&self) -> i64 {
        let base = self
            .best_pure_straight_score
            .max(self.best_mixed_triple_chow_score)
            .max(self.best_mixed_straight_score)
            .saturating_sub(self.open_chow_count * 6);
        ((base * 55) / 100).clamp(0, 45)
    }

    fn generic_pung_family_score(&self) -> i64 {
        ((self.all_pungs_score() * 50) / 100)
            .saturating_sub(self.open_chow_count * 10)
            .clamp(0, 45)
    }

    fn allowed_ratio_score(&self, allowed_tile_count: i64, penalty_per_invalid: i64) -> i64 {
        let invalid = self.total_tiles.saturating_sub(allowed_tile_count);
        ((allowed_tile_count * 100 / self.total_tiles.max(1)) - invalid * penalty_per_invalid)
            .clamp(0, 100)
    }

    fn sequence_segment_score(&self, suit: char, start: i64) -> i64 {
        let covered_tiles = (0..3)
            .map(|offset| format!("{suit}{}", start + offset))
            .map(|tile_key| i64::from(self.full_counts.get(&tile_key).copied().unwrap_or(0) > 0))
            .sum::<i64>();
        covered_tiles * 100 / 3
    }

    fn triplet_slot_score(&self, suit: char, rank: i64) -> i64 {
        let tile_key = format!("{suit}{rank}");
        match self.full_counts.get(&tile_key).copied().unwrap_or(0) {
            count if count >= 3 => 100,
            2 => 72,
            1 => 18,
            _ => 0,
        }
    }
}

fn category_support_score(tile_count: i64) -> i64 {
    match tile_count {
        count if count >= 4 => 82,
        3 => 68,
        2 => 42,
        1 => 18,
        _ => 0,
    }
}

fn best_pure_straight_score(full_counts: &HashMap<String, i64>) -> i64 {
    let mut best = 0_i64;
    for suit in ['w', 't', 'b'] {
        let score = pure_straight_segment_score(full_counts, suit, 1)
            + pure_straight_segment_score(full_counts, suit, 4)
            + pure_straight_segment_score(full_counts, suit, 7);
        best = best.max((score * 12).min(100));
    }
    best
}

fn pure_straight_segment_score(full_counts: &HashMap<String, i64>, suit: char, start: i64) -> i64 {
    (0..3)
        .map(|offset| format!("{suit}{}", start + offset))
        .map(|tile_key| i64::from(full_counts.contains_key(&tile_key)))
        .sum::<i64>()
}

fn best_mixed_triple_chow_score(full_counts: &HashMap<String, i64>) -> i64 {
    let mut best = 0_i64;
    for start in 1..=7 {
        let present_suits = ['w', 't', 'b']
            .into_iter()
            .filter(|suit| {
                (0..3)
                    .map(|offset| format!("{suit}{}", start + offset))
                    .all(|tile_key| full_counts.contains_key(&tile_key))
            })
            .count() as i64;
        best = best.max((present_suits * 28).min(100));
    }
    best
}

fn best_mixed_straight_score(full_counts: &HashMap<String, i64>) -> i64 {
    let segment_scores = [1_i64, 4, 7]
        .into_iter()
        .map(|start| {
            ['w', 't', 'b']
                .into_iter()
                .filter(|suit| {
                    (0..3)
                        .map(|offset| format!("{suit}{}", start + offset))
                        .all(|tile_key| full_counts.contains_key(&tile_key))
                })
                .count() as i64
        })
        .collect::<Vec<_>>();
    (segment_scores.into_iter().sum::<i64>() * 12).min(100)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use crate::core::state::{
        MatchState, PendingTimeout, PlayerRoundState, RoomState, RoundScoreTrackers, RoundState,
        RuleRuntimeState, SeatState, WallState,
    };
    use crate::core::tile::Tile;
    use crate::projection::SeatProjectionSupport;
    use crate::projection::room_snapshot::room_snapshot_message;

    use super::{
        HandInsightRecommendationView, HandInsightView, RouteSummary,
        aggregate_preview_recommendations, build_hand_insights_view,
    };

    #[test]
    fn local_snapshot_projects_current_and_discard_preview_hand_insights() {
        let mut state = sample_state();
        let round = state.round_state.as_mut().expect("round");
        round.players[0].concealed_tiles = vec![
            suit_tile("w1#0", "w1"),
            suit_tile("w2#0", "w2"),
            suit_tile("w3#0", "w3"),
            suit_tile("w4#0", "w4"),
            suit_tile("w5#0", "w5"),
            suit_tile("w6#0", "w6"),
            suit_tile("w7#0", "w7"),
            suit_tile("w8#0", "w8"),
            suit_tile("w9#0", "w9"),
            suit_tile("t1#0", "t1"),
            suit_tile("t2#0", "t2"),
            suit_tile("t3#0", "t3"),
            suit_tile("t4#0", "t4"),
            suit_tile("b9#0", "b9"),
        ];
        round.players[1].discards = vec![suit_tile("t1#seen", "t1")];
        state.pending_timeout = Some(PendingTimeout {
            kind: "active_turn".to_string(),
            seat_index: 0,
            deadline_at: None,
            drawn_tile_id: Some("b9#0".to_string()),
        });

        let support = SeatProjectionSupport {
            can_ready_hand: true,
            ..Default::default()
        };
        let snapshot = room_snapshot_message(&state, 0, &support);
        let hand_insights = &snapshot["payload"]["private_state"]["hand_insights"];

        assert!(hand_insights["current"]["recommendations"].is_array());
        assert_eq!(
            hand_insights["by_discard_tile_id"]["b9#0"]["is_tenpai"],
            json!(true)
        );
        assert_eq!(
            hand_insights["by_discard_tile_id"]["b9#0"]["waits"],
            json!([
                { "code": "t1", "available_count": 2 },
                { "code": "t4", "available_count": 3 }
            ])
        );
    }

    #[test]
    fn open_meld_hand_drops_closed_only_recommendations() {
        let mut state = sample_state();
        state.round_state.as_mut().unwrap().players[0].concealed_tiles = vec![
            suit_tile("w1#0", "w1"),
            suit_tile("w1#1", "w1"),
            suit_tile("w2#0", "w2"),
            suit_tile("w2#1", "w2"),
            suit_tile("w3#0", "w3"),
            suit_tile("w4#0", "w4"),
            suit_tile("w5#0", "w5"),
            suit_tile("w6#0", "w6"),
            suit_tile("w7#0", "w7"),
            suit_tile("w8#0", "w8"),
            suit_tile("w9#0", "w9"),
        ];
        state.round_state.as_mut().unwrap().players[0].melds =
            vec![vec!["b3".to_string(), "b4".to_string(), "b5".to_string()]];

        let insights = build_hand_insights_view(&state, 0, &SeatProjectionSupport::default())
            .expect("local player should still receive insights");
        let keys = insights
            .current
            .expect("current insight")
            .recommendations
            .into_iter()
            .map(|entry| entry.fan_key)
            .collect::<Vec<_>>();

        assert!(!keys.iter().any(|key| key == "fully_concealed_hand"));
        assert!(!keys.iter().any(|key| key == "seven_pairs"));
    }

    #[test]
    fn current_recommendations_need_broad_preview_support() {
        let preview_map = BTreeMap::from([
            (
                "tile-1".to_string(),
                preview_with_recommendations(&[("full_flush", 24, 88), ("all_pungs", 6, 54)]),
            ),
            (
                "tile-2".to_string(),
                preview_with_recommendations(&[("all_pungs", 6, 58), ("mixed_straight", 8, 46)]),
            ),
            (
                "tile-3".to_string(),
                preview_with_recommendations(&[("all_pungs", 6, 55), ("mixed_straight", 8, 44)]),
            ),
            (
                "tile-4".to_string(),
                preview_with_recommendations(&[("mixed_straight", 8, 41)]),
            ),
        ]);

        let recommendations = aggregate_preview_recommendations(&preview_map);
        let all_pungs = recommendations
            .iter()
            .find(|entry| entry.fan_key == "all_pungs")
            .expect("all_pungs should survive aggregation");
        let full_flush = recommendations
            .iter()
            .find(|entry| entry.fan_key == "full_flush")
            .expect("full_flush should still be visible");

        assert!(all_pungs.similarity_percent > full_flush.similarity_percent);
        assert!(full_flush.similarity_percent < 40);
    }

    #[test]
    fn four_pure_shifted_pungs_is_not_scored_like_generic_all_pungs() {
        let summary = route_summary_for_tiles(&[
            "w1", "w1", "w1", "w5", "w5", "w5", "w9", "w9", "w9", "t2", "t2", "t2", "red", "red",
        ]);

        let shifted = summary.similarity_for("four_pure_shifted_pungs");
        let all_pungs = summary.similarity_for("all_pungs");

        assert!(all_pungs > shifted);
        assert!(shifted < 40);
    }

    #[test]
    fn all_types_needs_more_than_single_tile_presence() {
        let summary = route_summary_for_tiles(&[
            "w1", "w2", "w9", "t3", "t4", "t9", "b5", "b6", "b9", "east", "south", "red", "green",
            "white",
        ]);

        assert!(summary.similarity_for("all_types") < 40);
    }

    fn suit_tile(tile_id: &str, tile_key: &str) -> Tile {
        Tile {
            tile_id: tile_id.to_string(),
            tile_key: tile_key.to_string(),
            kind: "suit".to_string(),
            suit: None,
            rank: None,
            name: None,
        }
    }

    fn preview_with_recommendations(items: &[(&str, i64, i64)]) -> HandInsightView {
        HandInsightView {
            discard_tile_id: None,
            discard_tile_code: None,
            is_tenpai: false,
            waits: Vec::new(),
            recommendations: items
                .iter()
                .map(
                    |(fan_key, fan_value, similarity_percent)| HandInsightRecommendationView {
                        fan_key: (*fan_key).to_string(),
                        fan_value: *fan_value,
                        similarity_percent: *similarity_percent,
                    },
                )
                .collect(),
        }
    }

    fn route_summary_for_tiles(tile_keys: &[&str]) -> RouteSummary {
        let concealed_tile_keys = tile_keys
            .iter()
            .map(|tile_key| (*tile_key).to_string())
            .collect::<Vec<_>>();
        RouteSummary::from_hand(
            &concealed_tile_keys,
            &[],
            &[],
            &[],
            0,
            0,
            Some("east"),
            &[],
            40,
            &[Vec::new(), Vec::new(), Vec::new(), Vec::new()],
            0,
        )
    }

    fn sample_state() -> RoomState {
        RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            seats: (0..4)
                .map(|seat_index| SeatState {
                    seat_index,
                    connected: true,
                    ready: true,
                    seat_type: "human".to_string(),
                    ..Default::default()
                })
                .collect(),
            match_state: Some(MatchState {
                prevailing_wind: "east".to_string(),
                hand_number: 1,
                dealer_seat: 0,
                cumulative_scores: BTreeMap::from([(0, 0), (1, 0), (2, 0), (3, 0)]),
                match_finished: false,
                last_completed_round_id: None,
                statistics: Default::default(),
            }),
            round_state: Some(RoundState {
                round_id: "round-1".to_string(),
                dealer_seat: 0,
                round_wind: "east".to_string(),
                current_actor: 0,
                phase: "playing".to_string(),
                wall: WallState {
                    tiles: Vec::new(),
                    head_index: 0,
                    tail_index: 30,
                },
                players: (0..4)
                    .map(|seat| PlayerRoundState {
                        seat,
                        ..Default::default()
                    })
                    .collect(),
                settlement: None,
                pending_action: None,
                version: 1,
                score_trackers: RoundScoreTrackers::default(),
                last_action_context: Default::default(),
                rule_state: RuleRuntimeState::default(),
                restricted_discard_tile_key: None,
                last_discard: None,
            }),
            pending_timeout: None,
            continue_action: None,
        }
    }
}
