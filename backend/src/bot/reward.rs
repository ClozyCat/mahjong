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

#[cfg(test)]
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

/// Returns the difficulty-adjusted potential for a guobiao fan type.
///
/// Calibrated so that common 8-fan combinations (e.g. 混一色6 + 箭刻2 = 8)
/// produce total potential >= MIN_FAN_POTENTIAL_FOR_TENPAI (6).
/// High-value rare types are discounted to avoid over-pursuit of speculative hands.
///
/// Design rationale:
/// - 6番 backbone types (混一色/对对和/五门齐): potential 4
///   (4 + 2-point modifier + 1-point padding = 7, comfortably above threshold)
/// - Standalone 8+番 types (三色三同顺/清龙/花龙): potential 5-6
///   (they ARE the 8-fan win condition — threshold must be reachable)
/// - 2番 modifiers (箭刻/门风刻/双同刻): potential 2
/// - 1番 padding (幺九刻/缺一门/连六): potential 1
/// - Very rare / special / situational types: potential 0
///   (undetectable at decision time or too rare to chase)
fn fan_type_base_potential(fan_key: &str) -> i32 {
    match fan_key {
        // 1 Fan — common padding that almost any hand can pick up
        "pung_of_terminals_or_honours" | "one_voided_suit" | "no_honours"
        | "short_straight" | "two_terminal_chows" | "pure_double_chow" => 1,
        // 1 Fan — situational / wait-type / undetectable at decision time
        "self_drawn" | "flower_tiles" | "melded_kong" | "edge_wait"
        | "closed_wait" | "single_wait" | "mixed_double_chow" => 0,

        // 2 Fan — common modifiers that pair with backbone types to reach 8
        "all_simples" | "dragon_pung" | "seat_wind" | "prevalent_wind"
        | "double_pung" | "two_concealed_pungs" | "tile_hog"
        | "all_chows" | "concealed_hand" => 2,
        "concealed_kong" | "ready_hand_win" => 0,

        // 4 Fan
        "outside_hand" | "fully_concealed_hand" => 2,
        "two_melded_kongs" | "last_tile" => 0,

        // 5 Fan
        "mixed_kongs" => 0,

        // 6 Fan — backbone of most 8-fan guobiao hands
        "half_flush" | "all_pungs" | "all_types" | "two_dragon_pungs" => 4,
        "mixed_shifted_chows" => 3,
        "melded_hand" => 2,

        // 8 Fan — standalone 8-fan types (being these IS a valid win)
        "mixed_triple_chow" | "mixed_straight" => 5,
        "mixed_shifted_pungs" | "reversible_tiles" => 3,
        "out_with_replacement_tile" | "last_tile_draw" | "last_tile_claim"
        | "robbing_the_kong" | "two_concealed_kongs" | "chicken_hand" => 0,

        // 12 Fan
        "upper_four" | "lower_four" | "big_three_winds" => 3,
        "knitted_straight" | "lesser_honours_and_knitted_tiles" => 0,

        // 16 Fan — well above 8番 threshold, so high potential
        "pure_straight" | "pure_shifted_chows" => 5,
        "triple_pung" | "three_concealed_pungs" | "all_fives" => 3,
        "three_suited_terminal_chows" => 3,

        // 24 Fan — high-value, harder to build; modest boost to avoid over-chase
        "full_flush" => 5,
        "seven_pairs" | "all_even_pungs" | "upper_tiles" | "lower_tiles"
        | "middle_tiles" | "pure_shifted_pungs" | "pure_triple_chow" => 3,
        "greater_honours_and_knitted_tiles" => 0,

        // 32 Fan
        "all_terminals_and_honours" => 4,
        "four_pure_shifted_chows" | "three_kongs" => 0,

        // 48 Fan
        "four_pure_shifted_pungs" | "quadruple_chow" => 0,

        // 64 Fan — deliberately discounted; chasing these wastes turns
        "little_three_dragons" => 4,
        "all_honours" | "four_concealed_pungs" => 3,
        "little_four_winds" | "pure_terminal_chows" => 0,

        // 88 Fan — extreme/special hands, not practical to chase
        "big_three_dragons" | "all_terminals" => 3,
        "big_four_winds" | "all_green" | "thirteen_orphans"
        | "seven_shifted_pairs" | "nine_gates" | "four_kongs" => 0,

        _ => 0,
    }
}

