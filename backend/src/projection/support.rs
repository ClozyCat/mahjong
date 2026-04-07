use serde_json::Value;

use crate::room_scoring::RoomScoringCache;
use crate::rules::standard::meld::available_self_kongs_from_cache;
use crate::rules::standard::win::can_declare_hu_with_cache;

use super::SeatProjectionSupport;
use crate::core::state::RoomState;

pub fn build_seat_projection_support(
    room: &Value,
    state: &RoomState,
    local_seat: usize,
) -> SeatProjectionSupport {
    let cache = RoomScoringCache::from_state(state);
    let player = cache.player(local_seat);
    let restricted_tile_key = cache.restricted_discard_tile_key.as_deref();

    SeatProjectionSupport {
        has_concealed_flower: player.is_some_and(|player| {
            player
                .concealed_tiles
                .iter()
                .any(|tile| tile.kind == "flower")
        }),
        has_self_kong: !available_self_kongs_from_cache(&cache, local_seat).is_empty(),
        can_hu: can_declare_hu_with_cache(room, &cache, local_seat, None, None),
        restricted_discard_tile_ids: player
            .map(|player| {
                player
                    .concealed_tiles
                    .iter()
                    .filter(|tile| Some(tile.tile_key.as_str()) == restricted_tile_key)
                    .map(|tile| tile.tile_id.clone())
                    .collect()
            })
            .unwrap_or_default(),
    }
}
