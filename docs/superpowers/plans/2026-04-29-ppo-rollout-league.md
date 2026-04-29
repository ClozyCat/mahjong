# PPO Rollout League Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the current Mahjong RL smoke pipeline into a defensible PPO training loop by adding stochastic neural rollouts, GAE/value clipping/KL regularization, opponent-pool training, and automatic candidate acceptance checks.

**Architecture:** Rust remains the authoritative game engine, legal-action provider, rollout generator, and arena evaluator. Python owns PPO math, trajectory filtering, opponent-pool config generation, checkpoint export, and candidate gating. Existing ONNX input/output contracts and split action heads stay unchanged.

**Tech Stack:** Rust backend (`backend/src/bot`, `backend/src/rules/standard`, `rand 0.9`, `ort`), Python trainer (`torch`, JSONL trajectory datasets), existing V2 feature schema (`tile_planes: 10 x 34`, `scalar_features: 10`), PowerShell/Bash training wrappers, arena JSONL summaries.

---

## Current Baseline

The repository already has these foundations:

- `backend/bot_trainer/v2/model.py` uses `SuitAwareTileResNet` with heads for discard, claim, self-kong, hu, value, and risk.
- `backend/src/bot/features.rs` and `backend/bot_trainer/v2/dataset.py` use stable `10 x 34` tile planes plus 10 scalar features.
- `backend/src/bot/arena.rs` exports trajectory rows with `log_prob`, `value`, `step_reward`, `terminal_reward`, and shanten/fan diagnostics.
- `backend/bot_trainer/v2/rl_train.py` has a PPO smoke trainer with masked log-probs, entropy decay, old-policy stat recomputation, and return-based value loss.
- `backend/bot_trainer/v2/arena_summary.py` summarizes average score, win rate, deal-in rate, tenpai, and latency.

The gaps this plan addresses are:

1. Arena rollouts are deterministic argmax decisions, so PPO is not cleanly on-policy.
2. PPO advantage computation uses discounted returns minus old values, not GAE.
3. PPO lacks value clipping and supervised-policy KL regularization.
4. Self-play uses a fixed policy layout instead of a sampled opponent pool with a trainable learner seat.
5. Candidate acceptance is documented but not enforced by a script.

## Non-Goals

- Do not replace the Rust rule engine with a Python environment.
- Do not change the ONNX runtime contract.
- Do not replace the current ResNet with a Transformer in this plan.
- Do not implement centralized critic training in the first execution pass. This plan adds only the small data-contract hooks needed to start that work safely in a separate pass.

## Implementation Order

1. Add stochastic neural rollout support in Rust while keeping production and evaluation deterministic by default.
2. Filter learner trajectories and compute GAE in Python.
3. Add value clipping and supervised-policy KL regularization to PPO.
4. Add opponent-pool config generation and learner-seat rotation.
5. Add candidate acceptance gate and wire it into the training wrapper.
6. Add a narrow centralized-critic schema probe without using it in PPO yet.

## File Map

- Modify: `backend/src/bot/arena.rs`
  - Add rollout sampling fields to arena policy config, seed a per-match rollout RNG, and preserve deterministic evaluation defaults.
- Modify: `backend/src/bot/policy.rs`
  - Add stochastic neural action-selection entry points for active turns, claims, and self-kongs.
- Modify: `backend/src/rules/standard/automation.rs`
  - Route arena trace decisions through the stochastic policy entry point when the policy config enables sampling.
- Modify: `backend/bot_trainer/v2/rl_dataset.py`
  - Filter trajectories by learner policy id and compute GAE-ready tensors.
- Modify: `backend/bot_trainer/v2/rl_train.py`
  - Use precomputed advantages, value clipping, and optional KL against the supervised baseline.
- Modify: `backend/bot_trainer/v2/test_rl_dataset.py`
  - Add tests for policy-id filtering, GAE, value clipping, and masked KL helpers.
- Create: `backend/bot_trainer/v2/opponent_pool.json`
  - Store heuristic, production neural, and historical candidate opponent definitions.
- Create: `backend/bot_trainer/v2/league_config.py`
  - Generate arena configs for learner-seat rotation and candidate evaluation.
- Create: `backend/bot_trainer/v2/candidate_gate.py`
  - Enforce candidate acceptance criteria from arena summaries.
- Modify: `backend/bot_trainer/v2/train_rl_model.ps1`
  - Use league config generation, learner-only PPO rows, and candidate gate.
- Modify: `backend/bot_trainer/v2/train_rl_model.sh`
  - Mirror the PowerShell wrapper behavior on Linux.
- Modify: `backend/bot_trainer/v2/README.md`
  - Document stochastic rollout, opponent pool, GAE/KL knobs, and acceptance gate commands.

## Phase 1: Stochastic Neural Rollouts

### Task 1: Add Sampling Fields To Arena Policy Config

**Files:**
- Modify: `backend/src/bot/arena.rs`
- Test: `backend/src/bot/arena.rs`

- [ ] **Step 1: Add config fields with deterministic defaults**

Update `ArenaBotPolicyConfig`:

```rust
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ArenaBotPolicyConfig {
    pub id: String,
    pub mode: ArenaPolicyMode,
    pub neural_weight: i64,
    pub model_path: Option<String>,
    #[serde(default)]
    pub sample_actions: bool,
    #[serde(default = "default_policy_temperature")]
    pub temperature: f32,
}

fn default_policy_temperature() -> f32 {
    1.0
}
```

Because `ArenaBotPolicyConfig` now contains `f32`, remove `Eq` from `ArenaConfig` as well:

```rust
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ArenaConfig {
    pub matches: usize,
    pub seed: u64,
    #[serde(default = "default_max_actions_per_match")]
    pub max_actions_per_match: usize,
    #[serde(default)]
    pub report_trajectories: bool,
    pub policies: Vec<ArenaBotPolicyConfig>,
}
```

Update `ArenaBotPolicyConfig::heuristic()`:

```rust
impl ArenaBotPolicyConfig {
    pub fn heuristic() -> Self {
        Self {
            id: "heuristic".to_string(),
            mode: ArenaPolicyMode::Heuristic,
            neural_weight: 0,
            model_path: None,
            sample_actions: false,
            temperature: 1.0,
        }
    }
}
```

- [ ] **Step 2: Update config parsing tests**

Add this assertion to `arena_config_parses_policy_ids`:

```rust
assert!(!config.policies[0].sample_actions);
assert_eq!(config.policies[0].temperature, 1.0);
```

Add a new test:

```rust
#[test]
fn arena_config_parses_stochastic_neural_rollout_policy() {
    let raw = r#"{
        "matches": 1,
        "seed": 20260429,
        "policies": [
            {
                "id":"learner",
                "mode":"neural",
                "neural_weight":0,
                "model_path":"backend/assets/models/mahjong_policy_net.onnx",
                "sample_actions":true,
                "temperature":0.8
            }
        ]
    }"#;

    let config: ArenaConfig = serde_json::from_str(raw).expect("config");

    assert_eq!(config.policies[0].id, "learner");
    assert!(config.policies[0].sample_actions);
    assert_eq!(config.policies[0].temperature, 0.8);
}
```

- [ ] **Step 3: Run the targeted Rust test**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::arena::tests::arena_config_parses -- --nocapture
```

Expected: both arena config parsing tests pass.

- [ ] **Step 4: Commit**

```powershell
git add backend/src/bot/arena.rs
git commit -m "feat(rl): 支持竞技场随机采样配置"
```

### Task 2: Add Masked Sampling Helpers

**Files:**
- Modify: `backend/src/bot/policy.rs`
- Test: `backend/src/bot/policy.rs`

- [ ] **Step 1: Import seeded RNG traits**

Add imports near the top of `policy.rs`:

```rust
use rand::{Rng, rngs::StdRng};
```

- [ ] **Step 2: Add a stable masked sampler**

Add this helper near the neural selection helpers:

```rust
fn sample_masked_index<const N: usize>(
    logits: &[f32; N],
    mask: &[bool; N],
    temperature: f32,
    rng: &mut StdRng,
) -> Option<usize> {
    let temperature = temperature.clamp(0.05, 5.0);
    let max_logit = logits
        .iter()
        .zip(mask.iter())
        .filter_map(|(logit, allowed)| (*allowed && logit.is_finite()).then_some(*logit))
        .max_by(f32::total_cmp)?;
    let mut weights = [0.0_f32; N];
    let mut total = 0.0_f32;
    for (index, (logit, allowed)) in logits.iter().zip(mask.iter()).enumerate() {
        if !*allowed || !logit.is_finite() {
            continue;
        }
        let weight = ((*logit - max_logit) / temperature).exp();
        if weight.is_finite() && weight > 0.0 {
            weights[index] = weight;
            total += weight;
        }
    }
    if total <= 0.0 || !total.is_finite() {
        return None;
    }
    let mut threshold = rng.random_range(0.0..total);
    for (index, weight) in weights.iter().enumerate() {
        threshold -= *weight;
        if threshold <= 0.0 {
            return Some(index);
        }
    }
    weights.iter().rposition(|weight| *weight > 0.0)
}
```

- [ ] **Step 3: Add deterministic tests for the sampler**

Add this test:

```rust
#[test]
fn sample_masked_index_never_selects_illegal_action() {
    use rand::SeedableRng;

    let logits = [100.0_f32, 1.0, 2.0];
    let mask = [false, true, true];
    let mut rng = rand::rngs::StdRng::seed_from_u64(7);

    for _ in 0..64 {
        let selected = sample_masked_index(&logits, &mask, 1.0, &mut rng)
            .expect("sample should exist");
        assert_ne!(selected, 0);
    }
}
```

- [ ] **Step 4: Run the sampler test**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::policy::tests::sample_masked_index_never_selects_illegal_action -- --nocapture
```

Expected: test passes.

- [ ] **Step 5: Commit**

```powershell
git add backend/src/bot/policy.rs
git commit -m "feat(rl): 增加合法动作随机采样"
```

### Task 3: Route Arena Decisions Through Stochastic Neural Selection

**Files:**
- Modify: `backend/src/bot/policy.rs`
- Modify: `backend/src/rules/standard/automation.rs`
- Modify: `backend/src/bot/arena.rs`
- Test: `backend/src/bot/policy.rs`
- Test: `backend/src/bot/arena.rs`

- [ ] **Step 1: Add stochastic policy entry points**

Add public crate-visible wrappers in `policy.rs`:

```rust
pub(crate) fn choose_active_turn_action_with_config_and_rng(
    context: &BotContext,
    config: &ArenaBotPolicyConfig,
    rng: Option<&mut StdRng>,
) -> Option<BotAction> {
    if config.sample_actions {
        if let Some(rng) = rng {
            if matches!(config.mode, ArenaPolicyMode::Neural) {
                if let Some(scores) = neural_decision_scores_for_policy(context, config) {
                    if let Some(action) =
                        sample_neural_active_turn_action(context, &scores, config.temperature, rng)
                    {
                        return Some(action);
                    }
                }
            }
        }
    }
    choose_active_turn_action_with_config(context, config)
}

pub(crate) fn choose_claim_action_with_config_and_rng(
    context: &BotContext,
    config: &ArenaBotPolicyConfig,
    rng: Option<&mut StdRng>,
) -> Option<BotAction> {
    if config.sample_actions {
        if let Some(rng) = rng {
            if matches!(config.mode, ArenaPolicyMode::Neural) {
                if let Some(scores) = neural_decision_scores_for_policy(context, config) {
                    if let Some(action) =
                        sample_neural_claim_action(context, &scores, config.temperature, rng)
                    {
                        return Some(action);
                    }
                }
            }
        }
    }
    choose_claim_action_with_config(context, config)
}
```

- [ ] **Step 2: Add active-turn sampling implementation**

Add this helper in `policy.rs`:

```rust
fn sample_neural_active_turn_action(
    context: &BotContext,
    scores: &NeuralDecisionScores,
    temperature: f32,
    rng: &mut StdRng,
) -> Option<BotAction> {
    let features = crate::bot::features::encode_bot_context_v2(context);
    if context.self_kong_candidates.is_empty() {
        return sample_neural_discard_action(context, scores, temperature, rng);
    }

    let selected = sample_masked_index(
        &scores.self_kong_logits,
        &features.self_kong_mask,
        temperature,
        rng,
    )?;
    match selected {
        0 => sample_neural_discard_action(context, scores, temperature, rng),
        1 | 2 => {
            let expected_kind = if selected == 1 {
                BotSelfKongKind::Concealed
            } else {
                BotSelfKongKind::Add
            };
            context
                .self_kong_candidates
                .iter()
                .find(|candidate| {
                    candidate.kind == expected_kind
                        && !(candidate.kind == BotSelfKongKind::Add
                            && context.add_kong_risk_tiles.contains(&candidate.tile_key))
                })
                .map(|candidate| BotAction {
                    seat_index: context.seat_index,
                    action_type: "kong".to_string(),
                    tile_ids: candidate.tile_ids.clone(),
                })
                .or_else(|| sample_neural_discard_action(context, scores, temperature, rng))
        }
        _ => sample_neural_discard_action(context, scores, temperature, rng),
    }
}

fn sample_neural_discard_action(
    context: &BotContext,
    scores: &NeuralDecisionScores,
    temperature: f32,
    rng: &mut StdRng,
) -> Option<BotAction> {
    let features = crate::bot::features::encode_bot_context_v2(context);
    let tile_index = sample_masked_index(
        &scores.discard_logits,
        &features.discard_mask,
        temperature,
        rng,
    )?;
    let tile_key = crate::bot::context::tile_key_for_index(tile_index);
    let tile_id = context
        .player
        .concealed_tiles
        .iter()
        .find(|tile| !tile.is_flower && tile.tile_key == tile_key)
        .map(|tile| tile.tile_id.clone())?;
    Some(BotAction {
        seat_index: context.seat_index,
        action_type: "discard".to_string(),
        tile_ids: vec![tile_id],
    })
}
```

