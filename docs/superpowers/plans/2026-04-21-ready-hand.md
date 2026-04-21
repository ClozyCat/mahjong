# Ready Hand Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a formal `ready_hand` action so a player can declare “听”, lock their hand, auto-discard non-winning draws, and receive a `ready_hand_win` fan shown as `听牌成和`.

**Architecture:** Backend owns the ready-hand truth. Add a new `ready_hand` player action, persist `is_ready_hand` on each `PlayerRoundState`, expose that state through the existing snapshot projection, and broaden the current bot-action scheduler so it also performs immediate forced actions for ready-hand humans. Scoring gets a dedicated boolean on `EvaluationInput` so `ready_hand_win` stays an explicit fan rule instead of leaking through generic timing flags. Frontend remains projection-driven: it shows the `听` button only when the selected tile can enter ready hand, disables the local hand after declaration, reuses the optimistic discard path for the action request, and renders the `听` callout plus the new fan label.

**Tech Stack:** Rust (`serde`, `tokio`, existing rule engine / scheduler), React 19 + TypeScript, Vite, Vitest

---

## File Map

### Backend domain and projection

- Create: `backend/src/rules/standard/ready_hand.rs`
  Structural ready-hand helpers shared by validation and projection. This module should answer:
  - whether a specific discard can declare ready hand
  - whether the active player has any ready-hand discard at all
- Modify: `backend/src/rules/standard/mod.rs`
  Export the new `ready_hand` module.
- Modify: `backend/src/core/action.rs`
  Add the new `PlayerAction::ReadyHand { tile_id }` variant.
- Modify: `backend/src/core/engine/command.rs`
  Parse `"ready_hand"` and cover it with parser tests.
- Modify: `backend/src/core/engine/validation.rs`
  Classify the new action and block manual `discard` / `flower` / `kong` once a player is already in ready-hand state.
- Modify: `backend/src/core/engine/flow.rs`
  Route the new action to the standard rules layer.
- Modify: `backend/src/core/state/player.rs`
  Persist `is_ready_hand` on `PlayerRoundState`.
- Modify: `backend/src/projection/mod.rs`
  Add `can_ready_hand` to `SeatProjectionSupport`.
- Modify: `backend/src/projection/support.rs`
  Compute `can_ready_hand` from the new backend helper.
- Modify: `backend/src/projection/room_snapshot.rs`
  Project `is_ready_hand` to the frontend and include `"ready_hand"` in `pending_action.options` only when it should be visible.
- Modify: `backend/src/rules/standard/actions.rs`
  Implement `apply_ready_hand_action_output_in_room_state`, emit `ready_hand_declared`, and keep `tile_discarded` first in the message list.
- Modify: `backend/src/mahjong.rs`
  Add local integration tests that drive the typed room state through the public `try_handle_action` entrypoint.

### Backend automation and scoring

- Modify: `backend/src/rules/standard/automation.rs`
  Reuse the existing `BotAction` envelope for ready-hand humans so the scheduler can run the same action pipe.
- Modify: `backend/src/rules/standard/win.rs`
  Feed the winner’s `is_ready_hand` flag into scoring.
- Modify: `backend/src/rules/scoring/evaluator.rs`
  Add `ready_hand_declared` to `EvaluationInput` / `FanContext`, register `ready_hand_win`, and cover it with evaluator tests.

### Frontend action flow and visuals

- Modify: `frontend/src/types/match.ts`
  Add `ready_hand` to action unions, expose `is_ready_hand` in `PrivatePlayerState`, and extend `ActionEffectView['calloutTone']`.
- Modify: `frontend/src/lib/matchViewModel.ts`
  Enable `ready_hand` only when the selected discard can enter ready hand, keep ready-hand waits informational after declaration, disable the local hand once `is_ready_hand` is true, and map `ready_hand_declared` to a `听` action effect.
- Modify: `frontend/src/App.tsx`
  Send `ready_hand` through the existing action request path and reuse the optimistic discard flow.
- Modify: `frontend/src/components/battle-screen/BottomActionDock.tsx`
  Put `听` immediately to the right of `出牌` and apply a themed outline class.
- Modify: `frontend/src/styles/dock.css`
  Add the `听` button’s theme-outline styling.
- Modify: `frontend/src/components/battle-screen/TableStage.tsx`
  Add the `听` callout tone and glyph copy.
- Modify: `frontend/src/styles/table.css`
  Add the themed `听` callout palette.
- Modify: `frontend/src/lib/roundEventCopy.ts`
  Give `ready_hand_declared` a Chinese toast string instead of the generic fallback.
- Modify: `frontend/src/components/battle-screen/fanGuide.ts`
  Add `ready_hand_win` => `听牌成和` with a 2-fan explanation.

### Test files

- Modify: `backend/src/core/engine/command.rs`
- Modify: `backend/src/projection/room_snapshot.rs`
- Modify: `backend/src/mahjong.rs`
- Modify: `backend/src/rules/standard/automation.rs`
- Modify: `backend/src/rules/standard/win.rs`
- Modify: `backend/src/rules/scoring/evaluator.rs`
- Modify: `frontend/src/lib/matchViewModel.test.ts`
- Modify: `frontend/src/App.test.tsx`
- Modify: `frontend/src/components/battle-screen/BottomActionDock.test.tsx`
- Modify: `frontend/src/components/battle-screen/TableStage.test.tsx`
- Modify: `frontend/src/lib/roundEventCopy.test.ts`
- Create: `frontend/src/components/battle-screen/fanGuide.test.ts`

### Design constraints to keep explicit during implementation

- Backend ready-hand validation should follow the existing frontend notion of “结构上听牌”. Use structural winning decomposition after a candidate draw; do not call settlement-only minimum-fan gating when deciding whether a discard is eligible for declaration.
- After a player has declared ready hand, manual `discard` / `flower` / `kong` actions must be rejected by backend validation, even if the frontend is already locking the hand.
- `ready_hand_declared` must be emitted after the ordinary `tile_discarded` round event so the river stays correct and the frontend can still use the latest event for the `听` callout.

### Task 1: Backend Ready-Hand Action, State, and Projection

**Files:**
- Create: `backend/src/rules/standard/ready_hand.rs`
- Modify: `backend/src/rules/standard/mod.rs`
- Modify: `backend/src/core/action.rs`
- Modify: `backend/src/core/engine/command.rs`
- Modify: `backend/src/core/engine/validation.rs`
- Modify: `backend/src/core/engine/flow.rs`
- Modify: `backend/src/core/state/player.rs`
- Modify: `backend/src/projection/mod.rs`
- Modify: `backend/src/projection/support.rs`
- Modify: `backend/src/projection/room_snapshot.rs`
- Modify: `backend/src/rules/standard/actions.rs`
- Test: `backend/src/core/engine/command.rs`
- Test: `backend/src/projection/room_snapshot.rs`
- Test: `backend/src/mahjong.rs`

- [ ] **Step 1: Write the failing tests**

```rust
// backend/src/core/engine/command.rs
#[test]
fn parses_ready_hand_command() {
    let command = parse_player_command(2, "ready_hand", &[String::from("b9#discard")])
        .expect("ready_hand should be recognized")
        .expect("ready_hand should parse");
    assert_eq!(
        command,
        GameCommand::PlayerAction {
            actor: 2,
            action: PlayerAction::ReadyHand {
                tile_id: "b9#discard".to_string(),
            },
        }
    );
}
```

```rust
// backend/src/projection/room_snapshot.rs
#[test]
fn active_turn_projection_includes_ready_hand_for_local_readyable_hand() {
    let state = RoomState {
        table_code: "ROOM42".to_string(),
        phase: "playing".to_string(),
        mode: "normal".to_string(),
        test_mode: false,
        enforce_minimum_eight_fan: true,
        seats: seats(),
        match_state: None,
        round_state: Some(RoundState {
            round_id: "round-1".to_string(),
            dealer_seat: 0,
            round_wind: "east".to_string(),
            current_actor: 0,
            phase: "playing".to_string(),
            players: players(),
            ..Default::default()
        }),
        pending_timeout: Some(PendingTimeout {
            kind: "active_turn".to_string(),
            seat_index: 0,
            deadline_at: Some("2026-04-21T12:00:30.000Z".to_string()),
            drawn_tile_id: Some("b9#discard".to_string()),
        }),
        continue_action: None,
    };

    let pending_action = build_pending_action_view(
        &state,
        0,
        &SeatProjectionSupport {
            can_ready_hand: true,
            ..Default::default()
        },
    )
    .expect("local active turn should be projected");

    assert_eq!(
        serde_json::to_value(&pending_action).expect("pending action should serialize")["options"],
        serde_json::json!(["discard", "ready_hand"])
    );
}
```

