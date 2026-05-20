#[allow(unused_imports)]
pub(crate) use crate::projection::bot_view::{
    BotClaimOption, BotContextView as BotContext, BotPlayerView as BotPlayerContext,
    BotSelfKongKind, BotTileCounts as ProjectionTileCounts, BotTileView,
};

pub(crate) const TILE_KIND_COUNT: usize = 34;
pub(crate) const HONOR_TILE_START: usize = 27;
const STANDARD_TILE_KEYS: [&str; 34] = [
    "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "t1", "t2", "t3", "t4", "t5", "t6", "t7",
    "t8", "t9", "b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9", "east", "south", "west",
    "north", "red", "green", "white",
];
const WIND_ORDER: [&str; 4] = ["east", "south", "west", "north"];

pub(crate) type TileCounts = ProjectionTileCounts;

#[derive(Clone)]
pub struct BotAction {
    pub seat_index: usize,
    pub action_type: String,
    pub tile_ids: Vec<String>,
}

pub(crate) fn seat_wind_key(seat_index: usize, dealer_seat: usize) -> String {
    WIND_ORDER[(seat_index + 4 - dealer_seat) % 4].to_string()
}

#[cfg(test)]
pub(crate) fn tile_counts34<'a>(tile_keys: impl Iterator<Item = &'a str>) -> TileCounts {
    let mut counts = [0_u8; TILE_KIND_COUNT];
    for tile_key in tile_keys {
        if let Some(tile_index) = tile_index(tile_key) {
            counts[tile_index] = counts[tile_index].saturating_add(1);
        }
    }
    counts
}

pub(crate) fn tile_index(tile_key: &str) -> Option<usize> {
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

pub(crate) fn tile_key_for_index(tile_index: usize) -> &'static str {
    STANDARD_TILE_KEYS
        .get(tile_index)
        .copied()
        .unwrap_or_default()
}
