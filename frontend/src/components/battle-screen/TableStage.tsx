import { Fragment, useEffect, useRef, useState, type CSSProperties } from 'react';

import type { ThemeId } from '../../lib/themes';
import type {
  ActionEffectView,
  BattleActionView,
  BattlePromptView,
  QuickChatEmoji,
  QuickChatEventView,
  Seat,
  TableMode,
  WaitingSeatSlot,
} from '../../types/match';
import { FanGuideDialog } from './FanGuideDialog';
import { MahjongTile } from './MahjongTile';
import { MeldRack } from './MeldRack';
import { PlayerInfoBar, type TableStagePlayer } from './PlayerInfoBar';
import { SETTLEMENT_CALLOUT_DURATION_CSS, SETTLEMENT_CALLOUT_LINGER_MS } from './settlementTiming';

interface TableStageProps {
  discards: Record<Seat, string[]>;
  roomMode?: TableMode;
  activeSeat: Seat;
  actionIndicatorSeat?: Seat | null;
  lastDiscard: string | null;
  lastDiscardSeat?: Seat | null;
  settlementWinnerSeat?: Seat | null;
  settlementWinType?: string | null;
  settlementWinTypeLabel?: string | null;
  remainingTileCount?: number | null;
  promptText: string | null;
  promptCue?: BattlePromptView | null;
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
  waitingSeatSlots?: WaitingSeatSlot[];
  tileScale?: number;
  canDecreaseTileScale?: boolean;
  canIncreaseTileScale?: boolean;
  canLeaveTable?: boolean;
  themeId?: ThemeId;
  themeLabel?: string;
  onLeaveTable?: () => void;
  onCycleTheme?: () => void;
  onAction?: (actionId: BattleActionView['id']) => void;
  onQuickChat?: (targetSeat: number, emoji: QuickChatEmoji) => void;
  onReserveAiSeat?: (seatIndex: number) => void;
  onConfigureAiSeat?: (seatIndex: number, apiKey: string, baseUrl: string, model: string) => void;
  onCancelAiSeat?: (seatIndex: number) => void;
  onUseDefaultBot?: (seatIndex: number) => void;
  onDecreaseTileScale?: () => void;
  onIncreaseTileScale?: () => void;
}

const SEATS: Seat[] = ['top', 'left', 'right', 'bottom'];

