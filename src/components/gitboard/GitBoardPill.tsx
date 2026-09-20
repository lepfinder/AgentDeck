/**
 * 顶栏 Git 状态胶囊 — 最近活跃工作区 Git 状态一瞥
 *
 * 数据复用后端 git_board 模块（只读采集），点击进入 / 退出 Git 看板视图。
 */
import { useEffect, useState } from 'react';
import { GitBranch } from 'lucide-react';
import { api, isTauri } from '../../api/tauriBridge';
import { useI18n } from '../../i18n';
import { getHiddenPaths, onHiddenChange } from '../../lib/gitBoardHidden';
import type { GitBoardResponse } from './GitBoardView';

/** 胶囊轮询节奏：本地 git status 扫描很轻，2 分钟一拍足够。 */
const POLL_MS = 2 * 60_000;

export function GitBoardPill({ active, onOpen }: { active: boolean; onOpen: () => void }) {
  const { t } = useI18n();
  const [data, setData] = useState<GitBoardResponse | null>(null);
  const [hiddenPaths, setHiddenPathsState] = useState<string[]>(() => getHiddenPaths());

  useEffect(() => onHiddenChange(() => setHiddenPathsState(getHiddenPaths())), []);

  useEffect(() => {
    if (!isTauri()) return;
    let cancelled = false;
    const load = async () => {
      if (typeof document !== 'undefined' && document.visibilityState === 'hidden') return;
      try {
        const res = await api.gitBoard.get();
        if (!cancelled) setData(res);
      } catch {
        /* 静默失败：胶囊不打扰用户 */
      }
    };
    void load();
    const timer = setInterval(load, POLL_MS);
    const onVisibility = () => {
      if (document.visibilityState === 'visible') void load();
    };
    document.addEventListener('visibilitychange', onVisibility);
    return () => {
      cancelled = true;
      clearInterval(timer);
      document.removeEventListener('visibilitychange', onVisibility);
    };
  }, []);

  if (!isTauri()) return null;

  // 隐藏的仓库不计入胶囊统计
  const entries = (data?.entries ?? []).filter(
    (e) => !hiddenPaths.includes(e.workspace_path)
  );
  const conflicts = entries.reduce((acc, e) => acc + (e.conflicts || 0), 0);
  const dirty = entries.filter((e) => e.staged + e.unstaged + e.untracked + e.conflicts > 0).length;
  const ahead = entries.filter((e) => (e.ahead ?? 0) > 0).length;
  const hasRepos = entries.length > 0;
  const allClean = hasRepos && dirty === 0 && ahead === 0 && conflicts === 0;
  const showPlaceholder = !data || (!hasRepos && dirty === 0 && ahead === 0);

  return (
    <button
      type="button"
      onClick={onOpen}
      title={t('gitBoard.pillTitle')}
      className={`flex items-center gap-1.5 px-2 py-1 text-[11px] border rounded-lg transition-colors cursor-pointer shadow-sm ${
        active
          ? 'border-black/40 dark:border-white/40 theme-bg-sub theme-text-main'
          : 'theme-bg-sub theme-border theme-text-muted hover:theme-text-main hover:opacity-90'
      }`}
    >
      <GitBranch className="h-3.5 w-3.5 text-orange-500 shrink-0" />
      {conflicts > 0 && (
        <span className="font-medium text-red-500 whitespace-nowrap tabular-nums">
          {t('gitBoard.pillConflicts', { n: conflicts })}
        </span>
      )}
      {dirty > 0 && (
        <span className="whitespace-nowrap tabular-nums">
          {t('gitBoard.pillDirtyRepos', { n: dirty })}
        </span>
      )}
      {ahead > 0 && (
        <span className="whitespace-nowrap tabular-nums">{t('gitBoard.pillAhead', { n: ahead })}</span>
      )}
      {allClean && <span>{t('gitBoard.pillClean')}</span>}
      {showPlaceholder && <span>Git</span>}
    </button>
  );
}