```rust
// backend/src/mahjong.rs
fn room_for_local_ready_hand() -> Value {
    let mut room = room_for_local_discard();
    room["round_state"]["players"][0]["concealed_tiles"] = json!([
        suit("w1", "w1#0"),
        suit("w2", "w2#1"),
        suit("w3", "w3#2"),
        suit("w4", "w4#3"),
        suit("w5", "w5#4"),
        suit("w6", "w6#5"),
        suit("w7", "w7#6"),
        suit("w8", "w8#7"),
        suit("w9", "w9#8"),
        suit("t1", "t1#9"),
        suit("t2", "t2#10"),
        suit("t3", "t3#11"),
        suit("t4", "t4#12"),
        suit("b9", "b9#discard")
    ]);
    room["round_state"]["last_action_context"]["tile_id"] = json!("b9#discard");
    room["pending_timeout"]["drawn_tile_id"] = json!("b9#discard");
    room
}

#[test]
fn local_ready_hand_sets_flag_and_emits_ready_hand_event() {
    let mut room = room_for_local_ready_hand();

    let result = try_handle_action(&mut room, 0, "ready_hand", &[String::from("b9#discard")])
        .expect("ready hand should be handled locally")
        .expect("ready hand should succeed");

    assert_eq!(result.len(), 2);
    assert_eq!(result[0]["payload"]["event_type"], "tile_discarded");
    assert_eq!(result[1]["payload"]["event_type"], "ready_hand_declared");
    assert_eq!(room["round_state"]["players"][0]["is_ready_hand"], true);
    assert_eq!(
        room["round_state"]["players"][0]["discards"][0]["tile_id"],
        "b9#discard"
    );
}
```

- [ ] **Step 2: Run the targeted backend tests and verify they fail**

```bash
cd backend
cargo test parses_ready_hand_command -- --exact
cargo test active_turn_projection_includes_ready_hand_for_local_readyable_hand -- --exact
cargo test local_ready_hand_sets_flag_and_emits_ready_hand_event -- --exact
```

Expected:

- `parses_ready_hand_command` fails because `"ready_hand"` is not recognized.
- `active_turn_projection_includes_ready_hand_for_local_readyable_hand` fails because `SeatProjectionSupport` has no `can_ready_hand` field and the options list never contains `"ready_hand"`.
- `local_ready_hand_sets_flag_and_emits_ready_hand_event` fails because `ready_hand` is not routable yet.

- [ ] **Step 3: Implement the backend action, state, and projection**

```rust
// backend/src/core/action.rs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerAction {
    Flower {
        tile_ids: Vec<TileId>,
    },
    Discard {
        tile_id: TileId,
    },
    ReadyHand {
        tile_id: TileId,
    },
    Chow {
        tile_ids: Vec<TileId>,
    },
    Pung {
        tile_ids: Vec<TileId>,
    },
    Kong {
        tile_ids: Vec<TileId>,
    },
    Hu,
    Pass,
}
```

```rust
// backend/src/core/engine/command.rs
pub fn parse_player_command(
    actor: Seat,
    action_type: &str,
    tile_ids: &[String],
) -> Option<Result<GameCommand, String>> {
    let action = match action_type {
        "hu" => Ok(PlayerAction::Hu),
        "flower" => Ok(PlayerAction::Flower {
            tile_ids: tile_ids.to_vec(),
        }),
        "discard" => {
            if tile_ids.len() != 1 {
                Err("select_tile_first".to_string())
            } else {
                Ok(PlayerAction::Discard {
                    tile_id: tile_ids[0].clone(),
                })
            }
        }
        "ready_hand" => {
            if tile_ids.len() != 1 {
                Err("select_tile_first".to_string())
            } else {
                Ok(PlayerAction::ReadyHand {
                    tile_id: tile_ids[0].clone(),
                })
            }
        }
        "chow" => Ok(PlayerAction::Chow {
            tile_ids: tile_ids.to_vec(),
        }),
        "pung" => Ok(PlayerAction::Pung {
            tile_ids: tile_ids.to_vec(),
        }),
        "kong" => Ok(PlayerAction::Kong {
            tile_ids: tile_ids.to_vec(),
        }),
        "pass" => Ok(PlayerAction::Pass),
        _ => return None,
    };

    Some(action.map(|action| GameCommand::PlayerAction { actor, action }))
}
```

```rust
// backend/src/core/state/player.rs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PlayerRoundState {
    pub seat: Seat,
    pub concealed_tiles: Vec<Tile>,
    pub melds: Vec<Vec<TileKey>>,
    pub display_melds: Vec<DisplayMeldState>,
    pub flowers: Vec<Tile>,
    pub discards: Vec<Tile>,
    pub is_ready_hand: bool,
}
```

```rust
// backend/src/rules/standard/ready_hand.rs
use crate::core::state::RoomState;
use crate::rules::scoring::decompose_winning_hand_with_melds;

const READY_HAND_TILE_KEYS: [&str; 34] = [
    "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9",
    "t1", "t2", "t3", "t4", "t5", "t6", "t7", "t8", "t9",
    "b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9",
    "east", "south", "west", "north", "red", "green", "white",
];

pub fn has_ready_hand_discard_in_room_state(state: &RoomState, seat_index: usize) -> bool {
    let Some(round) = state.round_state.as_ref() else {
        return false;
    };
    let Some(player) = round.players.get(seat_index) else {
        return false;
    };
    player
        .concealed_tiles
        .iter()
        .any(|tile| can_declare_ready_hand_with_tile_id(state, seat_index, &tile.tile_id))
}

pub fn can_declare_ready_hand_with_tile_id(
    state: &RoomState,
    seat_index: usize,
    tile_id: &str,
) -> bool {
    let Some(round) = state.round_state.as_ref() else {
        return false;
    };
    if round.current_actor != seat_index || round.pending_action.is_some() {
        return false;
    }
    let Some(player) = round.players.get(seat_index) else {
        return false;
    };
    if player.is_ready_hand {
        return false;
    }
    let Some(tile_index) = player
        .concealed_tiles
        .iter()
        .position(|tile| tile.tile_id == tile_id)
    else {
        return false;
    };
    let discarded_tile = &player.concealed_tiles[tile_index];
    if discarded_tile.kind == "flower" {
        return false;
    }
    if round
        .restricted_discard_tile_key
        .as_deref()
        .is_some_and(|restricted| restricted == discarded_tile.tile_key)
    {
        return false;
    }

    let mut concealed_tile_keys = player
        .concealed_tiles
        .iter()
        .map(|tile| tile.tile_key.clone())
        .collect::<Vec<_>>();
    concealed_tile_keys.remove(tile_index);
    let expected_concealed_count = (4 - player.melds.len()) * 3 + 1;
    if concealed_tile_keys.len() != expected_concealed_count {
        return false;
    }

    READY_HAND_TILE_KEYS.iter().any(|candidate| {
        if concealed_tile_keys
            .iter()
            .filter(|tile_key| tile_key.as_str() == *candidate)
            .count()
            >= 4
        {
            return false;
        }
        let mut winning_concealed = concealed_tile_keys.clone();
        winning_concealed.push((*candidate).to_string());
        !decompose_winning_hand_with_melds(&winning_concealed, &player.melds).is_empty()
    })
}
```

```rust
// backend/src/core/engine/validation.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalPlayerActionKind {
    Hu,
    Flower,
    Discard,
    ReadyHand,
    ClaimWindow,
    SelfKong,
    RobKongPass,
}

pub fn classify_local_player_action(
    context: &EngineContext,
    actor: Seat,
    action: &PlayerAction,
) -> Option<LocalPlayerActionKind> {
    match action {
        PlayerAction::Hu => Some(LocalPlayerActionKind::Hu),
        PlayerAction::Flower { tile_ids } => {
            flower_supported_locally(context, actor, tile_ids).then_some(LocalPlayerActionKind::Flower)
        }
        PlayerAction::Discard { tile_id } => discard_supported_locally(context, actor, tile_id)
            .then_some(LocalPlayerActionKind::Discard),
        PlayerAction::ReadyHand { tile_id } => {
            ready_hand_supported_locally(context, actor, tile_id).then_some(LocalPlayerActionKind::ReadyHand)
        }
        PlayerAction::Chow { .. } => claim_window_action_supported(context, actor, "chow")
            .then_some(LocalPlayerActionKind::ClaimWindow),
        PlayerAction::Pung { .. } => claim_window_action_supported(context, actor, "pung")
            .then_some(LocalPlayerActionKind::ClaimWindow),
        PlayerAction::Kong { .. } => {
            if claim_window_action_supported(context, actor, "kong") {
                Some(LocalPlayerActionKind::ClaimWindow)
            } else if self_kong_supported(context, actor) {
                Some(LocalPlayerActionKind::SelfKong)
            } else {
                None
            }
        }
        PlayerAction::Pass => {
            if claim_window_action_supported(context, actor, "pass") {
                Some(LocalPlayerActionKind::ClaimWindow)
            } else if rob_kong_pass_supported(context, actor) {
                Some(LocalPlayerActionKind::RobKongPass)
            } else {
                None
            }
        }
    }
}
```

