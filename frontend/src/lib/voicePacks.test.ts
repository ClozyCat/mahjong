import { describe, expect, it, vi } from 'vitest';

import {
  getVoiceClipNameForAction,
  getVoiceClipNameForTile,
  getVoicePackNames,
  playVoiceClip,
  resolveVoiceClipUrl,
  selectVoicePackName,
} from './voicePacks';

const testVoiceAssets = {
  '../../voices/alpha/yi_wan.mp3': '/assets/alpha/yi_wan.mp3',
  '../../voices/alpha/dong.mp3': '/assets/alpha/dong.mp3',
  '../../voices/alpha/peng.mp3': '/assets/alpha/peng.mp3',
  '../../voices/beta/yi_wan.mp3': '/assets/beta/yi_wan.mp3',
  '../../voices/beta/dong.mp3': '/assets/beta/dong.mp3',
  '../../voices/beta/peng.mp3': '/assets/beta/peng.mp3',
};

describe('voicePacks', () => {
  it('maps suited tile codes to pinyin voice clip names', () => {
    expect(getVoiceClipNameForTile('w1')).toBe('yi_wan');
    expect(getVoiceClipNameForTile('m9')).toBe('jiu_wan');
    expect(getVoiceClipNameForTile('b4')).toBe('si_tong');
    expect(getVoiceClipNameForTile('p6')).toBe('liu_tong');
    expect(getVoiceClipNameForTile('c7')).toBe('qi_tiao');
    expect(getVoiceClipNameForTile('t8')).toBe('ba_tiao');
  });

  it('maps wind and dragon tile codes to pinyin voice clip names', () => {
    expect(getVoiceClipNameForTile('east')).toBe('dong');
    expect(getVoiceClipNameForTile('south')).toBe('nan');
    expect(getVoiceClipNameForTile('west')).toBe('xi');
    expect(getVoiceClipNameForTile('north')).toBe('bei');
    expect(getVoiceClipNameForTile('red')).toBe('zhong');
    expect(getVoiceClipNameForTile('green')).toBe('fa');
    expect(getVoiceClipNameForTile('white')).toBe('bai');
  });

  it('maps compact honor tile aliases to pinyin voice clip names', () => {
    expect(getVoiceClipNameForTile('d1')).toBe('dong');
    expect(getVoiceClipNameForTile('d2')).toBe('nan');
    expect(getVoiceClipNameForTile('d3')).toBe('xi');
    expect(getVoiceClipNameForTile('d4')).toBe('bei');
    expect(getVoiceClipNameForTile('d5')).toBe('zhong');
    expect(getVoiceClipNameForTile('d6')).toBe('fa');
    expect(getVoiceClipNameForTile('d7')).toBe('bai');
  });

  it('ignores tiles without matching voice clips', () => {
    expect(getVoiceClipNameForTile('f1')).toBeNull();
    expect(getVoiceClipNameForTile(null)).toBeNull();
  });

  it('maps claim and win tones to operation voice clip names', () => {
    expect(getVoiceClipNameForAction('chow')).toBe('chi');
    expect(getVoiceClipNameForAction('pung')).toBe('peng');
    expect(getVoiceClipNameForAction('kong')).toBe('gang');
    expect(getVoiceClipNameForAction('hu')).toBe('hu');
    expect(getVoiceClipNameForAction('ready_hand')).toBeNull();
  });

  it('discovers voice pack names from asset paths in stable order', () => {
    expect(getVoicePackNames(testVoiceAssets)).toEqual(['alpha', 'beta']);
  });

  it('selects the same voice pack for the same table and absolute seat', () => {
    const first = selectVoicePackName('AB12CD', 2, testVoiceAssets);
    const second = selectVoicePackName('AB12CD', 2, testVoiceAssets);

    expect(first).toBe(second);
    expect(['alpha', 'beta']).toContain(first);
  });

  it('resolves clip URLs through the selected seat voice pack', () => {
    const packName = selectVoicePackName('AB12CD', 2, testVoiceAssets);
    const url = resolveVoiceClipUrl('AB12CD', 2, 'yi_wan', testVoiceAssets);

    expect(url).toBe(`/assets/${packName}/yi_wan.mp3`);
  });

  it('silently starts audio playback when the browser Audio API is available', async () => {
    let endedHandler: (() => void) | undefined;
    const play = vi.fn(() => Promise.resolve());
    const audio = vi.fn(() => ({
      addEventListener: vi.fn((eventName: string, handler: () => void) => {
        if (eventName === 'ended') {
          endedHandler = handler;
        }
      }),
      removeEventListener: vi.fn(),
      play,
    }));
    const originalAudio = globalThis.Audio;

    globalThis.Audio = audio as unknown as typeof Audio;
    const playback = playVoiceClip('/assets/alpha/peng.mp3');

    await Promise.resolve();
    await Promise.resolve();

    expect(audio).toHaveBeenCalledWith('/assets/alpha/peng.mp3');
    expect(play).toHaveBeenCalled();

    endedHandler?.();
    await playback;

    globalThis.Audio = originalAudio;
  });

  it('allows concurrent voice playback and stops previous audio for the same seat', async () => {
    type EventHandler = () => void;

    const handlersByUrl = new Map<string, Map<string, EventHandler>>();
    const pauseFnsByUrl = new Map<string, ReturnType<typeof vi.fn>>();
    const play = vi.fn(() => Promise.resolve());
    const audio = vi.fn((url: string) => {
      const handlers = new Map<string, EventHandler>();
      handlersByUrl.set(url, handlers);
      const pause = vi.fn();
      pauseFnsByUrl.set(url, pause);

      return {
        addEventListener: vi.fn((eventName: string, handler: EventHandler) => {
          handlers.set(eventName, handler);
        }),
        removeEventListener: vi.fn(),
        play,
        pause,
        currentTime: 0,
      };
    });
    const originalAudio = globalThis.Audio;

    globalThis.Audio = audio as unknown as typeof Audio;

    try {
      const firstPlayback = playVoiceClip('/assets/alpha/peng.mp3', 0);
      const secondPlayback = playVoiceClip('/assets/alpha/gang.mp3', 1);

      await Promise.resolve();
      await Promise.resolve();

      // Both should start playing immediately
      expect(audio).toHaveBeenCalledTimes(2);
      expect(audio).toHaveBeenCalledWith('/assets/alpha/peng.mp3');
      expect(audio).toHaveBeenCalledWith('/assets/alpha/gang.mp3');

      // Now play another clip for seat 0, should stop the first one
      const thirdPlayback = playVoiceClip('/assets/alpha/hu.mp3', 0);
      await Promise.resolve();

      expect(pauseFnsByUrl.get('/assets/alpha/peng.mp3')).toHaveBeenCalled();
      expect(audio).toHaveBeenCalledTimes(3);
      expect(audio).toHaveBeenLastCalledWith('/assets/alpha/hu.mp3');

      handlersByUrl.get('/assets/alpha/peng.mp3')?.get('ended')?.();
      handlersByUrl.get('/assets/alpha/gang.mp3')?.get('ended')?.();
      handlersByUrl.get('/assets/alpha/hu.mp3')?.get('ended')?.();
      await Promise.all([firstPlayback, secondPlayback, thirdPlayback]);
    } finally {
      globalThis.Audio = originalAudio;
    }
  });
});
