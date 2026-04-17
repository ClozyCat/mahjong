/// <reference types="vite/client" />

const env = import.meta.env as Record<string, string | undefined>;

export function getApiBaseUrl(): string {
  const configured = env.VITE_API_BASE_URL;
  if (configured) return configured.replace(/\/$/, "");
  return window.location.origin;
}

export function getWsBaseUrl(): string {
  const configured = env.VITE_WS_BASE_URL;
  if (configured) return configured.replace(/\/$/, "");
  const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${proto}//${window.location.host}`;
}
