import { useEffect, useRef, useState, type CSSProperties } from 'react';
import { createPortal } from 'react-dom';

import type { ResultSeatView, ResultView, Seat } from '../../types/match';
import { getFanGuideEntry, getFanLabel } from './fanGuide';
import { getFanColor } from './FanGuideCard';

interface DramaticRevealOverlayProps {
  result: ResultView;
  onComplete: () => void;
}

type Phase = 'entrance' | 'revealing-fans' | 'revealed-total' | 'exit';

const ENTRANCE_MS = 800;
const REVEAL_INTERVAL_MS = 250;
const TOTAL_REVEAL_DELAY_MS = 400;
const FALLBACK_SAFETY_MS = 60000;

export function DramaticRevealOverlay({ result, onComplete }: DramaticRevealOverlayProps) {
  const [phase, setPhase] = useState<Phase>('entrance');
  const [revealedCount, setRevealedCount] = useState(0);
  const completeFiredRef = useRef(false);

  const isSelfDraw = result.winType === 'self_draw';
  const isDiscard = result.winType === 'discard';

  // Resolve winner/discarder info from pages (multi-win) or top-level
  const resultPages = getRevealPages(result);

  // Find the page with the highest fan count
  let primaryPage = resultPages[0] ?? null;
  if (resultPages.length > 1) {
    let maxFanTotal = -1;
    for (const page of resultPages) {
      const fanTotal = calculateDetailedFanTotal(page.fanBreakdown) ?? page.fanTotal ?? 1;
      if (fanTotal > maxFanTotal) {
        maxFanTotal = fanTotal;
        primaryPage = page;
      }
    }
  }

  const displayWinnerSeat = primaryPage?.winnerSeat ?? result.winnerSeat;
  const displayDiscarderSeat = primaryPage?.discarderSeat ?? result.discarderSeat;
  const displayFanBreakdown = primaryPage?.fanBreakdown ?? result.fanBreakdown;
  const hasMultiWin = resultPages.length > 1;
  const finalFanTotal = calculateDetailedFanTotal(displayFanBreakdown) ?? result.fanTotal ?? 1;

  // Resolve winner names: if multi-win, join names of all winning seats with '、'
  let winnerName = null;
  if (hasMultiWin) {
    const winnerNames = resultPages
      .map((page) => page.winnerSeat ? (findPlayerName(page.winnerSeat, result.seats) ?? null) : null)
      .filter((name): name is string => Boolean(name));
    winnerName = winnerNames.length > 0 ? winnerNames.join('、') : null;
  } else {
    winnerName = displayWinnerSeat
      ? findPlayerName(displayWinnerSeat, result.seats) ?? null
      : null;
  }

  const discarderName = displayDiscarderSeat
    ? findPlayerName(displayDiscarderSeat, result.seats) ?? null
    : null;

  const winTitle = isSelfDraw ? '自摸！' : '荣和！';

  // Phase sequencing
  useEffect(() => {
    // 1. Entrance -> Start revealing fans
    const tEntrance = setTimeout(() => {
      setPhase('revealing-fans');
    }, ENTRANCE_MS);

    // Safety fallback auto-dismiss
    const tSafety = setTimeout(() => {
      if (!completeFiredRef.current) {
        completeFiredRef.current = true;
        setPhase('exit');
        setTimeout(onComplete, 300);
      }
    }, FALLBACK_SAFETY_MS);

    return () => {
      clearTimeout(tEntrance);
      clearTimeout(tSafety);
    };
  }, [onComplete]);

  // Handle fan badges sequential reveal
  useEffect(() => {
    if (phase !== 'revealing-fans') return;

    if (displayFanBreakdown.length === 0) {
      // If no fans, transition to revealed-total immediately
      const t = setTimeout(() => {
        setPhase('revealed-total');
      }, TOTAL_REVEAL_DELAY_MS);
      return () => clearTimeout(t);
    }

    if (revealedCount < displayFanBreakdown.length) {
      const t = setTimeout(() => {
        setRevealedCount((prev) => prev + 1);
      }, REVEAL_INTERVAL_MS);
      return () => clearTimeout(t);
    } else {
      // All fans revealed, wait a bit then show total multiplier
      const t = setTimeout(() => {
        setPhase('revealed-total');
      }, TOTAL_REVEAL_DELAY_MS);
      return () => clearTimeout(t);
    }
  }, [phase, revealedCount, displayFanBreakdown.length]);

  // Click handler to skip animations or exit
  const handleClick = () => {
    if (phase === 'entrance' || phase === 'revealing-fans') {
      // Skip animations: show all fans and final total immediately
      setRevealedCount(displayFanBreakdown.length);
      setPhase('revealed-total');
      return;
    }
    if (phase !== 'revealed-total' || completeFiredRef.current) return;
    completeFiredRef.current = true;
    setPhase('exit');
    setTimeout(onComplete, 300);
  };

  const content = (
    <div
      className={`dramatic-reveal dramatic-reveal--${phase}`}
      onClick={handleClick}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') handleClick(); }}
    >
      <div className="dramatic-reveal__bg" />

      <div className="dramatic-reveal__content">
        {/* Win type title */}
        <div className="dramatic-reveal__title-row">
          <span
            className={`dramatic-reveal__win-type dramatic-reveal__win-type--${result.winType}`}
          >
            {winTitle}
          </span>
        </div>

        {/* Winner & Discarder */}
        <div className="dramatic-reveal__actors">
          {winnerName && (
            <div className="dramatic-reveal__actor dramatic-reveal__actor--winner">
              <span className="dramatic-reveal__actor-label">和牌</span>
              <span className="dramatic-reveal__actor-name">{winnerName}</span>
            </div>
          )}
          {discarderName && isDiscard && (
            <div className="dramatic-reveal__actor dramatic-reveal__actor--discarder">
              <span className="dramatic-reveal__actor-label">放铳</span>
              <span className="dramatic-reveal__actor-name">{discarderName}</span>
            </div>
          )}
          {isSelfDraw && (
            <div className="dramatic-reveal__actor dramatic-reveal__actor--self-draw-label">
              <span className="dramatic-reveal__actor-self-draw-text">自摸和牌</span>
            </div>
          )}
        </div>

        {/* Fan badges (only show revealed ones) */}
        {displayFanBreakdown.length > 0 && (
          <div className="dramatic-reveal__fans">
            {displayFanBreakdown.slice(0, revealedCount).map((fan) => {
              const entry = getFanGuideEntry(fan.fanKey);
              const displayValue = entry?.fanValue ?? fan.fanValue;
              return (
                <span
                  key={fan.fanKey}
                  className="dramatic-reveal__fan-badge"
                  style={{
                    '--fan-bg': getFanColor(displayValue),
                    '--badge-delay': '0s', // pop immediately on mount
                  } as CSSProperties}
                >
                  <span className="dramatic-reveal__fan-label">
                    {getFanLabel(fan.fanKey)}
                  </span>
                  <span className="dramatic-reveal__fan-value">
                    ×{formatFanValue(displayValue)}
                  </span>
                </span>
              );
            })}
          </div>
        )}

        {/* Multiplier total */}
        <div className="dramatic-reveal__multiplier">
          <div className="dramatic-reveal__multiplier-number">
            <span className="dramatic-reveal__multiplier-value">
              {formatFanValue(finalFanTotal)}
            </span>
            <span className="dramatic-reveal__multiplier-suffix">番</span>
          </div>
        </div>
      </div>
    </div>
  );

  if (typeof document === 'undefined') return null;
  return createPortal(content, document.body);
}

