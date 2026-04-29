from __future__ import annotations

import argparse
import json
from collections import defaultdict
from pathlib import Path
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=None)
    return parser.parse_args()


def load_reports(path: Path) -> list[dict[str, Any]]:
    return [
        json.loads(line)
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]


def summarize_reports(reports: list[dict[str, Any]]) -> dict[str, Any]:
    by_policy: dict[str, dict[str, float]] = defaultdict(lambda: defaultdict(float))
    completed_matches = 0
    for report in reports:
        completed_matches += 1 if report.get("completed") else 0
        for seat in report["seats"]:
            policy = seat["policy_id"]
            metrics = by_policy[policy]
            metrics["seat_count"] += 1
            metrics["score_delta_sum"] += seat["score_delta"]
            metrics["wins"] += seat["wins"]
            metrics["dealt_in"] += seat["dealt_in"]
            metrics["claim_count"] += seat["claim_count"]
            metrics["discard_count"] += seat["discard_count"]
            metrics["decision_count"] += seat["decision_count"]
            metrics["decision_latency_ms_sum"] += seat["decision_latency_ms_sum"]
            metrics["final_tenpai"] += 1 if seat["final_tenpai"] else 0

    policies = {}
    for policy, metrics in sorted(by_policy.items()):
        seat_count = max(metrics["seat_count"], 1.0)
        decision_count = max(metrics["decision_count"], 1.0)
        policies[policy] = {
            "seat_count": int(metrics["seat_count"]),
            "avg_score_delta": metrics["score_delta_sum"] / seat_count,
            "win_rate": metrics["wins"] / seat_count,
            "deal_in_rate": metrics["dealt_in"] / seat_count,
            "final_tenpai_rate": metrics["final_tenpai"] / seat_count,
            "avg_claims": metrics["claim_count"] / seat_count,
            "avg_discards": metrics["discard_count"] / seat_count,
            "avg_decisions": metrics["decision_count"] / seat_count,
            "avg_latency_ms_per_decision": metrics["decision_latency_ms_sum"] / decision_count,
        }

    return {
        "matches": len(reports),
        "completed_matches": completed_matches,
        "policies": policies,
    }


def print_summary(summary: dict[str, Any]) -> None:
    print(
        f"Arena summary: matches={summary['matches']} "
        f"completed={summary['completed_matches']}"
    )
    for policy, metrics in summary["policies"].items():
        print(
            f"  {policy}: "
            f"avg_score_delta={metrics['avg_score_delta']:.4f} "
            f"win_rate={metrics['win_rate']:.4f} "
            f"deal_in_rate={metrics['deal_in_rate']:.4f} "
            f"final_tenpai_rate={metrics['final_tenpai_rate']:.4f} "
            f"avg_latency_ms_per_decision={metrics['avg_latency_ms_per_decision']:.2f}"
        )


def main() -> None:
    args = parse_args()
    summary = summarize_reports(load_reports(args.input))
    print_summary(summary)
    if args.output is not None:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(
            json.dumps(summary, indent=2, ensure_ascii=False),
            encoding="utf-8",
        )


if __name__ == "__main__":
    main()
