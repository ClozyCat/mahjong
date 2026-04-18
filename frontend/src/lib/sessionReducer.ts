import type {
  ActionRejectedMessage,
  DisplayMeldView,
  MatchStatisticsState,
  OptimisticDiscardState,
  OptimisticFlowerState,
  RoomSnapshotMessage,
  SeatSnapshot,
  ServerMessage,
  SessionState,
  ToastMessage,
} from '../types/match';
import { getRoundEventCopy } from './roundEventCopy';

export type SessionAction =
  | { type: 'set_config'; apiBaseUrl?: string; wsBaseUrl?: string }
  | { type: 'set_credentials'; tableCode?: string; nickname?: string }
  | { type: 'set_connection_status'; status: SessionState['connectionStatus'] }
  | { type: 'queue_optimistic_discard'; tileId: string }
  | { type: 'queue_optimistic_flower'; tileId: string }
  | { type: 'set_selected_tiles'; tileIds: string[]; mode: SessionState['selectionMode'] }
  | { type: 'reset_transient_feedback' }
  | { type: 'return_to_lobby'; tableCode?: string; keepNickname?: boolean }
  | { type: 'ws_message'; message: ServerMessage };

const ACTION_REJECTION_COPY: Record<string, string> = {
  table_not_found: '牌桌不存在，请检查牌桌编号。',
  table_full: '牌桌已满，暂时无法加入。',
  invalid_reconnect_token: '重连凭证已失效，请重新加入牌桌。',
  room_already_started: '牌桌已经开始对局。',
  match_not_finished: '整场比赛尚未结束，暂时不能再来一局。',
  seat_not_owned: '当前连接没有对应座位，无法执行该操作。',
  room_not_ready: '四个座位都准备完成后才能开始。',
  round_not_ready: '结算尚未完成，暂时不能开始下一局。',
  not_your_turn: '还没轮到你操作。',
  invalid_action: '当前动作不合法，已按服务器状态为准。',
  select_tile_first: '请先选择需要操作的牌。',
  restricted_same_turn_discard: '吃、碰、杠后的同名牌本回合不能立即打出。',
  unsupported_message: '客户端发送了服务器不支持的消息。',
};

