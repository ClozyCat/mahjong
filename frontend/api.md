# 当前前端联机协议与状态文档

## 1. 文档目标

这份文档面向“重写前端”的工程场景，目标不是描述理想接口，而是还原当前代码里的真实前后端交互。

覆盖范围：

- 前端入口、连接流程、状态来源与本地持久化
- HTTP / WebSocket 接口
- 所有当前已实现的消息类型、数据结构、字段含义
- 前端基于协议做出的派生逻辑
- 当前 transport contract 与前端 TypeScript 类型之间的已知偏差

本文件以当前仓库源码为准，不以旧文档或历史实现为准。

## 2. 总体架构

当前前端是一个单页 React 应用，核心流程如下：

1. 通过 `POST /api/tables` 创建牌桌。
2. 通过 `ws://<host>/ws/{table_code}` 建立 WebSocket。
3. 首次连接发送 `join_table`；断线重连发送 `reconnect`。
4. 之后几乎所有状态都依赖服务器推送：
   - `room_snapshot`
   - `action_prompt`
   - `round_event`
   - `match_result`
   - `player_presence`
   - `quick_chat`
   - `action_rejected`
   - `leave_table_accepted`

关键原则：

- HTTP 只用于 bootstrap，不用于同步牌局状态。
- `room_snapshot` 是唯一完整权威状态。
- `round_event` 主要用于动画和瞬时反馈，不适合单独恢复状态。
- `action_prompt` 主要用于“提醒当前该你操作了”和启动倒计时，不是完整状态源。
- 当前没有 spectator 模式。收到 `room_snapshot` 的连接一定已经拥有某个座位。

## 3. 当前前端运行时行为

### 3.1 基础地址

前端默认从以下来源决定后端地址：

- `VITE_API_BASE_URL`
- `VITE_WS_BASE_URL`
- 如果环境变量未设置：
  - `apiBaseUrl = window.location.origin`
  - `wsBaseUrl = ws(s)://window.location.host`

当前前端代码：

- 创建牌桌使用 `apiBaseUrl`
- 进入牌桌、加入牌局、重连牌局都使用 `wsBaseUrl`

### 3.2 本地持久化

当前前端会把以下内容保存到 `localStorage`：

- `mahjong:session`
  - `tableCode`
  - `nickname`
  - `reconnectToken`
  - `wsBaseUrl`
- `mahjong:theme`

刷新页面后的恢复方式不是重新走 HTTP，而是：

1. 从 `mahjong:session` 读取历史会话
2. 直接重连 WebSocket
3. 发送 `reconnect`

### 3.3 计时与自动化

当前实现中的重要时间参数：

- 前端心跳间隔：20 秒
- 服务端断线保座宽限期：120 秒
- 服务端继续操作自动推进倒计时：30 秒
- 服务端 bot 动作延迟：
  - `normal` / `skill` 模式：600ms
  - `test` 模式：0ms

### 3.4 牌桌模式

当前前后端都支持以下模式：

- `normal`
- `skill`
- `test`

模式差异：

- `normal`
  - 常规流程
  - 4 个座位都准备后手动开始
- `skill`
  - 在奇数局开局阶段进入技能抽取流程
  - 前端需要处理 `skill_draft`、`equipped_skills`、`visible_effects`、`private_knowledge`
- `test`
  - 第一个真人加入后，后端立即补满 bot 并直接开局
  - 前端通常不会长期停留在等待房间

### 3.5 八番起胡配置

创建牌桌时可传 `enforce_minimum_eight_fan`：

- `true`: 八番起胡
- `false`: 放宽限制

该配置不会单独出现在 `room_snapshot` 顶层，而是内化在后端规则状态里。

## 4. HTTP API

## 4.1 `GET /api/health`

用途：

- 存活检查

响应：

```json
{
  "status": "ok"
}
```

当前前端里存在调用封装，但主流程里并不依赖它。

## 4.2 `POST /api/tables`

用途：

- 创建新牌桌
- 可指定牌桌号
- 可指定模式

请求体字段：

- `table_code?: string`
  - 可选
  - 不传则后端自动生成
  - 服务端会转成大写
  - 只允许 `A-Z0-9`，长度 `1..12`
- `mode?: "normal" | "skill" | "test"`
  - 可选
  - 不传时由后端默认值决定；当前默认通常为 `normal`
- `test_mode?: boolean`
  - 兼容历史字段
  - 当前前端不再使用
  - `true -> test`，`false -> normal`
- `enforce_minimum_eight_fan?: boolean`
  - 可选
  - 默认 `true`

当前前端实际发送格式：

```json
{
  "table_code": "AB12CD",
  "mode": "skill",
  "enforce_minimum_eight_fan": true
}
```

成功响应：`201 Created`

```json
{
  "table_code": "AB12CD",
  "phase": "waiting",
  "mode": "skill",
  "created_at": "2026-04-17T13:00:00.000000Z",
  "seats": []
}
```

字段含义：

- `table_code`
  - 牌桌号
  - HTTP 创建后，前端会用它拼 WebSocket URL
- `phase`
  - 当前固定为 `waiting`
- `mode`
  - 创建出的牌桌模式
- `created_at`
  - ISO 时间字符串
- `seats`
  - 当前后端直接复用 `SeatState` 序列化
  - 但新建牌桌时实际基本总是空数组

错误状态：

- `409 Conflict`
  - `{"detail":"table_code_exists"}`
- `422 Unprocessable Entity`
  - `{"detail":"invalid_table_code"}`
  - `{"detail":"unsupported_mode"}`
- `500 Internal Server Error`
  - `{"detail":"..." }`

当前前端创建牌桌时的行为：

- 如果 `409 + table_code_exists`
  - 会弹窗询问是否直接加入这个已存在牌桌
- 其余错误直接展示文本错误

