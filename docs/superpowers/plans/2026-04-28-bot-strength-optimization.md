# Mahjong Bot Strength Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve the neural Mahjong bot's practical strength, especially speed to ready hand, by fixing train/runtime mismatches, making training objectives experimentable, and adding an arena that measures real game outcomes across bot policies and parameters.

**Architecture:** First make offline data and runtime inference agree on legal actions and claim direction semantics. Then make training objectives configurable so policy-only and auxiliary-head variants can be compared. Finally add an in-process arena that runs deterministic all-bot matches with per-seat policy configs and reports win rate, score delta, deal-in rate, first-tenpai turn, final-tenpai rate, claim rate, and decision latency.

**Tech Stack:** Rust 2024 backend (`serde`, `serde_json`, existing standard rules, bot policy/search/neural modules, `ort`), Python trainer (`torch`, `pytest`, ONNX export), PowerShell/Bash wrappers, JSONL experiment reports.

---

## Current Findings To Preserve

- Runtime defaults currently prefer pure neural mode in Docker (`MAHJONG_BOT_POLICY=neural`), which bypasses search for active-turn and claim decisions.
- Runtime claim masks collapse all `chow` options to `chow_mid`, while training labels distinguish `chow_left`, `chow_mid`, and `chow_right`.
- Runtime discard legality rejects `restricted_discard_tile_key` by tile key, while exported training legal actions currently keep that tile key legal.
- The `risk` head labels every discard by a player who eventually dealt in as risky, which is too noisy for a shared trunk unless weighted carefully.
- Existing validation metrics are imitation metrics. They do not directly measure first-tenpai turn, win rate, score delta, or deal-in rate.

## File Map

- Modify: `docker-compose.yml`
  Responsibility: default production bot mode to `hybrid`.
- Modify: `docker-compose.prebuilt.yml`
  Responsibility: mirror production bot default.
- Modify: `backend/src/bot/features.rs`
  Responsibility: encode chow claim masks with the same left/mid/right semantics used by training data.
- Modify: `backend/src/bot/policy.rs`
  Responsibility: select the chow claim option whose direction matches the ranked neural action; add configurable policy entry points for arena.
- Modify: `backend/src/bot/neural.rs`
  Responsibility: expose model scoring behind explicit policy config while keeping existing env-based wrapper.
- Modify: `backend/src/bot/search.rs`
  Responsibility: expose a small shanten metric helper for arena telemetry.
- Modify: `backend/src/rules/standard/automation.rs`
  Responsibility: add policy-configurable bot action selection for arena while preserving production env behavior.
- Modify: `backend/src/bot_trainer/replay.rs`
  Responsibility: align exported legal discards with runtime restricted-discard legality.
- Modify: `backend/src/bot_trainer/export.rs`
  Responsibility: skip and count samples whose historical label is illegal under runtime legality instead of aborting the full export.
- Modify: `backend/bot_trainer/v2/dataset.py`
  Responsibility: support optional per-sample weights or keep targets compatible after exporter filtering.
- Modify: `backend/bot_trainer/v2/train.py`
  Responsibility: add auxiliary-loss weights and report individual losses.
- Modify: `backend/bot_trainer/v2/train_and_export_model.ps1`
  Responsibility: pass experiment loss weights.
- Modify: `backend/bot_trainer/v2/train_and_export_model.sh`
  Responsibility: pass experiment loss weights.
- Modify: `backend/bot_trainer/v2/run_full_training_pipeline.sh`
  Responsibility: pass experiment loss weights through full pipeline.
- Create: `backend/src/bin/bot_arena.rs`
  Responsibility: run deterministic all-bot matches and emit JSONL plus summary metrics.
- Create: `backend/src/bot/arena.rs`
  Responsibility: arena-only metrics structs, policy config parsing, and per-match aggregation helpers.
- Create: `backend/bot_trainer/v2/arena_matrix.ps1`
  Responsibility: run common Windows arena experiment matrix.
- Create: `backend/bot_trainer/v2/arena_matrix.sh`
  Responsibility: run common Linux arena experiment matrix.
- Modify: `backend/bot_trainer/v2/README.md`
  Responsibility: document training variants and arena commands.

---

## Task 1: Default Runtime To Hybrid

**Files:**
- Modify: `docker-compose.yml:13`
- Modify: `docker-compose.prebuilt.yml:10`
- Test: repository search output

- [ ] **Step 1: Change Docker default policy**

Replace:

```yaml
MAHJONG_BOT_POLICY: ${MAHJONG_BOT_POLICY:-neural}
```

with:

```yaml
MAHJONG_BOT_POLICY: ${MAHJONG_BOT_POLICY:-hybrid}
```

- [ ] **Step 2: Verify defaults**

Run:

```powershell
rg -n "MAHJONG_BOT_POLICY" docker-compose.yml docker-compose.prebuilt.yml
```

Expected:

```text
docker-compose.yml:13:      MAHJONG_BOT_POLICY: ${MAHJONG_BOT_POLICY:-hybrid}
docker-compose.prebuilt.yml:10:      MAHJONG_BOT_POLICY: ${MAHJONG_BOT_POLICY:-hybrid}
```

- [ ] **Step 3: Commit**

```bash
git add docker-compose.yml docker-compose.prebuilt.yml
git commit -m "chore(bot): 默认使用混合策略"
```

