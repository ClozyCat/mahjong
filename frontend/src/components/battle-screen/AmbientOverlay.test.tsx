import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
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

  it('shows bot controls and a leave-table button on the waiting-player overlay when leaving is allowed', async () => {
    const user = userEvent.setup();
    const onAddBot = vi.fn();
    const onRemoveBot = vi.fn();
    const { container } = render(
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

    const waitingActions = container.querySelector('.ambient-overlay__waiting-actions');
    const leaveButton = screen.getByRole('button', { name: '离开牌桌' });
    const botControls = screen.getByRole('group', { name: '蒙版 BOT 数量控制' });

    expect(screen.getByText('等待牌手')).toBeInTheDocument();
    expect(waitingActions).not.toBeNull();
    expect(waitingActions?.firstElementChild).toBe(leaveButton);
    expect(waitingActions?.lastElementChild).toBe(botControls);

    await user.click(screen.getByRole('button', { name: '蒙版增加 BOT' }));
    await user.click(screen.getByRole('button', { name: '蒙版减少 BOT' }));

    expect(onAddBot).toHaveBeenCalledTimes(1);
    expect(onRemoveBot).toHaveBeenCalledTimes(1);
  });

  it('hides the waiting-player veil after all four seats are occupied', () => {
    render(
      <AmbientOverlay
        mode="disconnected_or_waiting"
        promptText={null}
        waitingControls={{
          canReady: true,
          canStart: false,
          isReady: false,
          occupiedSeats: 4,
          botCount: 0,
          canAddBot: false,
          canRemoveBot: false,
        }}
      />,
    );

    expect(screen.queryByText('等待牌手')).toBeNull();
  });
});
