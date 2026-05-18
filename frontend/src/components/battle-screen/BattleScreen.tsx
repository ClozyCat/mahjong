import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';

import type {
  BattleActionId,
  BattleViewModel,
  ClaimActionId,
  QuickChatEmoji,
} from '../../types/match';
import type { ThemeId } from '../../lib/themes';
import {
  getVoiceClipNameForAction,
  getVoiceClipNameForTile,
  playVoiceClip,
  resolveVoiceClipUrl,
  type VoiceCue,
} from '../../lib/voicePacks';
import { BottomActionDock } from './BottomActionDock';
import { ResultOverlay } from './ResultOverlay';
import { SETTLEMENT_CALLOUT_LINGER_MS } from './settlementTiming';
import { TableStage } from './TableStage';
import { SnakeOverlay } from './SnakeOverlay';
import {
  PlayerInviteDialog,
  type InviteDialogUser,
  type SentInviteStatus,
} from './PlayerInviteDialog';

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
  onLeaveTable: () => void;
  onInvitePlayer?: (userId: number) => void;
  onAddBot?: () => void;
  onRemoveBot?: () => void;
  onQuickChat?: (targetSeat: number, emoji: QuickChatEmoji) => void;
  onPointGesture?: (targetSeat: number) => void;
  isBgmEnabled?: boolean;
  onToggleBgm?: () => void;
  isVoiceEnabled?: boolean;
  isBotTakeoverEnabled?: boolean;
  isAutoPassKongEnabled?: boolean;
  canToggleAutoPassKong?: boolean;
  onToggleVoice?: () => void;
  onToggleBotTakeover?: (enabled: boolean) => void;
  onToggleAutoPassKong?: (enabled: boolean) => void;
  inviteHumanUsers?: InviteDialogUser[];
  inviteAiUsers?: InviteDialogUser[];
  currentUserId?: number | null;
  inviteStatusesByUserId?: Record<number, SentInviteStatus>;
  pendingInvitePanel?: ReactNode;
}