重要事实：

- 当前没有 `GET /api/tables/{code}` 之类的接口。
- “加入已有牌桌”不经过 HTTP，而是直接连 WebSocket 并发 `join_table`。

## 5. WebSocket 总览

### 5.1 连接地址

```text
/ws/{table_code}
```

示例：

```text
ws://localhost:8000/ws/AB12CD
```

### 5.2 统一消息包裹

客户端与服务端绝大多数消息都使用：

```json
{
  "type": "message_type",
  "payload": {}
}
```

例外说明：

- `round_event.payload.event` 内部还有一个子对象，它自己的 `type` 只是事件内部类型，不是外层 WebSocket 消息类型。

### 5.3 当前前端连接流程

创建牌桌：

1. `POST /api/tables`
2. `new WebSocket(/ws/{table_code})`
3. `onopen` 发送 `join_table`
4. 每 20 秒发送一次 `heartbeat`
5. 收到 `room_snapshot` 后进入房间界面

加入已有牌桌：

1. 直接 `new WebSocket(/ws/{table_code})`
2. `onopen` 发送 `join_table`

断线恢复：

1. 如果本地保存了 `reconnectToken`
2. `socket.onclose` 后前端进入 `reconnecting`
3. 1 秒后重新建连
4. `onopen` 发送 `reconnect`
5. 收到新的 `room_snapshot` 后替换本地 token

当前前端的重连保护：

- 若本地仅有缓存 token、但连续 3 次都无法恢复且始终拿不到 `room_snapshot`，前端会清空历史会话并退回大厅

## 6. 客户端 -> 服务端消息

## 6.1 `join_table`

用途：

- 以真人身份加入牌桌

请求：

```json
{
  "type": "join_table",
  "payload": {
    "nickname": "Claude"
  }
}
```

服务端行为：

- 如果 `nickname` 为空字符串，会自动替换为 `"Player"`
- 为该连接随机分配一个空座位
- 生成 `reconnect_token`
- 写入房间状态和数据库
- 向本连接推送：
  - `room_snapshot`
  - 可能的 `match_result`
  - 可能的 `action_prompt`
- 向其他连接推送：
  - `player_presence`
  - 新的 `room_snapshot`
  - 可能新的 `action_prompt`

失败时可能返回：

- `table_not_found`
- `table_full`
- `seat_already_owned`

模式特殊行为：

- `test` 模式下，真人加入后后端会自动补满 bot 并直接开局

## 6.2 `reconnect`

用途：

- 使用历史 `reconnect_token` 重新拿回座位

请求：

```json
{
  "type": "reconnect",
  "payload": {
    "reconnect_token": "token"
  }
}
```

成功后：

- 后端会校验：
  - token 是否存在
  - 是否属于当前牌桌
  - seat / session 是否一致
- 成功后会旋转 token
- 新 token 会出现在新的 `room_snapshot.payload.reconnect_token`
- 前端必须立刻覆盖本地旧 token

失败时可能返回：

- `invalid_reconnect_token`
- `table_not_found`
- `seat_already_owned`

## 6.3 `ready`

用途：

- 在 `waiting` 阶段切换本座位准备状态

请求：

```json
{
  "type": "ready",
  "payload": {
    "ready": true
  }
}
```

注意：

- 服务端对 `ready` 有默认值，payload 缺省时等价于 `true`
- 当前前端一定显式传布尔值
- 不要等待 success ack，结果看后续 `room_snapshot`

常见拒绝：

- `table_not_found`
- `room_already_started`
- `seat_not_owned`

## 6.4 `adjust_bots`

用途：

- 等待房间中增减 bot

请求：

```json
{
  "type": "adjust_bots",
  "payload": {
    "delta": 1
  }
}
```

约束：

- 只接受 `1` 或 `-1`
- 只允许房间拥有者通过其所占座位发送
- 只在 `waiting` 阶段有效

可能拒绝：

- `table_not_found`
- `seat_not_owned`
- `room_already_started`
- `room_full`
- `bot_not_found`
- `invalid_bot_adjustment`

## 6.5 `start_match`

用途：

- 从等待房间开始整场对局

请求：

```json
{
  "type": "start_match",
  "payload": {}
}
```

服务端要求：

- 房间还没开局
- 4 个座位都已占用
- 所有座位都 ready
- bot 视为 ready

当前前端行为：

- 只在 `waitingControls.canStart === true` 时允许点击
- 仍然以服务器校验结果为准

常见拒绝：

- `table_not_found`
- `seat_not_owned`
- `room_already_started`
- `room_not_ready`

## 6.6 `start_next_round`

用途：

- 在 `settlement` 阶段确认进入下一局

请求：

```json
{
  "type": "start_next_round",
  "payload": {}
}
```

当前不是“一人点击立即开始”，而是继续确认机制：

- 服务端会维护 `continue_action`
- 只有需要确认的人类座位参与确认，bot 不参与
- 如果所有在线需要确认的真人都确认了，但仍有离线真人未确认，服务端会启动 30 秒自动推进倒计时

前端 UI 依赖：

- `room_snapshot.payload.continue_action`

常见拒绝：

- `table_not_found`
- `seat_not_owned`
- `round_not_ready`
- `invalid_action`

## 6.7 `restart_match`

用途：

- 整场结束后，在同一桌重新开一整场新比赛

请求：

```json
{
  "type": "restart_match",
  "payload": {}
}
```

服务端行为：

- 清空旧的 `continue_action`
- 在当前占用座位里随机新庄
- 重新开始一整场
- `skill` 模式下也会清空旧技能负载

常见拒绝：

- `table_not_found`
- `seat_not_owned`
- `match_not_finished`
- `invalid_action`

## 6.8 `leave_table`

用途：

- 主动离桌

请求：

```json
{
  "type": "leave_table",
  "payload": {}
}
```

