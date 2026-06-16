# Design: PPO → AWR/AWAC Offline Policy Improvement

**Date:** 2026-06-16
**Status:** Approved
**Scope:** Full removal of PPO/RL training pipeline, model architecture simplification, AWR/AWAC-style offline policy improvement, arena batch inference, trajectory format optimization.

## Motivation

PPO training is slow (rollout-dominated, 67% ONNX inference time) and ineffective (policy_loss ≈ -0.0006, clip_fraction=0, value_ev≈0.008). Root causes:
1. Learner rollout is greedy (sample_actions: false) — not on-policy
2. Only 1 learner per 4-player table — 75% data wasted
3. Weak terminal reward credit assignment in high-variance mahjong
4. Critic starts at 0.0 with no pretraining
5. PPO loss diluted by auxiliary terms (opponent_loss * 0.3 >> policy_loss)
6. Single-action ONNX inference with no batching
7. Heavy model architecture (Transformer, large hidden dims)

## Design Decisions

| Decision | Choice |
|----------|--------|
| RL approach | Completely remove PPO/RL code |
| Critic architecture | Light value head on actor trunk (no global critic) |
| Model size | Slim down: channels 64, hidden 512/256, GRU instead of Transformer |
| Arena perf | Batch inference + Arrow/Parquet trajectory format |
| Rollout data | Collect all 4 seats, stochastic sampling enforced |

## Architecture

### New Training Pipeline

```
SFT (train.py)
  → Strong supervised policy checkpoint
  → export_onnx.py → sft.onnx
       │
       ▼
Value Pretrain (train_value.py)  ← NEW
  → Freeze actor trunk, train only value head
  → MSE on MC returns from SFT/arena trajectories
  → Light value head on actor: 256 → 1
       │
       ▼
League Rollout (arena)  ← OPTIMIZED
  → Stochastic opponent pool sampling (sample_actions=true)
  → Collect all 4 seats' data
  → Arrow/Parquet binary format
  → Batch ONNX inference across parallel matches
       │
       ▼
AWR/AWAC Training (train_awr.py)  ← NEW
  → Load SFT checkpoint + pretrained value
  → Compute advantage = return - V(s)
  → Advantage-weighted behavior cloning
  → weight = exp(advantage / temperature), clip to [0, w_max]
  → Only weight samples where advantage > 0 (AWAC style)
  → Combined loss: SFT CE * AWR weight + auxiliary heads
       │
       ▼
League Evaluation + Candidate Gate (RETAINED)
  → candidate_gate.py / candidate_selector.py
  → Safety checks before promotion
```

### Model: LightweightActor (`model.py`)

Replaces: `MahjongPolicyNetV2`, `MahjongActorNetV2`, `MahjongCriticNetV2`, `MahjongActorCriticV2`, `GlobalTileEncoder`

```
Inputs: tile_planes (1,10,34), scalar_features (1,12), discard_sequence (1,32,40)

Shared backbone: Conv1d(10→64, 3) → ReLU → ResidualConvBlock(64)

├─ policy_tile_encoder: SuitFusionTileEncoder(256) with shared backbone
├─ risk_tile_encoder:   SuitFusionTileEncoder(256) with shared backbone
├─ scalar_encoder: Linear(12→96) → ReLU → LayerNorm
└─ discard_sequence_encoder: GRUEncoder (40→96 hidden, output 192)
                              (replaces TransformerDiscardSequenceEncoder)

Combined features: [256 + 96 + 192] = 544 dims (was 928)

├─ policy_trunk: Linear(544→512) → ReLU → Dropout(0.15) → LayerNorm
│                → Linear(512→256) → ReLU → Dropout(0.15) → LayerNorm
├─ risk_trunk:   Linear(544→384) → ReLU → Dropout(0.15) → LayerNorm
│                → Linear(384→256) → ReLU → Dropout(0.15) → LayerNorm

ONNX-Exported Heads (deployment):
├─ discard_head:   256 → 34
├─ claim_head:     256 → 7
├─ self_kong_head: 256 → 3
├─ hu_head:        256 → 2
├─ risk_head:      256 → 34  (raw logits, aggregated in Rust like before)
└─ value_for_risk: 256 → 1   (for risk weight modulation)

Training-Only Heads (not exported to ONNX):
├─ value_head:           256 → 1   (for AWR advantage computation)
├─ opponent_tenpai_head: 256 → 3
└─ opponent_risk_head:   256 → 3×34
```

### Key Hyperparameter Changes

