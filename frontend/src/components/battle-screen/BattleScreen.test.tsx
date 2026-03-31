import type { ComponentProps } from 'react';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import type { BattleViewModel } from '../../types/match';
import { BattleScreen } from './BattleScreen';

function createBattleViewModel(overrides: Partial<BattleViewModel> = {}): BattleViewModel {
  return {
    mode: 'watching',
    tableCode: 'AB12CD',
    canLeaveTable: false,
    phaseLabel: 'playing',
    roundLabel: 'round-123',
    scoreSummaryLabel: '总分 12',
    deadlineAt: null,
    topStatusLabel: 'Live Match',
    activePlayerSeat: 'bottom',
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
    drawnTileId: 'w2#2',
    centerBanner: 'Opponent Turn',
    promptText: null,
    promptCue: null,
    result: null,
    lastDiscard: 'b4',
    lastDiscardSeat: 'left',
    actionEffect: null,
    celebrationEffect: null,
    toasts: [],
    ...overrides,
  };
}

function renderBattleScreen(viewModel: BattleViewModel, overrides?: Partial<ComponentProps<typeof BattleScreen>>) {
  return render(
    <BattleScreen
      viewModel={viewModel}
      onAction={vi.fn()}
      onTileSelect={vi.fn()}
      onCopyTableCode={vi.fn()}
      onLeaveTable={vi.fn()}
      {...overrides}
    />,
  );
}

describe('BattleScreen', () => {
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
        },
        actions: [
          { id: 'ready', label: 'Ready', enabled: true, emphasis: 'medium' },
          { id: 'start_match', label: 'Start Match', enabled: true, emphasis: 'high' },
        ],
      }),
    );

    expect(screen.getByRole('button', { name: /ready/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /start match/i })).toBeInTheDocument();
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

    expect(screen.getByText(/番数合计/i)).toBeInTheDocument();
    expect(screen.getByText('平胡')).toBeInTheDocument();
    expect(screen.getByText('幺九刻')).toBeInTheDocument();
    expect(screen.queryByText('七对')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: '展开剩余 1 项番种' })).toBeInTheDocument();
  });

  it('can expand and collapse extra fan breakdown rows in settlement view', async () => {
    const user = userEvent.setup();

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

    expect(screen.queryByText('七对')).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: '展开剩余 1 项番种' }));
    expect(screen.getByText('七对')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: '收起番种' }));
    expect(screen.queryByText('七对')).not.toBeInTheDocument();
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

  it('renders the top player ring separately from the table content', () => {
    renderBattleScreen(createBattleViewModel());

    expect(screen.getByText('Player Top')).toBeInTheDocument();
    expect(screen.getByText('Player Left')).toBeInTheDocument();
  });

  it('does not show per-round delta text on other player info panels', () => {
    const { container } = renderBattleScreen(createBattleViewModel());

    expect(container.querySelector('.player-ring--left .player-ring__detail')?.textContent).toBe('手牌 13 · 花 1');
    expect(container.querySelector('.player-ring--top .player-ring__detail')?.textContent).toBe('手牌 13 · 花 0');
  });

  it('renders local hand tiles with mahjong tile presentation', () => {
    renderBattleScreen(createBattleViewModel());

    const hand = screen.getByLabelText(/local hand/i);
    expect(hand.querySelectorAll('.mahjong-tile--hand')).toHaveLength(2);
  });

  it('renders action spectacle copy when a battle action effect is active', () => {
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

    expect(screen.getAllByText('碰').length).toBeGreaterThan(0);
    expect(screen.getByText('左家')).toBeInTheDocument();
  });

  it('falls back to a draw spectacle when a new drawnTileId is present', () => {
    renderBattleScreen(
      createBattleViewModel({
        actionEffect: null,
        drawnTileId: 'w9#draw-1',
      }),
    );

    expect(screen.getByText('摸牌')).toBeInTheDocument();
    expect(screen.getByText('你')).toBeInTheDocument();
  });

  it('renders a celebration overlay when a winning celebration effect is active', () => {
    renderBattleScreen(
      createBattleViewModel({
        celebrationEffect: {
          key: 'win-1',
          label: '自摸',
          winnerSeat: 'bottom',
          winType: 'self_draw',
        },
      }),
    );

    expect(screen.getAllByText('自摸').length).toBeGreaterThan(0);
    expect(screen.getByText('华彩自摸')).toBeInTheDocument();
  });

  it('moves the local player identity into the side drawer instead of the table center', () => {
    const { container } = renderBattleScreen(createBattleViewModel());

    expect(container.querySelector('.battle-stage .player-ring--bottom')).toBeNull();
    expect(screen.getByText('Player A')).toBeInTheDocument();
    expect(container.querySelector('.battle-drawer .player-ring--bottom')).not.toBeNull();
    expect(document.body.querySelector('.action-dock__player')).toBeNull();
  });

  it('renders the battle screen as a plain full-window table layout', () => {
    const { container } = renderBattleScreen(createBattleViewModel());

    expect(container.querySelector('.win10-window')).toBeNull();
    expect(container.querySelector('.stage-background')).toBeNull();
    expect(container.querySelector('.battle-shell')).not.toBeNull();
    expect(screen.getByText(/牌桌编号/i)).toBeInTheDocument();
  });

  it('renders the local control area as a retro control panel with chinese labels', () => {
    renderBattleScreen(createBattleViewModel());
    const localPlayerCard = screen.getByText('Player A').closest('.player-ring');

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

  it('renders room controls inside the right-side drawer instead of the hand dock', () => {
    renderBattleScreen(
      createBattleViewModel({
        actions: [
          { id: 'ready', label: '准备', enabled: true, emphasis: 'medium' },
          { id: 'start_match', label: '开始对局', enabled: true, emphasis: 'high' },
          { id: 'discard', label: '出牌', enabled: true, emphasis: 'high' },
        ],
      }),
    );

    expect(screen.getByLabelText('牌桌侧边面板')).toBeInTheDocument();
    expect(document.body.querySelector('.action-dock')?.textContent).toContain('出牌');
    expect(document.body.querySelector('.action-dock')?.textContent).not.toContain('准备');
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
        },
      }),
      { onLeaveTable },
    );

    await user.click(screen.getByRole('button', { name: '离开牌桌' }));

    expect(onLeaveTable).toHaveBeenCalledTimes(1);
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
    expect(huButton).toHaveClass('action-dock__action--response-glow', 'action-dock__action--response-glow-hu');
    expect(pungButton).toHaveClass('action-dock__action--response-glow', 'action-dock__action--response-glow-pung');
    expect(passButton).not.toHaveClass('action-dock__action--response-glow');
    expect(screen.queryByText('左家刚打出可响应牌')).toBeNull();
  });
});
