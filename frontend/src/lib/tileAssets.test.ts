import { describe, expect, it } from 'vitest';

import { getTileAsset } from './tileAssets';

describe('getTileAsset', () => {
  it('maps suited tiles to the expected svg file names', () => {
    expect(getTileAsset('w1')).toMatchObject({
      kind: 'image',
      assetName: '1man.svg',
    });
    expect(getTileAsset('b4')).toMatchObject({
      kind: 'image',
      assetName: '4pin.svg',
    });
    expect(getTileAsset('c7')).toMatchObject({
      kind: 'image',
      assetName: '7sou.svg',
    });
  });

  it('supports alternate suited aliases so sou tiles still resolve when keyed as t-rank', () => {
    expect(getTileAsset('t7')).toMatchObject({
      kind: 'image',
      assetName: '7sou.svg',
    });
  });

  it('normalizes svg assets into a full-tile viewBox data uri', () => {
    const tile = getTileAsset('c7');

    expect(tile).toMatchObject({
      kind: 'image',
      assetName: '7sou.svg',
    });
    expect(tile.kind === 'image' ? tile.src : '').toContain('viewBox');
    expect(tile.kind === 'image' ? tile.src : '').toContain('preserveAspectRatio');
  });

  it('maps honors and dragon aliases to the expected assets', () => {
    expect(getTileAsset('east')).toMatchObject({
      kind: 'image',
      assetName: 'east.svg',
    });
    expect(getTileAsset('d5')).toMatchObject({
      kind: 'image',
      assetName: 'zhong.svg',
    });
    expect(getTileAsset('d6')).toMatchObject({
      kind: 'image',
      assetName: 'fa.svg',
    });
  });

  it('renders white dragon aliases as blank faces', () => {
    expect(getTileAsset('white')).toEqual({ kind: 'blank' });
    expect(getTileAsset('d7')).toEqual({ kind: 'blank' });
  });

  it('returns a neutral placeholder for unknown tile codes', () => {
    expect(getTileAsset('mystery')).toEqual({ kind: 'placeholder' });
  });
});
