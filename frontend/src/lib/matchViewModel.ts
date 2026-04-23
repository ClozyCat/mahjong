import type {
  ActionEffectView,
  BackendActionType,
  BattleActionId,
  BattlePromptView,
  BattleActionView,
  BattleViewModel,
  ClaimActionId,
  ClaimCandidateTileView,
  ClaimCandidateView,
  DisplayMeldView,
  MatchResultPayload,
  PlayerView,
  PrivatePlayerState,
  PrivateState,
  QuickChatEventView,
  ResultPageView,
  ResultSeatView,
  Seat,
  SessionState,
  WaitingControls,
} from '../types/match';
import {
  getActionCandidateGroups,
  getFlowerCandidateTileIds,
  getOptimisticFlowerTileId,
  getLocalTurnKongCandidateGroups,
  isFlowerTileKey,
} from './kongSelection';
import { getReadyHandWaits } from './readyHand';

const RELATIVE_SEATS: Seat[] = ['bottom', 'right', 'top', 'left'];
const WINDS: PlayerView['wind'][] = ['East', 'South', 'West', 'North'];
const ACTION_ORDER: BattleActionId[] = [
  'ready',
  'start_match',
  'start_next_round',
  'restart_match',
  'discard',
  'ready_hand',
  'flower',
  'kong',
  'hu',
  'chow',
  'pung',
  'pass',
];

const PROMPT_ACTION_PRIORITY: Record<BackendActionType, number> = {
  hu: 0,
  kong: 1,
  pung: 2,
  chow: 3,
  flower: 4,
  discard: 5,
  ready_hand: 6,
  pass: 7,
};

const ACTION_LABELS: Record<BattleActionId, string> = {
  ready: '准备',
  start_match: '开始对局',
  start_next_round: '下一局',
  restart_match: '再来一局',
  discard: '出牌',
  ready_hand: '听',
  flower: '补花',
  kong: '杠',
  hu: '和牌',
  chow: '吃',
  pung: '碰',
  pass: '过',
};

function isBackendActionType(value: unknown): value is BackendActionType {
  return (
    value === 'discard' ||
    value === 'ready_hand' ||
    value === 'flower' ||
    value === 'kong' ||
    value === 'hu' ||
    value === 'chow' ||
    value === 'pung' ||
    value === 'pass'
  );
}

const PHASE_LABELS = {
  waiting: '等待中',
  playing: '进行中',
  settlement: '结算中',
  finished: '已结束',
} as const;

const SUIT_ORDER = {
  w: 0,
  m: 0,
  b: 1,
  p: 1,
  c: 2,
  t: 2,
} as const;

const HONOR_ORDER = {
  east: 0,
  south: 1,
  west: 2,
  north: 3,
  red: 4,
  green: 5,
  white: 6,
  d1: 0,
  d2: 1,
  d3: 2,
  d4: 3,
  d5: 4,
  d6: 5,
  d7: 6,
  f1: 7,
  f2: 8,
  f3: 9,
  f4: 10,
  f5: 11,
  f6: 12,
  f7: 13,
  f8: 14,
} as const;

function getLocalSeat(state: SessionState): number {
  return state.roomSnapshot?.payload.local_seat ?? 0;
}

function getOptimisticDiscard(state: SessionState) {
  return state.optimisticDiscard ?? null;
}

function hasOptimisticDiscardPending(state: SessionState) {
  return Boolean(getOptimisticDiscard(state));
}

function toRelativeSeat(localSeat: number, absoluteSeat: number): Seat {
  const offset = (absoluteSeat - localSeat + 4) % 4;
  return RELATIVE_SEATS[offset] ?? 'bottom';
}

function getWindForSeat(seatIndex: number, dealerSeat: number | null | undefined): PlayerView['wind'] {
  if (typeof dealerSeat !== 'number') {
    return WINDS[seatIndex] ?? 'East';
  }

  return WINDS[(seatIndex - dealerSeat + 4) % 4] ?? 'East';
}

function formatSignedNumber(value: number) {
  if (value > 0) {
    return `+${value}`;
  }

  return `${value}`;
}

function getPendingActionOptions(pendingAction: { options?: unknown } | null | undefined): BackendActionType[] {
  const options = pendingAction?.options;
  return Array.isArray(options) ? options.filter(isBackendActionType) : [];
}

function formatActionLabels(options: BackendActionType[]) {
  const visibleLabels = options.filter((item) => item !== 'pass').map((item) => ACTION_LABELS[item]);

  if (visibleLabels.length > 0) {
    return visibleLabels.join(' / ');
  }

  return options.includes('pass') ? ACTION_LABELS.pass : '';
}

function orderPromptActions(options: BackendActionType[]) {
  return options
    .slice()
    .sort((left, right) => (PROMPT_ACTION_PRIORITY[left] ?? Number.MAX_SAFE_INTEGER) - (PROMPT_ACTION_PRIORITY[right] ?? Number.MAX_SAFE_INTEGER));
}

function getSeatName(state: SessionState, seatIndex: number | null | undefined) {
  if (typeof seatIndex !== 'number') {
    return '一名玩家';
  }

  return state.roomSnapshot?.payload.seats.find((seat) => seat.seat_index === seatIndex)?.nickname ?? `玩家${seatIndex + 1}`;
}

function createActorPrompt(actorLabel: string, options: BackendActionType[]) {
  const actionLabels = formatActionLabels(options);
  return actionLabels ? `${actorLabel}正在执行操作：${actionLabels}` : null;
}

function getPendingActionSeatIndex(pendingAction: { seat_index?: unknown }) {
  return typeof pendingAction.seat_index === 'number' ? pendingAction.seat_index : null;
}

function getCurrentActorSeatIndex(privateState: PrivateState | null | undefined) {
  if (!privateState) {
    return null;
  }

  if (
    privateState.pending_action?.type === 'active_turn' &&
    typeof privateState.pending_action.seat_index === 'number'
  ) {
    return privateState.pending_action.seat_index;
  }

  return typeof privateState.current_actor === 'number' ? privateState.current_actor : null;
}

function createPublicTurnPrompt(state: SessionState) {
  const snapshot = state.roomSnapshot?.payload;
  if (snapshot?.phase !== 'playing') {
    return null;
  }

  const actorSeat = getCurrentActorSeatIndex(snapshot.private_state);
  if (typeof actorSeat !== 'number') {
    return null;
  }

  return createActorPrompt(getSeatName(state, actorSeat), ['discard']);
}

function createCenterDeadlineAt(state: SessionState) {
  const snapshot = state.roomSnapshot?.payload;
  const pendingAction = snapshot?.private_state?.pending_action;

  if (pendingAction && 'deadline_at' in pendingAction) {
    return String(pendingAction.deadline_at);
  }

  if (!state.latestActionPrompt) {
    return null;
  }

  const currentActor = getCurrentActorSeatIndex(snapshot?.private_state);
  const promptSeat = state.latestActionPrompt.payload.seat_index;

  if (typeof currentActor !== 'number' || currentActor === promptSeat) {
    return state.latestActionPrompt.payload.deadline_at;
  }

  return createPublicTurnPrompt(state) ? null : state.latestActionPrompt.payload.deadline_at;
}

interface MatchViewModelOptions {
  showLocalTurnKongPrompt?: boolean;
}

