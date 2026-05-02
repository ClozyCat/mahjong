use super::{
    action_space::{TILE_KIND_COUNT, tile_index},
    context::{BotContext, seat_wind_key},
    search::min_shanten_for_counts,
};
use crate::core::state::RoomState;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FanPotential {
    pub(crate) value: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RewardSnapshot {
    pub(crate) shanten: i32,
    pub(crate) fan_potential: i32,
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
    let all_tile_keys = concealed_tile_keys
        .iter()
        .cloned()
        .chain(
            context
                .player
                .meld_tile_key_groups
                .iter()
                .flatten()
                .cloned(),
        )
        .collect::<Vec<_>>();
    let seat_wind = seat_wind_key(context.seat_index, context.dealer_seat);
    reward_snapshot_for_tile_keys(
        &concealed_tile_keys,
        &all_tile_keys,
        open_meld_count,
        Some(seat_wind.as_str()),
        context.round_wind.as_deref(),
    )
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
    let all_tile_keys = concealed_tile_keys
        .iter()
        .cloned()
        .chain(player.melds.iter().flatten().cloned())
        .collect::<Vec<_>>();
    let seat_wind = seat_wind_key(seat_index, round.dealer_seat);
    reward_snapshot_for_tile_keys(
        &concealed_tile_keys,
        &all_tile_keys,
        player.melds.len(),
        Some(seat_wind.as_str()),
        Some(round.round_wind.as_str()),
    )
}

pub(crate) fn reward_snapshot_for_tile_keys(
    concealed_tile_keys: &[String],
    all_tile_keys: &[String],
    open_meld_count: usize,
    seat_wind: Option<&str>,
    round_wind: Option<&str>,
) -> Option<RewardSnapshot> {
    Some(RewardSnapshot {
        shanten: shanten_for_tile_keys(concealed_tile_keys, open_meld_count)?,
        fan_potential: fan_potential_for_tile_keys(all_tile_keys, seat_wind, round_wind).value,
    })
}

pub(crate) fn fan_potential_for_tile_keys(
    tile_keys: &[String],
    seat_wind: Option<&str>,
    round_wind: Option<&str>,
) -> FanPotential {
    let Some(counts) = tile_counts_for_tile_keys(tile_keys) else {
        return FanPotential::default();
    };
    let mut suit_mask = 0_u8;
    let mut honor_count = 0_u8;
    for (index, count) in counts.iter().enumerate() {
        if *count == 0 {
            continue;
        }
        if index < 27 {
            suit_mask |= 1 << (index / 9);
        } else {
            honor_count = honor_count.saturating_add(*count);
        }
    }

    let mut value = 0;
    if suit_mask.count_ones() == 1 {
        value += if honor_count > 0 { 2 } else { 3 };
    }
    for index in 31..=33 {
        if counts[index] >= 2 {
            value += 1;
        }
    }
    for wind in [seat_wind, round_wind].into_iter().flatten() {
        if let Some(index) = tile_index(wind) {
            if counts[index] >= 2 {
                value += 1;
            }
        }
    }
    FanPotential {
        value: value.clamp(0, 6),
    }
}

pub(crate) fn shaping_reward(before: RewardSnapshot, after: RewardSnapshot) -> f32 {
    let shanten_delta = before.shanten - after.shanten;
    let fan_delta = after.fan_potential - before.fan_potential;
    let shanten_reward = shanten_delta.clamp(-1, 1) as f32 * 0.02;
    let fan_reward = fan_delta.clamp(-1, 1) as f32 * 0.02;
    let tenpai_bonus = if before.shanten > 0 && after.shanten == 0 {
        0.03
    } else {
        0.0
    };
    shanten_reward + fan_reward + tenpai_bonus
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
    fn fan_potential_prefers_one_suit_honor_heavy_shape() {
        let mixed = string_keys(&[
            "w1", "w2", "w3", "t1", "t2", "t3", "b1", "b2", "b3", "east", "south",
        ]);
        let one_suit_honors = string_keys(&[
            "w1", "w2", "w3", "w4", "w5", "w6", "east", "east", "red", "red", "white",
        ]);

        assert!(
            fan_potential_for_tile_keys(&one_suit_honors, Some("east"), Some("east")).value
                > fan_potential_for_tile_keys(&mixed, Some("east"), Some("east")).value
        );
    }

    #[test]
    fn shaping_reward_rewards_shanten_improvement() {
        let before = RewardSnapshot {
            shanten: 2,
            fan_potential: 1,
        };
        let after = RewardSnapshot {
            shanten: 1,
            fan_potential: 1,
        };

        assert!(shaping_reward(before, after) > 0.0);
        assert!(shaping_reward(after, before) < 0.0);
    }

    #[test]
    fn shaping_reward_is_weak_auxiliary_signal() {
        let before = RewardSnapshot {
            shanten: 1,
            fan_potential: 1,
        };
        let after = RewardSnapshot {
            shanten: 0,
            fan_potential: 2,
        };

        assert!(shaping_reward(before, after).abs() <= 0.07);
    }

    fn string_keys(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|key| (*key).to_string()).collect()
    }
}
