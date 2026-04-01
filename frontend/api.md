# Mahjong Frontend API Doc

## Scope

This document is written for frontend development.

Goal: a frontend engineer should be able to implement the full client by using only this document.

Protocol split:

- HTTP is used only for bootstrap.
- WebSocket is used for all room, round, action, and realtime state updates.

Base assumptions:

- Backend base URL example: `http://localhost:8000`
- WebSocket URL example: `ws://localhost:8000/ws/{table_code}`
- CORS is enabled for:
  - `http://localhost:5173`
  - `http://127.0.0.1:5173`

## Quick Start

Typical frontend flow:

1. Call `POST /api/tables` to create a table and get `table_code`.
2. Connect to `ws://<host>/ws/{table_code}`.
3. Send `join_table`.
4. Render `room_snapshot`.
5. Send `ready` when the user clicks Ready.
6. Send `start_match` when everyone is ready.
7. Drive the game entirely from:
   - `room_snapshot`
   - `action_prompt`
   - `round_event`
   - `match_result`
   - `player_presence`
   - `action_rejected`

Important:

- Most successful WebSocket actions do not return a dedicated success ack.
- The frontend must treat server-pushed state as authoritative.
- After sending an action, wait for pushed messages instead of waiting for a success response.

## HTTP API

### `GET /api/health`

Purpose:

- Liveness check

Response:

```json
{
  "status": "ok"
}
```

### `POST /api/tables`

Purpose:

- Create a new table

Request body:

- None

Response `201 Created`:

```json
{
  "table_code": "AB12CD",
  "phase": "waiting",
  "created_at": "2026-03-27T14:00:00.000000",
  "seats": []
}
```

Field meanings:

- `table_code`: room code used by HTTP and WebSocket clients
- `phase`: initial room phase, always `waiting` on create
- `created_at`: ISO datetime string
- `seats`: initially empty

## WebSocket

### Endpoint

`/ws/{table_code}`

Example:

```text
ws://localhost:8000/ws/AB12CD
```

### General Envelope

Every WebSocket message uses:

```json
{
  "type": "message_type",
  "payload": {}
}
```

Exceptions:

- Some server `round_event` messages contain an inner event object with its own `type`.
- That inner event object is not the outer WebSocket message type.

## Client -> Server Messages

### `join_table`

Purpose:

- Join a waiting or active room by occupying the next available seat

Payload:

```json
{
  "nickname": "Player 1"
}
```

Server behavior on success:

- Sends `room_snapshot` to the joining socket
- Sends `player_presence` and `room_snapshot` updates to peers
- May also send `action_prompt` if the joining player is immediately actionable

Server behavior on failure:

- Sends `action_rejected`

Possible rejection reasons:

- `table_not_found`
- `table_full`

### `reconnect`

Purpose:

- Reclaim a disconnected seat

Payload:

```json
{
  "reconnect_token": "token-from-last-room-snapshot"
}
```

Server behavior on success:

- Restores the seat
- Issues a new reconnect token
- Sends fresh `room_snapshot`
- May send `match_result`
- May send `action_prompt`

Server behavior on failure:

- Sends `action_rejected`

Possible rejection reasons:

- `invalid_reconnect_token`
- `table_not_found`

Important:

- Reconnect tokens rotate.
- After a successful reconnect, the new `room_snapshot.payload.reconnect_token` replaces the old token.
- The old token should be discarded immediately.

### `ready`

Purpose:

- Mark the local seat ready or not ready while the room is still waiting

Payload:

```json
{
  "ready": true
}
```

Success semantics:

- No dedicated success ack should be relied on.
- Use the pushed `room_snapshot` updates as the result.

Possible rejection reasons:

- `table_not_found`
- `room_already_started`
- `seat_not_owned`

### `start_match`

Purpose:

- Start the match after all 4 seats are present and ready

Payload:

```json
{}
```

Success semantics:

- Do not expect a direct success message.
- Expect pushed `room_snapshot` messages and then `action_prompt`.

Possible rejection reasons:

- `table_not_found`
- `room_already_started`
- `seat_not_owned`
- `room_not_ready`

### `start_next_round`