function createPromptText(state: SessionState, options: MatchViewModelOptions = {}): string | null {
  const optimisticDiscard = getOptimisticDiscard(state);
  if (optimisticDiscard) {
    return optimisticDiscard.actionType === 'ready_hand'
      ? '你已听牌，等待服务器确认...'
      : '你已出牌，等待服务器确认...';
  }

  const snapshot = state.roomSnapshot?.payload;
  const currentActor = getCurrentActorSeatIndex(snapshot?.private_state);
  const pendingAction = state.roomSnapshot?.payload.private_state?.pending_action;
  const localPromptOptions = getLocalPromptOptions(state);

  if (
    options.showLocalTurnKongPrompt &&
    pendingAction?.type === 'active_turn' &&
    typeof pendingAction.seat_index === 'number' &&
    pendingAction.seat_index === getLocalSeat(state)
  ) {
    const turnKongOptions = orderPromptActions([
      ...(localPromptOptions.includes('hu') ? (['hu'] as BackendActionType[]) : []),
      'kong',
      'pass',
    ]);

    return createActorPrompt(getSeatName(state, pendingAction.seat_index), turnKongOptions);
  }

  if (pendingAction && typeof pendingAction.type === 'string') {
    if (pendingAction.type === 'active_turn') {
      const actorSeat =
        getPendingActionSeatIndex(pendingAction) ?? getCurrentActorSeatIndex(state.roomSnapshot?.payload.private_state);
      const options = getPendingActionOptions(pendingAction as { options?: unknown });
      return createActorPrompt(getSeatName(state, actorSeat), options.length > 0 ? options : ['discard']);
    }
    if (pendingAction.type === 'claim_window') {
      const claimLabels = getPendingActionOptions(pendingAction as { options?: unknown });
      return createActorPrompt('一名玩家', claimLabels.length > 0 ? claimLabels : ['chow', 'pung', 'kong', 'hu']);
    }
    if (pendingAction.type === 'rob_kong_window') {
      const options = getPendingActionOptions(pendingAction as { options?: unknown });
      return createActorPrompt('一名玩家', options.length > 0 ? options : ['hu']);
    }
  }

  if (state.latestActionPrompt) {
    const promptSeat = state.latestActionPrompt.payload.seat_index;
    if (typeof currentActor !== 'number' || currentActor === promptSeat) {
      return createActorPrompt(
        getSeatName(state, promptSeat),
        state.latestActionPrompt.payload.options.filter(isBackendActionType),
      );
    }
  }

  const publicTurnPrompt = createPublicTurnPrompt(state);
  if (publicTurnPrompt) {
    return publicTurnPrompt;
  }

  if (state.latestActionPrompt) {
    return createActorPrompt(
      getSeatName(state, state.latestActionPrompt.payload.seat_index),
      state.latestActionPrompt.payload.options.filter(isBackendActionType),
    );
  }

  if (state.roomSnapshot?.payload.phase === 'finished') {
    return '整场对局已结束，可发起再来一局';
  }

  return null;
}

function getPromptSourceSeatLabel(seat: Seat | null) {
  if (!seat) {
    return '当前牌局';
  }

  return PROMPT_SEAT_COPY[seat];
}

function getLocalPromptOptions(state: SessionState): BackendActionType[] {
  const localSeat = getLocalSeat(state);
  const pendingAction = state.roomSnapshot?.payload.private_state?.pending_action;

  if (pendingAction && 'options' in pendingAction) {
    const options = (pendingAction as { options?: unknown }).options;
    if (Array.isArray(options)) {
      return options.filter(isBackendActionType);
    }
  }

  if (state.latestActionPrompt?.payload.seat_index === localSeat) {
    return state.latestActionPrompt.payload.options.filter(isBackendActionType);
  }

  return [];
}

function createPromptCue(state: SessionState, options: MatchViewModelOptions = {}): BattlePromptView | null {
  if (hasOptimisticDiscardPending(state)) {
    return null;
  }

  const snapshot = state.roomSnapshot?.payload;
  const pendingAction = snapshot?.private_state?.pending_action;
  const localSeat = getLocalSeat(state);
  const localPromptOptions = orderPromptActions(getLocalPromptOptions(state));
  const highlightedActionIds = orderPromptActions(localPromptOptions.filter((option) => option !== 'pass'));

  if (
    options.showLocalTurnKongPrompt &&
    pendingAction?.type === 'active_turn' &&
    typeof pendingAction.seat_index === 'number' &&
    pendingAction.seat_index === localSeat
  ) {
    const turnKongActionIds = orderPromptActions([
      ...(localPromptOptions.includes('hu') ? (['hu'] as BackendActionType[]) : []),
      'kong',
      'pass',
    ]);
    const turnKongHighlightedActionIds = orderPromptActions(turnKongActionIds.filter((option) => option !== 'pass'));

    return {
      kind: 'turn_kong',
      tone: turnKongHighlightedActionIds.includes('hu') ? 'critical' : 'urgent',
      title: '当前可选择是否杠牌',
      detail: `你可以 ${formatActionLabels(turnKongActionIds)}`,
      actionIds: turnKongActionIds,
      highlightedActionIds: turnKongHighlightedActionIds,
      sourceSeat: null,
      isUrgent: true,
    };
  }

  if (pendingAction?.type === 'claim_window' && highlightedActionIds.length > 0) {
    const sourceSeat =
      typeof pendingAction.discarder_seat === 'number' ? toRelativeSeat(localSeat, pendingAction.discarder_seat) : createLastDiscardSeat(state);

    return {
      kind: 'claim',
      tone: highlightedActionIds.includes('hu') ? 'critical' : 'urgent',
      title: `${getPromptSourceSeatLabel(sourceSeat)}刚打出可响应牌`,
      detail: `你可以 ${formatActionLabels(localPromptOptions)}`,
      actionIds: localPromptOptions,
      highlightedActionIds,
      sourceSeat,
      isUrgent: true,
    };
  }

  if (pendingAction?.type === 'rob_kong_window' && highlightedActionIds.length > 0) {
    const sourceSeat = typeof pendingAction.actor_seat === 'number' ? toRelativeSeat(localSeat, pendingAction.actor_seat) : null;

    return {
      kind: 'rob_kong',
      tone: 'critical',
      title: `${getPromptSourceSeatLabel(sourceSeat)}正在补杠`,
      detail: `你可以 ${formatActionLabels(localPromptOptions)}`,
      actionIds: localPromptOptions,
      highlightedActionIds,
      sourceSeat,
      isUrgent: true,
    };
  }

  if (
    pendingAction?.type === 'active_turn' &&
    typeof pendingAction.seat_index === 'number' &&
    pendingAction.seat_index === localSeat &&
    highlightedActionIds.length > 0
  ) {
    const hasHu = highlightedActionIds.includes('hu');
    const hasDiscard = localPromptOptions.includes('discard');
    const title = hasHu
      ? '当前手牌可直接和牌'
      : hasDiscard
        ? '轮到你操作'
        : '当前手牌可执行操作';

    return {
      kind: 'turn',
      tone: hasHu ? 'critical' : 'info',
      title,
      detail: `你可以 ${formatActionLabels(localPromptOptions)}`,
      actionIds: localPromptOptions,
      highlightedActionIds,
      sourceSeat: null,
      isUrgent: hasHu,
    };
  }

  if (state.latestActionPrompt?.payload.seat_index === localSeat && localPromptOptions.length > 0) {
    return {
      kind: 'turn',
      tone: highlightedActionIds.includes('hu') ? 'critical' : 'info',
      title: '轮到你操作',
      detail: `你可以 ${formatActionLabels(localPromptOptions)}`,
      actionIds: localPromptOptions,
      highlightedActionIds,
      sourceSeat: null,
      isUrgent: highlightedActionIds.includes('hu'),
    };
  }

  return null;
}

function createWaitingControls(state: SessionState): WaitingControls | null {
  const snapshot = state.roomSnapshot?.payload;
  if (!snapshot || snapshot.phase !== 'waiting') {
    return null;
  }

  const localSeat = snapshot.local_seat;
  const localSeatState = typeof localSeat === 'number' ? snapshot.seats.find((seat) => seat.seat_index === localSeat) : null;
  const occupiedSeats = snapshot.seats.length;
  const botCount = snapshot.seats.filter((seat) => seat.is_bot).length;
  const allReady =
    occupiedSeats === 4 &&
    snapshot.seats.every((seat) => seat.ready && (seat.connected || seat.is_bot));

  return {
    canReady: Boolean(localSeatState),
    canStart: allReady,
    isReady: Boolean(localSeatState?.ready),
    occupiedSeats,
    botCount,
    canAddBot: Boolean(localSeatState) && occupiedSeats < 4,
    canRemoveBot: Boolean(localSeatState) && botCount > 0,
  };
}

function getPromptOptions(state: SessionState): BackendActionType[] {
  const pendingAction = state.roomSnapshot?.payload.private_state?.pending_action;
  if (pendingAction && 'options' in pendingAction) {
    const options = (pendingAction as { options?: unknown }).options;
    if (Array.isArray(options)) {
      return options.filter(isBackendActionType);
    }
  }

  return (state.latestActionPrompt?.payload.options ?? []).filter(isBackendActionType);
}

