/**
 * 额度悬浮条（独立置顶小窗 quota-pill 专用入口）。
 * 竖向深底圆角条：每个「provider × 窗口类型」一个进度环（Antigravity 的 5h 与周各占一环，
 * 同类型多窗口合并取最高）；hover 展开该 provider 的右侧明细卡。
 * 窗口尺寸由本组件通过 quota_pill_resize 命令动态调整（Rust 侧限制上下限）。
 */
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { RefreshCw } from 'lucide-react';
import { api, isTauri } from '../../api/tauriBridge';
import type { ProviderQuota, QuotaSnapshot, QuotaWindow } from '../../types';
import { useI18n, type MessageKey } from '../../i18n';
import { IdeIcon } from '../browse/ideIcons';

const BAR_W = 64;
const PAD = 10;
/** 顶部拖拽把手高度（30 本体 + 4 与条目间距） */
const HEADER_H = 34;
const ITEM_H = 60;
const ITEM_GAP = 6;
const POPOVER_W = 280;
const POPOVER_GAP = 10;
const CLOSE_DELAY_MS = 300;

const ACCENT_DEFAULT = '#a1a1aa';

/** 用量分档着色，阈值与顶栏 QuotaBar 一致 */
function tint(percent: number, exhausted: boolean): string {
  if (exhausted || percent >= 95) return '#ef4444';
  if (percent >= 80) return '#f59e0b';
  if (percent >= 50) return '#eab308';
  return '#22c55e';
}

const MOCK_SNAPSHOT: QuotaSnapshot = {
  generated_at: new Date().toISOString(),
  providers: [
    {
      id: 'cursor', name: 'Cursor', available: true, status: 'ok', message: null,
      plan: 'Pro', credit_balance: null, source: 'usage_summary',
      observed_at: null, age_seconds: null,
      windows: [
        { id: 'm1', kind: 'monthly', scope: 'Cursor Models', used_fraction: 0.02, used_percent: 2, exhausted: false, resets_at: '2026-09-30T13:33:00' },
        { id: 'm2', kind: 'monthly', scope: 'Other Models', used_fraction: 0, used_percent: 0, exhausted: false, resets_at: '2026-09-30T13:33:00' },
      ],
      headline: [],
    },
    {
      id: 'antigravity', name: 'Antigravity', available: true, status: 'ok', message: null,
      plan: null, credit_balance: null, source: 'language_server',
      observed_at: null, age_seconds: null,
      windows: [
        { id: 'w1', kind: 'fiveHour', scope: null, used_fraction: 0.13, used_percent: 13, exhausted: false, resets_at: '2026-09-20T15:00:00' },
        { id: 'w2', kind: 'weekly', scope: null, used_fraction: 0.9, used_percent: 90, exhausted: false, resets_at: '2026-09-27T00:00:00' },
      ],
      headline: [],
    },
  ],
};

type T = (key: MessageKey, vars?: Record<string, string | number>) => string;

function kindLabel(kind: string, t: T): string {
  switch (kind) {
    case 'fiveHour': return t('quota.kind5h');
    case 'weekly': return t('quota.kindWeekly');
    case 'daily': return t('quota.kindDaily');
    case 'monthly': return t('quota.kindMonthly');
    case 'spend': return t('quota.kindSpend');
    default: return kind;
  }
}

function sourceLabel(source: string | null | undefined, t: T): string {
  switch (source) {
    case 'usage_summary': return t('quota.srcCursorApi');
    case 'language_server': return t('quota.srcLocalLs');
    case 'cloud_code': return t('quota.srcCloud');
    default: return source || '—';
  }
}

function formatReset(iso: string | null | undefined, locale: string): string {
  if (!iso) return '—';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(locale === 'zh' ? 'zh-CN' : 'en-US', {
    month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
  });
}

const KIND_ORDER = ['fiveHour', 'daily', 'weekly', 'monthly', 'spend'];

function shortKind(kind: string, t: T): string {
  switch (kind) {
    case 'fiveHour': return t('quota.pill.short5h');
    case 'daily': return t('quota.pill.shortDaily');
    case 'weekly': return t('quota.pill.shortWeekly');
    case 'monthly': return t('quota.pill.shortMonthly');
    case 'spend': return t('quota.pill.shortSpend');
    default: return kind;
  }
}

