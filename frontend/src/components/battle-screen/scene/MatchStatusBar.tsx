import { memo, useEffect, useRef, useState } from 'react';

import type { DealerSelectionView, Seat } from '../../../types/match';

interface MatchStatusBarProps {
  remainingCount: number | null;
  actionSeat: Seat | null;
  dealerSelection?: DealerSelectionView | null;
  deadlineAt: string | null;
  isAmbiguous?: boolean;
}

const SEAT_LABELS: Record<Seat, string> = {
  bottom: '本家',
  right: '下家',
  top: '对家',
  left: '上家',
};

const SeatArrow = ({ seat }: { seat: Seat }) => {
  const seatOrder: Seat[] = ['bottom', 'right', 'top', 'left'];
  const getBaseRotation = (s: Seat) => {
    const index = seatOrder.indexOf(s);
    return 180 - (index * 90);
  };

  const [rotation, setRotation] = useState(() => getBaseRotation(seat));
  const prevSeatRef = useRef<Seat>(seat);

  useEffect(() => {
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
  }, [seat]);

  return (
    <svg 
      className="match-status-bar__arrow" 
      viewBox="0 0 24 24" 
      style={{ transform: `rotate(${rotation}deg)` }}
    >
      <path d="M12 6l-6 6 1.41 1.41L12 8.83l4.59 4.58L18 12z" fill="currentColor" />
    </svg>
  );
};

export const MatchStatusBar = memo(function MatchStatusBar({
  remainingCount,
  actionSeat,
  dealerSelection = null,
  deadlineAt,
  isAmbiguous: isAmbiguousProp = false,
}: MatchStatusBarProps) {
  const [remainingSeconds, setRemainingSeconds] = useState<number | null>(null);
  const [isAmbiguous, setIsAmbiguous] = useState(false);
  
  // Track "stable" action seat info to avoid flickering during optimistic updates
  const [stableActionSeat, setStableActionSeat] = useState<Seat | null>(actionSeat);
  const [stableDealerSelection, setStableDealerSelection] = useState<DealerSelectionView | null>(dealerSelection);

  useEffect(() => {
    // If we have a concrete action or dealer selection, update immediately
    if (actionSeat || dealerSelection) {
      setStableActionSeat(actionSeat);
      setStableDealerSelection(dealerSelection);
      setIsAmbiguous(false);
      return;
    }

    // If we enter an ambiguous/waiting state (e.g. after discard), wait before updating UI
    const timer = window.setTimeout(() => {
      setStableActionSeat(null);
      setStableDealerSelection(null);
      if (isAmbiguousProp) {
        setIsAmbiguous(true);
      }
    }, 500); // Slightly longer delay for text stability

    return () => {
      window.clearTimeout(timer);
    };
  }, [actionSeat, dealerSelection, isAmbiguousProp]);

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

  const activeSeatLabel = stableDealerSelection ? '抽取东家' : (stableActionSeat ? SEAT_LABELS[stableActionSeat] : '等待中');
  const showUrgent = remainingSeconds !== null && remainingSeconds <= 5;

  return (
    <div className={`match-status-bar ${isAmbiguous ? 'match-status-bar--ambiguous' : ''}`}>
      <div className="match-status-bar__section">
        <span className="match-status-bar__label">剩余</span>
        <span className="match-status-bar__value">{remainingCount ?? 0}</span>
      </div>
      
      <div className="match-status-bar__divider" />
      
      <div className={`match-status-bar__section match-status-bar__section--action ${stableActionSeat ? `match-status-bar__action--${stableActionSeat}` : ''}`}>
        {stableActionSeat && !stableDealerSelection ? <SeatArrow seat={stableActionSeat} /> : null}
        <span className="match-status-bar__value">{activeSeatLabel}</span>
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
