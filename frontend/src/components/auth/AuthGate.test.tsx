import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { AuthGate } from './AuthGate';

describe('AuthGate', () => {
  it('shows login UI by default', () => {
    render(<AuthGate status="idle" onLogin={vi.fn()} onRegister={vi.fn()} />);

    expect(screen.getByRole('tab', { name: '登录' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByLabelText('账号昵称')).toBeInTheDocument();
    expect(screen.getByLabelText('密码')).toBeInTheDocument();
  });

  it('uses browser form semantics and disables empty login submits', async () => {
    const user = userEvent.setup();
    const onLogin = vi.fn();

    render(<AuthGate status="idle" onLogin={onLogin} onRegister={vi.fn()} />);

    const identifierInput = screen.getByLabelText('账号昵称');
    const passwordInput = screen.getByLabelText('密码');
    const submitButton = screen.getByRole('button', { name: '登录' });

    expect(identifierInput).toHaveAttribute('autocomplete', 'username');
    expect(identifierInput).toBeRequired();
    expect(passwordInput).toHaveAttribute('autocomplete', 'current-password');
    expect(passwordInput).toBeRequired();
    expect(submitButton).toBeDisabled();

    await user.type(identifierInput, '  阿明  ');
    await user.type(passwordInput, '  secret-123  ');

    expect(submitButton).toBeEnabled();
    await user.click(submitButton);

    expect(onLogin).toHaveBeenCalledWith({
      identifier: '阿明',
      password: 'secret-123',
    });
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

  it('uses invite registration semantics and disables empty registration submits', async () => {
    const user = userEvent.setup();
    const onRegister = vi.fn();

    render(<AuthGate status="idle" onLogin={vi.fn()} onRegister={onRegister} />);

    await user.click(screen.getByRole('tab', { name: '邀请码注册' }));

    const inviteCodeInput = screen.getByLabelText('邀请码');
    const displayNameInput = screen.getByLabelText('昵称');
    const passwordInput = screen.getByLabelText('密码');
    const submitButton = screen.getByRole('button', { name: '注册并登录' });

    expect(inviteCodeInput).toHaveAttribute('autocomplete', 'one-time-code');
    expect(inviteCodeInput).toBeRequired();
    expect(displayNameInput).toHaveAttribute('autocomplete', 'nickname');
    expect(displayNameInput).toBeRequired();
    expect(passwordInput).toHaveAttribute('autocomplete', 'new-password');
    expect(passwordInput).toBeRequired();
    expect(submitButton).toBeDisabled();

    await user.type(inviteCodeInput, '  INVITE-2  ');
    await user.type(displayNameInput, '  小夏  ');
    await user.type(passwordInput, '  secret-456  ');

    expect(submitButton).toBeEnabled();
    await user.click(submitButton);

    expect(onRegister).toHaveBeenCalledWith({
      inviteCode: 'INVITE-2',
      displayName: '小夏',
      password: 'secret-456',
    });
  });
});
