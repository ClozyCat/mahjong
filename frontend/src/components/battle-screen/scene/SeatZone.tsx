import { memo, type CSSProperties } from 'react';

import type { Seat } from '../../../types/match';
import { MahjongTile } from '../MahjongTile';
import { MeldRack } from '../MeldRack';

import type { TableSeatScene } from './sceneModel';

interface SeatZoneProps {
  scene: TableSeatScene;
  discards: string[];
  activeSeat: Seat;
  lastDiscardSeat: Seat | null;
  selectedTileCode: string | null;
  hasSpotlightDiscard: boolean;
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

export const SeatZone = memo(function SeatZone({
  scene,
  discards,
  activeSeat,
  lastDiscardSeat,
  selectedTileCode,
  hasSpotlightDiscard,
}: SeatZoneProps) {
  const { player, seat } = scene;

  return (
    <div
      className={`table-stage__seat-zone table-stage__seat-zone--${seat}`}
      style={scene.zoneStyle}
      data-seat={seat}
    >
      {player ? (
        <div
          className={`table-stage__player-edge-info table-stage__player-edge-info--${seat} ${player.isReadyHand ? 'table-stage__player-edge-info--tenpai' : ''}`}
        >
          <div className="table-stage__player-stats">
            <div
              className="table-stage__stat-plate table-stage__stat-plate--seat"
              data-player-name={player.name}
              data-absolute-seat={player.absoluteSeat}
            >
              <FanIcon className="table-stage__stat-icon" />
              <span className="table-stage__stat-value">{scene.windLabel}</span>
            </div>
            <div
              className={`table-stage__stat-plate table-stage__stat-plate--score${scene.shouldMuteWaitingStats ? ' table-stage__stat-plate--muted' : ''}`}
              title="分数"
            >
              <IngotIcon className="table-stage__stat-icon" />
              <span className="table-stage__stat-value">{player.score?.toLocaleString() ?? 0}</span>
            </div>
            <div
              className={`table-stage__stat-plate table-stage__stat-plate--flower${scene.shouldMuteWaitingStats ? ' table-stage__stat-plate--muted' : ''}`}
              title="花牌数量"
            >
              <LotusIcon className="table-stage__stat-icon" />
              <span className="table-stage__stat-value">{player.flowerCount ?? 0}</span>
            </div>
            <div
              className={`table-stage__stat-plate table-stage__stat-plate--hand${scene.shouldMuteWaitingStats ? ' table-stage__stat-plate--muted' : ''}`}
              title="手牌数量"
            >
              <TileStackIcon className="table-stage__stat-icon" />
              <span className="table-stage__stat-value">{player.concealedCount ?? 0}</span>
            </div>
            {player.isReadyHand ? (
              <div className="table-stage__stat-plate table-stage__stat-plate--tenpai" title="已听牌">
                <ReadyIcon className="table-stage__stat-icon" />
                <span className="table-stage__stat-value">听</span>
                <div className="table-stage__tenpai-glow" />
              </div>
            ) : null}
          </div>
        </div>
      ) : null}

      <div className={`table-stage__seat-group table-stage__seat-group--${seat}`}>
        <div className={`table-stage__seat-panel table-stage__seat-panel--${seat}`}>
          <div
            className={`table-stage__river table-stage__river--${seat} ${activeSeat === seat ? 'table-stage__river--active' : ''}`.trim()}
          >
            <div
              className={`table-stage__river-track table-stage__river-track--${seat}`}
              style={scene.trackStyle}
            >
              {discards.map((tile, index) => {
                const isLastDiscard = lastDiscardSeat === seat && index === discards.length - 1;
                const isSpotlightHidden = isLastDiscard && hasSpotlightDiscard;

                return (
                  <MahjongTile
                    key={`river-${seat}-${index}-${tile}`}
                    code={tile}
                    variant="discard"
                    isLastDiscard={isLastDiscard}
                    relatedTileCode={selectedTileCode}
                    style={isSpotlightHidden ? { visibility: 'hidden' } : undefined}
                  />
                );
              })}
            </div>
          </div>
        </div>

        {player && scene.hasMelds ? (
          <div
            className={`table-stage__melds table-stage__melds--${seat} ${scene.isDenseMeldRack ? 'table-stage__melds--dense' : ''}`.trim()}
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
    </div>
  );
});

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

function ReadyIcon({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 32 32" className={className} fill="currentColor">
      <circle cx="16" cy="16" r="10" opacity="0.2" />
      <path d="M16 8C11.6 8 8 11.6 8 16C8 20.4 11.6 24 16 24C20.4 24 24 20.4 24 16C24 11.6 20.4 8 16 8ZM16 22C12.7 22 10 19.3 10 16C10 12.7 12.7 10 16 10C19.3 10 22 12.7 22 16C22 19.3 19.3 22 16 22Z" />
      <path d="M16 12C13.8 12 12 13.8 12 16C12 18.2 13.8 20 16 20C18.2 20 20 18.2 20 16C20 13.8 18.2 12 16 12Z" className="table-stage__tenpai-pulse" />
    </svg>
  );
}
