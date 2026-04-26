import { memo, useEffect, useRef, useState, type CSSProperties, type PointerEvent as ReactPointerEvent } from 'react';

import type { ThemeId } from '../../../lib/themes';
import type { BattleActionView } from '../../../types/match';
import { FAN_GUIDE_ENTRIES, type FanGuideEntry } from '../fanGuide';
import { FanGuideDialog } from '../FanGuideDialog';

interface TableChromeProps {
  tableCode: string;
  resolvedOccupiedSeatCount: number;
  seatCapacity: number;
  themeId: ThemeId;
  themeLabel: string;
  tableSummary: string | null;
  preMatchActions: BattleActionView[];
  botCount: number;
  canAddBot: boolean;
  canRemoveBot: boolean;
  canLeaveTable: boolean;
  onLeaveTable?: () => void;
  onCycleTheme?: () => void;
  onAction?: (actionId: BattleActionView['id']) => void;
  onAddBot?: () => void;
  onRemoveBot?: () => void;
}

export const TableChrome = memo(function TableChrome({
  tableCode,
  resolvedOccupiedSeatCount,
  seatCapacity,
  themeId,
  themeLabel,
  tableSummary,
  preMatchActions,
  botCount,
  canAddBot,
  canRemoveBot,
  canLeaveTable,
  onLeaveTable,
  onCycleTheme,
  onAction,
  onAddBot,
  onRemoveBot,
}: TableChromeProps) {
  const [isFanGuideOpen, setIsFanGuideOpen] = useState(false);
  const [pinnedFanKeys, setPinnedFanKeys] = useState<string[]>(() => {
    if (typeof window === 'undefined') {
      return [];
    }

    const stored = localStorage.getItem('mahjong_pinned_fans');
    if (!stored) {
      return [];
    }

    try {
      return JSON.parse(stored) as string[];
    } catch {
      return [];
    }
  });

  useEffect(() => {
    if (pinnedFanKeys.length > 0) {
      localStorage.setItem('mahjong_pinned_fans', JSON.stringify(pinnedFanKeys));
      return;
    }

    localStorage.removeItem('mahjong_pinned_fans');
  }, [pinnedFanKeys]);

  const shouldShowPreMatchActions = preMatchActions.length > 0;
  const shouldShowBotControls =
    shouldShowPreMatchActions || botCount > 0 || canAddBot || canRemoveBot;

  return (
    <>
      {tableCode || seatCapacity > 0 ? (
        <div className="table-stage__table-info" aria-label="牌桌信息">
          {tableCode ? <span>牌桌编号：{tableCode}</span> : null}
          <span>
            房间座位数：{resolvedOccupiedSeatCount}/{seatCapacity}
          </span>
        </div>
      ) : null}
      <div className="table-stage__corner-controls">
        <button
          type="button"
          className="table-stage__help-button"
          aria-label="打开国标麻将番种说明"
          title="番种说明"
          onClick={() => setIsFanGuideOpen(true)}
        >
          <span aria-hidden="true">?</span>
        </button>
        {onCycleTheme ? (
          <button
            type="button"
            className="table-stage__theme-button"
            data-theme={themeId}
            aria-label={`切换整体配色，当前 ${themeLabel}`}
            title={`切换配色：${themeLabel}`}
            onClick={onCycleTheme}
          >
            <span aria-hidden="true">换</span>
          </button>
        ) : null}
        {canLeaveTable ? (
          <button
            type="button"
            className="table-stage__leave-button"
            aria-label="快捷离开牌桌"
            onClick={onLeaveTable}
          >
            <span aria-hidden="true">×</span>
          </button>
        ) : null}
      </div>
      {shouldShowPreMatchActions || shouldShowBotControls ? (
        <div className="table-stage__lobby-controls">
          {shouldShowPreMatchActions ? (
            <div className="table-stage__room-actions" role="group" aria-label="开局前房间操作">
              {preMatchActions.map((action) => (
                <button
                  key={action.id}
                  type="button"
                  className={`table-stage__room-action table-stage__room-action--${action.emphasis}`}
                  disabled={!action.enabled}
                  onClick={() => onAction?.(action.id)}
                >
                  {action.label}
                </button>
              ))}
            </div>
          ) : null}
          {shouldShowBotControls ? (
            <div className="table-stage__bot-controls" role="group" aria-label="BOT 数量控制">
              <span className="table-stage__bot-label">BOT 数量</span>
              <button
                type="button"
                className="table-stage__bot-button"
                aria-label="减少 BOT"
                disabled={!canRemoveBot}
                onClick={onRemoveBot}
              >
                -
              </button>
              <strong className="table-stage__bot-count" aria-label={`当前 BOT 数量 ${botCount}`}>
                {botCount}
              </strong>
              <button
                type="button"
                className="table-stage__bot-button"
                aria-label="增加 BOT"
                disabled={!canAddBot}
                onClick={onAddBot}
              >
                +
              </button>
            </div>
          ) : null}
        </div>
      ) : null}
      {tableSummary ? <div className="table-stage__status-summary">{tableSummary}</div> : null}
      <FanGuideDialog
        isOpen={isFanGuideOpen}
        onClose={() => setIsFanGuideOpen(false)}
        pinnedFanKeys={pinnedFanKeys}
        onPinFan={(key) =>
          setPinnedFanKeys((previousKeys) =>
            previousKeys.includes(key)
              ? previousKeys.filter((previousKey) => previousKey !== key)
              : [...previousKeys, key],
          )
        }
      />
      {pinnedFanKeys.length > 0 ? (
        <PinnedFanOverlay
          entries={pinnedFanKeys
            .map((key) => FAN_GUIDE_ENTRIES.find((entry) => entry.fanKey === key))
            .filter((entry): entry is FanGuideEntry => Boolean(entry))}
          onRemove={(key) =>
            setPinnedFanKeys((previousKeys) =>
              previousKeys.filter((previousKey) => previousKey !== key),
            )
          }
        />
      ) : null}
    </>
  );
});

