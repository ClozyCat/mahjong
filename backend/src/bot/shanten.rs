use super::context::{HONOR_TILE_START, TILE_KIND_COUNT, TileCounts};

pub(crate) fn min_shanten_for_counts(concealed_counts: &TileCounts, open_meld_count: usize) -> i32 {
    standard_shanten_with_open_melds(concealed_counts, open_meld_count)
        .min(seven_pairs_shanten(concealed_counts, open_meld_count))
        .min(thirteen_orphans_shanten(concealed_counts, open_meld_count))
        .min(special_knitted_shanten(concealed_counts, open_meld_count))
}

const KNITTED_PATTERNS: [[usize; 9]; 6] = [
    [0, 3, 6, 10, 13, 16, 20, 23, 26],
    [0, 3, 6, 19, 22, 25, 11, 14, 17],
    [9, 12, 15, 1, 4, 7, 20, 23, 26],
    [9, 12, 15, 19, 22, 25, 2, 5, 8],
    [18, 21, 24, 1, 4, 7, 11, 14, 17],
    [18, 21, 24, 10, 13, 16, 2, 5, 8],
];
const HONOR_INDICES: [usize; 7] = [27, 28, 29, 30, 31, 32, 33];

fn standard_shanten_with_open_melds(counts: &TileCounts, open_meld_count: usize) -> i32 {
    fn dfs(
        counts: &mut TileCounts,
        start_index: usize,
        melds: i32,
        taatsu: i32,
        has_pair: i32,
        open_meld_count: i32,
        best: &mut i32,
    ) {
        let total_melds = melds + open_meld_count;
        if total_melds > 4 {
            return;
        }
        let available_taatsu = (4 - total_melds).max(0);
        let capped_taatsu = taatsu.min(available_taatsu);
        let shanten = 8 - total_melds * 2 - capped_taatsu - has_pair;
        if shanten < *best {
            *best = shanten;
        }
        if *best <= -1 {
            return;
        }

        let Some(tile_index) = (start_index..TILE_KIND_COUNT).find(|index| counts[*index] > 0)
        else {
            return;
        };

        if counts[tile_index] >= 3 {
            counts[tile_index] -= 3;
            dfs(
                counts,
                tile_index,
                melds + 1,
                taatsu,
                has_pair,
                open_meld_count,
                best,
            );
            counts[tile_index] += 3;
        }

        if tile_index < HONOR_TILE_START
            && tile_index % 9 <= 6
            && counts[tile_index + 1] > 0
            && counts[tile_index + 2] > 0
        {
            counts[tile_index] -= 1;
            counts[tile_index + 1] -= 1;
            counts[tile_index + 2] -= 1;
            dfs(
                counts,
                tile_index,
                melds + 1,
                taatsu,
                has_pair,
                open_meld_count,
                best,
            );
            counts[tile_index] += 1;
            counts[tile_index + 1] += 1;
            counts[tile_index + 2] += 1;
        }

        if has_pair == 0 && counts[tile_index] >= 2 {
            counts[tile_index] -= 2;
            dfs(counts, tile_index, melds, taatsu, 1, open_meld_count, best);
            counts[tile_index] += 2;
        }

        if taatsu < 4 {
            if counts[tile_index] >= 2 {
                counts[tile_index] -= 2;
                dfs(
                    counts,
                    tile_index,
                    melds,
                    taatsu + 1,
                    has_pair,
                    open_meld_count,
                    best,
                );
                counts[tile_index] += 2;
            }

            if tile_index < HONOR_TILE_START && tile_index % 9 <= 7 && counts[tile_index + 1] > 0 {
                counts[tile_index] -= 1;
                counts[tile_index + 1] -= 1;
                dfs(
                    counts,
                    tile_index,
                    melds,
                    taatsu + 1,
                    has_pair,
                    open_meld_count,
                    best,
                );
                counts[tile_index] += 1;
                counts[tile_index + 1] += 1;
            }

            if tile_index < HONOR_TILE_START && tile_index % 9 <= 6 && counts[tile_index + 2] > 0 {
                counts[tile_index] -= 1;
                counts[tile_index + 2] -= 1;
                dfs(
                    counts,
                    tile_index,
                    melds,
                    taatsu + 1,
                    has_pair,
                    open_meld_count,
                    best,
                );
                counts[tile_index] += 1;
                counts[tile_index + 2] += 1;
            }
        }

        counts[tile_index] -= 1;
        dfs(
            counts,
            tile_index,
            melds,
            taatsu,
            has_pair,
            open_meld_count,
            best,
        );
        counts[tile_index] += 1;
    }

    let mut best = 8;
    let mut working = *counts;
    dfs(&mut working, 0, 0, 0, 0, open_meld_count as i32, &mut best);
    best
}