```rust
// backend/src/core/engine/validation.rs
use crate::rules::standard::ready_hand::can_declare_ready_hand_with_tile_id;

fn player_is_ready_hand(context: &EngineContext, actor: Seat) -> bool {
    context
        .room
        .round_state
        .as_ref()
        .and_then(|round| round.players.get(actor))
        .map(|player| player.is_ready_hand)
        .unwrap_or(false)
}

fn flower_supported_locally(context: &EngineContext, actor: Seat, tile_ids: &[String]) -> bool {
    if player_is_ready_hand(context, actor) || tile_ids.len() != 1 {
        return false;
    }
    context.room.phase == "playing"
        && context.current_actor() == Some(actor)
        && context
            .room
            .round_state
            .as_ref()
            .and_then(|round| round.players.get(actor))
            .is_some_and(|player| {
                player
                    .concealed_tiles
                    .iter()
                    .any(|tile| tile.tile_id == tile_ids[0] && tile.kind == "flower")
            })
}

pub fn discard_supported_locally(context: &EngineContext, actor: Seat, tile_id: &str) -> bool {
    if context.room.phase != "playing" {
        return false;
    }
    let Some(round) = context.room.round_state.as_ref() else {
        return false;
    };
    if round.current_actor != actor || round.pending_action.is_some() {
        return false;
    }
    let Some(player) = round.players.get(actor) else {
        return false;
    };
    if player.is_ready_hand {
        return false;
    }
    let Some(tile) = player.concealed_tiles.iter().find(|tile| tile.tile_id == tile_id) else {
        return false;
    };
    match round.restricted_discard_tile_key.as_deref() {
        Some(restricted) => tile.tile_key != restricted,
        None => true,
    }
}

fn ready_hand_supported_locally(context: &EngineContext, actor: Seat, tile_id: &str) -> bool {
    can_declare_ready_hand_with_tile_id(&context.room, actor, tile_id)
}

fn self_kong_supported(context: &EngineContext, actor: Seat) -> bool {
    !player_is_ready_hand(context, actor)
        && context.room.phase == "playing"
        && context.current_actor() == Some(actor)
        && context
            .room
            .pending_timeout
            .as_ref()
            .map(|timeout| timeout.kind.as_str())
            == Some("active_turn")
}
```

```rust
// backend/src/core/engine/flow.rs
use crate::rules::standard::actions::{
    apply_claim_window_action_in_room_state, apply_discard_action_output_in_room_state,
    apply_ready_hand_action_output_in_room_state, apply_rob_kong_hu_in_room_state,
    apply_rob_kong_pass_in_room_state, try_handle_self_kong_action_output_in_room_state,
};

// add this new match arm inside try_handle_player_action_command(...)
(LocalPlayerActionKind::ReadyHand, PlayerAction::ReadyHand { tile_id }) => {
    Some(apply_ready_hand_action_output_in_room_state(room, seat_index, &tile_id))
}
```

```rust
// backend/src/projection/mod.rs
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatProjectionSupport {
    pub has_concealed_flower: bool,
    pub has_self_kong: bool,
    pub can_hu: bool,
    pub can_ready_hand: bool,
    pub restricted_discard_tile_ids: Vec<String>,
}
```

```rust
// backend/src/projection/support.rs
use crate::rules::standard::ready_hand::has_ready_hand_discard_in_room_state;

pub fn build_seat_projection_support_for_state(
    state: &RoomState,
    local_seat: usize,
) -> SeatProjectionSupport {
    let cache = RoomScoringCache::from_state(state);
    let player = cache.player(local_seat);
    let restricted_tile_key = cache.restricted_discard_tile_key.as_deref();

    SeatProjectionSupport {
        has_concealed_flower: player.is_some_and(|player| {
            player
                .concealed_tiles
                .iter()
                .any(|tile| tile.kind == "flower")
        }),
        has_self_kong: !available_self_kongs_from_cache(&cache, local_seat).is_empty(),
        can_hu: can_declare_hu_with_cache_for_state(state, &cache, local_seat, None, None),
        can_ready_hand: has_ready_hand_discard_in_room_state(state, local_seat),
        restricted_discard_tile_ids: player
            .map(|player| {
                player
                    .concealed_tiles
                    .iter()
                    .filter(|tile| Some(tile.tile_key.as_str()) == restricted_tile_key)
                    .map(|tile| tile.tile_id.clone())
                    .collect()
            })
            .unwrap_or_default(),
    }
}
```

```rust
// backend/src/projection/room_snapshot.rs
#[derive(Debug, Clone, Serialize)]
struct PlayerSeatView {
    seat_index: Seat,
    nickname: Option<String>,
    connected: bool,
    concealed_count: usize,
    concealed_tiles: Option<Vec<PrivateTileView>>,
    melds: Vec<Vec<String>>,
    display_melds: Vec<DisplayMeldView>,
    flowers: Vec<String>,
    discards: Vec<String>,
    is_ready_hand: bool,
}

// update the PlayerSeatView construction inside private_round_state(...)
PlayerSeatView {
    seat_index: player.seat,
    nickname: seat_info.and_then(|seat| seat.nickname.clone()),
    connected: seat_info.map(|seat| seat.connected).unwrap_or(false),
    concealed_count: player.concealed_tiles.len(),
    concealed_tiles,
    melds: player.melds.clone(),
    display_melds: project_display_melds(&player.display_melds),
    flowers: player.flowers.iter().map(|tile| tile.tile_key.clone()).collect(),
    discards: player.discards.iter().map(|tile| tile.tile_key.clone()).collect(),
    is_ready_hand: player.is_ready_hand,
}
```

```rust
// backend/src/projection/room_snapshot.rs
let is_local_turn = pending_timeout.seat_index == local_seat;
let is_local_ready_hand = round
    .players
    .get(local_seat)
    .map(|player| player.is_ready_hand)
    .unwrap_or(false);
let mut options = Vec::new();
if is_local_turn {
    if !is_local_ready_hand {
        options.push("discard".to_string());
        if support.can_ready_hand {
            options.push("ready_hand".to_string());
        }
        if support.has_concealed_flower {
            options.push("flower".to_string());
        }
        if support.has_self_kong {
            options.push("kong".to_string());
        }
    }
    if support.can_hu {
        options.push("hu".to_string());
    }
}

Some(PendingActionView::ActiveTurn {
    seat_index: pending_timeout.seat_index,
    deadline_at,
    drawn_tile_id: if is_local_turn {
        pending_timeout.drawn_tile_id.clone()
    } else {
        None
    },
    restricted_discard_tile_ids: if is_local_turn {
        support.restricted_discard_tile_ids.clone()
    } else {
        Vec::new()
    },
    options,
})
```

```rust
// backend/src/rules/standard/actions.rs
fn ready_hand_declared_message(seat_index: usize, tile: &Tile) -> Value {
    round_event_message(
        "ready_hand_declared",
        json!({
            "type": "ready_hand_declared",
            "seat": seat_index,
            "discard_tile_id": tile.tile_id,
            "discard_tile_key": tile.tile_key,
        }),
    )
}

pub fn apply_ready_hand_action_output_in_room_state(
    room: &mut RoomState,
    seat_index: usize,
    tile_id: &str,
) -> Result<EngineOutput, String> {
    if !crate::rules::standard::ready_hand::can_declare_ready_hand_with_tile_id(
        room,
        seat_index,
        tile_id,
    ) {
        return Err("invalid_action".to_string());
    }
    let discarded_tile = round_state_ref(room)?
        .players
        .get(seat_index)
        .and_then(|player| {
            player
                .concealed_tiles
                .iter()
                .find(|tile| tile.tile_id == tile_id)
        })
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;

    round_state_mut(room)?
        .players
        .get_mut(seat_index)
        .ok_or_else(|| "invalid_action".to_string())?
        .is_ready_hand = true;

    let mut output = apply_discard_action_output_in_room_state(room, seat_index, tile_id)?;
    output
        .emitted_messages
        .push(ready_hand_declared_message(seat_index, &discarded_tile));
    Ok(output)
}
```

- [ ] **Step 4: Run the targeted backend tests again**

```bash
cd backend
cargo test parses_ready_hand_command -- --exact
cargo test active_turn_projection_includes_ready_hand_for_local_readyable_hand -- --exact
cargo test local_ready_hand_sets_flag_and_emits_ready_hand_event -- --exact
```

Expected:

- All three tests PASS.

- [ ] **Step 5: Commit the backend action/projection slice**

```bash
git add backend/src/rules/standard/ready_hand.rs backend/src/rules/standard/mod.rs backend/src/core/action.rs backend/src/core/engine/command.rs backend/src/core/engine/validation.rs backend/src/core/engine/flow.rs backend/src/core/state/player.rs backend/src/projection/mod.rs backend/src/projection/support.rs backend/src/projection/room_snapshot.rs backend/src/rules/standard/actions.rs backend/src/mahjong.rs
git commit -m "feat(backend): 支持听牌宣告基础链路"
```

### Task 2: Backend Auto-Discard for Ready-Hand Humans

**Files:**
- Modify: `backend/src/rules/standard/automation.rs`
- Modify: `backend/src/app/scheduler.rs`
- Test: `backend/src/rules/standard/automation.rs`

- [ ] **Step 1: Write the failing automation tests**