Purpose:

- Advance from settlement into the next round

Payload:

```json
{}
```

Success semantics:

- Do not expect a direct success ack.
- Expect a new `room_snapshot`, and usually an `action_prompt`.

Possible rejection reasons:

- `table_not_found`
- `round_not_ready`
- `seat_not_owned`

### `restart_match`

Purpose:

- Start a brand-new match in the same room after the previous match has fully finished

Payload:

```json
{}
```

Success semantics:

- Do not expect a direct success ack.
- Expect pushed `room_snapshot` messages and then `action_prompt`.

Possible rejection reasons:

- `table_not_found`
- `match_not_finished`
- `seat_not_owned`

### `action_request`

Purpose:

- Submit all in-round player actions

Payload:

```json
{
  "action_type": "discard",
  "tile_ids": ["w3#p0-4"]
}
```

Supported `action_type` values:

- `discard`
- `flower`
- `kong`
- `hu`
- `chow`
- `pung`
- `pass`

Success semantics:

- No direct success ack should be assumed.
- Wait for pushed `round_event`, `room_snapshot`, `action_prompt`, or `match_result`.

Possible rejection reasons:

- `seat_not_owned`
- `not_your_turn`
- `invalid_action`
- `select_tile_first`
- `round_not_ready`

`tile_ids` requirements by action:

- `discard`: exactly 1 tile id
- `flower`: exactly 1 tile id, must be a flower tile
- `kong`:
  - self-kong: 4 tile ids for concealed kong, or 1 tile id for add-kong
  - claim kong in claim window: 3 tile ids
- `hu`: empty array
- `chow`: exactly 2 tile ids
- `pung`: exactly 2 tile ids
- `pass`: empty array

Authoritative rule:

- Frontend should only offer buttons listed in current `action_prompt.payload.options`.

### `heartbeat`

Purpose:

- Keep the socket active and measure round-trip timing if desired

Payload example:

```json
{
  "sent_at": "2026-03-27T14:00:00+00:00"
}
```

Server response:

- Echoes the same payload in a `heartbeat` message

## Server -> Client Messages

### `room_snapshot`

This is the most important message in the protocol.

Use it as the authoritative state for:

- room phase
- seat roster
- local seat
- reconnect token
- match state
- round private state

Shape:

```json
{
  "type": "room_snapshot",
  "payload": {
    "table_code": "AB12CD",
    "phase": "playing",
    "seats": [
      {
        "seat_index": 0,
        "nickname": "P0",
        "connected": true,
        "ready": true
      }
    ],
    "local_seat": 0,
    "reconnect_token": "latest-reconnect-token",
    "match_state": {
      "prevailing_wind": "east",
      "hand_number": 1,
      "dealer_seat": 0,
      "cumulative_scores": {
        "0": 0,
        "1": 0,
        "2": 0,
        "3": 0
      },
      "match_finished": false,
      "last_completed_round_id": null
    },
    "private_state": {
      "round_id": "east-1-dealer-0-12345",
      "round_wind": "east",
      "dealer_seat": 0,
      "current_actor": 0,
      "last_discard": null,
      "pending_action": {
        "type": "active_turn",
        "seat_index": 0,
        "deadline_at": "2026-03-27T14:00:30+00:00",
        "drawn_tile_id": "w3#p0-4",
        "options": ["discard", "flower", "kong", "hu"]
      },
      "players": [
        {
          "seat_index": 0,
          "nickname": "P0",
          "connected": true,
          "concealed_count": 14,
          "concealed_tiles": [
            {
              "tile_id": "w3#p0-4",
              "tile_key": "w3"
            }
          ],
          "melds": [],
          "flowers": [],
          "discards": []
        },
        {
          "seat_index": 1,
          "nickname": "P1",
          "connected": true,
          "concealed_count": 13,
          "concealed_tiles": null,
          "melds": [],
          "flowers": [],
          "discards": []
        }
      ]
    }
  }
}
```

Top-level `payload.phase` values:

- `waiting`
- `playing`
- `settlement`
- `finished`

`private_state` visibility rules:

- In waiting rooms, `private_state` is `null`.
- In active rooms, every player receives a private view.
- The local player's own `concealed_tiles` is a full list of `{tile_id, tile_key}`.
- In `playing`, other players’ `concealed_tiles` is `null`.
- In `settlement`, all players’ `concealed_tiles` is exposed so the frontend can render final hands.
- Other players still expose `concealed_count`.

Important JSON note:

- Seat-index keyed maps are serialized as JSON objects.
- Their keys should be treated as strings in frontend code.
- Example: `cumulative_scores["0"]`, not `cumulative_scores[0]`.

### `room_snapshot.payload.seats`

Always sorted by `seat_index`.

Fields:

- `seat_index`: `0..3`
- `nickname`: player name
- `connected`: whether the socket is currently attached
- `ready`: waiting-room ready flag

### `room_snapshot.payload.match_state`

Available once a match exists.

Fields:

- `prevailing_wind`: `east | south | west | north`
- `hand_number`: `1..4`
- `dealer_seat`: `0..3`
- `cumulative_scores`: object keyed by seat index strings
- `match_finished`: boolean
- `last_completed_round_id`: string or `null`

### `room_snapshot.payload.private_state`

Fields:

- `round_id`: unique round identifier
- `round_wind`: `east | south | west | north`
- `dealer_seat`: `0..3`
- `current_actor`: `0..3`
- `last_discard`: tile key string or `null`
- `pending_action`: object or `null`
- `score_state`: current in-round scoring snapshot
- `players`: 4-seat player list

### `private_state.score_state`

This field exists so the frontend can render live in-round scoring before settlement.

Shape:

```json
{
  "flower_count_by_seat": {
    "0": 1,
    "1": 0,
    "2": 0,
    "3": 0
  },
  "kong_score_detail": [
    {
      "kong_type": "concealed_kong",
      "actor_seat": 0,
      "payer_seats": [1, 2, 3],
      "delta_by_seat": {
        "0": 3,
        "1": -1,
        "2": -1,
        "3": -1
      }
    }
  ],
  "kong_delta_by_seat": {
    "0": 3,
    "1": -1,
    "2": -1,
    "3": -1
  },
  "current_round_delta_by_seat": {
    "0": 3,
    "1": -1,
    "2": -1,
    "3": -1
  },
  "base_cumulative_scores": {
    "0": 10,
    "1": -10,
    "2": 0,
    "3": 0
  },
  "projected_cumulative_scores": {
    "0": 13,
    "1": -11,
    "2": -1,
    "3": -1
  }
}
```

Field meanings:

- `flower_count_by_seat`: currently exposed flower count for each seat
- `kong_score_detail`: per-kong live scoring detail
- `kong_delta_by_seat`: net kong score currently earned/lost this round
- `current_round_delta_by_seat`: current live round delta
- `base_cumulative_scores`: completed-match totals before applying current live round delta
- `projected_cumulative_scores`: what totals would be if the round ended immediately with only current live kong scoring applied

Current backend behavior:

- During a round, live scoring only reflects already-settled kong points.
- Winning hand settlement is still finalized only at round end.

### `private_state.players[]`

Fields:

- `seat_index`
- `nickname`
- `connected`
- `concealed_count`
- `concealed_tiles`
- `melds`
- `flowers`
- `discards`

`melds` format:

- Array of melds
- Each meld is an array of tile keys, not tile ids

Example:

```json
[
  ["w3", "w3", "w3"],
  ["east", "east", "east", "east"]
]
```

### `private_state.pending_action`

Possible `type` values:

- `opening_flowers`
- `active_turn`
- `claim_window`
- `rob_kong_window`

#### `opening_flowers`

Used only during round startup.

Fields:

- `type`: `"opening_flowers"`
- `seat_index`
- `deadline_at`
- `options`

Options:

- `["flower"]` if the current seat still has a flower in hand
- `["pass"]` if the current seat has no flower and must advance startup order

#### `active_turn`

Fields:

- `type`: `"active_turn"`
- `seat_index`
- `deadline_at`
- `drawn_tile_id`
- `options`

Possible options:

- `discard`
- `flower`
- `kong`
- `hu`

Notes:

