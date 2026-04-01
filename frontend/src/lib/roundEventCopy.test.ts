import { describe, expect, it } from 'vitest';

import { getRoundEventCopy } from './roundEventCopy';

describe('getRoundEventCopy', () => {
  it('maps tile_discarded to player-and-tile Chinese UI copy', () => {
    expect(
      getRoundEventCopy(
        'tile_discarded',
        {
          seat: 0,
          tile_id: 't5#p0-3',
        },
        [{ seat_index: 0, nickname: '小李', connected: true, ready: true }],
      ),
    ).toBe('小李打出五条');
  });

  it('maps self_hu_declared to a Chinese action confirmation copy', () => {
    expect(
      getRoundEventCopy(
        'self_hu_declared',
        {
          seat: 0,
          tile_id: 't5#p0-3',
        },
        [{ seat_index: 0, nickname: '小李', connected: true, ready: true }],
      ),
    ).toBe('小李已点和');
  });

  it('falls back to a generic Chinese system message for unknown events', () => {
    expect(getRoundEventCopy('mystery_event')).toBe('牌局状态已更新');
  });
});
