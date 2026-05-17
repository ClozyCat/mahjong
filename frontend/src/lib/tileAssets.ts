const RANK_GLYPHS = ['一', '二', '三', '四', '五', '六', '七', '八', '九'] as const;

const SUITED_ASSET_METADATA = {
  w: { group: '01', suffix: '萬' },
  m: { group: '01', suffix: '萬' },
  b: { group: '02', suffix: '餅' },
  p: { group: '02', suffix: '餅' },
  c: { group: '03', suffix: '條' },
  t: { group: '03', suffix: '條' },
} as const;

const HONOR_ASSET_NAMES: Record<string, string> = {
  east: '0401東風.svg',
  south: '0403南風.svg',
  west: '0402西風.svg',
  north: '0404北風.svg',
  red: '0405中.svg',
  green: '0406發.svg',
  white: '0407白.svg',
  d1: '0401東風.svg',
  d2: '0403南風.svg',
  d3: '0402西風.svg',
  d4: '0404北風.svg',
  d5: '0405中.svg',
  d6: '0406發.svg',
  d7: '0407白.svg',
};

const FLOWER_ASSET_NAMES: Record<string, string> = {
  f1: '0501春.svg',
  f2: '0502夏.svg',
  f3: '0503秋.svg',
  f4: '0504冬.svg',
  f5: '0505梅.svg',
  f6: '0506蘭.svg',
  f7: '0508竹.svg',
  f8: '0507菊.svg',
};

const SVG_URL_MAP = import.meta.glob('../../images/*.svg', {
  eager: true,
  query: '?url',
  import: 'default',
}) as Record<string, string>;

function assetUrl(fileName: string) {
  const filePath = `../../images/${fileName}`;
  return SVG_URL_MAP[filePath] ?? new URL(filePath, import.meta.url).href;
}

export type TileAsset =
  | {
      kind: 'image';
      assetName: string;
      src: string;
    }
  | {
      kind: 'blank';
    }
  | {
      kind: 'placeholder';
    };

export function getTileAsset(code: string): TileAsset {
  const normalized = code.trim().toLowerCase();
  const suited = normalized.match(/^([wbcmpt])([1-9])$/);

  if (suited) {
    const [, prefix, rank] = suited;
    const rankValue = Number(rank);
    const suit = SUITED_ASSET_METADATA[prefix as keyof typeof SUITED_ASSET_METADATA];
    const fileName = `${suit.group}${rank.padStart(2, '0')}${RANK_GLYPHS[rankValue - 1]}${suit.suffix}.svg`;

    return {
      kind: 'image',
      assetName: fileName,
      src: assetUrl(fileName),
    };
  }

  const honorFileName = HONOR_ASSET_NAMES[normalized];
  if (honorFileName) {
    return {
      kind: 'image',
      assetName: honorFileName,
      src: assetUrl(honorFileName),
    };
  }

  const flowerFileName = FLOWER_ASSET_NAMES[normalized];
  if (flowerFileName) {
    return {
      kind: 'image',
      assetName: flowerFileName,
      src: assetUrl(flowerFileName),
    };
  }

  return {
    kind: 'placeholder',
  };
}
