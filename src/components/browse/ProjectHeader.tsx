/**
 * 项目品字型 Header — 项目名 / 目录 / Git 摘要 + 右侧「Git 看板」切换按钮
 */
import React, { useCallback, useEffect, useState } from 'react';
import {
  GitBranch,
  ArrowUp,
  ArrowDown,
  CheckCircle2,
  Loader2,
  RefreshCw,
  Layers,
} from 'lucide-react';
import { api } from '../../api/tauriBridge';
import type { GitBoardEntry } from '../gitboard/GitBoardView';
import { useI18n } from '../../i18n';

function projectDisplayTitle(path: string): string {
  const parts = path.split('/').filter(Boolean);
  if (parts.length >= 2) return `${parts[parts.length - 2]}/${parts[parts.length - 1]}`;
  return parts[parts.length - 1] || path;
}

const badgeCls =
  'inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md text-[10.5px] font-medium border whitespace-nowrap';

interface Props {
  workspacePath: string;
  onOpenGitBoard?: () => void;
  /** 当前是否已在项目 Git 变更视图 */
  gitBoardActive?: boolean;
  /** 批量总结会话（置于 Git 看板按钮左侧） */
  onBatchSummarize?: () => void;
  batchSummarizeDisabled?: boolean;
}

export const ProjectHeader: React.FC<Props> = ({
  workspacePath,
  onOpenGitBoard,
  gitBoardActive = false,
  onBatchSummarize,
  batchSummarizeDisabled = false,
}) => {
  const { t } = useI18n();
  const [entry, setEntry] = useState<GitBoardEntry | null>(null);
  const [loadingGit, setLoadingGit] = useState(false);
  const [reloadKey, setReloadKey] = useState(0);

  const loadGit = useCallback(async (path: string) => {
    setLoadingGit(true);
    try {
      const e = await api.gitBoard.workspaceEntry(path);
      setEntry(e);
    } catch {
      setEntry(null);
    } finally {
      setLoadingGit(false);
    }
  }, []);

  useEffect(() => {
    if (!workspacePath) {
      setEntry(null);
      return;
    }
    void loadGit(workspacePath);
  }, [workspacePath, loadGit, reloadKey]);

  const dirty =
    entry != null
      ? entry.staged + entry.unstaged + entry.untracked + entry.conflicts
      : 0;

  return (
    <div className="px-6 py-4 border-b theme-border flex-shrink-0 theme-bg-main">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0 flex-1">
          <h1 className="text-xl font-bold tracking-tight theme-text-main truncate">
            {projectDisplayTitle(workspacePath)}
          </h1>
          <p
            className="text-xs theme-text-muted font-mono mt-0.5 break-all"
            title={workspacePath}
          >
            {workspacePath}
          </p>

          {/* Git 摘要徽章 */}
          <div className="flex items-center gap-1.5 flex-wrap mt-2 min-h-[22px]">
            {loadingGit && !entry ? (
              <span className="text-[11px] theme-text-sub flex items-center gap-1">
                <Loader2 className="h-3 w-3 animate-spin text-orange-500" />
                {t('common.loading')}
              </span>
            ) : entry && entry.is_repo ? (
              <>
                {entry.detached ? (
                  <span className={`${badgeCls} bg-amber-500/15 text-amber-600 dark:text-amber-400 border-amber-500/30`}>
                    {t('gitBoard.detached')}
                  </span>
                ) : entry.branch ? (
                  <span
                    className={`${badgeCls} bg-blue-500/15 text-blue-600 dark:text-blue-400 border-blue-500/30`}
                    title={t('gitBoard.branch')}
                  >
                    <GitBranch className="h-3 w-3" />
                    {entry.branch}
                  </span>
                ) : null}
                {(entry.ahead ?? 0) > 0 && (
                  <span
                    className={`${badgeCls} bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 border-emerald-500/30`}
                  >
                    <ArrowUp className="h-3 w-3" />
                    {entry.ahead}
                  </span>
                )}
                {(entry.behind ?? 0) > 0 && (
                  <span
                    className={`${badgeCls} bg-sky-500/15 text-sky-600 dark:text-sky-400 border-sky-500/30`}
                  >
                    <ArrowDown className="h-3 w-3" />
                    {entry.behind}
                  </span>
                )}
                {dirty > 0 ? (
                  <span
                    className={`${badgeCls} bg-amber-500/15 text-amber-600 dark:text-amber-400 border-amber-500/30`}
                  >
                    {t('gitBoard.pillDirty', { n: dirty })}
                  </span>
                ) : (
                  <span
                    className={`${badgeCls} bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 border-emerald-500/30`}
                  >
                    <CheckCircle2 className="h-3 w-3" />
                    {t('gitBoard.clean')}
                  </span>
                )}
                {entry.last_commit_time && (
                  <span className="text-[10.5px] theme-text-sub truncate max-w-[280px]">
                    {entry.last_commit_short_id ? `${entry.last_commit_short_id} · ` : ''}
                    {entry.last_commit_message || t('gitBoard.lastCommit')}
                  </span>
                )}
              </>
            ) : entry && !entry.is_repo ? (
              <span className="text-[11px] theme-text-sub">
                {t('gitBoard.empty', { d: 30 })}
              </span>
            ) : null}

            <button
              type="button"
              onClick={() => setReloadKey((k) => k + 1)}
              title={t('gitBoard.refresh')}
              className="p-1 rounded-md theme-text-sub hover:theme-text-main hover:theme-bg-card transition-colors cursor-pointer"
            >
              <RefreshCw className={`h-3 w-3 ${loadingGit ? 'animate-spin text-orange-500' : ''}`} />
            </button>
          </div>
        </div>

        {/* 右侧：Git 操作按钮插槽 + 统计/Git 切换 */}
        <div className="flex items-center gap-2 flex-shrink-0 mt-0.5">
          {gitBoardActive && (
            <div id="project-git-actions" className="flex items-center gap-2" />
          )}
          {onBatchSummarize && (
            <button
              type="button"
              onClick={onBatchSummarize}
              disabled={batchSummarizeDisabled}
              title={t('conv.batchSummarizeHint')}
              className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium border theme-border theme-bg-sub theme-text-muted rounded-lg transition-colors cursor-pointer shadow-xs hover:text-violet-600 dark:hover:text-violet-400 hover:border-violet-500/40 hover:bg-violet-500/10 disabled:opacity-40 disabled:hover:text-inherit disabled:hover:bg-transparent"
            >
              <Layers className="h-3.5 w-3.5 text-violet-500" />
              <span>{t('conv.batchSummarize')}</span>
            </button>
          )}
          {onOpenGitBoard && (
            <button
              type="button"
              onClick={onOpenGitBoard}
              title={t('gitBoard.openForProject')}
              className={`flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium border rounded-lg transition-colors cursor-pointer shadow-xs ${
                gitBoardActive
                  ? 'bg-orange-500 text-white border-orange-500'
                  : 'theme-bg-sub theme-text-muted theme-border hover:text-orange-600 dark:hover:text-orange-400 hover:border-orange-500/40 hover:bg-orange-500/10'
              }`}
            >
              <GitBranch className={`h-3.5 w-3.5 ${gitBoardActive ? 'text-white' : 'text-orange-500'}`} />
              <span>{gitBoardActive ? t('gitBoard.exit') : t('nav.gitBoard')}</span>
            </button>
          )}
        </div>
      </div>
    </div>
  );
};
