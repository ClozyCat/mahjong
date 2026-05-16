use super::{
    action_space::{tile_index, TILE_KIND_COUNT},
    context::{seat_wind_key, BotContext},
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
    let concealed_counts = tile_counts_for_tile_keys(concealed_tile_keys)?;
    let all_counts = tile_counts_for_tile_keys(all_tile_keys)?;
    Some(RewardSnapshot {
        shanten: shanten_for_tile_keys(concealed_tile_keys, open_meld_count)?,
        fan_potential: fan_potential_for_counts(
            &all_counts,
            &concealed_counts,
            seat_wind,
            round_wind,
        )
        .value,
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
    fan_potential_for_counts(&counts, &counts, seat_wind, round_wind)
}

fn fan_potential_for_counts(
    all_counts: &[u8; TILE_KIND_COUNT],
    concealed_counts: &[u8; TILE_KIND_COUNT],
    seat_wind: Option<&str>,
    round_wind: Option<&str>,
) -> FanPotential {
    let mut suit_mask = 0_u8;
    let mut honor_count = 0_u8;
    let mut terminal_count = 0_u8;
    for (index, count) in all_counts.iter().enumerate() {
        if *count == 0 {
            continue;
        }
        if index < 27 {
            suit_mask |= 1 << (index / 9);
            if index % 9 == 0 || index % 9 == 8 {
                terminal_count = terminal_count.saturating_add(*count);
            }
        } else {
            honor_count = honor_count.saturating_add(*count);
        }
    }

    let mut value = 0i32;

    // 1. Suit concentration (清一色/混一色 potential)
    match suit_mask.count_ones() {
        1 if honor_count == 0 => value += 3, // 清一色
        1 => value += 2,                     // 混一色
        2 => value += 1,                     // partial
        _ => {}
    }

    // 2. Dragon tile pairs (三元牌)
    for index in 31..=33 {
        if all_counts[index] >= 2 {
            value += 1;
        }
    }

    // 3. Seat/round wind pairs (自风/场风)
    for wind in [seat_wind, round_wind].into_iter().flatten() {
        if let Some(index) = tile_index(wind) {
            if all_counts[index] >= 2 {
                value += 1;
            }
        }
    }

    // 4. Tanyao (断幺) potential: no terminal (1/9) or honor tiles
    if terminal_count == 0 && honor_count == 0 {
        value += 1;
    }

    // 5. 七对子 potential: many pairs in concealed tiles
    let concealed_pair_count = concealed_counts.iter().filter(|&&c| c >= 2).count();
    if concealed_pair_count >= 4 {
        value += 1 + (concealed_pair_count.saturating_sub(4) as i32).min(2);
    }

    // 6. 三色同顺 potential: same number present in all three suits
    for num in 0..9 {
        let suits_present = (all_counts[num] > 0) as i32
            + (all_counts[9 + num] > 0) as i32
            + (all_counts[18 + num] > 0) as i32;
        if suits_present >= 3 {
            value += 2;
        } else if suits_present == 2 {
            value += 1;
        }
    }

    // 7. 对对和 potential: many triplets (pungs/kongs)
    let triplet_count = all_counts.iter().filter(|&&c| c >= 3).count();
    if triplet_count >= 2 {
        value += 1 + (triplet_count.saturating_sub(2) as i32).min(2);
    }

    // 8. Terminal/honor concentration (混老头/清老头 potential)
    let total_tiles: u8 = all_counts.iter().sum();
    if total_tiles > 0 {
        let terminal_honor_pct = (terminal_count + honor_count) as f32 / total_tiles as f32;
        if terminal_honor_pct > 0.6 {
            value += 1;
        }
    }

    // 9. 清龙 potential: one suit covers low(1-3), mid(4-6), and high(7-9) groups
    for suit in 0..3 {
        let base = suit * 9;
        let has_low = (0..3).any(|i| all_counts[base + i] > 0);
        let has_mid = (3..6).any(|i| all_counts[base + i] > 0);
        let has_high = (6..9).any(|i| all_counts[base + i] > 0);
        if has_low && has_mid && has_high {
            value += 1;
            break;
        }
    }

    // 10. 连六 potential: 6+ consecutive number positions in one suit
    for suit in 0..3 {
        let base = suit * 9;
        let mut max_run = 0u8;
        let mut current_run = 0u8;
        for i in 0..9 {
            if all_counts[base + i] > 0 {
                current_run += 1;
                max_run = max_run.max(current_run);
            } else {
                current_run = 0;
            }
        }
        if max_run >= 6 {
            value += 1;
            break;
        }
    }

    // 11. 五门齐 potential: tiles in all 3 suits + winds + dragons
    let has_suits = (0..9).any(|i| all_counts[i] > 0)
        && (9..18).any(|i| all_counts[i] > 0)
        && (18..27).any(|i| all_counts[i] > 0);
    let has_honors = (27..31).any(|i| all_counts[i] > 0) && (31..34).any(|i| all_counts[i] > 0);
    if has_suits && has_honors {
        value += 1;
    }

    // 12. 三色三步高/花龙 potential: 5-consecutive numbers spanning 3 suits
    for n in 0..7 {
        let mut suit_bits = 0u8;
        for offset in 0..5 {
            for s in 0..3 {
                if all_counts[s * 9 + n + offset] > 0 {
                    suit_bits |= 1 << s;
                }
            }
        }
        if suit_bits.count_ones() >= 3 {
            value += 1;
            break;
        }
    }

    // 13. 箭刻/风刻 potential: dragon or wind pung (≥3 of a kind)
    let has_dragon_pung = (31..=33).any(|i| all_counts[i] >= 3);
    let has_wind_pung = (27..31).any(|i| all_counts[i] >= 3);
    if has_dragon_pung {
        value += 1;
    }
    if has_wind_pung {
        value += 1;
    }

    // 14. 双同刻/三同刻 potential: same number ≥3 in 2 or 3 suits
    for num in 0..9 {
        let pung_suits = (all_counts[num] >= 3) as i32
            + (all_counts[9 + num] >= 3) as i32
            + (all_counts[18 + num] >= 3) as i32;
        if pung_suits >= 3 {
            value += 2;
        } else if pung_suits >= 2 {
            value += 1;
        }
    }

    // 15. 四归一 potential: any single tile appears 4 times
    if all_counts.iter().any(|&c| c >= 4) {
        value += 1;
    }

    // 16. 老少副 potential: same suit has both 123 and 789 sequences
    for suit in 0..3 {
        let base = suit * 9;
        let has_123 = all_counts[base] > 0 && all_counts[base + 1] > 0 && all_counts[base + 2] > 0;
        let has_789 =
            all_counts[base + 6] > 0 && all_counts[base + 7] > 0 && all_counts[base + 8] > 0;
        if has_123 && has_789 {
            value += 1;
            break;
        }
    }

    // 17. 一般高 potential: 3 consecutive numbers in one suit each ≥2
    'yiban_gao: for suit in 0..3 {
        let base = suit * 9;
        for i in 0..7 {
            if all_counts[base + i] >= 2
                && all_counts[base + i + 1] >= 2
                && all_counts[base + i + 2] >= 2
            {
                value += 1;
                break 'yiban_gao;
            }
        }
    }

    // 18. 全带幺 potential: each suit present has at least one terminal (1 or 9)
    let mut quandaidai = true;
    for suit in 0..3 {
        let base = suit * 9;
        let suit_has_tiles = (0..9).any(|i| all_counts[base + i] > 0);
        if suit_has_tiles && all_counts[base] == 0 && all_counts[base + 8] == 0 {
            quandaidai = false;
            break;
        }
    }
    if quandaidai {
        value += 1;
    }

    // 19/20. 大于五/小于五 potential: tiles skewed to high(6-9) or low(1-4) range
    if honor_count == 0 {
        let mut low_tiles: u8 = all_counts[..4].iter().sum(); // tiles 1-4 in wan
        low_tiles += all_counts[9..13].iter().sum::<u8>(); // tiles 1-4 in tong
        low_tiles += all_counts[18..22].iter().sum::<u8>(); // tiles 1-4 in tiao
        let mut high_tiles: u8 = all_counts[5..9].iter().sum(); // tiles 6-9 in wan
        high_tiles += all_counts[14..18].iter().sum::<u8>(); // tiles 6-9 in tong
        high_tiles += all_counts[23..27].iter().sum::<u8>(); // tiles 6-9 in tiao
        let tile5_total = all_counts[4] + all_counts[13] + all_counts[22];
        let numeric_total = low_tiles + high_tiles + tile5_total;
        if numeric_total > 0 {
            if high_tiles as f32 / numeric_total as f32 > 0.8 && tile5_total == 0 {
                value += 1; // 大于五 potential
            }
            if low_tiles as f32 / numeric_total as f32 > 0.8 && tile5_total == 0 {
                value += 1; // 小于五 potential
            }
        }
    }

    // 21. 全带五 potential: tile 5 in all 3 suits
    if all_counts[4] > 0 && all_counts[13] > 0 && all_counts[22] > 0 {
        value += 1;
    }

    // 22. 平和 potential: no triplets, no honor pairs
    let honor_pair_count = (27..33).filter(|&i| all_counts[i] >= 2).count();
    if triplet_count == 0 && honor_pair_count == 0 {
        value += 1;
    }

    FanPotential {
        value: value.clamp(0, 20),
    }
}

/// Minimum fan potential heuristic for a hand to realistically reach 国标's 8番 minimum.
/// Below this threshold, entering tenpai is usually a trap — the hand can't legally win.
const MIN_FAN_POTENTIAL_FOR_TENPAI: i32 = 6;

pub(crate) fn shaping_reward(before: RewardSnapshot, after: RewardSnapshot) -> f32 {
    let shanten_delta = before.shanten - after.shanten;
    let fan_delta = after.fan_potential - before.fan_potential;
    let shanten_reward = shanten_delta.clamp(-1, 1) as f32 * 0.10;
    let fan_reward = fan_delta.clamp(-1, 1) as f32 * 0.10;
    let tenpai_bonus = if before.shanten > 0 && after.shanten == 0 {
        0.15
    } else {
        0.0
    };
    // 国标 requires minimum 8番 to win. Penalize entering tenpai when fan potential
    // is too low — the hand can't legally win and is just dealing-in bait.
    // Scales from 0 at threshold to fully negating tenpai_bonus at 3 below threshold.
    let tenpai_quality_penalty = if before.shanten > 0 && after.shanten == 0 && after.fan_potential < MIN_FAN_POTENTIAL_FOR_TENPAI {
        let deficit = (MIN_FAN_POTENTIAL_FOR_TENPAI - after.fan_potential).min(3) as f32;
        -0.05 * deficit
    } else {
        0.0
    };
    shanten_reward + fan_reward + tenpai_bonus + tenpai_quality_penalty
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
        // Mixed hand without 三色同顺 patterns (scattered tiles)
        let mixed = string_keys(&[
            "w1", "w2", "w4", "t2", "t5", "t7", "b3", "b6", "b8", "east", "south",
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
    fn fan_potential_detects_triple_chow_pattern() {
        let triple_chow = string_keys(&[
            "w1", "t1", "b1", "w4", "t4", "b4", "w7", "t7", "b7", "east", "south",
        ]);
        let scattered = string_keys(&[
            "w1", "w2", "w4", "t2", "t5", "t7", "b3", "b6", "b8", "east", "south",
        ]);

        assert!(
            fan_potential_for_tile_keys(&triple_chow, Some("east"), Some("east")).value
                > fan_potential_for_tile_keys(&scattered, Some("east"), Some("east")).value
        );
    }

    #[test]
    fn fan_potential_detects_pure_dragon_shape() {
        // 清龙: one suit covers low(1-3), mid(4-6), high(7-9)
        let pure_dragon = string_keys(&[
            "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "east", "south",
        ]);
        let scattered = string_keys(&[
            "w1", "w2", "w4", "t2", "t5", "t7", "b3", "b6", "b8", "east", "south",
        ]);

        assert!(
            fan_potential_for_tile_keys(&pure_dragon, Some("east"), Some("east")).value
                > fan_potential_for_tile_keys(&scattered, Some("east"), Some("east")).value
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

        assert!(shaping_reward(before, after).abs() <= 0.35);
    }

    fn string_keys(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|key| (*key).to_string()).collect()
    }
}
