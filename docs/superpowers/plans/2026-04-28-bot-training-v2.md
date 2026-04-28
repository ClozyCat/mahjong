# Mahjong Bot Training V2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the legacy discard-only trainer path with a backend-native, multi-head Mahjong bot training and inference pipeline that learns active-turn, claim, kong, hu, value, and risk decisions from full four-player match replay.

**Architecture:** Rust owns BotZone parsing, replay, backend-like state reconstruction, legal action generation, and training sample export so offline data and runtime inference share the same `BotContextView` semantics. Python owns tensorization, model training, metrics, and ONNX export, using metadata generated with the dataset. Runtime deletes the v1 feature path and consumes only the v2 multi-head ONNX model through backend-native context encoding.

**Tech Stack:** Rust 2024 backend (`serde`, `serde_json`, existing standard rules, `RoomState`, `RoomScoringCache`, `BotContextView`, `ort`), Python trainer (`torch`, `numpy`, `onnx`, `onnxruntime`), BotZone dataset under `backend/bot_trainer/dataset`.

---

## Non-Negotiable Constraints

- Delete the legacy v1 discard-only path. Do not keep `TRAINER_FEATURE_LAYERS`, `build_trainer_features`, `trainer_index_for_backend_index`, or hybrid v1 fallback behavior.
- Replace the old ONNX asset with a v2 multi-head model. Do not ship both `mahjong_policy_net.onnx` v1 and v2 side by side.
- Training data must be generated from all four players' actions, not winner-only histories.
- Training and runtime must share one tile vocabulary, one action vocabulary, one legal-mask convention, and one feature schema version.
- Final score, fan description, and settlement data may be labels or sample weights only. They must not be encoded into model input features.
- `Ignore Player ...` declarations are positive claim labels for the ignored player.
- Pass labels must be generated for players who had a legal claim opportunity but did not declare it.
- Train, validation, and test splits must be by `Match ID`, never by line or decision row.

## File Map

- Create: `backend/src/bot/features.rs`
  Responsibility: encode `BotContextView` into v2 dense tensors and legal masks for runtime inference.
- Modify: `backend/src/bot/mod.rs`
  Responsibility: export the new feature module and updated neural policy API.
- Replace: `backend/src/bot/neural.rs`
  Responsibility: remove v1 feature construction and implement v2 multi-head ONNX loading, output decoding, and masked scoring.
- Modify: `backend/src/bot/policy.rs`
  Responsibility: consume v2 neural decision scores for active-turn and claim decisions, with search only as a scoring component or fallback when the model cannot load.
- Create: `backend/src/bot/action_space.rs`
  Responsibility: define tile vocabulary, action ids, legal mask layout, and stable metadata export constants.
- Create: `backend/src/bot_trainer.rs`
  Responsibility: backend module root for trainer-only parser/replay/export code.
- Create: `backend/src/bot_trainer/botzone.rs`
  Responsibility: parse `data.txt` / `sample.txt` into typed BotZone match records.
- Create: `backend/src/bot_trainer/replay.rs`
  Responsibility: replay a BotZone match into backend-like `RoomState` snapshots and emit decision samples.
- Create: `backend/src/bot_trainer/export.rs`
  Responsibility: serialize v2 samples, split metadata, schema metadata, and export statistics.
- Create: `backend/src/bin/export_bot_dataset_v2.rs`
  Responsibility: command-line dataset exporter.
- Create: `backend/bot_trainer/v2/dataset.py`
  Responsibility: load exported shards and convert samples to PyTorch tensors using `metadata.json`.
- Create: `backend/bot_trainer/v2/model.py`
  Responsibility: define the multi-head policy/value/risk network.
- Create: `backend/bot_trainer/v2/train.py`
  Responsibility: train, validate, log metrics, and save checkpoints.
- Create: `backend/bot_trainer/v2/export_onnx.py`
  Responsibility: export the best checkpoint as ONNX with named outputs.
- Create: `backend/bot_trainer/v2/README.md`
  Responsibility: document exporter, trainer, metrics, and deployment commands.
- Replace: `backend/assets/models/mahjong_policy_net.onnx`
  Responsibility: store only the v2 production model.
- Modify: `docker-compose.yml`
  Responsibility: keep `MAHJONG_BOT_MODEL_PATH` pointing at the single v2 model asset if the path name changes.
- Modify: `docker-compose.prebuilt.yml`
  Responsibility: mirror the v2 model path.
- Modify: `.env.example`
  Responsibility: document v2-only bot policy environment variables.

## Data Shapes

### Training Sample JSONL

Each exported row is one decision point:

```json
{
  "schema_version": 2,
  "match_id": "61602cb45ddc087351c04358",
  "decision_index": 42,
  "seat_index": 1,
  "decision_kind": "claim_window",
  "context": {
    "seat_index": 1,
    "seat_count": 4,
    "dealer_seat": 0,
    "round_wind": "north",
    "cumulative_scores": [0, 0, 0, 0],
    "wall_tiles_remaining": 68,
    "visible_tile_keys": ["t6", "t2"],
    "opponent_discards_by_seat": [[], ["t2"], ["f4"], []],
    "opponent_melds_by_seat": [[], [], [], []],
    "kong_entries": [],
    "player": {
      "concealed_tile_counts": [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
      "meld_tile_key_groups": [],
      "flower_count": 0
    },
    "restricted_discard_tile_key": null,
    "drawn_tile_id": null,
    "self_kong_candidates": [],
    "claim_options": [{"action_type": "pung", "tile_ids": ["j1#p1-0", "j1#p1-1"]}],
    "last_discard_tile_key": "j1",
    "add_kong_risk_tiles": []
  },
  "legal_actions": ["pass", "claim:pung:j1"],
  "label": "pass",
  "outcome": {
    "score_delta": -8,
    "won": false,
    "dealt_in": false,
    "round_drawn": false
  }
}
```

### Metadata JSON

```json
{
  "schema_version": 2,
  "tile_keys": ["w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "t1", "t2", "t3", "t4", "t5", "t6", "t7", "t8", "t9", "b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9", "east", "south", "west", "north", "red", "green", "white"],
  "decision_kinds": ["active_turn", "claim_window", "rob_kong"],
  "claim_actions": ["pass", "hu", "pung", "kong", "chow_left", "chow_mid", "chow_right"],
  "self_kong_actions": ["pass", "concealed_kong", "add_kong"],
  "model_outputs": ["discard_logits", "claim_logits", "self_kong_logits", "hu_logits", "value", "risk_logits"],
  "split_strategy": "match_id_hash"
}
```

## Task 1: Define V2 Action Space and Runtime Feature Encoder

**Files:**
- Create: `backend/src/bot/action_space.rs`
- Create: `backend/src/bot/features.rs`
- Modify: `backend/src/bot/mod.rs`
- Test: `backend/src/bot/action_space.rs`
- Test: `backend/src/bot/features.rs`

- [ ] **Step 1: Add failing tests for tile vocabulary and legal masks**

```rust
#[test]
fn bot_v2_tile_keys_match_backend_order() {
    assert_eq!(TILE_KEYS[0], "w1");
    assert_eq!(TILE_KEYS[8], "w9");
    assert_eq!(TILE_KEYS[9], "t1");
    assert_eq!(TILE_KEYS[18], "b1");
    assert_eq!(TILE_KEYS[27], "east");
    assert_eq!(TILE_KEYS[33], "white");
}

#[test]
fn active_turn_mask_allows_only_unrestricted_concealed_tiles() {
    let context = sample_context_with_tiles(&["w1", "w1", "t5", "red"]);
    let mut context = context;
    context.restricted_discard_tile_key = Some("w1".to_string());

    let encoded = encode_bot_context_v2(&context);

    assert!(!encoded.discard_mask[tile_index("w1").unwrap()]);
    assert!(encoded.discard_mask[tile_index("t5").unwrap()]);
    assert!(encoded.discard_mask[tile_index("red").unwrap()]);
    assert!(!encoded.discard_mask[tile_index("b9").unwrap()]);
}
```

Run: `cargo test bot::action_space bot::features -- --nocapture`  
Expected: FAIL because the modules do not exist.

- [ ] **Step 2: Implement action-space constants**

```rust
pub(crate) const TILE_KIND_COUNT: usize = 34;

pub(crate) const TILE_KEYS: [&str; TILE_KIND_COUNT] = [
    "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9",
    "t1", "t2", "t3", "t4", "t5", "t6", "t7", "t8", "t9",
    "b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9",
    "east", "south", "west", "north", "red", "green", "white",
];

pub(crate) const CLAIM_ACTIONS: [&str; 7] = [
    "pass", "hu", "pung", "kong", "chow_left", "chow_mid", "chow_right",
];

pub(crate) const SELF_KONG_ACTIONS: [&str; 3] = [
    "pass", "concealed_kong", "add_kong",
];

pub(crate) fn tile_index(tile_key: &str) -> Option<usize> {
    TILE_KEYS.iter().position(|key| *key == tile_key)
}

pub(crate) fn tile_key_for_index(index: usize) -> Option<&'static str> {
    TILE_KEYS.get(index).copied()
}
```

Run: `cargo test bot::action_space -- --nocapture`  
Expected: PASS.

- [ ] **Step 3: Implement runtime encoder output struct**

```rust
#[derive(Clone, Debug)]
pub(crate) struct BotFeaturesV2 {
    pub(crate) tile_planes: Vec<f32>,
    pub(crate) scalar_features: Vec<f32>,
    pub(crate) discard_mask: [bool; TILE_KIND_COUNT],
    pub(crate) claim_mask: [bool; CLAIM_ACTION_COUNT],
    pub(crate) self_kong_mask: [bool; SELF_KONG_ACTION_COUNT],
    pub(crate) hu_mask: [bool; 2],
}

pub(crate) fn encode_bot_context_v2(context: &BotContext) -> BotFeaturesV2 {
    BotFeaturesV2 {
        tile_planes: encode_tile_planes(context),
        scalar_features: encode_scalar_features(context),
        discard_mask: legal_discard_mask(context),
        claim_mask: legal_claim_mask(context),
        self_kong_mask: legal_self_kong_mask(context),
        hu_mask: legal_hu_mask(context),
    }
}
```

