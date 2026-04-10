import type { ComponentProps } from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { BattleViewModel } from '../../types/match';
import { BattleScreen } from './BattleScreen';

function setViewportSize(width: number, height: number) {
  Object.defineProperty(window, 'innerWidth', {
    configurable: true,
    writable: true,
    value: width,
  });
  Object.defineProperty(window, 'innerHeight', {
    configurable: true,
    writable: true,
    value: height,
  });
  window.dispatchEvent(new Event('resize'));
}

function createBattleViewModel(overrides: Partial<BattleViewModel> = {}): BattleViewModel {
  return {
    roomMode: 'normal',
    mode: 'watching',
    tableCode: 'AB12CD',
    canLeaveTable: false,
    phaseLabel: 'playing',
    roundLabel: 'round-123',
    scoreSummaryLabel: '总分 12',
    deadlineAt: null,
    topStatusLabel: 'Live Match',
    activePlayerSeat: 'bottom',
    actionIndicatorSeat: null,
    isActionDockElevated: false,
    players: [
      {
        seat: 'top',
        name: 'Player Top',
        score: 26800,
        liveDelta: 0,
        flowerCount: 0,
        wind: 'North',
        isDealer: false,
        isActive: false,
        isLocal: false,
        connected: true,
        ready: true,
        concealedCount: 13,
        meldCount: 0,
        melds: [],
        flowers: [],
        statusText: 'Live',
      },
      {
        seat: 'left',
        name: 'Player Left',
        score: 24300,
        liveDelta: -8,
        flowerCount: 1,
        wind: 'West',
        isDealer: false,
        isActive: false,
        isLocal: false,
        connected: true,
        ready: true,
        concealedCount: 13,
        meldCount: 1,
        melds: [['b2', 'b3', 'b4']],
        flowers: ['f1'],
        statusText: 'Live',
      },
      {
        seat: 'bottom',
        name: 'Player A',
        score: 25000,
        liveDelta: 8,
        flowerCount: 0,
        wind: 'East',
        isDealer: true,
        isActive: true,
        isLocal: true,
        connected: true,
        ready: true,
        concealedCount: 14,
        meldCount: 0,
        melds: [['w3', 'w3', 'w3']],
        flowers: [],
        statusText: 'Live',
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
        meldCount: 1,
        melds: [['c7', 'c8', 'c9']],
        flowers: [],
        statusText: 'Live',
      },
    ],
    actions: [],
    waitingControls: null,
    discards: {
      bottom: ['w1'],
      left: [],
      top: [],
      right: ['b4'],
    },
    localHand: [
      { tileId: 'w1#1', code: 'w1', isSelected: false, isDrawn: false, isFlower: false },
      { tileId: 'w2#2', code: 'w2', isSelected: true, isDrawn: true, isFlower: false },
    ],
    readyHandInsight: null,
    claimCandidates: [],
    drawnTileId: 'w2#2',
    centerBanner: 'Opponent Turn',
    centerStatusText: null,
    promptText: null,
    promptCue: null,
    result: null,
    settlementHands: null,
    lastDiscard: 'b4',
    lastDiscardSeat: 'left',
    shouldAutoReturnLastDiscardToRiver: false,
    actionEffect: null,
    toasts: [],
    ...overrides,
  };
}

function renderBattleScreen(viewModel: BattleViewModel, overrides?: Partial<ComponentProps<typeof BattleScreen>>) {
  setViewportSize(1720, 900);

  return render(
    <BattleScreen
      viewModel={viewModel}
      themeId="tian-shui-bi"
      themeLabel="天水碧"
      onCycleTheme={vi.fn()}
      onAction={vi.fn()}
      onTileSelect={vi.fn()}
      onTileDoubleClick={vi.fn()}
      onClaimCandidateSelect={vi.fn()}
      onClaimCandidateActivate={vi.fn()}
      onCopyTableCode={vi.fn()}
      onLeaveTable={vi.fn()}
      {...overrides}
    />,
  );
}

function mockResultOverlayScrollLayout({ panelHeight }: { panelHeight: number }) {
  const scorePanel = document.body.querySelector('.result-overlay__score-panel') as HTMLElement | null;
  const fanPanel = document.body.querySelector('.result-overlay__fan-panel') as HTMLElement | null;
  const fanViewport = document.body.querySelector('.result-overlay__fan-list-viewport') as HTMLElement | null;

  if (!scorePanel || !fanPanel || !fanViewport) {
    throw new Error('result overlay scroll nodes are missing');
  }

  Object.defineProperty(scorePanel, 'getBoundingClientRect', {
    configurable: true,
    value: () => ({
      width: 360,
      height: panelHeight,
      top: 0,
      right: 360,
      bottom: panelHeight,
      left: 0,
      x: 0,
      y: 0,
      toJSON: () => '',
    }),
  });

  act(() => {
    window.dispatchEvent(new Event('resize'));
  });

  return { fanPanel, fanViewport };
}

function getVisibleFanList() {
  const fanViewport = document.body.querySelector('.result-overlay__fan-list-viewport') as HTMLElement | null;

  if (!fanViewport) {
    throw new Error('visible fan list is missing');
  }

  return within(fanViewport);
}

