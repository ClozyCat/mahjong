import type { GameSummary, PublicUser, UserFanStat } from '../../types/match';

interface UserProfilePanelProps {
  user: PublicUser | null;
  fallbackName?: string | null;
  fanStats: UserFanStat[];
  recentGames: GameSummary[];
  loading?: boolean;
  message?: string | null;
}

export function UserProfilePanel({
  user,
  fallbackName = null,
  fanStats,
  recentGames,
  loading = false,
  message = null,
}: UserProfilePanelProps) {
  if (!user && !fallbackName) {
    return <p className="user-profile-panel__empty">请选择一名玩家查看公开资料</p>;
  }

  const heading = user?.display_label ?? fallbackName ?? '未知玩家';
  const bio = user?.bio?.trim() ? user.bio.trim() : '暂无公开简介';
  const visibleFans = fanStats.slice(0, 8);
  const visibleGames = recentGames.slice(0, 5);

  return (
    <section className="user-profile-panel" aria-label="User profile panel">
      <header className="user-profile-panel__header">
        <div>
          <h3>{heading}</h3>
          {user ? <p>{user.points} 分</p> : <p>未关联公开账号</p>}
        </div>
      </header>

      <p className="user-profile-panel__bio">{bio}</p>

      {loading ? <p className="user-profile-panel__status">正在加载资料...</p> : null}
      {!loading && message ? <p className="user-profile-panel__status">{message}</p> : null}

      <section className="user-profile-panel__section">
        <h4>番种统计</h4>
        <ul className="user-profile-panel__list">
          {visibleFans.length === 0 ? <li className="user-profile-panel__empty">暂无番种记录</li> : null}
          {visibleFans.map((fan) => (
            <li key={`${fan.user_id}:${fan.fan_key}`} className="user-profile-panel__row">
              <span>{fan.fan_label}</span>
              <strong>{fan.count}</strong>
            </li>
          ))}
        </ul>
      </section>

      <section className="user-profile-panel__section">
        <h4>最近牌局</h4>
        <ul className="user-profile-panel__list">
          {visibleGames.length === 0 ? <li className="user-profile-panel__empty">暂无牌局记录</li> : null}
          {visibleGames.map((game) => (
            <li key={game.game_id} className="user-profile-panel__row user-profile-panel__row--stacked">
              <strong>{game.table_code}</strong>
              <span>{game.round_count} 局</span>
            </li>
          ))}
        </ul>
      </section>
    </section>
  );
}
