export function createServerNowOffsetMs(
  serverNow: string | null | undefined,
  clientSentAtMs?: number,
  clientReceivedAtMs = Date.now(),
) {
  const serverNowMs = Date.parse(serverNow ?? '');
  if (!Number.isFinite(serverNowMs)) {
    return 0;
  }

  const clientReferenceMs =
    typeof clientSentAtMs === 'number' && Number.isFinite(clientSentAtMs)
      ? (clientSentAtMs + clientReceivedAtMs) / 2
      : clientReceivedAtMs;

  return Math.round(serverNowMs - clientReferenceMs);
}

export function getServerNowMs(serverNowOffsetMs = 0) {
  return Date.now() + serverNowOffsetMs;
}

export function getRemainingMs(deadlineAt: string, serverNowOffsetMs = 0) {
  const deadlineMs = new Date(deadlineAt).getTime();
  if (!Number.isFinite(deadlineMs)) {
    return 0;
  }

  return Math.max(0, deadlineMs - getServerNowMs(serverNowOffsetMs));
}

export function getRemainingSeconds(deadlineAt: string, serverNowOffsetMs = 0) {
  return Math.max(0, Math.ceil(getRemainingMs(deadlineAt, serverNowOffsetMs) / 1000));
}