- [ ] **Step 3: Add claim sampling implementation**

Add this helper in `policy.rs`:

```rust
fn sample_neural_claim_action(
    context: &BotContext,
    scores: &NeuralDecisionScores,
    temperature: f32,
    rng: &mut StdRng,
) -> Option<BotAction> {
    let features = crate::bot::features::encode_bot_context_v2(context);
    let selected = sample_masked_index(
        &scores.claim_logits,
        &features.claim_mask,
        temperature,
        rng,
    )?;
    let action_name = crate::bot::action_space::CLAIM_ACTIONS.get(selected)?;
    if *action_name == "pass" {
        return Some(pass_action(context.seat_index));
    }
    let option = claim_option_for_ranked_action(context, action_name)?;
    Some(BotAction {
        seat_index: context.seat_index,
        action_type: option.action_type.clone(),
        tile_ids: option.tile_ids.clone(),
    })
}
```

- [ ] **Step 4: Seed rollout RNG in arena**

In `backend/src/bot/arena.rs`, import:

```rust
use rand::{SeedableRng, rngs::StdRng};
```

In `run_arena_match`, add after `action_count` initialization:

```rust
let mut rollout_rng = StdRng::seed_from_u64(seed ^ 0xA17E_5EED);
```

- [ ] **Step 5: Pass RNG through automation trace**

Change the trace function signature in `automation.rs`:

```rust
pub(crate) fn next_bot_decision_trace_in_room_state_with_policy_resolver(
    room: &RoomState,
    policy_for_seat: BotPolicyResolver<'_>,
    rollout_rng: Option<&mut rand::rngs::StdRng>,
) -> Result<Option<BotDecisionTrace>, String> {
    Ok(next_bot_decision_trace_for_state_with_policy_resolver(
        room,
        policy_for_seat,
        rollout_rng,
    ))
}
```

Change the private helper signature in the same way, and replace:

```rust
let action = bot::choose_active_turn_action_with_config(&context, &policy_config)?;
```

with:

```rust
let action = bot::policy::choose_active_turn_action_with_config_and_rng(
    &context,
    &policy_config,
    rollout_rng,
)?;
```

Replace:

```rust
let action = bot::choose_claim_action_with_config(&context, &policy_config)?;
```

with:

```rust
let action = bot::policy::choose_claim_action_with_config_and_rng(
    &context,
    &policy_config,
    rollout_rng,
)?;
```

In `arena.rs`, update the trace call:

```rust
next_bot_decision_trace_in_room_state_with_policy_resolver(&room, &|seat| {
    policy_for_seat(config, seat)
}, Some(&mut rollout_rng))
```

- [ ] **Step 6: Run targeted tests**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::policy bot::arena rules::standard::automation -- --nocapture
```

Expected: all targeted tests pass.

- [ ] **Step 7: Commit**

```powershell
git add backend/src/bot/policy.rs backend/src/rules/standard/automation.rs backend/src/bot/arena.rs
git commit -m "feat(rl): 竞技场轨迹支持神经策略采样"
```

## Phase 2: GAE And PPO Stabilization

### Task 4: Filter Learner Rows And Compute GAE

**Files:**
- Modify: `backend/bot_trainer/v2/rl_dataset.py`
- Modify: `backend/bot_trainer/v2/test_rl_dataset.py`

- [ ] **Step 1: Add a policy-id filtered dataset test**

Add this test:

```python
def test_dataset_filters_policy_id(tmp_path: Path) -> None:
    path = tmp_path / "trajectories.jsonl"
    rows = [
        base_trajectory_row("learner", 0, reward=1.0, value=0.2),
        base_trajectory_row("opponent", 1, reward=9.0, value=0.0),
    ]
    path.write_text("\n".join(json.dumps(row) for row in rows) + "\n", encoding="utf-8")

    dataset = ArenaTrajectoryDataset(path, policy_id="learner")

    assert len(dataset) == 1
    assert dataset[0]["reward"].item() == 1.0
```

Add this helper in the test file:

```python
def base_trajectory_row(
    policy_id: str,
    seat_index: int,
    reward: float,
    value: float,
    done: bool = True,
) -> dict[str, object]:
    return {
        "schema_version": 1,
        "match_id": "m1",
        "decision_index": seat_index,
        "seat_index": seat_index,
        "policy_id": policy_id,
        "decision_kind": "active_turn",
        "tile_planes": [0.0] * 340,
        "scalar_features": [0.0] * 10,
        "discard_mask": [True] + [False] * 33,
        "claim_mask": [True] + [False] * 6,
        "self_kong_mask": [True, False, False],
        "hu_mask": [True, False],
        "action_head": "discard",
        "action_index": 0,
        "action_semantic": "discard:w1",
        "log_prob": -0.3,
        "value": value,
        "reward": reward,
        "step_reward": 0.0,
        "terminal_reward": reward,
        "shanten_before": None,
        "shanten_after": None,
        "fan_potential_before": None,
        "fan_potential_after": None,
        "done": done,
    }
```

- [ ] **Step 2: Add GAE computation test**

Add this test:

```python
def test_compute_gae_for_rows_is_per_seat_episode() -> None:
    rows = [
        {"match_id": "m1", "seat_index": 0, "reward": 0.0, "value": 0.5},
        {"match_id": "m1", "seat_index": 1, "reward": 5.0, "value": 0.0},
        {"match_id": "m1", "seat_index": 0, "reward": 1.0, "value": 0.25},
    ]

    advantages, returns = compute_gae_for_rows(rows, gamma=1.0, gae_lambda=1.0)

    assert advantages == [0.5, 5.0, 0.75]
    assert returns == [1.0, 5.0, 1.0]
