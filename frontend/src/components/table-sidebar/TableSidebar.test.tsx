import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { TableSidebar } from './TableSidebar';

function renderSidebar(isOwner: boolean) {
  return render(
    <TableSidebar
      isOpen
      activeTab="requests"
      tablePlayers={[]}
      onlineUsers={[]}
      spectators={[]}
      spectatorRequests={[
        {
          id: 3,
          table_code: 'AB12CD',
          requester_user_id: 7,
          owner_user_id: 1,
          status: 'pending',
          created_at: '2026-05-06T12:00:00Z',
          decided_at: null,
        },
      ]}
      isOwner={isOwner}
      profilePanel={<div>profile</div>}
      onToggle={vi.fn()}
      onTabChange={vi.fn()}
      onSelectUser={vi.fn()}
      onApproveRequest={vi.fn()}
      onRejectRequest={vi.fn()}
    />,
  );
}

describe('TableSidebar', () => {
  it('does not show approval buttons for non-owner viewers', () => {
    renderSidebar(false);

    expect(screen.queryByRole('button', { name: '同意' })).toBeNull();
    expect(screen.queryByRole('button', { name: '拒绝' })).toBeNull();
    expect(screen.getByText('仅房主可审批观战申请')).toBeInTheDocument();
  });

  it('lets the owner approve and reject spectator requests', async () => {
    const user = userEvent.setup();
    const onApproveRequest = vi.fn();
    const onRejectRequest = vi.fn();

    render(
      <TableSidebar
        isOpen
        activeTab="requests"
        tablePlayers={[]}
        onlineUsers={[]}
        spectators={[]}
        spectatorRequests={[
          {
            id: 3,
            table_code: 'AB12CD',
            requester_user_id: 7,
            owner_user_id: 1,
            status: 'pending',
            created_at: '2026-05-06T12:00:00Z',
            decided_at: null,
          },
        ]}
        isOwner
        profilePanel={<div>profile</div>}
        onToggle={vi.fn()}
        onTabChange={vi.fn()}
        onSelectUser={vi.fn()}
        onApproveRequest={onApproveRequest}
        onRejectRequest={onRejectRequest}
      />,
    );

    await user.click(screen.getByRole('button', { name: '同意' }));
    await user.click(screen.getByRole('button', { name: '拒绝' }));

    expect(onApproveRequest).toHaveBeenCalledWith(3);
    expect(onRejectRequest).toHaveBeenCalledWith(3);
  });
});
