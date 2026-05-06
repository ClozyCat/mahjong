# Social User Match Records Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the public friend-circle user system, invite-only table entry, multi-device seat control, match records, points, fan statistics, right sidebar, and owner-approved spectator flow.

**Architecture:** Keep the existing Rust Axum runtime and SQLite persistence, but add authenticated users and public social records around it. Split realtime into a global `/ws/me` channel for social notifications and the existing `/ws/{table_code}` table channel for gameplay; refactor table connections so one user seat can have multiple active devices.

**Tech Stack:** Rust 2024, Axum, Tokio, rusqlite, Serde, Argon2 password hashing, React 19, TypeScript, Vite, Vitest.

---

## Scope And Sequencing

This is a large multi-subsystem feature. Implement it in vertical slices. Each task should leave the project in a buildable and testable state. Do not start the frontend social sidebar before the backend user, invitation, and realtime notification surfaces exist.

The design source is `docs/superpowers/specs/2026-05-06-social-user-match-records-design.md`.

## File Structure

- Create `backend/src/app/auth.rs`: password hashing, session token hashing, auth extraction helpers.
- Create `backend/src/app/users.rs`: user title calculation, public user projection, profile updates.
- Create `backend/src/app/invites.rs`: invitation business rules and table invite helpers.
- Create `backend/src/app/records.rs`: round archive, point events, fan statistics.
- Create `backend/src/app/social_ws.rs`: `/ws/me` notification channel.
- Modify `backend/src/app/persistence.rs`: schema creation, migrations, repositories.
- Modify `backend/src/app/server.rs`: auth, users, invites, records, spectator request routes.
- Modify `backend/src/app/ws.rs`: authenticated table WebSocket, invite-based join, multi-device seat ownership, always-on spectator approval.
- Modify `backend/src/app/room_runtime.rs`: multi-connection seats, owner metadata, spectator request state.
- Modify `backend/src/app/mod.rs`: settings, app state, shared projections, spectator cfg removal.
- Modify `backend/src/core/state/room.rs` and related state modules: add user ownership and table multiplier fields.
- Modify `backend/src/main.rs`: admin invite-code command dispatch before server startup.
- Modify `backend/Cargo.toml`: add `argon2`, `password-hash`, and `base64` or equivalent token encoding dependencies.
- Modify `frontend/src/types/match.ts`: auth, user, invite, record, sidebar, spectator request types.
- Create `frontend/src/lib/authApi.ts`: register/login/me/profile calls.
- Create `frontend/src/lib/socialApi.ts`: users, invites, records calls.
- Create `frontend/src/lib/meSocket.ts`: global notification WebSocket.
- Modify `frontend/src/lib/socket.ts`: authenticated table messages and remove dynamic spectator feature import.
- Modify `frontend/src/lib/sessionReducer.ts`: account state, invite state, multi-device friendly connection state.
- Modify `frontend/src/App.tsx`: logged-in shell, lobby, invitation entry, table flow.
- Create `frontend/src/components/auth/AuthGate.tsx`: login and invite-code registration.
- Create `frontend/src/components/lobby/SocialLobby.tsx`: user list, create table multiplier selector, invite actions.
- Create `frontend/src/components/table-sidebar/TableSidebar.tsx`: right-side collapsible sidebar.
- Create `frontend/src/components/user-profile/UserProfilePanel.tsx`: public profile, fan stats, records.
- Modify `frontend/src/components/battle-screen/BattleScreen.tsx` and `TableStage.tsx`: sidebar entry and spectator request controls.
- Modify `frontend/src/styles/*.css`: lobby, auth, sidebar, profile, notification styling.
- Update tests near each touched module.

---

### Task 1: Authentication Schema And Invite-Code Admin Command

**Files:**
- Modify: `backend/Cargo.toml`
- Modify: `backend/src/main.rs`
- Modify: `backend/src/app/mod.rs`
- Modify: `backend/src/app/persistence.rs`
- Create: `backend/src/app/auth.rs`
- Test: `backend/src/app/persistence.rs`
- Test: `backend/src/app/auth.rs`

