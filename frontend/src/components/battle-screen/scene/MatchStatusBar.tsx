import { memo, useEffect, useLayoutEffect, useRef, useState } from 'react';

import type { DealerSelectionView, Seat } from '../../../types/match';

interface MatchStatusBarProps {
  remainingCount: number | null;
  actionSeat: Seat | null;
  dealerSelection?: DealerSelectionView | null;
  deadlineAt: string | null;
  isAmbiguous?: boolean;
  shouldDebounceWaiting?: boolean;
  onSizeChange?: (size: { width: number; height: number }) => void;
}

const SEAT_LABELS: Record<Seat, string> = {
  bottom: '本家',
  right: '下家',
  top: '对家',
  left: '上家',
};

const WAITING_ACTION_LABEL = '等待中';

function getDealerSelectionTransitionMs(dealerSelection: DealerSelectionView) {
  const remainingMs = new Date(dealerSelection.revealAt).getTime() - Date.now();
  if (!Number.isFinite(remainingMs)) {
    return Math.max(300, Math.min(4_800, dealerSelection.durationMs));
  }

  return Math.max(300, Math.min(4_800, remainingMs));
}

const SeatArrow = ({ seat, dealerSelection }: { seat: Seat; dealerSelection?: DealerSelectionView | null }) => {
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

  const transitionMs = dealerSelection ? getDealerSelectionTransitionMs(dealerSelection) : 400;
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
  isAmbiguous: isAmbiguousProp = false,
  shouldDebounceWaiting = false,
  onSizeChange,
}: MatchStatusBarProps) {
  const statusBarRef = useRef<HTMLDivElement | null>(null);
  const [remainingSeconds, setRemainingSeconds] = useState<number | null>(null);
  const [isAmbiguous, setIsAmbiguous] = useState(false);
  
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
      const remainingMs = new Date(deadlineAt).getTime() - Date.now();
      setRemainingSeconds(Math.max(0, Math.ceil(remainingMs / 1000)));
    };

    update();
    const timer = window.setInterval(update, 250);
    return () => {
      window.clearInterval(timer);
    };
  }, [deadlineAt]);

  const activeSeatLabel = stableDealerSelection
    ? '东家'
    : (stableActionSeat ? SEAT_LABELS[stableActionSeat] : WAITING_ACTION_LABEL);
  const shouldShowActionArrow = Boolean(stableDealerSelection || stableActionSeat);
  const visibleActionLabel = shouldShowActionArrow ? null : WAITING_ACTION_LABEL;
  const showUrgent = remainingSeconds !== null && remainingSeconds <= 5;

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

  return (
    <div ref={statusBarRef} className={`match-status-bar ${isAmbiguous ? 'match-status-bar--ambiguous' : ''}`}>
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
            <SeatArrow seat={stableDealerSelection.dealerSeat} dealerSelection={stableDealerSelection} />
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
        <span className={`match-status-bar__value ${showUrgent ? 'match-status-bar__value--urgent' : ''}`}>
          {remainingSeconds !== null ? `${remainingSeconds}s` : '--'}
        </span>
      </div>
    </div>
  );
});