const LAST_DISCARD_SPOTLIGHT_LINGER_MS = 1500;
const READY_HAND_CALLOUT_LINGER_MS = 1000;

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
  onLeaveTable,
  onInvitePlayer,
  onAddBot,
  onRemoveBot,
  onQuickChat,
  onPointGesture,
  isBgmEnabled = false,
  onToggleBgm,
  isVoiceEnabled = true,
  isBotTakeoverEnabled = false,
  isAutoPassKongEnabled = false,
  canToggleAutoPassKong = false,
  onToggleVoice,
  onToggleBotTakeover,
  onToggleAutoPassKong,
  currentUserId = null,
  inviteHumanUsers = [],
  inviteAiUsers = [],
  inviteStatusesByUserId = {},
  pendingInvitePanel = null,
}: BattleScreenProps) {
  const [isSettlementPanelReady, setIsSettlementPanelReady] = useState(true);
  const [consumedActionEffect, setConsumedActionEffect] = useState(viewModel.actionEffect);
  const [returnedLastDiscardKey, setReturnedLastDiscardKey] = useState<string | null>(null);
  const [isSnakeActive, setIsSnakeActive] = useState(false);
  const [isInviteDialogOpen, setIsInviteDialogOpen] = useState(false);
  const consumedActionEffectKeyRef = useRef<string | null>(viewModel.actionEffect?.key ?? null);
  const consumedActionEffectRef = useRef(viewModel.actionEffect);
  const playedVoiceCueKeysRef = useRef<Set<string>>(new Set());
  const playedVoiceCueDedupKeysRef = useRef<Set<string>>(new Set());
  const hasObservedNoResultRef = useRef(viewModel.result === null);
  const previousSettlementPageCountRef = useRef(getSettlementPageCount(viewModel.result));
  const lastDiscardReturnTimerRef = useRef<number | null>(null);
  const trackedLastDiscardKeyRef = useRef<string | null>(null);
  const trackedLastDiscardStartedAtRef = useRef<number>(0);
  const trackedLastDiscardActionEffectKeyRef = useRef<string | null>(viewModel.actionEffect?.key ?? null);
  const preMatchActions = viewModel.actions.filter((action) => PRE_MATCH_ACTION_IDS.includes(action.id));
  const canInvitePlayers = Boolean(onInvitePlayer) && hasInviteableSeat(viewModel);
  const visiblePreMatchActions = viewModel.dealerSelection
    ? []
    : [
        {
          id: 'invite' as const,
          label: '邀请',
          enabled: canInvitePlayers,
          emphasis: 'medium' as const,
        },
        ...preMatchActions,
      ];
  const battleActions = viewModel.actions.filter((action) => !TABLE_ONLY_ACTION_IDS.includes(action.id));
  const occupiedSeatCount = viewModel.players.filter((player) => player.seatType !== 'bot').length;
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
  const visibleSettlementWinnerSeats =
    !isSettlementPanelReady && viewModel.result?.winType === 'discard'
      ? getSettlementWinnerSeats(viewModel.result)
      : [];
  const settlementVisibilityKey = getSettlementVisibilityKey(viewModel.result);
  const settlementResetKey = getSettlementResetKey(viewModel);

  function handleAction(actionId: BattleActionId) {
    if (actionId === 'invite') {
      setIsInviteDialogOpen(true);
      return;
    }

    onAction(actionId);
  }

  useEffect(() => {
    consumedActionEffectRef.current = consumedActionEffect;
  }, [consumedActionEffect]);

  useEffect(() => {
    const actionEffects = viewModel.actionEffects?.length ? viewModel.actionEffects : [viewModel.actionEffect];

    for (const actionEffect of actionEffects) {
      const voiceCue = createVoiceCue(viewModel, actionEffect);
      if (
        !voiceCue ||
        playedVoiceCueKeysRef.current.has(voiceCue.key) ||
        (voiceCue.dedupKey && playedVoiceCueDedupKeysRef.current.has(voiceCue.dedupKey))
      ) {
        continue;
      }

      playedVoiceCueKeysRef.current.add(voiceCue.key);
      if (voiceCue.dedupKey) {
        playedVoiceCueDedupKeysRef.current.add(voiceCue.dedupKey);
      }
      if (!isVoiceEnabled) {
        continue;
      }

      const voiceUrl = resolveVoiceClipUrl(viewModel.tableCode, voiceCue.absoluteSeat, voiceCue.clipName);
      if (!voiceUrl) {
        continue;
      }

      playVoiceClip(voiceUrl, voiceCue.absoluteSeat);
    }
  }, [
    viewModel.actionEffect,
    viewModel.actionEffects,
    viewModel.discards,
    viewModel.lastDiscard,
    viewModel.lastDiscardSeat,
    viewModel.players,
    viewModel.tableCode,
    isVoiceEnabled,
  ]);

  useEffect(() => {
    const nextActionEffect = viewModel.actionEffect;
    if (!nextActionEffect?.key) {
      setConsumedActionEffect(null);
      return;
    }

    const currentActionEffect = consumedActionEffectRef.current;
    if (
      currentActionEffect?.calloutTone === 'ready_hand' &&
      currentActionEffect.key.startsWith('optimistic-ready_hand:') &&
      nextActionEffect.calloutTone === 'ready_hand' &&
      nextActionEffect.seat === currentActionEffect.seat
    ) {
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
    const spotlightLingerMs =
      viewModel.actionEffect?.calloutTone === 'ready_hand'
        ? LAST_DISCARD_SPOTLIGHT_LINGER_MS + READY_HAND_CALLOUT_LINGER_MS
        : LAST_DISCARD_SPOTLIGHT_LINGER_MS;
    const remainingMs = Math.max(0, spotlightLingerMs - elapsedMs);

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
    viewModel.actionEffect?.calloutTone,
    viewModel.actionEffect?.key,
    viewModel.shouldAutoReturnLastDiscardToRiver,
  ]);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === 's') {
        event.preventDefault();
        setIsSnakeActive((prev) => !prev);
      }
    }

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
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
          <div className="battle-stage__table-wrap">
            <TableStage
              discards={viewModel.discards}
              selectedTileCode={viewModel.selectedTileCode}
              activeSeat={viewModel.activePlayerSeat}
              actionIndicatorSeat={viewModel.actionIndicatorSeat}
              shouldDebounceWaitingStatus={viewModel.shouldDebounceCenterWaiting ?? false}
              lastDiscard={visibleLastDiscard}
              lastDiscardSeat={visibleLastDiscardSeat}
              settlementWinnerSeat={viewModel.result?.winnerSeat ?? null}
              settlementWinnerSeats={visibleSettlementWinnerSeats}
              settlementWinType={viewModel.result?.winType ?? null}
              settlementWinTypeLabel={viewModel.result?.winTypeLabel ?? null}
              centerStatusText={viewModel.centerStatusText}
              remainingTileCount={viewModel.remainingTileCount}
              promptText={viewModel.promptText}
              promptCue={viewModel.promptCue}
              dealerSelection={viewModel.dealerSelection}
              deadlineAt={viewModel.deadlineAt}
              actionEffect={consumedActionEffect}
              quickChatEvent={viewModel.quickChatEvent}
              systemBroadcastEvent={viewModel.systemBroadcastEvent}
              players={viewModel.players}
              settlementCenterCalloutLabel={visibleSettlementCenterCalloutLabel}
              tableCode={viewModel.tableCode}
              roundLabel={viewModel.roundLabel}
              phaseLabel={viewModel.phaseLabel}
              occupiedSeatCount={occupiedSeatCount}
              seatCapacity={4}
              preMatchActions={viewModel.waitingControls ? visiblePreMatchActions : []}
              isWaitingForMatchStart={Boolean(viewModel.waitingControls)}
              botCount={viewModel.dealerSelection ? 0 : viewModel.waitingControls?.botCount ?? 0}
              canAddBot={!viewModel.dealerSelection && (viewModel.waitingControls?.canAddBot ?? false)}
              canRemoveBot={!viewModel.dealerSelection && (viewModel.waitingControls?.canRemoveBot ?? false)}
              canLeaveTable={viewModel.canLeaveTable}
              themeId={themeId}
              themeLabel={themeLabel}
              onLeaveTable={onLeaveTable}
              onOpenInviteDialog={onInvitePlayer ? () => setIsInviteDialogOpen(true) : undefined}
              onCycleTheme={onCycleTheme}
              onAction={handleAction}
              onAddBot={onAddBot}
              onRemoveBot={onRemoveBot}
              onQuickChat={onQuickChat}
              onPointGesture={onPointGesture}
              isBgmEnabled={isBgmEnabled}
              onToggleBgm={onToggleBgm}
              isVoiceEnabled={isVoiceEnabled}
              onToggleVoice={onToggleVoice}
              isBotTakeoverEnabled={isBotTakeoverEnabled}
              onToggleBotTakeover={onToggleBotTakeover}
              isPlaying={viewModel.mode === 'watching' || viewModel.mode === 'my_turn'}
              extendedWithExtra={viewModel.extendedWithExtra}
            >
              <BottomActionDock
                hand={viewModel.localHand}
                selectedTileCode={viewModel.selectedTileCode}
                handInsight={viewModel.handInsight}
                claimCandidates={viewModel.claimCandidates}
                actions={isBotTakeoverEnabled ? [] : battleActions}
                isElevated={viewModel.isActionDockElevated}
                isWaitingForMatchStart={Boolean(viewModel.waitingControls)}
                isHandInteractionDisabled={isBotTakeoverEnabled}
                isBotTakeoverEnabled={isBotTakeoverEnabled}
                canToggleBotTakeover={isBotTakeoverEnabled && Boolean(onToggleBotTakeover)}
                isAutoPassKongEnabled={isAutoPassKongEnabled}
                canToggleAutoPassKong={canToggleAutoPassKong}
                promptCue={viewModel.promptCue}
                deadlineAt={viewModel.deadlineAt}
                onTileSelect={onTileSelect}
                onTileDoubleClick={onTileDoubleClick}
                onClaimCandidateSelect={onClaimCandidateSelect}
                onClaimCandidateActivate={onClaimCandidateActivate}
                onAction={handleAction}
                onToggleBotTakeover={onToggleBotTakeover}
                onToggleAutoPassKong={onToggleAutoPassKong}
              />
            </TableStage>
          </div>
          <PlayerInviteDialog
            isOpen={isInviteDialogOpen}
            currentUserId={currentUserId}
            humanUsers={inviteHumanUsers}
            aiUsers={inviteAiUsers}
            canInvitePlayers={canInvitePlayers}
            inviteStatusesByUserId={inviteStatusesByUserId}
            onClose={() => setIsInviteDialogOpen(false)}
            onInvite={(userId) => onInvitePlayer?.(userId)}
          />
          {pendingInvitePanel}
          {isSnakeActive && <SnakeOverlay onGameOver={() => setTimeout(() => setIsSnakeActive(false), 2000)} />}
          {visibleResult ? (
            <ResultOverlay
              result={visibleResult}
              settlementKey={settlementResetKey}
              settlementHands={viewModel.settlementHands}
              players={viewModel.players}
              onAction={onAction}
            />
          ) : null}
        </div>
      </div>
    </main>
  );
}