- `drawn_tile_id` is the tile the auto-timeout will discard if the player times out.
- The local hand should still be rendered from `concealed_tiles`, not inferred from `drawn_tile_id`.

#### `claim_window`

Fields:

- `type`: `"claim_window"`
- `discarder_seat`
- `deadline_at`
- `responded_seats`
- `options`

Possible options:

- Any subset of `["hu", "kong", "pung", "chow"]`
- Always plus `pass` when actionable

Notes:

- `chow` is only offered to the next seat in turn order.
- Frontend should not infer claim legality itself.
- In a normal `claim_window`, once a seat confirms a non-`pass` action, the server may immediately auto-pass lower-priority seats that can no longer win priority against the recorded best claim.
- Equal-priority seats that can still win by turn order are not auto-passed.
- This early auto-pass behavior does not apply to `rob_kong_window`; rob-kong candidates still wait for all eligible `hu` / `pass` responses or timeout.

#### `rob_kong_window`

Fields:

- `type`: `"rob_kong_window"`
- `actor_seat`
- `tile_key`
- `deadline_at`
- `responded_seats`
- `options`

Options:

- `["hu", "pass"]` for eligible seats

### `action_prompt`

This is the lightweight "you need to act now" message.

Shape:

```json
{
  "type": "action_prompt",
  "payload": {
    "seat_index": 0,
    "options": ["discard", "kong"],
    "deadline_at": "2026-03-27T14:00:30+00:00"
  }
}
```

Usage:

- Show immediate action UI
- Start or reset the local countdown timer

Frontend rule:

- Use `room_snapshot.payload.private_state.pending_action` as the state source.
- Use `action_prompt` as the immediate trigger to focus the UI and start timers.

### `round_event`

Used for incremental animation-friendly updates.

Shape:

```json
{
  "type": "round_event",
  "payload": {
    "event_type": "tile_discarded",
    "event": {
      "type": "tile_discarded",
      "seat": 0,
      "tile_id": "w3#p0-4"
    }
  }
}
```

Known `event_type` values:

- `tile_drawn`
- `flower_exposed`
- `replacement_draw`
- `tile_discarded`
- `claim_made`
- `self_kong_declared`
- `claim_auto_passed`
- `rob_kong_auto_passed`
- `settlement_ready`
- `round_drawn`

Event payloads:

#### `tile_drawn`

```json
{
  "type": "tile_drawn",
  "seat": 0,
  "tile_id": "w3#p0-4"
}
```

#### `flower_exposed`

```json
{
  "type": "flower_exposed",
  "seat": 0,
  "tile_id": "f1#0"
}
```

#### `replacement_draw`

```json
{
  "type": "replacement_draw",
  "seat": 0,
  "tile_id": "b9#replacement"
}
```

#### `tile_discarded`

```json
{
  "type": "tile_discarded",
  "seat": 0,
  "tile_id": "w3#p0-4"
}
```

#### `claim_made`

```json
{
  "type": "claim_made",
  "seat": 2,
  "claim_type": "pung",
  "tile_id": "w5#discard"
}
```

#### `self_kong_declared`

```json
{
  "type": "self_kong_declared",
  "seat": 0,
  "kong_type": "concealed_kong",
  "tile_key": "t5",
  "tile_ids": ["t5#1", "t5#2", "t5#3", "t5#4"]
}
```

`kong_type` values seen in implementation:

- `concealed_kong`
- `add_kong`

For claim-kong flow, the visible user-facing action is represented by:

- `claim_made` with `claim_type: "kong"`

#### `claim_auto_passed`

```json
{
  "type": "claim_auto_passed",
  "discarder_seat": 0,
  "seats": [1, 2]
}
```

Meaning:

- These seats were treated as `pass` by the server in a normal discard-claim window.
- This can happen either because they timed out or because a higher-priority confirmed claim made their response unable to affect the final outcome.

#### `rob_kong_auto_passed`

```json
{
  "type": "rob_kong_auto_passed",
  "actor_seat": 0,
  "seats": [1]
}
```

#### `settlement_ready`

```json
{
  "type": "settlement_ready",
  "round_id": "east-1-dealer-0-12345"
}
```

