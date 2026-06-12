# 测试更新说明

## 当前状态

模型架构已更新为新版本（Transformer + OpponentModeling + 共享 Encoder + 双 Critic + MoE），测试已同步到新的 ONNX 输出合约：

- `opponent_tenpai_logits`: `(batch, 3)`
- `opponent_risk_logits`: `(batch, 3, 34)`

Rust runtime 会把 `opponent_risk_logits` 按牌聚合为 34 维风险，用于原有策略层的弃牌风险调整。

## 已覆盖的关键路径

- `test_model.py::test_sequence_aware_model_output_shapes`
- `test_model.py::test_actor_critic_export_wrapper_preserves_onnx_outputs`
- `test_dataset.py::test_risk_loss_ignores_unmasked_tiles`
- `test_dataset.py::test_opponent_targets_supervise_opponent_outputs`
- `test_dataset.py::test_schema_v5_encodes_opponent_targets_and_fan_targets`
- `test_rl_dataset.py::test_trajectory_dataset_encodes_risk_and_opponent_targets`
- `test_pretrain_critic.py`
- `test_rl_dataset.py::test_discard_log_probs_use_risk_adjusted_logits`
- `test_rl_dataset.py::test_discard_log_probs_can_use_deployable_zero_value_for_risk_adjustment`
- `test_rl_dataset.py::test_compute_shaped_reward_does_not_double_count_step_reward`
- `test_rl_dataset.py::test_actor_critic_lr_warmup_preserves_critic_multiplier`

## 训练建议

默认不要使用 `-SkipTests` 跳过训练前检查。只有在本机 Python 环境缺少测试依赖、且已在其他环境完成同等验证时，才临时跳过。

当前推荐的训练顺序：

1. 重新导出 schema v5 数据。
2. 运行 SFT 训练并导出 `backend/assets/sft/sft.onnx`。
3. 从 SFT checkpoint bootstrap actor-critic checkpoint。
4. 使用带 global features 与 opponent targets 的 arena trajectory 预训练 critic。
5. 使用 critic-pretrained actor-critic checkpoint 和对应 ONNX 启动 PPO。
6. 通过 candidate gate 后再推广 PPO ONNX。

更新时间: 2026-06-12
状态: 测试已适配新模型输出合约
