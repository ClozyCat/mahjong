# Spectator System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a compile-time optional spectator mode that lets observers watch all four hands and switch table perspective without occupying a player seat.

**Architecture:** Use a backend Cargo feature named `spectator` and a frontend Vite build constant named `__SPECTATOR_ENABLED__`. The backend registers spectator WebSocket connections separately from seat connections and sends an observer snapshot with all concealed tiles visible; the frontend exposes spectator entry points only when the build constant is true and reuses the existing battle screen with a local spectator focus seat.

**Tech Stack:** Rust 2024, Axum WebSocket, Tokio, Serde, React 19, TypeScript, Vite, Vitest, Docker Compose.

---

## File Structure

- Modify `backend/Cargo.toml`: add the `spectator` Cargo feature.
- Modify `backend/src/app/room_runtime.rs`: add cfg-gated spectator connection storage and snapshot helpers.
- Modify `backend/src/projection/room_snapshot.rs`: add cfg-gated observer snapshot projection.
- Modify `backend/src/app/mod.rs`: add cfg-gated observer outbound builders and include observers in broadcast paths.
- Modify `backend/src/app/ws.rs`: add cfg-gated `watch_table` parsing, connection role state, handler, disconnect cleanup, and tests.
- Modify `frontend/vite.config.ts`: define `__SPECTATOR_ENABLED__` from `MAHJONG_ENABLE_SPECTATOR`.
- Modify `frontend/src/vite-env.d.ts`: declare `__SPECTATOR_ENABLED__`.
- Modify `frontend/src/types/match.ts`: add spectator session types and `watch_table` client message.
- Modify `frontend/src/lib/socket.ts`: add cfg-reachable `createWatchTableMessage`.
- Modify `frontend/src/lib/sessionReducer.ts`: track spectator mode without reconnect tokens.
- Modify `frontend/src/lib/matchViewModel.ts`: add `perspectiveSeat` option and spectator disabled action behavior.
- Modify `frontend/src/components/connect-gate/ConnectGate.tsx`: add optional spectator button prop.
- Modify `frontend/src/components/battle-screen/BottomActionDock.tsx`: add spectator perspective switch control.
- Modify `frontend/src/components/battle-screen/BattleScreen.tsx`: pass spectator controls and hide actions in observer mode.
- Modify `frontend/src/App.tsx`: add spectator connection flow behind `__SPECTATOR_ENABLED__`.
- Modify `frontend/src/styles/dock.css`: style the perspective switch button.
- Modify `Dockerfile`: pass `MAHJONG_ENABLE_SPECTATOR` to frontend and backend build stages.
- Modify `docker-compose.yml`: add build args for source builds.
- Modify `docker-compose.prebuilt.yml`: document the prebuilt image expectation.
- Test files:
  - `backend/src/app/ws.rs`
  - `backend/src/projection/room_snapshot.rs`
  - `frontend/src/lib/socket.ts` or `frontend/src/lib/api.test.ts` if socket tests are colocated later
  - `frontend/src/lib/sessionReducer.test.ts`
  - `frontend/src/lib/matchViewModel.test.ts`
  - `frontend/src/components/connect-gate/ConnectGate.test.tsx`
  - `frontend/src/components/battle-screen/BottomActionDock.test.tsx`

---

### Task 1: Build-Time Feature Switches

**Files:**
- Modify: `backend/Cargo.toml`
- Modify: `frontend/vite.config.ts`
- Modify: `frontend/src/vite-env.d.ts`
- Modify: `Dockerfile`
- Modify: `docker-compose.yml`
- Modify: `docker-compose.prebuilt.yml`

- [ ] **Step 1: Add the backend feature flag**

Update `backend/Cargo.toml`:

```toml
[features]
default = []
spectator = []
```

- [ ] **Step 2: Add the frontend build constant**

Update `frontend/vite.config.ts`:

```ts
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

const spectatorEnabled = process.env.MAHJONG_ENABLE_SPECTATOR === 'true';

export default defineConfig({
  plugins: [react()],
  define: {
    __SPECTATOR_ENABLED__: JSON.stringify(spectatorEnabled),
  },
});
```

- [ ] **Step 3: Declare the build constant for TypeScript**

Append to `frontend/src/vite-env.d.ts`:

```ts
declare const __SPECTATOR_ENABLED__: boolean;
```

- [ ] **Step 4: Wire Docker build args into the Dockerfile**

In `Dockerfile`, add the build arg to the frontend build stage and pass it into `npm run build`:

```dockerfile
FROM node:22-bookworm-slim AS frontend-builder

ARG MAHJONG_ENABLE_SPECTATOR=false
ENV MAHJONG_ENABLE_SPECTATOR=${MAHJONG_ENABLE_SPECTATOR}
```

In the backend builder stage, add:

```dockerfile
FROM rust:1.94-bookworm AS rust-backend-builder

ARG MAHJONG_ENABLE_SPECTATOR=false
```

Replace the backend build command with:

```dockerfile
RUN if [ "$MAHJONG_ENABLE_SPECTATOR" = "true" ]; then \
      cargo build --release --features spectator; \
    else \
      cargo build --release; \
    fi
```

- [ ] **Step 5: Add Compose build args**

Update both build targets in `docker-compose.yml`:

```yaml
build:
  context: .
  dockerfile: Dockerfile
  target: backend-runtime
  args:
    MAHJONG_ENABLE_SPECTATOR: ${MAHJONG_ENABLE_SPECTATOR:-false}
```

```yaml
build:
  context: .
  dockerfile: Dockerfile
  target: frontend-runtime
  args:
    MAHJONG_ENABLE_SPECTATOR: ${MAHJONG_ENABLE_SPECTATOR:-false}
```

In `docker-compose.prebuilt.yml`, keep images unchanged and add this environment note to both services:

```yaml
environment:
  MAHJONG_DATABASE_URL: ${MAHJONG_DATABASE_URL:-sqlite+pysqlite:////data/mahjong.db}
  MAHJONG_ENABLE_SPECTATOR: ${MAHJONG_ENABLE_SPECTATOR:-false}
```

Also add a comment above the `image:` lines:

```yaml
# Prebuilt images must be built with the same MAHJONG_ENABLE_SPECTATOR value.
# This runtime variable documents the selected deployment mode; it cannot add compiled-out code.
```

- [ ] **Step 6: Verify build switches parse**

Run:

```powershell
cargo check --manifest-path backend/Cargo.toml
cargo check --manifest-path backend/Cargo.toml --features spectator
Push-Location frontend; npm run build; Pop-Location
```

Expected: all commands exit 0. The default frontend build must still succeed when `MAHJONG_ENABLE_SPECTATOR` is unset.

- [ ] **Step 7: Commit build switch work**

```powershell
git add backend/Cargo.toml frontend/vite.config.ts frontend/src/vite-env.d.ts Dockerfile docker-compose.yml docker-compose.prebuilt.yml
git commit -m "chore: 添加观战编译开关"
```

---

### Task 2: Backend Spectator Runtime And Observer Snapshot

**Files:**
- Modify: `backend/src/app/room_runtime.rs`
- Modify: `backend/src/projection/room_snapshot.rs`
- Modify: `backend/src/app/mod.rs`
- Test: `backend/src/projection/room_snapshot.rs`

- [ ] **Step 1: Write observer snapshot tests**

Add this cfg-gated test to `backend/src/projection/room_snapshot.rs` tests:

```rust
#[cfg(feature = "spectator")]
#[test]
fn observer_snapshot_exposes_all_concealed_tiles_without_prompt_options() {
    let mut state = RoomState {
        table_code: "ROOM42".to_string(),
        phase: "playing".to_string(),
        mode: "normal".to_string(),
        seats: seats(),
        match_state: None,
        round_state: Some(RoundState {
            round_id: "round-1".to_string(),
            dealer_seat: 0,
            round_wind: "east".to_string(),
            current_actor: 1,
            phase: "playing".to_string(),
            players: players(),
            ..Default::default()
        }),
        pending_timeout: Some(PendingTimeout {
            kind: "active_turn".to_string(),
            seat_index: 1,
            deadline_at: Some("2026-04-20T12:00:30.000Z".to_string()),
            drawn_tile_id: Some("w3#draw".to_string()),
        }),
        continue_action: None,
    };

    let round = state.round_state.as_mut().expect("round exists");
    for player in &mut round.players {
        player.concealed_tiles = vec![crate::core::tile::TileState {
            tile_id: format!("w{}#{}", player.seat + 1, player.seat),
            tile_key: format!("w{}", player.seat + 1),
            kind: "suit".to_string(),
        }];
    }

    let snapshot = observer_room_snapshot_message(&state);
    let players = snapshot["payload"]["private_state"]["players"]
        .as_array()
        .expect("players should serialize");

    assert!(snapshot["payload"]["local_seat"].is_null());
    assert!(snapshot["payload"]["reconnect_token"].is_null());
    assert_eq!(
        snapshot["payload"]["private_state"]["pending_action"]["options"],
        serde_json::json!([])
    );
    assert!(players.iter().all(|player| {
        player["concealed_tiles"].as_array().is_some_and(|tiles| tiles.len() == 1)
    }));
}
```

- [ ] **Step 2: Run the observer snapshot test and confirm it fails**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml --features spectator observer_snapshot_exposes_all_concealed_tiles_without_prompt_options
```

Expected: FAIL because `observer_room_snapshot_message` does not exist.

- [ ] **Step 3: Add spectator connection storage helpers**

In `backend/src/app/room_runtime.rs`, add the cfg-gated fields and helpers:

```rust
#[cfg(feature = "spectator")]
pub(crate) type SpectatorConnections = Vec<(u64, ConnectionHandle)>;

pub(crate) struct RoomRuntime {
    pub(crate) created_at: String,
    pub(crate) room: RoomState,
    pub(crate) connections: HashMap<usize, ConnectionHandle>,
    #[cfg(feature = "spectator")]
    pub(crate) spectator_connections: HashMap<u64, ConnectionHandle>,
    pub(crate) timeout_nonce: u64,
    pub(crate) continue_nonce: u64,
    pub(crate) disconnect_nonce: u64,
    pub(crate) bot_nonce: u64,
    pub(crate) timeout_task: Option<JoinHandle<()>>,
    pub(crate) continue_task: Option<JoinHandle<()>>,
    pub(crate) disconnect_task: Option<JoinHandle<()>>,
    pub(crate) bot_task: Option<JoinHandle<()>>,
}
```

Set it in `RoomRuntime::new`:

```rust
#[cfg(feature = "spectator")]
spectator_connections: HashMap::new(),
```

Update `close_runtime`:

```rust
#[cfg(feature = "spectator")]
{
    for connection in runtime.spectator_connections.values() {
        connection.request_close();
    }
    runtime.spectator_connections.clear();
}
```

Add helpers:

```rust
#[cfg(feature = "spectator")]
pub(crate) fn replace_spectator_connection(
    runtime: &mut RoomRuntime,
    spectator_id: u64,
    connection: &ConnectionHandle,
) {
    if let Some(previous) = runtime.spectator_connections.insert(spectator_id, connection.clone()) {
        if previous.id != connection.id {
            previous.request_close();
        }
    }
}

#[cfg(feature = "spectator")]
pub(crate) fn snapshot_spectator_connections(runtime: &RoomRuntime) -> SpectatorConnections {
    runtime
        .spectator_connections
        .iter()
        .map(|(spectator_id, handle)| (*spectator_id, handle.clone()))
        .collect()
}

