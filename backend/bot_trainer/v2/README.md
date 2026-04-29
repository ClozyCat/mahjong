# Mahjong Bot Trainer V2

V2 uses Rust to parse BotZone match records and export backend-native decision samples. Python trains a multi-head ResNet policy/value model and exports the production ONNX file used by the Rust bot.

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

The training wrapper defaults to the local `python` executable, automatic device selection, AMP, and batch size `4096`.

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

## Model Architecture

The policy keeps the existing ONNX contract:

- inputs: `tile_planes` shaped `batch x 10 x 34`, `scalar_features` shaped `batch x 10`
- outputs: `discard_logits`, `claim_logits`, `self_kong_logits`, `hu_logits`, `value`, `risk_logits`

The tile encoder is suit-aware: 万/条/筒 each pass through a shared 1D residual convolution encoder over rank order, while honors use a separate encoder. This preserves local sequence structure without letting convolutions treat suit boundaries such as `w9 -> t1` as adjacent ranks.

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
.\backend\bot_trainer\v2\arena_matrix.ps1 -Matches 200 -Seed 20260429 -SeatOrder rotate1
```

Linux matrix:

```bash
MATCHES=200 SEED=20260429 ./backend/bot_trainer/v2/arena_matrix.sh
MATCHES=200 SEED=20260429 SEAT_ORDER=rotate1 ./backend/bot_trainer/v2/arena_matrix.sh
```

`arena_matrix` compares `heuristic` and `neural` policies only. `SeatOrder=default` seats them as
`heuristic,neural,heuristic,neural`; `rotate1` swaps the order.

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
RL starts from a supervised checkpoint. By default the scripts expect `backend/bot_trainer/v2/checkpoints/best.pt` and `backend/assets/models/mahjong_policy_net.onnx`; pass `-BaselineCheckpoint` / `--baseline-checkpoint` and `-BaselineOnnx` / `--baseline-onnx` if your baseline files are elsewhere.
Trajectory generation prints progress every 20 matches by default. Use `-TrajectoryProgressEvery 10` or `--trajectory-progress-every 10` for more frequent updates, or `0` to disable script-level arena progress.
Arena self-play and candidate evaluation run in parallel by default from these scripts. Use `-ArenaJobs 4` or `--arena-jobs 4` to pin worker count; `0` means all available cores.
The script prints PPO epoch losses during `rl_train.py`, then prints an arena summary after candidate evaluation and writes `candidate_eval_summary.json`.

### PPO League Training

```powershell
.\backend\bot_trainer\v2\train_rl_model.ps1 `
  -OutputDir backend/bot_trainer/v2/rl_runs/league_smoke `
  -TrajectoryMatches 8 `
  -EvalMatches 4 `
  -Epochs 1 `
  -BatchSize 64 `
  -Device cpu `
  -LearnerPolicyId learner `
  -GaeLambda 0.95 `
  -KlCoef 0.01
```

```bash
./backend/bot_trainer/v2/train_rl_model.sh \
  --output-dir backend/bot_trainer/v2/rl_runs/league_smoke \
  --trajectory-matches 8 \
  --eval-matches 4 \
  --epochs 1 \
  --batch-size 64 \
  --device cpu \
  --learner-policy-id learner \
  --gae-lambda 0.95 \
  --kl-coef 0.01
```

The generated trajectory configs rotate the sampled `learner` policy through all four seats and fill the other seats from `opponent_pool.json`. PPO filters rows by `policy_id=learner`, so frozen opponents do not train the learner.

Manual PPO training command:

```powershell
python backend/bot_trainer/v2/rl_train.py --trajectories backend/bot_trainer/v2/arena_trajectories_smoke.jsonl --checkpoint backend/bot_trainer/v2/checkpoints/best.pt --epochs 1 --batch-size 64 --output backend/bot_trainer/v2/checkpoints_rl_smoke --device cpu --entropy-coef 0.02 --entropy-end-coef 0.005
```

RL uses linear entropy decay. `--entropy-coef` is the starting exploration weight and `--entropy-end-coef` is the final weight. If `--entropy-decay-steps` is omitted or `0`, decay spans the full training run. Epoch logs and `rl_metrics.json` include average `entropy` and `entropy_coef` so collapse can be spotted during training.

If you train from old trajectories that contain placeholder `log_prob=0` and `value=0`, pass `--recompute-old-policy-stats` with the rollout checkpoint. New neural-backed arena trajectories emit old log-prob/value directly.

Arena trajectory rows now split reward fields:

- `step_reward`: small fan-aware shanten shaping reward
- `terminal_reward`: score/win/deal-in result on the seat's final row
- `reward`: `step_reward`, plus `terminal_reward` only on that seat's final row
- `shanten_before` / `shanten_after`
- `fan_potential_before` / `fan_potential_after`

### Centralized Critic Boundary

Trajectory rows reserve `global_tile_planes` and `global_scalar_features` for a future centralized critic. They are currently `null` and ignored by PPO. Actor inputs remain strictly local observations, so exported ONNX policy behavior is unchanged.

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

The production policy modes are `heuristic` and `neural`. Keep `heuristic` as the fallback and promote a neural candidate only after it wins the rotated arena matrix without a higher deal-in rate.

### Candidate Gate

By default the RL wrapper writes `candidate_gate.json` but does not stop local experimentation when a model is rejected. Use `-EnforceCandidateGate` or `--enforce-candidate-gate` for promotion runs.

```powershell
.\backend\bot_trainer\v2\train_rl_model.ps1 `
  -OutputDir backend/bot_trainer/v2/rl_runs/promotion `
  -TrajectoryMatches 400 `
  -EvalMatches 400 `
  -Epochs 3 `
  -Device cuda `
  -EnforceCandidateGate
```

```bash
./backend/bot_trainer/v2/train_rl_model.sh \
  --output-dir backend/bot_trainer/v2/rl_runs/promotion \
  --trajectory-matches 400 \
  --eval-matches 400 \
  --epochs 3 \
  --device cuda \
  --enforce-candidate-gate
```
