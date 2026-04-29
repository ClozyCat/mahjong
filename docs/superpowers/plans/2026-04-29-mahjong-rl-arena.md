# Mahjong RL Arena Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the reinforcement-learning foundation for the Mahjong bot by adding a deterministic arena, policy configuration, trajectory export, PPO training, and league evaluation.

**Architecture:** Rust remains the authoritative rules and self-play engine. The first milestone adds a deterministic all-bot arena and explicit policy routing; the second milestone exports legal masked decision trajectories; the third milestone trains PPO from the supervised checkpoint and selects models through arena results.

**Tech Stack:** Rust 2024 backend (`serde`, `serde_json`, existing standard rules, bot policy/search/neural modules, `ort`), Python trainer (`torch`, existing V2 model/dataset/export tooling), JSONL experiment artifacts, PowerShell/Bash wrappers.

---

## Scope

This plan implements the approved design in `docs/superpowers/specs/2026-04-29-mahjong-rl-arena-design.md`.

The first implementation should stop after a working arena MVP if code risk grows. Trajectory export and PPO can be implemented after the arena proves deterministic and produces stable baseline metrics.

## File Map

- Create: `backend/src/bot/arena.rs`
  Responsibility: arena config structs, match/seat metric structs, trajectory structs, aggregation helpers, and tests.
- Modify: `backend/src/bot/mod.rs`
  Responsibility: export arena module and policy config types where needed.
- Modify: `backend/src/bot/policy.rs`
  Responsibility: add explicit-config policy entry points while keeping env-based production wrappers.
- Modify: `backend/src/bot/neural.rs`
  Responsibility: add explicit model-path scoring helper for arena policies.
- Modify: `backend/src/rules/standard/automation.rs`
  Responsibility: route bot actions through a policy resolver for arena simulations while preserving production behavior.
- Modify: `backend/src/rules/standard/flow.rs`
  Responsibility: expose deterministic next-round helper if arena needs multi-round match simulation without random seeds.
- Modify: `backend/src/bot/search.rs`
  Responsibility: expose a small shanten/tenpai helper for arena telemetry.
- Create: `backend/src/bin/bot_arena.rs`
  Responsibility: CLI that runs arena matches and writes reports/summary/trajectories.
- Create: `backend/bot_trainer/v2/arena_smoke.json`
  Responsibility: small reproducible arena config for smoke verification.
- Create: `backend/bot_trainer/v2/arena_matrix.ps1`
  Responsibility: Windows experiment matrix runner.
- Create: `backend/bot_trainer/v2/arena_matrix.sh`
  Responsibility: Linux experiment matrix runner.
- Modify: `backend/bot_trainer/v2/README.md`
  Responsibility: document arena, baseline matrix, trajectory export, and RL commands.
- Create: `backend/bot_trainer/v2/rl_dataset.py`
  Responsibility: load arena trajectory JSONL into tensors for PPO.
- Create: `backend/bot_trainer/v2/rl_train.py`
  Responsibility: PPO training from arena trajectories and supervised checkpoints.
- Modify: `backend/bot_trainer/v2/export_onnx.py`
  Responsibility: export PPO checkpoints if their checkpoint metadata uses the same model state format.
- Create: `backend/bot_trainer/v2/test_rl_dataset.py`
  Responsibility: Python tests for trajectory loading, returns, advantages, and masked PPO loss.

## Data Contracts

### Arena Config

```json
{
  "matches": 2,
  "seed": 20260429,
  "max_actions_per_match": 2400,
  "report_trajectories": false,
  "policies": [
    {"id": "heuristic", "mode": "heuristic", "neural_weight": 0, "model_path": null},
    {"id": "hybrid30", "mode": "hybrid", "neural_weight": 30, "model_path": "backend/assets/models/mahjong_policy_net.onnx"}
  ]
}
```

### Match Report Row

```json
{
  "match_index": 0,
  "seed": 20260429,
  "completed": true,
  "action_count": 312,
  "seats": [
    {
      "seat_index": 0,
      "policy_id": "heuristic",
      "score_delta": 8,
      "wins": 1,
      "dealt_in": 0,
      "first_tenpai_turn": 9,
      "final_tenpai": true,
      "claim_count": 1,
      "discard_count": 16,
      "decision_count": 17,
      "decision_latency_ms_sum": 25
    }
  ]
}
```

### Trajectory Row

```json
{
  "schema_version": 1,
  "match_id": "arena-20260429-0",
  "decision_index": 42,
  "seat_index": 2,
  "policy_id": "hybrid30",
  "decision_kind": "active_turn",
  "tile_planes": [0.0],
  "scalar_features": [0.0],
  "discard_mask": [true],
  "claim_mask": [true],
  "self_kong_mask": [true],
  "hu_mask": [true, false],
  "action_head": "discard",
  "action_index": 18,
  "action_semantic": "discard:b1",
  "log_prob": 0.0,
  "value": 0.0,
  "reward": 0.0,
  "done": false
}
```

The arrays above are shortened examples. The implementation must write complete arrays with stable shapes: `tile_planes = 10 * 34`, `scalar_features = 10`, `discard_mask = 34`, `claim_mask = 7`, `self_kong_mask = 3`, and `hu_mask = 2`.

## Task 1: Add Arena Data Types

**Files:**
- Create: `backend/src/bot/arena.rs`
- Modify: `backend/src/bot/mod.rs`

- [ ] **Step 1: Add arena config and report structs**

Create `backend/src/bot/arena.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArenaPolicyMode {
    Heuristic,
    Hybrid,
    Neural,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ArenaBotPolicyConfig {
    pub id: String,
    pub mode: ArenaPolicyMode,
    pub neural_weight: i64,
    pub model_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ArenaConfig {
    pub matches: usize,
    pub seed: u64,
    #[serde(default = "default_max_actions_per_match")]
    pub max_actions_per_match: usize,
    #[serde(default)]
    pub report_trajectories: bool,
    pub policies: Vec<ArenaBotPolicyConfig>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct ArenaSeatMetrics {
    pub seat_index: usize,
    pub policy_id: String,
    pub score_delta: i64,
    pub wins: u64,
    pub dealt_in: u64,
    pub first_tenpai_turn: Option<u64>,
    pub final_tenpai: bool,
    pub claim_count: u64,
    pub discard_count: u64,
    pub decision_count: u64,
    pub decision_latency_ms_sum: u128,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ArenaMatchReport {
    pub match_index: usize,
    pub seed: u64,
    pub completed: bool,
    pub action_count: usize,
    pub seats: Vec<ArenaSeatMetrics>,
}

fn default_max_actions_per_match() -> usize {
    2400
}

impl ArenaBotPolicyConfig {
    pub fn heuristic() -> Self {
        Self {
            id: "heuristic".to_string(),
            mode: ArenaPolicyMode::Heuristic,
            neural_weight: 0,
            model_path: None,
        }
    }
}
```