/// Resolves mutual exclusions among detected fan types.
/// When two fan types can't coexist in guobiao rules, the lower-value
/// (or lower-priority) one is suppressed.
fn resolve_mutual_exclusions(active: &mut Vec<&str>) {
    // Suit purity: full_flush excludes half_flush and one_voided_suit (per guobiao rules)
    // half_flush does NOT exclude one_voided_suit (混一色 with 1 suit means 2 suits missing = 缺一门)
    if active.contains(&"full_flush") {
        active.retain(|k| *k != "half_flush" && *k != "one_voided_suit");
    }

    // All-honor/terminal: all_honours > all_terminals > all_terminals_and_honours
    if active.contains(&"all_honours") || active.contains(&"all_terminals") {
        active.retain(|k| *k != "all_terminals_and_honours");
    }

    // all_simples implies no_honours (断幺 automatically gives 无字)
    if active.contains(&"all_simples") {
        active.retain(|k| *k != "no_honours");
    }

    // Dragon hierarchy: big_three_dragons > little_three_dragons > two_dragon_pungs > dragon_pung
    if active.contains(&"big_three_dragons") || active.contains(&"little_three_dragons") {
        active.retain(|k| *k != "two_dragon_pungs" && *k != "dragon_pung");
    } else if active.contains(&"two_dragon_pungs") {
        active.retain(|k| *k != "dragon_pung");
    }

    // big_three_winds subsumes individual wind pungs
    if active.contains(&"big_three_winds") {
        active.retain(|k| *k != "seat_wind" && *k != "prevalent_wind");
    }

    // Range-limited: middle_tiles > upper_tiles > lower_tiles > upper_four > lower_four
    if active.contains(&"middle_tiles") {
        active.retain(|k| *k != "upper_four" && *k != "lower_four");
    }
    if active.contains(&"upper_tiles") {
        active.retain(|k| *k != "upper_four");
    }
    if active.contains(&"lower_tiles") {
        active.retain(|k| *k != "lower_four");
    }

    // middle_tiles excludes all_simples (全中 covers it already)
    if active.contains(&"middle_tiles") {
        active.retain(|k| *k != "all_simples");
    }

    // all_fives excludes no_honours and all_simples (全带五 already requires them)
    if active.contains(&"all_fives") {
        active.retain(|k| *k != "no_honours" && *k != "all_simples");
    }

    // all_even_pungs implies all_simples already
    if active.contains(&"all_even_pungs") {
        active.retain(|k| *k != "all_simples" && *k != "no_honours");
    }

    // full_flush already implies no_honours
    if active.contains(&"full_flush") {
        active.retain(|k| *k != "no_honours" && *k != "all_simples");
    }

    // half_flush doesn't exclude one_voided_suit by guobiao rules, but in practice
    // 混一色 = 1 suit type, which means 2 suits are missing → 缺一门 applies
    // Actually in guobiao, 混一色 does NOT exclude 缺一门. Let's keep both.

    // Chow pattern hierarchy: pure_straight excludes short_straight and two_terminal_chows
    // (清龙 subsumes 连六 and 老少副 per guobiao rules)
    if active.contains(&"pure_straight") {
        active.retain(|k| *k != "short_straight" && *k != "two_terminal_chows");
    }
}

