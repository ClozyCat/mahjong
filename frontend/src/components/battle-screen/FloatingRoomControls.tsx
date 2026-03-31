import { useEffect, useMemo, useState } from 'react';

import type { BattleActionView, PlayerView, Seat, WaitingControls } from '../../types/match';
import { PlayerRing } from './PlayerRing';

interface FloatingRoomControlsProps {
  players: PlayerView[];
  actions: BattleActionView[];
  tableCode: string;
  canLeaveTable: boolean;
  phaseLabel: string;
  roundLabel: string;
  scoreSummaryLabel: string;
  deadlineAt: string | null;
  topStatusLabel: string;
  promptText: string | null;
  remainingTileCount?: number | null;
  waitingControls: WaitingControls | null;
  tableTileScale?: number;
  canDecreaseTileScale?: boolean;
  canIncreaseTileScale?: boolean;
  onCopyTableCode: () => void;
  onLeaveTable: () => void;
  onDecreaseTileScale?: () => void;
  onIncreaseTileScale?: () => void;
  onAction: (actionId: BattleActionView['id']) => void;
}

const PLAYER_ORDER: Seat[] = ['bottom', 'left', 'top', 'right'];

export function FloatingRoomControls({
  players,
  actions,
  tableCode,
  canLeaveTable,
  phaseLabel,
  roundLabel,
  scoreSummaryLabel,
  deadlineAt,
  topStatusLabel,
  promptText,
  remainingTileCount = null,
  waitingControls,
  tableTileScale = 1,
  canDecreaseTileScale = false,
  canIncreaseTileScale = false,
  onCopyTableCode,
  onLeaveTable,
  onDecreaseTileScale,
  onIncreaseTileScale,
  onAction,
}: FloatingRoomControlsProps) {
  const [isCollapsed, setIsCollapsed] = useState(false);
  const [remainingSeconds, setRemainingSeconds] = useState<number | null>(null);
  const orderedPlayers = useMemo(
    () =>
      PLAYER_ORDER.map((seat) => players.find((player) => player.seat === seat)).filter(
        (player): player is PlayerView => Boolean(player),
      ),
    [players],
  );
  const hasActionSection = actions.length > 0 || canLeaveTable;
  const shouldShowScaleControls = Boolean(onDecreaseTileScale || onIncreaseTileScale);
  const scalePercentLabel = `${Math.round(tableTileScale * 100)}%`;

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
    <>
      {!isCollapsed ? (
        <aside className="battle-drawer" aria-label="牌桌侧边面板">
          <div className="battle-drawer__header">
            <div>
              <span className="battle-drawer__eyebrow">Table Console</span>
              <strong>牌桌侧边面板</strong>
            </div>
            <button
              type="button"
              className="battle-drawer__collapse"
              aria-label="缩进牌桌侧边面板"
              onClick={() => setIsCollapsed(true)}
            >
              缩进
            </button>
          </div>

          <div className="battle-drawer__body">
            <section className="battle-drawer__section battle-drawer__section--summary">
              <div className="battle-drawer__summary-top">
                <div className="battle-drawer__status-block">
                  <span className="battle-drawer__section-label">牌桌状态</span>
                  <strong>{topStatusLabel}</strong>
                  <p>{getStatusCopy(promptText, waitingControls, remainingTileCount)}</p>
                </div>
                {remainingSeconds !== null ? (
                  <div
                    className={`battle-drawer__countdown ${
                      remainingSeconds <= 3 ? 'battle-drawer__countdown--critical' : ''
                    }`}
                    aria-label={`剩余 ${remainingSeconds} 秒`}
                  >
                    <span>倒计时</span>
                    <strong>{remainingSeconds}</strong>
                  </div>
                ) : null}
              </div>

              <div className="battle-drawer__meta-grid">
                <button type="button" className="battle-drawer__meta-card battle-drawer__meta-card--button" onClick={onCopyTableCode}>
                  <span>牌桌编号</span>
                  <strong>{tableCode}</strong>
                  <em>点击复制</em>
                </button>
                <div className="battle-drawer__meta-card">
                  <span>当前牌局</span>
                  <strong>{roundLabel}</strong>
                  <em>阶段 {phaseLabel}</em>
                </div>
                <div className="battle-drawer__meta-card">
                  <span>积分概览</span>
                  <strong>{scoreSummaryLabel}</strong>
                  <em>{typeof remainingTileCount === 'number' ? `剩余 ${remainingTileCount} 张` : '等待牌墙同步'}</em>
                </div>
                <div className="battle-drawer__meta-card">
                  <span>房间座位</span>
                  <strong>{waitingControls ? `${waitingControls.occupiedSeats}/4` : `${orderedPlayers.length}/4`}</strong>
                  <em>{waitingControls?.isReady ? '你已准备' : '实时同步中'}</em>
                </div>
              </div>
            </section>

            {shouldShowScaleControls ? (
              <section className="battle-drawer__section">
                <div className="battle-drawer__section-head">
                  <span className="battle-drawer__section-label">牌桌显示</span>
                  <strong>牌面尺寸</strong>
                </div>
                <div className="battle-drawer__scale-controls">
                  <span className="battle-drawer__scale-readout">牌面 {scalePercentLabel}</span>
                  <div className="battle-drawer__scale-buttons" role="group" aria-label="调整牌桌牌面大小">
                    <button
                      type="button"
                      className="battle-drawer__scale-button"
                      aria-label="缩小牌桌牌面"
                      onClick={onDecreaseTileScale}
                      disabled={!canDecreaseTileScale}
                    >
                      -
                    </button>
                    <button
                      type="button"
                      className="battle-drawer__scale-button"
                      aria-label="放大牌桌牌面"
                      onClick={onIncreaseTileScale}
                      disabled={!canIncreaseTileScale}
                    >
                      +
                    </button>
                  </div>
                </div>
              </section>
            ) : null}

            <section className="battle-drawer__section">
              <div className="battle-drawer__section-head">
                <span className="battle-drawer__section-label">玩家信息</span>
                <strong>座位面板</strong>
              </div>
              <div className="battle-drawer__players">
                {orderedPlayers.map((player) => (
                  <PlayerRing key={player.seat} player={player} />
                ))}
              </div>
            </section>

            {hasActionSection ? (
              <section className="battle-drawer__section">
                <div className="battle-drawer__section-head">
                  <span className="battle-drawer__section-label">房间操作</span>
                  <strong>控制按钮</strong>
                </div>
                <div className="battle-drawer__actions">
                  {actions.map((action) => (
                    <button
                      key={action.id}
                      type="button"
                      disabled={!action.enabled}
                      className={`battle-drawer__action battle-drawer__action--${action.emphasis}`}
                      onClick={() => onAction(action.id)}
                    >
                      {action.label}
                    </button>
                  ))}
                  {canLeaveTable ? (
                    <button type="button" className="battle-drawer__action battle-drawer__action--danger" onClick={onLeaveTable}>
                      离开牌桌
                    </button>
                  ) : null}
                </div>
              </section>
            ) : null}
          </div>
        </aside>
      ) : null}

      {isCollapsed ? (
        <button
          type="button"
          className="battle-drawer__restore"
          aria-label="展开牌桌侧边面板"
          onClick={() => setIsCollapsed(false)}
        >
          牌桌面板
        </button>
      ) : null}
    </>
  );
}

function getStatusCopy(promptText: string | null, waitingControls: WaitingControls | null, remainingTileCount: number | null) {
  if (promptText) {
    return promptText;
  }

  if (typeof remainingTileCount === 'number') {
    return `牌墙还剩 ${remainingTileCount} 张，界面会随浏览器窗口动态缩放。`;
  }

  if (waitingControls) {
    return `当前已有 ${waitingControls.occupiedSeats}/4 位牌手入座，准备状态会实时同步。`;
  }

  return '牌桌信息、玩家状态和房间操作已整合到右侧抽屉。';
}
