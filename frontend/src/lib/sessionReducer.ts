import type {
  ActionRejectedMessage,
  BackendActionType,
  MatchStatisticsState,
  OptimisticDiscardState,
  OptimisticFlowerState,
  RoomSnapshotMessage,
  SeatSnapshot,
  ServerMessage,
  SessionState,
  SystemBroadcastEventView,
  ToastMessage,
  UserPointsUpdatedMessage,
} from '../types/match';
import { getRoundEventCopy } from './roundEventCopy';
import {
  createPresenceSystemBroadcast,
  createRoundEventSystemBroadcast,
  createTitleChangeSystemBroadcast,
} from './systemBroadcastCopy';

export type SessionAction =
  | { type: 'set_config'; apiBaseUrl?: string; wsBaseUrl?: string }
  | { type: 'set_credentials'; tableCode?: string; nickname?: string }
  | { type: 'set_connection_status'; status: SessionState['connectionStatus'] }
  | { type: 'set_room_snapshot'; message: RoomSnapshotMessage }
  | { type: 'queue_optimistic_discard'; tileId: string; actionType: Extract<BackendActionType, 'discard' | 'ready_hand'> }
  | { type: 'queue_optimistic_flower'; tileId: string }
  | { type: 'set_selected_tiles'; tileIds: string[]; mode: SessionState['selectionMode'] }
  | { type: 'reset_transient_feedback' }
  | { type: 'return_to_lobby'; tableCode?: string; keepNickname?: boolean }
  | { type: 'user_points_updated'; message: UserPointsUpdatedMessage }
  | { type: 'ws_message'; message: ServerMessage };

const ACTION_REJECTION_COPY: Record<string, string> = {
  table_not_found: '牌桌不存在，请检查牌桌编号。',
  table_closed: '牌桌已关闭，请返回大厅重新进入。',
  table_full: '牌桌已满，暂时无法加入。',
  room_already_started: '牌桌已经开始对局。',
  seat_not_owned: '当前连接没有对应座位，无法执行该操作。',
  room_not_ready: '牌桌人数不足，或尚未加入 BOT。',
  round_not_ready: '结算尚未完成，暂时不能开始下一局。',
  not_your_turn: '还没轮到你操作。',
  invalid_action: '当前动作不合法，已按服务器状态为准。',
  select_tile_first: '请先选择需要操作的牌。',
  restricted_same_turn_discard: '吃、碰、杠后的同名牌本回合不能立即打出。',
  unsupported_message: '客户端发送了服务器不支持的消息。',
};

const RECENT_ROUND_EVENT_LIMIT = 24;

function createToast(kind: ToastMessage['kind'], text: string): ToastMessage {
  return {
    id: `${kind}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    kind,
    text,
    createdAt: new Date().toISOString(),
  };
}

function createSystemBroadcast(text: string): SystemBroadcastEventView {
  return {
    key: `system-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    text,
  };
}

function appendToast(state: SessionState, kind: ToastMessage['kind'], text: string): ToastMessage[] {
  const previous = state.toasts.at(-1);

  if (previous?.kind === kind && previous.text === text) {
    return state.toasts;
  }

  return [...state.toasts, createToast(kind, text)];
}

function getPresenceCopy(message: { seat_index: number; connected: boolean; nickname?: string | null }, seats?: SeatSnapshot[]) {
  const nickname = message.nickname ?? seats?.find((seat) => seat.seat_index === message.seat_index)?.nickname ?? `玩家${message.seat_index + 1}`;
  return `${nickname}${message.connected ? '已连接' : '已断开连接'}`;
}

function updateSeatPointSnapshot(snapshot: RoomSnapshotMessage | null, message: UserPointsUpdatedMessage) {
  if (!snapshot) {
    return snapshot;
  }

  const { user_id: userId, points, title } = message.payload;
  const updateSeat = <T extends { user_id?: number | null; points?: number | null; title?: string | null }>(seat: T): T =>
    seat.user_id === userId ? { ...seat, points, title: title ?? seat.title } : seat;
  const updatePrivatePlayer = <T extends { seat_index: number; points?: number | null; title?: string | null }>(player: T): T => {
    const publicSeat = snapshot.payload.seats.find((seat) => seat.seat_index === player.seat_index);
    return publicSeat?.user_id === userId ? { ...player, points, title: title ?? player.title } : player;
  };

  return {
    ...snapshot,
    payload: {
      ...snapshot.payload,
      seats: snapshot.payload.seats.map(updateSeat),
      private_state: snapshot.payload.private_state
        ? {
            ...snapshot.payload.private_state,
            players: snapshot.payload.private_state.players.map(updatePrivatePlayer),
          }
        : snapshot.payload.private_state,
    },
  };
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

function appendRecentRoundEvent(
  current: SessionState['recentRoundEvents'],
  incoming: Extract<ServerMessage, { type: 'round_event' }>,
) {
  return [...(current ?? []), incoming].slice(-RECENT_ROUND_EVENT_LIMIT);
}

function hasSnapshotRoundChanged(current: RoomSnapshotMessage | null, next: RoomSnapshotMessage) {
  const currentRoundId = current?.payload.private_state?.round_id;
  const nextRoundId = next.payload.private_state?.round_id;

  return Boolean(currentRoundId && nextRoundId && currentRoundId !== nextRoundId);
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
          dealInCount: 0,
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
            dealInCount: seatStats.deal_in_count ?? 0,
          },
        ]),
      ),
    };
  }

  return createMatchStatisticsFromScores(matchState?.cumulative_scores);
}

