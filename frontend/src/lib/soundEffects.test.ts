import { describe, expect, it, vi } from 'vitest';

import { playClearComboSound, playHuSound, playItemPickUpSound, playReadyHandSound } from './soundEffects';

describe('soundEffects', () => {
  it('plays the clear combo sound when chi/peng/gang occurs', async () => {
    const play = vi.fn(() => Promise.resolve());
    const audio = vi.fn(() => ({
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      play,
    }));
    const originalAudio = globalThis.Audio;

    globalThis.Audio = audio as unknown as typeof Audio;
    const playback = playClearComboSound();

    await Promise.resolve();

    expect(audio).toHaveBeenCalledTimes(1);
    expect(play).toHaveBeenCalled();

    globalThis.Audio = originalAudio;
  });

  it('plays the hu sound when a win occurs', async () => {
    const play = vi.fn(() => Promise.resolve());
    const audio = vi.fn(() => ({
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      play,
    }));
    const originalAudio = globalThis.Audio;

    globalThis.Audio = audio as unknown as typeof Audio;
    const playback = playHuSound();

    await Promise.resolve();

    expect(audio).toHaveBeenCalledTimes(1);
    expect(play).toHaveBeenCalled();

    globalThis.Audio = originalAudio;
  });

  it('plays the ready hand sound when ready hand is declared', async () => {
    const play = vi.fn(() => Promise.resolve());
    const audio = vi.fn(() => ({
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      play,
    }));
    const originalAudio = globalThis.Audio;

    globalThis.Audio = audio as unknown as typeof Audio;
    const playback = playReadyHandSound();

    await Promise.resolve();

    expect(audio).toHaveBeenCalledTimes(1);
    expect(play).toHaveBeenCalled();

    globalThis.Audio = originalAudio;
  });

  it('plays the item pick-up sound for turn notification', async () => {
    const play = vi.fn(() => Promise.resolve());
    const audio = vi.fn(() => ({
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      play,
    }));
    const originalAudio = globalThis.Audio;

    globalThis.Audio = audio as unknown as typeof Audio;
    const playback = playItemPickUpSound();

    await Promise.resolve();

    expect(audio).toHaveBeenCalledTimes(1);
    expect(play).toHaveBeenCalled();

    globalThis.Audio = originalAudio;
  });
});