服务端语义：

- `waiting` 阶段：
  - 直接移除座位
  - 如果房间已无人类，牌桌会被删除
- `playing` / `settlement` / `finished` 阶段：
  - 当前座位会被转成 bot
  - 原 reconnect token 失效
  - 服务端向该连接发送 `leave_table_accepted`
  - 然后关闭该连接

当前前端行为：

- 牌局开始后离桌会弹确认框
- 收到 `leave_table_accepted` 后立即回大厅并清空本地 session

## 6.9 `quick_chat`

用途：

- 发送快捷表情

请求：

```json
{
  "type": "quick_chat",
  "payload": {
    "target_seat": 2,
    "emoji": "😄"
  }
}
```

说明：

- `target_seat` 必须是已占用座位
- 可以给自己发
- 当前前端只把 `emoji` 当普通字符串，不做枚举限制

可能拒绝：

- `table_not_found`
- `seat_not_owned`
- `invalid_action`

## 6.10 `heartbeat`

用途：

- 保活
- 也可测 RTT

当前前端发送：

```json
{
  "type": "heartbeat",
  "payload": {
    "sent_at": "2026-04-17T13:00:00.000Z"
  }
}
```

服务端 transport 实际支持：

```json
{
  "request_id": "optional",
  "sent_at": "optional"
}
```

当前前端只使用 `sent_at`，不会发送 `request_id`。

## 6.11 `action_request`

用途：

- 所有局内操作都通过这一条消息发送

通用格式：

```json
{
  "type": "action_request",
  "payload": {
    "action_type": "discard",
    "tile_ids": ["w3#0"]
  }
}
```

### 6.11.1 标准麻将动作

支持值：

- `discard`
- `flower`
- `kong`
- `hu`
- `chow`
- `pung`
- `pass`

当前前端对这些动作的约束：

- `discard`
  - 需要 1 张已选中的手牌
  - 双击牌也可直接发送
- `flower`
  - 优先从手里找花牌
  - 只有 1 张花时可直接发送
- `kong` / `chow` / `pung`
  - 前端根据当前快照生成候选 tile id 组合
  - 只有选中的 tile id 组合匹配合法候选时才会发
- `pass`
  - claim / rob kong / opening flowers 时会真发给服务端
  - 但“本家自摸阶段可选杠”的本地提示里，点击 `pass` 只是关闭本地提示，不会发给服务端

当前服务端实际期望的 `tile_ids` 形状：

- `discard`
  - 恰好 1 个 `tile_id`
- `flower`
  - 恰好 1 个 `tile_id`
- `chow`
  - 恰好 2 个 `tile_id`
- `pung`
  - 恰好 2 个 `tile_id`
- `kong`
  - 本家暗杠：4 个 `tile_id`
  - 本家补杠：1 个 `tile_id`
  - claim window 明杠：3 个 `tile_id`
- `hu`
  - 空数组
- `pass`
  - 空数组

### 6.11.2 技能抽取动作

支持值：

- `select_skill`
- `decline_skill`

当前前端发送方式：

```json
{
  "type": "action_request",
  "payload": {
    "action_type": "select_skill",
    "tile_ids": ["jin_chan_tuo_qiao"]
  }
}
```

说明：

- `select_skill` 把 skill id 放在 `tile_ids[0]`
- `decline_skill` 的 `tile_ids` 为空数组

### 6.11.3 主动技能动作

服务端支持：

- `action_type = "skill:<skill_id>"`

当前前端不会直接消费 `pending_action.options` 里的 `skill:*`，而是：

1. 从 `equipped_skills[].can_activate_now` 判断是否可发动
2. 在 action dock 中额外插入本地动作 `activate_skill`
3. 最终发送：

```json
{
  "type": "action_request",
  "payload": {
    "action_type": "skill:peek_opponent_tile",
    "tile_ids": ["seat:2"]
  }
}
```

`tile_ids` 在技能动作里有三种语义：

- 真实手牌 id
  - 例：`"w3#0"`
- 目标座位占位符
  - 例：`"seat:2"`
- 副露索引占位符
  - 例：`"meld:1"`

后端解析行为：

- `seat:<n>` 会被抽成 `target = n`
- 其余字符串继续保留在 `tile_ids`
- `meld:<n>` 由技能实现内部再解析

### 6.11.4 成功与失败语义

关键原则：

- `action_request` 没有统一 success ack
- 正确做法是等待服务器推送：
  - `round_event`
  - `room_snapshot`
  - `action_prompt`
  - `match_result`

常见拒绝原因：

- `seat_not_owned`
- `not_your_turn`
- `round_not_ready`
- `invalid_action`
- `select_tile_first`
- `skill_not_equipped`
- `skill_no_charges`
- `skill_requires_target`
- `invalid_skill_target`

## 7. 服务端 -> 客户端消息

## 7.1 `room_snapshot`

这是当前协议里最重要的消息。

它用于承载：

- 房间阶段
- 座位列表
- 本地 seat
- reconnect token
- 整场比分
- 当前局私有视图
- 继续确认状态

当前 transport 结构：

```json
{
  "type": "room_snapshot",
  "payload": {
    "table_code": "AB12CD",
    "phase": "playing",
    "mode": "skill",
    "seats": [],
    "local_seat": 0,
    "reconnect_token": "token",
    "match_state": {},
    "private_state": {},
    "continue_action": null
  }
}
```

### 7.1.1 顶层字段

- `table_code: string`
  - 牌桌号
- `phase: "waiting" | "playing" | "settlement" | "finished"`
  - 房间阶段
- `mode: "normal" | "skill" | "test"`
  - 房间模式
- `seats: PublicSeatView[]`
  - 当前座位列表，按 `seat_index` 排序
- `local_seat: number`
  - 当前连接拥有的座位
