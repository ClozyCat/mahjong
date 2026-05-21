from __future__ import annotations

import argparse
import json
from collections import defaultdict
from itertools import combinations
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
    by_subject: dict[str, dict[str, float]] = defaultdict(lambda: defaultdict(float))
    subject_scores_by_pair: dict[tuple[int, int], dict[str, float]] = defaultdict(dict)
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
            metrics["model_loaded_seats"] += 1 if seat.get("model_loaded") else 0
            metrics["neural_action_count"] += seat.get("neural_action_count", 0)
            first_tenpai_turn = seat.get("first_tenpai_turn")
            if first_tenpai_turn is not None:
                metrics["first_tenpai_turn_sum"] += first_tenpai_turn
                metrics["first_tenpai_turn_count"] += 1

        subject_id = report.get("subject_id")
        if subject_id:
            metrics = by_subject[str(subject_id)]
            metrics["match_count"] += 1
            metrics["completed"] += 1 if report.get("completed") else 0
            if report.get("subject_final_score") is not None:
                metrics["score_sum"] += report["subject_final_score"]
                metrics["score_count"] += 1
            if report.get("subject_deal_in_count") is not None:
                metrics["deal_in_sum"] += report["subject_deal_in_count"]
            if report.get("subject_win_count") is not None:
                metrics["win_sum"] += report["subject_win_count"]
            if report.get("subject_final_score") is not None:
                subject_scores_by_pair[
                    (int(report.get("seed", 0)), int(report.get("match_index", 0)))
                ][str(subject_id)] = float(report["subject_final_score"])

    policies = {}
    for policy, metrics in sorted(by_policy.items()):
        seat_count = max(metrics["seat_count"], 1.0)
        decision_count = max(metrics["decision_count"], 1.0)
        first_tenpai_turn_count = metrics["first_tenpai_turn_count"]
        policies[policy] = {
            "seat_count": int(metrics["seat_count"]),
            "avg_score_delta": metrics["score_delta_sum"] / seat_count,
            "win_rate": metrics["wins"] / seat_count,
            "deal_in_rate": metrics["dealt_in"] / seat_count,
            "avg_first_tenpai_turn": (
                metrics["first_tenpai_turn_sum"] / first_tenpai_turn_count
                if first_tenpai_turn_count
                else None
            ),
            "final_tenpai_rate": metrics["final_tenpai"] / seat_count,
            "avg_claims": metrics["claim_count"] / seat_count,
            "avg_discards": metrics["discard_count"] / seat_count,
            "avg_decisions": metrics["decision_count"] / seat_count,
            "avg_latency_ms_per_decision": metrics["decision_latency_ms_sum"] / decision_count,
            "model_loaded_seats": int(metrics["model_loaded_seats"]),
            "neural_action_count": int(metrics["neural_action_count"]),
        }

    subjects = {}
    for subject_id, metrics in sorted(by_subject.items()):
        match_count = max(metrics["match_count"], 1.0)
        score_count = max(metrics["score_count"], 1.0)
        subjects[subject_id] = {
            "match_count": int(metrics["match_count"]),
            "completed_matches": int(metrics["completed"]),
            "avg_final_score": metrics["score_sum"] / score_count,
            "avg_deal_in_count": metrics["deal_in_sum"] / match_count,
            "avg_win_count": metrics["win_sum"] / match_count,
        }

    return {
        "matches": len(reports),
        "completed_matches": completed_matches,
        "policies": policies,
        "subjects": subjects,
        "paired_subjects": paired_subject_deltas(subject_scores_by_pair),
    }


def paired_subject_deltas(
    subject_scores_by_pair: dict[tuple[int, int], dict[str, float]],
) -> dict[str, Any]:
    subject_ids = sorted({
        subject_id
        for scores in subject_scores_by_pair.values()
        for subject_id in scores
    })
    paired: dict[str, Any] = {}
    for baseline_id, candidate_id in combinations(subject_ids, 2):
        deltas = [
            scores[candidate_id] - scores[baseline_id]
            for _, scores in sorted(subject_scores_by_pair.items())
            if baseline_id in scores and candidate_id in scores
        ]
        if not deltas:
            continue
        key = f"{baseline_id}__vs__{candidate_id}"
        paired[key] = {
            "baseline_policy": baseline_id,
            "candidate_policy": candidate_id,
            "paired_match_count": len(deltas),
            "avg_score_delta": sum(deltas) / len(deltas),
            "deltas": deltas,
        }
    return paired


def print_summary(summary: dict[str, Any]) -> None:
    print(
        f"Arena summary: matches={summary['matches']} "
        f"completed={summary['completed_matches']}"
    )
    for policy, metrics in summary["policies"].items():
        avg_first_tenpai_turn = metrics["avg_first_tenpai_turn"]
        avg_first_tenpai_turn_text = (
            f"{avg_first_tenpai_turn:.2f}"
            if avg_first_tenpai_turn is not None
            else "none"
        )
        print(
            f"  {policy}: "
            f"avg_score_delta={metrics['avg_score_delta']:.4f} "
            f"win_rate={metrics['win_rate']:.4f} "
            f"deal_in_rate={metrics['deal_in_rate']:.4f} "
            f"avg_first_tenpai_turn={avg_first_tenpai_turn_text} "
            f"final_tenpai_rate={metrics['final_tenpai_rate']:.4f} "
            f"avg_latency_ms_per_decision={metrics['avg_latency_ms_per_decision']:.2f} "
            f"model_loaded_seats={metrics['model_loaded_seats']} "
            f"neural_action_count={metrics['neural_action_count']}"
        )
    if summary.get("subjects"):
        print("Subjects:")
        for subject_id, metrics in summary["subjects"].items():
            print(
                f"  {subject_id}: "
                f"matches={metrics['match_count']} "
                f"completed={metrics['completed_matches']} "
                f"avg_final_score={metrics['avg_final_score']:.4f} "
                f"avg_deal_in_count={metrics['avg_deal_in_count']:.4f} "
                f"avg_win_count={metrics['avg_win_count']:.4f}"
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
