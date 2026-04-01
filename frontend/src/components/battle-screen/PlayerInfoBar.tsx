import type { CSSProperties } from 'react';

import type { ThemeId } from '../../lib/themes';
import type { PlayerView, Seat } from '../../types/match';

export type TableStagePlayer = Pick<PlayerView, 'seat' | 'name' | 'melds'> &
  Partial<Omit<PlayerView, 'seat' | 'name' | 'melds'>>;

interface PlayerInfoBarProps {
  player: TableStagePlayer;
  className?: string;
  accentStyle?: CSSProperties;
}

export function PlayerInfoBar({ player, className = '', accentStyle }: PlayerInfoBarProps) {
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
      style={accentStyle}
      aria-label={`${player.name} 信息栏`}
    >
      <span className="table-stage__player-info-eyebrow">{eyebrowText}</span>
      <strong className="table-stage__player-info-name">{player.name}</strong>
      <span className="table-stage__player-info-meta">{metaText}</span>
      <span className="table-stage__player-info-detail">{detailText}</span>
    </article>
  );
}

export function buildPlayerAccentStyles(
  players: Array<Pick<TableStagePlayer, 'seat' | 'name'>>,
  themeId: ThemeId,
): Map<Seat, CSSProperties> {
  const stylesBySeat = new Map<Seat, CSSProperties>();
  const usedPaletteIndexes = new Set<number>();
  const orderedPlayers = [...players].sort((left, right) => SEAT_ORDER[left.seat] - SEAT_ORDER[right.seat]);
  const themeAdjustment = THEME_ACCENT_ADJUSTMENTS[themeId];

  for (const player of orderedPlayers) {
    const preferredPaletteIndex = Math.abs(hashString(`${player.seat}:${player.name}`)) % PLAYER_ACCENT_PALETTE.length;
    const paletteIndex = findAvailablePaletteIndex(preferredPaletteIndex, usedPaletteIndexes);
    const palette = PLAYER_ACCENT_PALETTE[paletteIndex];
    const adjustedCoreColor = mixHexColors(palette.core, themeAdjustment.tint, themeAdjustment.amount);

    usedPaletteIndexes.add(paletteIndex);
    stylesBySeat.set(player.seat, createPlayerAccentStyle(adjustedCoreColor, themeAdjustment));
  }

  return stylesBySeat;
}

const WIND_LABELS: Partial<Record<PlayerView['wind'], string>> = {
  East: '东',
  South: '南',
  West: '西',
  North: '北',
};

const PLAYER_ACCENT_PALETTE = [
  { name: '天水碧', core: '#8ba89e' },
  { name: '松花', core: '#a6b56c' },
  { name: '月白', core: '#9ebfd0' },
  { name: '藕荷', core: '#bc9fad' },
  { name: '梅子青', core: '#88afa0' },
  { name: '青瓷', core: '#7fa9a2' },
  { name: '海天霞', core: '#c78d79' },
  { name: '黛青', core: '#7a8fa1' },
] as const;

const SEAT_ORDER: Record<Seat, number> = {
  top: 0,
  left: 1,
  right: 2,
  bottom: 3,
};

const THEME_ACCENT_ADJUSTMENTS: Record<
  ThemeId,
  {
    tint: string;
    amount: number;
    strongAlpha: number;
    softAlpha: number;
    surfaceAlpha: number;
    shadowAlpha: number;
  }
