import type {
  ActionPromptPayload,
  MatchResultPayload,
  PlayerPresencePayload,
  QuickChatPayload,
  RoomSnapshot,
  RoundEvent,
} from "../types/protocol";

export type AppView =
  | "lobby"
  | "connecting"
  | "reconnecting"
  | "table"
  | "error";

export interface ToastItem {
  id: string;
  message: string;
  tone: "info" | "warn" | "error";
  createdAt: number;
}

export interface ChatBubble {
  messageId: string;
  actorSeat: number;
  targetSeat: number;
  emoji: string;
  sentAt: number;
}

export interface RoundEventTag {
  seq: number;
  event: RoundEvent;
  receivedAt: number;
}

export interface SessionState {
  view: AppView;
  tableCode: string | null;
  nickname: string;
  reconnectToken: string | null;
  wsBaseUrl: string;
  snapshot: RoomSnapshot | null;
  prompt: ActionPromptPayload | null;
  matchResult: MatchResultPayload | null;
  latestEvent: RoundEventTag | null;
  eventSeq: number;
  toasts: ToastItem[];
  chatBubbles: ChatBubble[];
  optimisticDiscardTileId: string | null;
  optimisticFlowerTileId: string | null;
  error: string | null;
  reconnectAttempts: number;
}

export const INITIAL_STATE: SessionState = {
  view: "lobby",
  tableCode: null,
  nickname: "",
  reconnectToken: null,
  wsBaseUrl: "",
  snapshot: null,
  prompt: null,
  matchResult: null,
  latestEvent: null,
  eventSeq: 0,
  toasts: [],
  chatBubbles: [],
  optimisticDiscardTileId: null,
  optimisticFlowerTileId: null,
  error: null,
  reconnectAttempts: 0,
};

export type Action =
  | {
      type: "begin_connect";
      tableCode: string;
      nickname: string;
      wsBaseUrl: string;
    }
  | { type: "begin_reconnect" }
  | { type: "reset_to_lobby"; error?: string }
  | { type: "apply_snapshot"; snapshot: RoomSnapshot }
  | { type: "apply_prompt"; prompt: ActionPromptPayload }
  | { type: "apply_match_result"; result: MatchResultPayload }
  | { type: "apply_round_event"; event: RoundEvent }
  | { type: "apply_presence"; data: PlayerPresencePayload }
  | { type: "apply_quick_chat"; data: QuickChatPayload }
  | { type: "apply_action_rejected"; reason: string }
  | { type: "socket_closed" }
  | { type: "push_toast"; message: string; tone?: ToastItem["tone"] }
  | { type: "dismiss_toast"; id: string }
  | { type: "set_optimistic_discard"; tileId: string | null }
  | { type: "set_optimistic_flower"; tileId: string | null }
  | { type: "clear_chat_bubble"; messageId: string }
  | { type: "bump_reconnect_attempts" }
  | { type: "reset_reconnect_attempts" };

function withToast(
  state: SessionState,
  message: string,
  tone: ToastItem["tone"] = "info",
): SessionState {
  const id = `${Date.now().toString(36)}-${Math.random()
    .toString(36)
    .slice(2, 6)}`;
  return {
    ...state,
    toasts: [
      ...state.toasts,
      { id, message, tone, createdAt: Date.now() },
    ].slice(-4),
  };
}

const fatalRejectReasons = new Set([
  "table_not_found",
  "invalid_reconnect_token",
]);

