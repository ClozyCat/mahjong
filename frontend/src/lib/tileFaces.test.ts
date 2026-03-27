import { describe, expect, it } from 'vitest';

import { getTileFace } from './tileFaces';

describe('getTileFace', () => {
  it('maps wan tiles to a wan suit face', () => {
    expect(getTileFace('w1')).toMatchObject({
      kind: 'suited',
      suit: 'wan',
      rank: 1,
      glyph: '一',
      accent: 'crimson',
    });
  });

  it('maps tong and suo tiles to distinct suit metadata', () => {
    expect(getTileFace('b4')).toMatchObject({
      kind: 'suited',
      suit: 'tong',
      rank: 4,
      accent: 'ocean',
    });
    expect(getTileFace('c7')).toMatchObject({
      kind: 'suited',
      suit: 'suo',
      rank: 7,
      accent: 'jade',
    });
  });

  it('maps honor tiles to Chinese labels', () => {
    expect(getTileFace('east')).toMatchObject({
      kind: 'honor',
      label: '东',
      accent: 'ink',
    });
    expect(getTileFace('red')).toMatchObject({
      kind: 'honor',
      label: '中',
      accent: 'crimson',
    });
  });

  it('returns a readable fallback for unknown tile codes', () => {
    expect(getTileFace('mystery')).toMatchObject({
      kind: 'fallback',
      label: 'mystery',
      accent: 'ink',
    });
  });
});
