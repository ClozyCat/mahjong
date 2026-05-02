# Mahjong RL Stability Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. This repository rule allows the lightweight path for scoped changes; do not create a separate worktree unless the user asks for external isolation.

**Goal:** Stop PPO self-play from degrading the Guobiao Mahjong BOT and add the evaluation, reward, and diagnostics controls needed for safer promotion.

**Architecture:** Rust remains the source of truth for legal actions, arena rollout, policy telemetry, reward snapshots, and trajectory emission. Python owns league config generation, PPO math, candidate selection, baseline manifest checks, and arena summaries. The ONNX inference contract stays unchanged: `tile_planes`, `scalar_features` inputs and the existing six outputs.

**Tech Stack:** Rust backend (`backend/src/bot`, `backend/src/rules/standard`), Python trainer (`torch`, `onnxruntime`, JSONL), PowerShell/Bash wrappers, existing arena JSONL reports.

---

## Root Cause Summary

1. The default PPO checkpoint path has been overwritten by an RL checkpoint while the default baseline ONNX still points at SFT, so rollout policy, old policy stats, and training initialization can disagree.
2. Neural discard rollout uses risk-adjusted logits, but trajectory `old_log_prob` and Python PPO currently use raw discard logits.
3. `league_config.py` overrides every neural policy in the pool with the rollout model, turning frozen SFT opponents into learner clones.
4. Iterative training advances to the next iteration with the just-trained candidate even when candidate gate rejects it.
5. Evaluation is too narrow for promotion: fixed baseline/candidate seats and no historical model pool matrix.
6. Guobiao-specific reward shaping currently over-emphasizes shanten reduction relative to long-horizon terminal score and 8-fan quality.

## Implementation Status

Implemented on 2026-05-02:

- Baseline checkpoint/ONNX guard with `training_source` metadata and ONNX sidecar manifests.
- Learner-only rollout override so frozen SFT opponents keep their configured ONNX paths.
- Accepted-or-better rollout promotion logic in PowerShell and Bash training wrappers.
- Risk-adjusted discard logits aligned across Rust rollout selection, Rust trajectory `old_log_prob`, and Python PPO log-prob/entropy/KL.
- Cyclic seat rotation for direct candidate evaluation and historical matrix pool entries for `heuristic`, `sft`, `ppo1`, `ppo2`, and `ppo3`.
- Epoch-level candidate export/evaluation/selection as the default wrapper flow.
- Expanded candidate gate checks for claim-rate drift and heuristic agreement collapse.
- Conservative PPO defaults: `gamma=0.995`, `epochs=1`, `lr=0.000003`, `kl_coef=0.01`, `target_kl=0.03`.
- Weakened shanten/fan shaping and legal-hu safety margin.
- PPO and trajectory diagnostics for action-head distribution, reward breakdown, approximate KL, clip fraction, entropy, and value explained variance.

Deferred follow-up:

- Arena telemetry for legal-hu pass rate, average fan count, deal-in stage, dangerous discard class, and meld-after-deal-in attribution.
- Full 1000+ match multi-seed promotion run; scripts now default to promotion-grade `EvalMatches=1000`, but an actual run is intentionally left as an explicit expensive operation.

Latest verification:

- `python -m pytest backend/bot_trainer/v2 -q` -> 39 passed.
- `cargo test --manifest-path backend/Cargo.toml bot:: -- --nocapture` -> 88 passed.
- `cargo test --manifest-path backend/Cargo.toml rules::standard::automation -- --nocapture` -> 17 passed.
- `.\\backend\\bot_trainer\\v2\\train_rl_model.ps1 -OutputDir backend/bot_trainer/v2/rl_runs/stability_smoke2 -Iterations 1 -IterationMatches 8 -EvalMatches 8 -Epochs 1 -BatchSize 64 -Device cpu -SkipTests -TrajectoryProgressEvery 0` -> smoke pipeline completed; this is not a promotion run.

## Phase 1: Stop Training Drift

**Files:**
- Modify: `backend/bot_trainer/v2/league_config.py`
- Modify: `backend/bot_trainer/v2/train_rl_model.ps1`
- Modify: `backend/bot_trainer/v2/train_rl_model.sh`
- Modify: `backend/bot_trainer/v2/rl_train.py`
- Modify: `backend/bot_trainer/v2/rl_dataset.py`
- Modify: `backend/src/bot/policy.rs`
- Modify: `backend/src/bot/arena.rs`
- Test: `backend/bot_trainer/v2/test_rl_dataset.py`
- Test: `backend/src/bot/arena.rs`

