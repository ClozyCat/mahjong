import { describe, expect, it } from 'vitest';

import { formatBeijingDateTime } from './dateTime';

describe('formatBeijingDateTime', () => {
  it('formats ISO timestamps as 24-hour Beijing time to seconds', () => {
    expect(formatBeijingDateTime('2026-05-06T12:00:05Z')).toBe('2026-05-06 20:00:05');
  });

  it('returns the original value when timestamp parsing fails', () => {
    expect(formatBeijingDateTime('not-a-time')).toBe('not-a-time');
  });
});
