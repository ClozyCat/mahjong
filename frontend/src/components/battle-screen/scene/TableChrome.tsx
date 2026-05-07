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
  isBgmEnabled?: boolean;
  onToggleBgm?: () => void;
  isVoiceEnabled?: boolean;
  onToggleVoice?: () => void;
  isBotTakeoverEnabled?: boolean;
  onToggleBotTakeover?: (enabled: boolean) => void;
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
  isBgmEnabled = false,
  onToggleBgm,
  isVoiceEnabled = true,
  onToggleVoice,
  isBotTakeoverEnabled = false,
  onToggleBotTakeover,
}: TableChromeProps) {
  const [isFanGuideOpen, setIsFanGuideOpen] = useState(false);
  const [areQuickSettingsOpen, setAreQuickSettingsOpen] = useState(false);
  const [isFullscreen, setIsFullscreen] = useState(false);
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

  useEffect(() => {
    const handleFullscreenChange = () => {
      setIsFullscreen(!!document.fullscreenElement);
    };

    setIsFullscreen(!!document.fullscreenElement);
    document.addEventListener('fullscreenchange', handleFullscreenChange);
    return () => document.removeEventListener('fullscreenchange', handleFullscreenChange);
  }, []);

  const handleToggleFullscreen = async () => {
    try {
      if (!document.fullscreenElement) {
        await document.documentElement.requestFullscreen();
      } else {
        await document.exitFullscreen();
      }
    } catch (err) {
      console.error('Error toggling fullscreen:', err);
    }
  };

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
          className={`table-stage__settings-button ${areQuickSettingsOpen ? 'table-stage__settings-button--active' : ''}`.trim()}
          aria-label={areQuickSettingsOpen ? '收起牌桌快捷设置' : '展开牌桌快捷设置'}
          aria-expanded={areQuickSettingsOpen}
          title={areQuickSettingsOpen ? '收起' : '展开设置'}
          onClick={() => setAreQuickSettingsOpen((current) => !current)}
        >
          <svg className="table-stage__icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <path d={areQuickSettingsOpen ? "m18 15-6-6-6 6" : "m6 9 6 6 6-6"} />
          </svg>
        </button>
        <button
          type="button"
          className="table-stage__help-button"
          aria-label="打开国标麻将番种说明"
          title="番种说明"
          onClick={() => setIsFanGuideOpen(true)}
        >
          <svg className="table-stage__icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <path d="M12 17h.01" />
            <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3" />
          </svg>
        </button>
        {canLeaveTable ? (
          <button
            type="button"
            className="table-stage__leave-button"
            aria-label="快捷离开牌桌"
            title="离开牌桌"
            onClick={onLeaveTable}
          >
            <svg className="table-stage__icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d="M14 6h3a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1h-3" />
              <path d="m10 15 3-3-3-3" />
              <path d="M13 12H6" />
            </svg>
          </button>
        ) : null}
        {areQuickSettingsOpen ? (
          <div className="table-stage__quick-settings" role="group" aria-label="牌桌快捷设置">
            <button
              type="button"
              className={`table-stage__quick-setting table-stage__quick-setting--fullscreen ${isFullscreen ? 'table-stage__quick-setting--active' : ''}`.trim()}
              aria-label="全屏切换"
              aria-pressed={isFullscreen}
              title={isFullscreen ? '退出全屏' : '全屏显示'}
              onClick={handleToggleFullscreen}
            >
              <svg className="table-stage__icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                {isFullscreen ? (
                  <>
                    <path d="M8 3v3a2 2 0 0 1-2 2H3" />
                    <path d="M21 8h-3a2 2 0 0 1-2-2V3" />
                    <path d="M3 16h3a2 2 0 0 1 2 2v3" />
                    <path d="M16 21v-3a2 2 0 0 1 2-2h3" />
                  </>
                ) : (
                  <>
                    <path d="M8 3H5a2 2 0 0 0-2 2v3" />
                    <path d="M21 8V5a2 2 0 0 0-2-2h-3" />
                    <path d="M3 16v3a2 2 0 0 0 2 2h3" />
                    <path d="M16 21h3a2 2 0 0 0 2-2v-3" />
                  </>
                )}
              </svg>
            </button>
            <button
              type="button"
              className={`table-stage__quick-setting table-stage__quick-setting--music ${isBgmEnabled ? 'table-stage__quick-setting--active' : ''}`.trim()}
              aria-label="音乐开关"
              aria-pressed={isBgmEnabled}
              title={isBgmEnabled ? '关闭背景音乐' : '开启背景音乐'}
              onClick={onToggleBgm}
            >
              <svg className="table-stage__icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <path d="M9 18V5l12-2v13" />
                <circle cx="6" cy="18" r="3" />
                <circle cx="18" cy="16" r="3" />
              </svg>
            </button>
            <button
              type="button"
              className={`table-stage__quick-setting table-stage__quick-setting--voice ${isVoiceEnabled ? 'table-stage__quick-setting--active' : ''}`.trim()}
              aria-label="语音开关"
              aria-pressed={isVoiceEnabled}
              title={isVoiceEnabled ? '关闭语音' : '开启语音'}
              onClick={onToggleVoice}
            >
              <svg className="table-stage__icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" />
                <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
                <line x1="12" y1="19" x2="12" y2="22" />
              </svg>
            </button>
            <button
              type="button"
              className={`table-stage__quick-setting table-stage__quick-setting--bot ${isBotTakeoverEnabled ? 'table-stage__quick-setting--active' : ''}`.trim()}
              aria-label="BOT代打"
              aria-pressed={isBotTakeoverEnabled}
              title={isBotTakeoverEnabled ? '切换为人类操控' : '交给 BOT 代打'}
              onClick={() => onToggleBotTakeover?.(!isBotTakeoverEnabled)}
            >
              <svg className="table-stage__icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <rect width="18" height="10" x="3" y="11" rx="2" />
                <circle cx="12" cy="5" r="2" />
                <path d="M12 7v4" />
                <line x1="8" y1="16" x2="8" y2="16" />
                <line x1="16" y1="16" x2="16" y2="16" />
              </svg>
            </button>
            {onCycleTheme ? (
              <button
                type="button"
                className="table-stage__quick-setting table-stage__quick-setting--theme"
                data-theme={themeId}
                aria-label="换主题"
                title={`切换配色：${themeLabel}`}
                onClick={onCycleTheme}
              >
                <svg className="table-stage__icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                  <circle cx="13.5" cy="6.5" r=".5" />
                  <circle cx="17.5" cy="10.5" r=".5" />
                  <circle cx="8.5" cy="7.5" r=".5" />
                  <circle cx="6.5" cy="12.5" r=".5" />
                  <path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.9 0 1.5-.6 1.5-1.5 0-.4-.1-.8-.4-1.1-.3-.3-.4-.7-.4-1.1 0-.9.7-1.5 1.5-1.5H16c3.3 0 6-2.7 6-6 0-4.9-4.5-9-10-9Z" />
                </svg>
              </button>
            ) : null}
          </div>
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
