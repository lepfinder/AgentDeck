import React from 'react';
import { CheckCircle2, ListTree, Loader2, RefreshCw } from 'lucide-react';
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
  const primaryIsUpdate = hasOutline && openEnded;

  return (
    <div className="border-b theme-border theme-bg-sub/60">
      <div className="max-w-4xl mx-auto px-4 py-3">
        <div className="flex items-center justify-between gap-2 mb-2">
          <div className="flex items-center gap-2 text-xs font-semibold theme-text-main min-w-0">
            {generating ? (
              <Loader2 className="h-3.5 w-3.5 text-teal-500 shrink-0 animate-spin" />
            ) : (
              <ListTree className="h-3.5 w-3.5 text-teal-500 shrink-0" />
            )}
            <span>{t('conv.outlineTitle')}</span>
            {openEnded && (
              <span className="shrink-0 px-1.5 py-0.5 text-[10px] font-medium rounded bg-teal-500/15 text-teal-600 border border-teal-500/30">
                {t('conv.openEnded')}
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
            {model && !generating && (
              <span className="font-mono font-normal theme-text-muted truncate">· {model}</span>
            )}
            {generatedAt && !generating && (
              <span className="font-normal theme-text-muted shrink-0">
                · {t('conv.outlineGeneratedAt', { time: formatTime(generatedAt) })}
              </span>
            )}
          </div>
          <div className="flex items-center gap-2 shrink-0">
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
                {generating ? (
                  <Loader2 className="h-3 w-3 animate-spin" />
                ) : (
                  <RefreshCw className="h-3 w-3" />
                )}
                {generating
                  ? t('conv.outlineGenerating')
                  : hasOutline
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
                {generating ? <Loader2 className="h-3 w-3 animate-spin" /> : null}
                {t('conv.outlineRegen')}
              </button>
            )}
            <button
              type="button"
              onClick={onClose}
              className="text-[11px] theme-text-muted hover:theme-text-main cursor-pointer"
            >
              {t('conv.hideOutline')}
            </button>
          </div>
        </div>

        {statusMessage && (
          <div
            className={`mb-2 flex items-center gap-1.5 text-[11px] px-2.5 py-1.5 rounded-md border ${
              statusTone === 'error'
                ? 'bg-red-500/10 border-red-500/30 text-red-600 dark:text-red-400'
                : statusTone === 'success'
                  ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-700 dark:text-emerald-400'
                  : 'bg-teal-500/10 border-teal-500/30 text-teal-700 dark:text-teal-400'
            }`}
          >
            {statusTone === 'success' ? (
              <CheckCircle2 className="h-3.5 w-3.5 shrink-0" />
            ) : generating || statusTone == null ? (
              <Loader2 className="h-3.5 w-3.5 shrink-0 animate-spin" />
            ) : null}
            <span className="truncate">{statusMessage}</span>
          </div>
        )}

        {generating && !hasOutline ? (
          <div className="flex items-center gap-2 py-4 text-xs theme-text-muted">
            <Loader2 className="h-4 w-4 animate-spin text-teal-500 shrink-0" />
            <span>{t('conv.outlineWorking')}</span>
          </div>
        ) : !hasOutline ? (
          <p className="text-xs theme-text-muted py-2">
            {canGenerate ? t('conv.outlineEmpty') : t('conv.outlineNeedMore')}
          </p>
        ) : (
          <div className={generating ? 'opacity-60 pointer-events-none' : ''}>
            {generating && (
              <div className="mb-2 flex items-center gap-1.5 text-[11px] theme-text-muted">
                <Loader2 className="h-3.5 w-3.5 animate-spin text-teal-500" />
                <span>{primaryIsUpdate ? t('conv.outlineUpdating') : t('conv.outlineWorking')}</span>
              </div>
            )}
            <ol className="space-y-1 max-h-56 overflow-y-auto pr-1">
              {segments.map((seg, i) => (
                <li key={`${seg.start_user_index}-${i}`}>
                  <button
                    type="button"
                    onClick={() => onJump(seg)}
                    disabled={generating}
                    className="w-full text-left px-2.5 py-1.5 rounded-lg hover:bg-teal-500/10 border border-transparent hover:border-teal-500/25 transition-colors cursor-pointer group disabled:cursor-not-allowed"
                  >
                    <div className="flex items-baseline gap-2 min-w-0">
                      <span className="text-[10px] font-mono theme-text-muted shrink-0 w-5">
                        {i + 1}.
                      </span>
                      <span className="text-xs font-medium theme-text-main group-hover:text-teal-600 dark:group-hover:text-teal-400 truncate">
                        {seg.title}
                      </span>
                      <span className="text-[10px] font-mono theme-text-muted shrink-0 ml-auto">
                        #{seg.start_user_index} · step {seg.start_step_index}
                      </span>
                    </div>
                    {seg.summary && (
                      <p className="text-[11px] theme-text-sub leading-snug mt-0.5 pl-7 line-clamp-2">
                        {seg.summary}
                      </p>
                    )}
                  </button>
                </li>
              ))}
            </ol>
          </div>
        )}
      </div>
    </div>
  );
};
