#!/usr/bin/env python3
"""Policy improvement pipeline (DPO-based).

Per-iteration loop:
  1. Generate league trajectory configs
  2. Collect rollouts (trajectories + counterfactual discards)
  3. Train DPO on counterfactual discard preferences
  4. Export ONNX candidate
  5. Arena matrix evaluation
  6. Candidate gate (selection / promotion)

The offline policy guard has been removed — the arena gate is the sole
quality checkpoint.  This ensures every trained candidate gets a real
evaluated, rather than being silently blocked by a proxy metric.
"""
from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from datetime import datetime
from pathlib import Path

V2_DIR = Path(__file__).resolve().parent / "v2"
if str(V2_DIR) not in sys.path:
    sys.path.insert(0, str(V2_DIR))

from candidate_bank import CandidateRecord, load_candidate_bank, save_candidate_bank


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Policy improvement pipeline (DPO)")
    parser.add_argument("--iterations", type=int, default=3)
    parser.add_argument("--start-iteration", type=int, default=0)
    parser.add_argument("--trajectory-matches", type=int, default=400)
    parser.add_argument("--trajectory-chunk-matches", type=int, default=50)
    parser.add_argument("--matrix-matches", type=int, default=80)
    parser.add_argument("--promotion-matrix-matches", type=int, default=400)
    parser.add_argument("--seed", default=datetime.now().strftime("%Y%m%d"))
    parser.add_argument("--sft-onnx", default="backend/assets/sft/sft.onnx")
    parser.add_argument("--sft-checkpoint", default="backend/bot_trainer/v2/checkpoints/best.pt")
    parser.add_argument("--pool", default="backend/bot_trainer/v2/opponent_pool.json")
    parser.add_argument("--output-dir", default="output/policy_improvement")
    parser.add_argument("--jobs", type=int, default=4)
    # DPO hyperparameters
    parser.add_argument("--dpo-epochs", type=int, default=3)
    parser.add_argument("--dpo-lr", type=float, default=2e-5)
    parser.add_argument("--dpo-beta", type=float, default=0.5)
    parser.add_argument("--dpo-temperature", type=float, default=1.0)
    parser.add_argument("--dpo-kl-coef", type=float, default=0.05)
    parser.add_argument("--dpo-risk-penalty-weight", type=float, default=0.0)
    parser.add_argument("--dpo-expert-source", default="sft_logits")
    return parser.parse_args()


def run(cmd: list[str]) -> subprocess.CompletedProcess:
    print(f"RUN: {' '.join(cmd)}")
    return subprocess.run(cmd, check=True)


def load_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8-sig"))


def save_json(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")


def select_base_checkpoint(sft_checkpoint: Path, accepted_checkpoint: Path | None) -> Path:
    return accepted_checkpoint if accepted_checkpoint is not None else sft_checkpoint


def copy_onnx_bundle(src_onnx: Path, dst_onnx: Path) -> None:
    dst_onnx.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src_onnx, dst_onnx)
    for suffix in (".data", ".manifest.json"):
        src = Path(str(src_onnx) + suffix)
        if src.exists():
            shutil.copy2(src, Path(str(dst_onnx) + suffix))


def update_pool_after_promotion(pool_path: Path, model_path: str, iteration: int) -> None:
    pool = load_json(pool_path)
    promoted = {
        "id": f"promoted_iter_{iteration}",
        "model_path": model_path,
        "sample_actions": True,
        "temperature": 1.0,
        "weight": 2,
    }
    pool.setdefault("learner", {})["model_path"] = model_path
    opponents = [
        opponent
        for opponent in pool.get("rollout_opponents", [])
        if opponent.get("id") not in {promoted["id"], f"selected_iter_{iteration}"}
    ]
    opponents.append(promoted)
    pool["rollout_opponents"] = opponents[-8:]
    save_json(pool_path, pool)


def update_pool_after_selection(pool_path: Path, model_path: str, iteration: int) -> None:
    pool = load_json(pool_path)
    selected = {
        "id": f"selected_iter_{iteration}",
        "model_path": model_path,
        "sample_actions": True,
        "temperature": 1.0,
        "weight": 1,
    }
    opponents = [
        opponent
        for opponent in pool.get("rollout_opponents", [])
        if opponent.get("id") != selected["id"]
    ]
    opponents.append(selected)
    pool["rollout_opponents"] = opponents[-8:]
    save_json(pool_path, pool)


