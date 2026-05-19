import type { ActionEffectView } from '../types/match';

const VOICE_ASSETS = import.meta.glob('../../voices/*/*.mp3', {
  eager: true,
  query: '?url',
  import: 'default',
}) as Record<string, string>;

const ACTION_VOICE_NAMES: Partial<Record<NonNullable<ActionEffectView['calloutTone']>, string>> = {
  chow: 'chi',
  pung: 'peng',
  kong: 'gang',
  hu: 'hu',
  ready_hand: 'ting',
};

export type VoiceAssets = Record<string, string>;

export interface VoiceCue {
  key: string;
  voiceKey: string | number;
  clipName: string;
}

const activeAudioByVoiceKey = new Map<string, HTMLAudioElement>();

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
  voiceKey: string | number,
  assets: VoiceAssets = VOICE_ASSETS,
): string | null {
  const packNames = getVoicePackNames(assets);
  if (packNames.length === 0) {
    return null;
  }

  const hash = hashString(`${tableCode}:${voiceKey}`);
  return packNames[hash % packNames.length] ?? null;
}

export function resolveVoiceClipUrl(
  tableCode: string,
  voiceKey: string | number,
  clipName: string,
  assets: VoiceAssets = VOICE_ASSETS,
): string | null {
  const packName = selectVoicePackName(tableCode, voiceKey, assets);
  if (!packName) {
    return null;
  }

  return assets[`../../voices/${packName}/${clipName}.mp3`] ?? null;
}

export function playVoiceClip(url: string, voiceKey?: string | number): Promise<void> {
  return playVoiceClipNow(url, voiceKey);
}

function playVoiceClipNow(url: string, voiceKey?: string | number, onSettled?: () => void): Promise<void> {
  if (typeof Audio !== 'function') {
    onSettled?.();
    return Promise.resolve();
  }

  return new Promise((resolve) => {
    const normalizedVoiceKey = typeof voiceKey === 'undefined' ? null : String(voiceKey);

    if (normalizedVoiceKey) {
      const existingAudio = activeAudioByVoiceKey.get(normalizedVoiceKey);
      if (existingAudio) {
        try {
          if (!existingAudio.paused) {
            existingAudio.pause();
          }
          existingAudio.currentTime = 0;
        } catch {
          // Ignore errors when stopping audio
        }
        activeAudioByVoiceKey.delete(normalizedVoiceKey);
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

    if (normalizedVoiceKey) {
      activeAudioByVoiceKey.set(normalizedVoiceKey, audio);
    }

    let settled = false;
    const finish = () => {
      if (settled) {
        return;
      }

      settled = true;
      removeAudioEventListener?.('ended', finish);
      removeAudioEventListener?.('error', finish);

      if (normalizedVoiceKey && activeAudioByVoiceKey.get(normalizedVoiceKey) === audio) {
        activeAudioByVoiceKey.delete(normalizedVoiceKey);
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