```

- [ ] **Step 3: Implement filtering and GAE**

Change `ArenaTrajectoryDataset.__init__`:

```python
class ArenaTrajectoryDataset(Dataset):
    def __init__(
        self,
        path: Path,
        gamma: float = 0.99,
        gae_lambda: float = 0.95,
        policy_id: str | None = None,
    ) -> None:
        rows = [
            json.loads(line)
            for line in path.read_text(encoding="utf-8").splitlines()
            if line.strip()
        ]
        if policy_id is not None:
            rows = [row for row in rows if row.get("policy_id") == policy_id]
        self.rows = rows
        self.advantages, self.returns = compute_gae_for_rows(
            self.rows,
            gamma=gamma,
            gae_lambda=gae_lambda,
        )
```

Add:

```python
def compute_gae_for_rows(
    rows: list[dict[str, Any]],
    gamma: float,
    gae_lambda: float,
) -> tuple[list[float], list[float]]:
    advantages = [0.0 for _ in rows]
    returns = [0.0 for _ in rows]
    groups: dict[tuple[str, int], list[int]] = {}
    for index, row in enumerate(rows):
        key = (str(row["match_id"]), int(row["seat_index"]))
        groups.setdefault(key, []).append(index)

    for indices in groups.values():
        running_advantage = 0.0
        next_value = 0.0
        for index in reversed(indices):
            reward = float(rows[index]["reward"])
            value = float(rows[index].get("value", 0.0))
            delta = reward + gamma * next_value - value
            running_advantage = delta + gamma * gae_lambda * running_advantage
            advantages[index] = round(running_advantage, 6)
            returns[index] = round(value + running_advantage, 6)
            next_value = value
    return advantages, returns
```

In `encode_row`, add:

```python
"advantage": torch.tensor(row["advantage"], dtype=torch.float32),
```

Before calling `encode_row`, set the fields:

```python
def __getitem__(self, index: int) -> dict[str, torch.Tensor]:
    row = dict(self.rows[index])
    row["advantage"] = self.advantages[index]
    return encode_row(row, self.returns[index])
```

- [ ] **Step 4: Run dataset tests**

Run:

```powershell
python -m pytest backend/bot_trainer/v2/test_rl_dataset.py -q
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```powershell
git add backend/bot_trainer/v2/rl_dataset.py backend/bot_trainer/v2/test_rl_dataset.py
git commit -m "feat(rl): 使用GAE计算优势"
```

### Task 5: Add Value Clipping And Supervised KL

**Files:**
- Modify: `backend/bot_trainer/v2/rl_train.py`
- Modify: `backend/bot_trainer/v2/test_rl_dataset.py`

- [ ] **Step 1: Add tests for value clipping and KL**

Add:

```python
def test_clipped_value_loss_uses_larger_loss() -> None:
    import torch
    from rl_train import clipped_value_loss

    values = torch.tensor([3.0])
    old_values = torch.tensor([0.0])
    returns = torch.tensor([1.0])

    loss = clipped_value_loss(values, old_values, returns, clip_epsilon=0.2)

    assert loss.item() == 4.0
```

Add:

```python
def test_masked_categorical_kl_is_finite() -> None:
    import torch
    from rl_train import masked_categorical_kl

    teacher_logits = torch.tensor([[1.0, 0.0, 99.0]])
    student_logits = torch.tensor([[0.5, 0.2, -99.0]])
    mask = torch.tensor([[True, True, False]])

    kl = masked_categorical_kl(teacher_logits, student_logits, mask)

    assert torch.isfinite(kl)
    assert kl.item() >= 0.0
```

- [ ] **Step 2: Add PPO arguments**

In `parse_args()`:

```python
parser.add_argument("--gae-lambda", type=float, default=0.95)
parser.add_argument("--policy-id", default=None)
parser.add_argument("--value-clip-epsilon", type=float, default=0.2)
parser.add_argument("--kl-coef", type=float, default=0.01)
parser.add_argument("--kl-end-coef", type=float, default=0.0)
```

- [ ] **Step 3: Add value clipping helper**

Add:

```python
def clipped_value_loss(
    values: torch.Tensor,
    old_values: torch.Tensor,
    returns: torch.Tensor,
    clip_epsilon: float,
) -> torch.Tensor:
    clipped = old_values + (values - old_values).clamp(-clip_epsilon, clip_epsilon)
    unclipped_loss = (values - returns).pow(2)
    clipped_loss = (clipped - returns).pow(2)
    return torch.maximum(unclipped_loss, clipped_loss).mean()
```

- [ ] **Step 4: Add masked KL helper**

Add:

```python
def masked_categorical_kl(
    teacher_logits: torch.Tensor,
    student_logits: torch.Tensor,
    mask: torch.Tensor,
) -> torch.Tensor:
    teacher_masked = teacher_logits.masked_fill(~mask.bool(), -1.0e4)
    student_masked = student_logits.masked_fill(~mask.bool(), -1.0e4)
    teacher_log_probs = F.log_softmax(teacher_masked, dim=1)
    student_log_probs = F.log_softmax(student_masked, dim=1)
    teacher_probs = teacher_log_probs.exp()
    return (teacher_probs * (teacher_log_probs - student_log_probs)).sum(dim=1).mean()
```

Add active-head KL:

```python
def select_action_head_kl(
    teacher_outputs: dict[str, torch.Tensor],
    student_outputs: dict[str, torch.Tensor],
    batch: dict[str, torch.Tensor],
) -> torch.Tensor:
    result = torch.zeros((), device=batch["reward"].device)
    count = 0
    heads = [
        (0, "discard_logits", "discard_mask"),
        (1, "claim_logits", "claim_mask"),
        (2, "self_kong_logits", "self_kong_mask"),
        (3, "hu_logits", "hu_mask"),
    ]
    for head_index, logits_key, mask_key in heads:
        active = batch["action_head"] == head_index
        if torch.any(active):
            result = result + masked_categorical_kl(
                teacher_outputs[logits_key][active],
                student_outputs[logits_key][active],
                batch[mask_key][active],
            )
            count += 1
    return result / max(count, 1)
```

- [ ] **Step 5: Use GAE, value clipping, and KL in training**

Create the dataset with the new args:

```python
dataset = ArenaTrajectoryDataset(
    args.trajectories,
    gamma=args.gamma,
    gae_lambda=args.gae_lambda,
    policy_id=args.policy_id,
)
```

