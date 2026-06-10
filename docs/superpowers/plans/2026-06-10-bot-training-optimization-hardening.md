# BOT Training Optimization Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修正当前 BOT 监督学习/强化学习链路中会直接影响训练-部署一致性、训练稳定性与候选晋级质量的高优先级问题，并为后续数据质量优化保留可执行路线。

**Architecture:** 本轮优先做低侵入硬化：让 PPO actor 的风险调整只依赖部署时可获得的信息，关闭 PPO 更新中的 dropout 随机性，修正 RL PowerShell 包装脚本默认架构，并把候选晋级延迟门禁落到代码和测试。更大范围的数据标注、局面上下文与辅助任务改造进入后续阶段，避免与稳定性修复混在一起。

**Tech Stack:** Python, PyTorch, pytest, ONNX export/runtime, PowerShell training wrapper.

---

## 背景结论

当前训练链路最值得先修的点不是扩大模型，而是消除“训练目标与部署行为不一致”：

1. `backend/assets/ppo/ppo.onnx` 和 `backend/assets/ppo/actor_critic_bootstrap.onnx` 的 actor-critic 导出只暴露本地输入，`value` 输出在无 global 输入时为 0。PPO 训练时却用 critic `value` 调整 discard 风险权重，导致训练策略可能依赖部署时不可用的全局 critic 信息。
2. `rl_train.py` 的当前训练模型默认保持 `train()`，模型 trunk 中的 dropout 会参与 PPO 更新，而 old policy/teacher 是 `eval()`。这会增加 PPO ratio、KL 与 entropy 的噪声。
3. `train_rl_model.ps1` 默认 checkpoint/ONNX 指向 actor-critic，但 `-UseActorCritic` 默认关闭，会被架构 guard 拒绝；bash/README 默认则是 SFT。PowerShell 默认值需要回到 SFT 基线。
4. `candidate_gate.py` 只检查分数、胜率、放铳、听牌和副露漂移，没有落实评估文档中的 200ms 决策延迟门槛。
5. 后续还应优化监督数据与评估体系：非法标签分解、真实分数/排名上下文、反事实风险标签、稀有动作采样/权重、辅助任务恢复或清理、multi-seed 晋级报告。

## 并行策略

本轮不派发并行 agent。原因：立即实施项主要集中在 `rl_train.py`、`test_rl_dataset.py`、`candidate_gate.py` 和 `train_rl_model.ps1`，拆分后同文件写入和测试耦合较强，子代理合并成本高于收益。后续“数据质量”和“评估系统”阶段可以拆为独立 agent。

## File Structure

- Modify: `backend/bot_trainer/v2/rl_train.py`
  - 新增 PPO 更新前模型模式准备函数。
  - 为 discard 风险调整增加 deployable value source 配置。
  - actor-critic PPO 默认使用 zero value 做 actor 风险调整，避免依赖部署时不存在的 critic 全局值。
- Modify: `backend/bot_trainer/v2/test_rl_dataset.py`
  - 新增 PPO 风险值来源测试。
  - 新增 dropout 关闭测试。
  - 更新 candidate gate 延迟测试。
- Modify: `backend/bot_trainer/v2/candidate_gate.py`
  - 增加 `LATENCY_LIMIT_MS = 200.0`。
  - 候选平均决策延迟达到或超过 200ms 时拒绝晋级。
- Modify: `backend/bot_trainer/v2/train_rl_model.ps1`
  - 默认 baseline checkpoint/ONNX 改回 SFT。
  - actor-critic 仍通过显式 `-UseActorCritic` 和参数传入。
- Later: `backend/bot_trainer/v2/dataset.py`, `backend/src/bot_trainer/replay.rs`, `backend/bot_trainer/v2/train.py`, `backend/bot_trainer/v2/export_onnx.py`
  - 后续阶段再处理数据上下文、反事实风险和 checkpoint manifest。

---

## Task 1: PPO Actor 风险调整改为部署一致

**Files:**
- Modify: `backend/bot_trainer/v2/test_rl_dataset.py`
- Modify: `backend/bot_trainer/v2/rl_train.py`

- [x] **Step 1: 写 failing test**

新增测试：当 `policy_config["discard_value_source"] == "zero"` 时，discard 风险权重必须使用部署一致的 0 value，而不是 `outputs["value"]` 中的 critic 值。

