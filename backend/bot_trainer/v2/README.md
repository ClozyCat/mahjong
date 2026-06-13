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


小样本导出：

```powershell
.\backend\bot_trainer\v2\export_full_dataset.ps1 -OutputDir backend/bot_trainer/v2/out_smoke -MaxMatches 100 -ProgressEvery 10
```


## 2. 监督训练 SFT

监督训练脚本会训练多头策略/价值网络，并默认导出到 `backend/assets/sft/sft.onnx`。
CUDA 训练默认启用 BF16 AMP，并继续启用 TF32 加速 float32 matmul/convolution。训练不会使用 FP16 AMP；如果当前 CUDA 设备不支持 BF16，会自动关闭 AMP 以避免 FP16 带来的精度风险。

```powershell
.\backend\bot_trainer\v2\train_and_export_model.ps1 -Epochs 20 -BatchSize 4096 -Device cuda -NumWorkers 0
```


显存不足时先降 `BatchSize` 到 `1024`。CPU 训练可用 PowerShell 的 `-Device cpu`，或 Bash 的 `--device cpu`。
如需关闭 AMP，可传 PowerShell 的 `-NoAmp`，或 Bash 的 `--no-amp`；若要禁用 CUDA TF32，可传 `-NoTf32` 或 `--no-tf32`。

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
- `fan_value`：完整番数辅助回归输出，用于训练一致性，不参与当前 Rust runtime 决策
- `qualifying_fan_value`：8 番起和进度辅助回归输出，目标值为 `min(fan_count, 8) / 8`
- `opponent_tenpai_logits`：3 个对手听牌 logits
- `opponent_risk_logits`：`3 x 34` 个对手分牌风险 logits。Rust runtime 会按牌取 3 个对手风险的最大值，聚合为策略层使用的 34 维风险。

`discard_sequence` 右对齐保存最近 32 个公开弃牌事件。每个事件包含 34 维牌 one-hot、4 维相对座位 one-hot、1 维进度、1 维最新事件标记。

当前导出 metadata schema 为 v5。相较旧 schema，SFT 数据直接提供
`opponent_tenpai_target`、`opponent_risk_target` 与 `opponent_risk_mask`
训练对手建模头，并继续使用 `fan_target` 与 `qualifying_fan_target`
训练番数辅助头；旧 cache 与旧 metadata 不再兼容，需要重新导出数据并重新训练。

## 4. Arena 评估

基础 smoke：

```powershell
cargo run --manifest-path backend/Cargo.toml --release --bin bot_arena -- --config backend/bot_trainer/v2/arena_smoke.json --output backend/bot_trainer/v2/arena_smoke.jsonl
```

轨迹 smoke：

```powershell
cargo run --manifest-path backend/Cargo.toml --release --bin bot_arena -- --config backend/bot_trainer/v2/arena_smoke.json --output backend/bot_trainer/v2/arena_smoke.jsonl --trajectories backend/bot_trainer/v2/arena_trajectories_smoke.jsonl
```

PPO 轨迹的 step reward 包含弱向听 shaping，并在进入听牌时区分 8 番达标潜力：足番听牌会获得额外奖励，低番听牌会被轻微惩罚，避免 bot 只学习“任意听牌”。

矩阵评估使用 `arena_policy_pool.json`，格式与 `arena_smoke.json` 一致：顶层必须是
`subjects` 与正好 3 个 `opponents`。旧的 `policies` 池格式和
`learner`/`opponents` 加权池格式不再被 arena 评测脚本接受。

```powershell
.\backend\bot_trainer\v2\arena_matrix.ps1 -Config backend\bot_trainer\v2\arena_policy_pool.json -MatchCount 200 -Seed 20260429
```


主要评估指标：

- 平均分差
- 胜率
- 放铳率
- 首次听牌巡目
- 终局听牌率
- 平均决策耗时
- seat 级延迟 p50/p95/max
- paired subject 分差均值、95% 置信区间、正向比例

## 5. PPO 自博弈训练

PPO 从 SFT checkpoint 与 SFT ONNX 开始。默认：

- checkpoint：`backend/bot_trainer/v2/checkpoints/best.pt`
- baseline ONNX：`backend/assets/sft/sft.onnx`
- policy：`ppo`
- opponent pool：`backend/bot_trainer/v2/opponent_pool.json`

PPO 训练同样默认启用 BF16 AMP。CPU/DirectML 或不支持 BF16 的 CUDA 设备会自动关闭 AMP，不会改用 FP16；如需显式关闭，可传 PowerShell 的 `-NoAmp`，或 Bash 的 `--no-amp`。

快速 smoke：

```powershell
.\backend\bot_trainer\v2\train_rl_model.ps1 -OutputDir backend/bot_trainer/v2/rl_runs/smoke -IterationMatches 1 -EvalMatches 1 -Epochs 1 -BatchSize 64 -Device cpu -Policy ppo
```


本地正式实验建议把 `IterationMatches` / `--iteration-matches` 和 `EvalMatches` / `--eval-matches` 提高到至少 `200`。

常用 PPO league 命令：