- [ ] **Step 1: Add auth dependencies**

Add password hashing and token encoding dependencies to `backend/Cargo.toml`:

```toml
argon2 = "0.5"
base64 = "0.22"
password-hash = { version = "0.5", features = ["std"] }
```

- [ ] **Step 2: Write schema tests**

Add tests proving `Database::initialize()` creates `users`, `invite_codes`, and `auth_sessions`, and that an invite code can be created, consumed once, and rejected on second use.

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml invite_code
```

Expected first run before implementation: fails because invite-code repository functions do not exist.

- [ ] **Step 3: Implement auth schema**

Extend `Database::initialize()` with creation functions for `users`, `invite_codes`, and `auth_sessions`. Use explicit schema checks like existing `ensure_tables_schema()` so incompatible old schemas fail predictably or rebuild only the affected new tables.

- [ ] **Step 4: Implement password and token helpers**

Create `backend/src/app/auth.rs` with:

```rust
pub(crate) struct AuthenticatedUser {
    pub(crate) user_id: i64,
    pub(crate) username: String,
    pub(crate) display_name: String,
}
```

Add helpers for hashing passwords with Argon2, verifying passwords, generating session tokens, hashing session tokens, and generating invite codes.

- [ ] **Step 5: Add admin command dispatch**

Modify `backend/src/main.rs` so `backend admin create-invite --count N` opens the configured SQLite database, creates N invite codes, prints one code per line, and exits without starting the server. Normal startup remains `backend::run_server().await`.

- [ ] **Step 6: Verify**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml invite_code auth
cargo test --manifest-path backend/Cargo.toml
```

Expected: both commands exit 0.

- [x] **Step 7: Commit**

```powershell
git add backend/Cargo.toml backend/src/main.rs backend/src/app/mod.rs backend/src/app/persistence.rs backend/src/app/auth.rs
git commit -m "feat(auth): 添加邀请码注册基础"
```

---

### Task 2: Auth HTTP API And Daily Login Points

**Files:**
- Modify: `backend/src/app/server.rs`
- Modify: `backend/src/app/persistence.rs`
- Modify: `backend/src/app/auth.rs`
- Create: `backend/src/app/users.rs`
- Test: `backend/src/app/server.rs`
- Test: `backend/src/app/users.rs`

- [ ] **Step 1: Write API tests**

Add HTTP handler-level tests for:

- registering with a valid invite code creates a user and session;
- reusing the same invite code fails with `invite_code_invalid`;
- first login on a Beijing local date writes `daily_login` point event with `+50`;
- second login on the same Beijing local date does not add points again.

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml register login_daily_points
```

Expected first run before implementation: fails because auth routes do not exist.

- [ ] **Step 2: Add repository methods**

Add repository methods for creating users, consuming invite codes, creating/revoking sessions, loading authenticated users by bearer token, writing daily login point events, and reading public user summaries.

- [ ] **Step 3: Add auth routes**

Add routes:

```rust
.route("/api/auth/register", post(register))
.route("/api/auth/login", post(login))
.route("/api/auth/logout", post(logout))
.route("/api/me", get(get_me).patch(update_me))
```

Requests and responses use JSON. The session token is returned in response payload and accepted as `Authorization: Bearer <token>`.

- [ ] **Step 4: Implement title projection**

Create `backend/src/app/users.rs` with title calculation using the design thresholds:

```text
points < 0 -> 乞丐
0..499 -> 平民
500..1999 -> 小康
2000..4999 -> 富豪
5000+ -> 财神
```

Public user responses include `display_name`, `points`, `title`, and `display_label`.

- [ ] **Step 5: Verify**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml register login_daily_points user_title
cargo test --manifest-path backend/Cargo.toml
```

Expected: both commands exit 0.

- [ ] **Step 6: Commit**

```powershell
git add backend/src/app/server.rs backend/src/app/persistence.rs backend/src/app/auth.rs backend/src/app/users.rs
git commit -m "feat(auth): 添加用户登录与每日积分"
```

