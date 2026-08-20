import React, { useEffect, useRef, useState } from 'react';
import { ExternalLink, Loader2 } from 'lucide-react';
import { api } from '../../api/tauriBridge';
import type { IdeAppStatus } from '../../types';

const IDE_HINT: Record<string, string> = {
  cursor: '用 Cursor 打开此项目',
  antigravity: '用 Antigravity 打开此项目',
  claude: '在终端进入目录并启动 claude',
  codex: '在终端进入目录并启动 codex',
};

interface Props {
  workspacePath: string;
}

export const OpenInIdeMenu: React.FC<Props> = ({ workspacePath }) => {
  const [open, setOpen] = useState(false);
  const [apps, setApps] = useState<IdeAppStatus[]>([]);
  const [openingId, setOpeningId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    api.listIdeApps().then((list) => {
      if (!cancelled) setApps(list);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!open) return;
    const handleClickOutside = (event: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(event.target as Node)) {
        setOpen(false);
        setError(null);
      }
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setOpen(false);
        setError(null);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [open]);

  const handleOpen = async (ide: IdeAppStatus) => {
    if (!ide.installed || openingId) return;
    setOpeningId(ide.id);
    setError(null);
    try {
      await api.openWorkspaceInIde(ide.id, workspacePath);
      setOpen(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setOpeningId(null);
    }
  };

  return (
    <div ref={containerRef} className="relative">
      <button
        type="button"
        title="在 AI IDE 中打开"
        onClick={() => {
          setOpen((prev) => !prev);
          setError(null);
        }}
        className={`h-6 w-6 inline-flex items-center justify-center rounded-md theme-text-sub hover:theme-text-main hover:theme-bg-sub cursor-pointer transition-colors ${
          open ? 'text-blue-500 theme-bg-sub' : ''
        }`}
      >
        <ExternalLink className="h-3.5 w-3.5" />
      </button>

      {open && (
        <div className="absolute right-0 top-full z-50 mt-1.5 w-56 rounded-xl p-1 shadow-xl border theme-border theme-bg-card">
          <div className="px-2.5 py-1.5 text-[10px] font-semibold theme-text-sub tracking-wide">
            在 AI IDE 中打开
          </div>
          {apps.map((ide) => {
            const disabled = !ide.installed || openingId !== null;
            return (
              <button
                key={ide.id}
                type="button"
                disabled={disabled}
                onClick={() => handleOpen(ide)}
                className={`w-full flex items-start gap-2 px-2.5 py-1.5 rounded-lg text-left transition-colors ${
                  disabled
                    ? 'opacity-50 cursor-not-allowed'
                    : 'cursor-pointer hover:theme-bg-sub'
                }`}
              >
                <span className="mt-0.5 flex-shrink-0">
                  {openingId === ide.id ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin text-blue-500" />
                  ) : (
                    <IdeDot id={ide.id} />
                  )}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block text-xs font-medium theme-text-main">
                    {ide.label}
                    {!ide.installed && (
                      <span className="ml-1 text-[10px] font-normal theme-text-sub">未安装</span>
                    )}
                  </span>
                  <span className="block text-[10px] theme-text-sub truncate">
                    {ide.installed
                      ? IDE_HINT[ide.id] || (ide.kind === 'cli' ? '在终端打开该目录' : '打开此项目')
                      : ide.kind === 'cli'
                        ? '未在 PATH 中找到命令'
                        : '未找到应用程序'}
                  </span>
                </span>
              </button>
            );
          })}
          {error && (
            <div className="px-2.5 py-1.5 text-[10px] text-red-500 leading-snug">{error}</div>
          )}
        </div>
      )}
    </div>
  );
};

const IdeDot: React.FC<{ id: string }> = ({ id }) => {
  const color =
    id === 'cursor'
      ? 'bg-blue-500'
      : id === 'antigravity'
        ? 'bg-emerald-500'
        : id === 'claude'
          ? 'bg-orange-500'
          : id === 'codex'
            ? 'bg-pink-500'
            : 'bg-slate-400';
  return <span className={`mt-0.5 inline-block h-2 w-2 rounded-full ${color}`} />;
};
