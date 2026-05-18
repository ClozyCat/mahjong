use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use crate::core::ids::Seat;
use crate::core::state::{PendingTimeout, PlayerRoundState, RoomState, RoundState};
use crate::projection::SeatProjectionSupport;
use crate::room_scoring::RoomScoringCache;
use crate::rules::scoring::{
    EvaluationInput, FanBreakdownEntry, TimingFeatures, decompose_winning_hand_with_melds,
    evaluate_fans, extract_hand_features,
};
use crate::rules::standard::win::classify_meld_groups_for_projection;

const STANDARD_TILE_KEYS: [&str; 34] = [
    "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "t1", "t2", "t3", "t4", "t5", "t6", "t7",
    "t8", "t9", "b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9", "east", "south", "west",
    "north", "red", "green", "white",
];
const WIND_ORDER: [&str; 4] = ["east", "south", "west", "north"];

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
    winning_fans: Vec<HandInsightWinningFanView>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HandInsightWaitView {
    code: String,
    available_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HandInsightWinningFanView {
    fan_key: String,
    fan_value: i64,
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
        &cache,
        local_player,
        local_seat,
        current_concealed_tile_keys,
        None,
        &known_tile_counts,
        &public_visible_tile_keys,
    ));

    Some(HandInsightsView {
        current,
        by_discard_tile_id,
    })
}

fn build_discard_preview_map(
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
                cache,
                local_player,
                local_seat,
                concealed_tile_keys,
                Some((&tile.tile_id, &tile.tile_key)),
                known_tile_counts,
                &preview_visible_tile_keys,
            );
            (tile.tile_id.clone(), insight)
        })
        .collect()
}

fn build_insight(
    cache: &RoomScoringCache,
    local_player: &PlayerRoundState,
    local_seat: Seat,
    concealed_tile_keys: Vec<String>,
    discard_tile: Option<(&str, &str)>,
    known_tile_counts: &HashMap<String, i64>,
    public_visible_tile_keys: &[String],
) -> HandInsightView {
    let waits = build_waits(&concealed_tile_keys, &local_player.melds, known_tile_counts);
    let winning_fans = if !waits.is_empty() {
        build_winning_fans(
            cache,
            local_player,
            local_seat,
            &concealed_tile_keys,
            &waits,
            public_visible_tile_keys,
        )
    } else if discard_tile.is_none() {
        evaluate_current_winning_hand(
            cache,
            local_player,
            local_seat,
            &concealed_tile_keys,
            public_visible_tile_keys,
        )
    } else {
        Vec::new()
    };

    HandInsightView {
        discard_tile_id: discard_tile.map(|(tile_id, _)| tile_id.to_string()),
        discard_tile_code: discard_tile.map(|(_, tile_key)| tile_key.to_string()),
        is_tenpai: !waits.is_empty(),
        waits,
        winning_fans,
    }
}

fn evaluate_current_winning_hand(
    cache: &RoomScoringCache,
    local_player: &PlayerRoundState,
    local_seat: Seat,
    concealed_tile_keys: &[String],
    public_visible_tile_keys: &[String],
) -> Vec<HandInsightWinningFanView> {
    let decompositions =
        decompose_winning_hand_with_melds(concealed_tile_keys, &local_player.melds);
    if decompositions.is_empty() {
        return Vec::new();
    }

    let (open_meld_tile_key_groups, meld_open_flags) =
        classify_meld_groups_for_projection(local_seat, &local_player.melds, &cache.kong_entries);
    let features = extract_hand_features(
        concealed_tile_keys,
        &local_player.melds,
        Some(&meld_open_flags),
        None,
        Some(&seat_wind_key(local_seat, cache.dealer_seat)),
        cache.round_wind.as_deref(),
        Some(&decompositions),
    );
    let fan_result = evaluate_fans(EvaluationInput {
        win_type: "self_draw".to_string(),
        winner_seat: Some(local_seat),
        discarder_seat: None,
        ready_hand_declared: local_player.is_ready_hand,
        flower_count: local_player.flowers.len(),
        seat_count: cache.seat_count,
        features,
        timing: TimingFeatures::default(),
        kong_entries: cache.kong_entries.clone(),
        tile_keys: player_tile_keys_from_parts(concealed_tile_keys, &local_player.melds, None),
        visible_tile_keys: public_visible_tile_keys.to_vec(),
        concealed_tile_keys: concealed_tile_keys.to_vec(),
        meld_tile_key_groups: local_player.melds.clone(),
        open_meld_tile_key_groups,
        incoming_tile: None,
        winning_tile: concealed_tile_keys.last().cloned(),
        decompositions,
    });
    let mut winning_fans = fan_result
        .fan_breakdown
        .into_iter()
        .map(|entry| HandInsightWinningFanView {
            fan_key: entry.fan_key,
            fan_value: entry.fan_value,
        })
        .collect::<Vec<_>>();
    sort_winning_fans(&mut winning_fans);
    winning_fans
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

fn build_winning_fans(
    cache: &RoomScoringCache,
    local_player: &PlayerRoundState,
    local_seat: Seat,
    concealed_tile_keys: &[String],
    waits: &[HandInsightWaitView],
    public_visible_tile_keys: &[String],
) -> Vec<HandInsightWinningFanView> {
    let mut fan_value_by_key = BTreeMap::<String, i64>::new();
    for wait in waits.iter().filter(|wait| wait.available_count > 0) {
        for self_draw in [false, true] {
            if let Some(fan_breakdown) = evaluate_wait_scenario(
                cache,
                local_player,
                local_seat,
                concealed_tile_keys,
                &wait.code,
                public_visible_tile_keys,
                self_draw,
            ) {
                for entry in fan_breakdown {
                    fan_value_by_key
                        .entry(entry.fan_key)
                        .and_modify(|fan_value| *fan_value = (*fan_value).max(entry.fan_value))
                        .or_insert(entry.fan_value);
                }
            }
        }
    }

    let mut winning_fans = fan_value_by_key
        .into_iter()
        .map(|(fan_key, fan_value)| HandInsightWinningFanView { fan_key, fan_value })
        .collect::<Vec<_>>();
    sort_winning_fans(&mut winning_fans);
    winning_fans
}

fn evaluate_wait_scenario(
    cache: &RoomScoringCache,
    local_player: &PlayerRoundState,
    local_seat: Seat,
    concealed_tile_keys: &[String],
    wait_tile_key: &str,
    public_visible_tile_keys: &[String],
    self_draw: bool,
) -> Option<Vec<FanBreakdownEntry>> {
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
    let visible_tile_keys = public_visible_tile_keys.to_vec();
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
        winning_tile: Some(wait_tile_key.to_string()),
        decompositions,
    });

    Some(fan_result.fan_breakdown)
}

