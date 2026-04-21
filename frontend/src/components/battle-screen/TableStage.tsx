import { Fragment, useEffect, useRef, useState, type CSSProperties } from 'react';

import type { ThemeId } from '../../lib/themes';
import type {
  ActionEffectView,
  BattleActionView,
  BattlePromptView,
  PlayerView,
  QuickChatEmoji,
  QuickChatEventView,
  Seat,
} from '../../types/match';
import { FanGuideDialog } from './FanGuideDialog';
import { MahjongTile } from './MahjongTile';
import { MeldRack } from './MeldRack';
import { SETTLEMENT_CALLOUT_DURATION_CSS, SETTLEMENT_CALLOUT_LINGER_MS } from './settlementTiming';

export type TableStagePlayer = Pick<PlayerView, 'seat' | 'name' | 'melds'> &
  Partial<Omit<PlayerView, 'seat' | 'name' | 'melds'>>;

const PENDING_ACTION_DURATION_MS = 30_000;
const COUNTDOWN_RING_STROKE_WIDTH = 3;

interface TableStageProps {
  discards: Record<Seat, string[]>;
  selectedTileCode?: string | null;
  activeSeat: Seat;
  actionIndicatorSeat?: Seat | null;
  lastDiscard: string | null;
  lastDiscardSeat?: Seat | null;
  settlementWinnerSeat?: Seat | null;
  settlementWinType?: string | null;
  settlementWinTypeLabel?: string | null;
  centerStatusText?: string | null;
  remainingTileCount?: number | null;
  promptText: string | null;
  promptCue?: BattlePromptView | null;
  deadlineAt?: string | null;
  actionEffect?: ActionEffectView | null;
  quickChatEvent?: QuickChatEventView | null;
  players?: TableStagePlayer[];
  settlementHands?: Partial<Record<Seat, string[]>> | null;
  settlementCenterCalloutLabel?: string | null;
  tableCode?: string;
  roundLabel?: string;
  phaseLabel?: string;
  occupiedSeatCount?: number;
  seatCapacity?: number;
  preMatchActions?: BattleActionView[];
  botCount?: number;
  canAddBot?: boolean;
  canRemoveBot?: boolean;
  tileScale?: number;
  canDecreaseTileScale?: boolean;
  canIncreaseTileScale?: boolean;
  canLeaveTable?: boolean;
  themeId?: ThemeId;
  themeLabel?: string;
  onLeaveTable?: () => void;
  onCycleTheme?: () => void;
  onAction?: (actionId: BattleActionView['id']) => void;
  onAddBot?: () => void;
  onRemoveBot?: () => void;
  onQuickChat?: (targetSeat: number, emoji: QuickChatEmoji) => void;
  onDecreaseTileScale?: () => void;
  onIncreaseTileScale?: () => void;
}

const SEATS: Seat[] = ['top', 'left', 'right', 'bottom'];
const SETTLEMENT_HAND_COLUMN_COUNT: Record<Seat, number> = {
  top: 7,
  left: 4,
  right: 4,
  bottom: 7,
};

