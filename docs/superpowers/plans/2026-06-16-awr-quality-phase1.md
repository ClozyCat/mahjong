# AWR Phase 1 Quality Boost Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve AWR training signal quality via opponent diversity, advantage normalization, and weight strategy tuning.

**Architecture:** Three independent improvements layered on existing AWR pipeline. `opponent_pool.json` gains `rollout_opponents` for diverse self-play. `awr_dataset.py` gains per-match z-score advantage normalization. `train_awr.py` defaults tuned for normalized advantages.

**Tech Stack:** Python 3.12+, PyTorch, PowerShell

---

### Task 1: Add rollout_opponents to opponent_pool.json

**Files:**
- Modify: `backend/bot_trainer/v2/opponent_pool.json`

- [ ] **Step 1: Update opponent_pool.json with diverse opponent variants**

Replace the content of `opponent_pool.json`:

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
    {
      "id": "sft_cold",
      "model_path": "backend/assets/sft/sft.onnx",
      "sample_actions": true,
      "temperature": 0.5,
      "weight": 1
    },
    {
      "id": "sft_warm",
      "model_path": "backend/assets/sft/sft.onnx",
      "sample_actions": true,
      "temperature": 1.0,
      "weight": 2
    },
    {
      "id": "sft_hot",
      "model_path": "backend/assets/sft/sft.onnx",
      "sample_actions": true,
      "temperature": 2.0,
      "weight": 1
    }
  ]
}
```

- [ ] **Step 2: Commit**

```bash
git add backend/bot_trainer/v2/opponent_pool.json
git commit -m "feat(awr): add rollout_opponents with diverse temperatures for self-play"
```

---

### Task 2: Per-match weighted opponent sampling in league_config.py

**Files:**
- Modify: `backend/bot_trainer/v2/league_config.py`

- [ ] **Step 1: Add weighted random sample helper and update build_trajectory_configs**

Replace `league_config.py`:

```python
from __future__ import annotations

import argparse
import json
import random
from pathlib import Path
from typing import Any


def model_path_text(path: Path) -> str:
    return path.as_posix()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--pool", type=Path, default=None)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--matches", type=int, required=True)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--max-actions", type=int, default=2400)
    parser.add_argument("--mode", choices=["trajectory", "eval"], default="trajectory")
    parser.add_argument("--rollout-onnx", type=Path, default=None)
    parser.add_argument("--candidate-onnx", type=Path, default=None)
    parser.add_argument(
        "--baseline-onnx",
        type=Path,
        default=Path("backend/assets/sft/sft.onnx"),
    )
    return parser.parse_args()


