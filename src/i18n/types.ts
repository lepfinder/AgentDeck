export type Locale = 'zh' | 'en';

export const LOCALE_STORAGE_KEY = 'agentdeck_locale';

export function detectLocale(): Locale {
  try {
    const saved = localStorage.getItem(LOCALE_STORAGE_KEY);
    if (saved === 'zh' || saved === 'en') return saved;
  } catch {
    /* ignore */
  }
  try {
    const lang = (navigator.language || '').toLowerCase();
    if (lang.startsWith('zh')) return 'zh';
  } catch {
    /* ignore */
  }
  return 'en';
}

export function localeTag(locale: Locale): string {
  return locale === 'zh' ? 'zh-CN' : 'en';
}