const PRE_MATCH_ACTION_IDS: BattleActionId[] = ['start_match'];
const HIDDEN_TABLE_ACTION_IDS: BattleActionId[] = ['start_next_round'];
const TABLE_ONLY_ACTION_IDS: BattleActionId[] = ['invite', ...PRE_MATCH_ACTION_IDS, ...HIDDEN_TABLE_ACTION_IDS];

function hasInviteableSeat(viewModel: BattleViewModel) {
  const occupiedSeats = viewModel.waitingControls?.occupiedSeats ?? viewModel.players.length;
  if (occupiedSeats < 4) {
    return true;
  }

  return viewModel.players.some((player) => player.seatType === 'bot');
}

function getLastDiscardSpotlightKey(viewModel: BattleViewModel) {
  if (!viewModel.lastDiscard || !viewModel.lastDiscardSeat) {
    return null;
  }

  const discardCount = viewModel.discards[viewModel.lastDiscardSeat]?.length ?? 0;
  return `${viewModel.lastDiscardSeat}:${viewModel.lastDiscard}:${discardCount}`;
}

function createVoiceCue(
  viewModel: BattleViewModel,
  actionEffect: BattleViewModel['actionEffect'],
): VoiceCue | null {
  if (!actionEffect?.key || !actionEffect.seat) {
    return null;
  }

  const absoluteSeat = getAbsoluteSeatForRelativeSeat(viewModel, actionEffect.seat);
  if (typeof absoluteSeat !== 'number') {
    return null;
  }

  const actionClipName = getVoiceClipNameForAction(actionEffect.calloutTone);
  if (actionClipName) {
    return {
      key: `action:${actionEffect.key}:${absoluteSeat}:${actionClipName}`,
      absoluteSeat,
      clipName: actionClipName,
    };
  }

  if (actionEffect.emphasis === 'discard' || actionEffect.calloutTone === 'ready_hand') {
    const voiceTileCode = actionEffect.tileCode ?? viewModel.lastDiscard;
    const tileClipName = getVoiceClipNameForTile(voiceTileCode);
    if (tileClipName) {
      const discardSeat = viewModel.lastDiscardSeat ?? actionEffect.seat;
      const discardCount = discardSeat ? viewModel.discards[discardSeat]?.length ?? 0 : 0;
      const cueKey = actionEffect.tileCode
        ? `discard-event:${actionEffect.key}:${absoluteSeat}:${actionEffect.tileCode}:${tileClipName}`
        : `discard:${absoluteSeat}:${discardSeat ?? 'unknown'}:${discardCount}:${viewModel.lastDiscard}:${tileClipName}`;

      return {
        key: cueKey,
        dedupKey: getDiscardVoiceCueDedupKey(
          viewModel,
          actionEffect,
          absoluteSeat,
          discardSeat,
          discardCount,
          voiceTileCode,
        ),
        absoluteSeat,
        clipName: tileClipName,
      };
    }
  }

  return null;
}

