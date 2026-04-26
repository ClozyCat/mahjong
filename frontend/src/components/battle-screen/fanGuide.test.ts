import { describe, expect, it } from 'vitest';

import { getFanGuideEntry, getFanLabel } from './fanGuide';

describe('fanGuide', () => {
  it('maps ready_hand_win to 听牌成和 and two fan', () => {
    expect(getFanLabel('ready_hand_win')).toBe('听牌成和');
    expect(getFanGuideEntry('ready_hand_win')).toMatchObject({
      fanKey: 'ready_hand_win',
      fanValue: 2,
      label: '听牌成和',
    });
  });

  it('maps mixed_kongs to 明暗杠 and five fan', () => {
    expect(getFanLabel('mixed_kongs')).toBe('明暗杠');
    expect(getFanGuideEntry('mixed_kongs')).toMatchObject({
      fanKey: 'mixed_kongs',
      fanValue: 5,
      label: '明暗杠',
    });
  });
});
