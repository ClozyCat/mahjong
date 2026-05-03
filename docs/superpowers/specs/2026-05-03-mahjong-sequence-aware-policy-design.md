# Mahjong Sequence-Aware Policy Design

## Goal

一次性重构 `backend/bot_trainer/v2` 的监督训练、PPO 训练、ONNX 导出和 Rust 推理输入，使模型显式读取牌河时序、对手相对座位、局况进程，并解耦 policy/value/risk/fan 表达。新模型不兼容旧 checkpoint；旧的兼容加载器从代码中移除。

## Current Findings

现有模型只接收 `tile_planes` 和 `scalar_features`。`tile_planes` 已经按相对座位保存三家舍牌和副露累计计数，但没有保留事件顺序。`discard_history` 在监督数据 JSON 和 Rust `BotContextView` 中已经存在，当前 Python/Rust 编码没有把它转换为神经网络输入。PPO 轨迹只保存旧输入，ONNX 也只导出两个输入，因此 runtime 与训练端需要同步升级。

工作区已有用户改动 `backend/bot_trainer/v2/arena_policy_pool.json`，本次不触碰。

## Recommendation Feedback

1. 时序信息与对手上下文缺失：采纳。新增 `discard_sequence` 输入，固定形状 `[32, 40]`，每个事件包含 34 维牌 one-hot、4 维相对座位 one-hot、1 维槽位进度、1 维最新事件标记。监督数据来自 `context.discard_history`，RL 轨迹来自 `encode_bot_context_v2`。模型使用轻量 GRU 编码该序列，避免 ONNX 导出风险高的 Transformer，同时解决“把已抹平数据喂给 GRU 无效”的根因。

2. 多任务 heads 共享瓶颈：采纳。模型改为 policy/value/risk 三条 trunk，并给 discard/claim/self_kong/hu/risk/fan 每个 head 独立 MLP。value 采用独立 tile encoder 与独立 trunk，不再直接依赖被 policy 损失主导的 256 维共享隐层。

3. 花色隔离导致跨花色役识别弱：采纳温和方案。保留 suit-aware 局部卷积，但把四组花色/字牌 embedding 拼接后接两层融合 MLP，形成专门的跨花色融合区。暂不做 cross-attention，原因是当前输入长度只有 4 个 suit token，MLP 足以覆盖跨 suit 组合，且更稳定地导出 ONNX。

4. 辅助任务权重偏低：采纳。默认目标权重调整为 `value=0.75`、`risk=1.0`、`fan=0.5`，并新增辅助损失 warmup，从 0.25 逐步升到目标值，避免 SFT 早期让噪声 value/risk 梯度压制动作模仿。

5. 防守与对手建模不足：采纳。`discard_sequence` 明确编码事件来源；累计对手舍牌/副露平面继续保留，负责对手长期公开信息。risk/value 分支直接消费序列 embedding，防守信息不会只挤在 scalar 里。

6. Dropout 与容量：采纳。移除 Dropout；卷积通道提升到 128；policy/value trunk 扩到 1024/512 级别；卷积归一化从 BatchNorm 改为 GroupNorm，降低 RL 小批量和 rollout/eval 状态差异的风险。

## Chosen Architecture

新增模型输入：

```text
tile_planes:      [batch, 10, 34]
scalar_features:  [batch, 12]
discard_sequence: [batch, 32, 40]
```

Python 与 Rust 使用同一编码：

```text
event[0:34]   = discarded tile one-hot
event[34:38]  = relative seat one-hot, 0 self, 1 next, 2 opposite, 3 previous
event[38]     = slot progress, newest events are closer to 1
event[39]     = 1 only for the latest event in the retained window
```

模型结构：

```text
policy_tile_encoder(tile_planes) -> 512
value_tile_encoder(tile_planes)  -> 512
scalar_encoder(scalar_features)  -> 160
discard_sequence_encoder(seq)    -> 256

policy_features = concat(policy_tile, scalar, seq)
value_features  = concat(value_tile, scalar, seq)
risk_features   = concat(policy_tile, scalar, seq)

policy_trunk -> action-specific MLP heads
value_trunk  -> value head
risk_trunk   -> risk head and fan head
```

## Data Flow Changes

监督训练缓存增加 `discard_sequence.npy`，缓存版本升级，旧缓存自动重建。`MahjongDecisionDataset.get_batch()` 返回新输入；训练 `forward_model()` 调用三输入模型。

RL 轨迹 JSON 增加 `discard_sequence`。`ArenaTrajectoryDataset` 强制读取该字段；旧轨迹不再兼容。Rust arena 生成轨迹时从 `BotFeaturesV2` 写入该字段。

ONNX 导出增加第三个输入名 `discard_sequence`，manifest 写入新的 `model_config`。Rust ORT 推理端同步传入第三个 tensor。旧 ONNX 不再作为新 runtime 的有效模型。

## Risk Handling

主要风险是 Rust/Python 编码不一致。用 Python 单测校验序列 shape、相对座位和最新事件标记，用 Rust 单测校验相同 shape 和事件编码。第二个风险是旧资产仍存在但输入 schema 已失效；代码不做兼容适配，最终需要重新训练并导出新 ONNX 替换现有资产。

## Verification

定向验证优先运行：

```powershell
python -m pytest backend/bot_trainer/v2/test_model.py backend/bot_trainer/v2/test_dataset.py backend/bot_trainer/v2/test_rl_dataset.py
cargo test -p backend bot::features bot::neural bot::arena
```

如果环境缺少 PyTorch、onnxruntime 或 Rust 依赖，需要记录阻塞并报告未验证范围。
