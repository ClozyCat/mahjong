import type { PublicSeatView } from "../types/protocol";

interface Props {
  seat: PublicSeatView;
  isLocal: boolean;
  isCurrent: boolean;
  wind: string;
  cumulativeScore: number;
}

export function SeatTag({ seat, isLocal, isCurrent, wind, cumulativeScore }: Props) {
  const name = seat.nickname ?? `座位${seat.seat_index + 1}`;
  return (
    <div className={`seat-tag glass ${isCurrent ? "active" : ""}`}>
      <div className="avatar">{wind}</div>
      <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <span>{name}</span>
          {isLocal ? <span className="gold" style={{ fontSize: 10 }}>本家</span> : null}
          {!seat.connected && !seat.is_bot ? (
            <span className="offline-dot" title="离线" />
          ) : null}
          {seat.is_bot ? <span style={{ fontSize: 10, color: "var(--ink-soft)" }}>AI</span> : null}
        </div>
        <span className="score-chip">
          {cumulativeScore >= 0 ? `+${cumulativeScore}` : cumulativeScore} 分
        </span>
      </div>
    </div>
  );
}
