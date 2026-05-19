import type { CSSProperties } from 'react';
import { memo, useEffect, useId, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

import type {
  BattleActionId,
  PlayerMeldView,
  PlayerView,
  ResultPageView,
  ResultSeatView,
  ResultView,
  Seat,
} from '../../types/match';
import { getFanGuideEntry, getFanLabel } from './fanGuide';
import { FanGuideCard, getFanColor } from './FanGuideCard';
import { MahjongTile } from './MahjongTile';
import { MeldRack } from './MeldRack';

interface ResultOverlayProps {
  result: ResultView;
  settlementKey: string;
  settlementHands?: Partial<Record<Seat, string[]>> | null;
  players?: Pick<PlayerView, 'seat' | 'absoluteSeat' | 'melds' | 'wind'>[];
  onAction: (actionId: BattleActionId) => void;
}

export function ResultOverlay({ result, settlementKey, settlementHands, players = [], onAction }: ResultOverlayProps) {
  const [isCollapsed, setIsCollapsed] = useState(false);
  const [activeResultPageIndex, setActiveResultPageIndex] = useState(0);
  const [activeScorePageIndex, setActiveScorePageIndex] = useState(0);
  const [continueActionRemainingSeconds, setContinueActionRemainingSeconds] = useState<number | null>(null);
  const [activeFanGuide, setActiveFanGuide] = useState<{
    rowKey: string;
    entry: NonNullable<ReturnType<typeof getFanGuideEntry>>;
  } | null>(null);
  const [fanGuidePopoverPosition, setFanGuidePopoverPosition] = useState<{
    top: number;
    left: number;
    placement: 'left' | 'right';
    arrowTop: number;
  } | null>(null);
  const [dynamicScale, setDynamicScale] = useState(1);

  const cardRef = useRef<HTMLDivElement | null>(null);
  const fanGuidePopoverRef = useRef<HTMLDivElement | null>(null);
  const activeFanGuideAnchorRef = useRef<HTMLElement | null>(null);
  const openFanGuideTimerRef = useRef<number | null>(null);
  const closeFanGuideTimerRef = useRef<number | null>(null);
  const previousSettlementKeyRef = useRef(settlementKey);

  const resultPages = getResultPages(result);
  const activeResultPage = resultPages[activeResultPageIndex] ?? null;
  const hasFanPanel = activeResultPage?.fanTotal !== null || (activeResultPage?.fanBreakdown.length ?? 0) > 0;
  const discardWinnerSeats = getDiscardWinnerSeats(resultPages, result.winType);
  const seatLabelByAbsoluteSeat = getSeatLabelByAbsoluteSeat(players);
  const meldsBySeat = getMeldsBySeat(players);
  const seatsWithWind = result.seats.map((seat) => {
    const playerWind = players.find((p) => p.seat === seat.seat)?.wind;
    return {
      ...seat,
      wind: playerWind || seat.wind,
    };
  });

  useEffect(() => {
    if (previousSettlementKeyRef.current === settlementKey) {
      return;
    }

    previousSettlementKeyRef.current = settlementKey;
    setIsCollapsed(false);
    setActiveResultPageIndex(0);
    setActiveScorePageIndex(0);
  }, [settlementKey]);

  useEffect(() => {
    setActiveResultPageIndex((currentIndex) => Math.min(currentIndex, Math.max(resultPages.length - 1, 0)));
    setActiveScorePageIndex((currentIndex) => Math.min(currentIndex, Math.max(seatsWithWind.length - 1, 0)));
  }, [resultPages.length, seatsWithWind.length]);

  useEffect(() => {
    clearFanGuideTimers(openFanGuideTimerRef, closeFanGuideTimerRef);
    activeFanGuideAnchorRef.current = null;
    setActiveFanGuide(null);
    setFanGuidePopoverPosition(null);
  }, [result]);

  useEffect(() => {
    const deadlineAt = result.continueAction?.countdownDeadlineAt;
    if (!deadlineAt) {
      setContinueActionRemainingSeconds(null);
      return undefined;
    }

    const update = () => {
      const nextRemaining = Math.max(0, Math.ceil((new Date(deadlineAt).getTime() - Date.now()) / 1000));
      setContinueActionRemainingSeconds(nextRemaining);
    };

    update();
    const timer = window.setInterval(update, 250);
    return () => window.clearInterval(timer);
  }, [result.continueAction?.countdownDeadlineAt]);

  useLayoutEffect(() => {
    if (!activeFanGuide || typeof window === 'undefined') {
      return undefined;
    }

    let animationFrameId = 0;
    const updatePosition = () => {
      const anchorRect = activeFanGuideAnchorRef.current?.getBoundingClientRect();
      if (!anchorRect) {
        setActiveFanGuide(null);
        setFanGuidePopoverPosition(null);
        return;
      }
      const popoverRect = fanGuidePopoverRef.current?.getBoundingClientRect();
      const nextPosition = getOverlayPopoverPosition(anchorRect, popoverRect?.width ?? 336, popoverRect?.height ?? 208);
      setFanGuidePopoverPosition((currentPosition) => {
        if (currentPosition &&
            Math.abs(currentPosition.top - nextPosition.top) < 1 &&
            Math.abs(currentPosition.left - nextPosition.left) < 1 &&
            currentPosition.placement === nextPosition.placement) {
          return currentPosition;
        }
        return nextPosition;
      });
    };

    const requestPositionUpdate = () => {
      window.cancelAnimationFrame(animationFrameId);
      animationFrameId = window.requestAnimationFrame(updatePosition);
    };

    requestPositionUpdate();
    window.addEventListener('resize', requestPositionUpdate);
    window.addEventListener('scroll', requestPositionUpdate, true);
    return () => {
      window.cancelAnimationFrame(animationFrameId);
      window.removeEventListener('resize', requestPositionUpdate);
      window.removeEventListener('scroll', requestPositionUpdate, true);
    };
  }, [activeFanGuide]);

  useLayoutEffect(() => {
    if (typeof window === 'undefined' || isCollapsed) {
      return undefined;
    }

    let animationFrameId = 0;
    const updateScale = () => {
      const cardElement = cardRef.current;
      if (!cardElement) return;

      const availableWidth = Math.max(window.innerWidth - 32, 1);
      const availableHeight = Math.max(window.innerHeight - 32, 1);
      const baseWidth = Math.max(cardElement.offsetWidth, 1);
      const baseHeight = Math.max(cardElement.offsetHeight, 1);
      const nextScale = Math.max(0.4, Math.min(availableWidth / baseWidth, availableHeight / baseHeight, 1));

      setDynamicScale((currentScale) => Math.abs(currentScale - nextScale) < 0.01 ? currentScale : nextScale);
    };

    const requestScaleUpdate = () => {
      window.cancelAnimationFrame(animationFrameId);
      animationFrameId = window.requestAnimationFrame(updateScale);
    };

    requestScaleUpdate();
    window.addEventListener('resize', requestScaleUpdate);
    return () => {
      window.cancelAnimationFrame(animationFrameId);
      window.removeEventListener('resize', requestScaleUpdate);
    };
  }, [isCollapsed, result, activeResultPageIndex, hasFanPanel]);

  const scheduleFanGuideOpen = (rowKey: string, fanKey: string, rowElement: HTMLElement) => {
    const nextEntry = getFanGuideEntry(fanKey);
    if (!nextEntry) return;

    clearOverlayPopoverCloseTimer(closeFanGuideTimerRef);

    if (activeFanGuide?.rowKey === rowKey) {
      activeFanGuideAnchorRef.current = rowElement;
      return;
    }

    if (openFanGuideTimerRef.current !== null) {
      window.clearTimeout(openFanGuideTimerRef.current);
    }

    openFanGuideTimerRef.current = window.setTimeout(() => {
      activeFanGuideAnchorRef.current = rowElement;
      setFanGuidePopoverPosition(getOverlayPopoverPosition(rowElement.getBoundingClientRect(), 336, 208));
      setActiveFanGuide({ rowKey, entry: nextEntry });
      openFanGuideTimerRef.current = null;
    }, FAN_GUIDE_POPOVER_DELAY_MS);
  };

  const scheduleFanGuideClose = () => {
    if (openFanGuideTimerRef.current !== null) {
      window.clearTimeout(openFanGuideTimerRef.current);
      openFanGuideTimerRef.current = null;
    }

    clearOverlayPopoverCloseTimer(closeFanGuideTimerRef);
    closeFanGuideTimerRef.current = window.setTimeout(() => {
      activeFanGuideAnchorRef.current = null;
      setActiveFanGuide(null);
      setFanGuidePopoverPosition(null);
      closeFanGuideTimerRef.current = null;
    }, FAN_GUIDE_POPOVER_CLOSE_DELAY_MS);
  };

  const fanGuidePopover =
    activeFanGuide && typeof document !== 'undefined'
      ? createPortal(
          <div
            ref={fanGuidePopoverRef}
            role="tooltip"
            aria-label={`${activeFanGuide.entry.label}番型说明`}
            className={`result-overlay__fan-tooltip result-overlay__fan-tooltip--${fanGuidePopoverPosition?.placement ?? 'right'}`.trim()}
            style={getOverlayPopoverStyle(fanGuidePopoverPosition, '--result-overlay-fan-tooltip-arrow-top')}
            onMouseEnter={() => clearOverlayPopoverCloseTimer(closeFanGuideTimerRef)}
            onMouseLeave={scheduleFanGuideClose}
          >
            <FanGuideCard entry={activeFanGuide.entry} className="result-overlay__fan-tooltip-card" />
          </div>,
          document.body,
        )
      : null;

  const content = isCollapsed ? (
    <section
      className="result-overlay result-overlay--collapsed"
      style={{ '--result-overlay-dynamic-scale': dynamicScale } as CSSProperties}
    >
      <button type="button" className="result-overlay__restore" onClick={() => setIsCollapsed(false)}>
        展开结算面板
      </button>
    </section>
  ) : (
    <section
      className="result-overlay result-overlay--expanded"
      style={{ '--result-overlay-dynamic-scale': dynamicScale } as CSSProperties}
    >
      <div ref={cardRef} className="result-overlay__card">

        <ScoreSection
          seats={seatsWithWind}
          pages={resultPages}
          settlementHands={settlementHands}
          meldsBySeat={meldsBySeat}
          discardWinnerSeats={discardWinnerSeats}
          seatLabelByAbsoluteSeat={seatLabelByAbsoluteSeat}
        />

        <div className="result-overlay__actions">
          <button type="button" className="result-overlay__collapse-btn" onClick={() => setIsCollapsed(true)}>
            收起面板
          </button>
          {result.continueAction && (
            <button
              type="button"
              className="result-overlay__primary-btn"
              disabled={!result.continueAction.enabled}
              onClick={() => onAction(result.continueAction!.id)}
            >
              {continueActionRemainingSeconds !== null
                ? `${continueActionRemainingSeconds}s后自动推进`
                : result.continueAction.label}
            </button>
          )}
        </div>
      </div>
      {fanGuidePopover}
    </section>
  );

  if (typeof document === 'undefined') return null;
  return createPortal(content, document.body);
}

/**
 * Sub-components for better organization and performance
 */



const ScoreSection = memo(({
  seats,
  pages,
  settlementHands,
  meldsBySeat,
  discardWinnerSeats,
  seatLabelByAbsoluteSeat
}: {
  seats: ResultSeatView[];
  pages: ResultPageView[];
  settlementHands?: Partial<Record<Seat, string[]>> | null;
  meldsBySeat: Partial<Record<Seat, PlayerMeldView[]>>;
  discardWinnerSeats: ReadonlySet<Seat>;
  seatLabelByAbsoluteSeat: SeatLabelByAbsoluteSeat;
}) => {
  return (
    <div className="result-overlay__score-panel">
      <div className="result-overlay__section-head">
        <span className="result-overlay__section-label">对局结算</span>
      </div>
      <div className="result-overlay__seat-list">
        {seats.map((seat) => {
          const winPage = pages.find(p => p.winnerSeat === seat.seat);
          const winTypeLabel = winPage ? (winPage.winTypeLabel ?? (winPage.winType ? WIN_TYPE_LABELS[winPage.winType] : null)) : null;
          const isDiscarder = pages.some(p => p.discarderSeat === seat.seat);

          return (
            <SeatRow
              key={seat.seat}
              seat={seat}
              winPage={winPage ?? null}
              hand={settlementHands?.[seat.seat]}
              melds={meldsBySeat[seat.seat] ?? []}
              isDiscardWinner={discardWinnerSeats.has(seat.seat)}
              winTypeLabel={winTypeLabel}
              isDiscarder={isDiscarder}
              seatLabel={getResultSeatLabel(seat, seatLabelByAbsoluteSeat)}
            />
          );
        })}
      </div>
    </div>
  );
});

const SeatRow = memo(({
  seat,
  winPage,
  hand,
  melds,
  isDiscardWinner,
  winTypeLabel,
  isDiscarder,
  seatLabel
}: {
  seat: ResultSeatView;
  winPage: ResultPageView | null;
  hand?: string[];
  melds: PlayerMeldView[];
  isDiscardWinner: boolean;
  winTypeLabel?: string | null;
  isDiscarder?: boolean;
  seatLabel: string;
}) => {
  const deltaClass = !seat.delta ? 'neutral' : seat.delta > 0 ? 'positive' : 'negative';
  const displayName = seat.displayLabel ?? seat.name;
  const hasHandTiles = Boolean(hand && hand.length > 0);
  const hasMelds = melds.length > 0;
  const hasStats = Boolean(seat.stats && seat.stats.scoreHistory.length > 0);
  const hasFanInfo = winPage !== null && winPage.fanTotal !== null && winPage.fanBreakdown.length > 0;
  const fanMultiplier = hasFanInfo ? formatFanTotalForDisplay(winPage!) : null;

  return (
    <div className="result-overlay__seat-row">
      <div className="result-overlay__seat-main">
        <div className="result-overlay__seat-main-left">
          <div className="result-overlay__seat-wind">
            {seat.wind && (
              <span className={`result-overlay__seat-absolute result-overlay__seat-absolute--${WIND_TO_LABEL[seat.wind]}`}>
                {WIND_TO_LABEL[seat.wind]}
              </span>
            )}
          </div>
          <div className="result-overlay__seat-info">
            <span className="result-overlay__seat-name">{displayName}</span>
            <span className="result-overlay__seat-tag">{seatLabel}</span>
          </div>
        </div>
        {hasFanInfo && (
          <div className="result-overlay__seat-fan-info">
            {winPage.fanBreakdown.map((f) => {
              const entry = getFanGuideEntry(f.fanKey);
              const displayValue = entry?.fanValue ?? f.fanValue;
              return (
                <span key={f.fanKey} className="result-overlay__seat-fan-badge" style={{ '--fan-bg': getFanColor(displayValue) } as CSSProperties}>
                  <span className="result-overlay__seat-fan-badge-label">{getFanLabel(f.fanKey)}</span>
                  <span className="result-overlay__seat-fan-badge-value">+{formatNumberForDisplay(displayValue)}番</span>
                </span>
              );
            })}
            <span className="result-overlay__seat-fan-total">
              最终<span className="result-overlay__seat-fan-total-value">{fanMultiplier}</span>番
            </span>
          </div>
        )}
        <div className="result-overlay__seat-main-right">
          <div className="result-overlay__seat-score-group">
            <strong className="result-overlay__seat-score">{seat.score}</strong>
            <span className={`result-overlay__seat-delta result-overlay__seat-delta--${deltaClass}`}>
              {seat.delta === null ? '总分' : `${seat.delta > 0 ? '+' : ''}${seat.delta}`}
            </span>
          </div>
          <div className="result-overlay__seat-status">
            {winTypeLabel && (
              <span className="result-overlay__seat-status-badge result-overlay__seat-status-badge--win">
                {winTypeLabel}
              </span>
            )}
            {isDiscarder && (
              <span className="result-overlay__seat-status-badge result-overlay__seat-status-badge--discard">
                放铳
              </span>
            )}
          </div>
        </div>
      </div>
      <div className="result-overlay__seat-tiles-area">
        {(hasHandTiles || hasMelds) && (
          <div className="result-overlay__seat-tiles">
            {hasHandTiles && (
              <div className="result-overlay__seat-hand" aria-label={`${displayName} 最终手牌`}>
                {hand!.map((tile, i) => (
                  <MahjongTile
                    key={`${seat.seat}-tile-${i}`}
                    code={tile}
                    variant="discard"
                    isLastDiscard={isDiscardWinner && i === hand!.length - 1}
                    className="result-overlay__seat-hand-tile"
                  />
                ))}
              </div>
            )}
            {hasMelds && (
              <div className="result-overlay__seat-melds">
                <MeldRack
                  seat={seat.seat}
                  melds={melds}
                  ariaLabel={`${displayName} 副露区`}
                />
              </div>
            )}
          </div>
        )}
        {hasStats && (
          <>
            <SeatStatsInline seat={seat} />
            <ScoreTrendChartCompact history={seat.stats!.scoreHistory} />
          </>
        )}
      </div>
    </div>
  );
});

/**
 * Utilities & Constants
 */

const WIN_TYPE_LABELS: Record<string, string> = {
  discard: '荣和',
  self_draw: '自摸',
  draw: '流局',
};
const FAN_GUIDE_POPOVER_DELAY_MS = 350;
const FAN_GUIDE_POPOVER_CLOSE_DELAY_MS = 100;
const SEAT_STATS_POPOVER_DELAY_MS = 350;
const SEAT_STATS_POPOVER_CLOSE_DELAY_MS = 80;
const OVERLAY_POPOVER_OFFSET_PX = 14;
const OVERLAY_POPOVER_MARGIN_PX = 12;
const OVERLAY_POPOVER_ARROW_SIZE_PX = 11;
const OVERLAY_POPOVER_ARROW_MARGIN_PX = 16;

const RELATIVE_SEAT_LABELS: Record<Seat, string> = {
  bottom: '本家',
  left: '上家',
  top: '对家',
  right: '下家',
};

const WIND_TO_LABEL: Record<string, string> = {
  East: '东',
  South: '南',
  West: '西',
  North: '北',
};

type SeatLabelByAbsoluteSeat = ReadonlyMap<number, string>;

function getRelativeSeatLabel(seat: Seat) {
  return RELATIVE_SEAT_LABELS[seat];
}

function getSeatLabelByAbsoluteSeat(players: Pick<PlayerView, 'seat' | 'absoluteSeat'>[]): SeatLabelByAbsoluteSeat {
  return new Map(
    players
      .filter((player): player is Pick<PlayerView, 'seat'> & { absoluteSeat: number } =>
        typeof player.absoluteSeat === 'number',
      )
      .map((player) => [player.absoluteSeat, getRelativeSeatLabel(player.seat)]),
  );
}

function getMeldsBySeat(players: Pick<PlayerView, 'seat' | 'melds'>[]): Partial<Record<Seat, PlayerMeldView[]>> {
  return Object.fromEntries(
    players
      .filter((player) => player.melds.length > 0)
      .map((player) => [player.seat, player.melds]),
  ) as Partial<Record<Seat, PlayerMeldView[]>>;
}

function getResultSeatLabel(seat: Pick<ResultSeatView, 'seat' | 'absoluteSeat'>, labels: SeatLabelByAbsoluteSeat) {
  return typeof seat.absoluteSeat === 'number'
    ? labels.get(seat.absoluteSeat) ?? getRelativeSeatLabel(seat.seat)
    : getRelativeSeatLabel(seat.seat);
}

function getResultPages(result: ResultView): ResultPageView[] {
  if (Array.isArray(result.pages) && result.pages.length > 0) return result.pages;
  return [{
    fanTotal: result.fanTotal,
    winnerSeat: result.winnerSeat,
    winnerAbsoluteSeat: result.winnerAbsoluteSeat,
    discarderSeat: result.discarderSeat,
    discarderAbsoluteSeat: result.discarderAbsoluteSeat,
    winType: result.winType,
    winTypeLabel: result.winTypeLabel,
    flowerCount: result.flowerCount,
    fanBreakdown: result.fanBreakdown,
  }];
}

function getDiscardWinnerSeats(pages: ResultPageView[], fallbackWinType: string | null) {
  return new Set(
    pages
      .filter((page) => (page.winType ?? fallbackWinType) === 'discard' && page.winnerSeat)
      .map((page) => page.winnerSeat as Seat),
  );
}

function formatFanTotalForDisplay(page: ResultPageView) {
  if (page.fanTotal === null) {
    return null;
  }

  const detailedFanTotal = calculateDetailedFanTotal(page.fanBreakdown);
  if (detailedFanTotal === null || Math.round(detailedFanTotal) !== page.fanTotal) {
    return formatNumberForDisplay(page.fanTotal);
  }

  return formatNumberForDisplay(detailedFanTotal);
}

function getFanColorForPage(page: ResultPageView) {
  const detailedFanTotal = calculateDetailedFanTotal(page.fanBreakdown);
  return getFanColor(detailedFanTotal ?? page.fanTotal ?? 1);
}

function calculateDetailedFanTotal(fanBreakdown: ResultPageView['fanBreakdown']) {
  if (fanBreakdown.length === 0) {
    return null;
  }

  let total = 0;
  for (const item of fanBreakdown) {
    const guideEntry = getFanGuideEntry(item.fanKey);
    const displayValue = guideEntry?.fanValue ?? item.fanValue;

    if (!Number.isFinite(displayValue) || displayValue < 0) {
      return null;
    }

    total += displayValue;
  }

  return total;
}

function formatNumberForDisplay(value: number) {
  return Number.isInteger(value)
    ? String(value)
    : value.toFixed(2).replace(/\.?0+$/, '');
}

function formatResultActor(
  seat: Seat,
  seats: ResultSeatView[],
  labels: SeatLabelByAbsoluteSeat,
  absoluteSeat?: number | null,
) {
  const view = typeof absoluteSeat === 'number'
    ? seats.find(s => s.absoluteSeat === absoluteSeat) ?? seats.find(s => s.seat === seat)
    : seats.find(s => s.seat === seat);
  const label = view
    ? getResultSeatLabel(view, labels)
    : typeof absoluteSeat === 'number'
      ? labels.get(absoluteSeat) ?? getRelativeSeatLabel(seat)
      : getRelativeSeatLabel(seat);
  return view?.name ? `${view.name}（${label}）` : label;
}

function clearFanGuideTimers(open: React.MutableRefObject<number | null>, close: React.MutableRefObject<number | null>) {
  if (open.current !== null) { window.clearTimeout(open.current); open.current = null; }
  if (close.current !== null) { window.clearTimeout(close.current); close.current = null; }
}

function clearOverlayPopoverCloseTimer(timerRef: React.MutableRefObject<number | null>) {
  if (timerRef.current !== null) { window.clearTimeout(timerRef.current); timerRef.current = null; }
}

function SeatStatsInline({ seat }: { seat: ResultSeatView }) {
  const stats = seat.stats!;
  const completed = stats.completedRoundCount;
  const winRate = `${(Math.round(stats.winRate * 1000) / 10).toFixed(1).replace(/\.0$/, '')}%`;

  return (
    <div className="result-overlay__seat-stats">
      <div className="result-overlay__seat-stats-metrics">
        <span className="result-overlay__seat-stats-metric">
          <span className="result-overlay__seat-stats-label">胜率</span>
          <strong className="result-overlay__seat-stats-value">{winRate}</strong>
        </span>
        <span className="result-overlay__seat-stats-metric">
          <span className="result-overlay__seat-stats-label">战绩</span>
          <strong className="result-overlay__seat-stats-value">{stats.winCount}/{completed || 0}</strong>
        </span>
        <span className="result-overlay__seat-stats-metric">
          <span className="result-overlay__seat-stats-label">放铳</span>
          <strong className="result-overlay__seat-stats-value">{stats.dealInCount}</strong>
        </span>
      </div>
    </div>
  );
}

function ScoreTrendChartCompact({ history }: { history: number[] }) {
  const chartId = useId();
  const width = 260;
  const height = 44;
  const padding = { l: 0, r: 0, t: 0, b: 0 };
  if (history.length === 0) return null;
  const min = Math.min(...history);
  const max = Math.max(...history);
  const range = Math.max(1, max - min);
  const innerW = width - padding.l - padding.r;
  const innerH = height - padding.t - padding.b;

  const points = history.map((s, i) => ({
    x: padding.l + (history.length === 1 ? innerW / 2 : (innerW * i) / (history.length - 1)),
    y: padding.t + innerH - ((s - min) / range) * innerH,
  }));

  const polylinePoints = points.map(p => `${p.x},${p.y}`).join(' ');

  return (
    <div className="result-overlay__seat-stats-chart">
      <svg viewBox={`0 0 ${width} ${height}`} className="result-overlay__seat-stats-chart-svg">
        <defs>
          <linearGradient id={`${chartId}-fill`} x1="0%" y1="0%" x2="0%" y2="100%">
            <stop offset="0%" stopColor="color-mix(in srgb, var(--accent) 20%, transparent)" />
            <stop offset="100%" stopColor="transparent" />
          </linearGradient>
        </defs>
        <polyline
          fill="none"
          stroke="var(--accent)"
          strokeWidth="1.5"
          strokeLinejoin="round"
          strokeLinecap="round"
          points={polylinePoints}
          className="result-overlay__seat-stats-chart-line"
        />
        {points.map((p, i) => (
          <circle
            key={i}
            cx={p.x}
            cy={p.y}
            r={i === points.length - 1 ? 3.5 : 2}
            className={`result-overlay__seat-stats-chart-dot ${i === points.length - 1 ? 'result-overlay__seat-stats-chart-dot--last' : ''}`.trim()}
          />
        ))}
      </svg>
    </div>
  );
}

function getOverlayPopoverPosition(anchorRect: DOMRect, popoverWidth: number, popoverHeight: number) {
  const canPlaceRight = anchorRect.right + OVERLAY_POPOVER_OFFSET_PX + popoverWidth <= window.innerWidth - OVERLAY_POPOVER_MARGIN_PX;
  const placement: 'left' | 'right' = canPlaceRight ? 'right' : 'left';
  const left = placement === 'right' ? anchorRect.right + OVERLAY_POPOVER_OFFSET_PX : Math.max(OVERLAY_POPOVER_MARGIN_PX, anchorRect.left - popoverWidth - OVERLAY_POPOVER_OFFSET_PX);
  const top = Math.min(Math.max(OVERLAY_POPOVER_MARGIN_PX, anchorRect.top + anchorRect.height / 2 - popoverHeight / 2), window.innerHeight - popoverHeight - OVERLAY_POPOVER_MARGIN_PX);
  const anchorCenterY = anchorRect.top + anchorRect.height / 2;
  const arrowTop = Math.min(Math.max(OVERLAY_POPOVER_ARROW_MARGIN_PX, anchorCenterY - top - OVERLAY_POPOVER_ARROW_SIZE_PX / 2), popoverHeight - OVERLAY_POPOVER_ARROW_SIZE_PX - OVERLAY_POPOVER_ARROW_MARGIN_PX);
  return { top, left, placement, arrowTop };
}

function getOverlayPopoverStyle(position: any, arrowCssVar: string): CSSProperties {
  if (!position) return { visibility: 'hidden' };
  return { top: `${position.top}px`, left: `${position.left}px`, [arrowCssVar]: `${position.arrowTop}px` } as CSSProperties;
}
