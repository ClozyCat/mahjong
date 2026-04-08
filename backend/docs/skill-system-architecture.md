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
- A first-pass skills boundary now exists under `rules/skills/*`. Registry, hook/context, effect/instance helpers, and a typed activation entrypoint are present, so `PlayerAction::ActivateSkill` no longer has to be hardcoded as an unconditional unsupported branch.
- `mahjong.rs` has already shrunk from a rules megafile into a transitional facade plus a smaller set of entrypoints and test fixtures. Large dead helper blocks and commented legacy code have been physically removed.

### 16.2 Still Transitional / Not Yet Finished

- The scoring split is only partially internalized. Root `scoring.rs` is now a compatibility facade, but most scoring internals still live inside `rules/scoring/evaluator.rs`; `model.rs` and `fan_table.rs` currently provide boundary structure more than deep implementation separation.
- `main.rs` still owns a lot of runtime / websocket / persistence / scheduling behavior and has not yet been moved to `app/*`.
- The skills layer is only scaffolded so far. `rules/skills/*` exists, and activation now goes through a registry boundary, but hooks are not yet invoked from standard draw / hu / scoring / projection flow and no builtin skills are registered.
- `mahjong.rs` still keeps external compatibility entrypoints and some thin wrappers, so it is much smaller but not yet eliminated as a transition facade.

### 16.3 Current Architectural Read

- The backend is no longer in the state where almost all standard-rule logic lives in `mahjong.rs`.
- The highest remaining leverage is now `main.rs` runtime extraction plus turning the new `rules/skills/*` boundary from scaffold into an actually-invoked hook chain.
- This is the right point to stop optimizing the facade first and instead stabilize the remaining subsystem boundaries while turning the new `rules/skills/*` scaffold into an actually-invoked extension layer.

## 17. Recommended Next Steps

### 17.1 First Priority: Split `main.rs` Runtime Responsibilities

Recommended target:

- `app/room_runtime.rs`
- `app/scheduler.rs`
- `app/persistence.rs`
- `app/ws.rs`

Goal:

- move websocket, room lifecycle, timeout scheduling, and persistence out of the app entrypoint
- reduce `main.rs` back to composition/bootstrap duties

### 17.2 Second Priority: Wire `rules/skills/*` Into Standard Flow

Recommended target:

- invoke skill hooks from draw / hu / scoring / projection boundaries
- start consuming `SkillLoadout` / `EffectState` instead of leaving them passive data only
- register the first builtin validation skill set

Goal:

- turn the current activation/registry scaffold into a real skills execution path
- prove that information, draw, and scoring modifiers can land without reopening the old megafile design

### 17.3 Third Priority: Deepen `rules/scoring/*` Internal Separation

Recommended target:

- keep `rules/scoring/model.rs` focused on request/result/data types
- move more rule-table-specific code behind `rules/scoring/fan_table.rs`
- reserve `rules/scoring/evaluator.rs` for evaluation flow, caching, and orchestration

Goal:

- make future `ScoreModifier` / skill-driven scoring adjustments land on a stable scoring boundary
- reduce the amount of scoring-specific domain data and fan-table definition still co-located in a single file

### 17.4 Fourth Priority: Introduce Skills On Stable Boundaries

After scoring / bot / app runtime are split, introduce:

- `rules/skills/registry.rs`
- `rules/skills/hooks.rs`
- `rules/skills/effects.rs`
- `rules/skills/instances.rs`

Suggested first validation skills:

- projection / information-visibility skill
- draw modification skill
- settlement / fan / score modifier skill

## 18. Current Conclusion

As of 2026-04-08, this refactor has passed the "preparing to refactor" stage and is now in the "standard-rule execution paths have materially left `mahjong.rs`, scoring and bot have subsystem boundaries, and skills have an initial registry/hook scaffold" stage. The most important next move is no longer more local facade cleanup, but extracting the remaining `main.rs` runtime responsibilities and then wiring the new `rules/skills/*` boundary into real rule flow so that skill-based国标麻将 can be implemented on stable extension points.
