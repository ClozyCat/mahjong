import { describe, expect, it } from 'vitest';

import { createSetBotTakeoverMessage } from './socket';

describe('socket message builders', () => {
  it('creates a bot takeover toggle message', () => {
    expect(createSetBotTakeoverMessage(true)).toEqual({
      type: 'set_bot_takeover',
      payload: {
        enabled: true,
      },
    });
  });
});
