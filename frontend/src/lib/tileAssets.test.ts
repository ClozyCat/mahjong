import { describe, expect, it } from 'vitest';

import { getTileAsset } from './tileAssets';

describe('getTileAsset', () => {
  it('maps suited tiles to the expected svg file names', () => {
    expect(getTileAsset('w1')).toMatchObject({
      kind: 'image',
      assetName: '0101一萬.svg',
    });
    expect(getTileAsset('b4')).toMatchObject({
      kind: 'image',
      assetName: '0204四餅.svg',
    });
    expect(getTileAsset('c7')).toMatchObject({
      kind: 'image',
      assetName: '0307七條.svg',
    });
  });

  it('supports alternate suited aliases so sou tiles still resolve when keyed as t-rank', () => {
    expect(getTileAsset('t7')).toMatchObject({
      kind: 'image',
      assetName: '0307七條.svg',
    });
  });

  it('normalizes svg assets into a full-tile viewBox data uri', () => {
    const tile = getTileAsset('c7');

    expect(tile).toMatchObject({
      kind: 'image',
      assetName: '0307七條.svg',
    });
    expect(tile.kind === 'image' ? tile.src : '').toContain('viewBox');
    expect(tile.kind === 'image' ? tile.src : '').toContain('preserveAspectRatio');
  });

  it('maps honors and dragon aliases to the expected assets', () => {
    expect(getTileAsset('east')).toMatchObject({
      kind: 'image',
      assetName: '0401東風.svg',
    });
    expect(getTileAsset('d5')).toMatchObject({
      kind: 'image',
      assetName: '0405中.svg',
    });
    expect(getTileAsset('d6')).toMatchObject({
      kind: 'image',
      assetName: '0406發.svg',
    });
  });

  it('maps flower tiles to the newly added flower-face assets', () => {
    expect(getTileAsset('f1')).toMatchObject({
      kind: 'image',
      assetName: '0501春.svg',
    });
    expect(getTileAsset('f4')).toMatchObject({
      kind: 'image',
      assetName: '0504冬.svg',
    });
    expect(getTileAsset('f8')).toMatchObject({
      kind: 'image',
      assetName: '0507菊.svg',
    });
  });

  it('maps white dragon aliases to the dedicated white-tile asset', () => {
    expect(getTileAsset('white')).toMatchObject({
      kind: 'image',
      assetName: '0407白.svg',
    });
    expect(getTileAsset('d7')).toMatchObject({
      kind: 'image',
      assetName: '0407白.svg',
    });
  });

  it('returns a neutral placeholder for unknown tile codes', () => {
    expect(getTileAsset('mystery')).toEqual({ kind: 'placeholder' });
  });
});
