# Mahjong Hand Insight Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move waits and recommended high-fan insights to backend snapshot projections, remove the frontend-local ready-hand derivation path, and render the unified dock popover from server data for both current-hand and selected-discard previews.

**Architecture:** Backend adds a dedicated `projection::hand_insight` builder that reads `RoomScoringCache`, reuses scoring metadata plus real win evaluation, and projects `current` plus `by_discard_tile_id` insight objects through `room_snapshot`. Frontend expands snapshot/view-model types, selects the active insight based on the single selected tile, updates the dock popover to render waits plus recommendations, and deletes the old `readyHand` derivation files.

**Tech Stack:** Rust 2024 backend (`serde`, scoring evaluator, room snapshot projection), React 19 + TypeScript + Vitest frontend, CSS in `frontend/src/styles/dock.css`

---

## File Map

- Create: `backend/src/projection/hand_insight.rs`
  Responsibility: build backend hand insight snapshots, including waits, recommendation ranking, and selected-discard previews.
- Modify: `backend/src/projection/mod.rs`
  Responsibility: export the new projection module.
- Modify: `backend/src/projection/room_snapshot.rs`
  Responsibility: serialize `hand_insights` into the local player private snapshot and cover snapshot-level tests.
- Modify: `backend/src/rules/scoring/evaluator.rs`
  Responsibility: expose recommendable fan-rule metadata to projections without duplicating the fan table.
- Modify: `backend/src/rules/scoring/mod.rs`
  Responsibility: re-export any new evaluator helpers needed by projections.
- Modify: `backend/src/rules/standard/win.rs`
  Responsibility: expose meld open/closed classification helpers so projection code can reuse the same win-evaluation semantics.
- Modify: `frontend/src/types/match.ts`
  Responsibility: add backend snapshot types for `hand_insights`, add battle-view types for the dock popover, and rename the view-model field from `readyHandInsight` to `handInsight`.
- Modify: `frontend/src/lib/matchViewModel.ts`
  Responsibility: map backend `hand_insights` into the active dock insight, update `ready_hand` button enablement, and remove local waits derivation.
- Modify: `frontend/src/lib/matchViewModel.test.ts`
  Responsibility: assert current vs selected-discard snapshot selection and `ready_hand` enablement from backend data.
- Modify: `frontend/src/components/battle-screen/BattleScreen.tsx`
  Responsibility: pass the renamed `handInsight` prop into the dock.
- Modify: `frontend/src/components/battle-screen/BattleScreen.test.tsx`
  Responsibility: keep the screen-level view-model fixture aligned with the `handInsight` rename.
- Modify: `frontend/src/components/battle-screen/BottomActionDock.tsx`
  Responsibility: render waits plus recommendation sections, update trigger labels, and show the non-tenpai black `i` state.
- Modify: `frontend/src/components/battle-screen/BottomActionDock.test.tsx`
  Responsibility: assert new trigger text, recommendation rendering, tenpai rendering, and selected-discard labels.
- Modify: `frontend/src/styles/dock.css`
  Responsibility: style the black translucent trigger, split-section popover, and recommendation rows.
- Delete: `frontend/src/lib/readyHand.ts`
  Responsibility: remove the old frontend-only waits calculator.
- Delete: `frontend/src/lib/readyHand.test.ts`
  Responsibility: remove tests for deleted local logic.

## Task 1: Backend Hand Insight Projection

**Files:**
- Create: `backend/src/projection/hand_insight.rs`
- Modify: `backend/src/projection/mod.rs`
- Modify: `backend/src/projection/room_snapshot.rs`
- Modify: `backend/src/rules/scoring/evaluator.rs`
- Modify: `backend/src/rules/scoring/mod.rs`
- Modify: `backend/src/rules/standard/win.rs`
- Test: `backend/src/projection/hand_insight.rs`
- Test: `backend/src/projection/room_snapshot.rs`

- [ ] **Step 1: Write failing backend projection tests**