- [ ] **Step 2: Export the arena module**

Modify `backend/src/bot/mod.rs`:

```rust
pub mod arena;
mod action_space;
mod context;
mod features;
mod neural;
mod policy;
mod search;

pub use policy::{choose_active_turn_action, choose_claim_action};
```

If `mod.rs` already exports the policy functions, preserve its current public exports and only add `pub mod arena;`.

- [ ] **Step 3: Add config parsing tests**

Append tests to `backend/src/bot/arena.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_config_parses_policy_ids() {
        let raw = r#"{
            "matches": 2,
            "seed": 20260429,
            "policies": [
                {"id":"heuristic","mode":"heuristic","neural_weight":0,"model_path":null},
                {"id":"hybrid30","mode":"hybrid","neural_weight":30,"model_path":"backend/assets/models/mahjong_policy_net.onnx"}
            ]
        }"#;

        let config: ArenaConfig = serde_json::from_str(raw).expect("config");

        assert_eq!(config.matches, 2);
        assert_eq!(config.seed, 20260429);
        assert_eq!(config.max_actions_per_match, 2400);
        assert!(!config.report_trajectories);
        assert_eq!(config.policies[1].id, "hybrid30");
        assert_eq!(config.policies[1].mode, ArenaPolicyMode::Hybrid);
    }

    #[test]
    fn heuristic_policy_config_has_stable_defaults() {
        let config = ArenaBotPolicyConfig::heuristic();

        assert_eq!(config.id, "heuristic");
        assert_eq!(config.mode, ArenaPolicyMode::Heuristic);
        assert_eq!(config.neural_weight, 0);
        assert_eq!(config.model_path, None);
    }
}
```

- [ ] **Step 4: Verify arena types**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::arena -- --nocapture
```

Expected: PASS.

## Task 2: Add Explicit Policy Entry Points

**Files:**
- Modify: `backend/src/bot/policy.rs`
- Modify: `backend/src/bot/neural.rs`
- Test: `backend/src/bot/policy.rs`
- Test: `backend/src/bot/neural.rs`

- [ ] **Step 1: Add env-to-config conversion**

In `backend/src/bot/policy.rs`, import the arena types:

```rust
use crate::bot::arena::{ArenaBotPolicyConfig, ArenaPolicyMode};
```

Add this helper near the existing policy mode helpers:

```rust
fn bot_policy_config_from_env() -> ArenaBotPolicyConfig {
    let mode = match env::var(POLICY_ENV).ok().as_deref() {
        Some(value) if value.eq_ignore_ascii_case("neural") => ArenaPolicyMode::Neural,
        Some(value) if value.eq_ignore_ascii_case("hybrid") => ArenaPolicyMode::Hybrid,
        _ => ArenaPolicyMode::Heuristic,
    };
    ArenaBotPolicyConfig {
        id: match mode {
            ArenaPolicyMode::Heuristic => "env-heuristic",
            ArenaPolicyMode::Hybrid => "env-hybrid",
            ArenaPolicyMode::Neural => "env-neural",
        }
        .to_string(),
        mode,
        neural_weight: neural_prior_weight(),
        model_path: env::var("MAHJONG_BOT_MODEL_PATH").ok(),
    }
}
```

- [ ] **Step 2: Route existing public functions through config wrappers**

Replace the top of `choose_active_turn_action` with:

```rust
pub fn choose_active_turn_action(context: &BotContext) -> Option<BotAction> {
    choose_active_turn_action_with_config(context, &bot_policy_config_from_env())
}
```

Replace the top of `choose_claim_action` with:

```rust
pub fn choose_claim_action(context: &BotContext) -> Option<BotAction> {
    choose_claim_action_with_config(context, &bot_policy_config_from_env())
}
```

Move the existing bodies into:

```rust
pub fn choose_active_turn_action_with_config(
    context: &BotContext,
    config: &ArenaBotPolicyConfig,
) -> Option<BotAction> {
    let policy_mode = bot_policy_mode_from_config(config);
    let neural_weight = config.neural_weight.max(0);
    choose_active_turn_action_inner(context, policy_mode, neural_weight, config)
}

pub fn choose_claim_action_with_config(
    context: &BotContext,
    config: &ArenaBotPolicyConfig,
) -> Option<BotAction> {
    let policy_mode = bot_policy_mode_from_config(config);
    let neural_weight = config.neural_weight.max(0);
    choose_claim_action_inner(context, policy_mode, neural_weight, config)
}
```

Create private helpers `choose_active_turn_action_inner` and `choose_claim_action_inner`. Move the current bodies of `choose_active_turn_action` and `choose_claim_action` into those helpers, with these exact replacements:

```rust
let policy_mode = bot_policy_mode();
```

becomes a function argument:

```rust
policy_mode: BotPolicyMode
```

Every call to:

```rust
neural_prior_weight()
```

becomes:

```rust
neural_weight
```

Every call to:

```rust
neural_decision_scores(context)
```

becomes:

```rust
neural_decision_scores_for_policy(context, config)
```

The only behavioral change is that policy mode, neural weight, and model path come from `config` for the new entry points. The existing env-based public wrappers still derive the same values from environment variables.

- [ ] **Step 3: Add config mode adapter**

Keep the existing internal `BotPolicyMode` enum and add:

```rust
fn bot_policy_mode_from_config(config: &ArenaBotPolicyConfig) -> BotPolicyMode {
    match config.mode {
        ArenaPolicyMode::Heuristic => BotPolicyMode::Heuristic,
        ArenaPolicyMode::Hybrid => BotPolicyMode::Hybrid,
        ArenaPolicyMode::Neural => BotPolicyMode::Neural,
    }
}
```

- [ ] **Step 4: Add explicit neural scoring helper**

In `backend/src/bot/neural.rs`, add:

```rust
pub(crate) fn neural_decision_scores_for_model_path(
    context: &BotContext,
    model_path: Option<&std::path::Path>,
) -> Option<NeuralDecisionScores> {
    let features = encode_bot_context_v2(context);
    if let Some(path) = model_path {
        return OrtNeuralSession::new(path.to_path_buf()).run(features).ok();
    }
    shared_session().lock().ok()?.run(features).ok()
}
```

In `backend/src/bot/policy.rs`, add:

```rust
fn neural_decision_scores_for_policy(
    context: &BotContext,
    config: &ArenaBotPolicyConfig,
) -> Option<NeuralDecisionScores> {
    let path = config.model_path.as_deref().map(std::path::Path::new);
    super::neural::neural_decision_scores_for_model_path(context, path)
}
```

- [ ] **Step 5: Update moved bodies to use config**

Inside `choose_active_turn_action_with_config` and `choose_claim_action_with_config`:

- replace `neural_decision_scores(context)` with `neural_decision_scores_for_policy(context, config)`
- replace `neural_prior_weight()` with `neural_weight`
- keep search fallback behavior unchanged
- keep `choose_active_turn_action` and `choose_claim_action` public behavior unchanged

- [ ] **Step 6: Add explicit config tests**

Append to `backend/src/bot/policy.rs` tests:

```rust
#[test]
fn explicit_heuristic_config_uses_existing_search_path() {
    let mut context = base_context();
    let concealed_tiles = tiles(&[
        "w1", "w2", "w3", "t1", "t2", "t3", "b1", "b2", "b3", "east", "east", "green", "w9",
        "w6",
    ]);
    context.player.concealed_tile_counts =
        tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));
    context.player.concealed_tiles = concealed_tiles;
    let config = crate::bot::arena::ArenaBotPolicyConfig::heuristic();

    let action = choose_active_turn_action_with_config(&context, &config).expect("action");

    assert_eq!(action.seat_index, 0);
    assert_eq!(action.action_type, "discard");
    assert_eq!(action.tile_ids.len(), 1);
}
```

- [ ] **Step 7: Verify policy routing**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::policy bot::neural -- --nocapture
```