---

### Task 3: Table Ownership, Multiplier, And Invite-Only Entry

**Files:**
- Modify: `backend/src/core/state/room.rs`
- Modify: `backend/src/app/server.rs`
- Modify: `backend/src/app/ws.rs`
- Modify: `backend/src/app/room_runtime.rs`
- Modify: `backend/src/app/persistence.rs`
- Create: `backend/src/app/invites.rs`
- Test: `backend/src/app/invites.rs`
- Test: `backend/src/app/ws.rs`

- [ ] **Step 1: Write multiplier and invitation tests**

Cover these cases:

- authenticated create table stores `owner_user_id` and `multiplier`;
- owner can change multiplier while waiting;
- owner cannot change multiplier after start, and receives `table_multiplier_locked`;
- non-owner cannot change multiplier;
- invite idle user succeeds;
- invite user in only-self-plus-bots table succeeds;
- invite user in table with another human fails with `target_player_busy`;
- accepting invite creates table participant and allows table WebSocket join.

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml multiplier invite_only
```

Expected first run before implementation: fails because multiplier and invite APIs do not exist.

- [ ] **Step 2: Extend room metadata**

Add table owner and multiplier fields to the server-side room/table metadata. Keep old room JSON readable by defaulting missing `owner_user_id` to `None` and missing `multiplier` to `1`.

- [ ] **Step 3: Add invite repositories**

Create `table_participants` and `table_invites` repository methods. Store participant nickname snapshots at join time.

- [ ] **Step 4: Add table APIs**

Add:

```rust
POST /api/tables
PATCH /api/tables/{table_code}/multiplier
POST /api/tables/{table_code}/invites
GET /api/me/invites
POST /api/invites/{invite_id}/accept
```

`POST /api/tables` now requires auth and accepts `{ "multiplier": 1 | 2 | 3 }`.

- [ ] **Step 5: Replace direct join semantics**

Change table WebSocket initial join path so unauthenticated or uninvited join attempts return `table_invite_required`. Existing `join_table` can remain parsed temporarily but must reject unless it carries a valid accepted invite context.

- [ ] **Step 6: Verify**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml multiplier invite_only
cargo test --manifest-path backend/Cargo.toml
```

Expected: both commands exit 0.

- [ ] **Step 7: Commit**

```powershell
git add backend/src/core/state/room.rs backend/src/app/server.rs backend/src/app/ws.rs backend/src/app/room_runtime.rs backend/src/app/persistence.rs backend/src/app/invites.rs
git commit -m "feat(tables): 改为邀请进入牌局"
```

---

### Task 4: Global User WebSocket Notifications

**Files:**
- Modify: `backend/src/app/server.rs`
- Modify: `backend/src/app/mod.rs`
- Create: `backend/src/app/social_ws.rs`
- Modify: `backend/src/app/invites.rs`
- Test: `backend/src/app/social_ws.rs`

- [ ] **Step 1: Write notification tests**

Test that a logged-in `/ws/me` connection receives `table_invite_created` when another user invites it, and receives `spectator_request_created` when a user asks to watch a table owned by the logged-in user.

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml social_ws
```

Expected first run before implementation: fails because `/ws/me` does not exist.

- [ ] **Step 2: Add user connection registry**

Add a registry in `AppState` mapping `user_id -> Vec<ConnectionHandle>` for global notifications. Removing one device must keep the user online if other devices remain connected.

- [ ] **Step 3: Add `/ws/me` route**

Authenticate the WebSocket using a query token or first client message with session token. After authentication, register the connection and send initial online state.

- [ ] **Step 4: Emit invite and spectator-request notifications**

When invite or spectator-request rows are created, send notification payloads to target user connections. If target is offline, persist rows only; frontend can fetch them on next login.

- [ ] **Step 5: Verify**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml social_ws
cargo test --manifest-path backend/Cargo.toml
```

Expected: both commands exit 0.

- [ ] **Step 6: Commit**

