import { memo, useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';

import type { ActionEffectView, BattlePromptView, QuickChatEventView, Seat } from '../../../types/match';
import { MahjongTile } from '../MahjongTile';
import { SETTLEMENT_CALLOUT_DURATION_CSS, SETTLEMENT_CALLOUT_LINGER_MS } from '../settlementTiming';

interface MotionLayerProps {
  discards: Record<Seat, string[]>;
  selectedTileCode?: string | null;
  lastDiscard: string | null;
  lastDiscardSeat?: Seat | null;
  settlementWinnerSeat?: Seat | null;
  settlementWinnerSeats?: Seat[];
  settlementWinType?: string | null;
  settlementWinTypeLabel?: string | null;
  settlementCenterCalloutLabel?: string | null;
  promptCue?: BattlePromptView | null;
  actionEffect?: ActionEffectView | null;
  quickChatEvent?: QuickChatEventView | null;
}

const ACTION_CALLOUT_COPY = {
  chow: '吃',
  pung: '碰',
  kong: '杠',
  hu: '和',
  ready_hand: '听',
} as const;

const ACTION_CALLOUT_LINGER_MS = SETTLEMENT_CALLOUT_LINGER_MS;
const READY_HAND_CALLOUT_LINGER_MS = 1000;
const QUICK_CHAT_BARRAGE_LINGER_MS = 9000;

type ActionCallout = {
  key: string;
  seat: Seat;
  tone: keyof typeof ACTION_CALLOUT_COPY;
  label: (typeof ACTION_CALLOUT_COPY)[keyof typeof ACTION_CALLOUT_COPY];
  huVariant: 'discard' | 'self-draw' | null;
};

type BarrageMessage = {
  key: string;
  text: string;
  topPercent: number;
};

const SPOTLIGHT_POSITION_VARS: Record<Seat, { left: string; top: string; rotation: string }> = {
  top: { left: '50%', top: 'calc(var(--table-stage-center-v) - var(--table-stage-spotlight-offset))', rotation: '180deg' },
  bottom: { left: '50%', top: 'calc(var(--table-stage-center-v) + var(--table-stage-spotlight-offset))', rotation: '0deg' },
  left: { left: 'calc(50% - var(--table-stage-spotlight-offset))', top: 'var(--table-stage-center-v)', rotation: '90deg' },
  right: { left: 'calc(50% + var(--table-stage-spotlight-offset))', top: 'var(--table-stage-center-v)', rotation: '-90deg' },
};

function getActionCalloutLingerMs(callout: ActionCallout) {
  if (callout.tone === 'ready_hand') {
    return READY_HAND_CALLOUT_LINGER_MS;
  }

  return ACTION_CALLOUT_LINGER_MS;
}

function getRandomBarrageTopPercent() {
  return 18 + Math.round(Math.random() * 60);
}

function getUniqueSeats(seats: Array<Seat | null | undefined>) {
  return Array.from(new Set(seats.filter((seat): seat is Seat => seat !== null && seat !== undefined)));
}

function getHuCalloutVariant(settlementWinType: string | null, settlementWinTypeLabel: string | null) {
  if (settlementWinType === 'self_draw') {
    return 'self-draw';
  }

  if (settlementWinType === 'discard') {
    return 'discard';
  }

  return settlementWinTypeLabel === '自摸' ? 'self-draw' : null;
}

function createActionCallout(
  actionEffect: ActionEffectView | null,
  settlementWinnerSeat: Seat | null,
  settlementWinType: string | null,
  settlementWinTypeLabel: string | null,
) {
  if (!actionEffect?.calloutTone) {
    return null;
  }

  const seat = actionEffect.seat ?? (actionEffect.calloutTone === 'hu' ? settlementWinnerSeat : null);
  if (!seat) {
    return null;
  }

  return {
    key: actionEffect.key,
    seat,
    tone: actionEffect.calloutTone,
    label: ACTION_CALLOUT_COPY[actionEffect.calloutTone],
    huVariant:
      actionEffect.calloutTone === 'hu'
        ? getHuCalloutVariant(settlementWinType, settlementWinTypeLabel)
        : null,
  } satisfies ActionCallout;
}

function createSettlementHuCallout(
  seat: Seat,
  settlementWinType: string | null,
  settlementWinTypeLabel: string | null,
): ActionCallout {
  return {
    key: `settlement-hu:${seat}`,
    seat,
    tone: 'hu',
    label: ACTION_CALLOUT_COPY.hu,
    huVariant: getHuCalloutVariant(settlementWinType, settlementWinTypeLabel),
  };
}

function getSettlementCalloutStyle(seat: Seat | null = null): CSSProperties {
  const position = seat
    ? SPOTLIGHT_POSITION_VARS[seat]
    : { left: '50%', top: 'var(--table-stage-center-v)', rotation: '0deg' };

  return {
    '--table-stage-action-callout-duration': SETTLEMENT_CALLOUT_DURATION_CSS,
    '--spotlight-left': position.left,
    '--spotlight-top': position.top,
    '--spotlight-rotation': position.rotation,
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

  (['top', 'left', 'right', 'bottom'] as Seat[]).forEach((seat) => {
    discards[seat].forEach((tile, index) => {
      if (tile === lastDiscard) {
        match = { seat, index };
      }
    });
  });

  return match;
}

export const MotionLayer = memo(function MotionLayer({
  discards,
  selectedTileCode = null,
  lastDiscard,
  lastDiscardSeat = null,
  settlementWinnerSeat = null,
  settlementWinnerSeats = [],
  settlementWinType = null,
  settlementWinTypeLabel = null,
  settlementCenterCalloutLabel = null,
  promptCue = null,
  actionEffect = null,
  quickChatEvent = null,
}: MotionLayerProps) {
  const lastDiscardPosition = useMemo(
    () => findLastDiscardPosition(discards, lastDiscard, lastDiscardSeat),
    [discards, lastDiscard, lastDiscardSeat],
  );
  const settlementWinningSeats = useMemo(
    () => getUniqueSeats([settlementWinnerSeat, ...settlementWinnerSeats]),
    [settlementWinnerSeat, settlementWinnerSeats],
  );
  const simultaneousSettlementHuCallouts = useMemo(
    () =>
      settlementWinType === 'discard' && settlementWinningSeats.length > 1
        ? settlementWinningSeats.map((seat) =>
          createSettlementHuCallout(seat, settlementWinType, settlementWinTypeLabel),
        )
        : [],
    [settlementWinningSeats, settlementWinType, settlementWinTypeLabel],
  );
  const incomingActionCallout = useMemo(
    () =>
      createActionCallout(
        actionEffect,
        settlementWinnerSeat,
        settlementWinType,
        settlementWinTypeLabel,
      ),
    [actionEffect, settlementWinnerSeat, settlementWinType, settlementWinTypeLabel],
  );
  const [activeActionCallout, setActiveActionCallout] = useState<ActionCallout | null>(null);
  const [pendingActionCallouts, setPendingActionCallouts] = useState<ActionCallout[]>([]);
  const [barrageMessages, setBarrageMessages] = useState<BarrageMessage[]>([]);
  const activeActionCalloutRef = useRef<ActionCallout | null>(null);
  const pendingActionCalloutsRef = useRef<ActionCallout[]>([]);
  const activeActionCalloutTimerRef = useRef<number | null>(null);
  const trackedSpotlightKeyRef = useRef<string | null>(null);
  const consumedActionCalloutKeyRef = useRef<string | null>(null);
  const consumedQuickChatKeyRef = useRef<string | null>(quickChatEvent?.key ?? null);
  const barrageRemovalTimersRef = useRef<Map<string, number>>(new Map());
  const spotlightSeat = lastDiscardPosition?.seat ?? null;
  const spotlightTile =
    spotlightSeat !== null && lastDiscardPosition !== null
      ? discards[spotlightSeat][lastDiscardPosition.index]
      : null;
  const spotlightKey =
    spotlightSeat !== null && spotlightTile !== null && lastDiscardPosition !== null
      ? `${spotlightSeat}:${lastDiscardPosition.index}:${spotlightTile}`
      : null;
  const hasSpotlightDiscard = spotlightSeat !== null && spotlightTile !== null;
  const shouldDelaySpotlightForIncomingReadyHand =
    incomingActionCallout?.tone === 'ready_hand' &&
    incomingActionCallout.seat === spotlightSeat &&
    consumedActionCalloutKeyRef.current !== incomingActionCallout.key;
  const shouldDelaySpotlightForReadyHand =
    hasSpotlightDiscard &&
    (shouldDelaySpotlightForIncomingReadyHand ||
      (activeActionCallout?.tone === 'ready_hand' && activeActionCallout.seat === spotlightSeat));
  const shouldShowSimultaneousSettlementHuCallouts = simultaneousSettlementHuCallouts.length > 0;

  useEffect(() => {
    activeActionCalloutRef.current = activeActionCallout;
  }, [activeActionCallout]);

  useEffect(() => {
    pendingActionCalloutsRef.current = pendingActionCallouts;
  }, [pendingActionCallouts]);

  useEffect(() => {
    return () => {
      if (activeActionCalloutTimerRef.current !== null) {
        window.clearTimeout(activeActionCalloutTimerRef.current);
      }

      barrageRemovalTimersRef.current.forEach((timer) => window.clearTimeout(timer));
      barrageRemovalTimersRef.current.clear();
    };
  }, []);

  useEffect(() => {
    if (!actionEffect) {
      return;
    }

    const nextActionCallout = incomingActionCallout;
    const actionCalloutKey = nextActionCallout?.key ?? actionEffect?.key;
    const currentActionCallout = activeActionCalloutRef.current;
    if (!actionCalloutKey || !nextActionCallout) {
      return;
    }

    if (consumedActionCalloutKeyRef.current === actionCalloutKey) {
      return;
    }

    if (currentActionCallout?.key === actionCalloutKey) {
      return;
    }

    if (pendingActionCalloutsRef.current.some((callout) => callout.key === actionCalloutKey)) {
      return;
    }

    const showActionCallout = (callout: ActionCallout) => {
      if (activeActionCalloutTimerRef.current !== null) {
        window.clearTimeout(activeActionCalloutTimerRef.current);
        activeActionCalloutTimerRef.current = null;
      }

      setActiveActionCallout(callout);
      activeActionCalloutTimerRef.current = window.setTimeout(() => {
        activeActionCalloutTimerRef.current = null;
        const [nextCallout, ...remainingCallouts] = pendingActionCalloutsRef.current;
        pendingActionCalloutsRef.current = remainingCallouts;
        setPendingActionCallouts(remainingCallouts);

        if (activeActionCalloutRef.current?.key !== callout.key) {
          return;
        }

        if (nextCallout) {
          showActionCallout(nextCallout);
          return;
        }

        setActiveActionCallout(null);
      }, getActionCalloutLingerMs(callout));
    };

    consumedActionCalloutKeyRef.current = actionCalloutKey;
    if (currentActionCallout && nextActionCallout.tone === 'hu' && currentActionCallout.tone !== 'hu') {
      pendingActionCalloutsRef.current = pendingActionCalloutsRef.current.filter(
        (callout) => callout.key !== actionCalloutKey,
      );
      setPendingActionCallouts(pendingActionCalloutsRef.current);
      showActionCallout(nextActionCallout);
      return;
    }

    if (currentActionCallout) {
      const nextPendingCallouts = [...pendingActionCalloutsRef.current, nextActionCallout];
      pendingActionCalloutsRef.current = nextPendingCallouts;
      setPendingActionCallouts(nextPendingCallouts);
      return;
    }

    showActionCallout(nextActionCallout);
  }, [actionEffect, incomingActionCallout]);

  useEffect(() => {
    if (!shouldShowSimultaneousSettlementHuCallouts) {
      return;
    }

    if (activeActionCalloutTimerRef.current !== null && activeActionCalloutRef.current?.tone === 'hu') {
      window.clearTimeout(activeActionCalloutTimerRef.current);
      activeActionCalloutTimerRef.current = null;
    }

    const nextPendingCallouts = pendingActionCalloutsRef.current.filter((callout) => callout.tone !== 'hu');
    if (nextPendingCallouts.length !== pendingActionCalloutsRef.current.length) {
      pendingActionCalloutsRef.current = nextPendingCallouts;
      setPendingActionCallouts(nextPendingCallouts);
    }

    if (activeActionCalloutRef.current?.tone === 'hu') {
      activeActionCalloutRef.current = null;
      setActiveActionCallout(null);
    }
  }, [shouldShowSimultaneousSettlementHuCallouts]);

  useEffect(() => {
    if (spotlightKey === trackedSpotlightKeyRef.current) {
      return;
    }

    trackedSpotlightKeyRef.current = spotlightKey;
    const currentActionCallout = activeActionCalloutRef.current;

    if (!spotlightKey || !spotlightSeat || !currentActionCallout || currentActionCallout.seat !== spotlightSeat) {
      return;
    }

    if (activeActionCalloutTimerRef.current !== null) {
      window.clearTimeout(activeActionCalloutTimerRef.current);
      activeActionCalloutTimerRef.current = null;
    }

    pendingActionCalloutsRef.current = [];
    setPendingActionCallouts([]);
    setActiveActionCallout(null);
  }, [spotlightKey, spotlightSeat]);

  useEffect(() => {
    if (!quickChatEvent?.key || consumedQuickChatKeyRef.current === quickChatEvent.key) {
      return;
    }

    consumedQuickChatKeyRef.current = quickChatEvent.key;
    const nextBarrageMessage: BarrageMessage = {
      key: quickChatEvent.key,
      text: quickChatEvent.text,
      topPercent: getRandomBarrageTopPercent(),
    };

    setBarrageMessages((current) => [...current, nextBarrageMessage]);

    const timer = window.setTimeout(() => {
      setBarrageMessages((current) => current.filter((message) => message.key !== quickChatEvent.key));
      barrageRemovalTimersRef.current.delete(quickChatEvent.key);
    }, QUICK_CHAT_BARRAGE_LINGER_MS);

    barrageRemovalTimersRef.current.set(quickChatEvent.key, timer);
  }, [quickChatEvent]);

  return (
    <>
      {barrageMessages.length > 0 ? (
        <div className="table-stage__barrage-layer" aria-hidden="true">
          {barrageMessages.map((message) => (
            <div
              key={message.key}
              className="table-stage__barrage-message"
              style={{ '--table-stage-barrage-top': `${message.topPercent}%` } as CSSProperties}
            >
              {message.text}
            </div>
          ))}
        </div>
      ) : null}
      {hasSpotlightDiscard && !shouldDelaySpotlightForReadyHand ? (
        <div
          className={`table-stage__spotlight table-stage__spotlight--${spotlightSeat} ${promptCue?.isUrgent && promptCue.sourceSeat === spotlightSeat ? 'table-stage__spotlight--urgent' : ''}`.trim()}
          style={getSettlementCalloutStyle(spotlightSeat)}
          aria-label="Latest discard spotlight"
        >
          <MahjongTile
            code={spotlightTile}
            variant="discard"
            isLastDiscard
            relatedTileCode={selectedTileCode}
            className="table-stage__spotlight-tile"
          />
        </div>
      ) : null}
      {settlementCenterCalloutLabel ? (
        <div
          className="table-stage__action-callout table-stage__action-callout--center table-stage__action-callout--draw table-stage__action-callout--active"
          aria-hidden="true"
          style={getSettlementCalloutStyle()}
        >
          <span className="table-stage__action-callout-glyph">{settlementCenterCalloutLabel}</span>
        </div>
      ) : null}
      {simultaneousSettlementHuCallouts.map((callout) => (
        <ActionCalloutMarker key={callout.key} callout={callout} phase="active" />
      ))}
      {activeActionCallout && (!shouldShowSimultaneousSettlementHuCallouts || activeActionCallout.tone !== 'hu') ? (
        <ActionCalloutMarker callout={activeActionCallout} phase="active" />
      ) : null}
    </>
  );
});

function ActionCalloutMarker({
  callout,
  phase,
}: {
  callout: ActionCallout;
  phase: 'active' | 'exit';
}) {
  return (
    <div
      className={`table-stage__action-callout table-stage__spotlight--${callout.seat} table-stage__action-callout--${callout.tone} ${callout.huVariant ? `table-stage__action-callout--hu-${callout.huVariant}` : ''} table-stage__action-callout--${phase}`.trim()}
      aria-hidden="true"
      style={getSettlementCalloutStyle(callout.seat)}
    >
      <span className="table-stage__action-callout-glyph">{callout.label}</span>
    </div>
  );
}
