/**
 * 额度悬浮条（独立置顶小窗 quota-pill 专用入口）。
 * 竖向深底圆角条：每个「provider × 窗口类型」一个进度环（Antigravity 的 5h 与周各占一环，
 * 同类型多窗口合并取最高）；hover 展开该 provider 的右侧明细卡。
 * 窗口高度由本组件通过 quota_pill_resize 调整（宽度恒定）；收拢/展开为 CSS 过渡。
 */
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { RefreshCw } from 'lucide-react';
import { api, isTauri } from '../../api/tauriBridge';
import type { ProviderQuota, QuotaSnapshot, QuotaWindow } from '../../types';
import { useI18n, type MessageKey } from '../../i18n';
import { IdeIcon } from '../browse/ideIcons';

const BAR_W = 64;
/** 停靠收拢后的细条宽度，与 Rust SLIVER_W 保持一致 */
const SLIVER_W = 8;
/** 细条高度：收拢时不必占满整条 bar 的高度，与 Rust SLIVER_H 保持一致 */
const SLIVER_H = 96;
/**
 * 贴边外扩弧（Pulse 的 DockBerthShape）：端头用一段内凹弧扫到屏幕边缘，
 * 贴边侧整段平齐，看起来像一滩水从边缘长出来而不是停在旁边。
 */
const FLARE_H = 20;
const FLARE_W = 30;
/** 展开态开口侧圆角 */
const CORNER_R = 20;
/** 内容内缩必须 ≥ 外扩弧高度：弧会吃掉端头 flareHeight 一段，超出轮廓的内容被裁掉 */
const PAD = FLARE_H;
/** 初始悬浮条高度，挂载后按内容测量 */
const INITIAL_BAR_H = 240;
/** 顶部拖拽把手高度（30 本体 + 4 与条目间距） */
const HEADER_H = 34;
const ITEM_H = 60;
const ITEM_GAP = 6;
const POPOVER_W = 280;
const POPOVER_GAP = 10;
const CLOSE_DELAY_MS = 300;
/** 环 44 宽居中在 64 的 bar 里，两侧各 10px 是空隙：指针压在空隙上不算选中该条 */
const RING_HIT_INSET = 10;
/**
 * 收起态细条底色：深壁纸上黑条几乎看不见，先试醒目红。
 * 展开时与深底在同一轮廓内交叉淡出，不做两层叠加（薄窗上叠色会糊成暗红）。
 */
const SLIVER_FILL = 'rgba(239,68,68,0.95)';
const BAR_FILL = 'rgba(16,16,18,0.92)';

const ACCENT_DEFAULT = '#a1a1aa';

/** 用量分档着色阈值：95 红 / 80 琥珀 / 50 黄 / 其余绿 */
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

