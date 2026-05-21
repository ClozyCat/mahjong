use super::action_space::{
    CLAIM_ACTION_COUNT, SELF_KONG_ACTION_COUNT, TILE_KIND_COUNT, claim_action_index,
    self_kong_action_index, tile_index,
};
use super::context::{BotContext, BotSelfKongKind, seat_wind_key};
use crate::room_scoring::RoomScoringCache;

const TILE_PLANE_COUNT: usize = 10;
const SCALAR_FEATURE_COUNT: usize = 12;
const DISCARD_SEQUENCE_LENGTH: usize = 32;
const DISCARD_EVENT_FEATURE_COUNT: usize = 40;

const GLOBAL_TILE_PLANE_COUNT: usize = 40;
const GLOBAL_SCALAR_FEATURE_COUNT: usize = 20;

#[derive(Clone, Debug)]
pub(crate) struct BotFeaturesV2 {
    pub(crate) tile_planes: Vec<f32>,
    pub(crate) scalar_features: Vec<f32>,
    pub(crate) discard_sequence: Vec<f32>,
    pub(crate) discard_mask: [bool; TILE_KIND_COUNT],
    pub(crate) claim_mask: [bool; CLAIM_ACTION_COUNT],
    pub(crate) self_kong_mask: [bool; SELF_KONG_ACTION_COUNT],
    pub(crate) hu_mask: [bool; 2],
}

pub(crate) fn encode_bot_context_v2(context: &BotContext) -> BotFeaturesV2 {
    BotFeaturesV2 {
        tile_planes: encode_tile_planes(context),
        scalar_features: encode_scalar_features(context),
        discard_sequence: encode_discard_sequence(context),
        discard_mask: legal_discard_mask(context),
        claim_mask: legal_claim_mask(context),
        self_kong_mask: legal_self_kong_mask(context),
        hu_mask: legal_hu_mask(context),
    }
}

pub(crate) fn tile_plane_count_v2() -> usize {
    TILE_PLANE_COUNT
}

pub(crate) fn scalar_feature_count_v2() -> usize {
    SCALAR_FEATURE_COUNT
}

pub(crate) fn discard_sequence_length_v2() -> usize {
    DISCARD_SEQUENCE_LENGTH
}

pub(crate) fn discard_event_feature_count_v2() -> usize {
    DISCARD_EVENT_FEATURE_COUNT
}

fn encode_tile_planes(context: &BotContext) -> Vec<f32> {
    let mut planes = vec![0.0_f32; TILE_PLANE_COUNT * TILE_KIND_COUNT];

    set_count_plane(
        &mut planes,
        0,
        context
            .player
            .concealed_tiles
            .iter()
            .filter(|tile| !tile.is_flower)
            .map(|tile| tile.tile_key.as_str()),
    );
    set_count_plane(
        &mut planes,
        1,
        context
            .player
            .meld_tile_key_groups
            .iter()
            .flat_map(|group| group.iter().map(String::as_str)),
    );
    set_count_plane(
        &mut planes,
        2,
        context.visible_tile_keys.iter().map(String::as_str),
    );

    for offset in 1..=3 {
        let seat = (context.seat_index + offset) % context.seat_count.max(1);
        if let Some(discards) = context.opponent_discards_by_seat.get(seat) {
            set_count_plane(
                &mut planes,
                2 + offset * 2 - 1,
                discards.iter().map(String::as_str),
            );
        }
        if let Some(melds) = context.opponent_melds_by_seat.get(seat) {
            set_count_plane(
                &mut planes,
                2 + offset * 2,
                melds
                    .iter()
                    .flat_map(|group| group.iter().map(String::as_str)),
            );
        }
    }

    if let Some(tile_key) = context.last_discard_tile_key.as_deref() {
        set_binary_tile(&mut planes, 9, tile_key);
    }

    planes
}

