from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


CLAIM_RATE_ABSOLUTE_DRIFT_LIMIT = 2.0
CLAIM_RATE_RATIO_LIMIT = 2.0


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
    if candidate["deal_in_rate"] > baseline["deal_in_rate"] + 0.01:
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
    if claim_rate_is_excessive(baseline, candidate):
        failures.append("claim_rate")

    return {
        "accepted": not failures,
        "failures": failures,
        "baseline": baseline,
        "candidate": candidate,
    }


def claim_rate_is_excessive(
    baseline: dict[str, Any],
    candidate: dict[str, Any],
) -> bool:
    baseline_claims = baseline.get("avg_claims")
    candidate_claims = candidate.get("avg_claims")
    if baseline_claims is None or candidate_claims is None:
        return False
    baseline_value = float(baseline_claims)
    candidate_value = float(candidate_claims)
    absolute_limit = baseline_value + CLAIM_RATE_ABSOLUTE_DRIFT_LIMIT
    ratio_limit = baseline_value * CLAIM_RATE_RATIO_LIMIT
    return candidate_value > max(absolute_limit, ratio_limit)


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
