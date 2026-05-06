import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { AuthGate } from './AuthGate';

describe('AuthGate', () => {
  it('shows login UI by default', () => {
    render(<AuthGate status="idle" onLogin={vi.fn()} onRegister={vi.fn()} />);

    expect(screen.getByRole('tab', { name: '登录' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByLabelText('账号或用户 ID')).toBeInTheDocument();
    expect(screen.getByLabelText('密码')).toBeInTheDocument();
  });

  it('submits invite-code registration payload', async () => {
    const user = userEvent.setup();
    const onRegister = vi.fn();

    render(<AuthGate status="idle" onLogin={vi.fn()} onRegister={onRegister} />);

    await user.click(screen.getByRole('tab', { name: '邀请码注册' }));
    await user.type(screen.getByLabelText('邀请码'), 'INVITE-1');
    await user.type(screen.getByLabelText('昵称'), '阿明');
    await user.type(screen.getByLabelText('密码'), 'secret-123');
    await user.click(screen.getByRole('button', { name: '注册并登录' }));

    expect(onRegister).toHaveBeenCalledWith({
      inviteCode: 'INVITE-1',
      displayName: '阿明',
      password: 'secret-123',
    });
  });
});
