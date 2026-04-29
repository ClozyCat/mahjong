# PPO ResNet Shanten Reward Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve Mahjong bot RL training in the requested order: fix PPO trajectories, replace the flattened MLP tile encoder with a CNN/ResNet encoder, then add fan-aware shanten shaping reward.

**Architecture:** Rust remains the authoritative arena and rule/reward source. Python owns PPO math, model architecture, checkpoint compatibility, and ONNX export. The ONNX input/output names and action heads stay compatible with the existing runtime.

**Tech Stack:** Rust backend (`backend/src/bot`, `backend/src/rules/standard`), Python trainer (`torch`, JSONL datasets), existing V2 feature schema (`tile_planes: 10 x 34`, `scalar_features: 10`), ONNX export.

---

## Implementation Order

1. Fix and enrich PPO trajectory data so PPO trains on returns/advantages and real old policy statistics instead of zero placeholders.
2. Replace only the Python tile encoder with a CNN/ResNet encoder while preserving existing model inputs, outputs, masks, and ONNX deployment.
3. Add fan-aware shanten shaping reward in Rust arena trajectory generation and consume it through the corrected PPO return pipeline.

## File Map

- Modify: `backend/src/rules/standard/automation.rs`
  - Add trace metadata needed by arena reward and behavior-policy logging.
- Modify: `backend/src/bot/arena.rs`
  - Extend trajectory schema, compute terminal reward separately from step reward, compute per-seat trajectory done markers, and add shaping fields later.
- Modify: `backend/src/bot/neural.rs`
  - Expose masked logits/value access for arena logging where the selected policy is neural-backed.
- Modify: `backend/src/bot/search.rs`
  - Expose a small public helper for shanten calculation from tile keys/open meld count.
- Create: `backend/src/bot/reward.rs`
  - Own shanten/fan-potential reward helpers and unit tests.
- Modify: `backend/src/bot/mod.rs`
  - Export `reward` module.
- Modify: `backend/bot_trainer/v2/rl_dataset.py`
  - Load trajectory rows by episode, compute returns and GAE-ready tensors, validate schema.
- Modify: `backend/bot_trainer/v2/rl_train.py`
  - Use returns/advantages, normalize advantages, include entropy/value clipping, and optionally recompute old log-probs from the rollout checkpoint when Rust did not emit them.
- Modify: `backend/bot_trainer/v2/model.py`
  - Replace `nn.Flatten() -> Linear` tile encoder with `SuitAwareTileResNet`.
- Modify: `backend/bot_trainer/v2/train.py`
  - Add checkpoint compatibility loader for old MLP checkpoints.
- Modify: `backend/bot_trainer/v2/export_onnx.py`
  - Verify new ResNet model exports with unchanged ONNX contract.
- Modify: `backend/bot_trainer/v2/test_rl_dataset.py`
  - Add tests for returns, per-seat episode boundaries, and shaped rewards.
- Create: `backend/bot_trainer/v2/test_model.py`
  - Add tests for ResNet output shapes and old-checkpoint partial loading.
- Modify: `backend/bot_trainer/v2/README.md`
  - Document staged training commands and acceptance metrics.

## Phase 1: Fix And Enhance PPO Trajectories

### Task 1: Split Terminal Reward, Step Reward, And Per-Seat Done

**Files:**
- Modify: `backend/src/bot/arena.rs`
- Test: `backend/src/bot/arena.rs`

- [ ] **Step 1: Add trajectory reward fields**

Update `ArenaTrajectoryRow` to keep the current `reward` for backward compatibility, but add explicit fields:

```rust
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
    pub step_reward: f32,
    pub terminal_reward: f32,
    pub done: bool,
}
```

- [ ] **Step 2: Assign terminal reward to each seat's last row only**

Replace the current all-row terminal reward assignment with per-seat assignment:

```rust
fn assign_terminal_rewards(rows: &mut [ArenaTrajectoryRow], report: &ArenaMatchReport) {
    for row in rows.iter_mut() {
        row.terminal_reward = 0.0;
        row.reward = row.step_reward;
        row.done = false;
    }

    for seat in &report.seats {
        let terminal = terminal_reward_for_seat(seat);
        if let Some(row) = rows.iter_mut().rev().find(|row| row.seat_index == seat.seat_index) {
            row.terminal_reward = terminal;
            row.reward = row.step_reward + terminal;
            row.done = true;
        }
    }
}

fn terminal_reward_for_seat(seat: &ArenaSeatMetrics) -> f32 {
    let score_reward = seat.score_delta as f32 / 100.0;
    let win_bonus = if seat.wins > 0 { 1.0 } else { 0.0 };
    let deal_in_penalty = if seat.dealt_in > 0 { -1.5 } else { 0.0 };
    score_reward + win_bonus + deal_in_penalty
}
```

- [ ] **Step 3: Add Rust test for per-seat done markers**

Add a test that creates rows for seats `0` and `1`, calls `assign_terminal_rewards`, and asserts only each seat's last row is `done`.

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::arena::tests::terminal_rewards_mark_each_seat_last_row_done -- --nocapture
```

Expected: `test result: ok`.

### Task 2: Make PPO Dataset Episode-Aware

**Files:**
- Modify: `backend/bot_trainer/v2/rl_dataset.py`
- Test: `backend/bot_trainer/v2/test_rl_dataset.py`

- [ ] **Step 1: Preserve row order and compute returns per `(match_id, seat_index)`**

Add a function that groups contiguous rows by `match_id` and `seat_index`, then computes discounted returns within each seat episode:

```python
def compute_discounted_returns_for_rows(
    rows: list[dict[str, Any]],
    gamma: float,
) -> list[float]:
    returns = [0.0 for _ in rows]
    groups: dict[tuple[str, int], list[int]] = {}
    for index, row in enumerate(rows):
        key = (str(row["match_id"]), int(row["seat_index"]))
        groups.setdefault(key, []).append(index)

    for indices in groups.values():
        running = 0.0
        for index in reversed(indices):
            running = float(rows[index]["reward"]) + gamma * running
            returns[index] = round(running, 6)
    return returns