Expected: PASS.

## Task 3: Route Automation Through Policy Resolver

**Files:**
- Modify: `backend/src/rules/standard/automation.rs`
- Test: `backend/src/rules/standard/automation.rs`

- [ ] **Step 1: Add policy resolver type aliases**

At the top of `backend/src/rules/standard/automation.rs`, add:

```rust
use crate::bot::arena::ArenaBotPolicyConfig;
```

Add:

```rust
type BotPolicyResolver<'a> = &'a dyn Fn(usize) -> ArenaBotPolicyConfig;
```

- [ ] **Step 2: Add public resolver entry point**

Replace:

```rust
pub fn next_bot_action_in_room_state(room: &RoomState) -> Result<Option<BotAction>, String> {
    Ok(next_bot_action_for_state(room))
}
```

with:

```rust
pub fn next_bot_action_in_room_state(room: &RoomState) -> Result<Option<BotAction>, String> {
    Ok(next_bot_action_for_state_with_policy_resolver(
        room,
        &|_| crate::bot::arena::ArenaBotPolicyConfig::heuristic(),
    ))
}

pub fn next_bot_action_in_room_state_with_policy_resolver(
    room: &RoomState,
    policy_for_seat: BotPolicyResolver<'_>,
) -> Result<Option<BotAction>, String> {
    Ok(next_bot_action_for_state_with_policy_resolver(room, policy_for_seat))
}
```

If this changes production default behavior, use `bot_policy_config_from_env` exposed from `policy.rs` instead of `heuristic()`. The production wrapper must preserve current env behavior.

- [ ] **Step 3: Add config-aware active-turn helper**

Change `choose_bot_active_turn_action_with_cache_for_state` signature to:

```rust
fn choose_bot_active_turn_action_with_cache_for_state(
    room: &RoomState,
    cache: &RoomScoringCache,
    seat_index: usize,
    policy_config: &ArenaBotPolicyConfig,
) -> Option<BotAction> {
```

Replace its final line:

```rust
bot::choose_active_turn_action(&bot_context)
```

with:

```rust
bot::policy::choose_active_turn_action_with_config(&bot_context, policy_config)
```

If `policy` is private in `bot::mod.rs`, export only the two config functions:

```rust
pub use policy::{
    choose_active_turn_action,
    choose_active_turn_action_with_config,
    choose_claim_action,
    choose_claim_action_with_config,
};
```

- [ ] **Step 4: Add config-aware claim helper**

Change `choose_bot_claim_action_with_cache_for_state` signature to:

```rust
fn choose_bot_claim_action_with_cache_for_state(
    room: &RoomState,
    cache: &RoomScoringCache,
    seat_index: usize,
    policy_config: &ArenaBotPolicyConfig,
) -> Option<BotAction> {
```

Replace:

```rust
bot::choose_claim_action(&bot_context)
```

with:

```rust
bot::choose_claim_action_with_config(&bot_context, policy_config)
```

- [ ] **Step 5: Add config-aware main resolver**

Rename:

```rust
fn next_bot_action_for_state(state: &RoomState) -> Option<BotAction>
```

to:

```rust
fn next_bot_action_for_state_with_policy_resolver(
    state: &RoomState,
    policy_for_seat: BotPolicyResolver<'_>,
) -> Option<BotAction>
```

At each selected bot seat:

```rust
let policy_config = policy_for_seat(seat_index);
choose_bot_active_turn_action_with_cache_for_state(state, &cache, seat_index, &policy_config)
```

and:

```rust
let policy_config = policy_for_seat(seat_index);
choose_bot_claim_action_with_cache_for_state(state, &cache, seat_index, &policy_config)
```

- [ ] **Step 6: Keep test-only wrappers compiling**

If test-only helpers call the old signatures, pass:

```rust
&ArenaBotPolicyConfig::heuristic()
```

or keep a private compatibility wrapper:

```rust
fn next_bot_action_for_state(state: &RoomState) -> Option<BotAction> {
    next_bot_action_for_state_with_policy_resolver(state, &|_| ArenaBotPolicyConfig::heuristic())
}
```

- [ ] **Step 7: Verify automation behavior**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml rules::standard::automation -- --nocapture
```

Expected: PASS.

## Task 4: Add Arena Match Runner

**Files:**
- Modify: `backend/src/bot/arena.rs`
- Create: `backend/src/bin/bot_arena.rs`
- Modify: `backend/src/bot/search.rs`
- Test: `backend/src/bot/arena.rs`

- [ ] **Step 1: Add all-bot room constructor**

In `backend/src/bot/arena.rs`, add:

```rust
use crate::core::state::{RoomState, SeatState};

pub fn arena_room(table_code: &str) -> RoomState {
    RoomState {
        table_code: table_code.to_string(),
        phase: "waiting".to_string(),
        mode: "normal".to_string(),
        seats: (0..4)
            .map(|seat_index| SeatState {
                seat_index,
                nickname: Some(format!("Arena Bot {seat_index}")),
                reconnect_token: None,
                player_session_id: Some(-((seat_index as i64) + 1)),
                connected: true,
                ready: true,
                is_bot: true,
                seat_type: "bot".to_string(),
                bot_persona: None,
                bot_aggression: None,
                disconnect_deadline_at: None,
            })
            .collect(),
        match_state: None,
        round_state: None,
        pending_timeout: None,
        continue_action: None,
    }
}
```

- [ ] **Step 2: Add action application helper in arena binary**

Create `backend/src/bin/bot_arena.rs`:

```rust
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    time::Instant,
};