```rust
// backend/src/rules/standard/automation.rs
fn dragon(tile_key: &str, tile_id: &str) -> serde_json::Value {
    json!({
        "tile_id": tile_id,
        "tile_key": tile_key,
        "kind": "dragon",
        "suit": null,
        "rank": null,
        "name": tile_key,
    })
}

#[test]
fn ready_hand_human_discards_drawn_tile_as_next_auto_action() {
    let mut room = claim_window_room_state();
    room.round_state
        .as_mut()
        .and_then(|round| round.players.get_mut(0))
        .expect("seat 0 should exist")
        .is_ready_hand = true;

    let action = next_bot_action_in_room_state(&room)
        .expect("auto lookup should succeed")
        .expect("ready hand human should auto act");

    assert_eq!(action.seat_index, 0);
    assert_eq!(action.action_type, "discard");
    assert_eq!(action.tile_ids, vec!["w3#discard"]);
}

#[test]
fn ready_hand_human_keeps_hu_when_draw_is_winning_tile() {
    let mut room = claim_window_room_state();
    let player = room
        .round_state
        .as_mut()
        .and_then(|round| round.players.get_mut(0))
        .expect("seat 0 should exist");
    player.is_ready_hand = true;
    player.concealed_tiles = vec![
        crate::core::tile::Tile::from_value(&suit("w1", "w1#0"), "tile").expect("tile"),
        crate::core::tile::Tile::from_value(&suit("w2", "w2#0"), "tile").expect("tile"),
        crate::core::tile::Tile::from_value(&suit("w3", "w3#0"), "tile").expect("tile"),
        crate::core::tile::Tile::from_value(&suit("w4", "w4#0"), "tile").expect("tile"),
        crate::core::tile::Tile::from_value(&suit("w5", "w5#0"), "tile").expect("tile"),
        crate::core::tile::Tile::from_value(&suit("w6", "w6#0"), "tile").expect("tile"),
        crate::core::tile::Tile::from_value(&suit("t1", "t1#0"), "tile").expect("tile"),
        crate::core::tile::Tile::from_value(&suit("t2", "t2#0"), "tile").expect("tile"),
        crate::core::tile::Tile::from_value(&suit("t3", "t3#0"), "tile").expect("tile"),
        crate::core::tile::Tile::from_value(&suit("b1", "b1#0"), "tile").expect("tile"),
        crate::core::tile::Tile::from_value(&suit("b2", "b2#0"), "tile").expect("tile"),
        crate::core::tile::Tile::from_value(&suit("b3", "b3#0"), "tile").expect("tile"),
        crate::core::tile::Tile::from_value(&dragon("red", "red#0a"), "tile").expect("tile"),
        crate::core::tile::Tile::from_value(&dragon("red", "red#0b"), "tile").expect("tile"),
    ];
    room.pending_timeout
        .as_mut()
        .expect("pending timeout")
        .drawn_tile_id = Some("red#0b".to_string());

    let action = next_bot_action_in_room_state(&room)
        .expect("auto lookup should succeed")
        .expect("winning ready hand human should auto hu");

    assert_eq!(action.seat_index, 0);
    assert_eq!(action.action_type, "hu");
    assert!(action.tile_ids.is_empty());
}
```

- [ ] **Step 2: Run the automation tests and verify they fail**

```bash
cd backend
cargo test ready_hand_human_discards_drawn_tile_as_next_auto_action -- --exact
cargo test ready_hand_human_keeps_hu_when_draw_is_winning_tile -- --exact
```

Expected:

- The first test fails because `next_bot_action_in_room_state` returns `None` for non-bot seats.
- The second test fails for the same reason.

- [ ] **Step 3: Implement immediate auto-actions for ready-hand humans**

```rust
// backend/src/rules/standard/automation.rs
fn next_ready_hand_human_action_for_state(state: &RoomState) -> Option<BotAction> {
    if state.phase != "playing" {
        return None;
    }
    let pending_timeout = state.pending_timeout.as_ref()?;
    if pending_timeout.kind != "active_turn" {
        return None;
    }
    let round = state.round_state.as_ref()?;
    let seat_index = round.current_actor;
    if seat_is_bot(state, seat_index) {
        return None;
    }
    let player = round.players.get(seat_index)?;
    if !player.is_ready_hand {
        return None;
    }

    let cache = RoomScoringCache::from_state(state);
    if can_declare_hu_with_cache_for_state(state, &cache, seat_index, None, None) {
        return Some(BotAction {
            seat_index,
            action_type: "hu".to_string(),
            tile_ids: vec![],
        });
    }
    if let Some(tile_id) = player_first_flower_tile_id_from_cache(&cache, seat_index) {
        return Some(BotAction {
            seat_index,
            action_type: "flower".to_string(),
            tile_ids: vec![tile_id],
        });
    }
    pending_timeout.drawn_tile_id.clone().map(|tile_id| BotAction {
        seat_index,
        action_type: "discard".to_string(),
        tile_ids: vec![tile_id],
    })
}

fn next_bot_action_for_state(state: &RoomState) -> Option<BotAction> {
    if let Some(action) = next_ready_hand_human_action_for_state(state) {
        return Some(action);
    }
    if state.phase != "playing" {
        return None;
    }
    let pending_timeout = state.pending_timeout.as_ref()?;
    let round = state.round_state.as_ref()?;
    match pending_timeout.kind.as_str() {
        "active_turn" => {
            let seat_index = round.current_actor;
            if !seat_is_bot(state, seat_index) {
                return None;
            }
            let cache = RoomScoringCache::from_state(state);
            if can_declare_hu_with_cache_for_state(state, &cache, seat_index, None, None) {
                return Some(BotAction {
                    seat_index,
                    action_type: "hu".to_string(),
                    tile_ids: vec![],
                });
            }
            if let Some(tile_id) = player_first_flower_tile_id_from_cache(&cache, seat_index) {
                return Some(BotAction {
                    seat_index,
                    action_type: "flower".to_string(),
                    tile_ids: vec![tile_id],
                });
            }
            choose_bot_active_turn_action_with_cache_for_state(state, &cache, seat_index)
        }
        "claim_window" => match round.pending_action.as_ref()? {
            PendingAction::RobKongWindow(rob) => {
                let seat_index =
                    next_rob_kong_responder_seat(rob).filter(|seat| seat_is_bot(state, *seat))?;
                Some(BotAction {
                    seat_index,
                    action_type: "hu".to_string(),
                    tile_ids: vec![],
                })
            }
            PendingAction::ClaimWindow(claim) => {
                let cache = RoomScoringCache::from_state(state);
                let seat_index = next_claim_window_responder_seat(claim)
                    .filter(|seat| seat_is_bot(state, *seat))?;
                choose_bot_claim_action_with_cache_for_state(state, &cache, seat_index)
            }
        },
        _ => None,
    }
}
```

```rust
// backend/src/app/scheduler.rs
if let Some(action) = standard_next_bot_action(&runtime.room).ok().flatten() {
    let state_clone = state.clone();
    let table_clone = table_code.clone();
    let nonce = runtime.bot_nonce;
    let is_bot_action = runtime
        .room
        .seats
        .iter()
        .any(|seat| seat.seat_index == action.seat_index && seat.is_bot);
    let delay_ms = if is_bot_action {
        if room_mode(&runtime.room) == "test" {
            BOT_ACTION_DELAY_TEST_MS
        } else {
            BOT_ACTION_DELAY_NORMAL_MS
        }
    } else {
        0
    };
    runtime.bot_task = Some(tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        process_due_bot_action(state_clone, table_clone, nonce).await;
    }));
}
```

- [ ] **Step 4: Run the automation tests again**

```bash
cd backend
cargo test ready_hand_human_discards_drawn_tile_as_next_auto_action -- --exact
cargo test ready_hand_human_keeps_hu_when_draw_is_winning_tile -- --exact
```

Expected:

- Both tests PASS.

- [ ] **Step 5: Commit the auto-action slice**

```bash
git add backend/src/rules/standard/automation.rs backend/src/app/scheduler.rs
git commit -m "feat(backend): 支持听牌后自动出牌"
```

### Task 3: Backend Scoring for `ready_hand_win`

**Files:**
- Modify: `backend/src/rules/standard/win.rs`
- Modify: `backend/src/rules/scoring/evaluator.rs`
- Test: `backend/src/rules/standard/win.rs`
- Test: `backend/src/rules/scoring/evaluator.rs`

- [ ] **Step 1: Write the failing scoring tests**

