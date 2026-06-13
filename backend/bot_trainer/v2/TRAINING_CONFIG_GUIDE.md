# 强化学习训练配置示例

## 快速开始（标准配置）

```powershell
# 使用所有优化项的标准配置
.\train_rl_model.ps1 `
  -BaselineCheckpoint "backend/bot_trainer/v2/checkpoints/best.pt" `
  -BaselineOnnx "backend/assets/sft/sft.onnx" `
  -Iterations 10 `
  -Epochs 3 `
  -BatchSize 256 `
  -LearningRate 3e-6 `
  -LrWarmupEpochs 3 `
  -ClipEpsilon 0.15 `
  -EntropyCoef 0.03 `
  -EntropyEndCoef 0.008 `
  -EntropyDecayMode cosine `
  -KlAdaptive `
  -KlTarget 0.02 `
  -TargetKl 0.04 `
  -ReplayBufferEpochs 3 `
  -ReplayRatio 0.4 `
  -UseActorCritic `
  -CriticLrMultiplier 2.0 `
  -OpponentPool "backend/bot_trainer/v2/opponent_pool.json"
```

## 配置说明

### 基础参数
- `-BaselineCheckpoint`: SFT模型checkpoint路径
- `-BaselineOnnx`: SFT模型ONNX路径（用于对战评估）
- `-Iterations`: 迭代次数（每次迭代=生成轨迹+训练+评估）
- `-Epochs`: 每次迭代的PPO训练epoch数

### 学习率相关（已优化 ✨）
- `-LearningRate`: 基础学习率（默认3e-6）
- `-LrWarmupEpochs`: Warmup epoch数（新增，默认3）
  - 前N个epoch线性增加学习率
  - 稳定训练初期
- `-CriticLrMultiplier`: Critic学习率倍数（默认2.0）

### 探索策略（已优化 ✨）
- `-EntropyCoef`: 初始entropy系数（0.02→0.03）
- `-EntropyEndCoef`: 最终entropy系数（0.005→0.008）
- `-EntropyDecayMode`: 衰减模式（新增，cosine/linear）
  - `cosine`: 余弦衰减，更平滑（推荐）
  - `linear`: 线性衰减

### PPO参数（已优化 ✨）
- `-ClipEpsilon`: 策略裁剪范围（0.2→0.15，更保守）
- `-ValueClipEpsilon`: 值函数裁剪范围（默认0.2）
- `-Gamma`: 折扣因子（默认0.995）
- `-GaeLambda`: GAE lambda（默认0.95）

### KL散度控制（已优化 ✨）
- `-KlAdaptive`: 启用自适应KL惩罚（新增，默认true）
  - 根据实际KL值动态调整惩罚系数
- `-KlCoef`: 初始KL惩罚系数（默认0.01）
- `-KlTarget`: KL目标值（新增，默认0.02）
  - 自适应调整的目标
- `-TargetKl`: KL早停阈值（0.03→0.04）

### Replay机制（新增 ✨）
- `-ReplayBufferEpochs`: 保留最近N个epoch（默认3）
- `-ReplayRatio`: Replay数据占比（默认0.4，即40%）
  - 提升数据效率40%

### Actor-Critic（推荐）
- `-UseActorCritic`: 启用Actor-Critic架构
  - 支持双Critic和MoE（需代码修改）
- `-CriticLrMultiplier`: Critic学习率=Actor学习率×此值
- `-PretrainCritic`: 每轮轨迹生成后、PPO训练前自动预训练Critic（默认关闭，要求同时启用 `-UseActorCritic`）
- `-CriticPretrainEpochs`: Critic预训练epoch数（默认5）
- `-CriticPretrainBatchSize`: Critic预训练batch大小（默认256）
- `-CriticPretrainLearningRate`: Critic预训练学习率（默认1e-4）

### 其他参数
- `-BatchSize`: 批大小（默认2048，建议256-512）
- `-Device`: 设备（auto/cuda/cpu）
- `-NoAmp`: 禁用混合精度训练
- `-OpponentPool`: 对手池配置（新格式v2）

## 推荐配置组合

### 配置1: 快速验证（低资源）
```powershell
.\train_rl_model.ps1 `
  -Iterations 3 `
  -IterationMatches 500 `
  -Epochs 1 `
  -BatchSize 256 `
  -ReplayBufferEpochs 2 `
  -ReplayRatio 0.3