export function TableStage({
  discards,
  roomMode = 'normal',
  activeSeat,
  actionIndicatorSeat = null,
  lastDiscard,
  lastDiscardSeat = null,
  settlementWinnerSeat = null,
  settlementWinType = null,
  settlementWinTypeLabel = null,
  remainingTileCount = null,
  promptText,
  promptCue = null,
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
  waitingSeatSlots = [],
  tileScale = 1,
  canDecreaseTileScale = false,
  canIncreaseTileScale = false,
  canLeaveTable = false,
  themeId = 'tian-shui-bi',
  themeLabel = '天水碧',
  onLeaveTable,
  onCycleTheme,
  onAction,
  onQuickChat,
  onReserveAiSeat,
  onConfigureAiSeat,
  onCancelAiSeat,
  onUseDefaultBot,
  onDecreaseTileScale,
  onIncreaseTileScale,
}: TableStageProps) {
  const lastDiscardPosition = findLastDiscardPosition(discards, lastDiscard, lastDiscardSeat);
  const playerBySeat = new Map(players.map((player) => [player.seat, player]));
  const waitingSeatSlotBySeat = new Map(waitingSeatSlots.map((slot) => [slot.seat, slot]));
  const hasSettlementHands = Object.values(settlementHands ?? {}).some((tiles) => tiles.length > 0);
  const [activeActionCallout, setActiveActionCallout] = useState<ActionCallout | null>(null);
  const [exitingActionCallout, setExitingActionCallout] = useState<ActionCallout | null>(null);
  const [openQuickChatSeat, setOpenQuickChatSeat] = useState<Seat | null>(null);
  const [isFanGuideOpen, setIsFanGuideOpen] = useState(false);
  const [aiSeatModalState, setAiSeatModalState] = useState<AiSeatModalState | null>(null);
  const [barrageMessages, setBarrageMessages] = useState<BarrageMessage[]>([]);
  const activeActionCalloutRef = useRef<ActionCallout | null>(null);
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
  const scalePercentLabel = `${Math.round(tileScale * 100)}%`;
  const activeAiModalSlot = aiSeatModalState ? waitingSeatSlotBySeat.get(aiSeatModalState.seat) ?? null : null;
  const tableStageStyle = {
    '--table-stage-tile-scale': `${tileScale}`,
    '--table-stage-spotlight-scale': `${spotlightScale}`,
  } as CSSProperties;

  function openAiSeatModal(slot: WaitingSeatSlot) {
    setOpenQuickChatSeat(null);
    setAiSeatModalState((currentState) => ({
      seat: slot.seat,
      seatIndex: slot.absoluteSeat,
      apiKey: currentState?.seatIndex === slot.absoluteSeat ? currentState.apiKey : '',
      baseUrl: currentState?.seatIndex === slot.absoluteSeat ? currentState.baseUrl : '',
      model: currentState?.seatIndex === slot.absoluteSeat ? currentState.model : '',
    }));
  }

  useEffect(() => {
    activeActionCalloutRef.current = activeActionCallout;
  }, [activeActionCallout]);

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
    if (openQuickChatSeat && !playerBySeat.has(openQuickChatSeat)) {
      setOpenQuickChatSeat(null);
    }
  }, [openQuickChatSeat, playerBySeat]);

  useEffect(() => {
    if (!aiSeatModalState) {
      return;
    }

    const slot = waitingSeatSlotBySeat.get(aiSeatModalState.seat);
    if (!slot || roomMode !== 'ai') {
      setAiSeatModalState(null);
      return;
    }

    if (!slot.occupied && slot.canConfigureAi) {
      return;
    }

    if (slot.seatType === 'ai') {
      if (slot.aiStatus === 'ready') {
        setAiSeatModalState(null);
      }
      return;
    }

    if (slot.seatType === 'bot' || (slot.occupied && !slot.occupiedByLocalAi)) {
      setAiSeatModalState(null);
    }
  }, [aiSeatModalState, roomMode, waitingSeatSlotBySeat]);

  useEffect(() => {
    if (!openQuickChatSeat) {
      return undefined;
    }

    function handlePointerDown(event: PointerEvent) {
      const target = event.target;
      if (target instanceof Element && target.closest('[data-quick-chat-root="true"]')) {
        return;
      }
      setOpenQuickChatSeat(null);
    }

    document.addEventListener('pointerdown', handlePointerDown);
    return () => document.removeEventListener('pointerdown', handlePointerDown);
  }, [openQuickChatSeat]);

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

    if (currentActionCallout) {
      return;
    }

    if (activeActionCalloutTimerRef.current !== null) {
      window.clearTimeout(activeActionCalloutTimerRef.current);
      activeActionCalloutTimerRef.current = null;
    }

    consumedActionCalloutKeyRef.current = actionCalloutKey;
    setActiveActionCallout(nextActionCallout);

    if (nextActionCallout) {
      activeActionCalloutTimerRef.current = window.setTimeout(() => {
        setActiveActionCallout((current) => (current?.key === nextActionCallout.key ? null : current));
        activeActionCalloutTimerRef.current = null;
      }, ACTION_CALLOUT_LINGER_MS);
    }
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
      className={`table-stage ${promptCue?.isUrgent ? 'table-stage--urgent' : ''}`}
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
              title="国标麻将番种说明"
              onClick={() => {
                setOpenQuickChatSeat(null);
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
          <div
            className={`table-stage__center-meta ${promptCue ? 'table-stage__center-meta--with-cue' : ''} ${
              promptCue?.isUrgent ? 'table-stage__center-meta--urgent' : ''
            }`}
          >
            {promptCue ? (
              <span className={`table-stage__cue table-stage__cue--${promptCue.tone}`}>
                {PROMPT_KIND_COPY[promptCue.kind]}
              </span>
            ) : null}
            <strong>{typeof remainingTileCount === 'number' ? `剩余 ${remainingTileCount} 张` : '等待开局'}</strong>
            {promptText ? <em>{promptText}</em> : null}
          </div>
          {actionIndicatorSeat ? (
            <div
              className={`table-stage__action-pointer table-stage__action-pointer--${actionIndicatorSeat}`}
              aria-label={`${ACTION_POINTER_COPY[actionIndicatorSeat]}正在行动`}
            />
          ) : null}
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
          {tableSummary ? <div className="table-stage__status-summary">{tableSummary}</div> : null}
          {shouldShowScaleControls ? (
            <div className="table-stage__scale-controls" role="group" aria-label="调整牌桌牌面大小">
              <button
                type="button"
                className="table-stage__scale-button"
                aria-label="缩小牌桌牌面"
                onClick={onDecreaseTileScale}
                disabled={!canDecreaseTileScale}
              >
                -
              </button>
              <span className="table-stage__scale-readout" aria-label={`当前牌面大小 ${scalePercentLabel}`}>
                {scalePercentLabel}
              </span>
              <button
                type="button"
                className="table-stage__scale-button"
                aria-label="放大牌桌牌面"
                onClick={onIncreaseTileScale}
                disabled={!canIncreaseTileScale}
              >
                +
              </button>
            </div>
          ) : null}
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
            const waitingSeatSlot = waitingSeatSlotBySeat.get(seat);
            const hasMelds = (player?.melds.length ?? 0) > 0;
            const finalHandTiles = settlementHands?.[seat] ?? [];
            const settlementHandLabel = SETTLEMENT_HAND_COPY[seat];
            const shouldRenderSeatInfo = Boolean(player);
            const canEditOwnedAi = Boolean(waitingSeatSlot?.occupiedByLocalAi);
            const settlementWinningTileIndex =
              settlementWinType === 'discard' &&
              settlementWinnerSeat === seat &&
              lastDiscard !== null &&
              finalHandTiles.at(-1) === lastDiscard
                ? finalHandTiles.length - 1
                : -1;

            return (
              <Fragment key={seat}>
                <div className={`table-stage__seat-zone table-stage__seat-zone--${seat}`}>
                  <div className={`table-stage__seat-panel table-stage__seat-panel--${seat}`}>
                    <div
                      className={`table-stage__river table-stage__river--${seat} ${
                        activeSeat === seat ? 'table-stage__river--active' : ''
                      }`}
                      data-seat={seat}
                    >
                      <div className={`table-stage__river-track table-stage__river-track--${seat}`}>
                        {discards[seat].map((tile, index) => {
                          const isSpotlightTile =
                            lastDiscardPosition !== null &&
                            lastDiscardPosition.seat === seat &&
                            lastDiscardPosition.index === index;

                          if (isSpotlightTile) {
                            return null;
                          }

                          return <MahjongTile key={`${seat}-${tile}-${index}`} code={tile} variant="discard" />;
                        })}
                      </div>
                    </div>
                  </div>
                  {player && hasMelds ? (
                    <div
                      className={`table-stage__melds table-stage__melds--${seat} ${
                        shouldPinDenseMeldRack(seat, player.melds.length) ? 'table-stage__melds--dense' : ''
                      }`.trim()}
                    >
                      <MeldRack seat={seat} melds={player.melds} ariaLabel={`${player.name} melds`} />
                    </div>
                  ) : null}
                  {finalHandTiles.length > 0 && settlementHandLabel ? (
                    <div
                      className={`table-stage__settlement-hand table-stage__settlement-hand--${seat}`}
                      aria-label={settlementHandLabel}
                    >
                      <span className="table-stage__settlement-hand-eyebrow">{settlementHandLabel}</span>
                      <div className={`table-stage__settlement-hand-grid table-stage__settlement-hand-grid--${seat}`}>
                        {finalHandTiles.map((tile, index) => (
                          <MahjongTile
                            key={`${seat}-settlement-${tile}-${index}`}
                            code={tile}
                            variant="discard"
                            isLastDiscard={index === settlementWinningTileIndex}
                            className="table-stage__settlement-hand-tile"
                          />
                        ))}
                      </div>
                    </div>
                  ) : null}
                  {shouldRenderSeatInfo && player ? (
                    <div
                      className={`table-stage__player-info-cluster table-stage__player-info-cluster--${seat}`}
                      data-quick-chat-root="true"
                    >
                      <button
                        type="button"
                        className={`table-stage__player-info-button ${
                          openQuickChatSeat === seat ? 'table-stage__player-info-button--open' : ''
                        }`.trim()}
                        aria-label={canEditOwnedAi ? `打开${player.name}的AI配置` : `打开${player.name}的快捷表情`}
                        aria-expanded={canEditOwnedAi ? undefined : openQuickChatSeat === seat}
                        aria-controls={canEditOwnedAi ? undefined : `table-stage-quick-chat-${seat}`}
                        onClick={(event) => {
                          event.stopPropagation();
                          if (canEditOwnedAi && waitingSeatSlot) {
                            openAiSeatModal(waitingSeatSlot);
                            return;
                          }
                          setOpenQuickChatSeat((currentSeat) => (currentSeat === seat ? null : seat));
                        }}
                      >
                        <PlayerInfoBar player={player} />
                      </button>
                      {openQuickChatSeat === seat && !canEditOwnedAi ? (
                        <QuickChatMenu
                          seat={seat}
                          player={player}
                          onSelect={(emoji) => {
                            if (typeof player.absoluteSeat === 'number') {
                              onQuickChat?.(player.absoluteSeat, emoji);
                            }
                            setOpenQuickChatSeat(null);
                          }}
                        />
                      ) : null}
                    </div>
                  ) : null}
                  {!player && waitingSeatSlot?.canConfigureAi ? (
                    <div className={`table-stage__ai-seat table-stage__ai-seat--${seat}`}>
                      <button
                        type="button"
                        className="table-stage__ai-seat-button"
                        aria-label={`为${SEAT_COPY[seat]}添加AI牌手`}
                        onClick={() => {
                          onReserveAiSeat?.(waitingSeatSlot.absoluteSeat);
                          openAiSeatModal(waitingSeatSlot);
                        }}
                      >
                        <span className="table-stage__ai-seat-plus" aria-hidden="true">
                          +
                        </span>
                        <span className="table-stage__ai-seat-label">添加 AI</span>
                      </button>
                    </div>
                  ) : null}
                </div>
              </Fragment>
            );
          })}
          {!hasSettlementHands && spotlightSeat && spotlightTile ? (
            <div
              className={`table-stage__spotlight table-stage__spotlight--${spotlightSeat} ${
                promptCue?.isUrgent && promptCue.sourceSeat === spotlightSeat ? 'table-stage__spotlight--urgent' : ''
              }`}
              aria-label="Latest discard spotlight"
            >
              <MahjongTile
                code={spotlightTile}
                variant="discard"
                isLastDiscard
                className="table-stage__spotlight-tile"
              />
            </div>
          ) : null}
          {settlementCenterCalloutLabel ? <SettlementCenterCallout label={settlementCenterCalloutLabel} /> : null}
          {exitingActionCallout ? <ActionCalloutMarker callout={exitingActionCallout} phase="exit" /> : null}
          {activeActionCallout ? <ActionCalloutMarker callout={activeActionCallout} phase="active" /> : null}
        </div>
      </div>
      <AiSeatConfigDialog
        state={aiSeatModalState}
        slot={activeAiModalSlot}
        onClose={() => setAiSeatModalState(null)}
        onChange={(patch) => {
          setAiSeatModalState((currentState) => (currentState ? { ...currentState, ...patch } : currentState));
        }}
        onSubmit={() => {
          if (!aiSeatModalState) {
            return;
          }
          onConfigureAiSeat?.(
            aiSeatModalState.seatIndex,
            aiSeatModalState.apiKey.trim(),
            aiSeatModalState.baseUrl.trim(),
            aiSeatModalState.model.trim(),
          );
        }}
        onCancel={() => {
          if (!aiSeatModalState) {
            return;
          }
          onCancelAiSeat?.(aiSeatModalState.seatIndex);
          setAiSeatModalState(null);
        }}
        onUseDefaultBot={() => {
          if (!aiSeatModalState) {
            return;
          }
          onUseDefaultBot?.(aiSeatModalState.seatIndex);
          setAiSeatModalState(null);
        }}
      />
      <FanGuideDialog isOpen={isFanGuideOpen} onClose={() => setIsFanGuideOpen(false)} />
    </section>
  );
}