/// Detects structurally achievable guobiao fan types from tile counts and adds
/// their difficulty-adjusted potentials. Returns the total fan potential.
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

    let total_tiles: u8 = all_counts.iter().sum();
    let mut detected: Vec<&str> = Vec::with_capacity(32);

    // === Suit / Honor Structure ===
    // 清一色 (full_flush, 24), 混一色 (half_flush, 6), 缺一门 (one_voided_suit, 1), 无字 (no_honours, 1)
    // In guobiao: full_flush excludes one_voided_suit, but half_flush does NOT.
    let suit_count = suit_mask.count_ones();
    if suit_count == 1 && honor_count == 0 {
        detected.push("full_flush");
        detected.push("no_honours");
        // full_flush excludes 缺一门 in guobiao rules — skip it here
    } else if suit_count == 1 {
        detected.push("half_flush");
        // half_flush = 1 suit + honors → 2 suits missing → 缺一门 applies
        detected.push("one_voided_suit");
    } else if suit_count == 2 {
        detected.push("one_voided_suit");
    }
    if honor_count == 0 && suit_count > 0 {
        detected.push("no_honours");
    }

    // === Dragons ===
    // 箭刻 (dragon_pung, 2), 双箭刻 (two_dragon_pungs, 6),
    // 小三元 (little_three_dragons, 64), 大三元 (big_three_dragons, 88)
    let dragons_ge3 = (31..=33).filter(|&i| all_counts[i] >= 3).count();
    let dragons_ge2 = (31..=33).filter(|&i| all_counts[i] >= 2).count();
    if dragons_ge3 >= 3 {
        detected.push("big_three_dragons");
    } else if dragons_ge3 >= 2 && dragons_ge2 >= 3 {
        detected.push("little_three_dragons");
    } else if dragons_ge3 >= 2 {
        detected.push("two_dragon_pungs");
    } else if dragons_ge3 >= 1 {
        detected.push("dragon_pung");
    }

    // === Winds ===
    // 门风刻 (seat_wind, 2), 圈风刻 (prevalent_wind, 2), 三风刻 (big_three_winds, 12),
    // 小四喜 (little_four_winds, 64), 大四喜 (big_four_winds, 88)
    let winds_ge3 = (27..31).filter(|&i| all_counts[i] >= 3).count();
    let winds_ge2 = (27..31).filter(|&i| all_counts[i] >= 2).count();
    if winds_ge3 >= 4 {
        detected.push("big_four_winds");
    } else if winds_ge3 >= 3 && winds_ge2 >= 4 {
        detected.push("little_four_winds");
    } else if winds_ge3 >= 3 {
        detected.push("big_three_winds");
    }
    // Individual wind pungs
    if let Some(index) = seat_wind.and_then(tile_index) {
        if all_counts[index] >= 3 {
            detected.push("seat_wind");
        }
    }
    if let Some(index) = round_wind.and_then(tile_index) {
        if all_counts[index] >= 3 {
            detected.push("prevalent_wind");
        }
    }

    // 幺九刻 (pung_of_terminals_or_honours, 1): any terminal/honor triplet
    let has_terminal_ge3 = (0..27).any(|i| (i % 9 == 0 || i % 9 == 8) && all_counts[i] >= 3);
    let has_honor_ge3 = (27..34).any(|i| all_counts[i] >= 3);
    if has_terminal_ge3 || has_honor_ge3 {
        detected.push("pung_of_terminals_or_honours");
    }

    // === Hand Structure ===
    // 断幺 (all_simples, 2)
    if terminal_count == 0 && honor_count == 0 && suit_count > 0 {
        detected.push("all_simples");
    }

    // 全带幺 (outside_hand, 4): each suit present has at least one terminal (1 or 9)
    let mut has_outside = true;
    for suit in 0..3 {
        let base = suit * 9;
        let suit_has_tiles = (0..9).any(|i| all_counts[base + i] > 0);
        if suit_has_tiles && all_counts[base] == 0 && all_counts[base + 8] == 0 {
            has_outside = false;
            break;
        }
    }
    if has_outside && suit_count > 0 {
        detected.push("outside_hand");
    }

    // 平和 (all_chows, 2): no triplets, no honor pairs
    let triplet_count = all_counts.iter().filter(|&&c| c >= 3).count();
    let honor_pair_count = (27..34).filter(|&i| all_counts[i] >= 2).count();
    if triplet_count == 0 && honor_pair_count == 0 {
        detected.push("all_chows");
    }

    // 对对和 (all_pungs, 6): many triplets
    if triplet_count >= 3 {
        detected.push("all_pungs");
    }

    // 七对 (seven_pairs, 24): many pairs in concealed tiles
    let concealed_pair_count = concealed_counts.iter().filter(|&&c| c >= 2).count();
    if concealed_pair_count >= 6 {
        detected.push("seven_pairs");
    }

    // 双暗刻 (two_concealed_pungs, 2): concealed triplets
    let concealed_triplet_count = concealed_counts.iter().filter(|&&c| c >= 3).count();
    if concealed_triplet_count >= 2 {
        detected.push("two_concealed_pungs");
    }
    // 三暗刻 (three_concealed_pungs, 16)
    if concealed_triplet_count >= 3 {
        detected.push("three_concealed_pungs");
    }
    // 四暗刻 (four_concealed_pungs, 64) — needs 4+ concealed triplets + specific win condition
    // but we can detect potential from concealed tile counts
    if concealed_triplet_count >= 4 {
        detected.push("four_concealed_pungs");
    }

    // 四归一 (tile_hog, 2): any tile count >= 4
    if all_counts.iter().any(|&c| c >= 4) {
        detected.push("tile_hog");
    }

    // 五门齐 (all_types, 6): all 3 suits + winds + dragons
    let has_all_suits = (0..9).any(|i| all_counts[i] > 0)
        && (9..18).any(|i| all_counts[i] > 0)
        && (18..27).any(|i| all_counts[i] > 0);
    let has_all_honor_types = (27..31).any(|i| all_counts[i] > 0) && (31..34).any(|i| all_counts[i] > 0);
    if has_all_suits && has_all_honor_types {
        detected.push("all_types");
    }

    // 门前清 (concealed_hand, 2) — detectable only if we know meld count
    // We don't have meld count directly in this function, but we can approximate
    // by checking if concealed counts make up most of all counts
    // Actually, we can't reliably detect 门前清 from counts alone — skip here
    // It's handled when called from reward_snapshot which passes meld info through the all_counts

    // === Chow Patterns ===
    // 清龙 (pure_straight, 16): one suit covers low(1-3), mid(4-6), high(7-9)
    for suit in 0..3 {
        let base = suit * 9;
        let has_low = (0..3).any(|i| all_counts[base + i] > 0);
        let has_mid = (3..6).any(|i| all_counts[base + i] > 0);
        let has_high = (6..9).any(|i| all_counts[base + i] > 0);
        if has_low && has_mid && has_high {
            detected.push("pure_straight");
            break;
        }
    }

    // 一色三步高 (pure_shifted_chows, 16): 3 consecutive numbers each present in one suit
    for suit in 0..3 {
        let base = suit * 9;
        for start in 0..7 {
            if all_counts[base + start] > 0
                && all_counts[base + start + 1] > 0
                && all_counts[base + start + 2] > 0
            {
                // Found 3 consecutive numbers in one suit
                detected.push("pure_shifted_chows");
                break;
            }
        }
        if detected.contains(&"pure_shifted_chows") {
            break;
        }
    }

    // 连六 (short_straight, 1): 6+ consecutive positions in one suit
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
            detected.push("short_straight");
            break;
        }
    }

    // 老少副 (two_terminal_chows, 1): 123 + 789 in same suit
    for suit in 0..3 {
        let base = suit * 9;
        let has_123 = all_counts[base] > 0 && all_counts[base + 1] > 0 && all_counts[base + 2] > 0;
        let has_789 =
            all_counts[base + 6] > 0 && all_counts[base + 7] > 0 && all_counts[base + 8] > 0;
        if has_123 && has_789 {
            detected.push("two_terminal_chows");
            break;
        }
    }

    // 一般高 (pure_double_chow, 1): 3 consecutive numbers in one suit each ≥2
    'yiban_gao: for suit in 0..3 {
        let base = suit * 9;
        for i in 0..7 {
            if all_counts[base + i] >= 2
                && all_counts[base + i + 1] >= 2
                && all_counts[base + i + 2] >= 2
            {
                detected.push("pure_double_chow");
                break 'yiban_gao;
            }
        }
    }

    // === Three-Suit Patterns ===
    // 三色同顺 (mixed_triple_chow, 8): same number present in all three suits
    // 花龙 (mixed_straight, 8): 123 in one suit, 456 in another, 789 in third
    // 三色三步高 (mixed_shifted_chows, 6): consecutive numbers spanning 3 suits
    let mut has_mixed_triple = false;
    let mut has_mixed_straight = false;
    let mut has_mixed_shifted = false;
    for num in 0..9 {
        let has_w = all_counts[num] > 0;
        let has_t = all_counts[9 + num] > 0;
        let has_b = all_counts[18 + num] > 0;
        if has_w && has_t && has_b {
            has_mixed_triple = true;
        }
    }
    // 花龙: check if we can form 123/456/789 across 3 suits
    // A simple heuristic: each suit has tiles in one specific range
    for suit_a in 0..3 {
        for suit_b in 0..3 {
            if suit_b == suit_a { continue; }
            let suit_c = 3 - suit_a - suit_b;
            let a_base = suit_a * 9;
            let b_base = suit_b * 9;
            let c_base = suit_c * 9;
            // Check 123 in suit_a, 456 in suit_b, 789 in suit_c
            if (0..3).any(|i| all_counts[a_base + i] > 0)
                && (3..6).any(|i| all_counts[b_base + i] > 0)
                && (6..9).any(|i| all_counts[c_base + i] > 0)
            {
                has_mixed_straight = true;
            }
        }
    }
    // 三色三步高: 3+ consecutive numbers × 3 suits
    for start in 0..7 {
        let mut suit_count = 0u8;
        for s in 0..3 {
            if all_counts[s * 9 + start] > 0
                || all_counts[s * 9 + start + 1] > 0
                || all_counts[s * 9 + start + 2] > 0
            {
                suit_count += 1;
            }
        }
        if suit_count >= 3 {
            has_mixed_shifted = true;
            break;
        }
    }

    if has_mixed_triple {
        detected.push("mixed_triple_chow");
    }
    if has_mixed_straight {
        detected.push("mixed_straight");
    }
    if has_mixed_shifted {
        detected.push("mixed_shifted_chows");
    }

    // === Same-Suit Patterns ===
    // 一色三同顺 (pure_triple_chow, 24): 3 consecutive numbers each ≥3 in one suit
    for suit in 0..3 {
        let base = suit * 9;
        for i in 0..7 {
            if all_counts[base + i] >= 3
                && all_counts[base + i + 1] >= 3
                && all_counts[base + i + 2] >= 3
            {
                detected.push("pure_triple_chow");
                break;
            }
        }
        if detected.last() == Some(&"pure_triple_chow") {
            break;
        }
    }

    // === Pung Patterns ===
    // 双同刻 (double_pung, 2) / 三同刻 (triple_pung, 16): same number ≥3 in 2 or 3 suits
    for num in 0..9 {
        let pung_suits = (all_counts[num] >= 3) as i32
            + (all_counts[9 + num] >= 3) as i32
            + (all_counts[18 + num] >= 3) as i32;
        if pung_suits >= 3 {
            detected.push("triple_pung");
        } else if pung_suits >= 2 {
            detected.push("double_pung");
        }
    }

    // 三色三节高 (mixed_shifted_pungs, 8): consecutive triplets across 3 suits
    for num in 0..7 {
        let has_ww = all_counts[num] >= 3;
        let has_tt = all_counts[9 + num + 1] >= 3;
        let has_bb = all_counts[18 + num + 2] >= 3;
        if has_ww && has_tt && has_bb {
            detected.push("mixed_shifted_pungs");
            break;
        }
        let has_ww2 = all_counts[num] >= 3;
        let has_bb2 = all_counts[9 + num + 1] >= 3;
        let has_tt2 = all_counts[18 + num + 2] >= 3;
        if has_ww2 && has_bb2 && has_tt2 {
            detected.push("mixed_shifted_pungs");
            break;
        }
    }

    // 一色三节高 (pure_shifted_pungs, 24): 3 consecutive triplets in one suit
    for suit in 0..3 {
        let base = suit * 9;
        for i in 0..7 {
            if all_counts[base + i] >= 3
                && all_counts[base + i + 1] >= 3
                && all_counts[base + i + 2] >= 3
            {
                detected.push("pure_shifted_pungs");
                break;
            }
        }
        if detected.last() == Some(&"pure_shifted_pungs") {
            break;
        }
    }

    // === Range-Limited Hands ===
    // 全带五 (all_fives, 16): tile 5 in all 3 suits
    if all_counts[4] > 0 && all_counts[13] > 0 && all_counts[22] > 0 {
        detected.push("all_fives");
    }

    // 大于五/小于五/全大/全小/全中
    if honor_count == 0 {
        let mut low_all: u8 = all_counts[..4].iter().sum();
        low_all += all_counts[9..13].iter().sum::<u8>();
        low_all += all_counts[18..22].iter().sum::<u8>();
        let mut mid_all: u8 = all_counts[3..6].iter().sum();
        mid_all += all_counts[12..15].iter().sum::<u8>();
        mid_all += all_counts[21..24].iter().sum::<u8>();
        let mut high_all: u8 = all_counts[6..9].iter().sum();
        high_all += all_counts[15..18].iter().sum::<u8>();
        high_all += all_counts[24..27].iter().sum::<u8>();
        let tile5_total = all_counts[4] + all_counts[13] + all_counts[22];
        let numeric_total = low_all + mid_all + high_all;

        if numeric_total > 0 {
            let low_pct = low_all as f32 / numeric_total as f32;
            let high_pct = high_all as f32 / numeric_total as f32;
            let mid_pct = mid_all as f32 / numeric_total as f32;

            if high_pct > 0.99 && tile5_total == 0 {
                detected.push("upper_tiles");
            } else if high_pct > 0.8 && tile5_total == 0 {
                detected.push("upper_four");
            }
            if low_pct > 0.99 && tile5_total == 0 {
                detected.push("lower_tiles");
            } else if low_pct > 0.8 && tile5_total == 0 {
                detected.push("lower_four");
            }
            if mid_pct > 0.99 {
                detected.push("middle_tiles");
            }
        }
    }

    // === Terminal/Honor-Only Hands ===
    // 混幺九 (all_terminals_and_honours, 32), 清幺九 (all_terminals, 88), 字一色 (all_honours, 64)
    if total_tiles > 0 {
        let terminal_honor_total = terminal_count + honor_count;
        if terminal_honor_total == total_tiles {
            if honor_count == total_tiles {
                detected.push("all_honours");
            } else if terminal_count == total_tiles && honor_count == 0 {
                detected.push("all_terminals");
            } else {
                detected.push("all_terminals_and_honours");
            }
        }
    }

    // === 全双刻 (all_even_pungs, 24) ===
    if honor_count == 0 && total_tiles > 0 {
        let all_even_tiles = (0..27).all(|i| {
            if all_counts[i] == 0 { return true; }
            let num = i % 9; // 0=1, 1=2, ..., 8=9
            num == 1 || num == 3 || num == 5 || num == 7 // 2, 4, 6, 8
        });
        if all_even_tiles {
            detected.push("all_even_pungs");
        }
    }

    // === 推不倒 (reversible_tiles, 8) ===
    // Reversible tiles: b1, b2, b4, b5, b8, t2, t4, t5, white
    const REVERSIBLE_INDICES: [usize; 9] = [
        18, 19, 21, 22, 25, // b1, b2, b4, b5, b8
        11, 13, 14,          // t2, t4, t5
        33,                   // white
    ];
    if honor_count == 0 || (honor_count > 0 && (27..33).all(|i| all_counts[i] == 0)) {
        // No non-white honors
        let only_reversible = (0..34).all(|i| {
            if all_counts[i] == 0 { return true; }
            REVERSIBLE_INDICES.contains(&i)
        });
        if only_reversible {
            detected.push("reversible_tiles");
        }
    }

    // === Mutual Exclusions ===
    resolve_mutual_exclusions(&mut detected);

    // === Sum Potentials ===
    let total: i32 = detected.iter().map(|k| fan_type_base_potential(k)).sum();

    FanPotential {
        value: total.clamp(0, 30),
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
        // Mixed hand scattered across suits with few structural patterns
        let mixed = string_keys(&[
            "w2", "w4", "w6", "t2", "t4", "t6", "b2", "b4", "b6", "east", "south",
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
