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
        roomPanel={<div>room panel</div>}
        messagesPanel={<div>messages panel</div>}
        onToggle={onToggle}
        onTabChange={vi.fn()}
      />,
    );

    await user.click(screen.getByRole('button', { name: '收起牌桌侧边栏' }));

    expect(onToggle).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('tab', { name: '牌局' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getAllByRole('tab').map((tab) => tab.getAttribute('aria-label'))).toEqual([
      '牌局',
      '消息',
      '所有玩家',
    ]);
    expect(screen.getByText('room panel')).toBeInTheDocument();
  });

  it('shows alert badges on the messages tab with pending work', () => {
    render(
      <TableSidebar
        isOpen
        activeTab="messages"
        tablePlayers={[]}
        onlineUsers={[]}
        roomPanel={<div>room panel</div>}
        messagesPanel={<div>messages panel</div>}
        tabAlerts={{ messages: true }}
        onToggle={vi.fn()}
        onTabChange={vi.fn()}
      />,
    );

    expect(screen.getByRole('tab', { name: '牌局' }).querySelector('.table-sidebar__tab-alert')).toBeNull();
    expect(screen.getByRole('tab', { name: '消息' }).querySelector('.table-sidebar__tab-alert')).toHaveTextContent('!');
    expect(screen.getByText('messages panel')).toBeInTheDocument();
  });

  it('falls back to the all players tab when the requested room tab is unavailable', () => {
    render(
      <TableSidebar
        isOpen
        activeTab="room"
        tablePlayers={[]}
        onlineUsers={[]}
        onToggle={vi.fn()}
        onTabChange={vi.fn()}
      />,
    );

    expect(screen.queryByRole('tab', { name: '牌局' })).not.toBeInTheDocument();
    expect(screen.getByRole('tab', { name: '所有玩家' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByText('暂无玩家')).toBeInTheDocument();
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
      onToggle: vi.fn(),
      onTabChange: vi.fn(),
    };

    const { rerender } = render(<TableSidebar {...baseProps} onlineUserIds={[2]} />);

    expect(screen.getByRole('tab', { name: '所有玩家' })).toHaveAttribute('aria-selected', 'true');
    const rows = screen.getAllByRole('listitem');
    expect(within(rows[0]!).getByText(/阿成（雀士）/)).toBeInTheDocument();
    expect(within(rows[1]!).getByText(/阿强（平民）/)).toBeInTheDocument();
    expect(within(rows[2]!).getByText(/阿丹（平民）/)).toBeInTheDocument();
    expect(within(rows[1]!).getByText('在线')).toBeInTheDocument();
    expect(within(rows[0]!).getByText('离线')).toBeInTheDocument();

    rerender(<TableSidebar {...baseProps} onlineUserIds={[3]} />);

    expect(within(screen.getAllByRole('listitem')[0]!).getByText('在线')).toBeInTheDocument();
    expect(within(screen.getAllByRole('listitem')[1]!).getByText('离线')).toBeInTheDocument();
  });

  it('marks the current account without rendering legacy watch actions', () => {
    render(
      <TableSidebar
        isOpen
        activeTab="online"
        tablePlayers={[]}
        onlineUsers={[
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
            active_table_phase: 'playing',
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
            active_table_phase: 'playing',
          },
        ]}
        onlineUserIds={[1, 2]}
        currentUserId={1}
        onToggle={vi.fn()}
        onTabChange={vi.fn()}
      />,
    );

    const rows = screen.getAllByRole('listitem');

    expect(within(rows[0]!).getByText('当前账号')).toBeInTheDocument();
    expect(within(rows[0]!).getByText('与玩家对局中')).toBeInTheDocument();
    expect(within(rows[1]!).getByText('与玩家对局中')).toBeInTheDocument();
    expect(screen.getAllByRole('listitem')).toHaveLength(2);
  });

  it('shows creating and bot-game status labels', () => {
    render(
      <TableSidebar
        isOpen
        activeTab="online"
        tablePlayers={[]}
        onlineUsers={[
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
            active_table_phase: 'playing',
          },
        ]}
        onlineUserIds={[1, 2]}
        creatingTableCodes={['WAIT01']}
        onToggle={vi.fn()}
        onTabChange={vi.fn()}
      />,
    );

    const rows = screen.getAllByRole('listitem');

    expect(within(rows[0]!).getByText('创建牌局中')).toBeInTheDocument();
    expect(within(rows[1]!).getByText('与BOT对局中')).toBeInTheDocument();
  });
});
