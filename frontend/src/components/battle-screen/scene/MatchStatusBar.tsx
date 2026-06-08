import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';

import { getRemainingMs, getRemainingSeconds, getServerNowMs } from '../../../lib/timeSync';
import type { DealerSelectionView, Seat } from '../../../types/match';

interface MatchStatusBarProps {
  remainingCount: number | null;
  actionSeat: Seat | null;
  dealerSelection?: DealerSelectionView | null;
  deadlineAt: string | null;
  serverNowOffsetMs?: number;
  isAmbiguous?: boolean;
  shouldDebounceWaiting?: boolean;
  onSizeChange?: (size: { width: number; height: number }) => void;
  extendedWithExtra?: boolean;
}

const SEAT_LABELS: Record<Seat, string> = {
  bottom: '本家',
  right: '下家',
  top: '对家',
  left: '上家',
};

const WAITING_ACTION_LABEL = '等待中';
const NORMAL_TIMEOUT_SECONDS = 15;

function getDealerSelectionTransitionMs(dealerSelection: DealerSelectionView, serverNowOffsetMs = 0) {
  const remainingMs = new Date(dealerSelection.revealAt).getTime() - getServerNowMs(serverNowOffsetMs);
  if (!Number.isFinite(remainingMs)) {
    return Math.max(300, Math.min(4_800, dealerSelection.durationMs));
  }

  return Math.max(300, Math.min(4_800, remainingMs));
}

const SeatArrow = ({
  seat,
  dealerSelection,
  serverNowOffsetMs = 0,
}: {
  seat: Seat;
  dealerSelection?: DealerSelectionView | null;
  serverNowOffsetMs?: number;
}) => {
  const seatOrder: Seat[] = ['bottom', 'right', 'top', 'left'];
  const getBaseRotation = (s: Seat) => {
    const index = seatOrder.indexOf(s);
    return 180 - (index * 90);
  };

  const [rotation, setRotation] = useState(() => getBaseRotation(seat));
  const prevSeatRef = useRef<Seat>(seat);
  const dealerSelectionKey = dealerSelection?.key ?? null;

  useEffect(() => {
    if (dealerSelectionKey && dealerSelection) {
      // Handle spin for dealer selection
      const targetRotation = getBaseRotation(dealerSelection.dealerSeat);
      setRotation(prev => {
        let next = targetRotation;
        // Always spin many times CCW (decreasing angle)
        while (next >= prev - 180) {
          next -= 360;
        }
        return next - 360 * 5; // 5 extra full spins for lottery effect
      });
      prevSeatRef.current = dealerSelection.dealerSeat;
      return;
    }

    if (prevSeatRef.current !== seat) {
      const prevIndex = seatOrder.indexOf(prevSeatRef.current);
      const currIndex = seatOrder.indexOf(seat);

      // Calculate the shortest step in turn order (0-3)
      let step = currIndex - prevIndex;
      if (step < 0) step += 4;

      // Always rotate CCW (decrease angle)
      setRotation(prev => prev - (step * 90));
      prevSeatRef.current = seat;
    }
  }, [seat, dealerSelectionKey, dealerSelection]);

  const transitionMs = dealerSelection ? getDealerSelectionTransitionMs(dealerSelection, serverNowOffsetMs) : 200;
  const timingFunction = dealerSelection ? 'cubic-bezier(0.12, 0.78, 0.12, 1)' : 'var(--ease-spring)';

  return (
    <svg 
      className="match-status-bar__arrow" 
      viewBox="0 0 24 24" 
      style={{ 
        transform: `rotate(${rotation}deg)`,
        transitionDuration: `${transitionMs}ms`,
        transitionTimingFunction: timingFunction
      }}
    >
      <path d="M12 8.3l-6 6 1.41 1.41L12 11.13l4.59 4.58L18 14.3z" fill="currentColor" />
    </svg>
  );
};

