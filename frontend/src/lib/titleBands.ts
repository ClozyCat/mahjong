export type PlayerTitle = string;

export const CROWN_TITLE = '👑';

const POINTS_PER_LEVEL = 50;

export function titleForPoints(points: number): string {
  if (points <= 0) {
    return 'Lv.0';
  }

  return `Lv.${Math.floor(points / POINTS_PER_LEVEL)}`;
}

export function titleDescriptionForTitle(title: string): string {
  if (title === CROWN_TITLE) {
    return '当前唯一最高分玩家';
  }

  return `${title} 段位`;
}

export function titleRank(title: string): number {
  if (title === CROWN_TITLE) {
    return Number.MAX_SAFE_INTEGER;
  }

  const match = /^(?:LV|Lv\.)(\d+)$/.exec(title.trim());
  return match ? Number(match[1]) : 0;
}
