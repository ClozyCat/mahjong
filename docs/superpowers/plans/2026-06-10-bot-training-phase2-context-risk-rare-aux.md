# BOT Training Phase 2 Context Risk Rare Aux Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 完成 BOT 训练 phase 2：真实局面上下文、候选级反事实风险、稀有动作优化和辅助任务一致性，面向重新导出数据和重新训练模型。

**Architecture:** 训练数据升级到 schema v3，不兼容旧 metadata/cache/checkpoint。Rust replay 支持局前 dealer 和累计分上下文；Python dataset 增加 `risk_mask` 与 `fan_target`；SFT loss 使用 masked risk BCE、稀有动作类别权重和 fan auxiliary loss；模型恢复 `fan_value` 辅助输出，ONNX 可导出该输出，Rust runtime 继续读取已有关键输出。

**Tech Stack:** Rust replay/export, Python dataset, PyTorch model/training, pytest, cargo test, ONNX export.

---

## Scope Decisions

- 不兼容旧训练产物：`metadata.schema_version` 升到 3，`DISK_CACHE_VERSION` 升级，旧 cache 自然失效。
- 不扩 12 维 `scalar_features`：真实上下文优先修正已有 dealer/seat wind/score 槽位的数据来源，避免 Rust runtime 和 Python input 维度分叉。
- 不使用终局 `Score` 作为局中累计分：终局分数是未来信息，只能使用新增局前上下文字段；旧数据缺字段时仍为 0。
- `outcome.dealt_in` 改为决策级语义：只有当前 `Play` 后紧跟他人 `Hu` 的弃牌样本为 true，避免最终放铳者此前所有弃牌被错误标成风险正样本。
- 反事实风险采用候选级弱监督：非放铳 active discard 只监督实际弃牌为安全；放铳 active discard 监督实际放铳牌 target=1，并把同局面其他合法弃牌作为弱反事实负样本。
- 稀有动作优化先落在 loss 权重：claim/self-kong/hu 的非 pass 类别默认放大，不改采样器，降低训练脚本复杂度。
- fan auxiliary 用 `outcome.fan_count` 回归到 `fan_value`，不参与 Rust runtime 决策。

## File Structure

- Modify: `backend/src/bot_trainer/botzone.rs`
  - `BotZoneMatch` 增加 `dealer_seat` 和 `cumulative_scores`。
  - 解析可选 `Dealer <seat>` 与 `Scores <s0> <s1> <s2> <s3>` 局前上下文。
- Modify: `backend/src/bot_trainer/replay.rs`
  - context 使用真实 dealer 和 cumulative scores。
  - seat wind 由 standard seat/dealer 计算。
- Modify: `backend/src/bot_trainer/export.rs`
  - metadata schema 升到 v3。
  - model outputs 增加 `fan_value`。
- Modify: `backend/bot_trainer/v2/dataset.py`
  - metadata schema 要求 v3。
  - tensor cache version 升级。
  - 增加 `risk_mask` 与 `fan_target`。
- Modify: `backend/bot_trainer/v2/model.py`
  - shared policy 和 actor 输出 `fan_value`。
- Modify: `backend/bot_trainer/v2/train.py`
  - masked risk loss。
  - 稀有动作类别权重参数。
  - fan auxiliary loss、metrics 和 warmup。
- Modify: `backend/bot_trainer/v2/export_onnx.py`
  - ONNX outputs 增加 `fan_value`。
- Modify: `backend/bot_trainer/v2/bootstrap_actor_critic_checkpoint.py`
  - shared fan head 权重复制到 actor fan head。
- Modify tests:
  - `backend/src/bot_trainer/botzone.rs`
  - `backend/src/bot_trainer/replay.rs`
  - `backend/src/bot_trainer/export.rs`
  - `backend/bot_trainer/v2/test_dataset.py`
  - `backend/bot_trainer/v2/test_model.py`

---

## Task 1: 真实局面上下文

- [x] **Step 1: 写 Rust failing tests**
  - botzone 解析 `Dealer` 与 `Scores`。
  - replay context 输出 standard dealer、seat wind 和重排后的 cumulative scores。

- [x] **Step 2: 运行 Rust tests 验证失败**
  - `cargo test --manifest-path backend/Cargo.toml bot_trainer::botzone -- --nocapture`
  - `cargo test --manifest-path backend/Cargo.toml bot_trainer::replay -- --nocapture`

- [x] **Step 3: 实现 botzone/replay context**
  - `BotZoneMatch` 增加字段。
  - `parse_match_lines` 解析可选字段。
  - `ReplayState::context` 使用 `botzone_seat_to_standard(record.dealer_seat)` 和 `reorder_botzone_seat_array(&record.cumulative_scores)`。

- [x] **Step 4: 运行 Rust context tests**

---

## Task 2: 反事实风险 mask

- [x] **Step 1: 写 Python failing tests**
  - `encode_row` 输出 `risk_mask`。
  - 放铳 active discard 样本：所有合法 discard 被 mask，实际放铳牌 target=1。
  - masked risk loss 忽略 mask 外高损失位置。

- [x] **Step 2: 运行 Python tests 验证失败**
  - `python -m pytest backend/bot_trainer/v2/test_dataset.py -q`

- [x] **Step 3: 实现 dataset v3 risk_mask**
  - `DISK_CACHE_VERSION = 8`。
  - metadata schema 必须为 3。
  - tensor specs 增加 `risk_mask`。

- [x] **Step 4: 实现 masked risk BCE**
  - `compute_losses` 使用 `risk_mask` 做 element mask。

- [x] **Step 5: 运行 Python dataset tests**

---

## Task 3: 稀有动作 loss 权重

- [x] **Step 1: 写 Python failing tests**
  - 同 logits 下，hu 正类和 self-kong 非 pass 类的 loss 随 rare multiplier 增大。

- [x] **Step 2: 实现 weighted masked cross entropy**
  - 不使用 `F.cross_entropy(weight=...)` 的 normalized mean；改为 unreduced loss 乘类别权重后普通 mean。

- [x] **Step 3: 增加训练参数**
  - `--claim-rare-action-weight`
  - `--self-kong-rare-action-weight`
  - `--hu-positive-weight`

- [x] **Step 4: 运行 rare action tests**

---

## Task 4: fan auxiliary 一致性

- [x] **Step 1: 写 Python failing tests**
  - dataset 输出 `fan_target`。
  - model 输出 `fan_value`。
  - compute_losses 返回 `fan_loss` 并受 `fan_weight` 控制。
  - bootstrap actor-critic 复制 fan head。
  - export wrapper 输出包含 `fan_value`。

- [x] **Step 2: 实现 dataset/model/train/export**
  - `fan_target = outcome.fan_count / 16.0`。
  - shared model `fan_head` 接在 value hidden。
  - actor model `fan_head` 接在 policy/risk shared combined path的 policy hidden。
  - `OUTPUT_NAMES` 增加 `fan_value`。

- [x] **Step 3: 运行 fan auxiliary tests**

---

## Task 5: 最终验证

- [x] **Step 1: Python 回归**
  - `python -m pytest backend/bot_trainer/v2/test_dataset.py backend/bot_trainer/v2/test_model.py -q`
  - `python -m pytest backend/bot_trainer/v2/test_rl_dataset.py -q`

- [x] **Step 2: Rust 回归**
  - `cargo test --manifest-path backend/Cargo.toml bot_trainer -- --nocapture`
  - `cargo test --manifest-path backend/Cargo.toml bot::features -- --nocapture`

- [x] **Step 3: diff 检查**
  - `git diff --check`
  - `git diff --stat`