```python
def test_discard_log_probs_can_use_deployable_zero_value_for_risk_adjustment() -> None:
    import math
    import torch
    from rl_train import select_action_log_probs

    outputs = {
        "discard_logits": torch.tensor([[0.0, 0.0] + [-100.0] * 32]),
        "claim_logits": torch.zeros((1, 7)),
        "self_kong_logits": torch.zeros((1, 3)),
        "hu_logits": torch.zeros((1, 2)),
        "value": torch.tensor([[-8.0]]),
        "risk_logits": torch.tensor([[5.0, -5.0] + [0.0] * 32]),
    }
    batch = {
        "reward": torch.tensor([0.0]),
        "action_head": torch.tensor([0]),
        "action_index": torch.tensor([0]),
        "discard_mask": torch.tensor([[True, True] + [False] * 32]),
        "claim_mask": torch.zeros((1, 7), dtype=torch.bool),
        "self_kong_mask": torch.zeros((1, 3), dtype=torch.bool),
        "hu_mask": torch.tensor([[True, False]]),
    }
    policy_config = {
        "base_risk_weight": 0.90,
        "value_risk_range": 0.55,
        "min_risk_weight": 0.25,
        "max_risk_weight": 1.45,
        "discard_value_source": "zero",
    }

    log_prob = select_action_log_probs(outputs, batch, policy_config)

    risk_weight = 0.90
    first = -risk_weight * (1.0 / (1.0 + math.exp(-5.0)))
    second = -risk_weight * (1.0 / (1.0 + math.exp(5.0)))
    expected = first - max(first, second) - math.log(
        math.exp(first - max(first, second)) + math.exp(second - max(first, second))
    )
    assert log_prob.item() == pytest.approx(expected, abs=1e-5)
```

- [x] **Step 2: 运行 test 验证失败**

Run:

```powershell
python -m pytest backend/bot_trainer/v2/test_rl_dataset.py::test_discard_log_probs_can_use_deployable_zero_value_for_risk_adjustment -q
```

Expected: FAIL，原因是当前 `risk_adjusted_discard_logits` 仍使用 `outputs["value"]`。

- [x] **Step 3: 实现最小改动**

`risk_adjusted_discard_logits` 读取 `discard_value_source`：

```python
def discard_value_for_risk_adjustment(
    outputs: dict[str, torch.Tensor],
    policy_config: dict[str, object] | None,
) -> torch.Tensor | None:
    value = outputs.get("value")
    if value is None:
        return None
    if policy_config is not None and policy_config.get("discard_value_source") == "zero":
        return torch.zeros_like(value)
    return value
```

`main()` 中复制 policy config，并在 `--use-actor-critic` 时设置：

```python
policy_config = dict(POLICY_CONFIGS[args.policy])
if args.use_actor_critic:
    policy_config["discard_value_source"] = "zero"
else:
    policy_config["discard_value_source"] = "network"
```

- [x] **Step 4: 运行相关 PPO action 测试**

Run:

```powershell
python -m pytest backend/bot_trainer/v2/test_rl_dataset.py::test_discard_log_probs_use_risk_adjusted_logits backend/bot_trainer/v2/test_rl_dataset.py::test_discard_log_probs_can_use_deployable_zero_value_for_risk_adjustment -q
```

Expected: PASS。

---

## Task 2: PPO 更新关闭 dropout 随机性

**Files:**
- Modify: `backend/bot_trainer/v2/test_rl_dataset.py`
- Modify: `backend/bot_trainer/v2/rl_train.py`

- [x] **Step 1: 写 failing test**

新增测试：PPO 更新前模型整体仍是 training mode，但所有 `torch.nn.Dropout` module 应处于 eval mode，参数梯度保持开启。

```python
def test_prepare_model_for_ppo_updates_disables_dropout_without_freezing_params() -> None:
    import torch
    from rl_train import prepare_model_for_ppo_updates

    model = torch.nn.Sequential(
        torch.nn.Linear(2, 2),
        torch.nn.Dropout(0.5),
        torch.nn.LayerNorm(2),
    )

    prepare_model_for_ppo_updates(model)

    assert model.training is True
    assert model[1].training is False
    assert model[0].training is True
    assert all(parameter.requires_grad for parameter in model.parameters())
```

- [x] **Step 2: 运行 test 验证失败**

Run:

```powershell
python -m pytest backend/bot_trainer/v2/test_rl_dataset.py::test_prepare_model_for_ppo_updates_disables_dropout_without_freezing_params -q
```

Expected: FAIL，原因是函数尚不存在。

- [x] **Step 3: 实现最小改动**

在 `rl_train.py` 增加：

```python
def prepare_model_for_ppo_updates(model: torch.nn.Module) -> None:
    model.train()
    for module in model.modules():
        if isinstance(module, torch.nn.Dropout):
            module.eval()
```

在 `main()` 中 `load_checkpoint_if_present(model, args.checkpoint)` 之后调用：

```python
prepare_model_for_ppo_updates(model)
```

- [x] **Step 4: 运行 dropout 测试**

Run:

```powershell
python -m pytest backend/bot_trainer/v2/test_rl_dataset.py::test_prepare_model_for_ppo_updates_disables_dropout_without_freezing_params -q
```

Expected: PASS。

---

## Task 3: PowerShell RL 默认基线回到 SFT

**Files:**
- Modify: `backend/bot_trainer/v2/train_rl_model.ps1`

- [x] **Step 1: 修改默认参数**

将默认值改为：

```powershell
[string]$BaselineCheckpoint = "backend/bot_trainer/v2/checkpoints/best.pt",
[string]$BaselineOnnx = "backend/assets/sft/sft.onnx",
```

- [x] **Step 2: 验证脚本默认值**

Run:

```powershell
Select-String -Path backend/bot_trainer/v2/train_rl_model.ps1 -Pattern 'BaselineCheckpoint|BaselineOnnx|UseActorCritic'
```