#[cfg(feature = "spectator")]
pub(crate) fn remove_spectator_connection(
    runtime: &mut RoomRuntime,
    spectator_id: u64,
    connection_id: u64,
) {
    if runtime
        .spectator_connections
        .get(&spectator_id)
        .is_some_and(|handle| handle.id == connection_id)
    {
        runtime.spectator_connections.remove(&spectator_id);
    }
}
```

- [ ] **Step 4: Add observer snapshot projection**

In `backend/src/projection/room_snapshot.rs`, add cfg-gated public function:

```rust
#[cfg(feature = "spectator")]
pub fn observer_room_snapshot_message(state: &RoomState) -> Value {
    let payload = PlayerRoomSnapshot {
        table_code: state.table_code.clone(),
        phase: state.phase.clone(),
        mode: state.mode.clone(),
        seats: public_seats(state),
        local_seat: usize::MAX,
        reconnect_token: None,
        match_state: state.match_state.clone(),
        private_state: observer_round_state(state),
        continue_action: continue_action_snapshot(state),
    };

    let mut value = serde_json::to_value(RoomSnapshotMessage {
        kind: "room_snapshot",
        payload,
    })
    .unwrap_or_else(|_| json!({ "type": "room_snapshot", "payload": Value::Null }));
    value["payload"]["local_seat"] = Value::Null;
    value
}
```

Add cfg-gated helper. It should mirror `private_round_state`, but always includes `concealed_tiles` and uses empty support for pending action:

```rust
#[cfg(feature = "spectator")]
fn observer_round_state(state: &RoomState) -> Option<PlayerRoundView> {
    let round = state.round_state.as_ref()?;
    let players = round
        .players
        .iter()
        .map(|player| {
            let seat_info = state.seats.iter().find(|seat| seat.seat_index == player.seat);
            PlayerSeatView {
                seat_index: player.seat,
                nickname: seat_info.and_then(|seat| seat.nickname.clone()),
                connected: seat_info.map(|seat| seat.connected).unwrap_or(false),
                is_ready_hand: player.is_ready_hand,
                concealed_count: player.concealed_tiles.len(),
                concealed_tiles: Some(
                    player
                        .concealed_tiles
                        .iter()
                        .map(|tile| PrivateTileView {
                            tile_id: tile.tile_id.clone(),
                            tile_key: tile.tile_key.clone(),
                        })
                        .collect(),
                ),
                melds: player.melds.clone(),
                display_melds: project_display_melds(&player.display_melds),
                flowers: player.flowers.iter().map(|tile| tile.tile_key.clone()).collect(),
                discards: player.discards.iter().map(|tile| tile.tile_key.clone()).collect(),
            }
        })
        .collect();

    Some(PlayerRoundView {
        round_id: round.round_id.clone(),
        round_wind: round.round_wind.clone(),
        dealer_seat: round.dealer_seat,
        current_actor: round.current_actor,
        wall_tiles_remaining: round.wall.live_tiles_remaining(),
        last_discard: round.last_discard.as_ref().map(|tile| tile.tile_key.clone()),
        pending_action: observer_pending_action(state),
        hand_insights: None,
        score_state: score_state_view(state),
        players,
    })
}
```

Add cfg-gated pending action scrubber:

```rust
#[cfg(feature = "spectator")]
fn observer_pending_action(state: &RoomState) -> Option<PendingActionView> {
    let pending_timeout = state.pending_timeout.as_ref()?;
    match pending_timeout.kind.as_str() {
        "active_turn" => Some(PendingActionView::ActiveTurn {
            seat_index: pending_timeout.seat_index,
            deadline_at: pending_timeout.deadline_at.clone(),
            drawn_tile_id: None,
            restricted_discard_tile_ids: Vec::new(),
            options: Vec::new(),
        }),
        "claim_window" => {
            let round = state.round_state.as_ref()?;
            let PendingAction::ClaimWindow(claim) = round.pending_action.as_ref()? else {
                return None;
            };
            Some(PendingActionView::ClaimWindow {
                discarder_seat: claim.discarder_seat,
                deadline_at: pending_timeout.deadline_at.clone(),
                responded_seats: claim.responded_seats.clone(),
                options: Vec::new(),
            })
        }
        "rob_kong_window" => {
            let round = state.round_state.as_ref()?;
            let PendingAction::RobKongWindow(rob) = round.pending_action.as_ref()? else {
                return None;
            };
            Some(PendingActionView::RobKongWindow {
                actor_seat: rob.actor_seat,
                tile_key: rob.tile_key.clone(),
                deadline_at: pending_timeout.deadline_at.clone(),
                responded_seats: rob.responded_seats.clone(),
                options: Vec::new(),
            })
        }
        _ => None,
    }
}
```

- [ ] **Step 5: Add observer outbound builders**

In `backend/src/app/mod.rs`, import the cfg-gated projection:

```rust
#[cfg(feature = "spectator")]
use crate::projection::room_snapshot::observer_room_snapshot_message;
```

Add:

```rust
#[cfg(feature = "spectator")]
pub(crate) fn build_room_messages_for_observer(
    room: &RoomState,
    connection: &ConnectionHandle,
) -> Vec<OutboundMessage> {
    let mut payloads = vec![observer_room_snapshot_message(room)];
    if let Some(result) = match_result_message(room) {
        payloads.push(result);
    }
    payloads
        .into_iter()
        .map(|payload| connection.outbound(payload))
        .collect()
}

