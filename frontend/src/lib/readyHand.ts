import type { ReadyHandWaitView } from '../types/match';

const SUIT_KEYS = ['w', 't', 'b'] as const;
const HONOR_KEYS = new Set(['east', 'south', 'west', 'north', 'red', 'green', 'white']);
const KNITTED_GROUPS = [
  [1, 4, 7],
  [2, 5, 8],
  [3, 6, 9],
] as const;
const STANDARD_WIN_TILE_KEYS = [
  ...SUIT_KEYS.flatMap((prefix) => Array.from({ length: 9 }, (_, index) => `${prefix}${index + 1}`)),
  'east',
  'south',
  'west',
  'north',
  'red',
  'green',
  'white',
] as const;

const HONOR_ALIASES: Record<string, string> = {
  d1: 'east',
  d2: 'south',
  d3: 'west',
  d4: 'north',
  d5: 'red',
  d6: 'green',
  d7: 'white',
};

const SUIT_ALIASES: Record<string, string> = {
  m: 'w',
  p: 'b',
  c: 't',
};

const WINNING_TILE_KEY_SET = new Set<string>(STANDARD_WIN_TILE_KEYS);
const KNITTED_PATTERNS = buildKnittedPatterns();

interface ReadyHandWaitInput {
  concealedTileKeys: string[];
  meldTileKeyGroups: string[][];
  knownTileKeys: string[];
}

export function getReadyHandWaits({
  concealedTileKeys,
  meldTileKeyGroups,
  knownTileKeys,
}: ReadyHandWaitInput): ReadyHandWaitView[] {
  const normalizedConcealedTileKeys = normalizeWinningTileKeyList(concealedTileKeys);
  const normalizedMeldTileKeyGroups = normalizeWinningMeldTileKeyGroups(meldTileKeyGroups);
  if (!normalizedConcealedTileKeys || !normalizedMeldTileKeyGroups) {
    return [];
  }

  const expectedConcealedCount = (4 - normalizedMeldTileKeyGroups.length) * 3 + 1;
  if (normalizedConcealedTileKeys.length !== expectedConcealedCount) {
    return [];
  }

  const handCounts = buildTileCounts([
    ...normalizedConcealedTileKeys,
    ...normalizedMeldTileKeyGroups.flat(),
  ]);
  const knownCounts = buildTileCounts(
    knownTileKeys
      .map((tileKey) => normalizeTileKey(tileKey))
      .filter((tileKey): tileKey is string => typeof tileKey === 'string'),
  );

  const waits: ReadyHandWaitView[] = [];
  for (const tileKey of STANDARD_WIN_TILE_KEYS) {
    if (getTileCount(handCounts, tileKey) >= 4) {
      continue;
    }

    if (
      isWinningHandWithMelds(
        [...normalizedConcealedTileKeys, tileKey],
        normalizedMeldTileKeyGroups,
      )
    ) {
      waits.push({
        code: tileKey,
        availableCount: Math.max(0, 4 - getTileCount(knownCounts, tileKey)),
      });
    }
  }

  return waits.sort((left, right) => compareReadyHandTileCodes(left.code, right.code));
}

export function compareReadyHandTileCodes(left: string, right: string) {
  const leftKey = getTileSortKey(left);
  const rightKey = getTileSortKey(right);

  if (leftKey.group !== rightKey.group) {
    return leftKey.group - rightKey.group;
  }

  if (leftKey.order !== rightKey.order) {
    return leftKey.order - rightKey.order;
  }

  return left.localeCompare(right);
}

function normalizeTileKey(tileKey: string) {
  const normalized = tileKey.trim().toLowerCase();
  const suited = normalized.match(/^([wbcmpt])([1-9])$/);
  if (suited) {
    const [, prefix, rank] = suited;
    return `${SUIT_ALIASES[prefix] ?? prefix}${rank}`;
  }

  if (normalized in HONOR_ALIASES) {
    return HONOR_ALIASES[normalized];
  }

  if (normalized.startsWith('f')) {
    return normalized;
  }

  return WINNING_TILE_KEY_SET.has(normalized) ? normalized : null;
}

function normalizeWinningTileKeyList(tileKeys: string[]) {
  const normalizedTileKeys: string[] = [];
  for (const tileKey of tileKeys) {
    const normalized = normalizeTileKey(tileKey);
    if (!normalized || !WINNING_TILE_KEY_SET.has(normalized)) {
      return null;
    }
    normalizedTileKeys.push(normalized);
  }
  return normalizedTileKeys;
}

