import { describe, expect, it } from 'vitest';

import type { BattleViewModel, SessionState } from '../types/match';
import {
  buildSkillActivationRequest,
  closeSkillActivation,
  createInitialSkillRuntimeState,
  createSkillEnhancedBattleViewModel,
  openSkillActivation,
  syncSkillRuntimeWithSession,
  updateSkillActivationSelection,
} from './skillSystem';

function createSessionState(): SessionState {
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
            type: 'active_turn',
            seat_index: 0,
            deadline_at: '2099-03-30T12:10:40+08:00',
            options: ['discard'],
          },
          skill_draft: {
            cycle_key: 'east-1',
            cycle_label: '东1~东2局',
            deadline_at: '2099-03-30T12:10:40+08:00',
            title: '东1~东2局 · 技能签启',
            detail: '技能二选一',
            options: [
              {
                skill_id: 'an_du_chen_cang',
                serial: '08',
                name: '暗度陈仓',
                rarity: 'rare',
                rarity_label: '稀有',
                tone: 'azure',
                type: 'active',
                type_label: '主动技能',
                interaction_kind: 'select_target',
                summary: '查看一名对手的情报。',
                detail: '稀有效果：查看一名对手的情报。',
                interaction_hint: '选择一名目标牌手。',
                tags: ['信息', '目标'],
                remaining_rounds: 2,
                remaining_activations_this_round: 1,
                can_activate_now: false,
              },
            ],
          },
          equipped_skills: [
            {
              skill_id: 'an_du_chen_cang',
              serial: '08',
              name: '暗度陈仓',
              rarity: 'rare',
              rarity_label: '稀有',
              tone: 'azure',
              type: 'active',
              type_label: '主动技能',
              interaction_kind: 'select_target',
              summary: '查看一名对手的情报。',
              detail: '稀有效果：查看一名对手的情报。',
              interaction_hint: '选择一名目标牌手。',
              tags: ['信息', '目标'],
              remaining_rounds: 2,
              remaining_activations_this_round: 1,
              can_activate_now: true,
            },
          ],
          players: [
            {
              seat_index: 0,
              nickname: 'Player A',
              connected: true,
              concealed_count: 2,
              concealed_tiles: [
                { tile_id: 'w1#1', tile_key: 'w1' },
                { tile_id: 'w2#2', tile_key: 'w2' },
              ],
              melds: [],
              flowers: [],
              discards: [],
              equipped_skill: null,
            },
            {
              seat_index: 1,
              nickname: 'Player B',
              connected: true,
              concealed_count: 13,
              melds: [],
              flowers: [],
              discards: [],
              equipped_skill: null,
            },
            {
              seat_index: 2,
              nickname: 'Player C',
              connected: true,
              concealed_count: 13,
              melds: [],
              flowers: [],
              discards: [],
              equipped_skill: null,
            },
            {
              seat_index: 3,
              nickname: 'Player D',
              connected: true,
              concealed_count: 13,
              melds: [],
              flowers: [],
              discards: [],
              equipped_skill: null,
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
  };
}

function createBattleViewModel(): BattleViewModel {
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
        absoluteSeat: 0,
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
        concealedCount: 2,
        meldCount: 0,
        melds: [],
        flowers: [],
      },
      {
        seat: 'right',
        absoluteSeat: 1,
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
    quickChatEvent: null,
    skillSelection: {
      cycleKey: 'east-1',
      cycleLabel: '东1~东2局',
      deadlineAt: '2099-03-30T12:10:40+08:00',
      title: '东1~东2局 · 技能签启',
      detail: '技能二选一',
      options: [],
    },
    skillActivation: null,
    toasts: [],
  };
}

describe('skillSystem', () => {
  it('clears activation when the backend enters skill draft selection', () => {
    const sessionState = createSessionState();
    const runtime = openSkillActivation(createInitialSkillRuntimeState(), {
      ...sessionState,
      roomSnapshot: {
        ...sessionState.roomSnapshot!,
        payload: {
          ...sessionState.roomSnapshot!.payload,
          private_state: {
            ...sessionState.roomSnapshot!.payload.private_state!,
            skill_draft: null,
          },
        },
      },
    });

    const synced = syncSkillRuntimeWithSession(runtime, sessionState);

    expect(synced.activation).toBeNull();
  });

  it('injects the active-skill button when the backend says the skill can activate now', () => {
    const viewModel = createSkillEnhancedBattleViewModel(
      {
        ...createBattleViewModel(),
        skillSelection: null,
      },
      {
        ...createSessionState(),
        roomSnapshot: {
          ...createSessionState().roomSnapshot!,
          payload: {
            ...createSessionState().roomSnapshot!.payload,
            private_state: {
              ...createSessionState().roomSnapshot!.payload.private_state!,
              skill_draft: null,
            },
          },
        },
      },
      createInitialSkillRuntimeState(),
    );

    expect(viewModel.actions.some((action) => action.id === 'activate_skill' && action.enabled)).toBe(true);
  });

  it('builds a skill activation request from the selected target', () => {
    const sessionState = {
      ...createSessionState(),
      roomSnapshot: {
        ...createSessionState().roomSnapshot!,
        payload: {
          ...createSessionState().roomSnapshot!.payload,
          private_state: {
            ...createSessionState().roomSnapshot!.payload.private_state!,
            skill_draft: null,
          },
        },
      },
    };
    const opened = openSkillActivation(createInitialSkillRuntimeState(), sessionState);
    const selected = updateSkillActivationSelection(opened, {
      selectedTargetSeat: 1,
    });

    expect(buildSkillActivationRequest(selected)).toEqual({
      actionType: 'skill:an_du_chen_cang',
      tileIds: ['seat:1'],
    });
    expect(closeSkillActivation(selected).activation).toBeNull();
  });
});
