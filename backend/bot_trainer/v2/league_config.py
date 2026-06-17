from __future__ import annotations

import argparse
import json
import random
from pathlib import Path
from typing import Any


def model_path_text(path: Path) -> str:
    return path.as_posix()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--pool", type=Path, default=None)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--matches", type=int, required=True)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--max-actions", type=int, default=2400)
    parser.add_argument("--mode", choices=["trajectory", "eval", "matrix"], default="trajectory")
    parser.add_argument("--rollout-onnx", type=Path, default=None)
    parser.add_argument("--candidate-onnx", type=Path, default=None)
    parser.add_argument(
        "--baseline-onnx",
        type=Path,
        default=Path("backend/assets/sft/sft.onnx"),
    )
    return parser.parse_args()


def load_pool(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def clean_policy(policy: dict[str, Any]) -> dict[str, Any]:
    return {key: value for key, value in policy.items() if key != "weight"}


def weighted_sample(
    pool: list[dict[str, Any]],
    count: int,
    rng: random.Random,
) -> list[dict[str, Any]]:
    """Sample `count` items from pool with replacement, weighted by `weight` field."""
    if not pool:
        return []
    items = list(pool)
    weights = [float(item.get("weight", 1.0)) for item in items]
    total = sum(weights)
    if total <= 0:
        return [items[i % len(items)] for i in range(count)]
    chosen: list[dict[str, Any]] = []
    for _ in range(count):
        r = rng.random() * total
        cumulative = 0.0
        for item, w in zip(items, weights, strict=True):
            cumulative += w
            if r < cumulative:
                chosen.append(item)
                break
    return chosen


def default_neural_opponents(
    model_path: Path | str,
    *,
    prefix: str,
    sample_actions: bool,
    temperature: float = 1.0,
) -> list[dict[str, Any]]:
    path_text = model_path_text(model_path if isinstance(model_path, Path) else Path(model_path))
    return [
        {
            "id": f"{prefix}-{index + 1}",
            "model_path": path_text,
            "sample_actions": sample_actions,
            "temperature": temperature,
        }
        for index in range(3)
    ]


def repeated_opponents(policy: dict[str, Any], prefix: str) -> list[dict[str, Any]]:
    clean = clean_policy(policy)
    return [
        {
            **clean,
            "id": f"{prefix}-{index + 1}",
        }
        for index in range(3)
    ]


def build_trajectory_configs(
    pool: dict[str, Any],
    matches: int,
    seed: int,
    max_actions: int,
) -> list[dict[str, Any]]:
    learner = clean_policy(pool["learner"])
    learner["sample_actions"] = True
    learner.setdefault("temperature", 1.0)
    learner_subject = {**learner, "display_name": "Learner"}

    opponents_pool = pool.get("rollout_opponents") or pool.get("opponents") or []
    match_rng = random.Random(seed + 1)
    if opponents_pool:
        chosen = weighted_sample(opponents_pool, 3, match_rng)
        opponents = [clean_policy(o) for o in chosen]
    else:
        opponents = [clean_policy(learner) for _ in range(3)]
    return [{
        "matches": matches,
        "seed": seed,
        "max_actions_per_match": max_actions,
        "report_trajectories": True,
        "subjects": [learner_subject],
        "opponents": opponents,
    }]


def build_eval_config(
    pool: dict[str, Any],
    candidate_onnx: Path,
    baseline_onnx: Path,
    matches: int,
    seed: int,
    max_actions: int,
) -> dict[str, Any]:
    return {
        "matches": matches,
        "seed": seed,
        "max_actions_per_match": max_actions,
        "report_trajectories": False,
        "subjects": [
            {
                "id": "baseline_neural",
                "display_name": "Baseline",
                "model_path": model_path_text(baseline_onnx),
                "sample_actions": False,
                "temperature": 1.0,
            },
            {
                "id": "awr_candidate_neural",
                "display_name": "AWR candidate",
                "model_path": model_path_text(candidate_onnx),
                "sample_actions": False,
                "temperature": 1.0,
            },
        ],
        "opponents": default_neural_opponents(
            baseline_onnx,
            prefix="baseline-opponent",
            sample_actions=False,
        ),
    }


def apply_rollout_model_override(pool: dict[str, Any], rollout_onnx: Path | None) -> None:
    if rollout_onnx is None:
        return
    learner = pool["learner"]
    learner["model_path"] = model_path_text(rollout_onnx)


def build_matrix_configs(
    pool: dict[str, Any],
    candidate_onnx: Path,
    baseline_onnx: Path,
    matches: int,
    seed: int,
    max_actions: int,
) -> list[dict[str, Any]]:
    opponents_pool = pool.get("rollout_opponents", [])
    if not opponents_pool:
        raise ValueError("--mode matrix requires rollout_opponents in pool")
    configs: list[dict[str, Any]] = []
    for i, opp in enumerate(opponents_pool):
        opp_clean = clean_policy(opp)
        subjects: list[dict[str, Any]] = [
            {
                "id": "baseline_neural",
                "display_name": "Baseline",
                "model_path": model_path_text(baseline_onnx),
                "sample_actions": False,
                "temperature": 1.0,
            },
            {
                "id": "awr_candidate_neural",
                "display_name": "AWR Candidate",
                "model_path": model_path_text(candidate_onnx),
                "sample_actions": False,
                "temperature": 1.0,
            },
        ]
        configs.append({
            "matches": matches,
            "seed": seed + i * 1000,
            "max_actions_per_match": max_actions,
            "report_trajectories": False,
            "subjects": subjects,
            "opponents": repeated_opponents(opp_clean, f"{opp['id']}-opponent"),
        })
    return configs


def write_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")


def main() -> None:
    args = parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    if args.mode == "trajectory":
        if args.pool is None:
            raise SystemExit("--pool is required for trajectory mode")
        pool = load_pool(args.pool)
        apply_rollout_model_override(pool, args.rollout_onnx)
        for index, config in enumerate(
            build_trajectory_configs(
                pool,
                args.matches,
                args.seed,
                args.max_actions,
            )
        ):
            write_json(args.output_dir / f"trajectory_config_{index}.json", config)
    elif args.mode == "matrix":
        if args.pool is None:
            raise SystemExit("--pool is required for matrix mode")
        if args.candidate_onnx is None:
            raise SystemExit("--candidate-onnx is required for matrix mode")
        pool = load_pool(args.pool)
        for index, config in enumerate(
            build_matrix_configs(
                pool,
                args.candidate_onnx,
                args.baseline_onnx,
                args.matches,
                args.seed,
                args.max_actions,
            )
        ):
            write_json(args.output_dir / f"matrix_config_{index}.json", config)
    else:
        if args.pool is None:
            raise SystemExit("--pool is required for eval mode")
        if args.candidate_onnx is None:
            raise SystemExit("--candidate-onnx is required for eval mode")
        pool = load_pool(args.pool)
        write_json(
            args.output_dir / "candidate_eval_config.json",
            build_eval_config(
                pool,
                args.candidate_onnx,
                args.baseline_onnx,
                args.matches,
                args.seed,
                args.max_actions,
            ),
        )


if __name__ == "__main__":
    main()
