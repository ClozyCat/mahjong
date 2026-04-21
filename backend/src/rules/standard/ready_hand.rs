use crate::core::state::RoomState;
use crate::room_scoring::RoomScoringCache;
use crate::rules::scoring::decompose_winning_hand_with_melds;

const READY_HAND_TILE_KEYS: [&str; 34] = [
    "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "t1", "t2", "t3", "t4", "t5", "t6",
    "t7", "t8", "t9", "b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9", "east", "south",
    "west", "north", "red", "green", "white",
];

pub fn has_ready_hand_discard_in_room_state(state: &RoomState, seat_index: usize) -> bool {
    let Some(round) = state.round_state.as_ref() else {
        return false;
    };
    round
        .players
        .get(seat_index)
        .is_some_and(|player| {
            player
                .concealed_tiles
                .iter()
                .any(|tile| can_declare_ready_hand_with_tile_id(state, seat_index, &tile.tile_id))
        })
}

pub fn can_declare_ready_hand_with_tile_id(
    state: &RoomState,
    seat_index: usize,
    tile_id: &str,
) -> bool {
    if state.phase != "playing" {
        return false;
    }
    let Some(round) = state.round_state.as_ref() else {
        return false;
    };
    if round.current_actor != seat_index || round.pending_action.is_some() {
        return false;
    }
    let Some(player) = round.players.get(seat_index) else {
        return false;
    };
    if player.is_ready_hand {
        return false;
    }
    let Some(tile) = player
        .concealed_tiles
        .iter()
        .find(|tile| tile.tile_id == tile_id)
    else {
        return false;
    };
    if round.restricted_discard_tile_key.as_deref() == Some(tile.tile_key.as_str()) {
        return false;
    }

    let cache = RoomScoringCache::from_state(state);
    let Some(scoring_player) = cache.player(seat_index) else {
        return false;
    };
    let mut concealed_tile_keys = scoring_player.concealed_tile_keys.clone();
    let Some(removed_index) = concealed_tile_keys
        .iter()
        .position(|tile_key| tile_key == &tile.tile_key)
    else {
        return false;
    };
    concealed_tile_keys.remove(removed_index);

    READY_HAND_TILE_KEYS.iter().any(|candidate| {
        let mut simulated = concealed_tile_keys.clone();
        simulated.push((*candidate).to_string());
        !decompose_winning_hand_with_melds(&simulated, &scoring_player.meld_tile_key_groups)
            .is_empty()
    })
}
