#!/usr/bin/env python3
"""AWR/AWAC offline policy improvement pipeline — replaces train_awr_model.ps1."""

from __future__ import annotations

import argparse
import json
import os
import random
import shutil
import subprocess
import sys
from datetime import datetime
from pathlib import Path


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="AWR Training Pipeline")
    p.add_argument("--iterations", type=int, default=3)
    p.add_argument("--trajectory-matches", type=int, default=100)
    p.add_argument("--matrix-matches", type=int, default=20)
    p.add_argument("--seed", default=datetime.now().strftime("%Y%m%d"))
    p.add_argument("--sft-onnx", default="backend/assets/sft/sft.onnx")
    p.add_argument("--sft-checkpoint", default="backend/bot_trainer/v2/checkpoints/best.pt")
    p.add_argument("--output-dir", default="output/awr_training")
    p.add_argument("--pool", default="backend/bot_trainer/v2/opponent_pool.json")
    p.add_argument("--jobs", type=int, default=4)
    return p.parse_args()


def run(cmd: list[str], **kw) -> subprocess.CompletedProcess:
    print(f"  RUN: {' '.join(cmd)}")
    return subprocess.run(cmd, check=True, **kw)


def load_json(path: Path | str) -> dict:
    with open(path, encoding="utf-8-sig") as f:
        return json.load(f)


def save_json(path: Path | str, data: dict) -> None:
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)
        f.write("\n")


def update_league_pool(
    pool_path: str,
    output_dir: str,
    history: list[dict],
    iter_num: int,
    accepted: bool,
    rng: random.Random,
) -> None:
    """Rotate opponent pool: 1 SFT baseline + 1 required recent AWR + weighted random, cap at 6."""
    orig = load_json(pool_path)

    # SFT base variants
    sft_base = [o for o in orig["rollout_opponents"] if o["id"].startswith("sft_")]
    if not sft_base:
        sft_base = [
            {"id": "sft_cold", "model_path": "backend/assets/sft/sft.onnx", "sample_actions": True, "temperature": 0.5, "weight": 1},
            {"id": "sft_warm", "model_path": "backend/assets/sft/sft.onnx", "sample_actions": True, "temperature": 1.0, "weight": 2},
            {"id": "sft_hot",  "model_path": "backend/assets/sft/sft.onnx", "sample_actions": True, "temperature": 2.0, "weight": 1},
        ]

    # AWR league entries with recency weight
    awr_pool = []
    for i, h in enumerate(history):
        recency = len(history) - i  # more recent = higher weight
        temps = [0.5, 1.0, 1.5]
        awr_pool.append({
            "id": f"awr_iter_{h['iter']}",
            "model_path": h["model_path"],
            "sample_actions": True,
            "temperature": temps[h["iter"] % len(temps)],
            "weight": max(1, recency),
        })

    candidates = list(sft_base) + awr_pool
    max_slots = 6

    # 1 random SFT baseline
    sft_slot = rng.choice(sft_base)

    # Must include current iteration if accepted
    must_include = []
    current_id = f"awr_iter_{iter_num}"
    if accepted:
        must_include = [a for a in awr_pool if a["id"] == current_id]

    remaining = max_slots - 1 - len(must_include)
    if remaining < 0:
        remaining = 0

    # Weighted random from remaining candidates
    others = [c for c in candidates if c["id"] != sft_slot["id"] and c["id"] != current_id]
    weighted = []
    for o in others:
        weighted.extend([o] * int(o.get("weight", 1)))
    rng.shuffle(weighted)

    selected_others = {}
    for s in weighted:
        if len(selected_others) >= remaining:
            break
        selected_others[s["id"]] = s

    new_opponents = [sft_slot] + must_include + list(selected_others.values())
    orig["rollout_opponents"] = new_opponents

    # Update learner model if this iteration was accepted
    if accepted:
        league_onnx = f"backend/assets/league/iter_{iter_num}/awr.onnx"
        orig["learner"]["model_path"] = league_onnx

    save_json(pool_path, orig)
    print(f"  League pool: 1 SFT + {len(must_include)} required + {len(selected_others)} random = {len(new_opponents)} total"
          + (f" (learner updated: {league_onnx})" if accepted else ""))