```rust
#[test]
fn local_snapshot_projects_current_and_discard_preview_hand_insights() {
    let mut state = sample_state();
    let round = state.round_state.as_mut().expect("round");
    round.players[0].concealed_tiles = vec![
        suit_tile("w1#0", "w1"),
        suit_tile("w2#0", "w2"),
        suit_tile("w3#0", "w3"),
        suit_tile("w4#0", "w4"),
        suit_tile("w5#0", "w5"),
        suit_tile("w6#0", "w6"),
        suit_tile("w7#0", "w7"),
        suit_tile("w8#0", "w8"),
        suit_tile("w9#0", "w9"),
        suit_tile("t1#0", "t1"),
        suit_tile("t2#0", "t2"),
        suit_tile("t3#0", "t3"),
        suit_tile("t4#0", "t4"),
        suit_tile("b9#0", "b9"),
    ];
    state.pending_timeout = Some(PendingTimeout {
        kind: "active_turn".to_string(),
        seat_index: 0,
        deadline_at: None,
        drawn_tile_id: Some("b9#0".to_string()),
    });

    let support = SeatProjectionSupport {
        can_ready_hand: true,
        ..Default::default()
    };
    let snapshot = room_snapshot_message(&state, 0, &support);
    let hand_insights = &snapshot["payload"]["private_state"]["hand_insights"];

    assert!(hand_insights["current"]["recommendations"].is_array());
    assert_eq!(
        hand_insights["by_discard_tile_id"]["b9#0"]["is_tenpai"],
        serde_json::json!(true)
    );
    assert_eq!(
        hand_insights["by_discard_tile_id"]["b9#0"]["waits"],
        serde_json::json!([
            { "code": "t1", "available_count": 2 },
            { "code": "t4", "available_count": 3 }
        ])
    );
}

#[test]
fn open_meld_hand_drops_closed_only_recommendations() {
    let mut state = sample_state();
    state.round_state.as_mut().unwrap().players[0].melds = vec![vec![
        "w3".to_string(),
        "w4".to_string(),
        "w5".to_string(),
    ]];

    let insights = build_hand_insights_view(&state, 0, &SeatProjectionSupport::default())
        .expect("local player should still receive insights");
    let keys = insights.current.expect("current insight").recommendations
        .into_iter()
        .map(|entry| entry.fan_key)
        .collect::<Vec<_>>();

    assert!(!keys.iter().any(|key| key == "fully_concealed_hand"));
    assert!(!keys.iter().any(|key| key == "seven_pairs"));
}

fn suit_tile(tile_id: &str, tile_key: &str) -> Tile {
    Tile {
        tile_id: tile_id.to_string(),
        tile_key: tile_key.to_string(),
        kind: "suit".to_string(),
        suit: None,
        rank: None,
        name: None,
    }
}

fn sample_state() -> RoomState {
    RoomState {
        table_code: "ROOM42".to_string(),
        phase: "playing".to_string(),
        mode: "normal".to_string(),
        seats: (0..4)
            .map(|seat_index| SeatState {
                seat_index,
                connected: true,
                ready: true,
                seat_type: "human".to_string(),
                ..Default::default()
            })
            .collect(),
        match_state: None,
        round_state: Some(RoundState {
            round_id: "round-1".to_string(),
            dealer_seat: 0,
            round_wind: "east".to_string(),
            current_actor: 0,
            phase: "playing".to_string(),
            players: (0..4)
                .map(|seat| PlayerRoundState {
                    seat,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }),
        pending_timeout: None,
        continue_action: None,
    }
}
```

Run: `cargo test projection::hand_insight -- --nocapture`  
Expected: FAIL with missing `build_hand_insights_view`, missing serialized `hand_insights`, or missing recommendation metadata accessors.

- [ ] **Step 2: Expose recommendable fan metadata and meld-open helpers**

```rust
// backend/src/rules/scoring/evaluator.rs
impl FanRule {
    pub(crate) fn fan_key(&self) -> &'static str {
        self.fan_key
    }

    pub(crate) fn fan_value(&self) -> i64 {
        self.fan_value
    }
}

pub(crate) fn recommendable_fan_rules(min_value: i64) -> Vec<(&'static str, i64)> {
    registered_fan_rules()
        .iter()
        .filter(|rule| rule.fan_value() >= min_value && rule.fan_key() != "chicken_hand")
        .map(|rule| (rule.fan_key(), rule.fan_value()))
        .collect()
}
```