### Task 1: Baseline Manifest And Pairing

- [x] Add a lightweight `training_source` field to checkpoint payloads written by supervised training and PPO.
- [x] Add a Python manifest helper that can inspect checkpoint metadata and ONNX input shapes.
- [x] Make RL wrappers reject a baseline checkpoint whose `training_source` is `rl` unless the user explicitly passes a continuation flag.
- [x] Make final RL artifacts write to the run output directory only; do not overwrite `backend/bot_trainer/v2/checkpoints/best.pt`.

**Verification:**

```powershell
python -m pytest backend/bot_trainer/v2/test_rl_dataset.py -q
```

Expected: tests cover SFT/RL checkpoint classification and fail on accidental RL-as-SFT baseline use.

### Task 2: Freeze Opponent Pool

- [x] Change rollout model override to update only `pool["learner"]`.
- [x] Add a regression test proving `sft_default` remains `backend/assets/history_models/sft.onnx` after `--rollout-onnx`.

**Verification:**

```powershell
python -m pytest backend/bot_trainer/v2/test_rl_dataset.py::test_rollout_override_keeps_neural_opponents_frozen -q
```

Expected: generated configs contain one learner using the rollout ONNX and frozen neural opponents using their configured model paths.

### Task 3: Use Accepted Or Best Rollout Only

- [x] Track `current_*` separately from the latest candidate.
- [x] Advance `current_*` only when a candidate passes gate or improves the score margin over the current best.
- [x] If no candidate improves, the next iteration continues from the previous best.

**Verification:**

```powershell
python -m pytest backend/bot_trainer/v2/test_rl_dataset.py::test_rollout_state_keeps_previous_best_when_candidate_regresses -q
```

Expected: selection helper returns the previous rollout paths for regressed candidates.

### Task 4: Align Risk-Adjusted Discard Log-Prob

- [x] Expose a shared Rust helper for risk-adjusted discard logits.
- [x] Use that helper both for sampling and for arena trajectory log-prob when `action_head == "discard"`.
- [x] Add equivalent Python risk-adjustment for PPO log-prob, entropy, and KL when action head is discard.
- [x] Keep claim, self-kong, and hu heads unchanged.

**Verification:**

```powershell
cargo test --manifest-path backend/Cargo.toml bot::arena::tests::trajectory_log_prob_uses_risk_adjusted_discard_logits -- --nocapture
python -m pytest backend/bot_trainer/v2/test_rl_dataset.py::test_discard_log_probs_use_risk_adjusted_logits -q
```

Expected: Rust and Python both assign lower probability to high-risk discards when risk logits are high.

## Phase 2: Candidate Evaluation Gate

**Files:**
- Modify: `backend/bot_trainer/v2/league_config.py`
- Modify: `backend/bot_trainer/v2/candidate_gate.py`
- Modify: `backend/bot_trainer/v2/candidate_selector.py`
- Modify: `backend/bot_trainer/v2/arena_summary.py`
- Modify: `backend/bot_trainer/v2/arena_policy_pool.json`
- Modify: `backend/bot_trainer/v2/train_rl_model.ps1`
- Modify: `backend/bot_trainer/v2/train_rl_model.sh`
- Test: `backend/bot_trainer/v2/test_rl_dataset.py`

### Task 5: Cyclic Evaluation And Historical Pool

- [x] Generate candidate evaluation configs with cyclic seat rotation instead of fixed seat positions.
- [x] Include `heuristic`, `sft`, and historical PPO models in `arena_policy_pool.json`.
- [x] Keep promotion recommendation at `1000+` matches, with smoke tests allowed at smaller match counts.

**Verification:**

```powershell
python -m pytest backend/bot_trainer/v2/test_rl_dataset.py::test_eval_config_uses_cyclic_rotation -q
```

Expected: eval config has `seat_rotation = "cyclic"` and includes baseline/candidate policies only for direct gate checks.

### Task 6: Real Per-Epoch Candidate Selection

- [x] Use `CandidateSelectionMode=epoch` as the default.
- [x] Export and evaluate each `epoch_*.pt` checkpoint when epoch mode is enabled.
- [x] Write `candidate_selection.json` and promote the best candidate by accepted status, score margin, win margin, tenpai margin, deal-in margin, and latency margin.

**Verification:**

```powershell
python -m pytest backend/bot_trainer/v2/test_rl_dataset.py::test_candidate_selector_prefers_accepted_candidate -q
```

Expected: selector chooses accepted candidates first and chooses least-regressed candidates only for local inspection.

### Task 7: Gate More Than Average Score

