import { describe, expect, it } from 'vitest';

import type { SessionState } from '../types/match';
import {
  getActionCandidateGroups,
  getActionCandidateTileIds,
  getAutoPassKongCandidateTileKeys,
  getFlowerCandidateTileIds,
  getKongCandidateGroups,
  getKongCandidateTileIds,
  getLocalTurnKongCandidateGroups,
  getLocalTurnKongPromptSignature,
  getMatchingActionGroup,
  getMatchingKongGroup,
} from './kongSelection';

function createSessionState(overrides: Partial<SessionState> = {}): SessionState {
  return {
    apiBaseUrl: 'http://localhost:8080',
    wsBaseUrl: 'ws://localhost:8080',
    tableCode: 'AB12CD',
    nickname: 'Player A',
    connectionStatus: 'connected',
    roomSnapshot: {
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
        private_state: {
          round_id: 'round-1',
          round_wind: 'east',
          dealer_seat: 0,
          current_actor: 0,
          last_discard: 'b4',
          pending_action: {
            type: 'active_turn',
            seat_index: 0,
            deadline_at: '2026-03-26T06:01:00Z',
            drawn_tile_id: 'w3#3',
            options: ['discard', 'kong'],
          },
          players: [
            {
              seat_index: 0,
              nickname: 'Player A',
              connected: true,
              concealed_count: 8,
              concealed_tiles: [
                { tile_id: 'w3#0', tile_key: 'w3' },
                { tile_id: 'w3#1', tile_key: 'w3' },
                { tile_id: 'w3#2', tile_key: 'w3' },
                { tile_id: 'w3#3', tile_key: 'w3' },
                { tile_id: 'east#0', tile_key: 'east' },
              ],
              melds: [['east', 'east', 'east']],
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
      },
    },
    latestActionPrompt: {
      type: 'action_prompt',
      payload: {
        seat_index: 0,
        options: ['discard', 'kong'],
        deadline_at: '2026-03-26T06:01:00Z',
      },
    },
    latestMatchResult: null,
    latestRoundEvent: null,
    lastRejectedAction: null,
    selectedTileIds: [],
    selectionMode: null,
    toasts: [],
    ...overrides,
  };
}

describe('kongSelection', () => {
  it('finds concealed and add-kong candidates from the local private hand', () => {
    const groups = getKongCandidateGroups(createSessionState());

    expect(groups).toEqual([
      ['w3#0', 'w3#1', 'w3#2', 'w3#3'],
      ['east#0'],
    ]);
  });

  it('finds local-turn kong candidates even when the backend prompt only exposes discard', () => {
    const state = createSessionState({
      roomSnapshot: {
        type: 'room_snapshot',
        payload: {
          ...createSessionState().roomSnapshot!.payload,
          private_state: {
            ...createSessionState().roomSnapshot!.payload.private_state!,
            pending_action: {
              type: 'active_turn',
              seat_index: 0,
              deadline_at: '2026-03-26T06:01:00Z',
              drawn_tile_id: 'w3#3',
              options: ['discard'],
            },
          },
        },
      },
      latestActionPrompt: {
        type: 'action_prompt',
        payload: {
          seat_index: 0,
          options: ['discard'],
          deadline_at: '2026-03-26T06:01:00Z',
        },
      },
    });

    expect(getLocalTurnKongCandidateGroups(state)).toEqual([
      ['w3#0', 'w3#1', 'w3#2', 'w3#3'],
      ['east#0'],
    ]);
    expect(getLocalTurnKongPromptSignature(state)).toContain('turn-kong:round-1:0:2026-03-26T06:01:00Z:w3#3');
  });

  it('detects auto-pass kong possibilities from three local concealed tiles and no known outside tile', () => {
    const state = createSessionState({
      roomSnapshot: {
        type: 'room_snapshot',
        payload: {
          ...createSessionState().roomSnapshot!.payload,
          private_state: {
            ...createSessionState().roomSnapshot!.payload.private_state!,
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 6,
                concealed_tiles: [
                  { tile_id: 'w5#0', tile_key: 'w5' },
                  { tile_id: 'w5#1', tile_key: 'w5' },
                  { tile_id: 'w5#2', tile_key: 'w5' },
                  { tile_id: 'b1#0', tile_key: 'b1' },
                ],
                melds: [],
                flowers: [],
                discards: [],
              },
              ...createSessionState().roomSnapshot!.payload.private_state!.players.slice(1),
            ],
          },
        },
      },
    });

    expect(getAutoPassKongCandidateTileKeys(state)).toEqual(['w5']);
  });

  it('hides auto-pass kong possibilities once the matching tile is known outside', () => {
    const state = createSessionState({
      roomSnapshot: {
        type: 'room_snapshot',
        payload: {
          ...createSessionState().roomSnapshot!.payload,
          private_state: {
            ...createSessionState().roomSnapshot!.payload.private_state!,
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 6,
                concealed_tiles: [
                  { tile_id: 'w5#0', tile_key: 'w5' },
                  { tile_id: 'w5#1', tile_key: 'w5' },
                  { tile_id: 'w5#2', tile_key: 'w5' },
                ],
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                ...createSessionState().roomSnapshot!.payload.private_state!.players[1],
                discards: ['w5'],
              },
              ...createSessionState().roomSnapshot!.payload.private_state!.players.slice(2),
            ],
          },
        },
      },
    });

    expect(getAutoPassKongCandidateTileKeys(state)).toEqual([]);
  });

  it('detects auto-pass kong possibilities from a local pung meld and one matching concealed tile', () => {
    const state = createSessionState({
      roomSnapshot: {
        type: 'room_snapshot',
        payload: {
          ...createSessionState().roomSnapshot!.payload,
          private_state: {
            ...createSessionState().roomSnapshot!.payload.private_state!,
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 5,
                concealed_tiles: [
                  { tile_id: 'b7#0', tile_key: 'b7' },
                  { tile_id: 'w1#0', tile_key: 'w1' },
                ],
                melds: [['b7', 'b7', 'b7']],
                flowers: [],
                discards: [],
              },
              ...createSessionState().roomSnapshot!.payload.private_state!.players.slice(1),
            ],
          },
        },
      },
    });

    expect(getAutoPassKongCandidateTileKeys(state)).toEqual(['b7']);
  });

  it('flattens all candidate tile ids for first-click preselection', () => {
    const candidateTileIds = getKongCandidateTileIds(createSessionState());

    expect(candidateTileIds).toEqual(['w3#0', 'w3#1', 'w3#2', 'w3#3', 'east#0']);
  });

  it('matches only exact candidate groups on the confirming kong click', () => {
    const groups = getKongCandidateGroups(createSessionState());

    expect(getMatchingKongGroup(['w3#0', 'w3#1', 'w3#2', 'w3#3'], groups)).toEqual([
      'w3#0',
      'w3#1',
      'w3#2',
      'w3#3',
    ]);
    expect(getMatchingKongGroup(['w3#0', 'east#0'], groups)).toBeNull();
  });

  it('lists local concealed flower tiles as flower candidates', () => {
    const state = createSessionState({
      roomSnapshot: {
        type: 'room_snapshot',
        payload: {
          ...createSessionState().roomSnapshot!.payload,
          private_state: {
            ...createSessionState().roomSnapshot!.payload.private_state!,
            pending_action: {
              type: 'active_turn',
              seat_index: 0,
              deadline_at: '2026-03-26T06:01:00Z',
              drawn_tile_id: 'f1#0',
              options: ['discard', 'flower'],
            },
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 2,
                concealed_tiles: [
                  { tile_id: 'f1#0', tile_key: 'f1' },
                  { tile_id: 'w2#0', tile_key: 'w2' },
                ],
                melds: [],
                flowers: [],
                discards: [],
              },
              ...createSessionState().roomSnapshot!.payload.private_state!.players.slice(1),
            ],
          },
        },
      },
      latestActionPrompt: {
        type: 'action_prompt',
        payload: {
          seat_index: 0,
          options: ['discard', 'flower'],
          deadline_at: '2026-03-26T06:01:00Z',
        },
      },
    });

    expect(getFlowerCandidateTileIds(state)).toEqual(['f1#0']);
  });

  it('ignores an optimistic flower tile once it has been queued for exposure', () => {
    const state = createSessionState({
      roomSnapshot: {
        type: 'room_snapshot',
        payload: {
          ...createSessionState().roomSnapshot!.payload,
          private_state: {
            ...createSessionState().roomSnapshot!.payload.private_state!,
            pending_action: {
              type: 'active_turn',
              seat_index: 0,
              deadline_at: '2026-03-26T06:01:00Z',
              drawn_tile_id: 'f1#0',
              options: ['discard', 'flower'],
            },
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 2,
                concealed_tiles: [
                  { tile_id: 'f1#0', tile_key: 'f1' },
                  { tile_id: 'w2#0', tile_key: 'w2' },
                ],
                melds: [],
                flowers: [],
                discards: [],
              },
              ...createSessionState().roomSnapshot!.payload.private_state!.players.slice(1),
            ],
          },
        },
      },
      optimisticFlower: {
        tileId: 'f1#0',
        requestedAt: '2026-03-26T06:01:00Z',
      },
    });

    expect(getFlowerCandidateTileIds(state)).toEqual([]);
  });

  it('derives claim-kong candidates from the latest discard and three matching concealed tiles', () => {
    const groups = getKongCandidateGroups(
      createSessionState({
        roomSnapshot: {
          type: 'room_snapshot',
          payload: {
            ...createSessionState().roomSnapshot!.payload,
            private_state: {
              ...createSessionState().roomSnapshot!.payload.private_state!,
              pending_action: {
                type: 'claim_window',
                discarder_seat: 2,
                deadline_at: '2026-03-26T06:01:00Z',
                responded_seats: [],
                options: ['kong', 'pass'],
              },
              last_discard: 'b4',
              players: [
                {
                  seat_index: 0,
                  nickname: 'Player A',
                  connected: true,
                  concealed_count: 5,
                  concealed_tiles: [
                    { tile_id: 'b4#0', tile_key: 'b4' },
                    { tile_id: 'b4#1', tile_key: 'b4' },
                    { tile_id: 'b4#2', tile_key: 'b4' },
                    { tile_id: 'c1#0', tile_key: 'c1' },
                  ],
                  melds: [],
                  flowers: [],
                  discards: [],
                },
                ...createSessionState().roomSnapshot!.payload.private_state!.players.slice(1),
              ],
            },
          },
        },
        latestActionPrompt: {
          type: 'action_prompt',
          payload: {
            seat_index: 0,
            options: ['kong', 'pass'],
            deadline_at: '2026-03-26T06:01:00Z',
          },
        },
      }),
    );

    expect(groups).toEqual([['b4#0', 'b4#1', 'b4#2']]);
  });

  it('derives pung candidates as every valid pair matching the latest discard', () => {
    const groups = getActionCandidateGroups(
      createSessionState({
        roomSnapshot: {
          type: 'room_snapshot',
          payload: {
            ...createSessionState().roomSnapshot!.payload,
            private_state: {
              ...createSessionState().roomSnapshot!.payload.private_state!,
              pending_action: {
                type: 'claim_window',
                discarder_seat: 2,
                deadline_at: '2026-03-26T06:01:00Z',
                responded_seats: [],
                options: ['pung', 'pass'],
              },
              last_discard: 'east',
              players: [
                {
                  seat_index: 0,
                  nickname: 'Player A',
                  connected: true,
                  concealed_count: 5,
                  concealed_tiles: [
                    { tile_id: 'east#0', tile_key: 'east' },
                    { tile_id: 'east#1', tile_key: 'east' },
                    { tile_id: 'east#2', tile_key: 'east' },
                  ],
                  melds: [],
                  flowers: [],
                  discards: [],
                },
                ...createSessionState().roomSnapshot!.payload.private_state!.players.slice(1),
              ],
            },
          },
        },
        latestActionPrompt: {
          type: 'action_prompt',
          payload: {
            seat_index: 0,
            options: ['pung', 'pass'],
            deadline_at: '2026-03-26T06:01:00Z',
          },
        },
      }),
      'pung',
    );

    expect(groups).toEqual([
      ['east#0', 'east#1'],
      ['east#0', 'east#2'],
      ['east#1', 'east#2'],
    ]);
  });

  it('derives chow candidates as every valid sequence pair around the latest discard', () => {
    const state = createSessionState({
      roomSnapshot: {
        type: 'room_snapshot',
        payload: {
          ...createSessionState().roomSnapshot!.payload,
          private_state: {
            ...createSessionState().roomSnapshot!.payload.private_state!,
            pending_action: {
              type: 'claim_window',
              discarder_seat: 2,
              deadline_at: '2026-03-26T06:01:00Z',
              responded_seats: [],
              options: ['chow', 'pass'],
            },
            last_discard: 'b4',
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 6,
                concealed_tiles: [
                  { tile_id: 'b2#0', tile_key: 'b2' },
                  { tile_id: 'b3#0', tile_key: 'b3' },
                  { tile_id: 'b3#1', tile_key: 'b3' },
                  { tile_id: 'b5#0', tile_key: 'b5' },
                  { tile_id: 'b6#0', tile_key: 'b6' },
                ],
                melds: [],
                flowers: [],
                discards: [],
              },
              ...createSessionState().roomSnapshot!.payload.private_state!.players.slice(1),
            ],
          },
        },
      },
      latestActionPrompt: {
        type: 'action_prompt',
        payload: {
          seat_index: 0,
          options: ['chow', 'pass'],
          deadline_at: '2026-03-26T06:01:00Z',
        },
      },
    });

    expect(getActionCandidateGroups(state, 'chow')).toEqual([
      ['b2#0', 'b3#0'],
      ['b2#0', 'b3#1'],
      ['b3#0', 'b5#0'],
      ['b3#1', 'b5#0'],
      ['b5#0', 'b6#0'],
    ]);
    expect(getActionCandidateTileIds(state, 'chow')).toEqual(['b2#0', 'b3#0', 'b3#1', 'b5#0', 'b6#0']);
    expect(getMatchingActionGroup(['b3#1', 'b5#0'], getActionCandidateGroups(state, 'chow'))).toEqual([
      'b3#1',
      'b5#0',
    ]);
  });

  it('derives chow candidates for bamboo tile keys from the backend', () => {
    const state = createSessionState({
      roomSnapshot: {
        type: 'room_snapshot',
        payload: {
          ...createSessionState().roomSnapshot!.payload,
          private_state: {
            ...createSessionState().roomSnapshot!.payload.private_state!,
            pending_action: {
              type: 'claim_window',
              discarder_seat: 2,
              deadline_at: '2026-03-26T06:01:00Z',
              responded_seats: [],
              options: ['chow', 'pass'],
            },
            last_discard: 't4',
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 6,
                concealed_tiles: [
                  { tile_id: 't2#0', tile_key: 't2' },
                  { tile_id: 't3#0', tile_key: 't3' },
                  { tile_id: 't3#1', tile_key: 't3' },
                  { tile_id: 't5#0', tile_key: 't5' },
                  { tile_id: 't6#0', tile_key: 't6' },
                ],
                melds: [],
                flowers: [],
                discards: [],
              },
              ...createSessionState().roomSnapshot!.payload.private_state!.players.slice(1),
            ],
          },
        },
      },
      latestActionPrompt: {
        type: 'action_prompt',
        payload: {
          seat_index: 0,
          options: ['chow', 'pass'],
          deadline_at: '2026-03-26T06:01:00Z',
        },
      },
    });

    expect(getActionCandidateGroups(state, 'chow')).toEqual([
      ['t2#0', 't3#0'],
      ['t2#0', 't3#1'],
      ['t3#0', 't5#0'],
      ['t3#1', 't5#0'],
      ['t5#0', 't6#0'],
    ]);
  });
});
