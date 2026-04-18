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
  it('can collapse into a floating restore button and reopen', () => {
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
        claimCandidates={[]}
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
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    const huButton = screen.getByRole('button', { name: '和牌' });
    const pungButton = screen.getByRole('button', { name: '碰' });
    const passButton = screen.getByRole('button', { name: '过' });

    expect(document.body.querySelector('.action-dock--actionable')).toBeNull();
    expect(document.body.querySelector('.action-dock--elevated')).toBeNull();
    expect(screen.queryByText('左家刚打出可响应牌')).toBeNull();
    expect(huButton).toHaveClass('action-dock__action--hu-burn');
    expect(pungButton).not.toHaveClass('action-dock__action--hu-burn');
    expect(passButton).not.toHaveClass('action-dock__action--hu-burn');
    expect(pungButton).toHaveClass('action-dock__action--themed', 'action-dock__action--themed-pung');
    expect(passButton).toHaveClass('action-dock__action--themed', 'action-dock__action--themed-pass');
    expect(huButton).not.toHaveClass('action-dock__action--response-glow');
    expect(pungButton).not.toHaveClass('action-dock__action--response-glow');
    expect(passButton).not.toHaveClass('action-dock__action--response-glow');
    expect(screen.getByRole('button', { name: '收起手牌区' })).toBeInTheDocument();
  });

  it('shows all enabled local-turn actions without turning the dock into a response highlight', () => {
    render(
      <BottomActionDock
        hand={localHand}
        claimCandidates={[]}
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
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    expect(document.body.querySelector('.action-dock--elevated')).not.toBeNull();
    expect(document.body.querySelector('.action-dock--actionable')).toBeNull();
    expect(screen.getByRole('button', { name: '出牌' })).toHaveClass(
      'action-dock__action--themed',
      'action-dock__action--themed-discard',
    );
    expect(screen.getByRole('button', { name: '杠' })).toHaveClass(
      'action-dock__action--themed',
      'action-dock__action--themed-kong',
    );
    expect(screen.getByRole('button', { name: '和牌' })).toHaveClass('action-dock__action--hu-burn');
    expect(screen.getByLabelText(/剩余 \d+ 秒/)).toBeInTheDocument();
  });

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

  it('shows the ready-hand trigger and popover without shifting the dock controls', async () => {
    const user = userEvent.setup();

    render(
      <BottomActionDock
        hand={localHand}
        readyHandInsight={{
          source: 'current',
          discardTileId: null,
          discardTileCode: null,
          waits: [
            { code: 't1', availableCount: 2 },
            { code: 't4', availableCount: 3 },
          ],
        }}
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

    const trigger = screen.getByRole('button', { name: '查看当前听牌信息' });

    expect(screen.getByRole('button', { name: '收起手牌区' })).toBeInTheDocument();

    await user.hover(trigger);

    expect(screen.getByLabelText('当前听牌信息')).toBeInTheDocument();
  });

  it('hides manual actions when opening flowers will auto-pass locally', () => {
    render(
      <BottomActionDock
        hand={localHand}
        claimCandidates={[]}
        actions={[
          { id: 'pass', label: '过', enabled: true, emphasis: 'low' },
        ]}
        isElevated
        promptCue={{
          kind: 'turn',
          tone: 'info',
          title: '补花阶段',
          detail: '当前无花牌，系统将自动过',
          actionIds: [],
          highlightedActionIds: [],
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

    expect(screen.queryByRole('button', { name: '过' })).toBeNull();
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
