import type { ClientMessage } from '../../types/match';

export function createWatchTableMessage(nickname: string): ClientMessage {
  return {
    type: 'watch_table',
    payload: {
      nickname,
    },
  };
}