```rust
// backend/src/rules/scoring/mod.rs
pub use evaluator::{
    StandardScoreEvaluator, decompose_winning_hand, decompose_winning_hand_with_melds,
    evaluate_fans, extract_hand_features, is_winning_hand, recommendable_fan_rules,
};
```

```rust
// backend/src/rules/standard/win.rs
pub(crate) fn classify_meld_groups_for_projection(
    seat_index: usize,
    meld_tile_key_groups: &[Vec<String>],
    kong_entries: &[ScoringKongEntry],
) -> (Vec<Vec<String>>, Vec<bool>) {
    classify_meld_groups(seat_index, meld_tile_key_groups, kong_entries)
}
```

Run: `cargo test projection::hand_insight -- --nocapture`  
Expected: FAIL in the new projection builder or room snapshot wiring, but compile past fan-rule and meld-helper access.

- [ ] **Step 3: Implement `projection::hand_insight` builder with waits and recommendation ranking**

```rust
// backend/src/projection/hand_insight.rs
#[derive(Debug, Clone, Serialize)]
pub(crate) struct HandInsightsView {
    current: Option<HandInsightView>,
    by_discard_tile_id: BTreeMap<String, HandInsightView>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HandInsightView {
    discard_tile_id: Option<String>,
    discard_tile_code: Option<String>,
    is_tenpai: bool,
    waits: Vec<HandInsightWaitView>,
    recommendations: Vec<HandInsightRecommendationView>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HandInsightWaitView {
    code: String,
    available_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HandInsightRecommendationView {
    fan_key: String,
    fan_value: i64,
    similarity_percent: i64,
}

pub(crate) fn build_hand_insights_view(
    state: &RoomState,
    local_seat: Seat,
    support: &SeatProjectionSupport,
) -> Option<HandInsightsView> {
    let cache = RoomScoringCache::from_state(state);
    let player = cache.player(local_seat)?;
    if player.concealed_tiles.is_empty() {
        return None;
    }

    let current = build_current_insight(state, &cache, local_seat, support);
    let by_discard_tile_id = build_discard_preview_map(state, &cache, local_seat, support);

    Some(HandInsightsView {
        current,
        by_discard_tile_id,
    })
}
```

```rust
fn build_recommendations(
    state: &RoomState,
    cache: &RoomScoringCache,
    seat_index: usize,
    discard_tile: Option<(&str, &str)>,
    waits: &[HandInsightWaitView],
) -> Vec<HandInsightRecommendationView> {
    let candidates = recommendable_fan_rules(4);
    let exact_mode = !waits.is_empty();

    let mut recommendations = candidates
        .into_iter()
        .filter_map(|(fan_key, fan_value)| {
            let similarity_percent = if exact_mode {
                exact_similarity_percent(state, cache, seat_index, discard_tile, waits, fan_key)
            } else {
                heuristic_similarity_percent(state, cache, seat_index, discard_tile, fan_key)
            };

            (similarity_percent >= 20).then(|| HandInsightRecommendationView {
                fan_key: fan_key.to_string(),
                fan_value,
                similarity_percent,
            })
        })
        .collect::<Vec<_>>();

    recommendations.sort_by(|left, right| {
        right.similarity_percent.cmp(&left.similarity_percent)
            .then_with(|| right.fan_value.cmp(&left.fan_value))
            .then_with(|| left.fan_key.cmp(&right.fan_key))
    });
    recommendations.truncate(6);
    recommendations
}
```

Run: `cargo test projection::hand_insight -- --nocapture`  
Expected: FAIL only in snapshot integration or JSON field shape mismatches.

- [ ] **Step 4: Wire `hand_insights` into the room snapshot**

```rust
// backend/src/projection/mod.rs
pub mod hand_insight;
```

