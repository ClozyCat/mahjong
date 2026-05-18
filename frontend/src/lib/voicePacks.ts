import type { ActionEffectView } from '../types/match';

const VOICE_ASSETS = import.meta.glob('../../voices/*/*.mp3', {
  eager: true,
  query: '?url',
  import: 'default',
}) as Record<string, string>;

const RANK_VOICE_NAMES = ['yi', 'er', 'san', 'si', 'wu', 'liu', 'qi', 'ba', 'jiu'] as const;

const SUIT_VOICE_NAMES = {
  w: 'wan',
  m: 'wan',
  b: 'tong',
  p: 'tong',
  c: 'tiao',
  t: 'tiao',
} as const;

const HONOR_VOICE_NAMES: Record<string, string> = {
  east: 'dong',
  south: 'nan',
  west: 'xi',
  north: 'bei',
  red: 'zhong',
  green: 'fa',
  white: 'bai',
  d1: 'dong',
  d2: 'nan',
  d3: 'xi',
  d4: 'bei',
  d5: 'zhong',
  d6: 'fa',
  d7: 'bai',
};

const ACTION_VOICE_NAMES: Partial<Record<NonNullable<ActionEffectView['calloutTone']>, string>> = {
  chow: 'chi',
  pung: 'peng',
  kong: 'gang',
  hu: 'hu',
};

export type VoiceAssets = Record<string, string>;

export interface VoiceCue {
  key: string;
  dedupKey?: string;
  absoluteSeat: number;
  clipName: string;
}

interface QueuedVoiceClip {
  url: string;
  resolve: () => void;
}

const activeAudioBySeat = new Map<number, HTMLAudioElement>();

export function getVoiceClipNameForTile(tileCode: string | null | undefined): string | null {
  if (!tileCode) {
    return null;
  }

  const normalized = tileCode.trim().toLowerCase();
  const suited = normalized.match(/^([wbcmpt])([1-9])$/);
  if (!suited) {
    return HONOR_VOICE_NAMES[normalized] ?? null;
  }

  const [, suit, rankText] = suited;
  const rankIndex = Number(rankText) - 1;
  const rankName = RANK_VOICE_NAMES[rankIndex];
  const suitName = SUIT_VOICE_NAMES[suit as keyof typeof SUIT_VOICE_NAMES];

  return rankName && suitName ? `${rankName}_${suitName}` : null;
}

export function getVoiceClipNameForAction(calloutTone: ActionEffectView['calloutTone']): string | null {
  if (!calloutTone) {
    return null;
  }

  return ACTION_VOICE_NAMES[calloutTone] ?? null;
}

export function getVoicePackNames(assets: VoiceAssets = VOICE_ASSETS): string[] {
  return Array.from(
    new Set(
      Object.keys(assets)
        .map((path) => path.match(/\/voices\/([^/]+)\//)?.[1])
        .filter((name): name is string => Boolean(name)),
    ),
  ).sort();
}

export function selectVoicePackName(
  tableCode: string,
  absoluteSeat: number,
  assets: VoiceAssets = VOICE_ASSETS,
): string | null {
  const packNames = getVoicePackNames(assets);
  if (packNames.length === 0) {
    return null;
  }

  const hash = hashString(`${tableCode}:${absoluteSeat}`);
  return packNames[hash % packNames.length] ?? null;
}

export function resolveVoiceClipUrl(
  tableCode: string,
  absoluteSeat: number,
  clipName: string,
  assets: VoiceAssets = VOICE_ASSETS,
): string | null {
  const packName = selectVoicePackName(tableCode, absoluteSeat, assets);
  if (!packName) {
    return null;
  }

  return assets[`../../voices/${packName}/${clipName}.mp3`] ?? null;
}

export function playVoiceClip(url: string, absoluteSeat?: number): Promise<void> {
  return playVoiceClipNow(url, absoluteSeat);
}

function playVoiceClipNow(url: string, absoluteSeat?: number, onSettled?: () => void): Promise<void> {
  if (typeof Audio !== 'function') {
    onSettled?.();
    return Promise.resolve();
  }

  return new Promise((resolve) => {
    // Stop and remove any existing audio for this seat
    if (typeof absoluteSeat === 'number') {
      const existingAudio = activeAudioBySeat.get(absoluteSeat);
      if (existingAudio) {
        try {
          existingAudio.pause();
          existingAudio.currentTime = 0;
        } catch {
          // Ignore errors when stopping audio
        }
        activeAudioBySeat.delete(absoluteSeat);
      }
    }

    let audio: HTMLAudioElement;

    try {
      audio = new Audio(url);
    } catch {
      // Browser autoplay policies or unsupported media APIs should not interrupt the game.
      onSettled?.();
      resolve();
      return;
    }

    // Track this audio by seat
    if (typeof absoluteSeat === 'number') {
      activeAudioBySeat.set(absoluteSeat, audio);
    }

    let settled = false;
    const finish = () => {
      if (settled) {
        return;
      }

      settled = true;
      removeAudioEventListener?.('ended', finish);
      removeAudioEventListener?.('error', finish);

      // Clean up tracking
      if (typeof absoluteSeat === 'number' && activeAudioBySeat.get(absoluteSeat) === audio) {
        activeAudioBySeat.delete(absoluteSeat);
      }

      onSettled?.();
      resolve();
    };
    const addAudioEventListener =
      typeof audio.addEventListener === 'function' ? audio.addEventListener.bind(audio) : null;
    const removeAudioEventListener =
      typeof audio.removeEventListener === 'function' ? audio.removeEventListener.bind(audio) : null;

    if (addAudioEventListener) {
      addAudioEventListener('ended', finish, { once: true });
      addAudioEventListener('error', finish, { once: true });
    }

    try {
      const playResult = audio.play();
      if (playResult && typeof playResult.catch === 'function') {
        if (addAudioEventListener) {
          playResult.catch(finish);
        } else {
          playResult.then(finish, finish);
        }
      } else if (!addAudioEventListener) {
        finish();
      }
    } catch {
      finish();
    }
  });
}

function hashString(value: string) {
  let hash = 2166136261;

  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }

  return hash >>> 0;
}
