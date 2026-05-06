import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { ConnectGate } from './ConnectGate';

describe('ConnectGate', () => {
  it('renders the current lobby layout and controls', () => {
    render(
      <ConnectGate
        value={{
          tableCode: '',
          nickname: '',
        }}
        status="idle"
        themeLabel="天水碧"
        canCreate={false}
        canJoin={false}
        onChange={vi.fn()}
        onCreate={vi.fn()}
        onJoin={vi.fn()}
      />,
    );

    expect(screen.getByText('启局入席')).toBeInTheDocument();
    expect(screen.getByText('输入昵称后即可开启牌局。')).toBeInTheDocument();
    expect(screen.getByLabelText('牌桌编号')).toBeInTheDocument();
    expect(screen.getByLabelText('您的昵称')).toBeInTheDocument();
    expect(screen.getByText('天水碧')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '创建新局' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '加入牌桌' })).toBeInTheDocument();
  });

  it('forwards field edits and create/join actions', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const onCreate = vi.fn();
    const onJoin = vi.fn();

    render(
      <ConnectGate
        value={{
          tableCode: 'AB12CD',
          nickname: 'Player A',
        }}
        status="idle"
        themeLabel="秋香"
        canCreate={true}
        canJoin={true}
        onChange={onChange}
        onCreate={onCreate}
        onJoin={onJoin}
      />,
    );

    await user.type(screen.getByLabelText(/牌桌编号/i), 'Z');
    await user.click(screen.getByRole('button', { name: '创建新局' }));
    await user.click(screen.getByRole('button', { name: '加入牌桌' }));

    expect(onChange).toHaveBeenCalled();
    expect(onCreate).toHaveBeenCalledTimes(1);
    expect(onJoin).toHaveBeenCalledTimes(1);
  });

  it('renders spectator entry and respects canWatch state', async () => {
    const user = userEvent.setup();
    const onWatch = vi.fn();

    const { rerender } = render(
      <ConnectGate
        value={{
          tableCode: 'AB12CD',
          nickname: 'Viewer',
        }}
        status="idle"
        themeLabel="天水碧"
        canCreate={true}
        canJoin={true}
        canWatch={false}
        onChange={vi.fn()}
        onCreate={vi.fn()}
        onJoin={vi.fn()}
        onWatch={onWatch}
      />,
    );

    expect(screen.getByRole('button', { name: '观战牌桌' })).toBeDisabled();

    rerender(
      <ConnectGate
        value={{
          tableCode: 'AB12CD',
          nickname: 'Viewer',
        }}
        status="idle"
        themeLabel="天水碧"
        canCreate={true}
        canJoin={true}
        canWatch={true}
        onChange={vi.fn()}
        onCreate={vi.fn()}
        onJoin={vi.fn()}
        onWatch={onWatch}
      />,
    );

    await user.click(screen.getByRole('button', { name: '观战牌桌' }));

    expect(onWatch).toHaveBeenCalledTimes(1);
  });

  it('shows a validation hint and disables create/join when the table code is invalid', () => {
    render(
      <ConnectGate
        value={{
          tableCode: '房间-01',
          nickname: 'Player A',
        }}
        status="idle"
        themeLabel="月白"
        tableCodeError="牌桌编号仅支持数字和英文字母。"
        canCreate={false}
        canJoin={false}
        onChange={vi.fn()}
        onCreate={vi.fn()}
        onJoin={vi.fn()}
      />,
    );

    expect(screen.getAllByText('牌桌编号仅支持数字和英文字母。')).toHaveLength(2);
    expect(screen.getByRole('button', { name: '创建新局' })).toBeDisabled();
    expect(screen.getByRole('button', { name: '加入牌桌' })).toBeDisabled();
  });

  it('waits until composition ends before syncing ime input to the parent state', () => {
    const onChange = vi.fn();

    render(
      <ConnectGate
        value={{
          tableCode: '',
          nickname: '',
        }}
        status="idle"
        themeLabel="月白"
        canCreate={false}
        canJoin={false}
        onChange={onChange}
        onCreate={vi.fn()}
        onJoin={vi.fn()}
      />,
    );

    const tableCodeInput = screen.getByLabelText('牌桌编号');
    const nicknameInput = screen.getByLabelText('您的昵称');

    fireEvent.compositionStart(tableCodeInput);
    fireEvent.change(tableCodeInput, { target: { value: 'ab12' } });
    expect(tableCodeInput).toHaveValue('ab12');
    expect(onChange).not.toHaveBeenCalled();

    fireEvent.compositionEnd(tableCodeInput);
    expect(tableCodeInput).toHaveValue('AB12');
    expect(onChange).toHaveBeenLastCalledWith({ tableCode: 'AB12' });

    onChange.mockClear();

    fireEvent.compositionStart(nicknameInput);
    fireEvent.change(nicknameInput, { target: { value: 'ni' } });
    fireEvent.change(nicknameInput, { target: { value: '你' } });
    expect(nicknameInput).toHaveValue('你');
    expect(onChange).not.toHaveBeenCalled();

    fireEvent.compositionEnd(nicknameInput);
    expect(onChange).toHaveBeenLastCalledWith({ nickname: '你' });
  });
});