```rust
// backend/src/projection/room_snapshot.rs
use crate::projection::hand_insight::{HandInsightsView, build_hand_insights_view};

#[derive(Debug, Clone, Serialize)]
struct PlayerRoundView {
    round_id: String,
    round_wind: String,
    dealer_seat: Seat,
    current_actor: Seat,
    wall_tiles_remaining: usize,
    last_discard: Option<String>,
    pending_action: Option<PendingActionView>,
    hand_insights: Option<HandInsightsView>,
    score_state: ScoreStateView,
    players: Vec<PlayerSeatView>,
}

Some(PlayerRoundView {
    round_id: round.round_id.clone(),
    round_wind: round.round_wind.clone(),
    dealer_seat: round.dealer_seat,
    current_actor: round.current_actor,
    wall_tiles_remaining: round.wall.live_tiles_remaining(),
    last_discard: round.last_discard.as_ref().map(|tile| tile.tile_key.clone()),
    pending_action: build_pending_action_view(state, local_seat, support),
    hand_insights: build_hand_insights_view(state, local_seat, support),
    score_state: score_state_view(state),
    players: private_players,
})
```

Run: `cargo test projection::room_snapshot::tests -- --nocapture`  
Expected: PASS with snapshot JSON now containing `hand_insights` and its preview data.

- [ ] **Step 5: Run the backend regression slice**

Run: `cargo test projection::hand_insight -- --nocapture`  
Expected: PASS

Run: `cargo test projection::room_snapshot::tests -- --nocapture`  
Expected: PASS

Run: `cargo test rules::standard::win::tests -- --nocapture`  
Expected: PASS, confirming the extracted helper reuse did not break settlement evaluation.

## Task 2: Frontend Types and View-Model Migration

**Files:**
- Modify: `frontend/src/types/match.ts`
- Modify: `frontend/src/lib/matchViewModel.ts`
- Modify: `frontend/src/lib/matchViewModel.test.ts`
- Modify: `frontend/src/components/battle-screen/BattleScreen.tsx`
- Modify: `frontend/src/components/battle-screen/BattleScreen.test.tsx`
- Test: `frontend/src/lib/matchViewModel.test.ts`
- Test: `frontend/src/components/battle-screen/BattleScreen.test.tsx`

- [ ] **Step 1: Write failing view-model tests against backend-provided insights**

```ts
it('prefers the selected discard hand insight from the snapshot payload', () => {
  const base = createPlayingSessionState();
  const selectedTileId = 'b9#0';
  const viewModel = createMatchViewModel({
    ...base,
    selectedTileIds: [selectedTileId],
    roomSnapshot: {
      ...base.roomSnapshot!,
      payload: {
        ...base.roomSnapshot!.payload,
        private_state: {
          ...base.roomSnapshot!.payload.private_state!,
          hand_insights: {
            current: {
              discard_tile_id: null,
              discard_tile_code: null,
              is_tenpai: false,
              waits: [],
              recommendations: [{ fan_key: 'full_flush', fan_value: 24, similarity_percent: 61 }],
            },
            by_discard_tile_id: {
              [selectedTileId]: {
                discard_tile_id: selectedTileId,
                discard_tile_code: 'b9',
                is_tenpai: true,
                waits: [{ code: 't4', available_count: 3 }],
                recommendations: [{ fan_key: 'full_flush', fan_value: 24, similarity_percent: 83 }],
              },
            },
          },
        },
      },
    },
  });

  expect(viewModel.handInsight).toEqual({
    source: 'selected_discard',
    discardTileId: selectedTileId,
    discardTileCode: 'b9',
    isTenpai: true,
    waits: [{ code: 't4', availableCount: 3 }],
    recommendations: [{ fanKey: 'full_flush', fanValue: 24, similarityPercent: 83 }],
  });
});

it('enables ready_hand only when the selected discard preview is tenpai', () => {
  const base = createPlayingSessionState();
  const selectedTileId = 'b9#0';
  const viewModel = createMatchViewModel({
    ...base,
    selectedTileIds: [selectedTileId],
    roomSnapshot: {
      ...base.roomSnapshot!,
      payload: {
        ...base.roomSnapshot!.payload,
        private_state: {
          ...base.roomSnapshot!.payload.private_state!,
          hand_insights: {
            current: null,
            by_discard_tile_id: {
              [selectedTileId]: {
                discard_tile_id: selectedTileId,
                discard_tile_code: 'b9',
                is_tenpai: true,
                waits: [{ code: 't4', available_count: 3 }],
                recommendations: [
                  { fan_key: 'full_flush', fan_value: 24, similarity_percent: 83 },
                ],
              },
            },
          },
        },
      },
    },
  });

  expect(viewModel.actions.find((action) => action.id === 'ready_hand')?.enabled).toBe(true);
});
```

