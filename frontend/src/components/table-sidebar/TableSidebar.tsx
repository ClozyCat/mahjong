import type { ReactNode } from 'react';

import type { PublicUser, SpectatorRequest } from '../../types/match';

export type TableSidebarTab = 'room' | 'players' | 'online' | 'profile' | 'spectators' | 'requests';

export interface TableSidebarPlayer {
  key: string;
  seatLabel: string;
  displayLabel: string;
  score: number;
  liveDelta: number;
  points?: number | null;
  connected: boolean;
  isBotSeat?: boolean;
  isBotControlled?: boolean;
  profileUser?: PublicUser | null;
}

export interface TableSidebarSpectator {
  key: string;
  label: string;
  subtitle?: string | null;
}

interface TableSidebarProps {
  isOpen: boolean;
  activeTab: TableSidebarTab;
  tablePlayers: TableSidebarPlayer[];
  onlineUsers: PublicUser[];
  onlineUserIds?: number[];
  spectators: TableSidebarSpectator[];
  spectatorRequests: SpectatorRequest[];
  tabAlerts?: Partial<Record<TableSidebarTab, boolean>>;
  isOwner: boolean;
  roomPanel?: ReactNode;
  profilePanel: ReactNode;
  onToggle: () => void;
  onTabChange: (tab: TableSidebarTab) => void;
  onSelectUser: (user: PublicUser) => void;
  onApproveRequest: (requestId: number) => void;
  onRejectRequest: (requestId: number) => void;
}

const TAB_CONFIG: Record<TableSidebarTab, { label: string; icon: ReactNode }> = {
  room: {
    label: '牌局',
    icon: (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
        <line x1="3" y1="9" x2="21" y2="9" />
        <line x1="9" y1="21" x2="9" y2="9" />
      </svg>
    ),
  },
  players: {
    label: '本局玩家',
    icon: (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" />
        <circle cx="9" cy="7" r="4" />
        <path d="M23 21v-2a4 4 0 0 0-3-3.87" />
        <path d="M16 3.13a4 4 0 0 1 0 7.75" />
      </svg>
    ),
  },
  online: {
    label: '所有玩家',
    icon: (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <circle cx="12" cy="12" r="10" />
        <line x1="2" y1="12" x2="22" y2="12" />
        <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
      </svg>
    ),
  },
  profile: {
    label: '玩家信息',
    icon: (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
        <circle cx="12" cy="7" r="4" />
      </svg>
    ),
  },
  spectators: {
    label: '观战者',
    icon: (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
        <circle cx="12" cy="12" r="3" />
      </svg>
    ),
  },
  requests: {
    label: '观战申请',
    icon: (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M16 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" />
        <circle cx="9" cy="7" r="4" />
        <line x1="19" y1="8" x2="19" y2="14" />
        <line x1="22" y1="11" x2="16" y2="11" />
      </svg>
    ),
  },
};