> = {
  'tian-shui-bi': {
    tint: '#8ea9a1',
    amount: 0.12,
    strongAlpha: 0.68,
    softAlpha: 0.2,
    surfaceAlpha: 0.12,
    shadowAlpha: 0.24,
  },
  'qiu-xiang': {
    tint: '#97a26f',
    amount: 0.16,
    strongAlpha: 0.68,
    softAlpha: 0.2,
    surfaceAlpha: 0.12,
    shadowAlpha: 0.24,
  },
  'song-hua': {
    tint: '#a8b66a',
    amount: 0.2,
    strongAlpha: 0.7,
    softAlpha: 0.21,
    surfaceAlpha: 0.13,
    shadowAlpha: 0.25,
  },
  'yue-bai': {
    tint: '#9fb8c7',
    amount: 0.18,
    strongAlpha: 0.66,
    softAlpha: 0.18,
    surfaceAlpha: 0.11,
    shadowAlpha: 0.23,
  },
  'mei-zi-qing': {
    tint: '#8fb7a2',
    amount: 0.18,
    strongAlpha: 0.68,
    softAlpha: 0.2,
    surfaceAlpha: 0.12,
    shadowAlpha: 0.24,
  },
  'qing-ci': {
    tint: '#8eb4aa',
    amount: 0.22,
    strongAlpha: 0.68,
    softAlpha: 0.2,
    surfaceAlpha: 0.12,
    shadowAlpha: 0.24,
  },
  'ou-he': {
    tint: '#bf9eab',
    amount: 0.18,
    strongAlpha: 0.68,
    softAlpha: 0.2,
    surfaceAlpha: 0.12,
    shadowAlpha: 0.24,
  },
  'ya-qing': {
    tint: '#7e9ba5',
    amount: 0.16,
    strongAlpha: 0.66,
    softAlpha: 0.18,
    surfaceAlpha: 0.11,
    shadowAlpha: 0.23,
  },
  'dai-qing': {
    tint: '#6f8f9d',
    amount: 0.2,
    strongAlpha: 0.66,
    softAlpha: 0.18,
    surfaceAlpha: 0.11,
    shadowAlpha: 0.23,
  },
  'xuan-qing': {
    tint: '#6e8799',
    amount: 0.22,
    strongAlpha: 0.66,
    softAlpha: 0.18,
    surfaceAlpha: 0.11,
    shadowAlpha: 0.23,
  },
  'zhu-sha': {
    tint: '#c37263',
    amount: 0.22,
    strongAlpha: 0.7,
    softAlpha: 0.21,
    surfaceAlpha: 0.13,
    shadowAlpha: 0.26,
  },
  'tan-xiang-zi': {
    tint: '#b1939f',
    amount: 0.18,
    strongAlpha: 0.68,
    softAlpha: 0.2,
    surfaceAlpha: 0.12,
    shadowAlpha: 0.24,
  },
};

function createPlayerAccentStyle(
  coreColor: string,
  themeAdjustment: (typeof THEME_ACCENT_ADJUSTMENTS)[ThemeId],
): CSSProperties {
  return {
    '--table-player-accent': withAlpha(coreColor, 0.92),
    '--table-player-accent-strong': withAlpha(coreColor, themeAdjustment.strongAlpha),
    '--table-player-accent-soft': withAlpha(coreColor, themeAdjustment.softAlpha),
    '--table-player-accent-surface': withAlpha(coreColor, themeAdjustment.surfaceAlpha),
    '--table-player-accent-shadow': withAlpha(coreColor, themeAdjustment.shadowAlpha),
  } as CSSProperties;
}

function findAvailablePaletteIndex(preferredIndex: number, usedPaletteIndexes: Set<number>) {
  for (let offset = 0; offset < PLAYER_ACCENT_PALETTE.length; offset += 1) {
    const candidateIndex = (preferredIndex + offset) % PLAYER_ACCENT_PALETTE.length;

    if (!usedPaletteIndexes.has(candidateIndex)) {
      return candidateIndex;
    }
  }

  return preferredIndex % PLAYER_ACCENT_PALETTE.length;
}

function hashString(value: string) {
  let hash = 0;

  for (const char of value) {
    hash = (hash * 33 + char.charCodeAt(0)) | 0;
  }

  return hash;
}

function withAlpha(hexColor: string, alpha: number) {
  const [red, green, blue] = parseHexColor(hexColor);

  return `rgba(${red}, ${green}, ${blue}, ${alpha})`;
}

function mixHexColors(leftHexColor: string, rightHexColor: string, amount: number) {
  const [leftRed, leftGreen, leftBlue] = parseHexColor(leftHexColor);
  const [rightRed, rightGreen, rightBlue] = parseHexColor(rightHexColor);
  const mixChannel = (left: number, right: number) => Math.round(left + (right - left) * amount);

  return toHexColor(
    mixChannel(leftRed, rightRed),
    mixChannel(leftGreen, rightGreen),
    mixChannel(leftBlue, rightBlue),
  );
}

function parseHexColor(hexColor: string) {
  const normalized = hexColor.replace('#', '');
  const segments =
    normalized.length === 3
      ? normalized.split('').map((segment) => `${segment}${segment}`)
      : [normalized.slice(0, 2), normalized.slice(2, 4), normalized.slice(4, 6)];

  return segments.map((segment) => Number.parseInt(segment, 16)) as [number, number, number];
}

function toHexColor(red: number, green: number, blue: number) {
  return `#${[red, green, blue]
    .map((channel) => channel.toString(16).padStart(2, '0'))
    .join('')}`;
}
