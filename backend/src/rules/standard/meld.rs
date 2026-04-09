use std::collections::{HashMap, HashSet};

use crate::core::state::RoomState;
use crate::room_scoring::{RoomScoringCache, TileCounts};

use super::win::can_declare_hu_with_cache_for_state;

#[cfg(test)]
use super::win::can_declare_hu_with_cache;
#[cfg(test)]
use serde_json::Value;

const MAX_SEATS: usize = 4;
const HONOR_TILE_START: usize = 27;
pub const TILE_KIND_COUNT: usize = 34;
const STANDARD_TILE_KEYS: [&str; TILE_KIND_COUNT] = [
    "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "t1", "t2", "t3", "t4", "t5", "t6", "t7",
    "t8", "t9", "b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9", "east", "south", "west",
    "north", "red", "green", "white",
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SelfKongKind {
    Concealed,
    Add,
}

#[derive(Clone)]
pub struct SelfKongCandidate {
    pub kind: SelfKongKind,
    pub tile_ids: Vec<String>,
    pub tile_key: String,
    pub meld_index: Option<usize>,
}

#[cfg(test)]
#[allow(dead_code)]
pub fn available_self_kongs(room: &Value, seat_index: usize) -> Vec<SelfKongCandidate> {
    let cache = RoomScoringCache::from_room(room);
    available_self_kongs_from_cache(&cache, seat_index)
}

pub fn available_self_kongs_from_cache(
    cache: &RoomScoringCache,
    seat_index: usize,
) -> Vec<SelfKongCandidate> {
    let Some(player) = cache.player(seat_index) else {
        return Vec::new();
    };
    let mut by_key: HashMap<String, Vec<String>> = HashMap::new();
    for tile in &player.concealed_tiles {
        by_key
            .entry(tile.tile_key.clone())
            .or_default()
            .push(tile.tile_id.clone());
    }

    let mut candidates = Vec::new();
    for (tile_key, tile_ids) in &by_key {
        if tile_ids.len() >= 4 {
            candidates.push(SelfKongCandidate {
                kind: SelfKongKind::Concealed,
                tile_ids: tile_ids[0..4].to_vec(),
                tile_key: tile_key.clone(),
                meld_index: None,
            });
        }
    }

    for (meld_index, meld) in player.meld_tile_key_groups.iter().enumerate() {
        if meld.len() == 3 && meld.iter().all(|tile_key| tile_key == &meld[0]) {
            if let Some(tile_ids) = by_key.get(&meld[0]) {
                if let Some(tile_id) = tile_ids.first() {
                    candidates.push(SelfKongCandidate {
                        kind: SelfKongKind::Add,
                        tile_ids: vec![tile_id.clone()],
                        tile_key: meld[0].clone(),
                        meld_index: Some(meld_index),
                    });
                }
            }
        }
    }

    candidates
}

pub fn resolve_self_kong_selection(
    candidates: &[SelfKongCandidate],
    tile_ids: &[String],
) -> Option<SelfKongCandidate> {
    if tile_ids.is_empty() {
        return None;
    }
    if tile_ids.iter().collect::<HashSet<_>>().len() != tile_ids.len() {
        return None;
    }

    let mut normalized = tile_ids.to_vec();
    normalized.sort();
    candidates.iter().find_map(|candidate| {
        let mut candidate_ids = candidate.tile_ids.clone();
        candidate_ids.sort();
        if candidate_ids == normalized {
            Some(candidate.clone())
        } else {
            None
        }
    })
}

#[cfg(test)]
pub fn seats_with_hu_candidate_for_tile(
    room: &Value,
    actor_seat: usize,
    tile_key: &str,
) -> Vec<usize> {
    let cache = RoomScoringCache::from_room(room);
    (0..MAX_SEATS)
        .filter(|seat_index| *seat_index != actor_seat)
        .filter(|seat_index| {
            can_declare_hu_with_cache(room, &cache, *seat_index, Some(tile_key), None)
        })
        .collect()
}

pub fn seats_with_hu_candidate_for_tile_in_room_state(
    room: &RoomState,
    actor_seat: usize,
    tile_key: &str,
) -> Vec<usize> {
    let cache = RoomScoringCache::from_state(room);
    (0..MAX_SEATS)
        .filter(|seat_index| *seat_index != actor_seat)
        .filter(|seat_index| {
            can_declare_hu_with_cache_for_state(room, &cache, *seat_index, Some(tile_key), None)
        })
        .collect()
}

#[cfg(test)]
pub fn claim_window_options_after_discard(
    room: &Value,
    discarder_seat: usize,
    discarded_tile_key: &str,
) -> Vec<Vec<String>> {
    let ltw_after_discard = is_last_tile_wall_point_after_discard(room);
    let next_player = (discarder_seat + 1) % MAX_SEATS;
    let scoring_cache = RoomScoringCache::from_room(room);

    (0..MAX_SEATS)
        .map(|seat_index| {
            if seat_index == discarder_seat {
                return Vec::new();
            }

            let counts = scoring_cache
                .player(seat_index)
                .map(|player| player.concealed_tile_counts)
                .unwrap_or([0; TILE_KIND_COUNT]);
            let mut claims = Vec::new();
            if !ltw_after_discard {
                let same_tile_count = tile_index(discarded_tile_key)
                    .map(|tile_index| counts[tile_index])
                    .unwrap_or(0);
                if same_tile_count >= 2 {
                    claims.push("pung".to_string());
                }
                if same_tile_count >= 3 {
                    claims.push("kong".to_string());
                }
                if seat_index == next_player && can_chow(discarded_tile_key, &counts) {
                    claims.push("chow".to_string());
                }
            }
            if can_declare_hu_with_cache(
                room,
                &scoring_cache,
                seat_index,
                Some(discarded_tile_key),
                None,
            ) {
                claims.push("hu".to_string());
            }
            claims
        })
        .collect()
}

pub fn claim_window_options_after_discard_in_room_state(
    room: &RoomState,
    discarder_seat: usize,
    discarded_tile_key: &str,
) -> Vec<Vec<String>> {
    let ltw_after_discard = is_last_tile_wall_point_after_discard_in_room_state(room);
    let next_player = (discarder_seat + 1) % MAX_SEATS;
    let scoring_cache = RoomScoringCache::from_state(room);

    (0..MAX_SEATS)
        .map(|seat_index| {
            if seat_index == discarder_seat {
                return Vec::new();
            }

            let counts = scoring_cache
                .player(seat_index)
                .map(|player| player.concealed_tile_counts)
                .unwrap_or([0; TILE_KIND_COUNT]);
            let mut claims = Vec::new();
            if !ltw_after_discard {
                let same_tile_count = tile_index(discarded_tile_key)
                    .map(|tile_index| counts[tile_index])
                    .unwrap_or(0);
                if same_tile_count >= 2 {
                    claims.push("pung".to_string());
                }
                if same_tile_count >= 3 {
                    claims.push("kong".to_string());
                }
                if seat_index == next_player && can_chow(discarded_tile_key, &counts) {
                    claims.push("chow".to_string());
                }
            }
            if can_declare_hu_with_cache_for_state(
                room,
                &scoring_cache,
                seat_index,
                Some(discarded_tile_key),
                None,
            ) {
                claims.push("hu".to_string());
            }
            claims
        })
        .collect()
}

pub fn claim_tile_id_options(
    cache: &RoomScoringCache,
    seat_index: usize,
    action_type: &str,
) -> Vec<Vec<String>> {
    let Some(discard_tile_key) = cache.last_discard_tile_key.as_deref() else {
        return Vec::new();
    };
    let Some(player) = cache.player(seat_index) else {
        return Vec::new();
    };
    let concealed_tiles = &player.concealed_tiles;

    if action_type == "pung" || action_type == "kong" {
        let needed = if action_type == "pung" { 2 } else { 3 };
        let tile_ids = concealed_tiles
            .iter()
            .filter(|tile| tile.tile_key == discard_tile_key)
            .map(|tile| tile.tile_id.clone())
            .take(needed)
            .collect::<Vec<_>>();
        return (tile_ids.len() == needed)
            .then_some(tile_ids)
            .into_iter()
            .collect();
    }

    if action_type == "chow" {
        let Some(discard_index) = tile_index(discard_tile_key) else {
            return Vec::new();
        };
        let mut options = Vec::new();
        for (first_index, second_index) in chow_required_tile_pairs(discard_index) {
            let first_key = tile_key_for_index(first_index);
            let second_key = tile_key_for_index(second_index);
            let mut first_tile_id = None;
            let mut second_tile_id = None;
            for tile in concealed_tiles {
                if first_tile_id.is_none() && tile.tile_key == first_key {
                    first_tile_id = Some(tile.tile_id.clone());
                    continue;
                }
                if second_tile_id.is_none() && tile.tile_key == second_key {
                    second_tile_id = Some(tile.tile_id.clone());
                }
            }
            if let (Some(first), Some(second)) = (first_tile_id, second_tile_id) {
                options.push(vec![first, second]);
            }
        }
        return options;
    }

    Vec::new()
}

pub fn is_valid_chow_sequence_by_keys(discard_tile_key: &str, tiles: &[&str]) -> bool {
    if tiles.len() != 2 {
        return false;
    }

    let Some(discard_index) = tile_index(discard_tile_key) else {
        return false;
    };
    let Some((discard_suit, discard_rank)) = suited_tile_components(discard_index) else {
        return false;
    };

    let mut ranks = vec![discard_rank as i32];
    for tile_key in tiles {
        let Some(tile_index) = tile_index(tile_key) else {
            return false;
        };
        let Some((tile_suit, rank)) = suited_tile_components(tile_index) else {
            return false;
        };
        if tile_suit != discard_suit {
            return false;
        }
        ranks.push(rank as i32);
    }

    ranks.sort_unstable();
    ranks[0] + 1 == ranks[1] && ranks[1] + 1 == ranks[2]
}

pub fn can_chow(discarded_tile_key: &str, counts: &TileCounts) -> bool {
    let Some(discard_index) = tile_index(discarded_tile_key) else {
        return false;
    };
    for (left_index, right_index) in chow_required_tile_pairs(discard_index) {
        if counts[left_index] > 0 && counts[right_index] > 0 {
            return true;
        }
    }
    false
}

pub fn tile_index(tile_key: &str) -> Option<usize> {
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

pub fn suited_tile_components(tile_index: usize) -> Option<(usize, usize)> {
    if tile_index >= HONOR_TILE_START {
        return None;
    }
    Some((tile_index / 9, (tile_index % 9) + 1))
}

fn tile_key_for_index(tile_index: usize) -> &'static str {
    STANDARD_TILE_KEYS
        .get(tile_index)
        .copied()
        .unwrap_or_default()
}

fn chow_required_tile_pairs(tile_index: usize) -> Vec<(usize, usize)> {
    let Some((_, rank)) = suited_tile_components(tile_index) else {
        return Vec::new();
    };
    let mut pairs = Vec::with_capacity(3);
    if rank >= 3 {
        pairs.push((tile_index - 2, tile_index - 1));
    }
    if (2..=8).contains(&rank) {
        pairs.push((tile_index - 1, tile_index + 1));
    }
    if rank <= 7 {
        pairs.push((tile_index + 1, tile_index + 2));
    }
    pairs
}

#[cfg(test)]
fn is_last_tile_wall_point_after_discard(room: &Value) -> bool {
    room.get("round_state")
        .and_then(|round| round.get("last_action_context"))
        .map(|context| {
            context.get("kind").and_then(Value::as_str) == Some("discard")
                && context
                    .get("was_last_discard")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
        })
        .unwrap_or(false)
}

fn is_last_tile_wall_point_after_discard_in_room_state(room: &RoomState) -> bool {
    room.round_state
        .as_ref()
        .map(|round| {
            let context = &round.last_action_context;
            context.kind == "discard" && context.was_last_discard
        })
        .unwrap_or(false)
}
