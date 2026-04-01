import { act, fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { TableStage } from './TableStage';

describe('TableStage', () => {
  it('renders every discard tile in wide seat lanes without losing center metadata', () => {
    render(
      <TableStage
        discards={{
          top: Array.from({ length: 12 }, (_, index) => `w${(index % 9) + 1}`),
          left: Array.from({ length: 10 }, (_, index) => `b${(index % 9) + 1}`),
          right: Array.from({ length: 10 }, (_, index) => `c${(index % 9) + 1}`),
          bottom: Array.from({ length: 12 }, (_, index) => `w${(index % 9) + 1}`),
        }}
        activeSeat="top"
        lastDiscard="w3"
        lastDiscardSeat="top"
        remainingTileCount={66}
        promptText="Claim window open"
        tableCode="111"
        occupiedSeatCount={4}
        seatCapacity={4}
        roundLabel="东3局"
        phaseLabel="进行中"
      />,
    );

    expect(screen.getAllByTestId('mahjong-tile').length).toBe(44);
    expect(screen.getByText(/claim window open/i)).toBeInTheDocument();
    expect(screen.getByText('剩余 66 张')).toBeInTheDocument();
    expect(screen.getByText('牌桌编号：111')).toBeInTheDocument();
    expect(screen.getByText('房间座位数：4/4')).toBeInTheDocument();
    expect(screen.getByText('东3局 | 进行中')).toBeInTheDocument();
    expect(screen.queryByText('三万')).not.toBeInTheDocument();
    expect(screen.queryByText('Table Core')).not.toBeInTheDocument();
    expect(screen.queryByText(/最新出牌/)).not.toBeInTheDocument();
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

  it('renders a small action pointer between the center info and the acting seat when a public action seat is provided', () => {
    const { container } = render(
      <TableStage
        discards={{
          top: [],
          left: ['b1', 'b2'],
          right: [],
          bottom: [],
        }}
        activeSeat="left"
        actionIndicatorSeat="left"
        lastDiscard="b2"
        lastDiscardSeat="left"
        promptText="左家正在出牌"
      />,
    );

    expect(screen.getByLabelText('左家正在行动')).toHaveClass('table-stage__action-pointer--left');
    expect(container.querySelector('.table-stage__spotlight--left')).not.toBeNull();
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

  it('renders player info bars on the table and reuses the same accent on the spotlight seat', () => {
    const { container } = render(
      <TableStage
        discards={{
          top: ['w1'],
          left: ['b1', 'b2'],
          right: ['c1'],
          bottom: ['d1'],
        }}
        activeSeat="bottom"
        lastDiscard="b2"
        lastDiscardSeat="left"
        promptText={null}
        players={[
          {
            seat: 'top',
            name: 'Player Top',
            score: 26800,
            flowerCount: 0,
            wind: 'North',
            isDealer: false,
            isActive: false,
            isLocal: false,
            connected: true,
            concealedCount: 13,
            melds: [],
            statusText: 'Live',
          },
          {
            seat: 'left',
            name: 'Player Left',
            score: 24300,
            flowerCount: 1,
            wind: 'West',
            isDealer: false,
            isActive: false,
            isLocal: false,
            connected: true,
            concealedCount: 13,
            melds: [],
            statusText: 'Live',
          },
          {
            seat: 'right',
            name: 'Player Right',
            score: 25000,
            flowerCount: 0,
            wind: 'South',
            isDealer: false,
            isActive: false,
            isLocal: false,
            connected: true,
            concealedCount: 13,
            melds: [],
            statusText: 'Live',
          },
          {
            seat: 'bottom',
            name: 'Player Bottom',
            score: 25000,
            flowerCount: 0,
            wind: 'East',
            isDealer: true,
            isActive: true,
            isLocal: true,
            connected: true,
            concealedCount: 14,
            melds: [],
            statusText: 'Live',
          },
        ]}
      />,
    );

    expect(screen.getByLabelText('Player Top 信息栏')).toBeInTheDocument();
    expect(screen.getByLabelText('Player Left 信息栏')).toHaveTextContent('手牌 13 · 花 1');
    expect(screen.getByLabelText('Player Bottom 信息栏')).toHaveTextContent('手牌 14 · 花 0');
    expect(container.querySelector('.table-stage__spotlight--left')).not.toBeNull();
  });

  it('uses a unified theme-driven player info style and marks the dealer separately', () => {
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
          { seat: 'top', name: 'Player Top', melds: [] },
          { seat: 'left', name: 'Player Left', melds: [] },
          { seat: 'right', name: 'Player Right', melds: [] },
          { seat: 'bottom', name: 'Player Bottom', melds: [], isDealer: true },
        ]}
      />,
    );

    expect(screen.getByLabelText('Player Top 信息栏')).not.toHaveClass('table-stage__player-info--dealer');
    expect(screen.getByLabelText('Player Bottom 信息栏')).toHaveClass('table-stage__player-info--dealer');
  });

  it('marks the dealer spotlight with the dealer style modifier', () => {
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

    expect(container.querySelector('.table-stage__spotlight--bottom')).toHaveClass('table-stage__spotlight--dealer');
  });

  it('renders settlement hands beside each seat when the round has ended', () => {
    render(
      <TableStage
        discards={{
          top: [],
          left: [],
          right: [],
          bottom: [],
        }}
        activeSeat="bottom"
        promptText={null}
        players={[
          { seat: 'top', name: 'Player Top', melds: [] },
          { seat: 'left', name: 'Player Left', melds: [] },
          { seat: 'right', name: 'Player Right', melds: [] },
          { seat: 'bottom', name: 'Player Bottom', melds: [] },
        ]}
        settlementHands={{
          top: ['w1', 'w2'],
          left: ['b1', 'b2', 'b3', 'b4'],
          right: ['c1'],
          bottom: ['d1', 'd2'],
        }}
        lastDiscard="b4"
        lastDiscardSeat="top"
        settlementWinnerSeat="left"
        settlementWinType="discard"
      />,
    );

    expect(screen.getByLabelText('对家手牌').querySelectorAll('.mahjong-tile--discard')).toHaveLength(2);
    expect(screen.getByLabelText('左家手牌').querySelectorAll('.mahjong-tile--discard')).toHaveLength(4);
    expect(screen.getByLabelText('右家手牌').querySelectorAll('.mahjong-tile--discard')).toHaveLength(1);
    expect(screen.getByLabelText('左家手牌').querySelectorAll('.mahjong-tile--last-discard')).toHaveLength(1);
    expect(document.querySelector('.table-stage__spotlight')).toBeNull();
    expect(screen.queryByLabelText('本家手牌')).toBeNull();
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

  it('applies table tile scale variables to the table stage', () => {
    render(
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
        tileScale={1.12}
        canDecreaseTileScale
        canIncreaseTileScale
        onDecreaseTileScale={() => undefined}
        onIncreaseTileScale={() => undefined}
      />,
    );

    const table = screen.getByLabelText('Mahjong table');

    expect(table.style.getPropertyValue('--table-stage-tile-scale')).toBe('1.12');
    expect(table.style.getPropertyValue('--table-stage-spotlight-scale')).toBe('1.4');
    expect(screen.getByRole('group', { name: '调整牌桌牌面大小' })).toBeInTheDocument();
    expect(screen.getByText('112%')).toBeInTheDocument();
  });

  it('renders the pre-match room actions in the table center and keeps the corner leave button', () => {
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

  it('fast-fades the previous callout within 0.25s when a later action arrives', () => {
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

    expect(container.querySelector('.table-stage__action-callout--exit')).not.toBeNull();
    expect(screen.getByText('碰')).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(250);
    });

    expect(container.querySelector('.table-stage__action-callout')).toBeNull();
    vi.useRealTimers();
  });
});
