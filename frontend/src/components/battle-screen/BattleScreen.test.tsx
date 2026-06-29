import type { ComponentProps } from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { BattleViewModel } from '../../types/match';
import { BattleScreen } from './BattleScreen';
import { SETTLEMENT_CALLOUT_LINGER_MS } from './settlementTiming';

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
    deadlineAt: null,
    activePlayerSeat: 'bottom',
    actionIndicatorSeat: null,
    isActionDockElevated: false,
    players: [
      {
        seat: 'top',
        absoluteSeat: 0,
        name: 'Player Top',
        score: 26800,
        points: 0,
        liveDelta: 0,
        flowerCount: 0,
        wind: 'North',
        isDealer: false,
        isActive: false,
        isLocal: false,
        connected: true,
        isReadyHand: false,
        concealedCount: 13,
        meldCount: 0,
        melds: [],
        flowers: [],
        statusText: 'Live',
      },
      {
        seat: 'left',
        absoluteSeat: 1,
        name: 'Player Left',
        score: 24300,
        points: 0,
        liveDelta: -8,
        flowerCount: 1,
        wind: 'West',
        isDealer: false,
        isActive: false,
        isLocal: false,
        connected: true,
        isReadyHand: false,
        concealedCount: 13,
        meldCount: 1,
        melds: [['b2', 'b3', 'b4']],
        flowers: ['f1'],
        statusText: 'Live',
      },
      {
        seat: 'bottom',
        absoluteSeat: 2,
        name: 'Player A',
        score: 25000,
        points: 0,
        liveDelta: 8,
        flowerCount: 0,
        wind: 'East',
        isDealer: true,
        isActive: true,
        isLocal: true,
        connected: true,
        isReadyHand: false,
        concealedCount: 14,
        meldCount: 0,
        melds: [['w3', 'w3', 'w3']],
        flowers: [],
        statusText: 'Live',
      },
      {
        seat: 'right',
        absoluteSeat: 3,
        name: 'Player B',
        score: 25000,
        points: 0,
        liveDelta: 0,
        flowerCount: 0,
        wind: 'South',
        isDealer: false,
        isActive: false,
        isLocal: false,
        connected: true,
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
    tableSettings: {
      minimumHuFan: 8,
      dealerRepeatEnabled: false,
      dealerDoubleEnabled: false,
    },
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
    centerStatusText: null,
    promptText: null,
    promptCue: null,
    result: null,
    settlementHands: null,
    lastDiscard: 'b4',
    lastDiscardSeat: 'left',
    shouldAutoReturnLastDiscardToRiver: false,
    actionEffect: null,
    dealerSelection: null,
    extendedWithExtra: false,
    ...overrides,
  };
}

function renderBattleScreen(viewModel: BattleViewModel, overrides?: Partial<ComponentProps<typeof BattleScreen>>) {
  return renderBattleScreenAtViewport(viewModel, { width: 1720, height: 900 }, overrides);
}

function renderBattleScreenAtViewport(
  viewModel: BattleViewModel,
  viewport: { width: number; height: number },
  overrides?: Partial<ComponentProps<typeof BattleScreen>>,
) {
  setViewportSize(viewport.width, viewport.height);

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
      onLeaveTable={vi.fn()}
      {...overrides}
    />,
  );
}

const waitingControlDefaults = {
  minimumHuFan: 8 as const,
  canDecreaseMinimumHuFan: true,
  canIncreaseMinimumHuFan: false,
  dealerRepeatEnabled: false,
  dealerDoubleEnabled: false,
  canToggleDealerRepeat: true,
  canToggleDealerDouble: true,
};

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

function mockAudioPlayback() {
  const play = vi.fn(() => Promise.resolve());
  const audio = vi.fn((url: string) => {
    void url;
    const handlers = new Map<string, () => void>();

    return {
      addEventListener: vi.fn((eventName: string, handler: () => void) => {
        handlers.set(eventName, handler);
      }),
      removeEventListener: vi.fn(),
      play: vi.fn(() => {
        play();
        handlers.get('ended')?.();
        return Promise.resolve();
      }),
    };
  });
  const originalAudio = globalThis.Audio;

  globalThis.Audio = audio as unknown as typeof Audio;

  return {
    audio,
    play,
    restore: () => {
      globalThis.Audio = originalAudio;
    },
  };
}

