import type { ComponentProps } from 'react';
import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

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
    claimCandidates: [],
    drawnTileId: 'w2#2',
    centerBanner: 'Opponent Turn',
    promptText: null,
    promptCue: null,
    result: null,
    settlementHands: null,
    lastDiscard: 'b4',
    lastDiscardSeat: 'left',
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

  it('waits to show the settlement panel until the final discard returns to the river when no response is needed', () => {
    vi.useFakeTimers();

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
      }),
    );

    expect(screen.queryByText('本局结算')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Latest discard spotlight')).toBeNull();
    expect(document.body.querySelector('.table-stage__river-track--left')?.querySelectorAll('.mahjong-tile--discard')).toHaveLength(1);

    act(() => {
      vi.advanceTimersByTime(420);
    });

    expect(screen.getByText('本局结算')).toBeInTheDocument();
    vi.useRealTimers();
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
    expect(container.querySelector('.battle-stage .table-stage__player-info--bottom')).not.toBeNull();
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
        },
      }),
    );

    expect(screen.getByRole('group', { name: '开局前房间操作' })).toBeInTheDocument();
    expect(screen.queryByText('等待牌手')).toBeNull();
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
    expect(huButton).not.toHaveClass('action-dock__action--response-glow');
    expect(pungButton).not.toHaveClass('action-dock__action--response-glow');
    expect(passButton).not.toHaveClass('action-dock__action--response-glow');
    expect(screen.queryByText('左家刚打出可响应牌')).toBeNull();
  });
});
