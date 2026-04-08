import { useEffect, useLayoutEffect, useRef, useState } from 'react';

import type { BattleActionId, BattleViewModel, ClaimActionId, QuickChatEmoji } from '../../types/match';
import type { ThemeId } from '../../lib/themes';
import { AmbientOverlay } from './AmbientOverlay';
import { BottomActionDock } from './BottomActionDock';
import { ResultOverlay } from './ResultOverlay';
import { SETTLEMENT_CALLOUT_LINGER_MS } from './settlementTiming';
import { SkillActivationDialog } from './SkillActivationDialog';
import { SkillSelectionOverlay } from './SkillSelectionOverlay';
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
  onSkillSelect?: (skillId: string) => void;
  onSkillDecline?: () => void;
  onCloseSkillActivation?: () => void;
  onConfirmSkillActivation?: () => void;
  onSkillActivationTargetSelect?: (seatIndex: number) => void;
  onSkillActivationTileSelect?: (tileId: string) => void;
  onSkillActivationMeldSelect?: (meldIndex: number) => void;
  onCopyTableCode: () => void;
  onLeaveTable: () => void;
  onAddBot?: () => void;
  onRemoveBot?: () => void;
  onQuickChat?: (targetSeat: number, emoji: QuickChatEmoji) => void;
}

const DEFAULT_TABLE_TILE_SCALE = 1.12;
const TABLE_TILE_SCALE_STEP = 0.06;
const MIN_TABLE_TILE_SCALE = 0.88;
const MAX_TABLE_TILE_SCALE = 1.3;
const LAST_DISCARD_SPOTLIGHT_LINGER_MS = 1500;
const READY_ACTION_COOLDOWN_MS = 3000;
const MIN_BATTLE_VIEWPORT_WIDTH = 1280;
const MIN_BATTLE_VIEWPORT_HEIGHT = 720;
const MIN_BATTLE_VIEWPORT_RATIO = 16 / 9;

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
  onSkillSelect,
  onSkillDecline,
  onCloseSkillActivation,
  onConfirmSkillActivation,
  onSkillActivationTargetSelect,
  onSkillActivationTileSelect,
  onSkillActivationMeldSelect,
  onCopyTableCode,
  onLeaveTable,
  onAddBot,
  onRemoveBot,
  onQuickChat,
}: BattleScreenProps) {
  const [tableTileScale, setTableTileScale] = useState(DEFAULT_TABLE_TILE_SCALE);
  const [viewportState, setViewportState] = useState(getBattleViewportState);
  const [isSettlementPanelReady, setIsSettlementPanelReady] = useState(true);
  const [consumedActionEffect, setConsumedActionEffect] = useState(viewModel.actionEffect);
  const [returnedLastDiscardKey, setReturnedLastDiscardKey] = useState<string | null>(null);
  const [isReadyActionCoolingDown, setIsReadyActionCoolingDown] = useState(false);
  const consumedActionEffectKeyRef = useRef<string | null>(viewModel.actionEffect?.key ?? null);
  const hasObservedNoResultRef = useRef(viewModel.result === null);
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
    if (typeof window === 'undefined') {
      return undefined;
    }

    function handleResize() {
      setViewportState(getBattleViewportState());
    }

    window.addEventListener('resize', handleResize);

    return () => window.removeEventListener('resize', handleResize);
  }, []);

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
      setIsSettlementPanelReady(true);
      return undefined;
    }

    const settlementPanelDelayMs = getSettlementPanelDelayMs(
      viewModel.result.winType,
      hasObservedNoResultRef.current,
    );
    hasObservedNoResultRef.current = false;

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
    <main className={`battle-screen ${viewportState.isSupported ? '' : 'battle-screen--viewport-blocked'}`}>
      <div className="battle-shell">
        <div className="battle-stage">
          <div className="battle-stage__halo" />
          <div className="battle-stage__table-wrap">
            <TableStage
              discards={viewModel.discards}
              activeSeat={viewModel.activePlayerSeat}
              actionIndicatorSeat={viewModel.actionIndicatorSeat}
              lastDiscard={visibleLastDiscard}
              lastDiscardSeat={visibleLastDiscardSeat}
              settlementWinnerSeat={viewModel.result?.winnerSeat ?? null}
              settlementWinType={viewModel.result?.winType ?? null}
              settlementWinTypeLabel={viewModel.result?.winTypeLabel ?? null}
              remainingTileCount={viewModel.remainingTileCount}
              promptText={viewModel.promptText}
              promptCue={viewModel.promptCue}
              actionEffect={consumedActionEffect}
              quickChatEvent={viewModel.quickChatEvent}
              players={viewModel.players}
              settlementHands={isSettlementPanelReady ? viewModel.settlementHands : null}
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
          <AmbientOverlay
            mode={viewModel.mode}
            promptText={viewModel.promptText}
            waitingControls={viewModel.waitingControls}
            canLeaveTable={viewModel.canLeaveTable}
            onAddBot={onAddBot}
            onRemoveBot={onRemoveBot}
            onLeaveTable={onLeaveTable}
          />
          {visibleResult ? <ResultOverlay result={visibleResult} onAction={onAction} /> : null}
          {viewModel.skillSelection && onSkillSelect && onSkillDecline ? (
            <SkillSelectionOverlay selection={viewModel.skillSelection} onSelect={onSkillSelect} onDecline={onSkillDecline} />
          ) : null}
          {viewModel.skillActivation &&
          onCloseSkillActivation &&
          onConfirmSkillActivation &&
          onSkillActivationTargetSelect &&
          onSkillActivationTileSelect &&
          onSkillActivationMeldSelect ? (
            <SkillActivationDialog
              activation={viewModel.skillActivation}
              onClose={onCloseSkillActivation}
              onConfirm={onConfirmSkillActivation}
              onTargetSelect={onSkillActivationTargetSelect}
              onTileSelect={onSkillActivationTileSelect}
              onMeldSelect={onSkillActivationMeldSelect}
            />
          ) : null}
        </div>
      </div>
      <BottomActionDock
        hand={viewModel.localHand}
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
      {viewportState.isSupported ? null : (
        <div className="battle-screen__viewport-guard" role="alert" aria-live="assertive">
          <div className="battle-screen__viewport-guard-card">
            <span className="battle-screen__viewport-guard-eyebrow">显示条件不足</span>
            <strong>请把浏览器窗口调整到大于 1280 x 720，且宽高比大于 16:9</strong>
            <p>
              当前可用区域为 {viewportState.width} x {viewportState.height}，宽高比 {viewportState.ratioLabel}。
            </p>
          </div>
        </div>
      )}
    </main>
  );
}

