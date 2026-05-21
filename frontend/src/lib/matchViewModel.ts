import type {
  ActionEffectView,
  BackendActionType,
  BackendHandInsight,
  BattleActionId,
  BattlePromptView,
  BattleActionView,
  BattleViewModel,
  ClaimActionId,
  ClaimCandidateTileView,
  ClaimCandidateView,
  DealerSelectionView,
  DisplayMeldView,
  MatchResultPayload,
  MinimumHuFan,
  PlayerView,
  PrivatePlayerState,
  PrivateState,
  QuickChatEventView,
  ResultPageView,
  ResultSeatView,
  Seat,
  SeatSnapshot,
  SeatType,
  SessionState,
  TableSettingsView,
  HandInsightView,
  WaitingControls,
} from '../types/match';
import {
  getActionCandidateGroups,
  getFlowerCandidateTileIds,
  getOptimisticFlowerTileId,
  getLocalTurnKongCandidateGroups,
  isFlowerTileKey,
} from './kongSelection';

const RELATIVE_SEATS: Seat[] = ['bottom', 'right', 'top', 'left'];
const WINDS: PlayerView['wind'][] = ['East', 'South', 'West', 'North'];
const ACTION_ORDER: BattleActionId[] = [
  'start_match',
  'start_next_round',
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
  invite: '邀请',
  start_match: '开始对局',
  start_next_round: '下一局',
  match_decided: '大局已定',
  discard: '出牌',
  ready_hand: '听',
  flower: '补花',
  kong: '杠',
  hu: '和牌',
  chow: '吃',
  pung: '碰',
  pass: '过',
} as const satisfies Record<BattleActionId, string>;

const MINIMUM_HU_FAN_OPTIONS: MinimumHuFan[] = [0, 2, 4, 6, 8];

function normalizeMinimumHuFan(value: number | undefined): MinimumHuFan {
  return MINIMUM_HU_FAN_OPTIONS.includes(value as MinimumHuFan) ? (value as MinimumHuFan) : 8;
}

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

function getExtendedWithExtra(state: SessionState): boolean {
  const snapshot = state.roomSnapshot?.payload;
  const pendingAction = snapshot?.private_state?.pending_action;
  if (pendingAction && 'extended_with_extra' in pendingAction && typeof pendingAction.extended_with_extra === 'boolean') {
    return pendingAction.extended_with_extra;
  }
  return false;
}

function createCenterDeadlineAt(state: SessionState, options: MatchViewModelOptions = {}) {
  const snapshot = state.roomSnapshot?.payload;
  const pendingAction = snapshot?.private_state?.pending_action;

  if (options.hideLocalClaimPrompt) {
    if (pendingAction?.type === 'claim_window' || pendingAction?.type === 'rob_kong_window') {
      return null;
    }

    if (
      state.latestActionPrompt?.payload.options.some(
        (option) => option === 'hu' || option === 'chow' || option === 'pung' || option === 'kong',
      )
    ) {
      return null;
    }
  }

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
  showLocalSelfHuPassOption?: boolean;
  hideLocalSelfHuPrompt?: boolean;
  hideLocalClaimPrompt?: boolean;
}

function getLocalSeat(state: SessionState): number {
  return state.roomSnapshot?.payload.local_seat ?? 0;
}

function getPerspectiveSeat(state: SessionState, options: MatchViewModelOptions = {}): number {
  return getLocalSeat(state);
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
  const localPromptOptions = getLocalPromptOptions(state, options);

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
      const promptOptions =
        actorSeat === getLocalSeat(state)
          ? localPromptOptions
          : getPendingActionOptions(pendingAction as { options?: unknown });
      return createActorPrompt(getSeatName(state, actorSeat), promptOptions.length > 0 ? promptOptions : ['discard']);
    }
    if (pendingAction.type === 'claim_window') {
      if (options.hideLocalClaimPrompt) {
        return null;
      }
      const claimLabels = getPendingActionOptions(pendingAction as { options?: unknown });
      return createActorPrompt('一名玩家', claimLabels.length > 0 ? claimLabels : ['chow', 'pung', 'kong', 'hu']);
    }
    if (pendingAction.type === 'rob_kong_window') {
      if (options.hideLocalClaimPrompt) {
        return null;
      }
      const robKongOptions = getPendingActionOptions(pendingAction as { options?: unknown });
      return createActorPrompt('一名玩家', robKongOptions.length > 0 ? robKongOptions : ['hu']);
    }
  }

  if (state.latestActionPrompt) {
    if (
      options.hideLocalClaimPrompt &&
      state.latestActionPrompt.payload.options.some(
        (option) => option === 'hu' || option === 'chow' || option === 'pung' || option === 'kong',
      )
    ) {
      return null;
    }

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
    return '整场对局已结束';
  }

  return null;
}

function getPromptSourceSeatLabel(seat: Seat | null) {
  if (!seat) {
    return '当前牌局';
  }

  return PROMPT_SEAT_COPY[seat];
}

