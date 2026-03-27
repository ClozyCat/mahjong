import { useEffect, useState } from 'react';

interface TopMatchBarProps {
  tableCode: string;
  canLeaveTable: boolean;
  phaseLabel: string;
  roundLabel: string;
  scoreSummaryLabel: string;
  deadlineAt: string | null;
  topStatusLabel: string;
  onCopyTableCode: () => void;
  onLeaveTable: () => void;
}

export function TopMatchBar({
  tableCode,
  canLeaveTable,
  phaseLabel,
  roundLabel,
  scoreSummaryLabel,
  deadlineAt,
  topStatusLabel,
  onCopyTableCode,
  onLeaveTable,
}: TopMatchBarProps) {
  const [remainingSeconds, setRemainingSeconds] = useState<number | null>(null);

  useEffect(() => {
    if (!deadlineAt) {
      setRemainingSeconds(null);
      return;
    }

    const update = () => {
      const nextRemaining = Math.max(0, Math.ceil((new Date(deadlineAt).getTime() - Date.now()) / 1000));
      setRemainingSeconds(nextRemaining);
    };

    update();
    const timer = window.setInterval(update, 250);
    return () => {
      window.clearInterval(timer);
    };
  }, [deadlineAt]);

  return (
    <header className="top-match-bar">
      <div className="top-match-bar__brand">
        <span className="top-match-bar__eyebrow">当前牌局</span>
        <strong>{roundLabel}</strong>
      </div>
      <div className="top-match-bar__meta">
        <span>积分 {scoreSummaryLabel}</span>
        <span>阶段 {phaseLabel}</span>
        {remainingSeconds !== null ? <span>倒计时 {remainingSeconds}s</span> : null}
        <span className="top-match-bar__status">{topStatusLabel}</span>
      </div>
      <button type="button" className="top-match-bar__copy" onClick={onCopyTableCode}>
        牌桌编号 {tableCode}
      </button>
      {canLeaveTable ? (
        <button type="button" className="top-match-bar__leave" onClick={onLeaveTable}>
          离开牌桌
        </button>
      ) : null}
    </header>
  );
}
