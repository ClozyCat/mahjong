import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { FloatingRoomControls } from './FloatingRoomControls';

describe('FloatingRoomControls', () => {
  it('does not render anything after the table panel was removed', () => {
    render(
      <FloatingRoomControls
        players={[
          {
            seat: 'bottom',
            name: 'Player A',
            score: 25000,
            points: 0,
            liveDelta: 0,
            flowerCount: 1,
            wind: 'East',
            isDealer: true,
            isActive: true,
            isLocal: true,
            connected: true,
            isReadyHand: false,
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
            points: 0,
            liveDelta: 0,
            flowerCount: 0,
            wind: 'South',
            isDealer: false,
            isActive: false,
            isLocal: false,
            connected: true,
            isReadyHand: false,
            concealedCount: 13,
            meldCount: 0,
            melds: [],
            flowers: [],
            statusText: 'Ready',
          },
        ]}
        actions={[
          { id: 'invite', label: '邀请', enabled: true, emphasis: 'medium' },
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

    expect(screen.queryByLabelText('牌桌侧边面板')).toBeNull();
    expect(screen.queryByRole('button', { name: '邀请' })).toBeNull();
  });
});
