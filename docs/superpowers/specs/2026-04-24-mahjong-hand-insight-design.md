# 麻将“后端手牌洞察与推荐番型”功能设计方案

**日期**：2026-04-24  
**状态**：已完成设计确认，待用户审阅  
**范围**：将“听牌提示”改为后端权威投影，并在手牌区右下角 `i` 浮窗中新增“推荐番型”能力，支持当前手牌与“打出某张牌后”的预览结果，且推荐评估必须考虑副露

## 1. 背景与目标

当前项目已经具备以下基础能力：

- 后端已具备完整的摸打、吃碰杠和、听牌宣告、自动化动作、结算与计番链路。
- 前端底部动作区右下角已经存在“听牌提示” `i` 浮窗入口。
- 前端当前可本地推导“当前听什么”和“打出所选牌后听什么”。
- 后端计番 evaluator 已注册完整番种表，且可在带副露、带明暗信息的前提下做真实计番。

但当前实现仍有两个明显问题：

- “听牌提示”由前端本地推导，和后端权威规则存在双轨风险。
- 底部 `i` 浮窗只能显示 waits，无法表达玩家当前更接近哪些高番路线。

本次设计目标如下：

- 将“听牌提示”完全迁移为后端权威投影，删除前端本地推导链。
- 在同一个 `i` 浮窗中加入“推荐番型”展示。
- 推荐结果既支持当前手牌，也支持“打出某张牌后”的预览结果。
- 所有推荐评估都必须显式考虑副露、门清状态、可见牌与活牌供给。
- 前端只负责选中态切换和渲染，不再自行推导 waits 或推荐番型。

## 2. 已确认的范围边界

本次设计已确认以下约束：

- 浮窗入口继续放在手牌区右下角现有“和牌提示” `i` 图标位置。
- 默认状态下，`i` 图标使用较高透明度的黑色视觉。
- 当当前显示的 insight 存在 waits 时，`i` 图标沿用现在这种更强调“听牌提示”的视觉形式。
- 未听牌时，展开浮窗仅显示推荐番型。
- 听牌时，展开浮窗同时显示推荐番型、正在听的牌以及其数量。
- 推荐番型默认最多展示 6 条，允许少于 6 条。
- 推荐项展示格式固定为“中文番名 + 百分比”，例如 `清一色 79%`。
- 推荐评估范围覆盖所有 `fan_value >= 4` 的非兜底番种；`chicken_hand` 作为兜底结算番型不进入推荐展示。
- “选中某张牌时”需要切换成“打出该牌后”的 insight。
- “听牌提示”必须改由后端传输；前端旧的本地推导代码需要删除。

## 3. 方案对比与结论

### 3.1 推荐方案：后端一次性下发当前与按弃牌预览的双层 insight

做法：

- 后端在 `room_snapshot` 的本家私有视图中直接下发 `hand_insights`。
- `hand_insights` 同时包含：
  - `current`：当前应该显示的基础 insight
  - `by_discard_tile_id`：每张可合法打出手牌对应的预览 insight
- 前端只在本地做“当前展示哪一份 insight”的选择，不自行计算 waits 或推荐。

优点：

- 前端切换选牌时无额外往返，交互稳定。
- waits 与推荐番型都以后端权威为准，不会和规则链路分叉。
- 不需要新增新的 action/request 协议，兼容现有快照驱动结构。

缺点：

- `room_snapshot` 的体积会增加，但单手 13 到 14 张牌的范围可控。

### 3.2 备选方案：前端选牌后向后端请求单次 preview

做法：

- 当前快照只下发基础 insight。
- 每次选中一张牌时，前端再向后端发送 preview 请求。

问题：

- 每次点牌都需要一次往返，底部浮窗会出现明显抖动和延迟。
- 需要新增 preview 协议、缓存与竞态治理，复杂度明显升高。
- 对当前项目这种高频手牌选中交互来说，收益明显低于成本。

该方案不采用。

### 3.3 备选方案：仅把推荐番型后端化，waits 保留前端本地推导

做法：

- waits 继续由前端算。
- 推荐番型由后端算。

问题：

- 仍然保留两套手牌判断链路。
- 前端与后端可能对“是否听牌”“有哪些 waits”产生分歧。
- 不符合本次明确要求“听牌提示也改成后端传输”。