function calculateDetailedFanTotal(fanBreakdown: ResultView['fanBreakdown']): number | null {
  if (fanBreakdown.length === 0) return null;
  let total = 1;
  for (const item of fanBreakdown) {
    const entry = getFanGuideEntry(item.fanKey);
    const displayValue = entry?.fanValue ?? item.fanValue;
    const category = entry?.category ?? 'multiply';
    if (!Number.isFinite(displayValue) || displayValue <= 0) return null;
    if (category === 'multiply') {
      total *= displayValue;
    }
  }
  return total;
}

function findPlayerName(seat: Seat, seats: ResultSeatView[]): string | undefined {
  return seats.find((s) => s.seat === seat)?.name;
}

function getRevealPages(result: ResultView) {
  if (Array.isArray(result.pages) && result.pages.length > 0) {
    return result.pages;
  }
  if (result.winnerSeat || result.fanBreakdown.length > 0) {
    return [
      {
        fanTotal: result.fanTotal,
        winnerSeat: result.winnerSeat,
        winnerAbsoluteSeat: result.winnerAbsoluteSeat ?? null,
        discarderSeat: result.discarderSeat,
        discarderAbsoluteSeat: result.discarderAbsoluteSeat ?? null,
        winType: result.winType,
        winTypeLabel: result.winTypeLabel ?? null,
        fanBreakdown: result.fanBreakdown,
      },
    ];
  }
  return [];
}

function formatFanValue(value: number) {
  return Number.isInteger(value) ? String(value) : value.toFixed(1);
}
