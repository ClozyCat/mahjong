import type { SocialServerMessage } from '../types/match';

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

export function buildMeSocketUrl(baseUrl: string, sessionToken: string) {
  const url = new URL(`${normalizeBaseUrl(baseUrl)}/ws/me`);
  url.searchParams.set('session_token', sessionToken);
  return url.toString();
}

export function parseSocialServerMessage(raw: string): SocialServerMessage | null {
  try {
    return JSON.parse(raw) as SocialServerMessage;
  } catch {
    return null;
  }
}