use anyhow::{Context, Result, bail};
use backend::{
    bot::arena::{ArenaBotPolicyConfig, ArenaConfig, ArenaMatchReport, arena_room},
    core::engine::try_handle_player_action_in_room_state,
    rules::standard::{
        automation::next_bot_action_in_room_state_with_policy_resolver,
        flow::start_match_in_room_state,
    },
};

struct Args {
    config_path: PathBuf,
    output_path: PathBuf,
}

fn main() -> Result<()> {
    let args = parse_args()?;
    let config: ArenaConfig = serde_json::from_str(
        &std::fs::read_to_string(&args.config_path)
            .with_context(|| format!("failed to read {}", args.config_path.display()))?,
    )?;
    let reports = run_arena(&config)?;
    let mut writer = BufWriter::new(File::create(&args.output_path)?);
    for report in reports {
        serde_json::to_writer(&mut writer, &report)?;
        writer.write_all(b"\n")?;
    }
    Ok(())
}
```

- [ ] **Step 3: Implement CLI arg parsing**

Append to `backend/src/bin/bot_arena.rs`:

```rust
fn parse_args() -> Result<Args> {
    let mut config_path = None;
    let mut output_path = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => config_path = args.next().map(PathBuf::from),
            "--output" => output_path = args.next().map(PathBuf::from),
            _ => bail!("unknown argument: {arg}"),
        }
    }
    Ok(Args {
        config_path: config_path.context("--config is required")?,
        output_path: output_path.context("--output is required")?,
    })
}
```

- [ ] **Step 4: Implement per-seat policy assignment**

Append:

```rust
fn policy_for_seat(config: &ArenaConfig, seat_index: usize) -> ArenaBotPolicyConfig {
    config
        .policies
        .get(seat_index % config.policies.len())
        .cloned()
        .unwrap_or_else(ArenaBotPolicyConfig::heuristic)
}
```

- [ ] **Step 5: Implement arena loop**

Append:

```rust
fn run_arena(config: &ArenaConfig) -> Result<Vec<ArenaMatchReport>> {
    if config.policies.is_empty() {
        bail!("arena config requires at least one policy");
    }

    let mut reports = Vec::new();
    for match_index in 0..config.matches {
        let seed = config.seed.wrapping_add(match_index as u64);
        let mut room = arena_room(&format!("ARENA{match_index:04}"));
        start_match_in_room_state(&mut room, 0, seed)?;
        let mut action_count = 0_usize;

        while room.phase == "playing" && action_count < config.max_actions_per_match {
            let started = Instant::now();
            let action = next_bot_action_in_room_state_with_policy_resolver(
                &room,
                &|seat| policy_for_seat(config, seat),
            )?;
            let Some(action) = action else {
                break;
            };
            let elapsed_ms = started.elapsed().as_millis();
            let output = try_handle_player_action_in_room_state(
                &mut room,
                action.seat_index,
                &action.action_type,
                &action.tile_ids,
            )?;
            if output.is_none() {
                bail!("arena action was rejected: {:?}", action);
            }
            action_count += 1;
            backend::bot::arena::record_action_latency(
                &mut room,
                action.seat_index,
                elapsed_ms,
            );
        }

        reports.push(backend::bot::arena::build_match_report(
            match_index,
            seed,
            &room,
            config,
            action_count,
            action_count < config.max_actions_per_match,
        ));
    }
    Ok(reports)
}
```

This step uses `ArenaMatchAccumulator` from the next step. Create the accumulator before the loop, record each accepted action, and pass the accumulator into `build_match_report`.

- [ ] **Step 6: Add metric accumulator helpers**

In `backend/src/bot/arena.rs`, add a local accumulator instead of mutating `RoomState`:

```rust
#[derive(Clone, Debug, Default)]
pub struct ArenaMatchAccumulator {
    pub seats: Vec<ArenaSeatMetrics>,
}

impl ArenaMatchAccumulator {
    pub fn new(config: &ArenaConfig) -> Self {
        Self {
            seats: (0..4)
                .map(|seat_index| ArenaSeatMetrics {
                    seat_index,
                    policy_id: config
                        .policies
                        .get(seat_index % config.policies.len())
                        .map(|policy| policy.id.clone())
                        .unwrap_or_else(|| "heuristic".to_string()),
                    ..ArenaSeatMetrics::default()
                })
                .collect(),
        }
    }

    pub fn record_decision(&mut self, seat_index: usize, action_type: &str, latency_ms: u128) {
        if let Some(metrics) = self.seats.get_mut(seat_index) {
            metrics.decision_count += 1;
            metrics.decision_latency_ms_sum += latency_ms;
            match action_type {
                "discard" => metrics.discard_count += 1,
                "chow" | "pung" | "kong" => metrics.claim_count += 1,
                _ => {}
            }
        }
    }
}
```

Then update `bot_arena.rs` to create `let mut accumulator = ArenaMatchAccumulator::new(config);`, call `accumulator.record_decision(...)`, and pass `accumulator` into `build_match_report`.

- [ ] **Step 7: Build match report from final state**

In `backend/src/bot/arena.rs`, add:

```rust
pub fn build_match_report(
    match_index: usize,
    seed: u64,
    room: &RoomState,
    mut accumulator: ArenaMatchAccumulator,
    action_count: usize,
    completed: bool,
) -> ArenaMatchReport {
    if let Some(match_state) = &room.match_state {
        for metrics in &mut accumulator.seats {
            metrics.score_delta = match_state
                .cumulative_scores
                .get(&metrics.seat_index)
                .copied()
                .unwrap_or_default();
            if let Some(stats) = match_state
                .statistics
                .seat_stats_by_seat
                .get(&metrics.seat_index)
            {
                metrics.wins = stats.win_count as u64;
                metrics.dealt_in = stats.deal_in_count as u64;
            }
        }
    }
    if let Some(round) = &room.round_state {
        for player in &round.players {
            if let Some(metrics) = accumulator.seats.get_mut(player.seat) {
                metrics.final_tenpai = player.is_ready_hand;
            }
        }
    }
    ArenaMatchReport {
        match_index,
        seed,
        completed,
        action_count,
        seats: accumulator.seats,
    }
}
```

- [ ] **Step 8: Expose tenpai helper for arena telemetry**

In `backend/src/bot/search.rs`, expose a helper around the existing shanten functions:

```rust
pub(crate) fn min_shanten_for_counts(
    concealed_counts: &TileCounts,
    open_meld_count: usize,
) -> i32 {
    standard_shanten_with_open_melds(concealed_counts, open_meld_count)
        .min(seven_pairs_shanten(concealed_counts, open_meld_count))
        .min(thirteen_orphans_shanten(concealed_counts, open_meld_count))
}
```

Use it in `ArenaMatchAccumulator::record_decision` to set `first_tenpai_turn` the first time a seat reaches shanten `0`. Compute the turn number from that seat's `discard_count + 1`.

- [ ] **Step 9: Add arena smoke config**

Create `backend/bot_trainer/v2/arena_smoke.json`:

```json
{
  "matches": 2,
  "seed": 20260429,
  "max_actions_per_match": 2400,
  "report_trajectories": false,
  "policies": [
    {"id": "heuristic", "mode": "heuristic", "neural_weight": 0, "model_path": null},
    {"id": "hybrid30", "mode": "hybrid", "neural_weight": 30, "model_path": "backend/assets/models/mahjong_policy_net.onnx"}
  ]
}
```

- [ ] **Step 10: Verify arena binary**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::arena -- --nocapture
cargo run --manifest-path backend/Cargo.toml --bin bot_arena -- --config backend/bot_trainer/v2/arena_smoke.json --output backend/bot_trainer/v2/arena_smoke.jsonl
Get-Content backend/bot_trainer/v2/arena_smoke.jsonl
```

