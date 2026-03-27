import type {
  BackendActionType,
  BattleActionId,
  BattleActionView,
  BattleViewModel,
  PlayerView,
  PrivatePlayerState,
  ResultSeatView,
  Seat,
  SessionState,
  WaitingControls,
} from '../types/match';
import { getActionCandidateGroups, getFlowerCandidateTileIds, isFlowerTileKey } from './kongSelection';

const RELATIVE_SEATS: Seat[] = ['bottom', 'right', 'top', 'left'];
const WINDS: PlayerView['wind'][] = ['East', 'South', 'West', 'North'];
const ACTION_ORDER: BattleActionId[] = [
  'ready',
  'start_match',
  'start_next_round',
  'restart_match',
  'discard',
  'flower',
  'kong',
  'hu',
  'chow',
  'pung',
  'pass',
];

const ACTION_LABELS: Record<BattleActionId, string> = {
  ready: '准备',
  start_match: '开始对局',
  start_next_round: '下一局',
  restart_match: '再来一局',
  discard: '出牌',
  flower: '补花',
  kong: '杠',
  hu: '和牌',
  chow: '吃',
  pung: '碰',
  pass: '过',
};

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

function toRelativeSeat(localSeat: number, absoluteSeat: number): Seat {
  const offset = (absoluteSeat - localSeat + 4) % 4;
  return RELATIVE_SEATS[offset] ?? 'bottom';
}

function formatSignedNumber(value: number) {
  if (value > 0) {
    return `+${value}`;
  }

  return `${value}`;
}

