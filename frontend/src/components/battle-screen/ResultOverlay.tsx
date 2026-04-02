import { useEffect, useLayoutEffect, useRef, useState } from 'react';

import type { BattleActionId, ResultView, ResultSeatView, Seat } from '../../types/match';
import { getFanLabel } from './fanGuide';

interface ResultOverlayProps {
  result: ResultView;
  onAction: (actionId: BattleActionId) => void;
}

export function ResultOverlay({ result, onAction }: ResultOverlayProps) {
  const [isCollapsed, setIsCollapsed] = useState(false);
  const [fanPanelHeight, setFanPanelHeight] = useState<number | null>(null);
  const [continueActionRemainingSeconds, setContinueActionRemainingSeconds] = useState<number | null>(null);
  const scorePanelRef = useRef<HTMLDivElement | null>(null);
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

  return (
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
                {result.fanTotal !== null ? <strong className="result-overlay__fan-total">{result.fanTotal} 番</strong> : null}
              </div>
              {fanMeta ? <p className="result-overlay__fan-meta">{fanMeta}</p> : null}

              {result.fanBreakdown.length > 0 ? (
                <div className="result-overlay__fan-list-viewport">
                  <div className="result-overlay__fan-list" aria-label="番型明细列表">
                    {result.fanBreakdown.map((item, index) => (
                      <div key={`${item.fanKey}-${index}`} className="result-overlay__row">
                        <span>{getFanLabel(item.fanKey)}</span>
                        <strong>{item.fanValue}</strong>
                      </div>
                    ))}
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
  );
}

const WIN_TYPE_LABELS: Record<string, string> = {
  discard: '荣和',
  self_draw: '自摸',
  draw: '流局',
};

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
