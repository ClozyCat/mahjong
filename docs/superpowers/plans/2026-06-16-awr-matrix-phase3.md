# AWR Phase 3 Arena Matrix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace single-baseline candidate gate with multi-opponent arena matrix evaluation for reliable model promotion decisions.

**Architecture:** `league_config.py` gains `--mode matrix` generating per-opponent eval configs. New `arena_matrix.py` aggregates results into weighted summary. `candidate_gate.py` accepts multi-summary input. Pipeline script switches to matrix eval.

**Tech Stack:** Python 3.12+, JSON/Markdown output

---

### Task 1: Add --mode matrix to league_config.py

**Files:**
- Modify: `backend/bot_trainer/v2/league_config.py`

- [ ] **Step 1: Add build_matrix_configs and wire into main()**

Add `build_matrix_configs` after `build_eval_config`:

```python
def build_matrix_configs(
    pool: dict[str, Any],
    candidate_onnx: Path,
    matches: int,
    seed: int,
    max_actions: int,
) -> list[dict[str, Any]]:
    opponents_pool = pool.get("rollout_opponents", [])
    if not opponents_pool:
        raise ValueError("--mode matrix requires rollout_opponents in pool")
    configs: list[dict[str, Any]] = []
    for i, opp in enumerate(opponents_pool):
        opp_clean = clean_policy(opp)
        subjects: list[dict[str, Any]] = [
            {
                "id": "awr_candidate_neural",
                "display_name": "AWR Candidate",
                "model_path": model_path_text(candidate_onnx),
                "sample_actions": False,
                "temperature": 1.0,
            },
        ]
        for j in range(3):
            subjects.append({
                **opp_clean,
                "id": f"{opp['id']}_opponent_{j}",
                "display_name": f"{opp['id']} #{j+1}",
            })
        configs.append({
            "matches": matches,
            "seed": seed + i * 1000,
            "max_actions_per_match": max_actions,
            "report_trajectories": False,
            "subjects": subjects,
            "opponents": [],
        })
    return configs
```

In `main()`, add after the `if args.mode == "trajectory":` block:

```python
    elif args.mode == "matrix":
        if args.pool is None:
            raise SystemExit("--pool is required for matrix mode")
        if args.candidate_onnx is None:
            raise SystemExit("--candidate-onnx is required for matrix mode")
        pool = load_pool(args.pool)
        for index, config in enumerate(
            build_matrix_configs(
                pool,
                args.candidate_onnx,
                args.matches,
                args.seed,
                args.max_actions,
            )
        ):
            write_json(args.output_dir / f"matrix_config_{index}.json", config)
```

Update `parse_args()` to add `matrix` to choices:
```python
parser.add_argument("--mode", choices=["trajectory", "eval", "matrix"], default="trajectory")
```

- [ ] **Step 2: Verify generate matrix configs**

Run: `cd backend/bot_trainer/v2 && python -c "
from league_config import load_pool, build_matrix_configs
from pathlib import Path
pool = load_pool(Path('opponent_pool.json'))
candidate = Path('backend/assets/sft/sft.onnx')
configs = build_matrix_configs(pool, candidate, matches=20, seed=42, max_actions=2400)
print(f'Generated {len(configs)} matrix configs')
for c in configs:
    ids = [s['id'] for s in c['subjects']]
    print(f'  subjects: {ids}')
"`

Expected: 3 configs (one per rollout_opponent), each with candidate + 3 opponent seats.

- [ ] **Step 3: Commit**

```bash
git add backend/bot_trainer/v2/league_config.py
git commit -m "feat(awr): add --mode matrix for multi-opponent eval config generation"
```

---

### Task 2: Create arena_matrix.py aggregation script

**Files:**
- Create: `backend/bot_trainer/v2/arena_matrix.py`

- [ ] **Step 1: Write arena_matrix.py**