- `reconnect_token: string | null`
  - 当前座位最新可用 token
- `match_state: MatchState | null`
  - 整场级别状态
- `private_state: PrivateState | null`
  - 当前连接可见的局内私有状态
- `continue_action: ContinueActionView | null`
  - 结算后继续 / 再来一局确认状态

### 7.1.2 `seats[]`

当前后端投影形状：

```json
{
  "seat_index": 0,
  "nickname": "Alice",
  "connected": true,
  "ready": true,
  "is_bot": false,
  "seat_type": "human"
}
```

字段含义：

- `seat_index`
  - 绝对座位号，固定 `0..3`
- `nickname`
  - 玩家名
- `connected`
  - 当前座位是否有活跃连接
- `ready`
  - 仅等待房间有意义
- `is_bot`
  - 是否机器人
- `seat_type`
  - `"human"` 或 `"bot"`

### 7.1.3 `match_state`

结构：

```json
{
  "prevailing_wind": "east",
  "hand_number": 1,
  "dealer_seat": 0,
  "cumulative_scores": {
    "0": 0,
    "1": 0,
    "2": 0,
    "3": 0
  },
  "match_finished": false,
  "last_completed_round_id": null,
  "statistics": {
    "completed_round_count": 0,
    "seat_stats_by_seat": {
      "0": {
        "score_history": [0],
        "win_count": 0
      }
    }
  }
}
```

字段含义：

- `prevailing_wind`
  - 当前圈风
- `hand_number`
  - 当前局数，`1..4`
- `dealer_seat`
  - 当前庄家绝对座位
- `cumulative_scores`
  - 累计得分
  - 注意：JSON 对象 key 是字符串，不是 number
- `match_finished`
  - 整场是否已结束
- `last_completed_round_id`
  - 最近完成的一局 round id
- `statistics`
  - 服务端累计统计
  - 当前前端若该字段缺失，会用 `cumulative_scores` 自己推一个最简统计

### 7.1.4 `private_state`

只有已经开局的连接才会收到。

等待房间：

- `private_state = null`

局内结构：

```json
{
  "round_id": "east-1-xxx",
  "round_wind": "east",
  "dealer_seat": 0,
  "current_actor": 0,
  "wall_tiles_remaining": 70,
  "last_discard": "w3",
  "pending_action": {},
  "skill_draft": null,
  "score_state": {},
  "equipped_skills": [],
  "visible_effects": [],
  "private_knowledge": [],
  "players": []
}
```

字段含义：

- `round_id`
  - 当前局唯一标识
- `round_wind`
  - 当前局风
- `dealer_seat`
  - 当前局庄家
- `current_actor`
  - 当前轮到谁行动
- `wall_tiles_remaining`
  - 剩余可摸牌数
- `last_discard`
  - 最新打出的牌，使用 `tile_key`
- `pending_action`
  - 当前本连接需要响应的动作上下文
- `skill_draft`
  - 当前本连接可见的技能抽取选择
- `score_state`
  - 当前局内实时分数投影
- `equipped_skills`
  - 本地玩家完整已装备技能列表
- `visible_effects`
  - 本地玩家可见的技能效果投影
- `private_knowledge`
  - 本地玩家可见的私有情报投影
- `players`
  - 4 个座位的局内表现数据

### 7.1.5 `private_state.players[]`

结构：

```json
{
  "seat_index": 0,
  "nickname": "Alice",
  "connected": true,
  "concealed_count": 14,
  "concealed_tiles": [
    {
      "tile_id": "w3#0",
      "tile_key": "w3"
    }
  ],
  "melds": [["w1", "w2", "w3"]],
  "flowers": ["f1"],
  "discards": ["w9"],
  "equipped_skill": null
}
```

字段含义：

- `concealed_count`
  - 手牌数量
- `concealed_tiles`
  - 本地玩家永远可见
  - 其他玩家在 `playing` 阶段为 `null`
  - 到 `settlement` 阶段会公开所有人手牌
- `melds`
  - 副露，元素是 `tile_key[]`
- `flowers`
  - 已补出的花牌，元素是 `tile_key`
- `discards`
  - 河牌，元素是 `tile_key`
- `equipped_skill`
  - 该座位当前对外公开的装备技能视图

### 7.1.6 `pending_action`

当前可见类型：

- `opening_flowers`
- `active_turn`
- `claim_window`
- `rob_kong_window`
- `skill_draft`

#### `opening_flowers`

```json
{
  "type": "opening_flowers",
  "seat_index": 0,
  "deadline_at": "2026-04-17T13:00:30Z",
  "options": ["flower"]
}
```

含义：

- 回合起始的补花阶段
- 若当前本家手里没有花，后端会给出 `["pass"]`
- 当前前端会在“本家无花”时自动发送 `pass`

#### `active_turn`

```json
{
  "type": "active_turn",
  "seat_index": 0,
  "deadline_at": "2026-04-17T13:00:30Z",
  "drawn_tile_id": "w3#0",
  "restricted_discard_tile_ids": ["w5#1"],
  "options": ["discard", "flower", "kong", "hu", "skill:peek_opponent_tile"]
}
```

字段含义：

- `drawn_tile_id`
  - 当前摸到的牌
  - 服务端超时自动出牌时会使用它
- `restricted_discard_tile_ids`
  - 本回合禁止立即打出的具体 tile id
  - 当前前端会用它禁用这些牌
- `options`
  - 当前 transport 里可能包含标准动作，也可能包含 `skill:*`
  - 当前前端只直接消费标准动作；技能由 `equipped_skills[].can_activate_now` 驱动

#### `claim_window`

```json
{
  "type": "claim_window",
  "discarder_seat": 1,
  "deadline_at": "2026-04-17T13:00:15Z",
  "responded_seats": [2],
  "options": ["pung", "hu", "pass"]
}
```