```rust
// backend/src/rules/scoring/evaluator.rs
#[test]
fn scores_ready_hand_win_as_two_fan() {
    let tile_keys = vec![
        "w1", "w2", "w3", "w4", "w5", "w6", "t1", "t2", "t3", "b1", "b2", "b3", "red", "red",
    ]
    .into_iter()
    .map(ToString::to_string)
    .collect::<Vec<_>>();
    let decompositions = vec![Decomposition {
        kind: "standard".to_string(),
        pair: Some("red".to_string()),
        melds: vec![
            vec!["w1".to_string(), "w2".to_string(), "w3".to_string()],
            vec!["w4".to_string(), "w5".to_string(), "w6".to_string()],
            vec!["t1".to_string(), "t2".to_string(), "t3".to_string()],
            vec!["b1".to_string(), "b2".to_string(), "b3".to_string()],
        ],
        ..Default::default()
    }];
    let features = extract_hand_features(
        &tile_keys,
        &[],
        None,
        None,
        Some("east"),
        Some("east"),
        Some(&decompositions),
    );

    let base_result = evaluate_fans(EvaluationInput {
        win_type: "discard".to_string(),
        winner_seat: Some(0),
        discarder_seat: Some(1),
        flower_count: 0,
        seat_count: 4,
        features: features.clone(),
        timing: TimingFeatures::default(),
        kong_entries: vec![],
        tile_keys: tile_keys.clone(),
        visible_tile_keys: vec![],
        concealed_tile_keys: tile_keys.clone(),
        meld_tile_key_groups: vec![],
        open_meld_tile_key_groups: vec![],
        incoming_tile: None,
        ready_hand_declared: false,
        decompositions: decompositions.clone(),
    });

    let ready_hand_result = evaluate_fans(EvaluationInput {
        win_type: "discard".to_string(),
        winner_seat: Some(0),
        discarder_seat: Some(1),
        flower_count: 0,
        seat_count: 4,
        features,
        timing: TimingFeatures::default(),
        kong_entries: vec![],
        tile_keys: tile_keys.clone(),
        visible_tile_keys: vec![],
        concealed_tile_keys: tile_keys,
        meld_tile_key_groups: vec![],
        open_meld_tile_key_groups: vec![],
        incoming_tile: None,
        ready_hand_declared: true,
        decompositions,
    });

    assert!(!base_result.fan_keys.iter().any(|fan| fan == "ready_hand_win"));
    assert!(ready_hand_result.fan_keys.iter().any(|fan| fan == "ready_hand_win"));
    assert_eq!(ready_hand_result.fan_total, base_result.fan_total + 2);
}
```

```rust
// backend/src/rules/standard/win.rs
#[test]
fn settlement_includes_ready_hand_win_for_ready_hand_winner() {
    let tile_keys = [
        "w1", "w2", "w3", "w4", "w5", "w6", "t1", "t2", "t3", "b1", "b2", "b3", "red", "red",
    ];
    let mut state = test_room_state_with_concealed_tiles(&tile_keys);
    state
        .round_state
        .as_mut()
        .and_then(|round| round.players.get_mut(0))
        .expect("seat 0 should exist")
        .is_ready_hand = true;

    let settlement =
        compute_hu_settlement_for_state(&state, 0, "self_draw").expect("settlement should succeed");

    assert!(settlement.fan_keys.iter().any(|fan| fan == "ready_hand_win"));
}
```

- [ ] **Step 2: Run the scoring tests and verify they fail**

```bash
cd backend
cargo test scores_ready_hand_win_as_two_fan -- --exact
cargo test settlement_includes_ready_hand_win_for_ready_hand_winner -- --exact
```

Expected:

- Both tests fail because `EvaluationInput` does not yet carry the ready-hand flag and no fan rule matches it.

- [ ] **Step 3: Implement the scoring flag and fan rule**

```rust
// backend/src/rules/scoring/evaluator.rs
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EvaluationInput {
    pub win_type: String,
    pub winner_seat: Option<usize>,
    pub discarder_seat: Option<usize>,
    pub flower_count: usize,
    pub seat_count: usize,
    pub features: HandFeatures,
    pub timing: TimingFeatures,
    pub kong_entries: Vec<KongEntry>,
    pub tile_keys: Vec<String>,
    pub visible_tile_keys: Vec<String>,
    pub concealed_tile_keys: Vec<String>,
    pub meld_tile_key_groups: Vec<Vec<String>>,
    pub open_meld_tile_key_groups: Vec<Vec<String>>,
    pub incoming_tile: Option<String>,
    pub ready_hand_declared: bool,
    pub decompositions: Vec<Decomposition>,
}

#[derive(Clone, Debug)]
struct FanContext {
    win_type: String,
    winner_seat: Option<usize>,
    discarder_seat: Option<usize>,
    flower_count: usize,
    seat_count: usize,
    features: HandFeatures,
    timing: TimingFeatures,
    kong_entries: Vec<KongEntry>,
    visible_tile_keys: Vec<String>,
    concealed_tile_keys: Vec<String>,
    open_meld_tile_key_groups: Vec<Vec<String>>,
    decompositions: Vec<Decomposition>,
    standard_decompositions: Vec<Decomposition>,
    all_tile_keys: Vec<String>,
    wait_types: Vec<String>,
    winning_tile: Option<String>,
    ready_hand_declared: bool,
    standard_derived: StandardDerivedData,
    all_tile_derived: AllTileDerivedData,
}
```

```rust
// backend/src/rules/scoring/evaluator.rs
let EvaluationInput {
    win_type,
    winner_seat,
    discarder_seat,
    flower_count,
    seat_count,
    features,
    timing,
    kong_entries,
    tile_keys,
    visible_tile_keys,
    concealed_tile_keys,
    meld_tile_key_groups: _,
    open_meld_tile_key_groups,
    incoming_tile,
    ready_hand_declared,
    decompositions: input_decompositions,
} = input;
```

```rust
// backend/src/rules/scoring/evaluator.rs
// include the new flag when FanContext is constructed
Self {
    win_type,
    winner_seat,
    discarder_seat,
    flower_count,
    seat_count: seat_count.max(1),
    features,
    timing,
    kong_entries,
    visible_tile_keys,
    concealed_tile_keys,
    open_meld_tile_key_groups,
    decompositions,
    standard_decompositions,
    all_tile_keys: tile_keys,
    wait_types,
    winning_tile: incoming_tile,
    ready_hand_declared,
    standard_derived,
    all_tile_derived,
}
```

```rust
// backend/src/rules/scoring/evaluator.rs
FanRule {
    fan_key: "ready_hand_win",
    fan_value: 1,
    matcher: match_ready_hand_win,
    value_resolver: None,
    excludes: &[],
    forbidden_with: &[],
},

fn match_ready_hand_win(context: &FanContext) -> usize {
    usize::from(context.ready_hand_declared)
}
```

```rust
// backend/src/rules/standard/win.rs
let evaluation = ScoringEvaluationInput {
    win_type: win_type.clone(),
    winner_seat: Some(winner_seat),
    discarder_seat,
    flower_count: cache
        .player(winner_seat)
        .map(|player| player.flower_count)
        .unwrap_or(0),
    seat_count: cache.seat_count,
    features,
    timing: timing_features_for_win_state(state, incoming_tile.is_none()),
    kong_entries,
    tile_keys: player_tile_keys,
    visible_tile_keys: cache.visible_tile_keys.clone(),
    concealed_tile_keys,
    meld_tile_key_groups,
    open_meld_tile_key_groups,
    incoming_tile: incoming_tile.map(ToString::to_string),
    ready_hand_declared: state
        .round_state
        .as_ref()
        .and_then(|round| round.players.get(winner_seat))
        .map(|player| player.is_ready_hand)
        .unwrap_or(false),
    decompositions,
};
```

- [ ] **Step 4: Run the scoring tests again**

```bash
cd backend
cargo test scores_ready_hand_win_as_two_fan -- --exact
cargo test settlement_includes_ready_hand_win_for_ready_hand_winner -- --exact
```

Expected:

- Both tests PASS.

- [ ] **Step 5: Commit the scoring slice**

```bash
git add backend/src/rules/standard/win.rs backend/src/rules/scoring/evaluator.rs
git commit -m "feat(scoring): 增加听牌成和计番"
```

### Task 4: Frontend Ready-Hand Button, Lock-Hand State, and Optimistic Request Flow

**Files:**
- Modify: `frontend/src/types/match.ts`
- Modify: `frontend/src/lib/matchViewModel.ts`
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/components/battle-screen/BottomActionDock.tsx`
- Modify: `frontend/src/styles/dock.css`
- Test: `frontend/src/lib/matchViewModel.test.ts`
- Test: `frontend/src/App.test.tsx`
- Test: `frontend/src/components/battle-screen/BottomActionDock.test.tsx`

- [ ] **Step 1: Write the failing frontend interaction tests**

```tsx
// frontend/src/lib/matchViewModel.test.ts
it('enables ready_hand only when the selected discard enters ready hand', () => {
  const base = createPlayingSessionState();
  const viewModel = createMatchViewModel({
    ...base,
    selectedTileIds: ['b9#discard'],
    roomSnapshot: {
      type: 'room_snapshot',
      payload: {
        ...base.roomSnapshot!.payload,
        private_state: {
          ...base.roomSnapshot!.payload.private_state!,
          pending_action: {
            ...base.roomSnapshot!.payload.private_state!.pending_action!,
            options: ['discard', 'ready_hand'],
            drawn_tile_id: 'b9#discard',
          },
          players: base.roomSnapshot!.payload.private_state!.players.map((player) =>
            player.seat_index === 2
              ? {
                  ...player,
                  is_ready_hand: false,
                  concealed_count: 14,
                  concealed_tiles: [
                    { tile_id: 'w1#0', tile_key: 'w1' },
                    { tile_id: 'w2#1', tile_key: 'w2' },
                    { tile_id: 'w3#2', tile_key: 'w3' },
                    { tile_id: 'w4#3', tile_key: 'w4' },
                    { tile_id: 'w5#4', tile_key: 'w5' },
                    { tile_id: 'w6#5', tile_key: 'w6' },
                    { tile_id: 'w7#6', tile_key: 'w7' },
                    { tile_id: 'w8#7', tile_key: 'w8' },
                    { tile_id: 'w9#8', tile_key: 'w9' },
                    { tile_id: 't1#9', tile_key: 't1' },
                    { tile_id: 't2#10', tile_key: 't2' },
                    { tile_id: 't3#11', tile_key: 't3' },
                    { tile_id: 't4#12', tile_key: 't4' },
                    { tile_id: 'b9#discard', tile_key: 'b9' },
                  ],
                }
              : player,
          ),
        },
      },
    },
  });

  expect(viewModel.actions.find((action) => action.id === 'ready_hand')?.enabled).toBe(true);
});

