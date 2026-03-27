import type { BackendActionType, ClientMessage, ServerMessage } from '../types/match';

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

export function createJoinTableMessage(nickname: string): ClientMessage {
  return {
    type: 'join_table',
    payload: {
      nickname,
    },
  };
}

export function createReconnectMessage(reconnectToken: string): ClientMessage {
  return {
    type: 'reconnect',
    payload: {
      reconnect_token: reconnectToken,
    },
  };
}

export function createReadyMessage(ready: boolean): ClientMessage {
  return {
    type: 'ready',
    payload: {
      ready,
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

export function createActionRequestMessage(actionType: BackendActionType, tileIds?: string[]): ClientMessage {
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
