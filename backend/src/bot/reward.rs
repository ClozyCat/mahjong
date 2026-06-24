use super::{
    action_space::{TILE_KEYS, TILE_KIND_COUNT, tile_index},
    context::BotContext,
    shanten::min_shanten_for_counts,
};
use crate::{
    core::state::RoomState,
    rules::scoring::{
        EvaluationInput, TimingFeatures, decompose_winning_hand_with_melds, evaluate_fans,
        extract_hand_features,
    },
};

const QUALIFYING_FAN_TARGET: i64 = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RewardSnapshot {
    pub(crate) shanten: i32,
    pub(crate) qualifying_fan_potential: i64,
    pub(crate) raw_fan_potential: i64,
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
    reward_snapshot_for_tile_keys(
        &concealed_tile_keys,
        &context.player.meld_tile_key_groups,
        open_meld_count,
        context.seat_index,
        context.dealer_seat,
        context.round_wind.as_deref().unwrap_or("east"),
        context.minimum_hu_fan,
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
    reward_snapshot_for_tile_keys(
        &concealed_tile_keys,
        &player.melds,
        player.melds.len(),
        seat_index,
        round.dealer_seat,
        &round.round_wind,
        room.minimum_hu_fan,
    )
}

fn reward_snapshot_for_tile_keys(
    concealed_tile_keys: &[String],
    meld_tile_key_groups: &[Vec<String>],
    open_meld_count: usize,
    seat_index: usize,
    dealer_seat: usize,
    round_wind: &str,
    minimum_hu_fan: i64,
) -> Option<RewardSnapshot> {
    let shanten = shanten_for_tile_keys(concealed_tile_keys, open_meld_count)?;
    let (capped, raw) = qualifying_fan_potential(
        concealed_tile_keys,
        meld_tile_key_groups,
        shanten,
        seat_index,
        dealer_seat,
        round_wind,
        minimum_hu_fan,
    );
    Some(RewardSnapshot {
        shanten,
        qualifying_fan_potential: capped,
        raw_fan_potential: raw,
    })
}

pub(crate) fn shaping_reward(before: RewardSnapshot, after: RewardSnapshot) -> f32 {
    let shanten_delta = before.shanten - after.shanten;
    let shanten_reward = shanten_delta.clamp(-1, 1) as f32 * 0.10;
    let tenpai_bonus = if before.shanten > 0 && after.shanten == 0 {
        if after.qualifying_fan_potential >= QUALIFYING_FAN_TARGET {
            0.50
        } else {
            -0.30
        }
    } else {
        0.0
    };
    let fan_progress_reward =
        ((after.qualifying_fan_potential - before.qualifying_fan_potential) as f32 / 8.0)
            .clamp(-1.0, 1.0)
            * 0.15;
    shanten_reward + tenpai_bonus + fan_progress_reward
}

fn tile_counts_for_tile_keys(tile_keys: &[String]) -> Option<[u8; TILE_KIND_COUNT]> {
    let mut counts = [0_u8; TILE_KIND_COUNT];
    for tile_key in tile_keys {
        let index = tile_index(tile_key)?;
        counts[index] = counts[index].saturating_add(1);
    }
    Some(counts)
}

pub(crate) fn qualifying_fan_potential(
    concealed_tile_keys: &[String],
    meld_tile_key_groups: &[Vec<String>],
    shanten: i32,
    seat_index: usize,
    dealer_seat: usize,
    round_wind: &str,
    minimum_hu_fan: i64,
) -> (i64, i64) {
    if shanten != 0 {
        return (0, 0);
    }
    let max_fan = candidate_winning_tile_keys(concealed_tile_keys, meld_tile_key_groups)
        .into_iter()
        .filter_map(|winning_tile| {
            evaluate_candidate_fan(
                concealed_tile_keys,
                meld_tile_key_groups,
                &winning_tile,
                seat_index,
                dealer_seat,
                round_wind,
            )
        })
        .max()
        .unwrap_or(0);
    let cap = minimum_hu_fan.max(0);
    (max_fan.min(cap), max_fan)
}

fn candidate_winning_tile_keys(
    concealed_tile_keys: &[String],
    meld_tile_key_groups: &[Vec<String>],
) -> Vec<String> {
    let Some(counts) = tile_counts_for_tile_keys(concealed_tile_keys) else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    for index in 0..TILE_KIND_COUNT {
        if counts[index] >= 4 {
            continue;
        }
        let tile_key = TILE_KEYS[index].to_string();
        let mut with_incoming = concealed_tile_keys.to_vec();
        with_incoming.push(tile_key.clone());
        if !decompose_winning_hand_with_melds(&with_incoming, meld_tile_key_groups).is_empty() {
            candidates.push(tile_key);
        }
    }
    candidates
}

fn evaluate_candidate_fan(
    concealed_tile_keys: &[String],
    meld_tile_key_groups: &[Vec<String>],
    winning_tile: &str,
    seat_index: usize,
    dealer_seat: usize,
    round_wind: &str,
) -> Option<i64> {
    let mut winning_concealed = concealed_tile_keys.to_vec();
    winning_concealed.push(winning_tile.to_string());
    let decompositions =
        decompose_winning_hand_with_melds(&winning_concealed, meld_tile_key_groups);
    if decompositions.is_empty() {
        return None;
    }
    let mut tile_keys = winning_concealed.clone();
    tile_keys.extend(meld_tile_key_groups.iter().flatten().cloned());
    let seat_wind = crate::bot::context::seat_wind_key(seat_index, dealer_seat);
    let features = extract_hand_features(
        concealed_tile_keys,
        meld_tile_key_groups,
        None,
        Some(winning_tile),
        Some(&seat_wind),
        Some(round_wind),
        Some(&decompositions),
    );
    let input = EvaluationInput {
        win_type: "self_draw".to_string(),
        winner_seat: Some(seat_index),
        discarder_seat: None,
        ready_hand_declared: false,
        flower_count: 0,
        seat_count: 4,
        features,
        timing: TimingFeatures::default(),
        kong_entries: Vec::new(),
        tile_keys,
        visible_tile_keys: Vec::new(),
        concealed_tile_keys: concealed_tile_keys.to_vec(),
        meld_tile_key_groups: meld_tile_key_groups.to_vec(),
        open_meld_tile_key_groups: meld_tile_key_groups.to_vec(),
        incoming_tile: Some(winning_tile.to_string()),
        winning_tile: Some(winning_tile.to_string()),
        decompositions,
    };
    Some(evaluate_fans(input).minimum_qualifying_fan_total)
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
    fn reward_snapshot_detects_qualifying_fan_potential_for_tenpai() {
        let tile_keys = string_keys(&[
            "w1", "w9", "t1", "t9", "b1", "b9", "east", "south", "west", "north", "red", "green",
            "white",
        ]);

        let snapshot =
            reward_snapshot_for_tile_keys(&tile_keys, &[], 0, 0, 0, "east", 8).expect("snapshot");

        assert_eq!(snapshot.shanten, 0);
        assert_eq!(snapshot.qualifying_fan_potential, 8);
        assert!(snapshot.raw_fan_potential >= 8);
    }

    #[test]
    fn shaping_reward_rewards_shanten_improvement() {
        let before = RewardSnapshot {
            shanten: 2,
            qualifying_fan_potential: 0,
            raw_fan_potential: 0,
        };
        let after = RewardSnapshot {
            shanten: 1,
            qualifying_fan_potential: 0,
            raw_fan_potential: 0,
        };

        assert!(shaping_reward(before, after) > 0.0);
        assert!(shaping_reward(after, before) < 0.0);
    }

    #[test]
    fn shaping_reward_is_weak_auxiliary_signal() {
        let before = RewardSnapshot {
            shanten: 1,
            qualifying_fan_potential: 0,
            raw_fan_potential: 0,
        };
        let after = RewardSnapshot {
            shanten: 0,
            qualifying_fan_potential: 8,
            raw_fan_potential: 8,
        };

        assert!(shaping_reward(before, after).abs() <= 0.85);
    }

    #[test]
    fn shaping_reward_penalizes_low_fan_tenpai() {
        let before = RewardSnapshot {
            shanten: 1,
            qualifying_fan_potential: 0,
            raw_fan_potential: 0,
        };
        let after = RewardSnapshot {
            shanten: 0,
            qualifying_fan_potential: 2,
            raw_fan_potential: 2,
        };

        assert!(shaping_reward(before, after) < 0.0);
    }

    #[test]
    fn shaping_reward_rewards_qualifying_fan_tenpai_more_than_low_fan_tenpai() {
        let before = RewardSnapshot {
            shanten: 1,
            qualifying_fan_potential: 0,
            raw_fan_potential: 0,
        };
        let low_fan = RewardSnapshot {
            shanten: 0,
            qualifying_fan_potential: 2,
            raw_fan_potential: 2,
        };
        let qualifying = RewardSnapshot {
            shanten: 0,
            qualifying_fan_potential: 8,
            raw_fan_potential: 8,
        };

        assert!(shaping_reward(before, qualifying) > shaping_reward(before, low_fan));
    }

    fn string_keys(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|key| (*key).to_string()).collect()
    }
}
