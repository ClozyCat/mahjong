use crate::core::state::RoomState;
use crate::room_scoring::RoomScoringCache;
use crate::rules::scoring::decompose_winning_hand_with_melds;

const READY_HAND_TILE_KEYS: [&str; 34] = [
    "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "t1", "t2", "t3", "t4", "t5", "t6", "t7",
    "t8", "t9", "b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9", "east", "south", "west",
    "north", "red", "green", "white",
];

pub fn has_ready_hand_discard_in_room_state(state: &RoomState, seat_index: usize) -> bool {
    let Some(round) = state.round_state.as_ref() else {
        return false;
    };
    round.players.get(seat_index).is_some_and(|player| {
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

    has_winning_draw(&concealed_tile_keys, &scoring_player.meld_tile_key_groups)
}

pub fn is_tenpai_hand_with_melds(
    concealed_tile_keys: &[String],
    meld_tile_key_groups: &[Vec<String>],
) -> bool {
    let Some(waiting_tile_count) = waiting_concealed_tile_count(meld_tile_key_groups) else {
        return false;
    };

    if concealed_tile_keys.len() == waiting_tile_count {
        return has_winning_draw(concealed_tile_keys, meld_tile_key_groups);
    }

    if concealed_tile_keys.len() != waiting_tile_count + 1 {
        return false;
    }

    if !decompose_winning_hand_with_melds(concealed_tile_keys, meld_tile_key_groups).is_empty() {
        return true;
    }

    (0..concealed_tile_keys.len()).any(|removed_index| {
        let mut after_discard = concealed_tile_keys.to_vec();
        after_discard.remove(removed_index);
        has_winning_draw(&after_discard, meld_tile_key_groups)
    })
}

fn waiting_concealed_tile_count(meld_tile_key_groups: &[Vec<String>]) -> Option<usize> {
    if meld_tile_key_groups.len() > 4 {
        return None;
    }
    Some((4 - meld_tile_key_groups.len()) * 3 + 1)
}

fn has_winning_draw(concealed_tile_keys: &[String], meld_tile_key_groups: &[Vec<String>]) -> bool {
    READY_HAND_TILE_KEYS.iter().any(|candidate| {
        let mut simulated = concealed_tile_keys.to_vec();
        simulated.push((*candidate).to_string());
        !decompose_winning_hand_with_melds(&simulated, meld_tile_key_groups).is_empty()
    })
}

#[cfg(test)]
mod tests {
    use super::is_tenpai_hand_with_melds;

    fn keys(tile_keys: &[&str]) -> Vec<String> {
        tile_keys
            .iter()
            .map(|tile_key| (*tile_key).to_string())
            .collect()
    }

    #[test]
    fn detects_thirteen_tile_tenpai_without_ready_hand_declaration() {
        let concealed = keys(&[
            "w1", "w2", "w3", "w4", "w5", "w6", "t1", "t2", "t3", "b1", "b2", "b3", "east",
        ]);

        assert!(is_tenpai_hand_with_melds(&concealed, &[]));
    }

    #[test]
    fn detects_fourteen_tile_hand_that_can_discard_to_tenpai() {
        let concealed = keys(&[
            "w1", "w2", "w3", "w4", "w5", "w6", "t1", "t2", "t3", "b1", "b2", "b3", "east", "red",
        ]);

        assert!(is_tenpai_hand_with_melds(&concealed, &[]));
    }

    #[test]
    fn rejects_non_tenpai_hand() {
        let concealed = keys(&[
            "w1", "w1", "w4", "w7", "t2", "t5", "t8", "b3", "b6", "b9", "east", "south", "red",
        ]);

        assert!(!is_tenpai_hand_with_melds(&concealed, &[]));
    }
}
