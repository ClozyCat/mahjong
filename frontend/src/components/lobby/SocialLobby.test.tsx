import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { SocialLobby } from './SocialLobby';

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

describe('SocialLobby', () => {
  it('shows current user label without multiplier controls', () => {
    render(
      <SocialLobby
        currentUser={currentUser}
        leaderboard={leaderboard}
        onlineUserIds={[1, 2]}
        pendingInvites={[]}
        activeTableCode={null}
        busy={false}
        isCreateTableDisabled={false}
        onCreateTable={vi.fn()}
        onInvite={vi.fn()}
        onAcceptInvite={vi.fn()}
        onLogout={vi.fn()}
      />,
    );

    expect(screen.getByRole('heading', { name: '阿明（平民）' })).toBeInTheDocument();
    expect(screen.queryByRole('group', { name: '牌局倍数' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /x[123]/ })).not.toBeInTheDocument();
  });

  it('invokes invite callback for an online user', async () => {
    const user = userEvent.setup();
    const onInvite = vi.fn();

    render(
      <SocialLobby
        currentUser={currentUser}
        leaderboard={leaderboard}
        onlineUserIds={[1, 2]}
        pendingInvites={[]}
        activeTableCode="ROOM42"
        busy={false}
        isCreateTableDisabled={false}
        onCreateTable={vi.fn()}
        onInvite={onInvite}
        onAcceptInvite={vi.fn()}
        onLogout={vi.fn()}
      />,
    );

    await user.click(screen.getByRole('button', { name: '邀请' }));
    expect(onInvite).toHaveBeenCalledWith(2);
  });

  it('shows pending invite time as Beijing time without raw ISO separators', () => {
    render(
      <SocialLobby
        currentUser={currentUser}
        leaderboard={leaderboard}
        onlineUserIds={[1, 2]}
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
        activeTableCode={null}
        busy={false}
        isCreateTableDisabled={false}
        onCreateTable={vi.fn()}
        onInvite={vi.fn()}
        onAcceptInvite={vi.fn()}
        onLogout={vi.fn()}
      />,
    );

    expect(screen.getByText('邀请时间 2026-05-06 20:00:00')).toBeInTheDocument();
  });
});