Run: `npm test -- src/lib/matchViewModel.test.ts`  
Expected: FAIL with missing `hand_insights` types or the old `readyHandInsight` field still being used.

- [ ] **Step 2: Add backend snapshot types and battle-view types**

```ts
// frontend/src/types/match.ts
export interface BackendHandInsightWait {
  code: string;
  available_count: number;
}

export interface BackendHandInsightRecommendation {
  fan_key: string;
  fan_value: number;
  similarity_percent: number;
}

export interface BackendHandInsight {
  discard_tile_id: string | null;
  discard_tile_code: string | null;
  is_tenpai: boolean;
  waits: BackendHandInsightWait[];
  recommendations: BackendHandInsightRecommendation[];
}

export interface BackendHandInsights {
  current: BackendHandInsight | null;
  by_discard_tile_id: Record<string, BackendHandInsight>;
}

export interface HandInsightWaitView {
  code: string;
  availableCount: number;
}

export interface HandInsightRecommendationView {
  fanKey: string;
  fanValue: number;
  similarityPercent: number;
}

export interface HandInsightView {
  source: 'current' | 'selected_discard';
  discardTileId: string | null;
  discardTileCode: string | null;
  isTenpai: boolean;
  waits: HandInsightWaitView[];
  recommendations: HandInsightRecommendationView[];
}
```

```ts
export interface PrivateState {
  round_id: string;
  round_wind: 'east' | 'south' | 'west' | 'north';
  dealer_seat: number;
  current_actor: number;
  wall_tiles_remaining?: number;
  last_discard?: string | null;
  pending_action?: PendingAction | null;
  hand_insights?: BackendHandInsights | null;
  score_state?: ScoreState | null;
  players: PrivatePlayerState[];
}

export interface BattleViewModel {
  // ...
  handInsight: HandInsightView | null;
  // ...
}
```

Run: `npm test -- src/lib/matchViewModel.test.ts`  
Expected: FAIL inside the old selector path, but compile with the new snapshot and view-model types.

- [ ] **Step 3: Replace local waits derivation with snapshot selection logic**

```ts
// frontend/src/lib/matchViewModel.ts
function mapBackendHandInsight(
  insight: BackendHandInsight,
  source: HandInsightView['source'],
): HandInsightView {
  return {
    source,
    discardTileId: insight.discard_tile_id,
    discardTileCode: insight.discard_tile_code,
    isTenpai: insight.is_tenpai,
    waits: insight.waits.map((wait) => ({
      code: wait.code,
      availableCount: wait.available_count,
    })),
    recommendations: insight.recommendations.map((item) => ({
      fanKey: item.fan_key,
      fanValue: item.fan_value,
      similarityPercent: item.similarity_percent,
    })),
  };
}

function createHandInsight(state: SessionState): BattleViewModel['handInsight'] {
  if (hasOptimisticDiscardPending(state)) {
    return null;
  }

  const handInsights = state.roomSnapshot?.payload.private_state?.hand_insights;
  if (!handInsights) {
    return null;
  }

  const selectedTileId = state.selectedTileIds.length === 1 ? state.selectedTileIds[0] : null;
  const selectedPreview = selectedTileId ? handInsights.by_discard_tile_id[selectedTileId] : undefined;
  if (selectedPreview) {
    return mapBackendHandInsight(selectedPreview, 'selected_discard');
  }

  return handInsights.current ? mapBackendHandInsight(handInsights.current, 'current') : null;
}
```

```ts
const selectedInsight =
  state.selectedTileIds.length === 1
    ? state.roomSnapshot?.payload.private_state?.hand_insights?.by_discard_tile_id[state.selectedTileIds[0]] ?? null
    : null;
const canReadyHandFromSelection =
  !localReadyHandLocked &&
  Boolean(selectedInsight?.is_tenpai) &&
  !restrictedDiscardTileIdSet.has(selectedReadyHandTileId as string);
```