function normalizeWinningMeldTileKeyGroups(meldTileKeyGroups: string[][]) {
  const normalizedMeldTileKeyGroups: string[][] = [];
  for (const meldTileKeyGroup of meldTileKeyGroups) {
    const normalizedMeldTileKeyGroup = normalizeWinningTileKeyList(meldTileKeyGroup);
    if (!normalizedMeldTileKeyGroup) {
      return null;
    }
    normalizedMeldTileKeyGroups.push(normalizedMeldTileKeyGroup);
  }
  return normalizedMeldTileKeyGroups;
}

function buildTileCounts(tileKeys: string[]) {
  const counts = new Map<string, number>();
  for (const tileKey of tileKeys) {
    counts.set(tileKey, getTileCount(counts, tileKey) + 1);
  }
  return counts;
}

function getTileCount(counts: Map<string, number>, tileKey: string) {
  return counts.get(tileKey) ?? 0;
}

function parseSuit(tileKey: string): [string, number] | null {
  const suited = tileKey.match(/^([wtb])([1-9])$/);
  if (!suited) {
    return null;
  }

  return [suited[1], Number(suited[2])];
}

function isWinningHandWithMelds(concealedTileKeys: string[], meldTileKeyGroups: string[][]) {
  if (meldTileKeyGroups.length === 0) {
    return isWinningHand(concealedTileKeys);
  }

  const normalizedMelds = meldTileKeyGroups.map(normalizeMeldTileKeyGroup);
  if (normalizedMelds.some((meld) => meld === null)) {
    return false;
  }

  const remainingMeldCount = 4 - normalizedMelds.length;
  if (remainingMeldCount < 0) {
    return false;
  }

  if (concealedTileKeys.length !== remainingMeldCount * 3 + 2) {
    return false;
  }

  return isStandardHand(buildTileCounts(concealedTileKeys));
}

function isWinningHand(tileKeys: string[]) {
  if (tileKeys.length !== 14) {
    return false;
  }

  const counts = buildTileCounts(tileKeys);
  return (
    isSevenPairs(counts) ||
    isThirteenOrphans(counts) ||
    isSpecialKnittedHand(counts) ||
    isStandardHand(counts)
  );
}

function isSevenPairs(counts: Map<string, number>) {
  if (sumTileCounts(counts) !== 14) {
    return false;
  }

  let pairCount = 0;
  for (const count of counts.values()) {
    if (count !== 2 && count !== 4) {
      return false;
    }
    pairCount += count / 2;
  }

  return pairCount === 7;
}

function isThirteenOrphans(counts: Map<string, number>) {
  const requiredTileKeys = [
    'w1',
    'w9',
    't1',
    't9',
    'b1',
    'b9',
    'east',
    'south',
    'west',
    'north',
    'red',
    'green',
    'white',
  ];

  for (const tileKey of counts.keys()) {
    if (!requiredTileKeys.includes(tileKey)) {
      return false;
    }
  }

  let duplicateCount = 0;
  for (const tileKey of requiredTileKeys) {
    const count = getTileCount(counts, tileKey);
    if (count === 0) {
      return false;
    }
    if (count === 2) {
      duplicateCount += 1;
    } else if (count !== 1) {
      return false;
    }
  }

  return duplicateCount === 1 && sumTileCounts(counts) === 14;
}

function isSpecialKnittedHand(counts: Map<string, number>) {
  const isAllSingletons = Array.from(counts.values()).every((count) => count === 1);
  const honorTileKeys = Array.from(counts.keys())
    .filter((tileKey) => HONOR_KEYS.has(tileKey))
    .sort();

  for (const pattern of KNITTED_PATTERNS) {
    const isPatternSubset = Array.from(pattern).every((tileKey) => getTileCount(counts, tileKey) > 0);

    if (isPatternSubset) {
      const remainingCounts = cloneTileCounts(counts);
      for (const tileKey of pattern) {
        decrementTileCount(remainingCounts, tileKey, 1);
      }

      if (remainingCounts.size > 0) {
        if (
          Array.from(remainingCounts.keys()).every((tileKey) => HONOR_KEYS.has(tileKey)) &&
          remainingCounts.size === 5 &&
          Array.from(remainingCounts.values()).every((count) => count === 1)
        ) {
          return true;
        }

        if (hasFiveTileCompletion(remainingCounts)) {
          return true;
        }
      }
    }

    if (!isAllSingletons) {
      continue;
    }

    const suitTileKeys = Array.from(counts.keys()).filter((tileKey) => !HONOR_KEYS.has(tileKey));
    if (!suitTileKeys.every((tileKey) => pattern.has(tileKey))) {
      continue;
    }

    if (honorTileKeys.length >= 5) {
      return true;
    }

    if (
      honorTileKeys.length === 7 &&
      honorTileKeys.every((tileKey) => HONOR_KEYS.has(tileKey))
    ) {
      return true;
    }
  }

  return false;
}

