import React, { useEffect, useRef, useState } from 'react';
import { ChevronDown, Loader2 } from 'lucide-react';
import { api } from '../../api/tauriBridge';
import type { IdeAppStatus } from '../../types';
import { IdeIcon } from './ideIcons';
import { useI18n, type MessageKey } from '../../i18n';

const IDE_HINT_KEYS: Record<string, MessageKey> = {
  cursor: 'ide.hint.cursor',
  antigravity: 'ide.hint.antigravity',
  claude: 'ide.hint.claude',
  codex: 'ide.hint.codex',
};

const DEFAULT_IDES: IdeAppStatus[] = [
  { id: 'cursor', label: 'Cursor', kind: 'app', installed: true },
  { id: 'antigravity', label: 'Antigravity', kind: 'app', installed: false },
  { id: 'claude', label: 'Claude Code', kind: 'cli', installed: false },
  { id: 'codex', label: 'Codex', kind: 'cli', installed: false },
];

interface Props {
  workspacePath: string;
}

export const OpenInIdeMenu: React.FC<Props> = ({ workspacePath }) => {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const [apps, setApps] = useState<IdeAppStatus[]>(DEFAULT_IDES);
  const [openingId, setOpeningId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const primary = apps[0] ?? DEFAULT_IDES[0];

  useEffect(() => {
    let cancelled = false;
    api.listIdeApps().then((list) => {
      if (!cancelled && list.length) setApps(list);
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
        title={t('ide.openIn', { name: primary.label })}
        onClick={() => {
          setOpen((prev) => !prev);
          setError(null);
        }}
        className={`h-6 inline-flex items-center gap-0.5 px-1 rounded-md theme-text-main hover:theme-bg-sub cursor-pointer transition-colors ${
          open ? 'theme-bg-sub' : ''
        }`}
      >
        <IdeIcon id={primary.id} className="h-3.5 w-3.5" />
        <ChevronDown className={`h-3 w-3 theme-text-sub transition-transform ${open ? 'rotate-180' : ''}`} />
      </button>

      {open && (
        <div className="absolute right-0 top-full z-50 mt-1.5 w-56 rounded-xl p-1 shadow-xl border theme-border theme-bg-card">
          <div className="px-2.5 py-1.5 text-[10px] font-semibold theme-text-sub tracking-wide">
            {t('ide.menuTitle')}
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
                <span className="mt-0.5 flex-shrink-0 h-4 w-4 inline-flex items-center justify-center">
                  {openingId === ide.id ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin text-blue-500" />
                  ) : (
                    <IdeIcon id={ide.id} className="h-3.5 w-3.5" />
                  )}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block text-xs font-medium theme-text-main">
                    {ide.label}
                    {!ide.installed && (
                      <span className="ml-1 text-[10px] font-normal theme-text-sub">{t('ide.notInstalled')}</span>
                    )}
                  </span>
                  <span className="block text-[10px] theme-text-sub truncate">
                    {ide.installed
                      ? (IDE_HINT_KEYS[ide.id] ? t(IDE_HINT_KEYS[ide.id]) : t(ide.kind === 'cli' ? 'ide.openTerminal' : 'ide.openProject'))
                      : t(ide.kind === 'cli' ? 'ide.noPath' : 'ide.noApp')}
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