def load_pool(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def clean_policy(policy: dict[str, Any]) -> dict[str, Any]:
    return {key: value for key, value in policy.items() if key != "weight"}


def weighted_sample(
    pool: list[dict[str, Any]],
    count: int,
    rng: random.Random,
) -> list[dict[str, Any]]:
    """Sample `count` items from pool with replacement, weighted by `weight` field."""
    if not pool:
        return []
    items = list(pool)
    weights = [float(item.get("weight", 1.0)) for item in items]
    total = sum(weights)
    if total <= 0:
        return [items[i % len(items)] for i in range(count)]
    chosen: list[dict[str, Any]] = []
    for _ in range(count):
        r = rng.random() * total
        cumulative = 0.0
        for item, w in zip(items, weights, strict=True):
            cumulative += w
            if r < cumulative:
                chosen.append(item)
                break
    return chosen


def build_trajectory_configs(
    pool: dict[str, Any],
    matches: int,
    seed: int,
    max_actions: int,
) -> list[dict[str, Any]]:
    learner = clean_policy(pool["learner"])
    learner.setdefault("display_name", "Learner")
    learner["sample_actions"] = True
    learner.setdefault("temperature", 1.0)

    opponents_pool = pool.get("rollout_opponents", [])

    configs: list[dict[str, Any]] = []
    for m in range(matches):
        match_rng = random.Random(seed + m * 1000 + 1)
        if opponents_pool:
            chosen = weighted_sample(opponents_pool, 3, match_rng)
        else:
            chosen = []
        configs.append({
            "matches": 1,
            "seed": seed + m * 1000,
            "max_actions_per_match": max_actions,
            "report_trajectories": True,
            "subjects": [{**learner, "display_name": "Learner"}],
            "opponents": [clean_policy(o) for o in chosen],
        })
    return configs


def build_eval_config(
    pool: dict[str, Any],
    candidate_onnx: Path,
    baseline_onnx: Path,
    matches: int,
    seed: int,
    max_actions: int,
) -> dict[str, Any]:
    return {
        "matches": matches,
        "seed": seed,
        "max_actions_per_match": max_actions,
        "report_trajectories": False,
        "subjects": [
            {
                "id": "baseline_neural",
                "display_name": "Baseline",
                "model_path": model_path_text(baseline_onnx),
                "sample_actions": False,
                "temperature": 1.0,
            },
            {
                "id": "awr_candidate_neural",
                "display_name": "AWR candidate",
                "model_path": model_path_text(candidate_onnx),
                "sample_actions": False,
                "temperature": 1.0,
            },
        ],
        "opponents": [],
    }


def apply_rollout_model_override(pool: dict[str, Any], rollout_onnx: Path | None) -> None:
    if rollout_onnx is None:
        return
    learner = pool["learner"]
    learner["model_path"] = model_path_text(rollout_onnx)


def write_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")


def main() -> None:
    args = parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    if args.mode == "trajectory":
        if args.pool is None:
            raise SystemExit("--pool is required for trajectory mode")
        pool = load_pool(args.pool)
        apply_rollout_model_override(pool, args.rollout_onnx)
        for index, config in enumerate(
            build_trajectory_configs(
                pool,
                args.matches,
                args.seed,
                args.max_actions,
            )
        ):
            write_json(args.output_dir / f"trajectory_config_{index}.json", config)
    else:
        if args.pool is None:
            raise SystemExit("--pool is required for eval mode")
        if args.candidate_onnx is None:
            raise SystemExit("--candidate-onnx is required for eval mode")
        pool = load_pool(args.pool)
        write_json(
            args.output_dir / "candidate_eval_config.json",
            build_eval_config(
                pool,
                args.candidate_onnx,
                args.baseline_onnx,
                args.matches,
                args.seed,
                args.max_actions,
            ),
        )


if __name__ == "__main__":
    main()
```

- [ ] **Step 2: Verify league_config.py imports and runs**

Run: `cd backend/bot_trainer/v2 && python league_config.py --help`

Expected: prints help text.

- [ ] **Step 3: Smoke test — generate 1 trajectory config with diverse opponents**

Run: `cd backend/bot_trainer/v2 && python -c "
from league_config import load_pool, build_trajectory_configs, write_json
from pathlib import Path
pool = load_pool(Path('opponent_pool.json'))
configs = build_trajectory_configs(pool, matches=2, seed=42, max_actions=2400)
for i, c in enumerate(configs):
    print(f'Config {i}: subjects={len(c[\"subjects\"])}, opponents={len(c[\"opponents\"])}')
    for o in c['opponents']:
        print(f'  opponent: {o[\"id\"]} temp={o.get(\"temperature\")}')
"`

Expected: 2 configs, each with 1 subject + 3 opponents with varying temperatures.

- [ ] **Step 4: Commit**

```bash
git add backend/bot_trainer/v2/league_config.py
git commit -m "feat(awr): per-match weighted opponent sampling for trajectory diversity"
```

---

### Task 3: Add advantage normalization to awr_dataset.py

**Files:**
- Modify: `backend/bot_trainer/v2/awr_dataset.py`
- Modify: `backend/bot_trainer/v2/test_awr_dataset.py`

- [ ] **Step 1: Write failing tests for compute_normalized_advantages**

Add to `test_awr_dataset.py`:

```python
from awr_dataset import compute_normalized_advantages


class TestNormalizedAdvantages:
    def test_none_mode_returns_raw_advantage(self):
        rows = [
            {"match_id": "m1", "seat_index": 0, "reward": 0.1},
            {"match_id": "m1", "seat_index": 0, "reward": 0.5},
        ]
        returns = [0.2, 0.6]
        values = [0.3, 0.4]
        adv = compute_normalized_advantages(rows, returns, values, mode="none")
        assert abs(adv[0] - (-0.1)) < 0.001
        assert abs(adv[1] - 0.2) < 0.001

    def test_per_match_normalization(self):
        rows = [
            {"match_id": "m1", "seat_index": 0, "reward": 0.1},
            {"match_id": "m1", "seat_index": 0, "reward": 0.5},
            {"match_id": "m1", "seat_index": 0, "reward": -0.3},
        ]
        returns = [0.2, 0.6, -0.2]
        values = [0.3, 0.3, 0.3]
        adv = compute_normalized_advantages(rows, returns, values, mode="per_match")
        mean = sum(adv) / len(adv)
        assert abs(mean) < 0.001, f"mean should be ~0, got {mean}"
        std = (sum((a - mean) ** 2 for a in adv) / len(adv)) ** 0.5
        assert abs(std - 1.0) < 0.001, f"std should be ~1, got {std}"

    def test_per_seat_normalization(self):
        rows = [
            {"match_id": "m1", "seat_index": 0, "reward": 0.1},
            {"match_id": "m1", "seat_index": 0, "reward": -0.1},
            {"match_id": "m1", "seat_index": 1, "reward": 0.9},
            {"match_id": "m1", "seat_index": 1, "reward": 1.1},
        ]
        returns = [0.2, -0.2, 1.0, 1.2]
        values = [0.1, 0.1, 0.1, 0.1]
        adv = compute_normalized_advantages(rows, returns, values, mode="per_seat")
        assert abs(adv[0] - (-adv[1])) < 0.001
        assert abs(adv[2] - (-adv[3])) < 0.001

    def test_clips_outliers(self):
        rows = [{"match_id": "m1", "seat_index": 0, "reward": 100.0}]
        returns = [100.0]
        values = [0.0]
        adv = compute_normalized_advantages(rows, returns, values, mode="none")
        assert abs(adv[0]) <= 5.0
```

Run: `cd backend/bot_trainer/v2 && python -m pytest test_awr_dataset.py::TestNormalizedAdvantages -v`

Expected: 4 FAIL (function not defined).

- [ ] **Step 2: Implement compute_normalized_advantages**

Add to `awr_dataset.py` (after `compute_discounted_returns_for_rows`):

```python
def compute_normalized_advantages(
    rows: list[dict[str, Any]],
    returns: list[float],
    values: list[float],
    mode: str = "per_match",
) -> list[float]:
    """
    Compute normalized advantages from returns and values.
    
    mode:
      "none"       — raw advantage = return - value, clipped to [-5, 5]
      "per_match"  — z-score normalize within each match_id group
      "per_seat"   — z-score normalize within each (match_id, seat_index) group
      "batch"      — z-score normalize across entire dataset
    """
    n = len(rows)
    raw = [min(max(returns[i] - values[i], -5.0), 5.0) for i in range(n)]

    if mode == "none":
        return raw

    if mode == "batch":
        mean = sum(raw) / n
        var = sum((x - mean) ** 2 for x in raw) / n
        std = (var + 1e-8) ** 0.5
        return [(x - mean) / std for x in raw]

    groups: dict[tuple[str, ...], list[int]] = {}
    for i, row in enumerate(rows):
        if mode == "per_match":
            key = (str(row["match_id"]),)
        elif mode == "per_seat":
            key = (str(row["match_id"]), str(row["seat_index"]))
        else:
            key = ("_global",)
        groups.setdefault(key, []).append(i)

    result = [0.0] * n
    for indices in groups.values():
        group_raw = [raw[i] for i in indices]
        g_n = len(group_raw)
        if g_n < 2:
            for i in indices:
                result[i] = raw[i]
            continue
        mean = sum(group_raw) / g_n
        var = sum((x - mean) ** 2 for x in group_raw) / g_n
        std = (var + 1e-8) ** 0.5
        for i in indices:
            result[i] = min(max((raw[i] - mean) / std, -5.0), 5.0)
    return result
```

- [ ] **Step 3: Run tests**

Run: `cd backend/bot_trainer/v2 && python -m pytest test_awr_dataset.py::TestNormalizedAdvantages -v`

Expected: 4 PASS.

- [ ] **Step 4: Commit**

```bash
git add backend/bot_trainer/v2/awr_dataset.py backend/bot_trainer/v2/test_awr_dataset.py
git commit -m "feat(awr): add per-match/per-seat advantage normalization"
```

---

### Task 4: Add --adv-norm to train_awr.py and update defaults

**Files:**
- Modify: `backend/bot_trainer/v2/train_awr.py`

- [ ] **Step 1: Update train_awr.py — add --adv-norm, wire normalization, tune defaults**

Edit `train_awr.py`:

1. Add `--adv-norm` argument to `parse_args()`:
```python
parser.add_argument("--adv-norm", default="per_match",
                    choices=["none", "per_match", "per_seat", "batch"])
```

2. Change defaults:
```python
parser.add_argument("--temperature", type=float, default=0.5,
                    help="AWR temperature for exp(adv/T)")
parser.add_argument("--weight-clip", type=float, default=20.0,
                    help="Max advantage weight")
```

3. After computing advantages in `main()`, add normalization:
In `main()`, after `value = outputs["value"].squeeze(-1)` and `returns = batch["return"].float()`, change the advantage computation block from:

```python
            with torch.no_grad():
                advantage = returns - value.detach()
                weights = torch.exp(advantage / args.temperature).clamp(
                    max=args.weight_clip
                )
                if args.policy_filter == "positive":
                    weights = torch.where(advantage > 0, weights, torch.zeros_like(weights))
```

To:

```python
            with torch.no_grad():
                advantage = returns - value.detach()
                # Batch-level z-score normalization when adv_norm is "batch"
                if args.adv_norm == "batch":
                    adv_mean = advantage.mean()
                    adv_std = advantage.std() + 1e-8
                    advantage = (advantage - adv_mean) / adv_std
                    advantage = advantage.clamp(-5.0, 5.0)
                elif args.adv_norm == "none":
                    advantage = advantage.clamp(-5.0, 5.0)
                # "per_match" and "per_seat" are handled in dataset (pre-computed)
                weights = torch.exp(advantage / args.temperature).clamp(
                    max=args.weight_clip
                )
                if args.policy_filter == "positive":
                    weights = torch.where(advantage > 0, weights, torch.zeros_like(weights))
```

4. In `main()`, after loading the dataset, apply per-match/per-seat normalization:

Add after `ds = ArenaTrajectoryDataset(...)`:

```python
    if args.adv_norm in ("per_match", "per_seat"):
        values = [float(row.get("value", 0.0)) for row in ds.rows]
        norm_adv = compute_normalized_advantages(
            ds.rows, ds.returns, values, mode=args.adv_norm
        )
        for i, row in enumerate(ds.rows):
            row["advantage"] = norm_adv[i]
```

And add the import at the top:
```python
from awr_dataset import ArenaTrajectoryDataset, compute_normalized_advantages
```

- [ ] **Step 2: Verify train_awr.py CLI**

Run: `cd backend/bot_trainer/v2 && python train_awr.py --help`

Expected: shows `--adv-norm` with choices and updated defaults for `--temperature` (0.5) and `--weight-clip` (20.0).

- [ ] **Step 3: Commit**

```bash
git add backend/bot_trainer/v2/train_awr.py
git commit -m "feat(awr): add --adv-norm param, tune temperature/clip defaults for normalized advantages"
```

---

### Task 5: Bump default trajectory matches in orchestration script

**Files:**
- Modify: `backend/bot_trainer/train_awr_model.ps1`

- [ ] **Step 1: Change default TrajectoryMatches from 20 to 100**

Edit line 19:
```powershell
    [string]$TrajectoryMatches = "100",
```

- [ ] **Step 2: Commit**

```bash
git add backend/bot_trainer/train_awr_model.ps1
git commit -m "feat(awr): bump default TrajectoryMatches 20→100 for data diversity"
```

---

### Task 6: Final verification

**Files:**
- Verify: All tests pass, all imports work

- [ ] **Step 1: Run all tests**

Run: `cd backend/bot_trainer/v2 && python -m pytest test_model.py test_awr_dataset.py -v`

Expected: all tests pass (19 tests: 9 model + 6 dataset + 4 new advantage tests).

- [ ] **Step 2: Verify league_config generates diverse configs**

Run: `cd backend/bot_trainer/v2 && python -c "
from league_config import load_pool, build_trajectory_configs
from pathlib import Path
pool = load_pool(Path('opponent_pool.json'))
configs = build_trajectory_configs(pool, matches=3, seed=42, max_actions=2400)
opponent_ids = set()
for c in configs:
    for o in c['opponents']:
        opponent_ids.add(o['id'])
print(f'Unique opponent IDs across 3 matches: {opponent_ids}')
assert len(opponent_ids) > 0, 'Should have diverse opponents'
print('OK: diverse opponents generated')
"`

Expected: prints unique opponent IDs and `OK`.

- [ ] **Step 3: Verify advantage normalization pipeline end-to-end**

Run: `cd backend/bot_trainer/v2 && python -c "
from awr_dataset import compute_normalized_advantages
rows = [{'match_id':'m1','seat_index':0,'reward':0.1},{'match_id':'m1','seat_index':0,'reward':0.5},{'match_id':'m2','seat_index':0,'reward':-0.5},{'match_id':'m2','seat_index':0,'reward':-0.3}]
returns = [0.2,0.6,-0.4,-0.2]
values = [0.1,0.1,0.1,0.1]
adv = compute_normalized_advantages(rows, returns, values, mode='per_match')
m1_adv = adv[:2]
m2_adv = adv[2:]
assert abs(sum(m1_adv)) < 0.001
assert abs(sum(m2_adv)) < 0.001
assert abs((sum(a**2 for a in m1_adv)/2)**0.5 - 1.0) < 0.001
print('OK: per_match normalization works')
"`

Expected: `OK: per_match normalization works`

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "chore: final verification for AWR Phase 1 quality boost"
```
