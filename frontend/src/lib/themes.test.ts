import { afterEach, describe, expect, it, vi } from 'vitest';

import { getRandomThemeId, THEME_OPTIONS } from './themes';

describe('theme helpers', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('returns a random theme from the configured zhongguose palette', () => {
    vi.spyOn(Math, 'random').mockReturnValue(0);

    expect(getRandomThemeId()).toBe(THEME_OPTIONS[0].id);
  });

  it('can avoid repeating the current theme when another option is available', () => {
    vi.spyOn(Math, 'random').mockReturnValue(0);

    expect(getRandomThemeId('tian-shui-bi')).toBe(THEME_OPTIONS[1].id);
  });
});
