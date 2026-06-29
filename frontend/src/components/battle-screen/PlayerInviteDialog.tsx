import { createPortal } from 'react-dom';
import { useMemo } from 'react';

import type { PublicUser } from '../../types/match';

export type SentInviteStatus = 'pending' | 'rejected';
export type PlayerInviteStatus = 'online' | 'playing' | 'offline';

export interface InviteDialogUser {
  user: PublicUser;
  status: PlayerInviteStatus;
}

interface PlayerInviteDialogProps {
  isOpen: boolean;
  currentUserId?: number | null;
  users: InviteDialogUser[];
  canInvitePlayers: boolean;
  inviteStatusesByUserId: Record<number, SentInviteStatus>;
  onClose: () => void;
  onInvite: (userId: number) => void;
}

const STATUS_LABELS: Record<PlayerInviteStatus, string> = {
  online: '在线',
  playing: '对局中',
  offline: '离线',
};

const STATUS_ORDER: Record<PlayerInviteStatus, number> = {
  online: 0,
  playing: 1,
  offline: 2,
};

export function PlayerInviteDialog({
  isOpen,
  currentUserId = null,
  users,
  canInvitePlayers,
  inviteStatusesByUserId,
  onClose,
  onInvite,
}: PlayerInviteDialogProps) {
  const sortedUsers = useMemo(() => sortInviteUsers(users), [users]);

  if (!isOpen || typeof document === 'undefined') {
    return null;
  }

  return createPortal(
    <div className="player-list__backdrop" role="presentation">
      <section className="player-list__dialog" role="dialog" aria-modal="true" aria-label="玩家列表">
        <header className="player-list__header">
          <div className="player-list__title-block">
            <span className="player-list__eyebrow">玩家列表</span>
            <p className="player-list__hint">选择可加入当前牌桌的玩家。</p>
          </div>
          <button type="button" className="player-list__close" aria-label="关闭玩家列表" onClick={onClose}>
            关闭
          </button>
        </header>

        <div className="player-list__content">
          {sortedUsers.length === 0 ? (
            <div className="player-list__empty">暂无可显示玩家</div>
          ) : (
            <ul className="player-list__rows">
              {sortedUsers.map(({ user, status }) => (
                <PlayerInviteRow
                  key={user.user_id}
                  user={user}
                  status={status}
                  isSelf={currentUserId === user.user_id}
                  canInvitePlayers={canInvitePlayers}
                  inviteStatus={inviteStatusesByUserId[user.user_id]}
                  onInvite={onInvite}
                />
              ))}
            </ul>
          )}
        </div>
      </section>
    </div>,
    document.body,
  );
}

function PlayerInviteRow({
  user,
  status,
  isSelf,
  canInvitePlayers,
  inviteStatus,
  onInvite,
}: {
  user: PublicUser;
  status: PlayerInviteStatus;
  isSelf: boolean;
  canInvitePlayers: boolean;
  inviteStatus?: SentInviteStatus;
  onInvite: (userId: number) => void;
}) {
  const disabled =
    isSelf || !canInvitePlayers || status === 'playing' || status === 'offline' || inviteStatus === 'pending';
  const buttonLabel =
    inviteStatus === 'pending'
      ? '已发送'
      : inviteStatus === 'rejected'
        ? '已拒绝'
        : isSelf
          ? '自己'
          : '邀请';

  return (
    <li className="player-list__row">
      <div className="player-list__row-info">
        <strong>{user.display_label}</strong>
        <span>{user.points} 积分</span>
      </div>
      <span className={`player-list__status player-list__status--${status}`}>{STATUS_LABELS[status]}</span>
      <button type="button" disabled={disabled} onClick={() => onInvite(user.user_id)}>
        {buttonLabel}
      </button>
    </li>
  );
}

function sortInviteUsers(users: InviteDialogUser[]) {
  return [...users].sort((left, right) => {
    const statusDelta = STATUS_ORDER[left.status] - STATUS_ORDER[right.status];
    if (statusDelta !== 0) {
      return statusDelta;
    }
    return right.user.points - left.user.points;
  });
}