fn seven_pairs_shanten(counts: &TileCounts, open_meld_count: usize) -> i32 {
    if open_meld_count > 0 {
        return i32::MAX / 4;
    }
    let pair_units = counts
        .iter()
        .map(|count| i32::from(*count / 2))
        .sum::<i32>();
    let single_units = counts
        .iter()
        .map(|count| i32::from(*count % 2))
        .sum::<i32>();
    6 - pair_units + (7 - pair_units - single_units).max(0)
}

fn thirteen_orphans_shanten(counts: &TileCounts, open_meld_count: usize) -> i32 {
    if open_meld_count > 0 {
        return i32::MAX / 4;
    }
    const ORPHAN_INDICES: [usize; 13] = [0, 8, 9, 17, 18, 26, 27, 28, 29, 30, 31, 32, 33];
    let unique_count = ORPHAN_INDICES
        .iter()
        .filter(|index| counts[**index] > 0)
        .count() as i32;
    let has_pair = ORPHAN_INDICES.iter().any(|index| counts[*index] >= 2) as i32;
    13 - unique_count - has_pair
}

fn special_knitted_shanten(counts: &TileCounts, open_meld_count: usize) -> i32 {
    if open_meld_count > 1 {
        return i32::MAX / 4;
    }

    let mut best_missing = i32::MAX / 4;
    for pattern in KNITTED_PATTERNS {
        if open_meld_count == 0 {
            best_missing = best_missing.min(knitted_singletons_missing(counts, &pattern));
        }
        best_missing = best_missing.min(knitted_straight_completion_missing(
            counts,
            &pattern,
            open_meld_count,
        ));
    }
    best_missing - 1
}

fn knitted_singletons_missing(counts: &TileCounts, pattern: &[usize; 9]) -> i32 {
    let mut best = i32::MAX / 4;
    for honor_count in 5..=7 {
        let suited_count = 14 - honor_count;
        best = best.min(
            smallest_singleton_missing(counts, &HONOR_INDICES, honor_count)
                + smallest_singleton_missing(counts, pattern, suited_count),
        );
    }
    best
}

fn smallest_singleton_missing(
    counts: &TileCounts,
    candidate_indices: &[usize],
    target_count: usize,
) -> i32 {
    let mut missing_costs = candidate_indices
        .iter()
        .map(|index| i32::from(counts[*index] == 0))
        .collect::<Vec<_>>();
    missing_costs.sort_unstable();
    missing_costs.into_iter().take(target_count).sum()
}

fn knitted_straight_completion_missing(
    counts: &TileCounts,
    pattern: &[usize; 9],
    open_meld_count: usize,
) -> i32 {
    let mut best = i32::MAX / 4;
    if open_meld_count == 1 {
        for pair_index in 0..TILE_KIND_COUNT {
            best = best.min(knitted_target_missing(
                counts,
                pattern,
                &[pair_index, pair_index],
            ));
        }
        return best;
    }

    for pair_index in 0..TILE_KIND_COUNT {
        for triplet_index in 0..TILE_KIND_COUNT {
            best = best.min(knitted_target_missing(
                counts,
                pattern,
                &[
                    pair_index,
                    pair_index,
                    triplet_index,
                    triplet_index,
                    triplet_index,
                ],
            ));
        }
        for meld in sequence_melds() {
            best = best.min(knitted_target_missing(
                counts,
                pattern,
                &[pair_index, pair_index, meld[0], meld[1], meld[2]],
            ));
        }
    }
    best
}

fn knitted_target_missing(
    counts: &TileCounts,
    pattern: &[usize; 9],
    extra_indices: &[usize],
) -> i32 {
    let mut target = [0_u8; TILE_KIND_COUNT];
    for index in pattern {
        target[*index] += 1;
    }
    for index in extra_indices {
        target[*index] += 1;
    }
    if target.iter().any(|count| *count > 4) {
        return i32::MAX / 4;
    }
    target_missing(counts, &target)
}

fn sequence_melds() -> impl Iterator<Item = [usize; 3]> {
    [0, 9, 18].into_iter().flat_map(|suit_offset| {
        (0..=6).map(move |rank_offset| {
            let start = suit_offset + rank_offset;
            [start, start + 1, start + 2]
        })
    })
}