const PRE_MATCH_ACTION_IDS: BattleActionId[] = ['ready', 'start_match'];
const HIDDEN_TABLE_ACTION_IDS: BattleActionId[] = ['start_next_round', 'restart_match'];
const TABLE_ONLY_ACTION_IDS: BattleActionId[] = [...PRE_MATCH_ACTION_IDS, ...HIDDEN_TABLE_ACTION_IDS];

function getBattleViewportState() {
  if (typeof window === 'undefined') {
    return {
      width: 1920,
      height: 1080,
      ratioLabel: '1.78',
      isSupported: true,
    };
  }

  const width = window.innerWidth;
  const height = window.innerHeight;
  const ratio = height > 0 ? width / height : 0;

  return {
    width,
    height,
    ratioLabel: ratio.toFixed(2),
    isSupported:
      width > MIN_BATTLE_VIEWPORT_WIDTH &&
      height > MIN_BATTLE_VIEWPORT_HEIGHT &&
      ratio > MIN_BATTLE_VIEWPORT_RATIO,
  };
}

function getLastDiscardSpotlightKey(viewModel: BattleViewModel) {
  if (!viewModel.lastDiscard || !viewModel.lastDiscardSeat) {
    return null;
  }

  const discardCount = viewModel.discards[viewModel.lastDiscardSeat]?.length ?? 0;
  return `${viewModel.lastDiscardSeat}:${viewModel.lastDiscard}:${discardCount}`;
}

function getSettlementPanelDelayMs(winType: string | null, hasObservedNoResult: boolean) {
  if (winType === 'draw' || winType === 'discard' || winType === 'self_draw') {
    return hasObservedNoResult ? SETTLEMENT_CALLOUT_LINGER_MS : 0;
  }

  return 0;
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
    seats: result.seats,
  });
}
