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
  matchBarSize: {
    width: { ratio: number; minPx: number; maxPx: number };
    height: { ratio: number; minPx: number; maxPx: number };
  };
  spotlightGap: number;
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
    matchBarSize: {
      width: { ratio: 0.18, minPx: 160, maxPx: 280 },
      height: { ratio: 0.045, minPx: 36, maxPx: 56 },
    },
    spotlightGap: 8,
    riverBaseWidth: { ratio: 0.0336, minPx: 16.8, maxPx: 47.6 },
    riverGap: { ratio: 0.0008, minPx: 0.01, maxPx: 2 },
    handBaseWidth: { ratio: 0.0476, minPx: 22.4, maxPx: 67.2 },
    meldBaseWidth: { ratio: 0.0266, minPx: 16.8, maxPx: 36.4 },
    settlementBaseWidth: { ratio: 0.013, minPx: 12, maxPx: 20 },
  },
];

export function resolveTableLayoutProfile(_width: number, _height: number) {
  return TABLE_LAYOUT_PROFILES[0];
}
