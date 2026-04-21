import { useEffect, useLayoutEffect, useRef, useState } from 'react';

import type { BattleActionId, BattleViewModel, ClaimActionId, QuickChatEmoji } from '../../types/match';
import type { ThemeId } from '../../lib/themes';
import { BottomActionDock } from './BottomActionDock';
import { ResultOverlay } from './ResultOverlay';
import { SETTLEMENT_CALLOUT_LINGER_MS } from './settlementTiming';
import { TableStage } from './TableStage';

interface BattleScreenProps {
  viewModel: BattleViewModel;
  themeId: ThemeId;
  themeLabel: string;
  onCycleTheme: () => void;
  onTileSelect: (tileId: string) => void;
  onTileDoubleClick: (tileId: string) => void;
  onClaimCandidateSelect: (actionId: ClaimActionId, tileIds: string[]) => void;
  onClaimCandidateActivate: (actionId: ClaimActionId, tileIds: string[]) => void;
  onAction: (actionId: BattleActionId) => void;
  onCopyTableCode: () => void;
  onLeaveTable: () => void;
  onAddBot?: () => void;
  onRemoveBot?: () => void;
  onQuickChat?: (targetSeat: number, emoji: QuickChatEmoji) => void;
}

const DEFAULT_TABLE_TILE_SCALE = 1.00;
const TABLE_TILE_SCALE_STEP = 0.06;
const MIN_TABLE_TILE_SCALE = 0.88;
const MAX_TABLE_TILE_SCALE = 1.3;
const LAST_DISCARD_SPOTLIGHT_LINGER_MS = 1500;
const READY_ACTION_COOLDOWN_MS = 3000;