fn encode_discard_sequence(context: &BotContext) -> Vec<f32> {
    let mut sequence = vec![0.0_f32; DISCARD_SEQUENCE_LENGTH * DISCARD_EVENT_FEATURE_COUNT];
    let retained_len = context.discard_history.len().min(DISCARD_SEQUENCE_LENGTH);
    let start_history = context.discard_history.len().saturating_sub(retained_len);
    let start_slot = DISCARD_SEQUENCE_LENGTH - retained_len;
    let seat_count = context.seat_count.max(1);

    for (offset, event) in context.discard_history[start_history..].iter().enumerate() {
        let slot = start_slot + offset;
        let base = slot * DISCARD_EVENT_FEATURE_COUNT;
        if let Some(index) = tile_index(&event.tile_key) {
            sequence[base + index] = 1.0;
        }
        let relative_seat = (event.seat_index + seat_count - context.seat_index) % seat_count;
        if relative_seat < 4 {
            sequence[base + TILE_KIND_COUNT + relative_seat] = 1.0;
        }
        sequence[base + 38] = (slot + 1) as f32 / DISCARD_SEQUENCE_LENGTH as f32;
        if offset + 1 == retained_len {
            sequence[base + 39] = 1.0;
        }
    }

    sequence
}

fn encode_scalar_features(context: &BotContext) -> Vec<f32> {
    let mut features = vec![0.0_f32; SCALAR_FEATURE_COUNT];
    features[0] = context.seat_index as f32 / 3.0;
    features[1] = context.dealer_seat as f32 / 3.0;
    features[2] = context.wall_tiles_remaining.max(0) as f32 / 84.0;
    features[3] = context.player.meld_tile_key_groups.len() as f32 / 4.0;
    features[4] = context.player.flower_count as f32 / 8.0;
    features[5] = f32::from(context.restricted_discard_tile_key.is_some());
    features[6] = f32::from(context.drawn_tile_id.is_some());
    features[7] = context.self_kong_candidates.len() as f32 / 4.0;
    features[8] = context.claim_options.len() as f32 / 4.0;
    features[9] = context
        .cumulative_scores
        .get(context.seat_index)
        .copied()
        .unwrap_or(0) as f32
        / 100.0;
    let round_wind_index = standard_wind_index(context.round_wind.as_deref().unwrap_or("east"));
    features[10] = round_wind_index as f32 / 3.0;
    let seat_wind = seat_wind_key(context.seat_index, context.dealer_seat);
    features[11] = f32::from(context.round_wind.as_deref() == Some(seat_wind.as_str()));
    features
}

fn standard_wind_index(wind: &str) -> usize {
    match wind {
        "south" => 1,
        "west" => 2,
        "north" => 3,
        _ => 0,
    }
}

fn legal_discard_mask(context: &BotContext) -> [bool; TILE_KIND_COUNT] {
    let mut mask = [false; TILE_KIND_COUNT];
    for tile in &context.player.concealed_tiles {
        if tile.is_flower
            || Some(tile.tile_key.as_str()) == context.restricted_discard_tile_key.as_deref()
        {
            continue;
        }
        if let Some(index) = tile_index(&tile.tile_key) {
            mask[index] = true;
        }
    }
    mask
}

fn legal_claim_mask(context: &BotContext) -> [bool; CLAIM_ACTION_COUNT] {
    let mut mask = [false; CLAIM_ACTION_COUNT];
    mask[claim_action_index("pass").expect("pass action")] = true;
    for option in &context.claim_options {
        if let Some(index) = claim_action_index(claim_mask_action_name(context, option)) {
            mask[index] = true;
        }
    }
    mask
}

fn claim_mask_action_name<'a>(
    context: &BotContext,
    option: &'a crate::projection::bot_view::BotClaimOption,
) -> &'a str {
    if option.action_type != "chow" {
        return option.action_type.as_str();
    }
    chow_action_name(context, option).unwrap_or("chow_mid")
}

fn chow_action_name(
    context: &BotContext,
    option: &crate::projection::bot_view::BotClaimOption,
) -> Option<&'static str> {
    let last_discard = context.last_discard_tile_key.as_deref()?;
    let discard_index = tile_index(last_discard)?;
    if discard_index >= 27 {
        return Some("chow_mid");
    }

    let mut keys = vec![last_discard.to_string()];
    for tile_id in &option.tile_ids {
        let tile = context
            .player
            .concealed_tiles
            .iter()
            .find(|tile| &tile.tile_id == tile_id)?;
        keys.push(tile.tile_key.clone());
    }

    keys.sort_by_key(|key| tile_index(key).unwrap_or(usize::MAX));
    let middle_index = tile_index(keys.get(1)?)?;
    if middle_index >= 27 || middle_index / 9 != discard_index / 9 {
        return Some("chow_mid");
    }
    if discard_index == middle_index - 1 {
        return Some("chow_left");
    }
    if discard_index == middle_index + 1 {
        return Some("chow_right");
    }
    Some("chow_mid")
}