export function TableStage({
  discards,
  selectedTileCode = null,
  activeSeat,
  actionIndicatorSeat = null,
  lastDiscard,
  lastDiscardSeat = null,
  settlementWinnerSeat = null,
  settlementWinType = null,
  settlementWinTypeLabel = null,
  centerStatusText = null,
  remainingTileCount = null,
  promptText,
  promptCue = null,
  deadlineAt = null,
  actionEffect = null,
  quickChatEvent = null,
  players = [],
  settlementHands = null,
  settlementCenterCalloutLabel = null,
  tableCode = '',
  roundLabel = '',
  phaseLabel = '',
  occupiedSeatCount,
  seatCapacity = 4,
  preMatchActions = [],
  botCount = 0,
  canAddBot = false,
  canRemoveBot = false,
  tileScale = 1,
  canDecreaseTileScale = false,
  canIncreaseTileScale = false,
  canLeaveTable = false,
  themeId = 'tian-shui-bi',
  themeLabel = '天水碧',
  onLeaveTable,
  onCycleTheme,
  onAction,
  onAddBot,
  onRemoveBot,
  onQuickChat,
  onDecreaseTileScale,
  onIncreaseTileScale,
}: TableStageProps) {
  const lastDiscardPosition = findLastDiscardPosition(discards, lastDiscard, lastDiscardSeat);
  const playerBySeat = new Map(players.map((player) => [player.seat, player]));
  const hasSettlementHands = Object.values(settlementHands ?? {}).some((tiles) => tiles.length > 0);
  const [activeActionCallout, setActiveActionCallout] = useState<ActionCallout | null>(null);
  const [pendingActionCallouts, setPendingActionCallouts] = useState<ActionCallout[]>([]);
  const [exitingActionCallout, setExitingActionCallout] = useState<ActionCallout | null>(null);
  const [isQuickChatOpen, setIsQuickChatOpen] = useState(false);
  const [riverColumnsH, setRiverColumnsH] = useState(8);
  const [riverColumnsV, setRiverColumnsV] = useState(8);
  const [meldRowsH, setMeldRowsH] = useState(2);
  const [meldColsV, setMeldColsV] = useState(1);
  const [isFanGuideOpen, setIsFanGuideOpen] = useState(false);
  const [barrageMessages, setBarrageMessages] = useState<BarrageMessage[]>([]);
  const activeActionCalloutRef = useRef<ActionCallout | null>(null);
  const pendingActionCalloutsRef = useRef<ActionCallout[]>([]);
  const activeActionCalloutTimerRef = useRef<number | null>(null);
  const exitingActionCalloutTimerRef = useRef<number | null>(null);
  const trackedSpotlightKeyRef = useRef<string | null>(null);
  const consumedActionCalloutKeyRef = useRef<string | null>(null);
  const consumedQuickChatKeyRef = useRef<string | null>(quickChatEvent?.key ?? null);
  const barrageRemovalTimersRef = useRef<Map<string, number>>(new Map());
  const resolvedOccupiedSeatCount = occupiedSeatCount ?? players.length;
  const spotlightSeat = lastDiscardPosition?.seat ?? null;
  const spotlightTile = spotlightSeat !== null && lastDiscardPosition !== null
    ? discards[spotlightSeat][lastDiscardPosition.index]
    : null;
  const spotlightKey =
    spotlightSeat !== null && spotlightTile !== null && lastDiscardPosition !== null
      ? `${spotlightSeat}:${lastDiscardPosition.index}:${spotlightTile}`
      : null;
  const spotlightScale = Math.round(tileScale * 125) / 100;
  const tableSummary = buildTableSummary(roundLabel, phaseLabel);
  const shouldShowScaleControls = Boolean(onDecreaseTileScale || onIncreaseTileScale);
  const shouldShowPreMatchActions = preMatchActions.length > 0;
  const shouldShowBotControls = shouldShowPreMatchActions || botCount > 0 || canAddBot || canRemoveBot;
  const scalePercentLabel = `${Math.round(tileScale * 100)}%`;
  const centerPrimaryText =
    centerStatusText ?? (typeof remainingTileCount === 'number' ? `剩余 ${remainingTileCount} 张` : '等待开局');
  const tableStageStyle = {
    '--table-stage-tile-scale': `${tileScale}`,
    '--table-stage-spotlight-scale': `${spotlightScale}`,
    '--battle-hand-tile-width-base': 'clamp(1.38rem, 4.8vw, 3.25rem)',
    '--battle-hand-tile-width': 'calc(var(--battle-hand-tile-width-base) * var(--table-stage-tile-scale, 1))',
    '--battle-hand-tile-height': 'calc(var(--battle-hand-tile-width) * 1.57)',
    '--table-stage-river-columns': `${riverColumnsH}`,
    '--table-stage-meld-rows-h': `${meldRowsH}`,
    '--table-stage-meld-cols-v': `${meldColsV}`,
  } as CSSProperties;

  useEffect(() => {
    activeActionCalloutRef.current = activeActionCallout;
  }, [activeActionCallout]);

  useEffect(() => {
    pendingActionCalloutsRef.current = pendingActionCallouts;
  }, [pendingActionCallouts]);

  useEffect(() => {
    return () => {
      if (activeActionCalloutTimerRef.current !== null) {
        window.clearTimeout(activeActionCalloutTimerRef.current);
      }
      if (exitingActionCalloutTimerRef.current !== null) {
        window.clearTimeout(exitingActionCalloutTimerRef.current);
      }
      barrageRemovalTimersRef.current.forEach((timer) => window.clearTimeout(timer));
      barrageRemovalTimersRef.current.clear();
    };
  }, []);

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

  useEffect(() => {
    const handleResize = () => {
      const width = window.innerWidth;
      const height = window.innerHeight;
      const ratio = width / height;

      if (ratio > 1.6) {
        setRiverColumnsH(12);
        setRiverColumnsV(6);
        setMeldRowsH(2);
        setMeldColsV(1);
      } else if (ratio > 1.3) {
        setRiverColumnsH(10);
        setRiverColumnsV(6);
        setMeldRowsH(2);
        setMeldColsV(1);
      } else if (ratio < 0.8) {
        setRiverColumnsH(6);
        setRiverColumnsV(10);
        setMeldRowsH(2);
        setMeldColsV(2);
      } else {
        setRiverColumnsH(8);
        setRiverColumnsV(8);
        setMeldRowsH(2);
        setMeldColsV(1);
      }
    };

    handleResize();
    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, []);

  useEffect(() => {
    if (!actionEffect) {
      return;
    }

    const nextActionCallout = createActionCallout(
      actionEffect,
      settlementWinnerSeat,
      settlementWinType,
      settlementWinTypeLabel,
    );
    const actionCalloutKey = nextActionCallout?.key ?? actionEffect?.key;
    const currentActionCallout = activeActionCalloutRef.current;
    if (!actionCalloutKey) {
      return;
    }

    if (!nextActionCallout) {
      return;
    }

    if (consumedActionCalloutKeyRef.current === actionCalloutKey) {
      return;
    }

    if (currentActionCallout?.key === actionCalloutKey) {
      return;
    }

    if (pendingActionCalloutsRef.current.some((callout) => callout.key === actionCalloutKey)) {
      return;
    }

    const showActionCallout = (callout: ActionCallout) => {
      if (activeActionCalloutTimerRef.current !== null) {
        window.clearTimeout(activeActionCalloutTimerRef.current);
        activeActionCalloutTimerRef.current = null;
      }

      setActiveActionCallout(callout);
      activeActionCalloutTimerRef.current = window.setTimeout(() => {
        activeActionCalloutTimerRef.current = null;
        const [nextCallout, ...remainingCallouts] = pendingActionCalloutsRef.current;
        pendingActionCalloutsRef.current = remainingCallouts;
        setPendingActionCallouts(remainingCallouts);

        if (activeActionCalloutRef.current?.key !== callout.key) {
          return;
        }

        if (nextCallout) {
          showActionCallout(nextCallout);
          return;
        }

        setActiveActionCallout(null);
      }, ACTION_CALLOUT_LINGER_MS);
    };

    consumedActionCalloutKeyRef.current = actionCalloutKey;
    if (currentActionCallout) {
      const nextPendingCallouts = [...pendingActionCalloutsRef.current, nextActionCallout];
      pendingActionCalloutsRef.current = nextPendingCallouts;
      setPendingActionCallouts(nextPendingCallouts);
      return;
    }

    showActionCallout(nextActionCallout);
  }, [actionEffect, settlementWinnerSeat, settlementWinType, settlementWinTypeLabel]);

  useEffect(() => {
    if (spotlightKey === trackedSpotlightKeyRef.current) {
      return;
    }

    trackedSpotlightKeyRef.current = spotlightKey;
    const currentActionCallout = activeActionCalloutRef.current;

    if (!spotlightKey || !spotlightSeat || !currentActionCallout || currentActionCallout.seat !== spotlightSeat) {
      return;
    }

    if (activeActionCalloutTimerRef.current !== null) {
      window.clearTimeout(activeActionCalloutTimerRef.current);
      activeActionCalloutTimerRef.current = null;
    }

    pendingActionCalloutsRef.current = [];
    setPendingActionCallouts([]);
    setActiveActionCallout(null);
    if (exitingActionCalloutTimerRef.current !== null) {
      window.clearTimeout(exitingActionCalloutTimerRef.current);
      exitingActionCalloutTimerRef.current = null;
    }
    setExitingActionCallout(null);
  }, [spotlightKey, spotlightSeat]);

  useEffect(() => {
    if (!quickChatEvent?.key || consumedQuickChatKeyRef.current === quickChatEvent.key) {
      return;
    }

    consumedQuickChatKeyRef.current = quickChatEvent.key;
    const nextBarrageMessage: BarrageMessage = {
      key: quickChatEvent.key,
      text: quickChatEvent.text,
      topPercent: getRandomBarrageTopPercent(),
    };

    setBarrageMessages((current) => [...current, nextBarrageMessage]);

    const timer = window.setTimeout(() => {
      setBarrageMessages((current) => current.filter((message) => message.key !== quickChatEvent.key));
      barrageRemovalTimersRef.current.delete(quickChatEvent.key);
    }, QUICK_CHAT_BARRAGE_LINGER_MS);

    barrageRemovalTimersRef.current.set(quickChatEvent.key, timer);
  }, [quickChatEvent]);

  return (
    <section
      className="table-stage"
      aria-label="Mahjong table"
      style={tableStageStyle}
    >
      <div className="table-stage__frame">
        <div className="table-stage__core">
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
              onClick={() => {
                setIsQuickChatOpen(false);
                setIsFanGuideOpen(true);
              }}
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
          <CenterIndicator
            remainingCount={remainingTileCount}
            actionSeat={actionIndicatorSeat}
            deadlineAt={deadlineAt}
            isAmbiguous={!actionIndicatorSeat && !!remainingTileCount}
          />
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
          {barrageMessages.length > 0 ? (
            <div className="table-stage__barrage-layer" aria-hidden="true">
              {barrageMessages.map((message) => (
                <div
                  key={message.key}
                  className="table-stage__barrage-message"
                  style={{ '--table-stage-barrage-top': `${message.topPercent}%` } as CSSProperties}
                >
                  {message.text}
                </div>
              ))}
            </div>
          ) : null}
          {SEATS.map((seat) => {
            const player = playerBySeat.get(seat);
            const hasMelds = (player?.melds.length ?? 0) > 0;
            const finalHandTiles = settlementHands?.[seat] ?? [];
            const settlementHandLabel = SETTLEMENT_HAND_COPY[seat];
            const settlementWinningTileIndex =
              settlementWinType === 'discard' &&
                settlementWinnerSeat === seat &&
                lastDiscard !== null &&
                finalHandTiles.at(-1) === lastDiscard
                ? finalHandTiles.length - 1
                : -1;

            return (
              <Fragment key={`seat-zone-${seat}`}>
                <div
                  className={`table-stage__seat-zone table-stage__seat-zone--${seat}`}
                  style={
                    {
                      '--table-stage-river-columns':
                        seat === 'top' || seat === 'bottom'
                          ? `${riverColumnsH}`
                          : `${riverColumnsV}`,
                    } as CSSProperties
                  }
                >
                  {player ? (
                    <div className={`table-stage__player-edge-info table-stage__player-edge-info--${seat}`}>
                      <div className="table-stage__player-stats">
                        <div
                          className="table-stage__stat-plate table-stage__stat-plate--seat"
                          data-player-name={player.name}
                        >
                          <FanIcon className="table-stage__stat-icon" />
                          <span className="table-stage__stat-value">
                            {player?.wind ? WIND_NAME_TO_CHAR[player.wind] : WIND_COPY[seat]}
                          </span>
                        </div>
                        <div className="table-stage__stat-plate table-stage__stat-plate--score" title="分数">
                          <IngotIcon className="table-stage__stat-icon" />
                          <span className="table-stage__stat-value">{player.score?.toLocaleString() ?? 0}</span>
                        </div>
                        <div className="table-stage__stat-plate table-stage__stat-plate--flower" title="花牌数量">
                          <LotusIcon className="table-stage__stat-icon" />
                          <span className="table-stage__stat-value">{player.flowerCount ?? 0}</span>
                        </div>
                        <div className="table-stage__stat-plate table-stage__stat-plate--hand" title="手牌数量">
                          <TileStackIcon className="table-stage__stat-icon" />
                          <span className="table-stage__stat-value">{player.concealedCount ?? 0}</span>
                        </div>
                      </div>
                    </div>
                  ) : null}

                  <div className={`table-stage__seat-group table-stage__seat-group--${seat}`}>
                    <div className={`table-stage__seat-panel table-stage__seat-panel--${seat}`}>
                      <div
                        className={`table-stage__river table-stage__river--${seat} ${activeSeat === seat ? 'table-stage__river--active' : ''
                          }`}
                        data-seat={seat}
                      >
                        <div
                          className={`table-stage__river-track table-stage__river-track--${seat}`}
                          style={getRiverTrackStyle(seat)}
                        >
                          {player?.name ? (
                            <div className="table-stage__river-watermark" aria-hidden="true">
                              {player.name.charAt(0)}
                            </div>
                          ) : null}
                          {discards[seat].map((tile, index) => {
                            const isLastDiscard =
                              lastDiscardSeat === seat && index === discards[seat].length - 1;

                            // HIDE: Don't show in river while spotlighted in focus area
                            if (isLastDiscard && spotlightSeat && spotlightTile && !hasSettlementHands) {
                              return null;
                            }

                            return (
                              <MahjongTile
                                key={`river-${seat}-${index}-${tile}`}
                                code={tile}
                                variant="discard"
                                isLastDiscard={isLastDiscard}
                                relatedTileCode={selectedTileCode}
                              />
                            );
                          })}
                        </div>
                      </div>
                    </div>

                    {player && hasMelds ? (
                      <div
                        className={`table-stage__melds table-stage__melds--${seat} ${shouldPinDenseMeldRack(seat, player.melds.length)
                          ? 'table-stage__melds--dense'
                          : ''
                          }`}
                        style={getMeldRackPositionStyle(seat)}
                      >
                        <MeldRack
                          seat={seat}
                          melds={player.melds}
                          ariaLabel={`${player.name} melds`}
                          selectedTileCode={selectedTileCode}
                        />
                      </div>
                    ) : null}
                  </div>

                  {finalHandTiles.length > 0 && settlementHandLabel ? (
                    <div
                      className={`table-stage__settlement-hand table-stage__settlement-hand--${seat}`}
                      aria-label={settlementHandLabel}
                    >
                      <span className="table-stage__settlement-hand-eyebrow">{settlementHandLabel}</span>
                      <div className={`table-stage__settlement-hand-grid table-stage__settlement-hand-grid--${seat}`}>
                        {buildSettlementHandCells(seat, finalHandTiles).map((cell) =>
                          cell.kind === 'placeholder' ? (
                            <span
                              key={cell.key}
                              className="table-stage__settlement-hand-placeholder"
                              aria-hidden="true"
                            />
                          ) : (
                            <MahjongTile
                              key={cell.key}
                              code={cell.tile}
                              variant="discard"
                              isLastDiscard={cell.index === settlementWinningTileIndex}
                              relatedTileCode={selectedTileCode}
                              className="table-stage__settlement-hand-tile"
                            />
                          ),
                        )}
                      </div>
                    </div>
                  ) : null}
                </div>
              </Fragment>
            );
          })}
          <div className="table-stage__global-emoji-cluster" data-quick-chat-root="true">
            <button
              type="button"
              className={`table-stage__global-emoji-trigger ${isQuickChatOpen ? 'table-stage__global-emoji-trigger--open' : ''}`}
              aria-label="打开快捷表情"
              aria-expanded={isQuickChatOpen}
              onClick={() => setIsQuickChatOpen(!isQuickChatOpen)}
            >
              {isQuickChatOpen ? '×' : '🍵'}
            </button>
            {isQuickChatOpen ? (
              <QuickChatMenu
                seat="bottom"
                playerName={players.find((player) => player.isLocal)?.name ?? '本家'}
                isLocalTarget
                onSelect={(emoji) => {
                  const localPlayer = players.find((p) => p.isLocal);
                  if (localPlayer && typeof localPlayer.absoluteSeat === 'number') {
                    onQuickChat?.(localPlayer.absoluteSeat, emoji);
                  }
                  setIsQuickChatOpen(false);
                }}
              />
            ) : null}
          </div>
          {!hasSettlementHands && spotlightSeat && spotlightTile ? (
            <div
              className={`table-stage__spotlight table-stage__spotlight--${spotlightSeat} ${promptCue?.isUrgent && promptCue.sourceSeat === spotlightSeat ? 'table-stage__spotlight--urgent' : ''
                }`}
              style={getSettlementCalloutStyle(spotlightSeat)}
              aria-label="Latest discard spotlight"
            >
              <MahjongTile
                code={spotlightTile}
                variant="discard"
                isLastDiscard
                relatedTileCode={selectedTileCode}
                className="table-stage__spotlight-tile"
              />
            </div>
          ) : null}
          {settlementCenterCalloutLabel ? <SettlementCenterCallout label={settlementCenterCalloutLabel} /> : null}
          {exitingActionCallout ? <ActionCalloutMarker callout={exitingActionCallout} phase="exit" /> : null}
          {activeActionCallout ? <ActionCalloutMarker callout={activeActionCallout} phase="active" /> : null}
        </div>
      </div>
      <FanGuideDialog isOpen={isFanGuideOpen} onClose={() => setIsFanGuideOpen(false)} />
    </section>
  );
}


const SETTLEMENT_HAND_COPY: Partial<Record<Seat, string>> = {
  top: '对家手牌',
  left: '左家手牌',
  right: '右家手牌',
};

const ACTION_POINTER_COPY: Record<Seat, string> = {
  top: '对家',
  left: '左家',
  right: '右家',
  bottom: '你',
};

const WIND_COPY: Record<Seat, string> = {
  bottom: '东',
  right: '南',
  top: '西',
  left: '北',
};

const WIND_NAME_TO_CHAR: Record<string, string> = {
  East: '东',
  South: '南',
  West: '西',
  North: '北',
};

const ACTION_CALLOUT_COPY = {
  chow: '吃',
  pung: '碰',
  kong: '杠',
  hu: '和',
  ready_hand: '听',
} as const;

const ACTION_CALLOUT_LINGER_MS = SETTLEMENT_CALLOUT_LINGER_MS;
const QUICK_CHAT_BARRAGE_LINGER_MS = 9000;
const QUICK_CHAT_TEXT_LIMIT = 50;
const QUICK_CHAT_ITEMS: Array<{ emoji: QuickChatEmoji; label: string }> = [
  { emoji: '😄', label: '笑' },
  { emoji: '😭', label: '哭' },
  { emoji: '🀄', label: '红中' },
  { emoji: '☠️', label: '骷髅' },
  { emoji: '😡', label: '生气' },
  { emoji: '🙏', label: '谢谢' },
  { emoji: '👍', label: '赞' },
  { emoji: '🍵', label: '喝茶' },
];
const SPOTLIGHT_POSITION_VARS: Record<Seat, { left: string; top: string; rotation: string }> = {
  top: { left: '50%', top: 'calc(var(--table-stage-center-v) - var(--table-stage-spotlight-offset))', rotation: '180deg' },
  bottom: { left: '50%', top: 'calc(var(--table-stage-center-v) + var(--table-stage-spotlight-offset))', rotation: '0deg' },
  left: { left: 'calc(50% - var(--table-stage-spotlight-offset))', top: 'var(--table-stage-center-v)', rotation: '90deg' },
  right: { left: 'calc(50% + var(--table-stage-spotlight-offset))', top: 'var(--table-stage-center-v)', rotation: '-90deg' },
};

type ActionCallout = {
  key: string;
  seat: Seat;
  tone: keyof typeof ACTION_CALLOUT_COPY;
  label: (typeof ACTION_CALLOUT_COPY)[keyof typeof ACTION_CALLOUT_COPY];
  huVariant: 'discard' | 'self-draw' | null;
};

type BarrageMessage = {
  key: string;
  text: string;
  topPercent: number;
};

type SettlementHandCell =
  | {
    kind: 'placeholder';
    key: string;
  }
  | {
    kind: 'tile';
    key: string;
    tile: string;
    index: number;
  };

function buildSettlementHandCells(seat: Seat, tiles: string[]): SettlementHandCell[] {
  const tileCells = tiles.map(
    (tile, index): SettlementHandCell => ({
      kind: 'tile',
      key: `${seat}-settlement-${tile}-${index}`,
      tile,
      index,
    }),
  );

  if (seat !== 'right') {
    return tileCells;
  }

  const columnCount = SETTLEMENT_HAND_COLUMN_COUNT[seat];
  const remainder = tiles.length % columnCount;

  if (remainder === 0) {
    return tileCells;
  }

  const lastRowStartIndex = tiles.length - remainder;
  const placeholderCount = columnCount - remainder;

  return [
    ...tileCells.slice(0, lastRowStartIndex),
    ...Array.from({ length: placeholderCount }, (_, index): SettlementHandCell => ({
      kind: 'placeholder',
      key: `${seat}-settlement-placeholder-${tiles.length}-${index}`,
    })),
    ...tileCells.slice(lastRowStartIndex),
  ];
}


interface ActionCalloutMarkerProps {
  callout: ActionCallout;
  phase: 'active' | 'exit';
}

interface QuickChatMenuProps {
  seat: Seat;
  playerName: string;
  isLocalTarget?: boolean;
  onSelect: (emoji: QuickChatEmoji) => void;
}


function ActionCalloutMarker({ callout, phase }: ActionCalloutMarkerProps) {
  return (
    <div
      className={`table-stage__action-callout table-stage__spotlight--${callout.seat} table-stage__action-callout--${callout.tone} ${callout.huVariant ? `table-stage__action-callout--hu-${callout.huVariant}` : ''
        } table-stage__action-callout--${phase}`.trim()}
      aria-hidden="true"
      style={getSettlementCalloutStyle(callout.seat)}
    >
      <span className="table-stage__action-callout-glyph">{callout.label}</span>
    </div>
  );
}

interface CenterIndicatorProps {
  remainingCount: number | null;
  actionSeat: Seat | null;
  deadlineAt: string | null;
  isAmbiguous?: boolean;
}

const POINTER_ROTATION_BY_SEAT: Record<Seat, number> = {
  bottom: 0,
  right: -90,
  top: 180,
  left: 90,
};

function resolveShortestPointerRotation(previousRotation: number, actionSeat: Seat): number {
  let nextRotation = POINTER_ROTATION_BY_SEAT[actionSeat];

  while (nextRotation - previousRotation > 180) {
    nextRotation -= 360;
  }

  while (nextRotation - previousRotation < -180) {
    nextRotation += 360;
  }

  return nextRotation;
}

function getCountdownPercent(deadlineAt: string | null) {
  if (!deadlineAt) {
    return 1;
  }

  const deadlineTime = new Date(deadlineAt).getTime();
  if (Number.isNaN(deadlineTime)) {
    return 1;
  }

  return Math.max(0, Math.min(1, (deadlineTime - Date.now()) / PENDING_ACTION_DURATION_MS));
}

function CenterIndicator({
  remainingCount,
  actionSeat,
  deadlineAt,
  isAmbiguous = false,
}: CenterIndicatorProps) {
  const radius = 34; // Smaller radius to fit pointer outside
  const circumference = 2 * Math.PI * radius;
  const [countdownPercent, setCountdownPercent] = useState(() => getCountdownPercent(deadlineAt));
  const offset = circumference - countdownPercent * circumference;
  const isCountdownFull = offset <= COUNTDOWN_RING_STROKE_WIDTH;
  const countdownStrokeProps = isCountdownFull
    ? {}
    : {
      strokeDasharray: circumference,
      strokeDashoffset: offset,
    };
  const [pointerRotation, setPointerRotation] = useState(() => (actionSeat ? POINTER_ROTATION_BY_SEAT[actionSeat] : 0));

  useEffect(() => {
    if (!actionSeat) {
      return;
    }

    setPointerRotation((previousRotation) => resolveShortestPointerRotation(previousRotation, actionSeat));
  }, [actionSeat]);

  useEffect(() => {
    let frameId: number | null = null;
    let disposed = false;

    if (!deadlineAt) {
      setCountdownPercent(1);
      return;
    }

    const update = () => {
      if (disposed) {
        return;
      }

      const nextPercent = getCountdownPercent(deadlineAt);
      setCountdownPercent(nextPercent);

      if (nextPercent > 0) {
        frameId = requestAnimationFrame(update);
      } else {
        frameId = null;
      }
    };

    setCountdownPercent(getCountdownPercent(deadlineAt));
    frameId = requestAnimationFrame(update);

    return () => {
      disposed = true;
      if (frameId !== null) {
        cancelAnimationFrame(frameId);
      }
    };
  }, [deadlineAt]);

  return (
    <div className="table-stage__center-indicator" aria-label="游戏进度指示器">
      {isAmbiguous && <div className="table-stage__center-indicator-breathing" />}
      <svg className="table-stage__center-indicator-ring" viewBox="0 0 100 100">
        <circle className="table-stage__center-indicator-base" cx="50" cy="50" r="38" />
        <circle
          className="table-stage__center-indicator-countdown"
          cx="50"
          cy="50"
          r={radius}
          strokeWidth={COUNTDOWN_RING_STROKE_WIDTH}
          {...countdownStrokeProps}
        />
        {actionSeat && (
          <path
            className="table-stage__center-indicator-pointer"
            d="M44 90 L56 90 L50 98 Z"
            transform={`rotate(${pointerRotation} 50 50)`}
          />
        )}
      </svg>
      <div className="table-stage__center-indicator-remaining">
        <strong className="table-stage__center-indicator-count">
          {remainingCount ?? 0}
        </strong>
      </div>
    </div>
  );
}

function SettlementCenterCallout({ label }: { label: string }) {
  return (
    <div
      className="table-stage__action-callout table-stage__action-callout--center table-stage__action-callout--draw table-stage__action-callout--active"
      aria-hidden="true"
      style={getSettlementCalloutStyle()}
    >
      <span className="table-stage__action-callout-glyph">{label}</span>
    </div>
  );
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
        className={`table-stage__quick-chat-item ${isComposerOpen ? 'table-stage__quick-chat-item--active' : ''
          }`.trim()}
        role="menuitem"
        aria-label="发送自定义文字"
        title="发送自定义文字"
        style={getQuickChatItemStyle(seat, 0)}
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
          style={getQuickChatItemStyle(seat, index + 1)}
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

function createActionCallout(
  actionEffect: ActionEffectView | null,
  settlementWinnerSeat: Seat | null,
  settlementWinType: string | null,
  settlementWinTypeLabel: string | null,
): ActionCallout | null {
  if (!actionEffect?.calloutTone) {
    return null;
  }

  const seat = actionEffect.seat ?? (actionEffect.calloutTone === 'hu' ? settlementWinnerSeat : null);
  if (!seat) {
    return null;
  }

  return {
    key: actionEffect.key,
    seat,
    tone: actionEffect.calloutTone,
    label: ACTION_CALLOUT_COPY[actionEffect.calloutTone],
    huVariant:
      actionEffect.calloutTone === 'hu'
        ? getHuCalloutVariant(settlementWinType, settlementWinTypeLabel)
        : null,
  };
}

function getHuCalloutVariant(settlementWinType: string | null, settlementWinTypeLabel: string | null) {
  if (settlementWinType === 'self_draw') {
    return 'self-draw';
  }

  if (settlementWinType === 'discard') {
    return 'discard';
  }

  return null;
}

function buildTableSummary(roundLabel: string, phaseLabel: string) {
  if (roundLabel && phaseLabel) {
    return `${roundLabel} | ${phaseLabel}`;
  }

  return roundLabel || phaseLabel || null;
}

function getQuickChatItemStyle(_seat: Seat, index: number): CSSProperties {
  const itemHeightRem = 2.8;
  const gapRem = 0.4;
  const offsetRem = (index + 1) * (itemHeightRem + gapRem);

  return {
    '--quick-chat-x': '0rem',
    '--quick-chat-y': `-${offsetRem}rem`,
  } as CSSProperties;
}

function getRiverTrackStyle(seat: Seat): CSSProperties | undefined {
  if (seat !== 'right') {
    return undefined;
  }

  return {
    direction: 'ltr',
  };
}

function getRandomBarrageTopPercent() {
  return 18 + Math.round(Math.random() * 60);
}

function clampQuickChatText(value: string) {
  return Array.from(value).slice(0, QUICK_CHAT_TEXT_LIMIT).join('');
}

function normalizeQuickChatText(value: string) {
  return clampQuickChatText(value).trim();
}

function getSettlementCalloutStyle(seat: Seat | null = null): CSSProperties {
  const position = seat ? SPOTLIGHT_POSITION_VARS[seat] : { left: '50%', top: 'var(--table-stage-center-v)', rotation: '0deg' };

  return {
    '--table-stage-action-callout-duration': SETTLEMENT_CALLOUT_DURATION_CSS,
    '--spotlight-left': position.left,
    '--spotlight-top': position.top,
    '--spotlight-rotation': position.rotation,
  } as CSSProperties;
}

function findLastDiscardPosition(
  discards: Record<Seat, string[]>,
  lastDiscard: string | null,
  preferredSeat: Seat | null = null,
): { seat: Seat; index: number } | null {
  if (!lastDiscard) {
    return null;
  }

  if (preferredSeat) {
    for (let index = discards[preferredSeat].length - 1; index >= 0; index -= 1) {
      if (discards[preferredSeat][index] === lastDiscard) {
        return { seat: preferredSeat, index };
      }
    }
  }

  let match: { seat: Seat; index: number } | null = null;

  for (const seat of SEATS) {
    discards[seat].forEach((tile, index) => {
      if (tile === lastDiscard) {
        match = { seat, index };
      }
    });
  }

  return match;
}

function shouldPinDenseMeldRack(seat: Seat, meldCount: number) {
  return (seat === 'top' || seat === 'bottom') && meldCount >= 3;
}

function getMeldRackPositionStyle(seat: Seat): CSSProperties {
  const offset = 'calc(100% + clamp(0.4rem, 1vw, 0.8rem))';

  if (seat === 'left' || seat === 'right') {
    return {
      left: '50%',
      right: 'auto',
      top: 'auto',
      bottom: offset,
      transform: 'translateX(-50%)',
    };
  }

  return {
    left: offset,
    right: 'auto',
    top: '50%',
    bottom: 'auto',
    transform: 'translateY(-50%)',
  };
}
function FanIcon({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 32 32" className={className} fill="currentColor">
      <path d="M16 26C16 26 6 22 4 14C4 8 8 6 16 6C24 6 28 8 28 14C26 22 16 26 16 26Z" opacity="0.25" />
      <path d="M16 24L6 14C6 11 10 8 16 8C22 8 26 11 26 14L16 24Z" />
      <path d="M16 24L16 8" fill="none" stroke="currentColor" strokeWidth="1" opacity="0.5" />
      <path d="M16 24L11 10" fill="none" stroke="currentColor" strokeWidth="1" opacity="0.3" />
      <path d="M16 24L21 10" fill="none" stroke="currentColor" strokeWidth="1" opacity="0.3" />
    </svg>
  );
}

function IngotIcon({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 32 32" className={className} fill="currentColor">
      <path d="M16 6C11 6 7 9 7 13V15C7 19 11 22 16 22C21 22 25 19 25 15V13C25 9 21 6 16 6Z" opacity="0.25" />
      <path d="M16 8C12.5 8 9.5 10.5 9.5 13.5V14.5C9.5 17.5 12.5 20 16 20C19.5 20 22.5 17.5 22.5 14.5V13.5C22.5 10.5 19.5 8 16 8Z" />
      <path d="M9.5 13.5C9.5 10.5 12.5 8 16 8C19.5 8 22.5 10.5 22.5 13.5" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" />
    </svg>
  );
}

function LotusIcon({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 32 32" className={className} fill="currentColor">
      <path d="M16 26C16 26 8 21 8 14C8 9 12 6 16 6C20 6 24 9 24 14C24 21 16 26 16 26Z" opacity="0.25" />
      <path d="M16 24C12 20 10 16 10 12C10 10 12 11 16 15C20 11 22 10 22 12C22 16 20 20 16 24Z" />
      <path d="M10 12C10 10 12 11 16 15L16 6" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M22 12C22 10 20 11 16 15" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function TileStackIcon({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 32 32" className={className} fill="currentColor">
      <rect x="6" y="11" width="13" height="17" rx="2" opacity="0.25" />
      <rect x="13" y="4" width="13" height="17" rx="2" />
      <rect x="13" y="4" width="13" height="17" rx="2" fill="none" stroke="currentColor" strokeWidth="1.5" />
    </svg>
  );
}