#### `round_drawn`

```json
{
  "type": "round_drawn",
  "round_id": "east-1-dealer-0-12345"
}
```

### `match_result`

Sent when a round is in settlement.

Shape:

```json
{
  "type": "match_result",
  "payload": {
    "table_code": "AB12CD",
    "round_id": "east-1-dealer-0-12345",
    "phase": "settlement",
    "win_type": "self_draw",
    "winner_seat": 0,
    "discarder_seat": null,
    "fan_total": 8,
    "fan_keys": ["fully_concealed_hand", "self_drawn", "flower_tiles"],
    "fan_breakdown": [
      {"fan_key": "fully_concealed_hand", "fan_value": 4},
      {"fan_key": "self_drawn", "fan_value": 1},
      {"fan_key": "flower_tiles", "fan_value": 3}
    ],
    "flower_count": 3,
    "kong_score_detail": [],
    "score_delta": {
      "provisional": true,
      "basic_points": 8,
      "base_points": 8,
      "fan_total": 8,
      "minimum_qualifying_fan_total": 8,
      "fan_delta_by_seat": {
        "0": 48,
        "1": -16,
        "2": -16,
        "3": -16
      },
      "kong_delta_by_seat": {
        "0": 0,
        "1": 0,
        "2": 0,
        "3": 0
      },
      "total_delta_by_seat": {
        "0": 48,
        "1": -16,
        "2": -16,
        "3": -16
      }
    }
  }
}
```

Settlement notes:

- `win_type` can be:
  - `self_draw`
  - `discard`
  - `draw`
- `winner_seat` is `null` for exhaustive draws
- `discarder_seat` is `null` for self-draws and draws
- `draw_type: "exhaustive"` appears in draw settlements

### `player_presence`

Broadcast when a player connects or disconnects.

Shape:

```json
{
  "type": "player_presence",
  "payload": {
    "table_code": "AB12CD",
    "seat_index": 1,
    "connected": false
  }
}
```

### `action_rejected`

Sent on invalid requests or unsupported operations.

Shape:

```json
{
  "type": "action_rejected",
  "payload": {
    "reason": "invalid_action"
  }
}
```

Observed rejection reasons:

- `table_not_found`
- `table_full`
- `invalid_reconnect_token`
- `room_already_started`
- `match_not_finished`
- `seat_not_owned`
- `room_not_ready`
- `round_not_ready`
- `not_your_turn`
- `invalid_action`
- `select_tile_first`
- `unsupported_message`

Recommended frontend handling:

- `table_not_found`: show invalid room page
- `table_full`: show room-full state
- `invalid_reconnect_token`: clear saved token and force a fresh join
- `room_not_ready`: disable start button until all seats ready
- `match_not_finished`: only show "Play Again" after the room is in `finished`
- `round_not_ready`: disable post-settlement next-round button
- `select_tile_first`: prompt user to choose a tile
- `not_your_turn`: silent toast is enough
- `invalid_action`: resync from latest `room_snapshot`

### `heartbeat`

Echo format:

```json
{
  "type": "heartbeat",
  "payload": {
    "sent_at": "2026-03-27T14:00:00+00:00"
  }
}
```

## Frontend State Model

Recommended top-level client state:

- `connection`
- `tableCode`
- `reconnectToken`
- `roomPhase`
- `localSeat`
- `seats`
- `matchState`
- `privateState`
- `currentPrompt`
- `countdownDeadline`
- `lastRoundEvent`
- `lastMatchResult`

Recommended source-of-truth priority:

1. `room_snapshot`
2. `match_result`
3. `round_event`
4. `action_prompt`

Why:

- `room_snapshot` is canonical full state.
- `round_event` is best for animation, but not enough for state recovery.
- `action_prompt` is useful for urgency and timers, but should not replace `room_snapshot`.

## Action Availability Rules

Frontend should not derive legality itself.

Rules:

- Only render action buttons listed in `pending_action.options` or `action_prompt.payload.options`.
- For tile-targeted actions:
  - choose legal tiles locally from the visible hand
  - but still trust the server to validate
- If the server rejects an action, keep the latest snapshot and reset the local interaction state