function getRestrictedDiscardTileIdSet(state: SessionState) {
  const pendingAction = state.roomSnapshot?.payload.private_state?.pending_action;
  const restrictedDiscardTileIds =
    pendingAction?.type === 'active_turn' && Array.isArray(pendingAction.restricted_discard_tile_ids)
      ? pendingAction.restricted_discard_tile_ids
      : [];

  return new Set(restrictedDiscardTileIds);
}

function createActionViews(
  state: SessionState,
  waitingControls: WaitingControls | null,
  options: MatchViewModelOptions = {},
): BattleActionView[] {
  const snapshot = state.roomSnapshot?.payload;
  const nextRoundConfirmation = createContinueActionConfirmation(state, 'start_next_round');
  const restartMatchConfirmation = createContinueActionConfirmation(state, 'restart_match');
  const promptOptions = new Set<BackendActionType>(getPromptOptions(state));
  const localTurnKongCandidateGroups = getLocalTurnKongCandidateGroups(state);
  const kongCandidateGroups = options.showLocalTurnKongPrompt
    ? localTurnKongCandidateGroups
    : getActionCandidateGroups(state, 'kong');
  const chowCandidateGroups = getActionCandidateGroups(state, 'chow');
  const pungCandidateGroups = getActionCandidateGroups(state, 'pung');
  const flowerCandidateTileIds = getFlowerCandidateTileIds(state);
  const restrictedDiscardTileIdSet = getRestrictedDiscardTileIdSet(state);
  const optimisticDiscardPending = hasOptimisticDiscardPending(state);
  const localPlayer =
    typeof snapshot?.local_seat === 'number' ? findPrivatePlayer(state, snapshot.local_seat) : undefined;
  const localReadyHandLocked = localPlayer?.is_ready_hand === true;
  const hasSelectedFlower =
    state.selectedTileIds.length === 1 && flowerCandidateTileIds.includes(state.selectedTileIds[0]);
  const hasSelectedDiscard =
    !localReadyHandLocked &&
    state.selectedTileIds.length === 1 &&
    !restrictedDiscardTileIdSet.has(state.selectedTileIds[0]);
  const selectedReadyHandTileId = state.selectedTileIds.length === 1 ? state.selectedTileIds[0] : null;
  const canReadyHandFromSelection =
    !localReadyHandLocked &&
    Boolean(selectedReadyHandTileId) &&
    !restrictedDiscardTileIdSet.has(selectedReadyHandTileId as string) &&
    getReadyHandWaitsForLocalPlayer(state, selectedReadyHandTileId).length > 0;
  const canContinueRound = snapshot?.phase === 'settlement' && typeof snapshot.local_seat === 'number';
  const canRestartMatch =
    snapshot?.phase === 'finished' &&
    snapshot.match_state?.match_finished === true &&
    typeof snapshot.local_seat === 'number';

  return ACTION_ORDER.map((id) => {
    let enabled = false;

    if (id === 'ready') {
      enabled = waitingControls?.canReady ?? false;
    } else if (id === 'start_match') {
      enabled = waitingControls?.canStart ?? false;
    } else if (id === 'start_next_round') {
      enabled = canContinueRound && !nextRoundConfirmation?.isLocalConfirmed;
    } else if (id === 'restart_match') {
      enabled = canRestartMatch && !restartMatchConfirmation?.isLocalConfirmed;
    } else if (options.showLocalTurnKongPrompt && id === 'kong') {
      enabled = kongCandidateGroups.length > 0;
    } else if (options.showLocalTurnKongPrompt && id === 'pass') {
      enabled = true;
    } else if (promptOptions.has(id as BackendActionType)) {
      enabled =
        id === 'discard'
          ? hasSelectedDiscard
          : id === 'ready_hand'
            ? canReadyHandFromSelection
          : id === 'flower'
            ? hasSelectedFlower
            : id === 'kong'
              ? kongCandidateGroups.length > 0
              : id === 'chow'
                ? chowCandidateGroups.length > 0
                : id === 'pung'
                ? pungCandidateGroups.length > 0
                  : true;
    }

    if (
      optimisticDiscardPending &&
      (id === 'discard' ||
        id === 'ready_hand' ||
        id === 'flower' ||
        id === 'kong' ||
        id === 'hu' ||
        id === 'chow' ||
        id === 'pung' ||
        id === 'pass')
    ) {
      enabled = false;
    }

    const emphasis =
      id === 'start_match' || id === 'start_next_round' || id === 'restart_match' || (id === 'discard' && enabled)
        ? 'high'
        : enabled
          ? 'medium'
          : 'low';
    return {
      id,
      label: getActionLabel(state, id, waitingControls, {
        startNextRound: nextRoundConfirmation,
        restartMatch: restartMatchConfirmation,
      }),
      enabled,
      emphasis,
    };
  });
}

function getActionLabel(
  state: SessionState,
  id: BattleActionId,
  waitingControls: WaitingControls | null,
  continueActionConfirmations?: {
    startNextRound: ReturnType<typeof createContinueActionConfirmation>;
    restartMatch: ReturnType<typeof createContinueActionConfirmation>;
  },
) {
  if (id === 'ready' && waitingControls?.isReady) {
    return '取消准备';
  }

  const confirmation =
    id === 'start_next_round'
      ? continueActionConfirmations?.startNextRound
      : id === 'restart_match'
        ? continueActionConfirmations?.restartMatch
        : null;
  if (confirmation?.countdownDeadlineAt) {
    return formatContinueActionCountdownLabel(confirmation.countdownDeadlineAt);
  }

  if (confirmation?.isLocalConfirmed) {
    return formatContinueActionConfirmedLabel(confirmation.confirmedCount, confirmation.requiredCount);
  }

  if (id === 'start_next_round') {
    return getStartNextRoundLabel(state);
  }

  return ACTION_LABELS[id];
}

function findPrivatePlayer(state: SessionState, seatIndex: number): PrivatePlayerState | undefined {
  return state.roomSnapshot?.payload.private_state?.players.find((player) => player.seat_index === seatIndex);
}

function getDisplayedScores(state: SessionState) {
  const snapshot = state.roomSnapshot?.payload;
  const matchScores = snapshot?.match_state?.cumulative_scores ?? {};
  const liveScores = snapshot?.private_state?.score_state?.projected_cumulative_scores;

  if (liveScores) {
    return liveScores;
  }

  return matchScores;
}

function getLiveDeltaBySeat(state: SessionState) {
  const scoreState = state.roomSnapshot?.payload.private_state?.score_state;
  if (scoreState?.current_round_delta_by_seat) {
    return scoreState.current_round_delta_by_seat;
  }

  if (state.latestMatchResult?.payload.score_delta.total_delta_by_seat) {
    return state.latestMatchResult.payload.score_delta.total_delta_by_seat;
  }

  return {};
}

function getFlowerCountBySeat(state: SessionState) {
  return state.roomSnapshot?.payload.private_state?.score_state?.flower_count_by_seat ?? {};
}

function normalizeTileCodeList(tileCodes: readonly (string | null | undefined)[] | null | undefined) {
  return (tileCodes ?? []).filter((tileCode): tileCode is string => typeof tileCode === 'string' && tileCode.trim().length > 0);
}

function normalizeDisplayMelds(melds: string[][] | null | undefined) {
  return (melds ?? [])
    .map((meld) => normalizeTileCodeList(meld))
    .filter((meld) => meld.length > 0)
    .map(
      (meld): DisplayMeldView => ({
        tiles: meld.map((code) => ({ code, orientation: 'normal' as const })),
      }),
    );
}

