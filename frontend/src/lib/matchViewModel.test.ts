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
    expect(viewModel.waitingControls?.botCount).toBe(0);
    expect(viewModel.waitingControls?.canAddBot).toBe(true);
    expect(viewModel.waitingControls?.canRemoveBot).toBe(false);
  });

  it('surfaces dealer selection and disables waiting controls during the draw animation', () => {
    const base = createWaitingSessionState();
    const state: SessionState = {
      ...base,
      roomSnapshot: {
        ...base.roomSnapshot!,
        payload: {
          ...base.roomSnapshot!.payload,
          seats: [
            { seat_index: 0, nickname: 'Alice', connected: true, ready: true },
            { seat_index: 1, nickname: 'Bob', connected: true, ready: true },
            { seat_index: 2, nickname: 'Carol', connected: true, ready: true },
            { seat_index: 3, nickname: 'Dora', connected: true, ready: true },
          ],
        },
      },
      latestRoundEvent: {
        type: 'round_event',
        payload: {
          event_type: 'dealer_selection_started',
          event: {
            type: 'dealer_selection_started',
            dealer_seat: 1,
            started_at: '2026-04-27T12:00:00Z',
            reveal_at: '2026-04-27T12:00:04.200Z',
            duration_ms: 4200,
          },
        },
      },
    };

    const viewModel = createMatchViewModel(state);

    expect(viewModel.dealerSelection).toMatchObject({
      dealerSeat: 'right',
      dealerName: 'Bob',
      durationMs: 4200,
    });
    expect(viewModel.centerStatusText).toBe('抽取东家');
    expect(viewModel.waitingControls?.canReady).toBe(false);
    expect(viewModel.waitingControls?.canStart).toBe(false);
    expect(viewModel.actions.find((action) => action.id === 'start_match')?.enabled).toBe(false);
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

  it('projects the current hand insight directly from backend snapshot data', () => {
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
            hand_insights: {
              current: {
                discard_tile_id: null,
                discard_tile_code: null,
                is_tenpai: true,
                waits: [
                  { code: 't1', available_count: 2 },
                  { code: 't4', available_count: 3 },
                ],
                winning_fans: [{ fan_key: 'full_flush', fan_value: 24 }],
              },
              by_discard_tile_id: {},
            },
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

    expect(viewModel.handInsight).toEqual({
      source: 'current',
      discardTileId: null,
      discardTileCode: null,
      isTenpai: true,
      waits: [
        { code: 't1', availableCount: 2 },
        { code: 't4', availableCount: 3 },
      ],
      winningFans: [{ fanKey: 'full_flush', fanValue: 24 }],
    });
  });

  it('switches to the selected-discard hand insight from backend snapshot data', () => {
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
            hand_insights: {
              current: {
                discard_tile_id: null,
                discard_tile_code: null,
                is_tenpai: false,
                waits: [],
                winning_fans: [],
              },
              by_discard_tile_id: {
                [selectedTileId]: {
                  discard_tile_id: selectedTileId,
                  discard_tile_code: 'b9',
                  is_tenpai: true,
                  waits: [{ code: 't4', available_count: 3 }],
                  winning_fans: [{ fan_key: 'full_flush', fan_value: 24 }],
                },
              },
            },
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
    });

    expect(viewModel.handInsight).toEqual({
      source: 'selected_discard',
      discardTileId: selectedTileId,
      discardTileCode: 'b9',
      isTenpai: true,
      waits: [{ code: 't4', availableCount: 3 }],
      winningFans: [{ fanKey: 'full_flush', fanValue: 24 }],
    });
  });

  it('enables ready_hand only when the selected discard preview is tenpai', () => {
    const base = createPlayingSessionState();
    const selectedTileId = 'b9#0';
    const createReadyHandTurnState = (selectedTileIds: string[]) =>
      createMatchViewModel({
        ...base,
        selectedTileIds,
        roomSnapshot: {
          type: 'room_snapshot',
          payload: {
            ...base.roomSnapshot!.payload,
            private_state: {
              ...base.roomSnapshot!.payload.private_state!,
              hand_insights: {
                current: null,
                by_discard_tile_id: {
                  [selectedTileId]: {
                    discard_tile_id: selectedTileId,
                    discard_tile_code: 'b9',
                    is_tenpai: true,
                    waits: [{ code: 't4', available_count: 3 }],
                    winning_fans: [{ fan_key: 'full_flush', fan_value: 24 }],
                  },
                },
              },
              pending_action: {
                type: 'active_turn',
                seat_index: 2,
                deadline_at: '2026-03-26T06:01:00Z',
                drawn_tile_id: selectedTileId,
                options: ['discard', 'ready_hand'],
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
            options: ['discard', 'ready_hand'],
            deadline_at: '2026-03-26T06:01:00Z',
          },
        },
      });

    const unselectedViewModel = createReadyHandTurnState([]);
    const selectedViewModel = createReadyHandTurnState([selectedTileId]);

    expect(unselectedViewModel.actions.find((action) => action.id === 'ready_hand')?.enabled).toBe(false);
    expect(selectedViewModel.actions.find((action) => action.id === 'ready_hand')?.enabled).toBe(true);
    expect(selectedViewModel.promptCue).toMatchObject({
      actionIds: ['discard', 'ready_hand'],
      highlightedActionIds: ['discard', 'ready_hand'],
    });
  });

  it('locks the local hand after ready_hand is declared', () => {
    const base = createPlayingSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      selectedTileIds: ['w1#0'],
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
              drawn_tile_id: 'b9#0',
              options: ['hu'],
            },
            players: base.roomSnapshot!.payload.private_state!.players.map((player) =>
              player.seat_index === 2
                ? {
                    ...player,
                    is_ready_hand: true,
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
                      { tile_id: 'b9#0', tile_key: 'b9' },
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
          options: ['hu'],
          deadline_at: '2026-03-26T06:01:00Z',
        },
      },
    });

    expect(viewModel.localHand).toHaveLength(14);
    expect(viewModel.localHand.every((tile) => tile.isDisabled)).toBe(true);
    expect(viewModel.actions.find((action) => action.id === 'discard')?.enabled).toBe(false);
    expect(viewModel.actions.find((action) => action.id === 'ready_hand')?.enabled).toBe(false);
    expect(viewModel.actions.find((action) => action.id === 'hu')?.enabled).toBe(true);
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

  it('counts bot seats in the waiting room and enables removing them', () => {
    const base = createWaitingSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      roomSnapshot: {
        ...base.roomSnapshot!,
        payload: {
          ...base.roomSnapshot!.payload,
          seats: [
            { seat_index: 0, nickname: 'Player A', connected: true, ready: true, is_bot: false },
            { seat_index: 1, nickname: 'Bot 1', connected: true, ready: true, is_bot: true },
            { seat_index: 2, nickname: 'Bot 2', connected: true, ready: true, is_bot: true },
          ],
        },
      },
    });

    expect(viewModel.waitingControls).toMatchObject({
      occupiedSeats: 3,
      botCount: 2,
      canAddBot: true,
      canRemoveBot: true,
      canStart: false,
    });
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

  it('adds pass to a local self-hu prompt when requested', () => {
    const viewModel = createMatchViewModel(createPlayingSessionState(), {
      showLocalSelfHuPassOption: true,
    });

    expect(viewModel.promptText).toBe('Player C正在执行操作：出牌 / 杠 / 和牌');
    expect(viewModel.promptCue).toMatchObject({
      kind: 'turn',
      tone: 'critical',
      title: '当前手牌可直接和牌',
      actionIds: ['hu', 'kong', 'discard', 'pass'],
      highlightedActionIds: ['hu', 'kong', 'discard'],
    });
    expect(viewModel.actions.find((item) => item.id === 'pass')?.enabled).toBe(true);
  });

  it('hides local self-hu after passing while keeping the turn playable', () => {
    const viewModel = createMatchViewModel(createPlayingSessionState(), {
      hideLocalSelfHuPrompt: true,
    });

    expect(viewModel.promptCue).toMatchObject({
      kind: 'turn',
      tone: 'info',
      title: '轮到你操作',
      actionIds: ['kong', 'discard'],
      highlightedActionIds: ['kong', 'discard'],
    });
    expect(viewModel.actions.find((item) => item.id === 'hu')?.enabled).toBe(false);
    expect(viewModel.actions.find((item) => item.id === 'discard')?.enabled).toBe(false);
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

  it('shows flower as a local active-turn option when a flower tile is selected', () => {
    const base = createPlayingSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      selectedTileIds: ['f1#0'],
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
              drawn_tile_id: 'f1#0',
              options: ['discard', 'flower'],
            },
            players: base.roomSnapshot!.payload.private_state!.players.map((player) =>
              player.seat_index === 2
                ? {
                    ...player,
                    concealed_count: 2,
                    concealed_tiles: [
                      { tile_id: 'f1#0', tile_key: 'f1' },
                      { tile_id: 'w2#0', tile_key: 'w2' },
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
          options: ['discard', 'flower'],
          deadline_at: '2026-03-26T06:01:00Z',
        },
      },
    });

    expect(viewModel.promptText).toBe('Player C正在执行操作：出牌 / 补花');
    expect(viewModel.promptCue).toMatchObject({
      kind: 'turn',
      tone: 'info',
      title: '轮到你操作',
      detail: '你可以 补花 / 出牌',
      actionIds: ['flower', 'discard'],
      highlightedActionIds: ['flower', 'discard'],
      sourceSeat: null,
      isUrgent: false,
    });
    expect(viewModel.actions.find((action) => action.id === 'flower')).toMatchObject({
      enabled: true,
      label: '补花',
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
    expect(viewModel.deadlineAt).toBeNull();
    expect(viewModel.players.find((player) => player.name === 'Player A')?.isActive).toBe(true);
  });

  it('shows the acting player countdown and public discard prompt for an observed active turn', () => {
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
            pending_action: {
              type: 'active_turn',
              seat_index: 0,
              deadline_at: '2026-03-26T06:01:00Z',
              options: [],
            },
          },
        },
      },
      latestActionPrompt: null,
    });

    expect(viewModel.promptText).toBe('Player A正在执行操作：出牌');
    expect(viewModel.deadlineAt).toBe('2026-03-26T06:01:00Z');
    expect(viewModel.actionIndicatorSeat).toBe('top');
    expect(viewModel.actions.find((action) => action.id === 'discard')?.enabled).toBe(false);
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

  it('builds settlement pages and winning hands for multiple discard winners', () => {
    const base = createSettlementSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      latestMatchResult: {
        ...base.latestMatchResult!,
        payload: {
          ...base.latestMatchResult!.payload,
          display_win_label: '一炮多响',
          winning_details: [
            {
              winner_seat: 1,
              display_win_label: null,
              fan_total: 8,
              fan_keys: ['ping_hu'],
              fan_breakdown: [{ fan_key: 'ping_hu', fan_value: 8 }],
              flower_count: 0,
            },
            {
              winner_seat: 2,
              display_win_label: null,
              fan_total: 16,
              fan_keys: ['full_flush'],
              fan_breakdown: [{ fan_key: 'full_flush', fan_value: 16 }],
              flower_count: 1,
            },
          ],
        },
      },
    });

    expect(viewModel.result?.summary).toContain('2 家同时和牌');
    expect(viewModel.result?.pages).toHaveLength(2);
    expect(viewModel.result?.pages?.[0]).toMatchObject({
      winnerSeat: 'left',
      fanTotal: 8,
    });
    expect(viewModel.result?.pages?.[1]).toMatchObject({
      winnerSeat: 'bottom',
      fanTotal: 16,
      flowerCount: 1,
    });
    expect(viewModel.settlementHands).toEqual({
      top: ['w1', 'w9'],
      left: ['b1', 'b3', 'b4'],
      bottom: ['w2', 't9', 'b4'],
      right: ['d1', 'd2'],
    });
  });

  it('pins the settlement last discard seat to the recorded discarder instead of inferring from current actor', () => {
    const base = createSettlementSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      roomSnapshot: {
        ...base.roomSnapshot!,
        payload: {
          ...base.roomSnapshot!.payload,
          private_state: {
            ...base.roomSnapshot!.payload.private_state!,
            current_actor: 2,
          },
        },
      },
      latestRoundEvent: null,
    });

    expect(viewModel.lastDiscardSeat).toBe('top');
  });

  it('maps per-seat match statistics into the settlement score rows', () => {
    const base = createSettlementSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      matchStatistics: {
        completedRoundCount: 3,
        lastAppliedRoundId: 'round-123',
        seatStatsBySeat: {
          '0': { scoreHistory: [0, -8, -4, -8], winCount: 0, dealInCount: 2 },
          '1': { scoreHistory: [0, 8, 10, 8], winCount: 1, dealInCount: 0 },
          '2': { scoreHistory: [0, 0, -6, 0], winCount: 1, dealInCount: 0 },
          '3': { scoreHistory: [0, 0, 0, 0], winCount: 0, dealInCount: 1 },
        },
      },
    });

    expect(viewModel.result?.seats.find((seat) => seat.seat === 'left')?.stats).toEqual({
      scoreHistory: [0, 8, 10, 8],
      winCount: 1,
      dealInCount: 0,
      completedRoundCount: 3,
      winRate: 1 / 3,
    });
    expect(viewModel.result?.seats.find((seat) => seat.seat === 'bottom')?.stats).toEqual({
      scoreHistory: [0, 0, -6, 0],
      winCount: 1,
      dealInCount: 0,
      completedRoundCount: 3,
      winRate: 1 / 3,
    });
  });

  it('uses settlement snapshot cumulative scores without reapplying the current round delta', () => {
    const base = createSettlementSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      roomSnapshot: {
        ...base.roomSnapshot!,
        payload: {
          ...base.roomSnapshot!.payload,
          match_state: {
            ...base.roomSnapshot!.payload.match_state!,
            cumulative_scores: { '0': -8, '1': 8, '2': 0, '3': 0 },
            last_completed_round_id: 'round-123',
          },
        },
      },
    });

    expect(viewModel.result?.seats.find((seat) => seat.seat === 'left')?.score).toBe(8);
    expect(viewModel.result?.seats.find((seat) => seat.seat === 'top')?.score).toBe(-8);
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

  it('uses final-score copy for the north-four settlement action', () => {
    const base = createSettlementSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      roomSnapshot: {
        ...base.roomSnapshot!,
        payload: {
          ...base.roomSnapshot!.payload,
          match_state: {
            ...base.roomSnapshot!.payload.match_state!,
            prevailing_wind: 'north',
            hand_number: 4,
          },
          private_state: {
            ...base.roomSnapshot!.payload.private_state!,
            round_wind: 'north',
          },
        },
      },
    });

    expect(viewModel.actions.find((action) => action.id === 'restart_match')?.label).toBe('再来一局');
    expect(viewModel.result?.continueAction).toMatchObject({
      id: 'restart_match',
      label: '再来一局',
      enabled: true,
    });
  });

  it('falls back to the occupied human seat count when continue-action totals are missing', () => {
    const base = createSettlementSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      roomSnapshot: {
        ...base.roomSnapshot!,
        payload: {
          ...base.roomSnapshot!.payload,
          seats: base.roomSnapshot!.payload.seats.map((seat) =>
            seat.seat_index === 3
              ? {
                  ...seat,
                  is_bot: true,
                }
              : seat,
          ),
          continue_action: {
            action_id: 'start_next_round',
            confirmed_seats: [2],
            required_seats: [],
            online_seats: [],
          },
        },
      },
    });

    expect(viewModel.result?.continueAction).toMatchObject({
      id: 'start_next_round',
      label: '已确认 1/3',
      enabled: false,
      confirmation: {
        confirmedCount: 1,
        requiredCount: 3,
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

  it('disables the next-round action while the client is reconnecting', () => {
    const base = createSettlementSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      connectionStatus: 'reconnecting',
    });

    expect(viewModel.mode).toBe('disconnected_or_waiting');
    expect(viewModel.result?.continueAction).toMatchObject({
      id: 'start_next_round',
      label: '重连中...',
      enabled: false,
    });
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

  it('maps ready_hand_declared into a ting action effect', () => {
    const base = createPlayingSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      latestRoundEvent: {
        type: 'round_event',
        payload: {
          event_type: 'ready_hand_declared',
          event: {
            seat: 2,
            tile_id: 'b9#discard',
          },
        },
      },
    });

    expect(viewModel.actionEffect).toMatchObject({
      label: '听',
      emphasis: 'claim',
      seat: 'bottom',
      calloutTone: 'ready_hand',
      tileCode: 'b9',
    });
  });

  it('uses the tile from tile_discarded events for discard action effects', () => {
    const base = createPlayingSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      latestRoundEvent: {
        type: 'round_event',
        payload: {
          event_type: 'tile_discarded',
          event: {
            seat: 1,
            tile_id: 'b4#bot-7',
          },
        },
      },
    });

    expect(viewModel.actionEffect).toMatchObject({
      label: '出牌',
      emphasis: 'discard',
      seat: 'left',
      calloutTone: null,
      tileCode: 'b4',
    });
  });

  it('keeps earlier round events available for voice playback when a later event is visible', () => {
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
          round_id: 'round-123',
        },
      },
    };
    const base = createPlayingSessionState({
      latestRoundEvent: settlementEvent,
      recentRoundEvents: [discardEvent, settlementEvent],
    });
    const viewModel = createMatchViewModel(base);

    expect(viewModel.actionEffect).toMatchObject({
      label: '结算',
      emphasis: 'system',
    });
    expect(viewModel.actionEffects).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          label: '出牌',
          emphasis: 'discard',
          seat: 'left',
          tileCode: 'b7',
        }),
      ]),
    );
  });

  it('maps relative seats so the local seat is always bottom', () => {
    const viewModel = createMatchViewModel(createPlayingSessionState());

    expect(viewModel.players.find((item) => item.isLocal)?.seat).toBe('bottom');
    expect(viewModel.players.find((item) => item.name === 'Player B')?.seat).toBe('left');
  });

  it('uses spectator perspective seat for hand and relative positions', () => {
    const base = createPlayingSessionState();
    const viewModel = createMatchViewModel(
      {
        ...base,
        clientMode: 'spectator',
        spectatorFocusSeat: 1,
        roomSnapshot: {
          type: 'room_snapshot',
          payload: {
            ...base.roomSnapshot!.payload,
            local_seat: null,
            reconnect_token: null,
            private_state: {
              ...base.roomSnapshot!.payload.private_state!,
              pending_action: {
                type: 'active_turn',
                seat_index: 2,
                deadline_at: '2026-03-26T06:01:00Z',
                options: [],
              },
              players: base.roomSnapshot!.payload.private_state!.players.map((player) => ({
                ...player,
                concealed_tiles: [
                  { tile_id: `seat-${player.seat_index}#0`, tile_key: `w${player.seat_index + 1}` },
                ],
              })),
            },
          },
        },
        latestActionPrompt: null,
      },
      {
        perspectiveSeat: 1,
        isSpectator: true,
      },
    );

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

  it('preserves meld tile order for display so source orientation is not lost', () => {
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

    expect(viewModel.players.find((item) => item.name === 'Player B')?.melds).toEqual([
      {
        tiles: [
          { code: 'w8', orientation: 'normal' },
          { code: 'w6', orientation: 'normal' },
          { code: 'w7', orientation: 'normal' },
        ],
      },
    ]);
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

    expect(viewModel.players.find((item) => item.name === 'Player B')?.melds).toEqual([
      {
        tiles: [
          { code: 'w8', orientation: 'normal' },
          { code: 'w6', orientation: 'normal' },
          { code: 'w7', orientation: 'normal' },
        ],
      },
    ]);
  });

  it('prefers backend display meld metadata so refreshes keep the claimed-tile orientation', () => {
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
                ? ({
                    ...player,
                    display_melds: [
                      {
                        tiles: [
                          { code: 'w3', orientation: 'rotated' },
                          { code: 'w3', orientation: 'normal' },
                          { code: 'w3', orientation: 'normal' },
                        ],
                      },
                    ],
                  } as typeof player & {
                    display_melds: Array<{
                      tiles: Array<{ code: string; orientation: 'normal' | 'rotated' | 'face_down' }>;
                    }>;
                  })
                : player,
            ),
          },
        },
      },
    });

    expect(viewModel.players.find((item) => item.name === 'Player B')?.melds).toEqual([
      {
        tiles: [
          { code: 'w3', orientation: 'rotated' },
          { code: 'w3', orientation: 'normal' },
          { code: 'w3', orientation: 'normal' },
        ],
      },
    ]);
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

  it('uses projected score state and current round deltas from the room snapshot', () => {
    const base = createPlayingSessionState();
    const viewModel = createMatchViewModel({
      ...base,
      roomSnapshot: {
        ...base.roomSnapshot!,
        payload: {
          ...base.roomSnapshot!.payload,
          match_state: {
            ...base.roomSnapshot!.payload.match_state!,
            cumulative_scores: { '0': 0, '1': 0, '2': -3, '3': 0 },
          },
          private_state: {
            ...base.roomSnapshot!.payload.private_state!,
            score_state: {
              flower_count_by_seat: { '0': 0, '1': 0, '2': 0, '3': 0 },
              kong_score_detail: [],
              kong_delta_by_seat: { '0': 0, '1': 0, '2': 0, '3': 0 },
              current_round_delta_by_seat: { '0': 0, '1': 0, '2': -3, '3': 0 },
              base_cumulative_scores: { '0': 0, '1': 0, '2': 0, '3': 0 },
              projected_cumulative_scores: { '0': 0, '1': 0, '2': -3, '3': 0 },
            },
          },
        },
      },
    });

    expect(viewModel.players.find((item) => item.isLocal)).toMatchObject({
      score: -3,
      liveDelta: -3,
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

  it('marks the local replacement draw tile for the dock animation and keeps it at the hand tail', () => {
    const viewModel = createMatchViewModel(
      createPlayingSessionState({
        latestReplacementTileId: 'w1#p0-5',
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

    expect(viewModel.localHand.at(-1)).toMatchObject({
      tileId: 'w1#p0-5',
      isDrawn: true,
      isReplacementDrawn: true,
    });
    expect(viewModel.localHand.filter((tile) => tile.isReplacementDrawn)).toHaveLength(1);
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

  it('projects an optimistic discard into the local hand, river, and prompt state before the server snapshot arrives', () => {
    const base = createPlayingSessionState({
      selectedTileIds: ['w2#p0-1'],
      optimisticDiscard: {
        tileId: 'w2#p0-1',
        tileCode: 'w2',
        seatIndex: 2,
        actionType: 'discard',
        actionEffectKey: 'optimistic-discard:w2#p0-1',
        requestedAt: '2026-03-26T06:01:00Z',
      },
    });
    const viewModel = createMatchViewModel(base);

    expect(viewModel.mode).toBe('watching');
    expect(viewModel.localHand.map((tile) => tile.tileId)).not.toContain('w2#p0-1');
    expect(viewModel.localHand.every((tile) => tile.isDisabled)).toBe(true);
    expect(viewModel.discards.bottom).toEqual(['d1', 'w2']);
    expect(viewModel.lastDiscard).toBe('w2');
    expect(viewModel.lastDiscardSeat).toBe('bottom');
    expect(viewModel.promptText).toBe('你已出牌，等待服务器确认...');
    expect(viewModel.promptCue).toBeNull();
    expect(viewModel.actions.find((action) => action.id === 'discard')?.enabled).toBe(false);
    expect(viewModel.handInsight).toBeNull();
    expect(viewModel.shouldAutoReturnLastDiscardToRiver).toBe(false);
    expect(viewModel.actionIndicatorSeat).toBeNull();
  });

  it('projects an optimistic ready_hand as a ting callout instead of a normal discard effect', () => {
    const base = createPlayingSessionState({
      selectedTileIds: ['w2#p0-1'],
      optimisticDiscard: {
        tileId: 'w2#p0-1',
        tileCode: 'w2',
        seatIndex: 2,
        actionType: 'ready_hand',
        actionEffectKey: 'optimistic-ready_hand:w2#p0-1',
        requestedAt: '2026-03-26T06:01:00Z',
      },
    });
    const viewModel = createMatchViewModel(base);

    expect(viewModel.promptText).toBe('你已听牌，等待服务器确认...');
    expect(viewModel.actionEffect).toMatchObject({
      label: '听',
      emphasis: 'claim',
      seat: 'bottom',
      calloutTone: 'ready_hand',
    });
    expect(viewModel.shouldAutoReturnLastDiscardToRiver).toBe(false);
  });

});
