from __future__ import annotations

import argparse
import json
from collections import defaultdict
from pathlib import Path
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--trajectories", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=None)
    return parser.parse_args()


def load_rows(path: Path) -> list[dict[str, Any]]:
    return [
        json.loads(line)
        for line in path.read_text(encoding="utf-8-sig").splitlines()
        if line.strip()
    ]


def bucket_key(row: dict[str, Any]) -> str:
    phase = str(row.get("phase_bucket") or infer_phase_bucket(row))
    risk = str(row.get("risk_bucket") or infer_risk_bucket(row))
    action_head = str(row.get("action_head") or "unknown")
    return f"{phase}/{risk}/{action_head}"


def infer_phase_bucket(row: dict[str, Any]) -> str:
    wall_remaining = row.get("wall_remaining")
    if wall_remaining is None:
        return "unknown"
    wall = float(wall_remaining)
    if wall >= 50:
        return "early"
    if wall >= 25:
        return "mid"
    if wall >= 10:
        return "late"
    return "end"


def infer_risk_bucket(row: dict[str, Any]) -> str:
    risk_probs = row.get("risk_probs") or []
    max_risk = max([float(value) for value in risk_probs], default=0.0)
    if max_risk >= 0.6:
        return "high"
    if max_risk >= 0.3:
        return "medium"
    return "low"


def summarize_buckets(rows: list[dict[str, Any]]) -> dict[str, dict[str, dict[str, float | int]]]:
    stats: dict[str, dict[str, dict[str, float]]] = defaultdict(
        lambda: defaultdict(lambda: defaultdict(float))
    )
    for row in rows:
        policy = str(row.get("policy_id") or "unknown")
        key = bucket_key(row)
        bucket = stats[policy][key]
        bucket["count"] += 1.0
        bucket["reward_sum"] += float(row.get("reward", 0.0) or 0.0)
        bucket["terminal_reward_sum"] += float(row.get("terminal_reward", 0.0) or 0.0)
        bucket["done_count"] += 1.0 if row.get("done") else 0.0

    result: dict[str, dict[str, dict[str, float | int]]] = {}
    for policy, buckets in stats.items():
        result[policy] = {}
        for key, values in buckets.items():
            count = max(values["count"], 1.0)
            result[policy][key] = {
                "count": int(values["count"]),
                "avg_reward": values["reward_sum"] / count,
                "avg_terminal_reward": values["terminal_reward_sum"] / count,
                "done_rate": values["done_count"] / count,
            }
    return result


def main() -> None:
    args = parse_args()
    summary = {"bucket_metrics": summarize_buckets(load_rows(args.trajectories))}
    text = json.dumps(summary, indent=2, ensure_ascii=False)
    print(text)
    if args.output is not None:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text, encoding="utf-8")


if __name__ == "__main__":
    main()