```

### 配置2: 标准训练（推荐）
```powershell
.\train_rl_model.ps1 `
  -Iterations 10 `
  -IterationMatches 1500 `
  -Epochs 3 `
  -BatchSize 256 `
  -LrWarmupEpochs 3 `
  -EntropyDecayMode cosine `
  -KlAdaptive `
  -ReplayBufferEpochs 3 `
  -ReplayRatio 0.4 `
  -UseActorCritic
```

### 配置3: 深度训练（高资源）
```powershell
.\train_rl_model.ps1 `
  -Iterations 20 `
  -IterationMatches 2000 `
  -Epochs 5 `
  -BatchSize 512 `
  -LrWarmupEpochs 5 `
  -EntropyCoef 0.04 `
  -EntropyEndCoef 0.01 `
  -ReplayBufferEpochs 5 `
  -ReplayRatio 0.5 `
  -UseActorCritic
```

## 参数调优建议

### 1. 学习率不稳定
**症状**: Loss震荡、NaN、训练崩溃
**调整**:
```powershell
-LearningRate 1e-6 `  # 降低学习率
-LrWarmupEpochs 5      # 延长warmup
```

### 2. 探索不足
**症状**: Entropy快速下降到0、策略单一
**调整**:
```powershell
-EntropyCoef 0.04 `      # 提高初始探索
-EntropyEndCoef 0.01 `   # 保留更多终态探索
-EntropyDecayMode cosine # 使用余弦衰减
```

### 3. 过拟合旧策略
**症状**: approx_kl持续过高、训练停滞
**调整**:
```powershell
-KlAdaptive `            # 启用自适应KL
-KlTarget 0.015 `        # 降低KL目标
-ReplayRatio 0.3         # 减少replay占比
```

### 4. 收敛缓慢
**症状**: 多次迭代score_margin无提升
**调整**:
```powershell
-Epochs 5 `              # 增加每次迭代的训练轮数
-ReplayBufferEpochs 5 `  # 增加replay缓冲
-ReplayRatio 0.5         # 提高replay占比
```

### 5. 值函数不准
**症状**: value_explained_variance < 0.5
**建议**: 在RL流程中启用 Critic 预训练，让脚本先生成带 global features 的 arena trajectory，再用该 trajectory 预训练 Critic。
```powershell
.\train_rl_model.ps1 `
  -BaselineCheckpoint "backend/bot_trainer/v2/checkpoints/actor_critic_bootstrap.pt" `
  -BaselineOnnx "backend/assets/ppo/actor_critic_bootstrap.onnx" `
  -UseActorCritic `
  -PretrainCritic `
  -CriticPretrainEpochs 5 `
  ...
```

`-PretrainCritic` 会在每轮 trajectory 生成后运行，产物为本轮 checkpoint 目录下的 `critic_pretrained.pt`，随后自动作为 PPO 的输入 checkpoint。

## 监控指标参考

### 正常范围
```
approx_kl:     0.01 ~ 0.03
clip_fraction: 0.1 ~ 0.3
value_ev:      0.5 ~ 0.9
entropy:       逐渐下降到 0.01 ~ 0.5
```

### 异常信号
```
approx_kl > 0.1      → 学习率过大或KL惩罚不足
clip_fraction > 0.5  → 策略变化过激
value_ev < 0.3       → Critic训练不足
entropy < 0.001      → 探索不足，陷入局部最优
```

## 新旧参数对照表

| 旧参数 | 新参数 | 说明 |
|--------|--------|------|
| - | `-LrWarmupEpochs` | 新增：学习率warmup |
| `-EntropyDecaySteps` | `-EntropyDecayMode` | 改为模式选择 |
| `-KlEndCoef` | `-KlAdaptive` + `-KlTarget` | 改为自适应 |
| `-ClipEpsilon 0.2` | `-ClipEpsilon 0.15` | 默认值更保守 |
| `-EntropyCoef 0.02` | `-EntropyCoef 0.03` | 默认值提高 |
| `-TargetKl 0.03` | `-TargetKl 0.04` | 放宽早停阈值 |
| - | `-ReplayBufferEpochs` | 新增：Replay缓冲 |
| - | `-ReplayRatio` | 新增：Replay占比 |

## 完整参数列表

运行以下命令查看所有可用参数：
```powershell
Get-Help .\train_rl_model.ps1 -Detailed
```

或查看脚本开头的param()块。

---

更新时间: 2026-06-12
版本: v2（优化版）