```

- [ ] **Step 2: Store returns in `ArenaTrajectoryDataset`**

Change `ArenaTrajectoryDataset.__init__` to accept `gamma`, compute `self.returns`, and return `"return"` from `__getitem__`.

- [ ] **Step 3: Add Python test**

Use rows from two seats in the same match and assert returns do not leak across seats.

Run:

```powershell
python -m pytest backend/bot_trainer/v2/test_rl_dataset.py -q
```

Expected: all tests pass.

### Task 3: Correct PPO Loss Inputs

**Files:**
- Modify: `backend/bot_trainer/v2/rl_train.py`
- Test: `backend/bot_trainer/v2/test_rl_dataset.py`

- [ ] **Step 1: Train value against returns, not single-step reward**

Replace:

```python
advantages = batch["reward"].float() - batch["old_value"].float()
value_loss = F.mse_loss(values, batch["reward"].float())
```

with:

```python
returns = batch["return"].float()
advantages = returns - batch["old_value"].float()
advantages = (advantages - advantages.mean()) / (advantages.std(unbiased=False) + 1.0e-8)
value_loss = F.mse_loss(values, returns)
```

- [ ] **Step 2: Add entropy bonus**

Add masked entropy for the active head and use:

```python
loss = policy_loss + 0.5 * value_loss - args.entropy_coef * entropy_loss
```

Default `--entropy-coef` should be `0.01`.

- [ ] **Step 3: Add assertions for invalid PPO placeholders**

Reject trajectory files where every `log_prob` is `0.0` and every `value` is `0.0`, unless `--recompute-old-policy-stats` is passed.

Run:

```powershell
python backend/bot_trainer/v2/rl_train.py --trajectories backend/bot_trainer/v2/arena_trajectories_smoke.jsonl --checkpoint backend/bot_trainer/v2/checkpoints/best.pt --epochs 1 --batch-size 64 --output backend/bot_trainer/v2/checkpoints_rl_smoke --device cpu
```

Expected before Task 4: exits with a clear message explaining missing old policy stats.

### Task 4: Emit Or Recompute Old Log-Prob And Value

**Files:**
- Modify: `backend/src/bot/arena.rs`
- Modify: `backend/src/bot/neural.rs`
- Modify: `backend/bot_trainer/v2/rl_train.py`

- [ ] **Step 1: Short-term fallback in Python**

Add `--recompute-old-policy-stats`. When set, load the checkpoint before training, run a no-grad forward pass over all batches, and replace `old_log_prob` and `old_value` using the recorded action and masks. This makes PPO internally consistent for neural rollout checkpoints even if Rust emitted zeros.

- [ ] **Step 2: Long-term Rust emission**

Expose neural logits/value from `backend/src/bot/neural.rs` and fill `ArenaTrajectoryRow.log_prob`/`value` in `trajectory_row_from_trace` when the policy mode is `Neural` with a model path. For heuristic-only rows, keep zeros and mark them unsuitable for PPO unless recomputed from a checkpoint.

- [ ] **Step 3: Add smoke verification**

Run:

```powershell
.\backend\bot_trainer\v2\train_rl_model.ps1 -OutputDir backend/bot_trainer/v2/rl_runs/smoke -TrajectoryMatches 1 -EvalMatches 1 -Epochs 1 -BatchSize 64 -Device cpu
```

Expected: trajectory generation succeeds, PPO training no longer trains against all-zero old policy stats, and `candidate_eval_summary.json` is written.

## Phase 2: Replace MLP Tile Encoder With CNN/ResNet

### Task 5: Add Suit-Aware ResNet Tile Encoder

**Files:**
- Modify: `backend/bot_trainer/v2/model.py`
- Create: `backend/bot_trainer/v2/test_model.py`

- [ ] **Step 1: Add residual block**

Add:

```python
class ResidualConvBlock(nn.Module):
    def __init__(self, channels: int) -> None:
        super().__init__()
        self.net = nn.Sequential(
            nn.Conv1d(channels, channels, kernel_size=3, padding=1),
            nn.ReLU(),
            nn.Conv1d(channels, channels, kernel_size=3, padding=1),
        )
        self.norm = nn.BatchNorm1d(channels)

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        return torch.relu(self.norm(self.net(x) + x))
```

- [ ] **Step 2: Add `SuitAwareTileResNet`**

Use one shared suited-tile encoder on `0:9`, `9:18`, `18:27`, and a separate honor encoder on `27:34`. Concatenate embeddings and project to `512`.

- [ ] **Step 3: Replace `self.tile_encoder`**

Replace:

```python
self.tile_encoder = nn.Sequential(
    nn.Flatten(),
    nn.Linear(tile_plane_count * 34, 512),
    nn.ReLU(),
    nn.LayerNorm(512),
)
```

with:

```python
self.tile_encoder = SuitAwareTileResNet(tile_plane_count, embedding_size=512)
```

- [ ] **Step 4: Add output-shape test**

Test that all heads retain the same shapes:

```python
def test_resnet_model_output_shapes():
    model = build_model(ModelConfig(tile_plane_count=10, scalar_feature_count=10))
    outputs = model(torch.zeros((2, 10, 34)), torch.zeros((2, 10)))
    assert outputs["discard_logits"].shape == (2, 34)
    assert outputs["claim_logits"].shape == (2, 7)
    assert outputs["self_kong_logits"].shape == (2, 3)
    assert outputs["hu_logits"].shape == (2, 2)
    assert outputs["value"].shape == (2, 1)
    assert outputs["risk_logits"].shape == (2, 34)
