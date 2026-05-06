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
  it('shows current user label and multiplier controls', () => {
    render(
      <SocialLobby
        currentUser={currentUser}
        leaderboard={leaderboard}
        onlineUserIds={[1, 2]}
        pendingInvites={[]}
        activeTableCode={null}
        inviteDialog={null}
        multiplier={2}
        busy={false}
        onMultiplierChange={vi.fn()}
        onCreateTable={vi.fn()}
        onEnterTable={vi.fn()}
        onInvite={vi.fn()}
        onAcceptInvite={vi.fn()}
        onDismissInviteDialog={vi.fn()}
        onLogout={vi.fn()}
      />,
    );

    expect(screen.getByRole('heading', { name: '阿明（平民）' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'x1' })).toHaveAttribute('aria-pressed', 'false');
    expect(screen.getByRole('button', { name: 'x2' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByRole('button', { name: 'x3' })).toHaveAttribute('aria-pressed', 'false');
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
        inviteDialog={null}
        multiplier={1}
        busy={false}
        onMultiplierChange={vi.fn()}
        onCreateTable={vi.fn()}
        onEnterTable={vi.fn()}
        onInvite={onInvite}
        onAcceptInvite={vi.fn()}
        onDismissInviteDialog={vi.fn()}
        onLogout={vi.fn()}
      />,
    );

    await user.click(screen.getByRole('button', { name: '邀请' }));
    expect(onInvite).toHaveBeenCalledWith(2);
  });
});
