import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { EvaluationDialog } from './EvaluationDialog';
import type { InviteDialogUser } from './PlayerInviteDialog';

function createDialogUser(
  userId: number,
  displayName: string,
  status: InviteDialogUser['status'],
  isSpecialBot = false,
): InviteDialogUser {
  return {
    user: {
      user_id: userId,
      username: `user-${userId}`,
      display_name: displayName,
      points: 100,
      title: isSpecialBot ? 'AI' : '平民',
      display_label: `${displayName}（${isSpecialBot ? 'AI' : '平民'}）`,
      bio: '',
      avatar: null,
      is_special_bot: isSpecialBot,
    },
    status,
  };
}

describe('EvaluationDialog', () => {
  it('only shows online idle users as comparison candidates', () => {
    render(
      <EvaluationDialog
        isOpen
        currentUserId={1}
        humanUsers={[
          createDialogUser(1, '当前玩家', 'online'),
          createDialogUser(2, '在线玩家', 'online'),
          createDialogUser(3, '离线玩家', 'offline'),
          createDialogUser(4, '对局玩家', 'playing'),
        ]}
        aiUsers={[
          createDialogUser(5, '在线 AI', 'online', true),
          createDialogUser(6, '对局 AI', 'playing', true),
        ]}
        selectedUserIds={[]}
        onToggleSubject={vi.fn()}
        onStart={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    const dialog = screen.getByRole('dialog', { name: '创建评测' });

    expect(dialog).toHaveTextContent('在线玩家（平民）');
    expect(dialog).toHaveTextContent('在线 AI（AI）');
    expect(dialog).not.toHaveTextContent('当前玩家');
    expect(dialog).not.toHaveTextContent('离线玩家');
    expect(dialog).not.toHaveTextContent('对局玩家');
    expect(dialog).not.toHaveTextContent('对局 AI');
  });
});
