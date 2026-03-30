import { useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { createPortal } from 'react-dom';

import type { ActionEffectView, CelebrationEffectView } from '../../types/match';

interface ActionEffectsOverlayProps {
  actionEffect: ActionEffectView | null;
  celebrationEffect: CelebrationEffectView | null;
  drawnTileId: string | null;
}

const CELEBRATION_VARIANTS = ['comet', 'burst', 'ribbon'] as const;

export function ActionEffectsOverlay({ actionEffect, celebrationEffect, drawnTileId }: ActionEffectsOverlayProps) {
  const [activeActionEffect, setActiveActionEffect] = useState<ActionEffectView | null>(null);
  const [activeCelebration, setActiveCelebration] = useState<CelebrationEffectView | null>(null);
  const [celebrationVariant, setCelebrationVariant] = useState<(typeof CELEBRATION_VARIANTS)[number]>('comet');
  const previousDrawnTileIdRef = useRef<string | null>(null);
  const portalTarget = typeof document !== 'undefined' ? document.body : null;

  useEffect(() => {
    if (!actionEffect) {
      return;
    }

    setActiveActionEffect(actionEffect);
    const timeoutId = window.setTimeout(() => {
      setActiveActionEffect((current) => (current?.key === actionEffect.key ? null : current));
    }, 1650);

    return () => window.clearTimeout(timeoutId);
  }, [actionEffect]);

  useEffect(() => {
    if (!celebrationEffect) {
      return;
    }

    setActiveCelebration(celebrationEffect);
    setCelebrationVariant(CELEBRATION_VARIANTS[Math.floor(Math.random() * CELEBRATION_VARIANTS.length)] ?? 'comet');
    const timeoutId = window.setTimeout(() => {
      setActiveCelebration((current) => (current?.key === celebrationEffect.key ? null : current));
    }, 5200);

    return () => window.clearTimeout(timeoutId);
  }, [celebrationEffect]);

  useEffect(() => {
    const previousDrawnTileId = previousDrawnTileIdRef.current;
    previousDrawnTileIdRef.current = drawnTileId;

    if (actionEffect || !drawnTileId || drawnTileId === previousDrawnTileId) {
      return;
    }

    setActiveActionEffect({
      key: `drawn-${drawnTileId}`,
      label: '摸牌',
      emphasis: 'draw',
      seat: 'bottom',
    });

    const timeoutId = window.setTimeout(() => {
      setActiveActionEffect((current) => (current?.key === `drawn-${drawnTileId}` ? null : current));
    }, 1650);

    return () => window.clearTimeout(timeoutId);
  }, [actionEffect, drawnTileId]);

  const particles = useMemo(() => Array.from({ length: 14 }, (_, index) => index), []);
  const actionSeatClass = activeActionEffect?.seat ? `action-effects--seat-${activeActionEffect.seat}` : 'action-effects--seat-center';
  const celebrationSeatClass = activeCelebration?.winnerSeat
    ? `action-effects--winner-${activeCelebration.winnerSeat}`
    : 'action-effects--winner-center';
  const content = (
    <>
      {activeActionEffect ? (
        <div
          className={`action-effects action-effects--action ${actionSeatClass}`}
          aria-hidden="true"
          data-emphasis={activeActionEffect.emphasis}
        >
          <div className="action-effects__trail" />
          <div className="action-effects__origin-glow" />
          <div className="action-effects__ring action-effects__ring--outer" />
          <div className="action-effects__ring action-effects__ring--inner" />
          <div className="action-effects__caption">
            <span className="action-effects__eyebrow">{getSeatCopy(activeActionEffect.seat)}</span>
            <strong>{activeActionEffect.label}</strong>
          </div>
        </div>
      ) : null}
      {activeCelebration ? (
        <div
          className={`action-effects action-effects--celebration ${celebrationSeatClass} action-effects--${celebrationVariant}`}
          aria-hidden="true"
        >
          <div className="action-effects__veil" />
          <div className="action-effects__winner-beam" />
          <div className="action-effects__winner-glow" />
          <div className="action-effects__celebration-copy">
            <span className="action-effects__eyebrow">{getSeatCopy(activeCelebration.winnerSeat)}</span>
            <strong>{activeCelebration.label}</strong>
            <em>{activeCelebration.winType === 'self_draw' ? '华彩自摸' : '荣耀和牌'}</em>
          </div>
          <div className="action-effects__particles">
            {particles.map((particle) => (
              <span
                key={particle}
                className="action-effects__particle"
                style={
                  {
                    '--particle-index': particle,
                  } as CSSProperties
                }
              />
            ))}
          </div>
        </div>
      ) : null}
    </>
  );

  return portalTarget ? createPortal(content, portalTarget) : content;
}

function getSeatCopy(seat: ActionEffectView['seat'] | CelebrationEffectView['winnerSeat']) {
  if (!seat) {
    return '牌局播报';
  }

  return SEAT_COPY[seat];
}

const SEAT_COPY: Record<NonNullable<ActionEffectView['seat']>, string> = {
  bottom: '你',
  left: '左家',
  top: '对家',
  right: '右家',
};
