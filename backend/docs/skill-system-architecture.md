# 国标麻将后端技能系统架构改造方案

## 1. 背景

当前后端源码主要集中在 4 个大文件中：

- `src/main.rs`
- `src/mahjong.rs`
- `src/scoring.rs`
- `src/bot.rs`

这套结构在“纯标准规则、功能较少”的阶段可以工作，但如果后续要加入技能系统，现有结构会很快遇到三个核心问题：

1. **真实状态与玩家可见状态没有边界**
   现在房间状态主要以 `serde_json::Value` 形式存在，房间快照和私有状态也是直接从同一份 JSON 中拼装出来。  
   一旦技能影响“能看到什么”“感知到什么”，真实信息和表现信息会混在一起。

2. **规则判定缺少稳定的扩展点**
   摸牌、弃牌、碰杠吃胡、结算、超时、机器人决策等流程主要集中在 `mahjong.rs` 中，后续如果技能能改“摸牌数”“和牌条件”“算番方式”，会迫使很多技能直接侵入大函数。

3. **bot 与结算依赖的是房间整体状态，不是权限化视图**
   标准规则下这不一定出问题，但技能一旦引入“额外情报”或“错误感知”，bot 如果仍然从真实房间状态取值，就会天然作弊。

这份文档的目标不是一次性重写整个项目，而是给出一套**适合当前项目、能渐进迁移**的目录拆分方案与核心 trait 设计。

## 2. 改造目标

建议把后端拆成四层：

1. **Transport / Session 层**
   负责 WebSocket、房间生命周期、持久化、重连、超时调度。

2. **Game Core 层**
   负责强类型状态、命令处理、事件归约、回合推进。

3. **Rules / Skills 层**
   负责标准国标规则、技能效果、可扩展的合法性判定与结算修正。

4. **Projection / AI 层**
   负责给玩家生成权限化视图、给 bot 生成可见上下文。

核心原则：

- **真实状态只存一份**
- **玩家看到的内容通过 projection 生成**
- **规则变化通过 hook / policy 注入**
- **技能不直接到处改状态，而是通过有限扩展点生效**

## 3. 建议目录结构

下面是按你当前项目规模建议的一版目录结构。重点不是目录名本身，而是职责边界。

```text
backend/
  src/
    main.rs

    app/
      mod.rs
      server.rs
      ws.rs
      room_runtime.rs
      scheduler.rs
      persistence.rs

    core/
      mod.rs
      ids.rs
      tile.rs
      action.rs
      event.rs
      error.rs
      state/
        mod.rs
        room.rs
        match_state.rs
        round.rs
        player.rs
        wall.rs
        pending.rs
        effect.rs
      engine/
        mod.rs
        command.rs
        reducer.rs
        flow.rs
        validation.rs

    rules/
      mod.rs
      standard/
        mod.rs
        actions.rs
        hu.rs
        draw.rs
        meld.rs
        settlement.rs
      scoring/
        mod.rs
        model.rs
        evaluator.rs
        fan_table.rs
      skills/
        mod.rs
        registry.rs
        hooks.rs
        effects.rs
        instances.rs
        builtin/
          mod.rs
          clairvoyance.rs
          extra_draw.rs
          altered_win.rs
          score_modifier.rs

    projection/
      mod.rs
      room_snapshot.rs
      private_view.rs
      prompt.rs
      bot_view.rs

    bot/
      mod.rs
      context.rs
      policy.rs
      search.rs

    infra/
      mod.rs
      serde.rs
      time.rs
      random.rs
```

## 4. 现有文件如何映射到新结构

### 4.1 `main.rs`

保留为程序入口，职责缩小为：

- 读取配置
- 创建数据库/应用上下文
- 注册 HTTP/WebSocket 路由
- 启动 server

从当前 `main.rs` 中拆出去的内容：

- 房间 runtime 与调度逻辑 -> `app/room_runtime.rs`, `app/scheduler.rs`
- SQLite 持久化 -> `app/persistence.rs`
- WebSocket 消息分发 -> `app/ws.rs`

### 4.2 `mahjong.rs`

这是未来最需要拆的文件。建议拆为三块：

- **核心状态与回合推进** -> `core/state/*`, `core/engine/*`
- **标准国标规则实现** -> `rules/standard/*`
- **对外视图拼装** -> `projection/*`

拆分后，`mahjong.rs` 可以暂时保留为过渡 facade，内部转调新模块，避免一次性改动太大。

### 4.3 `scoring.rs`

目前已经相对独立，建议继续保留“算番内核”特征，但要把它从“唯一规则定义处”改成“标准番型求值器”：

- 纯标准 MCR 番型识别 -> `rules/scoring/evaluator.rs`
- 输入模型 -> `rules/scoring/model.rs`
- 番表注册 -> `rules/scoring/fan_table.rs`

外层再通过 `ScorePolicy` / `ScoringModifier` 允许技能修正结果。

### 4.4 `bot.rs`

bot 也应该按层次拆开：

- `bot/context.rs`：只定义 bot 可见上下文
- `bot/policy.rs`：高层决策
- `bot/search.rs`：搜索与估值

关键点是：bot 输入不能继续直接依赖完整房间状态，而必须依赖 `projection::bot_view` 产出的受限视图。

## 5. 核心数据模型

### 5.1 房间真实状态