字段含义：

- `discarder_seat`
  - 打出被响应牌的座位
- `responded_seats`
  - 已经回应过的座位
- `options`
  - 当前本连接可执行动作
  - 对没有资格响应的本连接可能是空数组

#### `rob_kong_window`

```json
{
  "type": "rob_kong_window",
  "actor_seat": 2,
  "tile_key": "w5",
  "deadline_at": "2026-04-17T13:00:15Z",
  "responded_seats": [],
  "options": ["hu", "pass"]
}
```

#### `skill_draft`

```json
{
  "type": "skill_draft",
  "seat_index": 1,
  "deadline_at": "2026-04-17T13:00:20Z",
  "options": []
}
```

重要语义：

- `pending_action.type === "skill_draft"` 可以对所有人可见
- 但 `private_state.skill_draft` 只会给“当前仍在等待选择技能的本地座位”
- 也就是说：
  - `pending_action.type === "skill_draft"` 且 `skill_draft === null`
  - 表示“当前轮到某人选技能，但不是你”

这也是当前前端显示“其他玩家正在选择技能”的依据。

### 7.1.7 `skill_draft`

仅当本地座位自己还没选技能时出现。

结构：

```json
{
  "cycle_key": "east-1",
  "cycle_label": "东1~东2局",
  "deadline_at": "2026-04-17T13:00:20Z",
  "title": "东1~东2局 · 技能签启",
  "detail": "每种技能持续两局，主动技能未使用次数不会累加。",
  "options": [
    {
      "skill_id": "jin_chan_tuo_qiao",
      "serial": "S-001",
      "name": "金蝉脱壳",
      "rarity": "rare",
      "rarity_label": "稀有",
      "tone": "azure",
      "type": "active",
      "type_label": "主动技能",
      "interaction_kind": "select_target",
      "summary": "...",
      "detail": "...",
      "interaction_hint": "...",
      "tags": ["control"],
      "remaining_rounds": 2,
      "remaining_activations_this_round": 1
    }
  ]
}
```

含义：

- `cycle_key`
  - 本轮技能周期标识
- `cycle_label`
  - 展示用周期标签，当前实现是“两局一轮”
- `options`
  - 当前可选技能列表

`skill` 模式的服务端实际规则：

- 只在奇数局开始时发起技能抽取
- 技能持续两局
- bot 当前会自动放弃技能，不会主动选

### 7.1.8 `equipped_skills` / `equipped_skill`

`equipped_skills`：

- 本地玩家完整装备列表
- 当前前端主要用它判断：
  - 是否显示主动技能按钮
  - 如何构造技能激活 UI

`equipped_skill`：

- 每个 `players[]` 条目上的公开技能视图
- 用于展示其他玩家当前携带的技能卡面

核心字段：

- `skill_id`
- `name`
- `rarity`
- `rarity_label`
- `tone`
- `type`
- `type_label`
- `interaction_kind`
- `summary`
- `detail`
- `interaction_hint`
- `tags`
- `remaining_rounds`
- `remaining_activations_this_round`
- `can_activate_now`

### 7.1.9 `visible_effects`

结构：

```json
{
  "effect_id": "effect-1",
  "effect_type": "test-effect",
  "owner": 0,
  "target_seats": [0],
  "remaining_turns": 1,
  "stacks": 1,
  "source_skill": "peek_opponent_tile",
  "payload": {
    "flag": true
  }
}
```

说明：

- 这是技能系统投影层提供的“可见效果”
- `payload` 是动态 JSON，当前前端没有为它做统一强类型建模
- rewrite 时应把它当成可扩展协议，不要写死某几个 effect_type

### 7.1.10 `private_knowledge`

结构：

```json
{
  "target_seat": 2,
  "tile_ids": ["w1#0"],
  "tile_keys": ["w1"],
  "source_skill": "peek_opponent_tile",
  "description": "peek"
}
```

用途：

- 技能给当前本地玩家暴露的私有情报
- 当前前端主要用它支持：
  - 预览尾牌
  - 展示“查看到的对手手牌”

### 7.1.11 `score_state`

结构：

```json
{
  "flower_count_by_seat": {
    "0": 1,
    "1": 0,
    "2": 0,
    "3": 0
  },
  "kong_score_detail": [],
  "kong_delta_by_seat": {
    "0": 0,
    "1": 0,
    "2": 0,
    "3": 0
  },
  "current_round_delta_by_seat": {
    "0": 0,
    "1": 0,
    "2": 0,
    "3": 0
  },
  "base_cumulative_scores": {
    "0": 0,
    "1": 0,
    "2": 0,
    "3": 0
  },
  "projected_cumulative_scores": {
    "0": 0,
    "1": 0,
    "2": 0,
    "3": 0
  }
}
```

字段含义：

- `flower_count_by_seat`
  - 当前已补花数量
- `kong_score_detail`
  - 每次杠分结算明细
- `kong_delta_by_seat`
  - 当前局已落地的杠分净变化
- `current_round_delta_by_seat`
  - 当前局实时分差
  - 平时为“杠分 + 技能即时分数修正”
  - 到 `settlement` 阶段会切成整局最终 delta
- `base_cumulative_scores`
  - 不含当前局实时变化的基准累计分
- `projected_cumulative_scores`
  - 基准累计分 + 当前局实时变化

### 7.1.12 `continue_action`

结构：

```json
{
  "action_id": "start_next_round",
  "confirmed_seats": [0],
  "required_seats": [0, 2],
  "online_seats": [0],
  "auto_advance_deadline_at": "2026-04-17T13:05:00Z"
}
```

字段含义：

- `action_id`
  - `start_next_round` 或 `restart_match`
- `confirmed_seats`
  - 已确认的人类座位
- `required_seats`
  - 需要确认的人类座位