## Typical Flows

### Waiting Room

1. `POST /api/tables`
2. connect WebSocket
3. send `join_table`
4. render `room_snapshot`
5. send `ready`
6. listen for snapshots reflecting ready states
7. send `start_match`
8. wait for pushed `room_snapshot`

### Round Start

1. server starts round
2. opening-flower phase may begin
3. current seat receives `action_prompt` with `["flower"]` or `["pass"]`
4. after startup order finishes, dealer receives `active_turn`

### Active Turn

1. current actor gets `active_turn`
2. local UI shows allowed options
3. player acts with `action_request`
4. server pushes `round_event`
5. server pushes `room_snapshot`
6. server may push next `action_prompt`

### Claim Window

1. discard occurs
2. eligible seats get `action_prompt`
3. claimant sends `action_request`
4. server may resolve immediately once no remaining unresolved seat can beat the current best recorded claim; otherwise it waits for the remaining required responses or timeout
5. server pushes updated `round_event` / `room_snapshot` state

### Settlement

1. round ends
2. server pushes `round_event` with `settlement_ready` or `round_drawn`
3. server pushes `room_snapshot` in `settlement`
4. server may also push `match_result`
5. a player sends `start_next_round`

### Match Finish And Replay

1. after the last hand of the match, `room_snapshot.payload.phase` becomes `finished`
2. `room_snapshot.payload.match_state.match_finished` becomes `true`
3. frontend shows a "Play Again" button
4. a player sends `restart_match`
5. server pushes fresh `room_snapshot` and `action_prompt`
6. cumulative scores reset for the new match

### Reconnect

1. save latest `reconnect_token` from every `room_snapshot`
2. on disconnect, reconnect socket
3. send `reconnect`
4. replace stored token with the new token from the returned snapshot
5. restore UI from `room_snapshot`

## Tile Identity Rules

There are two tile identifiers:

- `tile_key`
- `tile_id`

Use `tile_key` for:

- rendering suit/rank/honor/flower art
- discard piles
- meld display
- fan display logic if needed on frontend

Use `tile_id` for:

- sending actions
- identifying the exact selected tile in the local concealed hand

Examples:

- `tile_key`: `w3`
- `tile_id`: `w3#p0-4`

Do not send `tile_key` in actions where the API expects `tile_id`.

## Frontend Implementation Checklist

- Persist `table_code` and latest `reconnect_token`
- Handle reconnect token rotation
- Render seat list from `room_snapshot.payload.seats`
- Render local hand from `private_state.players[localSeat].concealed_tiles`
- Render opponent hidden hands using `concealed_count`
- Render melds, flowers, discards from `private_state.players`
- Drive all buttons from `pending_action.options`
- Start countdown timers from `deadline_at`
- Stop timers on any new `room_snapshot` or `action_prompt`
- Treat score maps as string-keyed objects
- Do not wait for success ack after `ready`, `start_match`, `start_next_round`, or `action_request`
- Use `round_event` for animation only, then reconcile against `room_snapshot`

## Suggested Frontend Pages

- Home/Create Table
- Join Room by table code
- Waiting Room
- Game Table
- Settlement Modal or Settlement Page
- Match Finished Page
- Reconnect/Resume Banner

## Suggested Error UX

- For `action_rejected`, prefer non-blocking UI unless the room is unusable
- For room-joining failures, show a dedicated page state
- For stale reconnect tokens, clear local session storage and return to join/create flow

## File References

Protocol behavior documented here is derived from:

- [app/api/http.py](/c:/Users/ZY/Desktop/Projects/dist/mahjong/app/api/http.py)
- [app/api/ws.py](/c:/Users/ZY/Desktop/Projects/dist/mahjong/app/api/ws.py)
- [app/api/schemas.py](/c:/Users/ZY/Desktop/Projects/dist/mahjong/app/api/schemas.py)
- [app/services/game_service.py](/c:/Users/ZY/Desktop/Projects/dist/mahjong/app/services/game_service.py)
- [app/services/timeout_service.py](/c:/Users/ZY/Desktop/Projects/dist/mahjong/app/services/timeout_service.py)