/** 悬浮条一项 = provider 的一个窗口类型（同类型多窗口合并取最高） */
interface Entry {
  key: string;
  provider: ProviderQuota;
  kind: string;
  percent: number;
  exhausted: boolean;
  /** 该 provider 是否有多个类型分组（决定标签是否带类型前缀） */
  multi: boolean;
}

function buildEntries(providers: ProviderQuota[]): Entry[] {
  const out: Entry[] = [];
  for (const p of providers) {
    if (!p.available || !p.windows.length) {
      out.push({ key: p.id, provider: p, kind: '', percent: 0, exhausted: false, multi: false });
      continue;
    }
    const groups = new Map<string, { max: number; exhausted: boolean }>();
    for (const w of p.windows) {
      const g = groups.get(w.kind) ?? { max: 0, exhausted: false };
      g.max = Math.max(g.max, w.used_percent);
      g.exhausted = g.exhausted || w.exhausted;
      groups.set(w.kind, g);
    }
    const multi = groups.size > 1;
    const sorted = [...groups.entries()].sort(
      (a, b) => KIND_ORDER.indexOf(a[0]) - KIND_ORDER.indexOf(b[0]),
    );
    for (const [kind, g] of sorted) {
      out.push({ key: `${p.id}:${kind}`, provider: p, kind, percent: g.max, exhausted: g.exhausted, multi });
    }
  }
  return out;
}

function itemTop(index: number): number {
  return PAD + HEADER_H + index * (ITEM_H + ITEM_GAP);
}

function Ring({ entry }: { entry: Entry }) {
  const usable = entry.provider.available && !!entry.kind;
  const accent = usable ? tint(entry.percent, entry.exhausted) : ACCENT_DEFAULT;
  const frac = usable ? Math.min(1, entry.percent / 100) : 0;
  const r = 20;
  const c = 2 * Math.PI * r;
  return (
    <span className="relative inline-flex h-[44px] w-[44px] items-center justify-center">
      <svg width="44" height="44" viewBox="0 0 44 44" className="absolute inset-0">
        <circle cx="22" cy="22" r="16" fill="rgba(255,255,255,0.07)" />
        <circle cx="22" cy="22" r={r} fill="none" stroke="rgba(255,255,255,0.14)" strokeWidth="3" />
        {frac > 0 && (
          <circle
            cx="22" cy="22" r={r} fill="none"
            stroke={accent} strokeWidth="3" strokeLinecap="round"
            strokeDasharray={`${c * frac} ${c}`}
            transform="rotate(-90 22 22)"
          />
        )}
      </svg>
      <IdeIcon id={entry.provider.id} className="relative h-[17px] w-[17px] text-white/90" />
    </span>
  );
}

function WindowBlock({ w, t, locale }: {
  w: QuotaWindow; t: T; locale: string;
}) {
  const label = [kindLabel(w.kind, t), w.scope].filter(Boolean).join(' · ');
  const fill = tint(w.used_percent, w.exhausted);
  return (
    <div className="space-y-1.5">
      <div className="text-[11px]" style={{ color: '#a1a1aa' }}>{label}</div>
      <div className="h-[3px] rounded-full" style={{ background: 'rgba(255,255,255,0.12)' }}>
        <div
          className="h-full rounded-full"
          style={{
            width: `${Math.min(100, Math.max(w.exhausted ? 100 : 2, w.used_percent))}%`,
            background: fill,
          }}
        />
      </div>
      <div className="flex items-center justify-between text-[11px]">
        <span style={{ color: '#d4d4d8' }}>
          {w.exhausted ? t('quota.exhausted') : t('quota.pill.used', { p: w.used_percent })}
        </span>
        <span style={{ color: '#71717a' }}>
          {t('quota.pill.reset', { time: formatReset(w.resets_at, locale) })}
        </span>
      </div>
    </div>
  );
}

