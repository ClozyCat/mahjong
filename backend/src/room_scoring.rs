use crate::core::state::RoomState;
use crate::core::tile::Tile;
use crate::rules::scoring::KongEntry as ScoringKongEntry;

#[cfg(test)]
use crate::core::error::EngineError;
#[cfg(test)]
use serde_json::Value;

const MAX_SEATS: usize = 4;
const TILE_KIND_COUNT: usize = 34;

pub type TileCounts = [u8; TILE_KIND_COUNT];

#[derive(Debug, Clone)]
pub struct RoomScoringPlayer {
    pub concealed_tiles: Vec<Tile>,
    pub concealed_tile_keys: Vec<String>,
    pub concealed_tile_counts: TileCounts,
    pub meld_tile_key_groups: Vec<Vec<String>>,
    pub flower_count: usize,
}

#[derive(Debug, Clone)]
pub struct RoomScoringCache {
    pub seat_count: usize,
    pub dealer_seat: usize,
    pub round_wind: Option<String>,
    pub visible_tile_keys: Vec<String>,
    pub kong_entries: Vec<ScoringKongEntry>,
    pub cumulative_scores: Vec<i64>,
    pub wall_tiles_remaining: i64,
    pub opponent_discards_by_seat: Vec<Vec<String>>,
    pub opponent_melds_by_seat: Vec<Vec<Vec<String>>>,
    pub discard_history: Vec<RoomDiscardEvent>,
    pub restricted_discard_tile_key: Option<String>,
    pub drawn_tile_id: Option<String>,
    pub last_discard_tile_key: Option<String>,
    players: Vec<RoomScoringPlayer>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomDiscardEvent {
    pub seat_index: usize,
    pub tile_key: String,
}

impl RoomScoringCache {
    #[cfg(test)]
    pub fn from_room(room: &Value) -> Self {
        Self::from_room_value(room).unwrap_or_else(|_| Self::from_state(&RoomState::default()))
    }

    #[cfg(test)]
    pub fn from_room_value(room: &Value) -> Result<Self, EngineError> {
        let state = RoomState::from_room_value(room)?;
        Ok(Self::from_state(&state))
    }

    pub fn from_state(state: &RoomState) -> Self {
        let seat_count = state
            .round_state
            .as_ref()
            .map(|round| round.players.len())
            .unwrap_or(MAX_SEATS);

        let cumulative_scores = (0..seat_count)
            .map(|seat| {
                state
                    .match_state
                    .as_ref()
                    .and_then(|match_state| match_state.cumulative_scores.get(&seat).copied())
                    .unwrap_or(0)
            })
            .collect::<Vec<_>>();

        let Some(round) = state.round_state.as_ref() else {
            return Self {
                seat_count,
                dealer_seat: state
                    .match_state
                    .as_ref()
                    .map(|match_state| match_state.dealer_seat)
                    .unwrap_or(0),
                round_wind: None,
                visible_tile_keys: Vec::new(),
                kong_entries: Vec::new(),
                cumulative_scores,
                wall_tiles_remaining: 0,
                opponent_discards_by_seat: Vec::new(),
                opponent_melds_by_seat: Vec::new(),
                discard_history: Vec::new(),
                restricted_discard_tile_key: None,
                drawn_tile_id: state
                    .pending_timeout
                    .as_ref()
                    .and_then(|timeout| timeout.drawn_tile_id.clone()),
                last_discard_tile_key: None,
                players: Vec::new(),
            };
        };

        let mut players = Vec::with_capacity(round.players.len());
        let mut visible_tile_keys = Vec::new();
        let mut opponent_discards_by_seat = Vec::with_capacity(round.players.len());
        let mut opponent_melds_by_seat = Vec::with_capacity(round.players.len());

        for player in &round.players {
            let discards = player
                .discards
                .iter()
                .map(|tile| tile.tile_key.clone())
                .collect::<Vec<_>>();
            visible_tile_keys.extend(discards.iter().cloned());
            opponent_discards_by_seat.push(discards);

            let meld_tile_key_groups = player.melds.clone();
            for meld in &meld_tile_key_groups {
                extend_visible_meld_tile_keys(&mut visible_tile_keys, meld);
            }
            opponent_melds_by_seat.push(meld_tile_key_groups.clone());

            let concealed_tiles = player.concealed_tiles.clone();
            let concealed_tile_keys = concealed_tiles
                .iter()
                .map(|tile| tile.tile_key.clone())
                .collect::<Vec<_>>();
            let concealed_tile_counts =
                tile_counts34(concealed_tile_keys.iter().map(String::as_str));

            players.push(RoomScoringPlayer {
                concealed_tiles,
                concealed_tile_keys,
                concealed_tile_counts,
                meld_tile_key_groups,
                flower_count: player.flowers.len(),
            });
        }

        Self {
            seat_count,
            dealer_seat: round.dealer_seat,
            round_wind: Some(round.round_wind.clone()),
            visible_tile_keys,
            kong_entries: round
                .score_trackers
                .kong_entries
                .iter()
                .map(|entry| ScoringKongEntry {
                    kong_type: entry.kong_type.clone(),
                    actor_seat: entry.actor_seat,
                    payer_seats: entry.payer_seats.clone(),
                    tile_key: entry.tile_key.clone(),
                })
                .collect(),
            cumulative_scores,
            wall_tiles_remaining: round.wall.live_tiles_remaining() as i64,
            opponent_discards_by_seat,
            opponent_melds_by_seat,
            discard_history: round
                .discard_history
                .iter()
                .map(|event| RoomDiscardEvent {
                    seat_index: event.seat_index,
                    tile_key: event.tile_key.clone(),
                })
                .collect(),
            restricted_discard_tile_key: round.restricted_discard_tile_key.clone(),
            drawn_tile_id: state
                .pending_timeout
                .as_ref()
                .and_then(|timeout| timeout.drawn_tile_id.clone()),
            last_discard_tile_key: round
                .last_discard
                .as_ref()
                .map(|tile| tile.tile_key.clone()),
            players,
        }
    }