export function BattleScreen({
  viewModel,
  themeId,
  themeLabel,
  onCycleTheme,
  onTileSelect,
  onTileDoubleClick,
  onClaimCandidateSelect,
  onClaimCandidateActivate,
  onAction,
  onCopyTableCode,
  onLeaveTable,
  onAddBot,
  onRemoveBot,
  onQuickChat,
}: BattleScreenProps) {
  const [tableTileScale, setTableTileScale] = useState(DEFAULT_TABLE_TILE_SCALE);
  const [isSettlementPanelReady, setIsSettlementPanelReady] = useState(true);
  const [consumedActionEffect, setConsumedActionEffect] = useState(viewModel.actionEffect);
  const [returnedLastDiscardKey, setReturnedLastDiscardKey] = useState<string | null>(null);
  const [isReadyActionCoolingDown, setIsReadyActionCoolingDown] = useState(false);
  const consumedActionEffectKeyRef = useRef<string | null>(viewModel.actionEffect?.key ?? null);
  const hasObservedNoResultRef = useRef(viewModel.result === null);
  const previousSettlementPageCountRef = useRef(getSettlementPageCount(viewModel.result));
  const lastDiscardReturnTimerRef = useRef<number | null>(null);
  const isReadyActionCoolingDownRef = useRef(false);
  const readyActionCooldownTimerRef = useRef<number | null>(null);
  const trackedLastDiscardKeyRef = useRef<string | null>(null);
  const trackedLastDiscardStartedAtRef = useRef<number>(0);
  const trackedLastDiscardActionEffectKeyRef = useRef<string | null>(viewModel.actionEffect?.key ?? null);
  const preMatchActions = viewModel.actions
    .filter((action) => PRE_MATCH_ACTION_IDS.includes(action.id))
    .map((action) =>
      action.id === 'ready'
        ? {
          ...action,
          enabled: action.enabled && !isReadyActionCoolingDown,
        }
        : action,
    );
  const battleActions = viewModel.actions.filter((action) => !TABLE_ONLY_ACTION_IDS.includes(action.id));
  const occupiedSeatCount = viewModel.waitingControls?.occupiedSeats ?? viewModel.players.length;
  const canDecreaseTableTileScale = tableTileScale > MIN_TABLE_TILE_SCALE;
  const canIncreaseTableTileScale = tableTileScale < MAX_TABLE_TILE_SCALE;
  const shouldReturnLastDiscardToRiver =
    Boolean(viewModel.result) &&
    Boolean(viewModel.lastDiscard) &&
    (viewModel.result?.winType === 'draw' || viewModel.result?.winType === 'self_draw');
  const lastDiscardSpotlightKey = getLastDiscardSpotlightKey(viewModel);
  const shouldHideLastDiscardSpotlight =
    shouldReturnLastDiscardToRiver ||
    (lastDiscardSpotlightKey !== null && returnedLastDiscardKey === lastDiscardSpotlightKey);
  const visibleLastDiscard = shouldHideLastDiscardSpotlight ? null : viewModel.lastDiscard;
  const visibleLastDiscardSeat = shouldHideLastDiscardSpotlight ? null : viewModel.lastDiscardSeat;
  const visibleResult = isSettlementPanelReady ? viewModel.result : null;
  const visibleSettlementCenterCalloutLabel =
    !isSettlementPanelReady && viewModel.result?.winType === 'draw' ? '流局' : null;
  const settlementVisibilityKey = getSettlementVisibilityKey(viewModel.result);

  function adjustTableTileScale(offset: number) {
    setTableTileScale((currentScale) => {
      const nextScale = Number((currentScale + offset).toFixed(2));

      return Math.min(MAX_TABLE_TILE_SCALE, Math.max(MIN_TABLE_TILE_SCALE, nextScale));
    });
  }

  function handleAction(actionId: BattleActionId) {
    if (actionId === 'ready') {
      if (isReadyActionCoolingDownRef.current) {
        return;
      }

      isReadyActionCoolingDownRef.current = true;
      setIsReadyActionCoolingDown(true);
      if (readyActionCooldownTimerRef.current !== null) {
        window.clearTimeout(readyActionCooldownTimerRef.current);
      }
      readyActionCooldownTimerRef.current = window.setTimeout(() => {
        isReadyActionCoolingDownRef.current = false;
        setIsReadyActionCoolingDown(false);
        readyActionCooldownTimerRef.current = null;
      }, READY_ACTION_COOLDOWN_MS);
    }

    onAction(actionId);
  }

  useEffect(() => {
    const nextActionEffect = viewModel.actionEffect;
    if (!nextActionEffect?.key) {
      setConsumedActionEffect(null);
      return;
    }

    if (consumedActionEffectKeyRef.current === nextActionEffect.key) {
      return;
    }

    consumedActionEffectKeyRef.current = nextActionEffect.key;
    setConsumedActionEffect(nextActionEffect);
  }, [viewModel.actionEffect]);

  useEffect(() => {
    if (lastDiscardSpotlightKey === trackedLastDiscardKeyRef.current) {
      return;
    }

    trackedLastDiscardKeyRef.current = lastDiscardSpotlightKey;
    trackedLastDiscardStartedAtRef.current = lastDiscardSpotlightKey ? Date.now() : 0;
    trackedLastDiscardActionEffectKeyRef.current = viewModel.actionEffect?.key ?? null;
    setReturnedLastDiscardKey(null);
  }, [lastDiscardSpotlightKey, viewModel.actionEffect?.key]);

  useEffect(() => {
    if (lastDiscardReturnTimerRef.current !== null) {
      window.clearTimeout(lastDiscardReturnTimerRef.current);
      lastDiscardReturnTimerRef.current = null;
    }

    if (!lastDiscardSpotlightKey) {
      setReturnedLastDiscardKey(null);
      return undefined;
    }

    if (shouldReturnLastDiscardToRiver) {
      setReturnedLastDiscardKey(lastDiscardSpotlightKey);
      return undefined;
    }

    if (!viewModel.shouldAutoReturnLastDiscardToRiver) {
      return undefined;
    }

    if (shouldReturnDiscardImmediatelyForNextAction(viewModel.actionEffect, trackedLastDiscardActionEffectKeyRef.current)) {
      setReturnedLastDiscardKey(lastDiscardSpotlightKey);
      return undefined;
    }

    const elapsedMs = Math.max(0, Date.now() - trackedLastDiscardStartedAtRef.current);
    const remainingMs = Math.max(0, LAST_DISCARD_SPOTLIGHT_LINGER_MS - elapsedMs);

    if (remainingMs === 0) {
      setReturnedLastDiscardKey(lastDiscardSpotlightKey);
      return undefined;
    }

    lastDiscardReturnTimerRef.current = window.setTimeout(() => {
      setReturnedLastDiscardKey((currentKey) =>
        currentKey === null || currentKey === lastDiscardSpotlightKey ? lastDiscardSpotlightKey : currentKey,
      );
      lastDiscardReturnTimerRef.current = null;
    }, remainingMs);

    return () => {
      if (lastDiscardReturnTimerRef.current !== null) {
        window.clearTimeout(lastDiscardReturnTimerRef.current);
        lastDiscardReturnTimerRef.current = null;
      }
    };
  }, [
    lastDiscardSpotlightKey,
    shouldReturnLastDiscardToRiver,
    viewModel.actionEffect?.key,
    viewModel.shouldAutoReturnLastDiscardToRiver,
  ]);

  useEffect(() => {
    return () => {
      if (readyActionCooldownTimerRef.current !== null) {
        window.clearTimeout(readyActionCooldownTimerRef.current);
        readyActionCooldownTimerRef.current = null;
      }
      isReadyActionCoolingDownRef.current = false;
    };
  }, []);

  useLayoutEffect(() => {
    if (!viewModel.result) {
      hasObservedNoResultRef.current = true;
      previousSettlementPageCountRef.current = 0;
      setIsSettlementPanelReady(true);
      return undefined;
    }

    const settlementPageCount = getSettlementPageCount(viewModel.result);
    const settlementPanelDelayMs = getSettlementPanelDelayMs(
      viewModel.result.winType,
      hasObservedNoResultRef.current,
      settlementPageCount,
      settlementPageCount > previousSettlementPageCountRef.current,
    );
    hasObservedNoResultRef.current = false;
    previousSettlementPageCountRef.current = settlementPageCount;

    if (settlementPanelDelayMs <= 0) {
      setIsSettlementPanelReady(true);
      return undefined;
    }

    setIsSettlementPanelReady(false);
    const timer = window.setTimeout(() => {
      setIsSettlementPanelReady(true);
    }, settlementPanelDelayMs);

    return () => window.clearTimeout(timer);
  }, [settlementVisibilityKey, viewModel.result?.winType]);

  return (
    <main className="battle-screen">
      <div className="battle-shell">
        <div className="battle-stage">
          <div className="battle-stage__halo" />
          <div className="battle-stage__table-wrap">
            <TableStage
              discards={viewModel.discards}
              selectedTileCode={viewModel.selectedTileCode}
              activeSeat={viewModel.activePlayerSeat}
              actionIndicatorSeat={viewModel.actionIndicatorSeat}
              lastDiscard={visibleLastDiscard}
              lastDiscardSeat={visibleLastDiscardSeat}
              settlementWinnerSeat={viewModel.result?.winnerSeat ?? null}
              settlementWinType={viewModel.result?.winType ?? null}
              settlementWinTypeLabel={viewModel.result?.winTypeLabel ?? null}
              centerStatusText={viewModel.centerStatusText}
              remainingTileCount={viewModel.remainingTileCount}
              promptText={viewModel.promptText}
              promptCue={viewModel.promptCue}
              deadlineAt={viewModel.deadlineAt}
              actionEffect={consumedActionEffect}
              quickChatEvent={viewModel.quickChatEvent}
              players={viewModel.players}
              settlementHands={null}
              settlementCenterCalloutLabel={visibleSettlementCenterCalloutLabel}
              tableCode={viewModel.tableCode}
              roundLabel={viewModel.roundLabel}
              phaseLabel={viewModel.phaseLabel}
              occupiedSeatCount={occupiedSeatCount}
              seatCapacity={4}
              preMatchActions={viewModel.waitingControls ? preMatchActions : []}
              botCount={viewModel.waitingControls?.botCount ?? 0}
              canAddBot={viewModel.waitingControls?.canAddBot ?? false}
              canRemoveBot={viewModel.waitingControls?.canRemoveBot ?? false}
              tileScale={tableTileScale}
              canDecreaseTileScale={canDecreaseTableTileScale}
              canIncreaseTileScale={canIncreaseTableTileScale}
              canLeaveTable={viewModel.canLeaveTable}
              themeId={themeId}
              themeLabel={themeLabel}
              onLeaveTable={onLeaveTable}
              onCycleTheme={onCycleTheme}
              onAction={handleAction}
              onAddBot={onAddBot}
              onRemoveBot={onRemoveBot}
              onQuickChat={onQuickChat}
              onDecreaseTileScale={() => adjustTableTileScale(-TABLE_TILE_SCALE_STEP)}
              onIncreaseTileScale={() => adjustTableTileScale(TABLE_TILE_SCALE_STEP)}
            />
          </div>
          <BottomActionDock
            hand={viewModel.localHand}
            selectedTileCode={viewModel.selectedTileCode}
            readyHandInsight={viewModel.readyHandInsight}
            claimCandidates={viewModel.claimCandidates}
            actions={battleActions}
            isElevated={viewModel.isActionDockElevated}
            isWaitingForMatchStart={Boolean(viewModel.waitingControls)}
            promptCue={viewModel.promptCue}
            deadlineAt={viewModel.deadlineAt}
            onTileSelect={onTileSelect}
            onTileDoubleClick={onTileDoubleClick}
            onClaimCandidateSelect={onClaimCandidateSelect}
            onClaimCandidateActivate={onClaimCandidateActivate}
            onAction={handleAction}
          />
          {visibleResult ? (
            <ResultOverlay
              result={visibleResult}
              settlementHands={viewModel.settlementHands}
              onAction={onAction}
            />
          ) : null}
        </div>
      </div>
    </main>
  );
}

