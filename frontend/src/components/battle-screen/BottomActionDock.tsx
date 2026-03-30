import { useState } from 'react';
import { createPortal } from 'react-dom';

import type { BattleActionView, BattleViewModel, PlayerView } from '../../types/match';
import { MahjongTile } from './MahjongTile';

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
  const presenceLabel = localPlayer ? (localPlayer.isBotControlled ? '离线' : localPlayer.connected ? '在线' : '离线') : null;
  const dockLabel = '手牌区';
  const portalTarget = typeof document !== 'undefined' ? document.body : null;

  const content = (
    <>
      {!isCollapsed ? (
        <section
          className={`action-dock ${isElevated ? 'action-dock--elevated' : ''}`}
          data-testid="action-dock"
          data-elevated={isElevated}
        >
          <div className="action-dock__tableau action-dock__tableau--full">
            <div className="action-dock__hand-zone">
              {hand.length > 0 ? (
                <div className="action-dock__hand" aria-label="Local hand">
                  {hand.map((tile, index) => (
                    <button
                      key={`${tile.tileId}-${index}`}
                      type="button"
                      className={tile.isSelected ? 'action-dock__tile action-dock__tile--selected' : 'action-dock__tile'}
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
            <div className="action-dock__info-rail">
              {localPlayer ? (
                <div className="action-dock__player">
                  <span className="action-dock__player-eyebrow">
                    {windLabel}
                    {localPlayer.isDealer ? ' 庄家' : ''}
                    {presenceLabel ? ` ${presenceLabel}` : ''}
                  </span>
                  <strong>{localPlayer.name}</strong>
                  <span className="action-dock__player-meta">
                    {localPlayer.score.toLocaleString()} · 花 {localPlayer.flowerCount} · {localPlayer.statusText ?? '就绪'}
                  </span>
                </div>
              ) : (
                <span className="action-dock__badge">{waitingControls?.isReady ? '已准备' : '待命中'}</span>
              )}
            </div>
          </div>

          {actions.length > 0 ? (
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
              <button
                type="button"
                className="action-dock__action action-dock__action--medium action-dock__action--collapse"
                aria-label={`收起${dockLabel}`}
                onClick={() => setIsCollapsed(true)}
              >
                收起
              </button>
            </div>
          ) : (
            <div className="action-dock__actions action-dock__actions--solo">
              <button
                type="button"
                className="action-dock__action action-dock__action--medium action-dock__action--collapse"
                aria-label={`收起${dockLabel}`}
                onClick={() => setIsCollapsed(true)}
              >
                收起
              </button>
            </div>
          )}
        </section>
      ) : null}
      {isCollapsed ? (
        <button
          type="button"
          className="action-dock__restore"
          aria-label={`展开${dockLabel}`}
          onClick={() => setIsCollapsed(false)}
        >
          展开手牌区
        </button>
      ) : null}
    </>
  );

  return portalTarget ? createPortal(content, portalTarget) : content;
}

const WIND_LABELS: Record<PlayerView['wind'], string> = {
  East: '东',
  South: '南',
  West: '西',
  North: '北',
};