def generate_trajectory_configs(
    pool_path: Path,
    config_dir: Path,
    matches: int,
    chunk_matches: int,
    seed: int,
    rollout_onnx: Path,
) -> None:
    run([
        sys.executable,
        "backend/bot_trainer/v2/league_config.py",
        "--pool",
        str(pool_path),
        "--output-dir",
        str(config_dir),
        "--matches",
        str(matches),
        "--seed",
        str(seed),
        "--mode",
        "trajectory",
        "--trajectory-chunk-matches",
        str(chunk_matches),
        "--rollout-onnx",
        str(rollout_onnx),
    ])


def collect_rollouts(
    config_dir: Path,
    iter_dir: Path,
    jobs: int,
) -> tuple[Path, Path]:
    trajectories = iter_dir / "trajectories.jsonl"
    counterfactual_discards = iter_dir / "counterfactual_discards.jsonl"
    for path in (trajectories, counterfactual_discards):
        if path.exists():
            path.unlink()
    for config_path in sorted(config_dir.glob("trajectory_config_*.json")):
        stem = config_path.stem
        report_path = iter_dir / f"trajectory_summary_{stem}.jsonl"
        chunk_traj = iter_dir / f"trajectories_{stem}.jsonl"
        chunk_cf = iter_dir / f"counterfactual_discards_{stem}.jsonl"
        run([
            "cargo",
            "run",
            "--release",
            "--manifest-path",
            "backend/Cargo.toml",
            "--bin",
            "bot_arena",
            "--",
            "--config",
            str(config_path),
            "--output",
            str(report_path),
            "--trajectories",
            str(chunk_traj),
            "--counterfactual-discards",
            str(chunk_cf),
            "--jobs",
            str(jobs),
        ])
        append_file(chunk_traj, trajectories)
        append_file(chunk_cf, counterfactual_discards)
    return trajectories, counterfactual_discards


def append_file(src: Path, dst: Path) -> None:
    if not src.exists():
        return
    dst.parent.mkdir(parents=True, exist_ok=True)
    with src.open("rb") as reader, dst.open("ab") as writer:
        shutil.copyfileobj(reader, writer)


def train_dpo(
    base_checkpoint: Path,
    counterfactual_discards: Path,
    iter_dir: Path,
    args: argparse.Namespace,
) -> Path:
    dpo_checkpoint = iter_dir / "dpo_best.pt"
    run([
        sys.executable,
        "backend/bot_trainer/v2/train_dpo.py",
        "--counterfactual-discards",
        str(counterfactual_discards),
        "--checkpoint",
        str(base_checkpoint),
        "--output",
        str(dpo_checkpoint),
        "--epochs",
        str(args.dpo_epochs),
        "--lr",
        str(args.dpo_lr),
        "--beta",
        str(args.dpo_beta),
        "--temperature",
        str(args.dpo_temperature),
        "--kl-coef",
        str(args.dpo_kl_coef),
        "--risk-penalty-weight",
        str(args.dpo_risk_penalty_weight),
        "--expert-source",
        args.dpo_expert_source,
        "--policy-id",
        "learner",
    ])
    return dpo_checkpoint


def export_candidate(checkpoint: Path, output: Path) -> None:
    run([
        sys.executable,
        "backend/bot_trainer/v2/export_onnx.py",
        "--checkpoint",
        str(checkpoint),
        "--output",
        str(output),
    ])


def matrix_eval(
    pool_path: Path,
    candidate_onnx: Path,
    iter_dir: Path,
    args: argparse.Namespace,
    seed: int,
    *,
    matches: int,
    label: str,
    config_dir_name: str,
) -> list[Path]:
    config_dir = iter_dir / config_dir_name
    run([
        sys.executable,
        "backend/bot_trainer/v2/league_config.py",
        "--pool",
        str(pool_path),
        "--output-dir",
        str(config_dir),
        "--matches",
        str(matches),
        "--seed",
        str(seed),
        "--mode",
        "matrix",
        "--candidate-onnx",
        str(candidate_onnx),
        "--baseline-onnx",
        args.sft_onnx,
    ])
    summaries: list[Path] = []
    for config_path in sorted(config_dir.glob("matrix_config_*.json")):
        result_path = iter_dir / f"{label}_matrix_result_{config_path.stem}.jsonl"
        summary_path = iter_dir / f"{label}_matrix_summary_{config_path.stem}.json"
        run([
            "cargo",
            "run",
            "--release",
            "--manifest-path",
            "backend/Cargo.toml",
            "--bin",
            "bot_arena",
            "--",
            "--config",
            str(config_path),
            "--output",
            str(result_path),
            "--jobs",
            str(args.jobs),
        ])
        run([
            sys.executable,
            "backend/bot_trainer/v2/arena_summary.py",
            "--input",
            str(result_path),
            "--output",
            str(summary_path),
        ])
        summaries.append(summary_path)
    return summaries


