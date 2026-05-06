import type { PublicUser, TableInvite, TableMultiplier } from '../../types/match';

interface SocialLobbyProps {
  currentUser: PublicUser;
  leaderboard: PublicUser[];
  onlineUserIds: number[];
  pendingInvites: TableInvite[];
  activeTableCode: string | null;
  inviteDialog: TableInvite | null;
  multiplier: TableMultiplier;
  busy: boolean;
  message?: string | null;
  onMultiplierChange: (multiplier: TableMultiplier) => void;
  onCreateTable: () => void;
  onEnterTable: () => void;
  onInvite: (userId: number) => void;
  onAcceptInvite: (invite: TableInvite) => void;
  onDismissInviteDialog: () => void;
  onLogout: () => void;
}

const MULTIPLIERS: TableMultiplier[] = [1, 2, 3];

export function SocialLobby({
  currentUser,
  leaderboard,
  onlineUserIds,
  pendingInvites,
  activeTableCode,
  inviteDialog,
  multiplier,
  busy,
  message,
  onMultiplierChange,
  onCreateTable,
  onEnterTable,
  onInvite,
  onAcceptInvite,
  onDismissInviteDialog,
  onLogout,
}: SocialLobbyProps) {
  const onlineUsers = leaderboard.filter(
    (user) => user.user_id !== currentUser.user_id && onlineUserIds.includes(user.user_id),
  );
  const topUsers = leaderboard.slice(0, 8);

  return (
    <main className="social-lobby" aria-label="Social lobby">
      <section className="social-lobby__hero">
        <div>
          <p className="social-lobby__eyebrow">Lobby</p>
          <h1>{currentUser.display_label}</h1>
          <p className="social-lobby__meta">当前积分 {currentUser.points}</p>
        </div>
        <button type="button" className="social-lobby__ghost" onClick={onLogout}>
          退出登录
        </button>
      </section>

      {message ? <p className="social-lobby__message">{message}</p> : null}

      <section className="social-lobby__grid">
        <div className="social-lobby__panel">
          <h2>创建牌局</h2>
          <div className="social-lobby__multiplier" role="group" aria-label="牌局倍数">
            {MULTIPLIERS.map((item) => (
              <button
                key={item}
                type="button"
                aria-pressed={multiplier === item}
                className={multiplier === item ? 'is-active' : undefined}
                onClick={() => onMultiplierChange(item)}
              >
                x{item}
              </button>
            ))}
          </div>
          <div className="social-lobby__actions">
            <button type="button" className="social-lobby__primary" disabled={busy} onClick={onCreateTable}>
              创建牌局
            </button>
            <button
              type="button"
              className="social-lobby__secondary"
              disabled={!activeTableCode || busy}
              onClick={onEnterTable}
            >
              进入牌桌
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
                  <span>邀请时间 {invite.created_at}</span>
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

      {inviteDialog ? (
        <div className="social-lobby__dialog-backdrop" role="presentation">
          <div className="social-lobby__dialog" role="dialog" aria-modal="true" aria-label="牌局邀请">
            <h2>收到牌局邀请</h2>
            <p>牌桌 {inviteDialog.table_code} 邀请你加入。</p>
            <div className="social-lobby__actions">
              <button type="button" className="social-lobby__primary" onClick={() => onAcceptInvite(inviteDialog)}>
                接受邀请
              </button>
              <button type="button" className="social-lobby__secondary" onClick={onDismissInviteDialog}>
                稍后处理
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </main>
  );
}