function findSnapshotPlayer(snapshot: RoomSnapshotMessage['payload'] | null | undefined, seatIndex: number | null | undefined) {
  if (!snapshot || typeof seatIndex !== 'number') {
    return null;
  }

  return snapshot.private_state?.players.find((player) => player.seat_index === seatIndex) ?? null;
}

function createOptimisticDiscard(
  state: SessionState,
  tileId: string,
  actionType: Extract<BackendActionType, 'discard' | 'ready_hand'>,
): OptimisticDiscardState | null {
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
    actionType,
    actionEffectKey: `optimistic-${actionType}:${tileId}:${Date.now()}`,
    requestedAt: new Date().toISOString(),
  };
}

function isOptimisticDiscardConfirmedBySnapshot(
  optimisticDiscard: SessionState['optimisticDiscard'],
  snapshot: RoomSnapshotMessage,
) {
  if (!optimisticDiscard || snapshot.payload.phase !== 'playing') {
    return false;
  }

  const localPlayer = findSnapshotPlayer(snapshot.payload, optimisticDiscard.seatIndex);
  const stillInHand = localPlayer?.concealed_tiles?.some((tile) => tile.tile_id === optimisticDiscard.tileId) ?? false;
  if (stillInHand) {
    return false;
  }

  if (optimisticDiscard.actionType === 'ready_hand') {
    return localPlayer?.is_ready_hand === true;
  }

  return true;
}

function isOptimisticDiscardConfirmedByRoundEvent(
  optimisticDiscard: SessionState['optimisticDiscard'],
  message: Extract<ServerMessage, { type: 'round_event' }> | null,
) {
  if (!optimisticDiscard || !message) {
    return false;
  }

  const eventSeat = message.payload.event?.seat;
  if (typeof eventSeat !== 'number' || eventSeat !== optimisticDiscard.seatIndex) {
    return false;
  }

  if (optimisticDiscard.actionType === 'ready_hand') {
    return message.payload.event_type === 'ready_hand_declared';
  }

  return message.payload.event_type === 'tile_discarded';
}

function reconcileOptimisticDiscardWithSnapshot(
  optimisticDiscard: SessionState['optimisticDiscard'],
  snapshot: RoomSnapshotMessage,
  latestRoundEvent: SessionState['latestRoundEvent'],
) {
  if (!optimisticDiscard) {
    return null;
  }

  if (snapshot.payload.phase !== 'playing') {
    return null;
  }

  const snapshotConfirmed = isOptimisticDiscardConfirmedBySnapshot(optimisticDiscard, snapshot);
  if (!snapshotConfirmed) {
    return optimisticDiscard;
  }

  if (
    optimisticDiscard.actionType === 'ready_hand' &&
    !isOptimisticDiscardConfirmedByRoundEvent(optimisticDiscard, latestRoundEvent)
  ) {
    return optimisticDiscard;
  }

  return null;
}

function reconcileOptimisticDiscardWithRoundEvent(
  optimisticDiscard: SessionState['optimisticDiscard'],
  message: Extract<ServerMessage, { type: 'round_event' }>,
  snapshot: SessionState['roomSnapshot'],
) {
  if (!optimisticDiscard) {
    return null;
  }

  if (!isOptimisticDiscardConfirmedByRoundEvent(optimisticDiscard, message)) {
    return optimisticDiscard;
  }

  if (
    snapshot &&
    isOptimisticDiscardConfirmedBySnapshot(optimisticDiscard, snapshot)
  ) {
    return null;
  }

  return optimisticDiscard;
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
  if (typeof localSeat !== 'number') {
    return null;
  }

  const localPlayer = findSnapshotPlayer(snapshot.payload, localSeat);
  const stillInHand = localPlayer?.concealed_tiles?.some((tile) => tile.tile_id === optimisticFlower.tileId) ?? false;

  return stillInHand ? optimisticFlower : null;
}