Expected:

- tests pass
- command exits successfully
- `backend/bot_trainer/v2/arena_smoke.jsonl` has 2 JSON lines
- each line has `seats` length 4

## Task 5: Add Arena Matrix Scripts

**Files:**
- Create: `backend/bot_trainer/v2/arena_matrix.ps1`
- Create: `backend/bot_trainer/v2/arena_matrix.sh`
- Modify: `backend/bot_trainer/v2/README.md`

- [ ] **Step 1: Add Windows matrix script**

Create `backend/bot_trainer/v2/arena_matrix.ps1`:

```powershell
param(
    [int]$Matches = 200,
    [int]$Seed = 20260429,
    [string]$OutputDir = "backend/bot_trainer/v2/arena_runs"
)

$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$config = @{
    matches = $Matches
    seed = $Seed
    max_actions_per_match = 2400
    report_trajectories = $false
    policies = @(
        @{ id = "heuristic"; mode = "heuristic"; neural_weight = 0; model_path = $null },
        @{ id = "neural"; mode = "neural"; neural_weight = 0; model_path = "backend/assets/models/mahjong_policy_net.onnx" },
        @{ id = "hybrid05"; mode = "hybrid"; neural_weight = 5; model_path = "backend/assets/models/mahjong_policy_net.onnx" },
        @{ id = "hybrid15"; mode = "hybrid"; neural_weight = 15; model_path = "backend/assets/models/mahjong_policy_net.onnx" },
        @{ id = "hybrid30"; mode = "hybrid"; neural_weight = 30; model_path = "backend/assets/models/mahjong_policy_net.onnx" },
        @{ id = "hybrid60"; mode = "hybrid"; neural_weight = 60; model_path = "backend/assets/models/mahjong_policy_net.onnx" }
    )
}

$configPath = Join-Path $OutputDir "arena_config.json"
$outputPath = Join-Path $OutputDir "arena_results.jsonl"
$config | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 $configPath

cargo run --manifest-path backend/Cargo.toml --release --bin bot_arena -- --config $configPath --output $outputPath
```

- [ ] **Step 2: Add Linux matrix script**

Create `backend/bot_trainer/v2/arena_matrix.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

MATCHES="${MATCHES:-200}"
SEED="${SEED:-20260429}"
OUTPUT_DIR="${OUTPUT_DIR:-backend/bot_trainer/v2/arena_runs}"

mkdir -p "$OUTPUT_DIR"
CONFIG_PATH="$OUTPUT_DIR/arena_config.json"
OUTPUT_PATH="$OUTPUT_DIR/arena_results.jsonl"

cat > "$CONFIG_PATH" <<JSON
{
  "matches": $MATCHES,
  "seed": $SEED,
  "max_actions_per_match": 2400,
  "report_trajectories": false,
  "policies": [
    {"id":"heuristic","mode":"heuristic","neural_weight":0,"model_path":null},
    {"id":"neural","mode":"neural","neural_weight":0,"model_path":"backend/assets/models/mahjong_policy_net.onnx"},
    {"id":"hybrid05","mode":"hybrid","neural_weight":5,"model_path":"backend/assets/models/mahjong_policy_net.onnx"},
    {"id":"hybrid15","mode":"hybrid","neural_weight":15,"model_path":"backend/assets/models/mahjong_policy_net.onnx"},
    {"id":"hybrid30","mode":"hybrid","neural_weight":30,"model_path":"backend/assets/models/mahjong_policy_net.onnx"},
    {"id":"hybrid60","mode":"hybrid","neural_weight":60,"model_path":"backend/assets/models/mahjong_policy_net.onnx"}
  ]
}
JSON

cargo run --manifest-path backend/Cargo.toml --release --bin bot_arena -- --config "$CONFIG_PATH" --output "$OUTPUT_PATH"
```

- [ ] **Step 3: Document arena commands**

Append to `backend/bot_trainer/v2/README.md`:

```markdown
## Arena Evaluation

Smoke:

```powershell
cargo run --manifest-path backend/Cargo.toml --bin bot_arena -- --config backend/bot_trainer/v2/arena_smoke.json --output backend/bot_trainer/v2/arena_smoke.jsonl
```

Windows matrix:

```powershell
.\backend\bot_trainer\v2\arena_matrix.ps1 -Matches 200 -Seed 20260429
```

Linux matrix:

```bash
MATCHES=200 SEED=20260429 ./backend/bot_trainer/v2/arena_matrix.sh
```

Primary model-selection metrics:

- average score delta
- win rate
- deal-in rate
- first-tenpai turn
- final-tenpai rate
- average decision latency
```

- [ ] **Step 4: Verify scripts are present**

Run:

```powershell
Test-Path backend/bot_trainer/v2/arena_matrix.ps1
Test-Path backend/bot_trainer/v2/arena_matrix.sh
rg -n "Arena Evaluation|arena_matrix|bot_arena" backend/bot_trainer/v2/README.md
```

Expected: both `Test-Path` commands print `True`; `rg` finds the new README section.

## Task 6: Add Trajectory Export

**Files:**
- Modify: `backend/src/bot/arena.rs`
- Modify: `backend/src/bin/bot_arena.rs`
- Test: `backend/src/bot/arena.rs`

- [ ] **Step 1: Add trajectory struct**

In `backend/src/bot/arena.rs`, add:

