import { useEffect } from 'react';

const RADIO_BROWSER_BASE_URL = 'https://de1.api.radio-browser.info';
const LOFI_STATION_LIMIT = 24;
const PLAYER_VOLUME = 0.35;
const MAX_FAILED_STATIONS_PER_CYCLE = 6;

export interface BackgroundMusicTrack {
  name: string;
  stationUuid: string;
  url: string;
  homepage?: string;
}

interface RadioBrowserStation {
  name?: unknown;
  stationuuid?: unknown;
  url_resolved?: unknown;
  url?: unknown;
  homepage?: unknown;
  codec?: unknown;
  lastcheckok?: unknown;
}

interface RadioBrowserClickResult {
  ok?: unknown;
  url?: unknown;
}

export function buildLofiStationSearchUrl(baseUrl = RADIO_BROWSER_BASE_URL) {
  const params = new URLSearchParams({
    tag: 'lofi',
    hidebroken: 'true',
    order: 'random',
    limit: String(LOFI_STATION_LIMIT),
  });

  return `${baseUrl}/json/stations/search?${params.toString()}`;
}

export function normalizeRadioBrowserStations(stations: RadioBrowserStation[]): BackgroundMusicTrack[] {
  const seen = new Set<string>();

  return stations.flatMap((station) => {
    const name = typeof station.name === 'string' ? station.name.trim() : '';
    const stationUuid = typeof station.stationuuid === 'string' ? station.stationuuid.trim() : '';
    const url = getStationStreamUrl(station);
    const homepage = typeof station.homepage === 'string' ? station.homepage.trim() : '';

    if (!name || !stationUuid || !url || seen.has(stationUuid) || station.lastcheckok === 0) {
      return [];
    }

    seen.add(stationUuid);
    return [
      {
        name,
        stationUuid,
        url,
        ...(homepage ? { homepage } : {}),
      },
    ];
  });
}

export async function fetchLofiBackgroundMusicTracks(fetcher: typeof fetch = fetch): Promise<BackgroundMusicTrack[]> {
  const response = await fetcher(buildLofiStationSearchUrl(), {
    headers: {
      Accept: 'application/json',
    },
  });

  if (!response.ok) {
    throw new Error('lofi_station_lookup_failed');
  }

  const body = await response.json();
  return Array.isArray(body) ? normalizeRadioBrowserStations(body) : [];
}

export function createShuffledTrackOrder(
  trackCount: number,
  random: () => number = Math.random,
  previousTrackIndex: number | null = null,
) {
  const order = Array.from({ length: Math.max(0, trackCount) }, (_, index) => index);

  for (let index = order.length - 1; index > 0; index -= 1) {
    const swapIndex = Math.floor(random() * (index + 1));
    [order[index], order[swapIndex]] = [order[swapIndex], order[index]];
  }

  if (order.length > 1 && order[0] === previousTrackIndex) {
    [order[0], order[1]] = [order[1], order[0]];
  }

  return order;
}

export function useSequentialBackgroundMusic(enabled: boolean) {
  useEffect(() => {
    if (!enabled) {
      return;
    }

    const player = new SequentialBackgroundMusicPlayer();

    player.attachInteractionStartListeners();
    return () => player.dispose();
  }, [enabled]);
}

class SequentialBackgroundMusicPlayer {
  private audio: HTMLAudioElement | null = null;
  private tracks: BackgroundMusicTrack[] = [];
  private playOrder: number[] = [];
  private playOrderPosition = 0;
  private previousTrackIndex: number | null = null;
  private failedStationsInCycle = 0;
  private hasStarted = false;
  private isDisposed = false;
  private isStarting = false;

  attachInteractionStartListeners() {
    if (typeof window === 'undefined') {
      return;
    }

    window.addEventListener('pointerdown', this.handleStartInteraction);
    window.addEventListener('keydown', this.handleStartInteraction);
    window.addEventListener('touchstart', this.handleStartInteraction);
  }

  dispose() {
    this.isDisposed = true;
    this.removeInteractionStartListeners();
    this.audio?.pause();
    this.audio = null;
  }

