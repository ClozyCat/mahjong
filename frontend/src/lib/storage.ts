const THEME_KEY = 'mahjong:theme';
const BGM_KEY = 'mahjong:bgm';
const VOICE_KEY = 'mahjong:voice';

export function clearStoredSession() {
  localStorage.removeItem('mahjong:session');
}

export function loadStoredThemeId() {
  return localStorage.getItem(THEME_KEY);
}

export function saveStoredThemeId(themeId: string) {
  localStorage.setItem(THEME_KEY, themeId);
}

export function loadStoredBgmEnabled(): boolean {
  return localStorage.getItem(BGM_KEY) === 'true';
}

export function saveStoredBgmEnabled(enabled: boolean) {
  localStorage.setItem(BGM_KEY, enabled ? 'true' : 'false');
}

export function loadStoredVoiceEnabled(): boolean {
  return localStorage.getItem(VOICE_KEY) !== 'false';
}

export function saveStoredVoiceEnabled(enabled: boolean) {
  localStorage.setItem(VOICE_KEY, enabled ? 'true' : 'false');
}
