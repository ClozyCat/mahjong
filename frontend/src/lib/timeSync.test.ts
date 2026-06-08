import { describe, expect, it, vi } from 'vitest';

import { createServerNowOffsetMs, getRemainingSeconds } from './timeSync';

describe('timeSync', () => {
  it('calculates countdowns from calibrated server time when the client clock is behind', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-04-02T11:59:00Z'));

    const offsetMs = createServerNowOffsetMs('2026-04-02T12:00:00Z');

    expect(getRemainingSeconds('2026-04-02T12:00:15Z', offsetMs)).toBe(15);

    vi.useRealTimers();
  });

  it('uses heartbeat round-trip midpoint when client send and receive times are available', () => {
    const offsetMs = createServerNowOffsetMs(
      '2026-04-02T12:00:02Z',
      Date.parse('2026-04-02T11:59:01Z'),
      Date.parse('2026-04-02T11:59:03Z'),
    );

    expect(offsetMs).toBe(60_000);
  });
});