def run_gate(mode: str, summaries: list[Path], pool_path: Path, output: Path) -> bool:
    try:
        run([
            sys.executable,
            "backend/bot_trainer/v2/candidate_gate.py",
            "--summary",
            *[str(path) for path in summaries],
            "--pool",
            str(pool_path),
            "--gate-mode",
            mode,
            "--output",
            str(output),
        ])
        return True
    except subprocess.CalledProcessError:
        return False


def main() -> None:
    args = parse_args()
    repo_root = Path(__file__).resolve().parent.parent.parent
    os.chdir(repo_root)
    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    pool_path = output_dir / "working_pool.json"
    if not pool_path.exists():
        shutil.copy2(args.pool, pool_path)
    bank_path = output_dir / "candidate_bank.json"
    candidate_bank = load_candidate_bank(bank_path)
    accepted_checkpoint: Path | None = None
    accepted_onnx = Path(args.sft_onnx)
    seed_base = int(args.seed)

    for iteration in range(args.start_iteration, args.start_iteration + args.iterations):
        iter_dir = output_dir / f"iter_{iteration}"
        iter_dir.mkdir(parents=True, exist_ok=True)
        iter_seed = seed_base + iteration * 1000
        base_checkpoint = select_base_checkpoint(Path(args.sft_checkpoint), accepted_checkpoint)
        generate_trajectory_configs(
            pool_path,
            iter_dir / "configs",
            args.trajectory_matches,
            args.trajectory_chunk_matches,
            iter_seed,
            accepted_onnx,
        )
        trajectories, counterfactual_discards = collect_rollouts(iter_dir / "configs", iter_dir, args.jobs)
        run([
            sys.executable,
            "backend/bot_trainer/v2/bucket_report.py",
            "--trajectories",
            str(trajectories),
            "--output",
            str(iter_dir / "bucket_report.json"),
        ])
        dpo_checkpoint = train_dpo(base_checkpoint, counterfactual_discards, iter_dir, args)
        candidate_onnx = iter_dir / "candidate.onnx"
        export_candidate(dpo_checkpoint, candidate_onnx)
        summaries = matrix_eval(
            pool_path,
            candidate_onnx,
            iter_dir,
            args,
            iter_seed + 500,
            matches=args.matrix_matches,
            label="selection",
            config_dir_name="selection_matrix_config",
        )
        selection_path = iter_dir / "selection_result.json"
        promotion_path = iter_dir / "promotion_result.json"
        selected = run_gate("selection", summaries, pool_path, selection_path)
        promoted = False
        promotion_summaries: list[Path] = []
        if selected:
            promotion_summaries = matrix_eval(
                pool_path,
                candidate_onnx,
                iter_dir,
                args,
                iter_seed + 1500,
                matches=args.promotion_matrix_matches,
                label="promotion",
                config_dir_name="promotion_matrix_config",
            )
            promoted = run_gate("promotion", promotion_summaries, pool_path, promotion_path)

        if selected and selection_path.exists():
            selected_onnx = Path("backend/assets/league") / f"selected_iter_{iteration}" / "policy.onnx"
            copy_onnx_bundle(candidate_onnx, selected_onnx)
            update_pool_after_selection(pool_path, selected_onnx.as_posix(), iteration)
            candidate_bank.add(CandidateRecord(
                iter=iteration,
                checkpoint=dpo_checkpoint.as_posix(),
                onnx=selected_onnx.as_posix(),
                gate_result=load_json(selection_path),
                selected=True,
                promoted=promoted,
            ))
            save_candidate_bank(bank_path, candidate_bank)
        if promoted:
            stable_onnx = Path("backend/assets/league") / f"iter_{iteration}" / "policy.onnx"
            copy_onnx_bundle(candidate_onnx, stable_onnx)
            update_pool_after_promotion(pool_path, stable_onnx.as_posix(), iteration)
            accepted_checkpoint = dpo_checkpoint
            accepted_onnx = stable_onnx


if __name__ == "__main__":
    main()