Run: `cargo test bot::features -- --nocapture`  
Expected: PASS with masks matching `BotContextView`.

- [ ] **Step 4: Export modules**

```rust
// backend/src/bot/mod.rs
mod action_space;
mod context;
mod features;
mod neural;
mod policy;
mod search;

pub use policy::{choose_active_turn_action, choose_claim_action};
```

Run: `cargo test bot::action_space bot::features -- --nocapture`  
Expected: PASS.

## Task 2: Build BotZone Parser and Tile Mapper

**Files:**
- Create: `backend/src/bot_trainer.rs`
- Create: `backend/src/bot_trainer/botzone.rs`
- Modify: `backend/src/main.rs`
- Test: `backend/src/bot_trainer/botzone.rs`

- [ ] **Step 1: Add failing parser tests using `sample.txt` fragments**

```rust
#[test]
fn maps_botzone_tile_codes_to_backend_tile_keys() {
    assert_eq!(map_botzone_tile("W1"), Some("w1".to_string()));
    assert_eq!(map_botzone_tile("T9"), Some("t9".to_string()));
    assert_eq!(map_botzone_tile("B5"), Some("b5".to_string()));
    assert_eq!(map_botzone_tile("F1"), Some("east".to_string()));
    assert_eq!(map_botzone_tile("F2"), Some("north".to_string()));
    assert_eq!(map_botzone_tile("F3"), Some("west".to_string()));
    assert_eq!(map_botzone_tile("F4"), Some("south".to_string()));
    assert_eq!(map_botzone_tile("J1"), Some("red".to_string()));
    assert_eq!(map_botzone_tile("J2"), Some("green".to_string()));
    assert_eq!(map_botzone_tile("J3"), Some("white".to_string()));
}

#[test]
fn parses_ignore_claims_on_action_line() {
    let event = parse_event_line("Player 1 Hu B3 Ignore Player 0 PENG B3 Ignore Player 3 CHI B4")
        .expect("event");

    assert_eq!(event.actor, 1);
    assert_eq!(event.action, BotZoneAction::Hu { tile_key: "b3".to_string() });
    assert_eq!(event.ignored_claims.len(), 2);
    assert_eq!(event.ignored_claims[0].actor, 0);
    assert_eq!(event.ignored_claims[0].action, BotZoneAction::Peng { tile_key: "b3".to_string() });
    assert_eq!(event.ignored_claims[1].actor, 3);
}
```

Run: `cargo test bot_trainer::botzone -- --nocapture`  
Expected: FAIL because parser code does not exist.