function createPromptText(state: SessionState): string | null {
  const pendingAction = state.roomSnapshot?.payload.private_state?.pending_action;
  if (pendingAction && typeof pendingAction.type === 'string') {
    if (pendingAction.type === 'opening_flowers') {
      const options = Array.isArray((pendingAction as { options?: unknown }).options)
        ? ((pendingAction as { options: BackendActionType[] }).options)
        : [];
      return options.includes('flower') ? '起手补花中，请选择花牌后点击补花' : '起手无花，请点击过牌';
    }
    if (pendingAction.type === 'claim_window') {
      return '可响应吃碰杠胡';
    }
    if (pendingAction.type === 'rob_kong_window') {
      return '可选择抢杠胡或过牌';
    }
  }

  if (state.latestActionPrompt) {
    return `可执行操作：${state.latestActionPrompt.payload.options.map((item) => ACTION_LABELS[item]).join(' / ')}`;
  }

  if (state.roomSnapshot?.payload.phase === 'finished') {
    return '整场对局已结束，可发起再来一局';
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
  const allReady = occupiedSeats === 4 && snapshot.seats.every((seat) => seat.ready);

  return {
    canReady: Boolean(localSeatState),
    canStart: allReady,
    isReady: Boolean(localSeatState?.ready),
    occupiedSeats,
  };
}

function getPromptOptions(state: SessionState): BackendActionType[] {
  const pendingAction = state.roomSnapshot?.payload.private_state?.pending_action;
  if (pendingAction && 'options' in pendingAction) {
    const options = (pendingAction as { options?: unknown }).options;
    if (Array.isArray(options)) {
      return options as BackendActionType[];
    }
  }

  return state.latestActionPrompt?.payload.options ?? [];
}

function createActionViews(state: SessionState, waitingControls: WaitingControls | null): BattleActionView[] {
  const snapshot = state.roomSnapshot?.payload;
  const promptOptions = new Set<BackendActionType>(getPromptOptions(state));
  const kongCandidateGroups = getActionCandidateGroups(state, 'kong');
  const chowCandidateGroups = getActionCandidateGroups(state, 'chow');
  const pungCandidateGroups = getActionCandidateGroups(state, 'pung');
  const flowerCandidateTileIds = getFlowerCandidateTileIds(state);
  const hasSelectedFlower =
    state.selectedTileIds.length === 1 && flowerCandidateTileIds.includes(state.selectedTileIds[0]);
  const hasSelectedDiscard = state.selectedTileIds.length === 1;
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
      enabled = canContinueRound;
    } else if (id === 'restart_match') {
      enabled = canRestartMatch;
    } else if (promptOptions.has(id as BackendActionType)) {
      enabled =
        id === 'discard'
          ? hasSelectedDiscard
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

    const emphasis =
      id === 'start_match' || id === 'start_next_round' || id === 'restart_match' || (id === 'discard' && enabled)
        ? 'high'
        : enabled
          ? 'medium'
          : 'low';
    return {
      id,
      label: ACTION_LABELS[id],
      enabled,
      emphasis,
    };
  });
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

  if (snapshot?.phase === 'settlement' && state.latestMatchResult) {
    const merged: Record<string, number> = { ...matchScores };
    for (const [seatIndex, delta] of Object.entries(state.latestMatchResult.payload.score_delta.total_delta_by_seat)) {
      merged[seatIndex] = (merged[seatIndex] ?? 0) + delta;
    }
    return merged;
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

function createPlayers(state: SessionState): PlayerView[] {
  const snapshot = state.roomSnapshot?.payload;
  if (!snapshot) {
    return [];
  }

  const localSeat = getLocalSeat(state);
  const currentActor = snapshot.private_state?.current_actor;
  const dealerSeat = snapshot.match_state?.dealer_seat ?? snapshot.private_state?.dealer_seat;
  const displayedScores = getDisplayedScores(state);
  const liveDeltaBySeat = getLiveDeltaBySeat(state);
  const flowerCountBySeat = getFlowerCountBySeat(state);

  return snapshot.seats
    .map((seat) => {
      const relativeSeat = toRelativeSeat(localSeat, seat.seat_index);
      const privatePlayer = findPrivatePlayer(state, seat.seat_index);
      const seatKey = String(seat.seat_index);

      return {
        seat: relativeSeat,
        name: seat.nickname,
        score: displayedScores[seatKey] ?? 0,
        liveDelta: liveDeltaBySeat[seatKey] ?? 0,
        flowerCount: flowerCountBySeat[seatKey] ?? privatePlayer?.flowers.length ?? 0,
        wind: WINDS[seat.seat_index] ?? 'East',
        isDealer: dealerSeat === seat.seat_index,
        isActive: currentActor === seat.seat_index,
        isLocal: seat.seat_index === localSeat,
        connected: seat.connected,
        ready: seat.ready,
        concealedCount: privatePlayer?.concealed_count ?? 0,
        meldCount: privatePlayer?.melds.length ?? 0,
        melds: privatePlayer?.melds ?? [],
        flowers: privatePlayer?.flowers ?? [],
        statusText:
          snapshot.phase === 'waiting'
            ? seat.ready
              ? '已准备'
              : '等待中'
            : snapshot.phase === 'settlement'
              ? '等待下一局'
              : snapshot.phase === 'finished'
                ? '整场完成'
                : seat.connected
                  ? '对局中'
                  : '已断线',
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
    empty[toRelativeSeat(localSeat, player.seat_index)] = player.discards;
  }

  return empty;
}

function createLocalHand(state: SessionState) {
  const localSeat = getLocalSeat(state);
  const localPlayer = findPrivatePlayer(state, localSeat);
  const drawnTileId =
    state.roomSnapshot?.payload.private_state?.pending_action?.type === 'active_turn'
      ? state.roomSnapshot.payload.private_state.pending_action.drawn_tile_id
      : undefined;

  return (localPlayer?.concealed_tiles ?? [])
    .map((tile) => ({
      tileId: tile.tile_id,
      code: tile.tile_key,
      isSelected: state.selectedTileIds.includes(tile.tile_id),
      isDrawn: tile.tile_id === drawnTileId,
      isFlower: isFlowerTileKey(tile.tile_key),
    }))
    .sort(compareLocalHandTiles);
}

function compareLocalHandTiles(
  left: BattleViewModel['localHand'][number],
  right: BattleViewModel['localHand'][number],
) {
  const leftKey = getTileSortKey(left.code);
  const rightKey = getTileSortKey(right.code);

  if (leftKey.group !== rightKey.group) {
    return leftKey.group - rightKey.group;
  }

  if (leftKey.order !== rightKey.order) {
    return leftKey.order - rightKey.order;
  }

  return left.tileId.localeCompare(right.tileId);
}

function getTileSortKey(code: string) {
  const normalized = code.trim().toLowerCase();
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
      };
    })
    .sort((left, right) => right.score - left.score);
}

