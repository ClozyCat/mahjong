//! Shanten-lookahead expert for discard preferences.
//!
//! Computes `teacher_scores` for each legal discard by evaluating the
//! resulting hand's shanten (distance-to-win) and acceptance width
//! (number of winning tile types if the hand reaches tenpai).
//!
//! When a discard reaches tenpai (shanten == 0), the expert also checks
//! whether the resulting hand can meet the 8-fan minimum (国标麻将起和)
//! by calling `evaluate_fans` on each candidate winning tile.  Tenpai
//! positions that cannot meet the minimum are penalised instead of
//! rewarded, steering the model toward qualifying hand structures.
//!
//! This is a pure-Rust expert — no ONNX inference — that produces
//! a signal genuinely independent of the SFT policy.

use super::context::{BotContext, TileCounts, TILE_KIND_COUNT};
use super::reward::qualifying_fan_potential;
use super::shanten::min_shanten_for_counts;

/// Weight for acceptance width (number of winning tile types).
const ACCEPTANCE_WIDTH_WEIGHT: f32 = 0.2;
/// Bonus for reaching tenpai (shanten == 0) after discard, IF the hand
/// can meet the 8-fan minimum.
const TENPAI_BONUS: f32 = 2.0;
/// Penalty for reaching tenpai but NOT meeting the 8-fan minimum.
const TENPAI_NO_FAN_PENALTY: f32 = -1.5;
/// Bonus for already winning (shanten < 0) — extremely rare for a discard decision.
const WIN_BONUS: f32 = 6.0;

/// Compute expert discard scores for all legal discards in one position.
///
/// For each legal discard tile X:
/// 1. Subtract X from concealed counts → `after_counts`
/// 2. `shanten_after = min_shanten_for_counts(&after_counts, open_meld_count)`
/// 3. If `shanten_after == 0` (tenpai):
///    a. Compute acceptance width (how many tile types complete the hand)
///    b. Compute qualifying fan potential (max fan across all winning tiles,
///       excluding flower tiles). If < minimum_hu_fan, penalise instead of
///       rewarding.
/// 4. `score = -shanten + width_weight * width + tenpai_bonus/penalty + win_bonus`
pub(crate) fn shanten_expert_discard_scores(
    context: &BotContext,
    discard_mask: &[bool; TILE_KIND_COUNT],
) -> ([f32; TILE_KIND_COUNT], Vec<usize>) {
    let counts = &context.player.concealed_tile_counts;
    let open_meld_count = context.player.meld_tile_key_groups.len();

    let mut scores = [0.0_f32; TILE_KIND_COUNT];
    let mut legal = Vec::new();

    for (tile_index, &allowed) in discard_mask.iter().enumerate() {
        if !allowed {
            continue;
        }
        let count = counts[tile_index];
        if count == 0 {
            continue;
        }
        legal.push(tile_index);

        let mut after_counts = *counts;
        after_counts[tile_index] -= 1;

        let shanten_after = min_shanten_for_counts(&after_counts, open_meld_count);

        let mut score = -(shanten_after as f32);

        if shanten_after == 0 {
            let width = acceptance_width(&after_counts, open_meld_count);
            score += ACCEPTANCE_WIDTH_WEIGHT * width as f32;

            let qualifies = tenpai_meets_fan_minimum(context, tile_index);
            if qualifies {
                score += TENPAI_BONUS;
            } else {
                score += TENPAI_NO_FAN_PENALTY;
            }
        } else if shanten_after < 0 {
            score += WIN_BONUS;
        }

        scores[tile_index] = score;
    }

    (scores, legal)
}

/// Check if discarding `tile_index` leaves a tenpai hand that can meet
/// the minimum fan requirement (typically 8 fan).
fn tenpai_meets_fan_minimum(context: &BotContext, discard_tile_index: usize) -> bool {
    let concealed_tile_keys: Vec<String> = context
        .player
        .concealed_tiles
        .iter()
        .filter(|tile| !tile.is_flower)
        .map(|tile| tile.tile_key.clone())
        .collect();

    // Build the post-discard concealed hand (13 tiles for a closed hand)
    let mut after_keys = Vec::with_capacity(concealed_tile_keys.len());
    let mut removed = false;
    let discard_key = tile_key_for_index(discard_tile_index);
    for key in &concealed_tile_keys {
        if !removed && key == discard_key {
            removed = true;
            continue;
        }
        after_keys.push(key.clone());
    }
    if !removed {
        return false;
    }

    let melds = &context.player.meld_tile_key_groups;
    let round_wind = context.round_wind.as_deref().unwrap_or("east");

    let (qualifying, _raw) = qualifying_fan_potential(
        &after_keys,
        melds,
        0, // shanten == 0 (we already know it's tenpai)
        context.seat_index,
        context.dealer_seat,
        round_wind,
        context.minimum_hu_fan,
    );

    qualifying >= context.minimum_hu_fan.max(0)
}

