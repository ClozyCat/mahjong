from __future__ import annotations

import argparse
import json
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
    parser.add_argument("--mode", choices=["trajectory", "eval"], default="trajectory")
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


def build_trajectory_configs(
    pool: dict[str, Any],
    matches: int,
    seed: int,
    max_actions: int,
) -> list[dict[str, Any]]:
    learner = clean_policy(pool["learner"])
    learner.setdefault("display_name", "Learner")
    learner["sample_actions"] = True
    learner.setdefault("temperature", 1.0)

    subjects = [
        {**learner, "display_name": f"Learner_{i}"}
        for i in range(4)
    ]

    return [
        {
            "matches": matches,
            "seed": seed,
            "max_actions_per_match": max_actions,
            "report_trajectories": True,
            "subjects": subjects,
            "opponents": [],
        }
    ]


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
        "opponents": [],
    }


def apply_rollout_model_override(pool: dict[str, Any], rollout_onnx: Path | None) -> None:
    if rollout_onnx is None:
        return
    learner = pool["learner"]
    learner["model_path"] = model_path_text(rollout_onnx)


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
