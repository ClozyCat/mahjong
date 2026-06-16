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


def sampled_policy(policy: dict[str, Any], *, display_name: str | None = None) -> dict[str, Any]:
    result = clean_policy(policy)
    if display_name is None:
        result.pop("display_name", None)
    else:
        result["display_name"] = display_name
    result["sample_actions"] = True
    result.setdefault("temperature", 1.0)
    return result


def fallback_opponents_from_policy(
    policy: dict[str, Any],
    *,
    id_prefix: str | None = None,
    sample_actions: bool,
) -> list[dict[str, Any]]:
    opponents = []
    for index in range(3):
        opponent = clean_policy(policy)
        opponent.pop("display_name", None)
        if id_prefix is not None:
            opponent["id"] = f"{id_prefix}-{index + 1}"
        opponent["sample_actions"] = sample_actions
        opponent.setdefault("temperature", 1.0)
        opponents.append(opponent)
    return opponents


def normalize_opponents(
    pool: dict[str, Any],
    fallback_policy: dict[str, Any],
    *,
    fallback_id_prefix: str | None,
    sample_actions: bool,
) -> list[dict[str, Any]]:
    raw_opponents = [clean_policy(opponent) for opponent in pool.get("opponents", [])]
    if not raw_opponents:
        return fallback_opponents_from_policy(
            fallback_policy,
            id_prefix=fallback_id_prefix,
            sample_actions=sample_actions,
        )
    opponents = []
    for index in range(3):
        opponent = dict(raw_opponents[index % len(raw_opponents)])
        opponent.pop("display_name", None)
        opponent["sample_actions"] = sample_actions
        opponent.setdefault("temperature", 1.0)
        opponents.append(opponent)
    return opponents


def build_trajectory_configs(
    pool: dict[str, Any],
    matches: int,
    seed: int,
    max_actions: int,
) -> list[dict[str, Any]]:
    learner = sampled_policy(pool["learner"], display_name="Learner")
    opponents = normalize_opponents(
        pool,
        learner,
        fallback_id_prefix=None,
        sample_actions=True,
    )

    return [
        {
            "matches": matches,
            "seed": seed,
            "max_actions_per_match": max_actions,
            "report_trajectories": True,
            "subjects": [learner],
            "opponents": opponents,
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
    baseline_policy = {
        "id": "baseline_neural",
        "model_path": model_path_text(baseline_onnx),
        "sample_actions": False,
        "temperature": 1.0,
    }
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
        "opponents": normalize_opponents(
            pool,
            baseline_policy,
            fallback_id_prefix="baseline-opponent",
            sample_actions=False,
        ),
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