- [x] Keep existing average score, win-rate, deal-in, tenpai, and latency checks.
- [x] Add claim-rate sanity bounds and same-as-heuristic minimum for promotion runs when summary contains those metrics.
- [ ] Add warning fields for legal-hu pass rate and average fan count once telemetry is available.

**Verification:**

```powershell
python -m pytest backend/bot_trainer/v2/test_rl_dataset.py::test_candidate_gate_rejects_excessive_claim_rate -q
```

Expected: a model with score improvement but extreme claim-rate drift is rejected.

## Phase 3: Guobiao Reward And PPO Constraints

**Files:**
- Modify: `backend/src/bot/reward.rs`
- Modify: `backend/bot_trainer/v2/rl_train.py`
- Modify: `backend/bot_trainer/v2/train_rl_model.ps1`
- Modify: `backend/bot_trainer/v2/train_rl_model.sh`
- Test: `backend/src/bot/reward.rs`
- Test: `backend/bot_trainer/v2/test_rl_dataset.py`

### Task 8: Rebalance Reward And Defaults

- [x] Reduce shanten shaping and tenpai bonus so step reward cannot dominate terminal score.
- [x] Change wrapper defaults to `gamma=0.995`, `epochs=1`, `lr=0.000003`, and `kl_coef=0.01`.
- [x] Add target KL early stop to `rl_train.py`.

**Verification:**

```powershell
cargo test --manifest-path backend/Cargo.toml bot::reward -- --nocapture
python -m pytest backend/bot_trainer/v2/test_rl_dataset.py::test_early_stop_triggers_when_approx_kl_exceeds_target -q
```

Expected: reward unit tests pass and PPO stops early when approximate KL exceeds the configured target.

### Task 9: Hu Safety Rule

- [x] Preserve the neural hu head, but require a strong margin to pass on a legal Guobiao hu.
- [ ] Track hu-pass telemetry so future gate checks can reject high legal-hu pass rate.

**Verification:**

```powershell
cargo test --manifest-path backend/Cargo.toml bot::policy::tests::neural_hu_requires_strong_pass_margin -- --nocapture
```

Expected: a small pass-over-hu logit edge no longer declines a legal hu.

## Phase 4: Diagnostics

**Files:**
- Modify: `backend/bot_trainer/v2/rl_train.py`
- Modify: `backend/bot_trainer/v2/rl_dataset.py`
- Modify: `backend/bot_trainer/v2/arena_summary.py`
- Modify: `backend/src/bot/arena.rs`
- Test: `backend/bot_trainer/v2/test_rl_dataset.py`

### Task 10: PPO And Reward Diagnostics

- [x] Log learner row count and action-head distribution before training.
- [x] Log approximate KL, clip fraction, entropy, entropy coefficient, value loss, and value explained variance each epoch.
- [x] Log terminal reward mean, step reward mean, terminal/step absolute ratio, shanten improvement count, and fan-potential improvement count.

**Verification:**

```powershell
python -m pytest backend/bot_trainer/v2/test_rl_dataset.py::test_trajectory_diagnostics_reports_reward_breakdown -q
```

Expected: diagnostics helper returns stable numeric fields for PPO logs and JSON metrics.

## Final Verification

Run these before any promotion claim:

```powershell
python -m pytest backend/bot_trainer/v2 -q
cargo test --manifest-path backend/Cargo.toml bot::arena bot::policy bot::reward bot::neural rules::standard::automation -- --nocapture
.\backend\bot_trainer\v2\train_rl_model.ps1 -OutputDir backend/bot_trainer/v2/rl_runs/stability_smoke -Iterations 1 -IterationMatches 8 -EvalMatches 8 -Epochs 1 -BatchSize 64 -Device cpu -SkipTests
```

Expected:

- Python trainer tests pass.
- Rust arena/policy/reward/neural targeted tests pass.
- Smoke run writes `iteration_history.json`, `candidate_selection.json` when epoch mode is enabled, `candidate_gate.json`, PPO metrics with KL/clip diagnostics, and a candidate ONNX under the run directory.

## Promotion Policy

A model can replace production only when all of these hold on a 1000+ match multi-seed arena run:

- average score delta improves over SFT production baseline
- win rate does not regress
- deal-in rate does not increase by more than 2 percentage points
- first-tenpai turn or final-tenpai rate improves or stays neutral
- claim-rate drift stays within configured sanity bounds
- same-as-heuristic rate does not collapse below the configured floor
- average decision latency remains under 200 ms
- legal-hu pass rate remains near zero once hu-pass telemetry is enabled