Build a frozen supervised model when `args.kl_coef > 0.0`:

```python
teacher_model = build_old_policy_model(args.checkpoint, device) if args.kl_coef > 0.0 else None
```

Replace advantage computation:

```python
advantages = batch["advantage"].float()
advantages = (advantages - advantages.mean()) / (
    advantages.std(unbiased=False) + 1.0e-8
)
```

Replace value loss:

```python
value_loss = clipped_value_loss(
    values,
    old_values,
    returns,
    args.value_clip_epsilon,
)
```

Add KL:

```python
kl_loss = torch.zeros((), device=device)
if teacher_model is not None:
    with torch.no_grad():
        teacher_outputs = teacher_model(
            batch["tile_planes"].float(),
            batch["scalar_features"].float(),
        )
    kl_loss = select_action_head_kl(teacher_outputs, outputs, batch)

kl_coef = entropy_coef_for_progress(
    global_step,
    entropy_decay_steps,
    args.kl_coef,
    args.kl_end_coef,
)
loss = policy_loss + 0.5 * value_loss - entropy_coef * entropy + kl_coef * kl_loss
```

Record `kl_loss` and `kl_coef` in metrics with the same pattern as entropy.

- [ ] **Step 6: Run PPO helper tests**

Run:

```powershell
python -m pytest backend/bot_trainer/v2/test_rl_dataset.py -q
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```powershell
git add backend/bot_trainer/v2/rl_train.py backend/bot_trainer/v2/test_rl_dataset.py
git commit -m "feat(rl): 增加GAE价值裁剪和监督KL"
```

## Phase 3: Opponent Pool And Learner Seat Rotation

### Task 6: Add Opponent Pool Config And Arena Config Generator

**Files:**
- Create: `backend/bot_trainer/v2/opponent_pool.json`
- Create: `backend/bot_trainer/v2/league_config.py`
- Modify: `backend/bot_trainer/v2/test_rl_dataset.py`

- [ ] **Step 1: Create the initial opponent pool**

Create `backend/bot_trainer/v2/opponent_pool.json`:

```json
{
  "schema_version": 1,
  "learner": {
    "id": "learner",
    "mode": "neural",
    "neural_weight": 0,
    "model_path": "backend/assets/models/mahjong_policy_net.onnx",
    "sample_actions": true,
    "temperature": 1.0
  },
  "opponents": [
    {
      "id": "heuristic",
      "mode": "heuristic",
      "neural_weight": 0,
      "model_path": null,
      "sample_actions": false,
      "temperature": 1.0,
      "weight": 3
    },
    {
      "id": "production_neural",
      "mode": "neural",
      "neural_weight": 0,
      "model_path": "backend/assets/models/mahjong_policy_net.onnx",
      "sample_actions": false,
      "temperature": 1.0,
      "weight": 2
    }
  ]
}
```

- [ ] **Step 2: Add generator tests**

Add:

```python
def test_league_config_rotates_learner_seat(tmp_path: Path) -> None:
    from league_config import build_trajectory_configs

    pool = {
        "learner": {
            "id": "learner",
            "mode": "neural",
            "neural_weight": 0,
            "model_path": "candidate.onnx",
            "sample_actions": True,
            "temperature": 1.0,
        },
        "opponents": [
            {
                "id": "heuristic",
                "mode": "heuristic",
                "neural_weight": 0,
                "model_path": None,
                "sample_actions": False,
                "temperature": 1.0,
                "weight": 1,
            }
        ],
    }

    configs = build_trajectory_configs(pool, matches=8, seed=10, max_actions=2400)

    assert len(configs) == 4
    assert [config["policies"].index(pool["learner"]) for config in configs] == [0, 1, 2, 3]
    assert all(config["matches"] == 2 for config in configs)
```

- [ ] **Step 3: Implement config generation**

Create `league_config.py`:

```python
from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--pool", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--matches", type=int, required=True)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--max-actions", type=int, default=2400)
    parser.add_argument("--mode", choices=["trajectory", "eval"], default="trajectory")
    parser.add_argument("--candidate-onnx", type=Path, default=None)
    parser.add_argument("--baseline-onnx", type=Path, default=Path("backend/assets/models/mahjong_policy_net.onnx"))
    return parser.parse_args()