该方案不采用。

### 3.4 最终结论

本次采用“后端一次性下发当前与按弃牌预览的双层 insight”方案。

## 4. 数据模型与协议设计

### 4.1 新增快照字段位置

在 [room_snapshot.rs](c:/Users/Claude/Desktop/mahjong_full/backend/src/projection/room_snapshot.rs) 的 `private_state` 中新增字段：

- `hand_insights`

该字段只对本家私有视图有意义，不向其他座位做公共投影。

### 4.2 数据结构

推荐新增以下投影结构：

```ts
interface HandInsightsView {
  current: HandInsightView | null;
  by_discard_tile_id: Record<string, HandInsightView>;
}

interface HandInsightView {
  discard_tile_id: string | null;
  discard_tile_code: string | null;
  is_tenpai: boolean;
  waits: HandInsightWaitView[];
  recommendations: HandInsightRecommendationView[];
}

interface HandInsightWaitView {
  code: string;
  available_count: number;
}

interface HandInsightRecommendationView {
  fan_key: string;
  fan_value: number;
  similarity_percent: number;
}
```

字段语义如下：

- `current`
  - 当前未选牌时默认展示的 insight。
  - 若本家已 `ready_hand` 且存在本回合刚摸入的锁定弃牌，则 `current` 按“移除该锁定弃牌后”的听牌状态计算。
- `by_discard_tile_id`
  - key 为本家当前所有可合法打出的 `tile_id`。
  - value 为“若立即打出这张牌”时对应的 waits 与推荐番型。
- `discard_tile_id` / `discard_tile_code`
  - `current` 为 `null`；预览 insight 记录对应的弃牌对象，供前端标识标题与调试。
- `is_tenpai`
  - 当前这份 insight 是否存在 waits。
- `waits`
  - 结构与现有前端 `ReadyHandWaitView` 语义一致，但改为后端下发。
- `recommendations`
  - 已按相似度从高到低排序，最大长度 6。

### 4.3 快照产出规则

`hand_insights` 的产出规则明确如下：

- 本家没有私有手牌时，`hand_insights = null`。
- `current` 永远尝试生成；即使 `waits` 为空，也应返回推荐番型结果。
- `by_discard_tile_id` 只为当前合法弃牌集合生成：
  - 不包含花牌。
  - 不包含同回合受 `restricted_discard_tile_ids` 限制的牌。
  - 不包含在已 `ready_hand` 锁手状态下不允许自由打出的历史暗牌。
- 当本家已 `ready_hand` 且当前主动回合存在 `drawn_tile_id` 时：
  - `current` 使用“移除本次锁定摸牌后的基础牌型”计算 waits 和推荐。
  - `by_discard_tile_id` 可以为空，因为此时前端不再允许自由选牌改变结果。

### 4.4 示例 payload

```json
{
  "hand_insights": {
    "current": {
      "discard_tile_id": null,
      "discard_tile_code": null,
      "is_tenpai": false,
      "waits": [],
      "recommendations": [
        { "fan_key": "full_flush", "fan_value": 24, "similarity_percent": 79 },
        { "fan_key": "all_pungs", "fan_value": 6, "similarity_percent": 56 }
      ]
    },
    "by_discard_tile_id": {
      "w9#discard": {
        "discard_tile_id": "w9#discard",
        "discard_tile_code": "w9",
        "is_tenpai": true,
        "waits": [
          { "code": "w3", "available_count": 2 },
          { "code": "w6", "available_count": 1 }
        ],
        "recommendations": [
          { "fan_key": "full_flush", "fan_value": 24, "similarity_percent": 83 },
          { "fan_key": "pure_straight", "fan_value": 16, "similarity_percent": 50 }
        ]
      }
    }
  }
}
```

## 5. 后端权威 waits 计算设计

### 5.1 当前 insight 的 waits

当前 insight 的 waits 计算规则如下：

- 普通未听牌状态：
  - 直接基于当前暗手牌与副露判断是否已经结构性听牌。
- 已 `ready_hand` 且当前主动回合存在 `drawn_tile_id`：
  - 先从暗手牌中移除本次摸入牌，再计算 waits。
  - 这样可以持续展示“锁手后的真实听牌集合”，与现有交互语义保持一致。