it('disables every local tile after ready hand is declared', () => {
  const base = createPlayingSessionState();
  const viewModel = createMatchViewModel({
    ...base,
    roomSnapshot: {
      type: 'room_snapshot',
      payload: {
        ...base.roomSnapshot!.payload,
        private_state: {
          ...base.roomSnapshot!.payload.private_state!,
          players: base.roomSnapshot!.payload.private_state!.players.map((player) =>
            player.seat_index === 2 ? { ...player, is_ready_hand: true } : player,
          ),
        },
      },
    },
  });

  expect(viewModel.localHand.every((tile) => tile.isDisabled)).toBe(true);
});
```

```tsx
// frontend/src/components/battle-screen/BottomActionDock.test.tsx
it('places 听 immediately to the right of 出牌 and uses the themed outline class', () => {
  render(
    <BottomActionDock
      hand={localHand}
      claimCandidates={[]}
      actions={[
        { id: 'discard', label: '出牌', enabled: true, emphasis: 'high' },
        { id: 'ready_hand', label: '听', enabled: true, emphasis: 'medium' },
        { id: 'pass', label: '过', enabled: true, emphasis: 'low' },
      ]}
      isElevated
      promptCue={{
        kind: 'turn',
        tone: 'info',
        title: '当前可出牌或听牌',
        detail: '你可以 出牌 / 听 / 过',
        actionIds: ['discard', 'ready_hand', 'pass'],
        highlightedActionIds: ['discard', 'ready_hand'],
        sourceSeat: null,
        isUrgent: false,
      }}
      deadlineAt="2099-03-30T12:10:40+08:00"
      onTileSelect={vi.fn()}
      onTileDoubleClick={vi.fn()}
      onClaimCandidateSelect={vi.fn()}
      onClaimCandidateActivate={vi.fn()}
      onAction={vi.fn()}
    />,
  );

  const actionButtons = Array.from(
    document.body.querySelectorAll('.action-dock__actions .action-dock__action-label'),
  ).map((node) => node.textContent);

  expect(actionButtons).toEqual(['出牌', '听', '过']);
  expect(screen.getByRole('button', { name: '听' })).toHaveClass(
    'action-dock__action--themed',
    'action-dock__action--themed-ready-hand',
  );
});
```

```tsx
// frontend/src/App.test.tsx
it('sends ready_hand and queues an optimistic discard when the button is clicked', async () => {
  const user = userEvent.setup();
  const socket = await joinTable(user);

  await act(async () => {
    socket.triggerMessage({
      type: 'room_snapshot',
      payload: createPlayingSnapshotPayload({
        private_state: {
          round_id: 'round-1',
          round_wind: 'east',
          dealer_seat: 0,
          current_actor: 0,
          last_discard: null,
          pending_action: {
            type: 'active_turn',
            seat_index: 0,
            deadline_at: '2026-03-27T12:00:00Z',
            drawn_tile_id: 'b9#discard',
            options: ['discard', 'ready_hand'],
          },
          players: [
            {
              seat_index: 0,
              nickname: 'Player A',
              connected: true,
              concealed_count: 14,
              is_ready_hand: false,
              concealed_tiles: [
                { tile_id: 'w1#0', tile_key: 'w1' },
                { tile_id: 'w2#1', tile_key: 'w2' },
                { tile_id: 'w3#2', tile_key: 'w3' },
                { tile_id: 'w4#3', tile_key: 'w4' },
                { tile_id: 'w5#4', tile_key: 'w5' },
                { tile_id: 'w6#5', tile_key: 'w6' },
                { tile_id: 'w7#6', tile_key: 'w7' },
                { tile_id: 'w8#7', tile_key: 'w8' },
                { tile_id: 'w9#8', tile_key: 'w9' },
                { tile_id: 't1#9', tile_key: 't1' },
                { tile_id: 't2#10', tile_key: 't2' },
                { tile_id: 't3#11', tile_key: 't3' },
                { tile_id: 't4#12', tile_key: 't4' },
                { tile_id: 'b9#discard', tile_key: 'b9' },
              ],
              melds: [],
              flowers: [],
              discards: [],
            },
            {
              seat_index: 1,
              nickname: 'Player B',
              connected: true,
              concealed_count: 13,
              melds: [],
              flowers: [],
              discards: [],
            },
            {
              seat_index: 2,
              nickname: 'Player C',
              connected: true,
              concealed_count: 13,
              melds: [],
              flowers: [],
              discards: [],
            },
            {
              seat_index: 3,
              nickname: 'Player D',
              connected: true,
              concealed_count: 13,
              melds: [],
              flowers: [],
              discards: [],
            },
          ],
        },
      }),
    });
  });

  await user.click(getLocalHandButtons()[13]!);
  await user.click(screen.getByRole('button', { name: '听' }));

  expect(socket.sentMessages.map((message) => JSON.parse(message))).toEqual([
    { type: 'join_table', payload: { nickname: 'Player A' } },
    { type: 'action_request', payload: { action_type: 'ready_hand', tile_ids: ['b9#discard'] } },
  ]);
  expect(countSelectedTiles(document.body)).toBe(0);
  expect(screen.getByLabelText('Latest discard spotlight')).toBeInTheDocument();
});
```

- [ ] **Step 2: Run the targeted frontend tests and verify they fail**

```bash
cd frontend
npm test -- --run src/lib/matchViewModel.test.ts
npm test -- --run src/components/battle-screen/BottomActionDock.test.tsx
npm test -- --run src/App.test.tsx
```

Expected:

- `matchViewModel` fails because `ready_hand` is not part of the unions and the local hand never locks.
- `BottomActionDock` fails because there is no `听` action or themed class.
- `App` fails because clicking `听` cannot send a `ready_hand` request yet.

- [ ] **Step 3: Implement the frontend action plumbing**

```ts
// frontend/src/types/match.ts
export type BackendActionType =
  | 'discard'
  | 'ready_hand'
  | 'flower'
  | 'kong'
  | 'hu'
  | 'chow'
  | 'pung'
  | 'pass';

export interface PrivatePlayerState {
  seat_index: number;
  nickname: string;
  connected: boolean;
  concealed_count: number;
  concealed_tiles?: ConcealedTile[] | null;
  melds: string[][];
  display_melds?: DisplayMeldView[];
  flowers: string[];
  discards: string[];
  is_ready_hand?: boolean;
}

export interface ActionEffectView {
  key: string;
  label: string;
  emphasis: 'draw' | 'discard' | 'claim' | 'kong' | 'system';
  seat: Seat | null;
  calloutTone?: 'chow' | 'pung' | 'kong' | 'hu' | 'ready_hand' | null;
}
```

```ts
// frontend/src/lib/matchViewModel.ts
const ACTION_ORDER: BattleActionId[] = [
  'ready',
  'start_match',
  'start_next_round',
  'restart_match',
  'discard',
  'ready_hand',
  'flower',
  'kong',
  'hu',
  'chow',
  'pung',
  'pass',
];

const PROMPT_ACTION_PRIORITY: Record<BackendActionType, number> = {
  hu: 0,
  kong: 1,
  pung: 2,
  chow: 3,
  flower: 4,
  discard: 5,
  ready_hand: 6,
  pass: 7,
};

const ACTION_LABELS: Record<BattleActionId, string> = {
  ready: '准备',
  start_match: '开始对局',
  start_next_round: '下一局',
  restart_match: '再来一局',
  discard: '出牌',
  ready_hand: '听',
  flower: '补花',
  kong: '杠',
  hu: '和牌',
  chow: '吃',
  pung: '碰',
  pass: '过',
};

