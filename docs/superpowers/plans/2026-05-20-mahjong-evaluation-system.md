# Mahjong Evaluation System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a shared evaluation system used by backend arena and frontend player skill tests.

**Architecture:** Introduce a shared Rust evaluation module for replicated subject schedules, fixed evaluation rules, and result summaries. Migrate arena from policy seat rotation to subject replicas against three configured BOT opponents, then add app-level evaluation sessions and a compact frontend entry/result flow.

**Tech Stack:** Rust 2024 backend (`serde`, existing `RoomState`, `bot::arena`, standard flow, app scheduler/API), React/Vite frontend (`App.tsx`, `socialApi.ts`, existing BattleScreen patterns), JSON/JSONL arena configs and reports.

---

## Scope

This plan implements `docs/superpowers/specs/2026-05-20-mahjong-evaluation-system-design.md`.

The implementation should proceed in four phases. Stop after each phase if targeted tests reveal contract drift.

## File Map

- Create: `backend/src/evaluation.rs`
  Responsibility: shared evaluation data contracts, default SFT opponents, fixed rule helpers, replica schedule, subject result aggregation.
- Modify: `backend/src/lib.rs`
  Responsibility: export `evaluation` for backend binary and tests.
- Modify: `backend/src/bot/arena.rs`
  Responsibility: remove arena hard seat rotation config, add subject-replica arena runner, keep existing metrics and trajectory rows.
- Modify: `backend/src/bin/bot_arena.rs`
  Responsibility: call the migrated arena runner and continue writing reports/trajectories.
- Modify: `backend/bot_trainer/v2/arena_smoke.json`
  Responsibility: update arena smoke config to subjects + three opponents.
- Modify: `backend/bot_trainer/v2/arena_policy_pool.json`
  Responsibility: remove seat rotation assumptions if present.
- Modify: `backend/bot_trainer/v2/arena_matrix.ps1`
  Responsibility: generate new subject-replica configs.
- Modify: `backend/bot_trainer/v2/arena_matrix.sh`
  Responsibility: generate new subject-replica configs.
- Modify: `backend/bot_trainer/v2/arena_summary.py`
  Responsibility: read subject-centered summaries while preserving existing seat metric aggregation.
- Create: `backend/src/app/evaluation.rs`
  Responsibility: API request/response structs, evaluation session creation helpers, replica room creation, result response projection.
- Modify: `backend/src/app/mod.rs`
  Responsibility: register app evaluation module, store live evaluation sessions, and add room-aware BOT delay resolver.
- Modify: `backend/src/app/server.rs`
  Responsibility: add `POST /api/evaluations` and `GET /api/evaluations/:evaluation_id`.
- Modify: `backend/src/app/scheduler.rs`
  Responsibility: use `bot_action_delay_ms(&room)` instead of fixed `BOT_ACTION_DELAY_MS`.
- Modify: `backend/src/app/ws.rs`
  Responsibility: prevent manual rule toggles in evaluation rooms and start evaluation rooms with deterministic seeds.
- Modify: `backend/src/app/protocol.rs`
  Responsibility: include `mode = "evaluation"` in normal snapshot/table responses through existing room serialization.
- Modify: `frontend/src/types/match.ts`
  Responsibility: add `TableMode = 'normal' | 'evaluation'` and evaluation API response types.
- Modify: `frontend/src/lib/socialApi.ts`
  Responsibility: add `createEvaluation` and `getEvaluation`.
- Modify: `frontend/src/App.tsx`
  Responsibility: add evaluation entry, subject selection state, evaluation API calls, and result/progress rendering.
- Create: `frontend/src/components/battle-screen/EvaluationDialog.tsx`
  Responsibility: compact subject picker for current user plus up to three human/AI subjects.
- Create: `frontend/src/components/battle-screen/EvaluationPanel.tsx`
  Responsibility: show evaluation progress and final per-subject score/deal-in results.
- Modify: `frontend/src/components/battle-screen/BattleScreen.tsx`
  Responsibility: expose evaluation entry/panel props without replacing existing table controls.
- Modify: `frontend/src/components/battle-screen/scene/TableChrome.tsx`
  Responsibility: add evaluation button placement consistent with existing compact table controls.
- Modify: `frontend/src/App.test.tsx`
  Responsibility: cover evaluation API call and result rendering.
- Modify: `frontend/src/lib/matchViewModel.ts`
  Responsibility: ensure `roomMode` accepts evaluation and hides normal rule toggles for evaluation rooms.
- Modify: `frontend/src/lib/matchViewModel.test.ts`
  Responsibility: cover evaluation mode waiting controls.

## Data Contracts

### Arena Evaluation Config

```json
{
  "matches": 2,
  "seed": 20260520,
  "max_actions_per_match": 2400,
  "report_trajectories": false,
  "subjects": [
    {
      "id": "candidate",
      "display_name": "Candidate",
      "model_path": "backend/assets/sft/sft.onnx",
      "sample_actions": false,
      "temperature": 1.0
    }
  ],
  "opponents": [
    { "id": "sft-a", "model_path": "backend/assets/sft/sft.onnx" },
    { "id": "sft-b", "model_path": "backend/assets/sft/sft.onnx" },
    { "id": "sft-c", "model_path": "backend/assets/sft/sft.onnx" }
  ]
}
```

### Arena Report Row

Keep the existing row fields and add subject metadata:

```json
{
  "match_index": 0,
  "seed": 20260520,
  "completed": true,
  "action_count": 312,
  "subject_id": "candidate",
  "subject_display_name": "Candidate",
  "subject_initial_seat": 0,
  "subject_final_score": 12,
  "subject_deal_in_count": 1,
  "subject_win_count": 2,
  "seats": []
}
```

### Frontend Create Evaluation Request

```json
{
  "subject_user_ids": [123, 456]
}
```

The authenticated user is always included by the backend even if omitted.

### Frontend Evaluation Response

