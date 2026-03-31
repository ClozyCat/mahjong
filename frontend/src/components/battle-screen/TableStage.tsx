import type { CSSProperties } from 'react';

import type { BattlePromptView, PlayerView, Seat } from '../../types/match';
import { MahjongTile } from './MahjongTile';
import { MeldRack } from './MeldRack';

interface TableStageProps {
  discards: Record<Seat, string[]>;
  activeSeat: Seat;
  lastDiscard: string | null;
  lastDiscardSeat?: Seat | null;
  remainingTileCount?: number | null;
  promptText: string | null;
  promptCue?: BattlePromptView | null;
  players?: Pick<PlayerView, 'seat' | 'name' | 'melds'>[];
  settlementHands?: Partial<Record<Seat, string[]>> | null;
  tileScale?: number;
  canDecreaseTileScale?: boolean;
  canIncreaseTileScale?: boolean;
  onDecreaseTileScale?: () => void;
  onIncreaseTileScale?: () => void;
}

const SEATS: Seat[] = ['top', 'left', 'right', 'bottom'];

export function TableStage({
  discards,
  activeSeat,
  lastDiscard,
  lastDiscardSeat = null,
  remainingTileCount = null,
  promptText,
  promptCue = null,
  players = [],
  settlementHands = null,
  tileScale = 1,
  canDecreaseTileScale = false,
  canIncreaseTileScale = false,
  onDecreaseTileScale,
  onIncreaseTileScale,
}: TableStageProps) {
  const lastDiscardPosition = findLastDiscardPosition(discards, lastDiscard, lastDiscardSeat);
  const playerBySeat = new Map(players.map((player) => [player.seat, player]));
  const spotlightSeat = lastDiscardPosition?.seat ?? null;
  const spotlightTile = spotlightSeat !== null && lastDiscardPosition !== null
    ? discards[spotlightSeat][lastDiscardPosition.index]
    : null;
  const spotlightScale = Math.round(tileScale * 125) / 100;
  const tableStageStyle = {
    '--table-stage-tile-scale': `${tileScale}`,
    '--table-stage-spotlight-scale': `${spotlightScale}`,
  } as CSSProperties;
  const shouldShowScaleControls = Boolean(onDecreaseTileScale || onIncreaseTileScale);
  const scalePercentLabel = `${Math.round(tileScale * 100)}%`;

  return (
    <section
      className={`table-stage ${promptCue?.isUrgent ? 'table-stage--urgent' : ''}`}
      aria-label="Mahjong table"
      style={tableStageStyle}
    >
      <div className="table-stage__frame">
        <div className="table-stage__core">
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
            {shouldShowScaleControls ? (
              <div className="table-stage__scale-controls">
                <span className="table-stage__scale-readout">牌面 {scalePercentLabel}</span>
                <div className="table-stage__scale-buttons" role="group" aria-label="调整牌桌牌面大小">
                  <button
                    type="button"
                    className="table-stage__scale-button"
                    aria-label="缩小牌桌牌面"
                    onClick={onDecreaseTileScale}
                    disabled={!canDecreaseTileScale}
                  >
                    -
                  </button>
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
              </div>
            ) : null}
          </div>
          {SEATS.map((seat) => {
            const player = playerBySeat.get(seat);
            const finalHandTiles = settlementHands?.[seat] ?? [];

            return (
              <div key={seat} className={`table-stage__seat-zone table-stage__seat-zone--${seat}`}>
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
                  {player && player.melds.length > 0 ? (
                    <div className="table-stage__melds">
                      <MeldRack seat={seat} melds={player.melds} ariaLabel={`${player.name} melds`} />
                    </div>
                  ) : null}
                </div>
                {finalHandTiles.length > 0 ? (
                  <div
                    className={`table-stage__settlement-hand table-stage__settlement-hand--${seat}`}
                    aria-label={`${player?.name ?? SEAT_COPY[seat]}终局手牌`}
                  >
                    <span className="table-stage__settlement-hand-eyebrow">终局手牌</span>
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
              </div>
            );
          })}
          {spotlightSeat && spotlightTile ? (
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
        </div>
      </div>
    </section>
  );
}

const PROMPT_KIND_COPY: Record<NonNullable<TableStageProps['promptCue']>['kind'], string> = {
  turn: '当前可操作',
  claim: '可响应',
  rob_kong: '抢杠',
};

const SEAT_COPY: Record<Seat, string> = {
  top: '上家',
  left: '左家',
  right: '右家',
  bottom: '本家',
};

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
