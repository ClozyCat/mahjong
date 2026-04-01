import { describe, expect, it } from 'vitest';

import { getTableCodeError, isTableCodeValid, normalizeTableCode } from './tableCode';

describe('table code helpers', () => {
  it('normalizes whitespace and casing', () => {
    expect(normalizeTableCode(' room42 ')).toBe('ROOM42');
  });

  it('accepts empty values when the code is optional', () => {
    expect(getTableCodeError('')).toBeNull();
    expect(isTableCodeValid('')).toBe(true);
  });

  it('rejects missing required values', () => {
    expect(getTableCodeError('', { required: true })).toBe('请输入牌桌编号。');
    expect(isTableCodeValid('', { required: true })).toBe(false);
  });

  it('rejects non-alphanumeric characters', () => {
    expect(getTableCodeError('牌局-01')).toBe('牌桌编号仅支持数字和英文字母。');
  });

  it('rejects overlong values', () => {
    expect(getTableCodeError('ABCDEFGHIJKLM')).toBe('牌桌编号最多 12 位。');
  });
});
