const SUITED_ASSET_NAMES = {
  w: 'man',
  m: 'man',
  b: 'pin',
  p: 'pin',
  c: 'sou',
  t: 'sou',
} as const;

const HONOR_ASSET_NAMES: Record<string, string> = {
  east: 'east.svg',
  south: 'south.svg',
  west: 'west.svg',
  north: 'north.svg',
  red: 'zhong.svg',
  green: 'fa.svg',
  d1: 'east.svg',
  d2: 'south.svg',
  d3: 'west.svg',
  d4: 'north.svg',
  d5: 'zhong.svg',
  d6: 'fa.svg',
};

const BLANK_CODES = new Set(['white', 'd7']);
const SVG_VIEWBOX = '0 0 320 446';
const SVG_SOURCE_MAP = import.meta.glob('../../images/*.svg', {
  eager: true,
  query: '?raw',
  import: 'default',
}) as Record<string, string>;

function normalizeSvgMarkup(svgText: string) {
  return svgText.replace(/<svg\b([^>]*)>/i, (match, attributes) => {
    let nextAttributes = attributes;

    if (!/viewBox=/i.test(nextAttributes)) {
      nextAttributes += ` viewBox="${SVG_VIEWBOX}"`;
    }

    if (!/preserveAspectRatio=/i.test(nextAttributes)) {
      nextAttributes += ' preserveAspectRatio="xMidYMid meet"';
    }

    return `<svg${nextAttributes}>`;
  });
}

function assetUrl(fileName: string) {
  const filePath = `../../images/${fileName}`;
  const rawSvg = SVG_SOURCE_MAP[filePath];

  if (!rawSvg) {
    return new URL(filePath, import.meta.url).href;
  }

  return `data:image/svg+xml,${encodeURIComponent(normalizeSvgMarkup(rawSvg))}`;
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
    const fileName = `${rank}${SUITED_ASSET_NAMES[prefix as keyof typeof SUITED_ASSET_NAMES]}.svg`;

    return {
      kind: 'image',
      assetName: fileName,
      src: assetUrl(fileName),
    };
  }

  if (BLANK_CODES.has(normalized)) {
    return {
      kind: 'blank',
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

  return {
    kind: 'placeholder',
  };
}