```rust
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ArenaTrajectoryRow {
    pub schema_version: u32,
    pub match_id: String,
    pub decision_index: u64,
    pub seat_index: usize,
    pub policy_id: String,
    pub decision_kind: String,
    pub tile_planes: Vec<f32>,
    pub scalar_features: Vec<f32>,
    pub discard_mask: Vec<bool>,
    pub claim_mask: Vec<bool>,
    pub self_kong_mask: Vec<bool>,
    pub hu_mask: Vec<bool>,
    pub action_head: String,
    pub action_index: i64,
    pub action_semantic: String,
    pub log_prob: f32,
    pub value: f32,
    pub reward: f32,
    pub done: bool,
}
```

- [ ] **Step 2: Add action encoding helper**

In `backend/src/bot/arena.rs`, add:

```rust
pub fn action_semantic(action_type: &str, tile_key: Option<&str>) -> String {
    match tile_key {
        Some(tile_key) if action_type == "discard" => format!("discard:{tile_key}"),
        Some(tile_key) if action_type == "chow" => format!("claim:chow:{tile_key}"),
        Some(tile_key) if action_type == "pung" => format!("claim:pung:{tile_key}"),
        Some(tile_key) if action_type == "kong" => format!("claim:kong:{tile_key}"),
        _ => action_type.to_string(),
    }
}
```

- [ ] **Step 3: Add shape consistency test**

Append to `backend/src/bot/arena.rs` tests:

```rust
#[test]
fn trajectory_row_serializes_action_metadata() {
    let row = ArenaTrajectoryRow {
        schema_version: 1,
        match_id: "arena-1".to_string(),
        decision_index: 0,
        seat_index: 0,
        policy_id: "heuristic".to_string(),
        decision_kind: "active_turn".to_string(),
        tile_planes: vec![0.0; 340],
        scalar_features: vec![0.0; 10],
        discard_mask: vec![true; 34],
        claim_mask: vec![true; 7],
        self_kong_mask: vec![true; 3],
        hu_mask: vec![true, false],
        action_head: "discard".to_string(),
        action_index: 0,
        action_semantic: "discard:w1".to_string(),
        log_prob: 0.0,
        value: 0.0,
        reward: 0.0,
        done: false,
    };

    let value = serde_json::to_value(row).expect("row");

    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["action_head"], "discard");
    assert_eq!(value["discard_mask"].as_array().expect("mask").len(), 34);
}
```

- [ ] **Step 4: Add trajectory CLI output arg**

In `backend/src/bin/bot_arena.rs`, extend `Args`:

```rust
struct Args {
    config_path: PathBuf,
    output_path: PathBuf,
    trajectories_path: Option<PathBuf>,
}
```

Update parser:

```rust
"--trajectories" => trajectories_path = args.next().map(PathBuf::from),
```

Return it in `Args`.

- [ ] **Step 5: Write trajectory rows for selected decisions**

Expose the V2 feature encoder from `backend/src/bot/mod.rs` so arena code can serialize the same observation used by runtime policy:

```rust
pub(crate) use features::{BotFeaturesV2, encode_bot_context_v2};
```

Add a trace-capable automation entry point that returns the selected action and the `BotContext` used to select it:

```rust
pub struct BotDecisionTrace {
    pub seat_index: usize,
    pub decision_kind: String,
    pub context: crate::bot::BotContext,
    pub action: BotAction,
}
```

Export `BotContext` from `backend/src/bot/mod.rs`:

```rust
pub(crate) use context::BotContext;
```

Add:

```rust
pub fn next_bot_decision_trace_in_room_state_with_policy_resolver(
    room: &RoomState,
    policy_for_seat: BotPolicyResolver<'_>,
) -> Result<Option<BotDecisionTrace>, String>
```

This function should share the active-turn and claim-window context-building code with `next_bot_action_in_room_state_with_policy_resolver`, then return both the chosen action and the context. In `bot_arena.rs`, call this trace entry point when `--trajectories` is provided. Convert `trace.context` with `encode_bot_context_v2(&trace.context)` and write the full feature arrays and masks to `ArenaTrajectoryRow`.

- [ ] **Step 6: Add reward assignment after match settlement**

In `bot_arena.rs`, keep trajectory rows in memory per match. After the match completes:

```rust
fn assign_terminal_rewards(
    rows: &mut [backend::bot::arena::ArenaTrajectoryRow],
    report: &ArenaMatchReport,
) {
    for row in rows {
        if let Some(seat) = report.seats.iter().find(|seat| seat.seat_index == row.seat_index) {
            let score_reward = seat.score_delta as f32 / 100.0;
            let win_bonus = if seat.wins > 0 { 1.0 } else { 0.0 };
            let deal_in_penalty = if seat.dealt_in > 0 { -1.5 } else { 0.0 };
            row.reward = score_reward + win_bonus + deal_in_penalty;
        }
    }
    if let Some(last) = rows.last_mut() {
        last.done = true;
    }
}
```

This first version assigns terminal return to each decision. PPO converts these rewards to discounted returns in `rl_dataset.py`. Tenpai and early-win shaping are introduced only after the baseline terminal-reward training smoke succeeds.

- [ ] **Step 7: Verify trajectory export**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::arena -- --nocapture
cargo run --manifest-path backend/Cargo.toml --bin bot_arena -- --config backend/bot_trainer/v2/arena_smoke.json --output backend/bot_trainer/v2/arena_smoke.jsonl --trajectories backend/bot_trainer/v2/arena_trajectories_smoke.jsonl
Get-Content backend/bot_trainer/v2/arena_trajectories_smoke.jsonl -TotalCount 2
```

Expected:

- command exits successfully
- trajectory file exists
- each row has legal mask arrays with expected lengths
- at least one row has `done=true`

## Task 7: Add Python RL Dataset

**Files:**
- Create: `backend/bot_trainer/v2/rl_dataset.py`
- Create: `backend/bot_trainer/v2/test_rl_dataset.py`

- [ ] **Step 1: Add trajectory loading test**

Create `backend/bot_trainer/v2/test_rl_dataset.py`:

```python
from pathlib import Path

from rl_dataset import ArenaTrajectoryDataset, compute_returns


def test_loads_trajectory_row(tmp_path: Path) -> None:
    path = tmp_path / "trajectories.jsonl"
    path.write_text(
        '{"schema_version":1,"match_id":"m1","decision_index":0,"seat_index":0,'
        '"policy_id":"p","decision_kind":"active_turn","tile_planes":[0.0,0.0],'
        '"scalar_features":[0.0],"discard_mask":[true,false],"claim_mask":[true],'
        '"self_kong_mask":[true],"hu_mask":[true,false],"action_head":"discard",'
        '"action_index":0,"action_semantic":"discard:w1","log_prob":0.0,'
        '"value":0.0,"reward":1.0,"done":true}\\n',
        encoding="utf-8",
    )

    dataset = ArenaTrajectoryDataset(path)
    row = dataset[0]

    assert row["action_index"].item() == 0
    assert row["reward"].item() == 1.0


