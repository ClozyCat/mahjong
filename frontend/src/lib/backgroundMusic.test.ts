import { describe, expect, it } from 'vitest';

import { createShuffledTrackOrder, getBackgroundMusicTracks } from './backgroundMusic';

describe('backgroundMusic', () => {
  it('orders background music tracks by file name with numeric sorting', () => {
    const tracks = getBackgroundMusicTracks({
      '../../bgm/PipaGameNight10.mp3': '/assets/PipaGameNight10.mp3',
      '../../bgm/PipaGameNight2.mp3': '/assets/PipaGameNight2.mp3',
      '../../bgm/PipaGameNight1.mp3': '/assets/PipaGameNight1.mp3',
    });

    expect(tracks).toEqual([
      { name: 'PipaGameNight1.mp3', url: '/assets/PipaGameNight1.mp3' },
      { name: 'PipaGameNight2.mp3', url: '/assets/PipaGameNight2.mp3' },
      { name: 'PipaGameNight10.mp3', url: '/assets/PipaGameNight10.mp3' },
    ]);
  });

  it('creates a shuffled play order that contains every track exactly once', () => {
    const order = createShuffledTrackOrder(4, () => 0);

    expect(order).toHaveLength(4);
    expect(new Set(order)).toEqual(new Set([0, 1, 2, 3]));
  });

  it('avoids starting the next shuffled cycle with the just-played track when possible', () => {
    const order = createShuffledTrackOrder(3, () => 0, 2);

    expect(order[0]).not.toBe(2);
    expect(new Set(order)).toEqual(new Set([0, 1, 2]));
  });

  it('returns an empty order for an empty playlist', () => {
    expect(createShuffledTrackOrder(0)).toEqual([]);
  });
});
