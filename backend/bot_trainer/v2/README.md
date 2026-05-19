# 麻将 Bot 训练 V2

本目录包含当前生产 bot 的数据导出、监督训练、PPO 自博弈训练、ONNX 导出与候选评估脚本。

当前模型资产只保留两套：

- `backend/assets/sft/sft.onnx`：监督学习基线模型，也是普通 bot 与训练脚本的默认兜底模型。
- `backend/assets/ppo/ppo.onnx`：PPO 训练后的生产策略模型。多个 bot 的差异不再来自不同 ONNX，而是来自同一 PPO 模型的 temperature。

旧模型目录和旧 policy 命名已废弃，不再提供兼容入口。PPO 训练入口只接受 `ppo`。

## 1. 导出监督数据

默认数据源是 `backend/bot_trainer/datasets/data.txt`，导出结果写入 `backend/bot_trainer/v2/out`。

```powershell
.\backend\bot_trainer\v2\export_full_dataset.ps1 -ProgressEvery 100
```

```bash
./backend/bot_trainer/v2/export_full_dataset.sh --progress-every 100
```

小样本导出：

```powershell
.\backend\bot_trainer\v2\export_full_dataset.ps1 -OutputDir backend/bot_trainer/v2/out_smoke -MaxMatches 100 -ProgressEvery 10
```

```bash
./backend/bot_trainer/v2/export_full_dataset.sh --output backend/bot_trainer/v2/out_smoke --max-matches 100 --progress-every 10
```

## 2. 监督训练 SFT

监督训练脚本会训练多头策略/价值网络，并默认导出到 `backend/assets/sft/sft.onnx`。

```powershell
.\backend\bot_trainer\v2\train_and_export_model.ps1 -Epochs 20 -BatchSize 4096 -Device cuda -NumWorkers 0
```

```bash
./backend/bot_trainer/v2/train_and_export_model.sh --epochs 20 --batch-size 4096 --device cuda --num-workers 0
```

显存不足时先降 `BatchSize` 到 `1024`。CPU 训练可用 PowerShell 的 `-Device cpu -NoAmp`，或 Bash 的 `--device cpu --no-amp`。

主要产物：

- `backend/bot_trainer/v2/checkpoints/best.pt`
- `backend/bot_trainer/v2/checkpoints/metrics.json`
- `backend/assets/sft/sft.onnx`

导出 ONNX 后，脚本会运行 Rust ONNX smoke test。

## 3. 模型结构与 ONNX 合约

模型输入：

- `tile_planes`：`batch x 10 x 34`
- `scalar_features`：`batch x 12`
- `discard_sequence`：`batch x 32 x 40`

模型输出：

- `discard_logits`：34 个弃牌 logits
- `claim_logits`：7 个响应 logits，顺序为 pass、hu、pung、kong、chow_left、chow_mid、chow_right
- `self_kong_logits`：3 个自杠 logits，顺序为 pass、concealed_kong、add_kong
- `hu_logits`：2 个自摸/不自摸 logits
- `value`：预期分差
- `risk_logits`：34 个牌风险 logits

`discard_sequence` 右对齐保存最近 32 个公开弃牌事件。每个事件包含 34 维牌 one-hot、4 维相对座位 one-hot、1 维进度、1 维最新事件标记。

## 4. Arena 评估

基础 smoke：

```powershell
cargo run --manifest-path backend/Cargo.toml --release --bin bot_arena -- --config backend/bot_trainer/v2/arena_smoke.json --output backend/bot_trainer/v2/arena_smoke.jsonl
```

轨迹 smoke：

```powershell
cargo run --manifest-path backend/Cargo.toml --release --bin bot_arena -- --config backend/bot_trainer/v2/arena_smoke.json --output backend/bot_trainer/v2/arena_smoke.jsonl --trajectories backend/bot_trainer/v2/arena_trajectories_smoke.jsonl
```

矩阵评估使用 `arena_policy_pool.json`。当前池里只应出现 `sft` 与 `ppo` 相关模型路径。

```powershell
.\backend\bot_trainer\v2\arena_matrix.ps1 -Matches 200 -Seed 20260429
```

```bash
MATCHES=200 SEED=20260429 ./backend/bot_trainer/v2/arena_matrix.sh
```

主要评估指标：

- 平均分差
- 胜率
- 放铳率
- 首次听牌巡目
- 终局听牌率
- 平均决策耗时

## 5. PPO 自博弈训练

PPO 从 SFT checkpoint 与 SFT ONNX 开始。默认：

- checkpoint：`backend/bot_trainer/v2/checkpoints/best.pt`
- baseline ONNX：`backend/assets/sft/sft.onnx`
- policy：`ppo`
- opponent pool：`backend/bot_trainer/v2/opponent_pool.json`

快速 smoke：

