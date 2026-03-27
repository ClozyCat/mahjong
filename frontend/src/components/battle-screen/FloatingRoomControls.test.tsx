import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { FloatingRoomControls } from './FloatingRoomControls';

describe('FloatingRoomControls', () => {
  it('renders room actions vertically and can collapse into a side button', () => {
    render(
      <FloatingRoomControls
        actions={[
          { id: 'ready', label: '准备', enabled: true, emphasis: 'medium' },
          { id: 'start_match', label: '开始对局', enabled: true, emphasis: 'high' },
          { id: 'start_next_round', label: '下一局', enabled: true, emphasis: 'high' },
          { id: 'restart_match', label: '再来一局', enabled: false, emphasis: 'medium' },
        ]}
        onAction={vi.fn()}
      />,
    );

    expect(screen.getByLabelText('房间操作窗口')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '准备' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '开始对局' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '下一局' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '再来一局' })).toBeDisabled();

    fireEvent.click(screen.getByRole('button', { name: '收起房间操作窗口' }));

    expect(screen.queryByLabelText('房间操作窗口')).toBeNull();
    expect(screen.getByRole('button', { name: '展开房间操作窗口' })).toBeInTheDocument();
  });
});
