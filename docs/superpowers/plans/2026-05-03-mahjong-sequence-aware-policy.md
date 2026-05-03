# Mahjong Sequence-Aware Policy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor the Mahjong policy model to consume explicit discard history sequence input and decouple policy/value/risk learning paths.

**Architecture:** Add `discard_sequence` as a third model input across Python dataset, SFT, PPO, ONNX export, and Rust inference. Replace the old shared 256-dim bottleneck with separate policy/value/risk trunks and task-specific MLP heads.

**Tech Stack:** Python, PyTorch, pytest, Rust, ort, cargo test.

---

### Task 1: Python Tests Define New Model Contract

**Files:**
- Modify: `backend/bot_trainer/v2/test_model.py`
- Modify: `backend/bot_trainer/v2/test_dataset.py`
- Modify: `backend/bot_trainer/v2/test_rl_dataset.py`

- [ ] Add model tests that instantiate `ModelConfig(tile_plane_count=10, scalar_feature_count=12, discard_sequence_length=32, discard_event_feature_count=40)`, call `model(tile_planes, scalar_features, discard_sequence)`, assert all output shapes including `fan_logits`, assert there is no `torch.nn.Dropout`, and assert no `load_compatible_state_dict` import remains.
- [ ] Add dataset tests that `encode_row()` returns `discard_sequence` shape `(32, 40)`, keeps the latest discard marker at column `39`, and encodes relative seat one-hot columns `34:38`.
- [ ] Add RL dataset tests that fixture rows include `discard_sequence` and `ArenaTrajectoryDataset[0]["discard_sequence"].shape == (32, 40)`.
- [ ] Run `python -m pytest backend/bot_trainer/v2/test_model.py backend/bot_trainer/v2/test_dataset.py backend/bot_trainer/v2/test_rl_dataset.py`; expected result before implementation is failure on missing config fields, missing input, or missing encoded key.

### Task 2: Implement Python Dataset and Model Schema

**Files:**
- Modify: `backend/bot_trainer/v2/dataset.py`
- Modify: `backend/bot_trainer/v2/model.py`

- [ ] Add constants `DISCARD_SEQUENCE_LENGTH = 32` and `DISCARD_EVENT_FEATURE_COUNT = 40`.
- [ ] Add `discard_sequence` to disk cache specs and bump `DISK_CACHE_VERSION`.
- [ ] Implement `encode_discard_sequence(context, tile_to_index)` using the right-aligned last 32 `discard_history` events.
- [ ] Apply suit augmentation to the first 34 columns of `discard_sequence`.
- [ ] Replace the old model with `SuitFusionTileEncoder`, `DiscardSequenceEncoder`, `HeadMLP`, and the new `MahjongPolicyNetV2` three-input forward signature.
- [ ] Remove `load_compatible_state_dict` from `model.py`.
- [ ] Run the Python tests from Task 1; expected result is pass or failures only in training/RL callers still using the old signature.

### Task 3: Update SFT and PPO Training Callers

**Files:**
- Modify: `backend/bot_trainer/v2/train.py`
- Modify: `backend/bot_trainer/v2/rl_dataset.py`
- Modify: `backend/bot_trainer/v2/rl_train.py`

- [ ] Update `forward_model()` in SFT and RL to pass `discard_sequence`.
- [ ] Update `model_config_from_args()` and checkpoint loading to include sequence dimensions.
- [ ] Replace compatible checkpoint loading with strict `model.load_state_dict(state)`.
- [ ] Add `loss_weights_for_epoch()` and warmup CLI args, with defaults `value=0.75`, `risk=1.0`, `fan=0.5`, start weights `0.25`, warmup `4`.
- [ ] Run Python tests again; expected result is pass for model/dataset/RL tests.

### Task 4: Update ONNX Export and Baseline Guard

**Files:**
- Modify: `backend/bot_trainer/v2/export_onnx.py`
- Modify: `backend/bot_trainer/v2/baseline_guard.py`

- [ ] Add `discard_sequence` dummy tensor, input name, dynamic axes, smoke input, and manifest config.
- [ ] Validate ONNX input shape for `discard_sequence` when onnxruntime is available.
- [ ] Run `python -m pytest backend/bot_trainer/v2/test_model.py backend/bot_trainer/v2/test_dataset.py backend/bot_trainer/v2/test_rl_dataset.py`; expected result is pass.

### Task 5: Update Rust Feature Encoding and ORT Inference

**Files:**
- Modify: `backend/src/bot/features.rs`
- Modify: `backend/src/bot/neural.rs`
- Modify: `backend/src/bot/arena.rs`

- [ ] Add `discard_sequence` to `BotFeaturesV2`, constants for length and event feature count, and encode right-aligned discard history events.
- [ ] Add third ORT tensor input named `discard_sequence`.
- [ ] Add `discard_sequence` to `ArenaTrajectoryRow` and trajectory JSON generation.
- [ ] Update Rust tests and fixtures from `vec![0.0; 340]`/`scalar_features` only to include `vec![0.0; 32 * 40]`.
- [ ] Run `cargo test -p backend bot::features bot::neural bot::arena`; expected result is pass, except local stale ONNX smoke may be skipped by schema guard.

### Task 6: Final Verification and Cleanup

**Files:**
- Review: `backend/bot_trainer/v2/model.py`
- Review: `backend/bot_trainer/v2/train.py`
- Review: `backend/bot_trainer/v2/rl_train.py`
- Review: `backend/src/bot/features.rs`
- Review: `backend/src/bot/neural.rs`

- [ ] Run targeted Python tests.
- [ ] Run targeted Rust tests.
- [ ] Run `git diff --stat` and verify `arena_policy_pool.json` remains untouched by this task.
- [ ] Report exact commands, exit status, and any unverified areas.