fn legal_self_kong_mask(context: &BotContext) -> [bool; SELF_KONG_ACTION_COUNT] {
    let mut mask = [false; SELF_KONG_ACTION_COUNT];
    mask[self_kong_action_index("pass").expect("pass action")] = true;
    for candidate in &context.self_kong_candidates {
        let action_name = match candidate.kind {
            BotSelfKongKind::Concealed => "concealed_kong",
            BotSelfKongKind::Add => "add_kong",
        };
        if let Some(index) = self_kong_action_index(action_name) {
            mask[index] = true;
        }
    }
    mask
}

fn legal_hu_mask(context: &BotContext) -> [bool; 2] {
    let can_hu = context
        .claim_options
        .iter()
        .any(|option| option.action_type == "hu");
    [true, can_hu]
}

fn set_count_plane<'a>(planes: &mut [f32], plane: usize, tile_keys: impl Iterator<Item = &'a str>) {
    for tile_key in tile_keys {
        if let Some(index) = tile_index(tile_key) {
            let offset = plane * TILE_KIND_COUNT + index;
            planes[offset] = (planes[offset] + 1.0).min(4.0);
        }
    }
}

fn set_binary_tile(planes: &mut [f32], plane: usize, tile_key: &str) {
    if let Some(index) = tile_index(tile_key) {
        planes[plane * TILE_KIND_COUNT + index] = 1.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::bot_view::{BotContextView, BotPlayerView, BotTileView};
    use std::collections::HashSet;

    fn sample_context_with_tiles(tile_keys: &[&str]) -> BotContext {
        let concealed_tiles = tile_keys
            .iter()
            .enumerate()
            .map(|(index, tile_key)| BotTileView {
                tile_id: format!("{tile_key}#{index}"),
                tile_key: (*tile_key).to_string(),
                is_flower: false,
            })
            .collect::<Vec<_>>();
        let mut counts = [0_u8; TILE_KIND_COUNT];
        for tile_key in tile_keys {
            if let Some(index) = tile_index(tile_key) {
                counts[index] = counts[index].saturating_add(1);
            }
        }

        BotContextView {
            seat_index: 0,
            seat_count: 4,
            dealer_seat: 0,
            round_wind: Some("east".to_string()),
            minimum_hu_fan: crate::core::state::room::default_minimum_hu_fan(),
            cumulative_scores: vec![0, 0, 0, 0],
            wall_tiles_remaining: 42,
            visible_tile_keys: Vec::new(),
            opponent_discards_by_seat: vec![vec![], vec![], vec![], vec![]],
            opponent_melds_by_seat: vec![vec![], vec![], vec![], vec![]],
            discard_history: Vec::new(),
            kong_entries: Vec::new(),
            player: BotPlayerView {
                concealed_tiles,
                concealed_tile_counts: counts,
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

    #[test]
    fn active_turn_mask_allows_only_unrestricted_concealed_tiles() {
        let mut context = sample_context_with_tiles(&["w1", "w1", "t5", "red"]);
        context.restricted_discard_tile_key = Some("w1".to_string());

        let encoded = encode_bot_context_v2(&context);

        assert!(!encoded.discard_mask[tile_index("w1").unwrap()]);
        assert!(encoded.discard_mask[tile_index("t5").unwrap()]);
        assert!(encoded.discard_mask[tile_index("red").unwrap()]);
        assert!(!encoded.discard_mask[tile_index("b9").unwrap()]);
    }

    #[test]
    fn chow_claim_mask_preserves_left_middle_right_direction() {
        use crate::bot::action_space::claim_action_index;
        use crate::projection::bot_view::BotClaimOption;

        let mut context = sample_context_with_tiles(&["w4", "w5", "t1"]);
        context.last_discard_tile_key = Some("w3".to_string());
        context.claim_options = vec![BotClaimOption {
            action_type: "chow".to_string(),
            tile_ids: vec!["w4#0".to_string(), "w5#1".to_string()],
        }];

        let encoded = encode_bot_context_v2(&context);

        assert!(encoded.claim_mask[claim_action_index("pass").unwrap()]);
        assert!(encoded.claim_mask[claim_action_index("chow_left").unwrap()]);
        assert!(!encoded.claim_mask[claim_action_index("chow_mid").unwrap()]);
        assert!(!encoded.claim_mask[claim_action_index("chow_right").unwrap()]);
    }

    #[test]
    fn feature_shapes_are_stable() {
        let context = sample_context_with_tiles(&["w1", "t5", "red"]);
        let encoded = encode_bot_context_v2(&context);

        assert_eq!(
            encoded.tile_planes.len(),
            tile_plane_count_v2() * TILE_KIND_COUNT
        );
        assert_eq!(encoded.scalar_features.len(), scalar_feature_count_v2());
        assert_eq!(
            encoded.discard_sequence.len(),
            discard_sequence_length_v2() * discard_event_feature_count_v2()
        );
        assert_eq!(encoded.claim_mask.len(), CLAIM_ACTION_COUNT);
        assert_eq!(encoded.self_kong_mask.len(), SELF_KONG_ACTION_COUNT);
        assert_eq!(encoded.hu_mask.len(), 2);
    }

    #[test]
    fn scalar_features_include_round_wind_and_seat_wind_match() {
        let mut context = sample_context_with_tiles(&["w1", "t5", "red"]);
        context.seat_index = 1;
        context.dealer_seat = 0;
        context.round_wind = Some("south".to_string());

        let encoded = encode_bot_context_v2(&context);

        assert_eq!(encoded.scalar_features.len(), 12);
        assert_eq!(encoded.scalar_features[10], 1.0 / 3.0);
        assert_eq!(encoded.scalar_features[11], 1.0);
    }

    #[test]
    fn scalar_features_do_not_treat_absolute_seat_as_seat_wind() {
        let mut context = sample_context_with_tiles(&["w1", "t5", "red"]);
        context.seat_index = 1;
        context.dealer_seat = 0;
        context.round_wind = Some("north".to_string());

        let encoded = encode_bot_context_v2(&context);

        assert_eq!(encoded.scalar_features[10], 3.0 / 3.0);
        assert_eq!(encoded.scalar_features[11], 0.0);
    }

    #[test]
    fn discard_sequence_encodes_relative_source_and_latest_marker() {
        let mut context = sample_context_with_tiles(&["w1", "t5", "red"]);
        context.discard_history = vec![
            crate::projection::bot_view::BotDiscardEventView {
                seat_index: 1,
                tile_key: "w3".to_string(),
            },
            crate::projection::bot_view::BotDiscardEventView {
                seat_index: 2,
                tile_key: "t5".to_string(),
            },
        ];

        let encoded = encode_bot_context_v2(&context);
        let previous = (DISCARD_SEQUENCE_LENGTH - 2) * DISCARD_EVENT_FEATURE_COUNT;
        let latest = (DISCARD_SEQUENCE_LENGTH - 1) * DISCARD_EVENT_FEATURE_COUNT;

        assert_eq!(
            encoded.discard_sequence[previous + tile_index("w3").unwrap()],
            1.0
        );
        assert_eq!(
            encoded.discard_sequence[previous + TILE_KIND_COUNT + 1],
            1.0
        );
        assert_eq!(encoded.discard_sequence[previous + 39], 0.0);
        assert_eq!(
            encoded.discard_sequence[latest + tile_index("t5").unwrap()],
            1.0
        );
        assert_eq!(encoded.discard_sequence[latest + TILE_KIND_COUNT + 2], 1.0);
        assert_eq!(encoded.discard_sequence[latest + 39], 1.0);
    }
}

pub(crate) fn encode_global_features_v2(
    cache: &RoomScoringCache,
    current_seat: usize,
) -> (Vec<f32>, Vec<f32>) {
    let global_tile_planes = encode_global_tile_planes_v2(cache, current_seat);
    let global_scalar_features = encode_global_scalar_features_v2(cache, current_seat);
    (global_tile_planes, global_scalar_features)
}

#[cfg(test)]
pub(crate) fn global_tile_plane_count_v2() -> usize {
    GLOBAL_TILE_PLANE_COUNT
}

#[cfg(test)]
pub(crate) fn global_scalar_feature_count_v2() -> usize {
    GLOBAL_SCALAR_FEATURE_COUNT
}

fn encode_global_tile_planes_v2(cache: &RoomScoringCache, current_seat: usize) -> Vec<f32> {
    let mut planes = vec![0.0_f32; GLOBAL_TILE_PLANE_COUNT * TILE_KIND_COUNT];

    for player_offset in 0..cache.seat_count.min(4) {
        let absolute_seat = (current_seat + player_offset) % cache.seat_count.max(1);
        let base_plane = player_offset * TILE_PLANE_COUNT;

        if let Some(player) = cache.player(absolute_seat) {
            set_count_plane(
                &mut planes,
                base_plane,
                player.concealed_tile_keys.iter().map(String::as_str),
            );

            set_count_plane(
                &mut planes,
                base_plane + 1,
                player
                    .meld_tile_key_groups
                    .iter()
                    .flat_map(|group| group.iter().map(String::as_str)),
            );
        }

        set_count_plane(
            &mut planes,
            base_plane + 2,
            cache.visible_tile_keys.iter().map(String::as_str),
        );

        for opponent_offset in 1..=3 {
            let opponent_seat = (absolute_seat + opponent_offset) % cache.seat_count.max(1);
            if let Some(discards) = cache.opponent_discards_by_seat.get(opponent_seat) {
                set_count_plane(
                    &mut planes,
                    base_plane + 2 + opponent_offset * 2 - 1,
                    discards.iter().map(String::as_str),
                );
            }
            if let Some(melds) = cache.opponent_melds_by_seat.get(opponent_seat) {
                set_count_plane(
                    &mut planes,
                    base_plane + 2 + opponent_offset * 2,
                    melds
                        .iter()
                        .flat_map(|group| group.iter().map(String::as_str)),
                );
            }
        }

        if let Some(tile_key) = cache.last_discard_tile_key.as_deref() {
            set_binary_tile(&mut planes, base_plane + 9, tile_key);
        }
    }

    planes
}

fn encode_global_scalar_features_v2(cache: &RoomScoringCache, current_seat: usize) -> Vec<f32> {
    let mut features = vec![0.0_f32; GLOBAL_SCALAR_FEATURE_COUNT];
    let seat_count = cache.seat_count.max(1);

    for player_offset in 0..cache.seat_count.min(4) {
        let absolute_seat = (current_seat + player_offset) % seat_count;
        let base_idx = player_offset * 4;

        features[base_idx] = player_offset as f32 / 3.0;

        if let Some(player) = cache.player(absolute_seat) {
            features[base_idx + 1] = player.meld_tile_key_groups.len() as f32 / 4.0;
            features[base_idx + 2] = player.flower_count as f32 / 8.0;
        }

        features[base_idx + 3] = cache
            .cumulative_scores
            .get(absolute_seat)
            .copied()
            .unwrap_or(0) as f32
            / 100.0;
    }

    features[16] = cache.dealer_seat as f32 / 3.0;
    features[17] = cache.wall_tiles_remaining.max(0) as f32 / 84.0;

    let round_wind_index = standard_wind_index(cache.round_wind.as_deref().unwrap_or("east"));
    features[18] = round_wind_index as f32 / 3.0;

    features[19] = f32::from(cache.drawn_tile_id.is_some());

    features
}

#[cfg(test)]
mod global_features_tests {
    use super::*;
    use crate::core::state::{
        LastActionContext, MatchState, PendingTimeout, PlayerRoundState, RoomState,
        RoundScoreTrackers, RoundState, RuleRuntimeState, WallState,
    };
    use crate::core::tile::Tile;
    use crate::room_scoring::RoomScoringCache;
    use std::collections::BTreeMap;

    fn sample_global_cache() -> RoomScoringCache {
        let state = RoomState {
            table_code: "TEST".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            minimum_hu_fan: crate::core::state::room::default_minimum_hu_fan(),
            dealer_repeat_enabled: false,
            dealer_double_enabled: false,
            seats: Vec::new(),
            match_state: Some(MatchState {
                seed: 0,
                prevailing_wind: "east".to_string(),
                hand_number: 1,
                dealer_seat: 0,
                dealer_repeat_count: 0,
                cumulative_scores: BTreeMap::from([(0, 100), (1, -50), (2, 0), (3, 50)]),
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
                    tail_index: 41,
                },
                players: vec![
                    PlayerRoundState {
                        seat: 0,
                        is_ready_hand: false,
                        concealed_tiles: vec![
                            Tile::tile_key_only("w1"),
                            Tile::tile_key_only("w2"),
                            Tile::tile_key_only("w3"),
                        ],
                        melds: vec![vec!["t1".to_string(), "t2".to_string(), "t3".to_string()]],
                        display_melds: Vec::new(),
                        discards: vec![Tile::tile_key_only("w9")],
                        flowers: vec![Tile::tile_key_only("plum")],
                    },
                    PlayerRoundState {
                        seat: 1,
                        is_ready_hand: false,
                        concealed_tiles: vec![Tile::tile_key_only("b5"), Tile::tile_key_only("b6")],
                        melds: Vec::new(),
                        display_melds: Vec::new(),
                        discards: vec![Tile::tile_key_only("t9")],
                        flowers: Vec::new(),
                    },
                    PlayerRoundState {
                        seat: 2,
                        is_ready_hand: false,
                        concealed_tiles: vec![
                            Tile::tile_key_only("red"),
                            Tile::tile_key_only("green"),
                        ],
                        melds: Vec::new(),
                        display_melds: Vec::new(),
                        discards: Vec::new(),
                        flowers: vec![Tile::tile_key_only("orchid"), Tile::tile_key_only("bamboo")],
                    },
                    PlayerRoundState {
                        seat: 3,
                        is_ready_hand: false,
                        concealed_tiles: vec![Tile::tile_key_only("east")],
                        melds: Vec::new(),
                        display_melds: Vec::new(),
                        discards: Vec::new(),
                        flowers: Vec::new(),
                    },
                ],
                discard_history: Vec::new(),
                pending_action: None,
                last_discard: Some(Tile::tile_key_only("w9")),
                restricted_discard_tile_key: None,
                score_trackers: RoundScoreTrackers::default(),
                last_action_context: LastActionContext::default(),
                rule_state: RuleRuntimeState {},
                settlement: None,
                version: 1,
            }),
            pending_timeout: Some(PendingTimeout {
                kind: "active_turn".to_string(),
                seat_index: 0,
                deadline_at: None,
                drawn_tile_id: Some("tile#123".to_string()),
                extended_with_extra: false,
            }),
            continue_action: None,
        };

        RoomScoringCache::from_state(&state)
    }

    #[test]
    fn global_feature_shapes_are_correct() {
        let cache = sample_global_cache();
        let (tile_planes, scalar_features) = encode_global_features_v2(&cache, 0);

        assert_eq!(
            tile_planes.len(),
            global_tile_plane_count_v2() * TILE_KIND_COUNT
        );
        assert_eq!(scalar_features.len(), global_scalar_feature_count_v2());
        assert_eq!(tile_planes.len(), 40 * 34);
        assert_eq!(scalar_features.len(), 20);
    }

    #[test]
    fn global_tile_planes_encode_all_players() {
        let cache = sample_global_cache();
        let (tile_planes, _) = encode_global_features_v2(&cache, 0);

        let player0_base = 0 * TILE_KIND_COUNT;
        assert_eq!(tile_planes[player0_base + tile_index("w1").unwrap()], 1.0);
        assert_eq!(tile_planes[player0_base + tile_index("w2").unwrap()], 1.0);

        let player1_base = 10 * TILE_KIND_COUNT;
        assert_eq!(tile_planes[player1_base + tile_index("b5").unwrap()], 1.0);
    }

    #[test]
    fn global_scalar_features_encode_per_player_stats() {
        let cache = sample_global_cache();
        let (_, scalar_features) = encode_global_features_v2(&cache, 0);

        assert_eq!(scalar_features[0], 0.0 / 3.0);
        assert_eq!(scalar_features[1], 1.0 / 4.0);
        assert_eq!(scalar_features[2], 1.0 / 8.0);
        assert_eq!(scalar_features[3], 100.0 / 100.0);

        assert_eq!(scalar_features[4], 1.0 / 3.0);
        assert_eq!(scalar_features[5], 0.0 / 4.0);
        assert_eq!(scalar_features[6], 0.0 / 8.0);
        assert_eq!(scalar_features[7], -50.0 / 100.0);
    }

    #[test]
    fn global_scalar_features_encode_shared_stats() {
        let cache = sample_global_cache();
        let (_, scalar_features) = encode_global_features_v2(&cache, 0);

        assert_eq!(scalar_features[16], 0.0 / 3.0);
        assert_eq!(scalar_features[17], 42.0 / 84.0);
        assert_eq!(scalar_features[18], 0.0 / 3.0);
        assert_eq!(scalar_features[19], 1.0);
    }

    #[test]
    fn global_features_handle_relative_seat_transformation() {
        let cache = sample_global_cache();
        let (tile_planes, scalar_features) = encode_global_features_v2(&cache, 1);

        let player0_base = 0 * TILE_KIND_COUNT;
        assert_eq!(tile_planes[player0_base + tile_index("b5").unwrap()], 1.0);

        assert_eq!(scalar_features[0], 0.0 / 3.0);
        assert_eq!(scalar_features[3], -50.0 / 100.0);
    }
}