def main() -> None:
    args = parse_args()
    seed_base = int(args.seed)
    rng = random.Random(seed_base)

    repo_root = Path(__file__).resolve().parent.parent.parent
    os.chdir(repo_root)
    print(f"=== AWR Training Pipeline ===")
    print(f"Iterations: {args.iterations}, TrajectoryMatches: {args.trajectory_matches}, Seed: {args.seed}")
    print(f"Working dir: {repo_root}")

    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    # Copy pool to working copy so original stays pristine
    pool_path = output_dir / "working_pool.json"
    if not pool_path.exists():
        shutil.copy2(args.pool, pool_path)
        print(f"Copied pool: {args.pool} -> {pool_path}")
    pool_path = str(pool_path)

    history: list[dict] = []

    for iter_num in range(args.iterations):
        print(f"\n--- Iteration {iter_num + 1}/{args.iterations} ---")
        iter_seed = seed_base + iter_num * 1000

        # 1. Generate trajectories
        print("[1/4] Generating trajectories...")
        config_dir = output_dir / f"iter_{iter_num}" / "configs"
        config_dir.mkdir(parents=True, exist_ok=True)

        traj_cmd = [
            sys.executable, "backend/bot_trainer/v2/league_config.py",
            "--pool", pool_path,
            "--output-dir", str(config_dir),
            "--matches", str(args.trajectory_matches),
            "--seed", str(iter_seed),
            "--mode", "trajectory",
        ]
        # Only override with SFT for iter 0 (before any AWR model exists)
        if iter_num == 0:
            traj_cmd += ["--rollout-onnx", args.sft_onnx]
        run(traj_cmd)

        traj_out = output_dir / f"iter_{iter_num}" / "trajectories.jsonl"
        if traj_out.exists():
            traj_out.unlink()

        config_files = sorted(config_dir.glob("trajectory_config_*.json"))
        for cf in config_files:
            chunk_summary = output_dir / f"iter_{iter_num}" / f"trajectory_summary_{cf.stem}.json"
            chunk_traj = output_dir / f"iter_{iter_num}" / f"trajectories_{cf.stem}.jsonl"
            run([
                "cargo", "run", "--release", "--manifest-path", "backend/Cargo.toml",
                "--bin", "bot_arena", "--",
                "--config", str(cf),
                "--output", str(chunk_summary),
                "--trajectories", str(chunk_traj),
                "--jobs", str(args.jobs),
            ])
            if chunk_traj.exists():
                with open(traj_out, "ab") as dst:
                    with open(chunk_traj, "rb") as src:
                        shutil.copyfileobj(src, dst)

        if not traj_out.exists():
            print(f"ERROR: Trajectory generation failed for iteration {iter_num}", file=sys.stderr)
            sys.exit(1)

        # 2. Value pretraining
        print("[2/4] Pretraining value head...")
        value_ckpt = output_dir / f"iter_{iter_num}" / "value_pretrained.pt"

        sft_ckpt = Path(args.sft_checkpoint)
        if not sft_ckpt.exists():
            for alt in ["output/sft/best.pt", "output/sft_checkpoints/best.pt"]:
                if Path(alt).exists():
                    sft_ckpt = Path(alt)
                    break
            else:
                print(f"ERROR: SFT checkpoint not found", file=sys.stderr)
                sys.exit(1)

        run([
            sys.executable, "backend/bot_trainer/v2/train_value.py",
            "--trajectories", str(traj_out),
            "--checkpoint", str(sft_ckpt),
            "--output", str(value_ckpt),
            "--batch-size", "512",
            "--lr", "1e-3",
            "--policy-id", "learner",
        ])

        # 3. AWR training
        print("[3/4] Running AWR training...")
        awr_dir = output_dir / f"iter_{iter_num}" / "awr_checkpoints"
        run([
            sys.executable, "backend/bot_trainer/v2/train_awr.py",
            "--trajectories", str(traj_out),
            "--checkpoint", str(value_ckpt),
            "--output-dir", str(awr_dir),
            "--epochs", "5",
            "--value-finetune-epochs", "3",
            "--batch-size", "512",
            "--lr", "3e-5",
            "--temperature", "0.5",
            "--sft-checkpoint", str(sft_ckpt),
            "--policy-id", "learner",
        ])

        # 4. Export ONNX
        print("[4/4] Exporting AWR ONNX...")
        awr_onnx = output_dir / f"iter_{iter_num}" / "awr.onnx"
        run([
            sys.executable, "backend/bot_trainer/v2/export_onnx.py",
            "--checkpoint", str(awr_dir / "awr_best.pt"),
            "--output", str(awr_onnx),
        ])

        # 5. Matrix evaluation
        print("Evaluating candidate vs multiple opponents...")
        matrix_config_dir = output_dir / f"iter_{iter_num}" / "matrix_config"
        run([
            sys.executable, "backend/bot_trainer/v2/league_config.py",
            "--pool", pool_path,
            "--output-dir", str(matrix_config_dir),
            "--matches", str(args.matrix_matches),
            "--seed", str(iter_seed + 2),
            "--mode", "matrix",
            "--candidate-onnx", str(awr_onnx),
            "--baseline-onnx", args.sft_onnx,
        ])

        matrix_summaries = []
        for cf in sorted(matrix_config_dir.glob("matrix_config_*.json")):
            result_file = output_dir / f"iter_{iter_num}" / f"matrix_result_{cf.stem}.jsonl"
            summary_file = output_dir / f"iter_{iter_num}" / f"matrix_summary_{cf.stem}.json"
            run([
                "cargo", "run", "--release", "--manifest-path", "backend/Cargo.toml",
                "--bin", "bot_arena", "--",
                "--config", str(cf),
                "--output", str(result_file),
            ])
            if result_file.exists():
                run([
                    sys.executable, "backend/bot_trainer/v2/arena_summary.py",
                    "--input", str(result_file),
                    "--output", str(summary_file),
                ])
                matrix_summaries.append(str(summary_file))

        # 6. Candidate gate
        gate_result = output_dir / f"iter_{iter_num}" / "gate_result.json"
        gate_exit = 0
        try:
            run([
                sys.executable, "backend/bot_trainer/v2/candidate_gate.py",
                "--summary", *matrix_summaries,
                "--pool", pool_path,
                "--output", str(gate_result),
            ])
        except subprocess.CalledProcessError as e:
            gate_exit = e.returncode

        accepted = gate_exit == 0
        if accepted:
            print(f">>> ITERATION {iter_num} : CANDIDATE ACCEPTED <<<")
        else:
            print(f">>> ITERATION {iter_num} : CANDIDATE REJECTED <<<")

        # 7. League pool update
        if accepted:
            league_dir = Path(f"backend/assets/league/iter_{iter_num}")
            league_dir.mkdir(parents=True, exist_ok=True)
            # Keep original filename so embedded .data path stays valid
            stable_onnx = league_dir / "awr.onnx"
            shutil.copy2(awr_onnx, stable_onnx)
            for suffix in [".data", ".manifest.json"]:
                src = Path(str(awr_onnx) + suffix)
                if src.exists():
                    shutil.copy2(src, league_dir / f"awr.onnx{suffix}")
            history.append({
                "iter": iter_num,
                "model_path": str(stable_onnx.as_posix()),
            })
            print(f"  Added to league pool ({stable_onnx})")

        if history:
            update_league_pool(pool_path, str(output_dir), history, iter_num, accepted, rng)

    print(f"\nAWR training pipeline complete. Output: {args.output_dir}")


if __name__ == "__main__":
    main()
