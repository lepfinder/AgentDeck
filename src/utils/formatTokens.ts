import type { Locale } from '../i18n';

// token 量级走中文单位（万/亿）或英文缩写（K/M/B），一位小数、去尾零
export function fmtTokens(n: number, locale: Locale): string {
  const one = (v: number) => {
    const s = v.toFixed(1);
    return s.endsWith('.0') ? s.slice(0, -2) : s;
  };
  if (locale === 'en') {
    if (n >= 1e9) return `${one(n / 1e9)}B`;
    if (n >= 1e6) return `${one(n / 1e6)}M`;
    if (n >= 1e3) return `${one(n / 1e3)}K`;
    return n.toLocaleString();
  }
  if (n >= 1e8) return `${one(n / 1e8)}亿`;
  const wan = one(n / 1e4);
  if (Number(wan) >= 1e4) return `${one(n / 1e8)}亿`;
  return n >= 1e4 ? `${wan}万` : n.toLocaleString();
}
