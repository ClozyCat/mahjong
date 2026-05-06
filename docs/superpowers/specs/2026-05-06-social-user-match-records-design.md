# 社交用户与牌局记录系统设计

## 背景

当前项目以“创建房间 + 房间号加入牌桌”的方式组织牌局。用户身份主要来自加入牌桌时输入的昵称，牌桌状态和重连凭证保存在 SQLite 中，实时交互通过 `/ws/{table_code}` 完成。项目已经具备牌桌运行、结算、番种明细、局内快速聊天和观战基础，但缺少长期用户身份、跨牌局记录、公开社交信息、邀请入局、观战审批和全局积分体系。

本设计面向朋友之间自部署使用。所有用户资料、牌局记录、番种统计、在线状态、观战者信息默认公开可见；身份校验只用于证明操作发起者是谁，不用于隐藏资料。

## 目标

- 增加邀请码注册、密码登录、可修改昵称的用户系统。
- 将进入牌局方式改为界面邀请驱动，普通用户不再通过手动输入房间号加入。
- 支持玩家在多个设备登录同一账号，并同时操作自己账号所在牌桌的同一座位。
- 增加牌局记录、每局结算积分、每日登录积分和用户番种统计。
- 增加按服务器全体玩家积分动态计算的称号，用户名显示为 `昵称（称号）`。
- 增加牌局内右侧默认收起侧边栏，展示当前牌局内玩家、在线玩家、玩家信息、观战者列表和观战申请。
- 将观战功能改为默认启用，删除编译期观战开关。
- 观战必须由当前牌局房主审批，且当前牌局内玩家不能观战本牌局。
- 牌局开始前由房主选择倍数 `x1`、`x2` 或 `x3`；开局后锁定，结算积分按锁定倍数增减。

## 非目标

- 不做私密资料、好友权限、消息加密或复杂访问控制。
- 不做第三方 OAuth、邮箱验证、找回密码流程。
- 不做管理员网页后台，第一版管理员能力通过后端命令完成。
- 不做跨服务器联邦、云同步或公网账号体系。
- 不改变麻将核心规则、番种计算和局内行动规则。
- 不支持玩家绕过邀请直接输入房间号加入牌桌。

## 现有基础

- 后端使用 Rust、Axum、Tokio、rusqlite 和 Serde。
- 前端使用 React、TypeScript、Vite 和 Vitest。
- HTTP 当前提供 `/api/health` 和 `/api/tables` 创建牌桌。
- WebSocket 当前通过 `/ws/{table_code}` 承载加入、重连、ready、动作、局内 quick chat、观战等消息。
- SQLite 当前核心表为 `tables` 和 `reconnect_tokens`。
- 当前观战使用 Cargo feature `spectator` 和前端常量 `__SPECTATOR_ENABLED__` 控制。
- `RoundSettlement` 已包含 `fan_keys`、`fan_breakdown`、`winning_details` 和 `score_delta.total_delta_by_seat`，可用于番种统计和全局积分变更。

## 推荐架构

采用“账号身份 + 公开社交数据 + 牌桌运行时 + 牌局归档”的分层结构。

后端保留当前内存房间运行时和 SQLite 持久化模型，新增用户、邀请、观战申请和积分事件表。实时牌桌 WebSocket 继续承载牌局动作；新增全局用户 WebSocket `/ws/me`，用于向已登录用户推送邀请、观战申请和在线状态变化。

前端从单一牌桌入口升级为登录后的大厅。大厅负责创建牌桌、选择倍数、邀请玩家、查看公开用户和处理收到的邀请。牌桌页面保持主牌桌沉浸式布局，新增右侧默认收起侧边栏承载社交和资料信息。

## 身份与注册

### 邀请码注册

管理员通过后端命令生成邀请码：

```powershell
cargo run --manifest-path backend/Cargo.toml -- admin create-invite --count 5
```

邀请码只可使用一次。注册时用户提交邀请码、昵称和密码。注册成功后创建用户、标记邀请码已使用，并创建登录 session。

### 登录与 session

用户使用用户名或用户 ID 加密码登录。服务端返回 session token，前端保存到本地存储。session 支持多个设备同时存在。所有需要身份的 HTTP 和 WebSocket 请求必须携带 session token。

密码使用 `argon2` 哈希保存，不保存明文。session token 使用随机字节生成并只保存哈希值。

### 昵称修改

用户可随时修改当前昵称。牌局记录保存当时昵称快照，公开用户资料和新的牌桌展示使用最新昵称。

## 积分与称号

### 每日登录积分

