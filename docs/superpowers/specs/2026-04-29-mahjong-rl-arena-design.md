# Mahjong RL Arena Design

## Goal

Build a reinforcement-learning path for the Mahjong bot that improves practical playing strength. The primary objective is long-term match strength: average score delta and win rate first, deal-in control second, and faster tenpai / faster wins as auxiliary goals.

## Current Context

The bot already has a backend-native supervised learning pipeline:

- Rust exports decision samples from replayed matches.
- Python trains `MahjongPolicyNetV2`, a multi-head MLP.
- Runtime loads a single ONNX model and masks illegal actions.
- Existing heads are `discard_logits`, `claim_logits`, `self_kong_logits`, `hu_logits`, `value`, and `risk_logits`.
- The standard rule engine already supports deterministic round starts through seeded wall generation.

This design keeps those boundaries. Rust remains the source of truth for game rules, legal actions, scoring, and self-play simulation. Python owns model training and ONNX export.

## Non-Goals

- Do not replace the Rust rule engine with a separate Python Mahjong environment.
- Do not start with full AlphaZero-style search.
- Do not optimize only for fastest win at the cost of high deal-in rate or weak score delta.
- Do not replace the production ONNX deployment path in the first iteration.

## Recommended Approach

Use an arena-first RL pipeline:

1. Build a deterministic all-bot arena.
2. Add explicit bot policy configuration so one process can compare multiple policies and models.
3. Export self-play trajectories from arena games.
4. Train with PPO / actor-critic from the existing supervised checkpoint.
5. Select models by arena results, not validation imitation accuracy.

This is intentionally incremental. The arena is valuable even before RL because it measures whether the current supervised model actually improves match outcomes.

## Architecture

### Rust Arena

Create an in-process arena that runs standard Mahjong matches using existing `RoomState`, `start_match_in_room_state`, action handlers, settlement, and bot policy code.

Responsibilities:

- Create four bot seats.
- Start matches with deterministic seeds.
- Resolve active turns, claim windows, flowers, kongs, hu, exhaustive draw, and settlement through existing rules.
- Run multiple policy configurations in the same match batch.
- Emit per-match JSONL reports and aggregate summaries.
- Optionally emit decision-level self-play trajectories.

The arena should be a backend binary such as `backend/src/bin/bot_arena.rs`, backed by reusable helpers in `backend/src/bot/arena.rs`.

### Policy Configuration

Current runtime policy selection is environment-variable based. The arena needs explicit config:

```json
{
  "matches": 200,
  "seed": 20260429,
  "policies": [
    {"id": "heuristic", "mode": "heuristic", "neural_weight": 0, "model_path": null},
    {"id": "hybrid30", "mode": "hybrid", "neural_weight": 30, "model_path": "backend/assets/models/mahjong_policy_net.onnx"},
    {"id": "rl_candidate", "mode": "hybrid", "neural_weight": 30, "model_path": "backend/bot_trainer/v2/checkpoints_rl/candidate.onnx"}
  ]
}
```

Runtime env wrappers should remain for production. Arena calls should route through explicit policy config so experiments are reproducible.

### RL Environment Boundary

The RL environment should be an adapter over the Rust arena, not a duplicate engine.

Conceptual interface:

```text
reset(seed) -> observation
legal_actions(seat) -> action_mask
step(action) -> observation, reward, done, info
export_trajectory() -> JSONL
```

The first implementation can be batch-oriented rather than interactive: Rust runs self-play games and writes trajectory JSONL; Python trains from those trajectories. A later version can add a live Rust-Python loop if training throughput requires it.

### Observation

Reuse the existing V2 feature schema:

- `tile_planes`: 10 x 34
- `scalar_features`: 10
- legal masks for discard, claim, self-kong, and hu
- decision kind
- seat index and match metadata for debugging

Any future feature additions must update Rust runtime encoding and Python dataset encoding together.

### Action Encoding

Keep the current split action heads:

- active turn discard: 34 tile logits
- claim window: 7 claim logits
- self kong: 3 self-kong logits
- hu: 2 logits

For RL trajectories, store both the semantic action and the selected head / target index. This avoids ambiguous reconstruction during training.

### Reward Design

The reward should optimize comprehensive strength:

```text
terminal_reward =
  normalized_score_delta
  + win_bonus
  - deal_in_penalty
  + early_win_bonus

step_reward =
  tenpai_progress_bonus
  - unsafe_discard_penalty
```

Recommended first weights:

- `normalized_score_delta`: `score_delta / 100.0`
- `win_bonus`: `+1.0` for winning the hand
- `deal_in_penalty`: `-1.5` for dealing into another player's win
- `early_win_bonus`: up to `+0.5`, scaled by remaining wall tiles
- `tenpai_progress_bonus`: `+0.05` when reaching tenpai for the first time
- `unsafe_discard_penalty`: start disabled unless danger labeling is reliable

The first RL version should rely mostly on terminal score delta. Shaped rewards should be small so they guide learning without overpowering actual match outcome.

### Training Algorithm

Use PPO as the first RL algorithm.

Reasons:

- It handles stochastic policy improvement better than DQN for masked, multi-head actions.
- It supports warm starting from the supervised checkpoint.
- It works with actor-critic value estimates.
- It is simpler than imperfect-information MCTS.

Training flow:

1. Load supervised `best.pt`.
2. Run arena self-play and export trajectories.
3. Compute returns and advantages per decision.
4. Update policy heads with PPO clipped objective.
5. Update value head against returns.
6. Export candidate ONNX.
7. Evaluate candidate in arena against baseline and historical checkpoints.

### League Evaluation

Avoid selecting a model that only beats its own latest version. Keep a small pool:

- heuristic baseline
- current production hybrid
- last accepted RL model
- one or two older RL checkpoints

Acceptance criteria for replacing production model:

- average score delta improves over current production baseline
- win rate does not regress
- deal-in rate does not increase by more than 2 percentage points
- first-tenpai turn or final-tenpai rate improves, or stays neutral
- average decision latency remains under 100 ms

## Data Outputs

### Match Report JSONL

One row per arena match:

```json
{
  "match_index": 0,
  "seed": 20260429,
  "seats": [
    {
      "seat_index": 0,
      "policy_id": "hybrid30",
      "score_delta": 12,
      "wins": 1,
      "dealt_in": 0,
      "first_tenpai_turn": 9,
      "final_tenpai": true,
      "claim_count": 2,
      "discard_count": 17,
      "decision_count": 19,
      "decision_latency_ms_sum": 48
    }
  ]
}
```

### Trajectory JSONL

One row per bot decision:

```json
{
  "schema_version": 1,
  "match_id": "arena-20260429-0",
  "decision_index": 42,
  "seat_index": 2,
  "policy_id": "rl_candidate",
  "decision_kind": "active_turn",
  "observation": {
    "tile_planes": [],
    "scalar_features": [],
    "discard_mask": [],
    "claim_mask": [],
    "self_kong_mask": [],
    "hu_mask": []
  },
  "action": {
    "head": "discard",
    "index": 18,
    "semantic": "discard:b1"
  },
  "log_prob": -1.72,
  "value": 0.14,
  "reward": 0.0,
  "done": false
}
```

## Implementation Phases

### Phase 1: Arena MVP

Add the arena binary and metrics model. It must run deterministic all-bot games and write match reports. No RL training yet.

Verification:

- run a 2-match smoke arena
- confirm JSONL has one line per match
- confirm score deltas and settlement are present
- confirm no action loop exceeds a hard cap such as 600 actions per hand

### Phase 2: Explicit Policy Config

Add policy config structs and route arena decisions through config-driven bot entry points.

Verification:

- arena can compare heuristic, neural, and hybrid in one config
- missing model path falls back safely for hybrid or fails clearly for neural-only
- production env behavior remains unchanged

### Phase 3: Baseline Matrix

Run the arena before RL to establish baselines:

- heuristic
- pure neural
- hybrid weights such as 5, 15, 30, 60
- current production default

Verification:

- record average score delta, win rate, deal-in rate, tenpai speed, and latency
- choose the strongest non-RL baseline as the control group

### Phase 4: Trajectory Export

Extend arena to export decision trajectories.

Verification:

- every trajectory action is legal under its mask
- rewards sum to the expected terminal score-related value
- trajectory rows contain enough information for PPO without replaying the game

### Phase 5: PPO Trainer

Add Python RL training scripts in `backend/bot_trainer/v2`.

Verification:

- load supervised checkpoint
- run one epoch on smoke trajectories
- export ONNX with the same named outputs consumed by Rust
- Rust ONNX smoke test still passes

### Phase 6: League Selection

Run candidate models against baseline and historical models.

Verification:

- candidate meets acceptance criteria before replacing production asset
- results are recorded in README or an experiment report

## Testing Strategy

Rust tests:

- arena config parsing
- deterministic seed reproducibility
- policy config routing
- legal action enforcement
- metric aggregation
- trajectory action-mask consistency

Python tests:

- trajectory dataset loading
- advantage / return computation
- PPO loss with action masks
- ONNX export output names and shapes

Smoke commands:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::arena bot::policy bot::neural rules::standard::automation -- --nocapture
cargo run --manifest-path backend/Cargo.toml --bin bot_arena -- --config backend/bot_trainer/v2/arena_smoke.json --output backend/bot_trainer/v2/arena_smoke.jsonl
uv run python backend/bot_trainer/v2/rl_train.py --trajectories backend/bot_trainer/v2/arena_trajectories_smoke.jsonl --epochs 1 --output backend/bot_trainer/v2/checkpoints_rl_smoke
```

## Risks

- RL may overfit to its current opponents. Mitigation: keep a league of historical models.
- Reward shaping may create bad incentives. Mitigation: keep terminal score delta dominant.
- Arena throughput may be too slow with ONNX per decision. Mitigation: start with evaluation correctness, then batch inference or cache model sessions.
- Feature mismatch between Rust and Python can corrupt training. Mitigation: reuse metadata and add shape / mask consistency tests.
- Pure neural may regress tactical safety. Mitigation: evaluate hybrid mode and deal-in rate before production replacement.

## Open Decisions

- Whether the first RL trainer reads offline trajectories only or uses a live Rust-Python loop.
- Whether the production policy after RL should be pure neural or hybrid. Default recommendation is hybrid until arena results prove pure neural is stronger and safe.
- Exact reward weights should be tuned after the first arena baseline matrix.

## Approval Status

The target objective is approved as comprehensive strength: score delta and win rate first, controlled deal-in risk second, speed as an auxiliary signal.