- 已 `ready_hand` 但无当前摸牌：
  - 直接基于当前锁定手牌计算 waits。

### 5.2 预览 insight 的 waits

每个 `by_discard_tile_id[tileId]` 的 waits 计算规则如下：

- 从暗手牌中移除该 `tile_id` 对应的牌。
- 保留当前所有副露与副露开闭语义。
- 基于移除后的 concealed tiles + melds 判断所有可和牌张。
- `available_count` 使用后端可见牌统计计算：
  - 以场上公开弃牌、副露、花牌、已知公共信息为基础。
  - 预览时还需将“拟打出的这张牌”计入已知牌，确保 live count 正确。

### 5.3 与现有能力的关系

后端 waits 计算不重新发明一套手牌判定逻辑，直接复用：

- [room_scoring.rs](c:/Users/Claude/Desktop/mahjong_full/backend/src/room_scoring.rs) 的牌面缓存与可见牌统计
- [ready_hand.rs](c:/Users/Claude/Desktop/mahjong_full/backend/src/rules/standard/ready_hand.rs) 的结构性听牌判断语义
- [evaluator.rs](c:/Users/Claude/Desktop/mahjong_full/backend/src/rules/scoring/evaluator.rs) / 分解入口对带副露手牌的和牌分解能力

## 6. 推荐番型候选集设计

### 6.1 候选来源

推荐番型候选集来源于已注册番表中所有满足以下条件的规则：

- `fan_value >= 4`
- 不是兜底番型 `chicken_hand`

这意味着推荐评估覆盖但不限于以下类别：

- 花色路线：`full_flush`、`half_flush`
- 结构路线：`all_pungs`、`seven_pairs`、`pure_straight`
- 高番组合：`mixed_triple_chow`、`mixed_straight`、`three_kongs`
- 门清与副露相关路线：`fully_concealed_hand`、`melded_hand`
- 特殊型：`thirteen_orphans`、`nine_gates`
- 时机 / 事件类高番：`out_with_replacement_tile`、`last_tile_draw`、`last_tile_claim`、`robbing_the_kong`

### 6.2 推荐输出数量

推荐结果输出规则如下：

- 先按 `similarity_percent` 从高到低排序。
- 再按 `fan_value` 从高到低作为次级排序。
- 默认最多返回 6 条。
- 低于展示阈值 `20%` 的推荐项不返回。
- 因为存在阈值过滤，最终返回数量允许少于 6 条。

## 7. 相似度算法设计

### 7.1 总体策略

相似度计算分为两层：

- 精确层：适用于已经听牌，或某个预览弃牌后会进入听牌的 insight。
- 启发式层：适用于尚未进入听牌的 insight。

统一输出范围为 `0..100` 的整数百分比。

### 7.2 精确层：基于 live waits 的真实和牌覆盖率

当某份 insight 的 `waits` 非空时，对每个候选番型使用如下精确公式：

```text
similarity_percent =
  round(
    100 *
    sum(available_count of waits whose hypothetical win contains target fan)
    /
    sum(available_count of all waits)
  )
```

计算流程如下：

1. 枚举该 insight 的全部 waits。
2. 对每个 `wait.code` 构造一手“补入该牌后的真实和牌手”。
3. 调用后端现有 scoring evaluator 做真实计番，保留副露与明暗信息。
4. 若结果 `fan_keys` 包含目标 `fan_key`，则该 wait 的 `available_count` 记入覆盖分子。
5. 使用全部 waits 的 `available_count` 总和作为分母。

该算法天然满足以下要求：

- waits 越多、覆盖越广，相似度越高。
- 某番型只覆盖部分听牌张时，相似度会被拉低。
- 某番型覆盖全部 live waits 时，相似度可接近或达到 `100%`。
- 副露、门清、杠信息直接由 evaluator 参与判断，不会遗漏。

### 7.3 启发式层：尚未听牌时的路线逼近度

当某份 insight 的 `waits` 为空时，使用启发式相似度评估。启发式输出统一按以下三段合成：

```text
heuristic_score =
  clamp(
    route_progress_score * 0.55 +
    live_supply_score * 0.25 +
    rule_compatibility_score * 0.20,
    0,
    100
  )
```

