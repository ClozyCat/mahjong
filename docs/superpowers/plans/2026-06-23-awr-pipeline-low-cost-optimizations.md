# AWR Pipeline Low-Cost Optimizations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve AWR rollout diversity, promotion signal correctness, and training diagnostics with low-risk Python-only changes.

**Architecture:** Keep the existing AWR pipeline shape. Fix paired evaluation lookup at the report/gate boundary, split trajectory generation into opponent-resampled chunks, and add aggregate AWR advantage/weight diagnostics to training logs and checkpoints.

**Tech Stack:** Python 3, pytest, existing `backend/bot_trainer/v2` scripts.

---

### Task 1: Paired Evaluation Direction

**Files:**
- Modify: `backend/bot_trainer/v2/candidate_gate.py`
- Test: `backend/bot_trainer/v2/test_awr_dataset.py`

- [ ] Add a test where `paired_subjects` contains only `candidate__vs__baseline`.
- [ ] Verify the test fails because `candidate_gate.py` reports `paired_subjects_missing`.
- [ ] Add reverse-key lookup that negates paired deltas and confidence fields into baseline-to-candidate orientation.
- [ ] Verify the targeted test passes.

### Task 2: Trajectory Opponent Resampling

**Files:**
- Modify: `backend/bot_trainer/v2/league_config.py`
- Test: `backend/bot_trainer/v2/test_league_config.py`

- [ ] Add a test expecting multiple trajectory configs when match count exceeds a chunk size.
- [ ] Verify the test fails with the current single-config behavior.
- [ ] Add `--trajectory-chunk-matches` and make `build_trajectory_configs()` split matches across chunks, re-sampling opponents per chunk.
- [ ] Verify trajectory config tests pass.

### Task 3: AWR Advantage and Weight Diagnostics

**Files:**
- Modify: `backend/bot_trainer/v2/train_awr.py`
- Test: `backend/bot_trainer/v2/test_awr_dataset.py`

- [ ] Add a test for a small diagnostics accumulator using known weights and advantages.
- [ ] Verify the test fails because the helper does not exist.
- [ ] Add helper functions for `adv_mean`, `adv_std`, `adv_pos_rate`, `weight_mean`, `weight_max`, and `active_weight_rate`.
- [ ] Wire diagnostics into epoch print output and checkpoint `awr_metrics`.
- [ ] Verify all Python trainer tests pass.

### Task 4: Matrix Gate Strictness

**Files:**
- Modify: `backend/bot_trainer/v2/candidate_gate.py`
- Test: `backend/bot_trainer/v2/test_awr_dataset.py`

- [ ] Add tests showing matrix evaluation rejects high latency and paired CI crossing zero even when weighted score/win/deal-in pass.
- [ ] Verify the tests fail with current weighted-only matrix acceptance.
- [ ] Include latency, paired CI, and per-opponent failures in matrix acceptance.
- [ ] Verify targeted gate tests pass.

### Task 5: Match-Level Validation Metrics

**Files:**
- Modify: `backend/bot_trainer/v2/awr_dataset.py`
- Modify: `backend/bot_trainer/v2/train_value.py`
- Modify: `backend/bot_trainer/v2/train_awr.py`
- Test: `backend/bot_trainer/v2/test_awr_dataset.py`

- [ ] Add tests for deterministic match-id train/validation splitting.
- [ ] Verify the tests fail because no splitter exists.
- [ ] Add a reusable `split_rows_by_match_id()` helper and dataset constructor support for in-memory rows.
- [ ] Report validation value MSE/EV in value pretraining and AWR.
- [ ] Prefer validation EV for AWR best-checkpoint selection when validation rows exist.

### Final Verification

- [ ] Run `python -m pytest backend/bot_trainer/v2 -q`.
- [ ] Inspect `git diff --stat` and ensure changes are limited to AWR Python scripts, tests, and this plan.
