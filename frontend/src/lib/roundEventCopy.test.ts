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
    ).toBe('小李打出5条');
    });

  it('falls back to a generic Chinese system message for unknown events', () => {
    expect(getRoundEventCopy('mystery_event')).toBe('牌局状态已更新');
  });
});
