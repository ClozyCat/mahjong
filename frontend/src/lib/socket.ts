import type { ActionRequestType, ClientMessage, QuickChatEmoji, ServerMessage } from '../types/match';

function normalizeBaseUrl(baseUrl: string) {
  const trimmed = baseUrl.replace(/\/+$/, '');
  if (trimmed.startsWith('http://')) {
    return `ws://${trimmed.slice('http://'.length)}`;
  }
  if (trimmed.startsWith('https://')) {
    return `wss://${trimmed.slice('https://'.length)}`;
  }
  return trimmed;
}

export function buildWebSocketUrl(baseUrl: string, tableCode: string) {
  return `${normalizeBaseUrl(baseUrl)}/ws/${tableCode}`;
}

export function serializeClientMessage(message: ClientMessage) {
  return JSON.stringify(message);
}

export function parseServerMessage(raw: string): ServerMessage | null {
  try {
    return JSON.parse(raw) as ServerMessage;
  } catch {
    return null;
  }
}

export function createJoinTableMessage(sessionToken: string): ClientMessage {
  return {
    type: 'join_table',
    payload: {
      session_token: sessionToken,
    },
  };
}

export function createLeaveTableMessage(): ClientMessage {
  return {
    type: 'leave_table',
    payload: {},
  };
}

export function createAdjustBotsMessage(delta: 1 | -1): ClientMessage {
  return {
    type: 'adjust_bots',
    payload: {
      delta,
    },
  };
}

export function createSetBotTakeoverMessage(enabled: boolean): ClientMessage {
  return {
    type: 'set_bot_takeover',
    payload: {
      enabled,
    },
  };
}

export function createStartMatchMessage(): ClientMessage {
  return {
    type: 'start_match',
    payload: {},
  };
}

export function createStartNextRoundMessage(): ClientMessage {
  return {
    type: 'start_next_round',
    payload: {},
  };
}

export function createRestartMatchMessage(): ClientMessage {
  return {
    type: 'restart_match',
    payload: {},
  };
}

export function createActionRequestMessage(actionType: ActionRequestType, tileIds?: string[]): ClientMessage {
  return {
    type: 'action_request',
    payload: {
      action_type: actionType,
      tile_ids: tileIds ?? [],
    },
  };
}

export function createHeartbeatMessage(sentAt: string): ClientMessage {
  return {
    type: 'heartbeat',
    payload: {
      sent_at: sentAt,
    },
  };
}

export function createQuickChatMessage(targetSeat: number, emoji: QuickChatEmoji): ClientMessage {
  return {
    type: 'quick_chat',
    payload: {
      target_seat: targetSeat,
      emoji,
    },
  };
}
