import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { BattleActionView, BattleViewModel, PlayerView } from '../../types/match';
import { BottomActionDock } from './BottomActionDock';

const localPlayer: PlayerView = {
  seat: 'bottom',
  name: 'Player A',
  score: 25000,
  liveDelta: 4,
  flowerCount: 1,
  wind: 'East',
  isDealer: true,
  isActive: true,
  isLocal: true,
  connected: true,
  ready: true,
  concealedCount: 14,
  meldCount: 1,
  melds: [['w3', 'w3', 'w3']],
  flowers: ['f1'],
  statusText: 'Live',
};

const localHand: BattleViewModel['localHand'] = [
  { tileId: 'w1#1', code: 'w1', isSelected: false, isDrawn: false, isFlower: false },
  { tileId: 'w2#2', code: 'w2', isSelected: true, isDrawn: true, isFlower: false },
];

const actions: BattleActionView[] = [
  { id: 'discard', label: '出牌', enabled: true, emphasis: 'high' },
];

describe('BottomActionDock', () => {
  it('can collapse into a floating restore button and reopen', () => {
    render(
      <BottomActionDock
        hand={localHand}
        actions={actions}
        isElevated={false}
        waitingControls={null}
        localPlayer={localPlayer}
        onTileSelect={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    expect(screen.getByText('手牌控制区')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '收起手牌控制区' }));

    expect(screen.queryByText('手牌控制区')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: '展开手牌控制区' })).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '展开手牌控制区' }));

    expect(screen.getByText('手牌控制区')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '展开手牌控制区' })).not.toBeInTheDocument();
  });
});