```powershell
.\backend\bot_trainer\v2\train_rl_model.ps1 `
  -OutputDir backend/bot_trainer/v2/rl_runs/ppo_smoke `
  -IterationMatches 8 `
  -EvalMatches 4 `
  -ArenaJobs 2 `
  -EpochEvalJobs 2 `
  -Epochs 1 `
  -BatchSize 64 `
  -Device cpu `
  -Policy ppo `
  -LearnerPolicyId learner `
  -GaeLambda 0.95 `
  -KlCoef 0.01
```


脚本流程：

1. 用当前 rollout ONNX 生成一份 evaluation arena 轨迹配置，由系统在完整 16 局比赛内自行换座并采集轨迹。
2. PPO 只读取 `policy_id=learner` 的轨迹。
3. 每个 epoch 保存 `epoch_*.pt`。
4. 默认评估每个 epoch 的候选 ONNX，并选出最优 epoch。
5. 最终产物写入运行目录下的 `candidate.onnx` 和 `checkpoints/best.pt`。

`ArenaJobs` / `--arena-jobs` 控制单次 `bot_arena` 内部并行的完整比赛数；
`EpochEvalJobs` / `--epoch-eval-jobs` 控制 epoch 候选评测的脚本层并发数。
两者设为 `1` 时串行，`ArenaJobs` 设为 `0` 时使用系统可用并行度。

若只想评估最后一个 checkpoint，可使用 `-CandidateSelectionMode final` 或 `--candidate-selection-mode final`。

## 6. 全局信息 Critic

Arena 轨迹包含可选的全局信息字段：

- `global_tile_planes`
- `global_scalar_features`

启用 actor-critic 后，actor 仍只使用本地观测，critic 会优先使用全局信息；旧轨迹没有全局字段时会回退到本地上下文。

首次启用 actor-critic 前，先从 SFT/shared checkpoint 生成一次性 bootstrap checkpoint。后续
`--use-actor-critic` / `-UseActorCritic` 只接受 actor-critic checkpoint，避免旧 checkpoint
让 actor/critic 参数静默随机初始化并污染训练结果。

```powershell
python backend/bot_trainer/v2/bootstrap_actor_critic_checkpoint.py `
  --source backend/bot_trainer/v2/checkpoints/best.pt `
  --output backend/bot_trainer/v2/checkpoints/actor_critic_bootstrap.pt

python backend/bot_trainer/v2/export_onnx.py `
  --checkpoint backend/bot_trainer/v2/checkpoints/actor_critic_bootstrap.pt `
  --output backend/assets/ppo/actor_critic_bootstrap.onnx
```

启用 `-PretrainCritic` 后，RL 脚本会在每轮 arena trajectory 生成完成后、
PPO 训练开始前调用 `pretrain_critic.py`。该步骤读取本轮 trajectory 的
discounted return，并默认要求轨迹含 `global_tile_planes` 与
`global_scalar_features`。预训练产物会写入本轮 checkpoint 目录下的
`critic_pretrained.pt`，并作为随后的 PPO 起点。

```powershell
.\backend\bot_trainer\v2\train_rl_model.ps1 `
  -OutputDir backend/bot_trainer/v2/rl_runs/global_critic_smoke `
  -BaselineCheckpoint backend/bot_trainer/v2/checkpoints/actor_critic_bootstrap.pt `
  -BaselineOnnx backend/assets/ppo/actor_critic_bootstrap.onnx `
  -IterationMatches 8 `
  -EvalMatches 4 `
  -Epochs 1 `
  -BatchSize 64 `
  -Device cpu `
  -Policy ppo `
  -UseActorCritic `
  -PretrainCritic `
  -CriticPretrainEpochs 5 `
  -CriticPretrainBatchSize 256 `
  -CriticLrMultiplier 2.0
```


`CriticLrMultiplier` / `--critic-lr-multiplier` 默认是 `2.0`。
`PretrainCritic` 默认关闭；开启时必须同时传 `-UseActorCritic`。

## 7. 候选验收与上线

候选 PPO 模型替换 `backend/assets/ppo/ppo.onnx` 前，应至少满足：

- 平均分差优于 SFT baseline。
- 胜率不回退。
- 放铳率不增加超过 1 个百分点。
- 首次听牌巡目或终局听牌率不退化。
- 平均副露数不出现异常膨胀。
- 平均决策耗时低于 200 ms。

`candidate_gate.json` 会输出 `failure_details` 与 `promotion_report`。其中
`promotion_report.metrics` 给出关键指标 margin，`promotion_report.paired`
给出 paired 分差与置信区间，`promotion_report.latency` 给出候选平均延迟和
seat 级 p95/max，`promotion_report.warnings` 标记 paired 样本缺失或置信区间
跨 0 等稳定性风险。

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


通过验收后，将选中的候选 ONNX 覆盖到 `backend/assets/ppo/ppo.onnx`。配套的外部权重文件 `weights.data` 必须和 ONNX 同目录保留。

## 8. 生产 bot 差异

生产特殊 bot 使用同一个 `backend/assets/ppo/ppo.onnx`，通过 temperature 区分行为：

- focused：`0.3`
- default：`1.0`
- exploratory：`2.0`

普通 bot 和缺省神经模型路径使用 `backend/assets/sft/sft.onnx`。
