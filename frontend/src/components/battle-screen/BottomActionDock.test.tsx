import { fireEvent, render, screen, waitFor } from '@testing-library/react';
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

  it('renders waits and winning fan list for a selected-discard tenpai preview', async () => {
    const user = userEvent.setup();

    render(
      <BottomActionDock
        hand={localHand}
        selectedTileCode="w2"
        handInsight={{
          source: 'selected_discard',
          discardTileId: 'w2#2',
          discardTileCode: 'w2',
          isTenpai: true,
          waits: [{ code: 'w3', availableCount: 2 }],
          winningFans: [
            { fanKey: 'all_pungs', fanValue: 6 },
            { fanKey: 'full_flush', fanValue: 24 },
            { fanKey: 'mixed_straight', fanValue: 8 },
            { fanKey: 'four_kongs', fanValue: 88 },
            { fanKey: 'outside_hand', fanValue: 4 },
            { fanKey: 'all_chows', fanValue: 2 },
            { fanKey: 'self_drawn', fanValue: 1 },
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

    await user.click(screen.getByRole('button', { name: '查看打出当前选中牌后的手牌洞察' }));

    expect(screen.getByText('打出后将听')).toBeInTheDocument();
    expect(screen.getByText('和牌番型')).toBeInTheDocument();
    expect(screen.queryByText(/%/)).toBeNull();

    const rows = screen
      .getByRole('list', { name: '和牌番型列表' })
      .querySelectorAll('.action-dock__hand-insight-winning-fan');

    expect(Array.from(rows, (row) => row.textContent)).toEqual([
      '四杠88番',
      '清一色24番',
      '花龙8番',
      '对对和6番',
      '全带幺4番',
      '平和2番',
      '自摸1番',
    ]);
  });

  it('uses winning fan copy for the current tenpai insight', async () => {
    const user = userEvent.setup();

    render(
      <BottomActionDock
        hand={localHand}
        selectedTileCode="w2"
        handInsight={{
          source: 'current',
          discardTileId: null,
          discardTileCode: null,
          isTenpai: true,
          waits: [{ code: 'w3', availableCount: 2 }],
          winningFans: [{ fanKey: 'full_flush', fanValue: 24 }],
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

    await user.click(screen.getByRole('button', { name: '查看当前听牌信息与和牌番型' }));

    expect(screen.getByText('正在听')).toBeInTheDocument();
    expect(screen.getByText('和牌番型')).toBeInTheDocument();
    expect(screen.getByText('24番')).toBeInTheDocument();
    expect(screen.queryByText(/%/)).toBeNull();
  });

  it('switches the hand insight popover to horizontal layout when its natural height exceeds three quarters of the viewport', async () => {
    const user = userEvent.setup();
    const originalInnerHeight = window.innerHeight;
    const originalScrollHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'scrollHeight');

    Object.defineProperty(window, 'innerHeight', { configurable: true, value: 400 });
    Object.defineProperty(HTMLElement.prototype, 'scrollHeight', {
      configurable: true,
      get() {
        return this.classList.contains('action-dock__ready-hand-popover') ? 360 : 0;
      },
    });

    try {
      render(
        <BottomActionDock
          hand={localHand}
          selectedTileCode="w2"
          handInsight={{
            source: 'current',
            discardTileId: null,
            discardTileCode: null,
            isTenpai: true,
            waits: [
              { code: 'w1', availableCount: 1 },
              { code: 'w2', availableCount: 2 },
              { code: 'w3', availableCount: 3 },
            ],
            winningFans: [{ fanKey: 'full_flush', fanValue: 24 }],
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

      await user.click(screen.getByRole('button', { name: '查看当前听牌信息与和牌番型' }));

      await waitFor(() => {
        expect(screen.getByLabelText('当前手牌洞察')).toHaveClass('action-dock__ready-hand-popover--horizontal');
      });
    } finally {
      Object.defineProperty(window, 'innerHeight', { configurable: true, value: originalInnerHeight });
      if (originalScrollHeight) {
        Object.defineProperty(HTMLElement.prototype, 'scrollHeight', originalScrollHeight);
      } else {
        delete (HTMLElement.prototype as { scrollHeight?: number }).scrollHeight;
      }
    }
  });

  it('keeps the hand insight trigger visible when the hu button is available', async () => {
    const user = userEvent.setup();

    render(
      <BottomActionDock
        hand={localHand}
        handInsight={{
          source: 'current',
          discardTileId: null,
          discardTileCode: null,
          isTenpai: false,
          waits: [],
          winningFans: [{ fanKey: 'all_chows', fanValue: 2 }],
        }}
        claimCandidates={[]}
        actions={[{ id: 'hu', label: '和牌', enabled: true, emphasis: 'high' }]}
        isElevated
        promptCue={{
          kind: 'turn',
          tone: 'critical',
          title: '当前手牌可直接和牌',
          detail: '你可以 和牌',
          actionIds: ['hu'],
          highlightedActionIds: ['hu'],
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

    expect(screen.getByRole('button', { name: '和牌' })).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: '查看当前和牌番型' }));

    expect(screen.getByText('和牌番型')).toBeInTheDocument();
    expect(screen.getByText('平和')).toBeInTheDocument();
  });

  it('anchors pinned winning fan detail popover next to the fan label on desktop screens', async () => {
    const user = userEvent.setup();
    const originalInnerWidth = window.innerWidth;
    const originalGetBoundingClientRect = HTMLElement.prototype.getBoundingClientRect;
    const makeRect = (left: number, top: number, width: number, height: number) =>
      ({
        x: left,
        y: top,
        left,
        top,
        width,
        height,
        right: left + width,
        bottom: top + height,
        toJSON: () => ({}),
      }) as DOMRect;

    Object.defineProperty(HTMLElement.prototype, 'getBoundingClientRect', {
      configurable: true,
      value(this: HTMLElement) {
        if (this.classList.contains('action-dock__fan-detail-popover')) {
          return makeRect(0, 0, 224, 120);
        }

        if (this.classList.contains('action-dock__hand-insight-winning-fan')) {
          return makeRect(400, 200, 260, 28);
        }

        if (this.tagName === 'SPAN' && this.textContent === '平和') {
          return makeRect(420, 204, 40, 18);
        }

        return originalGetBoundingClientRect.call(this);
      },
    });
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 1024 });

    try {
      render(
        <BottomActionDock
          hand={localHand}
          handInsight={{
            source: 'current',
            discardTileId: null,
            discardTileCode: null,
            isTenpai: false,
            waits: [],
            winningFans: [{ fanKey: 'all_chows', fanValue: 2 }],
          }}
          claimCandidates={[]}
          actions={[{ id: 'hu', label: '和牌', enabled: true, emphasis: 'high' }]}
          isElevated
          promptCue={null}
          deadlineAt={null}
          onTileSelect={vi.fn()}
          onTileDoubleClick={vi.fn()}
          onClaimCandidateSelect={vi.fn()}
          onClaimCandidateActivate={vi.fn()}
          onAction={vi.fn()}
        />,
      );

      await user.click(screen.getByRole('button', { name: '查看当前和牌番型' }));

      const fanRow = screen.getByText('平和').closest('.action-dock__hand-insight-winning-fan');
      expect(fanRow).not.toBeNull();

      fireEvent.mouseEnter(fanRow!);

      await waitFor(() => {
        const popover = document.body.querySelector('.action-dock__fan-detail-popover') as HTMLElement | null;

        expect(popover).not.toBeNull();
        expect(popover).toHaveStyle({ left: '474px' });
      });
    } finally {
      Object.defineProperty(window, 'innerWidth', { configurable: true, value: originalInnerWidth });
      Object.defineProperty(HTMLElement.prototype, 'getBoundingClientRect', {
        configurable: true,
        value: originalGetBoundingClientRect,
      });
    }
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

  it('keeps dock layout sizing on a stable active hand capacity', () => {
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
      '--action-dock-layout-hand-count': '14',
      '--action-dock-effective-layout-hand-count': '14',
      '--action-dock-layout-gap-count': '13',
      '--action-dock-layout-drawn-gap-count': '1',
    });
  });

  it('does not change dock layout sizing variables when the hand count changes', () => {
    const hand13 = Array.from({ length: 13 }, (_, index) => ({
      tileId: `w${(index % 9) + 1}#${index}`,
      code: `w${(index % 9) + 1}`,
      isSelected: false,
      isDrawn: false,
      isFlower: false,
    }));
    const hand14 = [
      ...hand13,
      { tileId: 'b1#13', code: 'b1', isSelected: false, isDrawn: true, isFlower: false },
    ];

    const { rerender } = render(
      <BottomActionDock
        hand={hand13}
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
    const stableLayoutStyle = {
      '--action-dock-layout-hand-count': '14',
      '--action-dock-effective-layout-hand-count': '14',
      '--action-dock-layout-gap-count': '13',
      '--action-dock-layout-drawn-gap-count': '1',
    };

    expect(dock).toHaveStyle(stableLayoutStyle);

    rerender(
      <BottomActionDock
        hand={hand14}
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

    expect(dock).toHaveStyle({
      '--action-dock-hand-count': '14',
      '--action-dock-drawn-gap-count': '1',
      ...stableLayoutStyle,
    });
  });

  it('keeps the hand insight rail fixed to the rendered hand edge', () => {
    render(
      <BottomActionDock
        hand={localHand}
        handInsight={{
          isTenpai: false,
          source: 'current',
          discardTileId: null,
          discardTileCode: null,
          waits: [],
          winningFans: [{ fanKey: 'all_chows', fanValue: 2 }],
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

    const hand = screen.getByLabelText(/local hand/i);
    const handZone = hand.closest('.action-dock__hand-zone') as HTMLElement | null;
    const handCluster = hand.closest('.action-dock__hand-cluster') as HTMLElement | null;
    const infoRail = handCluster?.querySelector(':scope > .action-dock__info-rail') as HTMLElement | null;

    expect(handZone).not.toBeNull();
    expect(handZone).toContainElement(handCluster);
    expect(handCluster).not.toBeNull();
    expect(handCluster).toContainElement(hand);
    expect(infoRail).not.toBeNull();
    expect(infoRail).toContainElement(screen.getByRole('button', { name: '查看当前和牌番型' }));
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
      '--action-dock-effective-layout-hand-count': '13',
      '--action-dock-layout-gap-count': '12',
      '--action-dock-layout-drawn-gap-count': '0',
    });
    expect(screen.queryByText('牌桌进入对局后，手牌和操作按钮会显示在这里。')).toBeNull();
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

  it('renders spectator perspective switch and disables tile interaction', async () => {
    const user = userEvent.setup();
    const onSwitchPerspective = vi.fn();
    const onTileSelect = vi.fn();

    render(
      <BottomActionDock
        hand={localHand}
        claimCandidates={[]}
        actions={[]}
        isElevated={false}
        isSpectator
        spectatorFocusName="Player B"
        promptCue={null}
        deadlineAt={null}
        onSwitchPerspective={onSwitchPerspective}
        onTileSelect={onTileSelect}
        onTileDoubleClick={vi.fn()}
        onClaimCandidateSelect={vi.fn()}
        onClaimCandidateActivate={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    await user.click(screen.getByRole('button', { name: '切换观战视角，当前 Player B' }));
    expect(onSwitchPerspective).toHaveBeenCalledTimes(1);

    await user.click(getLocalHandButton(0));
    expect(onTileSelect).not.toHaveBeenCalled();
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