function Popover({ p, top, t, locale, refreshing, onRefresh, onEnter, innerRef }: {
  p: ProviderQuota; top: number; t: T; locale: string;
  refreshing: boolean; onRefresh: () => void; onEnter: () => void;
  innerRef: React.Ref<HTMLDivElement>;
}) {
  const arrowTop = 22;
  return (
    <div
      ref={innerRef}
      className="absolute rounded-[14px] px-4 py-3.5"
      onMouseEnter={onEnter}
      style={{
        left: BAR_W + POPOVER_GAP,
        top,
        width: POPOVER_W,
        background: 'rgba(22,22,25,0.96)',
        border: '1px solid rgba(255,255,255,0.09)',
        boxShadow: '0 12px 32px rgba(0,0,0,0.5)',
      }}
    >
      <span
        className="absolute h-[10px] w-[10px] rotate-45"
        style={{
          left: -5,
          top: arrowTop,
          background: 'rgba(22,22,25,0.96)',
          borderLeft: '1px solid rgba(255,255,255,0.09)',
          borderBottom: '1px solid rgba(255,255,255,0.09)',
        }}
      />
      <div className="flex items-center gap-2">
        <IdeIcon id={p.id} className="h-4 w-4 text-white/90" />
        <span className="text-[13px] font-semibold text-white">
          {t('quota.pill.usage', { name: p.name })}
        </span>
      </div>
      <div className="mt-3 space-y-3.5">
        {p.available && p.windows.length > 0 ? (
          p.windows.map((w) => (
            <WindowBlock key={w.id} w={w} t={t} locale={locale} />
          ))
        ) : (
          <div className="text-[11px]" style={{ color: '#71717a' }}>
            {p.available ? t('quota.empty') : p.message || t('quota.unavailable')}
          </div>
        )}
      </div>
      <div className="mt-3 flex items-center justify-between">
        <span className="text-[10px]" style={{ color: '#52525b' }}>
          {sourceLabel(p.source, t)}
          {p.available && p.age_seconds != null && p.age_seconds > 120 && (
            <span style={{ color: '#d97706' }}> · {t('quota.stale', { n: String(Math.round(p.age_seconds / 60)) })}</span>
          )}
        </span>
        <button
          type="button"
          onClick={onRefresh}
          disabled={refreshing}
          className="cursor-pointer rounded-md p-1 text-white/40 transition-colors hover:text-white/80 disabled:opacity-50"
          title={t('quota.refresh')}
        >
          <RefreshCw className={`h-3 w-3 ${refreshing ? 'animate-spin' : ''}`} />
        </button>
      </div>
    </div>
  );
}