```powershell
.\backend\bot_trainer\v2\train_rl_model.ps1 -OutputDir backend/bot_trainer/v2/rl_runs/smoke -IterationMatches 1 -EvalMatches 1 -Epochs 1 -BatchSize 64 -Device cpu -Policy ppo
```

```bash
./backend/bot_trainer/v2/train_rl_model.sh --output-dir backend/bot_trainer/v2/rl_runs/smoke --iteration-matches 1 --eval-matches 1 --epochs 1 --batch-size 64 --device cpu --policy ppo
```

本地正式实验建议把 `IterationMatches` / `--iteration-matches` 和 `EvalMatches` / `--eval-matches` 提高到至少 `200`。

常用 PPO league 命令：

```powershell
.\backend\bot_trainer\v2\train_rl_model.ps1 `
  -OutputDir backend/bot_trainer/v2/rl_runs/ppo_smoke `
  -IterationMatches 8 `
  -EvalMatches 4 `
  -Epochs 1 `
  -BatchSize 64 `
  -Device cpu `
  -Policy ppo `
  -LearnerPolicyId learner `
  -GaeLambda 0.95 `
  -KlCoef 0.01
```

```bash
./backend/bot_trainer/v2/train_rl_model.sh \
  --output-dir backend/bot_trainer/v2/rl_runs/ppo_smoke \
  --iteration-matches 8 \
  --eval-matches 4 \
  --epochs 1 \
  --batch-size 64 \
  --device cpu \
  --policy ppo \
  --learner-policy-id learner \
  --gae-lambda 0.95 \
  --kl-coef 0.01
```

脚本流程：

1. 用当前 rollout ONNX 生成 arena 轨迹。
2. PPO 只读取 `policy_id=learner` 的轨迹。
3. 每个 epoch 保存 `epoch_*.pt`。
4. 默认评估每个 epoch 的候选 ONNX，并选出最优 epoch。
5. 最终产物写入运行目录下的 `candidate.onnx` 和 `checkpoints/best.pt`。

若只想评估最后一个 checkpoint，可使用 `-CandidateSelectionMode final` 或 `--candidate-selection-mode final`。

## 6. 全局信息 Critic

Arena 轨迹包含可选的全局信息字段：

- `global_tile_planes`
- `global_scalar_features`

启用 actor-critic 后，actor 仍只使用本地观测，critic 会优先使用全局信息；旧轨迹没有全局字段时会回退到本地上下文。

```powershell
.\backend\bot_trainer\v2\train_rl_model.ps1 `
  -OutputDir backend/bot_trainer/v2/rl_runs/global_critic_smoke `
  -IterationMatches 8 `
  -EvalMatches 4 `
  -Epochs 1 `
  -BatchSize 64 `
  -Device cpu `
  -Policy ppo `
  -UseActorCritic `
  -CriticLrMultiplier 2.0
```

```bash
./backend/bot_trainer/v2/train_rl_model.sh \
  --output-dir backend/bot_trainer/v2/rl_runs/global_critic_smoke \
  --iteration-matches 8 \
  --eval-matches 4 \
  --epochs 1 \
  --batch-size 64 \
  --device cpu \
  --policy ppo \
  --use-actor-critic \
  --critic-lr-multiplier 2.0
```

`CriticLrMultiplier` / `--critic-lr-multiplier` 默认是 `2.0`。

## 7. 候选验收与上线

候选 PPO 模型替换 `backend/assets/ppo/ppo.onnx` 前，应至少满足：

- 平均分差优于 SFT baseline。
- 胜率不回退。
- 放铳率不增加超过 2 个百分点。
- 首次听牌巡目或终局听牌率不退化。
- 平均决策耗时低于 200 ms。

Promotion 示例：

```powershell
.\backend\bot_trainer\v2\train_rl_model.ps1 `
  -OutputDir backend/bot_trainer/v2/rl_runs/promotion `
  -IterationMatches 400 `
  -EvalMatches 400 `
  -Epochs 3 `
  -Device cuda `
  -Policy ppo `
  -EnforceCandidateGate
```

```bash
./backend/bot_trainer/v2/train_rl_model.sh \
  --output-dir backend/bot_trainer/v2/rl_runs/promotion \
  --iteration-matches 400 \
  --eval-matches 400 \
  --epochs 3 \
  --device cuda \
  --policy ppo \
  --enforce-candidate-gate
```

通过验收后，将选中的候选 ONNX 覆盖到 `backend/assets/ppo/ppo.onnx`。配套的外部权重文件 `weights.data` 必须和 ONNX 同目录保留。

## 8. 生产 bot 差异

生产特殊 bot 使用同一个 `backend/assets/ppo/ppo.onnx`，通过 temperature 区分行为：

- focused：`0.3`
- default：`1.0`
- exploratory：`2.0`

普通 bot 和缺省神经模型路径使用 `backend/assets/sft/sft.onnx`。
