# 国标麻将前后端通信协议规范 (Guobiao Mahjong Protocol Specification)

本规范详细整理了国标麻将游戏中，React 前端与 Rust 后端之间现行的所有数据传输格式。系统全面使用 **JSON** 格式作为序列化载体，通信机制包括**传统的 HTTP REST 接口**以及基于 **WebSocket** 的实时长连接。

---

## 目录
1. [HTTP REST 接口](#一-http-rest-接口)
2. [WebSocket 实时大厅与社交接口 (`/ws/me`)](#二-websocket-实时大厅与社交接口-wsme)
3. [WebSocket 局内实时游戏接口 (`/ws/:tableCode`)](#三-websocket-局内实时游戏接口-wstablecode)
4. [核心数据类型与枚举定义](#四-核心数据类型与枚举定义)
5. [Godot (GDScript) 接收与解析指南](#五-godot-gdscript-接收与解析指南)

---

## 一、 HTTP REST 接口

主要负责静态的非实时事务（如注册登录、排行榜拉取、好友邀请应答等）。请求均通过标准的 HTTP JSON 载荷进行。

### 1. 基础与鉴权 (Auth & Account)
*   **健康检查**
    *   **接口**: `GET /api/health`
    *   **返回**:
        ```json
        { "status": "ok" }
        ```
*   **注册**
    *   **接口**: `POST /api/auth/register`
    *   **请求**:
        ```json
        {
          "invite_code": "INVITE_CODE_STR",
          "display_name": "玩家昵称",
          "password": "Password123"
        }
        ```
    *   **返回 (`AuthResponse`)**:
        ```json
        {
          "session_token": "jwt_token_string...",
          "user": { ...PublicUser... }
        }
        ```
*   **密码登录**
    *   **接口**: `POST /api/auth/login`
    *   **请求**:
        ```json
        {
          "identifier": "username_or_email",
          "password": "Password123"
        }
        ```
    *   **返回 (`AuthResponse`)**: 同上。
*   **登出**
    *   **接口**: `POST /api/auth/logout`
    *   **Header**: `Authorization: Bearer <session_token>`
    *   **返回**: `204 No Content`
*   **获取个人信息**
    *   **接口**: `GET /api/me`
    *   **Header**: `Authorization: Bearer <session_token>`
    *   **返回**: `PublicUser`
*   **更新个人信息**
    *   **接口**: `PATCH /api/me`
    *   **Header**: `Authorization: Bearer <session_token>`
    *   **请求**:
        ```json
        {
          "display_name": "新昵称",
          "bio": "个性签名",
          "avatar": "avatar_url_or_null"
        }
        ```
    *   **返回**: `PublicUser`

### 2. 牌桌管理与社交行为 (Tables & Invites)
*   **创建牌桌**
    *   **接口**: `POST /api/tables`
    *   **Header**: `Authorization: Bearer <session_token>`
    *   **请求**: `{}` 或指定创建代码 `{ "table_code": "AB12CD" }`
    *   **返回 (`CreateTableResponse`)**:
        ```json
        {
          "table_code": "AB12CD",
          "phase": "waiting",
          "mode": "normal",
          "owner_user_id": 10001,
          "multiplier": 1,
          "created_at": "2026-05-20T09:16:00Z",
          "seats": [
            {
              "seat_index": 0,
              "user_id": 10001,
              "nickname": "雀圣小白",
              "points": 1000,
              "title": "Lv.12",
              "connected": true,
              "is_bot": false,
              "seat_type": "human"
            }
          ]
        }
        ```
*   **获取排行榜**
    *   **接口**: `GET /api/leaderboard`
    *   **返回**: `PublicUser[]` 数组
*   **获取当前活跃牌桌 (断线重连判定)**
    *   **接口**: `GET /api/me/active-table`
    *   **Header**: `Authorization: Bearer <session_token>`
    *   **返回**:
        ```json
        {
          "table_code": "AB12CD",
          "seat_index": 0,
          "role": "player"
        }
        ```
        *(若当前不在任何对局中，返回 `null`)*
*   **获取当前收到的邀请**
    *   **接口**: `GET /api/me/invites`
    *   **Header**: `Authorization: Bearer <session_token>`
    *   **返回**: `TableInvite[]` 数组 (结构见第四节)
*   **创建房间邀请**
    *   **接口**: `POST /api/tables/:tableCode/invites`
    *   **Header**: `Authorization: Bearer <session_token>`
    *   **请求**:
        ```json
        {
          "invitee_user_id": 10002
        }
        ```
    *   **返回**: `TableInvite`
*   **应答房间邀请 (接受/拒绝)**
    *   **接受**: `POST /api/invites/:inviteId/accept`
        *   **返回**: `{ "invite_id": 12, "table_code": "AB12CD", "seat_index": 1, "status": "accepted" }`
    *   **拒绝**: `POST /api/invites/:inviteId/reject`
        *   **返回**: `TableInvite` (status 更新为 `"rejected"`)

---

## 二、 WebSocket 实时大厅与社交接口 (`/ws/me`)

当玩家登录系统后，连接至 `/ws/me?token=<session_token>`。此通道主要用于接收大厅广播、系统推送与离线通知，目前为**单向（后端 -> 客户端）推送**。

所有消息包含外层包裹：
```json
{
  "type": "消息类型",
  "payload": { ... }
}
```

### 1. 用户在线状态更新 (`user_presence_updated`)
*   **作用**: 更新好友大厅在线状态。
*   **Payload 结构**:
    ```json
    {
      "online_user_ids": [10001, 10002, 10005]
    }
    ```

### 2. 积分/天梯变动 (`user_points_updated`)
*   **作用**: 当另一场对局结算，或玩家在大厅被扣除/获得积分时下发。
*   **Payload 结构**:
    ```json
    {
      "user_id": 10001,
      "delta": 24,
      "old_points": 1000,
      "points": 1024,
      "old_title": "Lv.12",
      "title": "Lv.13",
      "display_name": "雀圣小白",
      "reason": "round_settlement",
      "source_table_code": "AB12CD",
      "source_round_id": "round_xxx"
    }
    ```

### 3. 活跃房间变化 (`user_active_table_updated`)
*   **作用**: 广播玩家当前正处于的牌桌及状态。
*   **Payload 结构**:
    ```json
    {
      "user_id": 10001,
      "active_table_code": "AB12CD", // 若为空说明已离桌
      "active_table_phase": "playing" // "waiting" | "playing" | "settlement" | "finished"
    }
    ```

### 4. 收到房间邀请 (`table_invite_created`)
*   **作用**: 实时通知玩家有其他人邀请他们加入麻将桌。
*   **Payload 结构**: `TableInvite` (见第四节)

---

## 三、 WebSocket 局内实时游戏接口 (`/ws/:tableCode`)

局内对局所有数据交互的通道，当连接建立后，需要发送 `join_table` 消息来通过后端身份校验。

### 1. 客户端发送的行牌操作消息 (`ClientMessage`)

客户端发送的数据均包含外层包裹：
```json
{
  "type": "操作指令",
  "payload": { ... }
}
```

*   **加入牌桌身份验证 (`join_table`)**
    ```json
    {
      "type": "join_table",
      "payload": { "session_token": "jwt_token..." }
    }
    ```
*   **离开牌桌 (`leave_table`)**
    ```json
    {
      "type": "leave_table",
      "payload": {}
    }
    ```
*   **大厅等待期 - 增减人机 Bot (`adjust_bots`)**
    ```json
    {
      "type": "adjust_bots",
      "payload": { "delta": 1 } // 可选 1 (增加人机) 或 -1 (移除人机)
    }
    ```
*   **大厅等待期 - 设定规则参数**
    *   起胡番数设定:
        ```json
        {
          "type": "set_minimum_hu_fan",
          "payload": { "minimum_hu_fan": 8 } // 可选 0, 2, 4, 6, 8
        }
        ```
    *   连庄设定:
        ```json
        {
          "type": "set_dealer_repeat",
          "payload": { "enabled": true }
        }
        ```
    *   庄家翻倍设定:
        ```json
        {
          "type": "set_dealer_double",
          "payload": { "enabled": true }
        }
        ```
*   **对局中 - 切换托管 (`set_bot_takeover`)**
    ```json
    {
      "type": "set_bot_takeover",
      "payload": { "enabled": true }
    }
    ```
*   **开始游戏对局 (`start_match`)**
    ```json
    {
      "type": "start_match",
      "payload": {}
    }
    ```
*   **进入下一小局 (`start_next_round`)**
    ```json
    {
      "type": "start_next_round",
      "payload": {}
    }
    ```
*   **行牌与动作决策请求 (`action_request`)**
    ```json
    {
      "type": "action_request",
      "payload": {
        "action_type": "discard", // 动作枚举: "discard", "ready_hand", "flower", "kong", "hu", "chow", "pung", "pass"
        "tile_ids": ["tile_001"] // 需要操作的牌的唯一ID数组
      }
    }
    ```
    > *   *`discard`（出牌）：传入被出掉的单张牌 `tile_id`。*
    > *   *`chow`（吃牌）：传入从手牌挑出用于配对的**两张**牌 ID，而非吃进来的那张。*
    > *   *`pung`（碰牌）：传入手牌中的**两张**同型牌 ID。*
    > *   *`ready_hand`（听牌立直）：打出的那张单张牌 ID。*
*   **快捷社交聊天/互动表情 (`quick_chat`)**
    ```json
    {
      "type": "quick_chat",
      "payload": {
        "target_seat": 1, // 目标座位索引 0~3
        "emoji": "applause",
        "chat_kind": "emoji" // "emoji" 或 "point_gesture" (指向手势)
      }
    }
    ```
*   **心跳包发送 (`heartbeat`)**
    ```json
    {
      "type": "heartbeat",
      "payload": { "sent_at": "2026-05-20T09:17:30.123Z" }
    }
    ```
    后端回执会保留 `sent_at`，并附加 `server_now` 供客户端校准本地倒计时基准。

---

##### 2. 后端推送的游戏广播消息 (`ServerMessage`)

同样包装在外层 `{ type: string, payload: ... }` 中。

*   **`room_snapshot` (完整局势快照)**
    *   **触发时机**: 玩家刚进入房间或断线重连。
    *   **Payload 结构**:
        ```json
        {
          "table_code": "AB12CD",
          "server_now": "2026-05-20T09:17:30.123456Z",
          "phase": "playing", // 房间状态
          "mode": "normal",
          "seats": [ ...SeatSnapshot... ],
          "local_seat": 0, // 当前连接客户端的绝对座位索引 0~3
          "match_state": {
            "prevailing_wind": "east", // 圈风: "east" | "south" | "west" | "north"
            "hand_number": 1, // 当前第几局
            "dealer_seat": 0, // 庄家座位号
            "cumulative_scores": { "0": 1000, "1": 1000 },
            "match_finished": false
          },
          "private_state": {
            "round_id": "uuid_of_round",
            "dealer_seat": 0,
            "current_actor": 0, // 当前摸牌/出牌的绝对座位索引
            "wall_tiles_remaining": 70, // 牌墙剩余张数
            "last_discard": "tile_045", // 最新打出的牌 ID
            "players": [ ...PrivatePlayerState... ], // 四个座位上玩家的详细行牌状态
            "pending_action": { ...PendingAction... } // 当前可进行的吃碰杠等操作提示
          }
        }
        ```
*   **`action_prompt` (唤醒操作菜单)**
    *   **触发时机**: 后端等待本客户端玩家进行摸牌出牌、或者响应他人的出牌。
    *   **Payload 结构**:
        ```json
        {
          "server_now": "2026-05-20T09:17:30.123456Z",
          "seat_index": 0,
          "options": ["pung", "kong", "pass"], // 可选操作数组
          "deadline_at": "2026-05-20T09:17:45.000Z", // 定时器截止时间
          "remaining_extra_time": 10
        }
        ```
*   **`round_event` (单步游戏事件)**
    *   **触发时机**: 游戏局中任何一步动作（出牌、吃碰杠、摸牌、补花）发生时。
    *   **Payload 结构**:
        ```json
        {
          "event_type": "discard", // 事件类型: "draw", "discard", "pung", "kong", "chow", "flower", "ready_hand"
          "event": {
            "seat_index": 0, // 谁操作的
            "tile_id": "tile_045",
            "tile_key": "1w", // 牌的简写，如 "1w" (一万), "9t" (九条)
            "is_drawn": false
          }
        }
        ```
*   **`match_result` (单小局得分结算)**
    *   **触发时机**: 某人胡牌或牌墙摸完荒庄。
    *   **Payload 结构**:
        ```json
        {
          "table_code": "AB12CD",
          "round_id": "round_uuid",
          "win_type": "self_draw", // "self_draw" (自摸) | "discard" (点炮) | "draw" (荒庄/流局)
          "winner_seat": 0,
          "discarder_seat": 1,
          "fan_total": 8, // 总番数
          "fan_keys": ["mixed_double_chow", "flower_tiles"],
          "fan_breakdown": [
            { "fan_key": "mixed_double_chow", "fan_value": 6 },
            { "fan_key": "flower_tiles", "fan_value": 2 }
          ],
          "score_delta": {
            "total_delta_by_seat": {
              "0": 24, // 胜者加分
              "1": -8, // 输家扣分
              "2": -8,
              "3": -8
            }
          }
        }
        ```
*   **`action_rejected` (非法操作被拒)**
    *   `payload`: `{ "reason": "操作不合法或当前不属于你的回合" }`
*   **`player_presence` (局内玩家连线状态)**
    *   `payload`: `{ "table_code": "AB12CD", "seat_index": 1, "connected": false }`
*   **`quick_chat` (快捷聊天/互动表情推送)**
    *   `payload`: `{ "message_id": "uuid", "actor_seat": 0, "target_seat": 1, "emoji": "sweat", "sent_at": "..." }`
*   **`heartbeat` (心跳回执)**
    *   `payload`: `{ "sent_at": "请求中带入的发送时间", "server_now": "服务器发送回执时的 ISO-8601 UTC 时间" }`

---

## 四、 核心数据类型与枚举定义

### 1. 房间状态与阶段定义
*   **`RoomPhase`**: `"waiting"` (大厅等待) | `"playing"` (激战中) | `"settlement"` (局后结算) | `"finished"` (整场比赛结束)
*   **`SeatType`**: `"human"` (人类玩家) | `"bot"` (普通AI) | `"special_bot"` (特殊设定AI)

### 2. 简写牌名对应表 (`tile_key` / `code`)
所有的牌面标识用字符串形式，采用 **数字 + 拼音首字母** 的形式：
*   **万 (Wan)**: `"1w"` ~ `"9w"`
*   **条 (Tiao)**: `"1t"` ~ `"9t"`
*   **筒 (Tong)**: `"1b"` ~ `"9b"` (B 代表 Bing/筒)
*   **风牌 (Winds)**: `"dn"` (东风), `"nn"` (南风), `"xn"` (西风), `"bn"` (北风) (N 代表 Nan/风)
*   **字牌 (Dragons)**: `"zhong"` (红中), `"fa"` (发财), `"bai"` (白板)
*   **花牌 (Flowers)**: `"me"` (梅), `"lan"` (兰), `"zhu"` (竹), `"ju"` (菊), `"chun"` (春), `"xia"` (夏), `"qiu"` (秋), `"dong"` (冬)

### 3. 公共数据结构
*   **`TableInvite` (房间邀请数据模型)**:
    ```json
    {
      "id": 12,
      "table_code": "AB12CD",
      "inviter_user_id": 10001,
      "invitee_user_id": 10002,
      "status": "pending", // "pending" | "accepted" | "rejected" | "expired"
      "created_at": "ISO-8601",
      "expires_at": "ISO-8601",
      "accepted_at": null
    }
    ```
*   **`PrivatePlayerState` (局内单个座位的玩家状态)**:
    ```json
    {
      "seat_index": 0,
      "nickname": "玩家昵称",
      "connected": true,
      "is_ready_hand": false, // 是否已听牌
      "concealed_count": 13, // 暗牌手牌张数
      "concealed_tiles": [ // 只有本地座位（当前玩家）才会有这个明细，其他玩家此项为 null 或空
        { "tile_id": "tile_001", "tile_key": "1w" }
      ],
      "melds": [ ["tile_004", "tile_005", "tile_006"] ], // 吃碰杠的明牌组
      "flowers": ["chun", "lan"], // 补过的花牌列表
      "discards": ["1w", "9t", "zhong"] // 已经打出到弃牌池中的牌面值
    }
    ```

---

## 五、 Godot (GDScript) 接收与解析指南

在 Godot 中重构时，您可以使用下面的范例解析从 WebSocket 接收到的 JSON 消息并进行派发：

```gdscript
# ws_client.gd
extends Node

signal room_snapshot_received(payload: Dictionary)
signal action_prompt_received(payload: Dictionary)
signal round_event_received(payload: Dictionary)
signal match_result_received(payload: Dictionary)

var ws = WebSocketPeer.new()

func _ready():
    # 连接到游戏桌长连接
    ws.connect_to_url("ws://localhost:8080/ws/AB12CD")

func _process(delta):
    ws.poll()
    var state = ws.get_ready_state()
    if state == WebSocketPeer.STATE_OPEN:
        while ws.get_available_packet_count() > 0:
            var packet = ws.get_packet()
            var text = packet.get_string_from_utf8()
            _handle_json_message(text)

func _handle_json_message(json_str: String):
    var json = JSON.new()
    var error = json.parse(json_str)
    if error == OK:
        var msg = json.data
        if msg is Dictionary and msg.has("type") and msg.has("payload"):
            var type = msg["type"]
            var payload = msg["payload"]
            
            match type:
                "room_snapshot":
                    emit_signal("room_snapshot_received", payload)
                "action_prompt":
                    emit_signal("action_prompt_received", payload)
                "round_event":
                    emit_signal("round_event_received", payload)
                "match_result":
                    emit_signal("match_result_received", payload)
                _:
                    print("未处理的 WebSocket 消息类型: ", type)
    else:
        print("JSON 解析失败: ", error)
```