- [ ] **Step 2: Implement typed BotZone records**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BotZoneMatch {
    pub(crate) match_id: String,
    pub(crate) round_wind: String,
    pub(crate) deals: [Vec<String>; 4],
    pub(crate) events: Vec<BotZoneEvent>,
    pub(crate) result: BotZoneResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BotZoneEvent {
    pub(crate) actor: usize,
    pub(crate) action: BotZoneAction,
    pub(crate) ignored_claims: Vec<BotZoneIgnoredClaim>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BotZoneIgnoredClaim {
    pub(crate) actor: usize,
    pub(crate) action: BotZoneAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BotZoneAction {
    Draw { tile_key: String },
    Play { tile_key: String },
    Chi { middle_tile_key: String },
    Peng { tile_key: String },
    Gang { tile_key: String },
    AnGang { tile_key: String },
    BuGang { tile_key: String },
    Hu { tile_key: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BotZoneResult {
    Hu { fan: i64, description: String, score_delta: [i64; 4] },
    Huang { score_delta: [i64; 4] },
}
```

Run: `cargo test bot_trainer::botzone -- --nocapture`  
Expected: parser tests still fail until line parsing is implemented.

- [ ] **Step 3: Implement tile mapping and event-line parsing**

```rust
pub(crate) fn map_botzone_tile(raw: &str) -> Option<String> {
    match raw {
        "F1" => Some("east".to_string()),
        "F2" => Some("north".to_string()),
        "F3" => Some("west".to_string()),
        "F4" => Some("south".to_string()),
        "J1" => Some("red".to_string()),
        "J2" => Some("green".to_string()),
        "J3" => Some("white".to_string()),
        _ => {
            let mut chars = raw.chars();
            let suit = match chars.next()? {
                'W' => 'w',
                'T' => 't',
                'B' => 'b',
                _ => return None,
            };
            let rank = chars.next()?;
            if chars.next().is_some() || !('1'..='9').contains(&rank) {
                return None;
            }
            Some(format!("{suit}{rank}"))
        }
    }
}
```

Run: `cargo test bot_trainer::botzone -- --nocapture`  
Expected: PASS for tile and event parsing.

- [ ] **Step 4: Wire the trainer module into the crate**

```rust
// backend/src/main.rs
mod app;
mod bot;
mod bot_trainer;
mod core;
mod mahjong;
mod projection;
mod room_scoring;
mod rules;
mod scoring;
```

Run: `cargo test bot_trainer::botzone -- --nocapture`  
Expected: PASS.

## Task 3: Replay Matches Into Backend-Like Decision Samples

**Files:**
- Create: `backend/src/bot_trainer/replay.rs`
- Create: `backend/src/bot_trainer/export.rs`
- Modify: `backend/src/bot_trainer.rs`
- Test: `backend/src/bot_trainer/replay.rs`

- [ ] **Step 1: Add failing replay tests for active-turn, claim, ignore, and pass labels**

```rust
#[test]
fn replay_emits_active_turn_discard_sample_before_play_event() {
    let record = parse_match(SIMPLE_MATCH).expect("match");
    let samples = replay_match_to_samples(&record).expect("samples");

    let first_discard = samples.iter()
        .find(|sample| sample.decision_kind == DecisionKind::ActiveTurn)
        .expect("active turn sample");

    assert_eq!(first_discard.seat_index, 0);
    assert_eq!(first_discard.label, TrainingLabel::Discard { tile_key: "t6".to_string() });
    assert!(first_discard.legal_actions.iter().any(|action| action == "discard:t6"));
}

#[test]
fn replay_treats_ignore_claim_as_positive_label() {
    let record = parse_match(IGNORE_CLAIM_MATCH).expect("match");
    let samples = replay_match_to_samples(&record).expect("samples");

    let ignored_claim = samples.iter()
        .find(|sample| sample.seat_index == 0 && sample.label == TrainingLabel::ClaimPung { tile_key: "b6".to_string() })
        .expect("ignored pung sample");

    assert_eq!(ignored_claim.decision_kind, DecisionKind::ClaimWindow);
    assert!(ignored_claim.legal_actions.iter().any(|action| action == "claim:pung:b6"));
}

#[test]
fn replay_emits_pass_for_unclaimed_legal_claim() {
    let record = parse_match(PASS_CLAIM_MATCH).expect("match");
    let samples = replay_match_to_samples(&record).expect("samples");

    let pass = samples.iter()
        .find(|sample| sample.seat_index == 3 && sample.legal_actions.iter().any(|action| action.starts_with("claim:chow")) && sample.label == TrainingLabel::Pass)
        .expect("legal chow pass sample");

    assert_eq!(pass.decision_kind, DecisionKind::ClaimWindow);
}
```

Run: `cargo test bot_trainer::replay -- --nocapture`  
Expected: FAIL because replay code does not exist.

- [ ] **Step 2: Implement serializable sample types**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum DecisionKind {
    ActiveTurn,
    ClaimWindow,
    RobKong,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum TrainingLabel {
    Discard { tile_key: String },
    ClaimChow { middle_tile_key: String },
    ClaimPung { tile_key: String },
    ClaimKong { tile_key: String },
    SelfKong { kind: String, tile_key: String },
    Hu,
    Pass,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TrainingDecisionSampleV2 {
    pub(crate) schema_version: u32,
    pub(crate) match_id: String,
    pub(crate) decision_index: u64,
    pub(crate) seat_index: usize,
    pub(crate) decision_kind: DecisionKind,
    pub(crate) context: SerializableBotContext,
    pub(crate) legal_actions: Vec<String>,
    pub(crate) label: TrainingLabel,
    pub(crate) outcome: SampleOutcome,
}
```

Run: `cargo test bot_trainer::replay -- --nocapture`  
Expected: compile errors only where replay logic is still missing.

- [ ] **Step 3: Implement replay state with backend tile ids**

```rust
fn tile_from_key(match_id: &str, seat: usize, sequence: usize, tile_key: &str) -> Tile {
    Tile {
        tile_id: format!("{match_id}:s{seat}:{tile_key}:{sequence}"),
        tile_key: tile_key.to_string(),
        kind: tile_kind(tile_key).to_string(),
        suit: tile_suit(tile_key).map(str::to_string),
        rank: tile_rank(tile_key),
        name: Some(tile_key.to_string()),
    }
}
```

Run: `cargo test bot_trainer::replay -- --nocapture`  
Expected: replay tests fail until decision extraction is implemented.

- [ ] **Step 4: Generate active-turn samples before applying true actions**

```rust
fn emit_active_turn_sample(
    state: &RoomState,
    match_id: &str,
    decision_index: &mut u64,
    seat_index: usize,
    label: TrainingLabel,
    outcome: SampleOutcome,
) -> Result<TrainingDecisionSampleV2, ReplayError> {
    let cache = RoomScoringCache::from_state(state);
    let self_kong_candidates = available_self_kongs_from_cache(&cache, seat_index);
    let add_kong_risk_tiles = add_kong_risk_tiles_for_state(state, seat_index, &self_kong_candidates);
    let context = build_bot_context_view(
        &cache,
        state,
        seat_index,
        Vec::new(),
        self_kong_candidates,
        add_kong_risk_tiles,
    ).ok_or(ReplayError::MissingContext)?;

    let legal_actions = active_turn_legal_actions(&context);
    let sample = TrainingDecisionSampleV2 {
        schema_version: 2,
        match_id: match_id.to_string(),
        decision_index: *decision_index,
        seat_index,
        decision_kind: DecisionKind::ActiveTurn,
        context: SerializableBotContext::from_context(&context),
        legal_actions,
        label,
        outcome,
    };
    *decision_index += 1;
    Ok(sample)
}
```

Run: `cargo test bot_trainer::replay -- --nocapture`  
Expected: active-turn test passes, claim tests still fail.

- [ ] **Step 5: Generate claim-window samples including ignored and pass labels**

```rust
fn emit_claim_samples_after_discard(
    state: &RoomState,
    match_id: &str,
    decision_index: &mut u64,
    discarder_seat: usize,
    discarded_tile_key: &str,
    declared_claims: &[DeclaredClaim],
    outcome: SampleOutcome,
) -> Result<Vec<TrainingDecisionSampleV2>, ReplayError> {
    let claim_window = claim_window_options_after_discard_in_room_state(
        state,
        discarder_seat,
        discarded_tile_key,
    );
    let cache = RoomScoringCache::from_state(state);
    let mut samples = Vec::new();

    for seat_index in 0..4 {
        if seat_index == discarder_seat {
            continue;
        }
        let Some(legal_claims) = claim_window.get(seat_index) else {
            continue;
        };
        if legal_claims.is_empty() {
            continue;
        }

        let label = declared_claims
            .iter()
            .find(|claim| claim.seat_index == seat_index)
            .map(|claim| claim.label.clone())
            .unwrap_or(TrainingLabel::Pass);
        let claim_options = build_claim_options(&cache, seat_index, legal_claims);
        let context = build_bot_context_view(
            &cache,
            state,
            seat_index,
            claim_options,
            Vec::new(),
            HashSet::new(),
        ).ok_or(ReplayError::MissingContext)?;

        samples.push(TrainingDecisionSampleV2 {
            schema_version: 2,
            match_id: match_id.to_string(),
            decision_index: *decision_index,
            seat_index,
            decision_kind: DecisionKind::ClaimWindow,
            context: SerializableBotContext::from_context(&context),
            legal_actions: claim_legal_actions(&context),
            label,
            outcome: outcome.clone(),
        });
        *decision_index += 1;
    }

    Ok(samples)
}
```

Run: `cargo test bot_trainer::replay -- --nocapture`  
Expected: PASS for active-turn, ignored claim, and legal pass tests.

## Task 4: Add Dataset Export CLI With Match-Based Splits

**Files:**
- Create: `backend/src/bin/export_bot_dataset_v2.rs`
- Modify: `backend/src/bot_trainer/export.rs`
- Test: `backend/src/bot_trainer/export.rs`

- [ ] **Step 1: Add failing split and metadata tests**

```rust
#[test]
fn split_assignment_is_stable_by_match_id() {
    let first = split_for_match_id("61602cb45ddc087351c04358");
    let second = split_for_match_id("61602cb45ddc087351c04358");
    assert_eq!(first, second);
}

#[test]
fn metadata_contains_v2_model_outputs() {
    let metadata = export_metadata();
    assert_eq!(metadata.schema_version, 2);
    assert!(metadata.model_outputs.contains(&"discard_logits".to_string()));
    assert!(metadata.model_outputs.contains(&"claim_logits".to_string()));
    assert!(metadata.model_outputs.contains(&"risk_logits".to_string()));
}
```

Run: `cargo test bot_trainer::export -- --nocapture`  
Expected: FAIL because export helpers do not exist.

- [ ] **Step 2: Implement exporter arguments and output layout**

```rust
struct ExportArgs {
    input_path: PathBuf,
    output_dir: PathBuf,
    max_matches: Option<usize>,
}

fn parse_args() -> Result<ExportArgs, anyhow::Error> {
    let mut input_path = PathBuf::from("backend/bot_trainer/dataset/data.txt");
    let mut output_dir = PathBuf::from("backend/bot_trainer/v2/out");
    let mut max_matches = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => input_path = PathBuf::from(args.next().ok_or_else(|| anyhow::anyhow!("--input requires a path"))?),
            "--output" => output_dir = PathBuf::from(args.next().ok_or_else(|| anyhow::anyhow!("--output requires a path"))?),
            "--max-matches" => {
                let value = args.next().ok_or_else(|| anyhow::anyhow!("--max-matches requires a number"))?;
                max_matches = Some(value.parse::<usize>()?);
            }
            _ => return Err(anyhow::anyhow!("unknown argument: {arg}")),
        }
    }

    Ok(ExportArgs { input_path, output_dir, max_matches })
}
```

Run: `cargo test bot_trainer::export -- --nocapture`  
Expected: metadata tests pass and CLI compiles.

- [ ] **Step 3: Write JSONL shards and reject illegal labels**

```rust
fn validate_sample(sample: &TrainingDecisionSampleV2) -> Result<(), ExportError> {
    let label_action = label_to_action_id(&sample.label);
    if !sample.legal_actions.iter().any(|action| action == &label_action) {
        return Err(ExportError::IllegalLabel {
            match_id: sample.match_id.clone(),
            decision_index: sample.decision_index,
            label: label_action,
            legal_actions: sample.legal_actions.clone(),
        });
    }
    Ok(())
}
```

Run: `cargo run --bin export_bot_dataset_v2 -- --input backend/bot_trainer/dataset/sample.txt --output backend/bot_trainer/v2/out_sample`  
Expected: command completes, writes `metadata.json`, `train.jsonl`, `val.jsonl`, `test.jsonl`, and `export_report.json`.

- [ ] **Step 4: Export the full dataset**

Run: `cargo run --release --bin export_bot_dataset_v2 -- --input backend/bot_trainer/dataset/data.txt --output backend/bot_trainer/v2/out`  
Expected: command completes with a report containing match count, sample count by decision kind, illegal label count `0`, parse error count, and skipped match ids if any.

## Task 5: Train Multi-Head Model and Export V2 ONNX

**Files:**
- Create: `backend/bot_trainer/v2/dataset.py`
- Create: `backend/bot_trainer/v2/model.py`
- Create: `backend/bot_trainer/v2/train.py`
- Create: `backend/bot_trainer/v2/export_onnx.py`
- Create: `backend/bot_trainer/v2/README.md`
- Replace: `backend/assets/models/mahjong_policy_net.onnx`

- [ ] **Step 1: Add dataset loader smoke command**

```python
def test_loads_one_exported_sample(export_dir: Path) -> None:
    dataset = MahjongDecisionDataset(export_dir / "train.jsonl", export_dir / "metadata.json")
    sample = dataset[0]

    assert sample["tile_planes"].shape[0] > 0
    assert sample["scalar_features"].ndim == 1
    assert sample["discard_mask"].shape == (34,)
    assert sample["claim_mask"].shape[0] == 7
```

Run: `python -m pytest backend/bot_trainer/v2 -q`  
Expected: FAIL until loader code exists.

- [ ] **Step 2: Implement PyTorch dataset loader**

```python
class MahjongDecisionDataset(Dataset):
    def __init__(self, jsonl_path: Path, metadata_path: Path) -> None:
        self.metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
        self.rows = [
            json.loads(line)
            for line in jsonl_path.read_text(encoding="utf-8").splitlines()
            if line.strip()
        ]

    def __len__(self) -> int:
        return len(self.rows)

    def __getitem__(self, index: int) -> dict[str, torch.Tensor]:
        row = self.rows[index]
        encoded = encode_row(row, self.metadata)
        return {
            key: torch.as_tensor(value)
            for key, value in encoded.items()
        }
```

Run: `python -m pytest backend/bot_trainer/v2 -q`  
Expected: dataset loader tests pass.

- [ ] **Step 3: Implement multi-head model**

```python
class MahjongPolicyNetV2(nn.Module):
    def __init__(self, tile_plane_count: int, scalar_count: int) -> None:
        super().__init__()
        self.tile_encoder = nn.Sequential(
            nn.Flatten(),
            nn.Linear(tile_plane_count * 34, 512),
            nn.ReLU(),
            nn.LayerNorm(512),
        )
        self.scalar_encoder = nn.Sequential(
            nn.Linear(scalar_count, 128),
            nn.ReLU(),
            nn.LayerNorm(128),
        )
        self.trunk = nn.Sequential(
            nn.Linear(640, 512),
            nn.ReLU(),
            nn.Dropout(0.1),
            nn.Linear(512, 256),
            nn.ReLU(),
        )
        self.discard_head = nn.Linear(256, 34)
        self.claim_head = nn.Linear(256, 7)
        self.self_kong_head = nn.Linear(256, 3)
        self.hu_head = nn.Linear(256, 2)
        self.value_head = nn.Linear(256, 1)
        self.risk_head = nn.Linear(256, 34)

    def forward(self, tile_planes: torch.Tensor, scalar_features: torch.Tensor) -> dict[str, torch.Tensor]:
        tile_embedding = self.tile_encoder(tile_planes)
        scalar_embedding = self.scalar_encoder(scalar_features)
        hidden = self.trunk(torch.cat([tile_embedding, scalar_embedding], dim=1))
        return {
            "discard_logits": self.discard_head(hidden),
            "claim_logits": self.claim_head(hidden),
            "self_kong_logits": self.self_kong_head(hidden),
            "hu_logits": self.hu_head(hidden),
            "value": self.value_head(hidden),
            "risk_logits": self.risk_head(hidden),
        }
```

Run: `python backend/bot_trainer/v2/train.py --data backend/bot_trainer/v2/out_sample --epochs 1 --batch-size 64 --output backend/bot_trainer/v2/checkpoints_smoke`  
Expected: one epoch completes and prints discard top-k, claim macro F1, hu recall, kong recall, and value loss.

- [ ] **Step 4: Train full model**

Run: `python backend/bot_trainer/v2/train.py --data backend/bot_trainer/v2/out --epochs 20 --batch-size 512 --output backend/bot_trainer/v2/checkpoints`  
Expected: best checkpoint saved with `metrics.json` containing validation metrics and model metadata.

- [ ] **Step 5: Export ONNX with named outputs**

Run: `python backend/bot_trainer/v2/export_onnx.py --checkpoint backend/bot_trainer/v2/checkpoints/best.pt --output backend/assets/models/mahjong_policy_net.onnx`  
Expected: `backend/assets/models/mahjong_policy_net.onnx` is replaced with v2 and `onnxruntime` smoke inference returns named outputs `discard_logits`, `claim_logits`, `self_kong_logits`, `hu_logits`, `value`, and `risk_logits`.

## Task 6: Replace Runtime Neural Policy With V2 Only

**Files:**
- Replace: `backend/src/bot/neural.rs`
- Modify: `backend/src/bot/policy.rs`
- Test: `backend/src/bot/neural.rs`
- Test: `backend/src/bot/policy.rs`

- [ ] **Step 1: Add failing tests that assert v1 symbols are gone**

```rust
#[test]
fn v2_session_outputs_named_multi_head_scores() {
    let context = base_context();
    let scores = neural_decision_scores(&context);

    if let Some(scores) = scores {
        assert_eq!(scores.discard_logits.len(), 34);
        assert_eq!(scores.claim_logits.len(), 7);
        assert_eq!(scores.self_kong_logits.len(), 3);
        assert_eq!(scores.hu_logits.len(), 2);
        assert_eq!(scores.risk_logits.len(), 34);
    }
}

#[test]
fn v2_masking_never_selects_illegal_discard() {
    let mut context = base_context();
    context.player.concealed_tiles = vec![tile("w1#0", "w1"), tile("t1#0", "t1")];
    context.restricted_discard_tile_key = Some("w1".to_string());

    let logits = [100.0_f32, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let ranked = rank_masked_discards(&context, &logits);

    assert_eq!(ranked[0].tile_key, "t1");
}
```

Run: `cargo test bot::neural -- --nocapture`  
Expected: FAIL while old `neural_discard_scores` and v1 feature code are still present.

- [ ] **Step 2: Replace `neural.rs` with v2 session API**

```rust
pub(crate) struct NeuralDecisionScores {
    pub(crate) discard_logits: [f32; TILE_KIND_COUNT],
    pub(crate) claim_logits: [f32; CLAIM_ACTION_COUNT],
    pub(crate) self_kong_logits: [f32; SELF_KONG_ACTION_COUNT],
    pub(crate) hu_logits: [f32; 2],
    pub(crate) value: f32,
    pub(crate) risk_logits: [f32; TILE_KIND_COUNT],
}

pub(crate) fn neural_decision_scores(context: &BotContext) -> Option<NeuralDecisionScores> {
    if !neural_policy_enabled() {
        return None;
    }
    let features = encode_bot_context_v2(context);
    shared_session().lock().ok()?.run(features).ok()
}
```

Remove these v1-only items from `backend/src/bot/neural.rs`:

```rust
const TRAINER_FEATURE_LAYERS: usize = 11;
const TRAINER_FEATURE_LEN: usize = TRAINER_FEATURE_LAYERS * TILE_KIND_COUNT;
fn build_trainer_features(context: &BotContext) -> Vec<f32>;
fn trainer_index_for_tile_key(tile_key: &str) -> Option<usize>;
fn trainer_index_for_backend_index(backend_index: usize) -> Option<usize>;
fn layer_offset(layer: usize, trainer_index: usize) -> usize;
pub(crate) fn neural_discard_scores(context: &BotContext) -> Option<Vec<NeuralDiscardScore>>;
```

Run: `cargo test bot::neural -- --nocapture`  
Expected: PASS, with no v1 feature test remaining.

- [ ] **Step 3: Update policy to use v2 decision scores**

```rust
fn select_neural_v2_discard(
    context: &BotContext,
    search_plans: &[BotDiscardPlan],
) -> Option<BotDiscardPlan> {
    let scores = neural_decision_scores(context)?;
    select_hybrid_discard_plan_v2(search_plans, &scores.discard_logits, &scores.risk_logits, neural_prior_weight())
}

fn select_neural_v2_claim(context: &BotContext, pass_score: i64, best_search_claim: Option<(BotAction, i64)>) -> Option<BotAction> {
    let scores = neural_decision_scores(context)?;
    let masked = rank_masked_claims(context, &scores.claim_logits);
    let best = masked.first()?;
    if best.action_type == "pass" {
        return Some(BotAction { seat_index: context.seat_index, action_type: "pass".to_string(), tile_ids: vec![] });
    }
    neural_claim_to_bot_action(context, best, pass_score, best_search_claim)
}
```

Run: `cargo test bot::policy -- --nocapture`  
Expected: PASS, and claim/discard policy tests compile against v2 API only.

## Task 7: Delete Legacy V1 Assets and Config References

**Files:**
- Replace: `backend/assets/models/mahjong_policy_net.onnx`
- Modify: `.env.example`
- Modify: `docker-compose.yml`
- Modify: `docker-compose.prebuilt.yml`
- Test: repository search output

- [ ] **Step 1: Remove v1 naming and comments from environment examples**

```env
MAHJONG_BOT_POLICY=neural
MAHJONG_BOT_MODEL_PATH=/app/assets/models/mahjong_policy_net.onnx
MAHJONG_BOT_NEURAL_WEIGHT=15
MAHJONG_BOT_STRENGTH=strong
```

Run: `rg -n "legacy|v1|TRAINER_FEATURE|build_trainer_features|trainer_index_for_backend_index|hybrid" backend docker-compose.yml docker-compose.prebuilt.yml .env.example`  
Expected: no v1 trainer symbols; `hybrid` may remain only if the policy mode still intentionally means neural-plus-search and is documented as v2 hybrid, not legacy v1.

- [ ] **Step 2: Verify only one production ONNX asset remains**

Run: `Get-ChildItem backend/assets/models -Filter *.onnx | Select-Object -ExpandProperty Name`  
Expected: exactly `mahjong_policy_net.onnx`.

- [ ] **Step 3: Run backend inference smoke test**

Run: `cargo test bot::neural::tests::runs_local_onnx_model_when_available -- --nocapture`  
Expected: PASS against the v2 ONNX model, validating all six named outputs.

## Task 8: Final Verification and Documentation

**Files:**
- Modify: `backend/bot_trainer/v2/README.md`
- Modify: `docs/superpowers/plans/2026-04-28-bot-training-v2.md`

- [ ] **Step 1: Document exact exporter and trainer commands**

````markdown
## Commands

```powershell
cargo run --release --bin export_bot_dataset_v2 -- --input backend/bot_trainer/dataset/data.txt --output backend/bot_trainer/v2/out
python backend/bot_trainer/v2/train.py --data backend/bot_trainer/v2/out --epochs 20 --batch-size 512 --output backend/bot_trainer/v2/checkpoints
python backend/bot_trainer/v2/export_onnx.py --checkpoint backend/bot_trainer/v2/checkpoints/best.pt --output backend/assets/models/mahjong_policy_net.onnx
cargo test bot::neural bot::policy bot_trainer -- --nocapture
```
````

Run: `Get-Content backend/bot_trainer/v2/README.md`  
Expected: README includes exporter, trainer, ONNX export, runtime environment variables, and validation commands.

- [ ] **Step 2: Run targeted verification**

Run: `cargo test bot_trainer -- --nocapture`  
Expected: PASS.

Run: `cargo test bot::action_space bot::features bot::neural bot::policy -- --nocapture`  
Expected: PASS.

Run: `cargo run --bin export_bot_dataset_v2 -- --input backend/bot_trainer/dataset/sample.txt --output backend/bot_trainer/v2/out_sample`  
Expected: PASS and `export_report.json` has `illegal_label_count` equal to `0`.

Run: `python backend/bot_trainer/v2/train.py --data backend/bot_trainer/v2/out_sample --epochs 1 --batch-size 64 --output backend/bot_trainer/v2/checkpoints_smoke`  
Expected: PASS with all output heads reporting finite losses.

- [ ] **Step 3: Run full regression before claiming completion**

Run: `cargo test`  
Expected: PASS, or only unrelated pre-existing failures with all v2 bot tests passing.

Run: `python -m pytest backend/bot_trainer/v2 -q`  
Expected: PASS.

Run: `rg -n "build_trainer_features|TRAINER_FEATURE_LAYERS|trainer_index_for_backend_index|neural_discard_scores" backend/src`  
Expected: no matches.

- [ ] **Step 4: Completion summary checklist**

```text
Done only when:
- `data.txt` can be exported into v2 shards by match split.
- Export report confirms legal labels for active-turn, claim, ignore, pass, kong, and hu samples.
- Python smoke training runs from exported sample shards.
- ONNX exports six named v2 outputs.
- Backend runtime consumes only v2 features and v2 ONNX outputs.
- v1 trainer feature code and v1-only tests are deleted.
- The old discard-only model path is replaced, not duplicated.
```

Run: `git status --short`  
Expected: changes are limited to the planned bot training, bot runtime, model asset, and documentation files.

---

## Execution Notes

- Full export completed at `backend/bot_trainer/v2/out/export_report.json`.
- Full export report:
  - matches: `98209`
  - samples: `5846387`
  - train/val/test: `4679967` / `579593` / `586827`
  - active_turn/claim_window: `4600401` / `1245986`
  - illegal labels: `0`
  - parse errors: `0`
- Full export command with progress:

```powershell
.\backend\bot_trainer\v2\export_full_dataset.ps1 -ProgressEvery 100
```

- RTX A3000 GPU training and ONNX export command:

```powershell
.\backend\bot_trainer\v2\train_and_export_model.ps1 -Epochs 20 -BatchSize 2048 -Device cuda -NumWorkers 4
```

- GPU smoke verification completed on sample data with CUDA AMP and Rust ONNX loading.