- `online_seats`
  - 当前在线的人类座位
- `auto_advance_deadline_at`
  - 若仅剩离线真人未确认时，自动推进截止时间

当前前端用它来决定：

- 按钮文案
- 是否禁用当前本地按钮
- 是否显示 `已确认 x/y`
- 是否显示 `Xs 后自动推进`

## 7.2 `action_prompt`

结构：

```json
{
  "type": "action_prompt",
  "payload": {
    "seat_index": 0,
    "options": ["discard", "kong"],
    "deadline_at": "2026-04-17T13:00:30Z"
  }
}
```

重要语义：

- 它是“轻量提醒消息”，不是完整状态
- 当前前端只把它当成：
  - 启动倒计时
  - 触发本地高亮 / 紧急提示
- `seat_index` 表示“这条提示对谁生效”
  - `active_turn` / `opening_flowers` / `skill_draft` 下，它通常等于当前需要行动的人
  - `claim_window` / `rob_kong_window` 下，后端会直接回填为当前本连接的 `local_seat`
  - 因此前端不能把它误当成“事件来源座位”

注意：

- 后端 transport 上 `deadline_at` 实际是可空的
- 当前前端 TypeScript 类型把它写成了必填 `string`

## 7.3 `match_result`

只在 `settlement` 阶段出现。

结构：

```json
{
  "type": "match_result",
  "payload": {
    "table_code": "AB12CD",
    "round_id": "east-1-xxx",
    "phase": "settlement",
    "provisional": true,
    "win_type": "discard",
    "winner_seat": 1,
    "discarder_seat": 0,
    "display_win_label": null,
    "fan_total": 8,
    "fan_keys": ["test_fan"],
    "fan_breakdown": [
      {
        "fan_key": "test_fan",
        "fan_value": 8
      }
    ],
    "score_delta": {
      "provisional": true,
      "basic_points": 8,
      "base_points": 8,
      "fan_total": 8,
      "minimum_qualifying_fan_total": 8,
      "fan_delta_by_seat": {},
      "kong_delta_by_seat": {},
      "total_delta_by_seat": {}
    },
    "flower_count": 0,
    "draw_type": null,
    "kong_score_detail": []
  }
}
```

字段说明：

- `win_type`
  - `discard`
  - `self_draw`
  - `draw`
- `draw_type`
  - 当前已见值：
    - `exhaustive`
    - `skill_forced`
- `score_delta`
  - 这是结算层最重要的 delta 来源
  - 当前前端主要用 `total_delta_by_seat`

## 7.4 `round_event`

这是“动画友好增量事件”。

统一结构：

```json
{
  "type": "round_event",
  "payload": {
    "event_type": "tile_discarded",
    "event": {}
  }
}
```

当前代码里已实现并实际广播的 `event_type`：

- `tile_discarded`
- `flower_exposed`
- `replacement_draw`
- `claim_made`
- `self_hu_declared`
- `self_kong_declared`
- `claim_auto_passed`
- `rob_kong_auto_passed`
- `settlement_ready`
- `round_drawn`
- `skill_activated`
- `skill_tile_replaced`
- `skill_reclaim_meld`
- `skill_force_draw`
- `skill_score_adjusted`

各事件 shape：

### `tile_discarded`

```json
{
  "type": "tile_discarded",
  "seat": 0,
  "tile_id": "w3#0",
  "tile_key": "w3"
}
```

### `flower_exposed`

```json
{
  "type": "flower_exposed",
  "seat": 0,
  "tile_id": "f1#0"
}
```

### `replacement_draw`

```json
{
  "type": "replacement_draw",
  "seat": 0,
  "tile_id": "b9#replacement",
  "tile_key": "b9"
}
```

### `claim_made`

```json
{
  "type": "claim_made",
  "seat": 2,
  "from": 1,
  "claim_type": "pung",
  "tile_id": "w5#discard",
  "tile_key": "w5",
  "meld": ["w5", "w5", "w5"]
}
```

注：

- `claim_type === "hu"` 时表示荣和，不一定有 `meld`

### `self_hu_declared`

```json
{
  "type": "self_hu_declared",
  "seat": 0,
  "tile_id": "w3#0"
}
```

### `self_kong_declared`

```json
{
  "type": "self_kong_declared",
  "seat": 0,
  "kong_type": "add_kong",
  "tile_key": "w5",
  "tile_ids": ["w5#0"]
}
```

当前已见 `kong_type`：

- `concealed_kong`
- `add_kong`

### `claim_auto_passed`

```json
{
  "type": "claim_auto_passed",
  "discarder_seat": 1,
  "seats": [2, 3]
}
```

### `rob_kong_auto_passed`

```json
{
  "type": "rob_kong_auto_passed",
  "actor_seat": 2,
  "seats": [1]
}
```

### `settlement_ready`

```json
{
  "type": "settlement_ready",
  "round_id": "east-1-xxx",
  "settlement": {}
}
```

### `round_drawn`

```json
{
  "type": "round_drawn",
  "round_id": "east-1-xxx",
  "settlement": {}
}
```

### `skill_activated`

```json
{
  "type": "skill_activated",
  "seat": 0,
  "skill_id": "peek_opponent_tile"
}
```

### `skill_tile_replaced`

```json
{
  "type": "skill_tile_replaced",
  "seat": 0,
  "removed_tile_id": "w1#0",
  "replacement_tile_id": "w9#new",
  "replacement_tile_key": "w9"
}
```

### `skill_reclaim_meld`

```json
{
  "type": "skill_reclaim_meld",
  "seat": 0,
  "meld_index": 1,
  "tile_keys": ["w3", "w3", "w3"]
}
```

### `skill_force_draw`

```json
{
  "type": "skill_force_draw",
  "seat": 0,
  "penalty": 8,
  "next_round_penalty": 4
}
```

