import { Fragment, type CSSProperties } from 'react';

import type { BattlePromptView, PlayerView, Seat } from '../../types/match';
import { MahjongTile } from './MahjongTile';
import { MeldRack } from './MeldRack';

type TableStagePlayer = Pick<PlayerView, 'seat' | 'name' | 'melds'> &
  Partial<Omit<PlayerView, 'seat' | 'name' | 'melds'>>;

interface TableStageProps {
  discards: Record<Seat, string[]>;
  activeSeat: Seat;
  lastDiscard: string | null;
  lastDiscardSeat?: Seat | null;
  remainingTileCount?: number | null;
  promptText: string | null;
  promptCue?: BattlePromptView | null;
  players?: TableStagePlayer[];
  settlementHands?: Partial<Record<Seat, string[]>> | null;
  tileScale?: number;
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
}: TableStageProps) {
  const lastDiscardPosition = findLastDiscardPosition(discards, lastDiscard, lastDiscardSeat);
  const playerBySeat = new Map(players.map((player) => [player.seat, player]));
  const playerAccentStyleBySeat = new Map(players.map((player) => [player.seat, buildPlayerAccentStyle(player)]));
  const spotlightSeat = lastDiscardPosition?.seat ?? null;
  const spotlightTile = spotlightSeat !== null && lastDiscardPosition !== null
    ? discards[spotlightSeat][lastDiscardPosition.index]
    : null;
  const spotlightScale = Math.round(tileScale * 125) / 100;
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
            const finalHandTiles = settlementHands?.[seat] ?? [];
            const settlementHandLabel = SETTLEMENT_HAND_COPY[seat];

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
                    <div className={`table-stage__melds table-stage__melds--${seat}`}>
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
                </div>
                {player ? renderPlayerInfoBar(player, playerAccentStyleBySeat.get(seat)) : null}
              </Fragment>
            );
          })}
          {spotlightSeat && spotlightTile ? (
            <div
              className={`table-stage__spotlight table-stage__spotlight--${spotlightSeat} ${
                promptCue?.isUrgent && promptCue.sourceSeat === spotlightSeat ? 'table-stage__spotlight--urgent' : ''
              }`}
              aria-label="Latest discard spotlight"
              style={playerAccentStyleBySeat.get(spotlightSeat)}
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

const SETTLEMENT_HAND_COPY: Partial<Record<Seat, string>> = {
  top: '对家手牌',
  left: '左家手牌',
  right: '右家手牌',
};

const WIND_LABELS: Partial<Record<PlayerView['wind'], string>> = {
  East: '东',
  South: '南',
  West: '西',
  North: '北',
};

const SEAT_ACCENT_OFFSET: Record<Seat, number> = {
  bottom: 29,
  left: 107,
  top: 191,
  right: 277,
};

function renderPlayerInfoBar(player: TableStagePlayer, accentStyle?: CSSProperties) {
  const windLabel = player.wind ? (WIND_LABELS[player.wind] ?? player.wind) : null;
  const presenceLabel = player.isBotControlled ? '离线' : player.connected === false ? '离线' : '在线';
  const eyebrowText = [windLabel, player.isDealer ? '庄家' : null, presenceLabel].filter(Boolean).join(' · ');
  const metaText = [
    typeof player.score === 'number' ? player.score.toLocaleString() : null,
    player.statusText ?? '待命',
  ]
    .filter(Boolean)
    .join(' · ');
  const detailText = `手牌 ${typeof player.concealedCount === 'number' ? player.concealedCount : '--'} · 花 ${
    typeof player.flowerCount === 'number' ? player.flowerCount : '--'
  }`;

  return (
    <article
      className={`table-stage__player-info table-stage__player-info--${player.seat} ${
        player.isActive ? 'table-stage__player-info--active' : ''
      } ${player.isLocal ? 'table-stage__player-info--local' : ''}`}
      style={accentStyle}
      aria-label={`${player.name} 信息栏`}
    >
      <span className="table-stage__player-info-eyebrow">{eyebrowText}</span>
      <strong className="table-stage__player-info-name">{player.name}</strong>
      <span className="table-stage__player-info-meta">{metaText}</span>
      <span className="table-stage__player-info-detail">{detailText}</span>
    </article>
  );
}

function buildPlayerAccentStyle(player: Pick<TableStagePlayer, 'seat' | 'name'>): CSSProperties {
  const seed = `${player.seat}:${player.name}`;
  let hash = 0;

  for (const char of seed) {
    hash = (hash * 33 + char.charCodeAt(0)) | 0;
  }

  const hue = (Math.abs(hash) + SEAT_ACCENT_OFFSET[player.seat]) % 360;

  return {
    '--table-player-accent': `hsl(${hue}, 82%, 68%)`,
    '--table-player-accent-strong': `hsla(${hue}, 86%, 62%, 0.72)`,
    '--table-player-accent-soft': `hsla(${hue}, 86%, 62%, 0.22)`,
    '--table-player-accent-surface': `hsla(${hue}, 86%, 62%, 0.12)`,
    '--table-player-accent-shadow': `hsla(${hue}, 92%, 58%, 0.28)`,
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
