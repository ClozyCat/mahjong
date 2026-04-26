import { memo, useEffect, useRef, useState } from 'react';

import type { Seat } from '../../../types/match';

const PENDING_ACTION_DURATION_MS = 30_000;
const COUNTDOWN_RING_STROKE_WIDTH = 3;

interface CenterIndicatorProps {
  remainingCount: number | null;
  actionSeat: Seat | null;
  deadlineAt: string | null;
  isAmbiguous?: boolean;
}

const POINTER_ROTATION_BY_SEAT: Record<Seat, number> = {
  bottom: 0,
  right: -90,
  top: 180,
  left: 90,
};

function resolveShortestPointerRotation(previousRotation: number, actionSeat: Seat) {
  let nextRotation = POINTER_ROTATION_BY_SEAT[actionSeat];

  while (nextRotation - previousRotation > 180) {
    nextRotation -= 360;
  }

  while (nextRotation - previousRotation < -180) {
    nextRotation += 360;
  }

  return nextRotation;
}

function getCountdownPercent(deadlineAt: string | null) {
  if (!deadlineAt) {
    return 1;
  }

  const deadlineTime = new Date(deadlineAt).getTime();
  if (Number.isNaN(deadlineTime)) {
    return 1;
  }

  return Math.max(0, Math.min(1, (deadlineTime - Date.now()) / PENDING_ACTION_DURATION_MS));
}

export const CenterIndicator = memo(function CenterIndicator({
  remainingCount,
  actionSeat,
  deadlineAt,
  isAmbiguous = false,
}: CenterIndicatorProps) {
  const radius = 34;
  const circumference = 2 * Math.PI * radius;
  const countdownRef = useRef<SVGCircleElement | null>(null);
  const [pointerRotation, setPointerRotation] = useState(() => (actionSeat ? POINTER_ROTATION_BY_SEAT[actionSeat] : 0));

  useEffect(() => {
    if (!actionSeat) {
      return;
    }

    setPointerRotation((previousRotation) => resolveShortestPointerRotation(previousRotation, actionSeat));
  }, [actionSeat]);

  useEffect(() => {
    const circle = countdownRef.current;
    if (!circle) {
      return undefined;
    }

    let frameId: number | null = null;
    let disposed = false;

    const renderCountdown = () => {
      if (disposed) {
        return;
      }

      const nextPercent = getCountdownPercent(deadlineAt);
      const nextOffset = circumference - (nextPercent * circumference);

      if (nextOffset <= COUNTDOWN_RING_STROKE_WIDTH) {
        circle.removeAttribute('stroke-dasharray');
        circle.removeAttribute('stroke-dashoffset');
      } else {
        circle.setAttribute('stroke-dasharray', `${circumference}`);
        circle.setAttribute('stroke-dashoffset', `${nextOffset}`);
      }

      if (nextPercent > 0) {
        frameId = requestAnimationFrame(renderCountdown);
      } else {
        frameId = null;
      }
    };

    if (!deadlineAt || Number.isNaN(new Date(deadlineAt).getTime())) {
      circle.removeAttribute('stroke-dasharray');
      circle.removeAttribute('stroke-dashoffset');
      return undefined;
    }

    renderCountdown();

    return () => {
      disposed = true;
      if (frameId !== null) {
        cancelAnimationFrame(frameId);
      }
    };
  }, [circumference, deadlineAt]);

  return (
    <div className="table-stage__center-indicator" aria-label="游戏进度指示器">
      {isAmbiguous ? <div className="table-stage__center-indicator-breathing" /> : null}
      <svg className="table-stage__center-indicator-ring" viewBox="0 0 100 100">
        <circle className="table-stage__center-indicator-base" cx="50" cy="50" r="38" />
        <circle
          ref={countdownRef}
          className="table-stage__center-indicator-countdown"
          cx="50"
          cy="50"
          r={radius}
          strokeWidth={COUNTDOWN_RING_STROKE_WIDTH}
        />
        {actionSeat ? (
          <path
            className="table-stage__center-indicator-pointer"
            d="M44 90 L56 90 L50 98 Z"
            transform={`rotate(${pointerRotation} 50 50)`}
          />
        ) : null}
      </svg>
      <div className="table-stage__center-indicator-remaining">
        <strong className="table-stage__center-indicator-count">{remainingCount ?? 0}</strong>
      </div>
    </div>
  );
});
