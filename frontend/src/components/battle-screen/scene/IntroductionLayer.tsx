import { memo, useMemo, type CSSProperties } from 'react';
import type { Seat } from '../../../types/match';
import type { TableStagePlayer } from './types';

interface IntroductionLayerProps {
  players: TableStagePlayer[];
  discards: Record<Seat, string[]>;
  isPlaying: boolean;
}

const SPOTLIGHT_POSITION_VARS: Record<Seat, { left: string; top: string; rotation: string }> = {
  top: { left: '50%', top: 'calc(var(--table-stage-center-v) - var(--table-stage-spotlight-offset))', rotation: '180deg' },
  bottom: { left: '50%', top: 'calc(var(--table-stage-center-v) + var(--table-stage-spotlight-offset))', rotation: '0deg' },
  left: { left: 'calc(50% - var(--table-stage-spotlight-offset-horizontal))', top: 'var(--table-stage-center-v)', rotation: '90deg' },
  right: { left: 'calc(50% + var(--table-stage-spotlight-offset-horizontal))', top: 'var(--table-stage-center-v)', rotation: '-90deg' },
};

export const IntroductionLayer = memo(function IntroductionLayer({
  players,
  discards,
  isPlaying,
}: IntroductionLayerProps) {
  const totalDiscards = useMemo(() => {
    return Object.values(discards).reduce((acc, d) => acc + d.length, 0);
  }, [discards]);

  const isVisible = isPlaying && totalDiscards === 0;

  if (!isVisible) {
    return null;
  }

  return (
    <div className="table-stage__intro-layer" aria-hidden="true">
      {players.map((player) => {
        const position = SPOTLIGHT_POSITION_VARS[player.seat];
        const style = {
          '--intro-left': position.left,
          '--intro-top': position.top,
          '--intro-rotation': position.rotation,
        } as CSSProperties;

        return (
          <div
            key={`intro-${player.seat}`}
            className={`table-stage__player-intro table-stage__player-intro--${player.seat}`}
            style={style}
          >
            <div className="table-stage__player-intro-content">
              <span className="table-stage__player-intro-name">{player.name}</span>
              {player.title && (
                <>
                  <span className="table-stage__player-intro-divider">-</span>
                  <span className="table-stage__player-intro-title">{player.title}</span>
                </>
              )}
              <span className="table-stage__player-intro-divider">-</span>
              <span className="table-stage__player-intro-points">
                {(player.points ?? 0).toLocaleString()}
              </span>
            </div>
          </div>
        );
      })}
    </div>
  );
});
