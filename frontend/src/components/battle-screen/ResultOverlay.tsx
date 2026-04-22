import type { CSSProperties } from 'react';
import { memo, useEffect, useId, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

import type { BattleActionId, ResultView, ResultSeatView, Seat, ResultPageView } from '../../types/match';
import { getFanGuideEntry, getFanLabel } from './fanGuide';
import { FanGuideCard, getFanColor } from './FanGuideCard';
import { MahjongTile } from './MahjongTile';

interface ResultOverlayProps {
  result: ResultView;
  settlementHands?: Partial<Record<Seat, string[]>> | null;
  onAction: (actionId: BattleActionId) => void;
}

export function ResultOverlay({ result, settlementHands, onAction }: ResultOverlayProps) {
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
  const [activeSeatStats, setActiveSeatStats] = useState<{
    rowKey: string;
    seat: ResultSeatView;
  } | null>(null);
  const [dynamicScale, setDynamicScale] = useState(1);
  const [seatStatsPopoverPosition, setSeatStatsPopoverPosition] = useState<{
    top: number;
    left: number;
    placement: 'left' | 'right';
    arrowTop: number;
  } | null>(null);

  const cardRef = useRef<HTMLDivElement | null>(null);
  const fanGuidePopoverRef = useRef<HTMLDivElement | null>(null);
  const seatStatsPopoverRef = useRef<HTMLDivElement | null>(null);
  const activeFanGuideAnchorRef = useRef<HTMLDivElement | null>(null);
  const activeSeatStatsAnchorRef = useRef<HTMLDivElement | null>(null);
  const openFanGuideTimerRef = useRef<number | null>(null);
  const closeFanGuideTimerRef = useRef<number | null>(null);
  const openSeatStatsTimerRef = useRef<number | null>(null);
  const closeSeatStatsTimerRef = useRef<number | null>(null);

  const resultPages = getResultPages(result);
  const activeResultPage = resultPages[activeResultPageIndex] ?? null;
  const hasFanPanel = activeResultPage?.fanTotal !== null || (activeResultPage?.fanBreakdown.length ?? 0) > 0;

  useEffect(() => {
    setIsCollapsed(false);
    setActiveResultPageIndex(0);
  }, [result]);

  useEffect(() => {
    clearFanGuideTimers(openFanGuideTimerRef, closeFanGuideTimerRef);
    activeFanGuideAnchorRef.current = null;
    setActiveFanGuide(null);
    setFanGuidePopoverPosition(null);
    clearSeatStatsTimers(openSeatStatsTimerRef, closeSeatStatsTimerRef);
    activeSeatStatsAnchorRef.current = null;
    setActiveSeatStats(null);
    setSeatStatsPopoverPosition(null);
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
    if (!activeSeatStats || typeof window === 'undefined') {
      return undefined;
    }

    let animationFrameId = 0;
    const updatePosition = () => {
      const anchorRect = activeSeatStatsAnchorRef.current?.getBoundingClientRect();
      if (!anchorRect) {
        activeSeatStatsAnchorRef.current = null;
        setActiveSeatStats(null);
        setSeatStatsPopoverPosition(null);
        return;
      }
      const popoverRect = seatStatsPopoverRef.current?.getBoundingClientRect();
      const nextPosition = getOverlayPopoverPosition(anchorRect, popoverRect?.width ?? 364, popoverRect?.height ?? 276);
      setSeatStatsPopoverPosition((currentPosition) => {
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
  }, [activeSeatStats]);

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
      const nextScale = Math.max(0.65, Math.min(availableWidth / baseWidth, availableHeight / baseHeight, 1));

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

  const scheduleFanGuideOpen = (rowKey: string, fanKey: string, rowElement: HTMLDivElement) => {
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

  const scheduleSeatStatsOpen = (rowKey: string, seat: ResultSeatView, rowElement: HTMLDivElement) => {
    clearOverlayPopoverCloseTimer(closeSeatStatsTimerRef);

    if (activeSeatStats?.rowKey === rowKey) {
      activeSeatStatsAnchorRef.current = rowElement;
      return;
    }

    if (openSeatStatsTimerRef.current !== null) {
      window.clearTimeout(openSeatStatsTimerRef.current);
    }

    openSeatStatsTimerRef.current = window.setTimeout(() => {
      activeSeatStatsAnchorRef.current = rowElement;
      setSeatStatsPopoverPosition(getOverlayPopoverPosition(rowElement.getBoundingClientRect(), 364, 276));
      setActiveSeatStats({ rowKey, seat });
      openSeatStatsTimerRef.current = null;
    }, SEAT_STATS_POPOVER_DELAY_MS);
  };

  const scheduleSeatStatsClose = () => {
    if (openSeatStatsTimerRef.current !== null) {
      window.clearTimeout(openSeatStatsTimerRef.current);
      openSeatStatsTimerRef.current = null;
    }

    clearOverlayPopoverCloseTimer(closeSeatStatsTimerRef);
    closeSeatStatsTimerRef.current = window.setTimeout(() => {
      activeSeatStatsAnchorRef.current = null;
      setActiveSeatStats(null);
      setSeatStatsPopoverPosition(null);
      closeSeatStatsTimerRef.current = null;
    }, SEAT_STATS_POPOVER_CLOSE_DELAY_MS);
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

  const seatStatsPopover =
    activeSeatStats?.seat.stats && typeof document !== 'undefined'
      ? createPortal(
          <div
            ref={seatStatsPopoverRef}
            role="tooltip"
            aria-label={`${activeSeatStats.seat.name} 战绩统计`}
            className={`result-overlay__seat-tooltip result-overlay__seat-tooltip--${seatStatsPopoverPosition?.placement ?? 'right'}`.trim()}
            style={getOverlayPopoverStyle(seatStatsPopoverPosition, '--result-overlay-seat-tooltip-arrow-top')}
            onMouseEnter={() => clearOverlayPopoverCloseTimer(closeSeatStatsTimerRef)}
            onMouseLeave={scheduleSeatStatsClose}
          >
            <SeatStatsTooltip seat={activeSeatStats.seat} />
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
      className="result-overlay"
      style={{ '--result-overlay-dynamic-scale': dynamicScale } as CSSProperties}
    >
      <div ref={cardRef} className="result-overlay__card">
        <div className={`result-overlay__columns${hasFanPanel ? '' : ' result-overlay__columns--score-only'}`}>
          {hasFanPanel && (
            <FanBreakdownSection
              result={result}
              pages={resultPages}
              activeIndex={activeResultPageIndex}
              onPageChange={setActiveResultPageIndex}
              onHoverFan={scheduleFanGuideOpen}
              onLeaveFan={scheduleFanGuideClose}
            />
          )}

          <ScoreSection
            seats={result.seats}
            activeIndex={activeScorePageIndex}
            hasFanPanel={hasFanPanel}
            settlementHands={settlementHands}
            winnerSeat={result.winnerSeat}
            winType={result.winType}
            onPageChange={setActiveScorePageIndex}
            onHoverSeat={scheduleSeatStatsOpen}
            onLeaveSeat={scheduleSeatStatsClose}
          />
        </div>

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
      {seatStatsPopover}
    </section>
  );

  if (typeof document === 'undefined') return null;
  return createPortal(content, document.body);
}

/**
 * Sub-components for better organization and performance
 */

const FanBreakdownSection = memo(({
  result,
  pages,
  activeIndex,
  onPageChange,
  onHoverFan,
  onLeaveFan
}: {
  result: ResultView;
  pages: ResultPageView[];
  activeIndex: number;
  onPageChange: (index: number | ((curr: number) => number)) => void;
  onHoverFan: (key: string, fanKey: string, el: HTMLDivElement) => void;
  onLeaveFan: () => void;
}) => {
  const activePage = pages[activeIndex];
  const winTypeLabel = activePage?.winTypeLabel ?? (activePage?.winType ? WIN_TYPE_LABELS[activePage.winType] : null);
  
  const metaItems = [
    winTypeLabel,
    activePage?.winnerSeat ? `胜者 ${formatResultActor(activePage.winnerSeat, result.seats)}` : null,
    activePage?.discarderSeat ? `放铳 ${formatResultActor(activePage.discarderSeat, result.seats)}` : null,
    activePage && activePage.flowerCount > 0 ? `花牌 ${activePage.flowerCount}` : null,
  ].filter(Boolean);

  return (
    <div className="result-overlay__fan-panel">
      <div className="result-overlay__section-head">
        <span className="result-overlay__section-label">番型明细</span>
        {activePage?.fanTotal !== null && (
          <strong className="result-overlay__fan-total">{activePage.fanTotal} 番</strong>
        )}
      </div>
      
      {metaItems.length > 0 && (
        <p className="result-overlay__fan-meta">{metaItems.join(' · ')}</p>
      )}

      {pages.length > 1 && (
        <div className="result-overlay__pagination">
          <button
            type="button"
            className="result-overlay__pagination-button"
            onClick={() => onPageChange(curr => (curr === 0 ? pages.length - 1 : curr - 1))}
          >
            上一位
          </button>
          <span className="result-overlay__pagination-status">{activeIndex + 1} / {pages.length}</span>
          <button
            type="button"
            className="result-overlay__pagination-button"
            onClick={() => onPageChange(curr => (curr === pages.length - 1 ? 0 : curr + 1))}
          >
            下一位
          </button>
        </div>
      )}

      <div className="result-overlay__fan-list-viewport">
        <div className="result-overlay__fan-list">
          {activePage?.fanBreakdown.map((item, index) => (
            <FanRow
              key={`${activeIndex}-${item.fanKey}-${index}`}
              item={item}
              rowKey={`${activeIndex}-${item.fanKey}-${index}`}
              onHover={onHoverFan}
              onLeave={onLeaveFan}
            />
          ))}
        </div>
      </div>
    </div>
  );
});

const FanRow = memo(({
  item,
  rowKey,
  onHover,
  onLeave
}: {
  item: { fanKey: string; fanValue: number };
  rowKey: string;
  onHover: (key: string, fanKey: string, el: HTMLDivElement) => void;
  onLeave: () => void;
}) => {
  const hasGuide = Boolean(getFanGuideEntry(item.fanKey));
  const fanBg = getFanColor(item.fanValue);

  return (
    <div
      className={`result-overlay__row ${hasGuide ? 'result-overlay__row--interactive' : ''}`}
      onMouseEnter={e => onHover(rowKey, item.fanKey, e.currentTarget)}
      onMouseLeave={onLeave}
    >
      <span>{getFanLabel(item.fanKey)}</span>
      <div className="result-overlay__fan-pill" style={{ '--fan-bg': fanBg } as CSSProperties}>
        <strong>{item.fanValue}</strong>
        <span className="result-overlay__pill-unit">番</span>
      </div>
    </div>
  );
});

const ScoreSection = memo(({
  seats,
  activeIndex,
  hasFanPanel,
  settlementHands,
  winnerSeat,
  winType,
  onPageChange,
  onHoverSeat,
  onLeaveSeat
}: {
  seats: ResultSeatView[];
  activeIndex: number;
  hasFanPanel: boolean;
  settlementHands?: Partial<Record<Seat, string[]>> | null;
  winnerSeat: Seat | null;
  winType: string | null;
  onPageChange: (index: number | ((curr: number) => number)) => void;
  onHoverSeat: (key: string, seat: ResultSeatView, el: HTMLDivElement) => void;
  onLeaveSeat: () => void;
}) => {
  return (
    <div className={`result-overlay__score-panel${hasFanPanel ? '' : ' result-overlay__score-panel--full'}`}>
      <div className="result-overlay__section-head">
        <span className="result-overlay__section-label">玩家分数</span>
        <div className="result-overlay__score-pagination">
          <button
            type="button"
            className="result-overlay__pagination-button"
            onClick={() => onPageChange(curr => (curr === 0 ? seats.length - 1 : curr - 1))}
          >
            上一个
          </button>
          <span className="result-overlay__pagination-status">{activeIndex + 1} / {seats.length}</span>
          <button
            type="button"
            className="result-overlay__pagination-button"
            onClick={() => onPageChange(curr => (curr === seats.length - 1 ? 0 : curr + 1))}
          >
            下一个
          </button>
        </div>
        <span className="result-overlay__score-hint">总分</span>
      </div>
      <div className="result-overlay__seat-list">
        {seats.map((seat, index) => (
          <SeatRow
            key={seat.seat}
            seat={seat}
            isActive={index === activeIndex}
            hand={settlementHands?.[seat.seat]}
            isWinner={winnerSeat === seat.seat}
            winType={winType}
            onHover={onHoverSeat}
            onLeave={onLeaveSeat}
          />
        ))}
      </div>
    </div>
  );
});

const SeatRow = memo(({
  seat,
  isActive,
  hand,
  isWinner,
  winType,
  onHover,
  onLeave
}: {
  seat: ResultSeatView;
  isActive: boolean;
  hand?: string[];
  isWinner: boolean;
  winType: string | null;
  onHover: (key: string, seat: ResultSeatView, el: HTMLDivElement) => void;
  onLeave: () => void;
}) => {
  const hasStats = Boolean(seat.stats && seat.stats.scoreHistory.length > 0);
  const deltaClass = !seat.delta ? 'neutral' : seat.delta > 0 ? 'positive' : 'negative';
  const rowClass = seat.delta && seat.delta > 0 ? 'positive' : seat.delta && seat.delta < 0 ? 'negative' : 'neutral';
  const rowKey = `${seat.seat}-${seat.name}`;

  return (
    <div
      className={`result-overlay__seat-row result-overlay__seat-row--${rowClass} ${hasStats ? 'result-overlay__seat-row--interactive' : ''} ${isActive ? 'result-overlay__seat-row--active' : 'result-overlay__seat-row--inactive'}`}
      onMouseEnter={e => hasStats && onHover(rowKey, seat, e.currentTarget)}
      onMouseLeave={() => hasStats && onLeave()}
    >
      <div className="result-overlay__seat-main">
        <div className="result-overlay__seat-info">
          <span className="result-overlay__seat-name">{seat.name}</span>
          <span className="result-overlay__seat-tag">{getRelativeSeatLabel(seat.seat)}</span>
        </div>
        <strong className="result-overlay__seat-score">{seat.score}</strong>
        <span className={`result-overlay__seat-delta result-overlay__seat-delta--${deltaClass}`}>
          {seat.delta === null ? '总分' : `${seat.delta > 0 ? '+' : ''}${seat.delta}`}
        </span>
      </div>
      {hand && (
        <div className="result-overlay__seat-hand">
          {hand.map((tile, i) => (
            <MahjongTile
              key={`${seat.seat}-tile-${i}`}
              code={tile}
              variant="discard"
              isLastDiscard={isWinner && winType === 'discard' && i === hand.length - 1}
              className="result-overlay__seat-hand-tile"
            />
          ))}
        </div>
      )}
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
  left: '左家',
  top: '对家',
  right: '右家',
};

function getRelativeSeatLabel(seat: Seat) {
  return RELATIVE_SEAT_LABELS[seat];
}

function getResultPages(result: ResultView): ResultPageView[] {
  if (Array.isArray(result.pages) && result.pages.length > 0) return result.pages;
  return [{
    fanTotal: result.fanTotal,
    winnerSeat: result.winnerSeat,
    discarderSeat: result.discarderSeat,
    winType: result.winType,
    winTypeLabel: result.winTypeLabel,
    flowerCount: result.flowerCount,
    fanBreakdown: result.fanBreakdown,
  }];
}

function formatResultActor(seat: Seat, seats: ResultSeatView[]) {
  const label = getRelativeSeatLabel(seat);
  const view = seats.find(s => s.seat === seat);
  return view?.name ? `${view.name}（${label}）` : label;
}

function clearFanGuideTimers(open: React.MutableRefObject<number | null>, close: React.MutableRefObject<number | null>) {
  if (open.current !== null) { window.clearTimeout(open.current); open.current = null; }
  if (close.current !== null) { window.clearTimeout(close.current); close.current = null; }
}

function clearSeatStatsTimers(open: React.MutableRefObject<number | null>, close: React.MutableRefObject<number | null>) {
  if (open.current !== null) { window.clearTimeout(open.current); open.current = null; }
  if (close.current !== null) { window.clearTimeout(close.current); close.current = null; }
}

function clearOverlayPopoverCloseTimer(timerRef: React.MutableRefObject<number | null>) {
  if (timerRef.current !== null) { window.clearTimeout(timerRef.current); timerRef.current = null; }
}

function SeatStatsTooltip({ seat }: { seat: ResultSeatView }) {
  if (!seat.stats) return null;
  const completed = seat.stats.completedRoundCount;
  const winRate = `${(Math.round(seat.stats.winRate * 1000) / 10).toFixed(1).replace(/\.0$/, '')}%`;
  const latestScore = seat.stats.scoreHistory.at(-1) ?? seat.score;

  return (
    <article className="result-overlay__seat-tooltip-card">
      <div className="result-overlay__seat-tooltip-head">
        <span className="result-overlay__seat-tooltip-eyebrow">本牌局统计</span>
        <strong>{seat.name}</strong>
        <span className="result-overlay__seat-tooltip-seat">{getRelativeSeatLabel(seat.seat)}</span>
      </div>
      <div className="result-overlay__seat-tooltip-metrics">
        <div className="result-overlay__seat-tooltip-metric">
          <span>胜率</span>
          <strong>{winRate}</strong>
        </div>
        <div className="result-overlay__seat-tooltip-metric">
          <span>战绩</span>
          <strong>{seat.stats.winCount}/{completed || 0}</strong>
        </div>
        <div className="result-overlay__seat-tooltip-metric">
          <span>当前总分</span>
          <strong>{latestScore.toLocaleString()}</strong>
        </div>
      </div>
      <ScoreTrendChart history={seat.stats.scoreHistory} seatName={seat.name} />
    </article>
  );
}

function ScoreTrendChart({ history, seatName }: { history: number[]; seatName: string }) {
  const chartId = useId();
  const width = 308;
  const height = 146;
  const padding = { l: 16, r: 16, t: 16, b: 22 };
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
  const areaPath = points.length > 0
    ? `M ${points[0].x} ${height - padding.b} ${points.map(p => `L ${p.x} ${p.y}`).join(' ')} L ${points.at(-1)?.x} ${height - padding.b} Z`
    : '';

  return (
    <div className="result-overlay__seat-tooltip-chart">
      <div className="result-overlay__seat-tooltip-chart-meta">
        <span>{max.toLocaleString()}</span>
        <span>{min.toLocaleString()}</span>
      </div>
      <svg viewBox={`0 0 ${width} ${height}`} className="result-overlay__seat-tooltip-svg">
        <defs>
          <linearGradient id={`${chartId}-fill`} x1="0%" y1="0%" x2="0%" y2="100%">
            <stop offset="0%" stopColor="color-mix(in srgb, var(--accent) 12%, transparent)" />
            <stop offset="100%" stopColor="transparent" />
          </linearGradient>
        </defs>
        <line x1={padding.l} y1={height - padding.b} x2={width - padding.r} y2={height - padding.b} className="result-overlay__seat-tooltip-axis" />
        {areaPath && <path d={areaPath} fill={`url(#${chartId}-fill)`} />}
        <polyline fill="none" stroke="var(--accent)" strokeWidth="1.5" strokeLinejoin="round" strokeLinecap="round" points={polylinePoints} />
        {points.map((p, i) => (
          <circle key={i} cx={p.x} cy={p.y} r={i === points.length - 1 ? 3 : 2} className="result-overlay__seat-tooltip-point" />
        ))}
      </svg>
      <div className="result-overlay__seat-tooltip-axis-labels">
        <span>开局</span>
        <span>{history.length > 1 ? `第 ${history.length - 1} 局` : '当前'}</span>
      </div>
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
