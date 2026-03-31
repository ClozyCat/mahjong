import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { BattleActionView, BattleViewModel } from '../../types/match';
import { BottomActionDock } from './BottomActionDock';

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
        onTileSelect={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    expect(screen.getByLabelText(/local hand/i)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '收起手牌区' }));

    expect(screen.queryByLabelText(/local hand/i)).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: '展开手牌区' })).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '展开手牌区' }));

    expect(screen.getByLabelText(/local hand/i)).toBeInTheDocument();
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
        onTileSelect={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    const huButton = screen.getByRole('button', { name: '和牌' });
    const pungButton = screen.getByRole('button', { name: '碰' });
    const passButton = screen.getByRole('button', { name: '过' });

    expect(document.body.querySelector('.action-dock--actionable')).toBeNull();
    expect(document.body.querySelector('.action-dock--elevated')).toBeNull();
    expect(screen.queryByText('左家刚打出可响应牌')).toBeNull();
    expect(huButton).toHaveClass('action-dock__action--response-glow', 'action-dock__action--response-glow-hu');
    expect(pungButton).toHaveClass('action-dock__action--response-glow', 'action-dock__action--response-glow-pung');
    expect(passButton).not.toHaveClass('action-dock__action--response-glow');
    expect(screen.getByRole('button', { name: '收起手牌区' })).toBeInTheDocument();
  });

  it('shows all enabled local-turn actions without turning the dock into a response highlight', () => {
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
        onTileSelect={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    expect(document.body.querySelector('.action-dock--elevated')).not.toBeNull();
    expect(document.body.querySelector('.action-dock--actionable')).toBeNull();
    expect(screen.getByRole('button', { name: '出牌' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '杠' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '和牌' })).toBeInTheDocument();
  });

  it('keeps pass available during opening flower prompts', () => {
    render(
      <BottomActionDock
        hand={localHand}
        actions={[
          { id: 'flower', label: '补花', enabled: true, emphasis: 'medium' },
          { id: 'pass', label: '过', enabled: true, emphasis: 'low' },
        ]}
        isElevated
        promptCue={{
          kind: 'turn',
          tone: 'info',
          title: '当前可以补花',
          detail: '你可以 过',
          actionIds: ['pass'],
          highlightedActionIds: [],
          sourceSeat: null,
          isUrgent: false,
        }}
        deadlineAt="2099-03-30T12:10:40+08:00"
        onTileSelect={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    expect(screen.getByRole('button', { name: '过' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '补花' })).toBeNull();
  });

  it('derives dock width variables from the current hand count', () => {
    render(
      <BottomActionDock
        hand={[
          { tileId: 'w1#1', code: 'w1', isSelected: false, isDrawn: false, isFlower: false },
          { tileId: 'w2#2', code: 'w2', isSelected: false, isDrawn: false, isFlower: false },
          { tileId: 'w3#3', code: 'w3', isSelected: false, isDrawn: false, isFlower: false },
        ]}
        actions={[]}
        isElevated={false}
        promptCue={null}
        deadlineAt={null}
        onTileSelect={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    const dock = screen.getByTestId('action-dock');

    expect(dock).toHaveStyle({
      '--action-dock-hand-count': '3',
      '--action-dock-effective-hand-count': '3',
      '--action-dock-gap-count': '2',
    });
  });
});