其中：

- `route_progress_score`
  - 当前手牌距离该番型核心结构还有多远。
- `live_supply_score`
  - 完成这条路线所需关键牌在可见牌压力下还剩多少空间。
- `rule_compatibility_score`
  - 当前门清 / 副露 / 杠状态是否与该番型兼容。

### 7.4 启发式层的兼容性硬规则

以下情况直接将目标番型相似度置为 `0`：

- 已存在明露副露时，不可能再成立：
  - `fully_concealed_hand`
  - `seven_pairs`
  - `seven_shifted_pairs`
  - `thirteen_orphans`
  - `nine_gates`
- 已存在吃牌副露时，不可能再成立纯刻子特化的某些闭合路线时，按具体规则归零。
- 当前牌张集合已经违反目标番型硬约束，例如：
  - `all_green` 出现非绿色牌
  - `all_terminals` 出现字牌或中张
  - `upper_tiles` / `lower_tiles` / `middle_tiles` 出现不允许的数字

### 7.5 启发式层的路线分组

为保持实现收敛，番型按以下家族共用评估器：

- 花色集中家族
  - `full_flush`
  - `half_flush`
  - `pure_straight`
  - `pure_shifted_chows`
  - `pure_triple_chow`
  - `four_pure_shifted_chows`
  - `pure_shifted_pungs`
  - `four_pure_shifted_pungs`
  - `pure_terminal_chows`
  - `nine_gates`
- 对子闭合家族
  - `seven_pairs`
  - `seven_shifted_pairs`
- 刻子 / 杠子家族
  - `all_pungs`
  - `all_even_pungs`
  - `triple_pung`
  - `three_concealed_pungs`
  - `four_concealed_pungs`
  - `two_melded_kongs`
  - `three_kongs`
  - `four_kongs`
  - `melded_hand`
- 混合顺子家族
  - `mixed_triple_chow`
  - `mixed_straight`
  - `mixed_shifted_chows`
  - `three_suited_terminal_chows`
  - `knitted_straight`
  - `lesser_honours_and_knitted_tiles`
  - `greater_honours_and_knitted_tiles`
- 字牌 / 幺九 / 特殊牌家族
  - `all_honours`
  - `all_terminals_and_honours`
  - `all_terminals`
  - `all_green`
  - `little_three_dragons`
  - `big_three_dragons`
  - `little_four_winds`
  - `big_four_winds`
  - `all_types`
  - `all_fives`
  - `outside_hand`
  - `upper_four`
  - `upper_tiles`
  - `lower_four`
  - `lower_tiles`
  - `middle_tiles`
- 时机 / 事件家族
  - `out_with_replacement_tile`
  - `last_tile`
  - `last_tile_draw`
  - `last_tile_claim`
  - `robbing_the_kong`
  - `fully_concealed_hand`

### 7.6 各家族的核心输入

各评估器统一只读取以下后端可得信息：

- 暗手牌 tile counts
- 全部副露 groups
- 每个副露是否明露
- 当前是否 `ready_hand`
- 当前是否存在可用自杠 / 补杠候选
- 可见牌数量与每张牌的剩余 live count
- 对子数、刻子数、顺子密度、字牌数、幺九数
- 主花色集中度与离散度

### 7.7 时机 / 事件类高番的非零前置条件

为避免出现无意义推荐，时机 / 事件类番型仅在满足可观察前置条件时才允许非零：

- `out_with_replacement_tile`
  - 当前存在合法自杠 / 补杠候选，且杠后路线仍未被破坏。
- `last_tile`
  - 目标 waits 中存在绝张场景，或当前关键牌 live count 已接近 1。
- `last_tile_draw`
  - 牌墙剩余张数已进入末段，且当前 insight 已听牌或接近听牌。
- `last_tile_claim`
  - 牌墙剩余张数已进入末段，且当前为荣和友好路线。
- `robbing_the_kong`
  - 至少一名对手已有可补杠风险的明刻结构，且该升级牌和本 insight 的可能 waits 存在交集。
- `fully_concealed_hand`
  - 当前无明露副露，且路线仍保持门清兼容。

若上述前置条件不满足，对应番型相似度直接为 `0`。

## 8. 副露处理设计

