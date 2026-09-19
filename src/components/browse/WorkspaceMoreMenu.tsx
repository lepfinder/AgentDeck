/**
 * 侧边栏工作区卡片右上角「更多操作」菜单
 *
 * 菜单项：合并 / 重命名 · 在 Finder 中显示 · 复制路径。
 * 未分类工作区（无路径）只保留合并项。
 */
import React, { useEffect, useRef, useState } from 'react';
import { MoreHorizontal, ArrowRightLeft, FolderOpen, Copy, Check } from 'lucide-react';
import { api } from '../../api/tauriBridge';
import { useI18n } from '../../i18n';

interface Props {
  workspacePath: string;
  onMerge: () => void;
}

const menuItemClass =
  'w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-left text-xs theme-text-main hover:theme-bg-sub cursor-pointer transition-colors';

export const WorkspaceMoreMenu: React.FC<Props> = ({ workspacePath, onMerge }) => {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const hasPath = Boolean(workspacePath);

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

  const handleReveal = async () => {
    setError(null);
    try {
      await api.revealInFolder(workspacePath);
      setOpen(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(workspacePath);
      setCopied(true);
      window.setTimeout(() => {
        setCopied(false);
        setOpen(false);
      }, 800);
    } catch {
      /* ignore */
    }
  };

  return (
    <div ref={containerRef} className="absolute top-2 right-2">
      <button
        type="button"
        title={t('ws.moreActions')}
        onClick={(e) => {
          e.stopPropagation();
          setOpen((prev) => !prev);
          setError(null);
        }}
        className={`p-1 rounded-md theme-text-muted hover:theme-text-main hover:theme-bg-card cursor-pointer transition-opacity ${
          open ? 'opacity-100' : 'opacity-0 group-hover:opacity-100'
        }`}
      >
        <MoreHorizontal className="h-3.5 w-3.5" />
      </button>

      {open && (
        <div
          onClick={(e) => e.stopPropagation()}
          className="absolute right-0 top-full z-50 mt-1 w-44 rounded-xl p-1 shadow-xl border theme-border theme-bg-card"
        >
          <button
            type="button"
            className={menuItemClass}
            onClick={(e) => {
              e.stopPropagation();
              setOpen(false);
              onMerge();
            }}
          >
            <ArrowRightLeft className="h-3.5 w-3.5 theme-text-sub shrink-0" />
            {t('ws.merge')}
          </button>
          {hasPath && (
            <>
              <button
                type="button"
                className={menuItemClass}
                onClick={(e) => {
                  e.stopPropagation();
                  void handleReveal();
                }}
              >
                <FolderOpen className="h-3.5 w-3.5 theme-text-sub shrink-0" />
                {t('ws.revealInFolder')}
              </button>
              <button
                type="button"
                className={menuItemClass}
                onClick={(e) => {
                  e.stopPropagation();
                  void handleCopy();
                }}
              >
                {copied ? (
                  <Check className="h-3.5 w-3.5 text-emerald-500 shrink-0" />
                ) : (
                  <Copy className="h-3.5 w-3.5 theme-text-sub shrink-0" />
                )}
                {copied ? t('ws.pathCopied') : t('ws.copyPath')}
              </button>
            </>
          )}
          {error && (
            <div className="px-2.5 py-1.5 text-[10px] text-red-500 leading-snug">{error}</div>
          )}
        </div>
      )}
    </div>
  );
};
