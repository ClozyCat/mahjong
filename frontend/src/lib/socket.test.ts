import { describe, expect, it } from 'vitest';

import { createJoinTableMessage, createSetBotTakeoverMessage, createSetDealerDoubleMessage, createSetDealerRepeatMessage } from './socket';

describe('socket message builders', () => {
  it('creates an authenticated join_table message', () => {
    expect(createJoinTableMessage('session-token-1')).toEqual({
      type: 'join_table',
      payload: {
        session_token: 'session-token-1',
      },
    });
  });

  it('creates a bot takeover toggle message', () => {
    expect(createSetBotTakeoverMessage(true)).toEqual({
      type: 'set_bot_takeover',
      payload: {
        enabled: true,
      },
    });
  });

  it('creates dealer rule toggle messages', () => {
    expect(createSetDealerRepeatMessage(true)).toEqual({
      type: 'set_dealer_repeat',
      payload: {
        enabled: true,
      },
    });
    expect(createSetDealerDoubleMessage(false)).toEqual({
      type: 'set_dealer_double',
      payload: {
        enabled: false,
      },
    });
  });
});
