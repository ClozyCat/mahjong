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
        isReadyHand: false,
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
        isReadyHand: false,
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
        isReadyHand: false,
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
        isReadyHand: false,
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
    handInsight: null,
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

    expect(visibleFanList.getByText('平和')).toBeInTheDocument();
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

  it('renders the settlement overlay through a top-layer portal', () => {
    const { container } = renderBattleScreen(
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
          fanBreakdown: [{ fanKey: 'ping_hu', fanValue: 8 }],
          scoreDeltaBySeat: {
            bottom: 0,
            left: -8,
            right: 8,
          },
          seats: [
            { seat: 'bottom', name: 'Player A', score: 25000, delta: 0 },
            { seat: 'left', name: 'Player Left', score: 24992, delta: -8 },
            { seat: 'top', name: 'Player Top', score: 25000, delta: 0 },
            { seat: 'right', name: 'Player B', score: 25008, delta: 8 },
          ],
          continueAction: null,
        },
      }),
    );

    expect(container.querySelector('.result-overlay')).toBeNull();
    expect(document.body.querySelector('.result-overlay')).not.toBeNull();
  });

  it('allows paging between multiple winning hands in the settlement overlay', () => {
    vi.useFakeTimers();

    renderBattleScreen(
      createBattleViewModel({
        mode: 'resolving',
        phaseLabel: 'settlement',
        result: {
          title: '本局结算',
          summary: '2 家同时和牌，等待下一局',
          fanTotal: 8,
          winnerSeat: 'right',
          discarderSeat: 'left',
          winType: 'discard',
          winTypeLabel: '荣和',
          provisional: false,
          flowerCount: 0,
          fanBreakdown: [{ fanKey: 'ping_hu', fanValue: 8 }],
          pages: [
            {
              fanTotal: 8,
              winnerSeat: 'right',
              discarderSeat: 'left',
              winType: 'discard',
              winTypeLabel: '荣和',
              flowerCount: 0,
              fanBreakdown: [{ fanKey: 'ping_hu', fanValue: 8 }],
            },
            {
              fanTotal: 16,
              winnerSeat: 'top',
              discarderSeat: 'left',
              winType: 'discard',
              winTypeLabel: '荣和',
              flowerCount: 1,
              fanBreakdown: [{ fanKey: 'full_flush', fanValue: 16 }],
            },
          ],
          scoreDeltaBySeat: {
            bottom: -8,
            left: -24,
            top: 16,
            right: 8,
          },
          seats: [
            { seat: 'top', name: 'Player Top', score: 26816, delta: 16 },
            { seat: 'right', name: 'Player B', score: 25008, delta: 8 },
            { seat: 'left', name: 'Player Left', score: 24268, delta: -24 },
          ],
          continueAction: {
            id: 'start_next_round',
            label: '下一局',
            enabled: true,
          },
        },
      }),
    );

    act(() => {
      vi.advanceTimersByTime(6000);
    });

    expect(screen.getByRole('button', { name: '上一位' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '下一位' })).toBeInTheDocument();
    expect(screen.getByText('1 / 2')).toBeInTheDocument();
    expect(screen.getByText(/胜者 Player B（右家）/)).toBeInTheDocument();
    expect(screen.getByText('平和')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '下一位' }));

    expect(screen.getByText('2 / 2')).toBeInTheDocument();
    expect(screen.getByText(/胜者 Player Top（对家）/)).toBeInTheDocument();
    expect(screen.queryByText(/花牌 1/)).not.toBeInTheDocument();
    expect(screen.getByText('清一色')).toBeInTheDocument();
    expect(screen.queryByText('平和')).toBeNull();
    vi.useRealTimers();
  });

  it('shows all multi-winner hu callouts together before opening the settlement overlay', () => {
    vi.useFakeTimers();

    const { container, rerender } = renderBattleScreen(
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

    expect(container.querySelector('.table-stage__action-callout.table-stage__spotlight--right')).not.toBeNull();

    rerender(
      <BattleScreen
        viewModel={createBattleViewModel({
          mode: 'resolving',
          phaseLabel: 'settlement',
          actionEffect: {
            key: 'hu-2',
            label: '和',
            emphasis: 'claim',
            seat: 'top',
            calloutTone: 'hu',
          },
          result: {
            title: '本局结算',
            summary: '2 家同时和牌，等待下一局',
            fanTotal: 8,
            winnerSeat: 'right',
            discarderSeat: 'left',
            winType: 'discard',
            winTypeLabel: '荣和',
            provisional: false,
            flowerCount: 0,
            fanBreakdown: [{ fanKey: 'ping_hu', fanValue: 8 }],
            pages: [
              {
                fanTotal: 8,
                winnerSeat: 'right',
                discarderSeat: 'left',
                winType: 'discard',
                winTypeLabel: '荣和',
                flowerCount: 0,
                fanBreakdown: [{ fanKey: 'ping_hu', fanValue: 8 }],
              },
              {
                fanTotal: 16,
                winnerSeat: 'top',
                discarderSeat: 'left',
                winType: 'discard',
                winTypeLabel: '荣和',
                flowerCount: 1,
                fanBreakdown: [{ fanKey: 'full_flush', fanValue: 16 }],
              },
            ],
            scoreDeltaBySeat: {
              left: -24,
              top: 16,
              right: 8,
            },
            seats: [
              { seat: 'top', name: 'Player Top', score: 26816, delta: 16 },
              { seat: 'right', name: 'Player B', score: 25008, delta: 8 },
              { seat: 'left', name: 'Player Left', score: 24268, delta: -24 },
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

    expect(document.body.querySelector('.result-overlay')).toBeNull();
    expect(container.querySelector('.table-stage__action-callout.table-stage__spotlight--right')).not.toBeNull();
    expect(container.querySelector('.table-stage__action-callout.table-stage__spotlight--top')).not.toBeNull();

    act(() => {
      vi.advanceTimersByTime(3000);
    });

    expect(container.querySelector('.table-stage__action-callout.table-stage__spotlight--right')).toBeNull();
    expect(container.querySelector('.table-stage__action-callout.table-stage__spotlight--top')).toBeNull();
    expect(document.body.querySelector('.result-overlay')).not.toBeNull();
    vi.useRealTimers();
  });

  it('shows the matching fan guide tooltip after hovering a settlement fan row for 0.35s', () => {
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

    const pingHuRow = screen.getByText('平和').closest('.result-overlay__row');
    expect(pingHuRow).not.toBeNull();

    fireEvent.mouseEnter(pingHuRow!);

    act(() => {
      vi.advanceTimersByTime(349);
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

  it('shows a player statistics tooltip with score trend and win rate when hovering a score row after 0.35s', () => {
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
                dealInCount: 3,
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
      vi.advanceTimersByTime(349);
    });

    expect(screen.queryByRole('tooltip', { name: 'Player A 战绩统计' })).toBeNull();

    act(() => {
      vi.advanceTimersByTime(1);
    });

    expect(screen.getByRole('tooltip', { name: 'Player A 战绩统计' })).toBeInTheDocument();
    expect(screen.getByText('50%')).toBeInTheDocument();
    expect(screen.getByText('2/4')).toBeInTheDocument();
    expect(screen.getByText('放铳次数')).toBeInTheDocument();
    expect(screen.getByText('3')).toBeInTheDocument();
    expect(document.body.querySelector('.result-overlay__seat-tooltip-svg')).not.toBeNull();

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
    expect(updatedFanList.queryByText('平和')).toBeNull();
    expect(screen.queryByRole('group', { name: '番型明细分页' })).toBeNull();
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

  it('does not replay the ready_hand callout when the optimistic action settles into the confirmed event', () => {
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
        actionEffect: {
          key: 'optimistic-ready_hand:b4#0',
          label: '听',
          emphasis: 'claim',
          seat: 'left',
          calloutTone: 'ready_hand',
        },
      }),
    );

    expect(screen.getByText('听')).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(500);
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
          actionEffect: {
            key: 'ready_hand_declared-1',
            label: '听',
            emphasis: 'claim',
            seat: 'left',
            calloutTone: 'ready_hand',
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
      vi.advanceTimersByTime(499);
    });

    expect(screen.getByText('听')).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(1);
    });

    expect(screen.queryByText('听')).toBeNull();
    vi.useRealTimers();
  });

  it('keeps the discard visible for 1.5 seconds after the ready_hand callout completes', () => {
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
        actionEffect: {
          key: 'optimistic-ready_hand:b4#0',
          label: '听',
          emphasis: 'claim',
          seat: 'left',
          calloutTone: 'ready_hand',
        },
      }),
    );

    act(() => {
      vi.advanceTimersByTime(500);
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
          actionEffect: {
            key: 'ready_hand_declared-1',
            label: '听',
            emphasis: 'claim',
            seat: 'left',
            calloutTone: 'ready_hand',
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
      vi.advanceTimersByTime(500);
    });

    expect(screen.queryByText('听')).toBeNull();
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

  it('does not render the deprecated reconnecting overlay when disconnected_or_waiting has no waiting controls', () => {
    renderBattleScreen(
      createBattleViewModel({
        mode: 'disconnected_or_waiting',
        waitingControls: null,
        centerBanner: 'Reconnecting',
        promptText: 'Trying to restore your seat.',
      }),
    );

    expect(screen.queryByText(/正在重连/i)).toBeNull();
  });




  it('renders local hand tiles with mahjong tile presentation', () => {
    renderBattleScreen(createBattleViewModel());

    const hand = screen.getByLabelText(/local hand/i);
    expect(hand.querySelectorAll('.mahjong-tile--hand')).toHaveLength(2);
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
          label: '和牌',
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


  it('renders the battle screen as a plain full-window table layout', () => {
    const { container } = renderBattleScreen(createBattleViewModel());

    expect(container.querySelector('.stage-background')).toBeNull();
    expect(container.querySelector('.battle-shell')).not.toBeNull();
    expect(container.querySelector('.table-stage > .action-dock')).not.toBeNull();
    expect(screen.getByText('牌桌编号：AB12CD')).toBeInTheDocument();
    expect(screen.getByText('房间座位数：4/4')).toBeInTheDocument();
    expect(screen.getByText('round-123 | playing')).toBeInTheDocument();
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

    const quickLeaveButton = screen.getByRole('button', { name: '快捷离开牌桌' });

    expect(quickLeaveButton).toBeInTheDocument();
    expect(screen.queryByText('等待牌手')).toBeNull();
    await user.click(quickLeaveButton);

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