const TimerIndicators = ({ 
  progress, 
  size, 
  seat, 
  isAmbiguous,
  showUrgent
}: { 
  progress: number; 
  size: { width: number; height: number }; 
  seat: Seat | null; 
  isAmbiguous: boolean;
  showUrgent: boolean;
}) => {
  const { width, height } = size;
  if (width === 0 || height === 0) return null;

  const R = height / 2;
  const W = width;
  const H = height;

  const paths = useMemo(() => {
    // Basic segments
    const top = `M ${R},0 L ${W - R},0`;
    const bottom = `M ${W - R},${H} L ${R},${H}`;
    const left = `M ${R},${H} A ${R},${R} 0 0 1 ${R},0`;
    const right = `M ${W - R},0 A ${R},${R} 0 0 1 ${W - R},${H}`;

    // Waiting state halves (shrink towards top/bottom centers)
    const topHalf = `M 0,${R} A ${R},${R} 0 0 1 ${R},0 L ${W - R},0 A ${R},${R} 0 0 1 ${W},${R}`;
    const bottomHalf = `M 0,${R} A ${R},${R} 0 0 0 ${R},${H} L ${W - R},${H} A ${R},${R} 0 0 0 ${W},${R}`;

    return { top, bottom, left, right, topHalf, bottomHalf };
  }, [W, H, R]);

  const getDashStyles = (length: number) => {
    const visible = length * progress;
    const gap = length * (1 - progress);
    return {
      strokeDasharray: `${visible} ${gap}`,
      strokeDashoffset: -(gap / 2),
    };
  };

  const pathClass = `match-status-bar__timer-path ${showUrgent ? 'match-status-bar__timer-path--urgent' : ''}`;

  return (
    <svg className="match-status-bar__timer-svg" viewBox={`0 0 ${W} ${H}`} fill="none">
      {/* Player Indicators */}
      {seat === 'top' && <path d={paths.top} className={pathClass} {...getDashStyles(W - 2 * R)} />}
      {seat === 'bottom' && <path d={paths.bottom} className={pathClass} {...getDashStyles(W - 2 * R)} />}
      {seat === 'left' && <path d={paths.left} className={pathClass} {...getDashStyles(Math.PI * R)} />}
      {seat === 'right' && <path d={paths.right} className={pathClass} {...getDashStyles(Math.PI * R)} />}

      {/* Waiting state indicators */}
      {isAmbiguous && (
        <>
          <path d={paths.topHalf} className={`${pathClass} match-status-bar__timer-path--waiting`} {...getDashStyles(W - 2 * R + Math.PI * R)} />
          <path d={paths.bottomHalf} className={`${pathClass} match-status-bar__timer-path--waiting`} {...getDashStyles(W - 2 * R + Math.PI * R)} />
        </>
      )}
    </svg>
  );
};

function reportStatusBarSize(
  element: HTMLDivElement | null,
  onSizeChange: ((size: { width: number; height: number }) => void) | undefined,
) {
  if (!element || !onSizeChange) {
    return;
  }

  const rect = element.getBoundingClientRect();
  if (rect.width > 0 && rect.height > 0) {
    onSizeChange({
      width: rect.width,
      height: rect.height,
    });
  }
}