function PinnedFanOverlay({
  entries,
  onRemove,
}: {
  entries: FanGuideEntry[];
  onRemove: (key: string) => void;
}) {
  const [position, setPosition] = useState(() => {
    if (typeof window === 'undefined') {
      return { x: 20, y: 80 };
    }

    const stored = localStorage.getItem('mahjong_pinned_fan_pos');
    if (!stored) {
      return { x: 20, y: 80 };
    }

    try {
      return JSON.parse(stored) as { x: number; y: number };
    } catch {
      return { x: 20, y: 80 };
    }
  });
  const [isDragging, setIsDragging] = useState(false);
  const dragStartPos = useRef({ x: 0, y: 0 });

  useEffect(() => {
    localStorage.setItem('mahjong_pinned_fan_pos', JSON.stringify(position));
  }, [position]);

  const handlePointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if ((event.target as HTMLElement).closest('.pinned-fan-overlay__close')) {
      return;
    }

    setIsDragging(true);
    dragStartPos.current = { x: event.clientX - position.x, y: event.clientY - position.y };
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const handlePointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (!isDragging) {
      return;
    }

    const nextX = event.clientX - dragStartPos.current.x;
    const nextY = event.clientY - dragStartPos.current.y;
    const x = Math.max(0, Math.min(window.innerWidth - 100, nextX));
    const y = Math.max(0, Math.min(window.innerHeight - 100, nextY));
    setPosition({ x, y });
  };

  const handlePointerUp = (event: ReactPointerEvent<HTMLDivElement>) => {
    setIsDragging(false);
    event.currentTarget.releasePointerCapture(event.pointerId);
  };

  return (
    <div
      className={`pinned-fan-list ${isDragging ? 'pinned-fan-list--dragging' : ''}`.trim()}
      style={{ left: position.x, top: position.y } as CSSProperties}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
    >
      <div className="pinned-fan-list__handle">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round">
          <line x1="8" y1="9" x2="16" y2="9" />
          <line x1="8" y1="15" x2="16" y2="15" />
        </svg>
      </div>
      <div className="pinned-fan-list__items">
        {entries.map((entry) => (
          <div key={entry.fanKey} className="pinned-fan-overlay">
            <div className="pinned-fan-overlay__header">
              <strong className="pinned-fan-overlay__title">{entry.label}</strong>
              <div className="pinned-fan-overlay__fan-value">
                <span>{entry.fanValue}</span>
                <small>番</small>
              </div>
              <button
                type="button"
                className="pinned-fan-overlay__close"
                onClick={() => onRemove(entry.fanKey)}
                aria-label="取消固定"
              >
                ×
              </button>
            </div>
            <div className="pinned-fan-overlay__body">
              <p>{entry.intro}</p>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
