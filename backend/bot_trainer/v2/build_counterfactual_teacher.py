from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from awr_dataset import compute_discounted_returns_for_rows


@dataclass(frozen=True)
class RolloutTeacherConfig:
    gamma: float = 0.995
    prior_weight: float = 0.25
    safety_weight: float = 0.10
    teacher_logit_weight: float = 1.0
    min_count: int = 5
    max_prior_abs: float = 3.0
    max_score_delta: float = 0.35
    policy_id: str = "learner"
    normalize_teacher_scores: bool = False


@dataclass(frozen=True)
class RolloutPriors:
    global_mean: float
    global_std: float
    by_phase_risk_action: dict[tuple[str, str, int], tuple[float, int]]
    by_action: dict[int, tuple[float, int]]

    def score(self, phase_bucket: str, risk_bucket: str, action_index: int, min_count: int, max_abs: float) -> tuple[float, bool]:
        value = self.by_phase_risk_action.get((phase_bucket, risk_bucket, action_index))
        if value is None or value[1] < min_count:
            value = self.by_action.get(action_index)
        if value is None or value[1] < min_count:
            return 0.0, False
        z_score = (value[0] - self.global_mean) / max(self.global_std, 1.0e-6)
        return min(max(z_score, -max_abs), max_abs), True


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Build rollout-enhanced counterfactual discard teacher")
    parser.add_argument("--counterfactual-discards", type=Path, required=True)
    parser.add_argument("--trajectories", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--policy-id", default="learner")
    parser.add_argument("--gamma", type=float, default=0.995)
    parser.add_argument("--prior-weight", type=float, default=0.25)
    parser.add_argument("--safety-weight", type=float, default=0.10)
    parser.add_argument("--teacher-logit-weight", type=float, default=1.0)
    parser.add_argument("--min-count", type=int, default=5)
    parser.add_argument("--max-score-delta", type=float, default=0.35)
    parser.add_argument("--normalize-teacher-scores", action="store_true")
    return parser.parse_args()


def load_jsonl(path: Path) -> list[dict[str, Any]]:
    return [
        json.loads(line)
        for line in path.read_text(encoding="utf-8-sig").splitlines()
        if line.strip()
    ]


def save_jsonl(rows: list[dict[str, Any]], path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as file:
        for row in rows:
            file.write(json.dumps(row, ensure_ascii=False) + "\n")


def mean(values: list[float]) -> float:
    return sum(values) / len(values) if values else 0.0


def std(values: list[float], center: float) -> float:
    if len(values) < 2:
        return 1.0
    return (sum((value - center) ** 2 for value in values) / len(values)) ** 0.5


def normalize_scores(scores: list[float]) -> list[float]:
    center = mean(scores)
    scale = std(scores, center)
    return [(score - center) / max(scale, 1.0e-6) for score in scores]


def build_rollout_priors(trajectory_rows: list[dict[str, Any]], config: RolloutTeacherConfig) -> RolloutPriors:
    rows = [row for row in trajectory_rows if row.get("policy_id") == config.policy_id]
    returns = compute_discounted_returns_for_rows(rows, gamma=config.gamma)
    by_phase_risk_action_values: dict[tuple[str, str, int], list[float]] = {}
    by_action_values: dict[int, list[float]] = {}
    all_returns: list[float] = []
    for row, discounted_return in zip(rows, returns, strict=True):
        if row.get("action_head") != "discard":
            continue
        action_index = int(row.get("action_index", -1))
        if not 0 <= action_index < 34:
            continue
        value = float(discounted_return)
        phase_bucket = str(row.get("phase_bucket", "unknown"))
        risk_bucket = str(row.get("risk_bucket", "unknown"))
        by_phase_risk_action_values.setdefault((phase_bucket, risk_bucket, action_index), []).append(value)
        by_action_values.setdefault(action_index, []).append(value)
        all_returns.append(value)

    global_mean = mean(all_returns)
    global_std = std(all_returns, global_mean)
    return RolloutPriors(
        global_mean=global_mean,
        global_std=global_std,
        by_phase_risk_action={key: (mean(values), len(values)) for key, values in by_phase_risk_action_values.items()},
        by_action={key: (mean(values), len(values)) for key, values in by_action_values.items()},
    )


def enhance_counterfactual_rows(
    counterfactual_rows: list[dict[str, Any]],
    trajectory_rows: list[dict[str, Any]],
    config: RolloutTeacherConfig,
) -> list[dict[str, Any]]:
    priors = build_rollout_priors(trajectory_rows, config)
    enhanced_rows: list[dict[str, Any]] = []
    changed_best_count = 0
    prior_hit_count = 0
    legal_count = 0
    total_abs_delta = 0.0
    max_abs_delta = 0.0
    for source_row in counterfactual_rows:
        if source_row.get("policy_id", config.policy_id) != config.policy_id:
            enhanced_rows.append(dict(source_row))
            continue

        row = dict(source_row)
        legal_discards = [int(index) for index in row["legal_discards"]]
        raw_teacher_scores = [float(score) for score in row["teacher_scores"]]
        risk_scores = [float(score) for score in row.get("risk_scores", [0.0] * len(legal_discards))]
        if len(legal_discards) != len(raw_teacher_scores) or len(legal_discards) != len(risk_scores):
            raise ValueError("legal_discards, teacher_scores and risk_scores must have the same length")

        teacher_scores = normalize_scores(raw_teacher_scores) if config.normalize_teacher_scores else raw_teacher_scores
        phase_bucket = str(row.get("phase_bucket", "unknown"))
        risk_bucket = str(row.get("risk_bucket", "unknown"))
        adjusted_scores: list[float] = []
        for tile_index, teacher_score, risk_score, raw_score in zip(
            legal_discards,
            teacher_scores,
            risk_scores,
            raw_teacher_scores,
            strict=True,
        ):
            rollout_prior, has_prior = priors.score(
                phase_bucket,
                risk_bucket,
                tile_index,
                config.min_count,
                config.max_prior_abs,
            )
            prior_hit_count += int(has_prior)
            legal_count += 1
            target_score = (
                config.teacher_logit_weight * teacher_score
                + config.prior_weight * rollout_prior
                - config.safety_weight * risk_score
            )
            delta = target_score - raw_score
            if config.max_score_delta > 0:
                delta = min(max(delta, -config.max_score_delta), config.max_score_delta)
            adjusted_score = raw_score + delta
            delta = adjusted_score - raw_score
            total_abs_delta += abs(delta)
            max_abs_delta = max(max_abs_delta, abs(delta))
            adjusted_scores.append(adjusted_score)

        best_offset = max(range(len(adjusted_scores)), key=adjusted_scores.__getitem__)
        old_best_index = int(row["teacher_best_index"])
        row["teacher_scores"] = adjusted_scores
        row["teacher_best_index"] = legal_discards[best_offset]
        changed_best_count += int(row["teacher_best_index"] != old_best_index)
        enhanced_rows.append(row)

    diagnostics = {
        "prior_weight": config.prior_weight,
        "safety_weight": config.safety_weight,
        "teacher_logit_weight": config.teacher_logit_weight,
        "normalize_teacher_scores": config.normalize_teacher_scores,
        "min_count": config.min_count,
        "max_score_delta": config.max_score_delta,
        "global_return_mean": priors.global_mean,
        "global_return_std": priors.global_std,
        "rows": len(enhanced_rows),
        "changed_best_rate": changed_best_count / max(1, len(enhanced_rows)),
        "prior_hit_rate": prior_hit_count / max(1, legal_count),
        "avg_abs_score_delta": total_abs_delta / max(1, legal_count),
        "max_abs_score_delta": max_abs_delta,
    }
    for row in enhanced_rows:
        if row.get("policy_id", config.policy_id) == config.policy_id:
            row["rollout_teacher"] = diagnostics
    return enhanced_rows


def main() -> None:
    args = parse_args()
    config = RolloutTeacherConfig(
        gamma=args.gamma,
        prior_weight=args.prior_weight,
        safety_weight=args.safety_weight,
        teacher_logit_weight=args.teacher_logit_weight,
        min_count=args.min_count,
        max_score_delta=args.max_score_delta,
        policy_id=args.policy_id,
        normalize_teacher_scores=args.normalize_teacher_scores,
    )
    counterfactual_rows = load_jsonl(args.counterfactual_discards)
    trajectory_rows = load_jsonl(args.trajectories)
    enhanced = enhance_counterfactual_rows(counterfactual_rows, trajectory_rows, config)
    save_jsonl(enhanced, args.output)
    diagnostics = next((row["rollout_teacher"] for row in enhanced if "rollout_teacher" in row), {})
    print(json.dumps({"output": args.output.as_posix(), **diagnostics}, ensure_ascii=False))


if __name__ == "__main__":
    main()
