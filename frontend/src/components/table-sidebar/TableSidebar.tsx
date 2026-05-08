import type { ReactNode } from 'react';

import type { PublicUser } from '../../types/match';

export type TableSidebarTab = 'room' | 'messages' | 'players' | 'online' | 'profile' | 'spectators';

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

type AllPlayerStatusTone = 'online' | 'offline' | 'creating' | 'playing-human' | 'playing-bot';

interface AllPlayerStatus {
  label: string;
  tone: AllPlayerStatusTone;
}

interface TableSidebarProps {
  isOpen: boolean;
  activeTab: TableSidebarTab;
  tablePlayers: TableSidebarPlayer[];
  onlineUsers: PublicUser[];
  onlineUserIds?: number[];
  creatingTableCodes?: string[];
  currentUserId?: number | null;
  requestedWatchTableCodes?: Iterable<string>;
  spectators: TableSidebarSpectator[];
  tabAlerts?: Partial<Record<TableSidebarTab, boolean>>;
  roomPanel?: ReactNode;
  messagesPanel?: ReactNode;
  profilePanel: ReactNode;
  onToggle: () => void;
  onTabChange: (tab: TableSidebarTab) => void;
  onSelectUser: (user: PublicUser) => void;
  onWatchUser?: (user: PublicUser) => void;
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
  messages: {
    label: '消息',
    icon: (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M21 15a4 4 0 0 1-4 4H8l-5 3V7a4 4 0 0 1 4-4h10a4 4 0 0 1 4 4z" />
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
};

function getAllPlayerStatus(
  user: PublicUser,
  onlineUserIdSet: Set<number>,
  activeTableUserCounts: Map<string, number>,
  creatingTableCodeSet: Set<string>,
): AllPlayerStatus {
  if (!user.active_table_code) {
    if (!onlineUserIdSet.has(user.user_id)) {
      return { label: '离线', tone: 'offline' };
    }

    return { label: '在线', tone: 'online' };
  }

  if (creatingTableCodeSet.has(user.active_table_code)) {
    return { label: '创建牌局中', tone: 'creating' };
  }

  if (user.active_table_phase === 'waiting') {
    return { label: '创建牌局中', tone: 'creating' };
  }

  if (user.active_table_phase !== 'playing') {
    if (!onlineUserIdSet.has(user.user_id)) {
      return { label: '离线', tone: 'offline' };
    }

    return { label: '在线', tone: 'online' };
  }

  if (!onlineUserIdSet.has(user.user_id)) {
    return { label: '离线', tone: 'offline' };
  }

  const activeUserCount = activeTableUserCounts.get(user.active_table_code) ?? 0;

  return activeUserCount > 1
    ? { label: '与玩家对局中', tone: 'playing-human' }
    : { label: '与BOT对局中', tone: 'playing-bot' };
}

export function TableSidebar({
  isOpen,
  activeTab,
  tablePlayers,
  onlineUsers,
  onlineUserIds = [],
  creatingTableCodes = [],
  currentUserId = null,
  requestedWatchTableCodes = [],
  spectators,
  tabAlerts = {},
  roomPanel,
  messagesPanel,
  profilePanel,
  onToggle,
  onTabChange,
  onSelectUser,
  onWatchUser,
}: TableSidebarProps) {
  const tabs: TableSidebarTab[] = [
    ...(roomPanel ? (['room'] as const) : []),
    ...(messagesPanel ? (['messages'] as const) : []),
    'players',
    'online',
    'profile',
    'spectators',
  ];
  const resolvedActiveTab = tabs.includes(activeTab) ? activeTab : tabs[0];
  const hasAnyAlert = tabs.some((tabId) => tabAlerts[tabId]);
  const onlineUserIdSet = new Set(onlineUserIds);
  const creatingTableCodeSet = new Set(creatingTableCodes);
  const requestedWatchTableCodeSet = new Set(requestedWatchTableCodes);
  const allUsers = [...onlineUsers].sort((left, right) => right.points - left.points);
  const activeTableUserCounts = allUsers.reduce((counts, user) => {
    if (!user.active_table_code) {
      return counts;
    }

    counts.set(user.active_table_code, (counts.get(user.active_table_code) ?? 0) + 1);
    return counts;
  }, new Map<string, number>());

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
                aria-label={TAB_CONFIG[tabId].label}
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

            {resolvedActiveTab === 'messages' ? messagesPanel : null}

            {resolvedActiveTab === 'players' ? (
              <ul className="table-sidebar__list">
                {tablePlayers.length === 0 ? <li className="table-sidebar__empty">尚未开局</li> : null}
                {tablePlayers.map((player) => (
                  <li key={player.key} className="table-sidebar__row table-sidebar__row--stacked">
                    <div className="table-sidebar__row-info">
                      <strong className="table-sidebar__row-name">
                        {player.seatLabel} {player.displayLabel}
                      </strong>
                      <div className="table-sidebar__row-stats">
                        <span className="table-sidebar__stat">
                          <small>分数</small> {player.score}
                        </span>
                        <span className="table-sidebar__stat">
                          <small>积分</small> {player.points ?? '--'}
                        </span>
                        <span className={`table-sidebar__connection ${player.connected ? 'is-online' : 'is-offline'}`}>
                          {player.connected ? '在线' : '离线'}
                        </span>
                      </div>
                    </div>
                    {player.profileUser ? (
                      <button type="button" onClick={() => onSelectUser(player.profileUser!)}>
                        查看信息
                      </button>
                    ) : null}
                  </li>
                ))}
              </ul>
            ) : null}

            {resolvedActiveTab === 'online' ? (
              <ul className="table-sidebar__list">
                {allUsers.length === 0 ? <li className="table-sidebar__empty">暂无玩家</li> : null}
                {allUsers.map((user) => {
                  const isSelf = currentUserId === user.user_id;
                  const isWaitingTable = Boolean(
                    user.active_table_phase === 'waiting' ||
                      (user.active_table_code && creatingTableCodeSet.has(user.active_table_code)),
                  );
                  const isWatchableTable = user.active_table_phase === 'playing';
                  const hasRequestedWatch = Boolean(
                    user.active_table_code && requestedWatchTableCodeSet.has(user.active_table_code),
                  );
                  const canWatch = Boolean(
                    user.active_table_code && !isSelf && onWatchUser && !hasRequestedWatch && isWatchableTable,
                  );
                  const playerStatus = getAllPlayerStatus(
                    user,
                    onlineUserIdSet,
                    activeTableUserCounts,
                    creatingTableCodeSet,
                  );

                  return (
                    <li key={user.user_id} className="table-sidebar__row">
                      <div className="table-sidebar__row-info">
                        <strong className="table-sidebar__row-name">
                          {user.display_label}
                          <span className={`table-sidebar__player-status table-sidebar__player-status--${playerStatus.tone}`}>
                            {playerStatus.label}
                          </span>
                        </strong>
                        <span className="table-sidebar__stat">{user.points} <small>积分</small></span>
                      </div>
                      <div className="table-sidebar__row-actions">
                        <button type="button" onClick={() => onSelectUser(user)}>
                          查看信息
                        </button>
                        <button
                          type="button"
                          disabled={!canWatch}
                          title={
                            hasRequestedWatch
                              ? `已申请观战 ${user.active_table_code}`
                              : canWatch
                              ? `申请观战 ${user.active_table_code}`
                              : isWaitingTable
                                ? '牌局尚未开始，暂不能观战'
                              : isSelf
                                ? '不能观战自己的牌局'
                              : user.active_table_code
                                ? '该玩家当前没有可观战的进行中牌局'
                                : '该玩家当前不在牌局中'
                          }
                          onClick={() => onWatchUser?.(user)}
                        >
                          {hasRequestedWatch ? '已申请' : '观战'}
                        </button>
                      </div>
                    </li>
                  );
                })}
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
          </div>
        </div>
      ) : null}
    </aside>
  );
}