def load_pool(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def weighted_opponents(pool: dict[str, Any]) -> list[dict[str, Any]]:
    opponents: list[dict[str, Any]] = []
    for opponent in pool["opponents"]:
        weight = max(1, int(opponent.get("weight", 1)))
        clean = {key: value for key, value in opponent.items() if key != "weight"}
        opponents.extend(clean for _ in range(weight))
    return opponents


def build_trajectory_configs(
    pool: dict[str, Any],
    matches: int,
    seed: int,
    max_actions: int,
) -> list[dict[str, Any]]:
    learner = pool["learner"]
    opponents = weighted_opponents(pool)
    matches_per_config = max(1, matches // 4)
    configs = []
    for learner_seat in range(4):
        policies = []
        opponent_index = learner_seat
        for seat in range(4):
            if seat == learner_seat:
                policies.append(learner)
            else:
                policies.append(opponents[opponent_index % len(opponents)])
                opponent_index += 1
        configs.append(
            {
                "matches": matches_per_config,
                "seed": seed + learner_seat * 100000,
                "max_actions_per_match": max_actions,
                "report_trajectories": True,
                "policies": policies,
            }
        )
    return configs


def build_eval_config(
    candidate_onnx: Path,
    baseline_onnx: Path,
    matches: int,
    seed: int,
    max_actions: int,
) -> dict[str, Any]:
    return {
        "matches": matches,
        "seed": seed,
        "max_actions_per_match": max_actions,
        "report_trajectories": False,
        "policies": [
            {
                "id": "baseline_neural",
                "mode": "neural",
                "neural_weight": 0,
                "model_path": str(baseline_onnx),
                "sample_actions": False,
                "temperature": 1.0,
            },
            {
                "id": "rl_candidate_neural",
                "mode": "neural",
                "neural_weight": 0,
                "model_path": str(candidate_onnx),
                "sample_actions": False,
                "temperature": 1.0,
            },
            {
                "id": "heuristic",
                "mode": "heuristic",
                "neural_weight": 0,
                "model_path": None,
                "sample_actions": False,
                "temperature": 1.0,
            },
            {
                "id": "heuristic",
                "mode": "heuristic",
                "neural_weight": 0,
                "model_path": None,
                "sample_actions": False,
                "temperature": 1.0,
            },
        ],
    }


def write_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")


def main() -> None:
    args = parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    if args.mode == "trajectory":
        pool = load_pool(args.pool)
        for index, config in enumerate(
            build_trajectory_configs(pool, args.matches, args.seed, args.max_actions)
        ):
            write_json(args.output_dir / f"trajectory_config_{index}.json", config)
    else:
        if args.candidate_onnx is None:
            raise SystemExit("--candidate-onnx is required for eval mode")
        write_json(
            args.output_dir / "candidate_eval_config.json",
            build_eval_config(
                args.candidate_onnx,
                args.baseline_onnx,
                args.matches,
                args.seed,
                args.max_actions,
            ),
        )


if __name__ == "__main__":
    main()
```

- [ ] **Step 4: Run generator tests**

Run:

```powershell
python -m pytest backend/bot_trainer/v2/test_rl_dataset.py -q
python backend/bot_trainer/v2/league_config.py --pool backend/bot_trainer/v2/opponent_pool.json --output-dir backend/bot_trainer/v2/rl_runs/config_smoke --matches 8 --seed 20260429 --mode trajectory
```

Expected:

- tests pass
- files `trajectory_config_0.json` through `trajectory_config_3.json` exist
- each config contains exactly one `learner` policy

- [ ] **Step 5: Commit**

```powershell
git add backend/bot_trainer/v2/opponent_pool.json backend/bot_trainer/v2/league_config.py backend/bot_trainer/v2/test_rl_dataset.py
git commit -m "feat(rl): 增加对手池配置生成"
```

### Task 7: Wire Opponent Pool Into Training Wrappers

**Files:**
- Modify: `backend/bot_trainer/v2/train_rl_model.ps1`
- Modify: `backend/bot_trainer/v2/train_rl_model.sh`
- Modify: `backend/bot_trainer/v2/README.md`

- [ ] **Step 1: Add PowerShell parameters**

Add parameters to `train_rl_model.ps1`:

```powershell
[string]$OpponentPool = "backend/bot_trainer/v2/opponent_pool.json",
[string]$LearnerPolicyId = "learner",
[double]$GaeLambda = 0.95,
[double]$KlCoef = 0.01,
[double]$KlEndCoef = 0.0,
[double]$ValueClipEpsilon = 0.2,
```

- [ ] **Step 2: Generate trajectory configs in PowerShell**

Before trajectory generation, add:

```powershell
$TrajectoryConfigDir = Join-Path $OutputDir "trajectory_configs"
New-Item -ItemType Directory -Force -Path $TrajectoryConfigDir | Out-Null
Invoke-TrainingPython @(
    "backend/bot_trainer/v2/league_config.py",
    "--pool", $OpponentPool,
    "--output-dir", $TrajectoryConfigDir,
    "--matches", "$TrajectoryMatches",
    "--seed", "$Seed",
    "--max-actions", "$MaxActionsPerMatch",
    "--mode", "trajectory"
)
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
```

Replace the single arena run with a loop:

```powershell
$trajectoryFiles = @()
Get-ChildItem -LiteralPath $TrajectoryConfigDir -Filter "trajectory_config_*.json" |
    Sort-Object Name |
    ForEach-Object {
        $index = [System.IO.Path]::GetFileNameWithoutExtension($_.Name).Replace("trajectory_config_", "")
        $partialReport = Join-Path $OutputDir "trajectory_arena_report_$index.jsonl"
        $partialTrajectory = Join-Path $OutputDir "trajectories_$index.jsonl"
        $trajectoryFiles += $partialTrajectory
        $arenaArgs = @(
            "run",
            "--manifest-path", "backend/Cargo.toml",
            "--release",
            "--bin", "bot_arena",
            "--",
            "--config", $_.FullName,
            "--output", $partialReport,
            "--trajectories", $partialTrajectory,
            "--jobs", "$ArenaJobs"
        )
        if ($TrajectoryProgressEvery -gt 0) {
            $arenaArgs += @("--progress-every", "$TrajectoryProgressEvery")
        }
        & $CargoExe @arenaArgs
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }

Get-Content -LiteralPath $trajectoryFiles | Set-Content -Encoding UTF8 $TrajectoryJsonl
```

- [ ] **Step 3: Pass learner and PPO stabilization args**

Append to `$rlTrainArgs`:

```powershell
"--policy-id", $LearnerPolicyId,
"--gae-lambda", "$GaeLambda",
"--value-clip-epsilon", "$ValueClipEpsilon",
"--kl-coef", "$KlCoef",
"--kl-end-coef", "$KlEndCoef",
```

- [ ] **Step 4: Mirror the same CLI flags in Bash**

Add shell variables near defaults in `train_rl_model.sh`:

```bash
OPPONENT_POOL="backend/bot_trainer/v2/opponent_pool.json"
LEARNER_POLICY_ID="learner"
GAE_LAMBDA="0.95"
KL_COEF="0.01"
KL_END_COEF="0.0"
VALUE_CLIP_EPSILON="0.2"
```

Add matching argument parsing flags:

```bash
--opponent-pool) OPPONENT_POOL="$2"; shift 2 ;;
--learner-policy-id) LEARNER_POLICY_ID="$2"; shift 2 ;;
--gae-lambda) GAE_LAMBDA="$2"; shift 2 ;;
--kl-coef) KL_COEF="$2"; shift 2 ;;
--kl-end-coef) KL_END_COEF="$2"; shift 2 ;;
--value-clip-epsilon) VALUE_CLIP_EPSILON="$2"; shift 2 ;;
```

Use `league_config.py` and loop over `trajectory_config_*.json` with the same file naming as PowerShell. Pass the same `rl_train.py` flags.

- [ ] **Step 5: Document the new training command**

Add to README:

```markdown
### PPO League Training

```powershell
.\backend\bot_trainer\v2\train_rl_model.ps1 `
  -OutputDir backend/bot_trainer/v2/rl_runs/league_smoke `
  -TrajectoryMatches 8 `
  -EvalMatches 4 `
  -Epochs 1 `
  -BatchSize 64 `
  -Device cpu `
  -LearnerPolicyId learner `
  -GaeLambda 0.95 `
  -KlCoef 0.01
```

The generated trajectory configs rotate the sampled `learner` policy through all four seats and fill the other seats from `opponent_pool.json`. PPO filters rows by `policy_id=learner`, so frozen opponents do not train the learner.
```

- [ ] **Step 6: Run wrapper smoke**

Run:

```powershell
.\backend\bot_trainer\v2\train_rl_model.ps1 -OutputDir backend/bot_trainer/v2/rl_runs/league_smoke -TrajectoryMatches 8 -EvalMatches 4 -Epochs 1 -BatchSize 64 -Device cpu -SkipTests
```

