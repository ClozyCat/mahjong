const SESSION_KEY = 'mahjong:session';

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
