import { describe, expect, it } from 'vitest';

import type { ActionPromptMessage, MatchResultMessage, PlayerPresenceMessage, RoomSnapshotMessage } from '../types/match';
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

const playingRoomSnapshotMessage: RoomSnapshotMessage = {
  type: 'room_snapshot',
  payload: {
    table_code: 'AB12CD',
    phase: 'playing',
    seats: [
      { seat_index: 0, nickname: 'Player A', connected: true, ready: true },
      { seat_index: 1, nickname: 'Player B', connected: true, ready: true },
      { seat_index: 2, nickname: 'Player C', connected: true, ready: true },
      { seat_index: 3, nickname: 'Player D', connected: true, ready: true },
    ],
    local_seat: 0,
    reconnect_token: 'token-1',
    match_state: {
      prevailing_wind: 'east',
      hand_number: 1,
      dealer_seat: 0,
      cumulative_scores: { '0': 0, '1': 0, '2': 0, '3': 0 },
      match_finished: false,
      last_completed_round_id: null,
    },
  },
};

const matchResultMessage: MatchResultMessage = {
  type: 'match_result',
  payload: {
    table_code: 'AB12CD',
    round_id: 'round-1',
    phase: 'settlement',
    win_type: 'discard',
    winner_seat: 1,
    discarder_seat: 0,
    display_win_label: null,
    fan_total: 8,
    fan_keys: ['ping_hu'],
    fan_breakdown: [{ fan_key: 'ping_hu', fan_value: 8 }],
    flower_count: 0,
    score_delta: {
      provisional: false,
      fan_total: 8,
      fan_delta_by_seat: { '0': -8, '1': 8, '2': 0, '3': 0 },
      kong_delta_by_seat: { '0': 0, '1': 0, '2': 0, '3': 0 },
      total_delta_by_seat: { '0': -8, '1': 8, '2': 0, '3': 0 },
    },
    kong_score_detail: [],
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

  it('keeps the latest round_event sticky across room_snapshot updates', () => {
    const roundEventMessage = {
      type: 'round_event' as const,
      payload: {
        event_type: 'claim_made',
        event: {
          seat: 0,
          claim_type: 'pung',
          tile_id: 't5#discard',
        },
      },
    };

    const next = sessionReducer(
      {
        ...createInitialSessionState(),
        latestRoundEvent: roundEventMessage,
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

    expect(next.latestRoundEvent).toEqual(roundEventMessage);
  });

  it('keeps a hu round_event active when settlement_ready arrives immediately after it', () => {
    const huRoundEventMessage = {
      type: 'round_event' as const,
      payload: {
        event_type: 'self_hu_declared',
        event: {
          seat: 0,
          tile_id: 't5#discard',
        },
      },
    };

    const next = sessionReducer(
      {
        ...createInitialSessionState(),
        latestRoundEvent: huRoundEventMessage,
      },
      {
        type: 'ws_message',
        message: {
          type: 'round_event',
          payload: {
            event_type: 'settlement_ready',
            event: {
              round_id: 'round-1',
            },
          },
        },
      },
    );

    expect(next.latestRoundEvent).toEqual(huRoundEventMessage);
    expect(next.toasts.at(-1)).toMatchObject({
      kind: 'event',
      text: '本局已进入结算',
    });
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

  it('localizes tile_discarded round events into win10 toast copy', () => {
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

  it('clears selected tiles that become restricted for same-turn discard', () => {
    const next = sessionReducer(
      {
        ...createInitialSessionState(),
        selectedTileIds: ['w1#0'],
        selectionMode: 'single',
      },
      {
        type: 'ws_message',
        message: {
          type: 'room_snapshot',
          payload: {
            table_code: 'AB12CD',
            phase: 'playing',
            seats: [{ seat_index: 0, nickname: 'Player A', connected: true, ready: true }],
            local_seat: 0,
            reconnect_token: 'token-1',
            private_state: {
              round_id: 'round-1',
              round_wind: 'east',
              dealer_seat: 0,
              current_actor: 0,
              last_discard: null,
              pending_action: {
                type: 'active_turn',
                seat_index: 0,
                deadline_at: '2026-03-26T06:01:00Z',
                restricted_discard_tile_ids: ['w1#0'],
                options: ['discard'],
              },
              players: [
                {
                  seat_index: 0,
                  nickname: 'Player A',
                  connected: true,
                  concealed_count: 2,
                  concealed_tiles: [
                    { tile_id: 'w1#0', tile_key: 'w1' },
                    { tile_id: 'w2#0', tile_key: 'w2' },
                  ],
                  melds: [],
                  flowers: [],
                  discards: [],
                },
              ],
            },
          },
        },
      },
    );

    expect(next.selectedTileIds).toEqual([]);
    expect(next.selectionMode).toBeNull();
  });

  it('accumulates score trends and win counts from match_result messages', () => {
    const afterSnapshot = sessionReducer(createInitialSessionState(), {
      type: 'ws_message',
      message: playingRoomSnapshotMessage,
    });
    const afterResult = sessionReducer(afterSnapshot, {
      type: 'ws_message',
      message: matchResultMessage,
    });

    expect(afterResult.matchStatistics).toEqual({
      completedRoundCount: 1,
      lastAppliedRoundId: 'round-1',
      seatStatsBySeat: {
        '0': { scoreHistory: [0, -8], winCount: 0 },
        '1': { scoreHistory: [0, 8], winCount: 1 },
        '2': { scoreHistory: [0, 0], winCount: 0 },
        '3': { scoreHistory: [0, 0], winCount: 0 },
      },
    });

    const deduped = sessionReducer(afterResult, {
      type: 'ws_message',
      message: matchResultMessage,
    });

    expect(deduped.matchStatistics).toEqual(afterResult.matchStatistics);
  });
});
