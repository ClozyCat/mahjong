# Analysis: AWR Fan Awareness — Gap Assessment & Implementation Plan

**Date:** 2026-06-18
**Scope:** Evaluate 4 identified shortcomings, rate feasibility, propose phased plan.

---

## Gap 1: `fan_head` not trained during AWR → degradation

**Current state:** `qualifying_fan_head` has distillation (Phase 2 fix). `fan_head` only trained in SFT, frozen during AWR.

| Factor | Rating |
|--------|--------|
| Difficulty | ★☆☆ (trivial) |
| Code changes | ~8 lines in `train_awr.py`, 1 CLI arg `--fan-value-distill-coef` |
| Retrain needed | No — just re-run AWR with updated script |
| Expected benefit | **Medium** — keeps fan prediction calibrated; prevents the model from forgetting what a high-fan hand looks like |
| Risk | None — MSE distillation from frozen SFT reference is non-invasive |

**Verdict: DO IT.** Same pattern as qualifying_fan_head, copy-paste.

---

## Gap 2: Tile plane encoding — binary presence, no precise counts

**Current state:** 10 tile planes (hand/melds/visible/opponent-discards/opponent-melds/last-discard). Plane 0 (own hand) caps at 4.0, meaning 3×w1 and 1×w1 encode identically unless other planes differ.

**Actual behavior (verified in features.rs):** The encoding writes `min(count, 4)` for each tile. This is NOT truly binary — 1 tile = 1.0, 3 tiles = 3.0, 4 tiles = 4.0. The model CAN distinguish 1× from 3×. The cap at 4 only matters for having >4 copies (impossible in legitimate play). **This gap is overstated.**

| Factor | Rating |
|--------|--------|
| Difficulty | ★★★★ (heavy) |
| Code changes | `features.rs` (plane encoding), `model.py` (`tile_plane_count`), `dataset.py`, retrain SFT |
| Retrain needed | Yes — SFT from scratch |
| Expected benefit | **Low** — current encoding already encodes counts up to 4. The real limitation is not count precision but the absence of *relational encoding* (sequences, pairs, triplets as explicit features). |
| Risk | High — changes input shape, invalidates all checkpoints |

**Verdict: SKIP for now.** The encoding is adequate. The real gap — relational pattern encoding (detecting "this is a sequence" vs "these are 3 unrelated tiles") — is handled by the Conv1d backbone and suit encoder, which already extract local patterns.

---

## Gap 3: No explicit fan type supervision

**Current state:** Model learns fan awareness indirectly via SFT-action imitation + fan_head regression + AWR advantages. Never explicitly learns "these tiles form 混一色" or "this hand has 三色同顺".

| Factor | Rating |
|--------|--------|
| Difficulty | ★★★★★ (prohibitively heavy) |
| Code changes | Multi-label classification head (82 classes for 81 fan types + "none"), SFT dataset labeling, reward shaping per-fan-type |
| Retrain needed | Yes — full SFT retrain with new dataset |
| Expected benefit | **Uncertain** — the model needs to BUILD hands with fans, not identify fans in completed hands. Action-space learning (which discard leads to which fan) is more relevant than classification. The SFT-action data already encodes this implicitly. |
| Risk | Very High — months of effort, may yield no improvement over current indirect learning |

**Verdict: SKIP.** The cost-benefit doesn't justify it. The current approach (action imitation + fan value regression + reward shaping) captures the essential signal. Explicit fan classification is a research project, not a practical improvement.

---

## Gap 4: Reward shaping doesn't differentiate fan quality

**Current state:** `shaping_reward` gives:
- ±0.10 per shanten step
- ±0.50/-0.30 for reaching qualifying/non-qualifying tenpai
- ±0.15 for fan progress scaled by 8

Once the hand reaches 8-fan qualifying status, all additional fan progress is identical in reward terms. A hand heading toward 32 fan gets the same per-step reward as one heading toward 8 fan.

| Factor | Rating |
|--------|--------|
| Difficulty | ★★☆ (easy) |
| Code changes | ~15 lines in `reward.rs` — add tiered fan bonus: 8+ = baseline, 16+ = extra bonus, 24+ = extra bonus |
| Retrain needed | No — arena recompilation only |
| Expected benefit | **Medium-High** — gives model gradient to pursue higher fan hands, which correlates with higher win probability and terminal reward |
| Risk | Low-Medium — could cause the bot to over-pursue fan at the expense of speed (going for a slow 24-fan hand vs fast 8-fan win). Mitigation: make the bonus small (0.02-0.05 per tier) |

**Verdict: DO IT.** Small change, clear directional benefit.

---

## Recommended Plan

### Phase A (implement immediately, ~30 min)

| Task | File | Change |
|------|------|--------|
| A1. `fan_head` distillation | `train_awr.py` | Add `--fan-value-distill-coef` (default 0.05), MSE loss vs SFT `fan_value`, add to total_loss and metrics |
| A2. Tiered fan rewards | `reward.rs` | Add tiered bonus: 8-15 fan → existing, 16-23 fan → +0.03 per step, 24+ fan → +0.05 per step |

### Phase B (defer, needs research)

| Task | Reason for deferral |
|------|-------------------|
| B1. Explicit fan type supervision | Prohibitive cost, uncertain benefit vs indirect learning |
| B2. Tile plane encoding overhaul | Current encoding is already adequate; relational patterns handled by Conv1d |

---

## Final Verdict

The architecture is **adequate** for 国标麻将 with 81 fan types and 8-fan minimum. The core loop (features → policy → reward → AWR improvement) captures fan awareness through multiple channels:

1. **SFT pretraining** teaches initial fan understanding via action imitation + fan value regression
2. **AWR reward shaping** (now strengthened) incentivizes fan progress
3. **Hu decision guard** prevents illegal low-fan and牌
4. **Frozen SFT distillation** (qualifying_fan + proposed fan_value) prevents forgetting

The two Phase A improvements close the remaining gaps without introducing significant risk or cost.
