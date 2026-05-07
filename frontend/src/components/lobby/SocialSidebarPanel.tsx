import type { PublicUser, TableInvite } from '../../types/match';

interface SocialSidebarPanelProps {
  currentUser: PublicUser;
  leaderboard: PublicUser[];
  onlineUserIds: number[];
  pendingInvites: TableInvite[];
  activeTableCode: string | null;
  inviteDialog: TableInvite | null;
  busy: boolean;
  message?: string | null;
  onCreateTable: () => void;
  onEnterTable: () => void;
  onInvite: (userId: number) => void;
  onAcceptInvite: (invite: TableInvite) => void;
  onDismissInviteDialog: () => void;
  onLogout: () => void;
}

export function SocialSidebarPanel({
  currentUser,
  leaderboard,
  onlineUserIds,
  pendingInvites,
  activeTableCode,
  inviteDialog,
  busy,
  message,
  onCreateTable,
  onEnterTable,
  onInvite,
  onAcceptInvite,
  onDismissInviteDialog,
  onLogout,
}: SocialSidebarPanelProps) {
  const onlineUsers = leaderboard.filter(
    (user) => user.user_id !== currentUser.user_id && onlineUserIds.includes(user.user_id),
  );
  const topUsers = leaderboard.slice(0, 8);

  return (
    <div className="social-sidebar" role="region" aria-label="牌桌侧栏首页">
      <header className="social-sidebar__header">
        <div>
          <p className="social-sidebar__eyebrow">Table Hub</p>
          <h2>{currentUser.display_label}</h2>
          <span>积分 {currentUser.points}</span>
        </div>
        <button type="button" className="social-sidebar__logout" onClick={onLogout}>
          退出
        </button>
      </header>

      {message ? <p className="social-sidebar__message">{message}</p> : null}

      {inviteDialog ? (
        <section className="social-sidebar__notice" aria-label="牌局邀请">
          <strong>收到牌局邀请</strong>
          <span>牌桌 {inviteDialog.table_code} 邀请你加入。</span>
          <div className="social-sidebar__actions">
            <button type="button" className="social-sidebar__primary" onClick={() => onAcceptInvite(inviteDialog)}>
              接受邀请
            </button>
            <button type="button" onClick={onDismissInviteDialog}>
              稍后处理
            </button>
          </div>
        </section>
      ) : null}

      <section className="social-sidebar__section">
        <h3>创建牌局</h3>
        <div className="social-sidebar__actions">
          <button type="button" className="social-sidebar__primary" disabled={busy} onClick={onCreateTable}>
            创建牌局
          </button>
          <button type="button" disabled={!activeTableCode || busy} onClick={onEnterTable}>
            进入牌桌
          </button>
        </div>
        <p>{activeTableCode ? `当前待开局牌桌：${activeTableCode}` : '创建后可在侧栏邀请在线玩家。'}</p>
      </section>

      <section className="social-sidebar__section">
        <h3>待处理邀请</h3>
        <ul className="table-sidebar__list">
          {pendingInvites.length === 0 ? <li className="table-sidebar__empty">暂无待处理邀请</li> : null}
          {pendingInvites.map((invite) => (
            <li key={invite.id} className="table-sidebar__row table-sidebar__row--stacked">
              <div>
                <strong>{invite.table_code}</strong>
                <span>邀请时间 {invite.created_at}</span>
              </div>
              <button type="button" onClick={() => onAcceptInvite(invite)}>
                接受
              </button>
            </li>
          ))}
        </ul>
      </section>

      <section className="social-sidebar__section">
        <h3>在线玩家</h3>
        <ul className="table-sidebar__list">
          {onlineUsers.length === 0 ? <li className="table-sidebar__empty">暂无在线玩家</li> : null}
          {onlineUsers.map((user) => (
            <li key={user.user_id} className="table-sidebar__row">
              <div>
                <strong>{user.display_label}</strong>
                <span>{user.points} 分</span>
              </div>
              <button type="button" disabled={!activeTableCode || busy} onClick={() => onInvite(user.user_id)}>
                邀请
              </button>
            </li>
          ))}
        </ul>
      </section>

      <section className="social-sidebar__section">
        <h3>积分榜</h3>
        <ol className="social-sidebar__ranking">
          {topUsers.map((user) => (
            <li key={user.user_id}>
              <span>{user.display_label}</span>
              <strong>{user.points}</strong>
            </li>
          ))}
        </ol>
      </section>
    </div>
  );
}
