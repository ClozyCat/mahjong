import { loadStoredAuthSession } from '../lib/authApi';

export const HEARTBEAT_INTERVAL_MS = 20_000;
export const SOCIAL_REFRESH_INTERVAL_MS = 15_000;
export const SOCIAL_SOCKET_RECONNECT_MS = 1_000;
export const TABLE_SEAT_CAPACITY = 4;
export const ACTIVE_TABLE_LOOKUP_MESSAGE = '正在检查当前账号所在牌桌...';
export const ACTIVE_TABLE_RETRY_MESSAGE = '牌桌连接已断开，正在重连你当前所在的牌桌。';
export const CLAIM_ACTION_IDS = ['chow', 'pung', 'kong'] as const;
export const CLAIM_RESPONSE_ACTION_IDS = ['chow', 'pung', 'kong', 'hu'] as const;

export type AuthStatus = 'loading' | 'anonymous' | 'ready';
export type SentInviteStatus = 'pending' | 'rejected';
export type RoomSocketOptions = {
  tableCode: string;
  nickname: string;
  wsBaseUrl: string;
  sessionToken?: string | null;
  reconnect?: boolean;
};

function getRuntimeDefaultBaseUrls() {
  if (typeof window === 'undefined') {
    return {
      apiBaseUrl: 'http://localhost:8000',
      wsBaseUrl: 'ws://localhost:8000',
    };
  }

  const { origin, protocol, host } = window.location;
  return {
    apiBaseUrl: origin,
    wsBaseUrl: `${protocol === 'https:' ? 'wss' : 'ws'}://${host}`,
  };
}

export function getDefaultConfig() {
  const env = ((import.meta as ImportMeta & { env?: Record<string, string | undefined> }).env ?? {});
  const runtimeDefaults = getRuntimeDefaultBaseUrls();
  const defaults = {
    apiBaseUrl: env.VITE_API_BASE_URL ?? runtimeDefaults.apiBaseUrl,
    wsBaseUrl: env.VITE_WS_BASE_URL ?? runtimeDefaults.wsBaseUrl,
  };
  const storedAuthSession = loadStoredAuthSession();

  return {
    defaults,
    storedAuthSession,
  };
}

