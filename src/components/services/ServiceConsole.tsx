import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  AlertTriangle,
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  Copy,
  EyeOff,
  Loader2,
  RefreshCw,
} from 'lucide-react';
import {
  findErrorLineIndices,
  formatErrorAgo,
  formatServiceLog,
  parseLogLineTimestamp,
  sliceLogContext,
} from '../../utils/formatServiceLog';
import { cn } from '../../lib/cn';

interface ServiceConsoleProps {
  serviceId: string | null;
  logFile?: string;
  live?: boolean;
  onFetchLog: (id: string, lines: number) => Promise<string>;
  /** Persisted ignore set from parent (localStorage-backed). */
  ignoredSignatures?: ReadonlySet<string>;
  ignoreEpoch?: number;
  onIgnoreErrors?: (signatures: string[]) => void;
}

function errorSignature(line: string): string {
  return line.trim();
}

export function ServiceConsole({
  serviceId,
  logFile,
  live = false,
  onFetchLog,
  ignoredSignatures,
  ignoreEpoch = 0,
  onIgnoreErrors,
}: ServiceConsoleProps) {
  const [raw, setRaw] = useState('');
  const [loading, setLoading] = useState(false);
  const [hideProgress, setHideProgress] = useState(true);
  const [stickToBottom, setStickToBottom] = useState(true);
  const [errorCursor, setErrorCursor] = useState(-1);
  const [copiedHint, setCopiedHint] = useState<string | null>(null);
  const [newErrorFlash, setNewErrorFlash] = useState(false);
  const [ignoredHint, setIgnoredHint] = useState(false);
  const [ignoredSigs, setIgnoredSigs] = useState<Set<string>>(() => new Set());
  const [nowMs, setNowMs] = useState(() => Date.now());
  const scrollRef = useRef<HTMLDivElement>(null);
  const lineRefs = useRef<Map<number, HTMLTableRowElement>>(new Map());
  const prevActiveCountRef = useRef(0);
  const jumpRequestRef = useRef(false);

  // Hydrate / refresh from persisted parent store (survives page switch).
  useEffect(() => {
    setIgnoredSigs(new Set(ignoredSignatures ?? []));
  }, [serviceId, ignoreEpoch]); // ignoredSignatures content tracked via ignoreEpoch

  const load = useCallback(async () => {
    if (!serviceId) return;
    setLoading(true);
    try {
      const content = await onFetchLog(serviceId, 200);
      setRaw(content);
    } finally {
      setLoading(false);
    }
  }, [onFetchLog, serviceId]);

  useEffect(() => {
    setRaw('');
    setStickToBottom(true);
    setErrorCursor(-1);
    prevActiveCountRef.current = 0;
    setNewErrorFlash(false);
    setIgnoredHint(false);
    setIgnoredSigs(new Set(ignoredSignatures ?? []));
    if (serviceId) void load();
    // only reset view when switching service; ignore set comes from effect above
    // eslint-disable-next-line react-hooks/exhaustive-deps -- intentional: service switch reset
  }, [serviceId, load]);

  useEffect(() => {
    if (!serviceId || !live) return;
    const timer = setInterval(() => void load(), 2000);
    return () => clearInterval(timer);
  }, [serviceId, live, load]);

  const lines = useMemo(
    () => formatServiceLog(raw, { hideProgress, maxLines: 300 }),
    [raw, hideProgress],
  );
  const allErrorIndices = useMemo(() => findErrorLineIndices(lines), [lines]);
  const errorIndices = useMemo(
    () => allErrorIndices.filter((idx) => !ignoredSigs.has(errorSignature(lines[idx] ?? ''))),
    [allErrorIndices, ignoredSigs, lines],
  );
  const errorSet = useMemo(() => new Set(errorIndices), [errorIndices]);
  const ignoredErrorSet = useMemo(
    () =>
      new Set(
        allErrorIndices.filter((idx) => ignoredSigs.has(errorSignature(lines[idx] ?? ''))),
      ),
    [allErrorIndices, ignoredSigs, lines],
  );
  const errorCount = errorIndices.length;
  const activeLineIdx =
    errorCursor >= 0 && errorCursor < errorIndices.length
      ? errorIndices[errorCursor]
      : -1;
  const latestErrorAtMs = useMemo(() => {
    let best: number | null = null;
    for (const idx of errorIndices) {
      const at = parseLogLineTimestamp(lines[idx] ?? '');
      if (at != null && (best == null || at > best)) best = at;
    }
    return best;
  }, [errorIndices, lines]);

  useEffect(() => {
    if (errorCount === 0) return;
    const t = window.setInterval(() => setNowMs(Date.now()), 15000);
    return () => window.clearInterval(t);
  }, [errorCount]);

  useEffect(() => {
    if (errorCount === 0) {
      setErrorCursor(-1);
      prevActiveCountRef.current = 0;
      return;
    }

    const prev = prevActiveCountRef.current;
    if (errorCount > prev) {
      setErrorCursor(errorCount - 1);
      jumpRequestRef.current = true;
      setStickToBottom(false);
      setNewErrorFlash(true);
      setIgnoredHint(false);
      const t = window.setTimeout(() => setNewErrorFlash(false), 2500);
      prevActiveCountRef.current = errorCount;
      return () => window.clearTimeout(t);
    }

    setErrorCursor((c) => (c < 0 || c >= errorCount ? errorCount - 1 : c));
    prevActiveCountRef.current = errorCount;
  }, [errorCount]);

  useEffect(() => {
    if (activeLineIdx < 0) return;
    if (!jumpRequestRef.current && stickToBottom) return;
    const row = lineRefs.current.get(activeLineIdx);
    if (row) {
      row.scrollIntoView({ block: 'center', behavior: 'smooth' });
      jumpRequestRef.current = false;
    }
  }, [activeLineIdx, lines, stickToBottom]);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el || loading || !stickToBottom || jumpRequestRef.current) return;
    el.scrollTop = el.scrollHeight;
  }, [raw, hideProgress, loading, stickToBottom, lines.length]);

  const handleScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    const nearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 48;
    setStickToBottom(nearBottom);
  };

  const goError = (delta: number) => {
    if (errorCount === 0) return;
    setStickToBottom(false);
    jumpRequestRef.current = true;
    setErrorCursor((c) => {
      const base = c < 0 ? (delta > 0 ? -1 : 0) : c;
      return (base + delta + errorCount) % errorCount;
    });
  };

  const jumpToLatestError = () => {
    if (errorCount === 0) return;
    setStickToBottom(false);
    jumpRequestRef.current = true;
    setErrorCursor(errorCount - 1);
  };

  const ignoreCurrentErrors = () => {
    if (errorIndices.length === 0) return;
    const signatures = errorIndices
      .map((idx) => errorSignature(lines[idx] ?? ''))
      .filter(Boolean);
    setIgnoredSigs((prev) => {
      const next = new Set(prev);
      for (const sig of signatures) next.add(sig);
      return next;
    });
    onIgnoreErrors?.(signatures);
    setErrorCursor(-1);
    setNewErrorFlash(false);
    prevActiveCountRef.current = 0;
    jumpRequestRef.current = false;
    setStickToBottom(true);
    setIgnoredHint(true);
    window.setTimeout(() => setIgnoredHint(false), 2000);
  };

  const copyText = async (text: string, id: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedHint(id);
      window.setTimeout(() => setCopiedHint(null), 1500);
    } catch (e) {
      console.error(e);
    }
  };

  const copyActiveError = () => {
    if (activeLineIdx < 0) return;
    void copyText(sliceLogContext(lines, activeLineIdx, 5), 'one');
  };

  const copyAllErrors = () => {
    if (errorIndices.length === 0) return;
    const blocks = errorIndices.map((idx, i) => {
      const ctx = sliceLogContext(lines, idx, 3);
      return `--- error ${i + 1} (L${idx + 1}) ---\n${ctx}`;
    });
    void copyText(blocks.join('\n\n'), 'all');
  };

  return (
    <div className="flex h-full min-h-0 flex-col gap-2">
      <div className="flex items-center justify-between gap-3 shrink-0">
        <div className="flex items-center gap-3 min-w-0">
          <span className="text-sm font-medium theme-text-main">控制台</span>
          {live && (
            <span className="inline-flex items-center gap-1 text-[10px] font-medium text-emerald-600 dark:text-emerald-400">
              <span className="h-1.5 w-1.5 rounded-full bg-emerald-500 animate-pulse" />
              实时
            </span>
          )}
          {logFile && (
            <span className="text-xs font-mono theme-text-sub truncate" title={logFile}>
              {logFile}
            </span>
          )}
        </div>
        <div className="flex items-center gap-3 shrink-0">
          <label className="flex items-center gap-1.5 text-xs theme-text-muted cursor-pointer">
            <input
              type="checkbox"
              checked={hideProgress}
              onChange={(e) => setHideProgress(e.target.checked)}
              className="rounded border theme-border"
            />
            隐藏进度噪声
          </label>
          <button
            type="button"
            onClick={() => void load()}
            disabled={!serviceId || loading}
            className="flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg border theme-border theme-bg-sub theme-text-muted hover:theme-text-main disabled:opacity-50 cursor-pointer"
          >
            {loading ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <RefreshCw className="h-3.5 w-3.5" />
            )}
            刷新
          </button>
        </div>
      </div>

      {serviceId && ignoredHint && errorCount === 0 && (
        <div className="shrink-0 flex items-center gap-2 rounded-xl border border-emerald-500/30 bg-emerald-500/10 px-3 py-2 text-xs text-emerald-700 dark:text-emerald-300">
          <CheckCircle2 className="h-3.5 w-3.5 shrink-0" />
          <span>已忽略当前错误，继续实时监控；出现新错误时会再次提示</span>
        </div>
      )}

      {serviceId && errorCount > 0 && (
        <div
          className={cn(
            'shrink-0 flex flex-wrap items-center gap-2 rounded-xl border px-3 py-2 text-xs',
            newErrorFlash
              ? 'border-red-500/50 bg-red-500/15 text-red-700 dark:text-red-300'
              : 'border-amber-500/40 bg-amber-500/10 text-amber-800 dark:text-amber-200',
          )}
        >
          <AlertTriangle className="h-3.5 w-3.5 shrink-0" />
          <span className="font-medium">
            {newErrorFlash ? '发现新错误 · ' : ''}
            共 {errorCount} 处可疑错误
            {latestErrorAtMs != null ? ` · 最新 ${formatErrorAgo(latestErrorAtMs, nowMs)}` : ''}
            {activeLineIdx >= 0 ? ` · 当前 L${activeLineIdx + 1}` : ''}
            {ignoredSigs.size > 0 ? ` · 已忽略 ${ignoredSigs.size}` : ''}
          </span>
          <div className="ml-auto flex flex-wrap items-center gap-1.5">
            <button
              type="button"
              onClick={() => goError(-1)}
              className="inline-flex items-center gap-0.5 rounded-md border border-current/20 px-1.5 py-0.5 hover:bg-black/5 dark:hover:bg-white/5 cursor-pointer"
              title="上一处错误"
            >
              <ChevronUp className="h-3.5 w-3.5" />
              上一个
            </button>
            <button
              type="button"
              onClick={() => goError(1)}
              className="inline-flex items-center gap-0.5 rounded-md border border-current/20 px-1.5 py-0.5 hover:bg-black/5 dark:hover:bg-white/5 cursor-pointer"
              title="下一处错误"
            >
              <ChevronDown className="h-3.5 w-3.5" />
              下一个
            </button>
            <button
              type="button"
              onClick={jumpToLatestError}
              className="rounded-md border border-current/20 px-1.5 py-0.5 hover:bg-black/5 dark:hover:bg-white/5 cursor-pointer"
            >
              最新
            </button>
            <button
              type="button"
              onClick={copyActiveError}
              disabled={activeLineIdx < 0}
              className="inline-flex items-center gap-1 rounded-md border border-current/20 px-1.5 py-0.5 hover:bg-black/5 dark:hover:bg-white/5 disabled:opacity-40 cursor-pointer"
            >
              {copiedHint === 'one' ? (
                <CheckCircle2 className="h-3 w-3" />
              ) : (
                <Copy className="h-3 w-3" />
              )}
              {copiedHint === 'one' ? '已复制' : '复制当前'}
            </button>
            <button
              type="button"
              onClick={copyAllErrors}
              className="inline-flex items-center gap-1 rounded-md border border-current/20 px-1.5 py-0.5 hover:bg-black/5 dark:hover:bg-white/5 cursor-pointer"
            >
              {copiedHint === 'all' ? (
                <CheckCircle2 className="h-3 w-3" />
              ) : (
                <Copy className="h-3 w-3" />
              )}
              {copiedHint === 'all' ? '已复制' : '复制全部'}
            </button>
            <button
              type="button"
              onClick={ignoreCurrentErrors}
              className="inline-flex items-center gap-1 rounded-md border border-current/20 px-1.5 py-0.5 hover:bg-black/5 dark:hover:bg-white/5 cursor-pointer"
              title="忽略当前这些错误并恢复跟滚监控"
            >
              <EyeOff className="h-3 w-3" />
              忽略
            </button>
          </div>
        </div>
      )}

      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className={cn(
          'flex-1 min-h-0 overflow-auto rounded-xl border theme-border',
          'bg-zinc-950 text-zinc-100 shadow-inner',
        )}
      >
        {!serviceId ? (
          <div className="flex h-full min-h-[160px] items-center justify-center text-zinc-500 text-sm">
            选择左侧项目查看日志
          </div>
        ) : loading && lines.length === 0 ? (
          <div className="flex h-full min-h-[160px] items-center justify-center text-zinc-500">
            <Loader2 className="h-5 w-5 animate-spin" />
          </div>
        ) : lines.length === 0 ? (
          <div className="flex h-full min-h-[160px] items-center justify-center text-zinc-500 text-sm">
            (暂无日志内容)
          </div>
        ) : (
          <table className="w-full border-collapse text-[12px] leading-relaxed font-mono">
            <tbody>
              {lines.map((line, idx) => {
                const isError = errorSet.has(idx);
                const isIgnored = ignoredErrorSet.has(idx);
                const isActive = idx === activeLineIdx;
                return (
                  <tr
                    key={idx}
                    ref={(el) => {
                      if (el) lineRefs.current.set(idx, el);
                      else lineRefs.current.delete(idx);
                    }}
                    onClick={() => {
                      if (!isError) return;
                      const pos = errorIndices.indexOf(idx);
                      if (pos >= 0) {
                        setStickToBottom(false);
                        jumpRequestRef.current = true;
                        setErrorCursor(pos);
                      }
                    }}
                    className={cn(
                      isActive && 'bg-red-500/25 ring-1 ring-inset ring-red-400/40',
                      !isActive && isError && 'bg-red-500/10 hover:bg-red-500/20 cursor-pointer',
                      isIgnored && 'opacity-40',
                      !isError && !isIgnored && 'hover:bg-zinc-900/80',
                    )}
                  >
                    <td
                      className={cn(
                        'select-none w-10 shrink-0 px-2.5 py-0.5 text-right align-top border-r border-zinc-800/80',
                        isError ? 'text-red-400' : 'text-zinc-600',
                      )}
                    >
                      {idx + 1}
                    </td>
                    <td
                      className={cn(
                        'px-2.5 py-0.5 align-top whitespace-pre-wrap break-words',
                        isError ? 'text-red-200' : 'text-zinc-200',
                      )}
                    >
                      {line.trim() === '' ? '\u00a0' : line}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
