# Design: AWR Training Quality Boost — Phase 1 (Trajectory + Advantage + Weight)

**Date:** 2026-06-16
**Status:** Approved
**Scope:** Trajectory opponent diversity, advantage normalization, AWR weight strategy tuning. Builds on top of the PPO→AWR refactor (already completed).

## Motivation

The AWR pipeline is architecturally correct but the training signal is weak:
1. All 4 seats use identical learner policy for self-play → narrow data distribution
2. Raw MC returns have high variance across matches → exp weighting unstable
3. Temperature/clip defaults not tuned for the advantage scale

## Design

### 1. Trajectory Quality — Opponent Diversity

**`opponent_pool.json`** gains a `rollout_opponents` list:

```json
{
  "schema_version": 3,
  "learner": {
    "id": "learner",
    "model_path": "backend/assets/sft/sft.onnx",
    "sample_actions": true,
    "temperature": 1.0
  },
  "rollout_opponents": [
    {"id": "sft_cold",   "model_path": "backend/assets/sft/sft.onnx", "sample_actions": true, "temperature": 0.5, "weight": 1},
    {"id": "sft_warm",   "model_path": "backend/assets/sft/sft.onnx", "sample_actions": true, "temperature": 1.0, "weight": 2},
    {"id": "sft_hot",    "model_path": "backend/assets/sft/sft.onnx", "sample_actions": true, "temperature": 2.0, "weight": 1}
  ]
}
```

**`league_config.py`** — `build_trajectory_configs` randomly samples 3 opponents from `rollout_opponents` (weighted) per match:

```python
def build_trajectory_configs(pool, matches, seed, max_actions):
    learner = clean_policy(pool["learner"])
    learner["sample_actions"] = True
    opponents_pool = pool.get("rollout_opponents", [])
    
    configs = []
    for m in range(matches):
        rng = random.Random(seed + m * 1000 + 1)
        chosen = weighted_sample(opponents_pool, 3, rng)
        configs.append({
            "matches": 1,
            "seed": seed + m * 1000,
            "max_actions_per_match": max_actions,
            "report_trajectories": True,
            "subjects": [{**learner, "display_name": "Learner"}],
            "opponents": [clean_policy(o) for o in chosen],
        })
    return configs
```

**`train_awr_model.ps1`** — `$TrajectoryMatches` default 20→100.

### 2. Advantage Estimation — Normalization

**`awr_dataset.py`** — new function `compute_normalized_advantages`:

```python
def compute_normalized_advantages(
    rows, returns, values, mode="per_match"
) -> list[float]:
    """
    mode:
      "none"       — raw advantage = return - value
      "per_match"  — z-score normalize within each (match_id) group
      "per_seat"   — z-score normalize within each (match_id, seat_index) group
      "batch"      — z-score normalize across entire dataset
    """
```

- Groups rows by match or (match, seat)
- Computes advantage = return - value
- Within each group: adv_norm = (adv - mean) / (std + 1e-8)
- Clips to [-5, 5] to prevent outliers

**`train_awr.py`** — new CLI arg `--adv-norm` (default `per_match`). Passed to dataset.

### 3. AWR Weight Strategy — Parameter Tuning

All changes are in `train_awr.py` defaults:

| Parameter | Old Default | New Default | Rationale |
|-----------|------------|-------------|-----------|
| `--temperature` | 1.0 | 0.5 | With normalized advantages (std≈1), exp(1.0/0.5)=7.4 gives strong but bounded weighting |
| `--weight-clip` | 10.0 | 20.0 | exp(3.0/0.5)≈403, but normalized adv rarely >3; 20 is conservative |
| `--policy-filter` | `positive` | `positive` | Unchanged — AWAC style, only reinforce good actions |
| `--adv-norm` | (none) | `per_match` | New parameter, enabled by default |

### Files Changed

| File | Change |
|------|--------|
| `opponent_pool.json` | Add `rollout_opponents` list |
| `league_config.py` | Per-match random opponent sampling from pool |
| `awr_dataset.py` | Add `compute_normalized_advantages` |
| `train_awr.py` | Add `--adv-norm` param, update defaults |
| `train_awr_model.ps1` | Bump `$TrajectoryMatches` default to 100 |

### Testing

- Unit test: `compute_normalized_advantages` produces mean≈0, std≈1 per group
- Unit test: weighted sampling from opponent pool respects weights
- Integration: run 1-match trajectory generation with diverse opponents, verify 4 different policy_ids in output

## Risks

- **Opponent model not found**: PPO model path may not exist. Fallback: use only SFT variants with different temperatures.
- **Advantage normalization across matches with different scores**: A match where learner wins big vs loses big will have different advantage distributions. Per-match normalization helps, but per-seat may be too aggressive (single seat has only ~300 rows). Start with `per_match`.
