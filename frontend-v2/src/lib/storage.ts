const SESSION_KEY = "mahjong:session";
const THEME_KEY = "mahjong:theme";

export interface SessionSnapshot {
  tableCode: string;
  nickname: string;
  reconnectToken: string | null;
  wsBaseUrl: string;
}

export function loadSession(): SessionSnapshot | null {
  try {
    const raw = localStorage.getItem(SESSION_KEY);
    if (!raw) return null;
    const data = JSON.parse(raw) as Partial<SessionSnapshot>;
    if (!data.tableCode || !data.nickname) return null;
    return {
      tableCode: data.tableCode,
      nickname: data.nickname,
      reconnectToken: data.reconnectToken ?? null,
      wsBaseUrl: data.wsBaseUrl ?? "",
    };
  } catch {
    return null;
  }
}

export function saveSession(session: SessionSnapshot): void {
  try {
    localStorage.setItem(SESSION_KEY, JSON.stringify(session));
  } catch {
    /* ignore */
  }
}

export function clearSession(): void {
  try {
    localStorage.removeItem(SESSION_KEY);
  } catch {
    /* ignore */
  }
}

export function loadTheme(): string | null {
  try {
    return localStorage.getItem(THEME_KEY);
  } catch {
    return null;
  }
}

export function saveTheme(theme: string): void {
  try {
    localStorage.setItem(THEME_KEY, theme);
  } catch {
    /* ignore */
  }
}