describe('BattleScreen', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('shows ready and start controls in waiting state', () => {
    renderBattleScreen(
      createBattleViewModel({
        mode: 'disconnected_or_waiting',
        phaseLabel: 'waiting',
        waitingControls: {
          canReady: true,
          canStart: true,
          isReady: false,
          occupiedSeats: 4,
          botCount: 0,
          canAddBot: false,
          canRemoveBot: false,
        },
        actions: [
          { id: 'ready', label: 'Ready', enabled: true, emphasis: 'medium' },
          { id: 'start_match', label: 'Start Match', enabled: true, emphasis: 'high' },
        ],
      }),
    );

    expect(screen.getByRole('button', { name: /ready/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /start match/i })).toBeInTheDocument();
    expect(screen.queryByLabelText('牌桌侧边面板')).toBeNull();
  });

  it('renders the skill selection overlay and forwards choice callbacks', async () => {
    const user = userEvent.setup();
    const onSkillSelect = vi.fn();
    const onSkillDecline = vi.fn();

    renderBattleScreen(
      createBattleViewModel({
        skillSelection: {
          cycleKey: 'east-1',
          cycleLabel: '东1~东2局',
          deadlineAt: '2099-03-30T12:10:40+08:00',
          title: '东1~东2局 · 技能签启',
          detail: '每种技能持续两局；主动技能每局可按技能品质发动对应次数。',
          options: [
            {
              cycleKey: 'east-1',
              skillId: '07',
              name: '无中生有',
              rarity: 'rare',
              rarityLabel: '稀有',
              tone: 'azure',
              type: 'active',
              typeLabel: '主动技能',
              summary: '选择一张手牌发动置换。',
              detail: '稀有效果：成功+5 / 失败-3',
              interactionHint: '发动时从当前手牌中点选一张。',
              tags: ['功能', '手牌'],
              cycleLabel: '东1~东2局',
              remainingRounds: 2,
              remainingActivationsThisRound: 1,
            },
            {
              cycleKey: 'east-1',
              skillId: '14',
              name: '借尸还魂',
              rarity: 'common',
              rarityLabel: '普通',
              tone: 'jade',
              type: 'passive',
              typeLabel: '被动技能',
              summary: '胡熟张加分；胡绝对生张扣分。',
              detail: '普通效果：+2 / -1',
              interactionHint: null,
              tags: ['熟张', '读牌'],
              cycleLabel: '东1~东2局',
              remainingRounds: 2,
              remainingActivationsThisRound: 0,
            },
          ],
        },
      }),
      {
        onSkillSelect,
        onSkillDecline,
      },
    );

    expect(screen.getByRole('dialog', { name: '东1~东2局 · 技能签启' })).toBeInTheDocument();

    await user.click(screen.getAllByRole('button', { name: '选择此技能' })[0]);
    await user.click(screen.getByRole('button', { name: '不需要技能' }));

    expect(onSkillSelect).toHaveBeenCalledWith('07');
    expect(onSkillDecline).toHaveBeenCalledTimes(1);
  });

  it('shows only tile faces in hand-tile skill activation choices', () => {
    renderBattleScreen(
      createBattleViewModel({
        skillActivation: {
          skill: {
            skillId: '07',
            name: '无中生有',
            rarity: 'rare',
            rarityLabel: '稀有',
            tone: 'azure',
            type: 'active',
            typeLabel: '主动技能',
            summary: '选择一张手牌发动置换。',
            detail: '稀有效果：成功+5 / 失败-3',
            interactionKind: 'select_hand_tile',
            interactionHint: '发动时从当前手牌中点选一张。',
            tags: ['功能', '手牌'],
            cycleLabel: '东1~东2局',
            remainingRounds: 2,
            remainingActivationsThisRound: 1,
          },
          kind: 'select_hand_tile',
          title: '无中生有 · 发动技能',
          description: '发动时从当前手牌中点选一张。',
          confirmLabel: '发动技能',
          canConfirm: false,
          handChoices: [
            { tileId: 'w1#1', code: 'w1', selected: false },
            { tileId: 'w2#2', code: 'w2', selected: true },
          ],
        },
      }),
      {
        onCloseSkillActivation: vi.fn(),
        onConfirmSkillActivation: vi.fn(),
        onSkillActivationTargetSelect: vi.fn(),
        onSkillActivationTileSelect: vi.fn(),
        onSkillActivationMeldSelect: vi.fn(),
      },
    );

    expect(screen.getByRole('dialog', { name: '无中生有 · 发动技能' })).toBeInTheDocument();
    expect(screen.queryByText('w1#1')).toBeNull();
    expect(screen.queryByText('w2#2')).toBeNull();
    expect(screen.queryByText('w1')).toBeNull();
    expect(screen.queryByText('w2')).toBeNull();
    expect(document.querySelectorAll('.skill-activation-dialog__tile-choice .mahjong-tile')).toHaveLength(2);
  });

  it('shows the local player skill tooltip after hovering the info bar for half a second', async () => {
    vi.useFakeTimers();

    renderBattleScreen(
      createBattleViewModel({
        players: createBattleViewModel().players.map((player) =>
          player.isLocal
            ? {
                ...player,
                skill: {
                  skillId: '08',
                  name: '暗度陈仓',
                  rarity: 'rare',
                  rarityLabel: '稀有',
                  tone: 'azure',
                  type: 'active',
                  typeLabel: '主动技能',
                  summary: '在自己回合指定一名对手，申请侦察其当前手牌情报。',
                  detail: '稀有效果：看2张 / 立即扣3',
                  interactionHint: '发动时从其他三家中选择一名作为侦察目标。',
                  tags: ['信息', '目标'],
                  cycleLabel: '东1~东2局',
                  remainingRounds: 2,
                  remainingActivationsThisRound: 1,
                },
              }
            : player,
        ),
      }),
    );

    fireEvent.mouseEnter(screen.getByRole('button', { name: '打开Player A的快捷表情' }));
    await act(async () => {
      vi.advanceTimersByTime(500);
    });

    expect(screen.getByText('暗度陈仓')).toBeInTheDocument();
    expect(screen.getByText('稀有效果：看2张 / 立即扣3')).toBeInTheDocument();
    expect(screen.getByRole('tooltip')).toHaveClass('table-stage__skill-tooltip--seat-bottom');
  });

  it('shows a visible knowledge dialog after a skill reveals opponent tiles', async () => {
    const user = userEvent.setup();

    renderBattleScreen(
      {
        ...createBattleViewModel(),
        skillKnowledge: {
          key: 'an-du-preview-1',
          title: '暗度陈仓',
          skillName: '暗度陈仓',
          targetName: 'Player B',
          detail: '已查看 Player B 的 2 张手牌',
          tileCodes: ['w8', 't3'],
        },
      } as BattleViewModel,
    );

    const dialog = screen.getByRole('dialog', { name: '暗度陈仓 · 情报' });
    expect(dialog).toBeInTheDocument();
    expect(within(dialog).getByText('Player B')).toBeInTheDocument();
    expect(within(dialog).getByText('已查看 Player B 的 2 张手牌')).toBeInTheDocument();
    expect(within(screen.getByLabelText('暗度陈仓 查看到的牌')).getAllByTestId('mahjong-tile')).toHaveLength(2);

    await user.click(screen.getByRole('button', { name: '关闭情报浮窗' }));
    expect(screen.queryByRole('dialog', { name: '暗度陈仓 · 情报' })).toBeNull();
  });

  it('shows settlement breakdown in resolving state', () => {
    renderBattleScreen(
      createBattleViewModel({
        mode: 'resolving',
        phaseLabel: 'settlement',
        result: {
          title: '本局结算',
          summary: '等待下一局',
          fanTotal: 8,
          winnerSeat: 'right',
          discarderSeat: 'left',
          winType: 'discard',
          winTypeLabel: '荣和',
          provisional: true,
          flowerCount: 0,
          fanBreakdown: [
            { fanKey: 'ping_hu', fanValue: 8 },
            { fanKey: 'self_draw', fanValue: 1 },
            { fanKey: 'full_flush', fanValue: 24 },
            { fanKey: 'pung_of_terminals_or_honours', fanValue: 1 },
            { fanKey: 'seven_pairs', fanValue: 24 },
          ],
          scoreDeltaBySeat: {
            bottom: 0,
            left: -8,
            right: 8,
          },
          seats: [
            { seat: 'right', name: 'Player B', score: 25008, delta: 8 },
            { seat: 'left', name: 'Player Left', score: 24292, delta: -8 },
          ],
          continueAction: {
            id: 'start_next_round',
            label: '下一局',
            enabled: true,
          },
        },
      }),
    );

    expect(screen.getByText('8 番')).toBeInTheDocument();
    const visibleFanList = getVisibleFanList();

    expect(visibleFanList.getByText('平胡')).toBeInTheDocument();
    expect(visibleFanList.getByText('清一色')).toBeInTheDocument();
    expect(screen.getByText(/胜者 Player B（右家）/)).toBeInTheDocument();
    expect(screen.getByText(/放铳 Player Left（左家）/)).toBeInTheDocument();
    expect(screen.queryByText(/胜者 right/)).not.toBeInTheDocument();
    expect(screen.queryByText(/放铳 left/)).not.toBeInTheDocument();
    expect(visibleFanList.getByText('幺九刻')).toBeInTheDocument();
    expect(visibleFanList.getByText('七对')).toBeInTheDocument();
    expect(screen.queryByText('当前为临时结算结果')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '展开剩余 2 项番种' })).toBeNull();
  });

  it('shows the matching fan guide tooltip after hovering a settlement fan row for 0.5s', () => {
    vi.useFakeTimers();

    renderBattleScreen(
      createBattleViewModel({
        mode: 'resolving',
        phaseLabel: 'settlement',
        result: {
          title: '本局结算',
          summary: '等待下一局',
          fanTotal: 8,
          winnerSeat: 'bottom',
          discarderSeat: null,
          winType: 'self_draw',
          winTypeLabel: '自摸',
          provisional: false,
          flowerCount: 0,
          fanBreakdown: [
            { fanKey: 'ping_hu', fanValue: 8 },
            { fanKey: 'self_draw', fanValue: 1 },
          ],
          scoreDeltaBySeat: {
            bottom: 8,
            left: -3,
            top: -3,
            right: -2,
          },
          seats: [
            { seat: 'bottom', name: 'Player A', score: 25008, delta: 8 },
            { seat: 'left', name: 'Player Left', score: 24297, delta: -3 },
          ],
          continueAction: {
            id: 'start_next_round',
            label: '下一局',
            enabled: true,
          },
        },
      }),
    );

    const pingHuRow = screen.getByText('平胡').closest('.result-overlay__row');
    expect(pingHuRow).not.toBeNull();

    fireEvent.mouseEnter(pingHuRow!);

    act(() => {
      vi.advanceTimersByTime(499);
    });

    expect(screen.queryByRole('tooltip')).toBeNull();

    act(() => {
      vi.advanceTimersByTime(1);
    });

    expect(screen.getByRole('tooltip', { name: '平和番型说明' })).toBeInTheDocument();
    expect(screen.getByText('和牌由四副顺子和一对将组成，不含任何刻子。')).toBeInTheDocument();

    fireEvent.mouseLeave(pingHuRow!);

    act(() => {
      vi.advanceTimersByTime(120);
    });

    expect(screen.queryByRole('tooltip')).toBeNull();
  });

  it('shows 屁和 in the settlement overlay when the backend provides the low-fan label', () => {
    renderBattleScreen(
      createBattleViewModel({
        mode: 'resolving',
        phaseLabel: 'settlement',
        result: {
          title: '本局结算',
          summary: '屁和，等待下一局',
          fanTotal: 4,
          winnerSeat: 'bottom',
          discarderSeat: null,
          winType: 'self_draw',
          winTypeLabel: '屁和',
          provisional: false,
          flowerCount: 0,
          fanBreakdown: [{ fanKey: 'self_draw', fanValue: 1 }],
          scoreDeltaBySeat: {
            bottom: 4,
            left: -2,
            top: -1,
            right: -1,
          },
          seats: [
            { seat: 'bottom', name: 'Player A', score: 25004, delta: 4 },
            { seat: 'left', name: 'Player Left', score: 24298, delta: -2 },
          ],
          continueAction: {
            id: 'start_next_round',
            label: '下一局',
            enabled: true,
          },
        },
      }),
    );

    expect(screen.getByText('屁和，等待下一局')).toBeInTheDocument();
    expect(screen.getByText('屁和 · 胜者 Player A（本家）')).toBeInTheDocument();
    expect(screen.queryByText('自摸 · 胜者 Player A（本家）')).toBeNull();
  });

  it('disables the continue button and shows confirmation progress after the local player confirms', () => {
    renderBattleScreen(
      createBattleViewModel({
        mode: 'resolving',
        phaseLabel: 'settlement',
        result: {
          title: '本局结算',
          summary: '等待下一局',
          fanTotal: 8,
          winnerSeat: 'bottom',
          discarderSeat: null,
          winType: 'self_draw',
          winTypeLabel: '自摸',
          provisional: false,
          flowerCount: 0,
          fanBreakdown: [{ fanKey: 'self_draw', fanValue: 1 }],
          scoreDeltaBySeat: {
            bottom: 8,
            left: -3,
            top: -3,
            right: -2,
          },
          seats: [
            { seat: 'bottom', name: 'Player A', score: 25008, delta: 8 },
            { seat: 'left', name: 'Player Left', score: 24297, delta: -3 },
          ],
          continueAction: {
            id: 'start_next_round',
            label: '已确认 2/3',
            enabled: false,
            confirmation: {
              confirmedCount: 2,
              requiredCount: 3,
              isLocalConfirmed: true,
            },
          },
        },
      }),
    );

    expect(screen.getByRole('button', { name: '已确认 2/3' })).toBeDisabled();
  });

  it('shows a live countdown instead of confirmation progress once all online players have confirmed', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-04-02T12:00:00Z'));

    renderBattleScreen(
      createBattleViewModel({
        mode: 'resolving',
        phaseLabel: 'settlement',
        result: {
          title: '本局结算',
          summary: '等待下一局',
          fanTotal: 8,
          winnerSeat: 'bottom',
          discarderSeat: null,
          winType: 'self_draw',
          winTypeLabel: '自摸',
          provisional: false,
          flowerCount: 0,
          fanBreakdown: [{ fanKey: 'self_draw', fanValue: 1 }],
          scoreDeltaBySeat: {
            bottom: 8,
            left: -3,
            top: -3,
            right: -2,
          },
          seats: [
            { seat: 'bottom', name: 'Player A', score: 25008, delta: 8 },
            { seat: 'left', name: 'Player Left', score: 24297, delta: -3 },
          ],
          continueAction: {
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
          },
        },
      }),
    );

    expect(screen.getByRole('button', { name: '60s后自动推进' })).toBeDisabled();

    act(() => {
      vi.advanceTimersByTime(1000);
    });

    expect(screen.getByRole('button', { name: '59s后自动推进' })).toBeDisabled();
    vi.useRealTimers();
  });

  it('shows all player scores directly without section expansion controls', () => {
    renderBattleScreen(
      createBattleViewModel({
        mode: 'resolving',
        phaseLabel: 'settlement',
        result: {
          title: '本局结算',
          summary: '等待下一局',
          fanTotal: 8,
          winnerSeat: 'bottom',
          discarderSeat: null,
          winType: 'self_draw',
          winTypeLabel: '自摸',
          provisional: false,
          flowerCount: 0,
          fanBreakdown: [{ fanKey: 'self_draw', fanValue: 1 }],
          scoreDeltaBySeat: {
            bottom: 8,
            left: -3,
            top: -3,
            right: -2,
          },
          seats: [
            { seat: 'right', name: 'Player B', score: 24998, delta: -2 },
            { seat: 'bottom', name: 'Player A', score: 25008, delta: 8 },
            { seat: 'top', name: 'Player Top', score: 26797, delta: -3 },
            { seat: 'left', name: 'Player Left', score: 24297, delta: -3 },
          ],
          continueAction: {
            id: 'start_next_round',
            label: '下一局',
            enabled: true,
          },
        },
      }),
    );

    const scorePanel = document.body.querySelector('.result-overlay__score-panel') as HTMLElement;

    expect(within(scorePanel).getByText('Player A')).toBeInTheDocument();
    expect(within(scorePanel).getByText('Player Left')).toBeInTheDocument();
    expect(within(scorePanel).getByText('Player Top')).toBeInTheDocument();
    expect(within(scorePanel).getByText('Player B')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '展开其他玩家分数（3）' })).toBeNull();
    expect(screen.queryByRole('button', { name: '收起其他玩家分数' })).toBeNull();
  });

  it('shows a player statistics tooltip with score trend and win rate when hovering a score row', () => {
    vi.useFakeTimers();

    renderBattleScreen(
      createBattleViewModel({
        mode: 'resolving',
        phaseLabel: 'settlement',
        result: {
          title: '本局结算',
          summary: '等待下一局',
          fanTotal: 8,
          winnerSeat: 'bottom',
          discarderSeat: null,
          winType: 'self_draw',
          winTypeLabel: '自摸',
          provisional: false,
          flowerCount: 0,
          fanBreakdown: [{ fanKey: 'self_draw', fanValue: 1 }],
          scoreDeltaBySeat: {
            bottom: 8,
            left: -3,
            top: -3,
            right: -2,
          },
          seats: [
            {
              seat: 'bottom',
              name: 'Player A',
              score: 25008,
              delta: 8,
              stats: {
                scoreHistory: [0, 12, 6, 18, 8],
                winCount: 2,
                completedRoundCount: 4,
                winRate: 0.5,
              },
            },
            { seat: 'left', name: 'Player Left', score: 24297, delta: -3 },
          ],
          continueAction: {
            id: 'start_next_round',
            label: '下一局',
            enabled: true,
          },
        },
      }),
    );

    const scorePanel = document.body.querySelector('.result-overlay__score-panel') as HTMLElement;
    const playerRow = within(scorePanel).getByText('Player A').closest('.result-overlay__seat-row');
    expect(playerRow).not.toBeNull();

    fireEvent.mouseEnter(playerRow!);

    expect(screen.queryByRole('tooltip', { name: 'Player A 战绩统计' })).toBeNull();

    act(() => {
      vi.advanceTimersByTime(499);
    });

    expect(screen.queryByRole('tooltip', { name: 'Player A 战绩统计' })).toBeNull();

    act(() => {
      vi.advanceTimersByTime(1);
    });

    expect(screen.getByRole('tooltip', { name: 'Player A 战绩统计' })).toBeInTheDocument();
    expect(screen.getByText('50%')).toBeInTheDocument();
    expect(screen.getByText('2/4')).toBeInTheDocument();
    expect(screen.getByRole('img', { name: 'Player A 本牌局战绩折线图' })).toBeInTheDocument();

    fireEvent.mouseLeave(playerRow!);

    act(() => {
      vi.advanceTimersByTime(90);
    });

    expect(screen.queryByRole('tooltip', { name: 'Player A 战绩统计' })).toBeNull();
    vi.useRealTimers();
  });

  it('renders side-by-side fan and score panels without per-section expand buttons', () => {
    renderBattleScreen(
      createBattleViewModel({
        mode: 'resolving',
        phaseLabel: 'settlement',
        result: {
          title: '本局结算',
          summary: '等待下一局',
          fanTotal: 16,
          winnerSeat: 'bottom',
          discarderSeat: null,
          winType: 'self_draw',
          winTypeLabel: '自摸',
          provisional: false,
          flowerCount: 2,
          fanBreakdown: [
            { fanKey: 'ping_hu', fanValue: 2 },
            { fanKey: 'self_draw', fanValue: 1 },
            { fanKey: 'full_flush', fanValue: 6 },
            { fanKey: 'all_pungs', fanValue: 2 },
            { fanKey: 'seven_pairs', fanValue: 3 },
          ],
          scoreDeltaBySeat: {
            bottom: 16,
            left: -6,
            top: -5,
            right: -5,
          },
          seats: [
            { seat: 'bottom', name: 'Player A', score: 25016, delta: 16 },
            { seat: 'left', name: 'Player Left', score: 24394, delta: -6 },
          ],
          continueAction: {
            id: 'start_next_round',
            label: '下一局',
            enabled: true,
          },
        },
      }),
    );

    expect(getVisibleFanList().getByText('七对')).toBeInTheDocument();
    expect(document.body.querySelector('.result-overlay__columns')).not.toBeNull();
    expect(document.body.querySelector('.result-overlay__fan-list')).not.toBeNull();
    expect(screen.queryByRole('button', { name: '展开剩余 2 项番种' })).toBeNull();
    expect(screen.queryByRole('button', { name: '收起番种' })).toBeNull();
  });

  it('locks the fan panel to the score panel height and renders overflowing fan rows in a scroll area', async () => {
    renderBattleScreen(
      createBattleViewModel({
        mode: 'resolving',
        phaseLabel: 'settlement',
        result: {
          title: '本局结算',
          summary: '等待下一局',
          fanTotal: 24,
          winnerSeat: 'bottom',
          discarderSeat: null,
          winType: 'self_draw',
          winTypeLabel: '自摸',
          provisional: false,
          flowerCount: 2,
          fanBreakdown: [
            { fanKey: 'ping_hu', fanValue: 2 },
            { fanKey: 'self_draw', fanValue: 1 },
            { fanKey: 'full_flush', fanValue: 6 },
            { fanKey: 'all_pungs', fanValue: 2 },
            { fanKey: 'seven_pairs', fanValue: 3 },
            { fanKey: 'three_concealed_pungs', fanValue: 2 },
            { fanKey: 'all_simples', fanValue: 1 },
          ],
          scoreDeltaBySeat: {
            bottom: 24,
            left: -8,
            top: -8,
            right: -8,
          },
          seats: [
            { seat: 'bottom', name: 'Player A', score: 25024, delta: 24 },
            { seat: 'left', name: 'Player Left', score: 24292, delta: -8 },
            { seat: 'top', name: 'Player Top', score: 26792, delta: -8 },
            { seat: 'right', name: 'Player B', score: 24992, delta: -8 },
          ],
          continueAction: {
            id: 'start_next_round',
            label: '下一局',
            enabled: true,
          },
        },
      }),
    );

    const { fanPanel, fanViewport } = mockResultOverlayScrollLayout({ panelHeight: 420 });

    await waitFor(() => {
      expect(fanPanel.style.height).toBe('420px');
    });

    const visibleFanList = getVisibleFanList();

    expect(visibleFanList.getByText('平胡')).toBeInTheDocument();
    expect(visibleFanList.getByText('自摸')).toBeInTheDocument();
    expect(visibleFanList.getByText('清一色')).toBeInTheDocument();
    expect(visibleFanList.getByText('对对胡')).toBeInTheDocument();
    expect(visibleFanList.getByText('七对')).toBeInTheDocument();
    expect(visibleFanList.getByText('三暗刻')).toBeInTheDocument();
    expect(visibleFanList.getByText('断幺')).toBeInTheDocument();
    expect(screen.queryByRole('group', { name: '番型明细分页' })).toBeNull();
    expect(screen.queryByRole('button', { name: '上一页' })).toBeNull();
    expect(screen.queryByRole('button', { name: '下一页' })).toBeNull();
    expect(fanViewport.className).toContain('result-overlay__fan-list-viewport');
  });

  it('updates the scrollable fan list without reintroducing pagination when a new settlement result arrives', async () => {
    const { rerender } = renderBattleScreen(
      createBattleViewModel({
        mode: 'resolving',
        phaseLabel: 'settlement',
        result: {
          title: '本局结算',
          summary: '等待下一局',
          fanTotal: 18,
          winnerSeat: 'bottom',
          discarderSeat: null,
          winType: 'self_draw',
          winTypeLabel: '自摸',
          provisional: false,
          flowerCount: 0,
          fanBreakdown: [
            { fanKey: 'ping_hu', fanValue: 2 },
            { fanKey: 'self_draw', fanValue: 1 },
            { fanKey: 'full_flush', fanValue: 6 },
            { fanKey: 'all_pungs', fanValue: 2 },
            { fanKey: 'seven_pairs', fanValue: 3 },
            { fanKey: 'three_concealed_pungs', fanValue: 2 },
          ],
          scoreDeltaBySeat: {
            bottom: 18,
            left: -6,
            top: -6,
            right: -6,
          },
          seats: [
            { seat: 'bottom', name: 'Player A', score: 25018, delta: 18 },
            { seat: 'left', name: 'Player Left', score: 24294, delta: -6 },
            { seat: 'top', name: 'Player Top', score: 26794, delta: -6 },
            { seat: 'right', name: 'Player B', score: 24994, delta: -6 },
          ],
          continueAction: {
            id: 'start_next_round',
            label: '下一局',
            enabled: true,
          },
        },
      }),
    );

    mockResultOverlayScrollLayout({ panelHeight: 420 });

    rerender(
      <BattleScreen
        viewModel={createBattleViewModel({
          mode: 'resolving',
          phaseLabel: 'settlement',
          result: {
            title: '本局结算',
            summary: '新的结算结果',
            fanTotal: 6,
            winnerSeat: 'right',
            discarderSeat: 'left',
            winType: 'discard',
            winTypeLabel: '荣和',
            provisional: false,
            flowerCount: 1,
            fanBreakdown: [
              { fanKey: 'mixed_double_chow', fanValue: 1 },
              { fanKey: 'short_straight', fanValue: 1 },
              { fanKey: 'double_pung', fanValue: 2 },
              { fanKey: 'dragon_pung', fanValue: 2 },
            ],
            scoreDeltaBySeat: {
              bottom: 0,
              left: -6,
              right: 6,
            },
            seats: [
              { seat: 'right', name: 'Player B', score: 25000, delta: 6 },
              { seat: 'left', name: 'Player Left', score: 24288, delta: -6 },
              { seat: 'bottom', name: 'Player A', score: 25000, delta: 0 },
            ],
            continueAction: {
              id: 'start_next_round',
              label: '下一局',
              enabled: true,
            },
          },
        })}
        themeId="tian-shui-bi"
        themeLabel="天水碧"
        onCycleTheme={vi.fn()}
        onAction={vi.fn()}
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onCopyTableCode={vi.fn()}
        onLeaveTable={vi.fn()}
      />,
    );

    mockResultOverlayScrollLayout({ panelHeight: 420 });

    const updatedFanList = getVisibleFanList();

    await waitFor(() => {
      expect(updatedFanList.getByText('喜相逢')).toBeInTheDocument();
    });

    expect(updatedFanList.getByText('连六')).toBeInTheDocument();
    expect(updatedFanList.getByText('双同刻')).toBeInTheDocument();
    expect(updatedFanList.getByText('箭刻')).toBeInTheDocument();
    expect(updatedFanList.queryByText('平胡')).toBeNull();
    expect(screen.queryByRole('group', { name: '番型明细分页' })).toBeNull();
  });

  it('can collapse the settlement panel and restore it from the table center', async () => {
    const user = userEvent.setup();

    renderBattleScreen(
      createBattleViewModel({
        mode: 'resolving',
        phaseLabel: 'settlement',
        settlementHands: {
          top: ['w1', 'w2'],
          left: ['b1', 'b2'],
          right: ['c1', 'c2'],
          bottom: ['d1', 'd2'],
        },
        result: {
          title: '本局结算',
          summary: '等待下一局',
          fanTotal: 8,
          winnerSeat: 'right',
          discarderSeat: 'left',
          winType: 'discard',
          winTypeLabel: '荣和',
          provisional: false,
          flowerCount: 0,
          fanBreakdown: [{ fanKey: 'ping_hu', fanValue: 8 }],
          scoreDeltaBySeat: {
            bottom: 0,
            left: -8,
            right: 8,
          },
          seats: [
            { seat: 'right', name: 'Player B', score: 25008, delta: 8 },
            { seat: 'left', name: 'Player Left', score: 24292, delta: -8 },
          ],
          continueAction: {
            id: 'start_next_round',
            label: '下一局',
            enabled: true,
          },
        },
      }),
    );

    expect(screen.getByRole('button', { name: '收起结算面板' })).toBeInTheDocument();
    expect(screen.getByLabelText('对家手牌')).toBeInTheDocument();
    expect(screen.queryByLabelText('本家手牌')).toBeNull();

    await user.click(screen.getByRole('button', { name: '收起结算面板' }));
    expect(screen.queryByText('本局结算')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: '展开结算面板' })).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: '展开结算面板' }));
    expect(screen.getByText('本局结算')).toBeInTheDocument();
  });

  it('holds settlement hands and the settlement panel until the hu callout has fully finished', () => {
    vi.useFakeTimers();

    const { rerender } = renderBattleScreen(
      createBattleViewModel({
        actionEffect: {
          key: 'hu-1',
          label: '和',
          emphasis: 'claim',
          seat: 'right',
          calloutTone: 'hu',
        },
        result: null,
        settlementHands: null,
      }),
    );

    expect(screen.getByText('和')).toBeInTheDocument();

    rerender(
      <BattleScreen
        viewModel={createBattleViewModel({
          mode: 'resolving',
          phaseLabel: 'settlement',
          actionEffect: {
            key: 'round-done-1',
            label: '结算',
            emphasis: 'system',
            seat: null,
            calloutTone: null,
          },
          settlementHands: {
            top: ['w1', 'w2'],
            left: ['b1', 'b2', 'b3', 'b4'],
            right: ['c1', 'c2'],
            bottom: ['d1', 'd2'],
          },
          result: {
            title: '本局结算',
            summary: '荣和，等待下一局',
            fanTotal: 8,
            winnerSeat: 'right',
            discarderSeat: 'left',
            winType: 'discard',
            winTypeLabel: '荣和',
            provisional: false,
            flowerCount: 0,
            fanBreakdown: [{ fanKey: 'ping_hu', fanValue: 8 }],
            scoreDeltaBySeat: {
              left: -8,
              right: 8,
            },
            seats: [
              { seat: 'right', name: 'Player B', score: 25008, delta: 8 },
              { seat: 'left', name: 'Player Left', score: 24292, delta: -8 },
            ],
            continueAction: {
              id: 'start_next_round',
              label: '下一局',
              enabled: true,
            },
          },
        })}
        themeId="tian-shui-bi"
        themeLabel="天水碧"
        onCycleTheme={vi.fn()}
        onAction={vi.fn()}
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onCopyTableCode={vi.fn()}
        onLeaveTable={vi.fn()}
      />,
    );

    expect(screen.getByText('和')).toBeInTheDocument();
    expect(screen.queryByText('本局结算')).toBeNull();
    expect(screen.queryByLabelText('对家手牌')).toBeNull();
    expect(screen.queryByLabelText('左家手牌')).toBeNull();

    act(() => {
      vi.advanceTimersByTime(2999);
    });

    expect(screen.getByText('和')).toBeInTheDocument();
    expect(screen.queryByText('本局结算')).toBeNull();
    expect(screen.queryByLabelText('对家手牌')).toBeNull();

    act(() => {
      vi.advanceTimersByTime(1);
    });

    expect(screen.getByText('本局结算')).toBeInTheDocument();
    expect(screen.getByLabelText('对家手牌')).toBeInTheDocument();
    expect(screen.getByLabelText('左家手牌')).toBeInTheDocument();
    vi.useRealTimers();
  });

  it('waits to show the settlement panel until the final discard returns to the river when no response is needed', () => {
    vi.useFakeTimers();

    const { rerender } = renderBattleScreen(
      createBattleViewModel({
        result: null,
      }),
    );

    rerender(
      <BattleScreen
        viewModel={createBattleViewModel({
          mode: 'resolving',
          discards: {
            bottom: ['w1'],
            left: ['b4'],
            top: [],
            right: [],
          },
          lastDiscard: 'b4',
          lastDiscardSeat: 'left',
          result: {
            title: '本局结算',
            summary: '本局流局，等待下一局',
            fanTotal: null,
            winnerSeat: null,
            discarderSeat: null,
            winType: 'draw',
            winTypeLabel: '流局',
            provisional: false,
            flowerCount: 0,
            fanBreakdown: [],
            scoreDeltaBySeat: {
              bottom: 0,
              left: 0,
              top: 0,
              right: 0,
            },
            seats: [
              { seat: 'bottom', name: 'Player A', score: 25000, delta: 0 },
              { seat: 'left', name: 'Player Left', score: 24300, delta: 0 },
            ],
            continueAction: {
              id: 'start_next_round',
              label: '下一局',
              enabled: true,
            },
          },
        })}
        themeId="tian-shui-bi"
        themeLabel="天水碧"
        onCycleTheme={vi.fn()}
        onAction={vi.fn()}
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onCopyTableCode={vi.fn()}
        onLeaveTable={vi.fn()}
      />,
    );

    expect(screen.getByText('流局')).toBeInTheDocument();
    expect(screen.queryByText('本局结算')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Latest discard spotlight')).toBeNull();
    expect(document.body.querySelector('.table-stage__river-track--left')?.querySelectorAll('.mahjong-tile--discard')).toHaveLength(1);

    act(() => {
      vi.advanceTimersByTime(2999);
    });

    expect(screen.getByText('流局')).toBeInTheDocument();
    expect(screen.queryByText('本局结算')).not.toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(1);
    });

    expect(screen.getByText('本局结算')).toBeInTheDocument();
    expect(screen.queryByText('流局')).toBeNull();
    vi.useRealTimers();
  });

  it('does not replay the draw callout after the next round starts', () => {
    vi.useFakeTimers();

    const { rerender, container } = renderBattleScreen(
      createBattleViewModel({
        result: null,
      }),
    );

    rerender(
      <BattleScreen
        viewModel={createBattleViewModel({
          mode: 'resolving',
          discards: {
            bottom: ['w1'],
            left: ['b4'],
            top: [],
            right: [],
          },
          lastDiscard: 'b4',
          lastDiscardSeat: 'left',
          actionEffect: null,
          result: {
            title: '本局结算',
            summary: '本局流局，等待下一局',
            fanTotal: null,
            winnerSeat: null,
            discarderSeat: null,
            winType: 'draw',
            winTypeLabel: '流局',
            provisional: false,
            flowerCount: 0,
            fanBreakdown: [],
            scoreDeltaBySeat: {
              bottom: 0,
              left: 0,
              top: 0,
              right: 0,
            },
            seats: [
              { seat: 'bottom', name: 'Player A', score: 25000, delta: 0 },
              { seat: 'left', name: 'Player Left', score: 24300, delta: 0 },
            ],
            continueAction: {
              id: 'start_next_round',
              label: '下一局',
              enabled: true,
            },
          },
        })}
        themeId="tian-shui-bi"
        themeLabel="天水碧"
        onCycleTheme={vi.fn()}
        onAction={vi.fn()}
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onCopyTableCode={vi.fn()}
        onLeaveTable={vi.fn()}
      />,
    );

    expect(container.querySelector('.table-stage__action-callout--draw')).not.toBeNull();

    act(() => {
      vi.advanceTimersByTime(3000);
    });

    expect(screen.getByText('本局结算')).toBeInTheDocument();
    expect(container.querySelector('.table-stage__action-callout--draw')).toBeNull();

    rerender(
      <BattleScreen
        viewModel={createBattleViewModel({
          mode: 'watching',
          phaseLabel: 'playing',
          promptText: '新一局开始',
          result: null,
          actionEffect: null,
          lastDiscard: null,
          lastDiscardSeat: null,
        })}
        themeId="tian-shui-bi"
        themeLabel="天水碧"
        onCycleTheme={vi.fn()}
        onAction={vi.fn()}
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onCopyTableCode={vi.fn()}
        onLeaveTable={vi.fn()}
      />,
    );

    expect(container.querySelector('.table-stage__action-callout--draw')).toBeNull();
    expect(screen.queryByText('流局')).toBeNull();
    vi.useRealTimers();
  });

  it('returns an unclaimed discard to the river 1.5 seconds after the next turn begins', () => {
    vi.useFakeTimers();

    renderBattleScreen(
      createBattleViewModel({
        discards: {
          bottom: ['w1'],
          left: ['b4'],
          top: [],
          right: [],
        },
        lastDiscard: 'b4',
        lastDiscardSeat: 'left',
        shouldAutoReturnLastDiscardToRiver: true,
      }),
    );

    expect(screen.getByLabelText('Latest discard spotlight')).toBeInTheDocument();
    expect(document.body.querySelector('.table-stage__river-track--left')?.querySelectorAll('.mahjong-tile--discard')).toHaveLength(0);

    act(() => {
      vi.advanceTimersByTime(1499);
    });

    expect(screen.getByLabelText('Latest discard spotlight')).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(1);
    });

    expect(screen.queryByLabelText('Latest discard spotlight')).toBeNull();
    expect(document.body.querySelector('.table-stage__river-track--left')?.querySelectorAll('.mahjong-tile--discard')).toHaveLength(1);
    vi.useRealTimers();
  });

  it('keeps the discard in the spotlight while the claim window is still open', () => {
    vi.useFakeTimers();

    renderBattleScreen(
      createBattleViewModel({
        discards: {
          bottom: ['w1'],
          left: ['b4'],
          top: [],
          right: [],
        },
        lastDiscard: 'b4',
        lastDiscardSeat: 'left',
        shouldAutoReturnLastDiscardToRiver: false,
      }),
    );

    act(() => {
      vi.advanceTimersByTime(1500);
    });

    expect(screen.getByLabelText('Latest discard spotlight')).toBeInTheDocument();
    expect(document.body.querySelector('.table-stage__river-track--left')?.querySelectorAll('.mahjong-tile--discard')).toHaveLength(0);
    vi.useRealTimers();
  });

  it('also lingers for 1.5 seconds before returning to the river when the next player is opening flowers', () => {
    vi.useFakeTimers();

    renderBattleScreen(
      createBattleViewModel({
        discards: {
          bottom: ['w1'],
          left: ['b4'],
          top: [],
          right: [],
        },
        lastDiscard: 'b4',
        lastDiscardSeat: 'left',
        shouldAutoReturnLastDiscardToRiver: true,
      }),
    );

    expect(screen.getByLabelText('Latest discard spotlight')).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(1499);
    });

    expect(screen.getByLabelText('Latest discard spotlight')).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(1);
    });

    expect(screen.queryByLabelText('Latest discard spotlight')).toBeNull();
    expect(document.body.querySelector('.table-stage__river-track--left')?.querySelectorAll('.mahjong-tile--discard')).toHaveLength(1);
    vi.useRealTimers();
  });

  it('counts the 1.5 second linger from the discard itself and only waits the remaining time after a claim window closes', () => {
    vi.useFakeTimers();

    const { rerender } = renderBattleScreen(
      createBattleViewModel({
        discards: {
          bottom: ['w1'],
          left: ['b4'],
          top: [],
          right: [],
        },
        lastDiscard: 'b4',
        lastDiscardSeat: 'left',
        shouldAutoReturnLastDiscardToRiver: false,
      }),
    );

    act(() => {
      vi.advanceTimersByTime(1000);
    });

    rerender(
      <BattleScreen
        viewModel={createBattleViewModel({
          discards: {
            bottom: ['w1'],
            left: ['b4'],
            top: [],
            right: [],
          },
          lastDiscard: 'b4',
          lastDiscardSeat: 'left',
          shouldAutoReturnLastDiscardToRiver: true,
        })}
        themeId="tian-shui-bi"
        themeLabel="天水碧"
        onCycleTheme={vi.fn()}
        onAction={vi.fn()}
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onCopyTableCode={vi.fn()}
        onLeaveTable={vi.fn()}
      />,
    );

    act(() => {
      vi.advanceTimersByTime(499);
    });

    expect(screen.getByLabelText('Latest discard spotlight')).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(1);
    });

    expect(screen.queryByLabelText('Latest discard spotlight')).toBeNull();
    expect(document.body.querySelector('.table-stage__river-track--left')?.querySelectorAll('.mahjong-tile--discard')).toHaveLength(1);
    vi.useRealTimers();
  });

  it('returns the discard immediately when the claim window closes after the 1.5 second linger has already elapsed', () => {
    vi.useFakeTimers();

    const { rerender } = renderBattleScreen(
      createBattleViewModel({
        discards: {
          bottom: ['w1'],
          left: ['b4'],
          top: [],
          right: [],
        },
        lastDiscard: 'b4',
        lastDiscardSeat: 'left',
        shouldAutoReturnLastDiscardToRiver: false,
      }),
    );

    act(() => {
      vi.advanceTimersByTime(1500);
    });

    rerender(
      <BattleScreen
        viewModel={createBattleViewModel({
          discards: {
            bottom: ['w1'],
            left: ['b4'],
            top: [],
            right: [],
          },
          lastDiscard: 'b4',
          lastDiscardSeat: 'left',
          shouldAutoReturnLastDiscardToRiver: true,
        })}
        themeId="tian-shui-bi"
        themeLabel="天水碧"
        onCycleTheme={vi.fn()}
        onAction={vi.fn()}
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onCopyTableCode={vi.fn()}
        onLeaveTable={vi.fn()}
      />,
    );

    expect(screen.queryByLabelText('Latest discard spotlight')).toBeNull();
    expect(document.body.querySelector('.table-stage__river-track--left')?.querySelectorAll('.mahjong-tile--discard')).toHaveLength(1);
    vi.useRealTimers();
  });

  it('returns the discard immediately when a different follow-up action happens before 1.5 seconds elapse', () => {
    vi.useFakeTimers();

    const { rerender } = renderBattleScreen(
      createBattleViewModel({
        discards: {
          bottom: ['w1'],
          left: ['b4'],
          top: [],
          right: [],
        },
        lastDiscard: 'b4',
        lastDiscardSeat: 'left',
        shouldAutoReturnLastDiscardToRiver: true,
        actionEffect: {
          key: 'discard-1',
          label: '出牌',
          emphasis: 'discard',
          seat: 'left',
        },
      }),
    );

    act(() => {
      vi.advanceTimersByTime(400);
    });

    act(() => {
      rerender(
        <BattleScreen
          viewModel={createBattleViewModel({
            discards: {
              bottom: ['w1'],
              left: ['b4'],
              top: [],
              right: [],
            },
            lastDiscard: 'b4',
            lastDiscardSeat: 'left',
            shouldAutoReturnLastDiscardToRiver: true,
            actionEffect: {
              key: 'draw-1',
              label: '摸牌',
              emphasis: 'draw',
              seat: 'top',
            },
          })}
          themeId="tian-shui-bi"
          themeLabel="天水碧"
          onCycleTheme={vi.fn()}
          onAction={vi.fn()}
          onTileSelect={vi.fn()}
          onTileDoubleClick={vi.fn()}
          onClaimCandidateSelect={vi.fn()}
          onClaimCandidateActivate={vi.fn()}
          onCopyTableCode={vi.fn()}
          onLeaveTable={vi.fn()}
        />,
      );
    });

    expect(screen.queryByLabelText('Latest discard spotlight')).toBeNull();
    expect(document.body.querySelector('.table-stage__river-track--left')?.querySelectorAll('.mahjong-tile--discard')).toHaveLength(1);
    vi.useRealTimers();
  });

  it.each([
    {
      winType: 'discard',
      winTypeLabel: '荣和',
      summary: '荣和，等待下一局',
      winnerSeat: 'right' as const,
      discarderSeat: 'left' as const,
      scoreDeltaBySeat: {
        bottom: 0,
        left: -8,
        right: 8,
      },
      seats: [
        { seat: 'right' as const, name: 'Player B', score: 25008, delta: 8 },
        { seat: 'left' as const, name: 'Player Left', score: 24292, delta: -8 },
      ],
    },
    {
      winType: 'self_draw',
      winTypeLabel: '自摸',
      summary: '自摸，等待下一局',
      winnerSeat: 'bottom' as const,
      discarderSeat: null,
      scoreDeltaBySeat: {
        bottom: 8,
        left: -3,
        top: -3,
        right: -2,
      },
      seats: [
        { seat: 'bottom' as const, name: 'Player A', score: 25008, delta: 8 },
        { seat: 'left' as const, name: 'Player Left', score: 24297, delta: -3 },
      ],
    },
  ])('waits 3 seconds before showing the settlement panel after a %s win appears mid-hand', ({ winType, winTypeLabel, summary, winnerSeat, discarderSeat, scoreDeltaBySeat, seats }) => {
    vi.useFakeTimers();

    const { rerender } = renderBattleScreen(
      createBattleViewModel({
        result: null,
      }),
    );

    rerender(
      <BattleScreen
        viewModel={createBattleViewModel({
          mode: 'resolving',
          phaseLabel: 'settlement',
          result: {
            title: '本局结算',
            summary,
            fanTotal: 8,
            winnerSeat,
            discarderSeat,
            winType,
            winTypeLabel,
            provisional: false,
            flowerCount: 0,
            fanBreakdown: [{ fanKey: 'ping_hu', fanValue: 8 }],
            scoreDeltaBySeat,
            seats,
            continueAction: {
              id: 'start_next_round',
              label: '下一局',
              enabled: true,
            },
          },
        })}
        themeId="tian-shui-bi"
        themeLabel="天水碧"
        onCycleTheme={vi.fn()}
        onAction={vi.fn()}
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onCopyTableCode={vi.fn()}
        onLeaveTable={vi.fn()}
      />,
    );

    expect(screen.queryByText('本局结算')).not.toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(2999);
    });

    expect(screen.queryByText('本局结算')).not.toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(1);
    });

    expect(screen.getByText('本局结算')).toBeInTheDocument();
    vi.useRealTimers();
  });

  it('returns the previous discard to the river instead of keeping a spotlight during self draw settlement', () => {
    renderBattleScreen(
      createBattleViewModel({
        mode: 'resolving',
        discards: {
          bottom: ['w1'],
          left: ['b4'],
          top: [],
          right: [],
        },
        lastDiscard: 'b4',
        lastDiscardSeat: 'left',
        result: {
          title: '本局结算',
          summary: '自摸，等待下一局',
          fanTotal: 8,
          winnerSeat: 'bottom',
          discarderSeat: null,
          winType: 'self_draw',
          winTypeLabel: '自摸',
          provisional: false,
          flowerCount: 0,
          fanBreakdown: [{ fanKey: 'self_draw', fanValue: 1 }],
          scoreDeltaBySeat: {
            bottom: 8,
            left: -3,
            top: -3,
            right: -2,
          },
          seats: [
            { seat: 'bottom', name: 'Player A', score: 25008, delta: 8 },
            { seat: 'left', name: 'Player Left', score: 24297, delta: -3 },
          ],
          continueAction: {
            id: 'start_next_round',
            label: '下一局',
            enabled: true,
          },
        },
      }),
    );

    expect(screen.getByText('本局结算')).toBeInTheDocument();
    expect(screen.queryByLabelText('Latest discard spotlight')).toBeNull();
    expect(document.body.querySelector('.table-stage__river-track--left')?.querySelectorAll('.mahjong-tile--discard')).toHaveLength(1);
  });

  it('shows reconnecting overlay copy when disconnected_or_waiting has no waiting controls', () => {
    renderBattleScreen(
      createBattleViewModel({
        mode: 'disconnected_or_waiting',
        waitingControls: null,
        centerBanner: 'Reconnecting',
        promptText: 'Trying to restore your seat.',
      }),
    );

    expect(screen.getByText(/正在重连/i)).toBeInTheDocument();
    expect(screen.getAllByText(/trying to restore your seat/i).length).toBeGreaterThan(0);
  });

  it('renders table player info bars for every seat around the table', () => {
    renderBattleScreen(createBattleViewModel());

    expect(screen.getByLabelText('Player Top 信息栏')).toBeInTheDocument();
    expect(screen.getByLabelText('Player Left 信息栏')).toBeInTheDocument();
    expect(screen.getByLabelText('Player A 信息栏')).toBeInTheDocument();
  });

  it('passes the public action indicator seat through to the table stage', () => {
    renderBattleScreen(
      createBattleViewModel({
        actionIndicatorSeat: 'left',
        activePlayerSeat: 'left',
        promptText: 'Player Left正在执行操作：出牌',
      }),
    );

    expect(screen.getByLabelText('左家正在行动')).toBeInTheDocument();
  });

  it('shows table info bars without exposing per-round delta values', () => {
    renderBattleScreen(createBattleViewModel());

    expect(screen.getByLabelText('Player Left 信息栏')).toHaveTextContent('手牌 13 · 花 1');
    expect(screen.getByLabelText('Player Top 信息栏')).toHaveTextContent('手牌 13 · 花 0');
    expect(screen.getByLabelText('Player Left 信息栏')).not.toHaveTextContent('-8');
  });

  it('renders local hand tiles with mahjong tile presentation', () => {
    renderBattleScreen(createBattleViewModel());

    const hand = screen.getByLabelText(/local hand/i);
    expect(hand.querySelectorAll('.mahjong-tile--hand')).toHaveLength(2);
  });

  it('lets the player resize table tiles from the table panel without affecting the hand dock', async () => {
    const user = userEvent.setup();

    renderBattleScreen(createBattleViewModel());

    const table = screen.getByLabelText('Mahjong table');
    const hand = screen.getByLabelText(/local hand/i);
    const scaleControls = screen.getByRole('group', { name: '调整牌桌牌面大小' });

    expect(table.style.getPropertyValue('--table-stage-tile-scale')).toBe('1.12');
    expect(within(scaleControls).getByText('112%')).toBeInTheDocument();

    await user.click(within(scaleControls).getByRole('button', { name: '放大牌桌牌面' }));

    expect(table.style.getPropertyValue('--table-stage-tile-scale')).toBe('1.18');
    expect(table.style.getPropertyValue('--table-stage-spotlight-scale')).toBe('1.48');
    expect(within(scaleControls).getByText('118%')).toBeInTheDocument();
    expect(hand.querySelector('.mahjong-tile--hand')?.getAttribute('style')).toBeNull();

    await user.click(within(scaleControls).getByRole('button', { name: '缩小牌桌牌面' }));

    expect(table.style.getPropertyValue('--table-stage-tile-scale')).toBe('1.12');
    expect(within(scaleControls).getByText('112%')).toBeInTheDocument();
  });

  it('does not render an action overlay when a battle action effect is active', () => {
    renderBattleScreen(
      createBattleViewModel({
        actionEffect: {
          key: 'claim-1',
          label: '碰',
          emphasis: 'claim',
          seat: 'left',
        },
      }),
    );
  });

  it('passes settlement hu styling context through to the table-stage callout', () => {
    const { container } = renderBattleScreen(
      createBattleViewModel({
        mode: 'resolving',
        actionEffect: {
          key: 'claim-hu-1',
          label: '胡牌',
          emphasis: 'claim',
          seat: null,
          calloutTone: 'hu',
        },
        result: {
          title: '本局结算',
          summary: '自摸，等待下一局',
          fanTotal: 8,
          winnerSeat: 'right',
          discarderSeat: null,
          winType: 'self_draw',
          winTypeLabel: '自摸',
          provisional: false,
          flowerCount: 0,
          fanBreakdown: [{ fanKey: 'self_draw', fanValue: 1 }],
          scoreDeltaBySeat: {
            bottom: -3,
            left: -3,
            top: -2,
            right: 8,
          },
          seats: [
            { seat: 'right', name: 'Player B', score: 25008, delta: 8 },
            { seat: 'bottom', name: 'Player A', score: 24997, delta: -3 },
          ],
          continueAction: {
            id: 'start_next_round',
            label: '下一局',
            enabled: true,
          },
        },
      }),
    );

    expect(screen.getByText('和')).toBeInTheDocument();
    expect(container.querySelector('.table-stage__action-callout--hu-self-draw')).not.toBeNull();
    expect(container.querySelector('.table-stage__action-callout.table-stage__spotlight--right')).not.toBeNull();
  });

  it('keeps a kong callout visible when the next action effect is the automatic replacement draw', () => {
    vi.useFakeTimers();

    const { rerender } = renderBattleScreen(
      createBattleViewModel({
        actionEffect: {
          key: 'kong-1',
          label: '杠',
          emphasis: 'kong',
          seat: 'left',
          calloutTone: 'kong',
        },
      }),
    );

    expect(screen.getByText('杠')).toBeInTheDocument();

    rerender(
      <BattleScreen
        viewModel={createBattleViewModel({
          actionEffect: {
            key: 'replacement-1',
            label: '补牌',
            emphasis: 'draw',
            seat: 'left',
            calloutTone: null,
          },
        })}
        themeId="tian-shui-bi"
        themeLabel="天水碧"
        onCycleTheme={vi.fn()}
        onAction={vi.fn()}
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onCopyTableCode={vi.fn()}
        onLeaveTable={vi.fn()}
      />,
    );

    act(() => {
      vi.advanceTimersByTime(250);
    });

    expect(screen.getByText('杠')).toBeInTheDocument();
    vi.useRealTimers();
  });

  it('keeps a hu callout visible when settlement appears without a new spotlight on the same seat', () => {
    vi.useFakeTimers();

    const { rerender, container } = renderBattleScreen(
      createBattleViewModel({
        actionEffect: {
          key: 'hu-1',
          label: '和',
          emphasis: 'claim',
          seat: 'right',
          calloutTone: 'hu',
        },
        result: null,
      }),
    );

    expect(screen.getByText('和')).toBeInTheDocument();

    rerender(
      <BattleScreen
        viewModel={createBattleViewModel({
          actionEffect: {
            key: 'round-done-1',
            label: '结算',
            emphasis: 'system',
            seat: null,
            calloutTone: null,
          },
          result: {
            title: '本局结算',
            summary: '荣和，等待下一局',
            fanTotal: 8,
            winnerSeat: 'right',
            discarderSeat: 'left',
            winType: 'discard',
            winTypeLabel: '荣和',
            provisional: false,
            flowerCount: 0,
            fanBreakdown: [{ fanKey: 'ping_hu', fanValue: 8 }],
            scoreDeltaBySeat: {
              left: -8,
              right: 8,
            },
            seats: [
              { seat: 'right', name: 'Player B', score: 25008, delta: 8 },
              { seat: 'left', name: 'Player Left', score: 24292, delta: -8 },
            ],
            continueAction: {
              id: 'start_next_round',
              label: '下一局',
              enabled: true,
            },
          },
        })}
        themeId="tian-shui-bi"
        themeLabel="天水碧"
        onCycleTheme={vi.fn()}
        onAction={vi.fn()}
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onCopyTableCode={vi.fn()}
        onLeaveTable={vi.fn()}
      />,
    );

    act(() => {
      vi.advanceTimersByTime(250);
    });

    expect(container.querySelector('.table-stage__action-callout--exit')).toBeNull();
    expect(screen.getByText('和')).toBeInTheDocument();
    vi.useRealTimers();
  });

  it('does not replay a stale hu callout after settlement and next round rerenders', () => {
    vi.useFakeTimers();

    const { rerender, container } = renderBattleScreen(
      createBattleViewModel({
        actionEffect: {
          key: 'hu-1',
          label: '和',
          emphasis: 'claim',
          seat: 'right',
          calloutTone: 'hu',
        },
        result: null,
      }),
    );

    expect(container.querySelector('.table-stage__action-callout--hu')).not.toBeNull();

    act(() => {
      vi.advanceTimersByTime(3000);
    });

    expect(container.querySelector('.table-stage__action-callout--hu')).toBeNull();

    rerender(
      <BattleScreen
        viewModel={createBattleViewModel({
          mode: 'resolving',
          phaseLabel: 'settlement',
          actionEffect: null,
          result: {
            title: '本局结算',
            summary: '荣和，等待下一局',
            fanTotal: 8,
            winnerSeat: 'right',
            discarderSeat: 'left',
            winType: 'discard',
            winTypeLabel: '荣和',
            provisional: false,
            flowerCount: 0,
            fanBreakdown: [{ fanKey: 'ping_hu', fanValue: 8 }],
            scoreDeltaBySeat: {
              left: -8,
              right: 8,
            },
            seats: [
              { seat: 'right', name: 'Player B', score: 25008, delta: 8 },
              { seat: 'left', name: 'Player Left', score: 24292, delta: -8 },
            ],
            continueAction: {
              id: 'start_next_round',
              label: '下一局',
              enabled: true,
            },
          },
        })}
        themeId="tian-shui-bi"
        themeLabel="天水碧"
        onCycleTheme={vi.fn()}
        onAction={vi.fn()}
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onCopyTableCode={vi.fn()}
        onLeaveTable={vi.fn()}
      />,
    );

    expect(container.querySelector('.table-stage__action-callout--hu')).toBeNull();

    rerender(
      <BattleScreen
        viewModel={createBattleViewModel({
          mode: 'watching',
          phaseLabel: 'playing',
          promptText: '新一局开始',
          result: null,
          actionEffect: null,
          lastDiscard: null,
          lastDiscardSeat: null,
        })}
        themeId="tian-shui-bi"
        themeLabel="天水碧"
        onCycleTheme={vi.fn()}
        onAction={vi.fn()}
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onCopyTableCode={vi.fn()}
        onLeaveTable={vi.fn()}
      />,
    );

    expect(container.querySelector('.table-stage__action-callout--hu')).toBeNull();
    vi.useRealTimers();
  });

  it('keeps the freshly drawn tile separated at the end of the hand instead of showing a draw spectacle', () => {
    renderBattleScreen(
      createBattleViewModel({
        actionEffect: null,
        localHand: [
          { tileId: 'w1#1', code: 'w1', isSelected: false, isDrawn: false, isFlower: false },
          { tileId: 'w2#2', code: 'w2', isSelected: false, isDrawn: false, isFlower: false },
          { tileId: 'w9#draw-1', code: 'w9', isSelected: false, isDrawn: true, isFlower: false },
        ],
        drawnTileId: 'w9#draw-1',
      }),
    );

    const hand = screen.getByLabelText(/local hand/i);
    const buttons = hand.querySelectorAll('.action-dock__tile');

    expect(buttons).toHaveLength(3);
    expect(buttons[2]).toHaveClass('action-dock__tile--drawn');
  });

  it('does not replay the same sticky action effect after it fades when unrelated rerenders happen', () => {
    vi.useFakeTimers();

    const { rerender } = renderBattleScreen(
      createBattleViewModel({
        actionEffect: {
          key: 'claim-1',
          label: '碰',
          emphasis: 'claim',
          seat: 'left',
          calloutTone: 'pung',
        },
      }),
    );

    expect(screen.getByText('碰')).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(3000);
    });

    expect(screen.queryByText('碰')).toBeNull();

    rerender(
      <BattleScreen
        viewModel={createBattleViewModel({
          promptText: '无关重渲染',
          actionEffect: {
            key: 'claim-1',
            label: '碰',
            emphasis: 'claim',
            seat: 'left',
            calloutTone: 'pung',
          },
        })}
        themeId="tian-shui-bi"
        themeLabel="天水碧"
        onCycleTheme={vi.fn()}
        onAction={vi.fn()}
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onCopyTableCode={vi.fn()}
        onLeaveTable={vi.fn()}
      />,
    );

    expect(screen.queryByText('碰')).toBeNull();
    vi.useRealTimers();
  });

  it('renders the local player identity inside the table info bar instead of the side drawer', () => {
    const { container } = renderBattleScreen(createBattleViewModel());

    expect(container.querySelector('.battle-stage .battle-stage__local-player-info')).toBeNull();
    expect(container.querySelector('.battle-stage .table-stage__player-info-cluster--bottom')).not.toBeNull();
    expect(screen.getByText('Player A')).toBeInTheDocument();
    expect(container.querySelector('.battle-drawer .player-ring')).toBeNull();
    expect(document.body.querySelector('.action-dock__player')).toBeNull();
  });

  it('renders the battle screen as a plain full-window table layout', () => {
    const { container } = renderBattleScreen(createBattleViewModel());

    expect(container.querySelector('.win10-window')).toBeNull();
    expect(container.querySelector('.stage-background')).toBeNull();
    expect(container.querySelector('.battle-shell')).not.toBeNull();
    expect(screen.getByText('牌桌编号：AB12CD')).toBeInTheDocument();
    expect(screen.getByText('房间座位数：4/4')).toBeInTheDocument();
    expect(screen.getByText('round-123 | playing')).toBeInTheDocument();
  });

  it('blocks interaction with a viewport guard when the browser window is below the required size', () => {
    setViewportSize(1280, 720);

    render(
      <BattleScreen
        viewModel={createBattleViewModel()}
        themeId="tian-shui-bi"
        themeLabel="天水碧"
        onCycleTheme={vi.fn()}
        onAction={vi.fn()}
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onCopyTableCode={vi.fn()}
        onLeaveTable={vi.fn()}
      />,
    );

    expect(screen.getByRole('alert')).toHaveTextContent('请把浏览器窗口调整到大于 1280 x 720，且宽高比大于 16:9');
    expect(document.body.querySelector('.battle-screen--viewport-blocked')).not.toBeNull();
  });

  it('renders the local control area as a retro control panel with chinese labels', () => {
    renderBattleScreen(createBattleViewModel());
    const localPlayerCard = screen.getByLabelText('Player A 信息栏');

    expect(document.body.querySelector('.action-dock__player')).toBeNull();
    expect(localPlayerCard).not.toBeNull();
    expect(within(localPlayerCard as HTMLElement).getByText(/25,000 · Live/i)).toBeInTheDocument();
    expect(within(localPlayerCard as HTMLElement).getByText(/手牌 14 · 花 0/i)).toBeInTheDocument();
    expect(screen.getAllByRole('button', { name: '收起手牌区' }).length).toBeGreaterThan(0);
  });

  it('does not render any log window affordance even when toasts exist', () => {
    renderBattleScreen(
      createBattleViewModel({
        toasts: [
          { id: 't1', kind: 'event', text: '提示1', createdAt: '2026-03-30T12:10:36+08:00' },
          { id: 't2', kind: 'event', text: '提示2', createdAt: '2026-03-30T12:10:37+08:00' },
          { id: 't3', kind: 'event', text: '提示3', createdAt: '2026-03-30T12:10:38+08:00' },
          { id: 't4', kind: 'event', text: '提示4', createdAt: '2026-03-30T12:10:39+08:00' },
          { id: 't5', kind: 'event', text: '提示5', createdAt: '2026-03-30T12:10:40+08:00' },
        ],
      }),
    );

    expect(screen.queryByText('提示1')).not.toBeInTheDocument();
    expect(screen.queryByText('提示5')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '展开日志窗口' })).toBeNull();
    expect(screen.queryByLabelText('日志窗口')).toBeNull();
  });

  it('renders pre-match room controls in the table center instead of the hand dock', () => {
    renderBattleScreen(
      createBattleViewModel({
        mode: 'disconnected_or_waiting',
        actions: [
          { id: 'ready', label: '准备', enabled: true, emphasis: 'medium' },
          { id: 'start_match', label: '开始对局', enabled: true, emphasis: 'high' },
          { id: 'discard', label: '出牌', enabled: true, emphasis: 'high' },
        ],
        waitingControls: {
          canReady: true,
          canStart: true,
          isReady: false,
          occupiedSeats: 4,
          botCount: 0,
          canAddBot: false,
          canRemoveBot: false,
        },
      }),
    );

    expect(screen.getByRole('group', { name: '开局前房间操作' })).toBeInTheDocument();
    expect(screen.queryByText('等待牌手')).toBeNull();
    expect(document.body.querySelector('.action-dock')?.textContent).toContain('出牌');
    expect(document.body.querySelector('.action-dock')?.textContent).not.toContain('准备');
  });

  it('prevents repeated ready clicks for 3 seconds', () => {
    vi.useFakeTimers();
    const onAction = vi.fn();

    renderBattleScreen(
      createBattleViewModel({
        mode: 'disconnected_or_waiting',
        actions: [
          { id: 'ready', label: '准备', enabled: true, emphasis: 'medium' },
          { id: 'start_match', label: '开始对局', enabled: false, emphasis: 'high' },
        ],
        waitingControls: {
          canReady: true,
          canStart: false,
          isReady: false,
          occupiedSeats: 2,
          botCount: 0,
          canAddBot: true,
          canRemoveBot: false,
        },
      }),
      { onAction },
    );

    act(() => {
      screen.getByRole('button', { name: '准备' }).click();
      screen.getByRole('button', { name: '准备' }).click();
    });

    expect(onAction).toHaveBeenCalledTimes(1);
    expect(onAction).toHaveBeenCalledWith('ready');
    expect(screen.getByRole('button', { name: '准备' })).toBeDisabled();

    act(() => {
      vi.advanceTimersByTime(2999);
      screen.getByRole('button', { name: '准备' }).click();
    });

    expect(onAction).toHaveBeenCalledTimes(1);

    act(() => {
      vi.advanceTimersByTime(1);
    });

    expect(screen.getByRole('button', { name: '准备' })).toBeEnabled();

    act(() => {
      screen.getByRole('button', { name: '准备' }).click();
    });

    expect(onAction).toHaveBeenCalledTimes(2);
    vi.useRealTimers();
  });

  it('renders dedicated meld areas only for remote seats', () => {
    renderBattleScreen(createBattleViewModel());

    expect(screen.getByLabelText('Player Left melds').querySelectorAll('.mahjong-tile--discard')).toHaveLength(3);
    expect(screen.getByLabelText('Player B melds').querySelectorAll('.mahjong-tile--discard')).toHaveLength(3);
    expect(screen.queryByLabelText(/local melds/i)).toBeNull();
  });

  it('shows a leave-table button in waiting rooms and wires the callback', async () => {
    const user = userEvent.setup();
    const onLeaveTable = vi.fn();

    renderBattleScreen(
      createBattleViewModel({
        canLeaveTable: true,
        mode: 'disconnected_or_waiting',
        phaseLabel: 'waiting',
        waitingControls: {
          canReady: true,
          canStart: false,
          isReady: false,
          occupiedSeats: 2,
          botCount: 0,
          canAddBot: true,
          canRemoveBot: false,
        },
      }),
      { onLeaveTable },
    );

    expect(screen.getByRole('button', { name: '快捷离开牌桌' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: '离开牌桌' }));

    expect(onLeaveTable).toHaveBeenCalledTimes(1);
  });

  it('hides the pre-match room menu after the game has started', () => {
    renderBattleScreen(
      createBattleViewModel({
        waitingControls: null,
        actions: [
          { id: 'ready', label: '准备', enabled: true, emphasis: 'medium' },
          { id: 'start_match', label: '开始对局', enabled: true, emphasis: 'high' },
        ],
      }),
    );

    expect(screen.queryByRole('group', { name: '开局前房间操作' })).toBeNull();
  });

  it('renders claim actions inside the bottom dock when claim actions are available', () => {
    renderBattleScreen(
      createBattleViewModel({
        deadlineAt: '2099-03-30T12:10:40+08:00',
        promptCue: {
          kind: 'claim',
          tone: 'critical',
          title: '左家刚打出可响应牌',
          detail: '你可以 和牌 / 碰 / 过',
          actionIds: ['hu', 'pung', 'pass'],
          highlightedActionIds: ['hu', 'pung'],
          sourceSeat: 'left',
          isUrgent: true,
        },
        actions: [
          { id: 'hu', label: '和牌', enabled: true, emphasis: 'high' },
          { id: 'pung', label: '碰', enabled: true, emphasis: 'medium' },
          { id: 'pass', label: '过', enabled: true, emphasis: 'low' },
        ],
      }),
    );

    const huButton = screen.getByRole('button', { name: '和牌' });
    const pungButton = screen.getByRole('button', { name: '碰' });
    const passButton = screen.getByRole('button', { name: '过' });

    expect(document.body.querySelector('.battle-shell--response')).toBeNull();
    expect(document.body.querySelector('.battle-shell--response-hu')).toBeNull();
    expect(document.body.querySelector('.action-dock--elevated')).toBeNull();
    expect(document.body.querySelector('.action-dock--actionable')).toBeNull();
    expect(huButton).toHaveClass('action-dock__action--hu-burn');
    expect(pungButton).not.toHaveClass('action-dock__action--hu-burn');
    expect(passButton).not.toHaveClass('action-dock__action--hu-burn');
    expect(pungButton).toHaveClass('action-dock__action--themed', 'action-dock__action--themed-pung');
    expect(passButton).toHaveClass('action-dock__action--themed', 'action-dock__action--themed-pass');
    expect(huButton).not.toHaveClass('action-dock__action--response-glow');
    expect(pungButton).not.toHaveClass('action-dock__action--response-glow');
    expect(passButton).not.toHaveClass('action-dock__action--response-glow');
    expect(screen.queryByText('左家刚打出可响应牌')).toBeNull();
  });
});
