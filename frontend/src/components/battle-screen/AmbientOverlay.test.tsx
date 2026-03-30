import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { AmbientOverlay } from './AmbientOverlay';

describe('AmbientOverlay', () => {
  it('starts collapsed and only shows the log window after explicit expand', () => {
    render(
      <AmbientOverlay
        mode="watching"
        promptText={null}
        waitingControls={null}
        toasts={[
          { id: 't1', kind: 'event', text: '提示1', createdAt: '2026-03-30T12:10:36+08:00' },
          { id: 't2', kind: 'event', text: '提示2', createdAt: '2026-03-30T12:10:37+08:00' },
          { id: 't3', kind: 'system', text: '提示3', createdAt: '2026-03-30T12:10:38+08:00' },
          { id: 't4', kind: 'error', text: '提示4', createdAt: '2026-03-30T12:10:39+08:00' },
          { id: 't5', kind: 'event', text: '提示5', createdAt: '2026-03-30T12:10:40+08:00' },
        ]}
      />,
    );

    expect(screen.queryByLabelText('日志窗口')).toBeNull();
    expect(screen.getByRole('button', { name: '展开日志窗口' })).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '展开日志窗口' }));

    expect(screen.getByLabelText('日志窗口')).toBeInTheDocument();
    expect(screen.getByLabelText('日志列表')).toBeInTheDocument();
    expect(screen.getByText('提示1')).toBeInTheDocument();
    expect(screen.getByText('提示5')).toBeInTheDocument();
    expect(screen.getByText('12:10:36')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '收起日志窗口' }));

    expect(screen.queryByLabelText('日志窗口')).toBeNull();
    expect(screen.getByRole('button', { name: '展开日志窗口' })).toBeInTheDocument();
  });
});