Expected:

- four trajectory configs are generated
- combined `trajectories.jsonl` contains rows with `policy_id="learner"`
- `rl_train.py` logs PPO metrics with `kl_loss` and `kl_coef`
- candidate ONNX and evaluation summary are written

- [ ] **Step 7: Commit**

```powershell
git add backend/bot_trainer/v2/train_rl_model.ps1 backend/bot_trainer/v2/train_rl_model.sh backend/bot_trainer/v2/README.md
git commit -m "feat(rl): 对接对手池训练流程"
```

## Phase 4: Candidate Acceptance Gate

### Task 8: Add Candidate Gate Script

**Files:**
- Create: `backend/bot_trainer/v2/candidate_gate.py`
- Modify: `backend/bot_trainer/v2/test_rl_dataset.py`

- [ ] **Step 1: Add acceptance tests**

Add:

```python
def test_candidate_gate_accepts_safe_improvement() -> None:
    from candidate_gate import evaluate_candidate

    summary = {
        "policies": {
            "baseline_neural": {
                "avg_score_delta": 0.0,
                "win_rate": 0.20,
                "deal_in_rate": 0.10,
                "avg_first_tenpai_turn": 8.0,
                "final_tenpai_rate": 0.55,
                "avg_latency_ms_per_decision": 20.0,
            },
            "rl_candidate_neural": {
                "avg_score_delta": 1.5,
                "win_rate": 0.21,
                "deal_in_rate": 0.11,
                "avg_first_tenpai_turn": 7.8,
                "final_tenpai_rate": 0.55,
                "avg_latency_ms_per_decision": 22.0,
            },
        }
    }

    result = evaluate_candidate(summary, "baseline_neural", "rl_candidate_neural")

    assert result["accepted"] is True
```

Add:

```python
def test_candidate_gate_rejects_higher_deal_in() -> None:
    from candidate_gate import evaluate_candidate

    summary = {
        "policies": {
            "baseline_neural": {
                "avg_score_delta": 0.0,
                "win_rate": 0.20,
                "deal_in_rate": 0.10,
                "avg_first_tenpai_turn": 8.0,
                "final_tenpai_rate": 0.55,
                "avg_latency_ms_per_decision": 20.0,
            },
            "rl_candidate_neural": {
                "avg_score_delta": 2.0,
                "win_rate": 0.22,
                "deal_in_rate": 0.14,
                "avg_first_tenpai_turn": 7.7,
                "final_tenpai_rate": 0.56,
                "avg_latency_ms_per_decision": 23.0,
            },
        }
    }

    result = evaluate_candidate(summary, "baseline_neural", "rl_candidate_neural")

    assert result["accepted"] is False
    assert "deal_in_rate" in result["failures"]
```

- [ ] **Step 2: Implement gate logic**

Create `candidate_gate.py`:

```python
from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--summary", type=Path, required=True)
    parser.add_argument("--baseline-policy", default="baseline_neural")
    parser.add_argument("--candidate-policy", default="rl_candidate_neural")
    parser.add_argument("--output", type=Path, default=None)
    return parser.parse_args()


def evaluate_candidate(
    summary: dict[str, Any],
    baseline_policy: str,
    candidate_policy: str,
) -> dict[str, Any]:
    baseline = summary["policies"][baseline_policy]
    candidate = summary["policies"][candidate_policy]
    failures: list[str] = []

    if candidate["avg_score_delta"] <= baseline["avg_score_delta"]:
        failures.append("avg_score_delta")
    if candidate["win_rate"] < baseline["win_rate"]:
        failures.append("win_rate")
    if candidate["deal_in_rate"] > baseline["deal_in_rate"] + 0.02:
        failures.append("deal_in_rate")
    tenpai_turn_ok = (
        baseline["avg_first_tenpai_turn"] is None
        or (
            candidate["avg_first_tenpai_turn"] is not None
            and candidate["avg_first_tenpai_turn"] <= baseline["avg_first_tenpai_turn"]
        )
    )
    final_tenpai_ok = candidate["final_tenpai_rate"] >= baseline["final_tenpai_rate"]
    if not (tenpai_turn_ok or final_tenpai_ok):
        failures.append("tenpai")
    if candidate["avg_latency_ms_per_decision"] >= 100.0:
        failures.append("latency")

    return {
        "accepted": not failures,
        "failures": failures,
        "baseline": baseline,
        "candidate": candidate,
    }


def main() -> None:
    args = parse_args()
    summary = json.loads(args.summary.read_text(encoding="utf-8"))
    result = evaluate_candidate(summary, args.baseline_policy, args.candidate_policy)
    text = json.dumps(result, indent=2, ensure_ascii=False)
    print(text)
    if args.output is not None:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text, encoding="utf-8")
    if not result["accepted"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
```

- [ ] **Step 3: Run gate tests**

Run:

```powershell
python -m pytest backend/bot_trainer/v2/test_rl_dataset.py -q
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```powershell
git add backend/bot_trainer/v2/candidate_gate.py backend/bot_trainer/v2/test_rl_dataset.py
git commit -m "feat(rl): 增加候选模型验收门禁"
```

### Task 9: Wire Candidate Gate Into Wrappers

**Files:**
- Modify: `backend/bot_trainer/v2/train_rl_model.ps1`
- Modify: `backend/bot_trainer/v2/train_rl_model.sh`
- Modify: `backend/bot_trainer/v2/README.md`

- [ ] **Step 1: Add non-blocking gate switch in PowerShell**

Add parameter:

```powershell
[switch]$EnforceCandidateGate
```

After `arena_summary.py`, add:

```powershell
$GateOutput = Join-Path $OutputDir "candidate_gate.json"
$gateArgs = @(
    "backend/bot_trainer/v2/candidate_gate.py",
    "--summary", $EvalSummary,
    "--baseline-policy", "baseline_neural",
    "--candidate-policy", "rl_candidate_neural",
    "--output", $GateOutput
)
Invoke-TrainingPython $gateArgs
$gateExit = $LASTEXITCODE
if ($EnforceCandidateGate -and $gateExit -ne 0) {
    exit $gateExit
}
if (-not $EnforceCandidateGate -and $gateExit -ne 0) {
    Write-Warning "Candidate gate rejected this model. See $GateOutput"
}
```

- [ ] **Step 2: Add Bash gate flag**

Add default:

```bash
ENFORCE_CANDIDATE_GATE=0
```

Add argument parsing:

```bash
--enforce-candidate-gate) ENFORCE_CANDIDATE_GATE=1; shift ;;
```

After summary generation:

```bash
GATE_OUTPUT="$OUTPUT_DIR/candidate_gate.json"
python backend/bot_trainer/v2/candidate_gate.py \
  --summary "$EVAL_SUMMARY" \
  --baseline-policy baseline_neural \
  --candidate-policy rl_candidate_neural \
  --output "$GATE_OUTPUT"
