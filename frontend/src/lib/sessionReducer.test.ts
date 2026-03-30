import { describe, expect, it } from 'vitest';

import type { ActionPromptMessage, PlayerPresenceMessage, RoomSnapshotMessage } from '../types/match';
import { createInitialSessionState, sessionReducer } from './sessionReducer';

const roomSnapshotMessage: RoomSnapshotMessage = {
  type: 'room_snapshot',
  payload: {
    table_code: 'AB12CD',
    phase: 'waiting',
    seats: [{ seat_index: 0, nickname: 'Player A', connected: true, ready: false }],
    local_seat: 0,
    reconnect_token: 'token-1',
  },
};

const playerPresenceMessage: PlayerPresenceMessage = {
  type: 'player_presence',
  payload: {
    table_code: 'AB12CD',
    seat_index: 0,
    connected: false,
  },
};

const actionPromptMessage: ActionPromptMessage = {
  type: 'action_prompt',
  payload: {
    seat_index: 0,
    options: ['discard'],
    deadline_at: '2026-03-26T06:01:00Z',
  },
};

describe('sessionReducer', () => {
  it('stores reconnect token from room_snapshot and clears stale rejection errors', () => {
    const next = sessionReducer(
      {
        ...createInitialSessionState(),
        lastRejectedAction: {
          type: 'action_rejected',
          payload: {
            reason: 'room_not_ready',
          },
        },
      },
      {
        type: 'ws_message',
        message: roomSnapshotMessage,
      },
    );

    expect(next.reconnectToken).toBe('token-1');
    expect(next.lastRejectedAction).toBeNull();
    expect(next.roomSnapshot?.payload.table_code).toBe('AB12CD');
  });

  it('clears stale action prompts when a fresh room_snapshot arrives', () => {
    const next = sessionReducer(
      {
        ...createInitialSessionState(),
        latestActionPrompt: actionPromptMessage,
      },
      {
        type: 'ws_message',
        message: {
          ...roomSnapshotMessage,
          payload: {
            ...roomSnapshotMessage.payload,
            phase: 'playing',
          },
        },
      },
    );

    expect(next.latestActionPrompt).toBeNull();
  });

  it('keeps room_snapshot authoritative when player_presence arrives', () => {
    const next = sessionReducer(
      {
        ...createInitialSessionState(),
        roomSnapshot: roomSnapshotMessage,
      },
      {
        type: 'ws_message',
        message: playerPresenceMessage,
      },
    );

    expect(next.roomSnapshot?.payload.seats[0].connected).toBe(true);
    expect(next.toasts.at(-1)?.kind).toBe('presence');
  });

  it('localizes tile_discarded round events into Win98 toast copy', () => {
    const next = sessionReducer(
      {
        ...createInitialSessionState(),
        roomSnapshot: {
          ...roomSnapshotMessage,
          payload: {
            ...roomSnapshotMessage.payload,
            seats: [{ seat_index: 0, nickname: '小李', connected: true, ready: false }],
          },
        },
      },
      {
        type: 'ws_message',
        message: {
          type: 'round_event',
          payload: {
            event_type: 'tile_discarded',
            event: {
              seat: 0,
              tile_id: 't5#p0-3',
            },
          },
        },
      },
    );

    expect(next.toasts.at(-1)).toMatchObject({
      kind: 'event',
      text: '小李打出五条',
    });
  });
});
