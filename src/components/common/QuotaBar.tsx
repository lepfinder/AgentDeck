import { useCallback, useEffect, useRef, useState } from 'react';
import { Gauge, RefreshCw, X } from 'lucide-react';
import { api, isTauri } from '../../api/tauriBridge';
import type { ProviderQuota, QuotaSnapshot, QuotaWindow } from '../../types';
import { useI18n, type MessageKey } from '../../i18n';
import { IdeIcon } from '../browse/ideIcons';

/** Pulse floor — recently changed / user just looked. */
const FLOOR_MS = 2 * 60_000;
/** Pulse unwatchedCeiling — idle while still in foreground. */
const ACTIVE_MS = 5 * 60_000;
/** Frontend + backend skip non-forced network within this window. */
const MIN_FETCH_GAP_MS = 2 * 60_000;

function fingerprint(snapshot: QuotaSnapshot | null): string {
  if (!snapshot) return '';
  return snapshot.providers
    .map(
      (p) =>
        `${p.id}:${p.available}:${p.headline.join(',')}:${p.windows
          .map((w) => `${w.id}=${w.used_percent}`)
          .join(';')}`
    )
    .join('|');
}

function nextIntervalMs(lastChangeAt: number | null, lastLookedAt: number | null): number {
  const now = Date.now();
  const ages = [lastChangeAt, lastLookedAt]
    .filter((t): t is number => t != null)
    .map((t) => now - t)
    .filter((a) => a >= 0);
  if (ages.length === 0) return ACTIVE_MS;
  return Math.min(...ages) < 5 * 60_000 ? FLOOR_MS : ACTIVE_MS;
}

function tintClass(percent: number, exhausted: boolean): string {
  if (exhausted || percent >= 95) return 'text-red-500 border-red-500/30 bg-red-500/10';
  if (percent >= 80) return 'text-amber-500 border-amber-500/30 bg-amber-500/10';
  if (percent >= 50) return 'text-yellow-600 dark:text-yellow-400 border-yellow-500/30 bg-yellow-500/10';
  return 'text-emerald-600 dark:text-emerald-400 border-emerald-500/30 bg-emerald-500/10';
}

function barColor(percent: number, exhausted: boolean): string {
  if (exhausted || percent >= 95) return 'bg-red-500';
  if (percent >= 80) return 'bg-amber-500';
  if (percent >= 50) return 'bg-yellow-500';
  return 'bg-emerald-500';
}

function kindLabel(kind: string, t: (k: MessageKey) => string): string {
  switch (kind) {
    case 'fiveHour':
      return t('quota.kind5h');
    case 'weekly':
      return t('quota.kindWeekly');
    case 'daily':
      return t('quota.kindDaily');
    case 'monthly':
      return t('quota.kindMonthly');
    case 'spend':
      return t('quota.kindSpend');
    default:
      return kind;
  }
}

function sourceLabel(source: string | null | undefined, t: (k: MessageKey) => string): string {
  switch (source) {
    case 'usage_summary':
      return t('quota.srcCursorApi');
    case 'language_server':
      return t('quota.srcLocalLs');
    case 'cloud_code':
      return t('quota.srcCloud');
    default:
      return source || '—';
  }
}

