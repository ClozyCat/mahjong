import { describe, expect, it } from 'vitest';

import { getReadyHandWaits } from './readyHand';

describe('getReadyHandWaits', () => {
  it('derives standard waits and remaining counts from known tiles', () => {
    expect(
      getReadyHandWaits({
        concealedTileKeys: ['w1', 'w2', 'w3', 'w4', 'w5', 'w6', 'w7', 'w8', 'w9', 't1', 't2', 't3', 't4'],
        meldTileKeyGroups: [],
        knownTileKeys: ['w1', 'w2', 'w3', 'w4', 'w5', 'w6', 'w7', 'w8', 'w9', 't1', 't2', 't3', 't4', 't1'],
      }),
    ).toEqual([
      { code: 't1', availableCount: 2 },
      { code: 't4', availableCount: 3 },
    ]);
  });

  it('supports special waits such as seven pairs', () => {
    expect(
      getReadyHandWaits({
        concealedTileKeys: ['w1', 'w1', 'w2', 'w2', 'w3', 'w3', 'w4', 'w4', 'w5', 'w5', 'w6', 'w6', 'w7'],
        meldTileKeyGroups: [],
        knownTileKeys: ['w1', 'w1', 'w2', 'w2', 'w3', 'w3', 'w4', 'w4', 'w5', 'w5', 'w6', 'w6', 'w7'],
      }),
    ).toEqual(
      expect.arrayContaining([
        { code: 'w1', availableCount: 2 },
        { code: 'w4', availableCount: 2 },
        { code: 'w7', availableCount: 3 },
      ]),
    );
  });
});