function getLocalPromptOptions(state: SessionState, viewOptions: MatchViewModelOptions = {}): BackendActionType[] {
  if (viewOptions.hideLocalClaimPrompt) {
    return [];
  }

  const localSeat = getLocalSeat(state);
  const pendingAction = state.roomSnapshot?.payload.private_state?.pending_action;

  if (pendingAction && 'options' in pendingAction) {
    const rawOptions = (pendingAction as { options?: unknown }).options;
    if (Array.isArray(rawOptions)) {
      return normalizeLocalSelfHuPromptOptions(state, rawOptions.filter(isBackendActionType), viewOptions);
    }
  }

  if (state.latestActionPrompt?.payload.seat_index === localSeat) {
    return normalizeLocalSelfHuPromptOptions(
      state,
      state.latestActionPrompt.payload.options.filter(isBackendActionType),
      viewOptions,
    );
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
  const localPromptOptions = orderPromptActions(getLocalPromptOptions(state, options));
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

  if (!options.hideLocalClaimPrompt && pendingAction?.type === 'claim_window' && highlightedActionIds.length > 0) {
    const sourceSeat =
      typeof pendingAction.discarder_seat === 'number' ? toRelativeSeat(localSeat, pendingAction.discarder_seat) : createLastDiscardSeat(state, options);

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

function getSeatIdentityType(seat: Pick<SeatSnapshot, 'seat_type' | 'nickname'>): SeatType {
  if (seat.seat_type === 'bot' || seat.seat_type === 'human' || seat.seat_type === 'special_bot') {
    return seat.seat_type;
  }

  return typeof seat.nickname === 'string' && /^bot\b/i.test(seat.nickname) ? 'bot' : 'human';
}

function canLeaveTable(snapshot: SessionState['roomSnapshot']) {
  const payload = snapshot?.payload;
  if (!payload) {
    return false;
  }

  if (payload.phase !== 'waiting') {
    return typeof payload.local_seat === 'number';
  }

  const localSeatIndex = payload.local_seat;
  return payload.seats.some((seat) => {
    if (typeof localSeatIndex === 'number' && seat.seat_index === localSeatIndex) {
      return false;
    }

    return getSeatIdentityType(seat) !== 'bot';
  });
}

function createWaitingControls(state: SessionState, options: MatchViewModelOptions = {}): WaitingControls | null {
  const snapshot = state.roomSnapshot?.payload;
  if (!snapshot || snapshot.phase !== 'waiting') {
    return null;
  }
  const isEvaluationRoom = snapshot.mode === 'evaluation';

  const localSeat = snapshot.local_seat;
  const localSeatState = typeof localSeat === 'number' ? snapshot.seats.find((seat) => seat.seat_index === localSeat) : null;
  const occupiedSeats = snapshot.seats.length;
  const botCount = snapshot.seats.filter((seat) => getSeatIdentityType(seat) === 'bot').length;
  const minimumHuFan = normalizeMinimumHuFan(snapshot.minimum_hu_fan);
  const minimumHuFanIndex = MINIMUM_HU_FAN_OPTIONS.indexOf(minimumHuFan);
  const dealerSelection = createDealerSelection(state, options);
  const allSeatsAvailable =
    occupiedSeats === 4 &&
    snapshot.seats.every((seat) => seat.connected || seat.is_bot);

  return {
    canStart: allSeatsAvailable && !dealerSelection,
    occupiedSeats,
    botCount,
    canAddBot: Boolean(localSeatState) && occupiedSeats < 4 && !dealerSelection && !isEvaluationRoom,
    canRemoveBot: Boolean(localSeatState) && botCount > 0 && !dealerSelection && !isEvaluationRoom,
    minimumHuFan,
    canDecreaseMinimumHuFan:
      Boolean(localSeatState) && minimumHuFanIndex > 0 && !dealerSelection && !isEvaluationRoom,
    canIncreaseMinimumHuFan:
      Boolean(localSeatState) &&
      minimumHuFanIndex < MINIMUM_HU_FAN_OPTIONS.length - 1 &&
      !dealerSelection &&
      !isEvaluationRoom,
    dealerRepeatEnabled: snapshot.dealer_repeat_enabled ?? false,
    dealerDoubleEnabled: snapshot.dealer_double_enabled ?? false,
    canToggleDealerRepeat: Boolean(localSeatState) && !dealerSelection && !isEvaluationRoom,
    canToggleDealerDouble: Boolean(localSeatState) && !dealerSelection && !isEvaluationRoom,
  };
}

function createTableSettings(state: SessionState): TableSettingsView {
  const snapshot = state.roomSnapshot?.payload;

  return {
    minimumHuFan: normalizeMinimumHuFan(snapshot?.minimum_hu_fan),
    dealerRepeatEnabled: snapshot?.dealer_repeat_enabled ?? false,
    dealerDoubleEnabled: snapshot?.dealer_double_enabled ?? false,
  };
}

function getRawPromptOptions(state: SessionState): BackendActionType[] {
  const pendingAction = state.roomSnapshot?.payload.private_state?.pending_action;
  if (pendingAction && 'options' in pendingAction) {
    const options = (pendingAction as { options?: unknown }).options;
    if (Array.isArray(options)) {
      return options.filter(isBackendActionType);
    }
  }

  return (state.latestActionPrompt?.payload.options ?? []).filter(isBackendActionType);
}

function getPromptOptions(state: SessionState, options: MatchViewModelOptions = {}): BackendActionType[] {
  if (options.hideLocalClaimPrompt) {
    return [];
  }

  return normalizeLocalSelfHuPromptOptions(state, getRawPromptOptions(state), options);
}

function isLocalSelfHuPrompt(state: SessionState) {
  const snapshot = state.roomSnapshot?.payload;
  const localSeat = snapshot?.local_seat;
  const pendingAction = snapshot?.private_state?.pending_action;

  return (
    typeof localSeat === 'number' &&
    snapshot?.phase === 'playing' &&
    pendingAction?.type === 'active_turn' &&
    pendingAction.seat_index === localSeat &&
    getRawPromptOptions(state).includes('hu')
  );
}

function normalizeLocalSelfHuPromptOptions(
  state: SessionState,
  promptOptions: BackendActionType[],
  options: MatchViewModelOptions = {},
): BackendActionType[] {
  if (!isLocalSelfHuPrompt(state)) {
    return promptOptions;
  }

  let nextOptions = promptOptions;
  if (options.hideLocalSelfHuPrompt) {
    nextOptions = nextOptions.filter((option) => option !== 'hu' && option !== 'pass');
  }

  if (options.showLocalSelfHuPassOption && nextOptions.includes('hu') && !nextOptions.includes('pass')) {
    nextOptions = [...nextOptions, 'pass'];
  }

  return nextOptions;
}

export function getLocalSelfHuPromptSignature(state: SessionState): string | null {
  const snapshot = state.roomSnapshot?.payload;
  const pendingAction = snapshot?.private_state?.pending_action;

  if (!snapshot?.private_state || !isLocalSelfHuPrompt(state) || pendingAction?.type !== 'active_turn') {
    return null;
  }

  return [
    'self-hu',
    snapshot.private_state.round_id,
    pendingAction.seat_index,
    pendingAction.deadline_at,
    pendingAction.drawn_tile_id ?? '',
  ].join(':');
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
  const promptOptions = new Set<BackendActionType>(getPromptOptions(state, options));
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
  const selectedReadyHandPreview =
    selectedReadyHandTileId && !restrictedDiscardTileIdSet.has(selectedReadyHandTileId)
      ? state.roomSnapshot?.payload.private_state?.hand_insights?.by_discard_tile_id[selectedReadyHandTileId] ?? null
      : null;
  const canReadyHandFromSelection =
    !localReadyHandLocked &&
    Boolean(selectedReadyHandPreview?.is_tenpai);
  const canContinueRound = snapshot?.phase === 'settlement' && typeof snapshot.local_seat === 'number';

  return ACTION_ORDER.map((id) => {
    let enabled = false;

    if (id === 'start_match') {
      enabled = waitingControls?.canStart ?? false;
    } else if (id === 'start_next_round') {
      enabled =
        canContinueRound &&
        !isFinalSettlement(state) &&
        !nextRoundConfirmation?.isLocalConfirmed;
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
      id === 'start_match' || id === 'start_next_round' || (id === 'discard' && enabled)
        ? 'high'
        : enabled
          ? 'medium'
          : 'low';
    return {
      id,
      label: getActionLabel(state, id, waitingControls, {
        startNextRound: nextRoundConfirmation,
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
  },
) {
  const confirmation = id === 'start_next_round' ? continueActionConfirmations?.startNextRound : null;
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

function createPlayers(state: SessionState, options: MatchViewModelOptions = {}): PlayerView[] {
  const snapshot = state.roomSnapshot?.payload;
  if (!snapshot) {
    return [];
  }

  const localSeat = getPerspectiveSeat(state, options);
  const ownSeat = snapshot.local_seat;
  const currentActor = getCurrentActorSeatIndex(snapshot.private_state);
  const dealerSeat = snapshot.match_state?.dealer_seat ?? snapshot.private_state?.dealer_seat;
  const displayedScores = getDisplayedScores(state);
  const liveDeltaBySeat = getLiveDeltaBySeat(state);
  const flowerCountBySeat = getFlowerCountBySeat(state);
  const dealerSelection = createDealerSelection(state, options);

  return snapshot.seats
    .map((seat) => {
      const relativeSeat = toRelativeSeat(localSeat, seat.seat_index);
      const privatePlayer = findPrivatePlayer(state, seat.seat_index);
      const seatKey = String(seat.seat_index);
      const seatType = getSeatIdentityType(seat);
      const isBotSeat = seatType === 'bot';
      const isBotControlled = Boolean(seat.is_bot);

      return {
        seat: relativeSeat,
        absoluteSeat: seat.seat_index,
        userId: seat.user_id ?? null,
        name: seat.nickname,
        title: privatePlayer?.title ?? seat.title ?? null,
        seatType,
        score: displayedScores[seatKey] ?? 0,
        points: seat.points ?? 0,
        liveDelta: liveDeltaBySeat[seatKey] ?? 0,
        flowerCount: flowerCountBySeat[seatKey] ?? privatePlayer?.flowers.length ?? 0,
        wind: getWindForSeat(seat.seat_index, dealerSeat),
        isDealer: dealerSeat === seat.seat_index,
        isActive: currentActor === seat.seat_index,
        isLocal: typeof ownSeat === 'number' && seat.seat_index === ownSeat,
        connected: seat.connected,
        isBotControlled,
        isReadyHand: Boolean(privatePlayer?.is_ready_hand),
        concealedCount: privatePlayer?.concealed_count ?? 0,
        meldCount: privatePlayer?.melds.length ?? 0,
        melds: privatePlayer?.display_melds ?? normalizeDisplayMelds(privatePlayer?.melds),
        flowers: normalizeTileCodeList(privatePlayer?.flowers),
        statusText:
          isBotControlled
            ? 'Bot代打中'
            : !seat.connected
              ? '等待重连中'
              : snapshot.phase === 'waiting' && dealerSelection
                ? '抽座中'
              : snapshot.phase === 'waiting'
                ? '等待中'
                : snapshot.phase === 'settlement'
                  ? '等待下一局'
                  : snapshot.phase === 'finished'
                    ? '整场完成'
                    : '对局中',
      };
    })
    .sort((left, right) => RELATIVE_SEATS.indexOf(left.seat) - RELATIVE_SEATS.indexOf(right.seat));
}

function createDiscards(state: SessionState, options: MatchViewModelOptions = {}): Record<Seat, string[]> {
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

  const localSeat = getPerspectiveSeat(state, options);
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

function createSelectedTileCode(state: SessionState, options: MatchViewModelOptions = {}) {
  if (state.selectionMode !== 'single' || state.selectedTileIds.length !== 1) {
    return null;
  }

  const localSeat = getPerspectiveSeat(state, options);
  const localPlayer = findPrivatePlayer(state, localSeat);
  const selectedTileId = state.selectedTileIds[0];
  const selectedTile = (localPlayer?.concealed_tiles ?? []).find((tile) => tile.tile_id === selectedTileId);

  return selectedTile?.tile_key ?? null;
}

function createLocalHand(state: SessionState, options: MatchViewModelOptions = {}) {
  const localSeat = getPerspectiveSeat(state, options);
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

function mapBackendHandInsight(
  insight: BackendHandInsight,
  source: HandInsightView['source'],
): HandInsightView {
  return {
    source,
    discardTileId: insight.discard_tile_id,
    discardTileCode: insight.discard_tile_code,
    isTenpai: insight.is_tenpai,
    waits: insight.waits.map((wait) => ({
      code: wait.code,
      availableCount: wait.available_count,
    })),
    winningFans: insight.winning_fans.map((item) => ({
      fanKey: item.fan_key,
      fanValue: item.fan_value,
    })),
  };
}

function createHandInsight(state: SessionState): BattleViewModel['handInsight'] {
  if (hasOptimisticDiscardPending(state)) {
    return null;
  }

  const handInsights = state.roomSnapshot?.payload.private_state?.hand_insights;
  if (!handInsights) {
    return null;
  }

  const selectedTileId = state.selectedTileIds.length === 1 ? state.selectedTileIds[0] : null;
  if (selectedTileId) {
    const preview = handInsights.by_discard_tile_id[selectedTileId];
    if (preview?.is_tenpai) {
      return mapBackendHandInsight(preview, 'selected_discard');
    }
  }

  if (handInsights.current?.is_tenpai || handInsights.current?.winning_fans.length) {
    return mapBackendHandInsight(handInsights.current, 'current');
  }

  return null;
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

export function createClaimCandidates(state: SessionState, options: MatchViewModelOptions = {}): ClaimCandidateView[] {
  if (hasOptimisticDiscardPending(state) || options.hideLocalClaimPrompt) {
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

function createSettlementHands(state: SessionState, options: MatchViewModelOptions = {}): BattleViewModel['settlementHands'] {
  const snapshot = state.roomSnapshot?.payload;
  const privateState = snapshot?.private_state;
  const settlementWinningDiscard = getSettlementWinningDiscard(state, options);

  if (
    !snapshot ||
    !privateState ||
    (snapshot.phase !== 'settlement' && !(snapshot.phase === 'finished' && state.latestMatchResult))
  ) {
    return null;
  }

  const localSeat = getPerspectiveSeat(state, options);
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

function getSettlementWinningDiscard(
  state: SessionState,
  options: MatchViewModelOptions = {},
): { winnerSeats: Set<Seat>; tileCode: string } | null {
  const snapshot = state.roomSnapshot?.payload;
  const privateState = snapshot?.private_state;
  const result = state.latestMatchResult?.payload;
  const localSeat = getPerspectiveSeat(state, options);
  const winnerSeats = new Set(
    (result?.winning_details ?? [])
      .map((detail) =>
        typeof detail.winner_seat === 'number' ? toRelativeSeat(localSeat, detail.winner_seat) : null,
      )
      .filter((seat): seat is Seat => seat !== null),
  );

  if (
    !snapshot ||
    (snapshot.phase !== 'settlement' && snapshot.phase !== 'finished') ||
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
      winnerAbsoluteSeat: detail.winner_seat,
      discarderSeat: typeof result.discarder_seat === 'number' ? toRelativeSeat(localSeat, result.discarder_seat) : null,
      discarderAbsoluteSeat: typeof result.discarder_seat === 'number' ? result.discarder_seat : null,
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

function createPlayerDisplayLabel(name: string, title?: string | null) {
  const normalizedTitle = title?.trim();
  return normalizedTitle ? `${name}（${normalizedTitle}）` : name;
}

function createResultSeatIdentity(state: SessionState, seat: SeatSnapshot) {
  const snapshot = state.roomSnapshot?.payload;
  const privatePlayer = snapshot?.phase === 'finished' ? findPrivatePlayer(state, seat.seat_index) : null;
  const name = privatePlayer?.nickname ?? seat.nickname;
  const title = privatePlayer?.title ?? seat.title ?? null;

  return { name, title };
}

function createResultSeats(
  state: SessionState,
  scoreDeltaBySeat: Record<string, number> | null,
  options: MatchViewModelOptions = {},
): ResultSeatView[] {
  const snapshot = state.roomSnapshot?.payload;
  if (!snapshot) {
    return [];
  }

  const localSeat = getPerspectiveSeat(state, options);
  const scores = getDisplayedScores(state);

  return snapshot.seats
    .map((seat) => {
      const seatKey = String(seat.seat_index);
      const identity = createResultSeatIdentity(state, seat);
      return {
        seat: toRelativeSeat(localSeat, seat.seat_index),
        absoluteSeat: seat.seat_index,
        name: identity.name,
        title: identity.title,
        displayLabel: createPlayerDisplayLabel(identity.name, identity.title),
        score: scores[seatKey] ?? 0,
        delta: scoreDeltaBySeat ? (scoreDeltaBySeat[seatKey] ?? 0) : null,
        stats: createResultSeatStats(state, seat.seat_index, scores[seatKey] ?? 0),
      };
    })
    .sort((left, right) => right.score - left.score);
}

function createResult(state: SessionState, options: MatchViewModelOptions = {}): BattleViewModel['result'] {
  const snapshot = state.roomSnapshot?.payload;
  if (!snapshot) {
    return null;
  }

  const isConnectionInteractive = state.connectionStatus === 'connected';
  const localSeat = getPerspectiveSeat(state, options);
  const nextRoundConfirmation = createContinueActionConfirmation(state, 'start_next_round');

  if ((snapshot.phase === 'settlement' || snapshot.phase === 'finished') && state.latestMatchResult) {
    const result = state.latestMatchResult.payload;
    const isFinished = snapshot.phase === 'finished';
    const isFinalHand = isFinished || isFinalSettlement(state);
    const pages = createResultPages(result, localSeat);
    const primaryPage = pages[0] ?? null;
    const scoreDeltaBySeat: Partial<Record<Seat, number>> = {};
    for (const [seatIndex, delta] of Object.entries(result.score_delta.total_delta_by_seat)) {
      scoreDeltaBySeat[toRelativeSeat(localSeat, Number(seatIndex))] = delta;
    }

    return {
      roundId: result.round_id,
      title: isFinished ? ACTION_LABELS.match_decided : '本局结算',
      summary: isFinished ? '最终局已结算，本桌完整对局已经结束。' : createResultSummary(result, pages.length),
      fanTotal: primaryPage?.fanTotal ?? result.fan_total,
      winnerSeat:
        primaryPage?.winnerSeat ??
        (typeof result.winner_seat === 'number' ? toRelativeSeat(localSeat, result.winner_seat) : null),
      winnerAbsoluteSeat:
        primaryPage?.winnerAbsoluteSeat ??
        (typeof result.winner_seat === 'number' ? result.winner_seat : null),
      discarderSeat:
        primaryPage?.discarderSeat ??
        (typeof result.discarder_seat === 'number' ? toRelativeSeat(localSeat, result.discarder_seat) : null),
      discarderAbsoluteSeat:
        primaryPage?.discarderAbsoluteSeat ??
        (typeof result.discarder_seat === 'number' ? result.discarder_seat : null),
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
      seats: createResultSeats(state, result.score_delta.total_delta_by_seat, options),
      continueAction: isFinalHand
        ? createMatchDecidedPlaceholder()
        : createStartNextRoundContinueAction(state, nextRoundConfirmation, isConnectionInteractive),
    };
  }

  if (snapshot.phase === 'finished') {
    return {
      roundId: snapshot.match_state?.last_completed_round_id ?? null,
      title: ACTION_LABELS.match_decided,
      summary: '本桌完整对局已经结束，只能退出牌桌。',
      fanTotal: null,
      winnerSeat: null,
      discarderSeat: null,
      winType: null,
      winTypeLabel: null,
      provisional: false,
      flowerCount: 0,
      fanBreakdown: [],
      scoreDeltaBySeat: {},
      seats: createResultSeats(state, null, options),
      continueAction: createMatchDecidedPlaceholder(),
    };
  }

  return null;
}

function createLastDiscardSeat(state: SessionState, options: MatchViewModelOptions = {}): Seat | null {
  const optimisticDiscard = getOptimisticDiscard(state);
  if (optimisticDiscard) {
    return toRelativeSeat(getPerspectiveSeat(state, options), optimisticDiscard.seatIndex);
  }

  const snapshot = state.roomSnapshot?.payload;
  const privateState = snapshot?.private_state;
  const result = state.latestMatchResult?.payload;
  const lastDiscard = privateState?.last_discard ?? null;

  if (!snapshot || !privateState || typeof lastDiscard !== 'string' || lastDiscard.trim().length === 0) {
    return null;
  }

  const localSeat = getPerspectiveSeat(state, options);

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
    const baseLabel = `${WIND_COPY[roundWind]}${formatChineseCount(matchState.hand_number)}场`;
    const repeatLabel = formatDealerRepeatCount(matchState.dealer_repeat_count ?? 0);
    return repeatLabel ? `${baseLabel} | ${repeatLabel}` : baseLabel;
  }

  return snapshot?.private_state?.round_id ?? '等待牌桌';
}

function formatDealerRepeatCount(count: number) {
  if (count <= 0) {
    return null;
  }

  return `${formatChineseCount(count)}连庄`;
}

function formatChineseCount(count: number) {
  const digits = ['零', '一', '二', '三', '四', '五', '六', '七', '八', '九'];
  if (count <= 10) {
    return count === 10 ? '十' : digits[count] ?? String(count);
  }
  if (count < 20) {
    return `十${digits[count - 10]}`;
  }
  if (count < 100) {
    const tens = Math.floor(count / 10);
    const ones = count % 10;
    return `${digits[tens]}十${ones === 0 ? '' : digits[ones]}`;
  }
  return String(count);
}

function createCenterStatusText(state: SessionState) {
  return null;
}

function createRemainingTileCount(state: SessionState) {
  const remaining = state.roomSnapshot?.payload.private_state?.wall_tiles_remaining;
  return typeof remaining === 'number' ? remaining : null;
}

function getTileCodeFromEventTileId(tileId: unknown): string | null {
  if (typeof tileId !== 'string') {
    return null;
  }

  const [tileCode] = tileId.split('#');
  return tileCode?.trim().toLowerCase() || null;
}

function createActionIndicatorSeat(state: SessionState, options: MatchViewModelOptions = {}): Seat | null {
  if (hasOptimisticDiscardPending(state)) {
    return null;
  }

  const snapshot = state.roomSnapshot?.payload;
  const privateState = snapshot?.private_state;

  if (snapshot?.phase !== 'playing' || !privateState) {
    return null;
  }

  const localSeat = getPerspectiveSeat(state, options);
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

function createActionEffect(state: SessionState, options: MatchViewModelOptions = {}): ActionEffectView | null {
  return createOptimisticActionEffect(state, options) ?? createRoundEventActionEffect(state, state.latestRoundEvent, options);
}

function createActionEffects(state: SessionState, options: MatchViewModelOptions = {}): ActionEffectView[] {
  const effects = [
    createOptimisticActionEffect(state, options),
    ...(state.recentRoundEvents?.length ? state.recentRoundEvents : state.latestRoundEvent ? [state.latestRoundEvent] : [])
      .map((roundEvent) => createRoundEventActionEffect(state, roundEvent, options)),
  ].filter((effect): effect is ActionEffectView => Boolean(effect));
  const seenKeys = new Set<string>();

  return effects.filter((effect) => {
    if (seenKeys.has(effect.key)) {
      return false;
    }

    seenKeys.add(effect.key);
    return true;
  });
}

function createOptimisticActionEffect(state: SessionState, options: MatchViewModelOptions = {}): ActionEffectView | null {
  const optimisticDiscard = getOptimisticDiscard(state);
  if (optimisticDiscard) {
    return {
      key: optimisticDiscard.actionEffectKey,
      label: optimisticDiscard.actionType === 'ready_hand' ? '听' : '出牌',
      emphasis: optimisticDiscard.actionType === 'ready_hand' ? 'claim' : 'discard',
      seat: toRelativeSeat(getPerspectiveSeat(state, options), optimisticDiscard.seatIndex),
      calloutTone: optimisticDiscard.actionType === 'ready_hand' ? 'ready_hand' : null,
    };
  }

  return null;
}

function createRoundEventActionEffect(
  state: SessionState,
  roundEvent: SessionState['latestRoundEvent'],
  options: MatchViewModelOptions = {},
): ActionEffectView | null {
  const snapshot = state.roomSnapshot?.payload;
  const event = roundEvent?.payload;

  if (!snapshot || !event) {
    return null;
  }

  const localSeat = getPerspectiveSeat(state, options);
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
      tileCode: getTileCodeFromEventTileId(event.event?.tile_id),
    };
  }

  if (event.event_type === 'ready_hand_declared') {
    return {
      key,
      label: '听',
      emphasis: 'claim',
      seat: effectSeat,
      calloutTone: 'ready_hand',
      tileCode: getTileCodeFromEventTileId(event.event?.tile_id),
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

function createDealerSelection(state: SessionState, options: MatchViewModelOptions = {}): DealerSelectionView | null {
  const snapshot = state.roomSnapshot?.payload;
  const event = state.latestRoundEvent?.payload;

  if (snapshot?.phase !== 'waiting' || event?.event_type !== 'dealer_selection_started') {
    return null;
  }

  const dealerSeatIndex = event.event?.dealer_seat;
  const startedAt = event.event?.started_at;
  const revealAt = event.event?.reveal_at;
  const durationMs = event.event?.duration_ms;

  if (
    typeof dealerSeatIndex !== 'number' ||
    typeof startedAt !== 'string' ||
    typeof revealAt !== 'string'
  ) {
    return null;
  }

  return {
    key: `${dealerSeatIndex}:${startedAt}:${revealAt}`,
    dealerSeat: toRelativeSeat(getPerspectiveSeat(state, options), dealerSeatIndex),
    dealerName: getSeatName(state, dealerSeatIndex),
    startedAt,
    revealAt,
    durationMs: typeof durationMs === 'number' ? durationMs : 4_200,
  };
}

function getWinTypeLabel(result: MatchResultPayload) {
  return result.display_win_label ?? WIN_TYPE_LABELS[result.win_type] ?? result.win_type;
}

export function createMatchViewModel(state: SessionState, options: MatchViewModelOptions = {}): BattleViewModel {
  const snapshot = state.roomSnapshot?.payload;
  const optimisticDiscard = getOptimisticDiscard(state);
  const waitingControls = createWaitingControls(state, options);
  const isWaiting = snapshot?.phase === 'waiting';
  const dealerSelection = createDealerSelection(state, options);
  const isReconnecting = state.connectionStatus === 'reconnecting';
  const isSettlement = snapshot?.phase === 'settlement';
  const isFinished = snapshot?.phase === 'finished';
  const localSeat = getPerspectiveSeat(state, options);
  const activePlayerSeatIndex = getCurrentActorSeatIndex(snapshot?.private_state);
  const activePlayerSeat =
    typeof activePlayerSeatIndex === 'number' ? toRelativeSeat(localSeat, activePlayerSeatIndex) : 'bottom';
  const deadlineAt = createCenterDeadlineAt(state, options);
  const extendedWithExtra = getExtendedWithExtra(state);
  const promptCue = createPromptCue(state, options);
  const actionIndicatorSeat = createActionIndicatorSeat(state, options);
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
    canLeaveTable: canLeaveTable(state.roomSnapshot),
    phaseLabel: snapshot ? PHASE_LABELS[snapshot.phase] : PHASE_LABELS.waiting,
    roundLabel: createRoundLabel(state),
    deadlineAt,
    extendedWithExtra,
    activePlayerSeat,
    actionIndicatorSeat,
    shouldDebounceCenterWaiting: Boolean(optimisticDiscard),
    isActionDockElevated: mode === 'my_turn' || Boolean(promptCue?.isUrgent),
    players: createPlayers(state, options),
    actions: createActionViews(state, waitingControls, options),
    waitingControls,
    tableSettings: createTableSettings(state),
    discards: createDiscards(state, options),
    selectedTileCode: createSelectedTileCode(state, options),
    localHand: createLocalHand(state, options),
    handInsight: createHandInsight(state),
    claimCandidates: createClaimCandidates(state, options),
    drawnTileId: createDrawnTileId(state),
    centerStatusText: dealerSelection ? '抽取东家' : createCenterStatusText(state),
    remainingTileCount: createRemainingTileCount(state),
    promptText: createPromptText(state, options),
    promptCue,
    result: createResult(state, options),
    settlementHands: createSettlementHands(state, options),
    lastDiscard: optimisticDiscard?.tileCode ?? snapshot?.private_state?.last_discard ?? null,
    lastDiscardSeat: createLastDiscardSeat(state, options),
    shouldAutoReturnLastDiscardToRiver: createShouldAutoReturnLastDiscardToRiver(state),
    actionEffect: createActionEffect(state, options),
    actionEffects: createActionEffects(state, options),
    dealerSelection,
    quickChatEvent: createQuickChatEvent(state, options),
    systemBroadcastEvent: state.latestSystemBroadcast ?? null,
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
  hu: '和牌',
};

const KONG_EFFECT_LABELS: Record<string, string> = {
  concealed_kong: '暗杠',
  add_kong: '补杠',
};

const PROMPT_SEAT_COPY: Record<Seat, string> = {
  bottom: '你',
  left: '上家',
  top: '对家',
  right: '下家',
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

function createQuickChatEvent(state: SessionState, options: MatchViewModelOptions = {}): QuickChatEventView | null {
  const snapshot = state.roomSnapshot?.payload;
  const message = state.latestQuickChatMessage;

  if (!snapshot || !message) {
    return null;
  }

  const localSeat = getPerspectiveSeat(state, options);
  const actorSeat = toRelativeSeat(localSeat, message.payload.actor_seat);
  const targetSeat = toRelativeSeat(localSeat, message.payload.target_seat);
  const actorName = message.payload.actor_display_name?.trim() || getSeatName(state, message.payload.actor_seat);
  const targetName = getSeatName(state, message.payload.target_seat);
  const text = message.payload.chat_kind === 'point_gesture'
    ? createPointGestureText(state, message.payload.actor_seat, message.payload.target_seat, actorName, targetName)
    : message.payload.actor_seat === message.payload.target_seat
      ? `${actorName}：${message.payload.emoji}`
      : `${actorName} -> ${targetName} : ${message.payload.emoji}`;

  return {
    key: message.payload.message_id,
    actorSeat,
    targetSeat,
    actorName,
    targetName,
    emoji: message.payload.emoji,
    text,
  };
}

function createPointGestureText(
  state: SessionState,
  actorSeat: number,
  targetSeat: number,
  actorName: string,
  targetName: string,
) {
  const actorPoints = getSeatPoints(state, actorSeat);
  const targetPoints = getSeatPoints(state, targetSeat);

  return typeof actorPoints === 'number' &&
    typeof targetPoints === 'number' &&
    actorPoints > targetPoints
    ? `${actorName}对${targetName}指指点点💀`
    : `${actorName}对${targetName}五体投地🛐`;
}

function getSeatPoints(state: SessionState, seatIndex: number) {
  const seat = state.roomSnapshot?.payload.seats.find((candidate) => candidate.seat_index === seatIndex);
  return typeof seat?.points === 'number' ? seat.points : null;
}

function createContinueActionConfirmation(state: SessionState, actionId: Extract<BattleActionId, 'start_next_round'>) {
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
    ? snapshot.seats.filter((seat) => getSeatIdentityType(seat) !== 'bot').length
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

function getStartNextRoundLabel(_state: SessionState) {
  return ACTION_LABELS.start_next_round;
}

function createMatchDecidedPlaceholder(): NonNullable<BattleViewModel['result']>['continueAction'] {
  return {
    id: 'match_decided',
    label: ACTION_LABELS.match_decided,
    enabled: false,
  };
}

function createStartNextRoundContinueAction(
  state: SessionState,
  confirmation: ReturnType<typeof createContinueActionConfirmation>,
  isConnectionInteractive: boolean,
): NonNullable<BattleViewModel['result']>['continueAction'] {
  const snapshot = state.roomSnapshot?.payload;
  return {
    id: 'start_next_round',
    label: !isConnectionInteractive
      ? '重连中...'
      : confirmation?.countdownDeadlineAt
        ? formatContinueActionCountdownLabel(confirmation.countdownDeadlineAt)
        : confirmation?.isLocalConfirmed
          ? formatContinueActionConfirmedLabel(confirmation.confirmedCount, confirmation.requiredCount)
          : getStartNextRoundLabel(state),
    enabled:
      isConnectionInteractive &&
      typeof snapshot?.local_seat === 'number' &&
      !confirmation?.isLocalConfirmed,
    countdownDeadlineAt: confirmation?.countdownDeadlineAt ?? undefined,
    confirmation: confirmation ?? undefined,
  };
}

function isFinalSettlement(state: SessionState) {
  const snapshot = state.roomSnapshot?.payload;
  if (snapshot?.phase !== 'settlement') {
    return false;
  }

  const roundWind = snapshot.private_state?.round_wind ?? snapshot.match_state?.prevailing_wind;
  const handNumber = snapshot.match_state?.hand_number;
  return roundWind === 'north' && handNumber === 4;
}

function formatContinueActionConfirmedLabel(confirmedCount: number, requiredCount: number) {
  return `已确认 ${confirmedCount}/${requiredCount}`;
}

function formatContinueActionCountdownLabel(deadlineAt: string) {
  const remainingSeconds = Math.max(0, Math.ceil((new Date(deadlineAt).getTime() - Date.now()) / 1000));
  return `${remainingSeconds}s后自动推进`;
}