const PROMPT_KIND_COPY: Record<NonNullable<TableStageProps['promptCue']>['kind'], string> = {
  turn: '当前可操作',
  claim: '可响应',
  rob_kong: '抢杠',
  turn_kong: '杠响应',
};

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

const SEAT_COPY: Record<Seat, string> = {
  top: '对家',
  left: '左家',
  right: '右家',
  bottom: '你',
};

const ACTION_CALLOUT_COPY = {
  chow: '吃',
  pung: '碰',
  kong: '杠',
  hu: '和',
} as const;

const ACTION_CALLOUT_LINGER_MS = SETTLEMENT_CALLOUT_LINGER_MS;
const QUICK_CHAT_BARRAGE_LINGER_MS = 9000;
const QUICK_CHAT_ARC_SWEEP_DEGREES = 150;
const QUICK_CHAT_ITEM_RADIUS_REM = 5.1;
const QUICK_CHAT_ITEMS: Array<{ emoji: QuickChatEmoji; label: string }> = [
  { emoji: '😄', label: '笑' },
  { emoji: '😭', label: '哭' },
  { emoji: '🀄', label: '红中' },
  { emoji: '☠️', label: '骷髅' },
  { emoji: '😡', label: '生气' },
  { emoji: '🤮', label: '呕吐' },
];
const QUICK_CHAT_ARC_CENTER_DEGREES: Record<Seat, number> = {
  top: 135,
  right: 220,
  bottom: 220,
  left: 320,
};