def test_compute_returns_resets_on_done() -> None:
    returns = compute_returns([0.0, 1.0, 0.0, 2.0], [False, True, False, True], gamma=0.9)

    assert returns == [0.9, 1.0, 1.8, 2.0]
```

- [ ] **Step 2: Implement dataset loader**

Create `backend/bot_trainer/v2/rl_dataset.py`:

```python
from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import torch
from torch.utils.data import Dataset


class ArenaTrajectoryDataset(Dataset):
    def __init__(self, path: Path) -> None:
        self.rows = [
            json.loads(line)
            for line in path.read_text(encoding="utf-8").splitlines()
            if line.strip()
        ]

    def __len__(self) -> int:
        return len(self.rows)

    def __getitem__(self, index: int) -> dict[str, torch.Tensor]:
        row = self.rows[index]
        return encode_row(row)


def encode_row(row: dict[str, Any]) -> dict[str, torch.Tensor]:
    return {
        "tile_planes": torch.tensor(row["tile_planes"], dtype=torch.float32).view(-1, 34),
        "scalar_features": torch.tensor(row["scalar_features"], dtype=torch.float32),
        "discard_mask": torch.tensor(row["discard_mask"], dtype=torch.bool),
        "claim_mask": torch.tensor(row["claim_mask"], dtype=torch.bool),
        "self_kong_mask": torch.tensor(row["self_kong_mask"], dtype=torch.bool),
        "hu_mask": torch.tensor(row["hu_mask"], dtype=torch.bool),
        "action_index": torch.tensor(row["action_index"], dtype=torch.long),
        "reward": torch.tensor(row["reward"], dtype=torch.float32),
        "done": torch.tensor(row["done"], dtype=torch.bool),
        "old_log_prob": torch.tensor(row["log_prob"], dtype=torch.float32),
        "old_value": torch.tensor(row["value"], dtype=torch.float32),
        "action_head": torch.tensor(action_head_index(row["action_head"]), dtype=torch.long),
    }


def action_head_index(action_head: str) -> int:
    mapping = {"discard": 0, "claim": 1, "self_kong": 2, "hu": 3}
    return mapping[action_head]


def compute_returns(rewards: list[float], dones: list[bool], gamma: float) -> list[float]:
    returns = [0.0 for _ in rewards]
    running = 0.0
    for index in range(len(rewards) - 1, -1, -1):
        if dones[index]:
            running = 0.0
        running = rewards[index] + gamma * running
        returns[index] = round(running, 6)
    return returns
```

- [ ] **Step 3: Verify Python dataset**

Run:

```powershell
uv run python -m pytest backend/bot_trainer/v2/test_rl_dataset.py -q
```

Expected: PASS.

## Task 8: Add PPO Training Skeleton

**Files:**
- Create: `backend/bot_trainer/v2/rl_train.py`
- Modify: `backend/bot_trainer/v2/test_rl_dataset.py`

- [ ] **Step 1: Add PPO loss tests**

Append to `backend/bot_trainer/v2/test_rl_dataset.py`:

```python
def test_masked_ppo_loss_is_finite() -> None:
    import torch
    from rl_train import masked_head_log_probs, ppo_policy_loss

    logits = torch.tensor([[2.0, 0.0, -5.0]])
    mask = torch.tensor([[True, True, False]])
    actions = torch.tensor([0])
    old_log_probs = torch.tensor([-0.2])
    advantages = torch.tensor([1.0])

    log_probs = masked_head_log_probs(logits, mask, actions)
    loss = ppo_policy_loss(log_probs, old_log_probs, advantages, clip_epsilon=0.2)

    assert torch.isfinite(loss)
```

- [ ] **Step 2: Implement PPO helpers**

Create `backend/bot_trainer/v2/rl_train.py`:

```python
from __future__ import annotations

import argparse
from pathlib import Path

import torch
import torch.nn.functional as F
from torch.utils.data import DataLoader

from model import ModelConfig, build_model
from rl_dataset import ArenaTrajectoryDataset


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--trajectories", type=Path, required=True)
    parser.add_argument("--checkpoint", type=Path, default=None)
    parser.add_argument("--epochs", type=int, default=1)
    parser.add_argument("--batch-size", type=int, default=256)
    parser.add_argument("--lr", type=float, default=1e-5)
    parser.add_argument("--gamma", type=float, default=0.99)
    parser.add_argument("--clip-epsilon", type=float, default=0.2)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--device", default="auto")
    return parser.parse_args()


def masked_head_log_probs(
    logits: torch.Tensor,
    mask: torch.Tensor,
    actions: torch.Tensor,
) -> torch.Tensor:
    masked = logits.masked_fill(~mask.bool(), -1.0e4)
    log_probs = F.log_softmax(masked, dim=1)
    return log_probs.gather(1, actions.long().unsqueeze(1)).squeeze(1)


def ppo_policy_loss(
    log_probs: torch.Tensor,
    old_log_probs: torch.Tensor,
    advantages: torch.Tensor,
    clip_epsilon: float,
) -> torch.Tensor:
    ratio = torch.exp(log_probs - old_log_probs)
    clipped = torch.clamp(ratio, 1.0 - clip_epsilon, 1.0 + clip_epsilon)
    return -torch.minimum(ratio * advantages, clipped * advantages).mean()
```

- [ ] **Step 3: Add model loading and one-epoch training**

Append to `rl_train.py`:

```python
def resolve_device(requested: str) -> torch.device:
    if requested == "auto":
        return torch.device("cuda" if torch.cuda.is_available() else "cpu")
    return torch.device(requested)


def load_checkpoint_if_present(model: torch.nn.Module, checkpoint: Path | None) -> None:
    if checkpoint is None:
        return
    payload = torch.load(checkpoint, map_location="cpu")
    state = payload.get("model_state", payload)
    model.load_state_dict(state, strict=False)


def main() -> None:
    args = parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    device = resolve_device(args.device)
    dataset = ArenaTrajectoryDataset(args.trajectories)
    loader = DataLoader(dataset, batch_size=args.batch_size, shuffle=True)
    model = build_model(ModelConfig(tile_plane_count=10, scalar_feature_count=10)).to(device)
    load_checkpoint_if_present(model, args.checkpoint)
    optimizer = torch.optim.AdamW(model.parameters(), lr=args.lr)

    for _epoch in range(args.epochs):
        for batch in loader:
            batch = {key: value.to(device) for key, value in batch.items()}
            outputs = model(batch["tile_planes"].float(), batch["scalar_features"].float())
            log_probs = select_action_log_probs(outputs, batch)
            advantages = batch["reward"].float() - batch["old_value"].float()
            policy_loss = ppo_policy_loss(
                log_probs,
                batch["old_log_prob"].float(),
                advantages,
                args.clip_epsilon,
            )
            values = outputs["value"].squeeze(1)
            value_loss = F.mse_loss(values, batch["reward"].float())
            loss = policy_loss + 0.5 * value_loss
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            optimizer.step()

    torch.save(
        {
            "model_state": model.state_dict(),
            "model_config": {"tile_plane_count": 10, "scalar_feature_count": 10},
        },
        args.output / "best.pt",
    )


