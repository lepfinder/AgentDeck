import React from 'react';
import { CheckCircle2, ListTree, Loader2, PanelRightClose, RefreshCw } from 'lucide-react';
import type { ConversationOutline, OutlineSegment } from '../../types';
import { useI18n } from '../../i18n';

interface Props {
  outline: ConversationOutline | null;
  stale?: boolean;
  generating?: boolean;
  openEnded?: boolean;
  canGenerate: boolean;
  model?: string | null;
  generatedAt?: string | null;
  statusMessage?: string | null;
  statusTone?: 'success' | 'error' | null;
  onGenerate: (forceFull?: boolean) => void;
  onJump: (segment: OutlineSegment) => void;
  onClose: () => void;
  formatTime: (iso: string) => string;
}

export const SessionOutlinePanel: React.FC<Props> = ({
  outline,
  stale,
  generating,
  openEnded,
  canGenerate,
  model,
  generatedAt,
  statusMessage,
  statusTone,
  onGenerate,
  onJump,
  onClose,
  formatTime,
}) => {
  const { t } = useI18n();
  const segments = outline?.segments ?? [];
  const hasOutline = segments.length > 0;

  // Single loading/status surface: banner only (no title/button/list spinners).
  const bannerMessage =
    statusMessage ||
    (generating
      ? hasOutline && openEnded
        ? t('conv.outlineUpdating')
        : t('conv.outlineWorking')
      : null);
  const bannerTone: 'success' | 'error' | null = statusTone ?? null;

  return (
    <aside className="w-72 xl:w-80 shrink-0 h-full min-h-0 flex flex-col border-l theme-border theme-bg-sub/40">
      <div className="px-3 py-2.5 border-b theme-border flex items-start justify-between gap-2 shrink-0">
        <div className="min-w-0 space-y-1">
          <div className="flex items-center gap-1.5 text-xs font-semibold theme-text-main">
            <ListTree className="h-3.5 w-3.5 text-teal-500 shrink-0" />
            <span className="truncate">{t('conv.outlineTitle')}</span>
            {openEnded && (
              <span className="shrink-0 px-1.5 py-0.5 text-[10px] font-medium rounded bg-teal-500/15 text-teal-600 border border-teal-500/30">
                {t('conv.openEndedShort')}
              </span>
            )}
            {stale && !generating && (
              <span
                className="shrink-0 px-1.5 py-0.5 text-[10px] font-medium rounded bg-amber-500/15 text-amber-600 border border-amber-500/30"
                title={t('conv.outlineStaleHint')}
              >
                {t('conv.outlineStale')}
              </span>
            )}
          </div>
          {(model || generatedAt) && !generating && (
            <div className="text-[10px] theme-text-muted truncate pl-5">
              {model ? <span className="font-mono">{model}</span> : null}
              {model && generatedAt ? ' · ' : null}
              {generatedAt
                ? t('conv.outlineGeneratedAt', { time: formatTime(generatedAt) })
                : null}
            </div>
          )}
        </div>
        <button
          type="button"
          onClick={onClose}
          title={t('conv.hideOutline')}
          className="p-1 rounded-md theme-text-muted hover:theme-text-main hover:bg-black/5 dark:hover:bg-white/5 cursor-pointer shrink-0"
        >
          <PanelRightClose className="h-3.5 w-3.5" />
        </button>
      </div>

      <div className="px-3 py-2 flex flex-wrap items-center gap-1.5 shrink-0 border-b theme-border">
        {canGenerate && (
          <button
            type="button"
            disabled={generating}
            onClick={() => onGenerate(hasOutline && openEnded ? false : hasOutline)}
            className="inline-flex items-center gap-1 text-[11px] px-2 py-1 rounded-md border border-teal-500/35 bg-teal-500/10 text-teal-700 dark:text-teal-400 hover:bg-teal-500/20 disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer"
            title={
              hasOutline
                ? openEnded
                  ? t('conv.outlineUpdateHint')
                  : t('conv.outlineRegenHint')
                : t('conv.outlineGenerateHint')
            }
          >
            <RefreshCw className={`h-3 w-3 ${generating ? 'animate-spin' : ''}`} />
            {hasOutline
              ? openEnded
                ? t('conv.outlineUpdate')
                : t('conv.outlineRegen')
              : t('conv.outlineGenerate')}
          </button>
        )}
        {hasOutline && openEnded && canGenerate && (
          <button
            type="button"
            disabled={generating}
            onClick={() => onGenerate(true)}
            className="inline-flex items-center gap-1 text-[11px] theme-text-muted hover:theme-text-main cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
            title={t('conv.outlineRegenHint')}
          >
            {t('conv.outlineRegen')}
          </button>
        )}
      </div>

      {bannerMessage && (
        <div
          className={`mx-3 mt-2 shrink-0 flex items-center gap-1.5 text-[11px] px-2.5 py-1.5 rounded-md border ${
            bannerTone === 'error'
              ? 'bg-red-500/10 border-red-500/30 text-red-600 dark:text-red-400'
              : bannerTone === 'success'
                ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-700 dark:text-emerald-400'
                : 'bg-teal-500/10 border-teal-500/30 text-teal-700 dark:text-teal-400'
          }`}
        >
          {bannerTone === 'success' ? (
            <CheckCircle2 className="h-3.5 w-3.5 shrink-0" />
          ) : bannerTone === 'error' ? null : (
            <Loader2 className="h-3.5 w-3.5 shrink-0 animate-spin" />
          )}
          <span className="truncate">{bannerMessage}</span>
        </div>
      )}

      <div className="flex-1 min-h-0 overflow-y-auto px-2 py-2">
        {!hasOutline && !generating ? (
          <p className="text-xs theme-text-muted px-1.5 py-2">
            {canGenerate ? t('conv.outlineEmpty') : t('conv.outlineNeedMore')}
          </p>
        ) : hasOutline ? (
          <ol className={`space-y-1 ${generating ? 'opacity-60 pointer-events-none' : ''}`}>
            {segments.map((seg, i) => (
              <li key={`${seg.start_user_index}-${i}`}>
                <button
                  type="button"
                  onClick={() => onJump(seg)}
                  disabled={generating}
                  className="w-full text-left px-2.5 py-2 rounded-lg hover:bg-teal-500/10 border border-transparent hover:border-teal-500/25 transition-colors cursor-pointer group disabled:cursor-not-allowed"
                >
                  <div className="flex items-baseline gap-2 min-w-0">
                    <span className="text-[10px] font-mono theme-text-muted shrink-0 w-5">
                      {i + 1}.
                    </span>
                    <span className="text-xs font-medium theme-text-main group-hover:text-teal-600 dark:group-hover:text-teal-400 break-words">
                      {seg.title}
                    </span>
                  </div>
                  <div className="pl-7 mt-0.5 flex items-center gap-1.5 text-[10px] font-mono theme-text-muted">
                    <span>#{seg.start_user_index}</span>
                    <span>·</span>
                    <span>step {seg.start_step_index}</span>
                  </div>
                  {seg.summary && (
                    <p className="text-[11px] theme-text-sub leading-snug mt-1 pl-7 line-clamp-3">
                      {seg.summary}
                    </p>
                  )}
                </button>
              </li>
            ))}
          </ol>
        ) : generating ? (
          <div className="flex items-center gap-2 text-xs theme-text-muted px-1.5 py-3">
            <Loader2 className="h-3.5 w-3.5 animate-spin text-teal-500 shrink-0" />
            <span>{t('conv.outlineWorking')}</span>
          </div>
        ) : null}
      </div>
    </aside>
  );
};
