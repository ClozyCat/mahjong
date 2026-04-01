export const THEME_OPTIONS = [
  { id: 'tian-shui-bi', label: '天水碧' },
  { id: 'qiu-xiang', label: '秋香' },
  { id: 'song-hua', label: '松花' },
  { id: 'yue-bai', label: '月白' },
  { id: 'mei-zi-qing', label: '梅子青' },
  { id: 'qing-ci', label: '青瓷' },
  { id: 'ou-he', label: '藕荷' },
  { id: 'ya-qing', label: '鸦青' },
  { id: 'dai-qing', label: '黛青' },
  { id: 'xuan-qing', label: '玄青' },
  { id: 'zhu-sha', label: '朱砂' },
  { id: 'tan-xiang-zi', label: '檀香紫' },
] as const;

export type ThemeId = (typeof THEME_OPTIONS)[number]['id'];

export const DEFAULT_THEME_ID: ThemeId = 'tian-shui-bi';

const THEME_LABEL_BY_ID = Object.fromEntries(THEME_OPTIONS.map((theme) => [theme.id, theme.label])) as Record<
  ThemeId,
  string
>;

const THEME_ID_SET = new Set<ThemeId>(THEME_OPTIONS.map((theme) => theme.id));

export function isThemeId(value: string | null | undefined): value is ThemeId {
  if (!value) {
    return false;
  }

  return THEME_ID_SET.has(value as ThemeId);
}

export function getThemeLabel(themeId: ThemeId) {
  return THEME_LABEL_BY_ID[themeId];
}

export function getNextThemeId(themeId: ThemeId): ThemeId {
  const currentIndex = THEME_OPTIONS.findIndex((theme) => theme.id === themeId);
  const nextIndex = currentIndex >= 0 ? (currentIndex + 1) % THEME_OPTIONS.length : 0;

  return THEME_OPTIONS[nextIndex].id;
}
