import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { renderToStaticMarkup } from 'react-dom/server';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { TableStage } from './TableStage';

describe('TableStage', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('marks only the latest matching discard as the last discard', () => {
    const { container } = render(
      <TableStage
        discards={{
          top: ['w1', 'w3', 'w3'],
          left: [],
          right: [],
          bottom: [],
        }}
        activeSeat="top"
        lastDiscard="w3"
        lastDiscardSeat="top"
        promptText={null}
      />,
    );

    const tiles = screen.getAllByTestId('mahjong-tile');
    const riverTiles = container.querySelector('.table-stage__river-track--top')?.querySelectorAll('.mahjong-tile');
    const spotlight = container.querySelector('.table-stage__spotlight--top');

    expect(tiles.length).toBe(3);
    expect(tiles.filter((tile) => tile.classList.contains('mahjong-tile--last-discard'))).toHaveLength(1);
    expect(riverTiles).toHaveLength(2);
    expect(spotlight?.querySelector('.mahjong-tile--last-discard')).not.toBeNull();
  });

  it('uses seat-aware river track classes for stable discard grids', () => {
    const { container } = render(
      <TableStage
        discards={{
          top: ['w1', 'w2', 'w3', 'w4', 'w5', 'w6', 'w7'],
          left: ['b1', 'b2', 'b3', 'b4', 'b5', 'b6'],
          right: ['c1', 'c2', 'c3', 'c4', 'c5', 'c6'],
          bottom: ['d1', 'd2', 'd3', 'd4', 'd5', 'd6', 'd7'],
        }}
        activeSeat="bottom"
        lastDiscard="d7"
        lastDiscardSeat="bottom"
        promptText="Observation"
      />,
    );

    expect(container.querySelector('.table-stage__river-track--top')).not.toBeNull();
    expect(container.querySelector('.table-stage__river-track--left')).not.toBeNull();
    expect(container.querySelector('.table-stage__river-track--right')).not.toBeNull();
    expect(container.querySelector('.table-stage__river-track--bottom')).not.toBeNull();
  });

  it('keeps the right-side river in left-to-right order so discards start from the left edge', () => {
    const { container } = render(
      <TableStage
        discards={{
          top: [],
          left: [],
          right: ['c1', 'c2', 'c3'],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard={null}
        promptText={null}
      />,
    );

    const rightTrack = container.querySelector('.table-stage__river-track--right');

    expect(rightTrack).not.toBeNull();
    expect(rightTrack).toHaveStyle({ direction: 'ltr' });
  });

  it('renders each players melds beside the matching river when player data is provided', () => {
    const { container } = render(
      <TableStage
        discards={{
          top: ['w1', 'w2'],
          left: ['b1', 'b2'],
          right: ['c1', 'c2'],
          bottom: ['d1', 'd2'],
        }}
        activeSeat="bottom"
        lastDiscard="d2"
        lastDiscardSeat="bottom"
        promptText={null}
        players={[
          { seat: 'top', name: 'Player Top', melds: [['w3', 'w4', 'w5']] },
          { seat: 'left', name: 'Player Left', melds: [['b3', 'b4', 'b5']] },
          { seat: 'right', name: 'Player Right', melds: [['c3', 'c4', 'c5']] },
          { seat: 'bottom', name: 'Player Bottom', melds: [['d3', 'd4', 'd5']] },
        ]}
      />,
    );

    expect(screen.getByLabelText('Player Top melds').querySelectorAll('.mahjong-tile--discard')).toHaveLength(3);
    expect(screen.getByLabelText('Player Left melds').querySelectorAll('.mahjong-tile--discard')).toHaveLength(3);
    expect(screen.getByLabelText('Player Right melds').querySelectorAll('.mahjong-tile--discard')).toHaveLength(3);
    expect(screen.getByLabelText('Player Bottom melds').querySelectorAll('.mahjong-tile--discard')).toHaveLength(3);
    expect(container.querySelector('.table-stage__seat-zone--top .table-stage__melds--top')).not.toBeNull();
    expect(container.querySelector('.table-stage__seat-zone--left .table-stage__melds--left')).not.toBeNull();
    expect(container.querySelector('.table-stage__seat-zone--right .table-stage__melds--right')).not.toBeNull();
    expect(container.querySelector('.table-stage__seat-zone--bottom .table-stage__melds--bottom')).not.toBeNull();
  });

  it('uses seat-specific meld anchors so side racks do not reuse the river overlap position', () => {
    const { container } = render(
      <TableStage
        discards={{
          top: ['w1'],
          left: ['b1'],
          right: ['c1'],
          bottom: ['d1'],
        }}
        activeSeat="bottom"
        lastDiscard="d1"
        lastDiscardSeat="bottom"
        promptText={null}
        players={[
          { seat: 'top', name: 'Player Top', melds: [['w3', 'w4', 'w5']] },
          { seat: 'left', name: 'Player Left', melds: [['b3', 'b4', 'b5']] },
          { seat: 'right', name: 'Player Right', melds: [['c3', 'c4', 'c5']] },
          { seat: 'bottom', name: 'Player Bottom', melds: [['d3', 'd4', 'd5']] },
        ]}
      />,
    );

    const topMelds = container.querySelector('.table-stage__melds--top') as HTMLDivElement | null;
    const leftMelds = container.querySelector('.table-stage__melds--left') as HTMLDivElement | null;
    const rightMelds = container.querySelector('.table-stage__melds--right') as HTMLDivElement | null;

    expect(topMelds).not.toBeNull();
    expect(leftMelds).not.toBeNull();
    expect(rightMelds).not.toBeNull();
    expect(topMelds?.style.left).toContain('calc(');
    expect(topMelds?.style.top).toBe('50%');
    expect(topMelds?.style.bottom).toBe('auto');
    expect(topMelds?.style.transform).toBe('translateY(-50%)');
    expect(leftMelds?.style.left).toBe('50%');
    expect(leftMelds?.style.bottom).toContain('calc(');
    expect(leftMelds?.style.top).toBe('auto');
    expect(leftMelds?.style.transform).toBe('translateX(-50%)');
    expect(rightMelds?.style.left).toBe('50%');
    expect(rightMelds?.style.bottom).toContain('calc(');
    expect(rightMelds?.style.top).toBe('auto');
    expect(rightMelds?.style.transform).toBe('translateX(-50%)');
  });

  it('does not force top and bottom rivers into a two-row container when melds are present', () => {
    const { container } = render(
      <TableStage
        discards={{
          top: ['w1'],
          left: ['b1'],
          right: ['c1'],
          bottom: ['d1'],
        }}
        activeSeat="bottom"
        lastDiscard="d1"
        lastDiscardSeat="bottom"
        promptText={null}
        players={[
          { seat: 'top', name: 'Player Top', melds: [['w3', 'w4', 'w5']] },
          { seat: 'left', name: 'Player Left', melds: [['b3', 'b4', 'b5']] },
          { seat: 'right', name: 'Player Right', melds: [['c3', 'c4', 'c5']] },
          { seat: 'bottom', name: 'Player Bottom', melds: [['d3', 'd4', 'd5']] },
        ]}
      />,
    );

    expect(container.querySelector('.table-stage__seat-zone--fixed-meld-anchor')).toBeNull();
    expect(container.querySelector('.table-stage__river-track--top')?.querySelectorAll('.mahjong-tile')).toHaveLength(1);
    expect(container.querySelector('.table-stage__river-track--bottom')?.querySelectorAll('.mahjong-tile')).toHaveLength(0);
  });

  it('pins dense top and bottom meld racks to the river edge to keep them within the table frame', () => {
    const { container } = render(
      <TableStage
        discards={{
          top: ['w1', 'w2'],
          left: [],
          right: [],
          bottom: ['d1', 'd2'],
        }}
        activeSeat="bottom"
        lastDiscard="d2"
        lastDiscardSeat="bottom"
        promptText={null}
        players={[
          {
            seat: 'top',
            name: 'Player Top',
            melds: [
              ['w3', 'w4', 'w5'],
              ['w6', 'w7', 'w8'],
              ['w9', 'd1', 'd2'],
            ],
          },
          { seat: 'left', name: 'Player Left', melds: [] },
          { seat: 'right', name: 'Player Right', melds: [] },
          {
            seat: 'bottom',
            name: 'Player Bottom',
            melds: [
              ['d3', 'd4', 'd5'],
              ['d6', 'd7', 'd8'],
              ['d9', 'w1', 'w2'],
            ],
          },
        ]}
      />,
    );

    expect(container.querySelector('.table-stage__melds--top.table-stage__melds--dense')).not.toBeNull();
    expect(container.querySelector('.table-stage__melds--bottom.table-stage__melds--dense')).not.toBeNull();
    expect(container.querySelector('.table-stage__melds--left.table-stage__melds--dense')).toBeNull();
    expect(container.querySelector('.table-stage__melds--right.table-stage__melds--dense')).toBeNull();
  });

  it('keeps top and bottom meld racks in two-row mode on wide screens so the third meld starts a new column', () => {
    const originalWidth = window.innerWidth;
    const originalHeight = window.innerHeight;

    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 1920 });
    Object.defineProperty(window, 'innerHeight', { configurable: true, value: 900 });

    const { container, unmount } = render(
      <TableStage
        discards={{
          top: [],
          left: [],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard={null}
        promptText={null}
        players={[
          {
            seat: 'top',
            name: 'Player Top',
            melds: [
              ['w1', 'w2', 'w3'],
              ['w4', 'w5', 'w6'],
              ['w7', 'w8', 'w9'],
            ],
          },
          { seat: 'left', name: 'Player Left', melds: [] },
          { seat: 'right', name: 'Player Right', melds: [] },
          {
            seat: 'bottom',
            name: 'Player Bottom',
            melds: [
              ['b1', 'b2', 'b3'],
              ['b4', 'b5', 'b6'],
              ['b7', 'b8', 'b9'],
            ],
          },
        ]}
      />,
    );

    const tableStage = container.querySelector('.table-stage') as HTMLElement | null;

    expect(tableStage?.style.getPropertyValue('--table-stage-meld-rows-h')).toBe('2');
    expect(tableStage?.style.getPropertyValue('--table-stage-meld-cols-v')).toBe('1');

    unmount();
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: originalWidth });
    Object.defineProperty(window, 'innerHeight', { configurable: true, value: originalHeight });
  });

  it('exports the bottom player info guard for hand dock sizing', () => {
    const originalWidth = window.innerWidth;
    const originalHeight = window.innerHeight;

    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 1920 });
    Object.defineProperty(window, 'innerHeight', { configurable: true, value: 900 });

    const { container, unmount } = render(
      <TableStage
        discards={{
          top: [],
          left: [],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard={null}
        promptText={null}
        players={[{ seat: 'bottom', name: 'Player Bottom', melds: [] }]}
      />,
    );

    const tableStage = container.querySelector('.table-stage') as HTMLElement | null;

    expect(tableStage?.style.getPropertyValue('--table-stage-local-info-guard-bottom')).toBe('173px');

    unmount();
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: originalWidth });
    Object.defineProperty(window, 'innerHeight', { configurable: true, value: originalHeight });
  });

  it('shows the current latest discard in a larger spotlight near the discarding seat', () => {
    const { container } = render(
      <TableStage
        discards={{
          top: [],
          left: ['b1', 'b2'],
          right: [],
          bottom: [],
        }}
        activeSeat="top"
        lastDiscard="b2"
        lastDiscardSeat="left"
        promptText={null}
      />,
    );

    expect(container.querySelector('.table-stage__spotlight--left .table-stage__spotlight-tile')).not.toBeNull();
    expect(container.querySelector('.table-stage__river-track--left')?.querySelectorAll('.mahjong-tile')).toHaveLength(1);
  });

  it('does not render the action pointer when the current prompt has no unique public actor', () => {
    render(
      <TableStage
        discards={{
          top: [],
          left: ['b1'],
          right: [],
          bottom: [],
        }}
        activeSeat="left"
        actionIndicatorSeat={null}
        lastDiscard="b1"
        lastDiscardSeat="left"
        promptText="一名玩家正在响应"
        promptCue={{
          kind: 'claim',
          tone: 'urgent',
          title: '左家刚打出可响应牌',
          detail: '你可以 吃 / 过',
          actionIds: ['chow', 'pass'],
          highlightedActionIds: ['chow'],
          sourceSeat: 'left',
          isUrgent: true,
        }}
      />,
    );

    expect(screen.queryByLabelText(/正在行动/)).toBeNull();
  });

  it.each([
    { seat: 'left', expectedTransform: 'rotate(90 50 50)' },
    { seat: 'right', expectedTransform: 'rotate(-90 50 50)' },
  ] as const)('rotates the center indicator pointer toward the $seat seat', ({ seat, expectedTransform }) => {
    const { container } = render(
      <TableStage
        discards={{
          top: [],
          left: [],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        actionIndicatorSeat={seat}
        lastDiscard={null}
        promptText={null}
      />,
    );

    const pointer = container.querySelector('.table-stage__center-indicator-pointer');
    expect(pointer).not.toBeNull();
    expect(pointer?.getAttribute('transform')).toBe(expectedTransform);
  });

  it('keeps the center indicator pointer on the shortest counterclockwise path when moving from right to top', () => {
    const props = {
      discards: {
        top: [],
        left: [],
        right: [],
        bottom: [],
      },
      activeSeat: 'bottom' as const,
      lastDiscard: null,
      promptText: null,
    };

    const { container, rerender } = render(<TableStage {...props} actionIndicatorSeat="right" />);

    rerender(<TableStage {...props} actionIndicatorSeat="top" />);

    const pointer = container.querySelector('.table-stage__center-indicator-pointer');
    expect(pointer).not.toBeNull();
    expect(pointer?.getAttribute('transform')).toBe('rotate(-180 50 50)');
  });

  it('keeps the center countdown progress when the active seat changes without a new deadline string', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-04-20T12:00:00Z'));

    const deadlineAt = '2026-04-20T12:00:05Z';
    const discards = {
      top: [],
      left: [],
      right: [],
      bottom: [],
    };
    const readCountdownOffset = (container: HTMLElement) =>
      Number.parseFloat(
        container.querySelector('.table-stage__center-indicator-countdown')?.getAttribute('stroke-dashoffset') ?? 'NaN',
      );

    const { container, rerender } = render(
      <TableStage
        discards={discards}
        activeSeat="bottom"
        actionIndicatorSeat="bottom"
        lastDiscard={null}
        promptText={null}
        deadlineAt={deadlineAt}
      />,
    );

    act(() => {
      vi.advanceTimersByTime(2000);
    });

    const countdownBefore = readCountdownOffset(container);
    expect(countdownBefore).toBeGreaterThan(0);

    rerender(
      <TableStage
        discards={discards}
        activeSeat="right"
        actionIndicatorSeat="right"
        lastDiscard={null}
        promptText={null}
        deadlineAt={deadlineAt}
      />,
    );

    expect(readCountdownOffset(container)).toBeCloseTo(countdownBefore, 1);
  });

  it('keeps the center countdown progress when the response indicator seat changes without a new deadline string', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-04-20T12:00:00Z'));

    const deadlineAt = '2026-04-20T12:00:05Z';
    const discards = {
      top: [],
      left: [],
      right: [],
      bottom: [],
    };
    const readCountdownOffset = (container: HTMLElement) =>
      Number.parseFloat(
        container.querySelector('.table-stage__center-indicator-countdown')?.getAttribute('stroke-dashoffset') ?? 'NaN',
      );

    const { container, rerender } = render(
      <TableStage
        discards={discards}
        activeSeat="bottom"
        actionIndicatorSeat="left"
        lastDiscard={null}
        promptText={null}
        deadlineAt={deadlineAt}
      />,
    );

    act(() => {
      vi.advanceTimersByTime(2000);
    });

    const countdownBefore = readCountdownOffset(container);
    expect(countdownBefore).toBeGreaterThan(0);

    rerender(
      <TableStage
        discards={discards}
        activeSeat="bottom"
        actionIndicatorSeat="top"
        lastDiscard={null}
        promptText={null}
        deadlineAt={deadlineAt}
      />,
    );

    expect(readCountdownOffset(container)).toBeCloseTo(countdownBefore, 1);
  });

  it('renders a partial countdown immediately when the page mounts mid-turn', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-04-20T12:00:00Z'));

    const discards = {
      top: [],
      left: [],
      right: [],
      bottom: [],
    };
    const readCountdownOffset = (container: HTMLElement) =>
      Number.parseFloat(
        container.querySelector('.table-stage__center-indicator-countdown')?.getAttribute('stroke-dashoffset') ?? 'NaN',
      );

    const { container } = render(
      <TableStage
        discards={discards}
        activeSeat="top"
        actionIndicatorSeat="top"
        lastDiscard={null}
        promptText={null}
        deadlineAt="2026-04-20T12:00:10Z"
      />,
    );

    act(() => {
      vi.advanceTimersByTime(20);
    });

    expect(readCountdownOffset(container)).toBeGreaterThan(0);
  });

  it('renders a full center ring without dash trimming when there is no countdown deadline', () => {
    const discards = {
      top: [],
      left: [],
      right: [],
      bottom: [],
    };

    const { container } = render(
      <TableStage
        discards={discards}
        activeSeat="bottom"
        actionIndicatorSeat={null}
        lastDiscard={null}
        promptText={null}
        deadlineAt={null}
      />,
    );

    const countdownRing = container.querySelector('.table-stage__center-indicator-countdown');

    expect(countdownRing?.hasAttribute('stroke-dasharray')).toBe(false);
    expect(countdownRing?.hasAttribute('stroke-dashoffset')).toBe(false);
  });

  it('renders a closed center ring when only a small amount of countdown time has elapsed', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-04-20T12:00:00Z'));

    const discards = {
      top: [],
      left: [],
      right: [],
      bottom: [],
    };

    const { container } = render(
      <TableStage
        discards={discards}
        activeSeat="bottom"
        actionIndicatorSeat="bottom"
        lastDiscard={null}
        promptText={null}
        deadlineAt="2026-04-20T12:00:29.800Z"
      />,
    );

    const countdownRing = container.querySelector('.table-stage__center-indicator-countdown');

    expect(countdownRing?.hasAttribute('stroke-dasharray')).toBe(false);
    expect(countdownRing?.hasAttribute('stroke-dashoffset')).toBe(false);
  });

  it('stops the previous countdown loop after the center timer is cleared', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-04-20T12:00:00Z'));

    const discards = {
      top: [],
      left: [],
      right: [],
      bottom: [],
    };
    const readCountdownOffset = (container: HTMLElement) =>
      Number.parseFloat(
        container.querySelector('.table-stage__center-indicator-countdown')?.getAttribute('stroke-dashoffset') ?? 'NaN',
      );

    const { container, rerender } = render(
      <TableStage
        discards={discards}
        activeSeat="bottom"
        actionIndicatorSeat="bottom"
        lastDiscard={null}
        promptText={null}
        deadlineAt="2026-04-20T12:00:05Z"
      />,
    );

    act(() => {
      vi.advanceTimersByTime(2000);
    });

    expect(readCountdownOffset(container)).toBeGreaterThan(0);

    const countdownRing = container.querySelector('.table-stage__center-indicator-countdown');

    rerender(
      <TableStage
        discards={discards}
        activeSeat="right"
        actionIndicatorSeat={null}
        lastDiscard={null}
        promptText={null}
        deadlineAt={null}
      />,
    );

    expect(countdownRing?.hasAttribute('stroke-dasharray')).toBe(false);
    expect(countdownRing?.hasAttribute('stroke-dashoffset')).toBe(false);

    act(() => {
      vi.advanceTimersByTime(1000);
    });

    expect(countdownRing?.hasAttribute('stroke-dasharray')).toBe(false);
    expect(countdownRing?.hasAttribute('stroke-dashoffset')).toBe(false);
  });

  it('does not apply a separate dealer style modifier to the spotlight', () => {
    const { container } = render(
      <TableStage
        discards={{
          top: [],
          left: [],
          right: [],
          bottom: ['w1'],
        }}
        activeSeat="bottom"
        lastDiscard="w1"
        lastDiscardSeat="bottom"
        promptText={null}
        players={[{ seat: 'bottom', name: 'Player Bottom', melds: [], isDealer: true }]}
      />,
    );

    expect(container.querySelector('.table-stage__spotlight--bottom')).not.toHaveClass('table-stage__spotlight--dealer');
  });

  it('uses the provided discard seat to avoid spotlighting the same tile in another river', () => {
    const { container } = render(
      <TableStage
        discards={{
          top: ['w5'],
          left: ['w5'],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard="w5"
        lastDiscardSeat="left"
        promptText={null}
      />,
    );

    expect(container.querySelector('.table-stage__spotlight--left .table-stage__spotlight-tile')).not.toBeNull();
    expect(container.querySelector('.table-stage__river-track--top')?.querySelectorAll('.mahjong-tile')).toHaveLength(1);
    expect(container.querySelector('.table-stage__river-track--left')?.querySelectorAll('.mahjong-tile')).toHaveLength(0);
  });

  it('renders the pre-match room actions in the table center and keeps the corner leave button', () => {
    const onAddBot = vi.fn();
    const onRemoveBot = vi.fn();

    render(
      <TableStage
        discards={{
          top: [],
          left: [],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard={null}
        promptText={null}
        canLeaveTable
        onLeaveTable={() => undefined}
        onAction={() => undefined}
        botCount={2}
        canAddBot
        canRemoveBot
        onAddBot={onAddBot}
        onRemoveBot={onRemoveBot}
        preMatchActions={[
          { id: 'ready', label: '准备', enabled: true, emphasis: 'medium' },
          { id: 'start_match', label: '开始对局', enabled: true, emphasis: 'high' },
        ]}
      />,
    );

    expect(screen.getByRole('button', { name: '快捷离开牌桌' })).toBeInTheDocument();
    expect(screen.getByRole('group', { name: '开局前房间操作' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '准备' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '开始对局' })).toBeInTheDocument();
    expect(screen.getByRole('group', { name: 'BOT 数量控制' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '增加 BOT' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '减少 BOT' })).toBeInTheDocument();
    expect(screen.getByText('2')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '增加 BOT' }));
    fireEvent.click(screen.getByRole('button', { name: '减少 BOT' }));

    expect(onAddBot).toHaveBeenCalledTimes(1);
    expect(onRemoveBot).toHaveBeenCalledTimes(1);
  });

  it('mutes score, flower, and hand stat plates for unready players while waiting for match start', () => {
    const { container } = render(
      <TableStage
        discards={{
          top: [],
          left: [],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard={null}
        promptText={null}
        isWaitingForMatchStart
        players={[
          {
            seat: 'bottom',
            name: 'Player Bottom',
            melds: [],
            ready: false,
            score: 25000,
            flowerCount: 1,
            concealedCount: 13,
          },
          {
            seat: 'top',
            name: 'Player Top',
            melds: [],
            ready: true,
            score: 25000,
            flowerCount: 2,
            concealedCount: 13,
          },
        ]}
      />,
    );

    const bottomZone = container.querySelector('.table-stage__seat-zone--bottom');
    const topZone = container.querySelector('.table-stage__seat-zone--top');

    expect(bottomZone?.querySelector('.table-stage__stat-plate--score')).toHaveClass('table-stage__stat-plate--muted');
    expect(bottomZone?.querySelector('.table-stage__stat-plate--flower')).toHaveClass('table-stage__stat-plate--muted');
    expect(bottomZone?.querySelector('.table-stage__stat-plate--hand')).toHaveClass('table-stage__stat-plate--muted');
    expect(topZone?.querySelector('.table-stage__stat-plate--score')).not.toHaveClass('table-stage__stat-plate--muted');
    expect(topZone?.querySelector('.table-stage__stat-plate--flower')).not.toHaveClass('table-stage__stat-plate--muted');
    expect(topZone?.querySelector('.table-stage__stat-plate--hand')).not.toHaveClass('table-stage__stat-plate--muted');
  });

  it('opens the quick-chat radial menu from the global emoji trigger', async () => {
    const user = userEvent.setup();

    render(
      <TableStage
        discards={{
          top: [],
          left: [],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard={null}
        promptText={null}
        players={[
          { seat: 'bottom', name: 'Player A', isLocal: true, absoluteSeat: 0, melds: [] },
        ]}
      />,
    );

    const trigger = screen.getByRole('button', { name: '打开快捷表情' });

    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByRole('menu', { name: 'Player A 快捷表情' })).toBeNull();

    await user.click(trigger);

    const menu = screen.getByRole('menu', { name: 'Player A 快捷表情' });

    expect(trigger).toHaveAttribute('aria-expanded', 'true');
    expect(within(menu).getAllByRole('menuitem')).toHaveLength(6);
  });

  it('renders a theme switch button beside the leave control and forwards clicks', () => {
    const onCycleTheme = vi.fn();

    render(
      <TableStage
        discards={{
          top: [],
          left: [],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard={null}
        promptText={null}
        canLeaveTable
        themeId="qiu-xiang"
        themeLabel="秋香"
        onLeaveTable={() => undefined}
        onCycleTheme={onCycleTheme}
      />,
    );

    const themeButton = screen.getByRole('button', { name: '切换整体配色，当前 秋香' });

    expect(themeButton).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '快捷离开牌桌' })).toBeInTheDocument();

    fireEvent.click(themeButton);

    expect(onCycleTheme).toHaveBeenCalledTimes(1);
  });

  it('opens the fan guide dialog from the corner help button and shows all fan guide entries in one scrollable list', () => {
    render(
      <TableStage
        discards={{
          top: [],
          left: [],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard={null}
        promptText={null}
        canLeaveTable
        themeId="qiu-xiang"
        themeLabel="秋香"
        onLeaveTable={() => undefined}
        onCycleTheme={() => undefined}
      />,
    );

    const fullscreenButton = screen.getByRole('button', { name: '全屏显示' });
    const helpButton = screen.getByRole('button', { name: '打开国标麻将番种说明' });
    const themeButton = screen.getByRole('button', { name: '切换整体配色，当前 秋香' });
    const controls = helpButton.parentElement;

    expect(controls?.firstElementChild).toBe(fullscreenButton);
    expect(controls?.children[1]).toBe(helpButton);
    expect(controls?.children[2]).toBe(themeButton);

    fireEvent.click(helpButton);

    const dialog = screen.getByRole('dialog', { name: '国标麻将番种说明' });

    expect(within(dialog).getByText('自摸')).toBeInTheDocument();
    expect(within(dialog).getByText('一般高')).toBeInTheDocument();
    expect(within(dialog).getByText('无字')).toBeInTheDocument();
    expect(within(dialog).getByText('大三元')).toBeInTheDocument();
    expect(within(dialog).queryByRole('group', { name: '番种说明分页' })).toBeNull();
    expect(within(dialog).queryByRole('button', { name: '下一页' })).toBeNull();
    expect(dialog.querySelector('.fan-guide__content')).not.toBeNull();

    fireEvent.click(within(dialog).getByRole('button', { name: '关闭番种说明' }));

    expect(screen.queryByRole('dialog', { name: '国标麻将番种说明' })).toBeNull();
  });

  it('keeps the fan guide dialog open when the backdrop is clicked', () => {
    render(
      <TableStage
        discards={{
          top: [],
          left: [],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard={null}
        promptText={null}
        canLeaveTable
        themeId="qiu-xiang"
        themeLabel="秋香"
        onLeaveTable={() => undefined}
        onCycleTheme={() => undefined}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '打开国标麻将番种说明' }));

    const dialog = screen.getByRole('dialog', { name: '国标麻将番种说明' });
    const backdrop = document.querySelector('.fan-guide__backdrop');

    expect(backdrop).not.toBeNull();
    fireEvent.click(backdrop!);

    expect(dialog).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '关闭番种说明' })).toBeInTheDocument();
  });

  it('keeps the fan guide dialog open when Escape is pressed', () => {
    render(
      <TableStage
        discards={{
          top: [],
          left: [],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard={null}
        promptText={null}
        canLeaveTable
        themeId="qiu-xiang"
        themeLabel="秋香"
        onLeaveTable={() => undefined}
        onCycleTheme={() => undefined}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '打开国标麻将番种说明' }));
    fireEvent.keyDown(window, { key: 'Escape' });

    expect(screen.getByRole('dialog', { name: '国标麻将番种说明' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '关闭番种说明' })).toBeInTheDocument();
  });

  it('shows a themed callout on the matching seat when a chow, pung, kong, or hu effect arrives', () => {
    const { container } = render(
      <TableStage
        discards={{
          top: [],
          left: ['b1'],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard="b1"
        lastDiscardSeat="left"
        promptText={null}
        actionEffect={{
          key: 'claim-1',
          label: '碰',
          emphasis: 'claim',
          seat: 'right',
          calloutTone: 'pung',
        }}
      />,
    );

    expect(screen.getByText('碰')).toBeInTheDocument();
    expect(container.querySelector('.table-stage__action-callout.table-stage__spotlight--right')).not.toBeNull();
    expect(container.querySelector('.table-stage__action-callout--pung')).not.toBeNull();
  });

  it('renders the ready_hand callout with the matching themed class', () => {
    const { container } = render(
      <TableStage
        discards={{
          top: [],
          left: ['b1'],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard="b1"
        lastDiscardSeat="left"
        promptText={null}
        actionEffect={{
          key: 'ready-hand-1',
          label: '听',
          emphasis: 'claim',
          seat: 'right',
          calloutTone: 'ready_hand',
        }}
      />,
    );

    expect(screen.getByText('听')).toBeInTheDocument();
    expect(container.querySelector('.table-stage__action-callout.table-stage__spotlight--right')).not.toBeNull();
    expect(container.querySelector('.table-stage__action-callout--ready_hand')).not.toBeNull();
  });

  it('keeps the incoming ready_hand discard hidden on the initial render frame', () => {
    const markup = renderToStaticMarkup(
      <TableStage
        discards={{
          top: [],
          left: ['b1'],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard="b1"
        lastDiscardSeat="left"
        promptText={null}
        actionEffect={{
          key: 'ready-hand-initial-frame-1',
          label: '听',
          emphasis: 'claim',
          seat: 'left',
          calloutTone: 'ready_hand',
        }}
      />,
    );
    const container = document.createElement('div');
    container.innerHTML = markup;

    expect(container.querySelector('.table-stage__spotlight--left .table-stage__spotlight-tile')).toBeNull();
    expect(container.querySelector('.table-stage__river-track--left')?.querySelector('.mahjong-tile')).toBeNull();
  });

  it('delays showing the latest discard until the ready_hand callout disappears', () => {
    vi.useFakeTimers();

    const { container } = render(
      <TableStage
        discards={{
          top: [],
          left: ['b1'],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard="b1"
        lastDiscardSeat="left"
        promptText={null}
        actionEffect={{
          key: 'ready-hand-delay-1',
          label: '听',
          emphasis: 'claim',
          seat: 'left',
          calloutTone: 'ready_hand',
        }}
      />,
    );

    expect(screen.getByText('听')).toBeInTheDocument();
    expect(container.querySelector('.table-stage__spotlight--left .table-stage__spotlight-tile')).toBeNull();
    expect(container.querySelector('.table-stage__river-track--left')?.querySelectorAll('.mahjong-tile')).toHaveLength(0);

    act(() => {
      vi.advanceTimersByTime(999);
    });

    expect(screen.getByText('听')).toBeInTheDocument();
    expect(container.querySelector('.table-stage__spotlight--left .table-stage__spotlight-tile')).toBeNull();

    act(() => {
      vi.advanceTimersByTime(1);
    });

    expect(screen.queryByText('听')).toBeNull();
    expect(container.querySelector('.table-stage__spotlight--left .table-stage__spotlight-tile')).not.toBeNull();
    vi.useRealTimers();
  });

  it('keeps non-ready_hand callouts at the existing three-second duration', () => {
    vi.useFakeTimers();

    render(
      <TableStage
        discards={{
          top: [],
          left: ['b1'],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard="b1"
        lastDiscardSeat="left"
        promptText={null}
        actionEffect={{
          key: 'pung-duration-1',
          label: '碰',
          emphasis: 'claim',
          seat: 'right',
          calloutTone: 'pung',
        }}
      />,
    );

    expect(screen.getByText('碰')).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(2999);
    });

    expect(screen.getByText('碰')).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(1);
    });

    expect(screen.queryByText('碰')).toBeNull();
    vi.useRealTimers();
  });

  it.each([
    {
      name: '荣和',
      settlementWinType: 'discard',
      settlementWinTypeLabel: '荣和',
      className: '.table-stage__action-callout--hu-discard',
    },
    {
      name: '自摸',
      settlementWinType: 'self_draw',
      settlementWinTypeLabel: '自摸',
      className: '.table-stage__action-callout--hu-self-draw',
    },
  ])('renders the $name hu callout variant with the matching themed class', ({ settlementWinType, settlementWinTypeLabel, className }) => {
    const { container } = render(
      <TableStage
        discards={{
          top: [],
          left: ['b1'],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard="b1"
        lastDiscardSeat="left"
        settlementWinnerSeat="right"
        settlementWinType={settlementWinType}
        settlementWinTypeLabel={settlementWinTypeLabel}
        promptText={null}
        actionEffect={{
          key: `hu-${settlementWinTypeLabel}`,
          label: '和牌',
          emphasis: 'claim',
          seat: null,
          calloutTone: 'hu',
        }}
      />,
    );

    expect(screen.getByText('和')).toBeInTheDocument();
    expect(container.querySelector('.table-stage__action-callout.table-stage__spotlight--right')).not.toBeNull();
    expect(container.querySelector(className)).not.toBeNull();
  });

  it('renders all discard hu callouts together when settlement has multiple winners', () => {
    const { container } = render(
      <TableStage
        discards={{
          top: [],
          left: ['b1'],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard="b1"
        lastDiscardSeat="left"
        settlementWinnerSeat="right"
        settlementWinnerSeats={['right', 'top']}
        settlementWinType="discard"
        settlementWinTypeLabel="荣和"
        promptText={null}
        actionEffect={{
          key: 'hu-2',
          label: '和牌',
          emphasis: 'claim',
          seat: 'top',
          calloutTone: 'hu',
        }}
      />,
    );

    expect(container.querySelectorAll('.table-stage__action-callout--hu')).toHaveLength(2);
    expect(container.querySelectorAll('.table-stage__action-callout--hu-discard')).toHaveLength(2);
    expect(container.querySelector('.table-stage__action-callout.table-stage__spotlight--right')).not.toBeNull();
    expect(container.querySelector('.table-stage__action-callout.table-stage__spotlight--top')).not.toBeNull();
  });

  it('queues multiple hu callouts and shows them one by one', () => {
    vi.useFakeTimers();

    const props = {
      discards: {
        top: [],
        left: ['b1'],
        right: [],
        bottom: [],
      },
      activeSeat: 'bottom' as const,
      lastDiscard: 'b1',
      lastDiscardSeat: 'left' as const,
      promptText: null,
    };

    const { container, rerender } = render(
      <TableStage
        {...props}
        actionEffect={{
          key: 'hu-1',
          label: '和牌',
          emphasis: 'claim',
          seat: 'right',
          calloutTone: 'hu',
        }}
      />,
    );

    expect(container.querySelector('.table-stage__action-callout.table-stage__spotlight--right')).not.toBeNull();

    rerender(
      <TableStage
        {...props}
        actionEffect={{
          key: 'hu-2',
          label: '和牌',
          emphasis: 'claim',
          seat: 'top',
          calloutTone: 'hu',
        }}
      />,
    );

    expect(container.querySelector('.table-stage__action-callout.table-stage__spotlight--right')).not.toBeNull();
    expect(container.querySelector('.table-stage__action-callout.table-stage__spotlight--top')).toBeNull();

    act(() => {
      vi.advanceTimersByTime(3000);
    });

    expect(container.querySelector('.table-stage__action-callout.table-stage__spotlight--right')).toBeNull();
    expect(container.querySelector('.table-stage__action-callout.table-stage__spotlight--top')).not.toBeNull();

    act(() => {
      vi.advanceTimersByTime(3000);
    });

    expect(container.querySelector('.table-stage__action-callout')).toBeNull();
    vi.useRealTimers();
  });

  it('fades the action callout after three seconds', () => {
    vi.useFakeTimers();

    render(
      <TableStage
        discards={{
          top: [],
          left: ['b1'],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard="b1"
        lastDiscardSeat="left"
        promptText={null}
        actionEffect={{
          key: 'claim-1',
          label: '吃',
          emphasis: 'claim',
          seat: 'left',
          calloutTone: 'chow',
        }}
      />,
    );

    expect(screen.getByText('吃')).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(3000);
    });

    expect(screen.queryByText('吃')).toBeNull();
    vi.useRealTimers();
  });

  it('keeps the current callout when a later non-callout action arrives', () => {
    vi.useFakeTimers();

    const { container, rerender } = render(
      <TableStage
        discards={{
          top: [],
          left: ['b1'],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard="b1"
        lastDiscardSeat="left"
        promptText={null}
        actionEffect={{
          key: 'claim-1',
          label: '碰',
          emphasis: 'claim',
          seat: 'left',
          calloutTone: 'pung',
        }}
      />,
    );

    rerender(
      <TableStage
        discards={{
          top: [],
          left: ['b1'],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard="b1"
        lastDiscardSeat="left"
        promptText={null}
        actionEffect={{
          key: 'draw-2',
          label: '摸牌',
          emphasis: 'draw',
          seat: 'top',
          calloutTone: null,
        }}
      />,
    );

    expect(screen.getByText('碰')).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(250);
    });

    expect(container.querySelector('.table-stage__action-callout--exit')).toBeNull();
    expect(screen.getByText('碰')).toBeInTheDocument();
    vi.useRealTimers();
  });

  it('clears the previous callout immediately when a new spotlight appears on the same seat', () => {
    vi.useFakeTimers();

    const { container, rerender } = render(
      <TableStage
        discards={{
          top: [],
          left: ['b1'],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard="b1"
        lastDiscardSeat="left"
        promptText={null}
        actionEffect={{
          key: 'claim-1',
          label: '碰',
          emphasis: 'claim',
          seat: 'left',
          calloutTone: 'pung',
        }}
      />,
    );

    rerender(
      <TableStage
        discards={{
          top: [],
          left: ['b1', 'b2'],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard="b2"
        lastDiscardSeat="left"
        promptText={null}
        actionEffect={{
          key: 'draw-2',
          label: '摸牌',
          emphasis: 'draw',
          seat: 'top',
          calloutTone: null,
        }}
      />,
    );

    expect(container.querySelector('.table-stage__action-callout')).toBeNull();
    vi.useRealTimers();
  });

  it('renders quick-chat barrage text above the table felt and below the tiles layer', () => {
    const { rerender } = render(
      <TableStage
        discards={{
          top: ['w1'],
          left: [],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard={null}
        promptText={null}
      />,
    );

    rerender(
      <TableStage
        discards={{
          top: ['w1'],
          left: [],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        lastDiscard={null}
        promptText={null}
        quickChatEvent={{
          key: 'quick-chat-1',
          actorSeat: 'bottom',
          targetSeat: 'right',
          actorName: 'Player A',
          targetName: 'Player B',
          emoji: '🀄',
          text: 'Player A -> Player B : 🀄',
        }}
      />,
    );

    expect(screen.getByText('Player A -> Player B : 🀄')).toBeInTheDocument();
    expect(document.querySelector('.table-stage__barrage-layer')).not.toBeNull();
  });
});