---

## Task 2: Fix Runtime Chow Direction Masking

**Files:**
- Modify: `backend/src/bot/features.rs`
- Modify: `backend/src/bot/policy.rs`
- Test: `backend/src/bot/features.rs`
- Test: `backend/src/bot/policy.rs`

- [ ] **Step 1: Add failing feature test for chow direction**

Add to `backend/src/bot/features.rs` tests:

```rust
#[test]
fn chow_claim_mask_preserves_left_middle_right_direction() {
    use crate::bot::action_space::claim_action_index;
    use crate::projection::bot_view::BotClaimOption;

    let mut context = sample_context_with_tiles(&["w4", "w5", "t1"]);
    context.last_discard_tile_key = Some("w3".to_string());
    context.claim_options = vec![BotClaimOption {
        action_type: "chow".to_string(),
        tile_ids: vec!["w4#0".to_string(), "w5#1".to_string()],
    }];

    let encoded = encode_bot_context_v2(&context);

    assert!(encoded.claim_mask[claim_action_index("pass").unwrap()]);
    assert!(encoded.claim_mask[claim_action_index("chow_left").unwrap()]);
    assert!(!encoded.claim_mask[claim_action_index("chow_mid").unwrap()]);
    assert!(!encoded.claim_mask[claim_action_index("chow_right").unwrap()]);
}
```

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::features::tests::chow_claim_mask_preserves_left_middle_right_direction -- --nocapture
```

Expected: FAIL because runtime maps every `chow` to `chow_mid`.

- [ ] **Step 2: Implement runtime chow action-name derivation**

In `backend/src/bot/features.rs`, replace `claim_mask_action_name(&option.action_type)` usage with a helper that has access to context:

```rust
fn claim_mask_action_name(context: &BotContext, option: &crate::projection::bot_view::BotClaimOption) -> &str {
    if option.action_type != "chow" {
        return option.action_type.as_str();
    }
    chow_action_name(context, option).unwrap_or("chow_mid")
}

fn chow_action_name(
    context: &BotContext,
    option: &crate::projection::bot_view::BotClaimOption,
) -> Option<&'static str> {
    let last_discard = context.last_discard_tile_key.as_deref()?;
    let discard_index = tile_index(last_discard)?;
    if discard_index >= 27 {
        return Some("chow_mid");
    }
    let mut keys = vec![last_discard.to_string()];
    for tile_id in &option.tile_ids {
        let tile = context
            .player
            .concealed_tiles
            .iter()
            .find(|tile| &tile.tile_id == tile_id)?;
        keys.push(tile.tile_key.clone());
    }
    keys.sort_by_key(|key| tile_index(key).unwrap_or(usize::MAX));
    let middle = keys.get(1)?;
    let middle_index = tile_index(middle)?;
    if middle_index >= 27 || middle_index / 9 != discard_index / 9 {
        return Some("chow_mid");
    }
    if discard_index == middle_index - 1 {
        return Some("chow_left");
    }
    if discard_index == middle_index + 1 {
        return Some("chow_right");
    }
    Some("chow_mid")
}
```

Then update:

```rust
if let Some(index) = claim_action_index(claim_mask_action_name(context, option)) {
    mask[index] = true;
}
```

- [ ] **Step 3: Run feature test**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::features -- --nocapture
```

Expected: PASS.

- [ ] **Step 4: Add policy test for selecting matching chow option**

Add to `backend/src/bot/policy.rs` tests:

```rust
#[test]
fn ranked_chow_action_matches_the_same_chow_shape() {
    let mut context = base_context();
    let concealed_tiles = tiles(&["w2", "w4", "w4", "w5"]);
    context.player.concealed_tile_counts =
        tile_counts34(concealed_tiles.iter().map(|tile| tile.tile_key.as_str()));
    context.player.concealed_tiles = concealed_tiles.clone();
    context.last_discard_tile_key = Some("w3".to_string());
    context.claim_options = vec![
        BotClaimOption {
            action_type: "chow".to_string(),
            tile_ids: vec![concealed_tiles[0].tile_id.clone(), concealed_tiles[1].tile_id.clone()],
        },
        BotClaimOption {
            action_type: "chow".to_string(),
            tile_ids: vec![concealed_tiles[2].tile_id.clone(), concealed_tiles[3].tile_id.clone()],
        },
    ];

    let selected = claim_option_for_ranked_action(&context, "chow_left").expect("chow option");

    assert_eq!(
        selected.tile_ids,
        vec![concealed_tiles[2].tile_id.clone(), concealed_tiles[3].tile_id.clone()]
    );
}
```

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::policy::tests::ranked_chow_action_matches_the_same_chow_shape -- --nocapture
```

Expected: FAIL until policy selection uses the same chow action-name derivation.

- [ ] **Step 5: Implement policy helper**

In `backend/src/bot/policy.rs`, add a helper used by both neural-only and hybrid claim selection:

```rust
fn claim_option_for_ranked_action<'a>(
    context: &'a BotContext,
    ranked_action_name: &str,
) -> Option<&'a crate::projection::bot_view::BotClaimOption> {
    context.claim_options.iter().find(|option| {
        if option.action_type != "chow" {
            option.action_type == ranked_action_name
        } else {
            claim_chow_action_name(context, option) == ranked_action_name
        }
    })
}
```

Replace existing broad chow matching in `select_neural_only_claim` and `select_hybrid_claim` with `claim_option_for_ranked_action(context, best.action_name)`.

- [ ] **Step 6: Run policy tests**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::policy -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add backend/src/bot/features.rs backend/src/bot/policy.rs
git commit -m "fix(bot): 对齐吃牌方向掩码"
```

