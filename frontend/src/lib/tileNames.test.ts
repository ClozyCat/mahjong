import { describe, expect, it } from 'vitest';

import { formatTileName } from './tileNames';

describe('formatTileName', () => {
  it('formats suited tiles with chinese numerals and suits', () => {
    expect(formatTileName('t2')).toBe('二条');
    expect(formatTileName('b7')).toBe('七筒');
    expect(formatTileName('w9')).toBe('九万');
  });

  it('formats honors and flowers with chinese names', () => {
    expect(formatTileName('east')).toBe('东风');
    expect(formatTileName('d5')).toBe('红中');
    expect(formatTileName('f6')).toBe('兰');
  });
});