```ts
// createMatchViewModel return object
handInsight: createHandInsight(state),
```

Run: `npm test -- src/lib/matchViewModel.test.ts`  
Expected: PASS, with `ready_hand` enablement and dock insight selection both sourced from backend snapshot data.

- [ ] **Step 4: Rename the BattleScreen prop pass-through and screen fixture field**

```tsx
// frontend/src/components/battle-screen/BattleScreen.tsx
<BottomActionDock
  hand={viewModel.localHand}
  selectedTileCode={viewModel.selectedTileCode}
  handInsight={viewModel.handInsight}
  claimCandidates={viewModel.claimCandidates}
  actions={battleActions}
  isElevated={viewModel.isActionDockElevated}
  isWaitingForMatchStart={Boolean(viewModel.waitingControls)}
  promptCue={viewModel.promptCue}
  deadlineAt={viewModel.deadlineAt}
  onTileSelect={onTileSelect}
  onTileDoubleClick={onTileDoubleClick}
  onClaimCandidateSelect={onClaimCandidateSelect}
  onClaimCandidateActivate={onClaimCandidateActivate}
  onAction={handleAction}
/>
```

```tsx
// frontend/src/components/battle-screen/BattleScreen.test.tsx
function createBattleViewModel(overrides: Partial<BattleViewModel> = {}): BattleViewModel {
  return {
    // ...
    handInsight: null,
    claimCandidates: [],
    // ...
    ...overrides,
  };
}
```

Run: `npm test -- src/lib/matchViewModel.test.ts src/components/battle-screen/BattleScreen.test.tsx`  
Expected: PASS, with the renamed field propagated through the screen component.

## Task 3: Dock Popover UI for Waits and Recommendations

**Files:**
- Modify: `frontend/src/components/battle-screen/BottomActionDock.tsx`
- Modify: `frontend/src/components/battle-screen/BottomActionDock.test.tsx`
- Modify: `frontend/src/styles/dock.css`
- Test: `frontend/src/components/battle-screen/BottomActionDock.test.tsx`

- [ ] **Step 1: Write failing dock tests for the new popover sections**

```tsx
it('renders recommendations without waits for a non-tenpai insight', async () => {
  const user = userEvent.setup();

  render(
    <BottomActionDock
      hand={localHand}
      handInsight={{
        source: 'current',
        discardTileId: null,
        discardTileCode: null,
        isTenpai: false,
        waits: [],
        recommendations: [
          { fanKey: 'full_flush', fanValue: 24, similarityPercent: 79 },
          { fanKey: 'all_pungs', fanValue: 6, similarityPercent: 56 },
        ],
      }}
      claimCandidates={[]}
      actions={[]}
      isElevated={false}
      promptCue={null}
      deadlineAt={null}
      onTileSelect={vi.fn()}
      onTileDoubleClick={vi.fn()}
      onClaimCandidateSelect={vi.fn()}
      onClaimCandidateActivate={vi.fn()}
      onAction={vi.fn()}
    />,
  );

  await user.click(screen.getByRole('button', { name: '查看当前推荐番型' }));

  expect(screen.getByText('清一色')).toBeInTheDocument();
  expect(screen.getByText('79%')).toBeInTheDocument();
  expect(screen.queryByText('正在听')).toBeNull();
});

it('renders waits and recommendations for a selected-discard tenpai preview', async () => {
  const user = userEvent.setup();

  render(
    <BottomActionDock
      hand={localHand}
      selectedTileCode="w2"
      handInsight={{
        source: 'selected_discard',
        discardTileId: 'w2#2',
        discardTileCode: 'w2',
        isTenpai: true,
        waits: [{ code: 'w3', availableCount: 2 }],
        recommendations: [{ fanKey: 'full_flush', fanValue: 24, similarityPercent: 83 }],
      }}
      claimCandidates={[]}
      actions={[]}
      isElevated={false}
      promptCue={null}
      deadlineAt={null}
      onTileSelect={vi.fn()}
      onTileDoubleClick={vi.fn()}
      onClaimCandidateSelect={vi.fn()}
      onClaimCandidateActivate={vi.fn()}
      onAction={vi.fn()}
    />,
  );

  await user.click(screen.getByRole('button', { name: '查看打出当前选中牌后的手牌洞察' }));

  expect(screen.getByText('打出后将听')).toBeInTheDocument();
  expect(screen.getByText('推荐番型')).toBeInTheDocument();
  expect(screen.getByText('清一色')).toBeInTheDocument();
});
```