fn sort_winning_fans(winning_fans: &mut Vec<HandInsightWinningFanView>) {
    winning_fans.sort_by(|left, right| {
        right
            .fan_value
            .cmp(&left.fan_value)
            .then_with(|| left.fan_key.cmp(&right.fan_key))
    });
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

    use super::{HandInsightView, build_hand_insights_view};

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
            extended_with_extra: false,
        });

        let support = SeatProjectionSupport {
            can_ready_hand: true,
            ..Default::default()
        };
        let snapshot = room_snapshot_message(&state, 0, &support);
        let hand_insights = &snapshot["payload"]["private_state"]["hand_insights"];

        assert!(hand_insights["current"]["winning_fans"].is_array());
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
    fn non_tenpai_hand_has_no_winning_fans() {
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
        let current = insights.current.expect("current insight");

        assert!(!current.is_tenpai);
        assert!(current.winning_fans.is_empty());
    }

    #[test]
    fn current_tenpai_reports_actual_winning_fans() {
        let mut state = sample_state();
        state.round_state.as_mut().unwrap().players[0].concealed_tiles =
            low_fan_all_chows_tenpai_tiles();

        let insights = build_hand_insights_view(&state, 0, &SeatProjectionSupport::default())
            .expect("insights");
        let current = insights.current.expect("current insight");
        let keys = winning_fan_keys(&current);

        assert!(current.is_tenpai);
        assert!(keys.contains(&"all_chows"));
    }

    #[test]
    fn current_winning_hand_reports_actual_winning_fans() {
        let mut state = sample_state();
        let mut concealed_tiles = low_fan_all_chows_tenpai_tiles();
        concealed_tiles.push(suit_tile("b4#0", "b4"));
        state.round_state.as_mut().unwrap().players[0].concealed_tiles = concealed_tiles;

        let insights = build_hand_insights_view(&state, 0, &SeatProjectionSupport::default())
            .expect("insights");
        let current = insights.current.expect("current insight");
        let keys = winning_fan_keys(&current);

        assert!(!current.is_tenpai);
        assert!(keys.contains(&"all_chows"));
    }

    #[test]
    fn selected_discard_tenpai_reports_actual_winning_fans() {
        let mut state = sample_state();
        let mut concealed_tiles = low_fan_all_chows_tenpai_tiles();
        concealed_tiles.push(suit_tile("east#0", "east"));
        state.round_state.as_mut().unwrap().players[0].concealed_tiles = concealed_tiles;

        let insights = build_hand_insights_view(&state, 0, &SeatProjectionSupport::default())
            .expect("insights");
        let preview = insights
            .by_discard_tile_id
            .get("east#0")
            .expect("east discard should leave the hand tenpai");
        let keys = winning_fan_keys(preview);

        assert!(preview.is_tenpai);
        assert!(keys.contains(&"all_chows"));
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

    fn low_fan_all_chows_tenpai_tiles() -> Vec<Tile> {
        vec![
            suit_tile("w1#0", "w1"),
            suit_tile("w2#0", "w2"),
            suit_tile("w3#0", "w3"),
            suit_tile("w4#0", "w4"),
            suit_tile("w5#0", "w5"),
            suit_tile("w6#0", "w6"),
            suit_tile("t2#0", "t2"),
            suit_tile("t3#0", "t3"),
            suit_tile("t4#0", "t4"),
            suit_tile("b2#0", "b2"),
            suit_tile("b3#0", "b3"),
            suit_tile("b5#0", "b5"),
            suit_tile("b5#1", "b5"),
        ]
    }

    fn winning_fan_keys(insight: &HandInsightView) -> Vec<&str> {
        insight
            .winning_fans
            .iter()
            .map(|entry| entry.fan_key.as_str())
            .collect()
    }

    fn sample_state() -> RoomState {
        RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            minimum_hu_fan: crate::core::state::room::default_minimum_hu_fan(),
            dealer_repeat_enabled: false,
            dealer_double_enabled: false,
            seats: (0..4)
                .map(|seat_index| SeatState {
                    seat_index,
                    connected: true,
                    seat_type: "human".to_string(),
                    ..Default::default()
                })
                .collect(),
            match_state: Some(MatchState {
                prevailing_wind: "east".to_string(),
                hand_number: 1,
                dealer_seat: 0,
                dealer_repeat_count: 0,
                cumulative_scores: BTreeMap::from([(0, 0), (1, 0), (2, 0), (3, 0)]),
                match_finished: false,
                last_completed_round_id: None,
                statistics: Default::default(),
                extra_time_pool: Default::default(),
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
                discard_history: Vec::new(),
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