```python
from __future__ import annotations

import argparse
import json
from collections import defaultdict
from pathlib import Path
from typing import Any

from arena_summary import load_reports, summarize_reports


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--results", type=Path, nargs="+", required=True,
                        help="One or more arena output JSON files (one per opponent)")
    parser.add_argument("--pool", type=Path, required=True,
                        help="opponent_pool.json for opponent weights")
    parser.add_argument("--candidate-policy", default="awr_candidate_neural")
    parser.add_argument("--output", type=Path, default=None)
    parser.add_argument("--format", choices=["json", "markdown", "both"], default="both")
    return parser.parse_args()


def build_matrix(
    results_paths: list[Path],
    pool: dict[str, Any],
    candidate_policy: str,
) -> dict[str, Any]:
    opponents_pool = pool.get("rollout_opponents", [])
    rows: list[dict[str, Any]] = []
    weighted_scores: dict[str, float] = defaultdict(float)
    weighted_wins: dict[str, float] = defaultdict(float)
    weighted_dealt_in: dict[str, float] = defaultdict(float)
    weighted_tenpai: dict[str, float] = defaultdict(float)
    total_weight = 0.0
    total_matches = 0

    if len(results_paths) != len(opponents_pool):
        raise ValueError(
            f"Mismatch: {len(results_paths)} results for {len(opponents_pool)} opponents"
        )

    for i, (results_path, opp) in enumerate(zip(results_paths, opponents_pool, strict=True)):
        reports = load_reports(results_path)
        summary = summarize_reports(reports)
        candidate_stats = summary["policies"].get(candidate_policy, {})
        weight = float(opp.get("weight", 1.0))

        row = {
            "opponent": opp["id"],
            "temperature": opp.get("temperature"),
            "matches": summary.get("completed_matches", 0),
            "avg_score_delta": candidate_stats.get("avg_score_delta", 0.0),
            "win_rate": candidate_stats.get("win_rate", 0.0),
            "deal_in_rate": candidate_stats.get("deal_in_rate", 0.0),
            "final_tenpai_rate": candidate_stats.get("final_tenpai_rate", 0.0),
            "avg_latency_ms": candidate_stats.get("avg_latency_ms_per_decision"),
        }
        rows.append(row)

        for key in ["avg_score_delta", "win_rate", "deal_in_rate", "final_tenpai_rate"]:
            weighted_scores[key] += float(row.get(key, 0.0)) * weight
        total_weight += weight
        total_matches += row["matches"]

    weighted_summary: dict[str, float] = {}
    for key in weighted_scores:
        weighted_summary[key] = weighted_scores[key] / total_weight if total_weight > 0 else 0.0

    return {
        "candidate_policy": candidate_policy,
        "rows": rows,
        "weighted_summary": weighted_summary,
        "total_matches": total_matches,
    }


def format_markdown(matrix: dict[str, Any]) -> str:
    lines = [
        f"## Arena Matrix: {matrix['candidate_policy']}",
        "",
        "| Opponent | Temp | Matches | Score Δ | Win % | Deal-in % | Tenpai % | Latency ms |",
        "|----------|------|---------|---------|-------|-----------|----------|------------|",
    ]
    for row in matrix["rows"]:
        temp = row.get("temperature", "-")
        latency = f"{row['avg_latency_ms']:.2f}" if row.get("avg_latency_ms") else "-"
        lines.append(
            f"| {row['opponent']} | {temp} | {row['matches']} | "
            f"{row['avg_score_delta']:+.1f} | {row['win_rate']:.3f} | "
            f"{row['deal_in_rate']:.3f} | {row['final_tenpai_rate']:.3f} | {latency} |"
        )

    ws = matrix["weighted_summary"]
    lines.append(
        f"| **Weighted Avg** | | {matrix['total_matches']} | "
        f"{ws['avg_score_delta']:+.1f} | {ws['win_rate']:.3f} | "
        f"{ws['deal_in_rate']:.3f} | {ws['final_tenpai_rate']:.3f} | - |"
    )
    return "\n".join(lines)


def main() -> None:
    args = parse_args()
    pool = json.loads(args.pool.read_text(encoding="utf-8"))
    matrix = build_matrix(args.results, pool, args.candidate_policy)

    if args.format in ("json", "both"):
        json_text = json.dumps(matrix, indent=2, ensure_ascii=False)
        if args.output:
            json_path = args.output.with_suffix(".json")
            json_path.write_text(json_text, encoding="utf-8")
        else:
            print(json_text)

    if args.format in ("markdown", "both"):
        md = format_markdown(matrix)
        if args.output:
            md_path = args.output.with_suffix(".md")
            md_path.write_text(md + "\n", encoding="utf-8")
        else:
            print(md)


if __name__ == "__main__":
    main()
```

