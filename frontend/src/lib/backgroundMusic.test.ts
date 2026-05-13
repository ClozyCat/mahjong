import { describe, expect, it, vi } from 'vitest';

import {
  buildLofiStationSearchUrl,
  createShuffledTrackOrder,
  fetchLofiBackgroundMusicTracks,
  normalizeRadioBrowserStations,
} from './backgroundMusic';

describe('backgroundMusic', () => {
  it('builds a Radio Browser lofi station lookup URL', () => {
    const url = new URL(buildLofiStationSearchUrl('https://example.test'));

    expect(url.origin).toBe('https://example.test');
    expect(url.pathname).toBe('/json/stations/search');
    expect(url.searchParams.get('tag')).toBe('lofi');
    expect(url.searchParams.get('hidebroken')).toBe('true');
    expect(url.searchParams.get('order')).toBe('random');
    expect(url.searchParams.get('limit')).toBe('24');
  });

  it('normalizes valid Radio Browser stations and removes duplicates', () => {
    const tracks = normalizeRadioBrowserStations([
      {
        name: ' Lofi Study ',
        stationuuid: 'station-1',
        url_resolved: ' https://stream.example/lofi ',
        homepage: ' https://radio.example ',
        lastcheckok: 1,
      },
      {
        name: 'Duplicate',
        stationuuid: 'station-1',
        url_resolved: 'https://stream.example/duplicate',
        lastcheckok: 1,
      },
      {
        name: 'Broken',
        stationuuid: 'station-2',
        url_resolved: 'https://stream.example/broken',
        lastcheckok: 0,
      },
      {
        name: 'Fallback URL',
        stationuuid: 'station-3',
        url: 'https://stream.example/fallback',
        lastcheckok: 1,
      },
    ]);

    expect(tracks).toEqual([
      {
        name: 'Lofi Study',
        stationUuid: 'station-1',
        url: 'https://stream.example/lofi',
        homepage: 'https://radio.example',
      },
      {
        name: 'Fallback URL',
        stationUuid: 'station-3',
        url: 'https://stream.example/fallback',
      },
    ]);
  });

  it('loads and normalizes lofi stations from Radio Browser', async () => {
    const fetcher = vi.fn(async () => ({
      ok: true,
      json: async () => [
        {
          name: 'Lofi Station',
          stationuuid: 'station-1',
          url_resolved: 'https://stream.example/lofi',
          lastcheckok: 1,
        },
      ],
    })) as unknown as typeof fetch;

    await expect(fetchLofiBackgroundMusicTracks(fetcher)).resolves.toEqual([
      {
        name: 'Lofi Station',
        stationUuid: 'station-1',
        url: 'https://stream.example/lofi',
      },
    ]);
    expect(fetcher).toHaveBeenCalledWith(expect.stringContaining('/json/stations/search?'), {
      headers: {
        Accept: 'application/json',
      },
    });
  });

  it('reports lookup failures without leaking response details', async () => {
    const fetcher = vi.fn(async () => ({
      ok: false,
      json: async () => [],
    })) as unknown as typeof fetch;

    await expect(fetchLofiBackgroundMusicTracks(fetcher)).rejects.toThrow('lofi_station_lookup_failed');
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
