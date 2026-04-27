export interface TableLayoutProfile {
  id: 'balanced';
  centerVPercent: number;
  seatTop: { ratio: number; minPx: number; maxPx: number };
  safeInsetSide: { ratio: number; minPx: number; maxPx: number };
  safeInsetTop: { ratio: number; minPx: number; maxPx: number };
  safeInsetBottom: { ratio: number; minPx: number; maxPx: number };
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
    id: 'balanced',
    centerVPercent: 42,
    seatTop: { ratio: 0.012, minPx: 4, maxPx: 20 },
    safeInsetSide: { ratio: 0.012, minPx: 4, maxPx: 16 },
    safeInsetTop: { ratio: 0.012, minPx: 4, maxPx: 20 },
    safeInsetBottom: { ratio: 0.02, minPx: 10, maxPx: 32 },
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
];

export function resolveTableLayoutProfile(_width: number, _height: number) {
  return TABLE_LAYOUT_PROFILES[0];
}
