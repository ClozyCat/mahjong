from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


CLAIM_RATE_ABSOLUTE_DRIFT_LIMIT = 2.0
CLAIM_RATE_RATIO_LIMIT = 2.0
LATENCY_LIMIT_MS = 200.0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--summary", type=Path, required=True)
    parser.add_argument("--baseline-policy", default="baseline_neural")
    parser.add_argument("--candidate-policy", default="awr_candidate_neural")
    parser.add_argument("--output", type=Path, default=None)
    return parser.parse_args()


def evaluate_candidate(
    summary: dict[str, Any],
    baseline_policy: str,
    candidate_policy: str,
) -> dict[str, Any]:
    baseline = summary["policies"][baseline_policy]
    candidate = summary["policies"][candidate_policy]
    paired_key = f"{baseline_policy}__vs__{candidate_policy}"
    paired = summary.get("paired_subjects", {}).get(paired_key)
    report = build_promotion_report(baseline, candidate, paired)
    failures: list[str] = []
    failure_details: list[dict[str, Any]] = []

    if not report["metrics"]["avg_score_delta"]["passed"]:
        failures.append("avg_score_delta")
        failure_details.append(metric_failure_detail("avg_score_delta", report))
    if not report["metrics"]["win_rate"]["passed"]:
        failures.append("win_rate")
        failure_details.append(metric_failure_detail("win_rate", report))
    if not report["metrics"]["deal_in_rate"]["passed"]:
        failures.append("deal_in_rate")
        failure_details.append(metric_failure_detail("deal_in_rate", report))
    if not report["metrics"]["tenpai"]["passed"]:
        failures.append("tenpai")
        failure_details.append(metric_failure_detail("tenpai", report))
    if not report["claim_rate"]["passed"]:
        failures.append("claim_rate")
        failure_details.append(non_metric_failure_detail("claim_rate", report))
    if not report["latency"]["passed"]:
        failures.append("latency")
        failure_details.append(non_metric_failure_detail("latency", report))

    return {
        "accepted": not failures,
        "failures": failures,
        "failure_details": failure_details,
        "baseline": baseline,
        "candidate": candidate,
        "paired": paired,
        "promotion_report": report,
    }


def build_promotion_report(
    baseline: dict[str, Any],
    candidate: dict[str, Any],
    paired: dict[str, Any] | None,
) -> dict[str, Any]:
    report = {
        "metrics": {
            "avg_score_delta": greater_than_metric(
                baseline,
                candidate,
                "avg_score_delta",
            ),
            "win_rate": at_least_metric(baseline, candidate, "win_rate"),
            "deal_in_rate": deal_in_metric(baseline, candidate),
            "tenpai": tenpai_metric(baseline, candidate),
        },
        "claim_rate": claim_rate_report(baseline, candidate),
        "latency": latency_report(baseline, candidate),
        "paired": paired,
        "warnings": paired_warnings(paired),
    }
    return report


def greater_than_metric(
    baseline: dict[str, Any],
    candidate: dict[str, Any],
    key: str,
) -> dict[str, Any]:
    baseline_value = float(baseline[key])
    candidate_value = float(candidate[key])
    margin = candidate_value - baseline_value
    return {
        "baseline": baseline_value,
        "candidate": candidate_value,
        "threshold": baseline_value,
        "margin": margin,
        "passed": margin > 0.0,
    }


def at_least_metric(
    baseline: dict[str, Any],
    candidate: dict[str, Any],
    key: str,
) -> dict[str, Any]:
    baseline_value = float(baseline[key])
    candidate_value = float(candidate[key])
    margin = candidate_value - baseline_value
    return {
        "baseline": baseline_value,
        "candidate": candidate_value,
        "threshold": baseline_value,
        "margin": margin,
        "passed": margin >= 0.0,
    }


def deal_in_metric(
    baseline: dict[str, Any],
    candidate: dict[str, Any],
) -> dict[str, Any]:
    baseline_value = float(baseline["deal_in_rate"])
    candidate_value = float(candidate["deal_in_rate"])
    threshold = baseline_value + 0.01
    margin = threshold - candidate_value
    return {
        "baseline": baseline_value,
        "candidate": candidate_value,
        "threshold": threshold,
        "margin": margin,
        "passed": margin >= 0.0,
    }


