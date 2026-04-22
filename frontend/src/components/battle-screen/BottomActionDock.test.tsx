import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
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
  it('renders the flower action with a blossom effect when the button is available', () => {
    render(
      <BottomActionDock
        hand={localHand}
        claimCandidates={[]}
        actions={[
          { id: 'flower', label: '补花', enabled: true, emphasis: 'high' },
          { id: 'pass', label: '过', enabled: true, emphasis: 'low' },
        ]}
        isElevated
        promptCue={{
          kind: 'turn',
          tone: 'info',
          title: '当前可以补花',
          detail: '你可以 补花 / 过',
          actionIds: ['flower', 'pass'],
          highlightedActionIds: ['flower'],
          sourceSeat: null,
          isUrgent: false,
        }}
        deadlineAt="2099-03-30T12:10:40+08:00"
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    expect(screen.getByRole('button', { name: '补花' })).toHaveClass('action-dock__action--flower-bloom');
    expect(screen.getByRole('button', { name: '过' })).toHaveClass(
      'action-dock__action--themed',
      'action-dock__action--themed-pass',
    );
  });

  it('treats the local kong prompt as a response-style action stack', () => {
    render(
      <BottomActionDock
        hand={localHand}
        claimCandidates={[]}
        actions={[
          { id: 'kong', label: '杠', enabled: true, emphasis: 'medium' },
          { id: 'pass', label: '过', enabled: true, emphasis: 'low' },
        ]}
        isElevated
        promptCue={{
          kind: 'turn_kong',
          tone: 'urgent',
          title: '当前可选择是否杠牌',
          detail: '你可以 杠 / 过',
          actionIds: ['kong', 'pass'],
          highlightedActionIds: ['kong'],
          sourceSeat: null,
          isUrgent: true,
        }}
        deadlineAt="2099-03-30T12:10:40+08:00"
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    expect(document.body.querySelector('.action-dock--elevated')).toBeNull();
    expect(screen.getByRole('button', { name: '杠' })).toHaveClass(
      'action-dock__action--themed',
      'action-dock__action--themed-kong',
    );
    expect(screen.getByRole('button', { name: '过' })).toHaveClass(
      'action-dock__action--themed',
      'action-dock__action--themed-pass',
    );
  });

  it('keeps the ready_hand button to the right of discard with the themed outline style', () => {
    render(
      <BottomActionDock
        hand={localHand}
        claimCandidates={[]}
        actions={[
          { id: 'discard', label: '出牌', enabled: true, emphasis: 'high' },
          { id: 'ready_hand', label: '听', enabled: true, emphasis: 'medium' },
        ]}
        isElevated
        promptCue={{
          kind: 'turn',
          tone: 'info',
          title: '当前可选择出牌或听牌',
          detail: '你可以 出牌 / 听',
          actionIds: ['discard', 'ready_hand'],
          highlightedActionIds: ['discard', 'ready_hand'],
          sourceSeat: null,
          isUrgent: false,
        }}
        deadlineAt="2099-03-30T12:10:40+08:00"
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    const actionLabels = screen
      .getByLabelText('即时操作按钮')
      .querySelectorAll('.action-dock__action-label');

    expect(Array.from(actionLabels, (node) => node.textContent)).toEqual(['出牌', '听']);
    expect(screen.getByRole('button', { name: '听' })).toHaveClass(
      'action-dock__action--themed',
      'action-dock__action--themed-ready-hand',
    );
  });

  it('toggles the ready-hand insight popover from the info trigger', async () => {
    const user = userEvent.setup();

    render(
      <BottomActionDock
        hand={localHand}
        selectedTileCode="w2"
        readyHandInsight={{
          source: 'current',
          discardTileId: null,
          discardTileCode: null,
          waits: [
            { code: 'w3', availableCount: 2 },
            { code: 'b5', availableCount: 1 },
          ],
        }}
        claimCandidates={[]}
        actions={[]}
        isElevated={false}
        promptCue={null}
        deadlineAt={null}
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    const trigger = screen.getByRole('button', { name: '查看当前听牌信息' });

    expect(screen.queryByRole('region', { name: '当前听牌信息' })).toBeNull();

    await user.click(trigger);

    expect(trigger).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByRole('region', { name: '当前听牌信息' })).toBeInTheDocument();
    expect(screen.getAllByRole('listitem')).toHaveLength(2);
  });

  it('hides the dock countdown when only other players are responding', () => {
    render(
      <BottomActionDock
        hand={localHand}
        claimCandidates={[]}
        actions={[]}
        isElevated={false}
        promptCue={null}
        deadlineAt="2099-03-30T12:10:40+08:00"
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    expect(screen.queryByLabelText(/剩余 \d+ 秒/)).toBeNull();
  });

  it('derives dock width variables from the current hand count', () => {
    render(
      <BottomActionDock
        hand={[
          { tileId: 'w1#1', code: 'w1', isSelected: false, isDrawn: false, isFlower: false },
          { tileId: 'w2#2', code: 'w2', isSelected: false, isDrawn: false, isFlower: false },
          { tileId: 'w3#3', code: 'w3', isSelected: false, isDrawn: false, isFlower: false },
        ]}
        claimCandidates={[]}
        actions={[]}
        isElevated={false}
        promptCue={null}
        deadlineAt={null}
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    const dock = screen.getByTestId('action-dock');

    expect(dock).toHaveStyle({
      '--action-dock-hand-count': '3',
      '--action-dock-effective-hand-count': '3',
      '--action-dock-gap-count': '2',
      '--action-dock-drawn-gap-count': '0',
    });
  });

  it('marks the freshly drawn tile button so layout can leave a gap before it', () => {
    render(
      <BottomActionDock
        hand={localHand}
        claimCandidates={[]}
        actions={[]}
        isElevated={false}
        promptCue={null}
        deadlineAt={null}
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    const hand = screen.getByLabelText(/local hand/i);
    const buttons = hand.querySelectorAll('.action-dock__tile');

    expect(buttons[0]).not.toHaveClass('action-dock__tile--drawn');
    expect(buttons[1]).toHaveClass('action-dock__tile--drawn');
    expect(screen.getByTestId('action-dock')).toHaveStyle({
      '--action-dock-drawn-gap-count': '1',
    });
  });

  it('uses a waiting placeholder width when the local hand is empty before match start', () => {
    render(
      <BottomActionDock
        hand={[]}
        claimCandidates={[]}
        actions={[]}
        isElevated={false}
        isWaitingForMatchStart
        promptCue={null}
        deadlineAt={null}
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    const dock = screen.getByTestId('action-dock');

    expect(dock).toHaveStyle({
      '--action-dock-hand-count': '0',
      '--action-dock-layout-hand-count': '13',
      '--action-dock-layout-gap-count': '12',
    });
    expect(screen.getByText('牌桌进入对局后，手牌和操作按钮会显示在这里。')).toBeInTheDocument();
  });

  it('renders claim candidate panes above the action buttons and forwards candidate clicks', () => {
    const onClaimCandidateSelect = vi.fn();
    const onClaimCandidateActivate = vi.fn();

    render(
      <BottomActionDock
        hand={localHand}
        claimCandidates={[
          {
            key: 'pung:w5#1|w5#2',
            actionId: 'pung',
            actionLabel: '碰',
            tileIds: ['w5#1', 'w5#2'],
            tiles: [
              { code: 'w5', source: 'hand' },
              { code: 'w5', source: 'claim' },
              { code: 'w5', source: 'hand' },
            ],
            isSelected: true,
          },
        ]}
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
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={onClaimCandidateSelect}
        onClaimCandidateActivate={onClaimCandidateActivate}
        onAction={vi.fn()}
      />,
    );

    expect(screen.getByRole('button', { name: '和牌' })).toHaveClass('action-dock__action--hu-burn');
    expect(screen.getByRole('button', { name: '碰' })).toHaveClass(
      'action-dock__action--themed',
      'action-dock__action--themed-pung',
    );
    expect(screen.getByRole('button', { name: '过' })).toHaveClass(
      'action-dock__action--themed',
      'action-dock__action--themed-pass',
    );

    const candidateButton = screen.getByRole('button', { name: '碰候选组合 1' });

    expect(screen.getByLabelText('可选吃碰杠组合')).toBeInTheDocument();
    expect(candidateButton).toHaveAttribute('aria-pressed', 'true');
    expect(document.body.querySelector('.action-dock__claim-preview-tile--claim')).not.toBeNull();

    fireEvent.click(candidateButton);

    expect(onClaimCandidateSelect).toHaveBeenCalledWith('pung', ['w5#1', 'w5#2']);
    expect(onClaimCandidateActivate).not.toHaveBeenCalled();
  });

  it('triggers the candidate action immediately on double click', async () => {
    const user = userEvent.setup();
    const onClaimCandidateSelect = vi.fn();
    const onClaimCandidateActivate = vi.fn();

    render(
      <BottomActionDock
        hand={localHand}
        claimCandidates={[
          {
            key: 'chow:t7#0|t8#0',
            actionId: 'chow',
            actionLabel: '吃',
            tileIds: ['t7#0', 't8#0'],
            tiles: [
              { code: 't7', source: 'hand' },
              { code: 't8', source: 'hand' },
              { code: 't9', source: 'claim' },
            ],
            isSelected: false,
          },
        ]}
        actions={[
          { id: 'hu', label: '和牌', enabled: true, emphasis: 'high' },
          { id: 'chow', label: '吃', enabled: true, emphasis: 'medium' },
          { id: 'pass', label: '过', enabled: true, emphasis: 'low' },
        ]}
        isElevated
        promptCue={{
          kind: 'claim',
          tone: 'critical',
          title: '左家刚打出可响应牌',
          detail: '你可以 和牌 / 吃 / 过',
          actionIds: ['hu', 'chow', 'pass'],
          highlightedActionIds: ['hu', 'chow'],
          sourceSeat: 'left',
          isUrgent: true,
        }}
        deadlineAt="2099-03-30T12:10:40+08:00"
        onTileSelect={vi.fn()}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={onClaimCandidateSelect}
        onClaimCandidateActivate={onClaimCandidateActivate}
        onAction={vi.fn()}
      />,
    );

    expect(screen.getByRole('button', { name: '吃' })).toHaveClass(
      'action-dock__action--themed',
      'action-dock__action--themed-chow',
    );
    expect(screen.getByRole('button', { name: '过' })).toHaveClass(
      'action-dock__action--themed',
      'action-dock__action--themed-pass',
    );

    await user.dblClick(screen.getByRole('button', { name: '吃候选组合 1' }));

    expect(onClaimCandidateSelect).toHaveBeenCalled();
    expect(onClaimCandidateActivate).toHaveBeenCalledWith('chow', ['t7#0', 't8#0']);
  });

  it('forwards hand tile double clicks for quick discard interactions', () => {
    const onTileDoubleClick = vi.fn();

    render(
      <BottomActionDock
        hand={localHand}
        claimCandidates={[]}
        actions={[]}
        isElevated={false}
        promptCue={null}
        deadlineAt={null}
        onTileSelect={vi.fn()}
        onTileDoubleClick={onTileDoubleClick}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    fireEvent.doubleClick(getLocalHandButton(0));

    expect(onTileDoubleClick).toHaveBeenCalledWith('w1#1');
  });

  it('renders same-turn restricted tiles as disabled and ignores interaction', () => {
    const onTileSelect = vi.fn();
    const onTileDoubleClick = vi.fn();

    render(
      <BottomActionDock
        hand={[
          { tileId: 'w1#1', code: 'w1', isSelected: false, isDrawn: false, isFlower: false, isDisabled: true },
          { tileId: 'w2#2', code: 'w2', isSelected: false, isDrawn: false, isFlower: false },
        ]}
        claimCandidates={[]}
        actions={[]}
        isElevated={false}
        promptCue={null}
        deadlineAt={null}
        onTileSelect={onTileSelect}
        onTileDoubleClick={onTileDoubleClick}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    const disabledButton = getLocalHandButton(0);

    expect(disabledButton).toBeDisabled();
    expect(disabledButton).toHaveClass('action-dock__tile--disabled');
    expect(disabledButton.querySelector('.mahjong-tile--disabled')).not.toBeNull();

    fireEvent.click(disabledButton);
    fireEvent.doubleClick(disabledButton);

    expect(onTileSelect).not.toHaveBeenCalled();
    expect(onTileDoubleClick).not.toHaveBeenCalled();
  });
});

function getLocalHandButton(index: number) {
  const hand = screen.getByLabelText(/local hand/i);
  const buttons = hand.querySelectorAll('button');
  return buttons[index] as HTMLButtonElement;
}
