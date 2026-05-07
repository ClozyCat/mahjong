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
  spectators: TableSidebarSpectator[];
  spectatorRequests: SpectatorRequest[];
  isOwner: boolean;
  roomPanel?: ReactNode;
  profilePanel: ReactNode;
  onToggle: () => void;
  onTabChange: (tab: TableSidebarTab) => void;
  onSelectUser: (user: PublicUser) => void;
  onApproveRequest: (requestId: number) => void;
  onRejectRequest: (requestId: number) => void;
}

const DEFAULT_TAB_ITEMS: Array<{ id: TableSidebarTab; label: string }> = [
  { id: 'players', label: '本局玩家' },
  { id: 'online', label: '在线玩家' },
  { id: 'profile', label: '玩家信息' },
  { id: 'spectators', label: '观战者' },
  { id: 'requests', label: '观战申请' },
];

export function TableSidebar({
  isOpen,
  activeTab,
  tablePlayers,
  onlineUsers,
  spectators,
  spectatorRequests,
  isOwner,
  roomPanel,
  profilePanel,
  onToggle,
  onTabChange,
  onSelectUser,
  onApproveRequest,
  onRejectRequest,
}: TableSidebarProps) {
  const tabItems = roomPanel ? [{ id: 'room' as const, label: '牌局' }, ...DEFAULT_TAB_ITEMS] : DEFAULT_TAB_ITEMS;

  return (
    <aside className={`table-sidebar ${isOpen ? 'is-open' : 'is-collapsed'}`} aria-label="Table sidebar shell">
      <button
        type="button"
        className="table-sidebar__toggle"
        aria-label={isOpen ? '收起牌桌侧边栏' : '打开牌桌侧边栏'}
        onClick={onToggle}
      >
        {isOpen ? '收起' : '侧栏'}
      </button>

      {isOpen ? (
        <div className="table-sidebar__panel" role="complementary" aria-label="Table sidebar">
          <div className="table-sidebar__tabs" role="tablist" aria-label="牌桌侧栏标签">
            {tabItems.map((tab) => (
              <button
                key={tab.id}
                type="button"
                role="tab"
                aria-selected={activeTab === tab.id}
                className={activeTab === tab.id ? 'is-active' : undefined}
                onClick={() => onTabChange(tab.id)}
              >
                {tab.label}
              </button>
            ))}
          </div>

          <div className="table-sidebar__content">
            {activeTab === 'room' ? roomPanel : null}

            {activeTab === 'players' ? (
              <ul className="table-sidebar__list">
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

            {activeTab === 'online' ? (
              <ul className="table-sidebar__list">
                {onlineUsers.length === 0 ? <li className="table-sidebar__empty">暂无在线玩家</li> : null}
                {onlineUsers.map((user) => (
                  <li key={user.user_id} className="table-sidebar__row">
                    <div>
                      <strong>{user.display_label}</strong>
                      <span>{user.points} 分</span>
                    </div>
                    <button type="button" onClick={() => onSelectUser(user)}>
                      查看资料
                    </button>
                  </li>
                ))}
              </ul>
            ) : null}

            {activeTab === 'profile' ? profilePanel : null}

            {activeTab === 'spectators' ? (
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

            {activeTab === 'requests' ? (
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