function createPlayers(state: SessionState): PlayerView[] {
  const snapshot = state.roomSnapshot?.payload;
  if (!snapshot) {
    return [];
  }

  const localSeat = getLocalSeat(state);
  const currentActor = getCurrentActorSeatIndex(snapshot.private_state);
  const dealerSeat = snapshot.match_state?.dealer_seat ?? snapshot.private_state?.dealer_seat;
  const displayedScores = getDisplayedScores(state);
  const liveDeltaBySeat = getLiveDeltaBySeat(state);
  const flowerCountBySeat = getFlowerCountBySeat(state);

  return snapshot.seats
    .map((seat) => {
      const relativeSeat = toRelativeSeat(localSeat, seat.seat_index);
      const privatePlayer = findPrivatePlayer(state, seat.seat_index);
      const seatKey = String(seat.seat_index);
      const seatType = seat.seat_type ?? (seat.is_bot ? 'bot' : 'human');
      const isBotSeat = seatType === 'bot';

      return {
        seat: relativeSeat,
        absoluteSeat: seat.seat_index,
        name: seat.nickname,
        seatType,
        score: displayedScores[seatKey] ?? 0,
        liveDelta: liveDeltaBySeat[seatKey] ?? 0,
        flowerCount: flowerCountBySeat[seatKey] ?? privatePlayer?.flowers.length ?? 0,
        wind: getWindForSeat(seat.seat_index, dealerSeat),
        isDealer: dealerSeat === seat.seat_index,
        isActive: currentActor === seat.seat_index,
        isLocal: seat.seat_index === localSeat,
        connected: seat.connected,
        isBotControlled: Boolean(seat.is_bot),
        ready: seat.ready,
        concealedCount: privatePlayer?.concealed_count ?? 0,
        meldCount: privatePlayer?.melds.length ?? 0,
        melds: privatePlayer?.display_melds ?? normalizeDisplayMelds(privatePlayer?.melds),
        flowers: normalizeTileCodeList(privatePlayer?.flowers),
        statusText:
          isBotSeat
            ? 'Bot代打中'
            : !seat.connected
              ? '等待重连中'
              : snapshot.phase === 'waiting'
                ? seat.ready
                  ? '已准备'
                  : '等待中'
                : snapshot.phase === 'settlement'
                  ? '等待下一局'
                  : snapshot.phase === 'finished'
                    ? '整场完成'
                    : '对局中',
      };
    })
    .sort((left, right) => RELATIVE_SEATS.indexOf(left.seat) - RELATIVE_SEATS.indexOf(right.seat));
}

function createDiscards(state: SessionState): Record<Seat, string[]> {
  const empty: Record<Seat, string[]> = {
    bottom: [],
    left: [],
    top: [],
    right: [],
  };

  const snapshot = state.roomSnapshot?.payload;
  if (!snapshot?.private_state) {
    return empty;
  }

  const localSeat = getLocalSeat(state);
  for (const player of snapshot.private_state.players) {
    empty[toRelativeSeat(localSeat, player.seat_index)] = normalizeTileCodeList(player.discards);
  }

  const optimisticDiscard = getOptimisticDiscard(state);
  if (optimisticDiscard) {
    const optimisticSeat = toRelativeSeat(localSeat, optimisticDiscard.seatIndex);
    empty[optimisticSeat] = [...empty[optimisticSeat], optimisticDiscard.tileCode];
  }

  return empty;
}

function createSelectedTileCode(state: SessionState) {
  if (state.selectionMode !== 'single' || state.selectedTileIds.length !== 1) {
    return null;
  }

  const localSeat = getLocalSeat(state);
  const localPlayer = findPrivatePlayer(state, localSeat);
  const selectedTileId = state.selectedTileIds[0];
  const selectedTile = (localPlayer?.concealed_tiles ?? []).find((tile) => tile.tile_id === selectedTileId);

  return selectedTile?.tile_key ?? null;
}

function createLocalHand(state: SessionState) {
  const localSeat = getLocalSeat(state);
  const localPlayer = findPrivatePlayer(state, localSeat);
  const localReadyHandLocked = localPlayer?.is_ready_hand === true;
  const optimisticDiscard = getOptimisticDiscard(state);
  const optimisticFlowerTileId = getOptimisticFlowerTileId(state);
  const replacementDrawnTileId = state.latestReplacementTileId ?? null;
  const pendingAction = state.roomSnapshot?.payload.private_state?.pending_action;
  const drawnTileId =
    pendingAction?.type === 'active_turn' && pendingAction.drawn_tile_id !== optimisticDiscard?.tileId
      ? pendingAction.drawn_tile_id
      : undefined;
  const restrictedDiscardTileIdSet = getRestrictedDiscardTileIdSet(state);

  const sortedHand = (localPlayer?.concealed_tiles ?? [])
    .filter((tile) => tile.tile_id !== optimisticDiscard?.tileId && tile.tile_id !== optimisticFlowerTileId)
    .map((tile) => ({
      tileId: tile.tile_id,
      code: tile.tile_key,
      isSelected: state.selectedTileIds.includes(tile.tile_id),
      isDrawn: tile.tile_id === drawnTileId,
      isReplacementDrawn: tile.tile_id === replacementDrawnTileId,
      isFlower: isFlowerTileKey(tile.tile_key),
      isDisabled:
        Boolean(optimisticDiscard) ||
        localReadyHandLocked ||
        restrictedDiscardTileIdSet.has(tile.tile_id),
    }))
    .sort(compareLocalHandTiles);

  if (!drawnTileId) {
    return sortedHand;
  }

  const drawnTileIndex = sortedHand.findIndex((tile) => tile.tileId === drawnTileId);
  if (drawnTileIndex < 0) {
    return sortedHand;
  }

  const [drawnTile] = sortedHand.splice(drawnTileIndex, 1);
  return [...sortedHand, drawnTile];
}

function createReadyHandInsight(state: SessionState): BattleViewModel['readyHandInsight'] {
  if (hasOptimisticDiscardPending(state)) {
    return null;
  }

  const snapshot = state.roomSnapshot?.payload;
  const privateState = snapshot?.private_state;
  const localSeat = snapshot?.local_seat;
  if (!privateState || typeof localSeat !== 'number') {
    return null;
  }

  const localPlayer = findPrivatePlayer(state, localSeat);
  const concealedTiles = localPlayer?.concealed_tiles ?? [];
  if (concealedTiles.length === 0) {
    return null;
  }
  if (localPlayer?.is_ready_hand) {
    const lockedDiscardTileId =
      privateState.pending_action?.type === 'active_turn' &&
      typeof privateState.pending_action.drawn_tile_id === 'string'
        ? privateState.pending_action.drawn_tile_id
        : null;
    const waits = getReadyHandWaitsForLocalPlayer(state, lockedDiscardTileId);
    return waits.length > 0
      ? {
          source: 'current',
          discardTileId: null,
          discardTileCode: null,
          waits,
        }
      : null;
  }

  const selectedDiscardTile =
    state.selectedTileIds.length === 1
      ? concealedTiles.find((tile) => tile.tile_id === state.selectedTileIds[0]) ?? null
      : null;

  if (
    selectedDiscardTile &&
    !getRestrictedDiscardTileIdSet(state).has(selectedDiscardTile.tile_id) &&
    !isFlowerTileKey(selectedDiscardTile.tile_key)
  ) {
    const waits = getReadyHandWaitsForLocalPlayer(state, selectedDiscardTile.tile_id);
    return waits.length > 0
      ? {
          source: 'selected_discard',
          discardTileId: selectedDiscardTile.tile_id,
          discardTileCode: selectedDiscardTile.tile_key,
          waits,
        }
      : null;
  }

  const waits = getReadyHandWaitsForLocalPlayer(state, null);
  return waits.length > 0
    ? {
        source: 'current',
        discardTileId: null,
        discardTileCode: null,
        waits,
      }
    : null;
}

function getReadyHandWaitsForLocalPlayer(state: SessionState, discardTileId: string | null) {
  const snapshot = state.roomSnapshot?.payload;
  const privateState = snapshot?.private_state;
  const localSeat = snapshot?.local_seat;
  if (!privateState || typeof localSeat !== 'number') {
    return [];
  }

  const localPlayer = findPrivatePlayer(state, localSeat);
  const concealedTiles = localPlayer?.concealed_tiles ?? [];
  const concealedTileKeys = concealedTiles
    .filter((tile) => tile.tile_id !== discardTileId)
    .map((tile) => tile.tile_key);
  const knownTileKeys = collectKnownTileKeys(state, concealedTileKeys, discardTileId);

  if (discardTileId) {
    const discardedTile = concealedTiles.find((tile) => tile.tile_id === discardTileId);
    if (discardedTile) {
      knownTileKeys.push(discardedTile.tile_key);
    }
  }

  return getReadyHandWaits({
    concealedTileKeys,
    meldTileKeyGroups: localPlayer?.melds ?? [],
    knownTileKeys,
  });
}