---

## Task 3: Align Restricted Discard Legality In Exported Data

**Files:**
- Modify: `backend/src/bot_trainer/replay.rs`
- Modify: `backend/src/bot_trainer/export.rs`
- Test: `backend/src/bot_trainer/replay.rs`
- Test: `backend/src/bot_trainer/export.rs`

- [ ] **Step 1: Replace contradictory replay expectations**

Update `same_tile_key_after_chow_remains_legal_when_source_player_has_own_copy` in `backend/src/bot_trainer/replay.rs` to:

```rust
#[test]
fn same_tile_key_after_chow_matches_runtime_restricted_discard_rule() {
    let record = parse_match(
        r#"
Match same key after chow
Player 0 Deal B9 W1 W2
Player 1 Deal B7 B8 B9 T3 T4 J3
Player 2 Deal W3 W4 W5
Player 3 Deal T1 T2 T3
Player 0 Draw J1
Player 0 Play B9
Player 1 Chi B8
Player 1 Play B9
Score 0 0 0 0
"#,
    )
    .expect("match");
    let samples = replay_match_to_samples(&record).expect("samples");

    let claimed_turn = samples
        .iter()
        .find(|sample| {
            sample.seat_index == 1
                && sample.decision_kind == DecisionKind::ActiveTurn
                && sample.context.restricted_discard_tile_key.as_deref() == Some("b9")
        })
        .expect("active turn after chow");

    assert!(!claimed_turn
        .legal_actions
        .iter()
        .any(|action| action == "discard:b9"));
}
```

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot_trainer::replay::tests::same_tile_key_after_chow_matches_runtime_restricted_discard_rule -- --nocapture
```

Expected: FAIL until `legal_discard_actions` filters restricted tile keys.

- [ ] **Step 2: Filter restricted tile key in exporter legal actions**

In `backend/src/bot_trainer/replay.rs`, update `legal_discard_actions`:

```rust
fn legal_discard_actions(context: &SerializableBotContext) -> Vec<String> {
    let mut seen = HashSet::new();
    context
        .player
        .concealed_tiles
        .iter()
        .filter(|tile| !tile.is_flower)
        .filter(|tile| Some(tile.tile_key.as_str()) != context.restricted_discard_tile_key.as_deref())
        .filter(|tile| seen.insert(tile.tile_key.clone()))
        .map(|tile| format!("discard:{}", tile.tile_key))
        .collect()
}
```

- [ ] **Step 3: Skip externally illegal labels during export**

In `backend/src/bot_trainer/export.rs`, add a report field:

```rust
pub runtime_illegal_label_count: usize,
```

Change the sample write loop:

```rust
for sample in samples {
    if let Err(error) = validate_sample(&sample) {
        report.runtime_illegal_label_count += 1;
        eprintln!("skip runtime-illegal sample: {error}");
        continue;
    }
    *report.samples_by_split.entry(split).or_default() += 1;
    *report
        .samples_by_decision_kind
        .entry(decision_kind_name(&sample.decision_kind).to_string())
        .or_default() += 1;
    writers.write(split, &sample)?;
    report.sample_count += 1;
}
```

Keep `illegal_label_count` for hard exporter bugs, but use `runtime_illegal_label_count` for BotZone records that violate current runtime rules.

- [ ] **Step 4: Add export test for runtime-illegal discard filtering**

Add to `backend/src/bot_trainer/export.rs` tests:

```rust
#[test]
fn validate_sample_rejects_restricted_discard_label() {
    let mut sample = crate::bot_trainer::replay::TrainingDecisionSampleV2 {
        schema_version: 2,
        match_id: "fixture".to_string(),
        decision_index: 0,
        seat_index: 0,
        decision_kind: crate::bot_trainer::replay::DecisionKind::ActiveTurn,
        context: crate::bot_trainer::replay::SerializableBotContext {
            seat_index: 0,
            seat_count: 4,
            dealer_seat: 0,
            round_wind: "east".to_string(),
            cumulative_scores: vec![0, 0, 0, 0],
            wall_tiles_remaining: 70,
            visible_tile_keys: vec![],
            opponent_discards_by_seat: vec![vec![], vec![], vec![], vec![]],
            opponent_melds_by_seat: vec![vec![], vec![], vec![], vec![]],
            player: crate::bot_trainer::replay::SerializableBotPlayer {
                concealed_tiles: vec![],
                concealed_tile_counts: vec![0; 34],
                meld_tile_key_groups: vec![],
                flower_count: 0,
            },
            restricted_discard_tile_key: Some("w1".to_string()),
            drawn_tile_id: None,
            self_kong_candidates: vec![],
            claim_options: vec![],
            last_discard_tile_key: None,
            add_kong_risk_tiles: Default::default(),
        },
        legal_actions: vec!["discard:w2".to_string()],
        label: crate::bot_trainer::replay::TrainingLabel::Discard {
            tile_key: "w1".to_string(),
        },
        outcome: crate::bot_trainer::replay::SampleOutcome {
            score_delta: 0,
            won: false,
            dealt_in: false,
            round_drawn: false,
        },
    };

    assert!(validate_sample(&sample).is_err());
    sample.label = crate::bot_trainer::replay::TrainingLabel::Discard {
        tile_key: "w2".to_string(),
    };
    assert!(validate_sample(&sample).is_ok());
}
```

- [ ] **Step 5: Run trainer export tests**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot_trainer::replay bot_trainer::export -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Re-export dataset**

Run:

```powershell
.\backend\bot_trainer\v2\export_full_dataset.ps1 -ProgressEvery 1000
```

Expected: export completes; `export_report.json` includes `runtime_illegal_label_count`; `illegal_label_count` remains `0`.

- [ ] **Step 7: Commit**

```bash
git add backend/src/bot_trainer/replay.rs backend/src/bot_trainer/export.rs backend/bot_trainer/v2/out/export_report.json
git commit -m "fix(trainer): 对齐受限出牌合法性"
```

---

## Task 4: Make Auxiliary Training Losses Configurable

**Files:**
- Modify: `backend/bot_trainer/v2/train.py`
- Modify: `backend/bot_trainer/v2/test_dataset.py`
- Modify: `backend/bot_trainer/v2/train_and_export_model.ps1`
- Modify: `backend/bot_trainer/v2/train_and_export_model.sh`
- Modify: `backend/bot_trainer/v2/run_full_training_pipeline.sh`

- [ ] **Step 1: Add training-loss unit test**

Create a new test in `backend/bot_trainer/v2/test_dataset.py`:

```python
def test_auxiliary_loss_weights_can_disable_value_and_risk() -> None:
    import torch
    from train import compute_losses

    outputs = {
        "discard_logits": torch.tensor([[3.0, 0.0] + [-100.0] * 32]),
        "claim_logits": torch.zeros((1, 7)),
        "self_kong_logits": torch.zeros((1, 3)),
        "hu_logits": torch.zeros((1, 2)),
        "value": torch.tensor([[999.0]]),
        "risk_logits": torch.full((1, 34), 999.0),
    }
    batch = {
        "discard_mask": torch.tensor([[True, True] + [False] * 32]),
        "discard_target": torch.tensor([0]),
        "claim_mask": torch.zeros((1, 7), dtype=torch.bool),
        "claim_target": torch.tensor([-100]),
        "self_kong_mask": torch.zeros((1, 3), dtype=torch.bool),
        "self_kong_target": torch.tensor([-100]),
        "hu_mask": torch.tensor([[True, False]]),
        "hu_target": torch.tensor([-100]),
        "value_target": torch.tensor([[0.0]]),
        "risk_target": torch.zeros((1, 34)),
    }

    losses = compute_losses(outputs, batch, value_weight=0.0, risk_weight=0.0, hu_weight=1.0)

    assert losses["value_loss"].item() > 1000.0
    assert losses["risk_loss"].item() > 100.0
    assert losses["loss"].item() < 0.1