gate_exit=$?
if [ "$ENFORCE_CANDIDATE_GATE" = "1" ] && [ "$gate_exit" -ne 0 ]; then
  exit "$gate_exit"
fi
if [ "$ENFORCE_CANDIDATE_GATE" != "1" ] && [ "$gate_exit" -ne 0 ]; then
  echo "Candidate gate rejected this model. See $GATE_OUTPUT" >&2
fi
```

- [ ] **Step 3: Document promotion workflow**

Add to README:

```markdown
### Candidate Gate

By default the RL wrapper writes `candidate_gate.json` but does not stop local experimentation when a model is rejected. Use `-EnforceCandidateGate` or `--enforce-candidate-gate` for promotion runs.

```powershell
.\backend\bot_trainer\v2\train_rl_model.ps1 `
  -OutputDir backend/bot_trainer/v2/rl_runs/promotion `
  -TrajectoryMatches 400 `
  -EvalMatches 400 `
  -Epochs 3 `
  -Device cuda `
  -EnforceCandidateGate
```
```

- [ ] **Step 4: Run a tiny wrapper smoke without enforcement**

Run:

```powershell
.\backend\bot_trainer\v2\train_rl_model.ps1 -OutputDir backend/bot_trainer/v2/rl_runs/gate_smoke -TrajectoryMatches 8 -EvalMatches 4 -Epochs 1 -BatchSize 64 -Device cpu -SkipTests
```

Expected:

- `candidate_eval_summary.json` exists
- `candidate_gate.json` exists
- wrapper finishes even if the gate rejects the smoke candidate

- [ ] **Step 5: Commit**

```powershell
git add backend/bot_trainer/v2/train_rl_model.ps1 backend/bot_trainer/v2/train_rl_model.sh backend/bot_trainer/v2/README.md
git commit -m "feat(rl): 接入候选模型验收门禁"
```

## Phase 5: Centralized Critic Schema Probe

### Task 10: Add Global State Metadata Without Training On It

**Files:**
- Modify: `backend/src/bot/arena.rs`
- Modify: `backend/bot_trainer/v2/rl_dataset.py`
- Modify: `backend/bot_trainer/v2/test_rl_dataset.py`
- Modify: `backend/bot_trainer/v2/README.md`

- [ ] **Step 1: Add optional global feature fields to trajectory rows**

In `ArenaTrajectoryRow`, add:

```rust
pub global_tile_planes: Option<Vec<f32>>,
pub global_scalar_features: Option<Vec<f32>>,
```

When constructing rows in `trajectory_row_from_trace`, set both fields to `None`:

```rust
global_tile_planes: None,
global_scalar_features: None,
```

- [ ] **Step 2: Load optional global fields in Python**

In `rl_dataset.py`, add to `encode_row`:

```python
"has_global_state": torch.tensor(
    row.get("global_tile_planes") is not None and row.get("global_scalar_features") is not None,
    dtype=torch.bool,
),
```

- [ ] **Step 3: Add test for absent global fields**

Add:

```python
def test_dataset_accepts_missing_global_state(tmp_path: Path) -> None:
    path = tmp_path / "trajectories.jsonl"
    row = base_trajectory_row("learner", 0, reward=1.0, value=0.0)
    path.write_text(json.dumps(row) + "\n", encoding="utf-8")

    dataset = ArenaTrajectoryDataset(path, policy_id="learner")

    assert dataset[0]["has_global_state"].item() is False
```

- [ ] **Step 4: Document centralized critic boundary**

Add to README:

```markdown
### Centralized Critic Boundary

Trajectory rows reserve `global_tile_planes` and `global_scalar_features` for a future centralized critic. They are currently `null` and ignored by PPO. Actor inputs remain strictly local observations, so exported ONNX policy behavior is unchanged.
```

- [ ] **Step 5: Run tests**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::arena -- --nocapture
python -m pytest backend/bot_trainer/v2/test_rl_dataset.py -q
```

Expected: Rust arena tests and Python dataset tests pass.

- [ ] **Step 6: Commit**

```powershell
git add backend/src/bot/arena.rs backend/bot_trainer/v2/rl_dataset.py backend/bot_trainer/v2/test_rl_dataset.py backend/bot_trainer/v2/README.md
git commit -m "feat(rl): 预留中心化critic轨迹字段"
```

## Final Verification

Run:

```powershell
python -m pytest backend/bot_trainer/v2 -q
cargo test --manifest-path backend/Cargo.toml bot::arena bot::policy bot::neural rules::standard::automation -- --nocapture
.\backend\bot_trainer\v2\train_rl_model.ps1 -OutputDir backend/bot_trainer/v2/rl_runs/final_league_smoke -TrajectoryMatches 8 -EvalMatches 4 -Epochs 1 -BatchSize 64 -Device cpu -SkipTests
```

Expected:

- Python tests pass.
- Rust targeted tests pass.
- RL smoke generates four trajectory configs.
- Combined trajectories contain `policy_id="learner"` rows.
- PPO logs include `entropy`, `entropy_coef`, `kl_loss`, and `kl_coef`.
- Candidate ONNX, `candidate_eval_summary.json`, and `candidate_gate.json` are written.

## Rollback Strategy

- If stochastic rollout creates invalid actions, disable it by setting `sample_actions=false` in `opponent_pool.json`; deterministic evaluation remains intact.
- If GAE destabilizes training, set `--gae-lambda 1.0` to match discounted-return behavior.
- If KL over-constrains learning, set `--kl-coef 0.0` for ablation runs.
- If the opponent pool produces noisy results, run fixed seat-order evaluation with `arena_matrix.ps1` before promoting a candidate.

## Completion Criteria

- Arena can generate neural-sampled learner trajectories while evaluation remains deterministic.
- PPO trains only on learner rows from the trajectory file.
- PPO uses GAE, value clipping, entropy, and optional supervised KL.
- Opponent pool and learner-seat rotation are generated from versioned JSON config.
- Candidate acceptance is machine-enforced for promotion runs.
- Centralized critic fields are present as optional schema hooks and ignored by the current actor-critic trainer.
