from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--summary", type=Path, required=True)
    parser.add_argument("--baseline-policy", default="baseline_neural")
    parser.add_argument("--candidate-policy", default="rl_candidate_neural")
    parser.add_argument("--output", type=Path, default=None)
    return parser.parse_args()


def evaluate_candidate(
    summary: dict[str, Any],
    baseline_policy: str,
    candidate_policy: str,
) -> dict[str, Any]:
    baseline = summary["policies"][baseline_policy]
    candidate = summary["policies"][candidate_policy]
    failures: list[str] = []

    if candidate["avg_score_delta"] <= baseline["avg_score_delta"]:
        failures.append("avg_score_delta")
    if candidate["win_rate"] < baseline["win_rate"]:
        failures.append("win_rate")
    if candidate["deal_in_rate"] > baseline["deal_in_rate"] + 0.02:
        failures.append("deal_in_rate")
    tenpai_turn_ok = (
        baseline["avg_first_tenpai_turn"] is None
        or (
            candidate["avg_first_tenpai_turn"] is not None
            and candidate["avg_first_tenpai_turn"] <= baseline["avg_first_tenpai_turn"]
        )
    )
    final_tenpai_ok = candidate["final_tenpai_rate"] >= baseline["final_tenpai_rate"]
    if not (tenpai_turn_ok or final_tenpai_ok):
        failures.append("tenpai")
    if candidate["avg_latency_ms_per_decision"] >= 100.0:
        failures.append("latency")

    return {
        "accepted": not failures,
        "failures": failures,
        "baseline": baseline,
        "candidate": candidate,
    }


def main() -> None:
    args = parse_args()
    summary = json.loads(args.summary.read_text(encoding="utf-8"))
    result = evaluate_candidate(summary, args.baseline_policy, args.candidate_policy)
    text = json.dumps(result, indent=2, ensure_ascii=False)
    print(text)
    if args.output is not None:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text, encoding="utf-8")
    if not result["accepted"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