```

Run:

```powershell
python -m pytest backend/bot_trainer/v2/test_dataset.py::test_auxiliary_loss_weights_can_disable_value_and_risk -q
```

Expected: FAIL because `compute_losses` does not accept weights or return `risk_loss`.

- [ ] **Step 2: Add CLI args and weighted losses**

In `train.py`, add parser args:

```python
parser.add_argument("--claim-loss-weight", type=float, default=1.0)
parser.add_argument("--self-kong-loss-weight", type=float, default=1.0)
parser.add_argument("--hu-loss-weight", type=float, default=1.0)
parser.add_argument("--value-loss-weight", type=float, default=0.25)
parser.add_argument("--risk-loss-weight", type=float, default=0.25)
```

Update call site:

```python
losses = compute_losses(
    outputs,
    batch,
    claim_weight=args.claim_loss_weight,
    self_kong_weight=args.self_kong_loss_weight,
    hu_weight=args.hu_loss_weight,
    value_weight=args.value_loss_weight,
    risk_weight=args.risk_loss_weight,
)
```

Update `compute_losses` signature:

```python
def compute_losses(
    outputs: dict[str, torch.Tensor],
    batch: dict[str, torch.Tensor],
    claim_weight: float = 1.0,
    self_kong_weight: float = 1.0,
    hu_weight: float = 1.0,
    value_weight: float = 0.25,
    risk_weight: float = 0.25,
) -> dict[str, torch.Tensor]:
```

Return all component losses:

```python
loss = (
    discard_loss
    + claim_weight * claim_loss
    + self_kong_weight * self_kong_loss
    + hu_weight * hu_loss
    + value_weight * value_loss
    + risk_weight * risk_loss
)
return {
    "loss": loss,
    "discard_loss": discard_loss,
    "claim_loss": claim_loss,
    "self_kong_loss": self_kong_loss,
    "hu_loss": hu_loss,
    "value_loss": value_loss,
    "risk_loss": risk_loss,
}
```

- [ ] **Step 3: Report component losses**

Extend `MetricTotals` with `discard_loss_sum`, `claim_loss_sum`, `self_kong_loss_sum`, `hu_loss_sum`, and `risk_loss_sum`. In `update`, add each detached loss to its sum. In `as_metrics`, emit each average:

```python
"discard_loss": self.discard_loss_sum.item() / max(1, self.batch_count),
"claim_loss": self.claim_loss_sum.item() / max(1, self.batch_count),
"self_kong_loss": self.self_kong_loss_sum.item() / max(1, self.batch_count),
"hu_loss": self.hu_loss_sum.item() / max(1, self.batch_count),
"risk_loss": self.risk_loss_sum.item() / max(1, self.batch_count),
```

- [ ] **Step 4: Update wrappers**

Add pass-through options to PowerShell and Bash wrappers:

```text
--value-loss-weight VALUE
--risk-loss-weight VALUE
--hu-loss-weight VALUE
```

Default values must preserve current behavior:

```text
VALUE_LOSS_WEIGHT=0.25
RISK_LOSS_WEIGHT=0.25
HU_LOSS_WEIGHT=1.0
```

When calling `train.py`, include:

```bash
--value-loss-weight "$VALUE_LOSS_WEIGHT"
--risk-loss-weight "$RISK_LOSS_WEIGHT"
--hu-loss-weight "$HU_LOSS_WEIGHT"
```

- [ ] **Step 5: Run Python tests**

Run:

```powershell
python -m pytest backend/bot_trainer/v2 -q
```

Expected: PASS.

- [ ] **Step 6: Smoke train policy-only variant**

Run:

```powershell
uv run python backend/bot_trainer/v2/train.py --data backend/bot_trainer/v2/out_p1_context_smoke --epochs 1 --batch-size 64 --output backend/bot_trainer/v2/checkpoints_policy_smoke --device cpu --value-loss-weight 0 --risk-loss-weight 0 --hu-loss-weight 0
```

Expected: one epoch completes; metrics include component losses.

- [ ] **Step 7: Commit**

```bash
git add backend/bot_trainer/v2/train.py backend/bot_trainer/v2/test_dataset.py backend/bot_trainer/v2/train_and_export_model.ps1 backend/bot_trainer/v2/train_and_export_model.sh backend/bot_trainer/v2/run_full_training_pipeline.sh
git commit -m "feat(trainer): 支持辅助损失权重"
```

---

## Task 5: Add Configurable Bot Policy Entry Points

**Files:**
- Create: `backend/src/bot/arena.rs`
- Modify: `backend/src/bot/mod.rs`
- Modify: `backend/src/bot/policy.rs`
- Modify: `backend/src/bot/neural.rs`
- Modify: `backend/src/rules/standard/automation.rs`

- [ ] **Step 1: Define explicit bot policy config**

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

Export it from `backend/src/bot/mod.rs`:

```rust
pub mod arena;
```

- [ ] **Step 2: Add policy tests for explicit config**

Add to `backend/src/bot/policy.rs` tests:

```rust
#[test]
fn explicit_hybrid_config_uses_search_when_neural_is_unavailable() {
    let context = base_context();
    let config = crate::bot::arena::ArenaBotPolicyConfig {
        id: "hybrid-test".to_string(),
        mode: crate::bot::arena::ArenaPolicyMode::Hybrid,
        neural_weight: 15,
        model_path: Some("missing-model.onnx".to_string()),
    };

    let action = choose_active_turn_action_with_config(&context, &config);

    assert!(action.is_none() || action.as_ref().is_some_and(|action| action.seat_index == 0));
}
```

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::policy::tests::explicit_hybrid_config_uses_search_when_neural_is_unavailable -- --nocapture
```

