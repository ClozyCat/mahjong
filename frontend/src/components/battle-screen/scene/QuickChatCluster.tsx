import { memo, useEffect, useRef, useState, type CSSProperties } from 'react';

import type { QuickChatEmoji } from '../../../types/match';

interface QuickChatClusterProps {
  localPlayerName: string;
  localPlayerAbsoluteSeat: number | null;
  onQuickChat?: (targetSeat: number, emoji: QuickChatEmoji) => void;
}

const QUICK_CHAT_TEXT_LIMIT = 50;
const QUICK_CHAT_ITEMS: Array<{ emoji: QuickChatEmoji; label: string }> = [
  { emoji: '😄', label: '笑' },
  { emoji: '😭', label: '哭' },
  { emoji: '😡', label: '生气' },
  { emoji: '🙏', label: '谢谢' },
  { emoji: '🍵', label: '喝茶' },
];

function clampQuickChatText(value: string) {
  return Array.from(value).slice(0, QUICK_CHAT_TEXT_LIMIT).join('');
}

function normalizeQuickChatText(value: string) {
  return clampQuickChatText(value).trim();
}

function getQuickChatItemStyle(index: number): CSSProperties {
  const itemHeightRem = 2.8;
  const gapRem = 0.4;
  const offsetRem = (index + 1) * (itemHeightRem + gapRem);

  return {
    '--quick-chat-x': '0rem',
    '--quick-chat-y': `-${offsetRem}rem`,
  } as CSSProperties;
}

export const QuickChatCluster = memo(function QuickChatCluster({
  localPlayerName,
  localPlayerAbsoluteSeat,
  onQuickChat,
}: QuickChatClusterProps) {
  const [isQuickChatOpen, setIsQuickChatOpen] = useState(false);

  useEffect(() => {
    if (!isQuickChatOpen) {
      return undefined;
    }

    function handlePointerDown(event: PointerEvent) {
      const target = event.target;
      if (target instanceof Element && target.closest('[data-quick-chat-root="true"]')) {
        return;
      }

      setIsQuickChatOpen(false);
    }

    document.addEventListener('pointerdown', handlePointerDown);
    return () => document.removeEventListener('pointerdown', handlePointerDown);
  }, [isQuickChatOpen]);

  return (
    <div className="table-stage__global-emoji-cluster" data-quick-chat-root="true">
      <button
        type="button"
        className={`table-stage__global-emoji-trigger ${isQuickChatOpen ? 'table-stage__global-emoji-trigger--open' : ''}`.trim()}
        aria-label="打开快捷表情"
        aria-expanded={isQuickChatOpen}
        onClick={() => setIsQuickChatOpen((current) => !current)}
      >
        {isQuickChatOpen ? '×' : '🍵'}
      </button>
      {isQuickChatOpen ? (
        <QuickChatMenu
          seat="bottom"
          playerName={localPlayerName}
          isLocalTarget
          onSelect={(emoji) => {
            if (typeof localPlayerAbsoluteSeat === 'number') {
              onQuickChat?.(localPlayerAbsoluteSeat, emoji);
            }

            setIsQuickChatOpen(false);
          }}
        />
      ) : null}
    </div>
  );
});

interface QuickChatMenuProps {
  seat: 'bottom';
  playerName: string;
  isLocalTarget?: boolean;
  onSelect: (emoji: QuickChatEmoji) => void;
}

function QuickChatMenu({ seat, playerName, isLocalTarget = false, onSelect }: QuickChatMenuProps) {
  const menuId = `table-stage-quick-chat-${seat}`;
  const [isComposerOpen, setIsComposerOpen] = useState(false);
  const [draft, setDraft] = useState('');
  const [isComposing, setIsComposing] = useState(false);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (!isComposerOpen) {
      return;
    }

    inputRef.current?.focus();
  }, [isComposerOpen]);

  function submitDraft() {
    const nextMessage = normalizeQuickChatText(draft);
    if (!nextMessage) {
      return;
    }

    onSelect(nextMessage);
  }

  return (
    <div
      id={menuId}
      className={`table-stage__quick-chat-menu table-stage__quick-chat-menu--${seat}`}
      role="menu"
      aria-label={`${playerName} 快捷表情`}
      onPointerDown={(event) => event.stopPropagation()}
      onClick={(event) => event.stopPropagation()}
    >
      <button
        type="button"
        className={`table-stage__quick-chat-item ${isComposerOpen ? 'table-stage__quick-chat-item--active' : ''}`.trim()}
        role="menuitem"
        aria-label="发送自定义文字"
        title="发送自定义文字"
        style={getQuickChatItemStyle(0)}
        onClick={(event) => {
          event.stopPropagation();
          setIsComposerOpen((current) => !current);
          setDraft((current) => (isComposerOpen ? '' : current));
        }}
      >
        <span aria-hidden="true">+</span>
      </button>
      {QUICK_CHAT_ITEMS.map((item, index) => (
        <button
          key={`${seat}-${item.label}`}
          type="button"
          className="table-stage__quick-chat-item"
          role="menuitem"
          aria-label={`发送${item.label}表情`}
          title={`发送${item.label}表情`}
          style={getQuickChatItemStyle(index + 1)}
          onClick={(event) => {
            event.stopPropagation();
            onSelect(item.emoji);
          }}
        >
          <span aria-hidden="true">{item.emoji}</span>
        </button>
      ))}
      {isComposerOpen ? (
        <form
          className="table-stage__quick-chat-composer"
          onSubmit={(event) => {
            event.preventDefault();
            event.stopPropagation();
            submitDraft();
          }}
        >
          <input
            ref={inputRef}
            type="text"
            className="table-stage__quick-chat-input"
            aria-label={`${isLocalTarget ? '输入' : `向${playerName}发送`}快捷文字`}
            placeholder="输入文字"
            value={draft}
            onChange={(event) => setDraft(clampQuickChatText(event.target.value))}
            onCompositionStart={() => setIsComposing(true)}
            onCompositionEnd={() => setIsComposing(false)}
            onKeyDown={(event) => {
              if (event.key === 'Escape') {
                event.preventDefault();
                event.stopPropagation();
                setIsComposerOpen(false);
                setDraft('');
                return;
              }

              if (event.key !== 'Enter' || event.nativeEvent.isComposing || isComposing) {
                return;
              }

              event.preventDefault();
              event.stopPropagation();
              submitDraft();
            }}
          />
          <span className="table-stage__quick-chat-counter" aria-hidden="true">
            {Array.from(draft).length}/{QUICK_CHAT_TEXT_LIMIT}
          </span>
        </form>
      ) : null}
    </div>
  );
}