function collectKnownTileKeys(
  state: SessionState,
  localConcealedTileKeys: string[],
  discardedTileId: string | null,
) {
  const snapshot = state.roomSnapshot?.payload;
  const privateState = snapshot?.private_state;
  const localSeat = snapshot?.local_seat;
  if (!privateState || typeof localSeat !== 'number') {
    return [];
  }

  const knownTileKeys = [...localConcealedTileKeys];
  for (const player of privateState.players) {
    knownTileKeys.push(...player.discards);
    knownTileKeys.push(...player.flowers);
    for (const meld of player.melds) {
      knownTileKeys.push(...meld);
    }
  }

  if (!discardedTileId) {
    return knownTileKeys;
  }

  return knownTileKeys;
}

function hasMatchingTileSelection(selectedTileIds: string[], candidateTileIds: string[]) {
  if (selectedTileIds.length !== candidateTileIds.length) {
    return false;
  }

  const normalizedSelection = [...new Set(selectedTileIds)].sort();
  const normalizedCandidate = [...new Set(candidateTileIds)].sort();

  return normalizedCandidate.every((tileId, index) => tileId === normalizedSelection[index]);
}

function compareClaimPreviewTiles(left: ClaimCandidateTileView, right: ClaimCandidateTileView) {
  const leftKey = getTileSortKey(left.code);
  const rightKey = getTileSortKey(right.code);

  if (leftKey.group !== rightKey.group) {
    return leftKey.group - rightKey.group;
  }

  if (leftKey.order !== rightKey.order) {
    return leftKey.order - rightKey.order;
  }

  if (left.source !== right.source) {
    return left.source === 'hand' ? -1 : 1;
  }

  return 0;
}

function createClaimCandidateTiles(
  actionId: ClaimActionId,
  tileIds: string[],
  discardTileKey: string,
  concealedTileKeyById: Map<string, string>,
) {
  const handTiles = tileIds
    .map((tileId) => concealedTileKeyById.get(tileId))
    .filter((tileKey): tileKey is string => typeof tileKey === 'string')
    .map((tileKey) => ({ code: tileKey, source: 'hand' as const }));
  const previewTiles = [...handTiles, { code: discardTileKey, source: 'claim' as const }];

  if (actionId === 'chow') {
    return previewTiles.sort(compareClaimPreviewTiles);
  }

  return previewTiles;
}

function getClaimCandidateSignature(actionId: ClaimActionId, tiles: ClaimCandidateTileView[]) {
  return `${actionId}:${tiles.map((tile) => `${tile.source}:${tile.code}`).join('|')}`;
}

export function createClaimCandidates(state: SessionState): ClaimCandidateView[] {
  if (hasOptimisticDiscardPending(state)) {
    return [];
  }

  const privateState = state.roomSnapshot?.payload.private_state;
  const localSeat = getLocalSeat(state);
  const localPlayer =
    typeof localSeat === 'number'
      ? privateState?.players.find((player) => player.seat_index === localSeat)
      : null;
  const discardTileKey = privateState?.pending_action?.type === 'claim_window' ? privateState.last_discard : null;

  if (!localPlayer || privateState?.pending_action?.type !== 'claim_window' || !discardTileKey) {
    return [];
  }

  const concealedTileKeyById = new Map(
    (localPlayer.concealed_tiles ?? []).map((tile) => [tile.tile_id, tile.tile_key] as const),
  );
  const dedupedCandidates = new Map<
    string,
    ClaimCandidateView & {
      matchingGroups: string[][];
    }
  >();

  for (const actionId of ['kong', 'pung', 'chow'] as const) {
    for (const tileIds of getActionCandidateGroups(state, actionId)) {
      const tiles = createClaimCandidateTiles(actionId, tileIds, discardTileKey, concealedTileKeyById);
      const signature = getClaimCandidateSignature(actionId, tiles);
      const existing = dedupedCandidates.get(signature);

      if (existing) {
        existing.matchingGroups.push(tileIds);
        existing.isSelected = existing.isSelected || hasMatchingTileSelection(state.selectedTileIds, tileIds);
        continue;
      }

      dedupedCandidates.set(signature, {
        key: `${actionId}:${tileIds.slice().sort().join('|')}`,
        actionId,
        actionLabel: ACTION_LABELS[actionId],
        tileIds,
        tiles,
        isSelected: hasMatchingTileSelection(state.selectedTileIds, tileIds),
        matchingGroups: [tileIds],
      });
    }
  }

  return Array.from(dedupedCandidates.values()).map(({ matchingGroups: _matchingGroups, ...candidate }) => candidate);
}

function createDrawnTileId(state: SessionState) {
  const pendingAction = state.roomSnapshot?.payload.private_state?.pending_action;
  const optimisticDiscard = getOptimisticDiscard(state);

  if (
    pendingAction?.type === 'active_turn' &&
    typeof pendingAction.drawn_tile_id === 'string' &&
    pendingAction.drawn_tile_id !== optimisticDiscard?.tileId
  ) {
    return pendingAction.drawn_tile_id;
  }

  return null;
}

function createSettlementHands(state: SessionState): BattleViewModel['settlementHands'] {
  const snapshot = state.roomSnapshot?.payload;
  const privateState = snapshot?.private_state;
  const settlementWinningDiscard = getSettlementWinningDiscard(state);

  if (!snapshot || snapshot.phase !== 'settlement' || !privateState) {
    return null;
  }

  const localSeat = getLocalSeat(state);
  const settlementHands: Partial<Record<Seat, string[]>> = {};

  for (const player of privateState.players) {
    const seat = toRelativeSeat(localSeat, player.seat_index);
    const tiles = (player.concealed_tiles ?? [])
      .map((tile) => tile.tile_key)
      .slice()
      .sort(compareTileCodes);

    if (tiles.length === 0 && !settlementWinningDiscard?.winnerSeats.has(seat)) {
      continue;
    }

    settlementHands[seat] =
      settlementWinningDiscard?.winnerSeats.has(seat)
        ? [...tiles, settlementWinningDiscard.tileCode]
        : tiles;
  }

  return Object.keys(settlementHands).length > 0 ? settlementHands : null;
}

function getSettlementWinningDiscard(state: SessionState): { winnerSeats: Set<Seat>; tileCode: string } | null {
  const snapshot = state.roomSnapshot?.payload;
  const privateState = snapshot?.private_state;
  const result = state.latestMatchResult?.payload;
  const localSeat = getLocalSeat(state);
  const winnerSeats = new Set(
    (result?.winning_details ?? [])
      .map((detail) =>
        typeof detail.winner_seat === 'number' ? toRelativeSeat(localSeat, detail.winner_seat) : null,
      )
      .filter((seat): seat is Seat => seat !== null),
  );

  if (
    !snapshot ||
    snapshot.phase !== 'settlement' ||
    !privateState ||
    !result ||
    result.win_type !== 'discard' ||
    !privateState.last_discard
  ) {
    return null;
  }

  if (winnerSeats.size === 0 && typeof result.winner_seat === 'number') {
    winnerSeats.add(toRelativeSeat(localSeat, result.winner_seat));
  }
  if (winnerSeats.size === 0) {
    return null;
  }

  return {
    winnerSeats,
    tileCode: privateState.last_discard,
  };
}

function createResultPages(result: MatchResultPayload, localSeat: number): ResultPageView[] {
  const winningDetails =
    Array.isArray(result.winning_details) && result.winning_details.length > 0
      ? result.winning_details
      : result.win_type === 'draw'
        ? []
        : [
            {
              winner_seat: result.winner_seat ?? -1,
              display_win_label: result.display_win_label ?? null,
              fan_total: result.fan_total,
              fan_keys: result.fan_keys,
              fan_breakdown: result.fan_breakdown,
              flower_count: result.flower_count,
            },
          ];

  return winningDetails
    .filter((detail) => typeof detail.winner_seat === 'number' && detail.winner_seat >= 0)
    .map((detail) => ({
      fanTotal: detail.fan_total,
      winnerSeat: toRelativeSeat(localSeat, detail.winner_seat),
      discarderSeat: typeof result.discarder_seat === 'number' ? toRelativeSeat(localSeat, result.discarder_seat) : null,
      winType: result.win_type,
      winTypeLabel: detail.display_win_label ?? WIN_TYPE_LABELS[result.win_type] ?? result.win_type,
      flowerCount: detail.flower_count,
      fanBreakdown: detail.fan_breakdown.map((item) => ({
        fanKey: item.fan_key,
        fanValue: item.fan_value,
      })),
    }));
}

