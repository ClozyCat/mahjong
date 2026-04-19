import type { MatchPhase, WaitingControls } from '../../types/match';

interface AmbientOverlayProps {
  mode: MatchPhase;
  promptText: string | null;
  waitingControls: WaitingControls | null;
  canLeaveTable?: boolean;
  onAddBot?: () => void;
  onRemoveBot?: () => void;
  onLeaveTable?: () => void;
}

export function AmbientOverlay({
  mode,
  promptText,
  waitingControls,
  canLeaveTable = false,
  onAddBot,
  onRemoveBot,
  onLeaveTable,
}: AmbientOverlayProps) {
  const isWaiting = Boolean(waitingControls);
  const showVeil =
    mode === 'loading' ||
    mode === 'finished' ||
    (mode === 'disconnected_or_waiting' && !isWaiting);
  const isFinished = mode === 'finished';
  const shouldShowLeaveButton = isWaiting && canLeaveTable;

  return (
    <>
      {showVeil ? (
        <div className="ambient-overlay__veil">
          <div className="ambient-overlay__panel">
            <span className="ambient-overlay__eyebrow">{isFinished ? '对局状态' : isWaiting ? '房间状态' : '连接状态'}</span>
            <strong>{isFinished ? '整场结束' : isWaiting ? '等待牌手' : '正在重连'}</strong>
            <p>
              {isFinished
                ? promptText ?? '可以查看最终积分并直接发起再来一局。'
                : isWaiting
                ? waitingControls?.canStart
                  ? `四个座位都已准备完成，可以开始本局对战。当前已入座 ${waitingControls.occupiedSeats}/4。`
                  : `当前已入座 ${waitingControls?.occupiedSeats ?? 0}/4，座位、准备状态和在线信息会持续同步。`
                : promptText ?? '正在等待服务器同步下一帧状态。'}
            </p>
            {isWaiting ? (
              <div className="ambient-overlay__waiting-actions">
                {shouldShowLeaveButton ? (
                  <button type="button" className="ambient-overlay__leave-button" onClick={onLeaveTable}>
                    离开牌桌
                  </button>
                ) : null}
                <div className="ambient-overlay__bot-controls" role="group" aria-label="蒙版 BOT 数量控制">
                  <span className="ambient-overlay__bot-label">BOT 数量</span>
                  <button
                    type="button"
                    className="ambient-overlay__bot-button"
                    aria-label="蒙版减少 BOT"
                    disabled={!waitingControls?.canRemoveBot}
                    onClick={onRemoveBot}
                  >
                    -
                  </button>
                  <strong className="ambient-overlay__bot-count" aria-label={`蒙版当前 BOT 数量 ${waitingControls?.botCount ?? 0}`}>
                    {waitingControls?.botCount ?? 0}
                  </strong>
                  <button
                    type="button"
                    className="ambient-overlay__bot-button"
                    aria-label="蒙版增加 BOT"
                    disabled={!waitingControls?.canAddBot}
                    onClick={onAddBot}
                  >
                    +
                  </button>
                </div>
              </div>
            ) : null}
          </div>
        </div>
      ) : null}
    </>
  );
}