def select_action_log_probs(
    outputs: dict[str, torch.Tensor],
    batch: dict[str, torch.Tensor],
) -> torch.Tensor:
    result = torch.zeros_like(batch["reward"].float())
    heads = [
        (0, "discard_logits", "discard_mask"),
        (1, "claim_logits", "claim_mask"),
        (2, "self_kong_logits", "self_kong_mask"),
        (3, "hu_logits", "hu_mask"),
    ]
    for head_index, logits_key, mask_key in heads:
        active = batch["action_head"] == head_index
        if torch.any(active):
            result[active] = masked_head_log_probs(
                outputs[logits_key][active],
                batch[mask_key][active],
                batch["action_index"][active],
            )
    return result


if __name__ == "__main__":
    main()
```

- [ ] **Step 4: Verify PPO smoke**

Run:

```powershell
uv run python -m pytest backend/bot_trainer/v2/test_rl_dataset.py -q
uv run python backend/bot_trainer/v2/rl_train.py --trajectories backend/bot_trainer/v2/arena_trajectories_smoke.jsonl --checkpoint backend/bot_trainer/v2/checkpoints/best.pt --epochs 1 --batch-size 64 --output backend/bot_trainer/v2/checkpoints_rl_smoke --device cpu
```

Expected:

- tests pass
- `backend/bot_trainer/v2/checkpoints_rl_smoke/best.pt` exists

## Task 9: Export RL Candidate And Evaluate League

**Files:**
- Modify: `backend/bot_trainer/v2/README.md`
- Generated: `backend/bot_trainer/v2/checkpoints_rl_smoke/best.pt`
- Generated: `backend/bot_trainer/v2/arena_runs/arena_results.jsonl`

- [ ] **Step 1: Export RL checkpoint to ONNX**

Run:

```powershell
uv run python backend/bot_trainer/v2/export_onnx.py --checkpoint backend/bot_trainer/v2/checkpoints_rl_smoke/best.pt --output backend/bot_trainer/v2/checkpoints_rl_smoke/candidate.onnx
```

Expected:

- command exits successfully
- ONNX contains named outputs consumed by `backend/src/bot/neural.rs`

- [ ] **Step 2: Create candidate arena config**

Create `backend/bot_trainer/v2/arena_runs/rl_candidate_config.json`:

```json
{
  "matches": 200,
  "seed": 20260429,
  "max_actions_per_match": 2400,
  "report_trajectories": false,
  "policies": [
    {"id": "baseline_hybrid30", "mode": "hybrid", "neural_weight": 30, "model_path": "backend/assets/models/mahjong_policy_net.onnx"},
    {"id": "rl_candidate_hybrid30", "mode": "hybrid", "neural_weight": 30, "model_path": "backend/bot_trainer/v2/checkpoints_rl_smoke/candidate.onnx"}
  ]
}
```

- [ ] **Step 3: Run candidate league**

Run:

```powershell
cargo run --manifest-path backend/Cargo.toml --release --bin bot_arena -- --config backend/bot_trainer/v2/arena_runs/rl_candidate_config.json --output backend/bot_trainer/v2/arena_runs/rl_candidate_results.jsonl
```

Expected:

- arena writes `rl_candidate_results.jsonl`
- baseline and candidate policy ids both appear

- [ ] **Step 4: Record selection criteria in README**

Append to `backend/bot_trainer/v2/README.md`:

```markdown
## RL Candidate Acceptance

An RL candidate can replace the production model only when arena evaluation shows:

- average score delta improves over the current production baseline
- win rate does not regress
- deal-in rate does not increase by more than 2 percentage points
- first-tenpai turn or final-tenpai rate improves, or stays neutral
- average decision latency remains under 100 ms

The first RL runs should keep production policy in `hybrid` mode unless pure neural wins the same arena matrix without higher deal-in rate.
```

- [ ] **Step 5: Verify documentation**

Run:

```powershell
rg -n "RL Candidate Acceptance|average score delta|deal-in rate" backend/bot_trainer/v2/README.md
```

Expected: `rg` finds the new acceptance section.

## Task 10: Final Verification

**Files:**
- All files touched by previous tasks

- [ ] **Step 1: Run targeted Rust tests**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::arena bot::policy bot::neural rules::standard::automation -- --nocapture
```

Expected: PASS.

- [ ] **Step 2: Run targeted Python tests**

Run:

```powershell
uv run python -m pytest backend/bot_trainer/v2/test_rl_dataset.py backend/bot_trainer/v2/test_dataset.py -q
```

Expected: PASS.

- [ ] **Step 3: Run arena smoke**

Run:

```powershell
cargo run --manifest-path backend/Cargo.toml --bin bot_arena -- --config backend/bot_trainer/v2/arena_smoke.json --output backend/bot_trainer/v2/arena_smoke.jsonl --trajectories backend/bot_trainer/v2/arena_trajectories_smoke.jsonl
```

Expected:

- command exits successfully
- match report JSONL has 2 lines
- trajectory JSONL is non-empty

- [ ] **Step 4: Run PPO smoke**

Run:

```powershell
uv run python backend/bot_trainer/v2/rl_train.py --trajectories backend/bot_trainer/v2/arena_trajectories_smoke.jsonl --checkpoint backend/bot_trainer/v2/checkpoints/best.pt --epochs 1 --batch-size 64 --output backend/bot_trainer/v2/checkpoints_rl_smoke --device cpu
```

Expected:

- command exits successfully
- `backend/bot_trainer/v2/checkpoints_rl_smoke/best.pt` exists

- [ ] **Step 5: Check repository status**

Run:

```powershell
git status --short
```

Expected: changes are limited to arena, policy config, trajectory/PPO trainer, scripts, docs, and generated smoke artifacts.

## Completion Criteria

- Arena can run deterministic all-bot matches from JSON config.
- Arena writes match reports with four seat metric entries per match.
- Arena can compare heuristic, neural, and hybrid policies without changing environment variables.
- Production env-based bot behavior remains available.
- Trajectory export writes legal masked decisions with stable tensor shapes.
- Python PPO smoke training can load trajectory JSONL and save a checkpoint.
- Candidate selection is based on arena score delta, win rate, deal-in rate, tenpai metrics, and latency.
