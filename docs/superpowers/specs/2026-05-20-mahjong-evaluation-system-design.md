# Mahjong Evaluation System Design

## Goal

Introduce a shared evaluation system for the Guobiao Mahjong project. The system serves both backend arena model evaluation and frontend player-facing skill tests.

The evaluation mode compares multiple subjects fairly by letting each subject play an independent replicated 16-hand standard match against the same three configured BOT opponents. Every subject uses the same subject seat schedule and the same round seeds, so wall order is identical across replicas.

## Approved Direction

Use a shared `evaluation` concept instead of letting arena and frontend implement separate semantics.

The selected player-facing semantics are **parallel replicated evaluation**:

- Each subject gets an independent match replica.
- Each replica has one subject plus three BOT opponents.
- All replicas use the same subject seat positions and same seeds.
- Final reports compare subjects horizontally.

This avoids the unfairness of putting all subjects at the same real table, where their decisions would change each other's environment.

## Standard Evaluation Rules

Every evaluation match uses the standard Guobiao rules already implemented by the backend engine, with these fixed settings:

- 16 hands: east/south/west/north, 4 hands per wind.
- 8 fan minimum win threshold.
- Dealer repeat disabled.
- Dealer double disabled.
- Normal wind-end seat rotation.
- Standard round start wall generation from deterministic seeds.
- No arena-only hard seat rotation configuration.

The existing `RoomState` rule fields are sufficient for the fixed rules:

- `minimum_hu_fan = 8`
- `dealer_repeat_enabled = false`
- `dealer_double_enabled = false`

Seat rotation should remain the standard rule-engine rotation in `rules::standard::flow`, not the existing arena policy-seat rotation fields.

## Shared Evaluation Model

Add a backend module that owns evaluation data contracts and helpers. Proposed file:

- `backend/src/evaluation.rs`

Responsibilities:

- Define evaluation config structs.
- Generate deterministic replica schedules.
- Build subject + opponent policy assignments.
- Aggregate per-subject result summaries.
- Expose helpers usable by both `bot_arena` and app room creation.

Core concepts:

```rust
pub struct EvaluationConfig {
    pub seed: u64,
    pub hands: usize, // fixed to 16 for frontend; configurable only for tests/smoke
    pub subjects: Vec<EvaluationSubjectConfig>,
    pub opponents: [ArenaBotPolicyConfig; 3],
}

pub struct EvaluationSubjectConfig {
    pub id: String,
    pub display_name: String,
    pub kind: EvaluationSubjectKind,
    pub policy: Option<ArenaBotPolicyConfig>,
}

pub enum EvaluationSubjectKind {
    Human,
    Bot,
}
```

The subject seat schedule is derived from the normal match flow. The subject starts in the same initial seat in every replica, and the existing rule engine handles wind-end seat rotation consistently.

For frontend evaluation, the default opponents are three SFT BOT policies. The UI does not expose opponent selection.

For arena evaluation, the config may specify the three opponent policies explicitly.

## Arena Behavior

Replace the current arena evaluation style with replicated subject evaluation.

Arena config should describe:

- Number of evaluation samples or seed count.
- One or more subject policies.
- Three opponent BOT policies.
- Output paths and optional trajectory export, preserving existing arena output capabilities.

The current `seat_rotation` / `seat_rotation_offset` arena mechanism should be removed from the arena config and tests. Normal in-match seat rotation remains.

Arena output should preserve existing metrics where possible:

- Score delta / final score
- Win count
- Deal-in count
- Tenpai metrics
- Claim/discard/decision counts
- Decision latency metrics
- Neural model telemetry

The summary layer can continue to use the previous evaluation criteria. The important behavior change is that comparisons are now across replicated subject runs instead of policy-to-seat rotation.

## Frontend Evaluation Flow

Add a visible evaluation entry in the main app experience. The entry should let the current player start an evaluation session and optionally select 1-3 other subjects:

- Human users from the existing player list.
- AI users from existing seeded special BOT users, if available.

Once started, the backend creates an evaluation session containing one replica room per subject.

For a human subject:

- The subject connects to their assigned replica room.
- The other three seats are regular SFT BOT opponents.
- BOT action delay remains 150ms.

For a BOT or AI subject:

- The replica runs without human input.
- BOT action delay is 0ms so the replica can complete quickly.

The first frontend iteration should keep the UI small and operational:

- Entry action: "测评"
- Subject picker: current user fixed, up to 3 extra subjects.
- Progress view: each subject status, current hand if available, final score when done.
- Result view: subject display name, final score, deal-in count, and simple relative gap from the best score.

The evaluation report should be available after all replicas complete.

