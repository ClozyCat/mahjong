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
  { id: 'hu', label: '和牌', enabled: true, emphasis: 'high' },
  { id: 'pung', label: '碰', enabled: true, emphasis: 'medium' },
  { id: 'pass', label: '过', enabled: true, emphasis: 'low' },
];

describe('BottomActionDock', () => {
  it('can collapse into a floating restore button and reopen', () => {
    render(
      <BottomActionDock
        hand={localHand}
        actions={[]}
        isElevated={false}
        promptCue={null}
        deadlineAt={null}
        waitingControls={null}
        localPlayer={localPlayer}
        onTileSelect={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    expect(screen.getByText('Player A')).toBeInTheDocument();
    expect(screen.getByText(/25,000 · 花 1 · Live/i)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '收起手牌区' }));

    expect(screen.queryByText('Player A')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: '展开手牌区' })).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '展开手牌区' }));

    expect(screen.getByText('Player A')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '展开手牌区' })).not.toBeInTheDocument();
  });

  it('keeps the dock free of battle action buttons', () => {
    render(
      <BottomActionDock
        hand={localHand}
        actions={[]}
        isElevated={false}
        promptCue={null}
        deadlineAt={null}
        waitingControls={{
          canReady: true,
          canStart: true,
          isReady: false,
          occupiedSeats: 4,
        }}
        localPlayer={localPlayer}
        onTileSelect={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    expect(screen.queryByText('等待下一步可执行操作')).toBeNull();
    expect(screen.queryByText('可用操作已上浮显示')).toBeNull();
    expect(screen.queryByRole('button', { name: '出牌' })).toBeNull();
    expect(screen.queryByRole('button', { name: '过' })).toBeNull();
    expect(screen.getByRole('button', { name: '收起手牌区' })).toBeInTheDocument();
    expect(document.body.querySelector('.action-dock__side-panel')).toBeNull();
    expect(document.body.querySelector('.action-dock__caption')).toBeNull();
  });

  it('keeps urgent actions out of the dock itself', () => {
    render(
      <BottomActionDock
        hand={localHand}
        actions={actions}
        isElevated
        promptCue={{
          kind: 'claim',
          tone: 'critical',
          title: '左家刚打出可响应牌',
          detail: '你可以 和牌 / 碰 / 过',
          actionIds: ['hu', 'pung', 'pass'],
          highlightedActionIds: ['hu', 'pung'],
          sourceSeat: 'left',
          isUrgent: true,
        }}
        deadlineAt="2099-03-30T12:10:40+08:00"
        waitingControls={null}
        localPlayer={localPlayer}
        onTileSelect={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    expect(document.body.querySelector('.action-dock--actionable')).not.toBeNull();
    expect(screen.queryByText('左家刚打出可响应牌')).toBeNull();
    expect(screen.getByRole('button', { name: '和牌' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '碰' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '过' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '收起手牌区' })).toBeInTheDocument();
  });

  it('shows discard in a normal local turn without turning the dock into a response highlight', () => {
    render(
      <BottomActionDock
        hand={localHand}
        actions={[
          { id: 'discard', label: '出牌', enabled: true, emphasis: 'high' },
          { id: 'kong', label: '杠', enabled: true, emphasis: 'medium' },
          { id: 'hu', label: '和牌', enabled: true, emphasis: 'high' },
        ]}
        isElevated
        promptCue={{
          kind: 'turn',
          tone: 'critical',
          title: '轮到你操作',
          detail: '你可以 出牌 / 杠 / 和牌',
          actionIds: ['discard', 'kong', 'hu'],
          highlightedActionIds: ['discard', 'kong', 'hu'],
          sourceSeat: null,
          isUrgent: false,
        }}
        deadlineAt="2099-03-30T12:10:40+08:00"
        waitingControls={null}
        localPlayer={localPlayer}
        onTileSelect={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    expect(document.body.querySelector('.action-dock--elevated')).not.toBeNull();
    expect(document.body.querySelector('.action-dock--actionable')).toBeNull();
    expect(screen.getByRole('button', { name: '出牌' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '杠' })).toBeNull();
    expect(screen.queryByRole('button', { name: '和牌' })).toBeNull();
  });
});
