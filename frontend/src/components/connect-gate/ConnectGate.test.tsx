import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { ConnectGate } from './ConnectGate';

describe('ConnectGate', () => {
  it('renders chinese-first lobby copy inside a win98 shell', () => {
    const { container } = render(
      <ConnectGate
        value={{
          apiBaseUrl: 'http://localhost:8080',
          wsBaseUrl: 'ws://localhost:8080',
          tableCode: '',
          nickname: '',
          testMode: false,
        }}
        status="idle"
        onChange={vi.fn()}
        onCreate={vi.fn()}
        onJoin={vi.fn()}
      />,
    );

    expect(screen.getByText('联机大厅')).toBeInTheDocument();
    expect(screen.getByLabelText('服务地址')).toBeInTheDocument();
    expect(screen.getByLabelText('通信地址')).toBeInTheDocument();
    expect(screen.getByLabelText('昵称')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '测试模式：关' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '创建牌桌' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '加入牌桌' })).toBeInTheDocument();
    expect(container.querySelector('.win98-window')).not.toBeNull();
  });

  it('forwards field edits and create/join actions', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const onCreate = vi.fn();
    const onJoin = vi.fn();

    render(
      <ConnectGate
        value={{
          apiBaseUrl: 'http://localhost:8080',
          wsBaseUrl: 'ws://localhost:8080',
          tableCode: 'AB12CD',
          nickname: 'Player A',
          testMode: false,
        }}
        status="idle"
        onChange={onChange}
        onCreate={onCreate}
        onJoin={onJoin}
      />,
    );

    await user.type(screen.getByLabelText(/牌桌编号/i), 'Z');
    await user.click(screen.getByRole('button', { name: '测试模式：关' }));
    await user.click(screen.getByRole('button', { name: '创建牌桌' }));
    await user.click(screen.getByRole('button', { name: '加入牌桌' }));

    expect(onChange).toHaveBeenCalled();
    expect(onChange).toHaveBeenCalledWith({ testMode: true });
    expect(onCreate).toHaveBeenCalledTimes(1);
    expect(onJoin).toHaveBeenCalledTimes(1);
  });
});
