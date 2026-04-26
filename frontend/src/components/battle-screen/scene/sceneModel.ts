import type { CSSProperties } from 'react';

import type { Seat } from '../../../types/match';

import type { BattleViewportMetrics, TableStagePlayer } from './types';

const DISCARD_TILE_RATIO = 1.3586206897;

const SEATS: Seat[] = ['top', 'left', 'right', 'bottom'];

const WIND_COPY: Record<Seat, string> = {
  bottom: '东',
  right: '南',
  top: '西',
  left: '北',
};

const WIND_NAME_TO_CHAR: Record<string, string> = {
  East: '东',
  South: '南',
  West: '西',
  North: '北',
};

interface BuildTableSceneModelParams {
  viewport: BattleViewportMetrics;
  players: TableStagePlayer[];
  tileScale: number;
  occupiedSeatCount?: number;
  seatCapacity: number;
  isWaitingForMatchStart: boolean;
  roundLabel: string;
  phaseLabel: string;
}

export interface TableSeatScene {
  seat: Seat;
  player: TableStagePlayer | undefined;
  windLabel: string;
  riverColumns: number;
  hasMelds: boolean;
  shouldMuteWaitingStats: boolean;
  isDenseMeldRack: boolean;
  safeZone: {
    inlinePx: number;
    blockPx: number;
  };
  zIndex: number;
  zoneStyle: CSSProperties;
  trackStyle?: CSSProperties;
}