export function QuotaPillApp() {
  const { t, locale } = useI18n();
  const [snapshot, setSnapshot] = useState<QuotaSnapshot | null>(null);
  // 浏览器预览可用 &hover=1 直接展开第一张明细卡
  const [hovered, setHovered] = useState<string | null>(() =>
    typeof window !== 'undefined' &&
    window.location.search.includes('view=quota-pill') &&
    window.location.search.includes('hover=1')
      ? 'cursor:monthly'
      : null,
  );
  const [refreshing, setRefreshing] = useState(false);
  const closeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const barRef = useRef<HTMLDivElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const refreshingRef = useRef(false);

  const useMock = !isTauri() &&
    typeof window !== 'undefined' &&
    window.location.search.includes('mock=1');

  const load = useCallback(async (force: boolean) => {
    if (useMock) {
      setSnapshot(MOCK_SNAPSHOT);
      return;
    }
    if (refreshingRef.current) return;
    refreshingRef.current = true;
    setRefreshing(true);
    try {
      setSnapshot(await api.getQuotaSnapshot(force));
    } catch (e) {
      console.error('[quota-pill] load failed', e);
    } finally {
      refreshingRef.current = false;
      setRefreshing(false);
    }
  }, [useMock]);

  useEffect(() => {
    document.documentElement.style.background = 'transparent';
    document.body.style.background = 'transparent';
    document.getElementById('root')!.style.background = 'transparent';

    void load(false);
    if (!isTauri()) return;
    const un = listen<QuotaSnapshot>('quota-pill-updated', (e) => setSnapshot(e.payload));
    // 从托盘恢复显示时收起明细卡，窗口尺寸随 useLayoutEffect 回到悬浮条
    const un2 = listen('quota-pill-shown', () => {
      if (closeTimer.current) {
        clearTimeout(closeTimer.current);
        closeTimer.current = null;
      }
      setHovered(null);
    });
    return () => {
      void un.then((f) => f());
      void un2.then((f) => f());
    };
  }, [load]);

  const cancelClose = useCallback(() => {
    if (closeTimer.current) {
      clearTimeout(closeTimer.current);
      closeTimer.current = null;
    }
  }, []);

  const openItem = useCallback((key: string) => {
    cancelClose();
    setHovered(key);
  }, [cancelClose]);

  const scheduleClose = useCallback(() => {
    cancelClose();
    closeTimer.current = setTimeout(() => setHovered(null), CLOSE_DELAY_MS);
  }, [cancelClose]);

  // logo 右键弹出原生菜单（隐藏悬浮条 / 退出）
  const openMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    void api.showQuotaPillMenu();
  }, []);

  const entries = useMemo(() => buildEntries(snapshot?.providers ?? []), [snapshot]);
  const hoveredIndex = hovered ? entries.findIndex((e) => e.key === hovered) : -1;
  const hoveredEntry = hoveredIndex >= 0 ? entries[hoveredIndex] : null;

  // 窗口尺寸跟随内容：折叠 = 悬浮条；展开 = 悬浮条 + 明细卡
  useLayoutEffect(() => {
    const barH = barRef.current?.offsetHeight ?? 120;
    let width = BAR_W;
    let height = barH;
    if (popoverRef.current) {
      width = BAR_W + POPOVER_GAP + POPOVER_W;
      height = Math.max(barH, itemTop(hoveredIndex) + popoverRef.current.offsetHeight + PAD);
    }
    void api.resizeQuotaPill(width, height);
  }, [hoveredIndex, entries.length, snapshot]);

  return (
    <div
      className="fixed inset-0 select-none overflow-hidden"
      style={{ fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif" }}
      onMouseLeave={scheduleClose}
    >
      <div
        ref={barRef}
        data-tauri-drag-region
        className="absolute left-0 top-0 flex flex-col items-center"
        style={{
          width: BAR_W,
          gap: ITEM_GAP,
          padding: `${PAD}px 0`,
          borderRadius: 20,
          background: 'rgba(16,16,18,0.88)',
          border: '1px solid rgba(255,255,255,0.08)',
          boxShadow: '0 8px 24px rgba(0,0,0,0.45)',
        }}
      >
        <div
          data-tauri-drag-region
          onContextMenu={openMenu}
          className="mb-[4px] flex h-[30px] w-full shrink-0 items-center justify-center rounded-t-[20px]"
          style={{ background: 'rgba(255,255,255,0.05)' }}
          title={t('quota.pill.handleTip')}
        >
          <img
            src="/app-icon.png"
            alt=""
            draggable={false}
            className="pointer-events-none h-[20px] w-[20px] opacity-90"
          />
        </div>
        {entries.map((entry) => (          <button
            key={entry.key}
            type="button"
            onMouseEnter={() => openItem(entry.key)}
            onClick={() => openItem(entry.key)}
            className="flex cursor-pointer flex-col items-center"
            style={{ height: ITEM_H }}
          >
            <Ring entry={entry} />
            <span className="mt-[2px] text-[10px] font-medium tabular-nums" style={{ color: '#a1a1aa' }}>
              {entry.provider.available && entry.kind
                ? (entry.multi ? `${shortKind(entry.kind, t)} ${entry.percent}%` : `${entry.percent}%`)
                : '—'}
            </span>
          </button>
        ))}
        {!entries.length && (
          <span className="py-[14px] text-[10px]" style={{ color: '#52525b' }}>
            {t('quota.loading')}
          </span>
        )}
      </div>

      {hoveredEntry && (
        <Popover
          innerRef={popoverRef}
          p={hoveredEntry.provider}
          top={itemTop(hoveredIndex) - 6}
          t={t}
          locale={locale}
          refreshing={refreshing}
          onRefresh={() => void load(true)}
          onEnter={cancelClose}
        />
      )}
    </div>
  );
}
