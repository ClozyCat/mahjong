import type { ComponentProps } from 'react';
import { render, screen } from '@testing-library/react';
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
    centerBanner: 'Opponent Turn',
    promptText: null,
    result: null,
    lastDiscard: 'b4',
    lastDiscardSeat: 'left',
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
          provisional: true,
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

    expect(screen.getByText(/番数合计/i)).toBeInTheDocument();
    expect(screen.getByText(/ping_hu/i)).toBeInTheDocument();
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

  it('renders local hand tiles with mahjong tile presentation', () => {
    renderBattleScreen(createBattleViewModel());

    const hand = screen.getByLabelText(/local hand/i);
    expect(hand.querySelectorAll('.mahjong-tile--hand')).toHaveLength(2);
  });

  it('moves the local player identity into the action dock instead of the table center', () => {
    const { container } = renderBattleScreen(createBattleViewModel());

    expect(container.querySelector('.battle-stage .player-ring--bottom')).toBeNull();
    expect(screen.getByText('Player A')).toBeInTheDocument();
    expect(container.querySelector('.action-dock__player')).not.toBeNull();
  });

  it('renders the battle screen inside a retro client window', () => {
    const { container } = renderBattleScreen(createBattleViewModel());

    expect(container.querySelector('.win98-window')).not.toBeNull();
    expect(screen.getByText(/牌桌编号/i)).toBeInTheDocument();
  });

  it('renders the local control area as a retro control panel with chinese labels', () => {
    const { container } = renderBattleScreen(createBattleViewModel());

    expect(container.querySelector('.action-dock__player')).not.toBeNull();
    expect(screen.getByText(/当前操作/i)).toBeInTheDocument();
  });

  it('shows up to four latest toasts so local discard prompts are not immediately hidden', () => {
    renderBattleScreen(
      createBattleViewModel({
        toasts: [
          { id: 't1', kind: 'event', text: '提示1' },
          { id: 't2', kind: 'event', text: '提示2' },
          { id: 't3', kind: 'event', text: '提示3' },
          { id: 't4', kind: 'event', text: '提示4' },
          { id: 't5', kind: 'event', text: '提示5' },
        ],
      }),
    );

    expect(screen.queryByText('提示1')).not.toBeInTheDocument();
    expect(screen.getByText('提示2')).toBeInTheDocument();
    expect(screen.getByText('提示3')).toBeInTheDocument();
    expect(screen.getByText('提示4')).toBeInTheDocument();
    expect(screen.getByText('提示5')).toBeInTheDocument();
  });

  it('renders dedicated meld areas for remote seats and the local dock', () => {
    renderBattleScreen(createBattleViewModel());

    expect(screen.getByLabelText('Player Left melds').querySelectorAll('.mahjong-tile--discard')).toHaveLength(3);
    expect(screen.getByLabelText('Player B melds').querySelectorAll('.mahjong-tile--discard')).toHaveLength(3);
    expect(screen.getByLabelText(/local melds/i).querySelectorAll('.mahjong-tile--discard')).toHaveLength(3);
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
});
