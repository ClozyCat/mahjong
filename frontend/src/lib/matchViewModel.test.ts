import { describe, expect, it, vi } from 'vitest';

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
        private_state: {
          ...base.roomSnapshot!.payload.private_state!,
          pending_action: null,
          players: [
            {
              seat_index: 0,
              nickname: 'Player A',
              connected: true,
              concealed_count: 2,
              concealed_tiles: [
                { tile_id: 'w9#0', tile_key: 'w9' },
                { tile_id: 'w1#0', tile_key: 'w1' },
              ],
              melds: [],
              flowers: [],
              discards: ['b1', 'b9'],
            },
            {
              seat_index: 1,
              nickname: 'Player B',
              connected: true,
              concealed_count: 2,
              concealed_tiles: [
                { tile_id: 'b3#0', tile_key: 'b3' },
                { tile_id: 'b1#0', tile_key: 'b1' },
              ],
              melds: [['east', 'east', 'east']],
              flowers: [],
              discards: ['c1'],
            },
            {
              seat_index: 2,
              nickname: 'Player C',
              connected: true,
              concealed_count: 2,
              concealed_tiles: [
                { tile_id: 't9#0', tile_key: 't9' },
                { tile_id: 'w2#0', tile_key: 'w2' },
              ],
              melds: [],
              flowers: [],
              discards: ['d1'],
            },
            {
              seat_index: 3,
              nickname: 'Player D',
              connected: false,
              concealed_count: 2,
              concealed_tiles: [
                { tile_id: 'd2#0', tile_key: 'd2' },
                { tile_id: 'd1#0', tile_key: 'd1' },
              ],
              melds: [],
              flowers: [],
              discards: [],
            },
          ],
        },
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
        display_win_label: null,
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

  it('shows cancel-ready when the local seat is already ready in the waiting room', () => {
    const base = createWaitingSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      roomSnapshot: {
        ...base.roomSnapshot!,
        payload: {
          ...base.roomSnapshot!.payload,
          seats: base.roomSnapshot!.payload.seats.map((seat) =>
            seat.seat_index === 0
              ? {
                  ...seat,
                  ready: true,
                }
              : seat,
          ),
        },
      },
    });

    expect(viewModel.waitingControls?.isReady).toBe(true);
    expect(viewModel.actions.find((action) => action.id === 'ready')?.label).toBe('取消准备');
  });

  it('derives current ready-hand waits when the local hand is already in tenpai', () => {
    const base = createPlayingSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      roomSnapshot: {
        type: 'room_snapshot',
        payload: {
          ...base.roomSnapshot!.payload,
          private_state: {
            ...base.roomSnapshot!.payload.private_state!,
            current_actor: 1,
            pending_action: null,
            players: base.roomSnapshot!.payload.private_state!.players.map((player) =>
              player.seat_index === 2
                ? {
                    ...player,
                    concealed_count: 13,
                    concealed_tiles: [
                      { tile_id: 'w1#0', tile_key: 'w1' },
                      { tile_id: 'w2#0', tile_key: 'w2' },
                      { tile_id: 'w3#0', tile_key: 'w3' },
                      { tile_id: 'w4#0', tile_key: 'w4' },
                      { tile_id: 'w5#0', tile_key: 'w5' },
                      { tile_id: 'w6#0', tile_key: 'w6' },
                      { tile_id: 'w7#0', tile_key: 'w7' },
                      { tile_id: 'w8#0', tile_key: 'w8' },
                      { tile_id: 'w9#0', tile_key: 'w9' },
                      { tile_id: 't1#0', tile_key: 't1' },
                      { tile_id: 't2#0', tile_key: 't2' },
                      { tile_id: 't3#0', tile_key: 't3' },
                      { tile_id: 't4#0', tile_key: 't4' },
                    ],
                    melds: [],
                    flowers: [],
                    discards: [],
                  }
                : player,
            ),
          },
        },
      },
      latestActionPrompt: null,
      selectedTileIds: [],
    });

    expect(viewModel.readyHandInsight).toEqual({
      source: 'current',
      discardTileId: null,
      discardTileCode: null,
      waits: [
        { code: 't1', availableCount: 2 },
        { code: 't4', availableCount: 3 },
      ],
    });
  });

  it('switches to the waits produced by the currently selected discard', () => {
    const base = createPlayingSessionState();
    const selectedTileId = 'b9#0';
    const viewModel = createMatchViewModel({
      ...base,
      selectedTileIds: [selectedTileId],
      roomSnapshot: {
        type: 'room_snapshot',
        payload: {
          ...base.roomSnapshot!.payload,
          private_state: {
            ...base.roomSnapshot!.payload.private_state!,
            pending_action: {
              type: 'active_turn',
              seat_index: 2,
              deadline_at: '2026-03-26T06:01:00Z',
              drawn_tile_id: selectedTileId,
              options: ['discard'],
            },
            players: base.roomSnapshot!.payload.private_state!.players.map((player) =>
              player.seat_index === 2
                ? {
                    ...player,
                    concealed_count: 14,
                    concealed_tiles: [
                      { tile_id: 'w1#0', tile_key: 'w1' },
                      { tile_id: 'w2#0', tile_key: 'w2' },
                      { tile_id: 'w3#0', tile_key: 'w3' },
                      { tile_id: 'w4#0', tile_key: 'w4' },
                      { tile_id: 'w5#0', tile_key: 'w5' },
                      { tile_id: 'w6#0', tile_key: 'w6' },
                      { tile_id: 'w7#0', tile_key: 'w7' },
                      { tile_id: 'w8#0', tile_key: 'w8' },
                      { tile_id: 'w9#0', tile_key: 'w9' },
                      { tile_id: 't1#0', tile_key: 't1' },
                      { tile_id: 't2#0', tile_key: 't2' },
                      { tile_id: 't3#0', tile_key: 't3' },
                      { tile_id: 't4#0', tile_key: 't4' },
                      { tile_id: selectedTileId, tile_key: 'b9' },
                    ],
                    melds: [],
                    flowers: [],
                    discards: [],
                  }
                : player,
            ),
          },
        },
      },
      latestActionPrompt: {
        type: 'action_prompt',
        payload: {
          seat_index: 2,
          options: ['discard'],
          deadline_at: '2026-03-26T06:01:00Z',
        },
      },
    });

    expect(viewModel.readyHandInsight).toEqual({
      source: 'selected_discard',
      discardTileId: selectedTileId,
      discardTileCode: 'b9',
      waits: [
        { code: 't1', availableCount: 2 },
        { code: 't4', availableCount: 3 },
      ],
    });
  });

  it('keeps a disconnected waiting player seated but blocks match start until they reconnect', () => {
    const base = createWaitingSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      roomSnapshot: {
        ...base.roomSnapshot!,
        payload: {
          ...base.roomSnapshot!.payload,
          seats: [
            { seat_index: 0, nickname: 'Player A', connected: true, ready: true },
            { seat_index: 1, nickname: 'Player B', connected: true, ready: true },
            { seat_index: 2, nickname: 'Player C', connected: true, ready: true },
            { seat_index: 3, nickname: 'Player D', connected: false, ready: true },
          ],
        },
      },
    });

    expect(viewModel.waitingControls).toMatchObject({
      occupiedSeats: 4,
      canStart: false,
    });
    expect(viewModel.players.find((player) => player.name === 'Player D')).toMatchObject({
      connected: false,
      statusText: '等待重连中',
    });
    expect(viewModel.actions.find((action) => action.id === 'start_match')?.enabled).toBe(false);
  });

  it('maps a local active turn into selectable discard controls', () => {
    const viewModel = createMatchViewModel(createPlayingSessionState());

    expect(viewModel.mode).toBe('my_turn');
    expect(viewModel.actions.find((item) => item.id === 'discard')?.enabled).toBe(false);
    expect(viewModel.actions.find((item) => item.id === 'hu')?.enabled).toBe(true);
    expect(viewModel.drawnTileId).toBe('w2#p0-13');
    expect(viewModel.promptCue).toMatchObject({
      kind: 'turn',
      tone: 'critical',
      title: '当前手牌可直接和牌',
      actionIds: ['hu', 'kong', 'discard'],
      highlightedActionIds: ['hu', 'kong', 'discard'],
    });
    expect(viewModel.actionIndicatorSeat).toBe('bottom');
  });

  it('can synthesize a local kong-response prompt before the normal discard flow', () => {
    const base = createPlayingSessionState();
    const viewModel = createMatchViewModel(
      {
        ...base,
        roomSnapshot: {
          type: 'room_snapshot',
          payload: {
            ...base.roomSnapshot!.payload,
            private_state: {
              ...base.roomSnapshot!.payload.private_state!,
              pending_action: {
                type: 'active_turn',
                seat_index: 2,
                deadline_at: '2026-03-26T06:01:00Z',
                drawn_tile_id: 'w3#3',
                options: ['discard'],
              },
              players: base.roomSnapshot!.payload.private_state!.players.map((player) =>
                player.seat_index === 2
                  ? {
                      ...player,
                      concealed_count: 5,
                      concealed_tiles: [
                        { tile_id: 'w3#0', tile_key: 'w3' },
                        { tile_id: 'w3#1', tile_key: 'w3' },
                        { tile_id: 'w3#2', tile_key: 'w3' },
                        { tile_id: 'w3#3', tile_key: 'w3' },
                        { tile_id: 'b9#0', tile_key: 'b9' },
                      ],
                    }
                  : player,
              ),
            },
          },
        },
        latestActionPrompt: {
          type: 'action_prompt',
          payload: {
            seat_index: 2,
            options: ['discard'],
            deadline_at: '2026-03-26T06:01:00Z',
          },
        },
      },
      { showLocalTurnKongPrompt: true },
    );

    expect(viewModel.promptText).toBe('Player C正在执行操作：杠');
    expect(viewModel.promptCue).toMatchObject({
      kind: 'turn_kong',
      tone: 'urgent',
      title: '当前可选择是否杠牌',
      actionIds: ['kong', 'pass'],
      highlightedActionIds: ['kong'],
      isUrgent: true,
    });
    expect(viewModel.actions.find((item) => item.id === 'kong')?.enabled).toBe(true);
    expect(viewModel.actions.find((item) => item.id === 'pass')?.enabled).toBe(true);
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

  it('keeps the restart action clickable for players who have not confirmed yet', () => {
    const base = createFinishedSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      roomSnapshot: {
        ...base.roomSnapshot!,
        payload: {
          ...base.roomSnapshot!.payload,
          continue_action: {
            action_id: 'restart_match',
            confirmed_seats: [1],
            required_seats: [0, 1, 2, 3],
            online_seats: [0, 1, 2],
          },
        },
      },
    });

    expect(viewModel.result?.continueAction).toMatchObject({
      id: 'restart_match',
      label: '再来一局',
      enabled: true,
      confirmation: {
        confirmedCount: 1,
        requiredCount: 4,
        isLocalConfirmed: false,
      },
    });
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
    expect(viewModel.promptCue).toMatchObject({
      kind: 'claim',
      tone: 'urgent',
      sourceSeat: 'left',
      highlightedActionIds: ['chow'],
    });
    expect(viewModel.actionIndicatorSeat).toBeNull();
  });

  it('deduplicates visually identical claim candidates that only differ by tile ids', () => {
    const base = createPlayingSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      roomSnapshot: {
        type: 'room_snapshot',
        payload: {
          ...base.roomSnapshot!.payload,
          private_state: {
            ...base.roomSnapshot!.payload.private_state!,
            last_discard: 't9',
            pending_action: {
              type: 'claim_window',
              discarder_seat: 1,
              deadline_at: '2026-03-26T06:01:00Z',
              responded_seats: [],
              options: ['chow', 'pass'],
            },
            players: base.roomSnapshot!.payload.private_state!.players.map((player) =>
              player.seat_index === 2
                ? {
                    ...player,
                    concealed_count: 4,
                    concealed_tiles: [
                      { tile_id: 't7#0', tile_key: 't7' },
                      { tile_id: 't7#1', tile_key: 't7' },
                      { tile_id: 't8#0', tile_key: 't8' },
                      { tile_id: 'w1#0', tile_key: 'w1' },
                    ],
                  }
                : player,
            ),
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

    expect(viewModel.claimCandidates).toHaveLength(1);
    expect(viewModel.claimCandidates[0]).toMatchObject({
      actionId: 'chow',
      tiles: [
        { code: 't7', source: 'hand' },
        { code: 't8', source: 'hand' },
        { code: 't9', source: 'claim' },
      ],
    });
  });

  it('keeps a local opening-flower pass prompt actionable when no flower replacement is available', () => {
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
              type: 'opening_flowers',
              seat_index: 2,
              deadline_at: '2026-03-26T06:01:00Z',
              options: ['pass'],
            },
          },
        },
      },
      latestActionPrompt: {
        type: 'action_prompt',
        payload: {
          seat_index: 2,
          options: ['pass'],
          deadline_at: '2026-03-26T06:01:00Z',
        },
      },
    });

    expect(viewModel.promptText).toBe('Player C正在执行操作：过');
    expect(viewModel.promptCue).toMatchObject({
      kind: 'turn',
      tone: 'info',
      title: '当前可以补花',
      detail: '你可以 过',
      actionIds: ['pass'],
      highlightedActionIds: [],
      sourceSeat: null,
      isUrgent: false,
    });
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
    expect(viewModel.promptCue).toBeNull();
  });

  it('falls back to the public current actor after a claim resolves and the local action prompt is stale', () => {
    const base = createPlayingSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      roomSnapshot: {
        type: 'room_snapshot',
        payload: {
          ...base.roomSnapshot!.payload,
          private_state: {
            ...base.roomSnapshot!.payload.private_state!,
            current_actor: 0,
            pending_action: null,
          },
        },
      },
      latestActionPrompt: {
        type: 'action_prompt',
        payload: {
          seat_index: 1,
          options: ['chow', 'pass'],
          deadline_at: '2026-03-26T06:01:00Z',
        },
      },
    });

    expect(viewModel.promptText).toBe('Player A正在执行操作：出牌');
    expect(viewModel.activePlayerSeat).toBe('top');
    expect(viewModel.actionIndicatorSeat).toBe('top');
    expect(viewModel.players.find((player) => player.name === 'Player A')?.isActive).toBe(true);
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
    expect(viewModel.result?.winTypeLabel).toBe('荣和');
    expect(viewModel.settlementHands).toEqual({
      top: ['w1', 'w9'],
      left: ['b1', 'b3', 'b4'],
      bottom: ['w2', 't9'],
      right: ['d1', 'd2'],
    });
  });

  it('marks the next-round action as confirmed for the local player after they click it', () => {
    const base = createSettlementSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      roomSnapshot: {
        ...base.roomSnapshot!,
        payload: {
          ...base.roomSnapshot!.payload,
          continue_action: {
            action_id: 'start_next_round',
            confirmed_seats: [2],
            required_seats: [0, 1, 2, 3],
            online_seats: [0, 1, 2],
          },
        },
      },
    });

    expect(viewModel.result?.continueAction).toMatchObject({
      id: 'start_next_round',
      label: '已确认 1/4',
      enabled: false,
      confirmation: {
        confirmedCount: 1,
        requiredCount: 4,
        isLocalConfirmed: true,
      },
    });
  });

  it('shows a countdown after all online players confirm while offline humans are still pending', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-04-02T12:00:00Z'));

    const base = createSettlementSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      roomSnapshot: {
        ...base.roomSnapshot!,
        payload: {
          ...base.roomSnapshot!.payload,
          continue_action: {
            action_id: 'start_next_round',
            confirmed_seats: [0, 1, 2],
            required_seats: [0, 1, 2, 3],
            online_seats: [0, 1, 2],
            auto_advance_deadline_at: '2026-04-02T12:01:00Z',
          },
        },
      },
    });

    expect(viewModel.result?.continueAction).toMatchObject({
      id: 'start_next_round',
      label: '60s后自动推进',
      enabled: false,
      countdownDeadlineAt: '2026-04-02T12:01:00Z',
      confirmation: {
        confirmedCount: 3,
        requiredCount: 4,
        isLocalConfirmed: true,
        countdownDeadlineAt: '2026-04-02T12:01:00Z',
      },
    });

    vi.useRealTimers();
  });

  it('uses 屁和 copy for low-fan wins when eight-fan restriction is disabled', () => {
    const base = createSettlementSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      latestMatchResult: {
        ...base.latestMatchResult!,
        payload: {
          ...base.latestMatchResult!.payload,
          win_type: 'self_draw',
          display_win_label: '屁和',
          fan_total: 4,
          score_delta: {
            ...base.latestMatchResult!.payload.score_delta,
            fan_total: 4,
          },
        },
      },
    });

    expect(viewModel.result?.summary).toContain('屁和');
    expect(viewModel.result?.winTypeLabel).toBe('屁和');
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
      calloutTone: 'pung',
    });
  });

  it('maps self-hu round events into a hu action spectacle descriptor', () => {
    const base = createPlayingSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      latestRoundEvent: {
        type: 'round_event',
        payload: {
          event_type: 'self_hu_declared',
          event: {
            seat: 2,
            tile_id: 'w2#p0-13',
          },
        },
      },
    });

    expect(viewModel.actionEffect).toMatchObject({
      label: '和',
      emphasis: 'claim',
      seat: 'bottom',
      calloutTone: 'hu',
    });
  });

  it('maps relative seats so the local seat is always bottom', () => {
    const viewModel = createMatchViewModel(createPlayingSessionState());

    expect(viewModel.players.find((item) => item.isLocal)?.seat).toBe('bottom');
    expect(viewModel.players.find((item) => item.name === 'Player B')?.seat).toBe('left');
  });

  it('sorts each meld group for display so chow tiles render in suit order', () => {
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
              player.seat_index === 1
                ? {
                    ...player,
                    melds: [['w8', 'w6', 'w7']],
                  }
                : player,
            ),
          },
        },
      },
    });

    expect(viewModel.players.find((item) => item.name === 'Player B')?.melds).toEqual([['w6', 'w7', 'w8']]);
  });

  it('ignores invalid meld tile codes instead of crashing the table view', () => {
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
              player.seat_index === 1
                ? {
                    ...player,
                    melds: [['w8', null as unknown as string, 'w6', 'w7']],
                  }
                : player,
            ),
          },
        },
      },
    });

    expect(viewModel.players.find((item) => item.name === 'Player B')?.melds).toEqual([['w6', 'w7', 'w8']]);
  });

  it('computes player winds relative to the current dealer seat', () => {
    const viewModel = createMatchViewModel(createPlayingSessionState());

    expect(viewModel.players.find((item) => item.name === 'Player B')).toMatchObject({
      wind: 'East',
      isDealer: true,
    });
    expect(viewModel.players.find((item) => item.name === 'Player C')).toMatchObject({
      wind: 'South',
    });
    expect(viewModel.players.find((item) => item.name === 'Player D')).toMatchObject({
      wind: 'West',
    });
    expect(viewModel.players.find((item) => item.name === 'Player A')).toMatchObject({
      wind: 'North',
    });
  });

  it('keeps the freshly drawn tile at the end of the local hand until the turn advances', () => {
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
                drawn_tile_id: 'w1#p0-5',
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
                    { tile_id: 'w1#p0-5', tile_key: 'w1' },
                    { tile_id: 't9#p0-4', tile_key: 't9' },
                    { tile_id: 'b3#p0-3', tile_key: 'b3' },
                    { tile_id: 'w7#p0-2', tile_key: 'w7' },
                    { tile_id: 'd1#p0-1', tile_key: 'd1' },
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

    expect(viewModel.localHand.map((tile) => tile.code)).toEqual(['w7', 'b1', 'b3', 't9', 'd1', 'w1']);
    expect(viewModel.localHand.at(-1)).toMatchObject({
      tileId: 'w1#p0-5',
      isDrawn: true,
    });
  });

  it('returns the previously drawn tile to the normal sorted hand once active turn ends', () => {
    const base = createPlayingSessionState();
    const tiles = [
      { tile_id: 'w1#p0-5', tile_key: 'w1' },
      { tile_id: 't9#p0-4', tile_key: 't9' },
      { tile_id: 'b3#p0-3', tile_key: 'b3' },
      { tile_id: 'w7#p0-2', tile_key: 'w7' },
      { tile_id: 'd1#p0-1', tile_key: 'd1' },
      { tile_id: 'b1#p0-0', tile_key: 'b1' },
    ];
    const viewModel = createMatchViewModel({
      ...base,
      roomSnapshot: {
        type: 'room_snapshot',
        payload: {
          ...base.roomSnapshot!.payload,
          private_state: {
            ...base.roomSnapshot!.payload.private_state!,
            pending_action: null,
            players: base.roomSnapshot!.payload.private_state!.players.map((player) =>
              player.seat_index === 2
                ? {
                    ...player,
                    concealed_count: tiles.length,
                    concealed_tiles: tiles,
                  }
                : player,
            ),
          },
        },
      },
      latestActionPrompt: null,
    });

    expect(viewModel.localHand.map((tile) => tile.code)).toEqual(['w1', 'w7', 'b1', 'b3', 't9', 'd1']);
    expect(viewModel.localHand.some((tile) => tile.isDrawn)).toBe(false);
  });

  it('marks same-turn restricted tiles as disabled and keeps discard unavailable for them', () => {
    const base = createPlayingSessionState({
      selectedTileIds: ['w1#p0-0'],
    });
    const viewModel = createMatchViewModel({
      ...base,
      roomSnapshot: {
        type: 'room_snapshot',
        payload: {
          ...base.roomSnapshot!.payload,
          private_state: {
            ...base.roomSnapshot!.payload.private_state!,
            pending_action: {
              type: 'active_turn',
              seat_index: 2,
              deadline_at: '2026-03-26T06:01:00Z',
              drawn_tile_id: 'w2#p0-1',
              restricted_discard_tile_ids: ['w1#p0-0'],
              options: ['discard'],
            },
            players: base.roomSnapshot!.payload.private_state!.players.map((player) =>
              player.seat_index === 2
                ? {
                    ...player,
                    concealed_count: 2,
                    concealed_tiles: [
                      { tile_id: 'w1#p0-0', tile_key: 'w1' },
                      { tile_id: 'w2#p0-1', tile_key: 'w2' },
                    ],
                  }
                : player,
            ),
          },
        },
      },
      latestActionPrompt: {
        type: 'action_prompt',
        payload: {
          seat_index: 2,
          options: ['discard'],
          deadline_at: '2026-03-26T06:01:00Z',
        },
      },
    });

    expect(viewModel.localHand.find((tile) => tile.tileId === 'w1#p0-0')).toMatchObject({
      isDisabled: true,
    });
    expect(viewModel.actions.find((action) => action.id === 'discard')?.enabled).toBe(false);
  });
});