function createResultSummary(result: MatchResultPayload, pageCount: number) {
  if (result.win_type === 'draw') {
    return result.draw_type === 'exhaustive' ? '荒牌流局' : '本局流局';
  }

  if (result.win_type === 'discard' && pageCount > 1) {
    return `${pageCount} 家同时和牌，等待玩家开始下一局`;
  }

  return `${getWinTypeLabel(result)}，等待玩家开始下一局`;
}

function compareLocalHandTiles(
  left: BattleViewModel['localHand'][number],
  right: BattleViewModel['localHand'][number],
) {
  const codeComparison = compareTileCodes(left.code, right.code);
  return codeComparison !== 0 ? codeComparison : left.tileId.localeCompare(right.tileId);
}

function getTileSortKey(code: string | null | undefined) {
  const normalized = typeof code === 'string' ? code.trim().toLowerCase() : '';
  const suited = normalized.match(/^([wbcmpt])([1-9])$/);

  if (suited) {
    const [, suit, rankText] = suited;
    return {
      group: SUIT_ORDER[suit as keyof typeof SUIT_ORDER],
      order: Number(rankText),
    };
  }

  const honorOrder = HONOR_ORDER[normalized as keyof typeof HONOR_ORDER];
  if (typeof honorOrder === 'number') {
    return {
      group: normalized.startsWith('f') ? 4 : 3,
      order: honorOrder,
    };
  }

  return {
    group: 5,
    order: Number.MAX_SAFE_INTEGER,
  };
}

function compareTileCodes(left: string | null | undefined, right: string | null | undefined) {
  const leftKey = getTileSortKey(left);
  const rightKey = getTileSortKey(right);
  const leftText = typeof left === 'string' ? left : '';
  const rightText = typeof right === 'string' ? right : '';

  if (leftKey.group !== rightKey.group) {
    return leftKey.group - rightKey.group;
  }

  if (leftKey.order !== rightKey.order) {
    return leftKey.order - rightKey.order;
  }

  return leftText.localeCompare(rightText);
}

function createResultSeatStats(state: SessionState, seatIndex: number, score: number): ResultSeatView['stats'] {
  const matchStatistics = state.matchStatistics;
  const seatStats = matchStatistics?.seatStatsBySeat[String(seatIndex)];

  if (seatStats && seatStats.scoreHistory.length > 0) {
    const completedRoundCount = matchStatistics?.completedRoundCount ?? 0;
    return {
      scoreHistory: [...seatStats.scoreHistory],
      winCount: seatStats.winCount,
      dealInCount: seatStats.dealInCount,
      completedRoundCount,
      winRate: completedRoundCount > 0 ? seatStats.winCount / completedRoundCount : 0,
    };
  }

  return {
    scoreHistory: [score],
    winCount: 0,
    dealInCount: 0,
    completedRoundCount: 0,
    winRate: 0,
  };
}

function createResultSeats(state: SessionState, scoreDeltaBySeat: Record<string, number> | null): ResultSeatView[] {
  const snapshot = state.roomSnapshot?.payload;
  if (!snapshot) {
    return [];
  }

  const localSeat = getLocalSeat(state);
  const scores = getDisplayedScores(state);

  return snapshot.seats
    .map((seat) => {
      const seatKey = String(seat.seat_index);
      return {
        seat: toRelativeSeat(localSeat, seat.seat_index),
        name: seat.nickname,
        score: scores[seatKey] ?? 0,
        delta: scoreDeltaBySeat ? (scoreDeltaBySeat[seatKey] ?? 0) : null,
        stats: createResultSeatStats(state, seat.seat_index, scores[seatKey] ?? 0),
      };
    })
    .sort((left, right) => right.score - left.score);
}

function createResult(state: SessionState): BattleViewModel['result'] {
  const snapshot = state.roomSnapshot?.payload;
  if (!snapshot) {
    return null;
  }

  const isConnectionInteractive = state.connectionStatus === 'connected';
  const localSeat = getLocalSeat(state);
  const nextRoundConfirmation = createContinueActionConfirmation(state, 'start_next_round');
  const restartMatchConfirmation = createContinueActionConfirmation(state, 'restart_match');

  if (snapshot.phase === 'settlement' && state.latestMatchResult) {
    const result = state.latestMatchResult.payload;
    const pages = createResultPages(result, localSeat);
    const primaryPage = pages[0] ?? null;
    const scoreDeltaBySeat: Partial<Record<Seat, number>> = {};
    for (const [seatIndex, delta] of Object.entries(result.score_delta.total_delta_by_seat)) {
      scoreDeltaBySeat[toRelativeSeat(localSeat, Number(seatIndex))] = delta;
    }

    return {
      title: '本局结算',
      summary: createResultSummary(result, pages.length),
      fanTotal: primaryPage?.fanTotal ?? result.fan_total,
      winnerSeat:
        primaryPage?.winnerSeat ??
        (typeof result.winner_seat === 'number' ? toRelativeSeat(localSeat, result.winner_seat) : null),
      discarderSeat:
        primaryPage?.discarderSeat ??
        (typeof result.discarder_seat === 'number' ? toRelativeSeat(localSeat, result.discarder_seat) : null),
      winType: result.win_type,
      winTypeLabel: primaryPage?.winTypeLabel ?? getWinTypeLabel(result),
      provisional: result.score_delta.provisional,
      flowerCount: primaryPage?.flowerCount ?? result.flower_count,
      fanBreakdown:
        primaryPage?.fanBreakdown ??
        result.fan_breakdown.map((item) => ({
          fanKey: item.fan_key,
          fanValue: item.fan_value,
        })),
      pages,
      scoreDeltaBySeat,
      seats: createResultSeats(state, result.score_delta.total_delta_by_seat),
      continueAction: {
        id: 'start_next_round',
        label: !isConnectionInteractive
          ? '重连中...'
          : nextRoundConfirmation?.countdownDeadlineAt
            ? formatContinueActionCountdownLabel(nextRoundConfirmation.countdownDeadlineAt)
            : nextRoundConfirmation?.isLocalConfirmed
              ? formatContinueActionConfirmedLabel(
                  nextRoundConfirmation.confirmedCount,
                  nextRoundConfirmation.requiredCount,
                )
              : getStartNextRoundLabel(state),
        enabled:
          isConnectionInteractive &&
          typeof snapshot.local_seat === 'number' &&
          !nextRoundConfirmation?.isLocalConfirmed,
        countdownDeadlineAt: nextRoundConfirmation?.countdownDeadlineAt ?? undefined,
        confirmation: nextRoundConfirmation ?? undefined,
      },
    };
  }

  if (snapshot.phase === 'finished') {
    return {
      title: '整场结束',
      summary: '本桌完整对局已经结束，可以直接发起再来一局。',
      fanTotal: null,
      winnerSeat: null,
      discarderSeat: null,
      winType: null,
      winTypeLabel: null,
      provisional: false,
      flowerCount: 0,
      fanBreakdown: [],
      scoreDeltaBySeat: {},
      seats: createResultSeats(state, null),
      continueAction: {
        id: 'restart_match',
        label: !isConnectionInteractive
          ? '重连中...'
          : restartMatchConfirmation?.countdownDeadlineAt
            ? formatContinueActionCountdownLabel(restartMatchConfirmation.countdownDeadlineAt)
            : restartMatchConfirmation?.isLocalConfirmed
              ? formatContinueActionConfirmedLabel(
                  restartMatchConfirmation.confirmedCount,
                  restartMatchConfirmation.requiredCount,
                )
              : ACTION_LABELS.restart_match,
        enabled:
          isConnectionInteractive &&
          snapshot.match_state?.match_finished === true &&
          typeof snapshot.local_seat === 'number' &&
          !restartMatchConfirmation?.isLocalConfirmed,
        countdownDeadlineAt: restartMatchConfirmation?.countdownDeadlineAt ?? undefined,
        confirmation: restartMatchConfirmation ?? undefined,
      },
    };
  }

  return null;
}

