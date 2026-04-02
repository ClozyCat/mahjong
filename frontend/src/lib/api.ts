import type { CreateTableResponse, HealthResponse, TableMode } from '../types/match';

function normalizeBaseUrl(baseUrl: string) {
  return baseUrl.replace(/\/+$/, '');
}

export class ApiError extends Error {
  status: number;
  detail: unknown;

  constructor(status: number, message: string, detail: unknown) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.detail = detail;
  }
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
    throw new ApiError(response.status, `Request failed with ${response.status}: ${detailText}`, detail);
  }

  return (await response.json()) as T;
}

export function getHealth(baseUrl: string) {
  return requestJson<HealthResponse>(`${normalizeBaseUrl(baseUrl)}/api/health`);
}

export function createTable(
  baseUrl: string,
  tableCode?: string,
  mode: TableMode = 'normal',
  enforceMinimumEightFan = true,
) {
  return requestJson<CreateTableResponse>(`${normalizeBaseUrl(baseUrl)}/api/tables`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      ...(tableCode ? { table_code: tableCode } : {}),
      mode,
      enforce_minimum_eight_fan: enforceMinimumEightFan,
    }),
  });
}
