import { act, fireEvent, render, screen, within } from '@testing-library/react';
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

  it('right-aligns a short right-side settlement hand within the final row', () => {
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
        players={[{ seat: 'right', name: 'Player Right', melds: [] }]}
        settlementHands={{
          right: ['c1'],
        }}
      />,
    );

    const rightSettlementGrid = container.querySelector('.table-stage__settlement-hand-grid--right');
    expect(rightSettlementGrid?.querySelectorAll('.table-stage__settlement-hand-placeholder')).toHaveLength(3);
    expect(rightSettlementGrid?.querySelectorAll('.mahjong-tile--discard')).toHaveLength(1);
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

    const helpButton = screen.getByRole('button', { name: '打开国标麻将番种说明' });
    const themeButton = screen.getByRole('button', { name: '切换整体配色，当前 秋香' });
    const controls = helpButton.parentElement;

    expect(controls?.firstElementChild).toBe(helpButton);
    expect(controls?.children[1]).toBe(themeButton);

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
    {
      name: '屁和',
      settlementWinType: 'discard',
      settlementWinTypeLabel: '屁和',
      className: '.table-stage__action-callout--hu-low-fan',
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
          label: '胡牌',
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

