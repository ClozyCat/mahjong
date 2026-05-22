import { describe, expect, it } from 'vitest';

import type { ActionPromptMessage, MatchResultMessage, PlayerPresenceMessage, RoomSnapshotMessage } from '../types/match';
import { createInitialSessionState, sessionReducer } from './sessionReducer';

const roomSnapshotMessage: RoomSnapshotMessage = {
  type: 'room_snapshot',
  payload: {
    table_code: 'AB12CD',
    phase: 'waiting',
    seats: [{ seat_index: 0, nickname: 'Player A', connected: true }],
    local_seat: 0,
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
      { seat_index: 0, nickname: 'Player A', connected: true },
      { seat_index: 1, nickname: 'Player B', connected: true },
      { seat_index: 2, nickname: 'Player C', connected: true },
      { seat_index: 3, nickname: 'Player D', connected: true },
    ],
    local_seat: 0,
    match_state: {
      prevailing_wind: 'east',
      hand_number: 1,
      dealer_seat: 0,
      cumulative_scores: { '0': 0, '1': 0, '2': 0, '3': 0 },
      match_finished: false,
      last_completed_round_id: null,
      statistics: {
        completed_round_count: 1,
        seat_stats_by_seat: {
          '0': { score_history: [0, -8], win_count: 0, deal_in_count: 1 },
          '1': { score_history: [0, 8], win_count: 1, deal_in_count: 0 },
          '2': { score_history: [0, 0], win_count: 0, deal_in_count: 0 },
          '3': { score_history: [0, 0], win_count: 0, deal_in_count: 0 },
        },
      },
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

function createPlayingSnapshot(roundId: string): RoomSnapshotMessage {
  return {
    ...playingRoomSnapshotMessage,
    payload: {
      ...playingRoomSnapshotMessage.payload,
      private_state: {
        round_id: roundId,
        round_wind: 'east',
        dealer_seat: 0,
        current_actor: 0,
        last_discard: null,
        pending_action: {
          type: 'active_turn',
          seat_index: 0,
          deadline_at: '2026-03-26T06:01:00Z',
          drawn_tile_id: 'w1#0',
          options: ['discard'],
        },
        players: [
          {
            seat_index: 0,
            nickname: 'Player A',
            connected: true,
            concealed_count: 14,
            concealed_tiles: [{ tile_id: 'w1#0', tile_key: 'w1' }],
            melds: [],
            flowers: [],
            discards: [],
          },
          {
            seat_index: 1,
            nickname: 'Player B',
            connected: true,
            concealed_count: 13,
            concealed_tiles: [],
            melds: [],
            flowers: [],
            discards: [],
          },
        ],
      },
    },
  };
}

describe('sessionReducer', () => {
  it('stores room snapshot and clears stale rejection errors', () => {
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

  it('clears stale round events when a fresh room snapshot starts a new round', () => {
    const previousRoundEvent = {
      type: 'round_event' as const,
      payload: {
        event_type: 'tile_discarded',
        event: {
          seat: 0,
          tile_id: 't5#discard',
        },
      },
    };
    const currentRoundSnapshot = createPlayingSnapshot('round-1');
    const nextRoundSnapshot = createPlayingSnapshot('round-2');

    const sameRound = sessionReducer(
      {
        ...createInitialSessionState(),
        roomSnapshot: currentRoundSnapshot,
        latestRoundEvent: previousRoundEvent,
        recentRoundEvents: [previousRoundEvent],
      },
      {
        type: 'ws_message',
        message: currentRoundSnapshot,
      },
    );

    expect(sameRound.latestRoundEvent).toEqual(previousRoundEvent);
    expect(sameRound.recentRoundEvents).toEqual([previousRoundEvent]);

    const nextRound = sessionReducer(sameRound, {
      type: 'ws_message',
      message: nextRoundSnapshot,
    });

    expect(nextRound.latestRoundEvent).toBeNull();
    expect(nextRound.recentRoundEvents).toEqual([]);
  });

  it('retains recent round_events so transient cues are not overwritten before rendering', () => {
    const discardEvent = {
      type: 'round_event' as const,
      payload: {
        event_type: 'tile_discarded',
        event: {
          seat: 1,
          tile_id: 'b7#bot-8',
        },
      },
    };
    const settlementEvent = {
      type: 'round_event' as const,
      payload: {
        event_type: 'settlement_ready',
        event: {
          round_id: 'round-1',
        },
      },
    };

    const afterDiscard = sessionReducer(createInitialSessionState(), {
      type: 'ws_message',
      message: discardEvent,
    });
    const afterSettlement = sessionReducer(afterDiscard, {
      type: 'ws_message',
      message: settlementEvent,
    });

    expect(afterSettlement.latestRoundEvent).toEqual(settlementEvent);
    expect(afterSettlement.recentRoundEvents).toEqual([discardEvent, settlementEvent]);
  });

  it('does not synthesize local display meld state from round events anymore', () => {
    const afterClaim = sessionReducer(createInitialSessionState(), {
      type: 'ws_message',
      message: {
        type: 'round_event',
        payload: {
          event_type: 'claim_made',
          event: {
            seat: 1,
            from: 0,
            claim_type: 'pung',
            tile_key: 'w3',
            meld: ['w3', 'w3', 'w3'],
          },
        },
      },
    });

    expect((afterClaim as { displayMeldsBySeat?: unknown }).displayMeldsBySeat).toBeUndefined();

    const afterSnapshot = sessionReducer(afterClaim, {
      type: 'ws_message',
      message: {
        type: 'room_snapshot',
        payload: {
          table_code: 'AB12CD',
          phase: 'playing',
          seats: [
            { seat_index: 0, nickname: 'Player A', connected: true },
            { seat_index: 1, nickname: 'Player B', connected: true },
          ],
          local_seat: 0,
          private_state: {
            round_id: 'round-1',
            round_wind: 'east',
            dealer_seat: 0,
            current_actor: 1,
            last_discard: null,
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 10,
                concealed_tiles: [],
                melds: [],
                flowers: [],
                discards: ['w3'],
              },
              {
                seat_index: 1,
                nickname: 'Player B',
                connected: true,
                concealed_count: 10,
                concealed_tiles: [],
                melds: [['w3', 'w3', 'w3']],
                display_melds: [
                  {
                    tiles: [
                      { code: 'w3', orientation: 'rotated' },
                      { code: 'w3', orientation: 'normal' },
                      { code: 'w3', orientation: 'normal' },
                    ],
                  },
                ],
                flowers: [],
                discards: [],
              } as {
                seat_index: number;
                nickname: string;
                connected: boolean;
                concealed_count: number;
                concealed_tiles: [];
                melds: string[][];
                display_melds: Array<{
                  tiles: Array<{ code: string; orientation: 'normal' | 'rotated' | 'face_down' }>;
                }>;
                flowers: string[];
                discards: string[];
              },
            ],
          },
        },
      },
    });

    expect(afterSnapshot.roomSnapshot?.payload.private_state?.players[1]).toMatchObject({
      display_melds: [
        {
          tiles: [
            { code: 'w3', orientation: 'rotated' },
            { code: 'w3', orientation: 'normal' },
            { code: 'w3', orientation: 'normal' },
          ],
        },
      ],
    });
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

  it('localizes tile_discarded round events into themed toast copy', () => {
    const next = sessionReducer(
      {
        ...createInitialSessionState(),
        roomSnapshot: {
          ...roomSnapshotMessage,
          payload: {
            ...roomSnapshotMessage.payload,
            seats: [{ seat_index: 0, nickname: '小李', connected: true }],
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
            seats: [{ seat_index: 0, nickname: 'Player A', connected: true }],
            local_seat: 0,
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

  it('stores an optimistic discard until a fresh room_snapshot removes the tile from the local hand', () => {
    const queued = sessionReducer(
      {
        ...createInitialSessionState(),
        roomSnapshot: {
          type: 'room_snapshot',
          payload: {
            table_code: 'AB12CD',
            phase: 'playing',
            seats: [{ seat_index: 0, nickname: 'Player A', connected: true }],
            local_seat: 0,
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
                drawn_tile_id: 'w2#0',
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
      {
        type: 'queue_optimistic_discard',
        tileId: 'w2#0',
        actionType: 'discard',
      },
    );

    expect(queued.optimisticDiscard).toMatchObject({
      tileId: 'w2#0',
      tileCode: 'w2',
      seatIndex: 0,
      actionType: 'discard',
    });

    const confirmed = sessionReducer(queued, {
      type: 'ws_message',
      message: {
        type: 'room_snapshot',
        payload: {
          table_code: 'AB12CD',
          phase: 'playing',
          seats: [{ seat_index: 0, nickname: 'Player A', connected: true }],
          local_seat: 0,
          private_state: {
            round_id: 'round-1',
            round_wind: 'east',
            dealer_seat: 0,
            current_actor: 1,
            last_discard: 'w2',
            pending_action: {
              type: 'claim_window',
              discarder_seat: 0,
              deadline_at: '2026-03-26T06:01:05Z',
              responded_seats: [],
              options: [],
            },
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 1,
                concealed_tiles: [{ tile_id: 'w1#0', tile_key: 'w1' }],
                melds: [],
                flowers: [],
                discards: ['w2'],
              },
            ],
          },
        },
      },
    });

    expect(confirmed.optimisticDiscard).toBeNull();
  });

  it('clears the local replacement draw marker after the turn passes to another seat', () => {
    const next = sessionReducer(
      {
        ...createInitialSessionState(),
        latestReplacementTileId: 'w9#0',
      },
      {
        type: 'ws_message',
        message: {
          type: 'room_snapshot',
          payload: {
            table_code: 'AB12CD',
            phase: 'playing',
            seats: [
              { seat_index: 0, nickname: 'Player A', connected: true },
              { seat_index: 1, nickname: 'Player B', connected: true },
            ],
            local_seat: 0,
            private_state: {
              round_id: 'round-1',
              round_wind: 'east',
              dealer_seat: 0,
              current_actor: 1,
              last_discard: 'w1',
              pending_action: {
                type: 'active_turn',
                seat_index: 1,
                deadline_at: '2026-03-26T06:01:05Z',
                drawn_tile_id: 'b3#1',
                options: ['discard'],
              },
              players: [
                {
                  seat_index: 0,
                  nickname: 'Player A',
                  connected: true,
                  concealed_count: 13,
                  concealed_tiles: [
                    { tile_id: 'w2#0', tile_key: 'w2' },
                    { tile_id: 'w9#0', tile_key: 'w9' },
                  ],
                  melds: [],
                  flowers: [],
                  discards: ['w1'],
                },
                {
                  seat_index: 1,
                  nickname: 'Player B',
                  connected: true,
                  concealed_count: 14,
                  concealed_tiles: [{ tile_id: 'b3#1', tile_key: 'b3' }],
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

    expect(next.latestReplacementTileId).toBeNull();
  });

  it('rolls back an optimistic discard when the server rejects the action', () => {
    const queued = sessionReducer(
      {
        ...createInitialSessionState(),
        roomSnapshot: {
          type: 'room_snapshot',
          payload: {
            table_code: 'AB12CD',
            phase: 'playing',
            seats: [{ seat_index: 0, nickname: 'Player A', connected: true }],
            local_seat: 0,
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
                options: ['discard'],
              },
              players: [
                {
                  seat_index: 0,
                  nickname: 'Player A',
                  connected: true,
                  concealed_count: 1,
                  concealed_tiles: [{ tile_id: 'w1#0', tile_key: 'w1' }],
                  melds: [],
                  flowers: [],
                  discards: [],
                },
              ],
            },
          },
        },
      },
      {
        type: 'queue_optimistic_discard',
        tileId: 'w1#0',
        actionType: 'discard',
      },
    );

    const rolledBack = sessionReducer(queued, {
      type: 'ws_message',
      message: {
        type: 'action_rejected',
        payload: {
          reason: 'invalid_action',
        },
      },
    });

    expect(rolledBack.optimisticDiscard).toBeNull();
    expect(rolledBack.lastRejectedAction?.payload.reason).toBe('invalid_action');
  });

  it('keeps optimistic ready_hand until the ready_hand_declared round event arrives', () => {
    const queued = sessionReducer(
      createInitialSessionState(),
      {
        type: 'ws_message',
        message: {
          type: 'room_snapshot',
          payload: {
            table_code: 'AB12CD',
            phase: 'playing',
            seats: [{ seat_index: 0, nickname: 'Player A', connected: true }],
            local_seat: 0,
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
                drawn_tile_id: 'w2#0',
                options: ['discard', 'ready_hand'],
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

    const optimistic = sessionReducer(queued, {
      type: 'queue_optimistic_discard',
      tileId: 'w2#0',
      actionType: 'ready_hand',
    });

    const confirmedBySnapshot = sessionReducer(optimistic, {
      type: 'ws_message',
      message: {
        type: 'room_snapshot',
        payload: {
          table_code: 'AB12CD',
          phase: 'playing',
          seats: [{ seat_index: 0, nickname: 'Player A', connected: true }],
          local_seat: 0,
          private_state: {
            round_id: 'round-1',
            round_wind: 'east',
            dealer_seat: 0,
            current_actor: 1,
            last_discard: 'w2',
            pending_action: {
              type: 'active_turn',
              seat_index: 1,
              deadline_at: '2026-03-26T06:01:05Z',
              drawn_tile_id: 'b3#1',
              options: ['discard'],
            },
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 1,
                concealed_tiles: [{ tile_id: 'w1#0', tile_key: 'w1' }],
                melds: [],
                flowers: [],
                discards: ['w2'],
                is_ready_hand: true,
              },
              {
                seat_index: 1,
                nickname: 'Player B',
                connected: true,
                concealed_count: 13,
                concealed_tiles: [],
                melds: [],
                flowers: [],
                discards: [],
              },
            ],
          },
        },
      },
    });

    expect(confirmedBySnapshot.optimisticDiscard).toMatchObject({
      tileId: 'w2#0',
      actionType: 'ready_hand',
    });

    const confirmedByEvent = sessionReducer(confirmedBySnapshot, {
      type: 'ws_message',
      message: {
        type: 'round_event',
        payload: {
          event_type: 'ready_hand_declared',
          event: {
            seat: 0,
            tile_id: 'w2#discard',
          },
        },
      },
    });

    expect(confirmedByEvent.optimisticDiscard).toBeNull();
  });

  it('keeps optimistic ready_hand when ready_hand_declared arrives before the confirming snapshot', () => {
    const queued = sessionReducer(
      createInitialSessionState(),
      {
        type: 'ws_message',
        message: {
          type: 'room_snapshot',
          payload: {
            table_code: 'AB12CD',
            phase: 'playing',
            seats: [{ seat_index: 0, nickname: 'Player A', connected: true }],
            local_seat: 0,
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
                drawn_tile_id: 'w2#0',
                options: ['discard', 'ready_hand'],
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

    const optimistic = sessionReducer(queued, {
      type: 'queue_optimistic_discard',
      tileId: 'w2#0',
      actionType: 'ready_hand',
    });

    const confirmedByEvent = sessionReducer(optimistic, {
      type: 'ws_message',
      message: {
        type: 'round_event',
        payload: {
          event_type: 'ready_hand_declared',
          event: {
            seat: 0,
            tile_id: 'w2#discard',
          },
        },
      },
    });

    expect(confirmedByEvent.optimisticDiscard).toMatchObject({
      tileId: 'w2#0',
      actionType: 'ready_hand',
    });

    const confirmedBySnapshot = sessionReducer(confirmedByEvent, {
      type: 'ws_message',
      message: {
        type: 'room_snapshot',
        payload: {
          table_code: 'AB12CD',
          phase: 'playing',
          seats: [{ seat_index: 0, nickname: 'Player A', connected: true }],
          local_seat: 0,
          private_state: {
            round_id: 'round-1',
            round_wind: 'east',
            dealer_seat: 0,
            current_actor: 1,
            last_discard: 'w2',
            pending_action: {
              type: 'active_turn',
              seat_index: 1,
              deadline_at: '2026-03-26T06:01:05Z',
              drawn_tile_id: 'b3#1',
              options: ['discard'],
            },
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 1,
                concealed_tiles: [{ tile_id: 'w1#0', tile_key: 'w1' }],
                melds: [],
                flowers: [],
                discards: ['w2'],
                is_ready_hand: true,
              },
              {
                seat_index: 1,
                nickname: 'Player B',
                connected: true,
                concealed_count: 13,
                concealed_tiles: [],
                melds: [],
                flowers: [],
                discards: [],
              },
            ],
          },
        },
      },
    });

    expect(confirmedBySnapshot.optimisticDiscard).toBeNull();
  });

  it('hydrates match statistics from room_snapshot and keeps match_result read-only for chart data', () => {
    const afterSnapshot = sessionReducer(createInitialSessionState(), {
      type: 'ws_message',
      message: playingRoomSnapshotMessage,
    });
    expect(afterSnapshot.matchStatistics).toEqual({
      completedRoundCount: 1,
      lastAppliedRoundId: null,
      seatStatsBySeat: {
        '0': { scoreHistory: [0, -8], winCount: 0, dealInCount: 1 },
        '1': { scoreHistory: [0, 8], winCount: 1, dealInCount: 0 },
        '2': { scoreHistory: [0, 0], winCount: 0, dealInCount: 0 },
        '3': { scoreHistory: [0, 0], winCount: 0, dealInCount: 0 },
      },
    });

    const afterResult = sessionReducer(afterSnapshot, {
      type: 'ws_message',
      message: matchResultMessage,
    });

    expect(afterResult.matchStatistics).toEqual(afterSnapshot.matchStatistics);
  });

  it('keeps the latest match result when the room snapshot reaches finished', () => {
    const afterSnapshot = sessionReducer(createInitialSessionState(), {
      type: 'ws_message',
      message: playingRoomSnapshotMessage,
    });
    const afterResult = sessionReducer(afterSnapshot, {
      type: 'ws_message',
      message: matchResultMessage,
    });
    const finishedSnapshot = {
      ...playingRoomSnapshotMessage,
      payload: {
        ...playingRoomSnapshotMessage.payload,
        phase: 'finished' as const,
        match_state: {
          ...playingRoomSnapshotMessage.payload.match_state!,
          match_finished: true,
        },
      },
    };

    const afterFinished = sessionReducer(afterResult, {
      type: 'ws_message',
      message: finishedSnapshot,
    });

    expect(afterFinished.latestMatchResult).toMatchObject(matchResultMessage);
    expect(afterFinished.latestMatchResult?.payload.settlement_seats).toEqual(
      playingRoomSnapshotMessage.payload.seats,
    );
  });

});
