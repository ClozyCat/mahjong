from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def model_path_text(path: Path) -> str:
    return path.as_posix()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--pool", type=Path, required=True)
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
        default=Path("backend/assets/models/mahjong_policy_net.onnx"),
    )
    parser.add_argument("--record-heuristic-comparison", action="store_true")
    return parser.parse_args()


def load_pool(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def clean_policy(policy: dict[str, Any]) -> dict[str, Any]:
    return {key: value for key, value in policy.items() if key != "weight"}


def weighted_opponents(pool: dict[str, Any]) -> list[dict[str, Any]]:
    opponents: list[dict[str, Any]] = []
    for opponent in pool["opponents"]:
        weight = max(1, int(opponent.get("weight", 1)))
        clean = clean_policy(opponent)
        opponents.extend(dict(clean) for _ in range(weight))
    if not opponents:
        raise ValueError("opponent pool must contain at least one opponent")
    return opponents


def build_trajectory_configs(
    pool: dict[str, Any],
    matches: int,
    seed: int,
    max_actions: int,
    record_heuristic_comparison: bool = False,
) -> list[dict[str, Any]]:
    learner = clean_policy(pool["learner"])
    opponents = weighted_opponents(pool)
    matches_per_config = max(1, matches // 4)
    configs = []
    for learner_seat in range(4):
        policies = []
        opponent_index = learner_seat
        for seat in range(4):
            if seat == learner_seat:
                policies.append(dict(learner))
            else:
                policies.append(dict(opponents[opponent_index % len(opponents)]))
                opponent_index += 1
        configs.append(
            {
                "matches": matches_per_config,
                "seed": seed + learner_seat * 100000,
                "max_actions_per_match": max_actions,
                "report_trajectories": True,
                "record_heuristic_comparison": record_heuristic_comparison,
                "policies": policies,
            }
        )
    return configs


def build_eval_config(
    candidate_onnx: Path,
    baseline_onnx: Path,
    matches: int,
    seed: int,
    max_actions: int,
    record_heuristic_comparison: bool = False,
) -> dict[str, Any]:
    return {
        "matches": matches,
        "seed": seed,
        "max_actions_per_match": max_actions,
        "report_trajectories": False,
        "record_heuristic_comparison": record_heuristic_comparison,
        "seat_rotation": "cyclic",
        "seat_rotation_offset": 0,
        "policies": [
            {
                "id": "baseline_neural",
                "mode": "neural",
                "model_path": model_path_text(baseline_onnx),
                "sample_actions": False,
                "temperature": 1.0,
            },
            {
                "id": "rl_candidate_neural",
                "mode": "neural",
                "model_path": model_path_text(candidate_onnx),
                "sample_actions": False,
                "temperature": 1.0,
            },
        ],
    }


def apply_rollout_model_override(pool: dict[str, Any], rollout_onnx: Path | None) -> None:
    if rollout_onnx is None:
        return
    learner = pool["learner"]
    if learner.get("mode") == "neural":
        learner["model_path"] = model_path_text(rollout_onnx)


def write_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")


def main() -> None:
    args = parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    if args.mode == "trajectory":
        pool = load_pool(args.pool)
        apply_rollout_model_override(pool, args.rollout_onnx)
        for index, config in enumerate(
            build_trajectory_configs(
                pool,
                args.matches,
                args.seed,
                args.max_actions,
                args.record_heuristic_comparison,
            )
        ):
            write_json(args.output_dir / f"trajectory_config_{index}.json", config)
    else:
        if args.candidate_onnx is None:
            raise SystemExit("--candidate-onnx is required for eval mode")
        write_json(
            args.output_dir / "candidate_eval_config.json",
            build_eval_config(
                args.candidate_onnx,
                args.baseline_onnx,
                args.matches,
                args.seed,
                args.max_actions,
                args.record_heuristic_comparison,
            ),
        )


if __name__ == "__main__":
    main()
