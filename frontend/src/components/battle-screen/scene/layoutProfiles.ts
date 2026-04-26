export interface TableLayoutProfile {
  id: 'compact-portrait' | 'balanced' | 'wide';
  centerVPercent: number;
  seatTopPx: number;
  safeInsetSidePx: number;
  safeInsetTopPx: number;
  safeInsetBottomPx: number;
  topBottomSafeWidthRatio: number;
  sideSafeHeightRatio: number;
  spotlightScale: number;
  horizontalMeldRows: number;
  verticalMeldColumns: number;
  horizontalRiverColumns: {
    min: number;
    max: number;
  };
  verticalRiverColumns: {
    min: number;
    max: number;
  };
  centerIndicator: {
    ratio: number;
    minPx: number;
    maxPx: number;
  };
  spotlightOffset: {
    ratio: number;
    minPx: number;
    maxPx: number;
  };
  riverBaseWidth: {
    ratio: number;
    minPx: number;
    maxPx: number;
  };
  riverGap: {
    ratio: number;
    minPx: number;
    maxPx: number;
  };
  handBaseWidth: {
    ratio: number;
    minPx: number;
    maxPx: number;
  };
  meldBaseWidth: {
    ratio: number;
    minPx: number;
    maxPx: number;
  };
  settlementBaseWidth: {
    ratio: number;
    minPx: number;
    maxPx: number;
  };
}

export const TABLE_LAYOUT_PROFILES: TableLayoutProfile[] = [
  {
    id: 'wide',
    centerVPercent: 41,
    seatTopPx: 10,
    safeInsetSidePx: 14,
    safeInsetTopPx: 10,
    safeInsetBottomPx: 14,
    topBottomSafeWidthRatio: 0.76,
    sideSafeHeightRatio: 0.58,
    spotlightScale: 1.25,
    horizontalMeldRows: 2,
    verticalMeldColumns: 1,
    horizontalRiverColumns: { min: 8, max: 12 },
    verticalRiverColumns: { min: 5, max: 7 },
    centerIndicator: { ratio: 0.1, minPx: 48, maxPx: 96 },
    spotlightOffset: { ratio: 0.14, minPx: 42, maxPx: 120 },
    riverBaseWidth: { ratio: 0.022, minPx: 16, maxPx: 38 },
    riverGap: { ratio: 0.0012, minPx: 1, maxPx: 6 },
    handBaseWidth: { ratio: 0.03, minPx: 18, maxPx: 52 },
    meldBaseWidth: { ratio: 0.018, minPx: 14, maxPx: 28 },
    settlementBaseWidth: { ratio: 0.012, minPx: 12, maxPx: 22 },
  },
  {
    id: 'balanced',
    centerVPercent: 42,
    seatTopPx: 12,
    safeInsetSidePx: 12,
    safeInsetTopPx: 12,
    safeInsetBottomPx: 18,
    topBottomSafeWidthRatio: 0.72,
    sideSafeHeightRatio: 0.52,
    spotlightScale: 1.22,
    horizontalMeldRows: 2,
    verticalMeldColumns: 1,
    horizontalRiverColumns: { min: 7, max: 10 },
    verticalRiverColumns: { min: 5, max: 7 },
    centerIndicator: { ratio: 0.112, minPx: 42, maxPx: 90 },
    spotlightOffset: { ratio: 0.13, minPx: 38, maxPx: 104 },
    riverBaseWidth: { ratio: 0.024, minPx: 14, maxPx: 34 },
    riverGap: { ratio: 0.001, minPx: 1, maxPx: 5 },
    handBaseWidth: { ratio: 0.034, minPx: 16, maxPx: 48 },
    meldBaseWidth: { ratio: 0.019, minPx: 13, maxPx: 26 },
    settlementBaseWidth: { ratio: 0.013, minPx: 11, maxPx: 20 },
  },
  {
    id: 'compact-portrait',
    centerVPercent: 42,
    seatTopPx: 12,
    safeInsetSidePx: 8,
    safeInsetTopPx: 12,
    safeInsetBottomPx: 16,
    topBottomSafeWidthRatio: 0.66,
    sideSafeHeightRatio: 0.48,
    spotlightScale: 1.14,
    horizontalMeldRows: 2,
    verticalMeldColumns: 1,
    horizontalRiverColumns: { min: 6, max: 8 },
    verticalRiverColumns: { min: 6, max: 10 },
    centerIndicator: { ratio: 0.126, minPx: 16, maxPx: 82 },
    spotlightOffset: { ratio: 0.12, minPx: 14, maxPx: 88 },
    riverBaseWidth: { ratio: 0.028, minPx: 8, maxPx: 30 },
    riverGap: { ratio: 0.0008, minPx: 0.5, maxPx: 4 },
    handBaseWidth: { ratio: 0.037, minPx: 10, maxPx: 42 },
    meldBaseWidth: { ratio: 0.021, minPx: 8, maxPx: 24 },
    settlementBaseWidth: { ratio: 0.014, minPx: 6, maxPx: 18 },
  },
];

export function resolveTableLayoutProfile(width: number, height: number) {
  const aspectRatio = height > 0 ? width / height : 1;

  if (aspectRatio >= 1.55 && width >= 1180) {
    return TABLE_LAYOUT_PROFILES[0];
  }

  if (aspectRatio >= 1.02 && width >= 820) {
    return TABLE_LAYOUT_PROFILES[1];
  }

  return TABLE_LAYOUT_PROFILES[2];
}
