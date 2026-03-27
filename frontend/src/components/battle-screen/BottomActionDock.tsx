import { useState } from 'react';

import type { BattleActionView, BattleViewModel, PlayerView } from '../../types/match';
import { MahjongTile } from './MahjongTile';
import { MeldRack } from './MeldRack';

interface BottomActionDockProps {
  hand: BattleViewModel['localHand'];
  actions: BattleActionView[];
  isElevated: boolean;
  waitingControls: BattleViewModel['waitingControls'];
  localPlayer: PlayerView | null;
  onTileSelect: (tileId: string) => void;
  onAction: (actionId: BattleActionView['id']) => void;
}

export function BottomActionDock({
  hand,
  actions,
  isElevated,
  waitingControls,
  localPlayer,
  onTileSelect,
  onAction,
}: BottomActionDockProps) {
  const [isCollapsed, setIsCollapsed] = useState(false);
  const windLabel = localPlayer ? WIND_LABELS[localPlayer.wind] ?? localPlayer.wind : null;
  const headingEyebrow = waitingControls ? '房间控制' : '当前操作';
  const headingTitle = waitingControls ? '等待牌桌' : '手牌控制区';

  return (
    <>
      {!isCollapsed ? (
        <section
          className={`action-dock ${isElevated ? 'action-dock--elevated' : ''}`}
          data-testid="action-dock"
          data-elevated={isElevated}
        >
          <div className="action-dock__heading">
            <div>
              <span className="action-dock__eyebrow">{headingEyebrow}</span>
              <strong>{headingTitle}</strong>
            </div>
            <div className="action-dock__heading-side">
              {localPlayer ? (
                <div className="action-dock__player">
                  <span className="action-dock__player-eyebrow">
                    {windLabel}
                    {localPlayer.isDealer ? ' 庄家' : ''}
                    {localPlayer.connected ? ' 在线' : ' 离线'}
                  </span>
                  <strong>{localPlayer.name}</strong>
                  <span className="action-dock__player-meta">
                    {localPlayer.score.toLocaleString()} · {localPlayer.statusText ?? '就绪'}
                  </span>
                </div>
              ) : (
                <span className="action-dock__badge">{waitingControls?.isReady ? '已准备' : '待命中'}</span>
              )}
              <button
                type="button"
                className="action-dock__collapse"
                aria-label={`收起${headingTitle}`}
                onClick={() => setIsCollapsed(true)}
              >
                收起
              </button>
            </div>
          </div>

          <div className="action-dock__tableau">
            <div className="action-dock__hand-zone">
              {hand.length > 0 ? (
                <div className="action-dock__hand" aria-label="Local hand">
                  {hand.map((tile, index) => (
                    <button
                      key={`${tile.tileId}-${index}`}
                      type="button"
                      className="action-dock__tile"
                      onClick={() => onTileSelect(tile.tileId)}
                    >
                      <MahjongTile
                        code={tile.code}
                        variant="hand"
                        isSelected={tile.isSelected}
                        isDrawn={tile.isDrawn}
                      />
                    </button>
                  ))}
                </div>
              ) : (
                <div className="action-dock__empty">牌桌进入对局后，手牌和操作按钮会显示在这里。</div>
              )}
            </div>
            <div className="action-dock__meld-zone">
              <span className="action-dock__meld-eyebrow">副露区</span>
              <MeldRack
                seat="local"
                melds={localPlayer?.melds ?? []}
                ariaLabel="Local melds"
                emptyLabel="暂无副露"
              />
            </div>
          </div>

          <div className="action-dock__actions">
            {actions.map((action) => (
              <button
                key={action.id}
                type="button"
                disabled={!action.enabled}
                className={`action-dock__action action-dock__action--${action.emphasis}`}
                onClick={() => onAction(action.id)}
              >
                {action.label}
              </button>
            ))}
          </div>
        </section>
      ) : null}
      {isCollapsed ? (
        <button
          type="button"
          className="action-dock__restore"
          aria-label={`展开${headingTitle}`}
          onClick={() => setIsCollapsed(false)}
        >
          展开手牌区
        </button>
      ) : null}
    </>
  );
}

const WIND_LABELS: Record<PlayerView['wind'], string> = {
  East: '东',
  South: '南',
  West: '西',
  North: '北',
};