fn target_missing(counts: &TileCounts, target: &TileCounts) -> i32 {
    target
        .iter()
        .zip(counts.iter())
        .map(|(target_count, actual_count)| target_count.saturating_sub(*actual_count) as i32)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::context::tile_index;
    use crate::rules::standard::ready_hand::is_tenpai_hand_with_melds;

    const TILE_KEYS: [&str; TILE_KIND_COUNT] = [
        "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "t1", "t2", "t3", "t4", "t5", "t6",
        "t7", "t8", "t9", "b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9", "east", "south",
        "west", "north", "red", "green", "white",
    ];

    #[test]
    fn min_shanten_counts_complete_lesser_honours_knitted_hand() {
        let counts = counts_for_keys(&[
            "w1", "w4", "w7", "t2", "t5", "t8", "b3", "b6", "b9", "east", "south", "west", "north",
            "red",
        ]);

        assert_eq!(min_shanten_for_counts(&counts, 0), -1);
    }

    #[test]
    fn min_shanten_counts_lesser_honours_knitted_tenpai() {
        let counts = counts_for_keys(&[
            "w1", "w4", "w7", "t2", "t5", "t8", "b3", "b6", "east", "south", "west", "north", "red",
        ]);

        assert_eq!(min_shanten_for_counts(&counts, 0), 0);
    }

    #[test]
    fn min_shanten_counts_complete_greater_honours_knitted_hand() {
        let counts = counts_for_keys(&[
            "w1", "w4", "w7", "t2", "t5", "t8", "b3", "east", "south", "west", "north", "red",
            "green", "white",
        ]);

        assert_eq!(min_shanten_for_counts(&counts, 0), -1);
    }

    #[test]
    fn min_shanten_counts_complete_knitted_straight_with_honours() {
        let counts = counts_for_keys(&[
            "w1", "w4", "w7", "t2", "t5", "t8", "b3", "b6", "b9", "east", "south", "west", "north",
            "red",
        ]);

        assert_eq!(min_shanten_for_counts(&counts, 0), -1);
    }

    #[test]
    fn min_shanten_counts_complete_knitted_straight_with_pung_and_pair() {
        let counts = counts_for_keys(&[
            "w1", "w4", "w7", "t2", "t5", "t8", "b3", "b6", "b9", "east", "east", "red", "red",
            "red",
        ]);

        assert_eq!(min_shanten_for_counts(&counts, 0), -1);
    }

    #[test]
    fn honours_knitted_shanten_requires_closed_hand() {
        let counts = counts_for_keys(&[
            "w1", "w4", "w7", "t2", "t5", "t8", "b3", "b6", "b9", "east", "south",
        ]);

        assert!(knitted_singletons_missing(&counts, &KNITTED_PATTERNS[0]) <= 3);
        assert!(special_knitted_shanten(&counts, 2) > 8);
    }

    #[test]
    fn min_shanten_counts_open_knitted_straight_with_pair_wait() {
        let counts = counts_for_keys(&[
            "w1", "w4", "w7", "t2", "t5", "t8", "b3", "b6", "b9", "red", "red",
        ]);

        assert_eq!(min_shanten_for_counts(&counts, 1), -1);
    }

    #[test]
    fn open_knitted_straight_shanten_allows_only_one_open_meld() {
        let counts = counts_for_keys(&["w1", "w4", "w7", "t2", "t5", "t8", "b3", "b6"]);

        assert!(special_knitted_shanten(&counts, 2) > 8);
    }

    #[test]
    fn seven_pairs_shanten_counts_quad_as_two_pairs_for_complete_hand() {
        let counts = counts_for_keys(&[
            "w1", "w1", "w1", "w1", "w2", "w2", "w3", "w3", "t4", "t4", "t5", "t5", "red", "red",
        ]);

        assert_eq!(seven_pairs_shanten(&counts, 0), -1);
        assert_eq!(min_shanten_for_counts(&counts, 0), -1);
    }

    #[test]
    fn seven_pairs_shanten_counts_triplet_as_pair_plus_wait_for_tenpai() {
        let counts = counts_for_keys(&[
            "w1", "w1", "w1", "w2", "w2", "w3", "w3", "t4", "t4", "t5", "t5", "b6", "b6",
        ]);

        assert_eq!(seven_pairs_shanten(&counts, 0), 0);
        assert_eq!(min_shanten_for_counts(&counts, 0), 0);
    }

    #[test]
    fn min_shanten_zero_matches_actual_tenpai_for_sampled_closed_hands() {
        let mut seed = 0x5EED_2026_u64;
        for _ in 0..256 {
            let hand_indices = sampled_closed_hand_indices(&mut seed, 13);
            let counts = counts_for_indices(&hand_indices);
            let keys = hand_indices
                .iter()
                .map(|index| TILE_KEYS[*index].to_string())
                .collect::<Vec<_>>();
            let shanten = min_shanten_for_counts(&counts, 0);
            let actual_tenpai = is_tenpai_hand_with_melds(&keys, &[]);

            assert_eq!(
                shanten == 0,
                actual_tenpai,
                "hand={:?} shanten={shanten} actual_tenpai={actual_tenpai}",
                keys
            );
        }
    }

    #[test]
    fn min_shanten_ready_matches_actual_ready_state_for_sampled_drawn_hands() {
        let mut seed = 0xD0D0_2026_u64;
        for _ in 0..256 {
            let hand_indices = sampled_closed_hand_indices(&mut seed, 14);
            let counts = counts_for_indices(&hand_indices);
            let keys = hand_indices
                .iter()
                .map(|index| TILE_KEYS[*index].to_string())
                .collect::<Vec<_>>();
            let shanten = min_shanten_for_counts(&counts, 0);
            let actual_ready = is_tenpai_hand_with_melds(&keys, &[]);

            assert_eq!(
                shanten <= 0,
                actual_ready,
                "hand={:?} shanten={shanten} actual_ready={actual_ready}",
                keys
            );
        }
    }

    #[test]
    fn min_shanten_one_matches_one_draw_discard_tenpai_for_sampled_closed_hands() {
        let mut seed = 0x1BAD_2026_u64;
        for _ in 0..8 {
            let hand_indices = sampled_closed_hand_indices(&mut seed, 13);
            let counts = counts_for_indices(&hand_indices);
            let keys = hand_indices
                .iter()
                .map(|index| TILE_KEYS[*index].to_string())
                .collect::<Vec<_>>();
            let shanten = min_shanten_for_counts(&counts, 0);
            let actual_one_or_less = can_reach_tenpai_after_one_draw_discard(&keys);

            assert_eq!(
                shanten <= 1,
                actual_one_or_less,
                "hand={:?} shanten={shanten} actual_one_or_less={actual_one_or_less}",
                keys
            );
        }
    }

    fn counts_for_keys(keys: &[&str]) -> TileCounts {
        let mut counts = [0_u8; TILE_KIND_COUNT];
        for key in keys {
            counts[tile_index(key).expect("valid tile key")] += 1;
        }
        counts
    }

    fn counts_for_indices(indices: &[usize]) -> TileCounts {
        let mut counts = [0_u8; TILE_KIND_COUNT];
        for index in indices {
            counts[*index] += 1;
        }
        counts
    }

    fn can_reach_tenpai_after_one_draw_discard(keys: &[String]) -> bool {
        let counts = counts_for_string_keys(keys);
        for draw_index in 0..TILE_KIND_COUNT {
            if counts[draw_index] >= 4 {
                continue;
            }
            let mut drawn = keys.to_vec();
            drawn.push(TILE_KEYS[draw_index].to_string());
            if !crate::rules::scoring::decompose_winning_hand_with_melds(&drawn, &[]).is_empty() {
                return true;
            }
            for discard_index in 0..drawn.len() {
                let mut after_discard = drawn.clone();
                after_discard.remove(discard_index);
                if is_tenpai_hand_with_melds(&after_discard, &[]) {
                    return true;
                }
            }
        }
        false
    }

    fn counts_for_string_keys(keys: &[String]) -> TileCounts {
        let mut counts = [0_u8; TILE_KIND_COUNT];
        for key in keys {
            counts[tile_index(key).expect("valid tile key")] += 1;
        }
        counts
    }

    fn sampled_closed_hand_indices(seed: &mut u64, tile_count: usize) -> Vec<usize> {
        let mut wall = Vec::with_capacity(TILE_KIND_COUNT * 4);
        for index in 0..TILE_KIND_COUNT {
            wall.extend([index; 4]);
        }
        for index in (1..wall.len()).rev() {
            *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let swap_index = (*seed as usize) % (index + 1);
            wall.swap(index, swap_index);
        }
        wall.into_iter().take(tile_count).collect()
    }
}