### `skill_score_adjusted`

```json
{
  "type": "skill_score_adjusted",
  "seat": 0,
  "delta": -3,
  "reason": "xxx"
}
```

额外说明：

- 后端内部还有 `EffectApplied` / `EffectExpired` / `ViewKnowledgeGranted` / `RuleOverrideApplied` 等事件概念
- 这些不会直接作为 `round_event` 推给前端
- 它们的结果主要通过 `room_snapshot.private_state.visible_effects` / `private_knowledge` 体现

## 7.5 `player_presence`

结构：

```json
{
  "type": "player_presence",
  "payload": {
    "table_code": "AB12CD",
    "seat_index": 1,
    "connected": false
  }
}
```

当前前端用途：

- 只做 toast 提示
- 真正的 UI 状态还是以后续 `room_snapshot.seats[].connected` 为准

## 7.6 `quick_chat`

结构：

```json
{
  "type": "quick_chat",
  "payload": {
    "message_id": "abcd1234",
    "actor_seat": 0,
    "target_seat": 2,
    "emoji": "😄",
    "sent_at": "2026-04-17T13:00:00Z"
  }
}
```

当前前端用途：

- 保存最后一条快捷消息
- 转换成飘字 / 气泡展示

## 7.7 `leave_table_accepted`

结构：

```json
{
  "type": "leave_table_accepted",
  "payload": {
    "table_code": "AB12CD",
    "seat_index": 0
  }
}
```

当前前端行为：

- 收到后立即回大厅
- 清空本地缓存 session
- 不再等待其他消息

## 7.8 `action_rejected`

结构：

```json
{
  "type": "action_rejected",
  "payload": {
    "reason": "invalid_action"
  }
}
```

当前代码中实际可能出现的 `reason` 至少包括：

- `table_not_found`
- `table_full`
- `invalid_reconnect_token`
- `seat_already_owned`
- `seat_not_owned`
- `room_already_started`
- `room_not_ready`
- `round_not_ready`
- `match_not_finished`
- `invalid_action`
- `select_tile_first`
- `unsupported_message`
- `room_full`
- `bot_not_found`
- `invalid_bot_adjustment`
- `skill_not_equipped`
- `skill_no_charges`
- `skill_requires_target`
- `invalid_skill_target`

当前前端对 `action_rejected` 的处理分两档：

- 致命错误，直接回大厅：
  - `table_not_found`
  - `invalid_reconnect_token`
  - 首次加入时的 `table_full`
- 非致命错误：
  - 写 toast
  - 清理乐观更新
  - 保留当前房间，继续以最近一次 `room_snapshot` 为准

## 7.9 `heartbeat`

结构：

```json
{
  "type": "heartbeat",
  "payload": {
    "request_id": "optional",
    "sent_at": "optional"
  }
}
```

当前前端行为：

- 收到后不做任何状态更新

## 8. 当前前端的派生逻辑

## 8.1 状态优先级

当前前端的实际优先级：

1. `room_snapshot`
2. `match_result`
3. `round_event`
4. `action_prompt`

换句话说：

- `room_snapshot` 决定页面最终状态
- `round_event` 决定临时动画
- `action_prompt` 决定倒计时和提示语

## 8.2 乐观更新

当前只做两类乐观更新：

- 乐观出牌
- 乐观补花

行为：

- 发送 `discard` 后，本地立刻把这张牌临时从手牌上拿掉
- 收到新 `room_snapshot` 后，若这张牌仍在本地手牌里，则说明服务端未采纳，乐观状态被撤销
- `flower` 同理

## 8.3 自动过补花

当前前端在以下条件下会自动发 `pass`：

- `pending_action.type === "opening_flowers"`
- `pending_action.seat_index === local_seat`
- 本地手牌里没有任何花牌

这是前端主动行为，不是服务端自动帮你过。

## 8.4 吃碰杠候选组合生成

当前前端不是只发一个动作名，而是会先本地计算可发送的 tile id 组合：

- `chow`
  - 根据 `last_discard` 的花色和点数，在本地手牌里找左右组合
- `pung`
  - 找两张相同 tile key 的手牌
- `kong`
  - 当前轮自己杠：
    - 4 张暗杠
    - 或已有刻子 + 1 张补杠
  - claim window 明杠：
    - 找 3 张相同 tile key 的手牌

注意：

- 这是前端为了交互方便做的候选生成
- 合法性最终仍以服务端校验为准

## 8.5 本家“可杠”提示的特殊处理

当前前端有一个纯本地交互：

- 当 `active_turn` 下本家存在多种 `kong` 候选时
- 会弹出一个“是否杠牌”的本地提示

此时点击 `pass` 的语义不是发给服务端，而是：

- 仅关闭这个本地杠牌提示
- 保持当前 `active_turn` 不变

这是重写时最容易遗漏的当前行为。

## 8.6 ready hand 听牌提示

当前前端会用本地算法计算听牌信息，不依赖后端单独接口。

输入来源：

- 本地手牌 `concealed_tiles`
- 本家副露 `melds`
- 已知牌：
  - 所有人河牌
  - 所有人花牌
  - 所有人副露
  - 私有情报 `private_knowledge`

因此：

- ready hand insight 不是后端消息字段
- 是当前前端基于现有快照自行推导的展示层能力

## 8.7 技能按钮的真实来源

当前 transport 里，`pending_action.options` 在主动阶段可能直接包含：

- `skill:<skill_id>`

但当前前端没有直接使用这组 `skill:*`：

- `PromptActionType` 类型没有覆盖它
- `action dock` 也不直接从 `options` 渲染它

当前实现实际依赖：

- `private_state.equipped_skills[]`
- 其中的 `can_activate_now`
- 以及 `interaction_kind`

重写时建议：

