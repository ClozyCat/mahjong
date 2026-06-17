import { describe, expect, it, vi } from 'vitest';

import { playButtonSound, playClearComboSound, playItemPickUpSound } from './soundEffects';

describe('soundEffects', () => {
  it('plays the button sound when a discard occurs', async () => {
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
    const playback = playButtonSound();

    await Promise.resolve();
    await Promise.resolve();

    expect(audio).toHaveBeenCalledTimes(1);
    expect(play).toHaveBeenCalled();

    endedHandler?.();
    await playback;

    globalThis.Audio = originalAudio;
  });

  it('plays the clear combo sound when chi/peng/gang/hu occurs', async () => {
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