export const MatchStatusBar = memo(function MatchStatusBar({
  remainingCount,
  actionSeat,
  dealerSelection = null,
  deadlineAt,
  serverNowOffsetMs = 0,
  isAmbiguous: isAmbiguousProp = false,
  shouldDebounceWaiting = false,
  onSizeChange,
  extendedWithExtra = false,
}: MatchStatusBarProps) {
  const statusBarRef = useRef<HTMLDivElement | null>(null);
  const [remainingSeconds, setRemainingSeconds] = useState<number | null>(null);
  const [isAmbiguous, setIsAmbiguous] = useState(false);
  const [progress, setProgress] = useState(1);
  const [size, setSize] = useState({ width: 0, height: 0 });
  
  // Track "stable" action seat info to avoid flickering during optimistic discard acknowledgements.
  const [stableActionSeat, setStableActionSeat] = useState<Seat | null>(actionSeat);
  const [stableDealerSelection, setStableDealerSelection] = useState<DealerSelectionView | null>(dealerSelection);

  useEffect(() => {
    if (actionSeat || dealerSelection) {
      setStableActionSeat(actionSeat);
      setStableDealerSelection(dealerSelection);
      setIsAmbiguous(false);
      return;
    }

    if (!shouldDebounceWaiting) {
      setStableActionSeat(null);
      setStableDealerSelection(null);
      setIsAmbiguous(isAmbiguousProp);
      return;
    }

    const timer = window.setTimeout(() => {
      setStableActionSeat(null);
      setStableDealerSelection(null);
      setIsAmbiguous(isAmbiguousProp);
    }, 500);

    return () => {
      window.clearTimeout(timer);
    };
  }, [actionSeat, dealerSelection, isAmbiguousProp, shouldDebounceWaiting]);

  useEffect(() => {
    if (!deadlineAt) {
      setRemainingSeconds(null);
      return;
    }

    const update = () => {
      setRemainingSeconds(getRemainingSeconds(deadlineAt, serverNowOffsetMs));
    };

    update();
    const timer = window.setInterval(update, 250);
    return () => {
      window.clearInterval(timer);
    };
  }, [deadlineAt, serverNowOffsetMs]);

  useEffect(() => {
    if (!deadlineAt) {
      setProgress(1);
      return;
    }

    const targetDate = new Date(deadlineAt).getTime();
    const startRemainingMs = targetDate - getServerNowMs(serverNowOffsetMs);
    // Use a baseline duration of at least 5s, typical mahjong timers are 15-30s.
    const baselineDuration = Math.max(startRemainingMs, 5000);

    const update = () => {
      const remaining = getRemainingMs(deadlineAt, serverNowOffsetMs);
      const p = Math.max(0, Math.min(1, remaining / baselineDuration));
      setProgress(p);
    };

    update();
    const timer = setInterval(update, 60);
    return () => clearInterval(timer);
  }, [deadlineAt, serverNowOffsetMs]);

  const activeSeatLabel = stableDealerSelection
    ? '东家'
    : (stableActionSeat ? SEAT_LABELS[stableActionSeat] : WAITING_ACTION_LABEL);
  const shouldShowActionArrow = Boolean(stableDealerSelection || stableActionSeat);
  const visibleActionLabel = shouldShowActionArrow ? null : WAITING_ACTION_LABEL;
  const showUrgent = remainingSeconds !== null && remainingSeconds <= 5;
  const isExtraTime = extendedWithExtra;

  useLayoutEffect(() => {
    reportStatusBarSize(statusBarRef.current, onSizeChange);
  }, [activeSeatLabel, isAmbiguous, onSizeChange, remainingCount, remainingSeconds]);

  useLayoutEffect(() => {
    const resizeObserver = typeof ResizeObserver !== 'undefined'
      ? new ResizeObserver(() => reportStatusBarSize(statusBarRef.current, onSizeChange))
      : null;
    const element = statusBarRef.current;

    if (!resizeObserver || !element || !onSizeChange) {
      return undefined;
    }

    resizeObserver.observe(element);

    return () => {
      resizeObserver.disconnect();
    };
  }, [onSizeChange]);

  useEffect(() => {
    if (!statusBarRef.current) return;
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const rect = entry.target.getBoundingClientRect();
        setSize({
          width: rect.width,
          height: rect.height,
        });
      }
    });
    observer.observe(statusBarRef.current);
    return () => observer.disconnect();
  }, []);

  return (
    <div
      ref={statusBarRef}
      className={`match-status-bar ${isAmbiguous ? 'match-status-bar--ambiguous' : ''}`}
      data-active-seat={stableActionSeat ?? undefined}
    >
      <TimerIndicators 
        progress={progress} 
        size={size} 
        seat={stableActionSeat} 
        isAmbiguous={isAmbiguous}
        showUrgent={showUrgent}
      />
      <div className="match-status-bar__section">
        <span className="match-status-bar__label">剩余</span>
        <span className="match-status-bar__value">{remainingCount ?? 0}</span>
      </div>
      
      <div className="match-status-bar__divider" />
      
      <div
        className={`match-status-bar__section match-status-bar__section--action ${stableActionSeat ? `match-status-bar__action--${stableActionSeat}` : ''}`}
        aria-label={`当前行动：${activeSeatLabel}`}
        data-width-label={WAITING_ACTION_LABEL}
      >
        {stableDealerSelection ? (
          <span className="match-status-bar__arrow-wrap" aria-hidden="true">
            <SeatArrow
              seat={stableDealerSelection.dealerSeat}
              dealerSelection={stableDealerSelection}
              serverNowOffsetMs={serverNowOffsetMs}
            />
          </span>
        ) : stableActionSeat ? (
          <span className="match-status-bar__arrow-wrap" aria-hidden="true">
            <SeatArrow seat={stableActionSeat} />
          </span>
        ) : null}
        {visibleActionLabel ? (
          <span className="match-status-bar__value match-status-bar__action-text">{visibleActionLabel}</span>
        ) : null}
      </div>

      <div className="match-status-bar__divider" />

      <div className="match-status-bar__section">
        <span className="match-status-bar__label">倒数</span>
        <span className={`match-status-bar__value ${showUrgent ? 'match-status-bar__value--urgent' : ''} ${isExtraTime ? 'match-status-bar__value--extra-time' : ''}`}>
          {remainingSeconds !== null ? `${remainingSeconds}s` : '--'}
        </span>
      </div>
    </div>
  );
});
