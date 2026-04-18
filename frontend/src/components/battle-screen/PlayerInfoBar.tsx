import { useEffect, useState } from 'react';
import type { PlayerView } from '../../types/match';

export type TableStagePlayer = Pick<PlayerView, 'seat' | 'name' | 'melds'> &
  Partial<Omit<PlayerView, 'seat' | 'name' | 'melds'>>;

interface PlayerInfoBarProps {
  player: TableStagePlayer;
  deadlineAt?: string | null;
  className?: string;
}

export function PlayerInfoBar({
  player,
  deadlineAt = null,
  className = '',
}: PlayerInfoBarProps) {
  const [percent, setPercent] = useState(1);
  const windLabel = player.wind ? (WIND_LABELS[player.wind] ?? player.wind) : null;
  const presenceLabel =
    player.seatType === 'bot'
      ? 'BOT'
      : player.connected === false
          ? '离线'
          : '在线';
  const eyebrowText = [windLabel, player.isDealer ? '庄家' : null, presenceLabel].filter(Boolean).join(' · ');
  const metaText = [
    typeof player.score === 'number' ? player.score.toLocaleString() : null,
    player.statusText ?? '待命',
  ]
    .filter(Boolean)
    .join(' · ');
  const detailText = `手牌 ${typeof player.concealedCount === 'number' ? player.concealedCount : '--'} · 花 ${
    typeof player.flowerCount === 'number' ? player.flowerCount : '--'
  }`;

  useEffect(() => {
    if (!deadlineAt || !player.isActive) {
      setPercent(1);
      return;
    }

    const start = Date.now();
    const end = new Date(deadlineAt).getTime();
    const total = end - start;

    if (total <= 0) {
      setPercent(0);
      return;
    }

    const update = () => {
      const now = Date.now();
      const remaining = end - now;
      const nextPercent = Math.max(0, remaining / total);
      setPercent(nextPercent);

      if (nextPercent > 0) {
        requestAnimationFrame(update);
      }
    };

    const frameId = requestAnimationFrame(update);
    return () => cancelAnimationFrame(frameId);
  }, [deadlineAt, player.isActive]);

  const haloRadius = 24;
  const haloCircumference = 2 * Math.PI * haloRadius;
  const dashOffset = haloCircumference * (1 - percent);

  return (
    <article
      className={`table-stage__player-info ${player.isActive ? 'table-stage__player-info--active' : ''} ${
        player.isLocal ? 'table-stage__player-info--local' : ''
      } ${className}`.trim()}
      aria-label={`${player.name} 信息栏`}
    >
      {player.isActive && deadlineAt && (
        <div className="table-stage__player-halo">
          <svg viewBox="0 0 54 54">
            <circle
              cx="27"
              cy="27"
              r={haloRadius}
              stroke="currentColor"
              strokeWidth="1.6"
              fill="none"
              strokeLinecap="round"
              style={{
                strokeDasharray: haloCircumference,
                strokeDashoffset: dashOffset,
                transform: 'rotate(-90deg)',
                transformOrigin: 'center',
                transition: 'stroke-dashoffset 0.1s linear, color 0.3s ease',
              }}
            />
          </svg>
        </div>
      )}
      <span className="table-stage__player-info-eyebrow">{eyebrowText}</span>
      <strong className="table-stage__player-info-name">{player.name}</strong>
      <span className="table-stage__player-info-meta">{metaText}</span>
      <span className="table-stage__player-info-detail">{detailText}</span>
    </article>
  );
}

const WIND_LABELS: Partial<Record<PlayerView['wind'], string>> = {
  East: '东',
  South: '南',
  West: '西',
  North: '北',
};