#[cfg(feature = "spectator")]
pub(crate) fn collect_observer_outbound_from_snapshot(
    room: &RoomState,
    connections: &[(u64, ConnectionHandle)],
) -> Vec<OutboundMessage> {
    connections
        .iter()
        .flat_map(|(_, handle)| build_room_messages_for_observer(room, handle))
        .collect()
}
```

- [ ] **Step 6: Run projection tests**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml --features spectator observer_snapshot_exposes_all_concealed_tiles_without_prompt_options
cargo test --manifest-path backend/Cargo.toml
```

Expected: both commands exit 0.

- [ ] **Step 7: Commit backend projection work**

```powershell
git add backend/src/app/room_runtime.rs backend/src/projection/room_snapshot.rs backend/src/app/mod.rs
git commit -m "feat: 添加观战快照投影"
```

---

### Task 3: Backend WebSocket Watch Flow

**Files:**
- Modify: `backend/src/app/ws.rs`
- Modify: `backend/src/app/mod.rs`
- Test: `backend/src/app/ws.rs`

- [ ] **Step 1: Add parser tests**

In `backend/src/app/ws.rs` tests, add:

```rust
#[cfg(feature = "spectator")]
#[test]
fn parse_watch_table_when_spectator_feature_is_enabled() {
    let parsed = parse_client_message(r#"{"type":"watch_table","payload":{"nickname":"Viewer"}}"#)
        .expect("watch_table should parse");
    assert!(matches!(parsed, ClientMessage::WatchTable(_)));
}

#[cfg(not(feature = "spectator"))]
#[test]
fn reject_watch_table_when_spectator_feature_is_disabled() {
    let result = parse_client_message(r#"{"type":"watch_table","payload":{"nickname":"Viewer"}}"#);
    assert!(result.is_err());
}
```

- [ ] **Step 2: Run parser tests and confirm spectator-enabled test fails**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml --features spectator parse_watch_table_when_spectator_feature_is_enabled
```

Expected: FAIL because `ClientMessage::WatchTable` does not exist.

- [ ] **Step 3: Add cfg-gated client message and request**

In `ClientMessage`:

```rust
#[cfg(feature = "spectator")]
WatchTable(WatchTableRequest),
```

Near other request structs:

```rust
#[cfg(feature = "spectator")]
#[derive(Debug, Default, Deserialize)]
struct WatchTableRequest {
    #[serde(default)]
    nickname: String,
}
```

- [ ] **Step 4: Replace owned seat state with connection role**

Add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionRole {
    Unbound,
    Player { seat_index: usize },
    #[cfg(feature = "spectator")]
    Spectator { spectator_id: u64 },
}

impl ConnectionRole {
    fn owned_seat(self) -> Option<usize> {
        match self {
            ConnectionRole::Player { seat_index } => Some(seat_index),
            ConnectionRole::Unbound => None,
            #[cfg(feature = "spectator")]
            ConnectionRole::Spectator { .. } => None,
        }
    }
}
```

Change `MessageOutcome`:

```rust
pub(crate) struct MessageOutcome {
    pub(crate) outbound: Vec<OutboundMessage>,
    pub(crate) role: Option<ConnectionRole>,
    pub(crate) clear_role: bool,
    pub(crate) close_socket: bool,
}
```

In existing handlers, replace `owned_seat: Some(seat_index)` with `role: Some(ConnectionRole::Player { seat_index })`; replace `owned_seat: None` with `role: None`; replace `clear_owned_seat` with `clear_role`.

- [ ] **Step 5: Add watch handler**

Import cfg-gated helpers:

```rust
#[cfg(feature = "spectator")]
use super::room_runtime::{replace_spectator_connection, remove_spectator_connection, snapshot_spectator_connections};
#[cfg(feature = "spectator")]
use super::collect_observer_outbound_from_snapshot;
```

Add handler:

```rust
#[cfg(feature = "spectator")]
async fn handle_watch_table(
    state: AppContext,
    table_code: &str,
    connection: &ConnectionHandle,
    request: WatchTableRequest,
) -> MessageOutcome {
    let _nickname = request.nickname.trim();
    let Some(room_handle) = ensure_room_loaded(&state, table_code).await.ok().flatten() else {
        return reject_to(connection, "table_not_found");
    };
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }

    let spectator_id = connection.id;
    let mut runtime = room_handle.runtime.lock().await;
    if room_handle.is_closed() {
        return reject_to(connection, "table_not_found");
    }
    replace_spectator_connection(&mut runtime, spectator_id, connection);
    let room = runtime.room.clone();
    drop(runtime);

    MessageOutcome {
        outbound: collect_observer_outbound_from_snapshot(&room, &[(spectator_id, connection.clone())]),
        role: Some(ConnectionRole::Spectator { spectator_id }),
        clear_role: false,
        close_socket: false,
    }
}
```

- [ ] **Step 6: Route watch messages and reject player messages from spectators**

In `handle_client_message`, add:

```rust
#[cfg(feature = "spectator")]
ClientMessage::WatchTable(request) => {
    if !matches!(role, ConnectionRole::Unbound) {
        return reject_to(connection, "seat_already_owned");
    }
    handle_watch_table(state, table_code, connection, request).await
}
```

For player-only branches, use:

```rust
let Some(seat_index) =
    assert_active_owned_seat(&state, table_code, connection, role.owned_seat()).await
else {
    return reject_to(connection, "seat_not_owned");
};
```

- [ ] **Step 7: Clean up spectator disconnects**

Add cfg-gated function:

```rust
#[cfg(feature = "spectator")]
async fn handle_spectator_disconnect(
    state: AppContext,
    table_code: &str,
    spectator_id: u64,
    connection_id: u64,
) {
    let Some(room_handle) = room_handle(&state, table_code).await else {
        return;
    };
    if room_handle.is_closed() {
        return;
    }
    let mut runtime = room_handle.runtime.lock().await;
    remove_spectator_connection(&mut runtime, spectator_id, connection_id);
}
```