以北京时间自然日为准。用户每天第一次成功登录时获得 `+50` 积分。服务端在登录事务中读取 `users.last_login_local_date`，若不是当天，则写入 `user_point_events` 并更新 `users.points` 与 `last_login_local_date`。同一天重复登录不重复加分。

### 牌局结算积分

每次有效结算时，对每名真人玩家写入积分事件：

```text
global_delta = round_score_delta * table_multiplier
```

- `round_score_delta` 来自 `RoundSettlement.score_delta.total_delta_by_seat[seat]`。
- `table_multiplier` 只能是 `1`、`2` 或 `3`。
- 积分允许为负数。
- bot 不写入用户积分事件。
- 同一 `round_id` 对同一用户只允许写入一次，避免重复结算。

### 倍数锁定

倍数是牌桌级配置。只有房主可在牌桌等待阶段修改倍数。一旦进入选庄、发牌、正式对局或结算流程，倍数锁定。开局后任何修改请求返回 `table_multiplier_locked`。

### 动态称号

称号不落库为固定字段，而是在读取用户、在线列表、玩家列表和排行榜时按全体用户当前积分动态计算。第一版使用绝对阈值，避免小样本排名导致频繁跳变：

| 积分范围 | 称号 |
| --- | --- |
| `points < 0` | 乞丐 |
| `0 <= points < 500` | 平民 |
| `500 <= points < 2000` | 小康 |
| `2000 <= points < 5000` | 富豪 |
| `points >= 5000` | 财神 |

如果多个玩家积分相同，称号相同。后续可再升级为分位数称号。

## 牌桌与邀请

### 房主

创建牌桌的用户为房主。房主负责：

- 开局前选择或修改倍数。
- 邀请玩家进入牌局。
- 审批观战申请。

房主如果离开或断线，第一版不转移房主；其账号任一设备仍可继续审批和操作。若房主座位被托管为 bot，仍保留房主身份。

### 邀请进入

普通用户不再手动输入房号加入。牌桌创建后，房主从在线用户列表发起邀请。被邀请者通过弹窗接受后进入牌桌。

邀请规则：

- 目标用户空闲：创建邀请并推送弹窗。
- 目标用户只处在“该用户 + bot”的牌局中：创建邀请并推送弹窗。
- 目标用户处在存在其他真人玩家的牌局中：拒绝邀请，发起者收到 `该玩家正在牌局中，请稍后重试`。
- 目标用户已经在目标牌桌中：拒绝邀请，发起者收到 `该玩家已在本牌局中`。
- 邀请过期前可接受一次，接受后分配座位并连接牌桌。

### 内部房间号

`table_code` 继续作为内部房间标识、WebSocket 路径和记录关联键。UI 不提供普通手动输入房号加入入口，但可以保留复制房号用于排障和管理员确认。

## 多设备同账号操作

当前运行时 `connections` 以座位到单连接建模。新设计改为一个座位可拥有多个连接。

推荐结构：

```rust
pub(crate) struct SeatConnectionGroup {
    pub(crate) user_id: i64,
    pub(crate) connections: Vec<ConnectionHandle>,
}
```

运行时保存 `seat_connections: HashMap<usize, SeatConnectionGroup>`。同一用户的多个设备连接到自己的座位后都会接收 snapshot、action prompt、round event 和结算消息。任一设备提交合法动作后，服务端处理一次，并广播最新状态给该座位所有设备和其他玩家。

断线判断按用户座位维度处理：只有该座位所有连接都断开，才将座位标记为断线并启动断线宽限或托管逻辑。

## 观战系统

观战默认启用。删除后端 `spectator` Cargo feature、前端 `__SPECTATOR_ENABLED__` 常量、Docker 中的观战构建参数和相关静态裁剪逻辑。

观战流程：

1. 用户在大厅或侧边栏点击申请观战某牌桌。
2. 服务端校验申请者不在该牌桌的玩家列表中。
3. 服务端创建 `spectator_requests`，通过 `/ws/me` 推送给房主所有在线设备。
4. 房主同意后，申请者进入观战 WebSocket 流程。
5. 房主拒绝或申请超时，申请者收到提示。

观战者不能提交玩家动作。观战者列表通过侧边栏公开展示。第一版观战者不允许发送局内 quick chat，避免干扰牌局。

## 右侧侧边栏

牌局进行中，右侧固定一个默认收起的侧边栏入口。打开后显示标签页：

- `本局玩家`：座位、昵称、称号、在线设备数、积分、本局分差。
- `在线玩家`：全服在线用户、称号、是否可邀请。
- `玩家信息`：选中玩家的公开资料、番种统计摘要、最近牌局。
- `观战者`：当前牌局观战者列表。
- `观战申请`：仅房主可见，用于批准或拒绝申请。

