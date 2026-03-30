import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { clearStoredSession, loadStoredSession, saveStoredSession } from './storage';

function createMemoryStorage(): Storage {
  const store = new Map<string, string>();

  return {
    get length() {
      return store.size;
    },
    clear() {
      store.clear();
    },
    getItem(key) {
      return store.has(key) ? store.get(key)! : null;
    },
    key(index) {
      return Array.from(store.keys())[index] ?? null;
    },
    removeItem(key) {
      store.delete(key);
    },
    setItem(key, value) {
      store.set(key, value);
    },
  };
}

describe('storage helpers', () => {
  beforeEach(() => {
    vi.stubGlobal('localStorage', createMemoryStorage());
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('round-trips reconnect session payloads', () => {
    saveStoredSession({
      tableCode: 'AB12CD',
      nickname: 'Player A',
      reconnectToken: 'token-1',
      wsBaseUrl: 'ws://localhost:8080',
    });

    expect(loadStoredSession()).toEqual({
      tableCode: 'AB12CD',
      nickname: 'Player A',
      reconnectToken: 'token-1',
      wsBaseUrl: 'ws://localhost:8080',
    });
  });

  it('clears reconnect session payloads', () => {
    saveStoredSession({
      tableCode: 'AB12CD',
      nickname: 'Player A',
      reconnectToken: 'token-1',
      wsBaseUrl: 'ws://localhost:8080',
    });

    clearStoredSession();

    expect(loadStoredSession()).toBeNull();
  });
});
