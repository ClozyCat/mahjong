import { useEffect, useLayoutEffect, useRef, useState } from 'react';
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
  } | null>(null);
  const scorePanelRef = useRef<HTMLDivElement | null>(null);
  const fanGuidePopoverRef = useRef<HTMLDivElement | null>(null);
  const activeFanGuideAnchorRef = useRef<HTMLDivElement | null>(null);
  const openFanGuideTimerRef = useRef<number | null>(null);
  const closeFanGuideTimerRef = useRef<number | null>(null);
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
      const nextPosition = getFanGuidePopoverPosition(anchorRect, popoverRect?.width ?? 336, popoverRect?.height ?? 208);

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

  useEffect(() => {
    return () => {
      clearFanGuideTimers(openFanGuideTimerRef, closeFanGuideTimerRef);
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
      setFanGuidePopoverPosition(getFanGuidePopoverPosition(rowElement.getBoundingClientRect(), 336, 208));
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
            style={
              fanGuidePopoverPosition
                ? {
                    top: `${fanGuidePopoverPosition.top}px`,
                    left: `${fanGuidePopoverPosition.left}px`,
                  }
                : { visibility: 'hidden' }
            }
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

                  return (
                    <div key={`${seat.seat}-${seat.name}`} className={rowClassName}>
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
const FAN_GUIDE_POPOVER_OFFSET_PX = 14;
const FAN_GUIDE_POPOVER_MARGIN_PX = 12;

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

function getFanGuidePopoverPosition(anchorRect: DOMRect, popoverWidth: number, popoverHeight: number) {
  const canPlaceRight = anchorRect.right + FAN_GUIDE_POPOVER_OFFSET_PX + popoverWidth <= window.innerWidth - FAN_GUIDE_POPOVER_MARGIN_PX;
  const placement: 'left' | 'right' = canPlaceRight ? 'right' : 'left';
  const left =
    placement === 'right'
      ? anchorRect.right + FAN_GUIDE_POPOVER_OFFSET_PX
      : Math.max(FAN_GUIDE_POPOVER_MARGIN_PX, anchorRect.left - popoverWidth - FAN_GUIDE_POPOVER_OFFSET_PX);
  const top = Math.min(
    Math.max(FAN_GUIDE_POPOVER_MARGIN_PX, anchorRect.top + anchorRect.height / 2 - popoverHeight / 2),
    window.innerHeight - popoverHeight - FAN_GUIDE_POPOVER_MARGIN_PX,
  );

  return { top, left, placement };
}