| Parameter | Before | After | Reason |
|-----------|--------|-------|--------|
| channels | 128 | 64 | Smaller model, faster inference |
| policy hidden | 1024→512 | 512→256 | Halved trunk depth |
| scalar dim | 160 | 96 | Proportionally reduced |
| sequence hidden | 128 | 96 | GRU smaller than Transformer |
| sequence output | 256 | 192 | Proportionally reduced |
| combined features | 928 | 544 | 41% reduction |
| risk hidden | 768→512 | 384→256 | Proportionally reduced |
| sequence encoder | Transformer | GRU | Much faster inference |

### Performance Targets

| Metric | Before (smoke) | Target |
|--------|---------------|--------|
| Single-step ONNX inference | ~2.77ms | ~1.2ms |
| Arena rollout 1 match | ~5.4s | ~2s |
| Model param count | ~full | ~40% reduction |
| Training samples per rollout | 329 (learner only) | ~1300 (all 4 seats) |

## Files to Delete

- `backend/bot_trainer/v2/rl_train.py` — PPO training loop
- `backend/bot_trainer/v2/pretrain_critic.py` — Global critic pretrain
- `backend/bot_trainer/v2/bootstrap_actor_critic_checkpoint.py` — Actor-critic bootstrap
- `backend/bot_trainer/v2/checkpoint_architecture_guard.py` — Thin CLI wrapper

## Files to Create

- `backend/bot_trainer/v2/train_value.py` — Value head pretraining
- `backend/bot_trainer/v2/train_awr.py` — AWR/AWAC weighted training
- `backend/bot_trainer/v2/awr_dataset.py` — Trajectory dataset with advantage computation (replaces rl_dataset.py)

## Files to Modify

### Python Side

| File | Changes |
|------|---------|
| `model.py` | Replace all model classes with `LightweightActor`. Add `GRUEncoder`. Simplify `ModelConfig`. |
| `train.py` | Adapt to new model output keys and simplified heads. Remove fan/qualifying_fan heads from SFT loss (only value_for_risk needed). |
| `dataset.py` | Adapt feature encoding if model input shapes change. |
| `export_onnx.py` | Adapt output names to new model. Only export deployment heads. |
| `league_config.py` | All 4 seats as learner subjects; sample_actions=true for all. |
| `rl_dataset.py` → `awr_dataset.py` | Remove GAE, PPO-specific fields. Add Arrow/Parquet write/read. Keep trajectory loading and value computation. |
| `candidate_gate.py` | Adapt to new model output keys if changed. Minor. |
| `candidate_selector.py` | No functional change. |
| `arena_summary.py` | No functional change (reads arena JSON output, not model-specific). |
| `baseline_guard.py` | Adapt ONNX manifest validation to new output names. |
| `test_model.py` | Rewrite for new architecture. |
| `test_rl_dataset.py` → `test_awr_dataset.py` | Adapt to new dataset class. |
| `train_rl_model.ps1` → `train_awr_model.ps1` | Replace PPO orchestration with AWR pipeline. |

### Rust Side

| File | Changes |
|------|---------|
| `arena.rs` | Trajectory format: add Arrow/Parquet serialization alongside JSONL. Remove `opponent_risk_target` and other redundant fields. Collect all 4 seats' data. |
| `policy.rs` | Enforce `sample_actions=true` for rollout. Remove greedy path from rollout config. |
| `neural.rs` | Add batch inference: collect features from N parallel matches, run single ONNX call with batch=N. |
| `features.rs` | Adapt to new model input shapes if changed. |
| `reward.rs` | No change (shaping logic stays). |
| `action_space.rs` | No change. |
| `context.rs` | No change. |
| `shanten.rs` | No change. |

## Testing Strategy

1. Unit tests for new `LightweightActor` — shapes, forward pass, ONNX export roundtrip
2. Unit tests for `train_value.py` — value head converges on synthetic data
3. Unit tests for `train_awr.py` — AWR loss computation, advantage weighting
4. Unit tests for `awr_dataset.py` — Arrow read/write, advantage computation
5. Integration: smoke arena run with new ONNX model — 1 match, verify trajectories
6. Integration: full pipeline — SFT → value pretrain → AWR training → candidate gate
7. Rust: batch inference correctness (same action choices as single inference)
8. Rust: Arrow trajectory serialization roundtrip

## Risks

- **Model capacity reduction**: The slimmed model may lose accuracy on SFT tasks. Mitigation: benchmark SFT accuracy before/after, keep SFT training longer if needed.
- **Value head accuracy**: Light value head (local features only, no global context) may have poor explained variance. Mitigation: if EV < 0.1, add limited global features (score deltas) as scalars.
- **AWR stability**: Advantage-weighted training can degenerate if temperature is wrong. Mitigation: grid search temperature ∈ {0.1, 0.5, 1.0, 2.0} on a small held-out set.
