import type { ReactNode } from 'react';
import { createPortal } from 'react-dom';

import type { PublicUser } from '../../types/match';

export type TableSidebarTab = 'room' | 'messages' | 'online';

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
  tabAlerts?: Partial<Record<TableSidebarTab, boolean>>;
  roomPanel?: ReactNode;
  messagesPanel?: ReactNode;
  onToggle: () => void;
  onTabChange: (tab: TableSidebarTab) => void;
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
  tabAlerts = {},
  roomPanel,
  messagesPanel,
  onToggle,
  onTabChange,
}: TableSidebarProps) {
  const tabs: TableSidebarTab[] = [
    ...(roomPanel ? (['room'] as const) : []),
    ...(messagesPanel ? (['messages'] as const) : []),
    'online',
  ];
  const resolvedActiveTab = tabs.includes(activeTab) ? activeTab : tabs[0];
  const hasAnyAlert = tabs.some((tabId) => tabAlerts[tabId]);
  const onlineUserIdSet = new Set(onlineUserIds);
  const creatingTableCodeSet = new Set(creatingTableCodes);
  const allUsers = [...onlineUsers].sort((left, right) => right.points - left.points);
  const activeTableUserCounts = allUsers.reduce((counts, user) => {
    if (!user.active_table_code) {
      return counts;
    }

    counts.set(user.active_table_code, (counts.get(user.active_table_code) ?? 0) + 1);
    return counts;
  }, new Map<string, number>());

  const content = (
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

            {resolvedActiveTab === 'online' ? (
              <ul className="table-sidebar__list">
                {allUsers.length === 0 ? <li className="table-sidebar__empty">暂无玩家</li> : null}
                {allUsers.map((user) => {
                  const isSelf = currentUserId === user.user_id;
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
                      {isSelf ? <span className="table-sidebar__stat">当前账号</span> : null}
                    </li>
                  );
                })}
              </ul>
            ) : null}
          </div>
        </div>
      ) : null}
    </aside>
  );

  if (typeof document === 'undefined') return null;
  return createPortal(content, document.body);
}
