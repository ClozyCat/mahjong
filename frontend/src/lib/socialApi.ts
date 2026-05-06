import type {
  AcceptInviteResponse,
  CreateTableResponse,
  PublicUser,
  SpectatorRequest,
  TableInvite,
  TableMultiplier,
} from '../types/match';

function normalizeBaseUrl(baseUrl: string) {
  return baseUrl.replace(/\/+$/, '');
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

  return (await response.json()) as T;
}

export function createSocialTable(baseUrl: string, sessionToken: string, multiplier: TableMultiplier) {
  return requestJson<CreateTableResponse>(`${normalizeBaseUrl(baseUrl)}/api/tables`, {
    method: 'POST',
    headers: {
      ...authHeaders(sessionToken),
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({ multiplier }),
  });
}

export function getLeaderboard(baseUrl: string) {
  return requestJson<PublicUser[]>(`${normalizeBaseUrl(baseUrl)}/api/leaderboard`);
}

export function getMyInvites(baseUrl: string, sessionToken: string) {
  return requestJson<TableInvite[]>(`${normalizeBaseUrl(baseUrl)}/api/me/invites`, {
    headers: authHeaders(sessionToken),
  });
}

export function createTableInvite(
  baseUrl: string,
  sessionToken: string,
  tableCode: string,
  inviteeUserId: number,
) {
  return requestJson<TableInvite>(`${normalizeBaseUrl(baseUrl)}/api/tables/${tableCode}/invites`, {
    method: 'POST',
    headers: {
      ...authHeaders(sessionToken),
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      invitee_user_id: inviteeUserId,
    }),
  });
}

export function acceptTableInvite(baseUrl: string, sessionToken: string, inviteId: number) {
  return requestJson<AcceptInviteResponse>(`${normalizeBaseUrl(baseUrl)}/api/invites/${inviteId}/accept`, {
    method: 'POST',
    headers: {
      ...authHeaders(sessionToken),
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({}),
  });
}

export function getMySpectatorRequests(baseUrl: string, sessionToken: string) {
  return requestJson<SpectatorRequest[]>(`${normalizeBaseUrl(baseUrl)}/api/me/spectator-requests`, {
    headers: authHeaders(sessionToken),
  });
}
