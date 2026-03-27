import type {
  ActionRejectedMessage,
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
  unsupported_message: '客户端发送了服务器不支持的消息。',
};

function createToast(kind: ToastMessage['kind'], text: string): ToastMessage {
  return {
    id: `${kind}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    kind,
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

function getPresenceCopy(message: { seat_index: number; connected: boolean }, seats?: SeatSnapshot[]) {
  const nickname = seats?.find((seat) => seat.seat_index === message.seat_index)?.nickname ?? `玩家${message.seat_index + 1}`;
  return `${nickname}${message.connected ? '已连接' : '已断开连接'}`;
}

function getActionRejectedCopy(reason: string) {
  return ACTION_REJECTION_COPY[reason] ?? reason;
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
    lastRejectedAction: null,
    reconnectToken: null,
    selectedTileIds: [],
    selectionMode: null,
    toasts: [],
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
      const availableTileIds = localPlayer?.concealed_tiles?.map((tile) => tile.tile_id) ?? null;
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
        latestActionPrompt: message.payload.phase === 'playing' ? state.latestActionPrompt : null,
        selectedTileIds: nextSelectedTileIds,
        selectionMode: nextSelectedTileIds.length > 0 ? state.selectionMode : null,
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

      return {
        ...state,
        latestRoundEvent: message,
        toasts: appendToast(state, 'event', text),
      };
    }
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