```powershell
git add backend/src/app/server.rs backend/src/app/mod.rs backend/src/app/social_ws.rs backend/src/app/invites.rs
git commit -m "feat(realtime): 添加用户通知通道"
```

---

### Task 5: Multi-Device Seat Connections

**Files:**
- Modify: `backend/src/app/room_runtime.rs`
- Modify: `backend/src/app/ws.rs`
- Modify: `backend/src/app/mod.rs`
- Modify: `backend/src/app/scheduler.rs`
- Test: `backend/src/app/ws.rs`
- Test: `backend/src/app/room_runtime.rs`

- [ ] **Step 1: Write multi-device tests**

Test:

- two connections for the same user and seat both receive snapshots;
- either connection can submit a legal action;
- disconnecting one connection does not mark the seat disconnected;
- disconnecting the last connection marks the seat disconnected.

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml multi_device
```

Expected first run before implementation: fails because runtime stores a single handle per seat.

- [ ] **Step 2: Replace seat connection map**

Change runtime from `HashMap<usize, ConnectionHandle>` to a grouped structure that contains `user_id` and multiple handles per seat. Provide helper functions:

- `add_seat_connection`
- `remove_seat_connection`
- `snapshot_seat_connections`
- `seat_has_live_connections`
- `broadcast_to_seat_group`

- [ ] **Step 3: Update action ownership checks**

Change `assert_active_owned_seat` to validate that the session user owns the seat, not that the exact connection handle is the single stored handle.

- [ ] **Step 4: Update disconnect behavior**

Only call `set_seat_connected(false, ...)` when the final connection for that seat disappears.

- [ ] **Step 5: Verify**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml multi_device
cargo test --manifest-path backend/Cargo.toml
```

Expected: both commands exit 0.

- [ ] **Step 6: Commit**

```powershell
git add backend/src/app/room_runtime.rs backend/src/app/ws.rs backend/src/app/mod.rs backend/src/app/scheduler.rs
git commit -m "feat(ws): 支持多设备操作同一座位"
```

---

### Task 6: Match Records, Points Settlement, And Fan Statistics

**Files:**
- Create: `backend/src/app/records.rs`
- Modify: `backend/src/app/persistence.rs`
- Modify: `backend/src/app/ws.rs`
- Modify: `backend/src/rules/standard/win.rs`
- Modify: `backend/src/app/server.rs`
- Test: `backend/src/app/records.rs`
- Test: `backend/src/rules/standard/win.rs`

- [ ] **Step 1: Write record and point tests**

Test:

- settlement creates one `round_record`;
- each human participant gets one `round_player_result`;
- point delta equals score delta times locked multiplier;
- bot seats do not get point events;
- if settlement contains an independent bot seat, all human `point_delta` values stay `0` and no user points are updated;
- a human seat with bot takeover still counts as human for point settlement;
- repeated archive for the same round does not double-apply points;
- winning fan keys increment `user_fan_stats`.

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml records point_events fan_stats
```

Expected first run before implementation: fails because record repositories do not exist.

- [ ] **Step 2: Add record schema and repositories**

Create `game_records`, `round_records`, `round_player_results`, `user_point_events`, and `user_fan_stats`. Provide transactional write helpers so record rows, point events, user point totals, and fan stats are committed together. When a settled room contains an independent bot seat, persist record rows but skip player point application for that round.

- [ ] **Step 3: Hook settlement archival**

After `RoundSettlement` is finalized and room state is persisted, call record archival with table code, multiplier, participants, and settlement. Avoid writing records for provisional settlements.

- [ ] **Step 4: Add record APIs**

Add:

```rust
GET /api/games
GET /api/games/{game_id}
GET /api/users/{user_id}/games
GET /api/users/{user_id}/fans
GET /api/leaderboard
```

All are publicly readable.

- [ ] **Step 5: Verify**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml records point_events fan_stats
cargo test --manifest-path backend/Cargo.toml
```

Expected: both commands exit 0.

