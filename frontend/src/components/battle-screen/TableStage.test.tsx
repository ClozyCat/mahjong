import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

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
      />,
    );

    expect(screen.getAllByTestId('mahjong-tile').length).toBe(44);
    expect(screen.getByText(/claim window open/i)).toBeInTheDocument();
    expect(screen.getByText('剩余 66 张')).toBeInTheDocument();
    expect(screen.getByText('最新出牌 三万')).toBeInTheDocument();
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
    render(
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
});
