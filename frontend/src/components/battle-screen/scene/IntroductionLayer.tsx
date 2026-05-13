import { memo, useMemo, type CSSProperties } from 'react';
import type { ActionEffectView, Seat } from '../../../types/match';
import type { TableStagePlayer } from './types';

interface IntroductionLayerProps {
  players: TableStagePlayer[];
  discards: Record<Seat, string[]>;
  isPlaying: boolean;
  actionEffect?: ActionEffectView | null;
}

const SPOTLIGHT_POSITION_VARS: Record<Seat, { left: string; top: string; rotation: string }> = {
  top: { left: '50%', top: 'calc(var(--table-stage-center-v) - var(--table-stage-spotlight-offset))', rotation: '0deg' },
  bottom: { left: '50%', top: 'calc(var(--table-stage-center-v) + var(--table-stage-spotlight-offset))', rotation: '0deg' },
  left: { left: 'calc(50% - var(--table-stage-spotlight-offset-horizontal))', top: 'var(--table-stage-center-v)', rotation: '0deg' },
  right: { left: 'calc(50% + var(--table-stage-spotlight-offset-horizontal))', top: 'var(--table-stage-center-v)', rotation: '0deg' },
};

export const IntroductionLayer = memo(function IntroductionLayer({
  players,
  discards,
  isPlaying,
  actionEffect = null,
}: IntroductionLayerProps) {
  const totalDiscards = useMemo(() => {
    return Object.values(discards).reduce((acc, d) => acc + d.length, 0);
  }, [discards]);
  const hasPlayerAction = useMemo(() => {
    return Boolean(actionEffect) || players.some((player) =>
      (player.flowerCount ?? 0) > 0 ||
      Boolean(player.isReadyHand) ||
      player.melds.length > 0 ||
      (player.flowers?.length ?? 0) > 0,
    );
  }, [actionEffect, players]);

  const isVisible = isPlaying && totalDiscards === 0 && !hasPlayerAction;

  if (!isVisible) {
    return null;
  }

  return (
    <div className="table-stage__intro-layer" aria-hidden="true">
      {players.map((player) => {
        const position = SPOTLIGHT_POSITION_VARS[player.seat];
        const isVertical = player.seat === 'left' || player.seat === 'right';
        const translateX = player.seat === 'left' ? '-100%' : player.seat === 'right' ? '0%' : '-50%';
        const style = {
          '--intro-left': position.left,
          '--intro-top': position.top,
          '--intro-rotation': position.rotation,
          '--intro-translate-x': translateX,
        } as CSSProperties;

        return (
          <div
            key={`intro-${player.seat}`}
            className={`table-stage__player-intro table-stage__player-intro--${player.seat} ${isVertical ? 'table-stage__player-intro--vertical' : ''}`}
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