function createLastDiscardSeat(state: SessionState): Seat | null {
  const optimisticDiscard = getOptimisticDiscard(state);
  if (optimisticDiscard) {
    return toRelativeSeat(getLocalSeat(state), optimisticDiscard.seatIndex);
  }

  const snapshot = state.roomSnapshot?.payload;
  const privateState = snapshot?.private_state;
  const result = state.latestMatchResult?.payload;
  const lastDiscard = privateState?.last_discard ?? null;

  if (!snapshot || !privateState || typeof lastDiscard !== 'string' || lastDiscard.trim().length === 0) {
    return null;
  }

  const localSeat = getLocalSeat(state);

  if (
    snapshot.phase === 'settlement' &&
    result?.win_type === 'discard' &&
    typeof result.discarder_seat === 'number'
  ) {
    return toRelativeSeat(localSeat, result.discarder_seat);
  }

  const latestRoundEvent = state.latestRoundEvent;
  const latestDiscardEvent =
    latestRoundEvent?.payload.event_type === 'tile_discarded' ? latestRoundEvent.payload.event : null;
  const eventSeat = latestDiscardEvent?.seat;
  const eventTileId = latestDiscardEvent?.tile_id;

  if (typeof eventSeat === 'number' && typeof eventTileId === 'string') {
    const [eventTileCode] = eventTileId.split('#');

    if (eventTileCode?.trim().toLowerCase() === lastDiscard.trim().toLowerCase()) {
      return toRelativeSeat(localSeat, eventSeat);
    }
  }

  if (
    privateState.pending_action?.type === 'claim_window' &&
    typeof privateState.pending_action.discarder_seat === 'number'
  ) {
    return toRelativeSeat(localSeat, privateState.pending_action.discarder_seat);
  }

  return toRelativeSeat(localSeat, (privateState.current_actor + 3) % 4);
}

function createShouldAutoReturnLastDiscardToRiver(state: SessionState) {
  if (hasOptimisticDiscardPending(state)) {
    return false;
  }

  const snapshot = state.roomSnapshot?.payload;
  const privateState = snapshot?.private_state;

  if (snapshot?.phase !== 'playing' || !privateState?.last_discard) {
    return false;
  }

  return privateState.pending_action?.type !== 'claim_window';
}

function createRoundLabel(state: SessionState) {
  const snapshot = state.roomSnapshot?.payload;
  const matchState = snapshot?.match_state;
  const roundWind = snapshot?.private_state?.round_wind ?? matchState?.prevailing_wind;

  if (matchState && roundWind) {
    return `${WIND_COPY[roundWind]}${matchState.hand_number}局`;
  }

  return snapshot?.private_state?.round_id ?? '等待牌桌';
}

function createScoreSummaryLabel(state: SessionState) {
  const snapshot = state.roomSnapshot?.payload;
  const localSeat = snapshot?.local_seat;
  if (typeof localSeat !== 'number') {
    return '等待分数同步';
  }

  const seatKey = String(localSeat);
  const score = getDisplayedScores(state)[seatKey];
  const liveDelta = getLiveDeltaBySeat(state)[seatKey] ?? 0;

  if (typeof score !== 'number') {
    return '等待分数同步';
  }

  return `总分 ${score}${liveDelta !== 0 ? ` · 本局 ${formatSignedNumber(liveDelta)}` : ''}`;
}

function createCenterBanner(state: SessionState) {
  const snapshot = state.roomSnapshot?.payload;
  if (!snapshot) {
    return null;
  }

  if (snapshot.phase === 'settlement') {
    return '本局结算';
  }

  if (snapshot.phase === 'finished') {
    return '整场结束';
  }

  if (snapshot.phase === 'waiting') {
    return '等待牌手加入';
  }

  return snapshot.private_state?.last_discard ?? null;
}

function createCenterStatusText(state: SessionState) {
  return null;
}

function createRemainingTileCount(state: SessionState) {
  const remaining = state.roomSnapshot?.payload.private_state?.wall_tiles_remaining;
  return typeof remaining === 'number' ? remaining : null;
}

function createActionIndicatorSeat(state: SessionState): Seat | null {
  if (hasOptimisticDiscardPending(state)) {
    return null;
  }

  const snapshot = state.roomSnapshot?.payload;
  const privateState = snapshot?.private_state;

  if (snapshot?.phase !== 'playing' || !privateState) {
    return null;
  }

  const localSeat = getLocalSeat(state);
  const pendingAction = privateState.pending_action;

  if (pendingAction?.type === 'claim_window') {
    return null;
  }

  if (pendingAction?.type === 'rob_kong_window' && typeof pendingAction.actor_seat === 'number') {
    return toRelativeSeat(localSeat, pendingAction.actor_seat);
  }

  if (pendingAction?.type === 'active_turn' && typeof pendingAction.seat_index === 'number') {
    return toRelativeSeat(localSeat, pendingAction.seat_index);
  }

  if (typeof privateState.current_actor === 'number') {
    return toRelativeSeat(localSeat, privateState.current_actor);
  }

  return null;
}

function createActionEffect(state: SessionState): ActionEffectView | null {
  const optimisticDiscard = getOptimisticDiscard(state);
  if (optimisticDiscard) {
    return {
      key: optimisticDiscard.actionEffectKey,
      label: optimisticDiscard.actionType === 'ready_hand' ? '听' : '出牌',
      emphasis: optimisticDiscard.actionType === 'ready_hand' ? 'claim' : 'discard',
      seat: toRelativeSeat(getLocalSeat(state), optimisticDiscard.seatIndex),
      calloutTone: optimisticDiscard.actionType === 'ready_hand' ? 'ready_hand' : null,
    };
  }

  const snapshot = state.roomSnapshot?.payload;
  const event = state.latestRoundEvent?.payload;

  if (!snapshot || !event) {
    return null;
  }

  const localSeat = getLocalSeat(state);
  const seatValue = event.event?.seat;
  const effectSeat = typeof seatValue === 'number' ? toRelativeSeat(localSeat, seatValue) : null;
  const key = `${event.event_type}-${JSON.stringify(event.event)}`;

  if (event.event_type === 'tile_drawn') {
    return {
      key,
      label: '摸牌',
      emphasis: 'draw',
      seat: effectSeat,
      calloutTone: null,
    };
  }

  if (event.event_type === 'flower_exposed') {
    return {
      key,
      label: '补花',
      emphasis: 'draw',
      seat: effectSeat,
      calloutTone: null,
    };
  }

  if (event.event_type === 'replacement_draw') {
    return {
      key,
      label: '补牌',
      emphasis: 'draw',
      seat: effectSeat,
      calloutTone: null,
    };
  }

  if (event.event_type === 'tile_discarded') {
    return {
      key,
      label: '出牌',
      emphasis: 'discard',
      seat: effectSeat,
      calloutTone: null,
    };
  }

  if (event.event_type === 'ready_hand_declared') {
    return {
      key,
      label: '听',
      emphasis: 'claim',
      seat: effectSeat,
      calloutTone: 'ready_hand',
    };
  }

  if (event.event_type === 'claim_made') {
    const claimType = String(event.event?.claim_type ?? '');
    return {
      key,
      label: ACTION_EFFECT_LABELS[claimType] ?? '响应',
      emphasis: 'claim',
      seat: effectSeat,
      calloutTone: resolveActionEffectCalloutTone(claimType),
    };
  }

  if (event.event_type === 'self_hu_declared') {
    return {
      key,
      label: '和',
      emphasis: 'claim',
      seat: effectSeat,
      calloutTone: 'hu',
    };
  }

  if (event.event_type === 'self_kong_declared') {
    const kongType = String(event.event?.kong_type ?? '');
    return {
      key,
      label: KONG_EFFECT_LABELS[kongType] ?? '杠',
      emphasis: 'kong',
      seat: effectSeat,
      calloutTone: 'kong',
    };
  }

  if (event.event_type === 'round_drawn') {
    return {
      key,
      label: '流局',
      emphasis: 'system',
      seat: null,
      calloutTone: null,
    };
  }

  if (event.event_type === 'settlement_ready') {
    return {
      key,
      label: '结算',
      emphasis: 'system',
      seat: null,
      calloutTone: null,
    };
  }

  return null;
}