Expected: FAIL because configurable entry point does not exist.

- [ ] **Step 3: Implement config wrappers**

In `backend/src/bot/policy.rs`, keep existing env wrappers but route through explicit config:

```rust
pub fn choose_active_turn_action(context: &BotContext) -> Option<BotAction> {
    choose_active_turn_action_with_config(context, &bot_policy_config_from_env())
}

pub fn choose_claim_action(context: &BotContext) -> Option<BotAction> {
    choose_claim_action_with_config(context, &bot_policy_config_from_env())
}
```

Add:

```rust
pub fn choose_active_turn_action_with_config(
    context: &BotContext,
    config: &crate::bot::arena::ArenaBotPolicyConfig,
) -> Option<BotAction> {
    // Move the existing body here and replace env reads with config fields.
}

pub fn choose_claim_action_with_config(
    context: &BotContext,
    config: &crate::bot::arena::ArenaBotPolicyConfig,
) -> Option<BotAction> {
    // Move the existing body here and replace env reads with config fields.
}
```

Replace `neural_prior_weight()` call sites inside configurable functions with `config.neural_weight.max(0)`.

- [ ] **Step 4: Add explicit neural scoring model-path wrapper**

In `backend/src/bot/neural.rs`, add:

```rust
pub(crate) fn neural_decision_scores_for_model(
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

Configurable policy functions call this with `config.model_path.as_deref().map(Path::new)`.

- [ ] **Step 5: Add automation resolver entry point**

In `backend/src/rules/standard/automation.rs`, add:

```rust
pub fn next_bot_action_in_room_state_with_policy_resolver(
    room: &RoomState,
    policy_for_seat: &dyn Fn(usize) -> crate::bot::arena::ArenaBotPolicyConfig,
) -> Result<Option<BotAction>, String> {
    Ok(next_bot_action_for_state_with_policy_resolver(room, policy_for_seat))
}
```

The existing `next_bot_action_in_room_state` calls it with:

```rust
&|_| crate::bot::arena::ArenaBotPolicyConfig::from_env()
```

When building bot context for active turn and claim windows, call `choose_active_turn_action_with_config` and `choose_claim_action_with_config` using `policy_for_seat(seat_index)`.

- [ ] **Step 6: Run backend targeted tests**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::policy bot::neural rules::standard::automation -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add backend/src/bot/arena.rs backend/src/bot/mod.rs backend/src/bot/policy.rs backend/src/bot/neural.rs backend/src/rules/standard/automation.rs
git commit -m "feat(bot): 支持显式策略配置"
```