  private readonly handleStartInteraction = () => {
    if (this.hasStarted || this.isStarting || this.isDisposed) {
      return;
    }

    void this.startPlayback();
  };

  private async startPlayback() {
    this.isStarting = true;

    try {
      this.tracks = await fetchLofiBackgroundMusicTracks();
      await this.playCurrentTrack();
    } catch {
      // Background music is optional; lookup and playback failures must not interrupt the game.
    } finally {
      this.isStarting = false;
    }
  }

  private async playCurrentTrack() {
    if (this.isDisposed || this.tracks.length === 0 || typeof Audio !== 'function') {
      return;
    }

    if (this.playOrder.length === 0 || this.playOrderPosition >= this.playOrder.length) {
      this.playOrder = createShuffledTrackOrder(this.tracks.length, Math.random, this.previousTrackIndex);
      this.playOrderPosition = 0;
      this.failedStationsInCycle = 0;
    }

    const trackIndex = this.playOrder[this.playOrderPosition] ?? 0;
    const track = this.tracks[trackIndex];
    if (!track) {
      return;
    }

    const streamUrl = await resolvePlayableStreamUrl(track);
    if (!streamUrl) {
      if (this.skipFailedTrack()) {
        await this.playCurrentTrack();
      }
      return;
    }

    this.audio?.pause();
    this.audio = new Audio(streamUrl);
    this.audio.volume = PLAYER_VOLUME;
    this.audio.addEventListener('ended', this.handleTrackEnded);
    this.audio.addEventListener('error', this.handleTrackError);

    try {
      const playResult = this.audio.play();
      if (playResult && typeof playResult.then === 'function') {
        await playResult;
      }

      this.hasStarted = true;
      this.removeInteractionStartListeners();
    } catch {
      this.handleTrackError();
    }
  }

  private readonly handleTrackEnded = () => {
    if (this.isDisposed || this.tracks.length === 0) {
      return;
    }

    this.previousTrackIndex = this.playOrder[this.playOrderPosition] ?? null;
    this.playOrderPosition += 1;
    void this.playCurrentTrack();
  };

  private readonly handleTrackError = () => {
    if (this.isDisposed) {
      return;
    }

    if (this.skipFailedTrack()) {
      void this.playCurrentTrack();
    }
  };

  private skipFailedTrack() {
    this.previousTrackIndex = this.playOrder[this.playOrderPosition] ?? null;
    this.playOrderPosition += 1;
    this.failedStationsInCycle += 1;

    if (this.failedStationsInCycle >= Math.min(MAX_FAILED_STATIONS_PER_CYCLE, this.tracks.length)) {
      this.playOrderPosition = this.playOrder.length;
      return false;
    }

    return this.playOrderPosition < this.playOrder.length;
  }

  private removeInteractionStartListeners() {
    if (typeof window === 'undefined') {
      return;
    }

    window.removeEventListener('pointerdown', this.handleStartInteraction);
    window.removeEventListener('keydown', this.handleStartInteraction);
    window.removeEventListener('touchstart', this.handleStartInteraction);
  }
}

async function resolvePlayableStreamUrl(track: BackgroundMusicTrack, fetcher: typeof fetch = fetch) {
  try {
    const response = await fetcher(`${RADIO_BROWSER_BASE_URL}/json/url/${encodeURIComponent(track.stationUuid)}`, {
      headers: {
        Accept: 'application/json',
      },
    });

    if (!response.ok) {
      return track.url;
    }

    const body = (await response.json()) as RadioBrowserClickResult;
    return body.ok === true && typeof body.url === 'string' && body.url.trim() ? body.url.trim() : track.url;
  } catch {
    return track.url;
  }
}

function getStationStreamUrl(station: RadioBrowserStation) {
  const resolvedUrl = typeof station.url_resolved === 'string' ? station.url_resolved.trim() : '';
  const rawUrl = typeof station.url === 'string' ? station.url.trim() : '';

  return resolvedUrl || rawUrl;
}