推荐评估必须显式考虑副露，具体规则如下：

- 所有真实和牌枚举都以当前 `meld_tile_key_groups` 和 `open_meld_tile_key_groups` 为输入。
- 明露会降低或归零依赖门清的番型相似度。
- 已形成的副露若与某路线直接一致，则可显著提升该路线相似度，例如：
  - 明刻 / 明杠提升 `all_pungs`、`two_melded_kongs`、`three_kongs`
  - 单一花色副露提升 `half_flush`、`full_flush`
  - 已经四副明露只剩单钓时提升 `melded_hand`
- 已吃牌会弱化七对、十三幺、九莲宝灯等闭合特化路线。
- 已明杠 / 补杠要在推荐里保留对杠类高番的正向影响。

## 9. 前端展示与切换规则

### 9.1 insight 选择规则

前端 [matchViewModel.ts](c:/Users/Claude/Desktop/mahjong_full/frontend/src/lib/matchViewModel.ts) 只保留以下轻逻辑：

- 若当前没有 `hand_insights`，则底部 `i` 浮窗不显示。
- 若只选中 1 张牌，且该 `tile_id` 存在于 `by_discard_tile_id` 中：
  - 优先展示该预览 insight。
- 否则：
  - 展示 `current`。

前端不再自行调用任何 waits 计算函数。

### 9.2 图标视觉状态

`i` 图标视觉遵循“当前正在展示的 insight”：

- 若当前展示的 insight `is_tenpai === false`
  - 使用高透明黑色 `i` 图标。
- 若当前展示的 insight `is_tenpai === true`
  - 使用现有更强调“听牌提示”的视觉样式。

这意味着：

- 已真实听牌时保持当前视觉。
- 选中某张牌后若“打出该牌会进入听牌”，图标也切换为强调状态。

### 9.3 浮窗内容规则

浮窗内容拆为两段：

- `正在听` / `打出后将听`
- `推荐番型`

显示细则如下：

- insight `is_tenpai === false`
  - 只显示 `推荐番型`。
- insight `is_tenpai === true` 且 `discard_tile_id === null`
  - 标题显示 `正在听`，并展示 waits。
- insight `is_tenpai === true` 且 `discard_tile_id !== null`
  - 标题显示 `打出后将听`，并展示 waits。
- 推荐番型始终显示在 waits 下方。

### 9.4 推荐列表表现

推荐列表每行固定显示：

- 左侧：番型中文名
- 右侧：`XX%`

不在浮窗内额外显示长解释，不打断紧凑度。番型中文名继续复用 [fanGuide.ts](c:/Users/Claude/Desktop/mahjong_full/frontend/src/components/battle-screen/fanGuide.ts) 的映射。

## 10. 前端删除与迁移范围

以下前端本地推导能力需要移除：

- [readyHand.ts](c:/Users/Claude/Desktop/mahjong_full/frontend/src/lib/readyHand.ts)
- [matchViewModel.ts](c:/Users/Claude/Desktop/mahjong_full/frontend/src/lib/matchViewModel.ts) 中：
  - `createReadyHandInsight`
  - `getReadyHandWaitsForLocalPlayer`
  - 依赖本地 waits 推导的辅助函数链

迁移后的职责分布如下：

- 后端：生成 waits 与推荐番型
- 前端类型层：接收 `hand_insights`
- 前端 view model：选择显示哪一份 insight
- 前端 dock 组件：渲染 waits 与推荐列表

## 11. 主要影响区域

### 11.1 后端

预计影响但不限于以下区域：

- [backend/src/projection/room_snapshot.rs](c:/Users/Claude/Desktop/mahjong_full/backend/src/projection/room_snapshot.rs)
- [backend/src/projection/support.rs](c:/Users/Claude/Desktop/mahjong_full/backend/src/projection/support.rs)
- [backend/src/room_scoring.rs](c:/Users/Claude/Desktop/mahjong_full/backend/src/room_scoring.rs)
- 新增建议文件：
  - `backend/src/projection/hand_insight.rs`

### 11.2 前端

预计影响但不限于以下区域：

