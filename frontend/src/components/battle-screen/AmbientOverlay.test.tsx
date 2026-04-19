import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

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

  it('does not render the waiting-player veil even when waiting controls exist', () => {
    const onAddBot = vi.fn();
    const onRemoveBot = vi.fn();
    render(
      <AmbientOverlay
        mode="disconnected_or_waiting"
        promptText={null}
        waitingControls={{
          canReady: true,
          canStart: false,
          isReady: false,
          occupiedSeats: 2,
          botCount: 1,
          canAddBot: true,
          canRemoveBot: true,
        }}
        canLeaveTable
        onAddBot={onAddBot}
        onRemoveBot={onRemoveBot}
        onLeaveTable={() => undefined}
      />,
    );

    expect(screen.queryByText('等待牌手')).toBeNull();
    expect(screen.queryByRole('button', { name: '离开牌桌' })).toBeNull();
    expect(screen.queryByRole('group', { name: '蒙版 BOT 数量控制' })).toBeNull();
    expect(onAddBot).not.toHaveBeenCalled();
    expect(onRemoveBot).not.toHaveBeenCalled();
  });
});
