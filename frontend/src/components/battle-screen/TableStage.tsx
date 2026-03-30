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
}: TableStageProps) {
  const lastDiscardPosition = findLastDiscardPosition(discards, lastDiscard, lastDiscardSeat);
  const playerBySeat = new Map(players.map((player) => [player.seat, player]));
  const spotlightSeat = lastDiscardPosition?.seat ?? null;
  const spotlightTile = spotlightSeat !== null && lastDiscardPosition !== null
    ? discards[spotlightSeat][lastDiscardPosition.index]
    : null;

  return (
    <section className={`table-stage ${promptCue?.isUrgent ? 'table-stage--urgent' : ''}`} aria-label="Mahjong table">
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
          </div>
          {SEATS.map((seat) => {
            const player = playerBySeat.get(seat);

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