In `websocket_session`, call this when `role` is spectator. Keep player disconnect behavior unchanged:

```rust
match role {
    ConnectionRole::Player { seat_index } if !close_socket => {
        handle_disconnect(state, &table_code, Some(seat_index), connection_id).await;
    }
    #[cfg(feature = "spectator")]
    ConnectionRole::Spectator { spectator_id } if !close_socket => {
        handle_spectator_disconnect(state, &table_code, spectator_id, connection_id).await;
    }
    _ => {}
}
```

- [ ] **Step 8: Broadcast snapshots and round events to observers**

In each place that calls `snapshot_connections(&runtime)`, also collect `snapshot_spectator_connections(&runtime)` under cfg. After player outbound is built, extend it with observer outbound:

```rust
#[cfg(feature = "spectator")]
let spectator_connections = snapshot_spectator_connections(&runtime);
```

```rust
#[cfg(feature = "spectator")]
outbound.extend(collect_observer_outbound_from_snapshot(&room, &spectator_connections));
```

For raw round event broadcast in `handle_action_request`, include spectator handles in `broadcast_handles`:

```rust
#[cfg(feature = "spectator")]
broadcast_handles.extend(
    snapshot_spectator_connections(&runtime)
        .into_iter()
        .map(|(_, handle)| handle),
);
```

- [ ] **Step 9: Run backend tests**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml
cargo test --manifest-path backend/Cargo.toml --features spectator
```

Expected: both commands exit 0. Default build must pass `reject_watch_table_when_spectator_feature_is_disabled`.

- [ ] **Step 10: Commit backend WebSocket work**

```powershell
git add backend/src/app/ws.rs backend/src/app/mod.rs
git commit -m "feat: 添加观战连接流程"
```

---

### Task 4: Frontend Spectator Protocol And Lobby Entry

**Files:**
- Modify: `frontend/src/types/match.ts`
- Modify: `frontend/src/lib/socket.ts`
- Modify: `frontend/src/lib/sessionReducer.ts`
- Modify: `frontend/src/components/connect-gate/ConnectGate.tsx`
- Modify: `frontend/src/App.tsx`
- Test: `frontend/src/components/connect-gate/ConnectGate.test.tsx`
- Test: `frontend/src/lib/sessionReducer.test.ts`

- [ ] **Step 1: Add ConnectGate spectator tests**

Add to `ConnectGate.test.tsx`:

```tsx
it('renders spectator entry only when enabled by props', async () => {
  const user = userEvent.setup();
  const onWatch = vi.fn();

  const { rerender } = render(
    <ConnectGate
      value={{ tableCode: 'AB12CD', nickname: 'Viewer' }}
      status="idle"
      themeLabel="天水碧"
      canCreate={true}
      canJoin={true}
      canWatch={false}
      onChange={vi.fn()}
      onCreate={vi.fn()}
      onJoin={vi.fn()}
      onWatch={onWatch}
    />,
  );

  expect(screen.queryByRole('button', { name: '观战牌桌' })).toBeNull();

  rerender(
    <ConnectGate
      value={{ tableCode: 'AB12CD', nickname: 'Viewer' }}
      status="idle"
      themeLabel="天水碧"
      canCreate={true}
      canJoin={true}
      canWatch={true}
      onChange={vi.fn()}
      onCreate={vi.fn()}
      onJoin={vi.fn()}
      onWatch={onWatch}
    />,
  );

  await user.click(screen.getByRole('button', { name: '观战牌桌' }));
  expect(onWatch).toHaveBeenCalledTimes(1);
});
```

- [ ] **Step 2: Run the ConnectGate test and confirm it fails**

Run:

```powershell
Push-Location frontend; npm test -- ConnectGate.test.tsx; Pop-Location
```

Expected: FAIL because `canWatch` and `onWatch` props do not exist.

- [ ] **Step 3: Add spectator types**

In `frontend/src/types/match.ts`, add:

```ts
export type ClientMode = 'player' | 'spectator';
```

Update `SessionState`:

```ts
clientMode?: ClientMode;
spectatorFocusSeat?: number | null;
```

Add to `ClientMessage` union:

```ts
| { type: 'watch_table'; payload: { nickname: string } }
```

- [ ] **Step 4: Add socket message creator**

In `frontend/src/lib/socket.ts`:

```ts
export function createWatchTableMessage(nickname: string): ClientMessage {
  return {
    type: 'watch_table',
    payload: {
      nickname,
    },
  };
}
```

- [ ] **Step 5: Update session reducer for spectator mode**

In `createInitialSessionState`, set:

```ts
clientMode: 'player',
spectatorFocusSeat: null,
```

Add actions:

```ts
| { type: 'set_client_mode'; clientMode: SessionState['clientMode'] }
| { type: 'set_spectator_focus_seat'; seatIndex: number | null }
```

Handle them:

```ts
case 'set_client_mode':
  return {
    ...state,
    clientMode: action.clientMode,
    reconnectToken: action.clientMode === 'spectator' ? null : state.reconnectToken,
  };
case 'set_spectator_focus_seat':
  return {
    ...state,
    spectatorFocusSeat: action.seatIndex,
    selectedTileIds: [],
    selectionMode: null,
  };
```

In `room_snapshot`, avoid retaining reconnect tokens for spectators:

```ts
reconnectToken:
  state.clientMode === 'spectator'
    ? null
    : message.payload.reconnect_token ?? state.reconnectToken,
