import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ChevronDown, Eraser, Loader2, RefreshCw } from 'lucide-react';
import { findErrorLineIndices, formatServiceLog } from '../../utils/formatServiceLog';
import { cn } from '../../lib/cn';

interface ServiceConsoleProps {
  serviceId: string | null;
  logFile?: string;
  live?: boolean;
  onFetchLog: (id: string, lines: number) => Promise<string>;
  onClearLog?: (id: string) => Promise<unknown>;
}

export function ServiceConsole({
  serviceId,
  logFile,
  live = false,
  onFetchLog,
  onClearLog,
}: ServiceConsoleProps) {
  const [raw, setRaw] = useState('');
  const [loading, setLoading] = useState(false);
  const [clearing, setClearing] = useState(false);
  const [hideProgress, setHideProgress] = useState(true);
  const [stickToBottom, setStickToBottom] = useState(true);
  const scrollRef = useRef<HTMLDivElement>(null);
  /** 非跟滚时记录用户滚动位置，实时刷新后恢复，避免被拉回底部 */
  const lastScrollTopRef = useRef(0);

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
    lastScrollTopRef.current = 0;
    if (serviceId) void load();
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
  const errorSet = useMemo(() => new Set(findErrorLineIndices(lines)), [lines]);

  // 跟滚：仅在贴底且未手动上卷时滚到底；否则恢复上次阅读位置
  useEffect(() => {
    const el = scrollRef.current;
    if (!el || loading) return;
    if (stickToBottom) {
      el.scrollTop = el.scrollHeight;
    } else {
      const max = Math.max(0, el.scrollHeight - el.clientHeight);
      el.scrollTop = Math.min(lastScrollTopRef.current, max);
    }
  }, [raw, hideProgress, loading, stickToBottom, lines.length]);

  const handleScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    const nearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 48;
    lastScrollTopRef.current = el.scrollTop;
    setStickToBottom(nearBottom);
  };

  const scrollToBottom = () => {
    const el = scrollRef.current;
    setStickToBottom(true);
    if (el) {
      el.scrollTop = el.scrollHeight;
      lastScrollTopRef.current = el.scrollTop;
    }
  };

  const handleClear = useCallback(async () => {
    if (!serviceId || !onClearLog) return;
    setClearing(true);
    try {
      await onClearLog(serviceId);
      setRaw('');
      void load();
    } finally {
      setClearing(false);
    }
  }, [serviceId, onClearLog, load]);

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
          {onClearLog && (
            <button
              type="button"
              onClick={() => void handleClear()}
              disabled={!serviceId || clearing}
              title="清空磁盘上的日志文件"
              className="flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg border theme-border theme-bg-sub theme-text-muted hover:theme-text-main disabled:opacity-50 cursor-pointer"
            >
              {clearing ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Eraser className="h-3.5 w-3.5" />
              )}
              清空
            </button>
          )}
        </div>
      </div>

      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className={cn(
          'relative flex-1 min-h-0 overflow-auto rounded-xl border theme-border',
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
                return (
                  <tr
                    key={idx}
                    className={cn(
                      isError ? 'bg-red-500/10 hover:bg-red-500/20' : 'hover:bg-zinc-900/80',
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

        {/* 已上卷暂停跟滚时，一键回到底部 */}
        {serviceId && lines.length > 0 && !stickToBottom && (
          <button
            type="button"
            onClick={scrollToBottom}
            className="sticky bottom-2 left-1/2 z-10 -translate-x-1/2 flex items-center gap-1 rounded-full border border-zinc-600/80 bg-zinc-900/95 px-3 py-1 text-[11px] text-zinc-200 shadow-lg hover:bg-zinc-800 cursor-pointer"
          >
            <ChevronDown className="h-3 w-3" />
            回到底部 · 恢复实时跟随
          </button>
        )}
      </div>
    </div>
  );
}
