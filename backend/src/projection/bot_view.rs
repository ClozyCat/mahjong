use std::collections::HashSet;

use crate::core::state::RoomState;
use crate::room_scoring::RoomScoringCache;
use crate::rules::scoring::KongEntry as ScoringKongEntry;
#[cfg(not(test))]
pub use crate::rules::standard::meld::{
    SelfKongCandidate as BotSelfKongCandidate, SelfKongKind as BotSelfKongKind,
};
#[cfg(test)]
pub use crate::rules::standard::meld::{
    SelfKongCandidate as BotSelfKongCandidate, SelfKongKind as BotSelfKongKind,
    claim_tile_id_options,
};

pub type BotTileCounts = [u8; 34];

#[derive(Clone)]
pub struct BotTileView {
    pub tile_id: String,
    pub tile_key: String,
    pub is_flower: bool,
}

#[derive(Clone)]
pub struct BotClaimOption {
    pub action_type: String,
    pub tile_ids: Vec<String>,
}

#[derive(Clone)]
pub struct BotDiscardEventView {
    pub seat_index: usize,
    pub tile_key: String,
}

#[derive(Clone)]
pub struct BotPlayerView {
    pub concealed_tiles: Vec<BotTileView>,
    pub concealed_tile_counts: BotTileCounts,
    pub meld_tile_key_groups: Vec<Vec<String>>,
    pub flower_count: usize,
}

#[derive(Clone)]
pub struct BotContextView {
    pub seat_index: usize,
    pub seat_count: usize,
    pub dealer_seat: usize,
    pub round_wind: Option<String>,
    pub minimum_hu_fan: i64,
    pub cumulative_scores: Vec<i64>,
    pub wall_tiles_remaining: i64,
    pub visible_tile_keys: Vec<String>,
    pub opponent_discards_by_seat: Vec<Vec<String>>,
    pub opponent_melds_by_seat: Vec<Vec<Vec<String>>>,
    pub discard_history: Vec<BotDiscardEventView>,
    pub kong_entries: Vec<ScoringKongEntry>,
    pub player: BotPlayerView,
    pub restricted_discard_tile_key: Option<String>,
    pub drawn_tile_id: Option<String>,
    pub self_kong_candidates: Vec<BotSelfKongCandidate>,
    pub claim_options: Vec<BotClaimOption>,
    pub last_discard_tile_key: Option<String>,
    pub add_kong_risk_tiles: HashSet<String>,
}