---

## Task 6: Build Arena Metrics And Runner

**Files:**
- Modify: `backend/src/bot/arena.rs`
- Modify: `backend/src/bot/search.rs`
- Create: `backend/src/bin/bot_arena.rs`
- Test: `backend/src/bot/arena.rs`

- [ ] **Step 1: Add arena config and metric structs**

Append to `backend/src/bot/arena.rs`:

```rust
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ArenaConfig {
    pub matches: usize,
    pub seed: u64,
    pub policies: Vec<ArenaBotPolicyConfig>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
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
    pub decision_latency_ms_sum: u128,
    pub decision_count: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ArenaMatchReport {
    pub match_index: usize,
    pub seed: u64,
    pub seats: Vec<ArenaSeatMetrics>,
}
```

- [ ] **Step 2: Add aggregation test**

Add to `backend/src/bot/arena.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_config_parses_policy_ids() {
        let raw = r#"{
            "matches": 2,
            "seed": 7,
            "policies": [
                {"id":"heuristic","mode":"heuristic","neural_weight":0,"model_path":null},
                {"id":"hybrid15","mode":"hybrid","neural_weight":15,"model_path":null}
            ]
        }"#;

        let config: ArenaConfig = serde_json::from_str(raw).expect("config");

        assert_eq!(config.matches, 2);
        assert_eq!(config.policies[1].id, "hybrid15");
        assert_eq!(config.policies[1].mode, ArenaPolicyMode::Hybrid);
    }
}
```

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::arena -- --nocapture
```

Expected: PASS.

- [ ] **Step 3: Expose shanten metric helper**

In `backend/src/bot/search.rs`, add:

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

Update `SearchEngine::bot_min_shanten` to call this helper.

- [ ] **Step 4: Create initial arena binary**

Create `backend/src/bin/bot_arena.rs` with command parsing:

```rust
use std::{fs::File, io::Write, path::PathBuf};

use backend::bot::arena::{ArenaConfig, ArenaMatchReport};

struct Args {
    config_path: PathBuf,
    output_path: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args = parse_args()?;
    let config: ArenaConfig = serde_json::from_str(&std::fs::read_to_string(&args.config_path)?)?;
    let reports = run_arena(&config)?;
    let mut output = File::create(&args.output_path)?;
    for report in reports {
        serde_json::to_writer(&mut output, &report)?;
        output.write_all(b"\n")?;
    }
    Ok(())
}
```

Implement `parse_args` with required `--config` and `--output`.

- [ ] **Step 5: Implement arena run loop**

The first runner version must:

1. Build a room with four bot seats.
2. Start a standard match with deterministic seed.
3. Loop while phase is playing and a bot action is available.
4. Call `next_bot_action_in_room_state_with_policy_resolver`.
5. Apply the returned action through existing standard action handlers.
6. Track seat metrics after each action.
7. Stop when the round/match settles or when a hard action cap of `600` actions is reached.

If the existing standard flow does not expose deterministic wall seeding, add the seed field in the narrowest existing flow helper rather than creating a separate game engine.

- [ ] **Step 6: Add smoke command**

Create a temporary config:

```json
{
  "matches": 2,
  "seed": 7,
  "policies": [
    {"id":"heuristic","mode":"heuristic","neural_weight":0,"model_path":null},
    {"id":"hybrid15","mode":"hybrid","neural_weight":15,"model_path":null},
    {"id":"hybrid30","mode":"hybrid","neural_weight":30,"model_path":null},
    {"id":"neural","mode":"neural","neural_weight":0,"model_path":null}
  ]
}
```

Run:

```powershell
cargo run --manifest-path backend/Cargo.toml --bin bot_arena -- --config backend/bot_trainer/v2/arena_smoke.json --output backend/bot_trainer/v2/arena_smoke.jsonl
```

Expected: creates `arena_smoke.jsonl` with 2 JSON lines.

- [ ] **Step 7: Commit**

```bash
git add backend/src/bot/arena.rs backend/src/bot/search.rs backend/src/bin/bot_arena.rs
git commit -m "feat(bot): 新增策略竞技台"
```

---

## Task 7: Add Arena Matrix Scripts

**Files:**
- Create: `backend/bot_trainer/v2/arena_matrix.ps1`
- Create: `backend/bot_trainer/v2/arena_matrix.sh`
- Modify: `backend/bot_trainer/v2/README.md`

- [ ] **Step 1: Add Windows matrix script**

Create `backend/bot_trainer/v2/arena_matrix.ps1`:

```powershell
param(
    [int]$Matches = 200,
    [int]$Seed = 7,
    [string]$OutputDir = "backend/bot_trainer/v2/arena_runs"
)

