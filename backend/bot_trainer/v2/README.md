# Mahjong Bot Trainer V2

V2 uses Rust to parse BotZone match records and export backend-native decision samples. Python trains a multi-head model and exports the production ONNX file used by the Rust bot.

## Export Data

```powershell
.\backend\bot_trainer\v2\export_full_dataset.ps1 -ProgressEvery 100
```

```bash
./backend/bot_trainer/v2/export_full_dataset.sh --progress-every 100
```

Small export:

```powershell
.\backend\bot_trainer\v2\export_full_dataset.ps1 -OutputDir backend/bot_trainer/v2/out_smoke -MaxMatches 100 -ProgressEvery 10
```

```bash
./backend/bot_trainer/v2/export_full_dataset.sh --output backend/bot_trainer/v2/out_smoke --max-matches 100 --progress-every 10
```

Default dataset path is `backend/bot_trainer/datasets/data.txt`.

## GPU Training

The training wrapper defaults to `uv run python`, automatic device selection, AMP, and batch size `4096`.

```powershell
.\backend\bot_trainer\v2\train_and_export_model.ps1 -Epochs 20 -BatchSize 4096 -Device cuda -NumWorkers 0
```

```bash
./backend/bot_trainer/v2/train_and_export_model.sh --epochs 20 --batch-size 4096 --device cuda --num-workers 0
```

If VRAM is tight, reduce the batch size to `1024`. If you want CPU training, use `-Device cpu -NoAmp` on Windows or `--device cpu --no-amp` on Linux.

The wrapper writes:

- `backend/bot_trainer/v2/checkpoints/best.pt`
- `backend/bot_trainer/v2/checkpoints/metrics.json`
- `backend/assets/models/mahjong_policy_net.onnx`

After ONNX export, it runs the Rust ONNX smoke test.

## Direct Commands

```powershell
python backend/bot_trainer/v2/train.py --data backend/bot_trainer/v2/out --epochs 20 --batch-size 2048 --output backend/bot_trainer/v2/checkpoints --device cuda --amp
python backend/bot_trainer/v2/export_onnx.py --checkpoint backend/bot_trainer/v2/checkpoints/best.pt --output backend/assets/models/mahjong_policy_net.onnx
```

## Model Outputs

- `discard_logits`: 34 tile logits.
- `claim_logits`: 7 claim logits: pass, hu, pung, kong, chow_left, chow_mid, chow_right.
- `self_kong_logits`: 3 logits: pass, concealed_kong, add_kong.
- `hu_logits`: 2 logits.
- `value`: expected score delta.
- `risk_logits`: 34 tile risk logits.

## Arena Evaluation

Smoke:

```powershell
cargo run --manifest-path backend/Cargo.toml --release --bin bot_arena -- --config backend/bot_trainer/v2/arena_smoke.json --output backend/bot_trainer/v2/arena_smoke.jsonl
```

Trajectory smoke:

```powershell
cargo run --manifest-path backend/Cargo.toml --release --bin bot_arena -- --config backend/bot_trainer/v2/arena_smoke.json --output backend/bot_trainer/v2/arena_smoke.jsonl --trajectories backend/bot_trainer/v2/arena_trajectories_smoke.jsonl
```

Windows matrix:

```powershell
.\backend\bot_trainer\v2\arena_matrix.ps1 -Matches 200 -Seed 20260429
```

Linux matrix:

```bash
MATCHES=200 SEED=20260429 ./backend/bot_trainer/v2/arena_matrix.sh
```

Primary model-selection metrics:

- average score delta
- win rate
- deal-in rate
- first-tenpai turn
- final-tenpai rate
- average decision latency

## RL Training Smoke

Run the full RL pipeline with a small smoke configuration:

```powershell
.\backend\bot_trainer\v2\train_rl_model.ps1 -OutputDir backend/bot_trainer/v2/rl_runs/smoke -TrajectoryMatches 1 -EvalMatches 1 -Epochs 1 -BatchSize 64 -Device cpu
```

```bash
./backend/bot_trainer/v2/train_rl_model.sh --output-dir backend/bot_trainer/v2/rl_runs/smoke --trajectory-matches 1 --eval-matches 1 --epochs 1 --batch-size 64 --device cpu
```

For a normal local run, increase `-TrajectoryMatches` / `--trajectory-matches` and `-EvalMatches` / `--eval-matches` to at least `200`.
If your shell does not resolve the intended tools, pass `-PythonExe` / `--python-exe` or `-CargoExe` / `--cargo-exe` explicitly.
Trajectory generation prints progress every match by default. Use `-TrajectoryProgressEvery 10` or `--trajectory-progress-every 10` for less frequent updates, or `0` to disable script-level arena progress.

Manual PPO training command:

```powershell
python backend/bot_trainer/v2/rl_train.py --trajectories backend/bot_trainer/v2/arena_trajectories_smoke.jsonl --checkpoint backend/bot_trainer/v2/checkpoints/best.pt --epochs 1 --batch-size 64 --output backend/bot_trainer/v2/checkpoints_rl_smoke --device cpu
```

Export a trained RL checkpoint with the same ONNX exporter:

```powershell
python backend/bot_trainer/v2/export_onnx.py --checkpoint backend/bot_trainer/v2/checkpoints_rl_smoke/best.pt --output backend/bot_trainer/v2/checkpoints_rl_smoke/candidate.onnx
```

## RL Candidate Acceptance

An RL candidate can replace the production model only when arena evaluation shows:

- average score delta improves over the current production baseline
- win rate does not regress
- deal-in rate does not increase by more than 2 percentage points
- first-tenpai turn or final-tenpai rate improves, or stays neutral
- average decision latency remains under 100 ms

The first RL runs should keep production policy in `hybrid` mode unless pure neural wins the same arena matrix without higher deal-in rate.
