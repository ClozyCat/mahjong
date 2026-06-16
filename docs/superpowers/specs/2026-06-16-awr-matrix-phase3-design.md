# Design: AWR Phase 3 — Arena Matrix Evaluation

**Date:** 2026-06-16
**Status:** Approved
**Scope:** Multi-opponent arena evaluation matrix, weighted aggregation, updated candidate gate. Builds on Phase 1+2 (already completed).

## Motivation

Current evaluation runs candidate vs a single baseline (SFT greedy). Mahjong has high variance — a lucky/unlucky 50-match run can mislead promotion decisions. Phase 3 replaces single-baseline gating with a matrix that tests the candidate against multiple opponent types, providing more reliable signal.

## Design

### 1. Matrix Config Generation

**`league_config.py`** — new `--mode matrix`:

Reads `opponent_pool.json`'s `rollout_opponents` list. For each opponent type, generates one eval config where:
- 1 candidate seat (greedy, `sample_actions=False`)
- 3 opponent seats (all same opponent variant, with sampling if configured)

```python
def build_matrix_configs(pool, candidate_onnx, matches, seed, max_actions):
    opponents_pool = pool.get("rollout_opponents", [])
    configs = []
    for i, opp in enumerate(opponents_pool):
        opp_clean = clean_policy(opp)
        subjects = [
            {"id": "awr_candidate_neural", "display_name": "AWR Candidate",
             "model_path": model_path_text(candidate_onnx),
             "sample_actions": False, "temperature": 1.0},
        ]
        for j in range(3):
            subjects.append({
                **opp_clean,
                "id": f"{opp['id']}_opponent_{j}",
                "display_name": f"{opp['id']} #{j}",
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

CLI: `--mode matrix --candidate-onnx <path> --pool <path> --matches 20 --seed <seed>`

Output: `matrix_config_0.json`, `matrix_config_1.json`, ... (one per opponent type).

### 2. Matrix Aggregation Script

**New file `arena_matrix.py`**:

Input: multiple arena output JSON files + opponent pool for weights.
Output: Markdown matrix table + JSON.

```python
def build_matrix(results: list[dict], pool: dict) -> dict:
    """Aggregate per-opponent stats into weighted summary."""
    opponents = pool.get("rollout_opponents", [])
    rows = []
    weighted_summary = defaultdict(float)
    total_weight = 0.0
    
    for i, opp in enumerate(opponents):
        summary = results[i]  # pre-computed by arena_summary
        stats = summary["policies"]["awr_candidate_neural"]
        weight = float(opp.get("weight", 1.0))
        rows.append({"opponent": opp["id"], "temperature": opp.get("temperature"), **stats})
        for key in ["avg_score_delta", "win_rate", "deal_in_rate", "final_tenpai_rate"]:
            weighted_summary[key] += float(stats[key]) * weight
        total_weight += weight
    
    for key in weighted_summary:
        weighted_summary[key] /= total_weight
    
    return {"rows": rows, "weighted_summary": dict(weighted_summary), "total_matches": ...}
```

Markdown output format:
```
## Arena Matrix: awr_candidate vs opponents

| Opponent | Temp | Score Δ | Win % | Deal-in % | Tenpai % | Latency ms |
|----------|------|---------|-------|-----------|----------|------------|
| sft_cold | 0.5  | +12.3   | 0.285 | 0.142     | 0.823    | 1.21       |
| sft_warm | 1.0  | +8.1    | 0.261 | 0.168     | 0.791    | 1.19       |
| sft_hot  | 2.0  | +18.5   | 0.312 | 0.121     | 0.847    | 1.20       |
| **Weighted Avg** | | **+13.0** | **0.286** | **0.144** | **0.820** | **1.20** |
```

### 3. Updated Candidate Gate

**`candidate_gate.py`** — accept multi-summary input:

- `--summary` accepts comma-separated paths or a directory
- Collects per-opponent stats
- Computes weighted average for each metric
- Gate checks use weighted averages
- Additional check: no single opponent has deal_in_rate > baseline + 0.05 (outlier detection)
- Output includes per-opponent breakdown + weighted summary

### 4. Pipeline Integration

**`train_awr_model.ps1`** evaluation section: replace single eval with matrix.

New CLI param: `--matrix-matches 20` (default).

### Files Changed

| File | Change |
|------|--------|
| `league_config.py` | Add `--mode matrix`, `build_matrix_configs()` |
| `arena_matrix.py` | **NEW** — aggregation + Markdown output |
| `candidate_gate.py` | Multi-summary input, weighted average gate |
| `train_awr_model.ps1` | Matrix eval in pipeline |

### Files NOT Changed

| File | Reason |
|------|--------|
| `arena_summary.py` | Used as-is by arena_matrix.py; already handles per-policy paired stats |
| All training/model files | Evaluation-only feature |

### Testing

- Unit test: `build_matrix_configs` generates correct number of configs with correct opponent IDs
- Unit test: `arena_matrix.build_matrix` computes correct weighted averages
- Integration: `league_config.py --mode matrix` generates configs that `bot_arena` can consume
- Integration: `candidate_gate.py --summary dir/` processes multi-summary input

### Risks

- **All opponents rejected independently**: If candidate fails against every opponent type, it's genuinely bad. If it fails against only 1, the weighted average may still pass. This is intentional — the matrix reduces false negatives from lucky/unlucky single-opponent runs.
- **Matrix runtime**: 3 opponents × 20 matches = 60 matches. At ~2s/match (after optimization), ~2 minutes total. Acceptable for a gate step.
