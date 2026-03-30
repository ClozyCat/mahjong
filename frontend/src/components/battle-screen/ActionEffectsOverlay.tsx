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
  const actionEffectTimeoutRef = useRef<number | null>(null);
  const celebrationTimeoutRef = useRef<number | null>(null);
  const drawnFallbackTimeoutRef = useRef<number | null>(null);
  const previousActionEffectKeyRef = useRef<string | null>(null);
  const previousCelebrationKeyRef = useRef<string | null>(null);
  const previousDrawnTileIdRef = useRef<string | null>(null);
  const portalTarget = typeof document !== 'undefined' ? document.body : null;
  const actionEffectKey = actionEffect?.key ?? null;
  const celebrationEffectKey = celebrationEffect?.key ?? null;

  useEffect(() => {
    if (!actionEffect || !actionEffectKey || previousActionEffectKeyRef.current === actionEffectKey) {
      return;
    }

    previousActionEffectKeyRef.current = actionEffectKey;
    if (actionEffectTimeoutRef.current !== null) {
      window.clearTimeout(actionEffectTimeoutRef.current);
    }
    setActiveActionEffect(actionEffect);
    actionEffectTimeoutRef.current = window.setTimeout(() => {
      setActiveActionEffect((current) => (current?.key === actionEffectKey ? null : current));
      actionEffectTimeoutRef.current = null;
    }, 1650);
  }, [actionEffect, actionEffectKey]);

  useEffect(() => {
    if (!celebrationEffect || !celebrationEffectKey || previousCelebrationKeyRef.current === celebrationEffectKey) {
      return;
    }

    previousCelebrationKeyRef.current = celebrationEffectKey;
    if (celebrationTimeoutRef.current !== null) {
      window.clearTimeout(celebrationTimeoutRef.current);
    }
    setActiveCelebration(celebrationEffect);
    setCelebrationVariant(CELEBRATION_VARIANTS[Math.floor(Math.random() * CELEBRATION_VARIANTS.length)] ?? 'comet');
    celebrationTimeoutRef.current = window.setTimeout(() => {
      setActiveCelebration((current) => (current?.key === celebrationEffectKey ? null : current));
      celebrationTimeoutRef.current = null;
    }, 5200);
  }, [celebrationEffect, celebrationEffectKey]);

  useEffect(() => {
    const previousDrawnTileId = previousDrawnTileIdRef.current;
    previousDrawnTileIdRef.current = drawnTileId;

    if (actionEffect || !drawnTileId || drawnTileId === previousDrawnTileId) {
      return;
    }

    if (drawnFallbackTimeoutRef.current !== null) {
      window.clearTimeout(drawnFallbackTimeoutRef.current);
    }
    setActiveActionEffect({
      key: `drawn-${drawnTileId}`,
      label: '摸牌',
      emphasis: 'draw',
      seat: 'bottom',
    });

    drawnFallbackTimeoutRef.current = window.setTimeout(() => {
      setActiveActionEffect((current) => (current?.key === `drawn-${drawnTileId}` ? null : current));
      drawnFallbackTimeoutRef.current = null;
    }, 1650);
  }, [actionEffectKey, drawnTileId]);

  useEffect(() => {
    return () => {
      if (actionEffectTimeoutRef.current !== null) {
        window.clearTimeout(actionEffectTimeoutRef.current);
      }
      if (celebrationTimeoutRef.current !== null) {
        window.clearTimeout(celebrationTimeoutRef.current);
      }
      if (drawnFallbackTimeoutRef.current !== null) {
        window.clearTimeout(drawnFallbackTimeoutRef.current);
      }
    };
  }, []);

  const particles = useMemo(() => Array.from({ length: 14 }, (_, index) => index), []);
  const actionSeatClass = activeActionEffect?.seat ? `action-effects--seat-${activeActionEffect.seat}` : 'action-effects--seat-center';
  const celebrationSeatClass = activeCelebration?.winnerSeat
    ? `action-effects--winner-${activeCelebration.winnerSeat}`
    : 'action-effects--winner-center';
  const actionTone = activeActionEffect ? getActionTone(activeActionEffect.label, activeActionEffect.emphasis) : 'system';
  const actionGlyph = activeActionEffect ? getActionGlyph(activeActionEffect.label) : '';
  const content = (
    <>
      {activeActionEffect ? (
        <div
          className={`action-effects action-effects--action ${actionSeatClass} action-effects--type-${actionTone}`}
          aria-hidden="true"
          data-emphasis={activeActionEffect.emphasis}
        >
          <div className="action-effects__lane" />
          <div className="action-effects__trail" />
          <div className="action-effects__origin-glow" />
          <div className="action-effects__seat-flare" />
          <div className="action-effects__ring action-effects__ring--outer" />
          <div className="action-effects__ring action-effects__ring--inner" />
          <div className="action-effects__seal">
            <span>{actionGlyph}</span>
          </div>
          <div className="action-effects__caption">
            <span className="action-effects__eyebrow">{getSeatCopy(activeActionEffect.seat)}</span>
            <strong>{activeActionEffect.label}</strong>
          </div>
        </div>
      ) : null}
      {activeCelebration ? (
        <div
          className={`action-effects action-effects--celebration ${celebrationSeatClass} action-effects--${celebrationVariant} action-effects--win-${activeCelebration.winType}`}
          aria-hidden="true"
        >
          <div className="action-effects__veil" />
          <div className="action-effects__winner-beam" />
          <div className="action-effects__winner-glow" />
          <div className="action-effects__winner-seal">
            <span>{activeCelebration.winType === 'self_draw' ? '自摸' : '和'}</span>
          </div>
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

function getActionTone(label: string, emphasis: ActionEffectView['emphasis']) {
  if (label === '摸牌' || label === '补花' || label === '补牌') {
    return 'draw';
  }

  if (label === '出牌') {
    return 'discard';
  }

  if (label === '吃') {
    return 'chow';
  }

  if (label === '碰') {
    return 'pung';
  }

  if (label.includes('杠')) {
    return 'kong';
  }

  if (label.includes('胡')) {
    return 'hu';
  }

  return emphasis === 'system' ? 'system' : 'claim';
}

function getActionGlyph(label: string) {
  if (label === '摸牌') {
    return '摸';
  }

  if (label === '补花') {
    return '花';
  }

  if (label === '补牌') {
    return '补';
  }

  if (label === '出牌') {
    return '打';
  }

  if (label === '吃') {
    return '吃';
  }

  if (label === '碰') {
    return '碰';
  }

  if (label.includes('杠')) {
    return '杠';
  }

  if (label.includes('胡')) {
    return '和';
  }

  if (label === '流局') {
    return '流';
  }

  return '局';
}
