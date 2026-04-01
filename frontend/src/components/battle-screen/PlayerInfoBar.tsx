import type { CSSProperties } from 'react';

import type { PlayerView, Seat } from '../../types/match';

export type TableStagePlayer = Pick<PlayerView, 'seat' | 'name' | 'melds'> &
  Partial<Omit<PlayerView, 'seat' | 'name' | 'melds'>>;

interface PlayerInfoBarProps {
  player: TableStagePlayer;
  className?: string;
}

export function PlayerInfoBar({ player, className = '' }: PlayerInfoBarProps) {
  const windLabel = player.wind ? (WIND_LABELS[player.wind] ?? player.wind) : null;
  const presenceLabel = player.isBotControlled ? '离线' : player.connected === false ? '离线' : '在线';
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

  return (
    <article
      className={`table-stage__player-info ${player.isActive ? 'table-stage__player-info--active' : ''} ${
        player.isLocal ? 'table-stage__player-info--local' : ''
      } ${className}`.trim()}
      style={buildPlayerAccentStyle(player)}
      aria-label={`${player.name} 信息栏`}
    >
      <span className="table-stage__player-info-eyebrow">{eyebrowText}</span>
      <strong className="table-stage__player-info-name">{player.name}</strong>
      <span className="table-stage__player-info-meta">{metaText}</span>
      <span className="table-stage__player-info-detail">{detailText}</span>
    </article>
  );
}

export function buildPlayerAccentStyle(player: Pick<TableStagePlayer, 'seat' | 'name'>): CSSProperties {
  const seed = `${player.seat}:${player.name}`;
  let hash = 0;

  for (const char of seed) {
    hash = (hash * 33 + char.charCodeAt(0)) | 0;
  }

  const hue = (Math.abs(hash) + SEAT_ACCENT_OFFSET[player.seat]) % 360;

  return {
    '--table-player-accent': `hsl(${hue}, 60%, 70%)`,
    '--table-player-accent-strong': `hsla(${hue}, 64%, 62%, 0.68)`,
    '--table-player-accent-soft': `hsla(${hue}, 64%, 62%, 0.2)`,
    '--table-player-accent-surface': `hsla(${hue}, 64%, 62%, 0.12)`,
    '--table-player-accent-shadow': `hsla(${hue}, 70%, 58%, 0.24)`,
  } as CSSProperties;
}

const WIND_LABELS: Partial<Record<PlayerView['wind'], string>> = {
  East: '东',
  South: '南',
  West: '西',
  North: '北',
};

const SEAT_ACCENT_OFFSET: Record<Seat, number> = {
  bottom: 29,
  left: 107,
  top: 191,
  right: 277,
};