/** 距重置还有多久的口语化文案 */
function relativeUntil(iso: string | null | undefined, t: T): string {
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

function isGeminiWindow(w: QuotaWindow): boolean {
  const scope = (w.scope || '').toLowerCase();
  const id = (w.id || '').toLowerCase();
  if (
    scope.includes('claude') ||
    scope.includes('gpt') ||
    id.includes('claude') ||
    id.includes('gpt') ||
    id.startsWith('3p')
  ) {
    return false;
  }
  return scope.includes('gemini') || id.includes('gemini');
}

/**
 * 圆环用窗口：Antigravity 只统计 Gemini（5h/周），
 * 避免 Claude/GPT 同 kind 的更高用量顶掉 Gemini 环。
 * 明细 Popover 仍用完整 p.windows。
 */
function ringWindows(p: ProviderQuota): QuotaWindow[] {
  if (p.id !== 'antigravity') return p.windows;
  const gem = p.windows.filter(isGeminiWindow);
  return gem.length > 0 ? gem : p.windows;
}

function buildEntries(providers: ProviderQuota[]): Entry[] {
  const out: Entry[] = [];
  for (const p of providers) {
    const ringWin = ringWindows(p);
    if (!p.available || !ringWin.length) {
      out.push({ key: p.id, provider: p, kind: '', percent: 0, exhausted: false, multi: false });
      continue;
    }
    const groups = new Map<string, { max: number; exhausted: boolean }>();
    for (const w of ringWin) {
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
  const usedText = w.exhausted
    ? t('quota.exhausted')
    : t('quota.pill.used', { p: w.used_percent });
  return (
    <div className="space-y-1.5">
      {/* 标签 + 已用：已用在进度条右上角 */}
      <div className="flex items-baseline justify-between gap-2">
        <span className="text-[11px] truncate" style={{ color: '#a1a1aa' }}>{label}</span>
        <span className="text-[11px] shrink-0 font-medium" style={{ color: fill }}>
          {usedText}
        </span>
      </div>
      <div className="h-[3px] rounded-full" style={{ background: 'rgba(255,255,255,0.12)' }}>
        <div
          className="h-full rounded-full"
          style={{
            width: `${Math.min(100, Math.max(w.exhausted ? 100 : 2, w.used_percent))}%`,
            background: fill,
          }}
        />
      </div>
      {/* 重置时间左、剩余时长右对齐 */}
      <div className="flex items-start justify-between gap-2 text-[10px]">
        <span style={{ color: '#71717a' }}>
          {t('quota.resets')}: {formatReset(w.resets_at, locale)}
        </span>
        {w.resets_at && (
          <span className="shrink-0 text-right" style={{ color: '#a1a1aa' }}>
            {relativeUntil(w.resets_at, t)}
          </span>
        )}
      </div>
    </div>
  );
}

function Popover({ p, top, t, locale, refreshing, onRefresh, onEnter, innerRef, mirror }: {
  p: ProviderQuota; top: number; t: T; locale: string;
  refreshing: boolean; onRefresh: () => void; onEnter: () => void;
  innerRef: React.Ref<HTMLDivElement>; mirror: boolean;
}) {
  const arrowTop = 22;
  return (
    <div
      ref={innerRef}
      className="absolute rounded-[14px] px-4 py-3.5"
      onMouseEnter={onEnter}
      style={{
        left: mirror ? undefined : BAR_W + POPOVER_GAP,
        right: mirror ? BAR_W + POPOVER_GAP : undefined,
        top,
        width: POPOVER_W,
        background: 'rgba(22,22,25,0.96)',
        border: '1px solid rgba(255,255,255,0.09)',
        boxShadow: '0 12px 32px rgba(0,0,0,0.5)',
        // 入场：从 bar 一侧轻滑淡入
        animation: `qpp-in-${mirror ? 'r' : 'l'} 0.16s ease`,
      }}
    >
      <span
        className="absolute h-[10px] w-[10px] rotate-45"
        style={{
          left: mirror ? undefined : -5,
          right: mirror ? -5 : undefined,
          top: arrowTop,
          background: 'rgba(22,22,25,0.96)',
          borderLeft: mirror ? undefined : '1px solid rgba(255,255,255,0.09)',
          borderBottom: mirror ? undefined : '1px solid rgba(255,255,255,0.09)',
          borderRight: mirror ? '1px solid rgba(255,255,255,0.09)' : undefined,
          borderTop: mirror ? '1px solid rgba(255,255,255,0.09)' : undefined,
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

/** 停靠轮廓的贝塞尔控制点拉力，0.55 = 常用圆弧近似，保证端头扫得开而不是压成薄片 */
const BERTH_K = 0.55;

/**
 * 「贴右侧」标准轮廓（右缘整段平齐 + 上下内凹弧 + 左缘圆角），其余朝向靠 transform 翻转。
 * openness 0→1 是同一个轮廓从细条长到悬浮条，不是两个元素互换。
 */
function berthPath(w: number, h: number, openness: number, cornerR: number): string {
  const f = Math.min(FLARE_H * openness, h / 2);
  // 外扩弧要给圆角留位置，否则 body 的平边会倒走、轮廓自我折叠
  const r = Math.max(Math.min(cornerR, Math.min(w, (h - f * 2) / 2)), 0);
  const fw = Math.max(Math.min(FLARE_W * openness, w - r), 0);
  return [
    `M${r} ${f}`,
    `L${w - fw} ${f}`,
    `C${w - fw * (1 - BERTH_K)} ${f} ${w} ${f * BERTH_K} ${w} 0`,
    `L${w} ${h}`,
    `C${w} ${h - f * BERTH_K} ${w - fw * (1 - BERTH_K)} ${h - f} ${w - fw} ${h - f}`,
    `L${r} ${h - f}`,
    `A${r} ${r} 0 0 1 0 ${h - f - r}`,
    `L0 ${f + r}`,
    `A${r} ${r} 0 0 1 ${r} ${f}`,
    'Z',
  ].join(' ');
}

/** 未停靠时没有边缘可融合，退化成真胶囊：端头是半圆而非收圆的角 */
function capsulePath(w: number, h: number): string {
  const r = Math.min(w, h) / 2;
  return [
    `M${r} 0`,
    `L${w - r} 0`,
    `A${r} ${r} 0 0 1 ${w} ${r}`,
    `L${w} ${h - r}`,
    `A${r} ${r} 0 0 1 ${w - r} ${h}`,
    `L${r} ${h}`,
    `A${r} ${r} 0 0 1 0 ${h - r}`,
    `L0 ${r}`,
    `A${r} ${r} 0 0 1 ${r} 0`,
    'Z',
  ].join(' ');
}

/**
 * 轮廓形变只能逐帧重算 path，CSS 过渡帮不上忙（WKWebView 对 d 属性支持不齐）。
 * 指数逼近：每帧走掉剩余距离的 28%，约 250ms 收尾。
 */
function useOpenness(target: number): number {
  const [value, setValue] = useState(target);
  const current = useRef(target);

  useEffect(() => {
    let raf = 0;
    const step = () => {
      const next = current.current + (target - current.current) * 0.28;
      if (Math.abs(target - next) < 0.005) {
        current.current = target;
        setValue(target);
        return;
      }
      current.current = next;
      setValue(next);
      raf = requestAnimationFrame(step);
    };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
  }, [target]);

  return value;
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
  // 明细卡展开方向：true = 向左展开（docked 右缘 / 悬浮在右半屏），由 Rust 判定
  const [mirror, setMirror] = useState(false);
  // 停靠态：贴屏侧走外扩弧轮廓，未停靠退化为胶囊
  const [docked, setDocked] = useState(false);
  // 停靠态指针离开后收成细条
  const [collapsed, setCollapsed] = useState(false);
  // 悬浮条实测高度：SVG 轮廓需要数值坐标，不能只靠 CSS 撑开
  const [barH, setBarH] = useState(INITIAL_BAR_H);
  const dockedRef = useRef(false);
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
    document.documentElement.classList.add('quota-pill-window');
    document.documentElement.style.background = 'transparent';
    document.documentElement.style.backgroundColor = 'transparent';
    document.body.style.background = 'transparent';
    document.body.style.backgroundColor = 'transparent';
    const root = document.getElementById('root');
    if (root) {
      root.style.background = 'transparent';
      root.style.backgroundColor = 'transparent';
    }

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
      // 从托盘恢复显示时展开成完整悬浮条
      setCollapsed(false);
    });
    return () => {
      void un.then((f) => f());
      void un2.then((f) => f());
    };
  }, [load]);

  // 展开方向/停靠态跟随拖拽位置：挂载时查询 + 监听 Rust 推送
  useEffect(() => {
    if (!isTauri()) return;
    void api.quotaPillLayout().then((l) => {
      setMirror(l.mirror);
      setDocked(l.docked);
      dockedRef.current = l.docked;
      setCollapsed(l.docked);
    });
    const un = listen<{ mirror: boolean; docked: boolean }>('quota-pill-layout', (e) => {
      setMirror(e.payload.mirror);
      setDocked(e.payload.docked);
      dockedRef.current = e.payload.docked;
      if (!e.payload.docked) setCollapsed(false);
    });
    return () => {
      void un.then((f) => f());
    };
  }, []);

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
    closeTimer.current = setTimeout(() => {
      setHovered(null);
      // 停靠态指针离开后一并收拢成细条
      if (dockedRef.current) setCollapsed(true);
    }, CLOSE_DELAY_MS);
  }, [cancelClose]);

  const handleEnter = useCallback(() => {
    cancelClose();
    setCollapsed(false);
  }, [cancelClose]);

  // Rust 指针监视器：非前台时 macOS 不派发 hover 事件，靠轮询事件展开/收起
  useEffect(() => {
    if (!isTauri()) return;
    const un = listen<boolean>('quota-pill-hover', (e) => {
      if (e.payload) handleEnter();
      else scheduleClose();
    });
    return () => {
      void un.then((f) => f());
    };
  }, [handleEnter, scheduleClose]);

  // 收拢态 + 明细卡开合同步给 Rust：指针监视器据此圈定可交互区（细条 / bar / 整窗），
  // 区外整窗点击穿透，不吃下层 app 的鼠标事件
  useEffect(() => {
    if (!isTauri()) return;
    void api.setQuotaPillHoverState(collapsed, hovered != null);
  }, [collapsed, hovered]);

  // logo 右键弹出原生菜单（隐藏悬浮条 / 退出）
  const openMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    void api.showQuotaPillMenu();
  }, []);

  const entries = useMemo(() => buildEntries(snapshot?.providers ?? []), [snapshot]);
  const hoveredIndex = hovered ? entries.findIndex((e) => e.key === hovered) : -1;
  const hoveredEntry = hoveredIndex >= 0 ? entries[hoveredIndex] : null;

  const rowAt = useCallback((x: number, y: number): string | null => {
    const barLeft = mirror ? window.innerWidth - BAR_W : 0;
    if (x < barLeft + RING_HIT_INSET || x > barLeft + BAR_W - RING_HIT_INSET) return null;
    const index = entries.findIndex((_entry, i) => {
      const top = itemTop(i);
      return y >= top - 4 && y < top + ITEM_H + 4;
    });
    return index >= 0 ? entries[index].key : null;
  }, [mirror, entries]);

  // 明细卡命中：非前台窗口的 mousemove macOS 只发给 key window，
  // 不先点一下就把鼠标移上来永远不会触发 onMouseEnter —— 命中判定改用监视器报的指针坐标
  useEffect(() => {
    if (!isTauri()) return;
    const un = listen<{ x: number; y: number } | null>('quota-pill-pointer', (e) => {
      const p = e.payload;
      if (!p) {
        scheduleClose();
        return;
      }
      const key = rowAt(p.x, p.y);
      if (key) openItem(key);
      else cancelClose();
    });
    return () => {
      void un.then((f) => f());
    };
  }, [rowAt, openItem, scheduleClose, cancelClose]);

  // 窗口高度跟随内容（宽度恒定，由 Rust 按 PANEL_W 固定）；收拢/展开只是同一轮廓形变
  useLayoutEffect(() => {
    const measured = barRef.current?.offsetHeight || INITIAL_BAR_H;
    setBarH((prev) => (Math.abs(prev - measured) < 1 ? prev : measured));
    let height = measured;
    if (popoverRef.current) {
      height = Math.max(measured, itemTop(hoveredIndex) + popoverRef.current.offsetHeight + PAD);
    }
    void api.resizeQuotaPill(height);
  }, [hoveredIndex, entries.length, snapshot]);

  const openness = useOpenness(collapsed ? 0 : 1);
  const shapeW = SLIVER_W + (BAR_W - SLIVER_W) * openness;
  const shapeH = SLIVER_H + (barH - SLIVER_H) * openness;
  const cornerR = SLIVER_W + (CORNER_R - SLIVER_W) * openness;
  const shapeD = docked
    ? berthPath(shapeW, shapeH, openness, cornerR)
    : capsulePath(shapeW, shapeH);
  // 轮廓只画「贴右缘且顶部对齐」那一版：贴左缘就地镜像，细条短于 bar 时垂直居中
  const flip = docked && !mirror;
  const shapeTransform = `translate(${flip ? shapeW : 0} ${(barH - shapeH) / 2})${
    flip ? ' scale(-1 1)' : ''
  }`;
  // 环宽 44，轮廓长到装得下才开始显影，避免内容探出轮廓外
  const contentOpacity = Math.max(Math.min((openness - 0.6) / 0.4, 1), 0);

  return (
    <div
      className="fixed inset-0 select-none overflow-hidden"
      style={{ fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif" }}
      onMouseEnter={handleEnter}
      onMouseLeave={scheduleClose}
    >
      <style>{`
@keyframes qpp-in-l{from{opacity:0;transform:translateX(-6px)}to{opacity:1;transform:translateX(0)}}
@keyframes qpp-in-r{from{opacity:0;transform:translateX(6px)}to{opacity:1;transform:translateX(0)}}
`}</style>
      {/* 悬浮条本体：贴边侧整段平齐、端头内凹弧扫向屏幕边缘，收拢 = 同一轮廓的 flare 卷到 0 */}
      <div
        ref={barRef}
        data-tauri-drag-region
        onContextMenu={openMenu}
        title={t('quota.pill.handleTip')}
        className={`absolute top-0 ${mirror ? 'right-0' : 'left-0'}`}
        style={{ width: shapeW }}
      >
        {/* 不用外阴影——透明窗上 box-shadow 容易在四角糊成灰底 */}
        <svg
          className="pointer-events-none absolute left-0 top-0 overflow-visible"
          width={shapeW}
          height={barH}
        >
          <path
            d={shapeD}
            transform={shapeTransform}
            fill={SLIVER_FILL}
            opacity={1 - openness}
          />
          <path
            d={shapeD}
            transform={shapeTransform}
            fill={BAR_FILL}
            stroke="rgba(255,255,255,0.1)"
            strokeWidth={1}
            opacity={openness}
          />
        </svg>
        <div
          data-tauri-drag-region
          className="relative flex flex-col items-center"
          style={{
            // 恒定 BAR_W：宽度形变只发生在轮廓上，内容不参与重排
            width: BAR_W,
            gap: ITEM_GAP,
            padding: `${PAD}px 0`,
            opacity: contentOpacity,
            pointerEvents: collapsed ? 'none' : 'auto',
          }}
        >
          <div
            data-tauri-drag-region
            onContextMenu={openMenu}
            className="mb-[4px] flex h-[30px] w-full shrink-0 items-center justify-center"
            title={t('quota.pill.handleTip')}
          >
            <img
              src="/app-icon.png"
              alt=""
              draggable={false}
              className="pointer-events-none h-[20px] w-[20px] opacity-90"
            />
          </div>
          {entries.map((entry) => (
            <button
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
      </div>

      {hoveredEntry && (
        <Popover
          innerRef={popoverRef}
          mirror={mirror}
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