建议先定义强类型状态，序列化时仍可使用 `serde`，这样不会破坏现有数据库保存模式。

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomState {
    pub table_code: String,
    pub phase: RoomPhase,
    pub mode: RoomMode,
    pub seats: Vec<SeatState>,
    pub match_state: Option<MatchState>,
    pub round_state: Option<RoundState>,
    pub pending_timeout: Option<PendingTimeout>,
    pub continue_action: Option<ContinueActionState>,
}
```

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundState {
    pub round_id: RoundId,
    pub dealer_seat: Seat,
    pub round_wind: Wind,
    pub current_actor: Seat,
    pub phase: RoundPhase,
    pub wall: WallState,
    pub players: [PlayerRoundState; 4],
    pub last_discard: Option<DiscardRecord>,
    pub pending_action: Option<PendingAction>,
    pub score_trackers: RoundScoreTrackers,
    pub last_action_context: LastActionContext,
    pub rule_state: RuleRuntimeState,
    pub effect_state: EffectState,
}
```

### 5.2 技能运行态

技能系统至少要有三种对象：

1. **技能定义**
   描述技能是什么、有什么 hook。

2. **技能实例**
   某个玩家当前携带了哪个技能、等级多少、参数如何。

3. **效果状态**
   技能产生的持续效果、层数、持续时间、目标、是否已消耗。

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillLoadout {
    pub equipped: Vec<SkillInstance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInstance {
    pub skill_id: SkillId,
    pub owner: Seat,
    pub level: u8,
    pub cooldown: u8,
    pub charges: u8,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EffectState {
    pub ongoing: Vec<EffectInstance>,
    pub hidden_knowledge: Vec<KnowledgeEffect>,
    pub rule_overrides: Vec<RuleOverride>,
}
```

这里故意把 `config` 留成 JSON，是为了给技能参数留弹性；但**房间主状态和回合主状态不建议继续用大 JSON 结构**。

## 6. 命令、事件、归约

如果要支持技能，单纯在 `try_handle_action` 里继续加分支很快会失控。  
建议把核心交互改成：

```text
客户端请求 -> Command -> 校验 -> 触发 hooks -> 生成 Events -> Reducer 应用事件 -> 新状态
```

### 6.1 Command

```rust
pub enum GameCommand {
    StartMatch { dealer: Seat, seed: u64 },
    PlayerAction {
        actor: Seat,
        action: PlayerAction,
    },
    ResolveTimeout {
        kind: TimeoutKind,
        nonce: u64,
    },
    ContinueAction {
        actor: Seat,
        kind: ContinueActionKind,
    },
}
```

```rust
pub enum PlayerAction {
    Flower { tile_ids: Vec<TileId> },
    Discard { tile_id: TileId },
    Chow { tile_ids: Vec<TileId> },
    Pung { tile_ids: Vec<TileId> },
    Kong { tile_ids: Vec<TileId> },
    Hu,
    Pass,
    ActivateSkill {
        skill_id: SkillId,
        target: Option<Seat>,
        tile_ids: Vec<TileId>,
    },
}
```

### 6.2 Event

所有真实状态变化都尽量落在事件上，这样技能更容易插入，也更好做回放和测试。

```rust
pub enum GameEvent {
    MatchStarted { dealer: Seat },
    RoundStarted { round_id: RoundId },
    TileDrawn { seat: Seat, tile: Tile, source: DrawSource },
    TileDiscarded { seat: Seat, tile: Tile },
    MeldClaimed { seat: Seat, meld: Meld, from: Seat },
    HuDeclared { winner: Seat, source: WinSource },
    SettlementPrepared { settlement: SettlementResult },
    SkillActivated { seat: Seat, skill_id: SkillId },
    EffectApplied { effect: EffectInstance },
    EffectExpired { effect_id: EffectId },
    ViewKnowledgeGranted { seat: Seat, knowledge: KnowledgeEffect },
    RuleOverrideApplied { override_rule: RuleOverride },
}
```

### 6.3 Reducer

Reducer 只负责“根据事件修改状态”，不要在 reducer 里塞复杂规则推导。

```rust
pub trait EventReducer {
    fn apply(&self, state: &mut RoomState, event: &GameEvent) -> Result<(), EngineError>;
}
```

## 7. 规则与技能的扩展点设计

技能系统最大的设计问题不是“技能怎么写”，而是“技能在哪些固定点生效”。  
建议定义一组稳定 hook，让标准规则和技能都围绕这些 hook 工作。

### 7.1 Hook 总体设计

```rust
pub trait RuleHook {
    fn before_action(
        &self,
        ctx: &mut RuleContext,
        action: &mut PlayerAction,
    ) -> Result<(), EngineError> {
        Ok(())
    }

    fn after_action(
        &self,
        ctx: &mut RuleContext,
        action: &PlayerAction,
        events: &mut Vec<GameEvent>,
    ) -> Result<(), EngineError> {
        Ok(())
    }

    fn before_draw(
        &self,
        ctx: &mut RuleContext,
        request: &mut DrawRequest,
    ) -> Result<(), EngineError> {
        Ok(())
    }

    fn before_hu_check(
        &self,
        ctx: &RuleContext,
        request: &mut HuCheckRequest,
    ) -> Result<(), EngineError> {
        Ok(())
    }

    fn before_scoring(
        &self,
        ctx: &RuleContext,
        request: &mut ScoreRequest,
    ) -> Result<(), EngineError> {
        Ok(())
    }

    fn after_scoring(
        &self,
        ctx: &RuleContext,
        result: &mut ScoreResult,
    ) -> Result<(), EngineError> {
        Ok(())
    }

    fn build_view(
        &self,
        ctx: &RuleContext,
        seat: Seat,
        view: &mut PlayerView,
    ) -> Result<(), EngineError> {
        Ok(())
    }
}
```

这里的核心思路是：

- 标准规则本身可以实现一部分 hook
- 技能也实现 hook
- 引擎按顺序调用 hook 链

### 7.2 为什么不用“技能直接改房间 JSON”

因为你提到的技能能力已经覆盖了几类完全不同的修改：

- 修改信息可见性
- 修改摸牌行为
- 修改和牌合法性
- 修改算番与结算

这些逻辑如果都直接下沉到房间状态任意字段读写，很快就无法知道：

- 这个技能到底影响了哪一步
- 多个技能同时生效时先后顺序是什么
- 某个视图是“真实看到的”还是“技能幻觉”

所以技能必须通过**有限 hook + 有类型的 request/result** 来影响流程。

## 8. 建议的核心 trait

### 8.1 房间引擎

```rust
pub trait GameEngine {
    fn execute(
        &mut self,
        state: &mut RoomState,
        cmd: GameCommand,
    ) -> Result<EngineOutput, EngineError>;
}
```

```rust
pub struct EngineOutput {
    pub events: Vec<GameEvent>,
    pub emitted_messages: Vec<DomainMessage>,
}
```

### 8.2 标准规则集

```rust
pub trait RuleSet {
    fn validate_action(
        &self,
        ctx: &RuleContext,
        action: &PlayerAction,
    ) -> Result<(), EngineError>;

    fn generate_events(
        &self,
        ctx: &mut RuleContext,
        action: PlayerAction,
    ) -> Result<Vec<GameEvent>, EngineError>;

    fn next_turn(
        &self,
        ctx: &mut RuleContext,
    ) -> Result<Vec<GameEvent>, EngineError>;
}
```

`RuleSet` 管的是标准游戏流程。  
技能不直接替代 `RuleSet`，而是在 `RuleSet` 执行前后通过 hook 修改 request 和 result。

### 8.3 技能定义

```rust
pub trait SkillDefinition: Send + Sync {
    fn id(&self) -> SkillId;
    fn name(&self) -> &'static str;
    fn hooks(&self) -> &'static [SkillHookKind];

    fn on_event(
        &self,
        ctx: &mut SkillContext,
        event: &GameEvent,
        out: &mut Vec<GameEvent>,
    ) -> Result<(), EngineError> {
        Ok(())
    }

    fn apply_hook(
        &self,
        hook: SkillHookKind,
        ctx: &mut SkillContext,
    ) -> Result<(), EngineError>;
}
```

这个 trait 可以继续细分，但当前项目更适合先做成统一入口，避免接口爆炸。

### 8.4 技能注册表

```rust
pub trait SkillRegistry {
    fn get(&self, id: SkillId) -> Option<&dyn SkillDefinition>;
    fn all_for_seat<'a>(
        &'a self,
        loadout: &'a SkillLoadout,
        seat: Seat,
    ) -> Vec<&'a dyn SkillDefinition>;
}
```

### 8.5 视图投影

```rust
pub trait ViewProjector {
    fn room_snapshot(&self, state: &RoomState, seat: Seat) -> PlayerRoomSnapshot;
    fn action_prompt(&self, state: &RoomState, seat: Seat) -> Option<ActionPrompt>;
    fn bot_context(&self, state: &RoomState, seat: Seat) -> BotContextView;
}
```

这是技能系统里非常关键的接口。  
你提到的“影响对其他玩家牌的信息、对后续将摸到的牌的感知”，都应该从这里生效，而不是去污染真实状态。

### 8.6 算番与结算策略

```rust
pub trait HandEvaluator {
    fn can_hu(&self, request: &HuCheckRequest) -> Result<HuCheckResult, EngineError>;
}

pub trait ScoreEvaluator {
    fn evaluate(&self, request: &ScoreRequest) -> Result<ScoreResult, EngineError>;
}

pub trait ScoreModifier {
    fn modify(
        &self,
        ctx: &RuleContext,
        result: &mut ScoreResult,
    ) -> Result<(), EngineError>;
}
```

这样可以保留你当前 `scoring.rs` 的大部分能力，同时允许技能：

- 给某些牌型加成
- 修改最低成和番数
- 增加或减少某些番型
- 改最终 seat delta

## 9. 几类技能如何落到这套设计里

### 9.1 影响其他玩家牌的信息

例子：A 能看到 B 手牌中的一张牌。

建议实现：

- 真实状态不变
- 在 `EffectState.hidden_knowledge` 中登记一条知识效果
- `build_view` / `room_snapshot` 投影时，根据 `seat` 决定是否额外显示指定信息
- `bot_context` 也只拿到自己的知识投影

### 9.2 影响后续将摸到的牌的感知

这里要先分清楚两种技能：

1. **真实改摸牌**
   例如“下次多摸一张”“从牌尾摸”
   这应作用于 `before_draw`

2. **只改感知，不改真实结果**
   例如“你预感下一张可能是 5 万”
   这应作用于 `build_view` 或 `bot_context`

这两种能力一定要分开建模，不能都叫“预知摸牌”。

### 9.3 影响和牌条件

例子：某技能允许 6 番和牌，或者允许特定特殊牌型额外成和。

建议实现：

- `before_hu_check` 修改 `HuCheckRequest`
- 或增加 `RuleOverride::HuThreshold { seat, min_fan }`
- `HandEvaluator` / `ScoreEvaluator` 消费这些 override

### 9.4 影响摸牌张数

例子：某技能在指定时机“摸两张再弃一张”。

建议实现：

- `before_draw` 修改 `DrawRequest { count, source, reveal_to, ... }`
- 产生多个 `TileDrawn` 事件
- 如果后续流程不同，再在 `RuleSet::next_turn` 中解释该状态

### 9.5 影响算番方式

例子：某技能让“门前清”额外 +1 番，或让“某个番型不再互斥”。

建议实现两级能力：

1. **保守型扩展**
   标准 evaluator 先算出正常 `ScoreResult`
   然后 `ScoreModifier` 做增减修正

2. **深度型扩展**
   技能修改 `ScoreRequest`，允许改变番型排斥、最低门槛、加成规则

如果前期技能不多，建议先做保守型扩展，工程风险更低。

## 10. 视图层与真实状态必须分离

当前项目里，房间快照和 prompt 基本都直接从 `room` 派生。  
后续建议拆成两个明确对象：

```rust
pub struct PlayerRoomSnapshot {
    pub table_code: String,
    pub phase: RoomPhase,
    pub seats: Vec<PublicSeatView>,
    pub local_seat: Seat,
    pub match_state: Option<PublicMatchView>,
    pub private_state: Option<PlayerRoundView>,
    pub continue_action: Option<ContinueActionView>,
}
```

```rust
pub struct PlayerRoundView {
    pub round_id: RoundId,
    pub round_wind: Wind,
    pub dealer_seat: Seat,
    pub current_actor: Seat,
    pub wall_tiles_remaining: usize,
    pub players: Vec<PlayerSeatView>,
    pub pending_action: Option<ActionPrompt>,
    pub visible_effects: Vec<VisibleEffectView>,
    pub private_knowledge: Vec<KnowledgeView>,
}
```

这层分离有三个直接收益：

- 能处理“真实信息”和“技能幻觉信息”
- 能控制重连时每个玩家恢复到自己该看到的状态
- bot 可以复用同一套权限化投影

## 11. bot 改造建议

bot 当前已经有不错的结构化上下文，但上下文的来源还不够安全。  
建议把 bot 输入改为：

```rust
pub struct BotContextView {
    pub seat: Seat,
    pub round_meta: BotRoundMeta,
    pub self_state: BotSelfState,
    pub opponents: Vec<BotOpponentView>,
    pub public_knowledge: PublicKnowledgeView,
    pub private_knowledge: PrivateKnowledgeView,
    pub legal_actions: Vec<BotLegalAction>,
}
```

其中：

- `public_knowledge` 代表全员可见信息
- `private_knowledge` 代表技能额外带来的情报
- 不应该直接暴露完整真实牌墙或完整真实他家手牌

这样未来即使出现“错觉类技能”“假情报技能”“局部窥视技能”，bot 仍然可以被公平约束。

## 12. 渐进迁移方案

不建议一次性重写。按当前代码量，最稳妥的迁移步骤如下。

### 阶段 1：引入强类型状态，但不改外部接口

- 新建 `core/state/*`
- 为当前房间 JSON 建等价结构体
- 持久化仍然序列化成 JSON 字符串
- 旧逻辑和新逻辑短期共存

目标：

- 先把“状态类型化”做出来
- 不急着一次性把规则逻辑全部拆完

### 阶段 2：先拆 projection 层

- 把当前 `room_snapshot`、`private_round_state`、`private_pending_action` 迁到 `projection/*`
- 明确“真实状态 -> seat 专属视图”的边界

目标：

- 为技能中的“信息差”类效果打基础

### 阶段 3：把 `try_handle_action` 改成 command + reducer

- 定义 `GameCommand`
- 定义 `GameEvent`
- 把主要状态变化收束到 reducer

目标：

- 为技能 hook 提供固定介入点

### 阶段 4：接入技能 hook 框架

- 加 `rules/skills/*`
- 加 `RuleContext`, `SkillContext`
- 先支持 2 到 3 个简单技能验证架构

建议优先验证的技能类型：

- 信息投影类：窥视一张牌
- 摸牌修正类：额外摸一张
- 结算修正类：某类番型 +1

### 阶段 5：bot 切到权限化视图

- bot 改为只消费 `BotContextView`
- 清理 bot 对真实全局状态的直接依赖

## 13. 第一批建议优先落地的类型

如果你准备开始真正动代码，优先定义下面这些类型，收益最高：

```rust
RoomState
RoundState
PlayerRoundState
PendingAction
PendingTimeout
GameCommand
GameEvent
RuleContext
PlayerRoomSnapshot
BotContextView
SkillInstance
EffectInstance
ScoreRequest
ScoreResult
```

有了这批类型，后面无论是拆标准规则、接技能、还是重写 projection，都不会一直卡在 `serde_json::Value` 上。

## 14. 当前项目最重要的设计结论

对于你的项目，技能系统能否做好，关键不在于“技能模块单独放哪”，而在于先做出这三条边界：

1. **真实状态 vs 玩家视图**
2. **标准规则推进 vs 技能修改点**
3. **标准算番结果 vs 技能结算修正**

如果这三条边界立住了，后面即使继续维持国标麻将的标准规则内核，技能也能稳定扩展。  
如果不先立边界，而是继续在 `mahjong.rs` 的大函数里加条件分支，后续会非常难维护。

## 15. 推荐的第一轮落地顺序

建议真正开工时按这个顺序推进：

1. 新建 `core/state/*`，把房间和回合状态类型化
2. 新建 `projection/*`，把快照生成逻辑迁出去
3. 新建 `core/action.rs` 和 `core/event.rs`
4. 把 `try_handle_action` 迁成 `GameCommand -> Vec<GameEvent>`
5. 给 `scoring.rs` 外面包一层 `ScoreEvaluator + ScoreModifier`
6. 再接 `rules/skills/*`

这个顺序能保证每一步都能编译、能测试、能回退，不会出现“大重构做到一半系统不可用”的情况。

## 16. Current Refactor Status (2026-04-08)

### 16.1 Completed Or Largely Completed

- Phase 1 is largely complete. `core/state/*`, `core/action.rs`, `core/event.rs`, and `core/engine/*` are in place, and the project already has a typed state plus command/reducer transition skeleton.
- Phase 2 is mostly complete. `projection/room_snapshot.rs`, `projection/prompt.rs`, `projection/bot_view.rs`, and `projection/support.rs` now own snapshot/prompt/bot-view/projection-support responsibilities instead of leaving them inside `mahjong.rs`.
- Standard-rule execution flow has been moved substantially into `rules/standard/*`. `actions.rs`, `flow.rs`, `runtime.rs`, `settlement.rs`, `win.rs`, and `automation.rs` now carry discard/claim/kong flow, round progression, timeout/runtime projection, settlement, win evaluation, bot automation, and due-timeout automation.
- The scoring boundary now exists under `rules/scoring/*`. `rules/scoring/model.rs`, `rules/scoring/evaluator.rs`, and `rules/scoring/fan_table.rs` are in place, and `rules/standard/win.rs` plus other scoring consumers now depend on `crate::rules::scoring` instead of a root-level monolith.
- The bot boundary now exists under `bot/*`. `bot/context.rs`, `bot/policy.rs`, and `bot/search.rs` are in place, while `rules/standard/automation.rs` remains the orchestration layer that calls the bot policy entrypoints.
- Bot input boundaries are now meaningfully constrained. Bot decision code consumes `projection::bot_view` outputs instead of ad-hoc room-derived internal structs living in `mahjong.rs`.
- The app/runtime split is now materially in place under `app/*`. `app/persistence.rs`, `app/room_runtime.rs`, `app/scheduler.rs`, `app/server.rs`, and `app/ws.rs` now hold persistence, room lifecycle, timeout scheduling, HTTP, and WebSocket/session responsibilities; `main.rs` has been reduced to bootstrap plus tests.
- The skills layer is no longer scaffold-only. `rules/skills/*` now contains a live registry/hook system, all 36 技能 definitions are registered, and standard flow already invokes skill hooks from projection, hu/scoring, draw-settlement, active skill activation, and decline-hu passive paths.
- Explicit skill runtime trackers now exist on typed state, and they are no longer just opaque `Value` blobs. `core/state/skill_trackers.rs` now defines typed round/match tracker structures, `RoundState.skill_trackers` and `MatchState.skill_trackers` deserialize into those types, and skill logic reads/writes them through typed state plus reducer mutations instead of treating tracker payloads as anonymous JSON.
- The settlement model is now materially typed end-to-end inside the backend core. `core/state/settlement.rs` owns typed settlement data, `RoundState.settlement` carries `RoundSettlement`, standard hu/draw settlement builders produce typed settlement directly, and planner/reducer/match-score application consume typed deltas rather than reparsing settlement JSON.
- Settlement events now align with the typed model. Standard settlement messages (`settlement_ready` / `round_drawn`) carry the settlement payload, and `core/engine/command.rs` can now lift those messages into `GameEvent::SettlementPrepared { settlement: RoundSettlement }` instead of leaving settlement as a legacy-only side channel.
- Standard action hot paths now lean more directly on typed room reads. The local discard / claim resolution path in `rules/standard/actions.rs` now validates and derives follow-up draw/claim state primarily from projected `RoomState` / `RoundState` instead of repeatedly rereading the same facts from raw room JSON.
- The engine result boundary has moved further away from message-first adaptation on the hottest standard-action path. Local discard / claim / rob-kong-pass command execution now returns `EngineOutput { events, emitted_messages }` directly, with typed `TileDiscarded` / `MeldClaimed` / replacement-draw events assembled before legacy round messages are emitted.
- Additional standard-rule branches now cross that same boundary directly. Flower actions, self-kong resolution, rob-kong completion, opening-flower pass, hu settlement, and exhaustive-draw settlement now construct `EngineOutput` natively instead of relying on message-only helpers plus `extract_events_from_messages` inside engine flow.
- Planner output has crossed the same boundary for discard flow and no longer exposes legacy mutation payloads there. `plan_discard_action` now returns a typed discard continuation model directly.
- Planner output has now crossed that boundary in more than just discard flow. Opening-flower advance, flower replacement, claim-window response/continuation, and settlement-to-match progression now produce typed plan structures directly; the old compatibility mutation adapter methods have been removed from `planner.rs`.
- A typed room-state update helper now exists at the reducer boundary, and it has become the only runtime write path inside `backend/src`. `core/engine/reducer.rs` now normalizes room writes entirely through typed `RoomState`, while `rules/standard/runtime.rs`, large parts of `rules/standard/flow.rs`, standard win/draw settlement writeback, the main discard / claim / self-kong / rob-kong writeback paths in `rules/standard/actions.rs`, and many tracker/effect/score update helpers in `rules/skills/mod.rs` all update `RoomState` directly.
- The old reducer mutation language is no longer part of the active backend runtime. `LegacyRoomMutation` and `apply_legacy_room_mutations(...)` have been removed from `backend/src`, so runtime state transitions no longer flow through a compatibility mutation DSL before reaching typed state.
- The skills runtime bridge is shrinking further. Several `rules/skills/mod.rs` helpers that previously edited `round_state.skill_trackers`, `match_state.cumulative_scores`, `match_state.skill_trackers`, `effect_state`, and skill charge/version fields in-place now project typed room state, mutate typed round/match structures, and write back through typed reducer updates instead.
- The outer compatibility facade is also starting to follow the same rule. Parts of `mahjong.rs` that previously reasoned over raw room JSON for readiness checks, bot-seat injection, and local discard support now project or update typed `RoomState` first instead of hand-editing `serde_json::Value`.
- Engine command dispatch now also enters through typed context first. `core/engine/flow.rs` constructs `EngineContext` from projected `RoomState`, and the older `EngineContext::from_legacy_room` compatibility constructor has been removed.
- Several outward-facing runtime entrypoints now use neutral room/command APIs instead of explicit legacy naming. `parse_player_command`, `RoomState::from_room_value` / `to_room_value`, `RoomScoringCache::from_room_value`, and `apply_skill_events_to_room` / `apply_passive_skill_events_to_room` are now the active runtime-facing interfaces.
- The same neutralization has now pushed much deeper into core value types. `Tile`, `RoomState`, `RoundState`, `RoundSettlement`, `SkillLoadout`, `EffectState`, pending-action state, wall/player/match state, and skill tracker helpers all expose `from_value` / `to_value` style APIs for active runtime use, while the older `from_legacy_value` / `to_legacy_value` names are increasingly just thin compatibility aliases.
- The old `GameEvent::LegacyRoundEvent` outward transport has been removed from the active backend event model. Flower exposure, self-kong declaration, claim auto-pass, and skill-driven tile/score/meld/draw side effects now emit typed `GameEvent` variants directly, and the message-to-event shim in `core/engine/command.rs` only reconstructs those typed variants in tests.
- Outward projection code is now typed more consistently as well. `room_messages` in `mahjong.rs` now projects room state once and delegates match-result shaping to `projection/match_result.rs`; `projection/room_snapshot.rs` also serializes typed `MatchState` directly instead of first degrading it to raw `Value`.
- Websocket request intake has also started crossing that same seam. `app/ws.rs` now decodes join/reconnect/ready/action/quick-chat payloads through typed request structs before dispatch instead of repeatedly hand-reading `payload["..."]` fields inline.
- The `app` layer's internal room helpers have also moved substantially toward typed state. `initial_room_payload` now builds a `RoomState` first, common phase/seat/deadline/session lookups in `app/mod.rs` now read through `RoomState::from_room_value`, and bot/seat connectivity helpers there now write back through `update_room_state(...)` instead of mutating raw room JSON in-place. `app/ws.rs` join/reconnect/ready seat updates and `app/room_runtime.rs` bot-only / unattended checks now lean on the same typed room model.
- The persistence-facing room boundary has also started turning typed-first. `app/mod.rs` now exposes `parse_room_json(...)` and `serialize_room_state(...)`, `serialize_room(...)` normalizes through `RoomState`, `app/server.rs` creates new tables from typed room state rather than raw JSON assembly, and `app/room_runtime.rs` restores persisted rooms by parsing typed state first before applying reconnect/disconnect recovery.
- Outbound protocol shaping is also becoming less ad-hoc. `app/protocol.rs` now owns several common websocket/http payloads (`action_rejected`, `player_presence`, `quick_chat`, `leave_table_accepted`, heartbeat echoes, create-table responses, and common error/detail bodies), so `app/ws.rs`, `app/mod.rs`, and `app/server.rs` no longer hand-assemble those payloads inline with repeated `json!` fragments.
- The room JSON shape used internally by runtime/persistence has moved one step closer to the typed model as well. `RoomState::to_room_value()` now serializes `continue_action` as a nested typed object instead of re-expanding the old trio of top-level fields, active runtime helpers now read continue deadlines through typed `continue_action`, and most core test fixtures have been migrated to that new shape; flat continue-action fields remain only as parse-time backward compatibility.
- Seat-level skill configuration is now also represented in the typed room model itself. `SeatState` now carries `skill_loadout`, so start-match / restart-round setup no longer needs to fall back to raw room JSON just to preserve equipped skills into the next round.
- A large slice of compatibility alias APIs has also been physically removed. Unused `from_legacy_value` / `to_legacy_value` wrappers across tile/state modules have been deleted, so active core types present typed `from_value` / `to_value` interfaces directly instead of keeping parallel legacy-named entrypoints alive.
- The `app/runtime` layer has now crossed the same seam internally. `RoomRuntime` now stores `RoomState` instead of raw `Value`, app-level seat/deadline/bot helpers operate on typed room state directly, and websocket/scheduler flows now invoke typed standard/engine entrypoints directly instead of materializing JSON just to call `mahjong.rs`.
- Bot decision and due-timeout automation on the production path now also execute from typed room state first. Scheduler-triggered discard / flower / pass automation no longer needs a room-wide `RoomState <-> Value` conversion just to choose or dispatch the timed action.
- The backend no longer accepts the old flat continue-action schema on active room parsing, and core decode errors have also been renamed away from explicit `legacy_*` terminology. At this point the runtime-side room contract is effectively the typed nested shape.
- `mahjong.rs` is no longer part of the active backend runtime path. The remaining file is now test-only coverage around the older `Value`-based entrypoints, while production `app/*` code calls typed `RoomState` / engine / standard-rule APIs directly.
- The last production `RoomState <-> Value` bridges inside standard execution have now been removed as well. `rules/standard/actions.rs`, `rules/standard/flow.rs`, `rules/standard/win.rs`, and `rules/skills/mod.rs` now execute their typed room-state entrypoints natively; the remaining whole-room conversion is confined to the `mahjong.rs` test-only compatibility layer rather than the core runtime modules.
- That test-only compatibility layer has now also been narrowed further. `mahjong.rs` mostly converts into `RoomState` locally and then calls typed engine / standard-rule entrypoints, while several older `Value`-based standard flow/win wrappers are now explicitly test-only instead of remaining generic runtime entrypoints.
- Room persistence has now crossed the schema seam as well. `parse_room_json(...)` / `serialize_room_state(...)` use direct serde on typed `RoomState`, `RoundState` now flattens `rule_state` into the persisted room shape, pending-action state serializes through a tagged typed enum instead of ad-hoc JSON assembly, and claim-window response history is stored as typed `ClaimResponse` records instead of raw `Value` blobs.
- Websocket command intake is now typed at the payload boundary too. Action requests no longer deserialize `tile_ids` as anonymous JSON arrays, heartbeat traffic now uses a typed payload struct, and the common protocol helpers in `app/protocol.rs` are built around serializable payload types rather than hand-built shape fragments.
- The direct serde room schema now tolerates explicit `null` on optional/defaulted typed subtrees such as effect state, skill trackers, and skill loadouts. That keeps persistence and test fixtures aligned with the typed model without reintroducing legacy field shims.
- The app-side outbound transport container has now also been narrowed. `ConnectionHandle::outbound(...)` serializes payloads immediately, `OutboundMessage` stores the serialized JSON string instead of carrying `serde_json::Value` as an internal app-runtime message object, and `send_outbound(...)` no longer performs a second JSON serialization pass at flush time.
- The old mixed projection helper has been removed as well. Snapshot/prompt support building now flows through `build_seat_projection_support_for_state(...)` only, so the projection layer no longer needs a parallel `build_seat_projection_support(room_json, state, ...)` helper just to answer hu/kong/discard support questions.

### 16.2 Still Transitional / Not Yet Finished

- The typed state is now the single production source of truth inside `backend/src` across runtime execution, persistence, and websocket request intake. The remaining `Value` usage is much narrower and is no longer a legacy-state dependency: it mostly lives in outbound JSON transport, intentionally dynamic skill/effect payloads, and test-only compatibility helpers.
- The write path has moved fully into typed state inside the backend core, and the previously remaining standard/skill `RoomState <-> Value` execution bridges have been retired. The remaining whole-room conversion points are test-scoped shims (`mahjong.rs`, some `main.rs`/engine tests) rather than production runtime adapters.
- The engine split has moved past the original `mod.rs` catch-all layout. `core/engine/command.rs`, `flow.rs`, `validation.rs`, `planner.rs`, and `reducer.rs` now exist, and production room execution no longer routes through `mahjong.rs`. Standard claim-window / rob-kong timeout handling and hu-action hinting have also started consuming typed `PendingAction` instead of reparsing room JSON in-place. Discard / claim / flower / self-kong / hu / exhaustive-draw settlement execution now produce typed `EngineOutput` directly, and standard continue-action / restart-match progression now also runs through typed `RoomState` logic on the production path.
- Settlement is no longer the main typed-state blocker, and skill tracker state has also crossed into typed structures. The remaining compatibility seams are now concentrated primarily in outward-facing room/message serialization and protocol shims rather than in internal mutation planning or standard-action internals.
- The biggest remaining transition layer is no longer `mahjong.rs` or room persistence. Production runtime, stored room JSON, and websocket request intake have all crossed to typed-first schemas; what remains mostly concerns outward JSON message transport itself and a handful of deliberately opaque payload fields used by skills/effects.
- The most visible remaining transition surfaces are now transport-oriented rather than state-oriented. `RoomRuntime` is typed, persisted rooms are typed-first, request intake is typed-first, standard continue-action / restart progression no longer needs a typed-to-JSON detour, bot/timeout automation no longer needs a room-wide detour either, and the remaining whole-room conversion now lives only inside test shims; what remains is mainly websocket/server message serialization and intentionally dynamic effect payload contracts.
- The remaining `mahjong.rs` compatibility surface is now more clearly limited to tests than to shared projection/runtime helpers, but its timeout shim still intentionally preserves older test semantics. If we want to finish removing that file entirely, the remaining work is to port those tests onto typed room fixtures directly rather than to keep teaching the shim new behavior.
- The scoring split is only partially internalized. Root `scoring.rs` is now a compatibility facade, but much of the scoring domain still lives in `rules/scoring/evaluator.rs`; `room_scoring.rs` also remains as transitional glue.
- `mahjong.rs` no longer acts as a live facade for production room execution. What remains to finish is retiring the last internal `Value` adapter seams behind the typed standard/core APIs, plus the broader outward protocol/schema cleanup.
- Projection layering is still not fully finished. The current code has `room_snapshot`, `prompt`, `bot_view`, and `support`, but the document's dedicated `private_view.rs` style separation is not yet present.
- The `infra/*` layer proposed earlier in this document still does not exist. Serialization/time/random helpers are still spread across the current modules instead of being centralized behind an infra boundary.

### 16.3 Current Architectural Read

- The backend is no longer in the state where almost all standard-rule logic lives in `mahjong.rs`.
- The biggest remaining architecture debt is no longer "can skills be integrated?", because that milestone has already been crossed. The main debt is now "can the outward JSON room/protocol contracts be rewritten so typed state is not merely the runtime source of truth, but also the default persistence/API schema?"
- The app/runtime boundary is now good enough that further leverage mostly comes from consolidating write-time state transitions, not from additional `main.rs` cleanup.
- This is the right point to stop treating skills as the main unknown and instead finish the typed reducer / scoring / outward-schema cleanup so the skill-enabled architecture becomes the default architecture rather than a partially bridged one.
- Room creation/persistence helpers are also now more clearly typed-first: the active app path constructs new rooms from typed `RoomState` directly, and tests/persistence helpers increasingly serialize typed room state instead of synthesizing an intermediate room `Value` first.

## 17. Recommended Next Steps

### 17.1 Immediate Delivery Priority (2026-04-09): Connect The Frontend To The Real Skill Runtime

Recommended target:

- stop treating the frontend skill layer as an independent local runtime and move it onto backend-driven equipped-skill state plus real `skill:*` actions
- keep this step intentionally narrow: integrate equipped-skill display, activation affordances, and post-activation state/effect rendering before attempting any frontend-side skill draft / offer / selection loop
- if frontend display metadata is still partially catalog-backed for a transition period, keep the backend as the only source of runtime truth for charges, activation availability, effects, private knowledge, and action legality

Goal:

- remove the current split-brain state where backend skill execution is real but frontend skill selection/activation UX is still largely simulated
- prioritize player-visible end-to-end leverage over further backend-only internal cleanup whose remaining scope is now mostly test shims and intentionally dynamic payloads

Architectural read behind this priority:

- production backend state, persistence, request intake, and runtime mutation are already typed-first enough that the biggest remaining product gap is no longer internal room-state typing
- the main mismatch has shifted outward: the backend owns the real equipped-skill runtime, while the frontend still carries a separate catalog/runtime for offers, chosen skills, and activation state
- because backend does not yet implement a live skill-offer / draft loop, the right near-term integration target is **equipped skills only**, not a forced merge of the frontend's local offer-selection mechanics into the current server

### 17.2 First Cleanup Priority: Retire The Remaining Outer JSON Compatibility Boundary

Recommended target:

- replace the persisted room JSON shape with a schema centered on typed `RoomState` instead of the older ad-hoc payload shape
- collapse websocket/http command intake and outbound payload shaping onto typed request/response/event models instead of raw JSON glue
- delete the last test-only whole-room conversion wrapper in `core/engine/flow.rs` once `mahjong.rs` test coverage is migrated or retired

Goal:

- make `RoomState` / `RoundState` the default source of truth not only at runtime, but also at persistence and protocol boundaries
- remove the last architectural mismatch between the typed core and the still-JSON outward contracts

### 17.3 Second Cleanup Priority: Deepen `core/engine/*` Separation

Recommended target:

- build on the now-landed `command` / `validation` / `flow` split inside `core/engine/*`
- continue shrinking the remaining compatibility-oriented execution code and mutation orchestration that still sits behind `serde_json::Value` entrypoints
- make command parsing, validation, planning, reduction, and compatibility adaptation easier to test independently

Goal:

- stabilize the game core boundary so rules, app runtime, and compatibility layers stop leaning on one another's internal details

### 17.4 Third Cleanup Priority: Deepen `rules/scoring/*` Internal Separation

Recommended target:

- keep `rules/scoring/model.rs` focused on request/result/data types
- move more rule-table-specific code behind `rules/scoring/fan_table.rs`
- reserve `rules/scoring/evaluator.rs` for evaluation flow, caching, and orchestration
- continue shrinking `room_scoring.rs` until it becomes either a thin cache adapter or disappears entirely

Goal:

- make future `ScoreModifier` / skill-driven scoring adjustments land on a stable scoring boundary
- reduce the amount of scoring-specific domain data and fan-table definition still co-located in a single file

### 17.5 Fourth Cleanup Priority: Finish The Remaining Transitional Facades

Recommended target:

- continue shrinking `mahjong.rs`
- retire compatibility-only helper layers where the new modules already own the real behavior
- decide whether `projection/private_view.rs` and `infra/*` are worth adding as concrete modules or whether the current split should be documented as the final shape

Goal:

- convert the current "mostly migrated but still compatibility-aware" structure into the default steady-state architecture

## 18. Current Conclusion

As of 2026-04-08, this refactor has moved well past the "prepare the boundaries first" stage. Standard-rule execution has materially left `mahjong.rs`, app/runtime responsibilities are now mostly under `app/*`, bot/scoring/projection all have subsystem boundaries, and the skill system is already real enough to support the 36 技能 registry plus runtime trackers and hook-driven projection/scoring/settlement adjustments.

The highest remaining leverage is now consolidation rather than invention: finish replacing legacy JSON mutation as the dominant write path, finish the internal engine/scoring cleanup, and keep shrinking the remaining compatibility facades until the typed architecture is the actual runtime architecture rather than a partially wrapped one.