def tenpai_metric(
    baseline: dict[str, Any],
    candidate: dict[str, Any],
) -> dict[str, Any]:
    first_margin = first_tenpai_margin(baseline, candidate)
    final_margin = float(candidate["final_tenpai_rate"]) - float(baseline["final_tenpai_rate"])
    tenpai_turn_ok = baseline["avg_first_tenpai_turn"] is None or first_margin >= 0.0
    final_tenpai_ok = final_margin >= 0.0
    return {
        "baseline": {
            "avg_first_tenpai_turn": baseline["avg_first_tenpai_turn"],
            "final_tenpai_rate": baseline["final_tenpai_rate"],
        },
        "candidate": {
            "avg_first_tenpai_turn": candidate["avg_first_tenpai_turn"],
            "final_tenpai_rate": candidate["final_tenpai_rate"],
        },
        "threshold": "first_tenpai_not_later_or_final_tenpai_not_lower",
        "margin": max(first_margin, final_margin),
        "first_tenpai_margin": first_margin,
        "final_tenpai_margin": final_margin,
        "passed": tenpai_turn_ok or final_tenpai_ok,
    }


def first_tenpai_margin(baseline: dict[str, Any], candidate: dict[str, Any]) -> float:
    baseline_turn = baseline.get("avg_first_tenpai_turn")
    candidate_turn = candidate.get("avg_first_tenpai_turn")
    if baseline_turn is None or candidate_turn is None:
        return 0.0
    return float(baseline_turn) - float(candidate_turn)


def metric_failure_detail(metric: str, report: dict[str, Any]) -> dict[str, Any]:
    metric_report = report["metrics"][metric]
    return {
        "metric": metric,
        "baseline": metric_report["baseline"],
        "candidate": metric_report["candidate"],
        "threshold": metric_report["threshold"],
        "margin": metric_report["margin"],
    }


def non_metric_failure_detail(metric: str, report: dict[str, Any]) -> dict[str, Any]:
    metric_report = report[metric]
    return {
        "metric": metric,
        "baseline": metric_report["baseline"],
        "candidate": metric_report["candidate"],
        "threshold": metric_report["threshold"],
        "margin": metric_report["margin"],
    }


def claim_rate_report(
    baseline: dict[str, Any],
    candidate: dict[str, Any],
) -> dict[str, Any]:
    baseline_claims = baseline.get("avg_claims")
    candidate_claims = candidate.get("avg_claims")
    if baseline_claims is None or candidate_claims is None:
        return empty_optional_metric()
    baseline_value = float(baseline_claims)
    candidate_value = float(candidate_claims)
    threshold = max(
        baseline_value + CLAIM_RATE_ABSOLUTE_DRIFT_LIMIT,
        baseline_value * CLAIM_RATE_RATIO_LIMIT,
    )
    return {
        "baseline": baseline_value,
        "candidate": candidate_value,
        "threshold": threshold,
        "margin": threshold - candidate_value,
        "passed": candidate_value <= threshold,
    }


def latency_report(
    baseline: dict[str, Any],
    candidate: dict[str, Any],
) -> dict[str, Any]:
    baseline_latency = baseline.get("avg_latency_ms_per_decision")
    candidate_latency = candidate.get("avg_latency_ms_per_decision")
    if candidate_latency is None:
        return empty_optional_metric()
    candidate_value = float(candidate_latency)
    return {
        "baseline": float(baseline_latency) if baseline_latency is not None else None,
        "candidate": candidate_value,
        "candidate_avg_ms_per_decision": candidate_value,
        "candidate_p95_ms": candidate.get("latency_ms_p95"),
        "candidate_max_ms": candidate.get("latency_ms_max"),
        "threshold": LATENCY_LIMIT_MS,
        "limit_ms": LATENCY_LIMIT_MS,
        "margin": LATENCY_LIMIT_MS - candidate_value,
        "passed": candidate_value < LATENCY_LIMIT_MS,
    }


def empty_optional_metric() -> dict[str, Any]:
    return {
        "baseline": None,
        "candidate": None,
        "threshold": None,
        "margin": None,
        "passed": True,
    }


def paired_warnings(paired: dict[str, Any] | None) -> list[str]:
    if paired is None:
        return ["paired_subjects_missing"]
    confidence_low = paired.get("confidence95_low")
    if confidence_low is not None and float(confidence_low) <= 0.0:
        return ["paired_confidence_interval_crosses_zero"]
    return []


def claim_rate_is_excessive(
    baseline: dict[str, Any],
    candidate: dict[str, Any],
) -> bool:
    return not claim_rate_report(baseline, candidate)["passed"]


def latency_is_excessive(candidate: dict[str, Any]) -> bool:
    return not latency_report({}, candidate)["passed"]


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