function isBackendActionType(value: unknown): value is BackendActionType {
  return (
    value === 'discard' ||
    value === 'ready_hand' ||
    value === 'flower' ||
    value === 'kong' ||
    value === 'hu' ||
    value === 'chow' ||
    value === 'pung' ||
    value === 'pass'
  );
}
```

```ts
// frontend/src/lib/matchViewModel.ts
function createLocalHand(state: SessionState): BattleViewModel['localHand'] {
  const snapshot = state.roomSnapshot?.payload;
  const localSeat = snapshot?.local_seat;
  const localPlayer =
    typeof localSeat === 'number'
      ? snapshot?.private_state?.players.find((player) => player.seat_index === localSeat)
      : null;
  const localReadyHandLocked = localPlayer?.is_ready_hand === true;
  const restrictedDiscardTileIdSet = getRestrictedDiscardTileIdSet(state);
  const optimisticDiscard = getOptimisticDiscard(state);
  const optimisticFlowerTileId = getOptimisticFlowerTileId(state);
  const drawnTileId = createDrawnTileId(state);
  const replacementDrawnTileId = state.latestReplacementTileId ?? null;

  const sortedHand = (localPlayer?.concealed_tiles ?? [])
    .filter((tile) => tile.tile_id !== optimisticDiscard?.tileId && tile.tile_id !== optimisticFlowerTileId)
    .map((tile) => ({
      tileId: tile.tile_id,
      code: tile.tile_key,
      isSelected: state.selectedTileIds.includes(tile.tile_id),
      isDrawn: tile.tile_id === drawnTileId,
      isReplacementDrawn: tile.tile_id === replacementDrawnTileId,
      isFlower: isFlowerTileKey(tile.tile_key),
      isDisabled:
        localReadyHandLocked ||
        Boolean(optimisticDiscard) ||
        restrictedDiscardTileIdSet.has(tile.tile_id),
    }))
    .sort(compareLocalHandTiles);

  if (!drawnTileId) {
    return sortedHand;
  }
  const drawnTileIndex = sortedHand.findIndex((tile) => tile.tileId === drawnTileId);
  if (drawnTileIndex < 0) {
    return sortedHand;
  }
  const [drawnTile] = sortedHand.splice(drawnTileIndex, 1);
  return [...sortedHand, drawnTile];
}
```

```ts
// frontend/src/lib/matchViewModel.ts
function createReadyHandInsight(state: SessionState): BattleViewModel['readyHandInsight'] {
  if (hasOptimisticDiscardPending(state)) {
    return null;
  }

  const snapshot = state.roomSnapshot?.payload;
  const privateState = snapshot?.private_state;
  const localSeat = snapshot?.local_seat;
  if (!privateState || typeof localSeat !== 'number') {
    return null;
  }

  const localPlayer = findPrivatePlayer(state, localSeat);
  const concealedTiles = localPlayer?.concealed_tiles ?? [];
  if (concealedTiles.length === 0) {
    return null;
  }

  if (localPlayer?.is_ready_hand) {
    const waits = getReadyHandWaitsForLocalPlayer(state, null);
    return waits.length > 0
      ? {
          source: 'current',
          discardTileId: null,
          discardTileCode: null,
          waits,
        }
      : null;
  }

  const selectedDiscardTile =
    state.selectedTileIds.length === 1
      ? concealedTiles.find((tile) => tile.tile_id === state.selectedTileIds[0]) ?? null
      : null;

  if (
    selectedDiscardTile &&
    !getRestrictedDiscardTileIdSet(state).has(selectedDiscardTile.tile_id) &&
    !isFlowerTileKey(selectedDiscardTile.tile_key)
  ) {
    const waits = getReadyHandWaitsForLocalPlayer(state, selectedDiscardTile.tile_id);
    return waits.length > 0
      ? {
          source: 'selected_discard',
          discardTileId: selectedDiscardTile.tile_id,
          discardTileCode: selectedDiscardTile.tile_key,
          waits,
        }
      : null;
  }

  const waits = getReadyHandWaitsForLocalPlayer(state, null);
  return waits.length > 0
    ? {
        source: 'current',
        discardTileId: null,
        discardTileCode: null,
        waits,
      }
    : null;
}
```

```ts
// frontend/src/lib/matchViewModel.ts
const readyHandCandidateTileId =
  state.selectedTileIds.length === 1 &&
  !restrictedDiscardTileIdSet.has(state.selectedTileIds[0]) &&
  getReadyHandWaitsForLocalPlayer(state, state.selectedTileIds[0]).length > 0
    ? state.selectedTileIds[0]
    : null;

return ACTION_ORDER.map((id) => {
  let enabled = false;

  if (id === 'ready') {
    enabled = waitingControls?.canReady ?? false;
  } else if (id === 'start_match') {
    enabled = waitingControls?.canStart ?? false;
  } else if (id === 'start_next_round') {
    enabled = canContinueRound && !nextRoundConfirmation?.isLocalConfirmed;
  } else if (id === 'restart_match') {
    enabled = canRestartMatch && !restartMatchConfirmation?.isLocalConfirmed;
  } else if (options.showLocalTurnKongPrompt && id === 'kong') {
    enabled = kongCandidateGroups.length > 0;
  } else if (options.showLocalTurnKongPrompt && id === 'pass') {
    enabled = true;
  } else if (promptOptions.has(id as BackendActionType)) {
    enabled =
      id === 'discard'
        ? hasSelectedDiscard
        : id === 'ready_hand'
          ? Boolean(readyHandCandidateTileId)
          : id === 'flower'
            ? hasSelectedFlower
            : id === 'kong'
              ? kongCandidateGroups.length > 0
              : id === 'chow'
                ? chowCandidateGroups.length > 0
                : id === 'pung'
                  ? pungCandidateGroups.length > 0
                  : true;
  }

  if (
    optimisticDiscardPending &&
    (
      id === 'discard' ||
      id === 'ready_hand' ||
      id === 'flower' ||
      id === 'kong' ||
      id === 'hu' ||
      id === 'chow' ||
      id === 'pung' ||
      id === 'pass'
    )
  ) {
    enabled = false;
  }

  const emphasis =
    id === 'start_match' ||
    id === 'start_next_round' ||
    id === 'restart_match' ||
    (id === 'discard' && enabled)
      ? 'high'
      : enabled
        ? 'medium'
        : 'low';

  return {
    id,
    label: getActionLabel(state, id, waitingControls, {
      startNextRound: nextRoundConfirmation,
      restartMatch: restartMatchConfirmation,
    }),
    enabled,
    emphasis,
  };
});
```

```tsx
// frontend/src/App.tsx
function isActionBlockedByOptimisticDiscard(actionId: BattleActionId) {
  return (
    actionId === 'discard' ||
    actionId === 'ready_hand' ||
    actionId === 'flower' ||
    actionId === 'kong' ||
    actionId === 'hu' ||
    actionId === 'chow' ||
    actionId === 'pung' ||
    actionId === 'pass'
  );
}

function handleAction(actionId: BattleActionId) {
  if (state.optimisticDiscard && isActionBlockedByOptimisticDiscard(actionId)) {
    return;
  }

  if (actionId === 'discard' || actionId === 'ready_hand') {
    if (state.selectedTileIds.length !== 1) {
      return;
    }
    const discardTileId = state.selectedTileIds[0];
    if (!sendMessage(serializeClientMessage(createActionRequestMessage(actionId, [discardTileId])))) {
      return;
    }
    dispatch({ type: 'queue_optimistic_discard', tileId: discardTileId });
    dispatch({ type: 'set_selected_tiles', tileIds: [], mode: null });
    return;
  }
}
```

```tsx
// frontend/src/components/battle-screen/BottomActionDock.tsx
const ACTION_PRIORITY: Partial<Record<BattleActionView['id'], number>> = {
  hu: 0,
  kong: 1,
  pung: 2,
  chow: 3,
  flower: 4,
  discard: 5,
  ready_hand: 6,
  pass: 7,
};

