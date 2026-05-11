import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  clearStoredSession,
  loadStoredVoiceEnabled,
  loadStoredThemeId,
  saveStoredVoiceEnabled,
  saveStoredThemeId,
} from './storage';

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

  it('clears legacy reconnect session payloads', () => {
    localStorage.setItem('mahjong:session', 'legacy-session');

    clearStoredSession();

    expect(localStorage.getItem('mahjong:session')).toBeNull();
  });

  it('round-trips stored theme ids', () => {
    saveStoredThemeId('qiu-xiang');

    expect(loadStoredThemeId()).toBe('qiu-xiang');
  });

  it('defaults voice playback to enabled when no preference is stored', () => {
    expect(loadStoredVoiceEnabled()).toBe(true);
  });

  it('round-trips stored voice playback preferences', () => {
    saveStoredVoiceEnabled(false);
    expect(loadStoredVoiceEnabled()).toBe(false);

    saveStoredVoiceEnabled(true);
    expect(loadStoredVoiceEnabled()).toBe(true);
  });
});