function getWinTypeLabel(result: MatchResultPayload) {
  return result.display_win_label ?? WIN_TYPE_LABELS[result.win_type] ?? result.win_type;
}

export function createMatchViewModel(state: SessionState, options: MatchViewModelOptions = {}): BattleViewModel {
  const snapshot = state.roomSnapshot?.payload;
  const optimisticDiscard = getOptimisticDiscard(state);
  const waitingControls = createWaitingControls(state);
  const isWaiting = snapshot?.phase === 'waiting';
  const isReconnecting = state.connectionStatus === 'reconnecting';
  const isSettlement = snapshot?.phase === 'settlement';
  const isFinished = snapshot?.phase === 'finished';
  const localSeat = getLocalSeat(state);
  const activePlayerSeatIndex = getCurrentActorSeatIndex(snapshot?.private_state);
  const activePlayerSeat =
    typeof activePlayerSeatIndex === 'number' ? toRelativeSeat(localSeat, activePlayerSeatIndex) : 'bottom';
  const deadlineAt = createCenterDeadlineAt(state);
  const promptCue = createPromptCue(state, options);
  const actionIndicatorSeat = createActionIndicatorSeat(state);
  const mode = !snapshot
    ? 'loading'
    : isFinished
      ? 'finished'
      : isReconnecting || isWaiting
        ? 'disconnected_or_waiting'
        : isSettlement
          ? 'resolving'
          : optimisticDiscard
            ? 'watching'
          : snapshot.private_state?.pending_action?.type === 'active_turn' &&
              snapshot.private_state.pending_action.seat_index === localSeat
            ? 'my_turn'
            : 'watching';

  return {
    roomMode: snapshot?.mode ?? 'normal',
    mode,
    tableCode: snapshot?.table_code ?? state.tableCode,
    canLeaveTable: Boolean(snapshot),
    phaseLabel: snapshot ? PHASE_LABELS[snapshot.phase] : PHASE_LABELS.waiting,
    roundLabel: createRoundLabel(state),
    scoreSummaryLabel: createScoreSummaryLabel(state),
    deadlineAt,
    topStatusLabel: isReconnecting
      ? '正在重连'
      : snapshot?.phase === 'finished'
        ? '等待再来一局'
        : snapshot?.phase === 'settlement'
          ? '结算中'
          : snapshot?.phase === 'waiting'
            ? '等待牌手'
            : '对局中',
    activePlayerSeat,
    actionIndicatorSeat,
    isActionDockElevated: mode === 'my_turn' || Boolean(promptCue?.isUrgent),
    players: createPlayers(state),
    actions: createActionViews(state, waitingControls, options),
    waitingControls,
    discards: createDiscards(state),
    selectedTileCode: createSelectedTileCode(state),
    localHand: createLocalHand(state),
    readyHandInsight: createReadyHandInsight(state),
    claimCandidates: createClaimCandidates(state),
    drawnTileId: createDrawnTileId(state),
    centerBanner: createCenterBanner(state),
    centerStatusText: createCenterStatusText(state),
    remainingTileCount: createRemainingTileCount(state),
    promptText: createPromptText(state, options),
    promptCue,
    result: createResult(state),
    settlementHands: createSettlementHands(state),
    lastDiscard: optimisticDiscard?.tileCode ?? snapshot?.private_state?.last_discard ?? null,
    lastDiscardSeat: createLastDiscardSeat(state),
    shouldAutoReturnLastDiscardToRiver: createShouldAutoReturnLastDiscardToRiver(state),
    actionEffect: createActionEffect(state),
    quickChatEvent: createQuickChatEvent(state),
    toasts: state.toasts,
  };
}

const WIND_COPY: Record<string, string> = {
  east: '东',
  south: '南',
  west: '西',
  north: '北',
};

const WIN_TYPE_LABELS: Record<string, string> = {
  discard: '荣和',
  self_draw: '自摸',
  draw: '流局',
};

const ACTION_EFFECT_LABELS: Record<string, string> = {
  chow: '吃',
  pung: '碰',
  kong: '明杠',
  hu: '胡牌',
};

const KONG_EFFECT_LABELS: Record<string, string> = {
  concealed_kong: '暗杠',
  add_kong: '补杠',
};

const PROMPT_SEAT_COPY: Record<Seat, string> = {
  bottom: '你',
  left: '左家',
  top: '对家',
  right: '右家',
};

function resolveActionEffectCalloutTone(claimType: string): ActionEffectView['calloutTone'] {
  if (
    claimType === 'chow' ||
    claimType === 'pung' ||
    claimType === 'kong' ||
    claimType === 'hu' ||
    claimType === 'ready_hand'
  ) {
    return claimType;
  }

  return null;
}

function createQuickChatEvent(state: SessionState): QuickChatEventView | null {
  const snapshot = state.roomSnapshot?.payload;
  const message = state.latestQuickChatMessage;

  if (!snapshot || !message) {
    return null;
  }

  const localSeat = getLocalSeat(state);
  const actorSeat = toRelativeSeat(localSeat, message.payload.actor_seat);
  const targetSeat = toRelativeSeat(localSeat, message.payload.target_seat);
  const actorName = getSeatName(state, message.payload.actor_seat);
  const targetName = getSeatName(state, message.payload.target_seat);

  return {
    key: message.payload.message_id,
    actorSeat,
    targetSeat,
    actorName,
    targetName,
    emoji: message.payload.emoji,
    text:
      message.payload.actor_seat === message.payload.target_seat
        ? `${actorName}：${message.payload.emoji}`
        : `${actorName} -> ${targetName} : ${message.payload.emoji}`,
  };
}

function createContinueActionConfirmation(
  state: SessionState,
  actionId: Extract<BattleActionId, 'start_next_round' | 'restart_match'>,
) {
  const snapshot = state.roomSnapshot?.payload;
  const continueAction = snapshot?.continue_action;
  const localSeat = snapshot?.local_seat;

  if (!continueAction || continueAction.action_id !== actionId) {
    return null;
  }

  const confirmedSeats = Array.isArray(continueAction.confirmed_seats) ? continueAction.confirmed_seats : [];
  const requiredSeats = Array.isArray(continueAction.required_seats) ? continueAction.required_seats : [];
  const onlineSeats = Array.isArray(continueAction.online_seats) ? continueAction.online_seats : [];
  const occupiedHumanSeatCount = Array.isArray(snapshot?.seats)
    ? snapshot.seats.filter((seat) => seat.is_bot !== true).length
    : 0;
  const countdownDeadlineAt =
    typeof continueAction.auto_advance_deadline_at === 'string' ? continueAction.auto_advance_deadline_at : null;
  const requiredCount =
    requiredSeats.length > 0
      ? requiredSeats.length
      : onlineSeats.length > 0
        ? onlineSeats.length
        : occupiedHumanSeatCount;

  return {
    confirmedCount: confirmedSeats.length,
    requiredCount,
    isLocalConfirmed: typeof localSeat === 'number' && confirmedSeats.includes(localSeat),
    countdownDeadlineAt,
  };
}

function getStartNextRoundLabel(state: SessionState) {
  const snapshot = state.roomSnapshot?.payload;
  const roundWind = snapshot?.private_state?.round_wind ?? snapshot?.match_state?.prevailing_wind;
  const handNumber = snapshot?.match_state?.hand_number;

  if (roundWind === 'north' && handNumber === 4) {
    return '查看最终得分';
  }

  return ACTION_LABELS.start_next_round;
}

function formatContinueActionConfirmedLabel(confirmedCount: number, requiredCount: number) {
  return `已确认 ${confirmedCount}/${requiredCount}`;
}

function formatContinueActionCountdownLabel(deadlineAt: string) {
  const remainingSeconds = Math.max(0, Math.ceil((new Date(deadlineAt).getTime() - Date.now()) / 1000));
  return `${remainingSeconds}s后自动推进`;
}
