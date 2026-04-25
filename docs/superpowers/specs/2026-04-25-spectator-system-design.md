# 观战系统设计

## 目标

为朋友局增加可选观战系统。观战者可以实时查看四家全部手牌，并通过手牌栏左侧的向下按钮切换观察视角。切换视角时，底部手牌、座位相对位置、玩家状态、行动提示位置都跟随切换。

观战系统必须是编译期可选能力。关闭配置时，构建产物不包含观战前端入口、观战消息构造、观战后端消息类型、观战投影和观战 WebSocket 广播链路。

## 非目标

- 不做观战延时。
- 不做手牌隐私控制。
- 不做观战权限、密码或白名单。
- 第一版不做观战者重连凭证。
- 第一版不允许观战者发送牌局动作。
- 第一版不要求观战者参与快速聊天。

## 编译期开关

后端使用 Cargo feature：

```toml
[features]
default = []
spectator = []
```

开启观战：

```powershell
cargo build --release --features spectator
```

关闭观战：

```powershell
cargo build --release
```

前端使用 Vite 编译期常量：

```ts
__SPECTATOR_ENABLED__
```

该常量只在构建时确定。关闭时，观战入口、观战连接消息、观战视角状态和观战专用组件必须通过静态分支与独立模块组织被打包器裁剪。

## Docker 配置

Docker 部署通过 Compose 环境变量控制是否编译观战功能：

```yaml
services:
  backend:
    build:
      args:
        MAHJONG_ENABLE_SPECTATOR: ${MAHJONG_ENABLE_SPECTATOR:-false}

  frontend:
    build:
      args:
        MAHJONG_ENABLE_SPECTATOR: ${MAHJONG_ENABLE_SPECTATOR:-false}
```

`Dockerfile` 接收同名 build arg：

```dockerfile
ARG MAHJONG_ENABLE_SPECTATOR=false
```

前端构建阶段根据该参数设置 Vite 常量。推荐由 `vite.config.ts` 读取环境变量，并通过 `define` 注入：

```ts
define: {
  __SPECTATOR_ENABLED__: JSON.stringify(process.env.MAHJONG_ENABLE_SPECTATOR === 'true'),
}
```

后端构建阶段根据该参数选择 Cargo 命令：

```dockerfile
RUN if [ "$MAHJONG_ENABLE_SPECTATOR" = "true" ]; then \
      cargo build --release --features spectator; \
    else \
      cargo build --release; \
    fi
```

`docker-compose.yml` 与 `docker-compose.prebuilt.yml` 都应显式记录配置。源码构建模式通过 build args 影响编译产物；预构建镜像模式通过注释或环境变量约定说明镜像必须由对应开关预先构建，运行时不能改变已编译能力。

## 后端设计

现有 WebSocket 连接默认通过 `join_table` 或 `reconnect` 绑定玩家座位。观战需要把连接身份从“座位”中拆开，避免占座、ready 状态、断线托管和重连逻辑被污染。

开启 `spectator` feature 后新增客户端消息：

```json
{
  "type": "watch_table",
  "payload": {
    "nickname": "观众"
  }
}
```

后端连接角色：

```rust
enum ConnectionRole {
    Unbound,
    Player { seat_index: usize },
    Spectator { spectator_id: u64, nickname: Option<String> },
}
```

房间运行时在 feature 开启时新增观战连接表：

```rust
spectator_connections: HashMap<u64, ConnectionHandle>
```

观战连接只接收房间快照与结算消息。对 `ready`、`adjust_bots`、`start_match`、`start_next_round`、`restart_match`、`leave_table`、`action_request` 等玩家动作，后端返回 `seat_not_owned` 或 `unsupported_message`，不得修改房间状态。

关闭 `spectator` feature 时：

- `watch_table` 消息类型不参与反序列化。
- 房间运行时没有观战连接表。
- 广播函数没有观战分支。
- 观战投影函数不参与编译。

## 观战投影

新增 observer snapshot。该投影复用当前 `room_snapshot` 的结构，但语义如下：

- `local_seat` 为 `null`。
- `reconnect_token` 为 `null`。
- `private_state.players[*].concealed_tiles` 对所有玩家都可见。
- `pending_action.options` 始终为空。
- 不生成 `action_prompt`。
- `match_result` 继续广播给观战者。

观察视角不写入后端快照。后端只负责提供全量可见状态，前端本地决定当前聚焦座位。

## 前端设计

开启 `__SPECTATOR_ENABLED__` 后，大厅增加“观战牌桌”入口。用户输入牌桌编号后建立 WebSocket，并发送 `watch_table`。观战成功后进入现有牌桌界面。

前端新增本地状态：

```ts
spectatorFocusSeat: number
```

默认值优先选择 0 号座位；如果 0 号座位未入局，则选择当前房间中最小的已占座位。点击手牌栏左侧向下按钮后，在已占座位之间循环切换。

观战视角使用 `spectatorFocusSeat` 作为相对视角基准：

- 焦点玩家的手牌显示在底部手牌栏。
- 其他三家的相对位置随焦点玩家变化。
- 庄家、风位、分数、连线状态、出牌区和行动指示保持与玩家视角一致。
- 底部手牌栏左侧显示向下按钮，用于切换焦点玩家。
- 隐藏所有牌局操作按钮和动作提交入口。
- 保留退出观战、换主题、牌桌缩放等纯客户端控制。

关闭 `__SPECTATOR_ENABLED__` 时：

- 大厅不显示观战入口。
- 不创建观战 WebSocket。
- 不导入观战 feature 目录。
- 不维护 `spectatorFocusSeat`。
- 不出现观战按钮或观战相关文案。

## 数据流

1. 部署时设置 `MAHJONG_ENABLE_SPECTATOR=true` 并构建镜像。
2. 前端显示“观战牌桌”入口。
3. 观战者输入牌桌编号并建立 WebSocket。
4. 前端发送 `watch_table`。
5. 后端确认房间存在后登记 spectator connection。
6. 后端立即发送 observer snapshot 和当前结算信息。
7. 后续房间状态变化时，后端同时给玩家和观战者广播各自投影。
8. 前端通过 `spectatorFocusSeat` 本地切换观察视角，不向后端发送切换消息。

## 错误处理

- 牌桌不存在：返回 `table_not_found`，前端回到大厅并提示牌桌不存在。
- 观战功能未编译：前端没有入口；若手工发送 `watch_table`，后端按不支持消息处理。
- 观战连接断开：不生成重连 token，不影响玩家断线托管和房间清理。
- 房间结束或删除：观战连接随房间关闭而关闭，前端回到大厅。

## 测试与验证

后端：

- 默认构建不包含 spectator feature，`cargo test` 通过。
- spectator 构建下，`cargo test --features spectator` 通过。
- 测试 `watch_table` 不占座。
- 测试 observer snapshot 展示四家 `concealed_tiles`。
- 测试观战连接不能提交玩家动作。

前端：

- 默认构建不显示观战入口。
- spectator 构建显示观战入口。
- 观战模式隐藏动作按钮。
- 切换焦点座位时，底部手牌和座位相对位置同步变化。
- 观战模式不会保存或依赖 reconnect token。

Docker：

- `MAHJONG_ENABLE_SPECTATOR=false docker compose build` 后，前端无观战入口，后端无观战消息链路。
- `MAHJONG_ENABLE_SPECTATOR=true docker compose build` 后，前端可观战，后端接受 `watch_table`。
- 预构建镜像部署文档说明运行时环境变量不能改变已编译能力。
