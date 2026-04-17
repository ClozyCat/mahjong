import { useEffect, useState } from "react";

interface Props {
  wind: string;
  handNumber: number;
  wallRemaining: number;
  deadlineAt: string | null;
}

export function Compass({ wind, handNumber, wallRemaining, deadlineAt }: Props) {
  const [remaining, setRemaining] = useState<number | null>(null);
  const [total, setTotal] = useState<number>(30);

  useEffect(() => {
    if (!deadlineAt) {
      setRemaining(null);
      return;
    }
    const deadline = Date.parse(deadlineAt);
    if (Number.isNaN(deadline)) {
      setRemaining(null);
      return;
    }
    const totalSec = Math.max(1, Math.round((deadline - Date.now()) / 1000));
    setTotal(totalSec);
    const tick = () => {
      const left = Math.max(0, (deadline - Date.now()) / 1000);
      setRemaining(left);
    };
    tick();
    const id = window.setInterval(tick, 200);
    return () => window.clearInterval(id);
  }, [deadlineAt]);

  const r = 56;
  const circ = 2 * Math.PI * r;
  const progress =
    remaining === null ? 0 : Math.min(1, Math.max(0, remaining / total));
  const offset = circ * (1 - progress);
  const urgent = remaining !== null && remaining <= 3;

  return (
    <div className="compass">
      <div className="compass-wind">{wind}</div>
      <div className="compass-hint">第 {handNumber} 局</div>
      {remaining !== null ? (
        <div className={`compass-ring ${urgent ? "warning" : ""}`}>
          <svg viewBox="0 0 120 120">
            <circle
              cx="60"
              cy="60"
              r={r}
              strokeDasharray={circ}
              strokeDashoffset={offset}
            />
          </svg>
        </div>
      ) : null}
      <div className="wall-count">余 {wallRemaining} 牌</div>
    </div>
  );
}