- [ ] **Step 6: Commit**

```powershell
git add backend/src/app/records.rs backend/src/app/persistence.rs backend/src/app/ws.rs backend/src/rules/standard/win.rs backend/src/app/server.rs
git commit -m "feat(records): 归档牌局积分和番种"
```

---

### Task 7: Always-On Spectator Approval

**Files:**
- Modify: `backend/Cargo.toml`
- Modify: `backend/src/app/ws.rs`
- Modify: `backend/src/app/room_runtime.rs`
- Modify: `backend/src/app/server.rs`
- Modify: `backend/src/app/persistence.rs`
- Modify: `backend/src/projection/room_snapshot.rs`
- Modify: `Dockerfile`
- Modify: `docker-compose.yml`
- Modify: `frontend/vite.config.ts`
- Modify: `frontend/src/vite-env.d.ts`
- Modify: `frontend/src/features/spectator/socket.ts`
- Test: `backend/src/app/ws.rs`
- Test: `backend/src/projection/room_snapshot.rs`

- [ ] **Step 1: Write spectator approval tests**

Test:

- observer snapshot builds in default backend test run without `--features spectator`;
- player in a table cannot request to watch that same table;
- non-player request creates a pending request for owner;
- approved request allows `watch_table`;
- unapproved request rejects `watch_table` with `spectator_requires_owner_approval`.

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml spectator
```

Expected first run before implementation: fails because spectator code is still cfg-gated or lacks approval.

- [ ] **Step 2: Remove spectator feature gates**

Remove `spectator = []` from `backend/Cargo.toml` and remove `#[cfg(feature = "spectator")]` / `#[cfg_attr(not(feature = "spectator"), ...)]` around spectator code. Observer projection becomes default code.

- [ ] **Step 3: Remove frontend build gate**

Remove `__SPECTATOR_ENABLED__` declarations and Vite define usage. Replace dynamic spectator import in `App.tsx` with normal imports or move `createWatchTableMessage` into `frontend/src/lib/socket.ts`.

- [ ] **Step 4: Add spectator request schema and APIs**

Create `spectator_requests` repository and APIs:

```rust
POST /api/tables/{table_code}/spectator-requests
GET /api/me/spectator-requests
POST /api/spectator-requests/{request_id}/approve
POST /api/spectator-requests/{request_id}/reject
```

- [ ] **Step 5: Enforce approved watch**

`watch_table` must authenticate the user and verify an approved request for the table unless the user is the table owner using a future owner-preview path. In the first version, even owner uses player view for their own table and does not watch their own table.

- [ ] **Step 6: Verify**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml spectator
cargo test --manifest-path backend/Cargo.toml
Push-Location frontend; npm test -- App.test.tsx socket.test.ts; Pop-Location
```

Expected: all commands exit 0.

- [ ] **Step 7: Commit**

```powershell
git add backend/Cargo.toml backend/src/app/ws.rs backend/src/app/room_runtime.rs backend/src/app/server.rs backend/src/app/persistence.rs backend/src/projection/room_snapshot.rs Dockerfile docker-compose.yml frontend/vite.config.ts frontend/src/vite-env.d.ts frontend/src/features/spectator/socket.ts frontend/src/lib/socket.ts frontend/src/App.tsx
git commit -m "feat(spectator): 默认启用房主审批观战"
```

---

### Task 8: Frontend Auth And Lobby

**Files:**
- Create: `frontend/src/components/auth/AuthGate.tsx`
- Create: `frontend/src/components/auth/AuthGate.test.tsx`
- Create: `frontend/src/components/lobby/SocialLobby.tsx`
- Create: `frontend/src/components/lobby/SocialLobby.test.tsx`
- Create: `frontend/src/lib/authApi.ts`
- Create: `frontend/src/lib/socialApi.ts`
- Create: `frontend/src/lib/meSocket.ts`
- Modify: `frontend/src/App.test.tsx`
- Modify: `frontend/src/lib/socket.ts`
- Modify: `frontend/src/lib/socket.test.ts`
- Modify: `frontend/src/types/match.ts`
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/styles/layout.css`
- Modify: `frontend/src/styles/panels.css`

