import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { SocialSidebarPanel } from './SocialSidebarPanel';

const currentUser = {
  user_id: 1,
  username: 'alice',
  display_name: '阿明',
  points: 50,
  title: '平民',
  display_label: '阿明（平民）',
  bio: '',
  avatar: null,
};

const leaderboard = [
  currentUser,
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
];

const defaultProps = {
  currentUser,
  leaderboard,
  onlineUserIds: [1, 2],
  pendingInvites: [],
  spectatorRequests: [],
  activeTableCode: null,
  inviteDialog: null,
  busy: false,
  isCreateTableDisabled: false,
  canInvitePlayers: false,
  isOwner: false,
  onCreateTable: vi.fn(),
  onInvite: vi.fn(),
  onAcceptInvite: vi.fn(),
  onApproveSpectatorRequest: vi.fn(),
  onRejectSpectatorRequest: vi.fn(),
  onDismissInviteDialog: vi.fn(),
  onLogout: vi.fn(),
};

describe('SocialSidebarPanel', () => {
  it('shows current user without multiplier controls inside the sidebar', () => {
    render(
      <SocialSidebarPanel
        {...defaultProps}
      />,
    );

    expect(screen.getByRole('region', { name: '牌桌侧栏首页' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: '阿明（平民）' })).toBeInTheDocument();
    expect(screen.queryByRole('group', { name: '牌局倍数' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /x[123]/ })).not.toBeInTheDocument();
    expect(screen.getByRole('heading', { name: '待处理观战申请' })).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: '积分榜' })).not.toBeInTheDocument();
  });

  it('invokes invite callback for an online user', async () => {
    const user = userEvent.setup();
    const onInvite = vi.fn();

    render(
      <SocialSidebarPanel
        {...defaultProps}
        activeTableCode="ROOM42"
        canInvitePlayers={true}
        onInvite={onInvite}
      />,
    );

    await user.click(screen.getByRole('button', { name: '邀请' }));
    expect(onInvite).toHaveBeenCalledWith(2);
  });

  it('disables invite buttons when the active table has no replaceable bot seats', async () => {
    const user = userEvent.setup();
    const onInvite = vi.fn();

    render(
      <SocialSidebarPanel
        {...defaultProps}
        activeTableCode="ROOM42"
        canInvitePlayers={false}
        onInvite={onInvite}
      />,
    );

    const inviteButton = screen.getAllByRole('button').find((button) => button.textContent?.trim() === '邀请');
    expect(inviteButton).toBeDefined();
    expect(inviteButton).toBeDisabled();

    await user.click(inviteButton!);

    expect(onInvite).not.toHaveBeenCalled();
  });

  it('lets the owner approve and reject spectator requests from the room tab panel', async () => {
    const user = userEvent.setup();
    const onApproveSpectatorRequest = vi.fn();
    const onRejectSpectatorRequest = vi.fn();

    render(
      <SocialSidebarPanel
        {...defaultProps}
        isOwner
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
        onApproveSpectatorRequest={onApproveSpectatorRequest}
        onRejectSpectatorRequest={onRejectSpectatorRequest}
      />,
    );

    expect(screen.getByRole('heading', { name: '待处理观战申请' })).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: '积分榜' })).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: '同意' }));
    await user.click(screen.getByRole('button', { name: '拒绝' }));

    expect(onApproveSpectatorRequest).toHaveBeenCalledWith(3);
    expect(onRejectSpectatorRequest).toHaveBeenCalledWith(3);
  });
});