```json
{
  "evaluation_id": "eval-20260520-ab12",
  "seed": 20260520,
  "subjects": [
    {
      "subject_id": "user:123",
      "user_id": 123,
      "display_name": "Alice",
      "kind": "human",
      "table_code": "EVAB12A",
      "phase": "playing",
      "completed": false,
      "final_score": null,
      "deal_in_count": null,
      "win_count": null
    }
  ]
}
```

## Task 1: Add Shared Evaluation Core

**Files:**
- Create: `backend/src/evaluation.rs`
- Modify: `backend/src/lib.rs`

- [ ] **Step 1: Add failing tests for evaluation rules and schedules**

Create `backend/src/evaluation.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::state::RoomState;

    #[test]
    fn evaluation_room_rules_are_fixed() {
        let mut room = RoomState::default();
        room.minimum_hu_fan = 0;
        room.dealer_repeat_enabled = true;
        room.dealer_double_enabled = true;

        apply_evaluation_rules(&mut room);

        assert_eq!(room.mode, EVALUATION_ROOM_MODE);
        assert_eq!(room.minimum_hu_fan, 8);
        assert!(!room.dealer_repeat_enabled);
        assert!(!room.dealer_double_enabled);
    }

    #[test]
    fn evaluation_requires_exactly_three_opponents() {
        let subject = test_policy("candidate");
        let opponents = vec![test_policy("a"), test_policy("b")];

        let result = EvaluationArenaConfig::new_for_test(1, 7, vec![subject], opponents);

        assert!(result.is_err());
    }

    #[test]
    fn replicated_match_seeds_are_stable_per_subject() {
        let seeds = evaluation_match_seeds(100, 3);

        assert_eq!(seeds, vec![100, 101, 102]);
    }

    fn test_policy(id: &str) -> crate::bot::arena::ArenaBotPolicyConfig {
        crate::bot::arena::ArenaBotPolicyConfig {
            id: id.to_string(),
            model_path: Some(crate::special_bots::SFT_MODEL_PATH.to_string()),
            sample_actions: false,
            temperature: 1.0,
            discard_base_risk_weight: 0.90,
            discard_value_risk_range: 0.55,
            discard_min_risk_weight: 0.25,
            discard_max_risk_weight: 1.45,
        }
    }
}
```

- [ ] **Step 2: Run tests and confirm they fail**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml evaluation -- --nocapture
```

Expected: FAIL because `evaluation` module and symbols do not exist.

- [ ] **Step 3: Implement evaluation structs and helpers**

Replace `backend/src/evaluation.rs` with:

```rust
use serde::{Deserialize, Serialize};

use crate::bot::arena::ArenaBotPolicyConfig;
use crate::core::state::RoomState;

