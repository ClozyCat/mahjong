import type {
  GameSummary,
  PublicUser,
  UserFanStat,
  UserGamePlayerSummary,
} from '../../types/match';

interface ProfilePerformance {
  roundCount: number;
  winCount: number;
  selfDrawWinCount: number;
  discardWinCount: number;
  dealInCount: number;
  totalScoreDelta: number;
  averageCumulativeScore: number;
  highScoreRoundCount: number;
}

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
  const bio = generatePublicBio(recentGames, fanStats);
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

export function generatePublicBio(recentGames: GameSummary[], fanStats: UserFanStat[]) {
  const performance = aggregateProfilePerformance(recentGames);
  if (!performance || performance.roundCount === 0) {
    return '暂无公开简介';
  }

  const winRate = performance.winCount / performance.roundCount;
  const dealInRate = performance.dealInCount / performance.roundCount;
  const highScoreRate = performance.highScoreRoundCount / performance.roundCount;
  const topFan = fanStats[0];

  if (performance.dealInCount >= 3 && dealInRate >= 0.3) return '放铳王';
  if (winRate >= 0.4 && highScoreRate >= 0.5) return '雀圣';
  if (
    performance.selfDrawWinCount >= 2 &&
    performance.selfDrawWinCount >= performance.discardWinCount
  ) {
    return '自摸之星';
  }
  if (performance.discardWinCount >= 2) return '荣和猎手';
  if (topFan && topFan.count >= 3) return `${topFan.fan_label}专家`;
  if (performance.dealInCount === 0 && performance.roundCount >= 4) return '铁壁防守';
  if (performance.totalScoreDelta > 0 && winRate < 0.25) return '稳健派';
  if (performance.totalScoreDelta < 0 && dealInRate < 0.2) return '逆风修行中';
  if (
    performance.roundCount >= 8 &&
    Math.abs(performance.totalScoreDelta) <= performance.roundCount * 5
  ) {
    return '均衡型选手';
  }
  return '牌桌新星';
}

function aggregateProfilePerformance(recentGames: GameSummary[]): ProfilePerformance | null {
  const summaries = recentGames
    .map((game) => game.player_summary)
    .filter((summary): summary is UserGamePlayerSummary => Boolean(summary));
  if (summaries.length === 0) return null;

  const totals = summaries.reduce(
    (current, summary) => addProfilePerformance(current, summary),
    emptyProfilePerformance(),
  );
  totals.averageCumulativeScore = weightedAverageCumulativeScore(summaries);
  return totals;
}

function addProfilePerformance(
  current: ProfilePerformance,
  summary: UserGamePlayerSummary,
): ProfilePerformance {
  return {
    roundCount: current.roundCount + summary.round_count,
    winCount: current.winCount + summary.win_count,
    selfDrawWinCount: current.selfDrawWinCount + summary.self_draw_win_count,
    discardWinCount: current.discardWinCount + summary.discard_win_count,
    dealInCount: current.dealInCount + summary.deal_in_count,
    totalScoreDelta: current.totalScoreDelta + summary.total_score_delta,
    averageCumulativeScore: current.averageCumulativeScore,
    highScoreRoundCount: current.highScoreRoundCount + summary.high_score_round_count,
  };
}

function weightedAverageCumulativeScore(summaries: UserGamePlayerSummary[]) {
  const roundCount = summaries.reduce((total, summary) => total + summary.round_count, 0);
  if (roundCount === 0) return 0;

  const weightedTotal = summaries.reduce(
    (total, summary) => total + summary.average_cumulative_score * summary.round_count,
    0,
  );
  return Math.round(weightedTotal / roundCount);
}

function emptyProfilePerformance(): ProfilePerformance {
  return {
    roundCount: 0,
    winCount: 0,
    selfDrawWinCount: 0,
    discardWinCount: 0,
    dealInCount: 0,
    totalScoreDelta: 0,
    averageCumulativeScore: 0,
    highScoreRoundCount: 0,
  };
}