export function TableSidebar({
  isOpen,
  activeTab,
  tablePlayers,
  onlineUsers,
  onlineUserIds = [],
  spectators,
  spectatorRequests,
  tabAlerts = {},
  isOwner,
  roomPanel,
  profilePanel,
  onToggle,
  onTabChange,
  onSelectUser,
  onApproveRequest,
  onRejectRequest,
}: TableSidebarProps) {
  const tabs: TableSidebarTab[] = roomPanel
    ? ['room', 'players', 'online', 'profile', 'spectators', 'requests']
    : ['players', 'online', 'profile', 'spectators', 'requests'];
  const resolvedActiveTab = tabs.includes(activeTab) ? activeTab : tabs[0];
  const hasAnyAlert = tabs.some((tabId) => tabAlerts[tabId]);
  const onlineUserIdSet = new Set(onlineUserIds);
  const allUsers = [...onlineUsers].sort((left, right) => right.points - left.points);

  return (
    <aside className={`table-sidebar ${isOpen ? 'is-open' : 'is-collapsed'}`} aria-label="Table sidebar shell">
      <button
        type="button"
        className="table-sidebar__toggle"
        aria-label={isOpen ? '收起牌桌侧边栏' : '打开牌桌侧边栏'}
        onClick={onToggle}
      >
        <span className="table-sidebar__toggle-icon">
          {isOpen ? (
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="15 18 9 12 15 6" />
            </svg>
          ) : (
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="9 18 15 12 9 6" />
            </svg>
          )}
        </span>
        {hasAnyAlert ? <span className="table-sidebar__tab-alert" aria-hidden="true">!</span> : null}
      </button>

      {isOpen ? (
        <div className="table-sidebar__panel" role="complementary" aria-label="Table sidebar">
          <div className="table-sidebar__tabs" role="tablist" aria-label="牌桌侧栏标签">
            {tabs.map((tabId) => (
              <button
                key={tabId}
                type="button"
                role="tab"
                aria-selected={resolvedActiveTab === tabId}
                title={TAB_CONFIG[tabId].label}
                className={resolvedActiveTab === tabId ? 'is-active' : undefined}
                onClick={() => onTabChange(tabId)}
              >
                <span className="table-sidebar__tab-icon">{TAB_CONFIG[tabId].icon}</span>
                {tabAlerts[tabId] ? <span className="table-sidebar__tab-alert" aria-hidden="true">!</span> : null}
              </button>
            ))}
          </div>

          <div className="table-sidebar__content">
            {resolvedActiveTab === 'room' ? roomPanel : null}

            {resolvedActiveTab === 'players' ? (
              <ul className="table-sidebar__list">
                {tablePlayers.length === 0 ? <li className="table-sidebar__empty">尚未开局</li> : null}
                {tablePlayers.map((player) => (
                  <li key={player.key} className="table-sidebar__row table-sidebar__row--stacked">
                    <div>
                      <strong>
                        {player.seatLabel} {player.displayLabel}
                      </strong>
                      <span>
                        分数 {player.score} / 局外积分 {player.points ?? '--'} / {player.connected ? '在线' : '离线'}
                      </span>
                    </div>
                    {player.profileUser ? (
                      <button type="button" onClick={() => onSelectUser(player.profileUser!)}>
                        查看资料
                      </button>
                    ) : null}
                  </li>
                ))}
              </ul>
            ) : null}

            {resolvedActiveTab === 'online' ? (
              <ul className="table-sidebar__list">
                {allUsers.length === 0 ? <li className="table-sidebar__empty">暂无玩家</li> : null}
                {allUsers.map((user) => (
                  <li key={user.user_id} className="table-sidebar__row">
                    <div>
                      <strong>
                        {user.display_label}
                        {onlineUserIdSet.has(user.user_id) ? (
                          <span className="table-sidebar__online-label">online</span>
                        ) : null}
                      </strong>
                      <span>{user.points} 分</span>
                    </div>
                    <button type="button" onClick={() => onSelectUser(user)}>
                      查看资料
                    </button>
                  </li>
                ))}
              </ul>
            ) : null}

            {resolvedActiveTab === 'profile' ? profilePanel : null}

            {resolvedActiveTab === 'spectators' ? (
              <ul className="table-sidebar__list">
                {spectators.length === 0 ? <li className="table-sidebar__empty">暂无观战者信息</li> : null}
                {spectators.map((spectator) => (
                  <li key={spectator.key} className="table-sidebar__row table-sidebar__row--stacked">
                    <strong>{spectator.label}</strong>
                    {spectator.subtitle ? <span>{spectator.subtitle}</span> : null}
                  </li>
                ))}
              </ul>
            ) : null}

            {resolvedActiveTab === 'requests' ? (
              <ul className="table-sidebar__list">
                {!isOwner ? <li className="table-sidebar__empty">仅房主可审批观战申请</li> : null}
                {isOwner && spectatorRequests.length === 0 ? <li className="table-sidebar__empty">暂无待审批申请</li> : null}
                {isOwner
                  ? spectatorRequests.map((request) => (
                      <li key={request.id} className="table-sidebar__row table-sidebar__row--stacked">
                        <div>
                          <strong>用户 #{request.requester_user_id}</strong>
                          <span>申请观战 {request.table_code}</span>
                        </div>
                        <div className="table-sidebar__actions">
                          <button type="button" onClick={() => onApproveRequest(request.id)}>
                            同意
                          </button>
                          <button type="button" onClick={() => onRejectRequest(request.id)}>
                            拒绝
                          </button>
                        </div>
                      </li>
                    ))
                  : null}
              </ul>
            ) : null}
          </div>
        </div>
      ) : null}
    </aside>
  );
}
