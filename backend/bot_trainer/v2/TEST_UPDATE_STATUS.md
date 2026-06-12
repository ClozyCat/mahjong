# 测试更新说明

## 当前状态

模型架构已更新为新版本（Transformer + OpponentModeling + 共享Encoder + 双Critic + MoE），但部分测试用例尚未完全更新。

## 已更新的测试

- ✅ `test_sft_wrappers_forward_auxiliary_training_flags` - 支持可选的bash脚本
- ✅ `test_rl_training_defaults_to_bf16_amp_and_wrappers_can_disable_it` - 支持可选的bash脚本
- ✅ `test_auxiliary_loss_weights_can_disable_value_and_risk` - 适配opponent modeling loss
- ✅ `test_model.py::test_sequence_aware_model_output_shapes` - 更新输出key

## 待更新的测试

以下测试需要更新以适配新的模型输出格式（`risk_logits` → `opponent_tenpai_logits` + `opponent_risk_logits`）：

### test_dataset.py
- `test_risk_loss_ignores_unmasked_tiles` (line 430)
- `test_auxiliary_losses_use_float32_for_half_precision_outputs` (line 472)
- `test_losses_sanitize_nonfinite_model_outputs` (line 514)
- `test_fan_loss_contributes_when_weighted` (line 549)

### test_rl_dataset.py
- `test_loads_trajectory_row` (line 91) - reward计算变更
- `test_discard_log_probs_use_risk_adjusted_logits` (line 585, 605)
- `test_discard_log_probs_can_use_deployable_zero_value_for_risk_adjustment` (line 619, 646)

### export_onnx.py
- ONNX导出输出列表 (line 27)

## 快速修复建议

### 1. 模型输出格式变更

**旧格式:**
```python
outputs = {
    "risk_logits": torch.Tensor,  # (batch, 34)
}
```

**新格式:**
```python
outputs = {
    "opponent_tenpai_logits": torch.Tensor,  # (batch, 3)
    "opponent_risk_logits": torch.Tensor,    # (batch, 3, 34)
}
```

### 2. 临时跳过测试

训练时可以跳过这些测试：
```powershell
.\train_and_export_model.ps1 -SkipTests
```

### 3. 完整测试更新

需要系统性更新所有测试用例，将`risk_logits`相关的断言替换为新的对手建模输出。

## 训练优先级

当前优先级：**能够运行训练 > 所有测试通过**

建议：
1. 使用`-SkipTests`参数运行训练
2. 验证新架构的训练效果
3. 逐步更新测试用例

## 测试更新计划

1. **第一阶段**（当前）：核心功能测试通过，训练可运行
2. **第二阶段**：更新所有单元测试以匹配新架构
3. **第三阶段**：添加新功能的专项测试（MoE门控、双Critic等）

---

更新时间: 2026-06-12
状态: 训练脚本已更新，测试部分更新中