```

Run:

```powershell
python -m pytest backend/bot_trainer/v2/test_model.py -q
```

Expected: all tests pass.

### Task 6: Add Backward-Compatible Checkpoint Loading

**Files:**
- Modify: `backend/bot_trainer/v2/train.py`
- Modify: `backend/bot_trainer/v2/rl_train.py`
- Test: `backend/bot_trainer/v2/test_model.py`

- [ ] **Step 1: Add compatible loader**

Implement:

```python
def load_compatible_state_dict(model: torch.nn.Module, state: dict[str, torch.Tensor]) -> list[str]:
    current = model.state_dict()
    compatible = {
        key: value
        for key, value in state.items()
        if key in current and current[key].shape == value.shape
    }
    model.load_state_dict(compatible, strict=False)
    return sorted(set(state) - set(compatible))
```

- [ ] **Step 2: Use compatible loader for supervised and RL warm starts**

The old MLP tile encoder weights will be skipped; scalar encoder, trunk heads with compatible shapes, and output heads load when shapes match.

- [ ] **Step 3: Run supervised smoke**

Run:

```powershell
python backend/bot_trainer/v2/train.py --data backend/bot_trainer/v2/out_smoke --epochs 1 --batch-size 64 --output backend/bot_trainer/v2/checkpoints_resnet_smoke --device cpu
```

Expected: training starts and saves `best.pt`.

### Task 7: Verify ONNX Contract

**Files:**
- Modify: `backend/bot_trainer/v2/export_onnx.py`
- Modify: `backend/bot_trainer/v2/README.md`

- [ ] **Step 1: Export ResNet checkpoint**

Run:

```powershell
python backend/bot_trainer/v2/export_onnx.py --checkpoint backend/bot_trainer/v2/checkpoints_resnet_smoke/best.pt --output backend/bot_trainer/v2/checkpoints_resnet_smoke/candidate.onnx
```

Expected: input names remain `tile_planes`, `scalar_features`; output names remain `discard_logits`, `claim_logits`, `self_kong_logits`, `hu_logits`, `value`, `risk_logits`.

- [ ] **Step 2: Run Rust ONNX smoke**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::neural -- --nocapture
```

Expected: neural tests pass using the exported model path configured for the smoke.

## Phase 3: Add Fan-Aware Shanten Shaping Reward

### Task 8: Expose Shanten Helper

**Files:**
- Modify: `backend/src/bot/search.rs`
- Create: `backend/src/bot/reward.rs`
- Modify: `backend/src/bot/mod.rs`

- [ ] **Step 1: Add helper in `reward.rs`**

Create a helper that converts tile keys into `TileCounts` and calls `min_shanten_for_counts`.

```rust
pub(crate) fn shanten_for_tile_keys(tile_keys: &[String], open_meld_count: usize) -> Option<i32> {
    let mut counts = [0_u8; crate::bot::action_space::TILE_KIND_COUNT];
    for tile_key in tile_keys {
        let index = crate::bot::action_space::tile_index(tile_key)?;
        counts[index] = counts[index].saturating_add(1);
    }
    Some(crate::bot::search::min_shanten_for_counts(&counts, open_meld_count))
}
```

- [ ] **Step 2: Add test for known tenpai shape**

Use `w1 w2 w3 w4 w5 w6 t1 t2 t3 b1 b2 b3 east` and assert shanten is `0`.

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::reward -- --nocapture
```

Expected: reward helper tests pass.

### Task 9: Add Fan Potential Heuristic

**Files:**
- Modify: `backend/src/bot/reward.rs`
- Test: `backend/src/bot/reward.rs`

- [ ] **Step 1: Implement bounded fan potential**

Return a small integer potential score from visible/concealed structure. Start with deterministic, cheap signals:

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FanPotential {
    pub value: i32,
}
```

Signals:

- `+2` if all non-honor tiles are in one suit and honors exist.
- `+3` if all non-honor tiles are in one suit and no other suit exists.
- `+1` for each dragon pair/triplet candidate.
- `+1` for each seat/round wind pair/triplet candidate when wind info is available.
- Clamp total to `0..6`.

- [ ] **Step 2: Add tests**

Assert mixed-suit cheap tenpai gets lower potential than one-suit honor-heavy hand. This prevents shaping from blindly rewarding any shanten decrease.

