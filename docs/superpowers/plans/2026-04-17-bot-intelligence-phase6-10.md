# Bot Intelligence Phase 6-10 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在不明显增加 CPU 开销的前提下，完成 claim 路线感知、rollout 放铳 EV、路线库扩展、对手听型倾向、分数目标细化这 5 项 bot 智能优化。

**Architecture:** 继续以 `backend/src/bot/search.rs` 的启发式评分与缓存为核心，把新增智能收敛到常数级特征计算和现有 Monte Carlo rollout 中，避免增加搜索深度与候选宽度。`backend/src/bot/policy.rs` 只负责调用已有搜索/评分结果，不重复推理。

**Tech Stack:** Rust, cargo test, 现有 bot search/policy 评分框架

---

### Task 1: Claim 路线感知

**Files:**
- Modify: `backend/src/bot/search.rs`
- Modify: `backend/src/bot/policy.rs`

- [ ] 为 claim 新增失败测试，覆盖“破坏高番路线时应 pass / 稳定成型高番路线时应 claim”
- [ ] 让 `claim_action_bonus` 与 `choose_claim_action` 同时读取路线保留、闭手依赖、开门收益
- [ ] 运行 `cargo test bot::avoids_low_value_chow_that_breaks_eight_fan_progress bot::takes_value_honor_pung_when_it_forms_an_eight_fan_route -- --nocapture`

### Task 2: Rollout 显式放铳 EV

**Files:**
- Modify: `backend/src/bot/search.rs`

- [ ] 为 rollout 新增失败测试，覆盖“高危牌在 rollout 中被进一步压低”
- [ ] 在 `simulate_rollout_after_discard` / `best_rollout_discard_from_counts` 加入显式 deal-in EV 扣分
- [ ] 运行 `cargo test bot::monte_carlo_rollout_prefers_safer_honor_discard_under_threat -- --nocapture`

### Task 3: 高价值路线库扩展

**Files:**
- Modify: `backend/src/bot/search.rs`

- [ ] 为新增路线补失败测试，优先覆盖清幺九/混幺九/十三幺方向与对对和复合方向
- [ ] 扩展 `strategic_signals` 的番估计与 route bonus，但限制为 tile count 级别启发式
- [ ] 运行新增路线测试与现有 route 测试

### Task 4: 对手听型倾向细化

**Files:**
- Modify: `backend/src/bot/search.rs`

- [ ] 为危险度补失败测试，覆盖边张/嵌张/两面倾向与最近舍牌形状影响
- [ ] 扩展 `OpponentThreat` / `discard_danger_penalty`，细化 wait-shape 风险
- [ ] 运行新增危险度测试

### Task 5: 分数目标模型细化

**Files:**
- Modify: `backend/src/bot/search.rs`

- [ ] 为 mode / goal profile 新增失败测试，覆盖大领先守成、中幅落后追点、末盘名次压力
- [ ] 把当前粗粒度 `BotMode` / `ModeProfile` 扩成更连续的 placement pressure 模型
- [ ] 运行新增 mode/profile 测试

### Task 6: 总体验证

**Files:**
- Verify only

- [ ] 运行 `cargo test bot:: -- --nocapture`
- [ ] 复查实现是否只引入常数级启发式与缓存访问，无新增深层搜索
- [ ] 总结剩余可继续优化但暂不值得加 CPU 的方向