Run: `npm test -- src/components/battle-screen/BottomActionDock.test.tsx`  
Expected: FAIL because the component still expects `readyHandInsight` and has no recommendation layout.

- [ ] **Step 2: Render the new insight model in the dock**

```tsx
// frontend/src/components/battle-screen/BottomActionDock.tsx
import { getFanLabel } from './fanGuide';

interface BottomActionDockProps {
  hand: BattleViewModel['localHand'];
  selectedTileCode?: string | null;
  handInsight?: BattleViewModel['handInsight'];
  claimCandidates: BattleViewModel['claimCandidates'];
  actions: BattleActionView[];
  isElevated: boolean;
  isWaitingForMatchStart?: boolean;
  promptCue: BattlePromptView | null;
  deadlineAt: string | null;
  onTileSelect: (tileId: string) => void;
  onTileDoubleClick: (tileId: string) => void;
  onClaimCandidateSelect: (actionId: ClaimActionId, tileIds: string[]) => void;
  onClaimCandidateActivate: (actionId: ClaimActionId, tileIds: string[]) => void;
  onAction: (actionId: BattleActionView['id']) => void;
}

const handInsightControl = handInsight ? (
  <div
    ref={readyHandPopoverRef}
    className="action-dock__ready-hand-anchor"
    onMouseEnter={() => setIsReadyHandPopoverHovered(true)}
    onMouseLeave={() => setIsReadyHandPopoverHovered(false)}
  >
    <button
      type="button"
      className={[
        'action-dock__ready-hand-trigger',
        handInsight.isTenpai ? 'action-dock__ready-hand-trigger--tenpai' : 'action-dock__ready-hand-trigger--plain',
        isReadyHandPopoverOpen ? 'action-dock__ready-hand-trigger--open' : '',
      ].filter(Boolean).join(' ')}
      aria-label={getHandInsightTriggerLabel(handInsight)}
      aria-expanded={isReadyHandPopoverOpen}
      onClick={() => setIsReadyHandPopoverPinned((currentValue) => !currentValue)}
    >
      i
    </button>
    {isReadyHandPopoverOpen ? (
      <section className="action-dock__ready-hand-popover" aria-label={getHandInsightPopoverLabel(handInsight)}>
        {handInsight.isTenpai ? (
          <div className="action-dock__hand-insight-section">
            <strong className="action-dock__hand-insight-title">
              {handInsight.source === 'selected_discard' ? '打出后将听' : '正在听'}
            </strong>
            <div className="action-dock__ready-hand-list" role="list">
              {handInsight.waits.map((wait) => (
                <div key={wait.code} className="action-dock__ready-hand-row" role="listitem">
                  <div className="action-dock__ready-hand-tile">
                    <MahjongTile
                      code={wait.code}
                      variant="discard"
                      relatedTileCode={selectedTileCode}
                      className="action-dock__ready-hand-preview-tile"
                    />
                  </div>
                  <strong>{wait.availableCount}</strong>
                </div>
              ))}
            </div>
          </div>
        ) : null}
        <div className="action-dock__hand-insight-section">
          <strong className="action-dock__hand-insight-title">推荐番型</strong>
          <div className="action-dock__hand-insight-recommendations" role="list">
            {handInsight.recommendations.map((item) => (
              <div key={item.fanKey} className="action-dock__hand-insight-recommendation" role="listitem">
                <span>{getFanLabel(item.fanKey)}</span>
                <strong>{item.similarityPercent}%</strong>
              </div>
            ))}
          </div>
        </div>
      </section>
    ) : null}
  </div>
) : null;
```

