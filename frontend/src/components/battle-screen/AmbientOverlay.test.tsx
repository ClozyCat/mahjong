import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { AmbientOverlay } from './AmbientOverlay';

describe('AmbientOverlay', () => {
  it('shows the reconnect veil copy without rendering any log controls', () => {
    render(
      <AmbientOverlay
        mode="disconnected_or_waiting"
        promptText="正在等待服务器同步下一帧状态。"
        waitingControls={null}
      />,
    );

    expect(screen.getByText('正在重连')).toBeInTheDocument();
    expect(screen.getByText('正在等待服务器同步下一帧状态。')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '展开日志窗口' })).toBeNull();
    expect(screen.queryByLabelText('日志窗口')).toBeNull();
  });

  it('shows a leave-table button on the waiting-player overlay when leaving is allowed', () => {
    render(
      <AmbientOverlay
        mode="disconnected_or_waiting"
        promptText={null}
        waitingControls={{
          canReady: true,
          canStart: false,
          isReady: false,
          occupiedSeats: 2,
        }}
        canLeaveTable
        onLeaveTable={() => undefined}
      />,
    );

    expect(screen.getByText('等待牌手')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '离开牌桌' })).toBeInTheDocument();
  });
});
