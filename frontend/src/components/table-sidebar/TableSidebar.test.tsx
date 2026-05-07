import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { TableSidebar } from './TableSidebar';

function renderSidebar(isOwner: boolean) {
  return render(
    <TableSidebar
      isOpen
      activeTab="requests"
      tablePlayers={[]}
      onlineUsers={[]}
      spectators={[]}
      spectatorRequests={[
        {
          id: 3,
          table_code: 'AB12CD',
          requester_user_id: 7,
          owner_user_id: 1,
          status: 'pending',
          created_at: '2026-05-06T12:00:00Z',
          decided_at: null,
        },
      ]}
      isOwner={isOwner}
      profilePanel={<div>profile</div>}
      onToggle={vi.fn()}
      onTabChange={vi.fn()}
      onSelectUser={vi.fn()}
      onApproveRequest={vi.fn()}
      onRejectRequest={vi.fn()}
    />,
  );
}

describe('TableSidebar', () => {
  it('keeps the collapse toggle available while the sidebar panel is open', async () => {
    const user = userEvent.setup();
    const onToggle = vi.fn();

    render(
      <TableSidebar
        isOpen
        activeTab="room"
        tablePlayers={[]}
        onlineUsers={[]}
        spectators={[]}
        spectatorRequests={[]}
        isOwner
        roomPanel={<div>room panel</div>}
        profilePanel={<div>profile</div>}
        onToggle={onToggle}
        onTabChange={vi.fn()}
        onSelectUser={vi.fn()}
        onApproveRequest={vi.fn()}
        onRejectRequest={vi.fn()}
      />,
    );

    await user.click(screen.getByRole('button', { name: '收起牌桌侧边栏' }));

    expect(onToggle).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('tab', { name: '牌局' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByText('room panel')).toBeInTheDocument();
  });

  it('shows alert badges on tabs with pending work', () => {
    render(
      <TableSidebar
        isOpen
        activeTab="room"
        tablePlayers={[]}
        onlineUsers={[]}
        spectators={[]}
        spectatorRequests={[]}
        isOwner
        roomPanel={<div>room panel</div>}
        profilePanel={<div>profile</div>}
        tabAlerts={{ room: true, requests: true }}
        onToggle={vi.fn()}
        onTabChange={vi.fn()}
        onSelectUser={vi.fn()}
        onApproveRequest={vi.fn()}
        onRejectRequest={vi.fn()}
      />,
    );

    expect(screen.getByRole('tab', { name: '牌局' }).querySelector('.table-sidebar__tab-alert')).toHaveTextContent('!');
    expect(screen.getByRole('tab', { name: '观战申请' }).querySelector('.table-sidebar__tab-alert')).toHaveTextContent('!');
  });

  it('shows all users by points and updates online labels from presence', () => {
    const users = [
      {
        user_id: 2,
        username: 'bob',
        display_name: '阿强',
        points: 120,
        title: '平民',
        display_label: '阿强（平民）',
        bio: '',
        avatar: null,
      },
      {
        user_id: 3,
        username: 'chen',
        display_name: '阿成',
        points: 260,
        title: '雀士',
        display_label: '阿成（雀士）',
        bio: '',
        avatar: null,
      },
      {
        user_id: 4,
        username: 'dan',
        display_name: '阿丹',
        points: 80,
        title: '平民',
        display_label: '阿丹（平民）',
        bio: '',
        avatar: null,
      },
    ];
    const baseProps = {
      isOpen: true,
      activeTab: 'online' as const,
      tablePlayers: [],
      onlineUsers: users,
      spectators: [],
      spectatorRequests: [],
      isOwner: false,
      profilePanel: <div>profile</div>,
      onToggle: vi.fn(),
      onTabChange: vi.fn(),
      onSelectUser: vi.fn(),
      onApproveRequest: vi.fn(),
      onRejectRequest: vi.fn(),
    };

    const { rerender } = render(<TableSidebar {...baseProps} onlineUserIds={[2]} />);

    expect(screen.getByRole('tab', { name: '所有玩家' })).toHaveAttribute('aria-selected', 'true');
    const rows = screen.getAllByRole('listitem');
    expect(within(rows[0]).getByText(/阿成（雀士）/)).toBeInTheDocument();
    expect(within(rows[1]).getByText(/阿强（平民）/)).toBeInTheDocument();
    expect(within(rows[2]).getByText(/阿丹（平民）/)).toBeInTheDocument();
    expect(within(rows[1]).getByText('online')).toBeInTheDocument();
    expect(within(rows[0]).queryByText('online')).not.toBeInTheDocument();

    rerender(<TableSidebar {...baseProps} onlineUserIds={[3]} />);

    expect(within(screen.getAllByRole('listitem')[0]).getByText('online')).toBeInTheDocument();
    expect(within(screen.getAllByRole('listitem')[1]).queryByText('online')).not.toBeInTheDocument();
  });

  it('does not show approval buttons for non-owner viewers', () => {
    renderSidebar(false);

    expect(screen.queryByRole('button', { name: '同意' })).toBeNull();
    expect(screen.queryByRole('button', { name: '拒绝' })).toBeNull();
    expect(screen.getByText('仅房主可审批观战申请')).toBeInTheDocument();
  });

  it('lets the owner approve and reject spectator requests', async () => {
    const user = userEvent.setup();
    const onApproveRequest = vi.fn();
    const onRejectRequest = vi.fn();

    render(
      <TableSidebar
        isOpen
        activeTab="requests"
        tablePlayers={[]}
        onlineUsers={[]}
        spectators={[]}
        spectatorRequests={[
          {
            id: 3,
            table_code: 'AB12CD',
            requester_user_id: 7,
            owner_user_id: 1,
            status: 'pending',
            created_at: '2026-05-06T12:00:00Z',
            decided_at: null,
          },
        ]}
        isOwner
        profilePanel={<div>profile</div>}
        onToggle={vi.fn()}
        onTabChange={vi.fn()}
        onSelectUser={vi.fn()}
        onApproveRequest={onApproveRequest}
        onRejectRequest={onRejectRequest}
      />,
    );

    await user.click(screen.getByRole('button', { name: '同意' }));
    await user.click(screen.getByRole('button', { name: '拒绝' }));

    expect(onApproveRequest).toHaveBeenCalledWith(3);
    expect(onRejectRequest).toHaveBeenCalledWith(3);
  });
});
