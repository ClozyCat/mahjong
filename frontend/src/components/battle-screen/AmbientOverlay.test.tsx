import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { AmbientOverlay } from './AmbientOverlay';

describe('AmbientOverlay', () => {
  it('starts collapsed and only shows the message window after explicit expand', () => {
    render(
      <AmbientOverlay
        mode="watching"
        promptText={null}
        waitingControls={null}
        toasts={[
          { id: 't1', kind: 'event', text: '提示1' },
          { id: 't2', kind: 'event', text: '提示2' },
          { id: 't3', kind: 'system', text: '提示3' },
          { id: 't4', kind: 'error', text: '提示4' },
          { id: 't5', kind: 'event', text: '提示5' },
        ]}
      />,
    );

    expect(screen.queryByLabelText('消息窗口')).toBeNull();
    expect(screen.getByRole('button', { name: '展开消息窗口' })).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '展开消息窗口' }));

    expect(screen.getByLabelText('消息窗口')).toBeInTheDocument();
    expect(screen.getByLabelText('消息列表')).toBeInTheDocument();
    expect(screen.getByText('提示1')).toBeInTheDocument();
    expect(screen.getByText('提示5')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '收起消息窗口' }));

    expect(screen.queryByLabelText('消息窗口')).toBeNull();
    expect(screen.getByRole('button', { name: '展开消息窗口' })).toBeInTheDocument();
  });
});
