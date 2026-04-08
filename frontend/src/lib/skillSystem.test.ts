import { describe, expect, it, vi } from 'vitest';

import type { BattleViewModel, SessionState, SkillRarity } from '../types/match';
import {
  confirmSkillActivation,
  createInitialSkillRuntimeState,
  createSkillEnhancedBattleViewModel,
  openSkillActivation,
  syncSkillRuntimeWithSession,
  type SkillRuntimeState,
} from './skillSystem';

function createSessionState(overrides: Partial<SessionState> = {}): SessionState {
  return {
    apiBaseUrl: 'http://localhost:8000',
    wsBaseUrl: 'ws://localhost:8000',
    tableCode: 'AB12CD',
    nickname: 'Player A',
    connectionStatus: 'connected',
    roomSnapshot: {
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
        match_state: {
          prevailing_wind: 'east',
          hand_number: 1,
          dealer_seat: 0,
          cumulative_scores: { '0': 25000, '1': 25000, '2': 25000, '3': 25000 },
          match_finished: false,
          last_completed_round_id: null,
        },
        private_state: {
          round_id: 'round-1',
          round_wind: 'east',
          dealer_seat: 0,
          current_actor: 0,
          last_discard: null,
          pending_action: {
            type: 'opening_flowers',
            seat_index: 0,
            deadline_at: '2099-03-30T12:10:40+08:00',
            options: ['flower', 'pass'],
          },
          players: [
            {
              seat_index: 0,
              nickname: 'Player A',
              connected: true,
              concealed_count: 13,
              concealed_tiles: [
                { tile_id: 'w1#1', tile_key: 'w1' },
                { tile_id: 'w2#2', tile_key: 'w2' },
              ],
              melds: [['b2', 'b3', 'b4']],
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
    latestMatchResult: null,
    latestActionPrompt: null,
    latestRoundEvent: null,
    latestQuickChatMessage: null,
    lastRejectedAction: null,
    reconnectToken: null,
    selectedTileIds: [],
    selectionMode: null,
    toasts: [],
    matchStatistics: null,
    ...overrides,
  };
}

function createBattleViewModel(overrides: Partial<BattleViewModel> = {}): BattleViewModel {
  return {
    roomMode: 'normal',
    mode: 'my_turn',
    tableCode: 'AB12CD',
    canLeaveTable: true,
    phaseLabel: '进行中',
    roundLabel: '东1局',
    scoreSummaryLabel: '总分 25000',
    deadlineAt: null,
    topStatusLabel: '对局中',
    activePlayerSeat: 'bottom',
    actionIndicatorSeat: 'bottom',
    isActionDockElevated: true,
    players: [
      {
        seat: 'bottom',
        name: 'Player A',
        score: 25000,
        liveDelta: 0,
        flowerCount: 0,
        wind: 'East',
        isDealer: true,
        isActive: true,
        isLocal: true,
        connected: true,
        ready: true,
        concealedCount: 13,
        meldCount: 1,
        melds: [['b2', 'b3', 'b4']],
        flowers: [],
        statusText: '对局中',
      },
      {
        seat: 'right',
        name: 'Player B',
        score: 25000,
        liveDelta: 0,
        flowerCount: 0,
        wind: 'South',
        isDealer: false,
        isActive: false,
        isLocal: false,
        connected: true,
        ready: true,
        concealedCount: 13,
        meldCount: 0,
        melds: [],
        flowers: [],
        statusText: '对局中',
        absoluteSeat: 1,
      },
    ],
    actions: [{ id: 'discard', label: '出牌', enabled: true, emphasis: 'high' }],
    waitingControls: null,
    discards: { bottom: [], left: [], top: [], right: [] },
    localHand: [
      { tileId: 'w1#1', code: 'w1', isSelected: false, isDrawn: false, isFlower: false },
      { tileId: 'w2#2', code: 'w2', isSelected: false, isDrawn: false, isFlower: false },
    ],
    readyHandInsight: null,
    claimCandidates: [],
    drawnTileId: null,
    centerBanner: null,
    promptText: null,
    promptCue: null,
    result: null,
    settlementHands: null,
    lastDiscard: null,
    lastDiscardSeat: null,
    shouldAutoReturnLastDiscardToRiver: false,
    actionEffect: null,
    toasts: [],
    ...overrides,
  };
}

function createSelectedRuntime(skillId: string, rarity: SkillRarity = 'rare'): SkillRuntimeState {
  return {
    decisionsByCycle: {
      'east-1': {
        cycleKey: 'east-1',
        cycleLabel: '东1~东2局',
        deadlineAt: '2099-03-30T12:10:40+08:00',
        options: [{ skillId, rarity }],
        status: 'selected',
        selectedSkillId: skillId,
        selectedRarity: rarity,
        usedRoundIds: [],
      },
    },
    activation: null,
  };
}

describe('skillSystem', () => {
  it('opens a two-card skill offer during odd rounds before flower replacement', () => {
    vi.spyOn(Math, 'random').mockReturnValue(0);

    const runtime = syncSkillRuntimeWithSession(createInitialSkillRuntimeState(), createSessionState());
    const decision = runtime.decisionsByCycle['east-1'];

    expect(decision).toBeDefined();
    expect(decision?.status).toBe('pending');
    expect(decision?.options).toHaveLength(2);
  });

  it('keeps the selected skill active through the paired even round and injects the active-skill button', () => {
    const sessionState = createSessionState({
      roomSnapshot: {
        type: 'room_snapshot',
        payload: {
          ...createSessionState().roomSnapshot!.payload,
          match_state: {
            ...createSessionState().roomSnapshot!.payload.match_state!,
            hand_number: 2,
          },
          private_state: {
            ...createSessionState().roomSnapshot!.payload.private_state!,
            round_id: 'round-2',
            pending_action: {
              type: 'active_turn',
              seat_index: 0,
              deadline_at: '2099-03-30T12:10:40+08:00',
              options: ['discard'],
            },
          },
        },
      },
    });
    const runtime = createSelectedRuntime('08');

    const viewModel = createSkillEnhancedBattleViewModel(createBattleViewModel(), sessionState, runtime);
    const localPlayer = viewModel.players.find((player) => player.isLocal);

    expect(localPlayer?.skill?.name).toBe('暗度陈仓');
    expect(localPlayer?.skill?.remainingRounds).toBe(1);
    expect(viewModel.actions.some((action) => action.id === 'activate_skill' && action.enabled)).toBe(true);
  });

  it('marks an active skill as used for the current round after confirmation', () => {
    const sessionState = createSessionState({
      roomSnapshot: {
        type: 'room_snapshot',
        payload: {
          ...createSessionState().roomSnapshot!.payload,
          private_state: {
            ...createSessionState().roomSnapshot!.payload.private_state!,
            pending_action: {
              type: 'active_turn',
              seat_index: 0,
              deadline_at: '2099-03-30T12:10:40+08:00',
              options: ['discard'],
            },
          },
        },
      },
    });
    const openedRuntime = openSkillActivation(createSelectedRuntime('07'), sessionState);
    const targetedRuntime = {
      ...openedRuntime,
      activation: openedRuntime.activation
        ? {
            ...openedRuntime.activation,
            selectedTileId: 'w1#1',
          }
        : null,
    };

    const confirmedRuntime = confirmSkillActivation(targetedRuntime, sessionState);
    const viewModel = createSkillEnhancedBattleViewModel(createBattleViewModel(), sessionState, confirmedRuntime);

    expect(confirmedRuntime.decisionsByCycle['east-1']?.usedRoundIds).toContain('round-1');
    expect(viewModel.actions.some((action) => action.id === 'activate_skill' && !action.enabled)).toBe(true);
  });
});
