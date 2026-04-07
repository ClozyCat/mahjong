import type {
  ActionRejectedMessage,
  MatchStatisticsState,
  MatchResultMessage,
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

function haveSeatKeysChanged(
  current: MatchStatisticsState | null | undefined,
  scores: Record<string, number> | null | undefined,
) {
  if (!current || !scores) {
    return false;
  }

  const currentSeatKeys = Object.keys(current.seatStatsBySeat).sort();
  const nextSeatKeys = Object.keys(scores).sort();

  return currentSeatKeys.join('|') !== nextSeatKeys.join('|');
}

function shouldResetMatchStatistics(
  current: MatchStatisticsState | null | undefined,
  snapshot: RoomSnapshotMessage,
) {
  const matchState = snapshot.payload.match_state;
  const scores = matchState?.cumulative_scores;

  if (!current || !matchState || !scores) {
    return false;
  }

  if (haveSeatKeysChanged(current, scores)) {
    return true;
  }

  const allScoresAreZero = Object.values(scores).every((score) => score === 0);
  return current.completedRoundCount > 0 && matchState.last_completed_round_id === null && allScoresAreZero;
}

function reconcileMatchStatistics(
  current: MatchStatisticsState | null | undefined,
  snapshot: RoomSnapshotMessage,
) {
  const matchScores = snapshot.payload.match_state?.cumulative_scores;

  if (!matchScores) {
    return current ?? null;
  }

  if (!current || shouldResetMatchStatistics(current, snapshot)) {
    return createMatchStatisticsFromScores(matchScores);
  }

  return current;
}

function applyMatchResultToStatistics(
  current: MatchStatisticsState | null | undefined,
  roomSnapshot: RoomSnapshotMessage | null,
  message: MatchResultMessage,
) {
  const roundId = message.payload.round_id;
  if (current?.lastAppliedRoundId === roundId) {
    return current;
  }

  const baseScores = roomSnapshot?.payload.match_state?.cumulative_scores ?? {};
  const initialized =
    current ??
    createMatchStatisticsFromScores(baseScores) ?? {
      completedRoundCount: 0,
      lastAppliedRoundId: null,
      seatStatsBySeat: {},
    };
  const seatKeys = new Set([
    ...Object.keys(initialized.seatStatsBySeat),
    ...Object.keys(baseScores),
    ...Object.keys(message.payload.score_delta.total_delta_by_seat),
  ]);
  const seatStatsBySeat = Object.fromEntries(
    Array.from(seatKeys).map((seatKey) => {
      const existingSeatStats = initialized.seatStatsBySeat[seatKey];
      const baseScore = baseScores[seatKey] ?? existingSeatStats?.scoreHistory.at(-1) ?? 0;
      const nextScore = baseScore + (message.payload.score_delta.total_delta_by_seat[seatKey] ?? 0);
      const scoreHistory = existingSeatStats?.scoreHistory.length
        ? [...existingSeatStats.scoreHistory, nextScore]
        : [baseScore, nextScore];
      const previousWinCount = existingSeatStats?.winCount ?? 0;

      return [
        seatKey,
        {
          scoreHistory,
          winCount: previousWinCount + (message.payload.winner_seat === Number(seatKey) ? 1 : 0),
        },
      ];
    }),
  );

  return {
    completedRoundCount: initialized.completedRoundCount + 1,
    lastAppliedRoundId: roundId,
    seatStatsBySeat,
  };
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
    selectedTileIds: [],
    selectionMode: null,
    toasts: [],
    matchStatistics: null,
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

      return {
        ...state,
        roomSnapshot: message,
        tableCode: message.payload.table_code,
        reconnectToken: message.payload.reconnect_token ?? state.reconnectToken,
        lastRejectedAction: null,
        latestMatchResult: keepLatestResult ? state.latestMatchResult : null,
        latestActionPrompt: null,
        selectedTileIds: nextSelectedTileIds,
        selectionMode: nextSelectedTileIds.length > 0 ? state.selectionMode : null,
        matchStatistics: reconcileMatchStatistics(state.matchStatistics, message),
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
        matchStatistics: applyMatchResultToStatistics(state.matchStatistics, state.roomSnapshot, message),
      };
    case 'round_event': {
      const text = getRoundEventCopy(
        message.payload.event_type,
        message.payload.event,
        state.roomSnapshot?.payload.seats,
      );

      return {
        ...state,
        latestRoundEvent: getNextLatestRoundEvent(state.latestRoundEvent, message),
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
