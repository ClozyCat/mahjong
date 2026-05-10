import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { SocialSidebarMessagesPanel, SocialSidebarPanel } from './SocialSidebarPanel';

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
  busy: false,
  isCreateTableDisabled: false,
  canInvitePlayers: false,
  inviteStatusesByUserId: {},
  onCreateTable: vi.fn(),
  onInvite: vi.fn(),
  onAcceptInvite: vi.fn(),
  onRejectInvite: vi.fn(),
  onApproveSpectatorRequest: vi.fn(),
  onRejectSpectatorRequest: vi.fn(),
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
    expect(screen.queryByRole('heading', { name: '待处理邀请' })).not.toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: '待处理观战申请' })).not.toBeInTheDocument();
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

  it('shows pending sent invites as disabled already invited buttons', async () => {
    const user = userEvent.setup();
    const onInvite = vi.fn();

    render(
      <SocialSidebarPanel
        {...defaultProps}
        activeTableCode="ROOM42"
        canInvitePlayers={true}
        inviteStatusesByUserId={{ 2: 'pending' }}
        onInvite={onInvite}
      />,
    );

    const invitedButton = screen.getByRole('button', { name: '已邀请' });
    expect(invitedButton).toBeDisabled();

    await user.click(invitedButton);

    expect(onInvite).not.toHaveBeenCalled();
  });

  it('lets rejected sent invites be sent again from the rejected button', async () => {
    const user = userEvent.setup();
    const onInvite = vi.fn();

    render(
      <SocialSidebarPanel
        {...defaultProps}
        activeTableCode="ROOM42"
        canInvitePlayers={true}
        inviteStatusesByUserId={{ 2: 'rejected' }}
        onInvite={onInvite}
      />,
    );

    const rejectedButton = screen.getByRole('button', { name: '已被拒绝' });
    expect(rejectedButton).toBeEnabled();

    await user.click(rejectedButton);

    expect(onInvite).toHaveBeenCalledWith(2);
  });

  it('lets invitees reject pending table invites', async () => {
    const user = userEvent.setup();
    const onRejectInvite = vi.fn();

    render(
      <SocialSidebarMessagesPanel
        pendingInvites={[
          {
            id: 9,
            table_code: 'ROOM42',
            inviter_user_id: 2,
            invitee_user_id: 1,
            status: 'pending',
            created_at: '2026-05-06T12:00:00Z',
            expires_at: '2026-05-06T12:10:00Z',
          },
        ]}
        spectatorRequests={[]}
        onAcceptInvite={vi.fn()}
        onRejectInvite={onRejectInvite}
        onApproveSpectatorRequest={vi.fn()}
        onRejectSpectatorRequest={vi.fn()}
      />,
    );

    await user.click(screen.getByRole('button', { name: '拒绝' }));

    expect(screen.getByText('邀请时间 2026-05-06 20:00:00')).toBeInTheDocument();
    expect(onRejectInvite).toHaveBeenCalledWith(expect.objectContaining({ id: 9 }));
  });

  it('shows invite copy with inviter and table code', () => {
    render(
      <SocialSidebarMessagesPanel
        pendingInvites={[
          {
            id: 9,
            table_code: 'ROOM42',
            inviter_user_id: 2,
            invitee_user_id: 1,
            status: 'pending',
            created_at: '2026-05-06T12:00:00Z',
            expires_at: '2026-05-06T12:10:00Z',
          },
        ]}
        spectatorRequests={[]}
        inviteCreatorLabelsByUserId={{ 2: '阿强（平民）' }}
        onAcceptInvite={vi.fn()}
        onRejectInvite={vi.fn()}
        onApproveSpectatorRequest={vi.fn()}
        onRejectSpectatorRequest={vi.fn()}
      />,
    );

    expect(screen.getAllByText('阿强（平民）创建的牌桌ROOM42邀请你加入。')).toHaveLength(1);
    expect(screen.queryByRole('region', { name: '牌局邀请' })).not.toBeInTheDocument();
  });

  it('lets the target player approve and reject spectator requests from the room tab panel', async () => {
    const user = userEvent.setup();
    const onApproveSpectatorRequest = vi.fn();
    const onRejectSpectatorRequest = vi.fn();

    render(
      <SocialSidebarMessagesPanel
        pendingInvites={[]}
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
        spectatorRequesterLabelsByUserId={{ 7: '阿成（雀士）' }}
        onAcceptInvite={vi.fn()}
        onRejectInvite={vi.fn()}
        onApproveSpectatorRequest={onApproveSpectatorRequest}
        onRejectSpectatorRequest={onRejectSpectatorRequest}
      />,
    );

    expect(screen.getByRole('heading', { name: '待处理观战申请' })).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: '积分榜' })).not.toBeInTheDocument();
    expect(screen.getByText('阿成（雀士）')).toBeInTheDocument();
    expect(screen.queryByText('用户 #7')).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: '同意' }));
    await user.click(screen.getByRole('button', { name: '拒绝' }));

    expect(onApproveSpectatorRequest).toHaveBeenCalledWith(3);
    expect(onRejectSpectatorRequest).toHaveBeenCalledWith(3);
  });
});
