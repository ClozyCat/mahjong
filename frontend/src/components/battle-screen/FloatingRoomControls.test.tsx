import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { FloatingRoomControls } from './FloatingRoomControls';

describe('FloatingRoomControls', () => {
  it('renders the integrated right-side drawer and can collapse into a side button', () => {
    render(
      <FloatingRoomControls
        players={[
          {
            seat: 'bottom',
            name: 'Player A',
            score: 25000,
            liveDelta: 0,
            flowerCount: 1,
            wind: 'East',
            isDealer: true,
            isActive: true,
            isLocal: true,
            connected: true,
            ready: true,
            concealedCount: 14,
            meldCount: 0,
            melds: [],
            flowers: [],
            statusText: 'Live',
          },
          {
            seat: 'left',
            name: 'Player Left',
            score: 24800,
            liveDelta: 0,
            flowerCount: 0,
            wind: 'South',
            isDealer: false,
            isActive: false,
            isLocal: false,
            connected: true,
            ready: true,
            concealedCount: 13,
            meldCount: 0,
            melds: [],
            flowers: [],
            statusText: 'Ready',
          },
        ]}
        actions={[
          { id: 'ready', label: '准备', enabled: true, emphasis: 'medium' },
          { id: 'start_match', label: '开始对局', enabled: true, emphasis: 'high' },
          { id: 'start_next_round', label: '下一局', enabled: true, emphasis: 'high' },
          { id: 'restart_match', label: '再来一局', enabled: false, emphasis: 'medium' },
        ]}
        tableCode="AB12CD"
        canLeaveTable
        phaseLabel="playing"
        roundLabel="东一局"
        scoreSummaryLabel="总分 24"
        deadlineAt={null}
        topStatusLabel="等待出牌"
        promptText="牌桌信息、玩家状态和房间操作已整合到右侧抽屉。"
        remainingTileCount={72}
        waitingControls={null}
        onCopyTableCode={vi.fn()}
        onLeaveTable={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    expect(screen.getByLabelText('牌桌侧边面板')).toBeInTheDocument();
    expect(screen.getByText('Player A')).toBeInTheDocument();
    expect(screen.getByText('AB12CD')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '准备' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '开始对局' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '下一局' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '再来一局' })).toBeDisabled();
    expect(screen.getByRole('button', { name: '离开牌桌' })).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '缩进牌桌侧边面板' }));

    expect(screen.queryByLabelText('牌桌侧边面板')).toBeNull();
    expect(screen.getByRole('button', { name: '展开牌桌侧边面板' })).toBeInTheDocument();
  });
});