export function reduce(state: SessionState, action: Action): SessionState {
  switch (action.type) {
    case "begin_connect":
      return {
        ...INITIAL_STATE,
        view: "connecting",
        tableCode: action.tableCode,
        nickname: action.nickname,
        wsBaseUrl: action.wsBaseUrl,
      };
    case "begin_reconnect":
      return { ...state, view: "reconnecting" };
    case "reset_to_lobby":
      return {
        ...INITIAL_STATE,
        error: action.error ?? null,
      };
    case "apply_snapshot": {
      const snap = action.snapshot;
      // 乐观更新:若本地打出的牌仍在手中,说明被拒;清除
      let optDiscard = state.optimisticDiscardTileId;
      let optFlower = state.optimisticFlowerTileId;
      const me = snap.private_state?.players.find(
        (p) => p.seat_index === snap.local_seat,
      );
      const concealedIds = new Set(
        me?.concealed_tiles?.map((t) => t.tile_id) ?? [],
      );
      if (optDiscard && concealedIds.has(optDiscard)) optDiscard = null;
      if (optFlower && concealedIds.has(optFlower)) optFlower = null;
      // 若主/副状态不同则清除
      if (optDiscard && !concealedIds.has(optDiscard)) optDiscard = null;
      if (optFlower && !concealedIds.has(optFlower)) optFlower = null;
      return {
        ...state,
        view: "table",
        snapshot: snap,
        reconnectToken: snap.reconnect_token,
        tableCode: snap.table_code,
        reconnectAttempts: 0,
        optimisticDiscardTileId: optDiscard,
        optimisticFlowerTileId: optFlower,
        matchResult:
          snap.phase === "playing" && state.matchResult
            ? null
            : state.matchResult,
      };
    }
    case "apply_prompt":
      return { ...state, prompt: action.prompt };
    case "apply_match_result":
      return { ...state, matchResult: action.result };
    case "apply_round_event":
      return {
        ...state,
        eventSeq: state.eventSeq + 1,
        latestEvent: {
          seq: state.eventSeq + 1,
          event: action.event,
          receivedAt: Date.now(),
        },
      };
    case "apply_presence": {
      if (!state.snapshot) return state;
      const seat = state.snapshot.seats.find(
        (s) => s.seat_index === action.data.seat_index,
      );
      const who = seat?.nickname ?? `座位 ${action.data.seat_index + 1}`;
      const msg = action.data.connected
        ? `${who} 已连接`
        : `${who} 已离线`;
      return withToast(state, msg, action.data.connected ? "info" : "warn");
    }
    case "apply_quick_chat": {
      const bubble: ChatBubble = {
        messageId: action.data.message_id,
        actorSeat: action.data.actor_seat,
        targetSeat: action.data.target_seat,
        emoji: action.data.emoji,
        sentAt: Date.now(),
      };
      return {
        ...state,
        chatBubbles: [...state.chatBubbles.slice(-5), bubble],
      };
    }
    case "clear_chat_bubble":
      return {
        ...state,
        chatBubbles: state.chatBubbles.filter(
          (b) => b.messageId !== action.messageId,
        ),
      };
    case "apply_action_rejected": {
      if (fatalRejectReasons.has(action.reason)) {
        return {
          ...INITIAL_STATE,
          error: readableReason(action.reason),
        };
      }
      return withToast(
        {
          ...state,
          optimisticDiscardTileId: null,
          optimisticFlowerTileId: null,
        },
        readableReason(action.reason),
        "warn",
      );
    }
    case "socket_closed":
      if (state.view === "table" || state.view === "connecting") {
        return { ...state, view: "reconnecting" };
      }
      return state;
    case "push_toast":
      return withToast(state, action.message, action.tone ?? "info");
    case "dismiss_toast":
      return {
        ...state,
        toasts: state.toasts.filter((t) => t.id !== action.id),
      };
    case "set_optimistic_discard":
      return { ...state, optimisticDiscardTileId: action.tileId };
    case "set_optimistic_flower":
      return { ...state, optimisticFlowerTileId: action.tileId };
    case "bump_reconnect_attempts":
      return { ...state, reconnectAttempts: state.reconnectAttempts + 1 };
    case "reset_reconnect_attempts":
      return { ...state, reconnectAttempts: 0 };
    default:
      return state;
  }
}

const REASON_MAP: Record<string, string> = {
  table_not_found: "牌桌不存在",
  invalid_reconnect_token: "重连凭据失效",
  seat_already_owned: "座位已被占用",
  seat_not_owned: "当前没有座位",
  table_full: "牌桌已满",
  room_already_started: "本局已开始",
  room_not_ready: "房间未就绪",
  round_not_ready: "当前不可操作",
  match_not_finished: "对局尚未结束",
  invalid_action: "动作无效",
  select_tile_first: "请先选择牌",
  unsupported_message: "不支持的消息",
  room_full: "房间已满",
  bot_not_found: "bot 不存在",
  invalid_bot_adjustment: "无法调整 bot",
  skill_not_equipped: "技能未装备",
  skill_no_charges: "技能已耗尽",
  skill_requires_target: "技能需要目标",
  invalid_skill_target: "技能目标非法",
};

export function readableReason(reason: string): string {
  return REASON_MAP[reason] ?? reason;
}