$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$config = @{
    matches = $Matches
    seed = $Seed
    policies = @(
        @{ id = "heuristic"; mode = "heuristic"; neural_weight = 0; model_path = $null },
        @{ id = "hybrid05"; mode = "hybrid"; neural_weight = 5; model_path = $null },
        @{ id = "hybrid15"; mode = "hybrid"; neural_weight = 15; model_path = $null },
        @{ id = "hybrid30"; mode = "hybrid"; neural_weight = 30; model_path = $null },
        @{ id = "hybrid60"; mode = "hybrid"; neural_weight = 60; model_path = $null },
        @{ id = "neural"; mode = "neural"; neural_weight = 0; model_path = $null }
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
SEED="${SEED:-7}"
OUTPUT_DIR="${OUTPUT_DIR:-backend/bot_trainer/v2/arena_runs}"

mkdir -p "$OUTPUT_DIR"
CONFIG_PATH="$OUTPUT_DIR/arena_config.json"
OUTPUT_PATH="$OUTPUT_DIR/arena_results.jsonl"

cat > "$CONFIG_PATH" <<JSON
{
  "matches": $MATCHES,
  "seed": $SEED,
  "policies": [
    {"id":"heuristic","mode":"heuristic","neural_weight":0,"model_path":null},
    {"id":"hybrid05","mode":"hybrid","neural_weight":5,"model_path":null},
    {"id":"hybrid15","mode":"hybrid","neural_weight":15,"model_path":null},
    {"id":"hybrid30","mode":"hybrid","neural_weight":30,"model_path":null},
    {"id":"hybrid60","mode":"hybrid","neural_weight":60,"model_path":null},
    {"id":"neural","mode":"neural","neural_weight":0,"model_path":null}
  ]
}
JSON

cargo run --manifest-path backend/Cargo.toml --release --bin bot_arena -- --config "$CONFIG_PATH" --output "$OUTPUT_PATH"
```

- [ ] **Step 3: Document arena use**

Add to `backend/bot_trainer/v2/README.md`:

```markdown
## Arena

Windows:

```powershell
.\backend\bot_trainer\v2\arena_matrix.ps1 -Matches 200 -Seed 7
```

Linux:

```bash
MATCHES=200 SEED=7 ./backend/bot_trainer/v2/arena_matrix.sh
```

Primary metrics:
- score delta per policy
- win rate
- deal-in rate
- first-tenpai turn
- final-tenpai rate
- claim count
- discard count
- average decision latency
```

- [ ] **Step 4: Commit**

```bash
git add backend/bot_trainer/v2/arena_matrix.ps1 backend/bot_trainer/v2/arena_matrix.sh backend/bot_trainer/v2/README.md
git commit -m "docs(bot): 增加竞技台实验入口"
```

---

## Task 8: Run First Experiment Matrix

**Files:**
- Generated: `backend/bot_trainer/v2/arena_runs/arena_config.json`
- Generated: `backend/bot_trainer/v2/arena_runs/arena_results.jsonl`
- Generated: `backend/bot_trainer/v2/checkpoints_policy_only`

- [ ] **Step 1: Train policy-only checkpoint**

Run:

```powershell
.\backend\bot_trainer\v2\train_and_export_model.ps1 -Epochs 20 -BatchSize 4096 -Device cuda -NumWorkers 0 -ValueLossWeight 0 -RiskLossWeight 0 -HuLossWeight 0
```

Expected: writes a checkpoint and ONNX model; Rust ONNX smoke test passes.

- [ ] **Step 2: Run arena matrix**

Run:

```powershell
.\backend\bot_trainer\v2\arena_matrix.ps1 -Matches 500 -Seed 20260428
```

Expected: writes `backend/bot_trainer/v2/arena_runs/arena_results.jsonl`.

- [ ] **Step 3: Choose candidate policy**

Select the policy with:

- highest average score delta
- no worse than heuristic deal-in rate by more than `2%`
- lower or equal first-tenpai turn than heuristic
- average decision latency under `100ms`

- [ ] **Step 4: Record result**

Append an "Arena Results" section to `backend/bot_trainer/v2/README.md` with:

```markdown
## Latest Arena Results

- Dataset export: `<export_report.json timestamp or git hash>`
- Model checkpoint: `<checkpoint path>`
- Arena matches: `500`
- Seed: `20260428`
- Winner policy: `<policy id>`
- Selection reason: `<score delta / tenpai / deal-in summary>`
```

- [ ] **Step 5: Commit**

```bash
git add backend/bot_trainer/v2/README.md backend/bot_trainer/v2/arena_runs/arena_config.json backend/bot_trainer/v2/arena_runs/arena_results.jsonl
git commit -m "test(bot): 记录首轮竞技台结果"
```

---

## Task 9: Transformer Exploration Gate

**Files:**
- Modify: `backend/bot_trainer/v2/model.py`
- Modify: `backend/bot_trainer/v2/train.py`
- Modify: `backend/bot_trainer/v2/export_onnx.py`
- Test: `backend/bot_trainer/v2/test_dataset.py`

Do this only if Task 8 shows the best MLP variant still loses to heuristic or does not improve first-tenpai turn.

- [ ] **Step 1: Add model-kind CLI**

Add:

```python
parser.add_argument("--model-kind", choices=["mlp", "tile_transformer"], default="mlp")
```

- [ ] **Step 2: Add transformer output-shape test**

Add:

```python
def test_tile_transformer_outputs_match_mlp_heads() -> None:
    import torch
    from model import build_model, ModelConfig

    model = build_model(ModelConfig(10, 10), model_kind="tile_transformer")
    outputs = model(torch.zeros((2, 10, 34)), torch.zeros((2, 10)))

    assert outputs["discard_logits"].shape == (2, 34)
    assert outputs["claim_logits"].shape == (2, 7)
    assert outputs["self_kong_logits"].shape == (2, 3)
    assert outputs["hu_logits"].shape == (2, 2)
    assert outputs["value"].shape == (2, 1)
    assert outputs["risk_logits"].shape == (2, 34)
```

- [ ] **Step 3: Implement compact tile-token transformer**

Implement a 34-token encoder:

```python
class MahjongTileTransformerV2(nn.Module):
    def __init__(self, tile_plane_count: int, scalar_feature_count: int) -> None:
        super().__init__()
        self.tile_projection = nn.Linear(tile_plane_count, 128)
        self.tile_embedding = nn.Parameter(torch.zeros(34, 128))
        self.scalar_encoder = nn.Sequential(
            nn.Linear(scalar_feature_count, 128),
            nn.ReLU(),
            nn.LayerNorm(128),
        )
        encoder_layer = nn.TransformerEncoderLayer(
            d_model=128,
            nhead=4,
            dim_feedforward=256,
            dropout=0.1,
            batch_first=True,
            activation="gelu",
        )
        self.encoder = nn.TransformerEncoder(encoder_layer, num_layers=3)
        self.trunk = nn.Sequential(nn.Linear(256, 256), nn.ReLU())
        self.discard_head = nn.Linear(256, 34)
        self.claim_head = nn.Linear(256, 7)
        self.self_kong_head = nn.Linear(256, 3)
        self.hu_head = nn.Linear(256, 2)
        self.value_head = nn.Linear(256, 1)
        self.risk_head = nn.Linear(256, 34)

    def forward(self, tile_planes: torch.Tensor, scalar_features: torch.Tensor) -> dict[str, torch.Tensor]:
        tokens = tile_planes.transpose(1, 2)
        hidden_tokens = self.tile_projection(tokens) + self.tile_embedding.unsqueeze(0)
        encoded = self.encoder(hidden_tokens).mean(dim=1)
        scalar = self.scalar_encoder(scalar_features)
        hidden = self.trunk(torch.cat([encoded, scalar], dim=1))
        return {
            "discard_logits": self.discard_head(hidden),
            "claim_logits": self.claim_head(hidden),
            "self_kong_logits": self.self_kong_head(hidden),
            "hu_logits": self.hu_head(hidden),
            "value": self.value_head(hidden),
            "risk_logits": self.risk_head(hidden),
        }
```

- [ ] **Step 4: Compare by arena only**

Train with:

```powershell
uv run python backend/bot_trainer/v2/train.py --data backend/bot_trainer/v2/out --epochs 20 --batch-size 2048 --output backend/bot_trainer/v2/checkpoints_transformer --device cuda --amp --model-kind tile_transformer --value-loss-weight 0 --risk-loss-weight 0
```

Export and run arena with the same seed and match count used for the best MLP run.

- [ ] **Step 5: Keep transformer only if it wins in arena**

Acceptance:

- average score delta improves over best MLP by at least `3%`
- first-tenpai turn improves by at least `0.3` turns or final-tenpai rate improves by at least `2%`
- average decision latency stays under `100ms`

If not met, keep MLP and record the failed transformer result in README.

---

## Final Verification

Run before claiming completion:

```powershell
cargo test --manifest-path backend/Cargo.toml bot::features bot::policy bot::neural bot::arena bot_trainer rules::standard::automation -- --nocapture
python -m pytest backend/bot_trainer/v2 -q
.\backend\bot_trainer\v2\export_full_dataset.ps1 -MaxMatches 1000 -OutputDir backend/bot_trainer/v2/out_policy_smoke -ProgressEvery 100
uv run python backend/bot_trainer/v2/train.py --data backend/bot_trainer/v2/out_policy_smoke --epochs 1 --batch-size 64 --output backend/bot_trainer/v2/checkpoints_policy_smoke --device cpu --value-loss-weight 0 --risk-loss-weight 0 --hu-loss-weight 0
cargo run --manifest-path backend/Cargo.toml --bin bot_arena -- --config backend/bot_trainer/v2/arena_smoke.json --output backend/bot_trainer/v2/arena_smoke.jsonl
```

Expected:

- all targeted Rust tests pass
- all Python trainer tests pass
- smoke export completes with `illegal_label_count = 0`
- smoke training completes with finite component losses
- arena writes JSONL match reports

## Completion Criteria

- Production default uses `hybrid`, not pure `neural`.
- Runtime chow masks and policy selection preserve `chow_left/chow_mid/chow_right`.
- Exported legal discards match runtime restricted-discard legality.
- Training can disable noisy auxiliary heads without code changes.
- Arena can compare `heuristic`, `hybrid`, and `neural` configs in deterministic matches.
- Model selection is based on arena metrics, not only validation loss or discard top-k.
- Transformer is attempted only after the MLP-plus-arena baseline is measured.
