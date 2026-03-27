const NUMERAL_GLYPHS = ['一', '二', '三', '四', '五', '六', '七', '八', '九'] as const;

const SUIT_NAMES = {
  w: '万',
  m: '万',
  b: '筒',
  p: '筒',
  c: '条',
  t: '条',
} as const;

const HONOR_TILE_NAMES: Record<string, string> = {
  east: '东风',
  south: '南风',
  west: '西风',
  north: '北风',
  red: '红中',
  green: '发财',
  white: '白板',
  d1: '东风',
  d2: '南风',
  d3: '西风',
  d4: '北风',
  d5: '红中',
  d6: '发财',
  d7: '白板',
  f1: '春',
  f2: '夏',
  f3: '秋',
  f4: '冬',
  f5: '梅',
  f6: '兰',
  f7: '竹',
  f8: '菊',
} as const;

export function formatTileName(tileCode: string | null | undefined, fallback = '一张牌'): string {
  if (!tileCode) {
    return fallback;
  }

  const normalized = tileCode.trim().toLowerCase();
  const suited = normalized.match(/^([wbcmpt])([1-9])$/);

  if (suited) {
    const [, suit, rankText] = suited;
    const rank = Number(rankText);
    return `${NUMERAL_GLYPHS[rank - 1]}${SUIT_NAMES[suit as keyof typeof SUIT_NAMES]}`;
  }

  return HONOR_TILE_NAMES[normalized] ?? tileCode;
}
