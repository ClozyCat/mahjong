from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


CANDIDATE_LATENCY_LIMIT_MS = 200.0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def metric(metrics: dict[str, Any], key: str, default: float = 0.0) -> float:
    value = metrics.get(key)
    if value is None:
        return default
    return float(value)


def first_tenpai_margin(baseline: dict[str, Any], candidate: dict[str, Any]) -> float:
    baseline_turn = baseline.get("avg_first_tenpai_turn")
    candidate_turn = candidate.get("avg_first_tenpai_turn")
    if baseline_turn is None or candidate_turn is None:
        return 0.0
    return float(baseline_turn) - float(candidate_turn)


def summarize_candidate(entry: dict[str, Any]) -> dict[str, Any]:
    gate = entry["gate"]
    baseline = gate["baseline"]
    candidate = gate["candidate"]
    promotion_report = gate.get("promotion_report")
    paired = gate.get("paired") or (promotion_report or {}).get("paired")
    score_margin = metric(candidate, "avg_score_delta") - metric(baseline, "avg_score_delta")
    win_margin = metric(candidate, "win_rate") - metric(baseline, "win_rate")
    deal_in_margin = metric(baseline, "deal_in_rate") + 0.01 - metric(candidate, "deal_in_rate")
    final_tenpai_margin = metric(candidate, "final_tenpai_rate") - metric(
        baseline,
        "final_tenpai_rate",
    )
    tenpai_margin = max(first_tenpai_margin(baseline, candidate), final_tenpai_margin)
    latency_margin = CANDIDATE_LATENCY_LIMIT_MS - metric(candidate, "avg_latency_ms_per_decision")
    return {
        "epoch": entry.get("epoch"),
        "policy": entry.get("policy"),
        "checkpoint": entry.get("checkpoint"),
        "onnx": entry.get("onnx"),
        "summary": entry.get("summary"),
        "gate": entry.get("gate_path"),
        "accepted": bool(gate.get("accepted")),
        "failures": list(gate.get("failures", [])),
        "score_margin": round(score_margin, 6),
        "win_margin": round(win_margin, 6),
        "deal_in_margin": round(deal_in_margin, 6),
        "tenpai_margin": round(tenpai_margin, 6),
        "latency_margin": round(latency_margin, 6),
        "paired_avg_score_delta": paired_value(paired, "avg_score_delta"),
        "paired_confidence95_low": paired_value(paired, "confidence95_low"),
        "paired_confidence95_high": paired_value(paired, "confidence95_high"),
        "paired_positive_delta_rate": paired_value(paired, "positive_delta_rate"),
        "promotion_report": promotion_report,
    }


def rank_key(summary: dict[str, Any]) -> tuple[float, ...]:
    return (
        1.0 if summary["accepted"] else 0.0,
        summary["score_margin"],
        summary["win_margin"],
        summary["tenpai_margin"],
        summary["deal_in_margin"],
        summary["latency_margin"],
    )


def paired_value(paired: dict[str, Any] | None, key: str) -> float | None:
    if paired is None or paired.get(key) is None:
        return None
    return round(float(paired[key]), 6)


def choose_next_rollout(
    current: dict[str, Any],
    candidate: dict[str, Any],
) -> dict[str, Any]:
    if candidate.get("accepted"):
        return candidate
    current_margin = metric(current, "score_margin", float("-inf"))
    candidate_margin = metric(candidate, "score_margin", float("-inf"))
    if candidate_margin > current_margin:
        return candidate
    return current


def select_best_candidate(candidates: list[dict[str, Any]]) -> dict[str, Any]:
    if not candidates:
        raise ValueError("at least one candidate is required")
    summaries = [summarize_candidate(candidate) for candidate in candidates]
    selected = max(summaries, key=rank_key)
    return {
        "selected": selected,
        "epoch": selected["epoch"],
        "policy": selected.get("policy"),
        "checkpoint": selected["checkpoint"],
        "onnx": selected["onnx"],
        "accepted": selected["accepted"],
        "failures": selected["failures"],
        "score_margin": selected["score_margin"],
        "paired_avg_score_delta": selected["paired_avg_score_delta"],
        "paired_confidence95_low": selected["paired_confidence95_low"],
        "paired_confidence95_high": selected["paired_confidence95_high"],
        "paired_positive_delta_rate": selected["paired_positive_delta_rate"],
        "promotion_report": selected["promotion_report"],
        "candidates": summaries,
    }


def load_candidates(manifest: Path) -> list[dict[str, Any]]:
    payload = json.loads(manifest.read_text(encoding="utf-8"))
    raw_candidates = payload.get("candidates", payload)
    candidates = []
    for entry in raw_candidates:
        candidate = dict(entry)
        gate_path = candidate.get("gate_path") or candidate.get("gate")
        if "gate" not in candidate or isinstance(candidate["gate"], str):
            if gate_path is None:
                raise ValueError("candidate entry must include gate or gate_path")
            candidate["gate"] = json.loads(Path(gate_path).read_text(encoding="utf-8"))
            candidate["gate_path"] = str(gate_path)
        candidates.append(candidate)
    return candidates


def main() -> None:
    args = parse_args()
    result = select_best_candidate(load_candidates(args.manifest))
    text = json.dumps(result, indent=2, ensure_ascii=False)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(text + "\n", encoding="utf-8")
    print(text)


if __name__ == "__main__":
    main()