function getDiscardVoiceCueDedupKey(
  viewModel: BattleViewModel,
  actionEffect: NonNullable<BattleViewModel['actionEffect']>,
  absoluteSeat: number,
  discardSeat: string | null,
  discardCount: number,
  tileCode: string | null | undefined,
) {
  const tileIdentity = getTileIdentityFromActionEffectKey(actionEffect.key);
  if (tileIdentity) {
    return `discard:${absoluteSeat}:tile:${tileIdentity}`;
  }

  // Remove discardCount from dedupKey to prevent duplicate voice for same tile
  return `discard:${absoluteSeat}:river:${discardSeat ?? 'unknown'}:${tileCode ?? viewModel.lastDiscard ?? 'unknown'}`;
}

function getTileIdentityFromActionEffectKey(actionEffectKey: string) {
  const directTileId = actionEffectKey.match(/(?:^|:)([a-z]+\d?#[^:]+)$/i)?.[1];
  if (directTileId) {
    return directTileId;
  }

  const eventTileId = actionEffectKey.match(/"tile_id":"([^"]+)"/)?.[1];
  return eventTileId ?? null;
}

function getAbsoluteSeatForRelativeSeat(viewModel: BattleViewModel, seat: NonNullable<BattleViewModel['actionEffect']>['seat']) {
  if (!seat) {
    return null;
  }

  return viewModel.players.find((player) => player.seat === seat)?.absoluteSeat ?? null;
}

function getSettlementPanelDelayMs(
  winType: string | null,
  hasObservedNoResult: boolean,
  settlementPageCount: number,
  hasNewSettlementPages: boolean,
) {
  if (winType === 'draw' || winType === 'discard' || winType === 'self_draw') {
    if (winType === 'discard' && hasNewSettlementPages && settlementPageCount > 1) {
      return SETTLEMENT_CALLOUT_LINGER_MS;
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

function getSettlementWinnerSeats(result: BattleViewModel['result']) {
  if (!result) {
    return [];
  }

  if (Array.isArray(result.pages) && result.pages.length > 0) {
    return Array.from(
      new Set(
        result.pages
          .map((page) => page.winnerSeat)
          .filter((seat): seat is NonNullable<typeof result.winnerSeat> => seat !== null),
      ),
    );
  }

  return result.winnerSeat ? [result.winnerSeat] : [];
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

function getSettlementResetKey(viewModel: BattleViewModel) {
  const result = viewModel.result;
  if (!result) {
    return 'no-result';
  }

  return JSON.stringify({
    tableCode: viewModel.tableCode,
    roundId: result.roundId ?? null,
    roundLabel: result.roundId ? null : viewModel.roundLabel,
    title: result.title,
    winType: result.winType,
    winnerSeat: result.winnerSeat,
    discarderSeat: result.discarderSeat,
    pages: result.pages?.map((page) => ({
      winType: page.winType,
      winnerSeat: page.winnerSeat,
      discarderSeat: page.discarderSeat,
    })) ?? null,
  });
}
