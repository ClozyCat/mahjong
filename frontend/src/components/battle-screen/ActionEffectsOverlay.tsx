import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

import type { ActionEffectView, CelebrationEffectView } from '../../types/match';

interface ActionEffectsOverlayProps {
  actionEffect: ActionEffectView | null;
  celebrationEffect: CelebrationEffectView | null;
  drawnTileId: string | null;
}

export function ActionEffectsOverlay({ actionEffect, celebrationEffect: _celebrationEffect, drawnTileId }: ActionEffectsOverlayProps) {
  const [activeActionEffect, setActiveActionEffect] = useState<ActionEffectView | null>(null);
  const actionEffectTimeoutRef = useRef<number | null>(null);
  const drawnFallbackTimeoutRef = useRef<number | null>(null);
  const previousActionEffectKeyRef = useRef<string | null>(null);
  const previousDrawnTileIdRef = useRef<string | null>(null);
  const portalTarget = typeof document !== 'undefined' ? document.body : null;
  const actionEffectKey = actionEffect?.key ?? null;

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
      if (drawnFallbackTimeoutRef.current !== null) {
        window.clearTimeout(drawnFallbackTimeoutRef.current);
      }
    };
  }, []);

  const actionSeatClass = activeActionEffect?.seat ? `action-effects--seat-${activeActionEffect.seat}` : 'action-effects--seat-center';
  const actionTone = activeActionEffect ? getActionTone(activeActionEffect.label, activeActionEffect.emphasis) : 'system';
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
        </div>
      ) : null}
    </>
  );

  return portalTarget ? createPortal(content, portalTarget) : content;
}

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
