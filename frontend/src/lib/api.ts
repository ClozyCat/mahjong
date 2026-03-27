import type { CreateTableResponse, HealthResponse } from '../types/match';

function normalizeBaseUrl(baseUrl: string) {
  return baseUrl.replace(/\/+$/, '');
}

async function requestJson<T>(input: string, init?: RequestInit): Promise<T> {
  const response = await fetch(input, init);
  if (!response.ok) {
    const body = await response.text();
    throw new Error(`Request failed with ${response.status}: ${body || response.statusText}`);
  }

  return (await response.json()) as T;
}

export function getHealth(baseUrl: string) {
  return requestJson<HealthResponse>(`${normalizeBaseUrl(baseUrl)}/api/health`);
}

export function createTable(baseUrl: string) {
  return requestJson<CreateTableResponse>(`${normalizeBaseUrl(baseUrl)}/api/tables`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
  });
}
