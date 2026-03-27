const SUIT_LABELS = {
  wan: '万',
  tong: '筒',
  suo: '索',
} as const;

const NUMERAL_GLYPHS = ['一', '二', '三', '四', '五', '六', '七', '八', '九'] as const;

const SUIT_METADATA = {
  w: { suit: 'wan', accent: 'crimson' },
  b: { suit: 'tong', accent: 'ocean' },
  c: { suit: 'suo', accent: 'jade' },
} as const;

const HONOR_LABELS: Record<string, { label: string; accent: 'ink' | 'crimson' | 'jade' }> = {
  east: { label: '东', accent: 'ink' },
  south: { label: '南', accent: 'ink' },
  west: { label: '西', accent: 'ink' },
  north: { label: '北', accent: 'ink' },
  red: { label: '中', accent: 'crimson' },
  green: { label: '发', accent: 'jade' },
  white: { label: '白', accent: 'ink' },
  d1: { label: '东', accent: 'ink' },
  d2: { label: '南', accent: 'ink' },
  d3: { label: '西', accent: 'ink' },
  d4: { label: '北', accent: 'ink' },
  d5: { label: '中', accent: 'crimson' },
  d6: { label: '发', accent: 'jade' },
  d7: { label: '白', accent: 'ink' },
} as const;

export type TileFace =
  | {
      kind: 'suited';
      code: string;
      suit: 'wan' | 'tong' | 'suo';
      suitLabel: (typeof SUIT_LABELS)[keyof typeof SUIT_LABELS];
      rank: number;
      glyph: (typeof NUMERAL_GLYPHS)[number];
      accent: 'crimson' | 'ocean' | 'jade';
    }
  | {
      kind: 'honor';
      code: string;
      label: string;
      accent: 'ink' | 'crimson' | 'jade';
    }
  | {
      kind: 'fallback';
      code: string;
      label: string;
      accent: 'ink';
    };

export function getTileFace(code: string): TileFace {
  const normalized = code.trim().toLowerCase();
  const suitedMatch = normalized.match(/^([wbc])([1-9])$/);

  if (suitedMatch) {
    const [, prefix, rankText] = suitedMatch;
    const rank = Number(rankText);
    const suitMeta = SUIT_METADATA[prefix as keyof typeof SUIT_METADATA];

    return {
      kind: 'suited',
      code,
      suit: suitMeta.suit,
      suitLabel: SUIT_LABELS[suitMeta.suit],
      rank,
      glyph: NUMERAL_GLYPHS[rank - 1],
      accent: suitMeta.accent,
    };
  }

  const honor = HONOR_LABELS[normalized];
  if (honor) {
    return {
      kind: 'honor',
      code,
      label: honor.label,
      accent: honor.accent,
    };
  }

  return {
    kind: 'fallback',
    code,
    label: code,
    accent: 'ink',
  };
}