describe('BattleScreen', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('does not play any sound when a discard action effect arrives', () => {
    const audioMock = mockAudioPlayback();

    try {
      renderBattleScreen(
        createBattleViewModel({
          lastDiscard: 'w1',
          lastDiscardSeat: 'bottom',
          actionEffect: {
            key: 'tile_discarded:seat-2:w1',
            label: '出牌',
            emphasis: 'discard',
            seat: 'bottom',
            calloutTone: null,
          },
        }),
      );

      expect(audioMock.audio).not.toHaveBeenCalled();
      expect(audioMock.play).not.toHaveBeenCalled();
    } finally {
      audioMock.restore();
    }
  });

  it('plays the clear combo sound when a claim action effect arrives', () => {
    const audioMock = mockAudioPlayback();

    try {
      renderBattleScreen(
        createBattleViewModel({
          actionEffect: {
            key: 'claim_made:seat-1:pung',
            label: '碰',
            emphasis: 'claim',
            seat: 'left',
            calloutTone: 'pung',
          },
        }),
      );

      expect(audioMock.audio).toHaveBeenCalledTimes(1);
      expect(String(audioMock.audio.mock.calls[0][0])).toContain('freesound_gamestudio-clear-combo');
      expect(audioMock.play).toHaveBeenCalledTimes(1);
    } finally {
      audioMock.restore();
    }
  });

  it('plays the clear combo sound for claim actions consistently regardless of seat changes', () => {
    const audioMock = mockAudioPlayback();
    const playerB = {
      seat: 'left' as const,
      absoluteSeat: 1,
      userId: 102,
      name: 'Player B',
      score: 25000,
      points: 0,
      liveDelta: 0,
      flowerCount: 0,
      wind: 'South' as const,
      isDealer: false,
      isActive: false,
      isLocal: false,
      connected: true,
      isReadyHand: false,
      concealedCount: 13,
      meldCount: 0,
      melds: [],
      flowers: [],
      statusText: 'Live',
    };

    try {
      const { rerender } = renderBattleScreen(
        createBattleViewModel({
          actionEffect: {
            key: 'claim_made:before-rotation:pung',
            label: '碰',
            emphasis: 'claim',
            seat: 'left',
            calloutTone: 'pung',
          },
          players: [playerB],
        }),
      );

      rerender(
        <BattleScreen
          viewModel={createBattleViewModel({
            actionEffect: {
              key: 'claim_made:after-rotation:pung',
              label: '碰',
              emphasis: 'claim',
              seat: 'right',
              calloutTone: 'pung',
            },
            players: [
              {
                ...playerB,
                seat: 'right',
                absoluteSeat: 0,
              },
            ],
          })}
          themeId="tian-shui-bi"
          themeLabel="天水碧"
          onCycleTheme={vi.fn()}
          onAction={vi.fn()}
          onTileSelect={vi.fn()}
          onTileDoubleClick={vi.fn()}
          onClaimCandidateSelect={vi.fn()}
          onClaimCandidateActivate={vi.fn()}
          onLeaveTable={vi.fn()}
        />,
      );

      expect(audioMock.audio).toHaveBeenCalledTimes(2);
      expect(String(audioMock.audio.mock.calls[0][0])).toContain('freesound_gamestudio-clear-combo');
      expect(String(audioMock.audio.mock.calls[1][0])).toContain('freesound_gamestudio-clear-combo');
    } finally {
      audioMock.restore();
    }
  });

  it('does not play operation voice while the voice switch is off', () => {
    const audioMock = mockAudioPlayback();

    try {
      renderBattleScreen(
        createBattleViewModel({
          actionEffect: {
            key: 'claim_made:voice-off:pung',
            label: '碰',
            emphasis: 'claim',
            seat: 'left',
            calloutTone: 'pung',
          },
        }),
        { isVoiceEnabled: false },
      );

      expect(audioMock.audio).not.toHaveBeenCalled();
      expect(audioMock.play).not.toHaveBeenCalled();
    } finally {
      audioMock.restore();
    }
  });

  it('plays the ready hand sound for ready hand declaration', () => {
    const audioMock = mockAudioPlayback();

    try {
      renderBattleScreen(
        createBattleViewModel({
          lastDiscard: 'w1',
          lastDiscardSeat: 'bottom',
          actionEffect: {
            key: 'ready_hand_declared:seat-2:w1',
            label: '听',
            emphasis: 'claim',
            seat: 'bottom',
            calloutTone: 'ready_hand',
            tileCode: 'w1',
          },
        }),
      );

      expect(audioMock.audio).toHaveBeenCalledTimes(1);
      expect(String(audioMock.audio.mock.calls[0][0])).toContain('universfield-game-bonus-02-294436');
      expect(audioMock.play).toHaveBeenCalledTimes(1);
    } finally {
      audioMock.restore();
    }
  });

  it('plays clear combo sound only for claim actions, ignores discard effects in queued effects', () => {
    const audioMock = mockAudioPlayback();
    const discardEffect = {
      key: 'tile_discarded:seat-1:b7',
      label: '出牌',
      emphasis: 'discard',
      seat: 'left',
      calloutTone: null,
      tileCode: 'b7',
    } satisfies NonNullable<BattleViewModel['actionEffect']>;
    const pungEffect = {
      key: 'claim_made:seat-1:pung',
      label: '碰',
      emphasis: 'claim',
      seat: 'left',
      calloutTone: 'pung',
    } satisfies NonNullable<BattleViewModel['actionEffect']>;
    const viewModel = {
      ...createBattleViewModel({
        actionEffect: pungEffect,
      }),
      actionEffects: [discardEffect, pungEffect],
    } as BattleViewModel;

    try {
      renderBattleScreen(viewModel);

      expect(audioMock.audio).toHaveBeenCalledTimes(1);
      expect(String(audioMock.audio.mock.calls[0][0])).toContain('freesound_gamestudio-clear-combo');
      expect(audioMock.play).toHaveBeenCalledTimes(1);
    } finally {
      audioMock.restore();
    }
  });

  it('plays clear combo sound for each queued action even when the same operation repeats quickly', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-04-27T12:00:00Z'));
    const audioMock = mockAudioPlayback();
    const firstPungEffect = {
      key: 'claim_made:seat-1:pung:first',
      label: '碰',
      emphasis: 'claim',
      seat: 'left',
      calloutTone: 'pung',
    } satisfies NonNullable<BattleViewModel['actionEffect']>;
    const secondPungEffect = {
      key: 'claim_made:seat-1:pung:second',
      label: '碰',
      emphasis: 'claim',
      seat: 'left',
      calloutTone: 'pung',
    } satisfies NonNullable<BattleViewModel['actionEffect']>;
    const viewModel = {
      ...createBattleViewModel({
        actionEffect: secondPungEffect,
      }),
      actionEffects: [firstPungEffect, secondPungEffect],
    } as BattleViewModel;

    try {
      renderBattleScreen(viewModel);

      expect(audioMock.audio).toHaveBeenCalledTimes(2);
      expect(String(audioMock.audio.mock.calls[0][0])).toContain('freesound_gamestudio-clear-combo');
      expect(String(audioMock.audio.mock.calls[1][0])).toContain('freesound_gamestudio-clear-combo');
      expect(audioMock.play).toHaveBeenCalledTimes(2);
    } finally {
      audioMock.restore();
      vi.useRealTimers();
    }
  });

  it('shows invite and start controls in waiting state', () => {
    renderBattleScreen(
      createBattleViewModel({
        mode: 'disconnected_or_waiting',
        phaseLabel: 'waiting',
        waitingControls: {
          ...waitingControlDefaults,
          canStart: true,
          occupiedSeats: 4,
          botCount: 0,
          canAddBot: false,
          canRemoveBot: false,
        },
        actions: [
          { id: 'start_match', label: 'Start Match', enabled: true, emphasis: 'high' },
        ],
      }),
      { onInvitePlayer: vi.fn() },
    );

    expect(screen.getByRole('button', { name: '邀请' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /start match/i })).toBeInTheDocument();
    expect(screen.queryByLabelText('牌桌侧边面板')).toBeNull();
  });

  it.skip('shows settlement breakdown in resolving state', () => {
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
    expect(screen.getByText(/胜者 Player B（下家）/)).toBeInTheDocument();
    expect(screen.getByText(/放铳 Player Left（上家）/)).toBeInTheDocument();
    expect(screen.queryByText(/胜者 right/)).not.toBeInTheDocument();
    expect(screen.queryByText(/放铳 left/)).not.toBeInTheDocument();
    expect(visibleFanList.getByText('幺九刻')).toBeInTheDocument();
    expect(visibleFanList.getByText('七对')).toBeInTheDocument();
    expect(screen.queryByText('当前为临时结算结果')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '展开剩余 2 项番种' })).toBeNull();
  });

  it.skip('labels settlement seats from the current seat map after seat rotation', () => {
    renderBattleScreen(
      createBattleViewModel({
        mode: 'resolving',
        phaseLabel: 'settlement',
        players: [
          {
            ...createBattleViewModel().players[0],
            seat: 'left',
            absoluteSeat: 0,
            name: 'Player Left',
          },
          {
            ...createBattleViewModel().players[3],
            seat: 'bottom',
            absoluteSeat: 1,
            name: 'Player B',
            isLocal: true,
          },
          {
            ...createBattleViewModel().players[2],
            seat: 'right',
            absoluteSeat: 2,
            name: 'Player A',
            isLocal: false,
          },
          {
            ...createBattleViewModel().players[1],
            seat: 'top',
            absoluteSeat: 3,
            name: 'Player Top',
          },
        ],
        result: {
          title: '本局结算',
          summary: '荣和，等待下一局',
          fanTotal: 8,
          winnerSeat: 'right',
          winnerAbsoluteSeat: 1,
          discarderSeat: 'left',
          discarderAbsoluteSeat: 0,
          winType: 'discard',
          winTypeLabel: '荣和',
          provisional: false,
          flowerCount: 0,
          fanBreakdown: [{ fanKey: 'ping_hu', fanValue: 8 }],
          scoreDeltaBySeat: {
            right: 8,
            left: -8,
          },
          seats: [
            { seat: 'right', absoluteSeat: 1, name: 'Player B', score: 25008, delta: 8 },
            { seat: 'left', absoluteSeat: 0, name: 'Player Left', score: 24292, delta: -8 },
          ],
          continueAction: {
            id: 'start_next_round',
            label: '下一局',
            enabled: true,
          },
        },
      }),
    );

    const playerBRow = Array.from(document.body.querySelectorAll('.result-overlay__seat-row'))
      .find((row) => row.textContent?.includes('Player B'));

    expect(screen.getByText(/胜者 Player B（本家）/)).toBeInTheDocument();
    expect(playerBRow).not.toBeNull();
    expect(playerBRow).toHaveTextContent('本家');
    expect(playerBRow).not.toHaveTextContent('下家');
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

  it('does not play the dramatic hu reveal for a finished-match placeholder result', () => {
    vi.useFakeTimers();

    const { rerender } = renderBattleScreen(createBattleViewModel({ result: null }));

    rerender(
      <BattleScreen
        viewModel={createBattleViewModel({
          mode: 'finished',
          phaseLabel: '已结束',
          result: {
            title: '大局已定',
            summary: '本桌完整对局已经结束，只能退出牌桌。',
            fanTotal: null,
            winnerSeat: null,
            discarderSeat: null,
            winType: null,
            winTypeLabel: null,
            provisional: false,
            flowerCount: 0,
            fanBreakdown: [],
            scoreDeltaBySeat: {},
            seats: [
              { seat: 'bottom', name: 'Player A', score: 25000, delta: null },
              { seat: 'right', name: 'Player B', score: 25000, delta: null },
            ],
            continueAction: {
              id: 'match_decided',
              label: '大局已定',
              enabled: false,
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
        onLeaveTable={vi.fn()}
      />,
    );

    act(() => {
      vi.advanceTimersByTime(3500);
    });

    expect(document.body.querySelector('.dramatic-reveal')).toBeNull();
    expect(document.body.querySelector('.result-overlay')).not.toBeNull();
    expect(screen.queryByText('1番')).not.toBeInTheDocument();
  });

  it('keeps a manually collapsed settlement overlay collapsed across equivalent result refreshes', async () => {
    const user = userEvent.setup();
    const settlementResult = {
      title: '本局结算',
      summary: '等待下一局',
      fanTotal: 8,
      winnerSeat: 'right' as const,
      discarderSeat: 'left' as const,
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
        { seat: 'bottom' as const, name: 'Player A', score: 25000, delta: 0 },
        { seat: 'left' as const, name: 'Player Left', score: 24992, delta: -8 },
        { seat: 'top' as const, name: 'Player Top', score: 25000, delta: 0 },
        { seat: 'right' as const, name: 'Player B', score: 25008, delta: 8 },
      ],
      continueAction: {
        id: 'start_next_round' as const,
        label: '下一局',
        enabled: true,
      },
    };
    const props = {
      themeId: 'tian-shui-bi' as const,
      themeLabel: '天水碧',
      onCycleTheme: vi.fn(),
      onAction: vi.fn(),
      onTileSelect: vi.fn(),
      onTileDoubleClick: vi.fn(),
      onClaimCandidateSelect: vi.fn(),
      onClaimCandidateActivate: vi.fn(),
      onLeaveTable: vi.fn(),
    };
    const { rerender } = render(
      <BattleScreen
        {...props}
        viewModel={createBattleViewModel({
          mode: 'resolving',
          phaseLabel: 'settlement',
          result: settlementResult,
        })}
      />,
    );

    const expandedOverlay = document.body.querySelector('.result-overlay') as HTMLElement;
    expect(expandedOverlay).not.toHaveClass('result-overlay--collapsed');
    expect(expandedOverlay).toHaveClass('result-overlay--expanded');

    await user.click(screen.getByRole('button', { name: '收起面板' }));

    const collapsedOverlay = document.body.querySelector('.result-overlay') as HTMLElement;
    expect(screen.getByRole('button', { name: '展开结算面板' })).toBeInTheDocument();
    expect(collapsedOverlay).toHaveClass('result-overlay--collapsed');
    expect(collapsedOverlay).not.toHaveClass('result-overlay--expanded');

    rerender(
      <BattleScreen
        {...props}
        viewModel={createBattleViewModel({
          mode: 'resolving',
          phaseLabel: 'settlement',
          promptText: 'websocket 刷新',
          result: {
            ...settlementResult,
            fanBreakdown: settlementResult.fanBreakdown.map((fan) => ({ ...fan })),
            scoreDeltaBySeat: { ...settlementResult.scoreDeltaBySeat },
            seats: settlementResult.seats.map((seat) => ({ ...seat })),
            continueAction: { ...settlementResult.continueAction },
          },
        })}
      />,
    );

    expect(screen.getByRole('button', { name: '展开结算面板' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '收起面板' })).toBeNull();
  });

  it.skip('allows paging between multiple winning hands in the settlement overlay', () => {
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
    expect(screen.getByText(/胜者 Player B（下家）/)).toBeInTheDocument();
    expect(screen.getByText('平和')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '下一位' }));

    expect(screen.getByText('2 / 2')).toBeInTheDocument();
    expect(screen.getByText(/胜者 Player Top（对家）/)).toBeInTheDocument();
    expect(screen.queryByText(/花牌 1/)).not.toBeInTheDocument();
    expect(screen.getByText('清一色')).toBeInTheDocument();
    expect(screen.queryByText('平和')).toBeNull();
    vi.useRealTimers();
  });

  it('highlights the winning tile for every discard winner in a multi-winner settlement', () => {
    renderBattleScreen(
      createBattleViewModel({
        mode: 'resolving',
        phaseLabel: 'settlement',
        settlementHands: {
          right: ['c1', 'c2', 'c3', 'c4', 'c5'],
          top: ['w1', 'w2', 'w3', 'w4', 'w5'],
          left: ['b1', 'b2', 'b3', 'b4', 'b5'],
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
            { seat: 'right', name: 'Player B', score: 25008, delta: 8 },
            { seat: 'top', name: 'Player Top', score: 26816, delta: 16 },
            { seat: 'left', name: 'Player Left', score: 24268, delta: -24 },
          ],
          continueAction: null,
        },
      }),
    );

    const playerBHand = within(document.body).getByLabelText('Player B 最终手牌');
    const playerTopHand = within(document.body).getByLabelText('Player Top 最终手牌');
    const playerLeftHand = within(document.body).getByLabelText('Player Left 最终手牌');

    expect(playerBHand.querySelectorAll('.mahjong-tile--last-discard')).toHaveLength(1);
    expect(playerTopHand.querySelectorAll('.mahjong-tile--last-discard')).toHaveLength(1);
    expect(playerLeftHand.querySelector('.mahjong-tile--last-discard')).toBeNull();
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

    const dramaticOverlay = document.body.querySelector('.dramatic-reveal');
    expect(dramaticOverlay).not.toBeNull();

    // Advance timers step-by-step to let sequential useEffect transitions execute under fake timers.
    act(() => {
      vi.advanceTimersByTime(800); // ENTRANCE_MS
    });
    act(() => {
      vi.advanceTimersByTime(250); // REVEAL_INTERVAL_MS
    });
    act(() => {
      vi.advanceTimersByTime(400); // TOTAL_REVEAL_DELAY_MS
    });

    act(() => {
      fireEvent.click(dramaticOverlay!);
    });

    // Advance timers by another 500ms to let the exit animation finish and call onComplete.
    act(() => {
      vi.advanceTimersByTime(500);
    });

    expect(document.body.querySelector('.result-overlay')).not.toBeNull();
    vi.useRealTimers();
  });

  it('does not replay the dramatic fan reveal when the same settlement result returns after completion', () => {
    vi.useFakeTimers();

    const settlementResult = {
      title: '本局结算',
      summary: '荣和，等待下一局',
      fanTotal: 8,
      winnerSeat: 'right' as const,
      discarderSeat: 'left' as const,
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
        { seat: 'right' as const, name: 'Player B', score: 25008, delta: 8 },
        { seat: 'left' as const, name: 'Player Left', score: 24292, delta: -8 },
      ],
      continueAction: {
        id: 'start_next_round' as const,
        label: '下一局',
        enabled: true,
      },
    };

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
          result: settlementResult,
        })}
        themeId="tian-shui-bi"
        themeLabel="天水碧"
        onCycleTheme={vi.fn()}
        onAction={vi.fn()}
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onLeaveTable={vi.fn()}
      />,
    );

    act(() => {
      vi.advanceTimersByTime(3000);
    });

    let dramaticOverlay = document.body.querySelector('.dramatic-reveal');
    expect(dramaticOverlay).not.toBeNull();

    act(() => {
      vi.advanceTimersByTime(800);
    });
    act(() => {
      vi.advanceTimersByTime(250);
    });
    act(() => {
      vi.advanceTimersByTime(400);
    });
    act(() => {
      fireEvent.click(dramaticOverlay!);
    });
    act(() => {
      vi.advanceTimersByTime(500);
    });

    expect(document.body.querySelector('.dramatic-reveal')).toBeNull();
    expect(document.body.querySelector('.result-overlay')).not.toBeNull();

    rerender(
      <BattleScreen
        viewModel={createBattleViewModel({
          mode: 'watching',
          phaseLabel: 'playing',
          result: null,
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
        onLeaveTable={vi.fn()}
      />,
    );

    expect(document.body.querySelector('.dramatic-reveal')).toBeNull();

    rerender(
      <BattleScreen
        viewModel={createBattleViewModel({
          mode: 'resolving',
          phaseLabel: 'settlement',
          result: settlementResult,
        })}
        themeId="tian-shui-bi"
        themeLabel="天水碧"
        onCycleTheme={vi.fn()}
        onAction={vi.fn()}
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onLeaveTable={vi.fn()}
      />,
    );

    act(() => {
      vi.advanceTimersByTime(3000);
    });

    expect(document.body.querySelector('.dramatic-reveal')).toBeNull();
    vi.useRealTimers();
  });

  it.skip('shows the matching fan guide tooltip after hovering a settlement fan row for 0.35s', () => {
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
            { seat: 'bottom', name: 'Player A', title: 'Lv.11', displayLabel: 'Player A Lv.11', score: 25008, delta: 8 },
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
            { seat: 'bottom', name: 'Player A', title: 'Lv.11', displayLabel: 'Player A Lv.11', score: 25008, delta: 8 },
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
            { seat: 'bottom', name: 'Player A', title: 'Lv.11', displayLabel: 'Player A Lv.11', score: 25008, delta: 8 },
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

    expect(within(scorePanel).getByText('Player A Lv.11')).toBeInTheDocument();
    expect(within(scorePanel).getByText('Player Left')).toBeInTheDocument();
    expect(within(scorePanel).getByText('Player Top')).toBeInTheDocument();
    expect(within(scorePanel).getByText('Player B')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '展开其他玩家分数（3）' })).toBeNull();
    expect(screen.queryByRole('button', { name: '收起其他玩家分数' })).toBeNull();
  });

  it('shows settlement melds to the right of each player final hand', () => {
    renderBattleScreen(
      createBattleViewModel({
        mode: 'resolving',
        phaseLabel: 'settlement',
        settlementHands: {
          bottom: ['w1', 'w2', 'w3', 'w4', 'w5'],
          right: ['c1', 'c2', 'c3', 'c4', 'c5'],
        },
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
            { seat: 'right', name: 'Player B', score: 24998, delta: -2 },
          ],
          continueAction: null,
        },
      }),
    );

    const playerARow = Array.from(document.body.querySelectorAll('.result-overlay__seat-row'))
      .find((row) => row.textContent?.includes('Player A'));
    const playerBRow = Array.from(document.body.querySelectorAll('.result-overlay__seat-row'))
      .find((row) => row.textContent?.includes('Player B'));

    expect(playerARow).not.toBeNull();
    expect(playerBRow).not.toBeNull();
    expect(within(playerARow as HTMLElement).getByLabelText('Player A 最终手牌')).toBeInTheDocument();
    expect(within(playerARow as HTMLElement).getByLabelText('Player A 副露区')).toBeInTheDocument();
    expect(within(playerBRow as HTMLElement).getByLabelText('Player B 副露区')).toBeInTheDocument();
    expect((playerARow as HTMLElement).querySelector('.result-overlay__seat-hand + .result-overlay__seat-melds'))
      .not.toBeNull();
  });

  it.skip('shows a player statistics tooltip with score trend and win rate when hovering a score row after 0.35s', () => {
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

  it.skip('renders side-by-side fan and score panels without per-section expand buttons', () => {
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

  it.skip('updates the scrollable fan list without reintroducing pagination when a new settlement result arrives', async () => {
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
    expect(document.body.querySelector('.table-stage__river-track--left')?.querySelectorAll('.mahjong-tile--discard')).toHaveLength(1);
    expect(document.body.querySelectorAll('.table-stage__river-track--left .mahjong-tile--discard')[0]).toHaveStyle('visibility: hidden');

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
    expect(document.body.querySelector('.table-stage__river-track--left')?.querySelectorAll('.mahjong-tile--discard')).toHaveLength(1);
    expect(document.body.querySelectorAll('.table-stage__river-track--left .mahjong-tile--discard')[0]).toHaveStyle('visibility: hidden');
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
        onLeaveTable={vi.fn()}
      />,
    );

    act(() => {
      vi.advanceTimersByTime(500);
    });

    expect(screen.queryByText('听')).toBeNull();
    expect(screen.getByLabelText('Latest discard spotlight')).toBeInTheDocument();
    expect(document.body.querySelector('.table-stage__river-track--left')?.querySelectorAll('.mahjong-tile--discard')).toHaveLength(1);
    expect(document.body.querySelectorAll('.table-stage__river-track--left .mahjong-tile--discard')[0]).toHaveStyle('visibility: hidden');

    act(() => {
      vi.advanceTimersByTime(1499);
    });

    expect(screen.getByLabelText('Latest discard spotlight')).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(1);
    });

    expect(screen.queryByLabelText('Latest discard spotlight')).toBeNull();
    expect(document.body.querySelector('.table-stage__river-track--left')?.querySelectorAll('.mahjong-tile--discard')).toHaveLength(1);
    expect(document.body.querySelectorAll('.table-stage__river-track--left .mahjong-tile--discard')[0]).not.toHaveStyle('visibility: hidden');
    vi.useRealTimers();
  });

  it('does not render the deprecated reconnecting overlay when disconnected_or_waiting has no waiting controls', () => {
    renderBattleScreen(
      createBattleViewModel({
        mode: 'disconnected_or_waiting',
        waitingControls: null,
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

  it('disables and dims local hand tiles while bot takeover is enabled', async () => {
    const user = userEvent.setup();
    const onTileSelect = vi.fn();
    const onTileDoubleClick = vi.fn();

    renderBattleScreen(
      createBattleViewModel({
        actions: [
          { id: 'discard', label: '出牌', enabled: true, emphasis: 'high' },
        ],
      }),
      {
        isBotTakeoverEnabled: true,
        onTileSelect,
        onTileDoubleClick,
      },
    );

    const hand = screen.getByLabelText(/local hand/i);
    const tileButtons = Array.from(hand.querySelectorAll('button'));

    expect(screen.queryByRole('button', { name: '出牌' })).toBeNull();
    expect(tileButtons).toHaveLength(2);
    expect(tileButtons[0]).toBeDisabled();
    expect(tileButtons[0]).toHaveClass('action-dock__tile--disabled');
    expect(tileButtons[0].querySelector('.mahjong-tile--disabled')).not.toBeNull();

    await user.click(tileButtons[0]);
    await user.dblClick(tileButtons[1]);

    expect(onTileSelect).not.toHaveBeenCalled();
    expect(onTileDoubleClick).not.toHaveBeenCalled();
  });

  it('shows a hand-side bot takeover switch only while bot takeover is enabled', async () => {
    const user = userEvent.setup();
    const onToggleBotTakeover = vi.fn();
    const { rerender } = renderBattleScreen(
      createBattleViewModel(),
      {
        isBotTakeoverEnabled: true,
        onToggleBotTakeover,
      },
    );

    const botButton = screen.getByRole('button', { name: '切换为手动操作' });
    expect(botButton.closest('.action-dock__quick-controls')).not.toBeNull();
    expect(botButton).toHaveTextContent('关闭托管');

    await user.click(botButton);
    expect(onToggleBotTakeover).toHaveBeenCalledWith(false);

    rerender(
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
        onLeaveTable={vi.fn()}
        isBotTakeoverEnabled={false}
        onToggleBotTakeover={onToggleBotTakeover}
      />,
    );

    expect(screen.queryByRole('button', { name: '切换为 BOT 代打' })).toBeNull();
  });

  it('shows a hand-side auto-pass kong switch while the kong possibility is available', async () => {
    const user = userEvent.setup();
    const onToggleAutoPassKong = vi.fn();

    renderBattleScreen(
      createBattleViewModel(),
      {
        canToggleAutoPassKong: true,
        isAutoPassKongEnabled: true,
        onToggleAutoPassKong,
      },
    );

    const autoPassButton = screen.getByRole('button', { name: '关闭自动过杠' });

    expect(autoPassButton).toHaveAttribute('aria-pressed', 'true');
    expect(autoPassButton).toHaveTextContent('自动过杠');
    await user.click(autoPassButton);
    expect(onToggleAutoPassKong).toHaveBeenCalledWith(false);
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
        onLeaveTable={vi.fn()}
      />,
    );

    expect(container.querySelector('.table-stage__action-callout--hu')).toBeNull();
    vi.useRealTimers();
  });

  it('skips the dramatic reveal in evaluation rooms after the hu callout delay', () => {
    vi.useFakeTimers();

    const { container, rerender } = renderBattleScreen(createBattleViewModel({ result: null }));

    rerender(
      <BattleScreen
        viewModel={createBattleViewModel({
          roomMode: 'evaluation',
          mode: 'resolving',
          phaseLabel: 'settlement',
          actionEffect: {
            key: 'hu-evaluation-1',
            label: '和',
            emphasis: 'claim',
            seat: 'right',
            calloutTone: 'hu',
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
        onLeaveTable={vi.fn()}
      />,
    );

    expect(screen.getByText('和')).toBeInTheDocument();
    expect(container.querySelector('.table-stage__action-callout--hu')).not.toBeNull();
    expect(document.body.querySelector('.result-overlay')).toBeNull();
    expect(document.body.querySelector('.dramatic-reveal')).toBeNull();

    act(() => {
      vi.advanceTimersByTime(SETTLEMENT_CALLOUT_LINGER_MS);
    });

    expect(document.body.querySelector('.dramatic-reveal')).toBeNull();
    expect(document.body.querySelector('.result-overlay')).not.toBeNull();
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
    expect(screen.getByText('座位数：4/4')).toBeInTheDocument();
    expect(screen.getByText('round-123')).toBeInTheDocument();
  });

  it('counts only human seats in the room seat counter while keeping bot takeover seats human', () => {
    const basePlayers = createBattleViewModel().players;

    renderBattleScreen(
      createBattleViewModel({
        players: [
          {
            ...basePlayers[0],
            seatType: 'bot',
            name: 'Bot 1',
            isBotControlled: true,
          },
          {
            ...basePlayers[1],
            seatType: 'human',
            isBotControlled: true,
          },
          {
            ...basePlayers[2],
            seatType: 'human',
            isBotControlled: false,
          },
          {
            ...basePlayers[3],
            seatType: 'human',
            isBotControlled: false,
          },
        ],
      }),
    );

    expect(screen.getByText('座位数：3/4')).toBeInTheDocument();
  });

  it('prompts for rotation when the table viewport is taller than it is wide', () => {
    const { container } = renderBattleScreenAtViewport(createBattleViewModel(), { width: 390, height: 844 });

    expect(screen.getByRole('alert')).toHaveTextContent('请旋转屏幕或调整窗口比例');
    expect(screen.getByText('当前牌桌需要宽度大于或等于高度的画面比例。')).toBeInTheDocument();
    expect(container.querySelector('.table-stage')?.getAttribute('data-layout')).toBe('balanced');
  });

  it('does not show the rotation prompt when the table viewport is at least as wide as tall', () => {
    renderBattleScreenAtViewport(createBattleViewModel(), { width: 844, height: 390 });

    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.queryByText('请旋转屏幕或调整窗口比例')).toBeNull();
  });

  it('renders pre-match room controls in the table center instead of the hand dock', () => {
    const onMinimumHuFanChange = vi.fn();

    renderBattleScreen(
      createBattleViewModel({
        mode: 'disconnected_or_waiting',
        actions: [
          { id: 'start_match', label: '开始对局', enabled: true, emphasis: 'high' },
          { id: 'discard', label: '出牌', enabled: true, emphasis: 'high' },
        ],
        waitingControls: {
          ...waitingControlDefaults,
          canStart: true,
          occupiedSeats: 4,
          botCount: 0,
          canAddBot: false,
          canRemoveBot: false,
          minimumHuFan: 8,
          canDecreaseMinimumHuFan: true,
          canIncreaseMinimumHuFan: false,
        },
      }),
      { onInvitePlayer: vi.fn(), onMinimumHuFanChange },
    );

    expect(screen.getByRole('group', { name: '开局前房间操作' })).toBeInTheDocument();
    expect(screen.getByRole('group', { name: '起和番数控制' })).toBeInTheDocument();
    expect(screen.getByLabelText('当前起和番数 8 番')).toBeInTheDocument();
    expect(screen.queryByText('等待牌手')).toBeNull();
    expect(document.body.querySelector('.action-dock')?.textContent).toContain('出牌');
    expect(document.body.querySelector('.action-dock')?.textContent).not.toContain('准备');
    expect(screen.getByRole('button', { name: '邀请' })).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '降低起和番数' }));

    expect(onMinimumHuFanChange).toHaveBeenCalledWith(6);
  });

  it('renders the current table settings below the room seat count', () => {
    renderBattleScreen(
      createBattleViewModel({
        tableSettings: {
          minimumHuFan: 0,
          dealerRepeatEnabled: false,
          dealerDoubleEnabled: true,
        },
      }),
    );

    expect(screen.getByText('牌桌编号：AB12CD')).toBeInTheDocument();
    expect(screen.getByText('座位数：4/4')).toBeInTheDocument();
    expect(screen.getByText('设定：0番起和 | 庄家翻倍')).toBeInTheDocument();
  });

  it('hides pre-match room and bot controls while dealer selection is spinning', () => {
    renderBattleScreen(
      createBattleViewModel({
        mode: 'disconnected_or_waiting',
        actions: [
          { id: 'start_match', label: '开始对局', enabled: false, emphasis: 'high' },
        ],
        waitingControls: {
          ...waitingControlDefaults,
          canStart: false,
          occupiedSeats: 4,
          botCount: 2,
          canAddBot: false,
          canRemoveBot: false,
        },
        dealerSelection: {
          key: 'dealer-selection-1',
          dealerSeat: 'right',
          dealerName: 'Player B',
          startedAt: '2026-04-27T12:00:00Z',
          revealAt: '2026-04-27T12:00:04.200Z',
          durationMs: 4200,
        },
      }),
    );

    expect(screen.queryByRole('group', { name: '开局前房间操作' })).toBeNull();
    expect(screen.queryByRole('group', { name: 'BOT 数量控制' })).toBeNull();
    expect(screen.queryByRole('group', { name: '起和番数控制' })).toBeNull();
    expect(screen.queryByRole('button', { name: '开始对局' })).toBeNull();
    expect(screen.queryByRole('button', { name: '增加 BOT' })).toBeNull();
    expect(screen.queryByRole('button', { name: '减少 BOT' })).toBeNull();
  });

  it('skips the dealer selection spinner for evaluation rooms', () => {
    const { container } = renderBattleScreen(
      createBattleViewModel({
        roomMode: 'evaluation',
        mode: 'disconnected_or_waiting',
        actions: [
          { id: 'start_match', label: '开始对局', enabled: false, emphasis: 'high' },
        ],
        waitingControls: {
          ...waitingControlDefaults,
          canStart: false,
          occupiedSeats: 4,
          botCount: 2,
          canAddBot: false,
          canRemoveBot: false,
        },
        dealerSelection: {
          key: 'dealer-selection-evaluation-1',
          dealerSeat: 'right',
          dealerName: 'Player B',
          startedAt: '2026-04-27T12:00:00Z',
          revealAt: '2026-04-27T12:00:04.200Z',
          durationMs: 4200,
        },
      }),
    );

    expect(container.querySelector('.match-status-bar__arrow')).toBeNull();
    expect(screen.getByText('等待中')).toBeInTheDocument();
    expect(screen.queryByRole('group', { name: '开局前房间操作' })).toBeNull();
  });

  it('opens the player list from the invite action and allows closing it', async () => {
    const user = userEvent.setup();
    const onInvitePlayer = vi.fn();

    renderBattleScreen(
      createBattleViewModel({
        mode: 'disconnected_or_waiting',
        actions: [
          { id: 'start_match', label: '开始对局', enabled: false, emphasis: 'high' },
        ],
        waitingControls: {
          ...waitingControlDefaults,
          canStart: false,
          occupiedSeats: 2,
          botCount: 0,
          canAddBot: true,
          canRemoveBot: false,
        },
      }),
      {
        onInvitePlayer,
        currentUserId: 1,
        inviteUsers: [
          {
            user: {
              user_id: 2,
              username: 'player-b',
              display_name: 'Player B',
              points: 300,
              title: '新秀',
              display_label: 'Player B 新秀',
              bio: '',
              avatar: null,
            },
            status: 'online',
          },
        ],
      },
    );

    await user.click(screen.getByRole('button', { name: '邀请' }));
    expect(screen.getByRole('dialog', { name: '玩家列表' })).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: '关闭玩家列表' }));
    expect(screen.queryByRole('dialog', { name: '玩家列表' })).toBeNull();
  });

  it('renders dedicated meld areas only for remote seats', () => {
    renderBattleScreen(createBattleViewModel());

    expect(screen.getByLabelText('Player Left melds').querySelectorAll('.mahjong-tile--discard')).toHaveLength(3);
    expect(screen.getByLabelText('Player B melds').querySelectorAll('.mahjong-tile--discard')).toHaveLength(3);
    expect(screen.queryByLabelText(/local melds/i)).toBeNull();
  });

  it('asks for confirmation before leaving a waiting room', async () => {
    const user = userEvent.setup();
    const onLeaveTable = vi.fn();

    renderBattleScreen(
      createBattleViewModel({
        canLeaveTable: true,
        mode: 'disconnected_or_waiting',
        phaseLabel: 'waiting',
        waitingControls: {
          ...waitingControlDefaults,
          canStart: false,
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

    const dialog = screen.getByRole('dialog', { name: '确认离席' });

    expect(dialog).toHaveTextContent('离开牌桌 AB12CD 后，需要重新加入才能回到本桌。');
    expect(onLeaveTable).not.toHaveBeenCalled();

    await user.click(within(dialog).getByRole('button', { name: '确认离席' }));

    expect(onLeaveTable).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('dialog', { name: '确认离席' })).toBeNull();
  });

  it('keeps the player seated when leave-table confirmation is cancelled', async () => {
    const user = userEvent.setup();
    const onLeaveTable = vi.fn();

    renderBattleScreen(
      createBattleViewModel({
        canLeaveTable: true,
        mode: 'disconnected_or_waiting',
        phaseLabel: 'waiting',
        waitingControls: {
          ...waitingControlDefaults,
          canStart: false,
          occupiedSeats: 2,
          botCount: 0,
          canAddBot: true,
          canRemoveBot: false,
        },
      }),
      { onLeaveTable },
    );

    await user.click(screen.getByRole('button', { name: '快捷离开牌桌' }));
    await user.click(screen.getByRole('button', { name: '取消' }));

    expect(onLeaveTable).not.toHaveBeenCalled();
    expect(screen.queryByRole('dialog', { name: '确认离席' })).toBeNull();
  });

  it('hides the pre-match room menu after the game has started', () => {
    renderBattleScreen(
      createBattleViewModel({
        waitingControls: null,
        actions: [
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
          title: '上家刚打出可响应牌',
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
    expect(screen.queryByText('上家刚打出可响应牌')).toBeNull();
  });

  it('opens the player list without human and AI tabs', async () => {
    const user = userEvent.setup();

    renderBattleScreen(createBattleViewModel({
      players: createBattleViewModel().players.slice(0, 3),
    }), {
      onInvitePlayer: vi.fn(),
      inviteUsers: [
        {
          user: {
            user_id: 1,
            username: 'player-a',
            display_name: 'Player A',
            points: 150,
            title: '平民',
            display_label: 'Player A 平民',
            bio: '',
            avatar: null,
          },
          status: 'online',
        },
        {
          user: {
            user_id: 5,
            username: 'player-b',
            display_name: 'Player B',
            points: 600,
            title: '平民',
            display_label: 'Player B 平民',
            bio: '',
            avatar: null,
          },
          status: 'online',
        },
      ],
    });

    await user.click(screen.getByRole('button', { name: '打开玩家列表' }));

    expect(screen.getByRole('dialog', { name: '玩家列表' })).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: '人类' })).not.toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'AI' })).not.toBeInTheDocument();
    expect(screen.getByText('Player A 平民')).toBeInTheDocument();
    expect(screen.getByText('Player B 平民')).toBeInTheDocument();
  });

});