function createToast(kind: ToastMessage['kind'], text: string): ToastMessage {
  return {
    id: `${kind}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    kind,
    text,
    createdAt: new Date().toISOString(),
  };
}

function appendToast(state: SessionState, kind: ToastMessage['kind'], text: string): ToastMessage[] {
  const previous = state.toasts.at(-1);

  if (previous?.kind === kind && previous.text === text) {
    return state.toasts;
  }

  return [...state.toasts, createToast(kind, text)];
}

function getPresenceCopy(message: { seat_index: number; connected: boolean }, seats?: SeatSnapshot[]) {
  const nickname = seats?.find((seat) => seat.seat_index === message.seat_index)?.nickname ?? `玩家${message.seat_index + 1}`;
  return `${nickname}${message.connected ? '已连接' : '已断开连接'}`;
}

function getActionRejectedCopy(reason: string) {
  return ACTION_REJECTION_COPY[reason] ?? reason;
}

function isHuRoundEvent(
  message: Extract<ServerMessage, { type: 'round_event' }> | null,
) {
  if (!message) {
    return false;
  }

  if (message.payload.event_type === 'self_hu_declared') {
    return true;
  }

  return (
    message.payload.event_type === 'claim_made' &&
    message.payload.event?.claim_type === 'hu'
  );
}

function getNextLatestRoundEvent(
  current: SessionState['latestRoundEvent'],
  incoming: Extract<ServerMessage, { type: 'round_event' }>,
) {
  if (incoming.payload.event_type === 'settlement_ready' && isHuRoundEvent(current)) {
    return current;
  }

  return incoming;
}

function createMatchStatisticsFromScores(scores: Record<string, number> | null | undefined): MatchStatisticsState | null {
  if (!scores || Object.keys(scores).length === 0) {
    return null;
  }

  return {
    completedRoundCount: 0,
    lastAppliedRoundId: null,
    seatStatsBySeat: Object.fromEntries(
      Object.entries(scores).map(([seatKey, score]) => [
        seatKey,
        {
          scoreHistory: [score],
          winCount: 0,
        },
      ]),
    ),
  };
}

function createMatchStatisticsFromSnapshot(snapshot: RoomSnapshotMessage): MatchStatisticsState | null {
  const matchState = snapshot.payload.match_state;
  const statistics = matchState?.statistics;

  if (statistics) {
    return {
      completedRoundCount: statistics.completed_round_count ?? 0,
      lastAppliedRoundId: matchState?.last_completed_round_id ?? null,
      seatStatsBySeat: Object.fromEntries(
        Object.entries(statistics.seat_stats_by_seat ?? {}).map(([seatKey, seatStats]) => [
          seatKey,
          {
            scoreHistory: Array.isArray(seatStats.score_history) ? [...seatStats.score_history] : [],
            winCount: seatStats.win_count ?? 0,
          },
        ]),
      ),
    };
  }

  return createMatchStatisticsFromScores(matchState?.cumulative_scores);
}

function normalizeMeldTileCodeGroups(melds: string[][] | null | undefined) {
  return (melds ?? [])
    .map((meld) =>
      (meld ?? []).filter((tileCode): tileCode is string => typeof tileCode === 'string' && tileCode.trim().length > 0),
    )
    .filter((meld) => meld.length > 0);
}

function createDisplayMeld(meld: string[], claimedTileIndex: number | null = null): DisplayMeldView {
  return {
    tiles: meld.map((code, index) => ({
      code,
      source: claimedTileIndex !== null && index === claimedTileIndex ? ('claim' as const) : ('hand' as const),
    })),
  };
}

function getDisplayMeldCodes(meld: DisplayMeldView) {
  return meld.tiles.map((tile) => tile.code);
}

function getClaimedTileIndex(
  meld: string[],
  claimTileKey: string,
  preferredIndex: number,
) {
  const matchingIndexes = meld.flatMap((code, index) => (code === claimTileKey ? [index] : []));

  if (matchingIndexes.length === 0) {
    return null;
  }

  if (matchingIndexes.includes(preferredIndex)) {
    return preferredIndex;
  }

  return matchingIndexes[0];
}

function getPreferredClaimIndex(actorSeat: number, sourceSeat: number, meldLength: number) {
  const relativeSourceSeat = (sourceSeat - actorSeat + 4) % 4;

  if (relativeSourceSeat === 3) {
    return 0;
  }

  if (relativeSourceSeat === 2) {
    return Math.max(0, Math.floor((meldLength - 1) / 2));
  }

  return Math.max(0, meldLength - 1);
}

function createClaimDisplayMeld(meld: string[], actorSeat: number, sourceSeat: number, claimTileKey: string) {
  const preferredIndex = getPreferredClaimIndex(actorSeat, sourceSeat, meld.length);
  const claimedTileIndex = getClaimedTileIndex(meld, claimTileKey, preferredIndex);

  return createDisplayMeld(meld, claimedTileIndex);
}

function sameMeldTileOrder(left: string[], right: string[]) {
  return left.length === right.length && left.every((tile, index) => tile === right[index]);
}

function isMatchingAddKongUpgrade(previousMeld: DisplayMeldView, nextMeld: string[]) {
  const previousCodes = getDisplayMeldCodes(previousMeld);

  return (
    previousCodes.length >= 3 &&
    nextMeld.length === 4 &&
    new Set(previousCodes).size === 1 &&
    new Set(nextMeld).size === 1 &&
    previousCodes[0] === nextMeld[0]
  );
}

function mergeDisplayMeld(previousMeld: DisplayMeldView, nextMeld: string[]) {
  if (sameMeldTileOrder(getDisplayMeldCodes(previousMeld), nextMeld)) {
    return {
      tiles: nextMeld.map((code, index) => ({
        code,
        source: previousMeld.tiles[index]?.source === 'claim' ? ('claim' as const) : ('hand' as const),
      })),
    };
  }

  if (isMatchingAddKongUpgrade(previousMeld, nextMeld)) {
    const claimedTileIndex = previousMeld.tiles.findIndex((tile) => tile.source === 'claim');
    return createDisplayMeld(nextMeld, claimedTileIndex >= 0 ? claimedTileIndex : null);
  }

  return createDisplayMeld(nextMeld);
}

function reconcileDisplayMeldsWithSnapshot(
  previousDisplayMeldsBySeat: SessionState['displayMeldsBySeat'],
  snapshot: RoomSnapshotMessage,
) {
  const nextDisplayMeldsBySeat: Record<string, DisplayMeldView[]> = {};

  snapshot.payload.private_state?.players.forEach((player) => {
    const seatKey = String(player.seat_index);
    const snapshotMelds = normalizeMeldTileCodeGroups(player.melds);
    const previousDisplayMelds = previousDisplayMeldsBySeat?.[seatKey] ?? [];
    const usedPreviousIndexes = new Set<number>();

    nextDisplayMeldsBySeat[seatKey] = snapshotMelds.map((meld) => {
      const exactMatchIndex = previousDisplayMelds.findIndex(
        (previousMeld, index) =>
          !usedPreviousIndexes.has(index) && sameMeldTileOrder(getDisplayMeldCodes(previousMeld), meld),
      );

      if (exactMatchIndex >= 0) {
        usedPreviousIndexes.add(exactMatchIndex);
        return mergeDisplayMeld(previousDisplayMelds[exactMatchIndex], meld);
      }

      const addKongUpgradeIndex = previousDisplayMelds.findIndex(
        (previousMeld, index) =>
          !usedPreviousIndexes.has(index) && isMatchingAddKongUpgrade(previousMeld, meld),
      );

      if (addKongUpgradeIndex >= 0) {
        usedPreviousIndexes.add(addKongUpgradeIndex);
        return mergeDisplayMeld(previousDisplayMelds[addKongUpgradeIndex], meld);
      }

      return createDisplayMeld(meld);
    });
  });

  return nextDisplayMeldsBySeat;
}

function appendDisplayMeldForClaim(
  displayMeldsBySeat: SessionState['displayMeldsBySeat'],
  event: Extract<ServerMessage, { type: 'round_event' }>['payload']['event'],
) {
  const actorSeat = typeof event?.seat === 'number' ? event.seat : null;
  const sourceSeat = typeof event?.from === 'number' ? event.from : null;
  const claimTileKey = typeof event?.tile_key === 'string' ? event.tile_key : null;
  const meld =
    Array.isArray(event?.meld)
      ? event.meld.filter((tile): tile is string => typeof tile === 'string' && tile.trim().length > 0)
      : [];

  if (actorSeat === null || sourceSeat === null || !claimTileKey || meld.length === 0) {
    return displayMeldsBySeat ?? {};
  }

  const seatKey = String(actorSeat);

  return {
    ...(displayMeldsBySeat ?? {}),
    [seatKey]: [
      ...((displayMeldsBySeat ?? {})[seatKey] ?? []),
      createClaimDisplayMeld(meld, actorSeat, sourceSeat, claimTileKey),
    ],
  };
}

function updateDisplayMeldForSelfKong(
  displayMeldsBySeat: SessionState['displayMeldsBySeat'],
  event: Extract<ServerMessage, { type: 'round_event' }>['payload']['event'],
) {
  const actorSeat = typeof event?.seat === 'number' ? event.seat : null;
  const kongType = typeof event?.kong_type === 'string' ? event.kong_type : null;
  const tileKey = typeof event?.tile_key === 'string' ? event.tile_key : null;

  if (actorSeat === null || !kongType || !tileKey) {
    return displayMeldsBySeat ?? {};
  }

  const seatKey = String(actorSeat);
  const currentSeatMelds = [...((displayMeldsBySeat ?? {})[seatKey] ?? [])];

  if (kongType === 'add_kong') {
    const meldIndex = currentSeatMelds.findIndex((meld) => {
      const codes = getDisplayMeldCodes(meld);
      return codes.length >= 3 && new Set(codes).size === 1 && codes[0] === tileKey;
    });

    if (meldIndex >= 0) {
      currentSeatMelds[meldIndex] = mergeDisplayMeld(currentSeatMelds[meldIndex], [
        tileKey,
        tileKey,
        tileKey,
        tileKey,
      ]);
      return {
        ...(displayMeldsBySeat ?? {}),
        [seatKey]: currentSeatMelds,
      };
    }
  }

  return {
    ...(displayMeldsBySeat ?? {}),
    [seatKey]: [
      ...currentSeatMelds,
      createDisplayMeld([tileKey, tileKey, tileKey, tileKey]),
    ],
  };
}

function findSnapshotPlayer(snapshot: RoomSnapshotMessage['payload'] | null | undefined, seatIndex: number | null | undefined) {
  if (!snapshot || typeof seatIndex !== 'number') {
    return null;
  }

  return snapshot.private_state?.players.find((player) => player.seat_index === seatIndex) ?? null;
}

function createOptimisticDiscard(state: SessionState, tileId: string): OptimisticDiscardState | null {
  if (state.optimisticDiscard) {
    return state.optimisticDiscard;
  }

  const snapshot = state.roomSnapshot?.payload;
  const localSeat = snapshot?.local_seat;
  const pendingAction = snapshot?.private_state?.pending_action;
  const localPlayer = findSnapshotPlayer(snapshot ?? null, localSeat);
  const restrictedDiscardTileIds =
    pendingAction?.type === 'active_turn' && Array.isArray(pendingAction.restricted_discard_tile_ids)
      ? pendingAction.restricted_discard_tile_ids
      : [];

  if (
    !snapshot ||
    typeof localSeat !== 'number' ||
    pendingAction?.type !== 'active_turn' ||
    pendingAction.seat_index !== localSeat ||
    !Array.isArray(pendingAction.options) ||
    !pendingAction.options.includes('discard') ||
    restrictedDiscardTileIds.includes(tileId)
  ) {
    return null;
  }

  const tile = localPlayer?.concealed_tiles?.find((candidate) => candidate.tile_id === tileId);
  if (!tile) {
    return null;
  }

  return {
    tileId,
    tileCode: tile.tile_key,
    seatIndex: localSeat,
    actionEffectKey: `optimistic-discard:${tileId}:${Date.now()}`,
    requestedAt: new Date().toISOString(),
  };
}

function reconcileOptimisticDiscardWithSnapshot(
  optimisticDiscard: SessionState['optimisticDiscard'],
  snapshot: RoomSnapshotMessage,
) {
  if (!optimisticDiscard) {
    return null;
  }

  if (snapshot.payload.phase !== 'playing') {
    return null;
  }

  const localPlayer = findSnapshotPlayer(snapshot.payload, optimisticDiscard.seatIndex);
  const stillInHand = localPlayer?.concealed_tiles?.some((tile) => tile.tile_id === optimisticDiscard.tileId) ?? false;

  return stillInHand ? optimisticDiscard : null;
}

function createOptimisticFlower(tileId: string): OptimisticFlowerState {
  return {
    tileId,
    requestedAt: new Date().toISOString(),
  };
}

function reconcileOptimisticFlowerWithSnapshot(
  optimisticFlower: SessionState['optimisticFlower'],
  snapshot: RoomSnapshotMessage,
) {
  if (!optimisticFlower) {
    return null;
  }

  if (snapshot.payload.phase !== 'playing') {
    return null;
  }

  const localSeat = snapshot.payload.local_seat;
  const pendingAction = snapshot.payload.private_state?.pending_action;
  if (
    typeof localSeat !== 'number' ||
    pendingAction?.type !== 'opening_flowers' ||
    pendingAction.seat_index !== localSeat
  ) {
    return null;
  }

  const localPlayer = findSnapshotPlayer(snapshot.payload, localSeat);
  const stillInHand = localPlayer?.concealed_tiles?.some((tile) => tile.tile_id === optimisticFlower.tileId) ?? false;

  return stillInHand ? optimisticFlower : null;
}

export function createInitialSessionState(): SessionState {
  return {
    apiBaseUrl: undefined,
    wsBaseUrl: undefined,
    tableCode: '',
    nickname: '',
    connectionStatus: 'idle',
    roomSnapshot: null,
    latestMatchResult: null,
    latestActionPrompt: null,
    latestRoundEvent: null,
    latestQuickChatMessage: null,
    lastRejectedAction: null,
    reconnectToken: null,
    optimisticDiscard: null,
    optimisticFlower: null,
    selectedTileIds: [],
    selectionMode: null,
    toasts: [],
    matchStatistics: null,
    latestReplacementTileId: null,
    displayMeldsBySeat: {},
  };
}

function applyServerMessage(state: SessionState, message: ServerMessage): SessionState {
  switch (message.type) {
    case 'room_snapshot': {
      const localSeat = message.payload.local_seat;
      const localPlayer =
        typeof localSeat === 'number'
          ? message.payload.private_state?.players.find((player) => player.seat_index === localSeat)
          : null;
      const restrictedDiscardTileIds =
        message.payload.private_state?.pending_action?.type === 'active_turn' &&
        Array.isArray(message.payload.private_state.pending_action.restricted_discard_tile_ids)
          ? new Set(message.payload.private_state.pending_action.restricted_discard_tile_ids)
          : new Set<string>();
      const availableTileIds =
        localPlayer?.concealed_tiles
          ?.map((tile) => tile.tile_id)
          .filter((tileId) => !restrictedDiscardTileIds.has(tileId)) ?? null;
      const nextSelectedTileIds =
        availableTileIds === null
          ? []
          : state.selectedTileIds.filter((tileId) => availableTileIds.includes(tileId));
      const keepLatestResult = message.payload.phase === 'settlement' || message.payload.phase === 'finished';
      const keepLatestReplacement = message.payload.private_state?.pending_action?.type === 'active_turn';
      const nextOptimisticDiscard = reconcileOptimisticDiscardWithSnapshot(state.optimisticDiscard ?? null, message);
      const nextOptimisticFlower = reconcileOptimisticFlowerWithSnapshot(state.optimisticFlower ?? null, message);

      return {
        ...state,
        roomSnapshot: message,
        tableCode: message.payload.table_code,
        reconnectToken: message.payload.reconnect_token ?? state.reconnectToken,
        lastRejectedAction: null,
        optimisticDiscard: nextOptimisticDiscard,
        optimisticFlower: nextOptimisticFlower,
        latestMatchResult: keepLatestResult ? state.latestMatchResult : null,
        latestActionPrompt: null,
        selectedTileIds: nextSelectedTileIds,
        selectionMode: nextSelectedTileIds.length > 0 ? state.selectionMode : null,
        matchStatistics: createMatchStatisticsFromSnapshot(message),
        latestReplacementTileId: keepLatestReplacement ? state.latestReplacementTileId : null,
        displayMeldsBySeat: reconcileDisplayMeldsWithSnapshot(state.displayMeldsBySeat, message),
      };
    }
    case 'action_prompt':
      return {
        ...state,
        latestActionPrompt: message,
      };
    case 'match_result':
      return {
        ...state,
        latestMatchResult: message,
      };
    case 'round_event': {
      const text = getRoundEventCopy(
        message.payload.event_type,
        message.payload.event,
        state.roomSnapshot?.payload.seats,
      );

      const localSeat = state.roomSnapshot?.payload.local_seat;
      const nextReplacementTileId =
        message.payload.event_type === 'replacement_draw' && message.payload.event?.seat === localSeat
          ? (message.payload.event?.tile_id as string)
          : state.latestReplacementTileId;

      return {
        ...state,
        latestRoundEvent: getNextLatestRoundEvent(state.latestRoundEvent, message),
        latestReplacementTileId: nextReplacementTileId,
        displayMeldsBySeat:
          message.payload.event_type === 'claim_made' && message.payload.event?.claim_type !== 'hu'
            ? appendDisplayMeldForClaim(state.displayMeldsBySeat, message.payload.event)
            : message.payload.event_type === 'self_kong_declared'
              ? updateDisplayMeldForSelfKong(state.displayMeldsBySeat, message.payload.event)
              : state.displayMeldsBySeat,
        toasts: appendToast(state, 'event', text),
      };
    }
    case 'quick_chat':
      return {
        ...state,
        latestQuickChatMessage: message,
      };
    case 'player_presence':
      return {
        ...state,
        toasts: appendToast(
          state,
          'presence',
          getPresenceCopy(message.payload, state.roomSnapshot?.payload.seats),
        ),
      };
    case 'action_rejected':
      return {
        ...state,
        lastRejectedAction: message as ActionRejectedMessage,
        optimisticDiscard: null,
        optimisticFlower: null,
        toasts: appendToast(state, 'error', getActionRejectedCopy(message.payload.reason)),
      };
    case 'heartbeat':
      return state;
    default:
      return state;
  }
}

export function sessionReducer(state: SessionState, action: SessionAction): SessionState {
  switch (action.type) {
    case 'set_config':
      return {
        ...state,
        apiBaseUrl: action.apiBaseUrl ?? state.apiBaseUrl,
        wsBaseUrl: action.wsBaseUrl ?? state.wsBaseUrl,
      };
    case 'set_credentials':
      return {
        ...state,
        tableCode: action.tableCode ?? state.tableCode,
        nickname: action.nickname ?? state.nickname,
      };
    case 'set_connection_status':
      return {
        ...state,
        connectionStatus: action.status,
        optimisticDiscard: action.status === 'connected' ? state.optimisticDiscard ?? null : null,
        optimisticFlower: action.status === 'connected' ? state.optimisticFlower ?? null : null,
      };
    case 'queue_optimistic_discard': {
      const optimisticDiscard = createOptimisticDiscard(state, action.tileId);
      if (!optimisticDiscard) {
        return state;
      }

      return {
        ...state,
        optimisticDiscard,
      };
    }
    case 'queue_optimistic_flower':
      return {
        ...state,
        optimisticFlower: createOptimisticFlower(action.tileId),
      };
    case 'set_selected_tiles':
      return {
        ...state,
        selectedTileIds: [...new Set(action.tileIds)],
        selectionMode: action.tileIds.length > 0 ? action.mode : null,
      };
    case 'reset_transient_feedback':
      return {
        ...state,
        lastRejectedAction: null,
        toasts: [],
      };
    case 'return_to_lobby':
      return {
        ...createInitialSessionState(),
        apiBaseUrl: state.apiBaseUrl,
        wsBaseUrl: state.wsBaseUrl,
        nickname: action.keepNickname === false ? '' : state.nickname,
        tableCode: action.tableCode ?? state.tableCode,
      };
    case 'ws_message':
      return applyServerMessage(state, action.message);
    default:
      return state;
  }
}
