import type { MatchPhase, WaitingControls } from '../../types/match';

interface AmbientOverlayProps {
  mode: MatchPhase;
  promptText: string | null;
  waitingControls: WaitingControls | null;
}

export function AmbientOverlay({ mode, promptText, waitingControls }: AmbientOverlayProps) {
  const showVeil = mode === 'loading' || mode === 'disconnected_or_waiting' || mode === 'finished';
  const isWaiting = Boolean(waitingControls);
  const isFinished = mode === 'finished';

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
          </div>
        </div>
      ) : null}
    </>
  );
}