function formatReset(iso: string | null | undefined, locale: string): string {
  if (!iso) return '—';
  try {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return iso;
    return d.toLocaleString(locale === 'zh' ? 'zh-CN' : 'en-US', {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return iso;
  }
}

function relativeUntil(
  iso: string | null | undefined,
  t: (k: MessageKey, vars?: Record<string, string | number>) => string
): string {
  if (!iso) return '';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  const ms = d.getTime() - Date.now();
  if (ms <= 0) return t('quota.resetDone');
  const mins = Math.floor(ms / 60_000);
  const hours = Math.floor(mins / 60);
  const days = Math.floor(hours / 24);
  if (days >= 1) return t('quota.inDays', { d: days, h: hours % 24 });
  if (hours >= 1) return t('quota.inHours', { h: hours, m: mins % 60 });
  return t('quota.inMinutes', { m: Math.max(1, mins) });
}

function headlineText(p: ProviderQuota, locale: string): string {
  if (!p.available || p.headline.length === 0) return '—';
  return p.headline
    .map((h) => (locale === 'zh' ? h.replace(/^wk\b/, '周') : h))
    .join(' · ');
}

function maxUsed(p: ProviderQuota): number {
  if (!p.windows.length) return 0;
  return Math.max(...p.windows.map((w) => w.used_percent));
}

function anyExhausted(p: ProviderQuota): boolean {
  return p.windows.some((w) => w.exhausted);
}

function WindowRow({
  w,
  t,
  locale,
  hideScope,
}: {
  w: QuotaWindow;
  t: (k: MessageKey, vars?: Record<string, string | number>) => string;
  locale: string;
  hideScope?: boolean;
}) {
  const label = hideScope
    ? kindLabel(w.kind, t)
    : [kindLabel(w.kind, t), w.scope].filter(Boolean).join(' · ');
  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between gap-2 text-[11px]">
        <span className="theme-text-main font-medium truncate">{label}</span>
        <span
          className={`font-mono tabular-nums shrink-0 ${
            w.exhausted ? 'text-red-500 font-semibold' : 'theme-text-sub'
          }`}
        >
          {w.exhausted ? t('quota.exhausted') : `${w.used_percent}%`}
        </span>
      </div>
      <div className="h-1.5 rounded-full theme-bg-input overflow-hidden">
        <div
          className={`h-full rounded-full transition-all ${barColor(w.used_percent, w.exhausted)}`}
          style={{ width: `${Math.min(100, w.used_percent)}%` }}
        />
      </div>
      <div className="text-[10px] theme-text-muted">
        {t('quota.resets')}: {formatReset(w.resets_at, locale)}
        {w.resets_at && (
          <span className="theme-text-sub"> · {relativeUntil(w.resets_at, t)}</span>
        )}
      </div>
    </div>
  );
}

function WindowList({
  windows,
  t,
  locale,
  groupByScope,
}: {
  windows: QuotaWindow[];
  t: (k: MessageKey, vars?: Record<string, string | number>) => string;
  locale: string;
  groupByScope: boolean;
}) {
  if (!groupByScope) {
    return (
      <div className="space-y-3">
        {windows.map((w) => (
          <WindowRow key={w.id} w={w} t={t} locale={locale} />
        ))}
      </div>
    );
  }

  const blocks: { scope: string; items: QuotaWindow[] }[] = [];
  for (const w of windows) {
    const scope = w.scope?.trim() || '';
    const last = blocks[blocks.length - 1];
    if (last && last.scope === scope) {
      last.items.push(w);
    } else {
      blocks.push({ scope, items: [w] });
    }
  }

  return (
    <div className="space-y-3.5">
      {blocks.map((block) => (
        <div key={block.scope || 'default'} className="space-y-2.5">
          {block.scope && (
            <div className="text-[10px] font-semibold theme-text-sub tracking-wide">
              {t('quota.modelGroup', { name: block.scope })}
            </div>
          )}
          <div className="space-y-3">
            {block.items.map((w) => (
              <WindowRow key={w.id} w={w} t={t} locale={locale} hideScope />
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

export function QuotaBar() {
  const { t, locale } = useI18n();
  const [snapshot, setSnapshot] = useState<QuotaSnapshot | null>(null);
  const [loading, setLoading] = useState(false);
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** Bumps once a minute while the popover is open so countdowns stay live. */
  const [, setTick] = useState(0);
  const panelRef = useRef<HTMLDivElement>(null);
  const btnRef = useRef<HTMLButtonElement>(null);
  const loadingRef = useRef(false);
  /** Last time we asked the backend (forced or not). */
  const lastFetchAtRef = useRef(0);
  const lastChangeAtRef = useRef<number | null>(null);
  const lastLookedAtRef = useRef<number | null>(null);
  const fingerprintRef = useRef('');
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearTimer = () => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  };

  const load = useCallback(async (force = false) => {
    if (!isTauri() || loadingRef.current) return;
    if (!force) {
      const since = Date.now() - lastFetchAtRef.current;
      if (lastFetchAtRef.current > 0 && since < MIN_FETCH_GAP_MS) {
        return;
      }
    }
    loadingRef.current = true;
    setLoading(true);
    setError(null);
    try {
      const data = await api.getQuotaSnapshot(force);
      lastFetchAtRef.current = Date.now();
      const nextFp = fingerprint(data);
      if (nextFp && nextFp !== fingerprintRef.current) {
        fingerprintRef.current = nextFp;
        lastChangeAtRef.current = Date.now();
      } else if (!fingerprintRef.current && nextFp) {
        fingerprintRef.current = nextFp;
      }
      setSnapshot(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      loadingRef.current = false;
      setLoading(false);
    }
  }, []);

  const scheduleAuto = useCallback(() => {
    clearTimer();
    if (typeof document !== 'undefined' && document.visibilityState === 'hidden') {
      return;
    }
    const wait = nextIntervalMs(lastChangeAtRef.current, lastLookedAtRef.current);
    timerRef.current = setTimeout(() => {
      if (typeof document !== 'undefined' && document.visibilityState === 'hidden') {
        return;
      }
      void load(false).finally(() => scheduleAuto());
    }, wait);
  }, [load]);

  useEffect(() => {
    void load(false).finally(() => scheduleAuto());

    const onVisibility = () => {
      if (document.visibilityState === 'visible') {
        void load(false).finally(() => scheduleAuto());
      } else {
        clearTimer();
      }
    };
    document.addEventListener('visibilitychange', onVisibility);
    return () => {
      document.removeEventListener('visibilitychange', onVisibility);
      clearTimer();
    };
  }, [load, scheduleAuto]);

  useEffect(() => {
    if (!open) return;
    lastLookedAtRef.current = Date.now();
    scheduleAuto();

    const tickId = setInterval(() => setTick((v) => v + 1), 60_000);

    const onDown = (e: MouseEvent) => {
      const target = e.target as Node;
      if (panelRef.current?.contains(target) || btnRef.current?.contains(target)) return;
      setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    document.addEventListener('mousedown', onDown);
    document.addEventListener('keydown', onKey);
    return () => {
      clearInterval(tickId);
      document.removeEventListener('mousedown', onDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [open, scheduleAuto]);

  if (!isTauri()) return null;

  const providers = snapshot?.providers ?? [];
  const cursor = providers.find((p) => p.id === 'cursor');
  const ag = providers.find((p) => p.id === 'antigravity');

  return (
    <div className="relative">
      <button
        ref={btnRef}
        type="button"
        onClick={() => {
          setOpen((v) => !v);
        }}
        title={t('quota.title')}
        className="flex items-center gap-1.5 px-2 py-1 text-[11px] theme-bg-sub hover:opacity-90 border theme-border rounded-lg theme-text-muted hover:theme-text-main transition-colors cursor-pointer shadow-sm max-w-[280px]"
      >
        <Gauge className="h-3.5 w-3.5 text-blue-500 shrink-0" />
        {cursor && (
          <span
            className={`inline-flex items-center gap-1 px-1.5 py-0.5 rounded border font-mono tabular-nums truncate ${
              cursor.available
                ? tintClass(maxUsed(cursor), anyExhausted(cursor))
                : 'theme-text-muted border-transparent bg-transparent'
            }`}
          >
            <IdeIcon id="cursor" className="h-3 w-3 shrink-0" />
            <span>{headlineText(cursor, locale)}</span>
          </span>
        )}
        {ag && (
          <span
            className={`inline-flex items-center gap-1 px-1.5 py-0.5 rounded border font-mono tabular-nums truncate ${
              ag.available
                ? tintClass(maxUsed(ag), anyExhausted(ag))
                : 'theme-text-muted border-transparent bg-transparent'
            }`}
          >
            <IdeIcon id="antigravity" className="h-3 w-3 shrink-0" />
            <span>{headlineText(ag, locale)}</span>
          </span>
        )}
        {!cursor && !ag && (
          <span className="theme-text-muted">{loading ? t('quota.loading') : t('quota.idle')}</span>
        )}
      </button>

      {open && (
        <div
          ref={panelRef}
          className="absolute right-0 top-full mt-2 w-[340px] z-50 theme-bg-card border theme-border rounded-xl shadow-2xl overflow-hidden"
          onMouseDown={(e) => e.stopPropagation()}
        >
          <div className="flex items-center justify-between px-3 py-2.5 border-b theme-border">
            <div>
              <div className="text-xs font-semibold theme-text-main">{t('quota.title')}</div>
              <div className="text-[10px] theme-text-muted mt-0.5">{t('quota.subtitle')}</div>
            </div>
            <div className="flex items-center gap-1">
              <button
                type="button"
                onClick={() => {
                  void load(true);
                }}
                disabled={loading}
                className="p-1.5 rounded-lg theme-text-muted hover:theme-text-main hover:theme-bg-sub transition-colors cursor-pointer disabled:opacity-50"
                title={t('quota.refresh')}
              >
                <RefreshCw className={`h-3.5 w-3.5 ${loading ? 'animate-spin' : ''}`} />
              </button>
              <button
                type="button"
                onClick={() => setOpen(false)}
                className="p-1.5 rounded-lg theme-text-muted hover:theme-text-main hover:theme-bg-sub transition-colors cursor-pointer"
              >
                <X className="h-3.5 w-3.5" />
              </button>
            </div>
          </div>

          <div className="p-3 space-y-3 max-h-[70vh] overflow-y-auto">
            {error && (
              <div className="text-[11px] text-red-500 bg-red-500/10 border border-red-500/20 rounded-lg px-2.5 py-2">
                {error}
              </div>
            )}

            {providers.map((p) => (
              <div
                key={p.id}
                className="theme-bg-sub border theme-border rounded-xl p-3 space-y-2.5"
              >
                <div className="flex items-start justify-between gap-2">
                  <div>
                    <div className="text-xs font-semibold theme-text-main flex items-center gap-1.5">
                      <IdeIcon id={p.id} className="h-3.5 w-3.5 shrink-0" />
                      <span>{p.name}</span>
                      {p.plan && (
                        <span className="text-[10px] px-1.5 py-0.5 rounded bg-blue-500/10 text-blue-500 border border-blue-500/20 font-medium">
                          {p.plan}
                        </span>
                      )}
                    </div>
                    <div className="text-[10px] theme-text-muted mt-0.5">
                      {p.available
                        ? `${t('quota.source')}: ${sourceLabel(p.source, t)}`
                        : p.message || t('quota.unavailable')}
                    </div>
                  </div>
                  {p.credit_balance && (
                    <div className="text-[10px] font-mono theme-text-sub shrink-0">
                      {p.credit_balance}
                    </div>
                  )}
                </div>

                {p.available && p.windows.length > 0 ? (
                  <WindowList
                    windows={p.windows}
                    t={t}
                    locale={locale}
                    groupByScope={p.id === 'antigravity'}
                  />
                ) : (
                  !p.available && (
                    <div className="text-[11px] theme-text-muted leading-relaxed">
                      {p.message || t('quota.unavailable')}
                    </div>
                  )
                )}

                {p.available && p.age_seconds != null && p.age_seconds > 120 && (
                  <div className="text-[10px] text-amber-500">
                    {t('quota.stale', { n: String(Math.round(p.age_seconds / 60)) })}
                  </div>
                )}
              </div>
            ))}

            {!providers.length && !loading && (
              <div className="text-[11px] theme-text-muted text-center py-4">{t('quota.empty')}</div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