pub fn build_bot_context_view(
    cache: &RoomScoringCache,
    state: &RoomState,
    seat_index: usize,
    claim_options: Vec<BotClaimOption>,
    self_kong_candidates: Vec<BotSelfKongCandidate>,
    add_kong_risk_tiles: HashSet<String>,
) -> Option<BotContextView> {
    let player = cache.player(seat_index)?;
    Some(BotContextView {
        seat_index,
        seat_count: cache.seat_count,
        dealer_seat: cache.dealer_seat,
        round_wind: cache.round_wind.clone(),
        minimum_hu_fan: state.minimum_hu_fan,
        cumulative_scores: cache.cumulative_scores.clone(),
        wall_tiles_remaining: cache.wall_tiles_remaining,
        visible_tile_keys: cache.visible_tile_keys.clone(),
        opponent_discards_by_seat: cache.opponent_discards_by_seat.clone(),
        opponent_melds_by_seat: cache.opponent_melds_by_seat.clone(),
        discard_history: cache
            .discard_history
            .iter()
            .map(|event| BotDiscardEventView {
                seat_index: event.seat_index,
                tile_key: event.tile_key.clone(),
            })
            .collect(),
        kong_entries: cache.kong_entries.clone(),
        player: BotPlayerView {
            concealed_tiles: player
                .concealed_tiles
                .iter()
                .map(|tile| BotTileView {
                    tile_id: tile.tile_id.clone(),
                    tile_key: tile.tile_key.clone(),
                    is_flower: tile.kind == "flower",
                })
                .collect(),
            concealed_tile_counts: player.concealed_tile_counts,
            meld_tile_key_groups: player.meld_tile_key_groups.clone(),
            flower_count: player.flower_count,
        },
        restricted_discard_tile_key: cache.restricted_discard_tile_key.clone(),
        drawn_tile_id: cache.drawn_tile_id.clone(),
        self_kong_candidates,
        claim_options,
        last_discard_tile_key: cache.last_discard_tile_key.clone(),
        add_kong_risk_tiles,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashSet};

    use crate::core::state::{
        LastActionContext, MatchState, PendingTimeout, PlayerRoundState, RoomState,
        RoundScoreTrackers, RoundState, RuleRuntimeState, WallState,
    };
    use crate::core::tile::Tile;
    use crate::projection::bot_view::{
        BotClaimOption, BotSelfKongCandidate, BotSelfKongKind, build_bot_context_view,
        claim_tile_id_options,
    };
    use crate::room_scoring::RoomScoringCache;

    fn tile(tile_id: &str, tile_key: &str, kind: &str) -> Tile {
        Tile {
            tile_id: tile_id.to_string(),
            tile_key: tile_key.to_string(),
            kind: kind.to_string(),
            suit: None,
            rank: None,
            name: None,
        }
    }

    fn sample_state() -> RoomState {
        RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            minimum_hu_fan: crate::core::state::room::default_minimum_hu_fan(),
            seats: Vec::new(),
            match_state: Some(MatchState {
                prevailing_wind: "east".to_string(),
                hand_number: 1,
                dealer_seat: 0,
                cumulative_scores: BTreeMap::from([(0, 12), (1, -12)]),
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
                    head_index: 3,
                    tail_index: 9,
                },
                players: vec![PlayerRoundState {
                    seat: 0,
                    is_ready_hand: false,
                    concealed_tiles: vec![
                        tile("w2#0", "w2", "suit"),
                        tile("w3#0", "w3", "suit"),
                        tile("w4#0", "w4", "suit"),
                        tile("f1#0", "f1", "flower"),
                    ],
                    melds: vec![vec![
                        "east".to_string(),
                        "east".to_string(),
                        "east".to_string(),
                    ]],
                    display_melds: vec![],
                    flowers: vec![tile("f1#shown", "f1", "flower")],
                    discards: vec![tile("red#0", "red", "dragon")],
                }],
                discard_history: vec![crate::core::state::DiscardEventState {
                    seat_index: 0,
                    tile_key: "red".to_string(),
                }],
                last_discard: Some(tile("w3#discard", "w3", "suit")),
                pending_action: None,
                settlement: None,
                version: 1,
                score_trackers: RoundScoreTrackers::default(),
                last_action_context: LastActionContext::default(),
                rule_state: RuleRuntimeState {},
                restricted_discard_tile_key: Some("w3".to_string()),
            }),
            pending_timeout: Some(PendingTimeout {
                kind: "active_turn".to_string(),
                seat_index: 0,
                deadline_at: None,
                drawn_tile_id: Some("w4#0".to_string()),
                extended_with_extra: false,
            }),
            continue_action: None,
        }
    }

    #[test]
    fn builds_bot_context_from_scoring_cache() {
        let state = sample_state();
        let cache = RoomScoringCache::from_state(&state);
        let context = build_bot_context_view(
            &cache,
            &state,
            0,
            vec![BotClaimOption {
                action_type: "pung".to_string(),
                tile_ids: vec!["w3#0".to_string(), "w3#1".to_string()],
            }],
            vec![BotSelfKongCandidate {
                kind: BotSelfKongKind::Add,
                tile_ids: vec!["w3#0".to_string()],
                tile_key: "w3".to_string(),
                meld_index: Some(0),
            }],
            HashSet::from(["w3".to_string()]),
        )
        .expect("seat should exist");

        assert_eq!(context.cumulative_scores, vec![12]);
        assert_eq!(context.minimum_hu_fan, state.minimum_hu_fan);
        assert_eq!(context.wall_tiles_remaining, 7);
        assert_eq!(context.player.concealed_tiles.len(), 4);
        assert!(
            context
                .player
                .concealed_tiles
                .iter()
                .any(|tile| tile.is_flower)
        );
        assert_eq!(context.restricted_discard_tile_key.as_deref(), Some("w3"));
        assert_eq!(context.drawn_tile_id.as_deref(), Some("w4#0"));
        assert_eq!(context.discard_history.len(), 1);
        assert_eq!(context.discard_history[0].tile_key, "red");
        assert_eq!(
            context.add_kong_risk_tiles,
            HashSet::from(["w3".to_string()])
        );
    }

    #[test]
    fn derives_claim_tile_ids_from_cache() {
        let state = sample_state();
        let cache = RoomScoringCache::from_state(&state);
        let chow_options = claim_tile_id_options(&cache, 0, "chow");

        assert_eq!(
            chow_options,
            vec![vec!["w2#0".to_string(), "w4#0".to_string()]]
        );
    }
}
