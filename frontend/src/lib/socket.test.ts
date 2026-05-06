import { describe, expect, it } from 'vitest';

import { createJoinTableMessage, createSetBotTakeoverMessage, createWatchTableMessage } from './socket';

describe('socket message builders', () => {
  it('creates an authenticated join_table message', () => {
    expect(createJoinTableMessage('session-token-1')).toEqual({
      type: 'join_table',
      payload: {
        session_token: 'session-token-1',
      },
    });
  });

  it('creates an authenticated watch_table message', () => {
    expect(createWatchTableMessage('session-token-1', '观众')).toEqual({
      type: 'watch_table',
      payload: {
        session_token: 'session-token-1',
        nickname: '观众',
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
});
