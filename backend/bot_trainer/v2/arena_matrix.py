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
    pool = json.loads(args.pool.read_text(encoding="utf-8-sig"))
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