function reconcileLatestReplacementTileIdWithSnapshot(
  latestReplacementTileId: SessionState['latestReplacementTileId'],
  snapshot: RoomSnapshotMessage,
) {
  if (!latestReplacementTileId || snapshot.payload.phase !== 'playing') {
    return null;
  }

  const localSeat = snapshot.payload.local_seat;
  const pendingAction = snapshot.payload.private_state?.pending_action;
  if (
    typeof localSeat !== 'number' ||
    pendingAction?.type !== 'active_turn' ||
    pendingAction.seat_index !== localSeat ||
    pendingAction.drawn_tile_id !== latestReplacementTileId
  ) {
    return null;
  }

  const localPlayer = findSnapshotPlayer(snapshot.payload, localSeat);
  const stillInHand = localPlayer?.concealed_tiles?.some((tile) => tile.tile_id === latestReplacementTileId) ?? false;
  return stillInHand ? latestReplacementTileId : null;
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
    recentRoundEvents: [],
    latestQuickChatMessage: null,
    latestSystemBroadcast: null,
    lastRejectedAction: null,
    optimisticDiscard: null,
    optimisticFlower: null,
    selectedTileIds: [],
    selectionMode: null,
    toasts: [],
    matchStatistics: null,
    latestReplacementTileId: null,
  };
}

function applyRoomSnapshotMessage(state: SessionState, message: RoomSnapshotMessage): SessionState {
  const hasRoundChanged = hasSnapshotRoundChanged(state.roomSnapshot ?? null, message);
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
  const nextOptimisticDiscard = reconcileOptimisticDiscardWithSnapshot(
    state.optimisticDiscard ?? null,
    message,
    state.latestRoundEvent ?? null,
  );
  const nextOptimisticFlower = reconcileOptimisticFlowerWithSnapshot(state.optimisticFlower ?? null, message);
  const nextLatestReplacementTileId = reconcileLatestReplacementTileIdWithSnapshot(
    state.latestReplacementTileId ?? null,
    message,
  );

  return {
    ...state,
    roomSnapshot: message,
    tableCode: message.payload.table_code,
    lastRejectedAction: null,
    optimisticDiscard: nextOptimisticDiscard,
    optimisticFlower: nextOptimisticFlower,
    latestMatchResult: keepLatestResult ? state.latestMatchResult : null,
    latestActionPrompt: null,
    latestRoundEvent: hasRoundChanged ? null : state.latestRoundEvent,
    recentRoundEvents: hasRoundChanged ? [] : state.recentRoundEvents,
    selectedTileIds: nextSelectedTileIds,
    selectionMode: nextSelectedTileIds.length > 0 ? state.selectionMode : null,
    matchStatistics: createMatchStatisticsFromSnapshot(message),
    latestReplacementTileId: nextLatestReplacementTileId,
  };
}

function applyServerMessage(state: SessionState, message: ServerMessage): SessionState {
  switch (message.type) {
    case 'room_snapshot':
      return applyRoomSnapshotMessage(state, message);
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
      const systemBroadcastText = createRoundEventSystemBroadcast(
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
        recentRoundEvents: appendRecentRoundEvent(state.recentRoundEvents, message),
        optimisticDiscard: reconcileOptimisticDiscardWithRoundEvent(
          state.optimisticDiscard ?? null,
          message,
          state.roomSnapshot,
        ),
        latestReplacementTileId: nextReplacementTileId,
        latestSystemBroadcast: systemBroadcastText ? createSystemBroadcast(systemBroadcastText) : state.latestSystemBroadcast,
        toasts: appendToast(state, 'event', text),
      };
    }
    case 'quick_chat':
      return {
        ...state,
        latestQuickChatMessage: message,
      };
    case 'player_presence': {
      const presenceBroadcastText = createPresenceSystemBroadcast(
        message.payload,
        state.roomSnapshot?.payload.seats,
      );
      return {
        ...state,
        latestSystemBroadcast: presenceBroadcastText ? createSystemBroadcast(presenceBroadcastText) : state.latestSystemBroadcast,
        toasts: appendToast(
          state,
          'presence',
          getPresenceCopy(message.payload, state.roomSnapshot?.payload.seats),
        ),
      };
    }
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
    case 'set_room_snapshot':
      return applyRoomSnapshotMessage(state, action.message);
    case 'queue_optimistic_discard': {
      const optimisticDiscard = createOptimisticDiscard(state, action.tileId, action.actionType);
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
    case 'user_points_updated': {
      const nextRoomSnapshot = updateSeatPointSnapshot(state.roomSnapshot, action.message);
      const sourceTableCode = action.message.payload.source_table_code;
      const isCurrentTableUpdate =
        Boolean(sourceTableCode) && sourceTableCode === (state.roomSnapshot?.payload.table_code ?? state.tableCode);
      const systemBroadcastText = isCurrentTableUpdate
        ? createTitleChangeSystemBroadcast(action.message.payload)
        : null;

      return {
        ...state,
        roomSnapshot: nextRoomSnapshot,
        latestSystemBroadcast: systemBroadcastText ? createSystemBroadcast(systemBroadcastText) : state.latestSystemBroadcast,
      };
    }
    case 'ws_message':
      return applyServerMessage(state, action.message);
    default:
      return state;
  }
}