```

- [ ] **Step 6: Add ConnectGate props and button**

Update props:

```ts
canWatch?: boolean;
onWatch?: () => void;
```

Render the button only when `onWatch` is provided:

```tsx
{onWatch ? (
  <button
    type="button"
    className="connect-gate__btn connect-gate__btn--secondary"
    onClick={onWatch}
    disabled={!canWatch}
  >
    观战牌桌
  </button>
) : null}
```

- [ ] **Step 7: Add App spectator connection flow behind the build constant**

Import:

```ts
createWatchTableMessage,
```

Extend `openRoomSocket` options:

```ts
mode?: 'player' | 'spectator';
```

In `socket.onopen`:

```ts
const message =
  mode === 'spectator'
    ? createWatchTableMessage(nickname)
    : reconnect && reconnectToken
      ? createReconnectMessage(reconnectToken)
      : createJoinTableMessage(nickname);
```

Before opening:

```ts
dispatch({ type: 'set_client_mode', clientMode: mode ?? 'player' });
```

Add:

```ts
function handleWatch() {
  if (!__SPECTATOR_ENABLED__) {
    return;
  }
  if (!connectValue.tableCode.trim()) {
    setStatusMessage('观战前请先填写牌桌编号。');
    return;
  }
  if (tableCodeError) {
    return;
  }

  requestLandscapeOrientation();
  setStatusMessage('正在进入观战...');
  dispatch({ type: 'set_config', apiBaseUrl: defaults.apiBaseUrl, wsBaseUrl: defaults.wsBaseUrl });
  openRoomSocket({
    tableCode: normalizedRequestedTableCode,
    nickname: connectValue.nickname.trim() || '观众',
    wsBaseUrl: defaults.wsBaseUrl,
    mode: 'spectator',
  });
}
```

Pass props:

```tsx
canWatch={
  __SPECTATOR_ENABLED__ &&
  state.connectionStatus !== 'connecting' &&
  state.connectionStatus !== 'reconnecting' &&
  normalizedRequestedTableCode.length > 0 &&
  tableCodeError === null
}
onWatch={__SPECTATOR_ENABLED__ ? handleWatch : undefined}
```

- [ ] **Step 8: Prevent spectator reconnect persistence**

Change stored session effect:

```ts
if (state.clientMode === 'spectator') {
  clearStoredSession();
  return;
}
```

In `socket.onclose`, before reconnect-token retry:

```ts
if (current.clientMode === 'spectator') {
  handleLeaveToLobby(current.tableCode, '观战连接已断开。');
  return;
}
```

- [ ] **Step 9: Run frontend tests for lobby and reducer**

Run:

```powershell
Push-Location frontend; npm test -- ConnectGate.test.tsx sessionReducer.test.ts; Pop-Location
```

Expected: both test files pass.

- [ ] **Step 10: Commit frontend protocol and lobby work**

```powershell
git add frontend/src/types/match.ts frontend/src/lib/socket.ts frontend/src/lib/sessionReducer.ts frontend/src/components/connect-gate/ConnectGate.tsx frontend/src/App.tsx frontend/src/components/connect-gate/ConnectGate.test.tsx frontend/src/lib/sessionReducer.test.ts
git commit -m "feat: 添加观战入口"
```

---

### Task 5: Frontend Spectator Perspective Switching

**Files:**
- Modify: `frontend/src/lib/matchViewModel.ts`
- Modify: `frontend/src/components/battle-screen/BottomActionDock.tsx`
- Modify: `frontend/src/components/battle-screen/BattleScreen.tsx`
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/styles/dock.css`
- Test: `frontend/src/lib/matchViewModel.test.ts`
- Test: `frontend/src/components/battle-screen/BottomActionDock.test.tsx`

- [ ] **Step 1: Add view model spectator tests**

Add to `matchViewModel.test.ts`:

```ts
it('uses spectator perspective seat for hand and relative positions', () => {
  const base = createPlayingSessionState({
    clientMode: 'spectator',
    spectatorFocusSeat: 1,
    roomSnapshot: {
      type: 'room_snapshot',
      payload: {
        ...createPlayingSessionState().roomSnapshot!.payload,
        local_seat: null,
        reconnect_token: null,
        private_state: {
          ...createPlayingSessionState().roomSnapshot!.payload.private_state!,
          pending_action: {
            type: 'active_turn',
            seat_index: 2,
            deadline_at: '2026-03-26T06:01:00Z',
            options: [],
          },
          players: createPlayingSessionState().roomSnapshot!.payload.private_state!.players.map((player) => ({
            ...player,
            concealed_tiles: [
              { tile_id: `seat-${player.seat_index}#0`, tile_key: `w${player.seat_index + 1}` },
            ],
          })),
        },
      },
    },
    latestActionPrompt: null,
  });

  const viewModel = createMatchViewModel(base, {
    perspectiveSeat: 1,
    isSpectator: true,
  });

  expect(viewModel.players.find((player) => player.absoluteSeat === 1)).toMatchObject({
    seat: 'bottom',
    isLocal: false,
  });
  expect(viewModel.players.find((player) => player.absoluteSeat === 2)?.seat).toBe('right');
  expect(viewModel.localHand.map((tile) => tile.code)).toEqual(['w2']);
  expect(viewModel.actions.every((action) => action.enabled === false)).toBe(true);
  expect(viewModel.waitingControls).toBeNull();
  expect(viewModel.mode).toBe('watching');
});
```

- [ ] **Step 2: Run view model test and confirm it fails**

Run:

```powershell
Push-Location frontend; npm test -- matchViewModel.test.ts -t "spectator perspective"; Pop-Location
```

Expected: FAIL because `perspectiveSeat` and `isSpectator` options do not exist.

- [ ] **Step 3: Add perspective options to view model**

Update `MatchViewModelOptions`:

```ts
interface MatchViewModelOptions {
  showLocalTurnKongPrompt?: boolean;
  perspectiveSeat?: number | null;
  isSpectator?: boolean;
}
```

Replace `getLocalSeat` with:

```ts
function getPerspectiveSeat(state: SessionState, options: MatchViewModelOptions = {}): number {
  if (typeof options.perspectiveSeat === 'number') {
    return options.perspectiveSeat;
  }
  return state.roomSnapshot?.payload.local_seat ?? 0;
}
```

Update functions that currently call `getLocalSeat(state)` for display mapping to accept options or a `perspectiveSeat` argument:

```ts
function createPlayers(state: SessionState, options: MatchViewModelOptions = {}): PlayerView[] {
  const localSeat = getPerspectiveSeat(state, options);
```

Apply the same pattern to `createDiscards`, `createLocalHand`, `createSelectedTileCode`, `createHandInsight`, `createResult`, `createLastDiscardSeat`, `createActionIndicatorSeat`, `createActionEffect`, `createQuickChatEvent`, `createSettlementHands`, and `createScoreSummaryLabel`.

- [ ] **Step 4: Disable actions and waiting controls for spectators**

In `createWaitingControls`:

```ts
function createWaitingControls(state: SessionState, options: MatchViewModelOptions = {}): WaitingControls | null {
  if (options.isSpectator) {
    return null;
  }
```

In `createActionViews`, return disabled actions for spectators:

```ts
if (options.isSpectator) {
  return ACTION_ORDER.map((id) => ({
    id,
    label: ACTION_LABELS[id],
    enabled: false,
    emphasis: 'low',
  }));
}
```

In `createMatchViewModel`, compute:

```ts
const localSeat = getPerspectiveSeat(state, options);
const waitingControls = createWaitingControls(state, options);
```

Set spectator mode:

```ts
: options.isSpectator
  ? 'watching'
```

- [ ] **Step 5: Add BottomActionDock perspective switch test**

Add to `BottomActionDock.test.tsx`:

```tsx
it('renders spectator perspective switch and disables tile interaction', async () => {
  const user = userEvent.setup();
  const onSwitchPerspective = vi.fn();
  const onTileSelect = vi.fn();

  render(
    <BottomActionDock
      hand={localHand}
      claimCandidates={[]}
      actions={[]}
      isElevated={false}
      isSpectator
      spectatorFocusName="Player B"
      promptCue={null}
      deadlineAt={null}
      onSwitchPerspective={onSwitchPerspective}
      onTileSelect={onTileSelect}
      onTileDoubleClick={vi.fn()}
      onClaimCandidateSelect={vi.fn()}
      onClaimCandidateActivate={vi.fn()}
      onAction={vi.fn()}
    />,
  );

  await user.click(screen.getByRole('button', { name: '切换观战视角，当前 Player B' }));
  expect(onSwitchPerspective).toHaveBeenCalledTimes(1);

  await user.click(getLocalHandButton(0));
  expect(onTileSelect).not.toHaveBeenCalled();
});
```

- [ ] **Step 6: Add BottomActionDock spectator props and button**

Update props:

```ts
isSpectator?: boolean;
spectatorFocusName?: string | null;
onSwitchPerspective?: () => void;
```

Render before the hand:

```tsx
{isSpectator && onSwitchPerspective ? (
  <button
    type="button"
    className="action-dock__spectator-switch"
    aria-label={`切换观战视角，当前 ${spectatorFocusName ?? '未知玩家'}`}
    title="切换观战视角"
    onClick={onSwitchPerspective}
  >
    <span aria-hidden="true">↓</span>
  </button>
) : null}
```

In hand tile buttons:

```tsx
disabled={isSpectator || tile.isDisabled}
aria-label={isSpectator ? `${tile.code} 观战模式` : tile.isDisabled ? `${tile.code} 当前回合禁止打出` : undefined}
onClick={(event) => {
  if (isSpectator || event.detail > 1) {
    return;
  }
  onTileSelect(tile.tileId);
}}
onDoubleClick={() => {
  if (!isSpectator) {
    onTileDoubleClick(tile.tileId);
  }
}}
```

- [ ] **Step 7: Add BattleScreen spectator props**

Update `BattleScreenProps`:

```ts
isSpectator?: boolean;
spectatorFocusName?: string | null;
onSwitchSpectatorPerspective?: () => void;
```

Pass to `BottomActionDock`:

```tsx
isSpectator={isSpectator}
spectatorFocusName={spectatorFocusName}
onSwitchPerspective={onSwitchSpectatorPerspective}
```

Set actions passed to `BottomActionDock`:

```tsx
actions={isSpectator ? [] : battleActions}
```

Set pre-match actions:

```tsx
preMatchActions={!isSpectator && viewModel.waitingControls ? preMatchActions : []}
```

- [ ] **Step 8: Add App focus seat state and switching**

Add helpers in `App.tsx`:

```ts
function getOccupiedSpectatorSeats(snapshot: SessionState['roomSnapshot']) {
  return snapshot?.payload.seats.map((seat) => seat.seat_index).sort((left, right) => left - right) ?? [];
}

function resolveSpectatorFocusSeat(state: SessionState) {
  const seats = getOccupiedSpectatorSeats(state.roomSnapshot);
  if (seats.length === 0) {
    return 0;
  }
  if (typeof state.spectatorFocusSeat === 'number' && seats.includes(state.spectatorFocusSeat)) {
    return state.spectatorFocusSeat;
  }
  return seats.includes(0) ? 0 : seats[0];
}
```

Add effect:

```ts
useEffect(() => {
  if (state.clientMode !== 'spectator' || !state.roomSnapshot) {
    return;
  }
  const nextSeat = resolveSpectatorFocusSeat(state);
  if (state.spectatorFocusSeat !== nextSeat) {
    dispatch({ type: 'set_spectator_focus_seat', seatIndex: nextSeat });
  }
}, [state]);
```

Add switch handler:

```ts
function handleSwitchSpectatorPerspective() {
  const seats = getOccupiedSpectatorSeats(state.roomSnapshot);
  if (seats.length === 0) {
    return;
  }
  const current = resolveSpectatorFocusSeat(state);
  const currentIndex = seats.indexOf(current);
  const nextSeat = seats[(currentIndex + 1) % seats.length] ?? seats[0];
  dispatch({ type: 'set_spectator_focus_seat', seatIndex: nextSeat });
}
```

Create view model:

```ts
const isSpectator = state.clientMode === 'spectator';
const spectatorFocusSeat = isSpectator ? resolveSpectatorFocusSeat(state) : null;
const viewModel = createMatchViewModel(state, {
  showLocalTurnKongPrompt: !isSpectator && hasLocalTurnKongPrompt,
  isSpectator,
  perspectiveSeat: spectatorFocusSeat,
});
```

Pass:

```tsx
isSpectator={isSpectator}
spectatorFocusName={
  isSpectator
    ? state.roomSnapshot?.payload.seats.find((seat) => seat.seat_index === spectatorFocusSeat)?.nickname ?? null
    : null
}
onSwitchSpectatorPerspective={isSpectator ? handleSwitchSpectatorPerspective : undefined}
```

- [ ] **Step 9: Add dock CSS**

In `frontend/src/styles/dock.css`:

```css
.action-dock__spectator-switch {
  inline-size: 2.4rem;
  block-size: 2.4rem;
  border: 1px solid color-mix(in srgb, currentColor 25%, transparent);
  border-radius: 999px;
  background: color-mix(in srgb, var(--surface-elevated, #111827) 86%, transparent);
  color: inherit;
  display: inline-grid;
  place-items: center;
  cursor: pointer;
}

.action-dock__spectator-switch:hover {
  border-color: color-mix(in srgb, currentColor 45%, transparent);
}
```

- [ ] **Step 10: Run frontend spectator tests**

Run:

```powershell
Push-Location frontend; npm test -- matchViewModel.test.ts BottomActionDock.test.tsx; Pop-Location
```

Expected: both test files pass.

- [ ] **Step 11: Commit spectator perspective work**

```powershell
git add frontend/src/lib/matchViewModel.ts frontend/src/components/battle-screen/BottomActionDock.tsx frontend/src/components/battle-screen/BattleScreen.tsx frontend/src/App.tsx frontend/src/styles/dock.css frontend/src/lib/matchViewModel.test.ts frontend/src/components/battle-screen/BottomActionDock.test.tsx
git commit -m "feat: 添加观战视角切换"
```

---

### Task 6: Final Verification And Docker Builds

**Files:**
- Review: all changed files
- Optional docs update: `DEPLOYMENT_SOP.md` if deployment docs need a command example

- [ ] **Step 1: Run backend default and spectator tests**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml
cargo test --manifest-path backend/Cargo.toml --features spectator
```

Expected: both commands exit 0.

- [ ] **Step 2: Run frontend tests and builds**

Run:

```powershell
Push-Location frontend
npm test
npm run build
$env:MAHJONG_ENABLE_SPECTATOR='true'; npm run build; Remove-Item Env:\MAHJONG_ENABLE_SPECTATOR
Pop-Location
```

Expected: tests pass, default build passes, spectator build passes.

- [ ] **Step 3: Build Docker images in both modes**

Run:

```powershell
$env:MAHJONG_ENABLE_SPECTATOR='false'; docker compose build
$env:MAHJONG_ENABLE_SPECTATOR='true'; docker compose build
Remove-Item Env:\MAHJONG_ENABLE_SPECTATOR
```

Expected: both builds complete. If Docker is unavailable, record the exact error in the final handoff.

- [ ] **Step 4: Inspect compiled frontend behavior manually**

Default build:

```powershell
Push-Location frontend
npm run build
Select-String -Path dist\\assets\\*.js -Pattern '观战牌桌|watch_table'
Pop-Location
```

Expected: no matches.

Spectator build:

```powershell
Push-Location frontend
$env:MAHJONG_ENABLE_SPECTATOR='true'; npm run build; Remove-Item Env:\MAHJONG_ENABLE_SPECTATOR
Select-String -Path dist\\assets\\*.js -Pattern '观战牌桌|watch_table'
Pop-Location
```

Expected: matches exist.

- [ ] **Step 5: Check git status**

Run:

```powershell
git status --short
```

Expected: no uncommitted files except intentional local artifacts such as build output ignored by git.

- [ ] **Step 6: Final commit if verification caused doc changes**

If deployment docs were updated:

```powershell
git add DEPLOYMENT_SOP.md
git commit -m "docs: 补充观战部署配置"
```

If no docs were updated, skip this commit.

---

## Self-Review

Spec coverage:

- Four-hand visibility: Task 2 adds observer projection and test.
- Seat-free spectator connection: Task 3 adds separate spectator runtime storage and watch flow.
- Compile-time backend removal: Task 1 adds Cargo feature; Task 3 cfg-gates parser and flow.
- Compile-time frontend removal: Task 1 adds build constant; Task 4 gates lobby entry and watch flow.
- Docker deployment switch: Task 1 wires Compose and Dockerfile.
- Perspective switch button: Task 5 adds BottomActionDock control and App focus switching.
- No spectator player actions: Task 3 rejects player-only messages; Task 5 disables frontend actions.
- No reconnect token: Task 4 clears spectator reconnect persistence.
- Verification: Task 6 covers backend, frontend, Docker, and bundle inspection.

Placeholder scan:

- The plan contains no unresolved placeholder markers.
- Every task names exact files and verification commands.

Type consistency:

- Backend feature name is consistently `spectator`.
- Frontend build constant is consistently `__SPECTATOR_ENABLED__`.
- Deployment switch is consistently `MAHJONG_ENABLE_SPECTATOR`.
- Frontend state uses `clientMode` and `spectatorFocusSeat`.
