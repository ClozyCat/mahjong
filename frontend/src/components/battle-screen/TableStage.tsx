import { Fragment, type CSSProperties } from 'react';

import type { ThemeId } from '../../lib/themes';
import type { BattleActionView, BattlePromptView, Seat } from '../../types/match';
import { MahjongTile } from './MahjongTile';
import { MeldRack } from './MeldRack';
import { PlayerInfoBar, type TableStagePlayer } from './PlayerInfoBar';

interface TableStageProps {
  discards: Record<Seat, string[]>;
  activeSeat: Seat;
  actionIndicatorSeat?: Seat | null;
  lastDiscard: string | null;
  lastDiscardSeat?: Seat | null;
  remainingTileCount?: number | null;
  promptText: string | null;
  promptCue?: BattlePromptView | null;
  players?: TableStagePlayer[];
  settlementHands?: Partial<Record<Seat, string[]>> | null;
  tableCode?: string;
  roundLabel?: string;
  phaseLabel?: string;
  occupiedSeatCount?: number;
  seatCapacity?: number;
  preMatchActions?: BattleActionView[];
  tileScale?: number;
  canDecreaseTileScale?: boolean;
  canIncreaseTileScale?: boolean;
  canLeaveTable?: boolean;
  themeId?: ThemeId;
  themeLabel?: string;
  onLeaveTable?: () => void;
  onCycleTheme?: () => void;
  onAction?: (actionId: BattleActionView['id']) => void;
  onDecreaseTileScale?: () => void;
  onIncreaseTileScale?: () => void;
}

const SEATS: Seat[] = ['top', 'left', 'right', 'bottom'];

export function TableStage({
  discards,
  activeSeat,
  actionIndicatorSeat = null,
  lastDiscard,
  lastDiscardSeat = null,
  remainingTileCount = null,
  promptText,
  promptCue = null,
  players = [],
  settlementHands = null,
  tableCode = '',
  roundLabel = '',
  phaseLabel = '',
  occupiedSeatCount,
  seatCapacity = 4,
  preMatchActions = [],
  tileScale = 1,
  canDecreaseTileScale = false,
  canIncreaseTileScale = false,
  canLeaveTable = false,
  themeId = 'tian-shui-bi',
  themeLabel = '天水碧',
  onLeaveTable,
  onCycleTheme,
  onAction,
  onDecreaseTileScale,
  onIncreaseTileScale,
}: TableStageProps) {
  const lastDiscardPosition = findLastDiscardPosition(discards, lastDiscard, lastDiscardSeat);
  const playerBySeat = new Map(players.map((player) => [player.seat, player]));
  const resolvedOccupiedSeatCount = occupiedSeatCount ?? players.length;
  const spotlightSeat = lastDiscardPosition?.seat ?? null;
  const spotlightPlayer = spotlightSeat ? playerBySeat.get(spotlightSeat) : null;
  const spotlightTile = spotlightSeat !== null && lastDiscardPosition !== null
    ? discards[spotlightSeat][lastDiscardPosition.index]
    : null;
  const spotlightScale = Math.round(tileScale * 125) / 100;
  const tableSummary = buildTableSummary(roundLabel, phaseLabel);
  const shouldShowScaleControls = Boolean(onDecreaseTileScale || onIncreaseTileScale);
  const shouldShowPreMatchActions = preMatchActions.length > 0;
  const scalePercentLabel = `${Math.round(tileScale * 100)}%`;
  const tableStageStyle = {
    '--table-stage-tile-scale': `${tileScale}`,
    '--table-stage-spotlight-scale': `${spotlightScale}`,
  } as CSSProperties;

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
          {onCycleTheme || canLeaveTable ? (
            <div className="table-stage__corner-controls">
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
          ) : null}
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
          {SEATS.map((seat) => {
            const player = playerBySeat.get(seat);
            const finalHandTiles = settlementHands?.[seat] ?? [];
            const settlementHandLabel = SETTLEMENT_HAND_COPY[seat];
            const shouldRenderSeatInfo = Boolean(player);

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
                  {player && player.melds.length > 0 ? (
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
                            className="table-stage__settlement-hand-tile"
                          />
                        ))}
                      </div>
                    </div>
                  ) : null}
                  {shouldRenderSeatInfo && player ? (
                    <PlayerInfoBar player={player} className={`table-stage__player-info--${seat}`} />
                  ) : null}
                </div>
              </Fragment>
            );
          })}
          {spotlightSeat && spotlightTile ? (
            <div
              className={`table-stage__spotlight table-stage__spotlight--${spotlightSeat} ${
                spotlightPlayer?.isDealer ? 'table-stage__spotlight--dealer' : ''
              } ${
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
        </div>
      </div>
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

function buildTableSummary(roundLabel: string, phaseLabel: string) {
  if (roundLabel && phaseLabel) {
    return `${roundLabel} | ${phaseLabel}`;
  }

  return roundLabel || phaseLabel || null;
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
