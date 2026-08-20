import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';
import { detectLocale, localeTag, LOCALE_STORAGE_KEY, type Locale } from './types';
import { zh, type MessageKey } from './zh';
import { en } from './en';

type Vars = Record<string, string | number>;

type I18nContextValue = {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  t: (key: MessageKey, vars?: Vars) => string;
};

const I18nContext = createContext<I18nContextValue | null>(null);

const dicts: Record<Locale, Record<MessageKey, string>> = { zh, en };

let currentLocale: Locale = 'zh';

export function interpolate(template: string, vars?: Vars): string {
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (_, name: string) =>
    vars[name] === undefined ? `{${name}}` : String(vars[name])
  );
}

export function translate(key: MessageKey, vars?: Vars, locale: Locale = currentLocale): string {
  const text = dicts[locale][key] || zh[key] || key;
  return interpolate(text, vars);
}

export function getLocale(): Locale {
  return currentLocale;
}

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(() => detectLocale());

  useEffect(() => {
    currentLocale = locale;
    try {
      localStorage.setItem(LOCALE_STORAGE_KEY, locale);
    } catch {
      /* ignore */
    }
    document.documentElement.lang = localeTag(locale);
  }, [locale]);

  const value = useMemo<I18nContextValue>(
    () => ({
      locale,
      setLocale: setLocaleState,
      t: (key, vars) => translate(key, vars, locale),
    }),
    [locale]
  );

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nContextValue {
  const ctx = useContext(I18nContext);
  if (!ctx) {
    throw new Error('useI18n must be used within I18nProvider');
  }
  return ctx;
}

export type { MessageKey, Locale };

const WEEKDAY_KEYS = [
  'weekday.0',
  'weekday.1',
  'weekday.2',
  'weekday.3',
  'weekday.4',
  'weekday.5',
  'weekday.6',
] as const;

export function weekdayLabel(dayIndex: number): string {
  return translate(WEEKDAY_KEYS[dayIndex] ?? 'weekday.0');
}