export interface TableSceneModel {
  effectMode: BattleViewportMetrics['effectMode'];
  layoutId: string;
  stageStyle: CSSProperties;
  seats: TableSeatScene[];
  tableSummary: string | null;
  resolvedOccupiedSeatCount: number;
  localPlayer: {
    name: string;
    absoluteSeat: number | null;
  } | null;
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

function calculateColumns(
  availablePx: number,
  tileWidthPx: number,
  gapPx: number,
  minColumns: number,
  maxColumns: number,
) {
  const unitWidth = tileWidthPx + gapPx;
  if (unitWidth <= 0) {
    return minColumns;
  }

  return clamp(Math.floor((availablePx + gapPx) / unitWidth), minColumns, maxColumns);
}

function buildTableSummary(roundLabel: string, phaseLabel: string) {
  if (roundLabel && phaseLabel) {
    return `${roundLabel} | ${phaseLabel}`;
  }

  return roundLabel || phaseLabel || null;
}

function resolveSeatStyle(
  seat: Seat,
  centerYPx: number,
  seatTopPx: number,
  sideInsetPx: number,
  bottomInsetPx: number,
  riverColumns: number,
  maxInlineSizePx: number,
): CSSProperties {
  const bottomMultiplier = centerYPx > 200 ? 0.44 : 0.36;
  const computedBottomPx = Math.max(bottomInsetPx, Math.round(seatTopPx + (centerYPx * bottomMultiplier)));

  if (seat === 'top') {
    return {
      '--table-stage-seat-anchor-left': '50%',
      '--table-stage-seat-anchor-right': 'auto',
      '--table-stage-seat-anchor-top': `${seatTopPx}px`,
      '--table-stage-seat-anchor-bottom': 'auto',
      '--table-stage-seat-anchor-transform': 'translateX(-50%)',
      '--table-stage-seat-direction': 'column',
      '--table-stage-seat-river-columns': `${riverColumns}`,
      '--table-stage-seat-max-inline-size': `${maxInlineSizePx}px`,
      '--table-stage-seat-z-index': '1',
    } as CSSProperties;
  }

  if (seat === 'bottom') {
    return {
      '--table-stage-seat-anchor-left': '50%',
      '--table-stage-seat-anchor-right': 'auto',
      '--table-stage-seat-anchor-top': 'auto',
      '--table-stage-seat-anchor-bottom': `${computedBottomPx}px`,
      '--table-stage-seat-anchor-transform': 'translateX(-50%)',
      '--table-stage-seat-direction': 'column-reverse',
      '--table-stage-seat-river-columns': `${riverColumns}`,
      '--table-stage-seat-max-inline-size': `${maxInlineSizePx}px`,
      '--table-stage-seat-z-index': '1',
    } as CSSProperties;
  }

  if (seat === 'left') {
    return {
      '--table-stage-seat-anchor-left': `${sideInsetPx}px`,
      '--table-stage-seat-anchor-right': 'auto',
      '--table-stage-seat-anchor-top': `${centerYPx}px`,
      '--table-stage-seat-anchor-bottom': 'auto',
      '--table-stage-seat-anchor-transform': 'translateY(-50%)',
      '--table-stage-seat-direction': 'row',
      '--table-stage-seat-river-columns': `${riverColumns}`,
      '--table-stage-seat-max-inline-size': `${maxInlineSizePx}px`,
      '--table-stage-seat-z-index': '1',
    } as CSSProperties;
  }

  return {
    '--table-stage-seat-anchor-left': 'auto',
    '--table-stage-seat-anchor-right': `${sideInsetPx}px`,
    '--table-stage-seat-anchor-top': `${centerYPx}px`,
    '--table-stage-seat-anchor-bottom': 'auto',
    '--table-stage-seat-anchor-transform': 'translateY(-50%)',
    '--table-stage-seat-direction': 'row-reverse',
    '--table-stage-seat-river-columns': `${riverColumns}`,
    '--table-stage-seat-max-inline-size': `${maxInlineSizePx}px`,
    '--table-stage-seat-z-index': '1',
  } as CSSProperties;
}

function getWindLabel(player: TableStagePlayer | undefined, seat: Seat) {
  return player?.wind ? WIND_NAME_TO_CHAR[player.wind] ?? WIND_COPY[seat] : WIND_COPY[seat];
}

function shouldPinDenseMeldRack(seat: Seat, meldCount: number) {
  return (seat === 'top' || seat === 'bottom') && meldCount >= 3;
}

export function buildTableSceneModel({
  viewport,
  players,
  tileScale,
  occupiedSeatCount,
  seatCapacity,
  isWaitingForMatchStart,
  roundLabel,
  phaseLabel,
}: BuildTableSceneModelParams): TableSceneModel {
  const { layoutProfile } = viewport;
  const playerBySeat = new Map(players.map((player) => [player.seat, player]));
  const resolvedOccupiedSeatCount = occupiedSeatCount ?? players.length;
  const centerYPx = viewport.height * (layoutProfile.centerVPercent / 100);

  const seatTopPx = clamp(
    viewport.height * layoutProfile.seatTop.ratio,
    layoutProfile.seatTop.minPx,
    layoutProfile.seatTop.maxPx,
  );
  const sideInsetPx = clamp(
    viewport.width * layoutProfile.safeInsetSide.ratio,
    layoutProfile.safeInsetSide.minPx,
    layoutProfile.safeInsetSide.maxPx,
  );
  const topInsetPx = clamp(
    viewport.height * layoutProfile.safeInsetTop.ratio,
    layoutProfile.safeInsetTop.minPx,
    layoutProfile.safeInsetTop.maxPx,
  );
  const bottomInsetPx = clamp(
    viewport.height * layoutProfile.safeInsetBottom.ratio,
    layoutProfile.safeInsetBottom.minPx,
    layoutProfile.safeInsetBottom.maxPx,
  );

  const horizontalSafeInlinePx = Math.min(
    viewport.width - (sideInsetPx * 2),
    viewport.width * layoutProfile.topBottomSafeWidthRatio,
  );
  const verticalSafeInlinePx = Math.min(
    viewport.height - topInsetPx - bottomInsetPx,
    viewport.height * layoutProfile.sideSafeHeightRatio,
  );
  const riverBaseWidthPx = clamp(
    viewport.width * layoutProfile.riverBaseWidth.ratio,
    layoutProfile.riverBaseWidth.minPx,
    layoutProfile.riverBaseWidth.maxPx,
  );
  const riverGapPx = clamp(
    viewport.width * layoutProfile.riverGap.ratio,
    layoutProfile.riverGap.minPx,
    layoutProfile.riverGap.maxPx,
  );
  const scaledRiverWidthPx = riverBaseWidthPx * tileScale;
  const horizontalRiverColumns = calculateColumns(
    horizontalSafeInlinePx,
    scaledRiverWidthPx,
    riverGapPx,
    layoutProfile.horizontalRiverColumns.min,
    layoutProfile.horizontalRiverColumns.max,
  );
  const verticalRiverColumns = calculateColumns(
    verticalSafeInlinePx,
    scaledRiverWidthPx,
    riverGapPx,
    layoutProfile.verticalRiverColumns.min,
    layoutProfile.verticalRiverColumns.max,
  );
  const handBaseWidthPx = clamp(
    viewport.width * layoutProfile.handBaseWidth.ratio,
    layoutProfile.handBaseWidth.minPx,
    layoutProfile.handBaseWidth.maxPx,
  );
  const meldBaseWidthPx = clamp(
    viewport.width * layoutProfile.meldBaseWidth.ratio,
    layoutProfile.meldBaseWidth.minPx,
    layoutProfile.meldBaseWidth.maxPx,
  );
  const settlementBaseWidthPx = clamp(
    viewport.width * layoutProfile.settlementBaseWidth.ratio,
    layoutProfile.settlementBaseWidth.minPx,
    layoutProfile.settlementBaseWidth.maxPx,
  );
  const minDimension = Math.min(viewport.width, viewport.height);
  const spotlightOffsetPx = clamp(
    minDimension * layoutProfile.spotlightOffset.ratio,
    layoutProfile.spotlightOffset.minPx,
    layoutProfile.spotlightOffset.maxPx,
  );
  const centerIndicatorSizePx = clamp(
    minDimension * layoutProfile.centerIndicator.ratio,
    layoutProfile.centerIndicator.minPx,
    layoutProfile.centerIndicator.maxPx,
  );
  const localPlayer = players.find((player) => player.isLocal);

  return {
    effectMode: viewport.effectMode,
    layoutId: layoutProfile.id,
    stageStyle: {
      '--table-stage-tile-scale': `${tileScale}`,
      '--table-stage-spotlight-scale': `${viewport.effectMode === 'lowFx'
        ? Math.min(layoutProfile.spotlightScale, 1.16)
        : layoutProfile.spotlightScale}`,
      '--battle-hand-tile-width-base': `${handBaseWidthPx}px`,
      '--battle-hand-tile-width': `calc(${handBaseWidthPx}px * var(--table-stage-tile-scale, 1))`,
      '--battle-hand-tile-height': 'calc(var(--battle-hand-tile-width) * 1.57)',
      '--table-stage-river-columns': `${horizontalRiverColumns}`,
      '--table-stage-meld-rows-h': `${layoutProfile.horizontalMeldRows}`,
      '--table-stage-meld-cols-v': `${layoutProfile.verticalMeldColumns}`,
      '--table-stage-river-base-width': `${riverBaseWidthPx}px`,
      '--table-stage-river-base-height': `${riverBaseWidthPx * DISCARD_TILE_RATIO}px`,
      '--table-stage-river-gap': `${riverGapPx}px`,
      '--table-stage-meld-base-width': `${meldBaseWidthPx}px`,
      '--table-stage-settlement-base-width': `${settlementBaseWidthPx}px`,
      '--table-stage-settlement-base-height': `${settlementBaseWidthPx * 1.4}px`,
      '--table-stage-center-indicator-size': `${centerIndicatorSizePx}px`,
      '--table-stage-spotlight-offset': `${spotlightOffsetPx}px`,
      '--table-stage-center-v': `${layoutProfile.centerVPercent}%`,
      '--table-stage-seat-top-v': `${seatTopPx}px`,
    } as CSSProperties,
    seats: SEATS.map((seat) => {
      const player = playerBySeat.get(seat);
      const hasMelds = (player?.melds.length ?? 0) > 0;
      const riverColumns = seat === 'top' || seat === 'bottom' ? horizontalRiverColumns : verticalRiverColumns;
      const safeZoneInlinePx = seat === 'top' || seat === 'bottom' ? horizontalSafeInlinePx : verticalSafeInlinePx;

      return {
        seat,
        player,
        windLabel: getWindLabel(player, seat),
        riverColumns,
        hasMelds,
        shouldMuteWaitingStats: isWaitingForMatchStart && player?.ready === false,
        isDenseMeldRack: shouldPinDenseMeldRack(seat, player?.melds.length ?? 0),
        safeZone: {
          inlinePx: safeZoneInlinePx,
          blockPx: seat === 'top' || seat === 'bottom' ? centerYPx : viewport.width * 0.28,
        },
        zIndex: 1,
        zoneStyle: resolveSeatStyle(
          seat,
          centerYPx,
          seatTopPx,
          sideInsetPx,
          bottomInsetPx,
          riverColumns,
          safeZoneInlinePx,
        ),
        trackStyle: seat === 'right' ? ({ direction: 'ltr' } as CSSProperties) : undefined,
      };
    }),
    tableSummary: buildTableSummary(roundLabel, phaseLabel),
    resolvedOccupiedSeatCount,
    localPlayer: localPlayer
      ? {
        name: localPlayer.name,
        absoluteSeat: typeof localPlayer.absoluteSeat === 'number' ? localPlayer.absoluteSeat : null,
      }
      : null,
  };
}