function hasFiveTileCompletion(counts: Map<string, number>) {
  if (sumTileCounts(counts) !== 5) {
    return false;
  }

  for (const [pairTileKey, count] of counts.entries()) {
    if (count < 2) {
      continue;
    }

    const nextCounts = cloneTileCounts(counts);
    decrementTileCount(nextCounts, pairTileKey, 2);

    if (nextCounts.size === 1) {
      const [meldTileKey, meldCount] = nextCounts.entries().next().value as [string, number];
      if (meldCount === 3 && WINNING_TILE_KEY_SET.has(meldTileKey)) {
        return true;
      }
    }

    if (canFormMelds(nextCounts)) {
      return true;
    }
  }

  return false;
}

function isStandardHand(counts: Map<string, number>) {
  for (const [tileKey, count] of counts.entries()) {
    if (count < 2) {
      continue;
    }

    const nextCounts = cloneTileCounts(counts);
    decrementTileCount(nextCounts, tileKey, 2);
    if (canFormMelds(nextCounts)) {
      return true;
    }
  }

  return false;
}

function canFormMelds(counts: Map<string, number>): boolean {
  if (counts.size === 0) {
    return true;
  }

  const tileKey = Array.from(counts.keys()).sort()[0];
  if (!tileKey) {
    return true;
  }

  const count = getTileCount(counts, tileKey);
  if (count <= 0) {
    const nextCounts = cloneTileCounts(counts);
    nextCounts.delete(tileKey);
    return canFormMelds(nextCounts);
  }

  if (count >= 3) {
    const nextCounts = cloneTileCounts(counts);
    decrementTileCount(nextCounts, tileKey, 3);
    if (canFormMelds(nextCounts)) {
      return true;
    }
  }

  const parsedTile = parseSuit(tileKey);
  if (!parsedTile) {
    return false;
  }

  const [prefix, rank] = parsedTile;
  if (rank > 7) {
    return false;
  }

  const secondTileKey = `${prefix}${rank + 1}`;
  const thirdTileKey = `${prefix}${rank + 2}`;
  if (getTileCount(counts, secondTileKey) === 0 || getTileCount(counts, thirdTileKey) === 0) {
    return false;
  }

  const nextCounts = cloneTileCounts(counts);
  decrementTileCount(nextCounts, tileKey, 1);
  decrementTileCount(nextCounts, secondTileKey, 1);
  decrementTileCount(nextCounts, thirdTileKey, 1);
  return canFormMelds(nextCounts);
}

function normalizeMeldTileKeyGroup(meldTileKeys: string[]) {
  if (meldTileKeys.length === 3) {
    return meldTileKeys;
  }

  if (meldTileKeys.length === 4 && new Set(meldTileKeys).size === 1) {
    return meldTileKeys.slice(0, 3);
  }

  return null;
}

function decrementTileCount(counts: Map<string, number>, tileKey: string, amount: number) {
  const nextCount = getTileCount(counts, tileKey) - amount;
  if (nextCount > 0) {
    counts.set(tileKey, nextCount);
    return;
  }

  counts.delete(tileKey);
}

function cloneTileCounts(counts: Map<string, number>) {
  return new Map<string, number>(counts);
}

function sumTileCounts(counts: Map<string, number>) {
  let total = 0;
  for (const count of counts.values()) {
    total += count;
  }
  return total;
}

function buildKnittedPatterns() {
  const patterns = new Set<string>();
  for (const first of SUIT_KEYS) {
    for (const second of SUIT_KEYS) {
      if (second === first) {
        continue;
      }
      for (const third of SUIT_KEYS) {
        if (third === first || third === second) {
          continue;
        }

        const tileKeys = [
          ...KNITTED_GROUPS[0].map((rank) => `${first}${rank}`),
          ...KNITTED_GROUPS[1].map((rank) => `${second}${rank}`),
          ...KNITTED_GROUPS[2].map((rank) => `${third}${rank}`),
        ].sort();

        patterns.add(tileKeys.join('|'));
      }
    }
  }

  return Array.from(patterns, (pattern) => new Set(pattern.split('|')));
}

function getTileSortKey(tileKey: string) {
  const parsedTile = parseSuit(tileKey);
  if (parsedTile) {
    const [prefix, rank] = parsedTile;
    const suitOrder = { w: 0, b: 1, t: 2 } as const;
    return {
      group: 0,
      order: suitOrder[prefix as keyof typeof suitOrder] * 10 + rank,
    };
  }

  const honorOrder = {
    east: 0,
    south: 1,
    west: 2,
    north: 3,
    red: 4,
    green: 5,
    white: 6,
  } as const;

  return {
    group: 1,
    order: honorOrder[tileKey as keyof typeof honorOrder] ?? Number.MAX_SAFE_INTEGER,
  };
}
