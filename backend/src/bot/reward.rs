use super::{
    action_space::{TILE_KIND_COUNT, tile_index},
    context::BotContext,
    shanten::min_shanten_for_counts,
};
use crate::core::state::RoomState;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RewardSnapshot {
    pub(crate) shanten: i32,
}

pub(crate) fn shanten_for_tile_keys(tile_keys: &[String], open_meld_count: usize) -> Option<i32> {
    let counts = tile_counts_for_tile_keys(tile_keys)?;
    Some(min_shanten_for_counts(&counts, open_meld_count))
}

pub(crate) fn reward_snapshot_from_context(context: &BotContext) -> Option<RewardSnapshot> {
    let concealed_tile_keys = context
        .player
        .concealed_tiles
        .iter()
        .filter(|tile| !tile.is_flower)
        .map(|tile| tile.tile_key.clone())
        .collect::<Vec<_>>();
    let open_meld_count = context.player.meld_tile_key_groups.len();
    reward_snapshot_for_tile_keys(&concealed_tile_keys, open_meld_count)
}

pub(crate) fn reward_snapshot_from_room(
    room: &RoomState,
    seat_index: usize,
) -> Option<RewardSnapshot> {
    let round = room.round_state.as_ref()?;
    let player = round.players.get(seat_index)?;
    let concealed_tile_keys = player
        .concealed_tiles
        .iter()
        .filter(|tile| tile_index(&tile.tile_key).is_some())
        .map(|tile| tile.tile_key.clone())
        .collect::<Vec<_>>();
    reward_snapshot_for_tile_keys(&concealed_tile_keys, player.melds.len())
}

fn reward_snapshot_for_tile_keys(
    concealed_tile_keys: &[String],
    open_meld_count: usize,
) -> Option<RewardSnapshot> {
    Some(RewardSnapshot {
        shanten: shanten_for_tile_keys(concealed_tile_keys, open_meld_count)?,
    })
}

pub(crate) fn shaping_reward(before: RewardSnapshot, after: RewardSnapshot) -> f32 {
    let shanten_delta = before.shanten - after.shanten;
    let shanten_reward = shanten_delta.clamp(-1, 1) as f32 * 0.10;
    let tenpai_bonus = if before.shanten > 0 && after.shanten == 0 {
        0.15
    } else {
        0.0
    };
    shanten_reward + tenpai_bonus
}

fn tile_counts_for_tile_keys(tile_keys: &[String]) -> Option<[u8; TILE_KIND_COUNT]> {
    let mut counts = [0_u8; TILE_KIND_COUNT];
    for tile_key in tile_keys {
        let index = tile_index(tile_key)?;
        counts[index] = counts[index].saturating_add(1);
    }
    Some(counts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shanten_helper_identifies_known_tenpai_shape() {
        let tile_keys = string_keys(&[
            "w1", "w2", "w3", "w4", "w5", "w6", "t1", "t2", "t3", "b1", "b2", "b3", "east",
        ]);

        assert_eq!(shanten_for_tile_keys(&tile_keys, 0), Some(0));
    }

    #[test]
    fn shaping_reward_rewards_shanten_improvement() {
        let before = RewardSnapshot { shanten: 2 };
        let after = RewardSnapshot { shanten: 1 };

        assert!(shaping_reward(before, after) > 0.0);
        assert!(shaping_reward(after, before) < 0.0);
    }

    #[test]
    fn shaping_reward_is_weak_auxiliary_signal() {
        let before = RewardSnapshot { shanten: 1 };
        let after = RewardSnapshot { shanten: 0 };

        assert!(shaping_reward(before, after).abs() <= 0.35);
    }

    fn string_keys(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|key| (*key).to_string()).collect()
    }
}