function createResult(state: SessionState): BattleViewModel['result'] {
  const snapshot = state.roomSnapshot?.payload;
  if (!snapshot) {
    return null;
  }

  const localSeat = getLocalSeat(state);

  if (snapshot.phase === 'settlement' && state.latestMatchResult) {
    const result = state.latestMatchResult.payload;
    const scoreDeltaBySeat: Partial<Record<Seat, number>> = {};
    for (const [seatIndex, delta] of Object.entries(result.score_delta.total_delta_by_seat)) {
      scoreDeltaBySeat[toRelativeSeat(localSeat, Number(seatIndex))] = delta;
    }

    return {
      title: '本局结算',
      summary:
        result.win_type === 'draw'
          ? result.draw_type === 'exhaustive'
            ? '荒牌流局'
            : '本局流局'
          : `${WIN_TYPE_LABELS[result.win_type] ?? result.win_type}，等待玩家开始下一局`,
      fanTotal: result.fan_total,
      winnerSeat: typeof result.winner_seat === 'number' ? toRelativeSeat(localSeat, result.winner_seat) : null,
      discarderSeat: typeof result.discarder_seat === 'number' ? toRelativeSeat(localSeat, result.discarder_seat) : null,
      winType: result.win_type,
      provisional: result.score_delta.provisional,
      flowerCount: result.flower_count,
      fanBreakdown: result.fan_breakdown.map((item) => ({
        fanKey: item.fan_key,
        fanValue: item.fan_value,
      })),
      scoreDeltaBySeat,
      seats: createResultSeats(state, result.score_delta.total_delta_by_seat),
      continueAction: {
        id: 'start_next_round',
        label: ACTION_LABELS.start_next_round,
        enabled: typeof snapshot.local_seat === 'number',
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
      provisional: false,
      flowerCount: 0,
      fanBreakdown: [],
      scoreDeltaBySeat: {},
      seats: createResultSeats(state, null),
      continueAction: {
        id: 'restart_match',
        label: ACTION_LABELS.restart_match,
        enabled: snapshot.match_state?.match_finished === true && typeof snapshot.local_seat === 'number',
      },
    };
  }

  return null;
}

function createLastDiscardSeat(state: SessionState): Seat | null {
  const snapshot = state.roomSnapshot?.payload;
  const privateState = snapshot?.private_state;
  const lastDiscard = privateState?.last_discard ?? null;

  if (!snapshot || !privateState || !lastDiscard) {
    return null;
  }

  const localSeat = getLocalSeat(state);
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

export function createMatchViewModel(state: SessionState): BattleViewModel {
  const snapshot = state.roomSnapshot?.payload;
  const waitingControls = createWaitingControls(state);
  const isWaiting = snapshot?.phase === 'waiting';
  const isReconnecting = state.connectionStatus === 'reconnecting';
  const isSettlement = snapshot?.phase === 'settlement';
  const isFinished = snapshot?.phase === 'finished';
  const localSeat = getLocalSeat(state);
  const activePlayerSeat = snapshot?.private_state ? toRelativeSeat(localSeat, snapshot.private_state.current_actor) : 'bottom';
  const deadlineAt =
    snapshot?.private_state?.pending_action && 'deadline_at' in snapshot.private_state.pending_action
      ? String(snapshot.private_state.pending_action.deadline_at)
      : state.latestActionPrompt?.payload.deadline_at ?? null;
  const mode = !snapshot
    ? 'loading'
    : isFinished
      ? 'finished'
      : isReconnecting || isWaiting
        ? 'disconnected_or_waiting'
        : isSettlement
          ? 'resolving'
          : snapshot.private_state?.pending_action?.type === 'active_turn' &&
              snapshot.private_state.pending_action.seat_index === localSeat
            ? 'my_turn'
            : 'watching';

  return {
    mode,
    tableCode: snapshot?.table_code ?? state.tableCode,
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
    isActionDockElevated: mode === 'my_turn',
    players: createPlayers(state),
    actions: createActionViews(state, waitingControls),
    waitingControls,
    discards: createDiscards(state),
    localHand: createLocalHand(state),
    centerBanner: createCenterBanner(state),
    promptText: createPromptText(state),
    result: createResult(state),
    lastDiscard: snapshot?.private_state?.last_discard ?? null,
    lastDiscardSeat: createLastDiscardSeat(state),
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