侧边栏不影响牌桌主区域尺寸稳定。桌面端从右侧覆盖或推入，移动端使用底部抽屉式表现。默认收起，避免干扰出牌。

## 数据表

### users

```sql
CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    avatar TEXT,
    bio TEXT NOT NULL DEFAULT '',
    points INTEGER NOT NULL DEFAULT 0,
    last_login_local_date TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

### invite_codes

```sql
CREATE TABLE invite_codes (
    code TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    expires_at TEXT,
    used_at TEXT,
    used_by_user_id INTEGER,
    FOREIGN KEY(used_by_user_id) REFERENCES users(id)
);
```

### auth_sessions

```sql
CREATE TABLE auth_sessions (
    token_hash TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    revoked_at TEXT,
    FOREIGN KEY(user_id) REFERENCES users(id)
);
```

### table_participants

```sql
CREATE TABLE table_participants (
    table_code TEXT NOT NULL,
    user_id INTEGER NOT NULL,
    seat_index INTEGER NOT NULL,
    role TEXT NOT NULL,
    nickname_snapshot TEXT NOT NULL,
    joined_at TEXT NOT NULL,
    left_at TEXT,
    PRIMARY KEY(table_code, user_id),
    FOREIGN KEY(user_id) REFERENCES users(id)
);
```

### table_invites

```sql
CREATE TABLE table_invites (
    id INTEGER PRIMARY KEY,
    table_code TEXT NOT NULL,
    inviter_user_id INTEGER NOT NULL,
    invitee_user_id INTEGER NOT NULL,
    status TEXT NOT NULL,
    message TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    accepted_at TEXT,
    FOREIGN KEY(inviter_user_id) REFERENCES users(id),
    FOREIGN KEY(invitee_user_id) REFERENCES users(id)
);
```

### spectator_requests

```sql
CREATE TABLE spectator_requests (
    id INTEGER PRIMARY KEY,
    table_code TEXT NOT NULL,
    requester_user_id INTEGER NOT NULL,
    owner_user_id INTEGER NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    decided_at TEXT,
    FOREIGN KEY(requester_user_id) REFERENCES users(id),
    FOREIGN KEY(owner_user_id) REFERENCES users(id)
);
```

### game_records

```sql
CREATE TABLE game_records (
    id INTEGER PRIMARY KEY,
    table_code TEXT NOT NULL,
    owner_user_id INTEGER NOT NULL,
    multiplier INTEGER NOT NULL,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    final_room_json TEXT,
    FOREIGN KEY(owner_user_id) REFERENCES users(id)
);
```

### round_records

```sql
CREATE TABLE round_records (
    id INTEGER PRIMARY KEY,
    game_record_id INTEGER NOT NULL,
    round_id TEXT NOT NULL,
    ended_at TEXT NOT NULL,
    settlement_json TEXT NOT NULL,
    UNIQUE(game_record_id, round_id),
    FOREIGN KEY(game_record_id) REFERENCES game_records(id)
);
```

### round_player_results

```sql
CREATE TABLE round_player_results (
    round_record_id INTEGER NOT NULL,
    user_id INTEGER NOT NULL,
    seat_index INTEGER NOT NULL,
    score_delta INTEGER NOT NULL,
    point_delta INTEGER NOT NULL,
    cumulative_score INTEGER NOT NULL,
    is_winner INTEGER NOT NULL,
    win_type TEXT,
    nickname_snapshot TEXT NOT NULL,
    PRIMARY KEY(round_record_id, user_id),
    FOREIGN KEY(round_record_id) REFERENCES round_records(id),
    FOREIGN KEY(user_id) REFERENCES users(id)
);
```

### user_point_events

```sql
CREATE TABLE user_point_events (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL,
    delta INTEGER NOT NULL,
    reason TEXT NOT NULL,
    local_date TEXT,
    source_table_code TEXT,
    source_round_id TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(user_id, reason, local_date),
    FOREIGN KEY(user_id) REFERENCES users(id)
);
```

牌局结算事件不使用 `local_date` 唯一约束，而是在写入前以 `(user_id, reason, source_table_code, source_round_id)` 查询去重。

### user_fan_stats

```sql
CREATE TABLE user_fan_stats (
    user_id INTEGER NOT NULL,
    fan_key TEXT NOT NULL,
    fan_label TEXT NOT NULL,
    count INTEGER NOT NULL DEFAULT 0,
    last_seen_at TEXT NOT NULL,
    PRIMARY KEY(user_id, fan_key),
    FOREIGN KEY(user_id) REFERENCES users(id)
);
```

## HTTP API

### Auth

- `POST /api/auth/register`
- `POST /api/auth/login`
- `POST /api/auth/logout`
- `GET /api/me`
- `PATCH /api/me`

### Users

- `GET /api/users`
- `GET /api/users/{user_id}`
- `GET /api/users/{user_id}/fans`
- `GET /api/users/{user_id}/games`
- `GET /api/leaderboard`

### Tables

- `POST /api/tables`
- `PATCH /api/tables/{table_code}/multiplier`
- `POST /api/tables/{table_code}/invites`
- `GET /api/me/invites`
- `POST /api/invites/{invite_id}/accept`

### Spectator

- `POST /api/tables/{table_code}/spectator-requests`
- `GET /api/me/spectator-requests`
- `POST /api/spectator-requests/{request_id}/approve`
- `POST /api/spectator-requests/{request_id}/reject`

### Records

- `GET /api/games`
- `GET /api/games/{game_id}`

## WebSocket

### `/ws/me`

用户登录后建立全局 WebSocket。消息包括：

- `user_presence_updated`
- `table_invite_created`
- `table_invite_accepted`
- `table_invite_rejected`
- `spectator_request_created`
- `spectator_request_decided`
- `user_points_updated`

### `/ws/{table_code}`

牌桌 WebSocket 继续处理牌局内消息，但连接首包必须携带 session token 和进入方式：

- `join_table_by_invite`
- `reconnect_table`
- `watch_table`

`watch_table` 只允许已批准观战请求的用户进入。

## 错误处理

- `auth_required`：缺少或无效 session。
- `invite_code_invalid`：邀请码不存在、已使用或过期。
- `table_invite_required`：没有有效邀请，不能加入牌桌。
- `target_player_busy`：目标玩家正在有其他真人的牌局中。
- `target_already_in_table`：目标玩家已在本牌局中。
- `table_multiplier_locked`：开局后倍数锁定。
- `only_owner_can_change_multiplier`：只有房主可改倍数。
- `spectator_requires_owner_approval`：观战需要房主同意。
- `player_cannot_watch_own_table`：牌局内玩家不能观战本牌局。
- `seat_not_owned`：当前账号不拥有该座位。

## 迁移与兼容

第一版允许旧房间数据继续加载。旧 `RoomState` 中没有 `user_id` 的 seat 被视为旧匿名 seat，不能写入全局用户积分和番种统计。新用户系统启用后，新创建牌桌必须绑定房主用户。

旧的手动房号加入 UI 被移除，但后端可在短期内保留 `join_table` 解析，返回 `table_invite_required`，避免旧前端或缓存页面产生不可识别错误。

## 测试策略

后端测试：

- 邀请码注册一次后不能重复使用。
- 北京时间同一天重复登录只加一次每日积分。
- 房主可在 waiting 阶段修改倍数，开局后修改返回 `table_multiplier_locked`。
- 邀请空闲玩家成功。
- 邀请只和 bot 同桌的玩家成功。
- 邀请正在真人牌局中的玩家失败。
- 同一账号多个 WebSocket 连接同座位时都收到 snapshot，任一连接操作后全部同步。
- 所有连接断开前座位不进入断线状态。
- 牌局结算按锁定倍数写积分事件。
- 同一 round 重复归档不重复加减积分。
- 观战请求必须经房主批准。
- 本局玩家申请观战本局失败。

前端测试：

- 未登录时显示登录/注册界面。
- 创建牌桌时可选择 x1/x2/x3。
- 开局后倍数控件禁用或隐藏。
- 收到邀请时弹窗，接受后进入牌桌。
- 目标玩家忙碌时发起者看到忙碌提示。
- 牌桌右侧侧边栏默认收起，打开后显示各标签页。
- 房主可在侧边栏审批观战请求。
- 多端同账号不会覆盖本地连接状态。

## 验收标准

- 用户能通过管理员生成的邀请码注册并登录。
- 用户每日首次登录按北京时间加 50 积分。
- 房主能创建牌桌并选择倍数，开局后倍数不能修改。
- 玩家只能通过邀请进入牌局。
- 多设备登录同一账号能同时操作同一座位。
- 每局结算后玩家积分按分差和倍数变化，番种统计更新。
- 用户资料、排行榜、牌局记录均公开可见。
- 观战默认启用，且必须经过房主同意。
- 牌桌右侧侧边栏默认收起，打开后可查看玩家、在线用户、资料和观战者信息。