- [ ] **Step 2: Verify imports and CLI**

Run: `cd backend/bot_trainer/v2 && python arena_matrix.py --help`

Expected: prints help text with `--results`, `--pool`, `--candidate-policy`, `--format`.

- [ ] **Step 3: Commit**

```bash
git add backend/bot_trainer/v2/arena_matrix.py
git commit -m "feat(awr): add arena_matrix.py for multi-opponent weighted evaluation"
```

---

### Task 3: Update candidate_gate.py for multi-summary input

**Files:**
- Modify: `backend/bot_trainer/v2/candidate_gate.py`

- [ ] **Step 1: Update candidate_gate.py**

Change `parse_args()` to accept multiple summaries:
```python
parser.add_argument("--summary", type=Path, nargs="+", required=True,
                    help="One or more arena summary JSON files (from arena_summary.py)")
```

Change `evaluate_candidate` to accept a list of summaries and compute weighted averages. Add new function:

```python
def evaluate_candidate_matrix(
    summaries: list[dict[str, Any]],
    pool_path: Path | None,
    baseline_policy: str = "baseline_neural",
    candidate_policy: str = "awr_candidate_neural",
) -> dict[str, Any]:
    if pool_path is not None:
        pool = json.loads(pool_path.read_text(encoding="utf-8"))
        opponents = pool.get("rollout_opponents", [])
    else:
        opponents = [{"weight": 1.0} for _ in summaries]

    if len(summaries) != len(opponents):
        raise ValueError(f"Mismatch: {len(summaries)} summaries for {len(opponents)} opponents")

    per_opponent: list[dict[str, Any]] = []
    total_weight = 0.0
    weighted_metrics: dict[str, float] = defaultdict(float)
    all_failures: set[str] = set()

    for i, (summary, opp) in enumerate(zip(summaries, opponents, strict=True)):
        result = evaluate_candidate(summary, baseline_policy, candidate_policy)
        per_opponent.append({
            "opponent_id": opp.get("id", f"opponent_{i}"),
            "weight": float(opp.get("weight", 1.0)),
            "result": result,
        })
        w = float(opp.get("weight", 1.0))
        for metric_key in ["avg_score_delta", "win_rate", "deal_in_rate"]:
            m = result["promotion_report"]["metrics"].get(metric_key, {})
            weighted_metrics[metric_key] += float(m.get("margin", 0.0)) * w
        total_weight += w
        if not result["accepted"]:
            all_failures.update(result["failures"])

    for key in weighted_metrics:
        weighted_metrics[key] /= total_weight if total_weight > 0 else 1.0

    accepted = (
        weighted_metrics.get("avg_score_delta", -1.0) > 0
        and weighted_metrics.get("win_rate", -1.0) >= 0
        and weighted_metrics.get("deal_in_rate", -1.0) >= 0
    )

    return {
        "accepted": accepted,
        "weighted_metrics": dict(weighted_metrics),
        "per_opponent": per_opponent,
        "all_failures": sorted(all_failures),
    }
```

Add `--pool` argument:
```python
parser.add_argument("--pool", type=Path, default=None,
                    help="opponent_pool.json for weighted averaging")
```

