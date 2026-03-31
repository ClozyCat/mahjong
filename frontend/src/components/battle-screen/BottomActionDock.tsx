import type { CSSProperties } from 'react';
import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';

import type { BackendActionType, BattleActionView, BattlePromptView, BattleViewModel } from '../../types/match';
import { MahjongTile } from './MahjongTile';

interface BottomActionDockProps {
  hand: BattleViewModel['localHand'];
  actions: BattleActionView[];
  isElevated: boolean;
  promptCue: BattlePromptView | null;
  deadlineAt: string | null;
  onTileSelect: (tileId: string) => void;
  onAction: (actionId: BattleActionView['id']) => void;
}

export function BottomActionDock({
  hand,
  actions,
  isElevated,
  promptCue,
  deadlineAt,
  onTileSelect,
  onAction,
}: BottomActionDockProps) {
  const [isCollapsed, setIsCollapsed] = useState(false);
  const [remainingSeconds, setRemainingSeconds] = useState<number | null>(null);
  const handCount = hand.length;
  const dockLabel = '手牌区';
  const portalTarget = typeof document !== 'undefined' ? document.body : null;
  const dockStyle = {
    '--action-dock-hand-count': `${handCount}`,
    '--action-dock-effective-hand-count': `${Math.max(handCount, 1)}`,
    '--action-dock-gap-count': `${Math.max(handCount - 1, 0)}`,
  } as CSSProperties;
  const visibleActions = actions
    .filter((action) => {
      if (!action.enabled) {
        return false;
      }

      if (!promptCue) {
        return true;
      }

      return promptCue.actionIds.includes(action.id as BackendActionType);
    })
    .sort(
      (left, right) =>
        (ACTION_PRIORITY[left.id as BackendActionType] ?? Number.MAX_SAFE_INTEGER) -
        (ACTION_PRIORITY[right.id as BackendActionType] ?? Number.MAX_SAFE_INTEGER),
    );
  const shouldElevateDock = isElevated && !isResponsePrompt(promptCue);

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

  const content = (
    <>
      {!isCollapsed ? (
        <section
          className={`action-dock ${shouldElevateDock ? 'action-dock--elevated' : ''}`}
          data-testid="action-dock"
          data-elevated={shouldElevateDock}
          style={dockStyle}
        >
          {visibleActions.length > 0 ? (
            <div className="action-dock__actions" aria-label="即时操作按钮">
              {visibleActions.map((action) => {
                const isPassAction = action.id === 'pass';
                const responseGlowClassName = getResponseGlowClassName(promptCue, action.id as BackendActionType);

                return (
                  <button
                    key={action.id}
                    type="button"
                    className={`action-dock__action action-dock__action--response ${responseGlowClassName} ${
                      isPassAction ? 'action-dock__action--passive' : ''
                    }`.trim()}
                    onClick={() => onAction(action.id)}
                  >
                    {action.label}
                  </button>
                );
              })}
            </div>
          ) : null}
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
              {remainingSeconds !== null ? (
                <div
                  className={`action-dock__countdown ${remainingSeconds <= 3 ? 'action-dock__countdown--critical' : ''}`}
                  aria-label={`剩余 ${remainingSeconds} 秒`}
                >
                  <span>倒计时</span>
                  <strong>{remainingSeconds}</strong>
                </div>
              ) : null}
              <button
                type="button"
                className="action-dock__action action-dock__action--medium action-dock__action--collapse"
                aria-label={`收起${dockLabel}`}
                onClick={() => setIsCollapsed(true)}
              >
                收起
              </button>
            </div>
          </div>
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

const ACTION_PRIORITY: Partial<Record<BackendActionType, number>> = {
  hu: 0,
  kong: 1,
  pung: 2,
  chow: 3,
  flower: 4,
  discard: 5,
  pass: 6,
};

function isResponsePrompt(promptCue: BattlePromptView | null) {
  return promptCue?.kind === 'claim' || promptCue?.kind === 'rob_kong';
}

function getResponseGlowClassName(promptCue: BattlePromptView | null, actionId: BackendActionType) {
  if (!promptCue || !isResponsePrompt(promptCue) || actionId === 'pass' || !promptCue.highlightedActionIds.includes(actionId)) {
    return '';
  }

  if (actionId === 'hu') {
    return 'action-dock__action--response-glow action-dock__action--response-glow-hu';
  }

  if (actionId === 'kong') {
    return 'action-dock__action--response-glow action-dock__action--response-glow-kong';
  }

  if (actionId === 'pung') {
    return 'action-dock__action--response-glow action-dock__action--response-glow-pung';
  }

  if (actionId === 'chow') {
    return 'action-dock__action--response-glow action-dock__action--response-glow-chow';
  }

  return '';
}