- [x] **Step 1: Write auth and lobby tests**

Test:

- unauthenticated app shows login/register UI;
- registration sends invite code, nickname, and password;
- logged-in lobby shows current user label with title;
- create table form requires multiplier `1`, `2`, or `3`;
- invite action calls the invite API;
- receiving `table_invite_created` on `/ws/me` opens an invitation dialog.

Run:

```powershell
Push-Location frontend; npm test -- AuthGate.test.tsx SocialLobby.test.tsx App.test.tsx; Pop-Location
```

Expected first run before implementation: fails because components do not exist.

- [x] **Step 2: Implement auth API client**

Add `register`, `login`, `logout`, `getMe`, and `updateMe` functions. Store session token in existing storage helpers or a new focused auth storage helper.

- [x] **Step 3: Implement `AuthGate`**

Render login and invite-code registration modes. Use normal text labels and buttons; do not expose password in logs or status messages.

- [x] **Step 4: Implement `SocialLobby`**

Render current user, multiplier selector, create table button, online user list, invite buttons, pending invites, and public leaderboard summary.

- [x] **Step 5: Connect `/ws/me`**

Use `meSocket.ts` to receive invite, spectator request, and point update notifications. Keep reconnect logic separate from table WebSocket reconnect.

- [x] **Step 6: Verify**

Run:

```powershell
Push-Location frontend; npm test -- AuthGate.test.tsx SocialLobby.test.tsx App.test.tsx; npm run build; Pop-Location
```

Expected: tests and build exit 0.

- [ ] **Step 7: Commit**

```powershell
git add frontend/src/components/auth frontend/src/components/lobby frontend/src/lib/authApi.ts frontend/src/lib/socialApi.ts frontend/src/lib/meSocket.ts frontend/src/lib/socket.ts frontend/src/lib/socket.test.ts frontend/src/types/match.ts frontend/src/App.tsx frontend/src/App.test.tsx frontend/src/styles/layout.css frontend/src/styles/panels.css
git commit -m "feat(frontend): 添加登录和社交大厅"
```

---

### Task 9: Table Sidebar, Profiles, And Spectator UI

