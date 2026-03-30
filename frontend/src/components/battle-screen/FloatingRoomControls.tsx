import { useState } from 'react';
import { createPortal } from 'react-dom';

import type { BattleActionView } from '../../types/match';

interface FloatingRoomControlsProps {
  actions: BattleActionView[];
  onAction: (actionId: BattleActionView['id']) => void;
}

export function FloatingRoomControls({ actions, onAction }: FloatingRoomControlsProps) {
  const [isCollapsed, setIsCollapsed] = useState(false);

  if (actions.length === 0) {
    return null;
  }

  const portalTarget = typeof document !== 'undefined' ? document.body : null;

  const content = (
    <>
      {!isCollapsed ? (
        <aside className="room-control-window" aria-label="房间操作窗口">
          <div className="room-control-window__titlebar">
            <div>
              <span className="room-control-window__eyebrow">Room Ops</span>
              <strong>房间操作</strong>
            </div>
            <button
              type="button"
              className="room-control-window__collapse"
              aria-label="收起房间操作窗口"
              onClick={() => setIsCollapsed(true)}
            >
              收起
            </button>
          </div>
          <div className="room-control-window__actions">
            {actions.map((action) => (
              <button
                key={action.id}
                type="button"
                disabled={!action.enabled}
                className={`room-control-window__action room-control-window__action--${action.emphasis}`}
                onClick={() => onAction(action.id)}
              >
                {action.label}
              </button>
            ))}
          </div>
        </aside>
      ) : null}
      {isCollapsed ? (
        <button
          type="button"
          className="room-control-window__restore"
          aria-label="展开房间操作窗口"
          onClick={() => setIsCollapsed(false)}
        >
          房间操作
        </button>
      ) : null}
    </>
  );

  return portalTarget ? createPortal(content, portalTarget) : content;
}