## Backend App Flow

Add app-level support for evaluation sessions without replacing normal tables.

Proposed modules:

- `backend/src/app/evaluation.rs`
- API route additions in `backend/src/app/server.rs`
- WebSocket behavior additions in `backend/src/app/ws.rs` only where needed for evaluation room mode.

Suggested API:

```text
POST /api/evaluations
GET /api/evaluations/:evaluation_id
```

`POST /api/evaluations` accepts selected subject user ids. The authenticated user is always included as a human subject.

The backend creates:

- An evaluation session record.
- One replica table per subject.
- Seat 0 as the subject initial seat unless a future config says otherwise.
- Seats 1-3 as default SFT BOT opponents.

Replica rooms should use:

- `mode = "evaluation"`
- `minimum_hu_fan = 8`
- `dealer_repeat_enabled = false`
- `dealer_double_enabled = false`
- deterministic seed derived from the evaluation session seed.

If a human subject is invited, the existing invite/join flow can be reused where practical, but the evaluation session must preserve the replica relationship and result aggregation.

## Bot Delay Policy

Keep the global default delay:

- Normal rooms and human-involved evaluation replicas: 150ms.

Use fast delay:

- Evaluation replicas whose subject is BOT/AI and have no human-controlled seats: 0ms.
- Backend arena evaluation: 0ms because it runs in-process and does not use the app scheduler delay.

Implementation should avoid changing `BOT_ACTION_DELAY_MS` globally. Instead, add a room-aware resolver:

```rust
fn bot_action_delay_ms(room: &RoomState) -> u64
```

This returns 0 for fully automated evaluation replicas and 150 otherwise.

## Result Aggregation

Each subject report should include at minimum:

```json
{
  "subject_id": "user:123",
  "display_name": "Alice",
  "kind": "human",
  "completed": true,
  "final_score": 42,
  "deal_in_count": 2,
  "win_count": 3
}
```

Arena reports keep their current seat metrics but should also expose a subject-centered summary so model comparisons are easy.

Frontend result copy should focus on player-understandable output:

- Final score
- Deal-in count
- Gap to best subject

Avoid introducing a new public rating formula in the first version.

## Testing Strategy

Backend tests:

- Evaluation config rejects unknown arena seat-rotation fields.
- Replicated subject matches use the same seeds for each subject.
- Evaluation rooms force 8 fan, no dealer repeat, no dealer double.
- Normal rule-engine wind-end seat rotation still applies.
- Arena config can specify exactly three opponent policies.
- Frontend default evaluation opponents resolve to SFT policies.
- Bot delay resolver returns 150ms for human-involved replicas and 0ms for fully automated replicas.
- Result aggregation records final score and deal-in count per subject.

Frontend tests:

- Evaluation entry is visible for logged-in users.
- Subject picker includes current user and supports selecting up to 3 additional subjects.
- Starting evaluation calls the new API with selected subjects.
- Result view renders score and deal-in count.

Smoke verification:

```powershell
cargo test --manifest-path backend/Cargo.toml evaluation bot::arena rules::standard::flow app::scheduler -- --nocapture
npm --prefix frontend test -- --run
```

## Rollout Phases

### Phase 1: Shared Evaluation Core

Add shared evaluation structs, deterministic replica schedule helpers, result summary helpers, and targeted tests.

### Phase 2: Arena Migration

Refactor `bot_arena` to run subject replicas against three opponent BOTs. Remove arena hard seat rotation config and update smoke configs/scripts.

### Phase 3: Backend App Evaluation Sessions

Add evaluation session creation, replica room creation, result aggregation, and room-aware BOT delay.

### Phase 4: Frontend Entry And Results

Add evaluation entry, subject picker, session creation call, progress/result display, and focused tests.

## Non-Goals

- Do not build a permanent public rating ladder in the first version.
- Do not expose opponent BOT selection in the frontend.
- Do not replace the normal table flow.
- Do not change core Guobiao scoring rules beyond using the fixed evaluation toggles.
- Do not introduce a new frontend visual system unrelated to the existing BattleScreen.

## Risks

- Human evaluation replicas may finish at different times, so aggregation must tolerate partial results.
- If human subjects are offline, their replica should remain pending or be cancellable rather than blocking other completed subjects.
- Removing arena `seat_rotation` may require updating existing scripts and tests that rely on it.
- Result aggregation must track the moving player identity through normal wind-end seat rotation, not assume final seat index equals original subject identity.

## Approval Status

Direction approved by user:

- Use shared evaluation system,方案 C.
- Use parallel replicated evaluation semantics.
- Frontend default opponents are three SFT BOTs.
- Arena opponents are configurable.