```ts
function getHandInsightTriggerLabel(handInsight: NonNullable<BottomActionDockProps['handInsight']>) {
  if (handInsight.source === 'selected_discard') {
    return '查看打出当前选中牌后的手牌洞察';
  }
  return handInsight.isTenpai ? '查看当前听牌信息与推荐番型' : '查看当前推荐番型';
}
```

Run: `npm test -- src/components/battle-screen/BottomActionDock.test.tsx`  
Expected: FAIL only in styling assertions or stale prop names in existing tests.

- [ ] **Step 3: Update dock styling for the black trigger and split content sections**

```css
/* frontend/src/styles/dock.css */
.action-dock__ready-hand-trigger--plain {
  background: rgba(14, 14, 14, 0.58);
  border-color: rgba(14, 14, 14, 0.72);
  color: rgba(255, 255, 255, 0.92);
  box-shadow: 0 10px 22px rgba(0, 0, 0, 0.28);
}

.action-dock__ready-hand-trigger--tenpai {
  background: rgba(14, 14, 14, 0.88);
}

.action-dock__hand-insight-section {
  display: grid;
  gap: 0.4rem;
}

.action-dock__hand-insight-title {
  font-size: 0.72rem;
  letter-spacing: 0.08em;
  color: color-mix(in srgb, var(--theme-paper) 78%, transparent);
}

.action-dock__hand-insight-recommendations {
  display: grid;
  gap: 0.28rem;
}

.action-dock__hand-insight-recommendation {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 0.8rem;
  align-items: center;
  padding: 0.18rem 0;
  border-bottom: 1px solid color-mix(in srgb, var(--theme-paper) 7%, transparent);
}
```

Run: `npm test -- src/components/battle-screen/BottomActionDock.test.tsx`  
Expected: PASS, with the new trigger classes and split popover rendering covered.

## Task 4: Remove Old Frontend Logic and Run Final Verification

**Files:**
- Delete: `frontend/src/lib/readyHand.ts`
- Delete: `frontend/src/lib/readyHand.test.ts`
- Modify: `frontend/src/lib/matchViewModel.ts`
- Modify: `frontend/src/components/battle-screen/BottomActionDock.test.tsx`
- Test: `frontend/src/lib/matchViewModel.test.ts`
- Test: `frontend/src/components/battle-screen/BottomActionDock.test.tsx`

- [ ] **Step 1: Delete the old local waits calculator and its tests**

```diff
*** Delete File: frontend/src/lib/readyHand.ts
*** Delete File: frontend/src/lib/readyHand.test.ts
```

Run: `npm test -- src/lib/matchViewModel.test.ts src/components/battle-screen/BottomActionDock.test.tsx`  
Expected: PASS, confirming no remaining imports depend on the deleted files.

- [ ] **Step 2: Run the targeted backend and frontend verification slice**

Run: `cargo test projection::hand_insight -- --nocapture`  
Expected: PASS

Run: `cargo test projection::room_snapshot::tests -- --nocapture`  
Expected: PASS

Run: `npm test -- src/lib/matchViewModel.test.ts src/components/battle-screen/BottomActionDock.test.tsx`  
Expected: PASS

- [ ] **Step 3: Run build-level verification**

Run: `npm run build`  
Expected: PASS with Vite production build output and no TypeScript errors.

Run: `cargo test`  
Expected: PASS or, if unrelated legacy tests fail, only unrelated pre-existing failures with this feature’s new tests green.

- [ ] **Step 4: Summarize diff and prepare execution handoff**

```text
Backend complete when:
- room_snapshot includes `hand_insights.current`
- room_snapshot includes `hand_insights.by_discard_tile_id`
- recommendation ranking excludes impossible closed-hand routes after open melds

Frontend complete when:
- `BattleViewModel.handInsight` is sourced entirely from backend snapshot data
- `ready_hand` enablement uses selected preview tenpai state
- dock popover shows waits plus recommendations and black plain-state `i`
- `frontend/src/lib/readyHand.ts` is gone
```

Run: `git status --short`  
Expected: only the planned feature files are modified or deleted.
