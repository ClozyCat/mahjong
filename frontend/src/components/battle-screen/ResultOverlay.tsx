import type { CSSProperties } from 'react';
import { useEffect, useId, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

import type { BattleActionId, ResultView, ResultSeatView, Seat } from '../../types/match';
import { getFanGuideEntry, getFanLabel } from './fanGuide';
import { FanGuideCard } from './FanGuideCard';

interface ResultOverlayProps {
  result: ResultView;
  onAction: (actionId: BattleActionId) => void;
}

export function ResultOverlay({ result, onAction }: ResultOverlayProps) {
  const [isCollapsed, setIsCollapsed] = useState(false);
  const [fanPanelHeight, setFanPanelHeight] = useState<number | null>(null);
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
  const [seatStatsPopoverPosition, setSeatStatsPopoverPosition] = useState<{
    top: number;
    left: number;
    placement: 'left' | 'right';
    arrowTop: number;
  } | null>(null);
  const scorePanelRef = useRef<HTMLDivElement | null>(null);
  const fanGuidePopoverRef = useRef<HTMLDivElement | null>(null);
  const seatStatsPopoverRef = useRef<HTMLDivElement | null>(null);
  const activeFanGuideAnchorRef = useRef<HTMLDivElement | null>(null);
  const activeSeatStatsAnchorRef = useRef<HTMLDivElement | null>(null);
  const openFanGuideTimerRef = useRef<number | null>(null);
  const closeFanGuideTimerRef = useRef<number | null>(null);
  const openSeatStatsTimerRef = useRef<number | null>(null);
  const closeSeatStatsTimerRef = useRef<number | null>(null);
  const hasFanPanel = result.fanTotal !== null || result.fanBreakdown.length > 0;
  const winTypeLabel = result.winTypeLabel ?? (result.winType ? WIN_TYPE_LABELS[result.winType] ?? result.winType : null);
  const fanMeta = [
    winTypeLabel,
    result.winnerSeat ? `胜者 ${formatResultActor(result.winnerSeat, result.seats)}` : null,
    result.discarderSeat ? `放铳 ${formatResultActor(result.discarderSeat, result.seats)}` : null,
    result.flowerCount > 0 ? `花牌 ${result.flowerCount}` : null,
  ]
    .filter((item): item is string => Boolean(item))
    .join(' · ');

  useEffect(() => {
    setIsCollapsed(false);
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

  useEffect(() => {
    if (!hasFanPanel) {
      setFanPanelHeight(null);
    }
  }, [hasFanPanel]);

  useLayoutEffect(() => {
    if (!hasFanPanel || typeof window === 'undefined') {
      return undefined;
    }

    let animationFrameId = 0;

    const measurePanels = () => {
      const nextScorePanelHeight = scorePanelRef.current?.getBoundingClientRect().height ?? 0;

      setFanPanelHeight((currentHeight) => {
        if (nextScorePanelHeight <= 0) {
          return currentHeight;
        }

        return Math.abs((currentHeight ?? 0) - nextScorePanelHeight) < 1 ? currentHeight : nextScorePanelHeight;
      });
    };

    const requestMeasurement = () => {
      window.cancelAnimationFrame(animationFrameId);
      animationFrameId = window.requestAnimationFrame(measurePanels);
    };

    requestMeasurement();

    const resizeObserver = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(requestMeasurement);

    if (resizeObserver) {
      if (scorePanelRef.current) {
        resizeObserver.observe(scorePanelRef.current);
      }
    } else {
      window.addEventListener('resize', requestMeasurement);
    }

    return () => {
      window.cancelAnimationFrame(animationFrameId);
      resizeObserver?.disconnect();
      if (!resizeObserver) {
        window.removeEventListener('resize', requestMeasurement);
      }
    };
  }, [hasFanPanel, result.seats, fanMeta]);

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
        if (
          currentPosition &&
          Math.abs(currentPosition.top - nextPosition.top) < 1 &&
          Math.abs(currentPosition.left - nextPosition.left) < 1 &&
          currentPosition.placement === nextPosition.placement
        ) {
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
        if (
          currentPosition &&
          Math.abs(currentPosition.top - nextPosition.top) < 1 &&
          Math.abs(currentPosition.left - nextPosition.left) < 1 &&
          currentPosition.placement === nextPosition.placement
        ) {
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

  useEffect(() => {
    return () => {
      clearFanGuideTimers(openFanGuideTimerRef, closeFanGuideTimerRef);
      clearSeatStatsTimers(openSeatStatsTimerRef, closeSeatStatsTimerRef);
    };
  }, []);

  if (isCollapsed) {
    return (
      <section className="result-overlay result-overlay--collapsed" aria-label="Match settlement result">
        <button
          type="button"
          className="result-overlay__restore"
          onClick={() => setIsCollapsed(false)}
          aria-expanded="false"
        >
          展开结算面板
        </button>
      </section>
    );
  }

  function scheduleFanGuideOpen(
    rowKey: string,
    fanKey: string,
    rowElement: HTMLDivElement,
  ) {
    const nextEntry = getFanGuideEntry(fanKey);
    if (!nextEntry) {
      return;
    }

    if (closeFanGuideTimerRef.current !== null) {
      window.clearTimeout(closeFanGuideTimerRef.current);
      closeFanGuideTimerRef.current = null;
    }

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
  }

  function scheduleFanGuideClose() {
    if (openFanGuideTimerRef.current !== null) {
      window.clearTimeout(openFanGuideTimerRef.current);
      openFanGuideTimerRef.current = null;
    }

    if (closeFanGuideTimerRef.current !== null) {
      window.clearTimeout(closeFanGuideTimerRef.current);
    }

    closeFanGuideTimerRef.current = window.setTimeout(() => {
      activeFanGuideAnchorRef.current = null;
      setActiveFanGuide(null);
      setFanGuidePopoverPosition(null);
      closeFanGuideTimerRef.current = null;
    }, FAN_GUIDE_POPOVER_CLOSE_DELAY_MS);
  }

  function showSeatStatsPopover(rowKey: string, seat: ResultSeatView, rowElement: HTMLDivElement) {
    activeSeatStatsAnchorRef.current = rowElement;

    if (activeSeatStats?.rowKey === rowKey) {
      return;
    }

    setSeatStatsPopoverPosition(getOverlayPopoverPosition(rowElement.getBoundingClientRect(), 364, 276));
    setActiveSeatStats({ rowKey, seat });
  }

  function scheduleSeatStatsOpen(rowKey: string, seat: ResultSeatView, rowElement: HTMLDivElement) {
    clearOverlayPopoverCloseTimer(closeSeatStatsTimerRef);

    if (activeSeatStats?.rowKey === rowKey) {
      activeSeatStatsAnchorRef.current = rowElement;
      return;
    }

    if (openSeatStatsTimerRef.current !== null) {
      window.clearTimeout(openSeatStatsTimerRef.current);
    }

    openSeatStatsTimerRef.current = window.setTimeout(() => {
      showSeatStatsPopover(rowKey, seat, rowElement);
      openSeatStatsTimerRef.current = null;
    }, SEAT_STATS_POPOVER_DELAY_MS);
  }

  function scheduleSeatStatsClose() {
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
  }

  const fanGuidePopover =
    activeFanGuide && typeof document !== 'undefined'
      ? createPortal(
          <div
            ref={fanGuidePopoverRef}
            role="tooltip"
            aria-label={`${activeFanGuide.entry.label}番型说明`}
            className={`result-overlay__fan-tooltip result-overlay__fan-tooltip--${
              fanGuidePopoverPosition?.placement ?? 'right'
            }`.trim()}
            style={getOverlayPopoverStyle(fanGuidePopoverPosition, '--result-overlay-fan-tooltip-arrow-top')}
            onMouseEnter={() => {
              if (closeFanGuideTimerRef.current !== null) {
                window.clearTimeout(closeFanGuideTimerRef.current);
                closeFanGuideTimerRef.current = null;
              }
            }}
            onMouseLeave={() => {
              scheduleFanGuideClose();
            }}
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
            className={`result-overlay__seat-tooltip result-overlay__seat-tooltip--${
              seatStatsPopoverPosition?.placement ?? 'right'
            }`.trim()}
            style={getOverlayPopoverStyle(seatStatsPopoverPosition, '--result-overlay-seat-tooltip-arrow-top')}
            onMouseEnter={() => {
              clearOverlayPopoverCloseTimer(closeSeatStatsTimerRef);
            }}
            onMouseLeave={() => {
              scheduleSeatStatsClose();
            }}
          >
            <SeatStatsTooltip seat={activeSeatStats.seat} />
          </div>,
          document.body,
        )
      : null;

  return (
    <>
      <section className="result-overlay" aria-label="Match settlement result">
        <div className="result-overlay__card">
          <div className="result-overlay__header">
            <div className="result-overlay__heading">
              <span className="result-overlay__eyebrow">结算面板</span>
              <h2>{result.title}</h2>
            </div>
            <button
              type="button"
              className="result-overlay__collapse"
              onClick={() => setIsCollapsed(true)}
              aria-expanded="true"
            >
              收起结算面板
            </button>
          </div>
          <p className="result-overlay__summary">{result.summary}</p>

          <div className={`result-overlay__columns${hasFanPanel ? '' : ' result-overlay__columns--score-only'}`}>
            {hasFanPanel ? (
              <div
                className="result-overlay__fan-panel"
                style={fanPanelHeight ? { height: `${fanPanelHeight}px` } : undefined}
              >
                <div className="result-overlay__section-head">
                  <span className="result-overlay__section-label">番型明细</span>
                  {result.fanTotal !== null ? (
                    <strong className="result-overlay__fan-total">{result.fanTotal} 番</strong>
                  ) : null}
                </div>
                {fanMeta ? <p className="result-overlay__fan-meta">{fanMeta}</p> : null}

                {result.fanBreakdown.length > 0 ? (
                  <div className="result-overlay__fan-list-viewport">
                    <div className="result-overlay__fan-list" aria-label="番型明细列表">
                      {result.fanBreakdown.map((item, index) => {
                        const rowKey = `${item.fanKey}-${index}`;
                        const hasGuideEntry = Boolean(getFanGuideEntry(item.fanKey));

                        return (
                          <div
                            key={rowKey}
                            className={`result-overlay__row ${
                              hasGuideEntry ? 'result-overlay__row--interactive' : ''
                            }`.trim()}
                            onMouseEnter={(event) =>
                              scheduleFanGuideOpen(rowKey, item.fanKey, event.currentTarget as HTMLDivElement)
                            }
                            onMouseLeave={() => {
                              if (activeFanGuide?.rowKey === rowKey) {
                                scheduleFanGuideClose();
                                return;
                              }

                              if (openFanGuideTimerRef.current !== null) {
                                window.clearTimeout(openFanGuideTimerRef.current);
                                openFanGuideTimerRef.current = null;
                              }
                            }}
                          >
                            <span>{getFanLabel(item.fanKey)}</span>
                            <strong>{item.fanValue}</strong>
                          </div>
                        );
                      })}
                    </div>
                  </div>
                ) : null}
              </div>
            ) : null}

            <div
              ref={scorePanelRef}
              className={`result-overlay__score-panel${hasFanPanel ? '' : ' result-overlay__score-panel--full'}`}
            >
              <div className="result-overlay__section-head">
                <span className="result-overlay__section-label">玩家分数</span>
                <span className="result-overlay__score-hint">本局结算后总分</span>
              </div>
              <div className="result-overlay__seat-list">
                {result.seats.map((seat) => {
                  const deltaClassName =
                    seat.delta === null
                      ? 'result-overlay__seat-delta result-overlay__seat-delta--neutral'
                      : seat.delta > 0
                        ? 'result-overlay__seat-delta result-overlay__seat-delta--positive'
                        : seat.delta < 0
                          ? 'result-overlay__seat-delta result-overlay__seat-delta--negative'
                          : 'result-overlay__seat-delta result-overlay__seat-delta--neutral';

                  const rowClassName =
                    seat.delta !== null && seat.delta > 0
                      ? 'result-overlay__seat-row result-overlay__seat-row--positive'
                      : seat.delta !== null && seat.delta < 0
                        ? 'result-overlay__seat-row result-overlay__seat-row--negative'
                        : 'result-overlay__seat-row result-overlay__seat-row--neutral';
                  const rowKey = `${seat.seat}-${seat.name}`;
                  const hasSeatStats = Boolean(seat.stats && seat.stats.scoreHistory.length > 0);

                  return (
                    <div
                      key={rowKey}
                      className={`${rowClassName}${hasSeatStats ? ' result-overlay__seat-row--interactive' : ''}`}
                      tabIndex={hasSeatStats ? 0 : undefined}
                      onMouseEnter={(event) => {
                        if (!hasSeatStats) {
                          return;
                        }
                        scheduleSeatStatsOpen(rowKey, seat, event.currentTarget as HTMLDivElement);
                      }}
                      onMouseLeave={() => {
                        if (!hasSeatStats) {
                          return;
                        }
                        scheduleSeatStatsClose();
                      }}
                      onFocus={(event) => {
                        if (!hasSeatStats) {
                          return;
                        }
                        if (openSeatStatsTimerRef.current !== null) {
                          window.clearTimeout(openSeatStatsTimerRef.current);
                          openSeatStatsTimerRef.current = null;
                        }
                        clearOverlayPopoverCloseTimer(closeSeatStatsTimerRef);
                        showSeatStatsPopover(rowKey, seat, event.currentTarget as HTMLDivElement);
                      }}
                      onBlur={() => {
                        if (!hasSeatStats) {
                          return;
                        }
                        scheduleSeatStatsClose();
                      }}
                    >
                      <div className="result-overlay__seat-main">
                        <span className="result-overlay__seat-name">{seat.name}</span>
                        <span className="result-overlay__seat-tag">{getRelativeSeatLabel(seat.seat)}</span>
                      </div>
                      <strong className="result-overlay__seat-score">{seat.score}</strong>
                      <span className={deltaClassName}>
                        {seat.delta === null ? '总分' : `${seat.delta > 0 ? '+' : ''}${seat.delta}`}
                      </span>
                    </div>
                  );
                })}
              </div>
            </div>
          </div>

          {result.continueAction ? (
            <div className="result-overlay__actions">
              <button
                type="button"
                disabled={!result.continueAction.enabled}
                onClick={() => onAction(result.continueAction!.id)}
              >
                {continueActionRemainingSeconds !== null
                  ? `${continueActionRemainingSeconds}s后自动推进`
                  : result.continueAction.label}
              </button>
            </div>
          ) : null}
        </div>
      </section>
      {fanGuidePopover}
      {seatStatsPopover}
    </>
  );
}

const WIN_TYPE_LABELS: Record<string, string> = {
  discard: '荣和',
  self_draw: '自摸',
  draw: '流局',
};
const FAN_GUIDE_POPOVER_DELAY_MS = 500;
const FAN_GUIDE_POPOVER_CLOSE_DELAY_MS = 120;
const SEAT_STATS_POPOVER_DELAY_MS = 500;
const SEAT_STATS_POPOVER_CLOSE_DELAY_MS = 90;
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

function formatResultActor(seat: Seat, seats: ResultSeatView[]) {
  const relativeSeatLabel = getRelativeSeatLabel(seat);
  const seatView = seats.find((item) => item.seat === seat);

  if (!seatView?.name) {
    return relativeSeatLabel;
  }

  return `${seatView.name}（${relativeSeatLabel}）`;
}

function clearFanGuideTimers(
  openFanGuideTimerRef: React.MutableRefObject<number | null>,
  closeFanGuideTimerRef: React.MutableRefObject<number | null>,
) {
  if (openFanGuideTimerRef.current !== null) {
    window.clearTimeout(openFanGuideTimerRef.current);
    openFanGuideTimerRef.current = null;
  }

  if (closeFanGuideTimerRef.current !== null) {
    window.clearTimeout(closeFanGuideTimerRef.current);
    closeFanGuideTimerRef.current = null;
  }
}

function clearOverlayPopoverCloseTimer(timerRef: React.MutableRefObject<number | null>) {
  if (timerRef.current !== null) {
    window.clearTimeout(timerRef.current);
    timerRef.current = null;
  }
}

function clearSeatStatsTimers(
  openSeatStatsTimerRef: React.MutableRefObject<number | null>,
  closeSeatStatsTimerRef: React.MutableRefObject<number | null>,
) {
  if (openSeatStatsTimerRef.current !== null) {
    window.clearTimeout(openSeatStatsTimerRef.current);
    openSeatStatsTimerRef.current = null;
  }

  clearOverlayPopoverCloseTimer(closeSeatStatsTimerRef);
}

function SeatStatsTooltip({ seat }: { seat: ResultSeatView }) {
  if (!seat.stats) {
    return null;
  }

  const completedRoundCount = seat.stats.completedRoundCount;
  const winRateLabel = formatWinRate(seat.stats.winRate);
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
          <strong>{winRateLabel}</strong>
        </div>
        <div className="result-overlay__seat-tooltip-metric">
          <span>战绩</span>
          <strong>
            {seat.stats.winCount}/{completedRoundCount || 0}
          </strong>
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
  const paddingLeft = 16;
  const paddingRight = 16;
  const paddingTop = 16;
  const paddingBottom = 22;
  const minScore = Math.min(...history);
  const maxScore = Math.max(...history);
  const range = maxScore - minScore;
  const normalizedRange = range === 0 ? Math.max(1, Math.abs(maxScore) || 1) : range;
  const innerWidth = width - paddingLeft - paddingRight;
  const innerHeight = height - paddingTop - paddingBottom;
  const points = history.map((score, index) => {
    const x = paddingLeft + (history.length === 1 ? innerWidth / 2 : (innerWidth * index) / (history.length - 1));
    const y =
      paddingTop +
      innerHeight -
      ((score - minScore + (range === 0 ? normalizedRange / 2 : 0)) / (range === 0 ? normalizedRange * 2 : normalizedRange)) *
        innerHeight;

    return {
      x,
      y,
    };
  });
  const polylinePoints = points.map((point) => `${point.x},${point.y}`).join(' ');
  const areaPath = points.length > 0
    ? [
        `M ${points[0].x} ${height - paddingBottom}`,
        ...points.map((point) => `L ${point.x} ${point.y}`),
        `L ${points.at(-1)?.x ?? paddingLeft} ${height - paddingBottom}`,
        'Z',
      ].join(' ')
    : '';

  return (
    <div className="result-overlay__seat-tooltip-chart">
      <div className="result-overlay__seat-tooltip-chart-meta" aria-hidden="true">
        <span>{maxScore.toLocaleString()}</span>
        <span>{minScore.toLocaleString()}</span>
      </div>
      <svg
        viewBox={`0 0 ${width} ${height}`}
        className="result-overlay__seat-tooltip-svg"
        role="img"
        aria-label={`${seatName} 本牌局战绩折线图`}
      >
        <defs>
          <linearGradient id={`${chartId}-stroke`} x1="0%" y1="0%" x2="100%" y2="0%">
            <stop offset="0%" stopColor="color-mix(in srgb, var(--accent) 88%, white)" />
            <stop offset="100%" stopColor="color-mix(in srgb, var(--accent-2) 86%, white)" />
          </linearGradient>
          <linearGradient id={`${chartId}-fill`} x1="0%" y1="0%" x2="0%" y2="100%">
            <stop offset="0%" stopColor="color-mix(in srgb, var(--accent) 24%, transparent)" />
            <stop offset="100%" stopColor="color-mix(in srgb, var(--accent-2) 2%, transparent)" />
          </linearGradient>
        </defs>
        <line
          x1={paddingLeft}
          y1={height - paddingBottom}
          x2={width - paddingRight}
          y2={height - paddingBottom}
          className="result-overlay__seat-tooltip-axis"
        />
        {areaPath ? <path d={areaPath} fill={`url(#${chartId}-fill)`} /> : null}
        <polyline
          fill="none"
          stroke={`url(#${chartId}-stroke)`}
          strokeWidth="3"
          strokeLinejoin="round"
          strokeLinecap="round"
          points={polylinePoints}
        />
        {points.map((point, index) => (
          <circle
            key={`${point.x}-${point.y}-${index}`}
            cx={point.x}
            cy={point.y}
            r={index === points.length - 1 ? 4.5 : 3.2}
            className="result-overlay__seat-tooltip-point"
          />
        ))}
      </svg>
      <div className="result-overlay__seat-tooltip-axis-labels" aria-hidden="true">
        <span>开局</span>
        <span>{history.length > 1 ? `第 ${history.length - 1} 局` : '当前'}</span>
      </div>
    </div>
  );
}

function formatWinRate(winRate: number) {
  return `${(Math.round(winRate * 1000) / 10).toFixed(1).replace(/\.0$/, '')}%`;
}

function getOverlayPopoverPosition(anchorRect: DOMRect, popoverWidth: number, popoverHeight: number) {
  const canPlaceRight = anchorRect.right + OVERLAY_POPOVER_OFFSET_PX + popoverWidth <= window.innerWidth - OVERLAY_POPOVER_MARGIN_PX;
  const placement: 'left' | 'right' = canPlaceRight ? 'right' : 'left';
  const left =
    placement === 'right'
      ? anchorRect.right + OVERLAY_POPOVER_OFFSET_PX
      : Math.max(OVERLAY_POPOVER_MARGIN_PX, anchorRect.left - popoverWidth - OVERLAY_POPOVER_OFFSET_PX);
  const top = Math.min(
    Math.max(OVERLAY_POPOVER_MARGIN_PX, anchorRect.top + anchorRect.height / 2 - popoverHeight / 2),
    window.innerHeight - popoverHeight - OVERLAY_POPOVER_MARGIN_PX,
  );
  const anchorCenterY = anchorRect.top + anchorRect.height / 2;
  const arrowTop = Math.min(
    Math.max(
      OVERLAY_POPOVER_ARROW_MARGIN_PX,
      anchorCenterY - top - OVERLAY_POPOVER_ARROW_SIZE_PX / 2,
    ),
    popoverHeight - OVERLAY_POPOVER_ARROW_SIZE_PX - OVERLAY_POPOVER_ARROW_MARGIN_PX,
  );

  return { top, left, placement, arrowTop };
}

function getOverlayPopoverStyle(
  position: {
    top: number;
    left: number;
    placement: 'left' | 'right';
    arrowTop: number;
  } | null,
  arrowCssVariable:
    | '--result-overlay-fan-tooltip-arrow-top'
    | '--result-overlay-seat-tooltip-arrow-top',
): CSSProperties {
  if (!position) {
    return { visibility: 'hidden' };
  }

  return {
    top: `${position.top}px`,
    left: `${position.left}px`,
    [arrowCssVariable]: `${position.arrowTop}px`,
  } as CSSProperties;
}
