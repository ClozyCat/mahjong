import type { RoomSnapshot } from "../types/protocol";

interface Props {
  snapshot: RoomSnapshot;
  onReady: (ready: boolean) => void;
  onAdjustBots: (delta: 1 | -1) => void;
  onStart: () => void;
  onLeave: () => void;
}

export function WaitingRoom({
  snapshot,
  onReady,
  onAdjustBots,
  onStart,
  onLeave,
}: Props) {
  const localSeat = snapshot.local_seat;
  const me = snapshot.seats.find((s) => s.seat_index === localSeat);
  const imReady = !!me?.ready;
  const humans = snapshot.seats.filter((s) => !s.is_bot);
  const occupied = snapshot.seats.length;
  const canStart =
    occupied === 4 && snapshot.seats.every((s) => s.ready || s.is_bot);
  const iOwnRoom =
    humans.length > 0 && humans[0].seat_index === localSeat;
  const botCount = snapshot.seats.filter((s) => s.is_bot).length;

  // 补满显示 4 个座位槽
  const slots = Array.from({ length: 4 }).map((_, i) => {
    const seat = snapshot.seats.find((s) => s.seat_index === i);
    return { index: i, seat };
  });

  return (
    <div className="waiting-room">
      <div className="waiting-header">
        <div>
          <div className="waiting-code">
            {snapshot.table_code.split("").join(" ")}
          </div>
          <div className="waiting-code-sub">
            {modeLabel(snapshot.mode)} · 等待入席
          </div>
        </div>
        <div className="top-menu-row">
          {iOwnRoom ? (
            <>
              <button
                type="button"
                className="btn-ghost"
                disabled={botCount === 0}
                onClick={() => onAdjustBots(-1)}
              >
                − Bot
              </button>
              <button
                type="button"
                className="btn-ghost"
                disabled={occupied >= 4}
                onClick={() => onAdjustBots(1)}
              >
                + Bot
              </button>
            </>
          ) : null}
          <button type="button" className="btn-ghost" onClick={onLeave}>
            退出
          </button>
        </div>
      </div>

      <div className="waiting-body">
        {slots.map(({ index, seat }) => {
          if (!seat) {
            return (
              <div key={index} className="seat-card glass empty">
                <div className="seat-card-head">
                  <div className="seat-name muted">空位 · {windLabel(index)}</div>
                </div>
                <div className="seat-meta">等待入席</div>
              </div>
            );
          }
          return (
            <div
              key={index}
              className={`seat-card glass ${seat.seat_index === localSeat ? "local" : ""}`}
            >
              <div className="seat-card-head">
                <div className="seat-name">
                  <span className="gold serif">{windLabel(index)}</span>{" "}
                  {seat.nickname ?? `座位 ${index + 1}`}
                </div>
                <span
                  className={`seat-badge ${
                    seat.is_bot
                      ? "bot"
                      : !seat.connected
                        ? "offline"
                        : seat.ready
                          ? "ready"
                          : ""
                  }`}
                >
                  {seat.is_bot
                    ? "Bot"
                    : !seat.connected
                      ? "离线"
                      : seat.ready
                        ? "已备"
                        : "待备"}
                </span>
              </div>
              <div className="seat-meta">
                {seat.seat_index === localSeat ? "本家" : ""}
              </div>
            </div>
          );
        })}
      </div>

      <div className="waiting-footer">
        <button
          type="button"
          className={`ready-btn ${imReady ? "" : "unready"}`}
          onClick={() => onReady(!imReady)}
        >
          {imReady ? "取消就绪" : "我已就绪"}
        </button>
        {iOwnRoom ? (
          <button
            type="button"
            className="start-btn"
            disabled={!canStart}
            onClick={onStart}
          >
            启局
          </button>
        ) : null}
      </div>
    </div>
  );
}

function windLabel(index: number): string {
  return ["東", "南", "西", "北"][index] ?? String(index);
}

function modeLabel(mode: string) {
  return { normal: "常规局", skill: "技能局", test: "测试局" }[mode] ?? mode;
}
