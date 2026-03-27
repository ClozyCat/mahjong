const ENDPOINTS_KEY = 'mahjong:endpoints';
const SESSION_KEY = 'mahjong:session';

export interface StoredConfig {
  apiBaseUrl: string;
  wsBaseUrl: string;
}

export interface StoredSession {
  tableCode: string;
  nickname: string;
  reconnectToken: string;
  wsBaseUrl: string;
}

function safeParse<T>(value: string | null): T | null {
  if (!value) {
    return null;
  }

  try {
    return JSON.parse(value) as T;
  } catch {
    return null;
  }
}

export function loadStoredConfig(defaults: StoredConfig): StoredConfig {
  const stored = safeParse<Partial<StoredConfig>>(localStorage.getItem(ENDPOINTS_KEY));
  return {
    apiBaseUrl: stored?.apiBaseUrl ?? defaults.apiBaseUrl,
    wsBaseUrl: stored?.wsBaseUrl ?? defaults.wsBaseUrl,
  };
}

export function saveStoredConfig(config: StoredConfig) {
  localStorage.setItem(ENDPOINTS_KEY, JSON.stringify(config));
}

export function loadStoredSession(): StoredSession | null {
  const stored = safeParse<StoredSession>(localStorage.getItem(SESSION_KEY));
  if (!stored?.tableCode || !stored?.reconnectToken || !stored?.wsBaseUrl) {
    return null;
  }

  return stored;
}

export function saveStoredSession(session: StoredSession) {
  localStorage.setItem(SESSION_KEY, JSON.stringify(session));
}

export function clearStoredSession() {
  localStorage.removeItem(SESSION_KEY);
}
