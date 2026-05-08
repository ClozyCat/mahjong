import type { PublicUser, SpectatorRequest, TableInvite } from '../../types/match';
import { formatBeijingDateTime } from '../../lib/dateTime';

export type SentInviteStatus = 'pending' | 'rejected';

interface SocialSidebarPanelProps {
  currentUser: PublicUser;
  leaderboard: PublicUser[];
  onlineUserIds: number[];
  activeTableCode: string | null;
  busy: boolean;
  isCreateTableDisabled: boolean;
  canInvitePlayers: boolean;
  inviteStatusesByUserId: Record<number, SentInviteStatus>;
  message?: string | null;
  onCreateTable: () => void;
  onInvite: (userId: number) => void;
  onLogout: () => void;
}

interface SocialSidebarMessagesPanelProps {
  pendingInvites: TableInvite[];
  spectatorRequests: SpectatorRequest[];
  isOwner: boolean;
  inviteCreatorLabelsByUserId?: Record<number, string>;
  spectatorRequesterLabelsByUserId?: Record<number, string>;
  message?: string | null;
  onAcceptInvite: (invite: TableInvite) => void;
  onRejectInvite: (invite: TableInvite) => void;
  onApproveSpectatorRequest: (requestId: number) => void;
  onRejectSpectatorRequest: (requestId: number) => void;
}

function getInviteCreatorLabel(invite: TableInvite, labelsByUserId: Record<number, string>) {
  return labelsByUserId[invite.inviter_user_id] ?? `用户 #${invite.inviter_user_id}`;
}

function getInviteCopy(invite: TableInvite, labelsByUserId: Record<number, string>) {
  return `${getInviteCreatorLabel(invite, labelsByUserId)}创建的牌桌${invite.table_code}邀请你加入。`;
}

function getSpectatorRequesterLabel(request: SpectatorRequest, labelsByUserId: Record<number, string>) {
  return labelsByUserId[request.requester_user_id] ?? `用户 #${request.requester_user_id}`;
}

export function SocialSidebarPanel({
  currentUser,
  leaderboard,
  onlineUserIds,
  activeTableCode,
  busy,
  isCreateTableDisabled,
  canInvitePlayers,
  inviteStatusesByUserId,
  message,
  onCreateTable,
  onInvite,
  onLogout,
}: SocialSidebarPanelProps) {
  const onlineUsers = leaderboard.filter(
    (user) => user.user_id !== currentUser.user_id && onlineUserIds.includes(user.user_id),
  );

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

      <section className="social-sidebar__section">
        <h3>牌局管理</h3>
        <div className="social-sidebar__actions">
          {activeTableCode ? (
            <div className="social-sidebar__status-box">
              <span className="social-sidebar__status-label">当前牌桌</span>
              <strong className="social-sidebar__status-value">{activeTableCode}</strong>
            </div>
          ) : (
            <button
              type="button"
              className="social-sidebar__primary"
              disabled={isCreateTableDisabled}
              onClick={onCreateTable}
            >
              创建新牌局
            </button>
          )}
        </div>
        {!activeTableCode && <p>创建后可在侧栏邀请在线玩家。</p>}
      </section>

      <section className="social-sidebar__section">
        <h3>在线玩家</h3>
        <ul className="table-sidebar__list">
          {onlineUsers.length === 0 ? <li className="table-sidebar__empty">暂无在线玩家</li> : null}
          {onlineUsers.map((user) => {
            const inviteStatus = inviteStatusesByUserId[user.user_id];
            const inviteLabel =
              inviteStatus === 'pending' ? '已邀请' : inviteStatus === 'rejected' ? '已被拒绝' : '邀请';
            const isInviteDisabled = busy || !canInvitePlayers || inviteStatus === 'pending';

            return (
              <li key={user.user_id} className="table-sidebar__row">
                <div className="table-sidebar__row-info">
                  <strong className="table-sidebar__row-name">{user.display_label}</strong>
                  <span className="table-sidebar__stat">{user.points} <small>积分</small></span>
                </div>
                <button type="button" disabled={isInviteDisabled} onClick={() => onInvite(user.user_id)}>
                  {inviteLabel}
                </button>
              </li>
            );
          })}
        </ul>
      </section>
    </div>
  );
}

export function SocialSidebarMessagesPanel({
  pendingInvites,
  spectatorRequests,
  isOwner,
  inviteCreatorLabelsByUserId = {},
  spectatorRequesterLabelsByUserId = {},
  message,
  onAcceptInvite,
  onRejectInvite,
  onApproveSpectatorRequest,
  onRejectSpectatorRequest,
}: SocialSidebarMessagesPanelProps) {
  return (
    <div className="social-sidebar" role="region" aria-label="消息中心">
      {message ? <p className="social-sidebar__message">{message}</p> : null}

      <section className="social-sidebar__section">
        <h3>待处理邀请</h3>
        <ul className="table-sidebar__list">
          {pendingInvites.length === 0 ? <li className="table-sidebar__empty">暂无待处理邀请</li> : null}
          {pendingInvites.map((invite) => (
            <li key={invite.id} className="table-sidebar__row table-sidebar__row--stacked">
              <div className="table-sidebar__row-info">
                <strong className="table-sidebar__row-name">{invite.table_code}</strong>
                <span className="table-sidebar__stat">{getInviteCopy(invite, inviteCreatorLabelsByUserId)}</span>
                <span className="table-sidebar__stat">邀请时间 {formatBeijingDateTime(invite.created_at)}</span>
              </div>
              <div className="table-sidebar__actions">
                <button type="button" onClick={() => onAcceptInvite(invite)}>
                  接受
                </button>
                <button type="button" onClick={() => onRejectInvite(invite)}>
                  拒绝
                </button>
              </div>
            </li>
          ))}
        </ul>
      </section>

      <section className="social-sidebar__section">
        <h3>待处理观战申请</h3>
        <ul className="table-sidebar__list">
          {!isOwner ? <li className="table-sidebar__empty">仅房主可审批观战申请</li> : null}
          {isOwner && spectatorRequests.length === 0 ? <li className="table-sidebar__empty">暂无待审批申请</li> : null}
          {isOwner
            ? spectatorRequests.map((request) => (
                <li key={request.id} className="table-sidebar__row table-sidebar__row--stacked">
                  <div className="table-sidebar__row-info">
                    <strong className="table-sidebar__row-name">
                      {getSpectatorRequesterLabel(request, spectatorRequesterLabelsByUserId)}
                    </strong>
                    <span className="table-sidebar__stat">申请观战 {request.table_code}</span>
                  </div>
                  <div className="table-sidebar__actions">
                    <button type="button" onClick={() => onApproveSpectatorRequest(request.id)}>
                      同意
                    </button>
                    <button type="button" onClick={() => onRejectSpectatorRequest(request.id)}>
                      拒绝
                    </button>
                  </div>
                </li>
              ))
            : null}
        </ul>
      </section>
    </div>
  );
}