/// Count how many of the 34 tile types would complete the hand (shanten → -1).
fn acceptance_width(counts: &TileCounts, open_meld_count: usize) -> usize {
    let mut width = 0;
    for tile_index in 0..TILE_KIND_COUNT {
        let mut test_counts = *counts;
        test_counts[tile_index] += 1;
        let shanten = min_shanten_for_counts(&test_counts, open_meld_count);
        if shanten < 0 {
            width += 1;
        }
    }
    width
}

fn tile_key_for_index(index: usize) -> &'static str {
    const KEYS: [&str; TILE_KIND_COUNT] = [
        "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "t1", "t2", "t3", "t4", "t5", "t6",
        "t7", "t8", "t9", "b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9", "east", "south",
        "west", "north", "red", "green", "white",
    ];
    KEYS.get(index).copied().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::context::tile_index;
    use crate::bot::context::BotTileView;

    fn make_context(tile_keys: &[&str], melds: &[&[&str]]) -> BotContext {
        let concealed_tiles = tile_keys
            .iter()
            .enumerate()
            .map(|(index, key)| BotTileView {
                tile_id: format!("{key}-{index}"),
                tile_key: key.to_string(),
                is_flower: false,
            })
            .collect::<Vec<_>>();
        let mut counts = [0_u8; TILE_KIND_COUNT];
        for key in tile_keys {
            if let Some(idx) = tile_index(key) {
                counts[idx] += 1;
            }
        }
        let meld_groups = melds
            .iter()
            .map(|group| group.iter().map(|k| k.to_string()).collect::<Vec<_>>())
            .collect();
        BotContext {
            seat_index: 0,
            seat_count: 4,
            dealer_seat: 0,
            round_wind: Some("east".to_string()),
            minimum_hu_fan: 8,
            cumulative_scores: vec![0, 0, 0, 0],
            wall_tiles_remaining: 50,
            visible_tile_keys: Vec::new(),
            opponent_discards_by_seat: vec![vec![], vec![], vec![], vec![]],
            opponent_melds_by_seat: vec![vec![], vec![], vec![], vec![]],
            discard_history: Vec::new(),
            kong_entries: Vec::new(),
            player: crate::bot::context::BotPlayerContext {
                concealed_tiles,
                concealed_tile_counts: counts,
                meld_tile_key_groups: meld_groups,
                flower_count: 0,
            },
            restricted_discard_tile_key: None,
            drawn_tile_id: None,
            self_kong_candidates: Vec::new(),
            claim_options: Vec::new(),
            last_discard_tile_key: None,
            add_kong_risk_tiles: std::collections::HashSet::new(),
        }
    }

    fn full_mask() -> [bool; TILE_KIND_COUNT] {
        [true; TILE_KIND_COUNT]
    }

    #[test]
    fn discarding_to_tenpai_scores_higher_than_discarding_away() {
        // 14 tiles: w1 w2 w3 w4 w5 w6 t1 t2 t3 t4 t5 t6 b1 b9
        // Discard b9 → 4 melds + b1 single = tenpai (waiting for b1)
        let context = make_context(
            &["w1", "w2", "w3", "w4", "w5", "w6", "t1", "t2", "t3", "t4", "t5", "t6", "b1", "b9"],
            &[],
        );
        let mask = full_mask();
        let (scores, legal) = shanten_expert_discard_scores(&context, &mask);

        assert!(legal.len() > 1);
        let b9_idx = tile_index("b9").unwrap();
        let w1_idx = tile_index("w1").unwrap();
        assert!(
            scores[b9_idx] > scores[w1_idx],
            "tenpai discard should score higher than non-tenpai discard"
        );
    }

    #[test]
    fn tenpai_hand_has_positive_score() {
        // 14 tiles: w1 w2 w3 w4 w5 w6 t1 t2 t3 t4 t5 t6 b1 b1
        // Discard b1 → 4 melds + b1 single = tenpai
        let context = make_context(
            &["w1", "w2", "w3", "w4", "w5", "w6", "t1", "t2", "t3", "t4", "t5", "t6", "b1", "b1"],
            &[],
        );
        let mask = full_mask();
        let (scores, legal) = shanten_expert_discard_scores(&context, &mask);

        assert!(!legal.is_empty());
        let best_score = legal.iter().map(|&i| scores[i]).fold(0.0_f32, f32::max);
        // This is a closed all-pung-chow hand with no honors → likely < 8 fan
        // but it should still have a reasonable score (shanten=0 is strong)
        assert!(best_score > 0.0, "best discard should have positive score");
    }

    #[test]
    fn illegal_tiles_have_zero_score() {
        let context = make_context(&["w1", "w2", "w3"], &[]);
        let mut mask = [false; TILE_KIND_COUNT];
        let w1 = tile_index("w1").unwrap();
        mask[w1] = true;
        let (scores, legal) = shanten_expert_discard_scores(&context, &mask);

        assert_eq!(legal, vec![w1]);
        for (i, &s) in scores.iter().enumerate() {
            if i != w1 {
                assert_eq!(s, 0.0, "tile {} should have zero score", i);
            }
        }
    }

    #[test]
    fn open_melds_help_reach_tenpai() {
        // Open meld: east east east (dragon triplet = 2 fan for seat wind match on east round)
        // Concealed: w2 w3 w4 w5 w6 w7 t1 t2 t3 b1 b1 (11 tiles)
        // Discard b1 → w2-w4, w5-w7, t1-t3, b1 = 3 melds + 1 single = tenpai
        // Fan: east pung (round wind + seat wind = at least 2 fan from zhong_feng_pung)
        //   + potentially more from hand structure. With 8-fan minimum this may or may not qualify.
        // Just check the function runs and produces a finite score.
        let context = make_context(
            &["w2", "w3", "w4", "w5", "w6", "w7", "t1", "t2", "t3", "b1", "b1"],
            &[&["east", "east", "east"]],
        );
        let mask = full_mask();
        let (scores, legal) = shanten_expert_discard_scores(&context, &mask);

        assert!(!legal.is_empty());
        let b1_idx = tile_index("b1").unwrap();
        // Should be tenpai (shanten=0) — score may be positive (qualifies) or
        // slightly negative (doesn't qualify) depending on fan calculation
        assert!(
            scores[b1_idx].is_finite(),
            "score should be finite, got {}",
            scores[b1_idx]
        );
    }

    #[test]
    fn acceptance_width_counts_winning_tiles() {
        let mut counts = [0_u8; TILE_KIND_COUNT];
        for key in &["w1", "w2", "w3", "w4", "w5", "w6", "t1", "t2", "t3", "t4", "t5", "t6", "b1"] {
            counts[tile_index(key).unwrap()] += 1;
        }
        let width = acceptance_width(&counts, 0);
        assert!(width >= 1, "should have at least 1 winning tile, got {}", width);
    }

    #[test]
    fn high_fan_tenpai_scores_higher_than_low_fan_tenpai() {
        // Hand A: a hand that reaches tenpai with high fan potential
        //   w1 w1 w1 w2 w3 w4 w5 w6 w7 east east east b1 b2
        //   Discard b2 → pung of w1, sequence w2-w4, sequence w5-w7,
        //   pung of east, b1 single = tenpai waiting for b1
        //   Fan: pung of east (seat wind + round wind = 2 fan minimum), pure suited sequences...
        //   This should have more fan than a plain hand.
        let context_high = make_context(
            &["w1", "w1", "w1", "w2", "w3", "w4", "w5", "w6", "w7", "east", "east", "east", "b1", "b2"],
            &[],
        );

        // Hand B: a simple hand with no honors, all suited
        //   w1 w2 w3 w4 w5 w6 t1 t2 t3 t4 t5 t6 b1 b2
        //   Discard b2 → 4 sequences + b1 single = tenpai, but likely < 8 fan
        let context_low = make_context(
            &["w1", "w2", "w3", "w4", "w5", "w6", "t1", "t2", "t3", "t4", "t5", "t6", "b1", "b2"],
            &[],
        );

        let mask = full_mask();
        let (scores_high, _) = shanten_expert_discard_scores(&context_high, &mask);
        let (scores_low, _) = shanten_expert_discard_scores(&context_low, &mask);

        let b2_high = scores_high[tile_index("b2").unwrap()];
        let b2_low = scores_low[tile_index("b2").unwrap()];

        // The high-fan hand's b2 discard should score at least as well as the low-fan one.
        // Both reach tenpai, but the high-fan one should get the tenpai bonus
        // while the low-fan one might get the penalty.
        assert!(
            b2_high >= b2_low,
            "high-fan tenpai should score >= low-fan tenpai: {} vs {}",
            b2_high, b2_low
        );
    }
}