function getActionEffectClass(actionId: BattleActionView['id']) {
  const lookup: Partial<Record<BattleActionView['id'], string>> = {
    flower: 'action-dock__action--flower-bloom',
    chow: 'action-dock__action--themed action-dock__action--themed-chow',
    pung: 'action-dock__action--themed action-dock__action--themed-pung',
    kong: 'action-dock__action--themed action-dock__action--themed-kong',
    discard: 'action-dock__action--themed action-dock__action--themed-discard',
    ready_hand: 'action-dock__action--themed action-dock__action--themed-ready-hand',
    pass: 'action-dock__action--themed action-dock__action--themed-pass',
  };

  return lookup[actionId] ?? '';
}
```

```css
/* frontend/src/styles/dock.css */
.action-dock__action--themed-ready-hand {
  --action-button-accent: color-mix(in srgb, var(--accent) 82%, var(--theme-border-strong));
  --action-button-accent-soft: color-mix(in srgb, var(--accent) 18%, transparent);
  --action-button-ink: color-mix(in srgb, var(--accent) 72%, var(--theme-paper-strong));
}
```

- [ ] **Step 4: Run the targeted frontend tests again**

```bash
cd frontend
npm test -- --run src/lib/matchViewModel.test.ts
npm test -- --run src/components/battle-screen/BottomActionDock.test.tsx
npm test -- --run src/App.test.tsx
```

Expected:

- All targeted tests PASS.

- [ ] **Step 5: Commit the frontend interaction slice**

```bash
git add frontend/src/types/match.ts frontend/src/lib/matchViewModel.ts frontend/src/App.tsx frontend/src/components/battle-screen/BottomActionDock.tsx frontend/src/styles/dock.css frontend/src/lib/matchViewModel.test.ts frontend/src/App.test.tsx frontend/src/components/battle-screen/BottomActionDock.test.tsx
git commit -m "feat(frontend): 接入听牌按钮与锁手交互"
```

### Task 5: Frontend `听` Callout, Event Copy, and Fan Guide

**Files:**
- Modify: `frontend/src/lib/matchViewModel.ts`
- Modify: `frontend/src/components/battle-screen/TableStage.tsx`
- Modify: `frontend/src/styles/table.css`
- Modify: `frontend/src/lib/roundEventCopy.ts`
- Modify: `frontend/src/lib/roundEventCopy.test.ts`
- Modify: `frontend/src/components/battle-screen/fanGuide.ts`
- Create: `frontend/src/components/battle-screen/fanGuide.test.ts`
- Test: `frontend/src/lib/matchViewModel.test.ts`
- Test: `frontend/src/components/battle-screen/TableStage.test.tsx`

- [ ] **Step 1: Write the failing visual/copy tests**

```tsx
// frontend/src/lib/matchViewModel.test.ts
it('maps ready_hand_declared round events into a ting action spectacle descriptor', () => {
  const base = createPlayingSessionState();
  const viewModel = createMatchViewModel({
    ...base,
    latestRoundEvent: {
      type: 'round_event',
      payload: {
        event_type: 'ready_hand_declared',
        event: {
          seat: 3,
          discard_tile_id: 'b9#discard',
          discard_tile_key: 'b9',
        },
      },
    },
  });

  expect(viewModel.actionEffect).toMatchObject({
    label: '听',
    emphasis: 'claim',
    seat: 'right',
    calloutTone: 'ready_hand',
  });
});
```

```tsx
// frontend/src/components/battle-screen/TableStage.test.tsx
it('shows the ready hand callout when a ready_hand action effect arrives', () => {
  const { container } = render(
    <TableStage
      discards={{
        top: [],
        left: ['b1'],
        right: [],
        bottom: [],
      }}
      activeSeat="bottom"
      lastDiscard="b1"
      lastDiscardSeat="left"
      promptText={null}
      actionEffect={{
        key: 'ready-1',
        label: '听',
        emphasis: 'claim',
        seat: 'left',
        calloutTone: 'ready_hand',
      }}
    />,
  );

  expect(screen.getByText('听')).toBeInTheDocument();
  expect(container.querySelector('.table-stage__action-callout--ready_hand')).not.toBeNull();
});
```

```ts
// frontend/src/lib/roundEventCopy.test.ts
it('maps ready_hand_declared to a Chinese confirmation copy', () => {
  expect(
    getRoundEventCopy(
      'ready_hand_declared',
      {
        seat: 0,
        discard_tile_id: 't5#p0-3',
      },
      [{ seat_index: 0, nickname: '小李', connected: true, ready: true }],
    ),
  ).toBe('小李听牌并打出五条');
});
```

```ts
// frontend/src/components/battle-screen/fanGuide.test.ts
import { describe, expect, it } from 'vitest';

import { getFanGuideEntry, getFanLabel } from './fanGuide';

describe('fanGuide ready_hand_win', () => {
  it('exposes the ready hand win label and guide entry', () => {
    expect(getFanLabel('ready_hand_win')).toBe('听牌成和');
    expect(getFanGuideEntry('ready_hand_win')).toMatchObject({
      fanKey: 'ready_hand_win',
      fanValue: 2,
      label: '听牌成和',
    });
  });
});
```

- [ ] **Step 2: Run the targeted visual/copy tests and verify they fail**

```bash
cd frontend
npm test -- --run src/lib/matchViewModel.test.ts
npm test -- --run src/components/battle-screen/TableStage.test.tsx
npm test -- --run src/lib/roundEventCopy.test.ts
npm test -- --run src/components/battle-screen/fanGuide.test.ts
```

Expected:

- `matchViewModel` fails because `ready_hand_declared` is still ignored.
- `TableStage` fails because there is no `ready_hand` callout tone.
- `roundEventCopy` fails because the event falls through to the generic copy.
- `fanGuide.test.ts` fails because the guide has no `ready_hand_win` entry yet.

- [ ] **Step 3: Implement the `听` callout, toast copy, and fan guide**

```ts
// frontend/src/lib/matchViewModel.ts
if (event.event_type === 'ready_hand_declared') {
  return {
    key,
    label: '听',
    emphasis: 'claim',
    seat: effectSeat,
    calloutTone: 'ready_hand',
  };
}
```

```tsx
// frontend/src/components/battle-screen/TableStage.tsx
const ACTION_CALLOUT_COPY = {
  chow: '吃',
  pung: '碰',
  kong: '杠',
  hu: '和',
  ready_hand: '听',
} as const;
```

```css
/* frontend/src/styles/table.css */
.table-stage__action-callout--ready_hand {
  --table-stage-action-callout-ink: color-mix(in srgb, var(--accent) 88%, var(--theme-paper-strong));
  --table-stage-action-callout-glow: color-mix(in srgb, var(--accent) 34%, transparent);
  --table-stage-action-callout-aura-core: color-mix(in srgb, var(--accent) 24%, transparent);
  --table-stage-action-callout-aura-ring: color-mix(in srgb, var(--theme-paper) 42%, var(--accent));
}
```

```ts
// frontend/src/lib/roundEventCopy.ts
if (eventType === 'ready_hand_declared') {
  return `${getSeatName(event.seat, seats)}听牌并打出${getTileName(getTileCodeFromTileId(event.discard_tile_id))}`;
}
```

```ts
// frontend/src/components/battle-screen/fanGuide.ts
FAN_GUIDE_DEFINITIONS.unshift({
  fanKey: 'ready_hand_win',
  fanValue: 2,
  intro: '和牌前已经成功宣告听牌，本次成和额外计 2 番。',
  example: '例：先点击“听”锁定手牌，随后自摸或荣和完成和牌。',
});

FAN_LABELS.ready_hand_win = '听牌成和';
```

- [ ] **Step 4: Run the targeted visual/copy tests again**

```bash
cd frontend
npm test -- --run src/lib/matchViewModel.test.ts
npm test -- --run src/components/battle-screen/TableStage.test.tsx
npm test -- --run src/lib/roundEventCopy.test.ts
npm test -- --run src/components/battle-screen/fanGuide.test.ts
```

Expected:

- All targeted tests PASS.

- [ ] **Step 5: Commit the visual/result slice**

```bash
git add frontend/src/lib/matchViewModel.ts frontend/src/components/battle-screen/TableStage.tsx frontend/src/styles/table.css frontend/src/lib/roundEventCopy.ts frontend/src/lib/roundEventCopy.test.ts frontend/src/components/battle-screen/fanGuide.ts frontend/src/components/battle-screen/fanGuide.test.ts frontend/src/components/battle-screen/TableStage.test.tsx frontend/src/lib/matchViewModel.test.ts
git commit -m "feat(frontend): 增加听牌特效与番种展示"
```

### Task 6: Final Verification and Release-Ready Smoke Checks

**Files:**
- Modify: none
- Test: backend + frontend targeted suites and one build

- [ ] **Step 1: Run the backend ready-hand regression set**

```bash
cd backend
cargo test parses_ready_hand_command -- --exact
cargo test active_turn_projection_includes_ready_hand_for_local_readyable_hand -- --exact
cargo test local_ready_hand_sets_flag_and_emits_ready_hand_event -- --exact
cargo test ready_hand_human_discards_drawn_tile_as_next_auto_action -- --exact
cargo test ready_hand_human_keeps_hu_when_draw_is_winning_tile -- --exact
cargo test scores_ready_hand_win_as_two_fan -- --exact
cargo test settlement_includes_ready_hand_win_for_ready_hand_winner -- --exact
```

Expected:

- All seven backend tests PASS.

- [ ] **Step 2: Run the frontend ready-hand regression set**

```bash
cd frontend
npm test -- --run src/lib/matchViewModel.test.ts
npm test -- --run src/App.test.tsx
npm test -- --run src/components/battle-screen/BottomActionDock.test.tsx
npm test -- --run src/components/battle-screen/TableStage.test.tsx
npm test -- --run src/lib/roundEventCopy.test.ts
npm test -- --run src/components/battle-screen/fanGuide.test.ts
```

Expected:

- All targeted frontend suites PASS.

- [ ] **Step 3: Run the frontend build once**

```bash
cd frontend
npm run build
```

Expected:

- `vite build` completes successfully with no TypeScript errors.

- [ ] **Step 4: Manual smoke-check the full loop**

```text
1. Start a local match where the local human can enter ready hand by discarding one selected tile.
2. Confirm the action dock shows 出牌 / 听 with 听 to the right of 出牌 and the themed outline style.
3. Click 听 and verify:
   - the selected tile is discarded
   - the table shows the 楷体 “听” callout
   - the local hand becomes non-interactive
4. Advance to the next draw for the same player and verify:
   - a non-winning draw is auto-discarded without waiting for the timeout
   - a winning draw leaves only 和牌 available
5. Finish the hand and verify the result fan list contains 听牌成和 = 2.
```

Expected:

- The complete declare-ready -> auto-play -> win -> fan breakdown loop matches the spec.

- [ ] **Step 5: Commit the integrated feature after verification**

```bash
git status --short
git add backend/src frontend/src
git commit -m "feat(mahjong): 支持听牌宣告与听牌成和"
```