- 要么像当前前端一样把技能操作当单独系统处理
- 要么把 `skill:*` 纳入统一动作建模，但要与现有行为做兼容

## 8.8 继续确认 UI

当前前端把 `continue_action` 直接映射成结算页按钮状态：

- 未确认：
  - 文案 `下一局` / `再来一局`
- 已确认：
  - 文案 `已确认 x/y`
- 有 `auto_advance_deadline_at`
  - 文案 `Xs后自动推进`

## 9. 标识与数据约定

## 9.1 `table_code`

- 前后端都使用大写字母 + 数字
- 最大长度 12
- 前端输入时会自动转大写

## 9.2 绝对座位号

- 始终使用 `0..3`
- 后端所有 seat 相关字段都是绝对座位
- 当前前端会再把绝对座位映射成相对方位：
  - `bottom`
  - `right`
  - `top`
  - `left`

## 9.3 `tile_id` 与 `tile_key`

`tile_key`：

- 表示牌面种类
- 用于显示、河牌、副露、花牌、番种文案
- 示例：
  - `w3`
  - `east`
  - `f1`

`tile_id`：

- 表示某一张具体牌实例
- 仅本地手牌操作和局内动作发送时需要
- 示例：
  - `w3#p0-4`

结论：

- 渲染看 `tile_key`
- 发请求看 `tile_id`

## 9.4 字符串 key 的分数字典

所有按座位索引做 key 的字典在 JSON 里都会变成字符串 key：

```json
{
  "0": 12,
  "1": -8
}
```

前端处理时必须用：

- `scores["0"]`

而不是：

- `scores[0]`

## 9.5 前端兼容的 tile key 别名

当前前端辅助逻辑会做一层规范化：

- `m -> w`
- `p -> b`
- `c -> t`
- `d1..d7 -> east/south/west/north/red/green/white`

这意味着：

- 当前前端的局部算法能兼容少量历史别名
- 但当前后端主协议仍应视为输出标准化的 `w/t/b/east/...`

## 10. 当前实现中的已知偏差与注意点

这些是重写时应该优先修正或至少显式建模的点。

### 10.1 `action_prompt.deadline_at` 在 transport 层可空

后端实际类型：

- `Option<String>`

当前前端类型：

- `string`

虽然多数情况下服务端都会给 deadline，但重写时不应把它写成绝对非空。

### 10.2 `pending_action.options` 可能包含 `skill:*`

后端会把主动技能选项直接塞进 `active_turn.options`，例如：

- `skill:peek_opponent_tile`

当前前端类型与动作渲染没有完整覆盖它，而是绕过它走 `equipped_skills`。

### 10.3 `action_rejected` 的前端 copy 中有过时 reason

当前前端内置文案里包含：

- `restricted_same_turn_discard`

但当前后端源码没有直接返回这个 reason。

实际限制是通过：

- `restricted_discard_tile_ids`
- 或最终退化为 `invalid_action`

### 10.4 `create_table_response.seats` 的 transport 形状比前端类型宽

后端直接序列化 `SeatState`，理论上字段比前端 `SeatSnapshot` 更宽。

只是当前创建牌桌响应几乎总是：

```json
{
  "seats": []
}
```

所以当前前端一直没暴露这个问题。

### 10.5 `nickname` 在后端投影层是可空

后端 `PublicSeatView.nickname` / `PlayerSeatView.nickname` 都是 `Option<String>`。

当前前端类型把它写成必填 `string`，是基于当前运行时通常总会有昵称的经验假设。

### 10.6 继续确认与断线的组合逻辑很重要

`continue_action` 的自动推进不是“有人点击后固定倒计时”，而是：

1. 先看所有 `required_seats`
2. 若在线真人全部确认，但仍有离线真人未确认
3. 才生成 `auto_advance_deadline_at`

重写时不要把它误实现成简单的“结算后 30 秒自动下一局”。

## 11. 对重写前端的直接建议

建议在新前端里把模型拆成三层：

1. `transport model`
   - 原样对应后端 JSON
   - 保持 nullable / string-keyed map / skill:* 等真实形状
2. `session state`
   - 连接态、缓存态、重连态、乐观态
3. `view model`
   - 相对座位、按钮状态、提示文案、动画指令

最重要的边界：

- `room_snapshot` 永远是权威状态
- `round_event` 永远只是增量提示
- `action_prompt` 永远只是提醒，不是状态主源

## 12. 追溯源码

本文档主要整理自以下源码：

- `frontend/src/App.tsx`
- `frontend/src/lib/api.ts`
- `frontend/src/lib/socket.ts`
- `frontend/src/lib/sessionReducer.ts`
- `frontend/src/lib/matchViewModel.ts`
- `frontend/src/lib/skillSystem.ts`
- `frontend/src/lib/kongSelection.ts`
- `frontend/src/lib/readyHand.ts`
- `frontend/src/lib/storage.ts`
- `frontend/src/types/match.ts`
- `backend/src/app/server.rs`
- `backend/src/app/ws.rs`
- `backend/src/app/protocol.rs`
- `backend/src/app/mod.rs`
- `backend/src/app/scheduler.rs`
- `backend/src/projection/room_snapshot.rs`
- `backend/src/projection/prompt.rs`
- `backend/src/projection/match_result.rs`
- `backend/src/core/engine/command.rs`
- `backend/src/core/engine/validation.rs`
- `backend/src/core/state/*.rs`
- `backend/src/rules/standard/actions.rs`
- `backend/src/rules/standard/flow.rs`
- `backend/src/rules/standard/win.rs`
- `backend/src/rules/standard/settlement.rs`
- `backend/src/rules/skills/mod.rs`
- `backend/src/rules/skills/draft.rs`
- `backend/src/rules/skills/strategems.rs`

如果后续协议改动，优先同步这里这几个 backend projection / ws handler / frontend session 层文件。
