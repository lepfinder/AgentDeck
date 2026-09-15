import { useCallback, useEffect, useRef, useState } from 'react';
import { Loader2, RefreshCw } from 'lucide-react';
import { formatServiceLog } from '../../utils/formatServiceLog';
import { cn } from '../../lib/cn';

interface ServiceConsoleProps {
  serviceId: string | null;
  logFile?: string;
  live?: boolean;
  onFetchLog: (id: string, lines: number) => Promise<string>;
}

export function ServiceConsole({
  serviceId,
  logFile,
  live = false,
  onFetchLog,
}: ServiceConsoleProps) {
  const [raw, setRaw] = useState('');
  const [loading, setLoading] = useState(false);
  const [hideProgress, setHideProgress] = useState(true);
  const [stickToBottom, setStickToBottom] = useState(true);
  const scrollRef = useRef<HTMLDivElement>(null);

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
    if (serviceId) void load();
  }, [serviceId, load]);

  useEffect(() => {
    if (!serviceId || !live) return;
    const timer = setInterval(() => void load(), 2000);
    return () => clearInterval(timer);
  }, [serviceId, live, load]);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el || !stickToBottom || loading) return;
    el.scrollTop = el.scrollHeight;
  }, [raw, hideProgress, loading, stickToBottom]);

  const handleScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    const nearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 48;
    setStickToBottom(nearBottom);
  };

  const lines = formatServiceLog(raw, { hideProgress, maxLines: 300 });

  return (
    <div className="flex h-full min-h-0 flex-col gap-2">
      <div className="flex items-center justify-between gap-3 shrink-0">
        <div className="flex items-center gap-3 min-w-0">
          <span className="text-sm font-medium theme-text-main">控制台</span>
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

      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className={cn(
          'flex-1 min-h-0 overflow-auto rounded-xl border theme-border',
          'bg-zinc-950 text-zinc-100 shadow-inner'
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
              {lines.map((line, idx) => (
                <tr key={idx} className="hover:bg-zinc-900/80">
                  <td className="select-none w-10 shrink-0 px-2.5 py-0.5 text-right align-top text-zinc-600 border-r border-zinc-800/80">
                    {idx + 1}
                  </td>
                  <td className="px-2.5 py-0.5 align-top whitespace-pre-wrap break-words text-zinc-200">
                    {line.trim() === '' ? '\u00a0' : line}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
