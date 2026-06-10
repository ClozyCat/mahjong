# Bot Training Promotion Report Upgrade Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 升级 BOT 训练晋级报告，让候选模型晋级决策能看到 paired 置信区间、稳定性、延迟分布、失败明细与后续优化建议。

**Architecture:** 在 `arena_summary.py` 生成更丰富的统计摘要，在 `candidate_gate.py` 保持现有晋级门槛语义但输出结构化报告，在 `candidate_selector.py` 把关键报告指标带入候选排序产物。测试集中放在现有 `test_rl_dataset.py`，避免新增测试入口。

**Tech Stack:** Python 3, pytest, JSONL arena report, existing BOT trainer scripts.

---

### Task 1: 固定晋级报告新增行为

**Files:**
- Modify: `backend/bot_trainer/v2/test_rl_dataset.py`

- [x] **Step 1: 为 paired 统计增加置信区间断言**

覆盖 `paired_subjects` 输出中的样本标准差、标准误、95% CI、正向比例、最小/最大 delta。

- [x] **Step 2: 为策略延迟分布增加断言**

基于每个 seat 的平均决策延迟，断言 summary 输出 `latency_ms_p50`、`latency_ms_p95`、`latency_ms_max`。

- [x] **Step 3: 为 gate 结构化失败明细增加断言**

断言 `failure_details` 包含 metric、baseline、candidate、threshold、margin，并且 `promotion_report` 包含 margin 与 paired confidence 字段。

- [x] **Step 4: 为 selector 保留报告字段增加断言**

断言候选摘要包含 `paired_avg_score_delta`、`paired_confidence95_low`、`paired_positive_delta_rate` 与 `promotion_report`。

### Task 2: 扩展 arena summary 统计

**Files:**
- Modify: `backend/bot_trainer/v2/arena_summary.py`

- [x] **Step 1: 采集策略 seat 级延迟样本**

每个 seat 使用 `decision_latency_ms_sum / decision_count` 作为样本，只在 `decision_count > 0` 时纳入。

- [x] **Step 2: 输出延迟分布**

输出 p50、p95、max 和样本数，保留原有 `avg_latency_ms_per_decision`。

- [x] **Step 3: 扩展 paired score delta 统计**

输出样本标准差、标准误、95% CI、正向比例、最小/最大 delta，保留原有 `deltas` 字段。

### Task 3: 扩展 candidate gate 报告

**Files:**
- Modify: `backend/bot_trainer/v2/candidate_gate.py`

- [x] **Step 1: 增加统一 margin 与 failure detail helper**

把现有门槛判断转换成可读的结构化报告，保持 `failures` 字段兼容。

- [x] **Step 2: 构造 `promotion_report`**

输出 `metrics`、`paired`、`latency`、`claim_rate`、`warnings`，让训练流水线能直接落盘审阅。

- [x] **Step 3: 保持晋级语义不变**

不因置信区间低于 0 自动拒绝，只在 warnings 中提示样本稳定性风险。

### Task 4: 扩展 candidate selector 汇总

**Files:**
- Modify: `backend/bot_trainer/v2/candidate_selector.py`

- [x] **Step 1: 透传 gate 报告摘要**

在每个候选摘要中保留 `promotion_report`，并提取 paired 核心指标。

- [x] **Step 2: 增加 selected 顶层报告字段**

让最终选择结果也暴露 `promotion_report` 与 paired 指标，方便上层脚本展示。

### Task 5: 全局同步检查

**Files:**
- Modify if needed: `backend/src/bot_trainer/export.rs`
- Modify if needed: `docs/superpowers/plans/2026-06-10-bot-training-promotion-report-upgrade.md`

- [x] **Step 1: 扫描旧 schema/output 命名**

查找 v2/v3、`fan_value`、`risk_mask`、promotion/candidate 文档和测试命名是否不一致。

- [x] **Step 2: 修复低风险不一致**

只处理测试命名、局部文档说明等低风险同步问题，不扩张到训练逻辑重构。

- [x] **Step 3: 记录剩余优化项**

把不适合本轮直接改的优化项写入本文档的全局检查结果。

### Task 6: 验证

**Files:**
- Test: `backend/bot_trainer/v2/test_rl_dataset.py`

- [x] **Step 1: 运行定向 pytest**

Run: `python -m pytest backend/bot_trainer/v2/test_rl_dataset.py -q`

- [x] **Step 2: 运行 BOT trainer Python 回归**

Run: `python -m pytest backend/bot_trainer/v2/test_dataset.py backend/bot_trainer/v2/test_model.py backend/bot_trainer/v2/test_rl_dataset.py -q`

- [x] **Step 3: 运行 Rust bot_trainer 回归**

Run: `cargo test --manifest-path backend/Cargo.toml bot_trainer -- --nocapture`

- [x] **Step 4: 运行格式空白检查**

Run: `git diff --check`

---

## Global Follow-up Review

本轮扫描范围：

- `backend/bot_trainer/v2`
- `backend/src/bot_trainer`
- `docs/superpowers/plans/2026-06-10-bot-training-promotion-report-upgrade.md`

已处理的低风险不一致：

- `backend/bot_trainer/v2/README.md` 补充 schema v3、`risk_mask`、`fan_target`、`fan_value`、paired CI、延迟分布与 `promotion_report` 说明。
- `backend/bot_trainer/v2/README.md` 将晋级验收的放铳容忍度从“2 个百分点”同步为当前 gate 实际使用的“1 个百分点”。
- `backend/src/bot_trainer/export.rs` 将测试名从 `metadata_contains_v2_model_outputs` 同步为 `metadata_contains_v3_model_outputs`。

保留不改的历史记录：

- `docs/superpowers/plans/2026-04-28-bot-training-v2.md` 等旧计划文件仍含 schema v2 示例。这些文件描述历史实施路径，不作为当前训练合约。

后续可优化项：

1. **决策级延迟分布。** 当前 `arena_summary.py` 只能用 seat 级平均延迟样本估算 p50/p95/max。若要更准确识别长尾卡顿，需要 Rust arena report 输出单决策延迟直方图或分位数。
2. **paired CI 硬门槛。** 本轮只把 `confidence95_low <= 0` 作为 warning，不改变晋级语义。正式 promotion 可以增加可配置硬门槛，例如样本数达到阈值后要求 paired 95% CI 下界大于 0。
3. **selector 排序加入稳定性。** 当前 `candidate_selector.py` 仍按原来的 accepted、score、win、tenpai、deal-in、latency margin 排序。多候选多 seed 稳定后，可以把 `paired_confidence95_low` 与 `paired_positive_delta_rate` 纳入次级排序。
4. **报告模块拆分。** `candidate_gate.py` 已接近一个文件承担 gate 和报告两类职责。若继续增加指标，应拆出 `promotion_report.py`，让 gate 保持薄逻辑。
