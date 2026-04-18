import { Fragment, useEffect, useRef, useState, type CSSProperties } from 'react';

import type { ThemeId } from '../../lib/themes';
import { PLAYER_SKILL_TOOLTIP_DELAY_MS } from '../../lib/skillSystem';
import type {
  ActionEffectView,
  BattleActionView,
  BattlePromptView,
  QuickChatEmoji,
  QuickChatEventView,
  Seat,
} from '../../types/match';
import { FanGuideDialog } from './FanGuideDialog';
import { MahjongTile } from './MahjongTile';
import { MeldRack } from './MeldRack';
import { PlayerInfoBar, type TableStagePlayer } from './PlayerInfoBar';
import { SETTLEMENT_CALLOUT_DURATION_CSS, SETTLEMENT_CALLOUT_LINGER_MS } from './settlementTiming';

interface TableStageProps {
  discards: Record<Seat, string[]>;
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
  const [exitingActionCallout, setExitingActionCallout] = useState<ActionCallout | null>(null);
  const [openQuickChatSeat, setOpenQuickChatSeat] = useState<Seat | null>(null);
  const [openSkillTooltipSeat, setOpenSkillTooltipSeat] = useState<Seat | null>(null);
  const [isFanGuideOpen, setIsFanGuideOpen] = useState(false);
  const [barrageMessages, setBarrageMessages] = useState<BarrageMessage[]>([]);
  const activeActionCalloutRef = useRef<ActionCallout | null>(null);
  const activeActionCalloutTimerRef = useRef<number | null>(null);
  const exitingActionCalloutTimerRef = useRef<number | null>(null);
  const skillTooltipTimerRef = useRef<number | null>(null);
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
  } as CSSProperties;

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
      if (skillTooltipTimerRef.current !== null) {
        window.clearTimeout(skillTooltipTimerRef.current);
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
    if (openSkillTooltipSeat && !playerBySeat.has(openSkillTooltipSeat)) {
      setOpenSkillTooltipSeat(null);
    }
  }, [openSkillTooltipSeat, playerBySeat]);

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
            <strong>{centerPrimaryText}</strong>
            {promptText ? <em>{promptText}</em> : null}
          </div>
          {actionIndicatorSeat ? (
            <div
              className={`table-stage__action-pointer table-stage__action-pointer--${actionIndicatorSeat}`}
              aria-label={`${ACTION_POINTER_COPY[actionIndicatorSeat]}正在行动`}
            />
          ) : null}
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
            const hasMelds = (player?.melds.length ?? 0) > 0;
            const finalHandTiles = settlementHands?.[seat] ?? [];
            const settlementHandLabel = SETTLEMENT_HAND_COPY[seat];
            const shouldRenderSeatInfo = Boolean(player);
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
                              className="table-stage__settlement-hand-tile"
                            />
                          ),
                        )}
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
                        aria-label={`打开${player.name}的快捷表情`}
                        aria-expanded={openQuickChatSeat === seat}
                        aria-controls={`table-stage-quick-chat-${seat}`}
                        onClick={(event) => {
                          event.stopPropagation();
                          setOpenQuickChatSeat((currentSeat) => (currentSeat === seat ? null : seat));
                        }}
                        onMouseEnter={() => {
                          if (!player.skill) {
                            return;
                          }

                          if (skillTooltipTimerRef.current !== null) {
                            window.clearTimeout(skillTooltipTimerRef.current);
                          }

                          skillTooltipTimerRef.current = window.setTimeout(() => {
                            setOpenSkillTooltipSeat(seat);
                            skillTooltipTimerRef.current = null;
                          }, PLAYER_SKILL_TOOLTIP_DELAY_MS);
                        }}
                        onMouseLeave={() => {
                          if (skillTooltipTimerRef.current !== null) {
                            window.clearTimeout(skillTooltipTimerRef.current);
                            skillTooltipTimerRef.current = null;
                          }

                          setOpenSkillTooltipSeat((currentSeat) => (currentSeat === seat ? null : currentSeat));
                        }}
                      >
                        <PlayerInfoBar
                          player={player}
                          showSkillTooltip={openSkillTooltipSeat === seat}
                          tooltipPlacement={seat}
                        />
                      </button>
                      {openQuickChatSeat === seat ? (
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

const ACTION_CALLOUT_COPY = {
  chow: '吃',
  pung: '碰',
  kong: '杠',
  hu: '和',
  skill: '技',
} as const;

const ACTION_CALLOUT_LINGER_MS = SETTLEMENT_CALLOUT_LINGER_MS;
const QUICK_CHAT_BARRAGE_LINGER_MS = 9000;
const QUICK_CHAT_ARC_SWEEP_DEGREES = 150;
const QUICK_CHAT_ITEM_RADIUS_REM = 5.1;
const QUICK_CHAT_TEXT_LIMIT = 50;
const QUICK_CHAT_ITEMS: Array<{ emoji: QuickChatEmoji; label: string }> = [
  { emoji: '😄', label: '笑' },
  { emoji: '😭', label: '哭' },
  { emoji: '🀄', label: '红中' },
  { emoji: '☠️', label: '骷髅' },
  { emoji: '😡', label: '生气' },
];
const QUICK_CHAT_ARC_CENTER_DEGREES: Record<Seat, number> = {
  top: 135,
  right: 220,
  bottom: 220,
  left: 320,
};
const SPOTLIGHT_POSITION_VARS: Record<Seat, { left: string; top: string }> = {
  top: { left: '50%', top: '32%' },
  bottom: { left: '50%', top: '68%' },
  left: { left: '32%', top: '50%' },
  right: { left: '68%', top: '50%' },
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
  player: TableStagePlayer;
  onSelect: (emoji: QuickChatEmoji) => void;
}


function ActionCalloutMarker({ callout, phase }: ActionCalloutMarkerProps) {
  return (
    <div
      className={`table-stage__action-callout table-stage__spotlight--${callout.seat} table-stage__action-callout--${callout.tone} ${
        callout.huVariant ? `table-stage__action-callout--hu-${callout.huVariant}` : ''
      } table-stage__action-callout--${phase}`.trim()}
      aria-hidden="true"
      style={getSettlementCalloutStyle(callout.seat)}
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
      <button
        type="button"
        className={`table-stage__quick-chat-item ${
          isComposerOpen ? 'table-stage__quick-chat-item--active' : ''
        }`.trim()}
        role="menuitem"
        aria-label={`${isLocalTarget ? '发送' : `向${player.name}发送`}自定义文字`}
        title={`${isLocalTarget ? '发送' : `向${player.name}发送`}自定义文字`}
        style={getQuickChatItemStyle(seat, QUICK_CHAT_ITEMS.length)}
        onClick={(event) => {
          event.stopPropagation();
          setIsComposerOpen((current) => !current);
          setDraft((current) => (isComposerOpen ? '' : current));
        }}
      >
        <span aria-hidden="true">+</span>
      </button>
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
            aria-label={`${isLocalTarget ? '输入' : `向${player.name}发送`}快捷文字`}
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
  const itemCount = QUICK_CHAT_ITEMS.length + 1;
  const step = itemCount > 1 ? QUICK_CHAT_ARC_SWEEP_DEGREES / (itemCount - 1) : 0;
  const start = center - QUICK_CHAT_ARC_SWEEP_DEGREES / 2;

  return Array.from({ length: itemCount }, (_, index) => start + step * index);
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

function clampQuickChatText(value: string) {
  return Array.from(value).slice(0, QUICK_CHAT_TEXT_LIMIT).join('');
}

function normalizeQuickChatText(value: string) {
  return clampQuickChatText(value).trim();
}

function getSettlementCalloutStyle(seat: Seat | null = null): CSSProperties {
  const position = seat ? SPOTLIGHT_POSITION_VARS[seat] : { left: '50%', top: '50%' };

  return {
    '--table-stage-action-callout-duration': SETTLEMENT_CALLOUT_DURATION_CSS,
    '--spotlight-left': position.left,
    '--spotlight-top': position.top,
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