### Task 10: Compute Step Shaping Around Each Arena Action

**Files:**
- Modify: `backend/src/bot/arena.rs`
- Modify: `backend/src/bot/reward.rs`
- Test: `backend/src/bot/arena.rs`

- [ ] **Step 1: Extend trajectory row with diagnostics**

Add:

```rust
pub shanten_before: Option<i32>,
pub shanten_after: Option<i32>,
pub fan_potential_before: Option<i32>,
pub fan_potential_after: Option<i32>,
```

- [ ] **Step 2: Snapshot before and after action**

Before applying an action, compute the acting seat's shanten and fan potential from `trace.context`. After applying the action, compute the same metrics from the updated room state for that seat.

- [ ] **Step 3: Assign bounded shaping reward**

Use:

```rust
fn shaping_reward(before: RewardSnapshot, after: RewardSnapshot) -> f32 {
    let shanten_delta = before.shanten - after.shanten;
    let fan_delta = after.fan_potential - before.fan_potential;
    let shanten_reward = (shanten_delta.clamp(-1, 1) as f32) * 0.03;
    let fan_reward = (fan_delta.clamp(-1, 1) as f32) * 0.02;
    let tenpai_bonus = if before.shanten > 0 && after.shanten == 0 { 0.05 } else { 0.0 };
    shanten_reward + fan_reward + tenpai_bonus
}
```

Keep total step shaping small compared with terminal score reward.

- [ ] **Step 4: Add regression test**

Assert a discard that improves shanten produces positive `step_reward`, and a discard that worsens shanten produces non-positive `step_reward`.

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::arena bot::reward -- --nocapture
```

Expected: tests pass.

### Task 11: Train And Compare Three Checkpoints

**Files:**
- Modify: `backend/bot_trainer/v2/README.md`

- [ ] **Step 1: Baseline run**

Run current MLP PPO candidate before merging the ResNet/reward changes:

```powershell
.\backend\bot_trainer\v2\arena_matrix.ps1 -Matches 200 -Seed 20260429
```

- [ ] **Step 2: ResNet-only run**

Train/export/evaluate the ResNet model without shaping enabled.

- [ ] **Step 3: ResNet + fan-aware shanten shaping run**

Train/export/evaluate with shaping enabled.

- [ ] **Step 4: Acceptance criteria**

Accept the final candidate only if:

- average score delta improves over current production baseline
- win rate does not regress
- deal-in rate does not increase by more than 2 percentage points
- first-tenpai turn or final-tenpai rate improves or stays neutral
- average decision latency stays under 100 ms

## Final Verification

Run:

```powershell
python -m pytest backend/bot_trainer/v2 -q
cargo test --manifest-path backend/Cargo.toml bot::arena bot::reward bot::neural rules::standard::automation -- --nocapture
.\backend\bot_trainer\v2\train_rl_model.ps1 -OutputDir backend/bot_trainer/v2/rl_runs/final_smoke -TrajectoryMatches 2 -EvalMatches 2 -Epochs 1 -BatchSize 64 -Device cpu
```

Expected:

- Python tests pass.
- Rust tests pass.
- RL smoke writes a candidate checkpoint, candidate ONNX, and candidate evaluation summary.
- Arena reports include explicit `step_reward`, `terminal_reward`, `shanten_before`, `shanten_after`, `fan_potential_before`, and `fan_potential_after`.

## Risks And Controls

- PPO may still be off-policy if trajectories are generated by heuristic/neural deterministic choices. Control: use `--recompute-old-policy-stats` only for warm-start smoke, then add neural-backed stochastic rollout before judging strength.
- ResNet may initially underperform because old tile encoder weights cannot load. Control: warm-start compatible non-tile weights and compare supervised validation plus arena metrics.
- Shaping reward can create cheap tenpai bias. Control: keep weights small and require score delta/deal-in arena acceptance.
- Fan potential heuristic is intentionally coarse. Control: use it only as a bounded delta reward, not as a terminal objective.
