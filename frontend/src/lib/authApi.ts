import type { AuthResponse, PublicUser } from '../types/match';

const AUTH_SESSION_KEY = 'mahjong:auth';

export interface StoredAuthSession {
  sessionToken: string;
  user: PublicUser;
}

interface RegisterPayload {
  inviteCode: string;
  displayName: string;
  password: string;
}

interface LoginPayload {
  identifier: string;
  password: string;
}

interface UpdateMePayload {
  display_name?: string;
  bio?: string;
  avatar?: string | null;
}

function normalizeBaseUrl(baseUrl: string) {
  return baseUrl.replace(/\/+$/, '');
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

function authHeaders(sessionToken: string) {
  return {
    Authorization: `Bearer ${sessionToken}`,
  };
}

async function requestJson<T>(input: string, init?: RequestInit): Promise<T> {
  const response = await fetch(input, init);
  if (!response.ok) {
    const body = await response.text();
    let detail: unknown = body;

    if (body) {
      try {
        detail = JSON.parse(body);
      } catch {
        detail = body;
      }
    }

    const detailText =
      typeof detail === 'object' && detail !== null && 'detail' in detail
        ? String((detail as { detail: unknown }).detail)
        : body || response.statusText;
    throw new Error(detailText);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return (await response.json()) as T;
}

export function loadStoredAuthSession(): StoredAuthSession | null {
  const stored = safeParse<StoredAuthSession>(localStorage.getItem(AUTH_SESSION_KEY));
  if (!stored?.sessionToken || !stored.user?.user_id) {
    return null;
  }

  return stored;
}

export function saveStoredAuthSession(session: StoredAuthSession) {
  localStorage.setItem(AUTH_SESSION_KEY, JSON.stringify(session));
}

export function clearStoredAuthSession() {
  localStorage.removeItem(AUTH_SESSION_KEY);
}

export function registerWithInvite(baseUrl: string, payload: RegisterPayload) {
  return requestJson<AuthResponse>(`${normalizeBaseUrl(baseUrl)}/api/auth/register`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      invite_code: payload.inviteCode,
      display_name: payload.displayName,
      password: payload.password,
    }),
  });
}

export function loginWithPassword(baseUrl: string, payload: LoginPayload) {
  return requestJson<AuthResponse>(`${normalizeBaseUrl(baseUrl)}/api/auth/login`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(payload),
  });
}

export function logoutSession(baseUrl: string, sessionToken: string) {
  return requestJson<void>(`${normalizeBaseUrl(baseUrl)}/api/auth/logout`, {
    method: 'POST',
    headers: authHeaders(sessionToken),
  });
}

export function getMe(baseUrl: string, sessionToken: string) {
  return requestJson<PublicUser>(`${normalizeBaseUrl(baseUrl)}/api/me`, {
    headers: authHeaders(sessionToken),
  });
}

export function updateMe(baseUrl: string, sessionToken: string, payload: UpdateMePayload) {
  return requestJson<PublicUser>(`${normalizeBaseUrl(baseUrl)}/api/me`, {
    method: 'PATCH',
    headers: {
      ...authHeaders(sessionToken),
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(payload),
  });
}