Update `main()` to use matrix path when multiple summaries:
```python
def main() -> None:
    args = parse_args()
    summaries = [
        json.loads(p.read_text(encoding="utf-8"))
        for p in args.summary
    ]
    if len(summaries) > 1 or args.pool is not None:
        result = evaluate_candidate_matrix(
            summaries, args.pool,
            args.baseline_policy, args.candidate_policy,
        )
    else:
        result = evaluate_candidate(
            summaries[0], args.baseline_policy, args.candidate_policy
        )
    text = json.dumps(result, indent=2, ensure_ascii=False)
    print(text)
    if args.output is not None:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text, encoding="utf-8")
    if not result["accepted"]:
        raise SystemExit(1)
```

Import `defaultdict` and `json` (already imported).

- [ ] **Step 2: Verify CLI**

Run: `cd backend/bot_trainer/v2 && python candidate_gate.py --help`

Expected: `--summary` takes multiple paths, `--pool` option present.

- [ ] **Step 3: Commit**

```bash
git add backend/bot_trainer/v2/candidate_gate.py
git commit -m "feat(awr): update candidate_gate.py for multi-summary matrix evaluation"
```

---

### Task 4: Update train_awr_model.ps1 for matrix eval

**Files:**
- Modify: `backend/bot_trainer/train_awr_model.ps1`

- [ ] **Step 1: Replace single eval section in pipeline script**

Change the evaluation section (after "Exporting AWR ONNX") to use matrix mode. Update lines ~95-135 of the script:

```powershell
    # 5. Matrix evaluation
    Write-Host "Evaluating candidate vs multiple opponents..."
    $matrixConfigDir = "$OutputDir/iter_$iter/matrix_config"
    New-Item -ItemType Directory -Force -Path $matrixConfigDir | Out-Null

    python backend/bot_trainer/v2/league_config.py `
        --pool $Pool `
        --output-dir $matrixConfigDir `
        --matches $MatrixMatches `
        --seed $($iterSeed + 2) `
        --mode matrix `
        --candidate-onnx $awrOnnx

    $matrixResults = @()
    $configFiles = Get-ChildItem -LiteralPath $matrixConfigDir -Filter "matrix_config_*.json" | Sort-Object Name
    foreach ($configFile in $configFiles) {
        $resultFile = "$OutputDir/iter_$iter/matrix_result_$($configFile.BaseName).json"
        cargo run --manifest-path backend/Cargo.toml --bin bot_arena -- `
            --config $configFile.FullName `
            --output $resultFile
        if (Test-Path $resultFile) {
            $matrixResults += $resultFile
        }
    }

    # 6. Candidate gate (matrix mode)
    $gateResult = "$OutputDir/iter_$iter/gate_result.json"
    $exitCode = 0
    try {
        python backend/bot_trainer/v2/candidate_gate.py `
            --summary $($matrixResults -join ' ') `
            --pool $Pool `
            --output $gateResult
    } catch {
        $exitCode = $LASTEXITCODE
    }
```

Add new param at the top of the script:
```powershell
    [string]$MatrixMatches = "20",
```

- [ ] **Step 2: Commit**

```bash
git add backend/bot_trainer/train_awr_model.ps1
git commit -m "feat(awr): switch pipeline evaluation to matrix mode (multi-opponent)"
```

---

### Task 5: Final verification

- [ ] **Step 1: Verify all Python modules import**

Run: `cd backend/bot_trainer/v2 && python -c "
from league_config import build_matrix_configs
from arena_matrix import build_matrix, format_markdown
from candidate_gate import evaluate_candidate_matrix
print('All matrix modules import OK')
"`

Expected: `All matrix modules import OK`

- [ ] **Step 2: Generate smoke test matrix config**

Run: `cd backend/bot_trainer/v2 && python -c "
from league_config import load_pool, build_matrix_configs
from pathlib import Path
pool = load_pool(Path('opponent_pool.json'))
configs = build_matrix_configs(pool, Path('backend/assets/sft/sft.onnx'), 5, 42, 2400)
for c in configs:
    print(f'Config: {c[\"matches\"]} matches, subjects={[s[\"id\"] for s in c[\"subjects\"]]}')
"`

Expected: 3 configs printed with opponent IDs.

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "chore: final verification for AWR Phase 3 matrix evaluation"
```