- [frontend/src/types/match.ts](c:/Users/Claude/Desktop/mahjong_full/frontend/src/types/match.ts)
- [frontend/src/lib/matchViewModel.ts](c:/Users/Claude/Desktop/mahjong_full/frontend/src/lib/matchViewModel.ts)
- [frontend/src/lib/readyHand.ts](c:/Users/Claude/Desktop/mahjong_full/frontend/src/lib/readyHand.ts)
- [frontend/src/components/battle-screen/BottomActionDock.tsx](c:/Users/Claude/Desktop/mahjong_full/frontend/src/components/battle-screen/BottomActionDock.tsx)
- [frontend/src/components/battle-screen/BottomActionDock.test.tsx](c:/Users/Claude/Desktop/mahjong_full/frontend/src/components/battle-screen/BottomActionDock.test.tsx)
- [frontend/src/styles/dock.css](c:/Users/Claude/Desktop/mahjong_full/frontend/src/styles/dock.css)
- [frontend/src/components/battle-screen/fanGuide.ts](c:/Users/Claude/Desktop/mahjong_full/frontend/src/components/battle-screen/fanGuide.ts)

## 12. 测试与验证策略

### 12.1 后端测试

至少覆盖以下场景：

- `room_snapshot` 能投出 `hand_insights.current`
- `room_snapshot` 能为所有合法弃牌生成 `by_discard_tile_id`
- 预览 insight 会把拟打出的牌计入可见牌，从而得到正确 `available_count`
- 已听牌时，`current.waits` 与现有锁手语义一致
- 已副露时，不再推荐不可能成立的门清 / 七对 / 十三幺类番型
- 推荐结果按相似度排序，最多 6 条，可少于 6 条
- 听牌 insight 的精确相似度按 live waits 覆盖率计算
- `out_with_replacement_tile`、`last_tile_draw`、`robbing_the_kong` 等时机番型只有在前置条件满足时才允许非零

### 12.2 前端测试

至少覆盖以下场景：

- 默认 insight 非听牌时，显示高透明黑色 `i`
- 当前 insight 听牌时，保持现有强调视觉
- 展开浮窗后能渲染推荐番型百分比
- 听牌时同一浮窗能同时渲染 waits 与推荐番型
- 选中不同手牌时，会切换到对应 `by_discard_tile_id`
- 未命中 `by_discard_tile_id` 时，回退到 `current`
- 删除本地推导后，`BottomActionDock` 仍可完整工作

### 12.3 完成前验证

实现完成后至少执行：

- 后端相关 `cargo test`
- 前端相关 `vitest`
- 一次前端 build 或等价打包验证
- 一次联调式冒烟验证，确认：
  - 未听牌时可看推荐番型
  - 选牌后能切换预览 insight
  - 听牌后可同时看 waits 与推荐番型

## 13. 风险与控制

主要风险如下：

- 快照体积增加后，若实现无缓存，可能导致每次快照构建成本过高。
- 若启发式层与精确层的排序风格差异过大，玩家在“刚进听牌前后”可能感觉推荐突变。
- 时机 / 事件类高番若缺少前置条件约束，容易产生误导性推荐。

控制策略如下：

- 复用 [room_scoring.rs](c:/Users/Claude/Desktop/mahjong_full/backend/src/room_scoring.rs) 缓存与现有 evaluator，避免重复分解。
- 对已听牌 / 可听牌 insight 优先走精确层，减少高价值阶段的误差。
- 对时机类高番加明确的非零前置条件，不满足即归零。
- 前端不保留任何本地 waits 推导，保证单一权威来源。

## 14. 最终设计结论

本方案将底部 `i` 浮窗升级为“后端手牌洞察”入口，统一承载两类信息：

- 后端权威的听牌 waits
- 后端评估的推荐番型

后端一次性下发：

- `current`
- `by_discard_tile_id`

前端仅负责：

- 根据当前选牌态选择显示哪份 insight
- 按约定视觉规则渲染 `i` 图标与浮窗

推荐相似度采用“两层算法”：

- 已听牌或可听牌时，按 live waits 的真实和牌覆盖率精确计算
- 尚未听牌时，按结构进度、活牌供给与规则兼容性做启发式估计

整个评估过程必须显式考虑副露、门清与可见牌信息。实现完成后，前端旧的本地 waits 推导链应被完整删除，以保证“听牌提示”与“推荐番型”都由后端统一裁决。
