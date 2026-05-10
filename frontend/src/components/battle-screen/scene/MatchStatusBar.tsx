import { memo, useEffect, useState } from 'react';

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
  const rotations: Record<Seat, number> = {
    top: 0,
    right: 90,
    bottom: 180,
    left: -90,
  };
  return (
    <svg 
      className="match-status-bar__arrow" 
      viewBox="0 0 24 24" 
      style={{ transform: `rotate(${rotations[seat]}deg)` }}
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
  isAmbiguous = false,
}: MatchStatusBarProps) {
  const [remainingSeconds, setRemainingSeconds] = useState<number | null>(null);

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

  const activeSeatLabel = dealerSelection ? '抽取东家' : (actionSeat ? SEAT_LABELS[actionSeat] : '等待中');
  const showUrgent = remainingSeconds !== null && remainingSeconds <= 5;

  return (
    <div className={`match-status-bar ${isAmbiguous ? 'match-status-bar--ambiguous' : ''}`}>
      <div className="match-status-bar__section">
        <span className="match-status-bar__label">剩余</span>
        <span className="match-status-bar__value">{dealerSelection ? '东' : remainingCount ?? 0}</span>
      </div>
      
      <div className="match-status-bar__divider" />
      
      <div className={`match-status-bar__section match-status-bar__section--action ${actionSeat ? `match-status-bar__action--${actionSeat}` : ''}`}>
        {actionSeat && !dealerSelection ? <SeatArrow seat={actionSeat} /> : null}
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
