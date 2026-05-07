import { useEffect, useState } from 'react';

import type {
  GameSummary,
  PublicUser,
  UserFanStat,
  UserGamePlayerSummary,
} from '../../types/match';
import { getFanLabel } from '../battle-screen/fanGuide';

const FAN_STATS_PAGE_SIZE = 5;
const HISTORY_GAMES_PAGE_SIZE = 3;

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
  showCurrentUserAction?: boolean;
  onShowCurrentUser?: () => void;
}

export function UserProfilePanel({
  user,
  fallbackName = null,
  fanStats,
  recentGames,
  loading = false,
  message = null,
  showCurrentUserAction = false,
  onShowCurrentUser,
}: UserProfilePanelProps) {
  const [fanPage, setFanPage] = useState(0);
  const [gamePage, setGamePage] = useState(0);
  const fanPageCount = getPageCount(fanStats.length, FAN_STATS_PAGE_SIZE);
  const gamePageCount = getPageCount(recentGames.length, HISTORY_GAMES_PAGE_SIZE);
  const safeFanPage = Math.min(fanPage, fanPageCount - 1);
  const safeGamePage = Math.min(gamePage, gamePageCount - 1);

  useEffect(() => {
    setFanPage(0);
    setGamePage(0);
  }, [user?.user_id, fallbackName]);

  useEffect(() => {
    setFanPage((currentPage) => Math.min(currentPage, fanPageCount - 1));
  }, [fanPageCount]);

  useEffect(() => {
    setGamePage((currentPage) => Math.min(currentPage, gamePageCount - 1));
  }, [gamePageCount, safeGamePage]);

  if (!user && !fallbackName) {
    return <p className="user-profile-panel__empty">请选择一名玩家查看公开资料</p>;
  }

  const heading = user?.display_label ?? fallbackName ?? '未知玩家';
  const bio = generatePublicBio(recentGames, fanStats);
  const visibleFans = paginateItems(fanStats, safeFanPage, FAN_STATS_PAGE_SIZE);
  const visibleGames = paginateItems(recentGames, safeGamePage, HISTORY_GAMES_PAGE_SIZE);

  return (
    <section className="user-profile-panel" aria-label="User profile panel">
      <header className="user-profile-panel__header">
        <div>
          <h3>{heading}</h3>
          {user ? <p>{user.points} 分</p> : <p>未关联公开账号</p>}
        </div>
        {showCurrentUserAction ? (
          <button
            type="button"
            className="user-profile-panel__current-user-button"
            onClick={onShowCurrentUser}
          >
            回到我的信息
          </button>
        ) : null}
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
              <span>{formatFanStatLabel(fan)}</span>
              <strong>{fan.count}</strong>
            </li>
          ))}
        </ul>
        <PaginationControls
          ariaLabel="番种统计分页"
          page={safeFanPage}
          pageCount={fanPageCount}
          onPageChange={setFanPage}
        />
      </section>

      <section className="user-profile-panel__section">
        <h4>历史牌局</h4>
        <ul className="user-profile-panel__list">
          {visibleGames.length === 0 ? <li className="user-profile-panel__empty">暂无牌局记录</li> : null}
          {visibleGames.map((game) => (
            <li
              key={game.game_id}
              className="user-profile-panel__row user-profile-panel__row--stacked user-profile-panel__history-row"
            >
              <div className="user-profile-panel__history-main">
                <strong>{formatGameTime(game)}</strong>
                <span>{formatRoundCount(game)}</span>
              </div>
              <div className="user-profile-panel__history-meta">
                <strong className={scoreDeltaClassName(game.player_summary?.total_score_delta ?? 0)}>
                  {formatScoreDelta(game.player_summary?.total_score_delta ?? 0)}
                </strong>
                <span>{formatOpponentNames(game.opponent_names ?? [])}</span>
              </div>
            </li>
          ))}
        </ul>
        <PaginationControls
          ariaLabel="历史牌局分页"
          page={safeGamePage}
          pageCount={gamePageCount}
          onPageChange={setGamePage}
        />
      </section>
    </section>
  );
}

function PaginationControls({
  ariaLabel,
  page,
  pageCount,
  onPageChange,
}: {
  ariaLabel: string;
  page: number;
  pageCount: number;
  onPageChange: (page: number) => void;
}) {
  if (pageCount <= 1) return null;

  return (
    <nav className="user-profile-panel__pagination" aria-label={ariaLabel}>
      <button
        type="button"
        onClick={() => onPageChange(Math.max(0, page - 1))}
        disabled={page === 0}
        aria-label={`${ariaLabel}上一页`}
      >
        上一页
      </button>
      <span>
        {page + 1} / {pageCount}
      </span>
      <button
        type="button"
        onClick={() => onPageChange(Math.min(pageCount - 1, page + 1))}
        disabled={page >= pageCount - 1}
        aria-label={`${ariaLabel}下一页`}
      >
        下一页
      </button>
    </nav>
  );
}

function formatScoreDelta(delta: number) {
  return delta > 0 ? `+${delta}` : String(delta);
}

function scoreDeltaClassName(delta: number) {
  if (delta > 0) {
    return 'user-profile-panel__score-delta user-profile-panel__score-delta--positive';
  }
  if (delta < 0) {
    return 'user-profile-panel__score-delta user-profile-panel__score-delta--negative';
  }
  return 'user-profile-panel__score-delta';
}

function formatGameTime(game: GameSummary) {
  const rawTime = game.ended_at ?? game.started_at;
  const date = new Date(rawTime);
  if (Number.isNaN(date.getTime())) return rawTime;
  return new Intl.DateTimeFormat('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  }).format(date);
}

function formatRoundCount(game: GameSummary) {
  const playerRoundCount = game.player_summary?.round_count;
  const totalRoundCount = game.round_count;
  if (typeof playerRoundCount === 'number' && playerRoundCount !== totalRoundCount) {
    return `总 ${totalRoundCount} 局（参与 ${playerRoundCount} 局）`;
  }
  return `总 ${totalRoundCount} 局`;
}

function formatOpponentNames(names: string[]) {
  return names.length > 0 ? names.slice(0, 3).join('、') : '暂无对手';
}

function formatFanStatLabel(fan: UserFanStat) {
  const mappedLabel = getFanLabel(fan.fan_key);
  return mappedLabel === fan.fan_key ? fan.fan_label || fan.fan_key : mappedLabel;
}

function getPageCount(itemCount: number, pageSize: number) {
  return Math.max(1, Math.ceil(itemCount / pageSize));
}

function paginateItems<T>(items: T[], page: number, pageSize: number) {
  const start = page * pageSize;
  return items.slice(start, start + pageSize);
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
  if (topFan && topFan.count >= 3) return `${formatFanStatLabel(topFan)}专家`;
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