Expected: 默认 checkpoint/ONNX 指向 SFT，`UseActorCritic` 仍是显式 switch。

---

## Task 4: Candidate Gate 增加 200ms 延迟门禁

**Files:**
- Modify: `backend/bot_trainer/v2/test_rl_dataset.py`
- Modify: `backend/bot_trainer/v2/candidate_gate.py`

- [x] **Step 1: 写 failing test**

新增或改写测试：候选 `avg_latency_ms_per_decision >= 200.0` 时拒绝晋级，并在 failures 中包含 `latency`。

```python
def test_candidate_gate_rejects_high_latency() -> None:
    from candidate_gate import evaluate_candidate

    summary = {
        "policies": {
            "baseline_neural": {
                "avg_score_delta": 0.0,
                "win_rate": 0.20,
                "deal_in_rate": 0.10,
                "avg_first_tenpai_turn": 8.0,
                "final_tenpai_rate": 0.55,
                "avg_latency_ms_per_decision": 20.0,
                "avg_claims": 2.0,
            },
            "rl_candidate_neural": {
                "avg_score_delta": 1.5,
                "win_rate": 0.21,
                "deal_in_rate": 0.10,
                "avg_first_tenpai_turn": 7.8,
                "final_tenpai_rate": 0.56,
                "avg_latency_ms_per_decision": 220.0,
                "avg_claims": 2.2,
            },
        }
    }

    result = evaluate_candidate(summary, "baseline_neural", "rl_candidate_neural")

    assert result["accepted"] is False
    assert "latency" in result["failures"]
```

- [x] **Step 2: 运行 test 验证失败**

Run:

```powershell
python -m pytest backend/bot_trainer/v2/test_rl_dataset.py::test_candidate_gate_rejects_high_latency -q
```

Expected: FAIL，原因是 candidate gate 当前不检查 latency。

- [x] **Step 3: 实现最小改动**

`candidate_gate.py` 增加：

```python
LATENCY_LIMIT_MS = 200.0

def latency_is_excessive(candidate: dict[str, Any]) -> bool:
    latency = candidate.get("avg_latency_ms_per_decision")
    if latency is None:
        return False
    return float(latency) >= LATENCY_LIMIT_MS
```

并在 `evaluate_candidate` 中 append `latency` failure。

- [x] **Step 4: 运行 candidate gate 测试**

Run:

```powershell
python -m pytest backend/bot_trainer/v2/test_rl_dataset.py::test_candidate_gate_rejects_high_latency backend/bot_trainer/v2/test_rl_dataset.py::test_candidate_gate_rejects_excessive_claim_rate backend/bot_trainer/v2/test_rl_dataset.py::test_candidate_gate_accepts_safe_improvement -q
```

Expected: PASS。

---

## Task 5: 定向回归验证

**Files:**
- No code changes.

- [x] **Step 1: 运行 RL dataset/test gate 回归**

Run:

```powershell
python -m pytest backend/bot_trainer/v2/test_rl_dataset.py -q
```

Expected: PASS。

- [x] **Step 2: 运行 SFT/model 回归**

Run:

```powershell
python -m pytest backend/bot_trainer/v2/test_dataset.py backend/bot_trainer/v2/test_model.py -q
```

Expected: PASS。

- [x] **Step 3: 检查改动范围**

Run:

```powershell
git diff -- backend/bot_trainer/v2/rl_train.py backend/bot_trainer/v2/test_rl_dataset.py backend/bot_trainer/v2/candidate_gate.py backend/bot_trainer/v2/train_rl_model.ps1 docs/superpowers/plans/2026-06-10-bot-training-optimization-hardening.md
```

Expected: 只包含本计划描述的改动。

---

## 后续阶段建议

这些项本轮只形成路线，不直接实现：

1. 数据导出质量：在 `export_report.json` 增加非法标签按 action head、局面阶段、来源 replay 的 breakdown；当前 `runtime_illegal_label_count = 1517 / 5908990`，比例低但需要定位系统性来源。
2. 真实局面上下文：`backend/src/bot_trainer/replay.rs` 当前训练上下文使用固定 dealer、seat wind 与零累计分，应恢复真实 dealer、场风/自风、分数、排名和本场/立直棒信息。
3. 反事实风险：当前 risk label 只监督实际放铳牌，无法覆盖“同局面其他合法候选牌”的安全差异；可从 replay/规则引擎构建候选级 risk target 或 danger teacher。
4. 稀有动作优化：rob kong、self kong、claim window 分布稀疏，应加入分头采样权重、focal loss 或 rare-head over-sampling，并用 per-head metrics 晋级。
5. 辅助任务一致性：当前 metrics/checkpoint 中存在历史 `fan_head` 痕迹，但当前模型/训练代码不再包含 fan head；应明确是恢复 fan/shanten auxiliary，还是清理遗留指标与 checkpoint 兼容层。
6. 晋级报告升级：候选晋级从单 seed 扩展为 multi-seed paired eval，输出置信区间、失败原因和延迟分布，而不是只看均值。
