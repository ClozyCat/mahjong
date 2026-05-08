import type { PublicUser, TableInvite } from '../../types/match';
import { formatBeijingDateTime } from '../../lib/dateTime';

interface SocialLobbyProps {
  currentUser: PublicUser;
  leaderboard: PublicUser[];
  onlineUserIds: number[];
  pendingInvites: TableInvite[];
  activeTableCode: string | null;
  busy: boolean;
  isCreateTableDisabled: boolean;
  message?: string | null;
  onCreateTable: () => void;
  onInvite: (userId: number) => void;
  onAcceptInvite: (invite: TableInvite) => void;
  onLogout: () => void;
}

function getInviteCreatorLabel(invite: TableInvite, leaderboard: PublicUser[]) {
  return leaderboard.find((user) => user.user_id === invite.inviter_user_id)?.display_label ?? `用户 #${invite.inviter_user_id}`;
}

function getInviteCopy(invite: TableInvite, leaderboard: PublicUser[]) {
  return `${getInviteCreatorLabel(invite, leaderboard)}创建的牌桌${invite.table_code}邀请你加入。`;
}

export function SocialLobby({
  currentUser,
  leaderboard,
  onlineUserIds,
  pendingInvites,
  activeTableCode,
  busy,
  isCreateTableDisabled,
  message,
  onCreateTable,
  onInvite,
  onAcceptInvite,
  onLogout,
}: SocialLobbyProps) {
  const onlineUsers = leaderboard.filter(
    (user) => user.user_id !== currentUser.user_id && onlineUserIds.includes(user.user_id),
  );
  const topUsers = leaderboard.slice(0, 8);

  return (
    <main className="social-lobby" aria-label="Social lobby">
      <header className="social-lobby__hero">
        <div className="social-lobby__brand">
          <p className="social-lobby__eyebrow">Social Hub</p>
          <h1>{currentUser.display_label}</h1>
          <div className="social-lobby__meta">
            <span className="social-lobby__points-badge">
              积分 <strong>{currentUser.points}</strong>
            </span>
            <span className="social-lobby__status-indicator">在线</span>
          </div>
        </div>
        <button type="button" className="social-lobby__logout-btn" onClick={onLogout}>
          退出账号
        </button>
      </header>

      {message ? <p className="social-lobby__message">{message}</p> : null}

      <section className="social-lobby__grid">
        <div className="social-lobby__panel">
          <h2>创建牌局</h2>
          <div className="social-lobby__actions">
            <button type="button" className="social-lobby__primary" disabled={isCreateTableDisabled} onClick={onCreateTable}>
              创建牌局
            </button>
          </div>
          <p className="social-lobby__hint">
            {activeTableCode ? `当前待开局牌桌：${activeTableCode}` : '先创建牌局，再邀请其他玩家。'}
          </p>
        </div>

        <div className="social-lobby__panel">
          <h2>待处理邀请</h2>
          <ul className="social-lobby__list">
            {pendingInvites.length === 0 ? <li className="social-lobby__empty">暂无待处理邀请</li> : null}
            {pendingInvites.map((invite) => (
              <li key={invite.id} className="social-lobby__row">
                <div>
                  <strong>{invite.table_code}</strong>
                  <span>{getInviteCopy(invite, leaderboard)}</span>
                  <span>邀请时间 {formatBeijingDateTime(invite.created_at)}</span>
                </div>
                <button type="button" onClick={() => onAcceptInvite(invite)}>
                  接受邀请
                </button>
              </li>
            ))}
          </ul>
        </div>

        <div className="social-lobby__panel">
          <h2>在线玩家</h2>
          <ul className="social-lobby__list">
            {onlineUsers.length === 0 ? <li className="social-lobby__empty">暂无在线玩家</li> : null}
            {onlineUsers.map((user) => (
              <li key={user.user_id} className="social-lobby__row">
                <div>
                  <strong>{user.display_label}</strong>
                  <span>{user.points} 分</span>
                </div>
                <button
                  type="button"
                  disabled={!activeTableCode || user.user_id === currentUser.user_id || busy}
                  onClick={() => onInvite(user.user_id)}
                >
                  邀请
                </button>
              </li>
            ))}
          </ul>
        </div>

        <div className="social-lobby__panel">
          <h2>积分榜</h2>
          <ol className="social-lobby__ranking">
            {topUsers.map((user) => (
              <li key={user.user_id}>
                <span>{user.display_label}</span>
                <strong>{user.points}</strong>
              </li>
            ))}
          </ol>
        </div>
      </section>
    </main>
  );
}
