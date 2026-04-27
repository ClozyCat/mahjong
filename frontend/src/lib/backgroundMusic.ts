import { useEffect } from 'react';

const BGM_ASSETS = import.meta.glob('../../bgm/*.{mp3,ogg,wav,m4a}', {
  eager: true,
  query: '?url',
  import: 'default',
}) as Record<string, string>;

export type BackgroundMusicAssets = Record<string, string>;

export interface BackgroundMusicTrack {
  name: string;
  url: string;
}

export function getBackgroundMusicTracks(assets: BackgroundMusicAssets = BGM_ASSETS): BackgroundMusicTrack[] {
  return Object.entries(assets)
    .map(([path, url]) => ({
      name: getFileName(path),
      url,
    }))
    .sort((left, right) => left.name.localeCompare(right.name, undefined, { numeric: true }));
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

export function useSequentialBackgroundMusic() {
  useEffect(() => {
    const player = new SequentialBackgroundMusicPlayer(getBackgroundMusicTracks());

    player.attachInteractionStartListeners();
    return () => player.dispose();
  }, []);
}

class SequentialBackgroundMusicPlayer {
  private audio: HTMLAudioElement | null = null;
  private playOrder: number[] = [];
  private playOrderPosition = 0;
  private previousTrackIndex: number | null = null;
  private hasStarted = false;
  private isDisposed = false;

  constructor(private readonly tracks: BackgroundMusicTrack[]) {}

  attachInteractionStartListeners() {
    if (this.tracks.length === 0 || typeof window === 'undefined') {
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
    if (this.hasStarted || this.isDisposed) {
      return;
    }

    this.playCurrentTrack();
  };

  private playCurrentTrack() {
    if (this.playOrder.length === 0 || this.playOrderPosition >= this.playOrder.length) {
      this.playOrder = createShuffledTrackOrder(this.tracks.length, Math.random, this.previousTrackIndex);
      this.playOrderPosition = 0;
    }

    const trackIndex = this.playOrder[this.playOrderPosition] ?? 0;
    const track = this.tracks[trackIndex];
    if (!track || typeof Audio !== 'function') {
      return;
    }

    this.audio?.pause();
    this.audio = new Audio(track.url);
    this.audio.volume = 0.35;
    this.audio.addEventListener('ended', this.handleTrackEnded);
    this.audio.addEventListener('error', this.handleTrackEnded);

    try {
      const playResult = this.audio.play();
      if (playResult && typeof playResult.then === 'function') {
        playResult
          .then(() => {
            this.hasStarted = true;
            this.removeInteractionStartListeners();
          })
          .catch(() => {});
      } else {
        this.hasStarted = true;
        this.removeInteractionStartListeners();
      }
    } catch {
      // Background music must never interrupt the game when autoplay/media APIs are unavailable.
    }
  }

  private readonly handleTrackEnded = () => {
    if (this.isDisposed || this.tracks.length === 0) {
      return;
    }

    this.previousTrackIndex = this.playOrder[this.playOrderPosition] ?? null;
    this.playOrderPosition += 1;
    this.playCurrentTrack();
  };

  private removeInteractionStartListeners() {
    if (typeof window === 'undefined') {
      return;
    }

    window.removeEventListener('pointerdown', this.handleStartInteraction);
    window.removeEventListener('keydown', this.handleStartInteraction);
    window.removeEventListener('touchstart', this.handleStartInteraction);
  }
}

function getFileName(path: string) {
  return path.split('/').pop() ?? path;
}
