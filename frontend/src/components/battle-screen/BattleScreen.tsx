import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react';

import type {
  BattleActionId,
  BattleViewModel,
  ClaimActionId,
  PublicUser,
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
  TableSidebar,
  type TableSidebarPlayer,
  type TableSidebarTab,
} from '../table-sidebar/TableSidebar';

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
  isBgmEnabled?: boolean;
  onToggleBgm?: () => void;
  isVoiceEnabled?: boolean;
  isBotTakeoverEnabled?: boolean;
  onToggleVoice?: () => void;
  onToggleBotTakeover?: (enabled: boolean) => void;
  sidebarPlayers?: TableSidebarPlayer[];
  sidebarOnlineUsers?: PublicUser[];
  sidebarOnlineUserIds?: number[];
  sidebarCreatingTableCodes?: string[];
  sidebarCurrentUserId?: number | null;
  sidebarRoomPanel?: ReactNode;
  sidebarMessagesPanel?: ReactNode;
  sidebarDefaultOpen?: boolean;
  sidebarInitialTab?: TableSidebarTab;
  sidebarTabAlerts?: Partial<Record<TableSidebarTab, boolean>>;
}

const DEFAULT_TABLE_TILE_SCALE = 1.12;
const TABLE_TILE_SCALE_STEP = 0.06;
const MIN_TABLE_TILE_SCALE = 0.88;
const MAX_TABLE_TILE_SCALE = 1.3;
const LAST_DISCARD_SPOTLIGHT_LINGER_MS = 1500;
const READY_HAND_CALLOUT_LINGER_MS = 1000;
const READY_ACTION_COOLDOWN_MS = 3000;
const VOICE_CUE_DEDUP_MS = 1200;

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
  isBgmEnabled = false,
  onToggleBgm,
  isVoiceEnabled = true,
  isBotTakeoverEnabled = false,
  onToggleVoice,
  onToggleBotTakeover,
  sidebarPlayers = [],
  sidebarOnlineUsers = [],
  sidebarOnlineUserIds = [],
  sidebarCreatingTableCodes = [],
  sidebarCurrentUserId = null,
  sidebarRoomPanel = null,
  sidebarMessagesPanel = null,
  sidebarDefaultOpen = false,
  sidebarInitialTab = 'online',
  sidebarTabAlerts = {},
}: BattleScreenProps) {
  const [tableTileScale, setTableTileScale] = useState(DEFAULT_TABLE_TILE_SCALE);
  const [isSettlementPanelReady, setIsSettlementPanelReady] = useState(true);
  const [consumedActionEffect, setConsumedActionEffect] = useState(viewModel.actionEffect);
  const [returnedLastDiscardKey, setReturnedLastDiscardKey] = useState<string | null>(null);
  const [isReadyActionCoolingDown, setIsReadyActionCoolingDown] = useState(false);
  const [isSnakeActive, setIsSnakeActive] = useState(false);
  const [isSidebarOpen, setIsSidebarOpen] = useState(sidebarDefaultOpen);
  const [sidebarTab, setSidebarTab] = useState<TableSidebarTab>(sidebarInitialTab);
  const consumedActionEffectKeyRef = useRef<string | null>(viewModel.actionEffect?.key ?? null);
  const consumedActionEffectRef = useRef(viewModel.actionEffect);
  const playedVoiceCueKeysRef = useRef<Set<string>>(new Set());
  const recentVoiceCueSignaturesRef = useRef<Map<string, number>>(new Map());
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
  const visiblePreMatchActions = viewModel.dealerSelection ? [] : preMatchActions;
  const battleActions = viewModel.actions.filter((action) => !TABLE_ONLY_ACTION_IDS.includes(action.id));
  const occupiedSeatCount = viewModel.players.filter((player) => player.seatType !== 'bot').length;
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
  const visibleSettlementWinnerSeats =
    !isSettlementPanelReady && viewModel.result?.winType === 'discard'
      ? getSettlementWinnerSeats(viewModel.result)
      : [];
  const settlementVisibilityKey = getSettlementVisibilityKey(viewModel.result);
  const settlementResetKey = getSettlementResetKey(viewModel);

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

  function handleSidebarTabChange(tab: TableSidebarTab) {
    setSidebarTab(tab);
  }

  const battleStageStyle = {
    '--table-stage-tile-scale': `${tableTileScale}`,
  } as CSSProperties;

  useEffect(() => {
    consumedActionEffectRef.current = consumedActionEffect;
  }, [consumedActionEffect]);

  useEffect(() => {
    const actionEffects = viewModel.actionEffects?.length ? viewModel.actionEffects : [viewModel.actionEffect];

    for (const actionEffect of actionEffects) {
      const voiceCue = createVoiceCue(viewModel, actionEffect);
      if (!voiceCue || playedVoiceCueKeysRef.current.has(voiceCue.key)) {
        continue;
      }

      playedVoiceCueKeysRef.current.add(voiceCue.key);
      if (!isVoiceEnabled) {
        continue;
      }

      const voiceUrl = resolveVoiceClipUrl(viewModel.tableCode, voiceCue.absoluteSeat, voiceCue.clipName);
      if (!voiceUrl) {
        continue;
      }

      const now = Date.now();
      const voiceCueSignature = getVoiceCueSignature(voiceCue);
      const previousPlayedAt = recentVoiceCueSignaturesRef.current.get(voiceCueSignature);

      pruneRecentVoiceCues(recentVoiceCueSignaturesRef.current, now);

      if (typeof previousPlayedAt === 'number' && now - previousPlayedAt < VOICE_CUE_DEDUP_MS) {
        continue;
      }

      recentVoiceCueSignaturesRef.current.set(voiceCueSignature, now);
      playVoiceClip(voiceUrl);
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
    return () => {
      if (readyActionCooldownTimerRef.current !== null) {
        window.clearTimeout(readyActionCooldownTimerRef.current);
        readyActionCooldownTimerRef.current = null;
      }
      isReadyActionCoolingDownRef.current = false;
    };
  }, []);

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
        <div className="battle-stage" style={battleStageStyle}>
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
              isBgmEnabled={isBgmEnabled}
              onToggleBgm={onToggleBgm}
              isVoiceEnabled={isVoiceEnabled}
              onToggleVoice={onToggleVoice}
              isBotTakeoverEnabled={isBotTakeoverEnabled}
              onToggleBotTakeover={onToggleBotTakeover}
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
                promptCue={viewModel.promptCue}
                deadlineAt={viewModel.deadlineAt}
                onTileSelect={onTileSelect}
                onTileDoubleClick={onTileDoubleClick}
                onClaimCandidateSelect={onClaimCandidateSelect}
                onClaimCandidateActivate={onClaimCandidateActivate}
                onAction={handleAction}
              />
            </TableStage>
          </div>
          <TableSidebar
            isOpen={isSidebarOpen}
            activeTab={sidebarTab}
            tablePlayers={sidebarPlayers}
            onlineUsers={sidebarOnlineUsers}
            onlineUserIds={sidebarOnlineUserIds}
            creatingTableCodes={sidebarCreatingTableCodes}
            currentUserId={sidebarCurrentUserId}
            tabAlerts={sidebarTabAlerts}
            roomPanel={sidebarRoomPanel}
            messagesPanel={sidebarMessagesPanel}
            onToggle={() => setIsSidebarOpen((current) => !current)}
            onTabChange={handleSidebarTabChange}
          />
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
        absoluteSeat,
        clipName: tileClipName,
      };
    }
  }

  return null;
}

function getAbsoluteSeatForRelativeSeat(viewModel: BattleViewModel, seat: NonNullable<BattleViewModel['actionEffect']>['seat']) {
  if (!seat) {
    return null;
  }

  return viewModel.players.find((player) => player.seat === seat)?.absoluteSeat ?? null;
}

function getVoiceCueSignature(voiceCue: VoiceCue) {
  return `${voiceCue.absoluteSeat}:${voiceCue.clipName}`;
}

function pruneRecentVoiceCues(recentVoiceCues: Map<string, number>, now: number) {
  for (const [signature, playedAt] of recentVoiceCues) {
    if (now - playedAt >= VOICE_CUE_DEDUP_MS) {
      recentVoiceCues.delete(signature);
    }
  }
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