    pub fn player(&self, seat_index: usize) -> Option<&RoomScoringPlayer> {
        self.players.get(seat_index)
    }
}

pub fn tile_counts34<'a>(tile_keys: impl Iterator<Item = &'a str>) -> TileCounts {
    let mut counts = [0_u8; TILE_KIND_COUNT];
    for tile_key in tile_keys {
        if let Some(tile_index) = tile_index(tile_key) {
            counts[tile_index] = counts[tile_index].saturating_add(1);
        }
    }
    counts
}

fn extend_visible_meld_tile_keys(target: &mut Vec<String>, meld_tile_keys: &[String]) {
    if meld_tile_keys.len() == 4
        && meld_tile_keys
            .iter()
            .all(|tile_key| tile_key == &meld_tile_keys[0])
    {
        target.extend(meld_tile_keys.iter().take(3).cloned());
    } else {
        target.extend(meld_tile_keys.iter().cloned());
    }
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::core::state::{
        LastActionContext, MatchState, PendingTimeout, PlayerRoundState, RoomState,
        RoundScoreTrackers, RoundState, RuleRuntimeState, WallState,
    };

    use super::RoomScoringCache;

    #[test]
    fn builds_cache_from_typed_room_state() {
        let state = RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            minimum_hu_fan: crate::core::state::room::default_minimum_hu_fan(),
            dealer_repeat_enabled: false,
            dealer_double_enabled: false,
            seats: Vec::new(),
            match_state: Some(MatchState {
                prevailing_wind: "east".to_string(),
                hand_number: 1,
                dealer_seat: 1,
                dealer_repeat_count: 0,
                cumulative_scores: BTreeMap::from([(0, 10), (1, 20)]),
                match_finished: false,
                last_completed_round_id: None,
                statistics: Default::default(),
                extra_time_pool: Default::default(),
            }),
            round_state: Some(RoundState {
                round_id: "round-1".to_string(),
                dealer_seat: 1,
                round_wind: "east".to_string(),
                current_actor: 0,
                phase: "playing".to_string(),
                wall: WallState {
                    tiles: Vec::new(),
                    head_index: 0,
                    tail_index: 9,
                },
                players: vec![PlayerRoundState {
                    seat: 0,
                    is_ready_hand: false,
                    concealed_tiles: vec![
                        crate::core::tile::Tile {
                            tile_id: "w1#0".to_string(),
                            tile_key: "w1".to_string(),
                            kind: "suit".to_string(),
                            suit: Some("characters".to_string()),
                            rank: Some(1),
                            name: None,
                        },
                        crate::core::tile::Tile {
                            tile_id: "w1#1".to_string(),
                            tile_key: "w1".to_string(),
                            kind: "suit".to_string(),
                            suit: Some("characters".to_string()),
                            rank: Some(1),
                            name: None,
                        },
                    ],
                    melds: vec![vec![
                        "east".to_string(),
                        "east".to_string(),
                        "east".to_string(),
                    ]],
                    display_melds: vec![],
                    flowers: vec![crate::core::tile::Tile::tile_key_only("f1")],
                    discards: vec![crate::core::tile::Tile::tile_key_only("red")],
                }],
                discard_history: vec![crate::core::state::DiscardEventState {
                    seat_index: 0,
                    tile_key: "red".to_string(),
                }],
                last_discard: Some(crate::core::tile::Tile::tile_key_only("red")),
                pending_action: None,
                settlement: None,
                version: 1,
                score_trackers: RoundScoreTrackers {
                    kong_entries: vec![crate::core::state::KongTrackerEntry {
                        kong_type: "concealed_kong".to_string(),
                        actor_seat: 0,
                        payer_seats: vec![1, 2, 3],
                        tile_key: Some("w1".to_string()),
                    }],
                },
                last_action_context: LastActionContext::default(),
                rule_state: RuleRuntimeState {},
                restricted_discard_tile_key: Some("w1".to_string()),
            }),
            pending_timeout: Some(PendingTimeout {
                kind: "active_turn".to_string(),
                seat_index: 0,
                deadline_at: None,
                drawn_tile_id: Some("w1#1".to_string()),
                extended_with_extra: false,
            }),
            continue_action: None,
        };

        let cache = RoomScoringCache::from_state(&state);

        assert_eq!(cache.seat_count, 1);
        assert_eq!(cache.dealer_seat, 1);
        assert_eq!(cache.round_wind.as_deref(), Some("east"));
        assert_eq!(cache.cumulative_scores, vec![10]);
        assert_eq!(cache.wall_tiles_remaining, 10);
        assert_eq!(
            cache.visible_tile_keys,
            vec![
                "red".to_string(),
                "east".to_string(),
                "east".to_string(),
                "east".to_string()
            ]
        );
        assert_eq!(cache.restricted_discard_tile_key.as_deref(), Some("w1"));
        assert_eq!(cache.drawn_tile_id.as_deref(), Some("w1#1"));
        assert_eq!(cache.last_discard_tile_key.as_deref(), Some("red"));
        assert_eq!(cache.discard_history.len(), 1);
        assert_eq!(cache.discard_history[0].tile_key, "red");
        assert_eq!(
            cache
                .player(0)
                .map(|player| player.concealed_tile_counts[0]),
            Some(2)
        );
        assert_eq!(cache.player(0).map(|player| player.flower_count), Some(1));
    }
}
