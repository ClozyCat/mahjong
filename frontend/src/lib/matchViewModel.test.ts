import { describe, expect, it } from 'vitest';

import type { SessionState } from '../types/match';
import { createMatchViewModel } from './matchViewModel';

function createWaitingSessionState(): SessionState {
  return {
    connectionStatus: 'connected',
    tableCode: 'AB12CD',
    nickname: 'Player A',
    selectedTileIds: [],
    selectionMode: null,
    roomSnapshot: {
      type: 'room_snapshot',
      payload: {
        table_code: 'AB12CD',
        phase: 'waiting',
        seats: [
          { seat_index: 0, nickname: 'Player A', connected: true, ready: false },
          { seat_index: 1, nickname: 'Player B', connected: true, ready: true },
        ],
        local_seat: 0,
        reconnect_token: 'token-1',
      },
    },
    latestActionPrompt: null,
    latestMatchResult: null,
    latestRoundEvent: null,
    lastRejectedAction: null,
    reconnectToken: 'token-1',
    toasts: [],
  };
}

function createPlayingSessionState(overrides: Partial<SessionState> = {}): SessionState {
  return {
    connectionStatus: 'connected',
    tableCode: 'AB12CD',
    nickname: 'Player C',
    selectedTileIds: [],
    selectionMode: null,
    roomSnapshot: {
      type: 'room_snapshot',
      payload: {
        table_code: 'AB12CD',
        phase: 'playing',
        seats: [
          { seat_index: 0, nickname: 'Player A', connected: true, ready: true, is_bot: false },
          { seat_index: 1, nickname: 'Player B', connected: true, ready: true, is_bot: false },
          { seat_index: 2, nickname: 'Player C', connected: true, ready: true, is_bot: false },
          { seat_index: 3, nickname: 'Player D', connected: false, ready: true, is_bot: false },
        ],
        local_seat: 2,
        reconnect_token: 'token-2',
        match_state: {
          prevailing_wind: 'east',
          hand_number: 1,
          dealer_seat: 1,
          cumulative_scores: { '0': 0, '1': 0, '2': 0, '3': 0 },
          match_finished: false,
          last_completed_round_id: null,
        },
        private_state: {
          round_id: 'round-123',
          round_wind: 'east',
          dealer_seat: 1,
          current_actor: 2,
          wall_tiles_remaining: 67,
          last_discard: 'b4',
          pending_action: {
            type: 'active_turn',
            seat_index: 2,
            deadline_at: '2026-03-26T06:01:00Z',
            drawn_tile_id: 'w2#p0-13',
            options: ['discard', 'kong', 'hu'],
          },
          players: [
            {
              seat_index: 0,
              nickname: 'Player A',
              connected: true,
              concealed_count: 13,
              melds: [],
              flowers: [],
              discards: ['b1', 'b9'],
            },
            {
              seat_index: 1,
              nickname: 'Player B',
              connected: true,
              concealed_count: 13,
              melds: [['east', 'east', 'east']],
              flowers: [],
              discards: ['c1'],
            },
            {
              seat_index: 2,
              nickname: 'Player C',
              connected: true,
              concealed_count: 14,
              concealed_tiles: [
                { tile_id: 'w1#p0-0', tile_key: 'w1' },
                { tile_id: 'w2#p0-1', tile_key: 'w2' },
              ],
              melds: [],
              flowers: [],
              discards: ['d1'],
            },
            {
              seat_index: 3,
              nickname: 'Player D',
              connected: false,
              concealed_count: 10,
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
        seat_index: 2,
        options: ['discard', 'kong', 'hu'],
        deadline_at: '2026-03-26T06:01:00Z',
      },
    },
    latestMatchResult: null,
    latestRoundEvent: null,
    lastRejectedAction: null,
    reconnectToken: 'token-2',
    toasts: [],
    ...overrides,
  };
}

function createSettlementSessionState(): SessionState {
  const base = createPlayingSessionState();

  return {
    ...base,
    roomSnapshot: {
      type: 'room_snapshot',
      payload: {
        ...base.roomSnapshot!.payload,
        phase: 'settlement',
      },
    },
    latestMatchResult: {
      type: 'match_result',
      payload: {
        table_code: 'AB12CD',
        round_id: 'round-123',
        phase: 'settlement',
        win_type: 'discard',
        winner_seat: 1,
        discarder_seat: 0,
        fan_total: 8,
        fan_keys: ['ping_hu'],
        fan_breakdown: [{ fan_key: 'ping_hu', fan_value: 8 }],
        flower_count: 0,
        score_delta: {
          provisional: true,
          fan_total: 8,
          fan_delta_by_seat: { 0: -8, 1: 8, 2: 0, 3: 0 },
          kong_delta_by_seat: { 0: 0, 1: 0, 2: 0, 3: 0 },
          total_delta_by_seat: { 0: -8, 1: 8, 2: 0, 3: 0 },
        },
        kong_score_detail: [],
      },
    },
  };
}

function createFinishedSessionState(): SessionState {
  const base = createPlayingSessionState();

  return {
    ...base,
    roomSnapshot: {
      type: 'room_snapshot',
      payload: {
        ...base.roomSnapshot!.payload,
        phase: 'finished',
        match_state: {
          ...base.roomSnapshot!.payload.match_state!,
          match_finished: true,
        },
        private_state: {
          ...base.roomSnapshot!.payload.private_state!,
          pending_action: null,
        },
      },
    },
    latestActionPrompt: null,
  };
}

describe('createMatchViewModel', () => {
  it('maps a waiting snapshot into waiting-room controls', () => {
    const viewModel = createMatchViewModel(createWaitingSessionState());

    expect(viewModel.mode).toBe('disconnected_or_waiting');
    expect(viewModel.canLeaveTable).toBe(true);
    expect(viewModel.waitingControls?.canReady).toBe(true);
    expect(viewModel.waitingControls?.canStart).toBe(false);
  });

  it('maps a local active turn into selectable discard controls', () => {
    const viewModel = createMatchViewModel(createPlayingSessionState());

    expect(viewModel.mode).toBe('my_turn');
    expect(viewModel.actions.find((item) => item.id === 'discard')?.enabled).toBe(false);
    expect(viewModel.actions.find((item) => item.id === 'hu')?.enabled).toBe(true);
    expect(viewModel.drawnTileId).toBe('w2#p0-13');
  });

  it('maps action labels and battle status to chinese-first copy', () => {
    const viewModel = createMatchViewModel(createPlayingSessionState());

    expect(viewModel.actions.find((item) => item.id === 'discard')?.label).toBe('出牌');
    expect(viewModel.canLeaveTable).toBe(true);
    expect(viewModel.topStatusLabel).toBe('对局中');
    expect(viewModel.remainingTileCount).toBe(67);
    expect(viewModel.promptText).toBe('Player C正在执行操作：出牌 / 杠 / 和牌');
  });

  it('keeps the leave-table entry visible after the full match finishes', () => {
    const viewModel = createMatchViewModel(createFinishedSessionState());

    expect(viewModel.mode).toBe('finished');
    expect(viewModel.canLeaveTable).toBe(true);
    expect(viewModel.topStatusLabel).toBe('等待再来一局');
  });

  it('shows local claim options when the local seat can respond in a claim window', () => {
    const base = createPlayingSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      roomSnapshot: {
        type: 'room_snapshot',
        payload: {
          ...base.roomSnapshot!.payload,
          private_state: {
            ...base.roomSnapshot!.payload.private_state!,
            pending_action: {
              type: 'claim_window',
              discarder_seat: 1,
              deadline_at: '2026-03-26T06:01:00Z',
              responded_seats: [],
              options: ['chow', 'pass'],
            },
          },
        },
      },
      latestActionPrompt: {
        type: 'action_prompt',
        payload: {
          seat_index: 2,
          options: ['chow', 'pass'],
          deadline_at: '2026-03-26T06:01:00Z',
        },
      },
    });

    expect(viewModel.promptText).toBe('一名玩家正在执行操作：吃');
  });

  it('shows other players are responding when the local seat has no claim options', () => {
    const base = createPlayingSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      roomSnapshot: {
        type: 'room_snapshot',
        payload: {
          ...base.roomSnapshot!.payload,
          private_state: {
            ...base.roomSnapshot!.payload.private_state!,
            pending_action: {
              type: 'claim_window',
              discarder_seat: 1,
              deadline_at: '2026-03-26T06:01:00Z',
              responded_seats: [],
              options: [],
            },
          },
        },
      },
      latestActionPrompt: null,
    });

    expect(viewModel.promptText).toBe('一名玩家正在执行操作：吃 / 碰 / 杠 / 和牌');
  });

  it('marks bot-controlled players as offline with bot copy instead of online in-match copy', () => {
    const base = createPlayingSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      roomSnapshot: {
        type: 'room_snapshot',
        payload: {
          ...base.roomSnapshot!.payload,
          seats: base.roomSnapshot!.payload.seats.map((seat) =>
            seat.seat_index === 3 ? { ...seat, connected: true, is_bot: true } : { ...seat, is_bot: false },
          ),
        },
      },
    });

    expect(viewModel.players.find((player) => player.name === 'Player D')).toMatchObject({
      connected: true,
      isBotControlled: true,
      statusText: 'Bot代打中',
    });
  });

  it('marks disconnected players as waiting to reconnect instead of in-match copy', () => {
    const viewModel = createMatchViewModel(createPlayingSessionState());

    expect(viewModel.players.find((player) => player.name === 'Player D')).toMatchObject({
      connected: false,
      isBotControlled: false,
      statusText: '等待重连中',
    });
  });

  it('maps settlement state to a result overlay payload', () => {
    const viewModel = createMatchViewModel(createSettlementSessionState());

    expect(viewModel.mode).toBe('resolving');
    expect(viewModel.result?.fanTotal).toBe(8);
    expect(viewModel.result?.winnerSeat).toBe('left');
    expect(viewModel.celebrationEffect?.label).toBe('胡牌');
    expect(viewModel.celebrationEffect?.winnerSeat).toBe('left');
  });

  it('maps latest round events into action spectacle descriptors', () => {
    const base = createPlayingSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      latestRoundEvent: {
        type: 'round_event',
        payload: {
          event_type: 'claim_made',
          event: {
            seat: 1,
            claim_type: 'pung',
            tile_id: 'w5#discard',
          },
        },
      },
    });

    expect(viewModel.actionEffect).toMatchObject({
      label: '碰',
      emphasis: 'claim',
      seat: 'left',
    });
  });

  it('maps relative seats so the local seat is always bottom', () => {
    const viewModel = createMatchViewModel(createPlayingSessionState());

    expect(viewModel.players.find((item) => item.isLocal)?.seat).toBe('bottom');
    expect(viewModel.players.find((item) => item.name === 'Player B')?.seat).toBe('left');
  });

  it('sorts the local hand by wan, tong, sou, then honors from low to high', () => {
    const viewModel = createMatchViewModel(
      createPlayingSessionState({
        roomSnapshot: {
          type: 'room_snapshot',
          payload: {
            ...createPlayingSessionState().roomSnapshot!.payload,
            private_state: {
              ...createPlayingSessionState().roomSnapshot!.payload.private_state!,
              pending_action: {
                type: 'active_turn',
                seat_index: 2,
                deadline_at: '2026-03-26T06:01:00Z',
                drawn_tile_id: 'd1#p0-5',
                options: ['discard'],
              },
              players: [
                {
                  seat_index: 0,
                  nickname: 'Player A',
                  connected: true,
                  concealed_count: 13,
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
                  concealed_count: 6,
                  concealed_tiles: [
                    { tile_id: 'd1#p0-5', tile_key: 'd1' },
                    { tile_id: 't9#p0-4', tile_key: 't9' },
                    { tile_id: 'b3#p0-3', tile_key: 'b3' },
                    { tile_id: 'w7#p0-2', tile_key: 'w7' },
                    { tile_id: 'w1#p0-1', tile_key: 'w1' },
                    { tile_id: 'b1#p0-0', tile_key: 'b1' },
                  ],
                  melds: [],
                  flowers: [],
                  discards: [],
                },
                {
                  seat_index: 3,
                  nickname: 'Player D',
                  connected: false,
                  concealed_count: 10,
                  melds: [],
                  flowers: [],
                  discards: [],
                },
              ],
            },
          },
        },
      }),
    );

    expect(viewModel.localHand.map((tile) => tile.code)).toEqual(['w1', 'w7', 'b1', 'b3', 't9', 'd1']);
  });
});