pub const EVALUATION_ROOM_MODE: &str = "evaluation";
pub const EVALUATION_HAND_COUNT: usize = 16;
pub const EVALUATION_INITIAL_SUBJECT_SEAT: usize = 0;
pub const EVALUATION_MINIMUM_HU_FAN: i64 = 8;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EvaluationSubjectPolicyConfig {
    pub id: String,
    pub display_name: String,
    #[serde(flatten)]
    pub policy: ArenaBotPolicyConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EvaluationArenaConfig {
    pub matches: usize,
    pub seed: u64,
    #[serde(default = "default_max_actions_per_match")]
    pub max_actions_per_match: usize,
    #[serde(default)]
    pub report_trajectories: bool,
    pub subjects: Vec<EvaluationSubjectPolicyConfig>,
    pub opponents: Vec<ArenaBotPolicyConfig>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct EvaluationSubjectReport {
    pub subject_id: String,
    pub display_name: String,
    pub kind: String,
    pub completed: bool,
    pub final_score: i64,
    pub deal_in_count: u64,
    pub win_count: u64,
}

fn default_max_actions_per_match() -> usize {
    2400
}

impl EvaluationArenaConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.subjects.is_empty() {
            return Err("evaluation requires at least one subject".to_string());
        }
        if self.opponents.len() != 3 {
            return Err("evaluation requires exactly three opponents".to_string());
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn new_for_test(
        matches: usize,
        seed: u64,
        subjects: Vec<ArenaBotPolicyConfig>,
        opponents: Vec<ArenaBotPolicyConfig>,
    ) -> Result<Self, String> {
        let config = Self {
            matches,
            seed,
            max_actions_per_match: default_max_actions_per_match(),
            report_trajectories: false,
            subjects: subjects
                .into_iter()
                .map(|policy| EvaluationSubjectPolicyConfig {
                    display_name: policy.id.clone(),
                    id: policy.id.clone(),
                    policy,
                })
                .collect(),
            opponents,
        };
        config.validate()?;
        Ok(config)
    }
}

pub fn apply_evaluation_rules(room: &mut RoomState) {
    room.mode = EVALUATION_ROOM_MODE.to_string();
    room.minimum_hu_fan = EVALUATION_MINIMUM_HU_FAN;
    room.dealer_repeat_enabled = false;
    room.dealer_double_enabled = false;
}

pub fn evaluation_match_seeds(seed: u64, matches: usize) -> Vec<u64> {
    (0..matches)
        .map(|match_index| seed.wrapping_add(match_index as u64))
        .collect()
}

pub fn default_sft_opponents() -> Vec<ArenaBotPolicyConfig> {
    (0..3)
        .map(|index| ArenaBotPolicyConfig {
            id: format!("sft-opponent-{}", index + 1),
            model_path: Some(crate::special_bots::SFT_MODEL_PATH.to_string()),
            sample_actions: false,
            temperature: 1.0,
            discard_base_risk_weight: 0.90,
            discard_value_risk_range: 0.55,
            discard_min_risk_weight: 0.25,
            discard_max_risk_weight: 1.45,
        })
        .collect()
}
```

- [ ] **Step 4: Export module**

Modify `backend/src/lib.rs` by adding one line among the existing module exports:

```rust
pub mod evaluation;
```

Preserve existing modules and only add `pub mod evaluation;`.

- [ ] **Step 5: Verify shared core**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml evaluation -- --nocapture
```

Expected: PASS.

## Task 2: Migrate Arena Config To Subject Replicas

**Files:**
- Modify: `backend/src/bot/arena.rs`
- Modify: `backend/src/bin/bot_arena.rs`
- Modify: `backend/bot_trainer/v2/arena_smoke.json`

- [ ] **Step 1: Add failing arena config tests**

Append to `backend/src/bot/arena.rs` tests:

```rust
#[test]
fn arena_config_parses_subject_replica_shape() {
    let raw = r#"{
        "matches": 1,
        "seed": 20260520,
        "subjects": [
            {"id":"candidate","display_name":"Candidate","model_path":"backend/assets/sft/sft.onnx"}
        ],
        "opponents": [
            {"id":"sft-a","model_path":"backend/assets/sft/sft.onnx"},
            {"id":"sft-b","model_path":"backend/assets/sft/sft.onnx"},
            {"id":"sft-c","model_path":"backend/assets/sft/sft.onnx"}
        ]
    }"#;

    let config: crate::evaluation::EvaluationArenaConfig =
        serde_json::from_str(raw).expect("evaluation arena config");

    assert_eq!(config.subjects.len(), 1);
    assert_eq!(config.opponents.len(), 3);
}

#[test]
fn arena_config_rejects_hard_seat_rotation_fields() {
    let raw = r#"{
        "matches": 1,
        "seed": 20260520,
        "seat_rotation": "cyclic",
        "subjects": [
            {"id":"candidate","display_name":"Candidate","model_path":"backend/assets/sft/sft.onnx"}
        ],
        "opponents": [
            {"id":"sft-a","model_path":"backend/assets/sft/sft.onnx"},
            {"id":"sft-b","model_path":"backend/assets/sft/sft.onnx"},
            {"id":"sft-c","model_path":"backend/assets/sft/sft.onnx"}
        ]
    }"#;

    let parsed = serde_json::from_str::<crate::evaluation::EvaluationArenaConfig>(raw);

    assert!(parsed.is_err());
}
```

- [ ] **Step 2: Run tests and confirm current arena path is incomplete**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::arena::tests::arena_config_parses_subject_replica_shape bot::arena::tests::arena_config_rejects_hard_seat_rotation_fields -- --nocapture
```

Expected: PASS after Task 1, but existing binary still does not use the new config.

- [ ] **Step 3: Add subject fields to `ArenaMatchReport`**

Modify `ArenaMatchReport` in `backend/src/bot/arena.rs`:

```rust
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ArenaMatchReport {
    pub match_index: usize,
    pub seed: u64,
    pub completed: bool,
    pub action_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_initial_seat: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_final_score: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_deal_in_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_win_count: Option<u64>,
    pub seats: Vec<ArenaSeatMetrics>,
}
```

Update all tests constructing `ArenaMatchReport` to set the new fields to `None`.

- [ ] **Step 4: Add subject policy assignment helper**

Add to `backend/src/bot/arena.rs`:

```rust
fn evaluation_policy_for_seat(
    subject: &crate::evaluation::EvaluationSubjectPolicyConfig,
    opponents: &[ArenaBotPolicyConfig],
    seat_index: usize,
) -> ArenaBotPolicyConfig {
    if seat_index == crate::evaluation::EVALUATION_INITIAL_SUBJECT_SEAT {
        return subject.policy.clone();
    }
    opponents
        .get(seat_index - 1)
        .cloned()
        .expect("evaluation has exactly three opponents")
}
```

- [ ] **Step 5: Add evaluation arena runner wrapper**

Add to `backend/src/bot/arena.rs`:

```rust
pub fn run_evaluation_arena(
    config: &crate::evaluation::EvaluationArenaConfig,
    include_trajectories: bool,
) -> Result<ArenaRunOutput, String> {
    config.validate()?;
    let mut output = ArenaRunOutput::default();
    for subject in &config.subjects {
        for match_index in 0..config.matches {
            let seed = config.seed.wrapping_add(match_index as u64);
            let completed = run_evaluation_arena_match(
                config,
                subject,
                match_index,
                seed,
                include_trajectories,
            )?;
            output.trajectories.extend(completed.trajectories);
            output.reports.push(completed.report);
        }
    }
    Ok(output)
}
```

Implement `run_evaluation_arena_match` by reusing the body of `run_arena_match`, with these changes:

- Build `ArenaMatchAccumulator` from explicit four policies: subject at seat 0, opponents at seats 1-3.
- Call `crate::evaluation::apply_evaluation_rules(&mut room)` before `start_match_in_room_state`.
- Use `start_match_in_room_state(&mut room, 0, seed)`.
- Resolve policies through `evaluation_policy_for_seat(subject, &config.opponents, seat)`.
- Fill subject fields in the report from seat 0 metrics.

- [ ] **Step 6: Update `bot_arena` binary to parse evaluation config**

Modify `backend/src/bin/bot_arena.rs`:

```rust
use backend::{
    bot::arena::run_evaluation_arena,
    evaluation::EvaluationArenaConfig,
};
```

Replace config parsing:

```rust
let config: EvaluationArenaConfig = serde_json::from_str(&std::fs::read_to_string(&args.config_path)?)?;
let include_trajectories = args.trajectories_path.is_some();
let arena_output = run_evaluation_arena(&config, include_trajectories)
    .map_err(|reason| anyhow::anyhow!(reason))?;
```

If retaining `--jobs` support in this task becomes large, keep it serial and document that parallel support is reintroduced after migration. Do not leave dead branches compiling against the old config.

- [ ] **Step 7: Update smoke config**

Replace `backend/bot_trainer/v2/arena_smoke.json` with:

```json
{
  "matches": 1,
  "seed": 20260520,
  "max_actions_per_match": 2400,
  "report_trajectories": false,
  "subjects": [
    {
      "id": "sft-candidate",
      "display_name": "SFT Candidate",
      "model_path": "backend/assets/sft/sft.onnx"
    }
  ],
  "opponents": [
    { "id": "sft-opponent-1", "model_path": "backend/assets/sft/sft.onnx" },
    { "id": "sft-opponent-2", "model_path": "backend/assets/sft/sft.onnx" },
    { "id": "sft-opponent-3", "model_path": "backend/assets/sft/sft.onnx" }
  ]
}
```

- [ ] **Step 8: Verify arena migration**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::arena evaluation -- --nocapture
cargo run --manifest-path backend/Cargo.toml --bin bot_arena -- --config backend/bot_trainer/v2/arena_smoke.json --output backend/bot_trainer/v2/arena_smoke.jsonl
Get-Content backend/bot_trainer/v2/arena_smoke.jsonl -TotalCount 1
```

Expected:

- Tests pass.
- Arena writes one JSONL row.
- Row includes `subject_id`, `subject_final_score`, and seat metrics.

## Task 3: Add Room-Aware BOT Delay

**Files:**
- Modify: `backend/src/app/mod.rs`
- Modify: `backend/src/app/scheduler.rs`

- [ ] **Step 1: Add failing delay resolver tests**

Append to `backend/src/app/mod.rs` tests:

```rust
#[test]
fn evaluation_room_with_only_bots_has_zero_bot_delay() {
    let mut room = RoomState {
        mode: crate::evaluation::EVALUATION_ROOM_MODE.to_string(),
        seats: vec![SeatState {
            seat_index: 0,
            connected: true,
            is_bot: true,
            seat_type: "bot".to_string(),
            ..Default::default()
        }],
        ..RoomState::default()
    };

    assert_eq!(bot_action_delay_ms(&room), 0);

    room.seats[0].seat_type = "human".to_string();
    room.seats[0].is_bot = false;
    assert_eq!(bot_action_delay_ms(&room), BOT_ACTION_DELAY_MS);
}
```

- [ ] **Step 2: Run failing test**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml app::tests::evaluation_room_with_only_bots_has_zero_bot_delay -- --nocapture
```

Expected: FAIL because `bot_action_delay_ms` is missing.

- [ ] **Step 3: Implement resolver**

Add to `backend/src/app/mod.rs`:

```rust
pub(crate) fn bot_action_delay_ms(room: &RoomState) -> u64 {
    if room.mode == crate::evaluation::EVALUATION_ROOM_MODE
        && room.seats.iter().all(|seat| seat.is_bot || seat.seat_type != "human")
    {
        return 0;
    }
    BOT_ACTION_DELAY_MS
}
```

- [ ] **Step 4: Use resolver in scheduler**

Modify `backend/src/app/scheduler.rs` imports by adding `bot_action_delay_ms` to the existing `crate::app` import list:

```rust
use crate::app::{
    AppContext, bot_action_delay_ms, broadcast_to_handles,
    collect_snapshot_and_prompt_outbound_from_snapshot, continue_action_deadline,
    notify_all_user_connections, pending_timeout_deadline, record_timeout_auto_responses,
    records::{apply_point_updates_to_room, archive_current_round_if_needed},
    room_has_round_state, room_seats, send_outbound, serialize_room, sleep_until,
    timeout_auto_response_seats, user_active_table_updated_message,
};
```

Replace:

```rust
tokio::time::sleep(Duration::from_millis(BOT_ACTION_DELAY_MS)).await;
```

with:

```rust
tokio::time::sleep(Duration::from_millis(delay_ms)).await;
```

Capture before spawn:

```rust
let delay_ms = bot_action_delay_ms(&runtime.room);
```

- [ ] **Step 5: Verify delay tests**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml app::tests::evaluation_room_with_only_bots_has_zero_bot_delay app::scheduler -- --nocapture
```

Expected: PASS.

## Task 4: Add Backend Evaluation Session API

**Files:**
- Create: `backend/src/app/evaluation.rs`
- Modify: `backend/src/app/mod.rs`
- Modify: `backend/src/app/server.rs`

- [ ] **Step 1: Add API shape tests**

Append to `backend/src/app/server_table_tests.rs` or create tests in `backend/src/app/evaluation.rs`:

```rust
#[test]
fn create_evaluation_response_serializes_subject_results() {
    let response = crate::app::evaluation::EvaluationSessionResponse {
        evaluation_id: "eval-test".to_string(),
        seed: 7,
        subjects: vec![crate::app::evaluation::EvaluationSubjectResponse {
            subject_id: "user:1".to_string(),
            user_id: Some(1),
            display_name: "Alice".to_string(),
            kind: "human".to_string(),
            table_code: "EVAL1".to_string(),
            phase: "waiting".to_string(),
            completed: false,
            final_score: None,
            deal_in_count: None,
            win_count: None,
        }],
    };

    let value = serde_json::to_value(response).expect("response");

    assert_eq!(value["evaluation_id"], "eval-test");
    assert_eq!(value["subjects"][0]["deal_in_count"], serde_json::Value::Null);
}
```

- [ ] **Step 2: Run failing test**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml app::evaluation -- --nocapture
```

Expected: FAIL because module does not exist.

- [ ] **Step 3: Implement response/request structs**

Create `backend/src/app/evaluation.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub(crate) struct CreateEvaluationRequest {
    #[serde(default)]
    pub(crate) subject_user_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct EvaluationSubjectResponse {
    pub(crate) subject_id: String,
    pub(crate) user_id: Option<i64>,
    pub(crate) display_name: String,
    pub(crate) kind: String,
    pub(crate) table_code: String,
    pub(crate) phase: String,
    pub(crate) completed: bool,
    pub(crate) final_score: Option<i64>,
    pub(crate) deal_in_count: Option<u64>,
    pub(crate) win_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct EvaluationSessionResponse {
    pub(crate) evaluation_id: String,
    pub(crate) seed: u64,
    pub(crate) subjects: Vec<EvaluationSubjectResponse>,
}
```

- [ ] **Step 4: Register module**

Modify `backend/src/app/mod.rs`:

```rust
pub(crate) mod evaluation;
```

Add a live session store to `AppState`:

```rust
pub(crate) struct AppState {
    pub(crate) db: DbWorker,
    pub(crate) rooms: RwLock<HashMap<String, Arc<RoomHandle>>>,
    pub(crate) user_connections: RwLock<HashMap<i64, HashMap<u64, ConnectionHandle>>>,
    pub(crate) special_bot_user_ids: RwLock<HashSet<i64>>,
    pub(crate) evaluation_sessions:
        RwLock<HashMap<String, self::evaluation::EvaluationSessionResponse>>,
}
```

Initialize it in `AppContext::new`:

```rust
evaluation_sessions: RwLock::new(HashMap::new()),
```

- [ ] **Step 5: Add replica room builder**

Add to `backend/src/app/evaluation.rs`:

```rust
use crate::app::{generate_short_hex, initial_room_state_with_owner};
use crate::core::state::{RoomState, SeatState};

pub(crate) fn evaluation_table_code(prefix: &str, index: usize) -> String {
    format!("{}{:02}", prefix, index)
}

pub(crate) fn build_evaluation_room(
    table_code: &str,
    owner_user_id: i64,
    subject_user_id: Option<i64>,
    subject_name: &str,
    subject_is_bot: bool,
) -> RoomState {
    let mut room = initial_room_state_with_owner(table_code, Some(owner_user_id), 1);
    crate::evaluation::apply_evaluation_rules(&mut room);
    room.seats.push(SeatState {
        seat_index: 0,
        user_id: subject_user_id,
        nickname: Some(subject_name.to_string()),
        connected: subject_is_bot,
        is_bot: subject_is_bot,
        seat_type: if subject_is_bot { "bot" } else { "human" }.to_string(),
        ..SeatState::default()
    });
    for seat_index in 1..4 {
        room.seats.push(SeatState {
            seat_index,
            nickname: Some(format!("sft_bot_{seat_index}")),
            connected: true,
            is_bot: true,
            seat_type: "bot".to_string(),
            ..SeatState::default()
        });
    }
    room
}

pub(crate) fn new_evaluation_id() -> String {
    format!("eval-{}", generate_short_hex(6))
}
```

- [ ] **Step 6: Add server routes**

Modify `build_app` in `backend/src/app/server.rs`:

```rust
.route("/api/evaluations", post(create_evaluation))
.route("/api/evaluations/{evaluation_id}", get(get_evaluation))
```

Add first-pass handlers that compile and return a valid live-session response:

```rust
async fn create_evaluation(
    State(state): State<AppContext>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<super::evaluation::CreateEvaluationRequest>,
) -> Response {
    let authenticated_user = match require_authenticated_user(&state, &headers).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    let _ = payload;
    let response = super::evaluation::EvaluationSessionResponse {
        evaluation_id: super::evaluation::new_evaluation_id(),
        seed: rand::random::<u64>(),
        subjects: Vec::new(),
    };
    let _ = authenticated_user;
    Json(response).into_response()
}

async fn get_evaluation(
    State(_state): State<AppContext>,
    axum::extract::Path(evaluation_id): axum::extract::Path<String>,
) -> Response {
    Json(super::evaluation::EvaluationSessionResponse {
        evaluation_id,
        seed: 0,
        subjects: Vec::new(),
    })
    .into_response()
}
```

- [ ] **Step 7: Fill session creation behavior**

Replace first-pass logic in `create_evaluation` with:

- Load authenticated user record.
- Build subject id list from authenticated user plus `payload.subject_user_ids`, deduped, max 4.
- Reject more than 4 with `UNPROCESSABLE_ENTITY` and `too_many_evaluation_subjects`.
- For each subject, load user.
- Determine special BOT by `state.inner.special_bot_user_ids`.
- Build one evaluation room with `build_evaluation_room`.
- Save table with `save_table_and_upsert_participant` for human subjects; for special BOT subjects save table only.
- Insert loaded room handle into `state.inner.rooms` using `RoomHandle::new(RoomRuntime::new(created_at.clone(), room.clone()))`.
- Schedule room tasks detached for each replica.
- Return `EvaluationSessionResponse` with one subject row per replica.
- Store the response in `state.inner.evaluation_sessions` under `evaluation_id`.

Use existing server helpers for `now_iso`, `serialize_room_state`, and `json_error`.

- [ ] **Step 8: Implement `GET /api/evaluations/:id` from live session state**

Read `state.inner.evaluation_sessions`. If the id is missing, return `404` with `evaluation_not_found`.

When the id exists, refresh each subject row from its room:

```rust
let mut response = stored.clone();
for subject in &mut response.subjects {
    if let Some(room_handle) =
        crate::app::room_runtime::ensure_room_loaded(&state, &subject.table_code)
            .await
            .ok()
            .flatten()
    {
        let runtime = room_handle.runtime.lock().await;
        apply_room_result_to_evaluation_subject(subject, &runtime.room);
    }
}
```

Add `apply_room_result_to_evaluation_subject` in `backend/src/app/evaluation.rs`. It should read `room.phase`, seat 0 score, and seat 0 `MatchSeatStatistics` from `room.match_state.statistics.seat_stats_by_seat`.

- [ ] **Step 9: Verify backend API compile/tests**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml app::evaluation app::server_table_tests -- --nocapture
```

Expected: PASS.

## Task 5: Protect Evaluation Room Rule Toggles And Starts

**Files:**
- Modify: `backend/src/app/ws.rs`
- Modify: `backend/src/lib.rs` if module exports require adjustment

- [ ] **Step 1: Add tests for evaluation rule toggle rejection**

Append to `backend/src/app/ws.rs` tests:

```rust
#[test]
fn evaluation_mode_is_detected_for_restricted_table_settings() {
    let mut room = crate::app::initial_room_state("EVALROOM");
    crate::evaluation::apply_evaluation_rules(&mut room);

    assert!(room_is_evaluation(&room));
}
```

- [ ] **Step 2: Implement room helper**

Add near other private helpers in `ws.rs`:

```rust
fn room_is_evaluation(room: &RoomState) -> bool {
    room.mode == crate::evaluation::EVALUATION_ROOM_MODE
}
```

- [ ] **Step 3: Reject setting changes in evaluation rooms**

In `handle_set_minimum_hu_fan`, `handle_set_dealer_rule_toggle`, and `handle_adjust_bots`, after locking runtime and before mutating:

```rust
if room_is_evaluation(&runtime.room) {
    return reject_to(connection, "evaluation_settings_locked");
}
```

Do not block `start_match` or normal player actions.

- [ ] **Step 4: Start evaluation rooms with deterministic seed**

If evaluation session rooms are started automatically in app creation, no `start_match` change is needed. If they rely on player clicking start, add deterministic seed storage before implementation. Prefer automatic start for AI-only rooms and manual start for human rooms in this first version.

- [ ] **Step 5: Verify ws tests**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml app::ws -- --nocapture
```

Expected: PASS.

## Task 6: Frontend Types And API Client

**Files:**
- Modify: `frontend/src/types/match.ts`
- Modify: `frontend/src/lib/socialApi.ts`
- Modify: `frontend/src/lib/api.test.ts` or create focused tests if existing pattern fits

- [ ] **Step 1: Add evaluation types**

Modify `frontend/src/types/match.ts`:

```ts
export type TableMode = 'normal' | 'evaluation';

export interface EvaluationSubjectResult {
  subject_id: string;
  user_id?: number | null;
  display_name: string;
  kind: 'human' | 'bot' | string;
  table_code: string;
  phase: RoomPhase;
  completed: boolean;
  final_score?: number | null;
  deal_in_count?: number | null;
  win_count?: number | null;
}

export interface EvaluationSession {
  evaluation_id: string;
  seed: number;
  subjects: EvaluationSubjectResult[];
}
```

- [ ] **Step 2: Add API functions**

Modify `frontend/src/lib/socialApi.ts` imports:

```ts
import type {
  AcceptInviteResponse,
  ActiveTableResponse,
  CreateTableResponse,
  EvaluationSession,
  PublicUser,
  TableInvite,
} from '../types/match';
```

Add:

```ts
export function createEvaluation(baseUrl: string, sessionToken: string, subjectUserIds: number[]) {
  return requestJson<EvaluationSession>(`${normalizeBaseUrl(baseUrl)}/api/evaluations`, {
    method: 'POST',
    headers: Object.assign({}, authHeaders(sessionToken), {
      'Content-Type': 'application/json',
    }),
    body: JSON.stringify({
      subject_user_ids: subjectUserIds,
    }),
  });
}

export function getEvaluation(baseUrl: string, evaluationId: string) {
  return requestJson<EvaluationSession>(`${normalizeBaseUrl(baseUrl)}/api/evaluations/${evaluationId}`);
}
```

- [ ] **Step 3: Verify frontend typecheck**

Run:

```powershell
npm --prefix frontend test -- --run frontend/src/lib/api.test.ts
```

Expected: PASS or no matching tests. If no matching tests, run `npm --prefix frontend test -- --run`.

## Task 7: Add Frontend Evaluation UI Components

**Files:**
- Create: `frontend/src/components/battle-screen/EvaluationDialog.tsx`
- Create: `frontend/src/components/battle-screen/EvaluationPanel.tsx`
- Modify: `frontend/src/components/battle-screen/BattleScreen.tsx`
- Modify: `frontend/src/components/battle-screen/scene/TableChrome.tsx`

- [ ] **Step 1: Create dialog component**

Create `frontend/src/components/battle-screen/EvaluationDialog.tsx`:

```tsx
import type { PublicUser } from '../../types/match';

interface EvaluationDialogProps {
  isOpen: boolean;
  currentUserId: number | null;
  humanUsers: PublicUser[];
  aiUsers: PublicUser[];
  selectedUserIds: number[];
  onToggleSubject: (userId: number) => void;
  onStart: () => void;
  onClose: () => void;
}

export function EvaluationDialog({
  isOpen,
  currentUserId,
  humanUsers,
  aiUsers,
  selectedUserIds,
  onToggleSubject,
  onStart,
  onClose,
}: EvaluationDialogProps) {
  if (!isOpen) {
    return null;
  }
  const candidates = humanUsers.concat(aiUsers).filter((user) => user.user_id !== currentUserId);
  const canStart = selectedUserIds.length <= 3;
  return (
    <div className="player-invite-dialog" role="dialog" aria-modal="true" aria-label="测评对象">
      <div className="player-invite-dialog__panel">
        <div className="player-invite-dialog__header">
          <h2>测评对象</h2>
          <button type="button" onClick={onClose} aria-label="关闭">×</button>
        </div>
        <div className="player-invite-dialog__list">
          {candidates.map((user) => {
            const selected = selectedUserIds.includes(user.user_id);
            return (
              <button
                type="button"
                key={user.user_id}
                className={selected ? 'is-selected' : undefined}
                onClick={() => onToggleSubject(user.user_id)}
                disabled={!selected && selectedUserIds.length >= 3}
              >
                <span>{user.display_label}</span>
              </button>
            );
          })}
        </div>
        <div className="player-invite-dialog__actions">
          <button type="button" onClick={onStart} disabled={!canStart}>开始测评</button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Create panel component**

Create `frontend/src/components/battle-screen/EvaluationPanel.tsx`:

```tsx
import type { EvaluationSession } from '../../types/match';

interface EvaluationPanelProps {
  evaluation: EvaluationSession | null;
}

export function EvaluationPanel({ evaluation }: EvaluationPanelProps) {
  if (!evaluation) {
    return null;
  }
  const bestScore = Math.max.apply(
    null,
    evaluation.subjects.map((subject) => subject.final_score ?? Number.NEGATIVE_INFINITY),
  );
  return (
    <section className="evaluation-panel" aria-label="测评结果">
      <h2>测评</h2>
      {evaluation.subjects.map((subject) => {
        const score = subject.final_score;
        const gap = score === null || score === undefined || !Number.isFinite(bestScore) ? null : score - bestScore;
        return (
          <article key={subject.subject_id} className="evaluation-panel__row">
            <strong>{subject.display_name}</strong>
            <span>{subject.completed ? `分数 ${score ?? 0}` : '进行中'}</span>
            <span>放铳 {subject.deal_in_count ?? '-'}</span>
            {gap !== null ? <span>差距 {gap}</span> : null}
          </article>
        );
      })}
    </section>
  );
}
```

- [ ] **Step 3: Add props to BattleScreen**

Modify `BattleScreenProps`:

```ts
onOpenEvaluation?: () => void;
evaluationPanel?: ReactNode;
```

Pass to `TableStage` / `TableChrome` through existing prop chain or render a button next to invite in `BattleScreen`. Prefer adding a compact table action in `TableChrome`.

- [ ] **Step 4: Add evaluation button in TableChrome**

Add a button near invite/start controls:

```tsx
{onOpenEvaluation ? (
  <button type="button" className="table-stage__tool-button" onClick={onOpenEvaluation}>
    测评
  </button>
) : null}
```

Use existing compact button classes. Do not create a landing page or large hero UI.

- [ ] **Step 5: Render evaluation panel**

In `BattleScreen`, render:

```tsx
{evaluationPanel}
```

near invite dialog/pending invite panel so it overlays the table without replacing the play surface.

- [ ] **Step 6: Verify component compile**

Run:

```powershell
npm --prefix frontend test -- --run frontend/src/components/battle-screen/TableStage.test.tsx
```

Expected: PASS.

## Task 8: Wire Evaluation UI In App

**Files:**
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/App.test.tsx`

- [ ] **Step 1: Add failing App test**

Append to `frontend/src/App.test.tsx`:

```ts
it('starts an evaluation with selected subjects', async () => {
  const user = userEvent.setup();
  const fetchMock = installFetchMock({
    evaluationResponse: {
      evaluation_id: 'eval-1',
      seed: 7,
      subjects: [
        {
          subject_id: 'user:1',
          user_id: 1,
          display_name: 'Owner',
          kind: 'human',
          table_code: 'EV01',
          phase: 'waiting',
          completed: false,
          final_score: null,
          deal_in_count: null,
          win_count: null,
        },
      ],
    },
  });

  render(<App />);

  const evaluationButton = await screen.findByRole('button', { name: '测评' });
  await user.click(evaluationButton);
  await user.click(screen.getByRole('button', { name: '开始测评' }));

  expect(findFetchCall(fetchMock, '/api/evaluations', 'POST')).toBeDefined();
});
```

If `installFetchMock` does not support `evaluationResponse`, extend its local options and response switch.

- [ ] **Step 2: Add state and handlers**

Modify `App.tsx` imports:

```ts
import {
  acceptTableInvite,
  createEvaluation,
  createSocialTable,
  createTableInvite,
  getEvaluation,
  getLeaderboard,
  getMyActiveTable,
  getMyInvites,
  rejectTableInvite,
} from './lib/socialApi';
import { EvaluationDialog } from './components/battle-screen/EvaluationDialog';
import { EvaluationPanel } from './components/battle-screen/EvaluationPanel';
import type { EvaluationSession } from './types/match';
```

Add state:

```ts
const [isEvaluationDialogOpen, setIsEvaluationDialogOpen] = useState(false);
const [selectedEvaluationSubjectIds, setSelectedEvaluationSubjectIds] = useState<number[]>([]);
const [evaluationSession, setEvaluationSession] = useState<EvaluationSession | null>(null);
```

Add handlers:

```ts
function handleToggleEvaluationSubject(userId: number) {
  setSelectedEvaluationSubjectIds((current) =>
    current.includes(userId)
      ? current.filter((id) => id !== userId)
      : current.length >= 3
        ? current
        : current.concat(userId),
  );
}

async function handleStartEvaluation() {
  if (!authSession?.sessionToken) {
    setStatusMessage('请先登录。');
    return;
  }
  try {
    const session = await createEvaluation(
      defaults.apiBaseUrl,
      authSession.sessionToken,
      selectedEvaluationSubjectIds,
    );
    setEvaluationSession(session);
    setIsEvaluationDialogOpen(false);
    setStatusMessage('测评已开始。');
  } catch (error) {
    setStatusMessage(error instanceof Error ? getSocialStatusCopy(error.message) : '测评创建失败。');
  }
}
```

- [ ] **Step 3: Poll active evaluation lightly**

Add an effect:

```ts
useEffect(() => {
  if (!evaluationSession) {
    return;
  }
  const id = window.setInterval(() => {
    void getEvaluation(defaults.apiBaseUrl, evaluationSession.evaluation_id)
      .then(setEvaluationSession)
      .catch(() => undefined);
  }, 5000);
  return () => window.clearInterval(id);
}, [defaults.apiBaseUrl, evaluationSession?.evaluation_id]);
```

- [ ] **Step 4: Pass dialog and panel**

In `renderBattleScreen`, pass:

```tsx
onOpenEvaluation={() => setIsEvaluationDialogOpen(true)}
evaluationPanel={<EvaluationPanel evaluation={evaluationSession} />}
```

Render dialog:

```tsx
<EvaluationDialog
  isOpen={isEvaluationDialogOpen}
  currentUserId={currentUser?.user_id ?? null}
  humanUsers={leaderboard.filter((user) => !user.is_special_bot)}
  aiUsers={leaderboard.filter((user) => user.is_special_bot)}
  selectedUserIds={selectedEvaluationSubjectIds}
  onToggleSubject={handleToggleEvaluationSubject}
  onStart={handleStartEvaluation}
  onClose={() => setIsEvaluationDialogOpen(false)}
/>
```

- [ ] **Step 5: Verify App tests**

Run:

```powershell
npm --prefix frontend test -- --run frontend/src/App.test.tsx
```

Expected: PASS.

## Task 9: Update View Model For Evaluation Mode

**Files:**
- Modify: `frontend/src/lib/matchViewModel.ts`
- Modify: `frontend/src/lib/matchViewModel.test.ts`

- [ ] **Step 1: Add test for locked evaluation settings**

Append:

```ts
it('hides waiting rule toggles in evaluation mode', () => {
  const state = createWaitingState();
  state.roomSnapshot!.payload.mode = 'evaluation';

  const viewModel = createMatchViewModel(state);

  expect(viewModel.roomMode).toBe('evaluation');
  expect(viewModel.waitingControls?.canToggleDealerRepeat).toBe(false);
  expect(viewModel.waitingControls?.canToggleDealerDouble).toBe(false);
  expect(viewModel.waitingControls?.canIncreaseMinimumHuFan).toBe(false);
  expect(viewModel.waitingControls?.canDecreaseMinimumHuFan).toBe(false);
});
```

- [ ] **Step 2: Implement lock logic**

In `createWaitingControls` or the existing waiting controls builder, keep the current boolean expressions and gate them with `!isEvaluation`:

```ts
const isEvaluation = snapshot.mode === 'evaluation';
```

Set:

```ts
canAddBot: !isEvaluation && botCount < TABLE_SEAT_CAPACITY - occupiedSeats,
canRemoveBot: !isEvaluation && botCount > 0,
canDecreaseMinimumHuFan: !isEvaluation && minimumHuFan > MINIMUM_HU_FAN_VALUES[0],
canIncreaseMinimumHuFan:
  !isEvaluation && minimumHuFan < MINIMUM_HU_FAN_VALUES[MINIMUM_HU_FAN_VALUES.length - 1],
canToggleDealerRepeat: !isEvaluation,
canToggleDealerDouble: !isEvaluation,
```

- [ ] **Step 3: Verify view model tests**

Run:

```powershell
npm --prefix frontend test -- --run frontend/src/lib/matchViewModel.test.ts
```

Expected: PASS.

## Task 10: Arena Scripts And Summary Update

**Files:**
- Modify: `backend/bot_trainer/v2/arena_matrix.ps1`
- Modify: `backend/bot_trainer/v2/arena_matrix.sh`
- Modify: `backend/bot_trainer/v2/arena_summary.py`
- Modify: `backend/bot_trainer/v2/README.md`

- [ ] **Step 1: Update matrix scripts config template**

Replace generated config shape with:

```json
{
  "matches": MATCHES,
  "seed": SEED,
  "max_actions_per_match": 2400,
  "subjects": [
    {"id":"SUBJECT_ID","display_name":"SUBJECT_NAME","model_path":"SUBJECT_MODEL"}
  ],
  "opponents": [
    {"id":"sft-opponent-1","model_path":"backend/assets/sft/sft.onnx"},
    {"id":"sft-opponent-2","model_path":"backend/assets/sft/sft.onnx"},
    {"id":"sft-opponent-3","model_path":"backend/assets/sft/sft.onnx"}
  ]
}
```

- [ ] **Step 2: Update summary script**

In `arena_summary.py`, read subject fields when present:

```python
subject_id = row.get("subject_id")
subject_score = row.get("subject_final_score")
subject_deal_in = row.get("subject_deal_in_count")
```

Aggregate by `subject_id` when available; otherwise keep previous seat-policy fallback to support old files.

- [ ] **Step 3: Update README**

Add a short section:

```markdown
## Evaluation Arena

Arena evaluation now runs subject replicas. Each subject plays an independent 16-hand evaluation against three configured opponents with the same seeds and initial seat, while normal in-match seat rotation remains controlled by the standard rule engine.
```

- [ ] **Step 4: Verify scripts/docs**

Run:

```powershell
rg -n "subject|opponents|seat_rotation|Evaluation Arena" backend/bot_trainer/v2
```

Expected: new subject/opponent config references exist; no active smoke config uses `seat_rotation`.

## Task 11: Final Verification

**Files:**
- All touched files

- [ ] **Step 1: Run targeted backend tests**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml evaluation bot::arena app::scheduler app::ws app::evaluation -- --nocapture
```

Expected: PASS.

- [ ] **Step 2: Run arena smoke**

Run:

```powershell
cargo run --manifest-path backend/Cargo.toml --bin bot_arena -- --config backend/bot_trainer/v2/arena_smoke.json --output backend/bot_trainer/v2/arena_smoke.jsonl
Get-Content backend/bot_trainer/v2/arena_smoke.jsonl -TotalCount 1
```

Expected: command succeeds; row contains subject fields.

- [ ] **Step 3: Run frontend tests**

Run:

```powershell
npm --prefix frontend test -- --run frontend/src/App.test.tsx frontend/src/lib/matchViewModel.test.ts frontend/src/components/battle-screen/TableStage.test.tsx
```

Expected: PASS.

- [ ] **Step 4: Run full frontend test suite if targeted tests pass**

Run:

```powershell
npm --prefix frontend test -- --run
```

Expected: PASS.

- [ ] **Step 5: Check repository status**

Run:

```powershell
git status --short
```

Expected: changes are limited to evaluation core, arena migration, app API/scheduler, frontend evaluation UI/API, tests, and docs.

## Completion Criteria

- Shared evaluation module exists and fixes evaluation rules.
- Arena config uses subjects + exactly three opponents.
- Arena hard seat rotation config is removed from active config path.
- Arena smoke emits subject-centered metrics while preserving seat metrics.
- Frontend can create an evaluation session from a visible entry.
- Evaluation result/progress panel shows score and deal-in count.
- Evaluation rooms use 150ms delay when human-controlled and 0ms when fully automated.
- Targeted backend and frontend tests pass.
