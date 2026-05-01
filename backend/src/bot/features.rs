use super::action_space::{
    CLAIM_ACTION_COUNT, SELF_KONG_ACTION_COUNT, TILE_KIND_COUNT, claim_action_index,
    self_kong_action_index, tile_index,
};
use super::context::{BotContext, BotSelfKongKind};

const TILE_PLANE_COUNT: usize = 10;
const SCALAR_FEATURE_COUNT: usize = 10;
const DISCARD_SEQUENCE_LENGTH: usize = 64;
const DISCARD_EVENT_FEATURE_COUNT: usize = TILE_KIND_COUNT + 4;

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
    features
}

fn encode_discard_sequence(context: &BotContext) -> Vec<f32> {
    let mut sequence = vec![0.0_f32; DISCARD_SEQUENCE_LENGTH * DISCARD_EVENT_FEATURE_COUNT];
    let start = context
        .discard_history
        .len()
        .saturating_sub(DISCARD_SEQUENCE_LENGTH);
    for (row_index, event) in context.discard_history.iter().skip(start).enumerate() {
        let Some(tile_index) = tile_index(&event.tile_key) else {
            continue;
        };
        let relative_seat = (event.seat_index + context.seat_count.max(1) - context.seat_index)
            % context.seat_count.max(1);
        let row_offset = row_index * DISCARD_EVENT_FEATURE_COUNT;
        sequence[row_offset + tile_index] = 1.0;
        if relative_seat < 4 {
            sequence[row_offset + TILE_KIND_COUNT + relative_seat] = 1.0;
        }
    }
    sequence
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
    fn discard_sequence_encodes_tile_and_relative_seat_order() {
        use crate::projection::bot_view::BotDiscardEventView;

        let mut context = sample_context_with_tiles(&["w1", "t5", "red"]);
        context.discard_history = vec![
            BotDiscardEventView {
                seat_index: 3,
                tile_key: "w1".to_string(),
            },
            BotDiscardEventView {
                seat_index: 0,
                tile_key: "t1".to_string(),
            },
        ];

        let encoded = encode_bot_context_v2(&context);
        let width = discard_event_feature_count_v2();

        assert_eq!(encoded.discard_sequence[0], 1.0);
        assert_eq!(encoded.discard_sequence[TILE_KIND_COUNT + 3], 1.0);
        assert_eq!(encoded.discard_sequence[width + 9], 1.0);
        assert_eq!(encoded.discard_sequence[width + TILE_KIND_COUNT], 1.0);
        assert_eq!(
            encoded.discard_sequence[(2 * width)..(3 * width)]
                .iter()
                .sum::<f32>(),
            0.0
        );
    }
}