const PRE_MATCH_ACTION_IDS: BattleActionId[] = ['ready', 'start_match'];
const HIDDEN_TABLE_ACTION_IDS: BattleActionId[] = ['start_next_round', 'restart_match'];
const TABLE_ONLY_ACTION_IDS: BattleActionId[] = [...PRE_MATCH_ACTION_IDS, ...HIDDEN_TABLE_ACTION_IDS];

function getLastDiscardSpotlightKey(viewModel: BattleViewModel) {
  if (!viewModel.lastDiscard || !viewModel.lastDiscardSeat) {
    return null;
  }

  const discardCount = viewModel.discards[viewModel.lastDiscardSeat]?.length ?? 0;
  return `${viewModel.lastDiscardSeat}:${viewModel.lastDiscard}:${discardCount}`;
}

function getSettlementPanelDelayMs(
  winType: string | null,
  hasObservedNoResult: boolean,
  settlementPageCount: number,
  hasNewSettlementPages: boolean,
) {
  if (winType === 'draw' || winType === 'discard' || winType === 'self_draw') {
    if (winType === 'discard' && hasNewSettlementPages && settlementPageCount > 1) {
      return settlementPageCount * SETTLEMENT_CALLOUT_LINGER_MS;
    }

    return hasObservedNoResult ? SETTLEMENT_CALLOUT_LINGER_MS : 0;
  }

  return 0;
}

function getSettlementPageCount(result: BattleViewModel['result']) {
  if (!result) {
    return 0;
  }

  return Array.isArray(result.pages) && result.pages.length > 0 ? result.pages.length : 1;
}

function shouldReturnDiscardImmediatelyForNextAction(
  actionEffect: BattleViewModel['actionEffect'],
  trackedDiscardActionEffectKey: string | null,
) {
  if (!actionEffect?.key || actionEffect.key === trackedDiscardActionEffectKey) {
    return false;
  }

  return actionEffect.emphasis === 'draw' || actionEffect.emphasis === 'kong' || actionEffect.emphasis === 'system';
}

function getSettlementVisibilityKey(result: BattleViewModel['result']) {
  if (!result) {
    return null;
  }

  return JSON.stringify({
    title: result.title,
    summary: result.summary,
    fanTotal: result.fanTotal,
    winnerSeat: result.winnerSeat,
    discarderSeat: result.discarderSeat,
    winType: result.winType,
    winTypeLabel: result.winTypeLabel,
    provisional: result.provisional,
    flowerCount: result.flowerCount,
    fanBreakdown: result.fanBreakdown,
    pages: result.pages,
    seats: result.seats,
  });
}
