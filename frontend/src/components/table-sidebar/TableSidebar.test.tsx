import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { TableSidebar } from './TableSidebar';

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
        roomPanel={<div>room panel</div>}
        messagesPanel={<div>messages panel</div>}
        profilePanel={<div>profile</div>}
        onToggle={onToggle}
        onTabChange={vi.fn()}
        onSelectUser={vi.fn()}
      />,
    );

    await user.click(screen.getByRole('button', { name: '收起牌桌侧边栏' }));

    expect(onToggle).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('tab', { name: '牌局' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('tab', { name: '消息' })).toBeInTheDocument();
    expect(screen.getByText('room panel')).toBeInTheDocument();
  });

  it('shows alert badges on the messages tab with pending work', () => {
    render(
      <TableSidebar
        isOpen
        activeTab="messages"
        tablePlayers={[]}
        onlineUsers={[]}
        spectators={[]}
        roomPanel={<div>room panel</div>}
        messagesPanel={<div>messages panel</div>}
        profilePanel={<div>profile</div>}
        tabAlerts={{ messages: true }}
        onToggle={vi.fn()}
        onTabChange={vi.fn()}
        onSelectUser={vi.fn()}
      />,
    );

    expect(screen.getByRole('tab', { name: '牌局' }).querySelector('.table-sidebar__tab-alert')).toBeNull();
    expect(screen.getByRole('tab', { name: '消息' }).querySelector('.table-sidebar__tab-alert')).toHaveTextContent('!');
    expect(screen.getByText('messages panel')).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: '观战申请' })).not.toBeInTheDocument();
  });

  it('falls back to a visible tab when the requested room tab is unavailable', () => {
    render(
      <TableSidebar
        isOpen
        activeTab="room"
        tablePlayers={[]}
        onlineUsers={[]}
        spectators={[]}
        profilePanel={<div>profile</div>}
        onToggle={vi.fn()}
        onTabChange={vi.fn()}
        onSelectUser={vi.fn()}
      />,
    );

    expect(screen.queryByRole('tab', { name: '牌局' })).not.toBeInTheDocument();
    expect(screen.getByRole('tab', { name: '本局玩家' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByText('尚未开局')).toBeInTheDocument();
  });

  it('shows all users by points and updates player status labels from presence', () => {
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
        active_table_code: null,
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
        active_table_code: null,
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
        active_table_code: null,
      },
    ];
    const baseProps = {
      isOpen: true,
      activeTab: 'online' as const,
      tablePlayers: [],
      onlineUsers: users,
      spectators: [],
      profilePanel: <div>profile</div>,
      onToggle: vi.fn(),
      onTabChange: vi.fn(),
      onSelectUser: vi.fn(),
    };

    const { rerender } = render(<TableSidebar {...baseProps} onlineUserIds={[2]} />);

    expect(screen.getByRole('tab', { name: '所有玩家' })).toHaveAttribute('aria-selected', 'true');
    const rows = screen.getAllByRole('listitem');
    expect(within(rows[0]).getByText(/阿成（雀士）/)).toBeInTheDocument();
    expect(within(rows[1]).getByText(/阿强（平民）/)).toBeInTheDocument();
    expect(within(rows[2]).getByText(/阿丹（平民）/)).toBeInTheDocument();
    expect(within(rows[1]).getByText('在线')).toBeInTheDocument();
    expect(within(rows[0]).getByText('离线')).toBeInTheDocument();

    rerender(<TableSidebar {...baseProps} onlineUserIds={[3]} />);

    expect(within(screen.getAllByRole('listitem')[0]).getByText('在线')).toBeInTheDocument();
    expect(within(screen.getAllByRole('listitem')[1]).getByText('离线')).toBeInTheDocument();
  });

  it('splits online in-table status between player and bot games', () => {
    const users = [
      {
        user_id: 1,
        username: 'alice',
        display_name: '阿明',
        points: 300,
        title: '平民',
        display_label: '阿明（平民）',
        bio: '',
        avatar: null,
        active_table_code: 'HUMAN01',
        active_table_phase: 'playing' as const,
      },
      {
        user_id: 2,
        username: 'bob',
        display_name: '阿强',
        points: 120,
        title: '平民',
        display_label: '阿强（平民）',
        bio: '',
        avatar: null,
        active_table_code: 'HUMAN01',
        active_table_phase: 'playing' as const,
      },
      {
        user_id: 3,
        username: 'chen',
        display_name: '阿成',
        points: 80,
        title: '平民',
        display_label: '阿成（平民）',
        bio: '',
        avatar: null,
        active_table_code: 'BOT01',
        active_table_phase: 'playing' as const,
      },
    ];

    render(
      <TableSidebar
        isOpen
        activeTab="online"
        tablePlayers={[]}
        onlineUsers={users}
        onlineUserIds={[1, 2, 3]}
        spectators={[]}
        profilePanel={<div>profile</div>}
        onToggle={vi.fn()}
        onTabChange={vi.fn()}
        onSelectUser={vi.fn()}
      />,
    );

    const rows = screen.getAllByRole('listitem');

    expect(within(rows[0]).getByText('与玩家对局中')).toBeInTheDocument();
    expect(within(rows[1]).getByText('与玩家对局中')).toBeInTheDocument();
    expect(within(rows[2]).getByText('与BOT对局中')).toBeInTheDocument();
  });

  it('shows creating status for users in a waiting table', () => {
    const users = [
      {
        user_id: 1,
        username: 'alice',
        display_name: '阿明',
        points: 300,
        title: '平民',
        display_label: '阿明（平民）',
        bio: '',
        avatar: null,
        active_table_code: 'WAIT01',
      },
      {
        user_id: 2,
        username: 'bob',
        display_name: '阿强',
        points: 120,
        title: '平民',
        display_label: '阿强（平民）',
        bio: '',
        avatar: null,
        active_table_code: 'BOT01',
        active_table_phase: 'playing' as const,
      },
    ];

    render(
      <TableSidebar
        isOpen
        activeTab="online"
        tablePlayers={[]}
        onlineUsers={users}
        onlineUserIds={[1, 2]}
        creatingTableCodes={['WAIT01']}
        spectators={[]}
        profilePanel={<div>profile</div>}
        onToggle={vi.fn()}
        onTabChange={vi.fn()}
        onSelectUser={vi.fn()}
      />,
    );

    const rows = screen.getAllByRole('listitem');

    expect(within(rows[0]).getByText('创建牌局中')).toBeInTheDocument();
    expect(within(rows[1]).getByText('与BOT对局中')).toBeInTheDocument();
  });

  it('disables watch for users in waiting tables', async () => {
    const user = userEvent.setup();
    const onWatchUser = vi.fn();
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
        active_table_code: 'WAIT01',
        active_table_phase: 'waiting' as const,
      },
    ];

    render(
      <TableSidebar
        isOpen
        activeTab="online"
        tablePlayers={[]}
        onlineUsers={users}
        currentUserId={1}
        spectators={[]}
        profilePanel={<div>profile</div>}
        onToggle={vi.fn()}
        onTabChange={vi.fn()}
        onSelectUser={vi.fn()}
        onWatchUser={onWatchUser}
      />,
    );

    const row = screen.getByText(/阿强（平民）/).closest('li');
    expect(row).not.toBeNull();
    expect(within(row!).getByText('创建牌局中')).toBeInTheDocument();

    const watchButton = within(row!).getByRole('button', { name: '观战' });
    expect(watchButton).toBeDisabled();

    await user.click(watchButton);

    expect(onWatchUser).not.toHaveBeenCalled();
  });

  it('enables watch only for other users with active tables', async () => {
    const user = userEvent.setup();
    const onWatchUser = vi.fn();
    const users = [
      {
        user_id: 1,
        username: 'alice',
        display_name: '阿明',
        points: 300,
        title: '平民',
        display_label: '阿明（平民）',
        bio: '',
        avatar: null,
        active_table_code: 'SELF01',
        active_table_phase: 'playing' as const,
      },
      {
        user_id: 2,
        username: 'bob',
        display_name: '阿强',
        points: 120,
        title: '平民',
        display_label: '阿强（平民）',
        bio: '',
        avatar: null,
        active_table_code: 'ROOM42',
        active_table_phase: 'playing' as const,
      },
      {
        user_id: 3,
        username: 'chen',
        display_name: '阿成',
        points: 80,
        title: '平民',
        display_label: '阿成（平民）',
        bio: '',
        avatar: null,
        active_table_code: null,
      },
    ];

    render(
      <TableSidebar
        isOpen
        activeTab="online"
        tablePlayers={[]}
        onlineUsers={users}
        currentUserId={1}
        spectators={[]}
        profilePanel={<div>profile</div>}
        onToggle={vi.fn()}
        onTabChange={vi.fn()}
        onSelectUser={vi.fn()}
        onWatchUser={onWatchUser}
      />,
    );

    const rows = screen.getAllByRole('listitem');
    const selfWatchButton = within(rows[0]).getByRole('button', { name: '观战' });
    const activeWatchButton = within(rows[1]).getByRole('button', { name: '观战' });
    const inactiveWatchButton = within(rows[2]).getByRole('button', { name: '观战' });

    expect(selfWatchButton).toBeDisabled();
    expect(activeWatchButton).toBeEnabled();
    expect(inactiveWatchButton).toBeDisabled();

    await user.click(activeWatchButton);

    expect(onWatchUser).toHaveBeenCalledWith(users[1]);
  });

  it('disables watch when active table phase is unknown', async () => {
    const user = userEvent.setup();
    const onWatchUser = vi.fn();
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
        active_table_code: 'ROOM42',
      },
    ];

    render(
      <TableSidebar
        isOpen
        activeTab="online"
        tablePlayers={[]}
        onlineUsers={users}
        currentUserId={1}
        spectators={[]}
        profilePanel={<div>profile</div>}
        onToggle={vi.fn()}
        onTabChange={vi.fn()}
        onSelectUser={vi.fn()}
        onWatchUser={onWatchUser}
      />,
    );

    const watchButton = screen.getByRole('button', { name: '观战' });
    expect(watchButton).toBeDisabled();

    await user.click(watchButton);

    expect(onWatchUser).not.toHaveBeenCalled();
  });

  it('shows already requested watch buttons as disabled', async () => {
    const user = userEvent.setup();
    const onWatchUser = vi.fn();
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
        active_table_code: 'ROOM42',
      },
    ];

    render(
      <TableSidebar
        isOpen
        activeTab="online"
        tablePlayers={[]}
        onlineUsers={users}
        requestedWatchTableCodes={['ROOM42']}
        spectators={[]}
        profilePanel={<div>profile</div>}
        onToggle={vi.fn()}
        onTabChange={vi.fn()}
        onSelectUser={vi.fn()}
        onWatchUser={onWatchUser}
      />,
    );

    const requestedButton = screen.getByRole('button', { name: '已申请' });

    expect(requestedButton).toBeDisabled();

    await user.click(requestedButton);

    expect(onWatchUser).not.toHaveBeenCalled();
  });
});