type ActionCallout = {
  key: string;
  seat: Seat;
  tone: keyof typeof ACTION_CALLOUT_COPY;
  label: (typeof ACTION_CALLOUT_COPY)[keyof typeof ACTION_CALLOUT_COPY];
  huVariant: 'discard' | 'self-draw' | 'low-fan' | null;
};

type BarrageMessage = {
  key: string;
  text: string;
  topPercent: number;
};

type AiSeatModalState = {
  seat: Seat;
  seatIndex: number;
  apiKey: string;
  baseUrl: string;
  model: string;
};

interface ActionCalloutMarkerProps {
  callout: ActionCallout;
  phase: 'active' | 'exit';
}

interface QuickChatMenuProps {
  seat: Seat;
  player: TableStagePlayer;
  onSelect: (emoji: QuickChatEmoji) => void;
}

interface AiSeatConfigDialogProps {
  state: AiSeatModalState | null;
  slot: WaitingSeatSlot | null;
  onClose: () => void;
  onChange: (patch: Partial<AiSeatModalState>) => void;
  onSubmit: () => void;
  onCancel: () => void;
  onUseDefaultBot: () => void;
}

function ActionCalloutMarker({ callout, phase }: ActionCalloutMarkerProps) {
  return (
    <div
      className={`table-stage__action-callout table-stage__spotlight--${callout.seat} table-stage__action-callout--${callout.tone} ${
        callout.huVariant ? `table-stage__action-callout--hu-${callout.huVariant}` : ''
      } table-stage__action-callout--${phase}`.trim()}
      aria-hidden="true"
      style={getSettlementCalloutStyle()}
    >
      <span className="table-stage__action-callout-glyph">{callout.label}</span>
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

function QuickChatMenu({ seat, player, onSelect }: QuickChatMenuProps) {
  const menuId = `table-stage-quick-chat-${seat}`;
  const isLocalTarget = Boolean(player.isLocal);

  return (
    <div
      id={menuId}
      className={`table-stage__quick-chat-menu table-stage__quick-chat-menu--${seat}`}
      role="menu"
      aria-label={`${player.name} 快捷表情`}
      onPointerDown={(event) => event.stopPropagation()}
      onClick={(event) => event.stopPropagation()}
    >
      {QUICK_CHAT_ITEMS.map((item, index) => (
        <button
          key={`${seat}-${item.label}`}
          type="button"
          className="table-stage__quick-chat-item"
          role="menuitem"
          aria-label={`${isLocalTarget ? '发送' : `向${player.name}发送`}${item.label}表情`}
          title={`${isLocalTarget ? '发送' : `向${player.name}发送`}${item.label}表情`}
          style={getQuickChatItemStyle(seat, index)}
          onClick={(event) => {
            event.stopPropagation();
            onSelect(item.emoji);
          }}
        >
          <span aria-hidden="true">{item.emoji}</span>
        </button>
      ))}
    </div>
  );
}

function AiSeatConfigDialog({
  state,
  slot,
  onClose,
  onChange,
  onSubmit,
  onCancel,
  onUseDefaultBot,
}: AiSeatConfigDialogProps) {
  if (!state) {
    return null;
  }

  const isValidating = slot?.seatType === 'ai' && slot.aiStatus === 'validating';
  const canSubmit =
    !isValidating &&
    state.apiKey.trim().length > 0 &&
    state.baseUrl.trim().length > 0 &&
    state.model.trim().length > 0;
  const statusText =
    slot?.seatType === 'ai'
      ? slot.aiStatus === 'validating'
        ? '正在验证 AI 接口连通性...'
        : slot.aiStatus === 'error'
          ? '验证失败，请检查配置后重试。'
          : slot.aiStatus === 'configuring'
            ? '座位已预留，填写配置后即可验证。'
            : '配置已完成。'
      : '座位已预留，填写配置后即可验证。';

  return (
    <div className="table-stage__modal-backdrop" role="presentation" onClick={onClose}>
      <div
        className="table-stage__modal"
        role="dialog"
        aria-modal="true"
        aria-label={`配置${SEAT_COPY[state.seat]}AI牌手`}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="table-stage__modal-header">
          <div>
            <span className="table-stage__modal-eyebrow">{SEAT_COPY[state.seat]}座位</span>
            <strong>配置 AI 牌手</strong>
          </div>
          <button type="button" className="table-stage__modal-close" aria-label="关闭AI配置弹窗" onClick={onClose}>
            ×
          </button>
        </div>
        <div className="table-stage__modal-status">
          <span>{statusText}</span>
          {slot?.aiError ? <em>{slot.aiError}</em> : null}
        </div>
        <label className="table-stage__modal-field">
          <span>OpenAI API Key</span>
          <input
            value={state.apiKey}
            onChange={(event) => onChange({ apiKey: event.target.value })}
            placeholder="sk-..."
            disabled={isValidating}
          />
        </label>
        <label className="table-stage__modal-field">
          <span>Base URL</span>
          <input
            value={state.baseUrl}
            onChange={(event) => onChange({ baseUrl: event.target.value })}
            placeholder="https://api.openai.com/v1"
            disabled={isValidating}
          />
        </label>
        <label className="table-stage__modal-field">
          <span>模型名称</span>
          <input
            value={state.model}
            onChange={(event) => onChange({ model: event.target.value })}
            placeholder="gpt-4.1-mini"
            disabled={isValidating}
          />
        </label>
        <div className="table-stage__modal-actions">
          <button type="button" className="table-stage__modal-button table-stage__modal-button--primary" onClick={onSubmit} disabled={!canSubmit}>
            确定
          </button>
          <button type="button" className="table-stage__modal-button" onClick={onCancel}>
            取消
          </button>
          <button type="button" className="table-stage__modal-button table-stage__modal-button--ghost" onClick={onUseDefaultBot}>
            使用默认BOT
          </button>
        </div>
      </div>
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

  if (settlementWinTypeLabel === '屁和') {
    return 'low-fan';
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

function getQuickChatItemStyle(seat: Seat, index: number): CSSProperties {
  const angles = getQuickChatAngles(seat);
  const angle = angles[index] ?? getQuickChatArcCenterDegrees(seat);
  const radians = (angle * Math.PI) / 180;

  return {
    '--quick-chat-x': `${Math.cos(radians) * QUICK_CHAT_ITEM_RADIUS_REM}rem`,
    '--quick-chat-y': `${Math.sin(radians) * QUICK_CHAT_ITEM_RADIUS_REM}rem`,
  } as CSSProperties;
}

function getQuickChatAngles(seat: Seat): number[] {
  if (seat === 'left') {
    return getQuickChatAngles('right').map((angle) => mirrorAngleHorizontally(angle));
  }

  const center = getQuickChatArcCenterDegrees(seat);
  const step = QUICK_CHAT_ITEMS.length > 1 ? QUICK_CHAT_ARC_SWEEP_DEGREES / (QUICK_CHAT_ITEMS.length - 1) : 0;
  const start = center - QUICK_CHAT_ARC_SWEEP_DEGREES / 2;

  return QUICK_CHAT_ITEMS.map((_, index) => start + step * index);
}

function getQuickChatArcCenterDegrees(seat: Seat) {
  if (seat === 'left') {
    return mirrorAngleHorizontally(QUICK_CHAT_ARC_CENTER_DEGREES.right);
  }

  return QUICK_CHAT_ARC_CENTER_DEGREES[seat];
}

function mirrorAngleHorizontally(angle: number) {
  return (180 - angle + 360) % 360;
}

function getRandomBarrageTopPercent() {
  return 18 + Math.round(Math.random() * 60);
}

function getSettlementCalloutStyle(): CSSProperties {
  return {
    '--table-stage-action-callout-duration': SETTLEMENT_CALLOUT_DURATION_CSS,
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