**Files:**
- Create: `frontend/src/components/table-sidebar/TableSidebar.tsx`
- Create: `frontend/src/components/table-sidebar/TableSidebar.test.tsx`
- Create: `frontend/src/components/user-profile/UserProfilePanel.tsx`
- Create: `frontend/src/components/user-profile/UserProfilePanel.test.tsx`
- Modify: `backend/src/app/mod.rs`
- Modify: `backend/src/app/room_runtime.rs`
- Modify: `backend/src/app/scheduler.rs`
- Modify: `backend/src/app/ws.rs`
- Modify: `backend/src/projection/room_snapshot.rs`
- Modify: `frontend/src/components/battle-screen/BattleScreen.tsx`
- Modify: `frontend/src/components/battle-screen/TableStage.tsx`
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/lib/socialApi.ts`
- Modify: `frontend/src/types/match.ts`
- Modify: `frontend/src/styles/table.css`
- Modify: `frontend/src/styles/panels.css`

- [x] **Step 1: Write sidebar tests**

Test:

- sidebar is collapsed by default during a table;
- opening sidebar shows tabs for table players, online players, player info, spectators, and owner spectator requests;
- non-owner does not see approval buttons;
- owner can approve and reject spectator requests;
- profile panel renders fan stats and recent records.

Run:

```powershell
Push-Location frontend; npm test -- TableSidebar.test.tsx UserProfilePanel.test.tsx BattleScreen.test.tsx; Pop-Location
```

Expected first run before implementation: fails because sidebar components do not exist.

- [x] **Step 2: Implement `UserProfilePanel`**

Display public user label, points, title, fan stats, and recent games.

- [x] **Step 3: Implement `TableSidebar`**

Use stable width and overlay behavior. Tabs:

- 本局玩家
- 在线玩家
- 玩家信息
- 观战者
- 观战申请

- [x] **Step 4: Integrate into `BattleScreen`**

Add a right-edge icon button that opens the sidebar. Keep the table layout stable and avoid resizing tile grids when the sidebar opens.

- [x] **Step 5: Add spectator request UI**

In lobby and sidebar, allow users to request to watch. In owner sidebar, show pending requests with approve/reject buttons.

- [x] **Step 6: Verify**

Run:

```powershell
Push-Location frontend; npm test -- TableSidebar.test.tsx UserProfilePanel.test.tsx BattleScreen.test.tsx App.test.tsx; npm run build; Pop-Location
```

Expected: tests and build exit 0.

- [ ] **Step 7: Commit**

```powershell
git add frontend/src/components/table-sidebar frontend/src/components/user-profile frontend/src/components/battle-screen/BattleScreen.tsx frontend/src/components/battle-screen/TableStage.tsx frontend/src/App.tsx frontend/src/lib/socialApi.ts frontend/src/types/match.ts frontend/src/styles/table.css frontend/src/styles/panels.css
git commit -m "feat(ui): 添加牌桌社交侧边栏"
```

---

### Task 10: End-To-End Verification And Compatibility Cleanup

**Files:**
- Review all changed files.
- Update deployment docs if admin invite command or auth environment variables need documentation.

- [ ] **Step 1: Run backend tests**

```powershell
cargo test --manifest-path backend/Cargo.toml
```

Expected: exit 0.

- [ ] **Step 2: Run frontend tests and build**

```powershell
Push-Location frontend
npm test
npm run build
Pop-Location
```

Expected: exit 0 for both commands.

- [ ] **Step 3: Run targeted manual smoke**

Start the app locally and verify:

- admin command creates invite code;
- first user registers and creates a x2 table;
- second user receives invite popup and enters table;
- same user logs in from another browser context and controls the same seat;
- owner changes multiplier before start and cannot change after start;
- settlement updates points and fan stats;
- third user requests spectating and owner approval is required;
- sidebar opens from the right and lists players, online users, profile, spectators, and requests.

- [ ] **Step 4: Check removed spectator gate**

Search for removed build-time controls:

```powershell
rg "__SPECTATOR_ENABLED__|feature = \"spectator\"|MAHJONG_ENABLE_SPECTATOR" backend frontend Dockerfile docker-compose.yml
```

Expected: no matches except historical docs or tests that explicitly explain removal.

- [ ] **Step 5: Check git status**

```powershell
git status --short
```

Expected: no unintended untracked files. Generated local caches may remain ignored.

- [ ] **Step 6: Final commit for documentation updates**

If deployment docs changed:

```powershell
git add DEPLOYMENT_SOP.md DEPLOYMENT_SOP_PAGES_SERVER.md
git commit -m "docs: 补充用户系统部署说明"
```

If deployment docs did not change, skip this commit.

---

## Self-Review

Spec coverage:

- Invite-code registration is covered by Tasks 1 and 2.
- Daily login points are covered by Task 2.
- Invite-only entry and busy-player rules are covered by Task 3.
- Table multiplier and post-start locking are covered by Task 3.
- Global realtime invitation and spectator-request popups are covered by Task 4.
- Multi-device same-seat control is covered by Task 5.
- Match records, point settlement, and fan statistics are covered by Task 6.
- Always-on owner-approved spectator flow is covered by Task 7.
- Login lobby, invitation popups, and multiplier UI are covered by Task 8.
- Right sidebar, public profiles, online players, and spectator lists are covered by Task 9.
- Final verification and spectator build-gate cleanup are covered by Task 10.

Placeholder scan:

- The plan contains no unresolved placeholder markers.
- Each task names concrete files and verification commands.

Type consistency:

- `user_id`, `table_code`, `multiplier`, `session token`, `spectator_request`, and `table_invite` names are used consistently with the design document.
- Error identifiers match the design document where they affect API or WebSocket behavior.
