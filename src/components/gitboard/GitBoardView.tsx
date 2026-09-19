/**
 * Git 看板 — 最近 N 天有会话活动的工作区 Git 状态总览
 *
 * 数据由后端 git_board 模块只读采集（分支 / ahead-behind / 变更计数 / 最近提交），
 * 本视图纯展示，不做任何 git 写操作。
 */
import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  GitBranch,
  GitCommit,
  RefreshCw,
  Loader2,
  AlertCircle,
  ArrowUp,
  ArrowDown,
  CheckCircle2,
  FolderGit2,
  Activity,
  Send,
  FileDiff,
  X,
  ChevronUp,
  ChevronDown,
  Sparkles,
} from 'lucide-react';
import { api } from '../../api/tauriBridge';
import { useI18n } from '../../i18n';
import {
  generateCommitMessage,
  getCommitMsgLang,
  saveCommitMsgLang,
  type CommitMsgLang,
} from '../../lib/commitMessageAi';
import { formatBeijingTime, formatRelativeTime } from '../../utils/date';

export interface GitBoardEntry {
  workspace_path: string;
  display_name: string;
  is_repo: boolean;
  branch: string | null;
  detached: boolean;
  ahead: number | null;
  behind: number | null;
  staged: number;
  unstaged: number;
  untracked: number;
  conflicts: number;
  last_commit_message: string | null;
  last_commit_time: string | null;
  last_commit_short_id: string | null;
  last_activity: string | null;
  conversation_count: number;
}

export interface GitBoardSummary {
  days: number;
  total: number;
  repo_count: number;
  dirty_count: number;
  ahead_count: number;
  generated_at: string;
}

export interface GitBoardResponse {
  entries: GitBoardEntry[];
  summary: GitBoardSummary;
}

export interface GitCommitInfo {
  message: string;
  short_id: string;
  author: string | null;
  time: string;
}

export interface GitStatusFile {
  path: string;
  old_path: string | null;
  staged: string | null;
  worktree: string | null;
}

export interface GitStatusFilesResponse {
  files: GitStatusFile[];
  total: number;
}

export interface GitFileDiffResponse {
  diff: string;
  binary: boolean;
  truncated: boolean;
}

export interface GitPendingDiffResponse {
  files: string[];
  diff: string;
  truncated: boolean;
}

const DAY_OPTIONS = [7, 30, 90];

const badgeBase =
  'inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md text-[10.5px] font-medium border whitespace-nowrap';
const actionBtn =
  'flex items-center gap-1.5 px-2.5 py-1.5 text-xs theme-bg-sub border theme-border rounded-lg theme-text-muted hover:theme-text-main transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-default';
const badgeTones = {
  blue: 'bg-blue-500/10 text-blue-500 border-blue-500/25',
  emerald: 'bg-emerald-500/10 text-emerald-500 border-emerald-500/25',
  amber: 'bg-amber-500/10 text-amber-500 border-amber-500/25',
  red: 'bg-red-500/10 text-red-500 border-red-500/25',
  zinc: 'bg-zinc-500/10 text-zinc-400 border-zinc-500/25',
  sky: 'bg-sky-500/10 text-sky-500 border-sky-500/25',
} as const;

function Badge({
  tone,
  children,
  title,
}: {
  tone: keyof typeof badgeTones;
  children: React.ReactNode;
  title?: string;
}) {
  return (
    <span className={`${badgeBase} ${badgeTones[tone]}`} title={title}>
      {children}
    </span>
  );
}

function SummaryCard({
  icon,
  label,
  value,
  toneClass,
  onClick,
}: {
  icon: React.ReactNode;
  label: string;
  value: number;
  toneClass: string;
  onClick?: () => void;
}) {
  return (
    <div
      onClick={onClick}
      className={`rounded-xl border theme-border theme-bg-card px-4 py-3 flex items-center gap-3 transition-colors ${
        onClick ? 'cursor-pointer hover:theme-bg-sub' : ''
      }`}
    >
      <div className={`p-2 rounded-lg ${toneClass}`}>{icon}</div>
      <div className="min-w-0">
        <div className="text-xl font-bold theme-text-main leading-tight">{value}</div>
        <div className="text-[11px] theme-text-muted truncate">{label}</div>
      </div>
    </div>
  );
}

function GitBoardListItem({
  entry,
  active,
  divider,
  onSelect,
}: {
  entry: GitBoardEntry;
  active: boolean;
  divider: boolean;
  onSelect: () => void;
}) {
  const { t } = useI18n();
  const name =
    entry.display_name ||
    entry.workspace_path.split('/').slice(-1)[0] ||
    entry.workspace_path;
  const dirtyCount = entry.staged + entry.unstaged + entry.untracked + entry.conflicts;

  return (
    <button
      type="button"
      onClick={onSelect}
      className={`w-full text-left px-3 py-2.5 transition-colors cursor-pointer ${
        divider ? 'border-t theme-border-sub ' : ''
      }${
        active
          ? 'bg-black/[0.05] dark:bg-white/[0.07]'
          : 'hover:bg-black/[0.03] dark:hover:bg-white/[0.04]'
      }`}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="text-sm font-semibold truncate theme-text-main">{name}</span>
        <span className="text-[10px] theme-text-sub flex-shrink-0">
          {formatRelativeTime(entry.last_commit_time ?? entry.last_activity ?? undefined)}
        </span>
      </div>
      <div className="flex items-center gap-1 mt-1.5 flex-wrap">
        {entry.detached ? (
          <Badge tone="amber">{t('gitBoard.detached')}</Badge>
        ) : entry.branch ? (
          <Badge tone="blue" title={t('gitBoard.branch')}>
            <GitBranch className="h-3 w-3" />
            {entry.branch}
          </Badge>
        ) : null}
        {entry.ahead != null && entry.ahead > 0 && (
          <Badge tone="emerald" title={t('gitBoard.ahead', { n: entry.ahead })}>
            <ArrowUp className="h-3 w-3" />
            {entry.ahead}
          </Badge>
        )}
        {entry.behind != null && entry.behind > 0 && (
          <Badge tone="sky" title={t('gitBoard.behind', { n: entry.behind })}>
            <ArrowDown className="h-3 w-3" />
            {entry.behind}
          </Badge>
        )}
        {dirtyCount > 0 ? (
          <Badge tone="amber" title={t('gitBoard.pillDirty', { n: dirtyCount })}>
            {t('gitBoard.pillDirty', { n: dirtyCount })}
          </Badge>
        ) : (
          <Badge tone="emerald">
            <CheckCircle2 className="h-3 w-3" />
            {t('gitBoard.clean')}
          </Badge>
        )}
      </div>
    </button>
  );
}

/** 文件暂存/工作区状态字母（如 "A " / " M" / "??" / "UU"），颜色区分状态 */
function FileStatusGlyph({ f }: { f: GitStatusFile }) {
  const { t } = useI18n();
  const wt = f.worktree;
  const cls =
    wt === 'U'
      ? 'text-red-500'
      : wt === '?'
        ? 'text-zinc-400'
        : wt
          ? 'text-amber-500'
          : 'text-blue-500';
  const tip =
    wt === 'U'
      ? t('gitBoard.fileConflict')
      : wt === '?'
        ? t('gitBoard.fileUntracked')
        : wt
          ? t('gitBoard.fileUnstaged')
          : t('gitBoard.fileStaged');
  return (
    <span
      className={`font-mono text-[10px] font-bold flex-shrink-0 w-5 text-center ${cls}`}
      title={tip}
    >
      {f.staged ?? ' '}
      {wt ?? ' '}
    </span>
  );
}

function GitBoardDetail({ entry, onChanged }: { entry: GitBoardEntry; onChanged: () => void }) {
  const { t } = useI18n();
  const [commits, setCommits] = useState<GitCommitInfo[] | null>(null);
  const [commitsLoading, setCommitsLoading] = useState(false);
  const [commitsError, setCommitsError] = useState<string | null>(null);
  const [statusFiles, setStatusFiles] = useState<GitStatusFilesResponse | null>(null);
  const [filesLoading, setFilesLoading] = useState(false);
  const [filesError, setFilesError] = useState<string | null>(null);
  const [diffIndex, setDiffIndex] = useState<number | null>(null);
  const [commitMsg, setCommitMsg] = useState('');
  const [aiLoading, setAiLoading] = useState(false);
  const [committing, setCommitting] = useState(false);
  const [commitError, setCommitError] = useState<string | null>(null);
  const [commitDone, setCommitDone] = useState(false);
  const [pushing, setPushing] = useState(false);
  const [pushError, setPushError] = useState<string | null>(null);
  const [pushDone, setPushDone] = useState(false);
  const [pushConfirm, setPushConfirm] = useState(false);
  const [lang, setLang] = useState<CommitMsgLang>(() => getCommitMsgLang());

  const dirtyCount = entry.staged + entry.unstaged + entry.untracked + entry.conflicts;

  // 切换项目时重置提交/推送操作区状态
  useEffect(() => {
    setCommitMsg('');
    setCommitError(null);
    setCommitDone(false);
    setPushError(null);
    setPushDone(false);
    setPushConfirm(false);
  }, [entry.workspace_path]);

  useEffect(() => {
    let cancelled = false;
    setCommits(null);
    setCommitsError(null);
    setCommitsLoading(true);
    api.gitBoard
      .commits(entry.workspace_path, 50)
      .then((res) => {
        if (!cancelled) setCommits(res);
      })
      .catch((e) => {
        if (!cancelled) setCommitsError(String(e));
      })
      .finally(() => {
        if (!cancelled) setCommitsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [entry.workspace_path]);

  // 未提交文件列表：切换项目或变更计数变化（刷新）时重新加载
  useEffect(() => {
    let cancelled = false;
    setStatusFiles(null);
    setFilesError(null);
    setFilesLoading(true);
    api.gitBoard
      .statusFiles(entry.workspace_path, 200)
      .then((res) => {
        if (!cancelled) setStatusFiles(res);
      })
      .catch((e) => {
        if (!cancelled) setFilesError(String(e));
      })
      .finally(() => {
        if (!cancelled) setFilesLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [entry.workspace_path, dirtyCount]);

  const name =
    entry.display_name ||
    entry.workspace_path.split('/').slice(-1)[0] ||
    entry.workspace_path;

  const changeCells = [
    { label: t('gitBoard.statStaged'), value: entry.staged, cls: 'text-blue-500' },
    { label: t('gitBoard.statUnstaged'), value: entry.unstaged, cls: 'text-amber-500' },
    { label: t('gitBoard.statUntracked'), value: entry.untracked, cls: 'text-zinc-400' },
    { label: t('gitBoard.statConflicts'), value: entry.conflicts, cls: 'text-red-500' },
  ];

  const handleAiGenerate = useCallback(async () => {
    if (aiLoading) return;
    setAiLoading(true);
    setCommitError(null);
    try {
      const msg = await generateCommitMessage(entry, lang);
      setCommitMsg(msg);
    } catch (e) {
      setCommitError(String(e));
    } finally {
      setAiLoading(false);
    }
  }, [aiLoading, entry, lang]);

  const handleCommit = useCallback(async () => {
    const msg = commitMsg.trim();
    if (!msg || committing || aiLoading) return;
    setCommitting(true);
    setCommitError(null);
    setCommitDone(false);
    try {
      await api.gitBoard.stageAll(entry.workspace_path);
      await api.gitBoard.commit(entry.workspace_path, msg);
      setCommitMsg('');
      setCommitDone(true);
      onChanged();
    } catch (e) {
      setCommitError(String(e));
    } finally {
      setCommitting(false);
    }
  }, [commitMsg, committing, aiLoading, entry.workspace_path, onChanged]);

  const handlePush = useCallback(async () => {
    if (pushing) return;
    setPushing(true);
    setPushError(null);
    setPushDone(false);
    try {
      await api.gitBoard.push(entry.workspace_path);
      setPushConfirm(false);
      setPushDone(true);
      onChanged();
    } catch (e) {
      setPushError(String(e));
      setPushConfirm(false);
    } finally {
      setPushing(false);
    }
  }, [pushing, entry.workspace_path, onChanged]);

  return (
    <div className="h-full rounded-xl border theme-border theme-bg-card flex flex-col overflow-hidden">
      {/* 头部：项目名、路径与状态徽章 */}
      <div className="px-4 py-3 border-b theme-border flex-shrink-0">
        <div className="flex items-center gap-1.5 flex-wrap">
          <span className="text-sm font-bold theme-text-main">{name}</span>
          {entry.detached ? (
            <Badge tone="amber">{t('gitBoard.detached')}</Badge>
          ) : entry.branch ? (
            <Badge tone="blue" title={t('gitBoard.branch')}>
              <GitBranch className="h-3 w-3" />
              {entry.branch}
            </Badge>
          ) : null}
          {entry.ahead != null && entry.ahead > 0 && (
            <Badge tone="emerald" title={t('gitBoard.ahead', { n: entry.ahead })}>
              <ArrowUp className="h-3 w-3" />
              {entry.ahead}
            </Badge>
          )}
          {entry.behind != null && entry.behind > 0 && (
            <Badge tone="sky" title={t('gitBoard.behind', { n: entry.behind })}>
              <ArrowDown className="h-3 w-3" />
              {entry.behind}
            </Badge>
          )}
          {dirtyCount > 0 ? (
            <Badge tone="amber">{t('gitBoard.pillDirty', { n: dirtyCount })}</Badge>
          ) : (
            <Badge tone="emerald">
              <CheckCircle2 className="h-3 w-3" />
              {t('gitBoard.clean')}
            </Badge>
          )}
        </div>
        <div
          className="text-[11px] theme-text-sub truncate font-mono mt-1"
          title={entry.workspace_path}
        >
          {entry.workspace_path}
        </div>
        <div className="text-[10px] theme-text-sub mt-1 flex items-center gap-2 flex-wrap">
          {entry.conversation_count > 0 && (
            <span>{t('gitBoard.sessions', { n: entry.conversation_count })}</span>
          )}
          {entry.last_activity && (
            <span>
              · {t('gitBoard.lastActivity')} {formatRelativeTime(entry.last_activity)}
            </span>
          )}
          {entry.last_commit_time && (
            <span>
              · {t('gitBoard.lastCommit')} {formatRelativeTime(entry.last_commit_time)}
            </span>
          )}
        </div>
      </div>

      {/* 变更统计 */}
      <div className="grid grid-cols-4 gap-2 px-4 py-3 border-b theme-border flex-shrink-0">
        {changeCells.map((c) => (
          <div key={c.label} className="rounded-lg theme-bg-sub px-2 py-1.5 text-center">
            <div className={`text-sm font-bold ${c.cls}`}>{c.value}</div>
            <div className="text-[10px] theme-text-muted truncate">{c.label}</div>
          </div>
        ))}
      </div>

      {/* 提交 / 推送操作区 */}
      {(dirtyCount > 0 || (entry.ahead ?? 0) > 0) && (
        <div className="px-4 py-3 border-b theme-border flex-shrink-0 flex flex-col gap-2">
          {dirtyCount > 0 && (
            <>
              <textarea
                value={commitMsg}
                onChange={(e) => setCommitMsg(e.target.value)}
                rows={2}
                placeholder={t('gitBoard.commitPlaceholder')}
                disabled={aiLoading || committing}
                className="w-full text-xs theme-bg-sub theme-text-main rounded-lg border theme-border px-2.5 py-2 resize-none outline-none focus:border-black/30 dark:focus:border-white/30 placeholder:theme-text-sub disabled:opacity-60"
              />
              <div className="flex items-center gap-2 min-w-0">
                <div
                  className="flex items-center rounded-lg border theme-border theme-bg-sub overflow-hidden flex-shrink-0"
                  title={t('gitBoard.commitLangTitle')}
                >
                  {(['auto', 'zh', 'en'] as CommitMsgLang[]).map((l) => (
                    <button
                      key={l}
                      onClick={() => {
                        setLang(l);
                        saveCommitMsgLang(l);
                      }}
                      className={`px-2 py-1 text-[11px] transition-colors cursor-pointer ${
                        lang === l
                          ? 'bg-black/[0.06] dark:bg-white/[0.08] theme-text-main font-medium'
                          : 'theme-text-muted hover:theme-text-main'
                      }`}
                    >
                      {l === 'auto'
                        ? t('gitBoard.langAuto')
                        : l === 'zh'
                          ? t('gitBoard.langZh')
                          : t('gitBoard.langEn')}
                    </button>
                  ))}
                </div>
                <button
                  onClick={() => void handleAiGenerate()}
                  disabled={aiLoading || committing}
                  className={actionBtn}
                >
                  {aiLoading ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin text-orange-500" />
                  ) : (
                    <Sparkles className="h-3.5 w-3.5 text-orange-500" />
                  )}
                  <span>{aiLoading ? t('gitBoard.aiGenerating') : t('gitBoard.aiGenerate')}</span>
                </button>
                <button
                  onClick={() => void handleCommit()}
                  disabled={committing || aiLoading || !commitMsg.trim()}
                  className="flex items-center gap-1.5 px-2.5 py-1.5 text-xs bg-orange-500 hover:bg-orange-600 text-white border border-transparent rounded-lg transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-default flex-shrink-0"
                >
                  {committing ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <GitCommit className="h-3.5 w-3.5" />
                  )}
                  <span>
                    {committing ? t('gitBoard.committing') : t('gitBoard.commitAll')}
                  </span>
                </button>
                {commitError && (
                  <span className="text-[10px] text-red-400 truncate" title={commitError}>
                    {commitError}
                  </span>
                )}
                {commitDone && !commitError && (
                  <span className="text-[10px] text-emerald-500 flex-shrink-0">
                    {t('gitBoard.commitDone')}
                  </span>
                )}
              </div>
            </>
          )}
          {(entry.ahead ?? 0) > 0 && (
            <div className="flex items-center gap-2 min-w-0">
              {pushConfirm ? (
                <>
                  <span className="text-[11px] theme-text-muted flex-shrink-0">
                    {t('gitBoard.pushConfirm')}
                  </span>
                  <button
                    onClick={() => void handlePush()}
                    disabled={pushing}
                    className={actionBtn}
                  >
                    {pushing && (
                      <Loader2 className="h-3.5 w-3.5 animate-spin text-orange-500" />
                    )}
                    <span>{pushing ? t('gitBoard.pushing') : t('gitBoard.confirm')}</span>
                  </button>
                  <button
                    onClick={() => setPushConfirm(false)}
                    disabled={pushing}
                    className={actionBtn}
                  >
                    <span>{t('gitBoard.cancel')}</span>
                  </button>
                </>
              ) : (
                <button
                  onClick={() => setPushConfirm(true)}
                  disabled={pushing || (entry.behind ?? 0) > 0}
                  title={(entry.behind ?? 0) > 0 ? t('gitBoard.pushBlocked') : undefined}
                  className={actionBtn}
                >
                  <Send className="h-3.5 w-3.5 text-emerald-500" />
                  <span>{t('gitBoard.pushN', { n: entry.ahead ?? 0 })}</span>
                </button>
              )}
              {pushError && (
                <span className="text-[10px] text-red-400 truncate" title={pushError}>
                  {pushError}
                </span>
              )}
              {pushDone && !pushError && (
                <span className="text-[10px] text-emerald-500 flex-shrink-0">
                  {t('gitBoard.pushDone')}
                </span>
              )}
            </div>
          )}
        </div>
      )}

      {/* 未提交文件列表 */}
      {dirtyCount > 0 && (
        <>
          <div className="px-4 pt-3 pb-1 text-xs font-semibold theme-text-muted flex-shrink-0 flex items-center justify-between">
            <span>{t('gitBoard.filesTitle')}</span>
            <span className="text-[10px] font-normal theme-text-sub">{dirtyCount}</span>
          </div>
          <div className="flex-1 overflow-y-auto px-4 pb-2 min-h-0">
            {filesLoading ? (
              <div className="flex items-center justify-center py-6 gap-2">
                <Loader2 className="h-4 w-4 animate-spin text-orange-500" />
                <span className="text-xs theme-text-muted">{t('gitBoard.filesLoading')}</span>
              </div>
            ) : filesError ? (
              <div className="flex flex-col items-center justify-center py-6 gap-1 text-center">
                <AlertCircle className="h-5 w-5 text-red-400" />
                <div className="text-xs theme-text-muted">{t('gitBoard.filesError')}</div>
              </div>
            ) : statusFiles && statusFiles.files.length > 0 ? (
              <div>
                {statusFiles.files.map((f, i) => (
                  <div
                    key={`${f.path}-${f.staged ?? ''}-${f.worktree ?? ''}`}
                    onClick={() => setDiffIndex(i)}
                    className="flex items-center gap-2 py-1 border-b theme-border-sub last:border-b-0 cursor-pointer hover:bg-black/5 dark:hover:bg-white/5 transition-colors"
                  >
                    <FileStatusGlyph f={f} />
                    <span
                      className="text-xs theme-text-muted truncate font-mono"
                      title={f.old_path ? `${f.old_path} → ${f.path}` : f.path}
                    >
                      {f.path}
                    </span>
                  </div>
                ))}
                {statusFiles.total > statusFiles.files.length && (
                  <div className="text-[10px] theme-text-sub text-center py-1.5">
                    {t('gitBoard.filesMore', { n: statusFiles.total - statusFiles.files.length })}
                  </div>
                )}
              </div>
            ) : (
              <div className="flex items-center justify-center py-6 text-xs theme-text-sub">
                {t('gitBoard.filesEmpty')}
              </div>
            )}
          </div>
        </>
      )}

      {/* 最近提交记录 */}
      <div className="px-4 pt-3 pb-1 text-xs font-semibold theme-text-muted flex-shrink-0">
        {t('gitBoard.commitsTitle')}
      </div>
      <div className="flex-1 overflow-y-auto px-4 pb-4 min-h-0">
        {commitsLoading ? (
          <div className="flex items-center justify-center py-10 gap-2">
            <Loader2 className="h-4 w-4 animate-spin text-orange-500" />
            <span className="text-xs theme-text-muted">{t('gitBoard.commitsLoading')}</span>
          </div>
        ) : commitsError ? (
          <div className="flex flex-col items-center justify-center py-10 gap-1 text-center">
            <AlertCircle className="h-5 w-5 text-red-400" />
            <div className="text-xs theme-text-muted">{t('gitBoard.commitsError')}</div>
            <div className="text-[10px] theme-text-sub font-mono max-w-md truncate">
              {commitsError}
            </div>
          </div>
        ) : commits && commits.length > 0 ? (
          <div>
            {commits.map((c) => (
              <div
                key={`${c.short_id}-${c.time}`}
                className="flex items-start gap-2.5 py-2 border-b theme-border-sub last:border-b-0"
              >
                <span className="font-mono text-xs text-orange-500 flex-shrink-0 mt-0.5">
                  {c.short_id}
                </span>
                <div className="min-w-0 flex-1">
                  <div className="text-xs theme-text-main truncate" title={c.message}>
                    {c.message}
                  </div>
                  <div className="text-[10px] theme-text-sub mt-0.5 truncate">
                    {c.author && <span>{c.author} · </span>}
                    {formatBeijingTime(c.time)}
                  </div>
                </div>
              </div>
            ))}
          </div>
        ) : (
          <div className="flex items-center justify-center py-10 text-xs theme-text-sub">
            {t('gitBoard.commitsEmpty')}
          </div>
        )}
      </div>

      {/* 文件 diff 审查器 */}
      {diffIndex != null && statusFiles && statusFiles.files.length > 0 && (
        <GitDiffViewer
          workspacePath={entry.workspace_path}
          files={statusFiles.files}
          initialIndex={Math.min(diffIndex, statusFiles.files.length - 1)}
          onClose={() => setDiffIndex(null)}
        />
      )}
    </div>
  );
}

type BoardFilter = 'all' | 'dirty' | 'ahead';

export function GitBoardView() {
  const { t } = useI18n();
  const [days, setDays] = useState(30);
  const [data, setData] = useState<GitBoardResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [filter, setFilter] = useState<BoardFilter>('all');

  const filteredEntries = useMemo(() => {
    if (!data) return [];
    if (filter === 'dirty') {
      return data.entries.filter(
        (e) => e.staged + e.unstaged + e.untracked + e.conflicts > 0
      );
    }
    if (filter === 'ahead') {
      return data.entries.filter((e) => (e.ahead ?? 0) > 0);
    }
    return data.entries;
  }, [data, filter]);

  // 从筛选后的列表中选：优先保持已选项，否则回退到第一项
  const selected = useMemo(
    () =>
      filteredEntries.find((e) => e.workspace_path === selectedPath) ??
      filteredEntries[0] ??
      null,
    [filteredEntries, selectedPath]
  );

  const toggleFilter = useCallback(
    (f: Exclude<BoardFilter, 'all'>) => setFilter((prev) => (prev === f ? 'all' : f)),
    []
  );

  const load = useCallback(async (d: number) => {
    setLoading(true);
    setError(null);
    try {
      const res = await api.gitBoard.get(d);
      setData(res);
      // 默认选中第一个；已选中的项目在新列表中消失时回退到第一个
      setSelectedPath(
        (prev) =>
          res.entries.some((e) => e.workspace_path === prev)
            ? prev
            : (res.entries[0]?.workspace_path ?? null)
      );
    } catch (e) {
      console.error('Failed to load git board:', e);
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load(days);
  }, [days, load]);

  const summary = data?.summary;

  return (
    <div className="flex-1 flex flex-col h-full overflow-hidden theme-bg-main">
      {/* 工具栏 */}
      <div className="px-6 py-4 border-b theme-border flex items-center justify-between gap-4 flex-shrink-0">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <GitBranch className="h-5 w-5 text-orange-500" />
            <h1 className="text-lg font-bold theme-text-main">{t('gitBoard.title')}</h1>
          </div>
          <p className="text-xs theme-text-muted mt-1 truncate">
            {summary
              ? t('gitBoard.subtitle', {
                  d: summary.days,
                  n: summary.total,
                }) +
                (summary.generated_at
                  ? ` · ${t('gitBoard.generatedAt', { time: formatBeijingTime(summary.generated_at) })}`
                  : '')
              : t('gitBoard.subtitleLoading')}
          </p>
        </div>
        <div className="flex items-center gap-2 flex-shrink-0">
          <div className="flex items-center rounded-lg border theme-border theme-bg-sub overflow-hidden">
            {DAY_OPTIONS.map((d) => (
              <button
                key={d}
                onClick={() => setDays(d)}
                className={`px-2.5 py-1 text-xs font-medium transition-colors cursor-pointer ${
                  days === d
                    ? 'bg-orange-500/15 text-orange-500'
                    : 'theme-text-muted hover:theme-text-main'
                }`}
              >
                {t('gitBoard.days', { d })}
              </button>
            ))}
          </div>
          <button
            onClick={() => void load(days)}
            disabled={loading}
            className="flex items-center gap-1.5 px-2.5 py-1.5 text-xs theme-bg-sub hover:opacity-90 border theme-border rounded-lg theme-text-muted hover:theme-text-main transition-colors cursor-pointer shadow-sm disabled:opacity-50"
          >
            {loading ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin text-orange-500" />
            ) : (
              <RefreshCw className="h-3.5 w-3.5 text-orange-500" />
            )}
            <span>{t('gitBoard.refresh')}</span>
          </button>
        </div>
      </div>

      {/* 汇总卡片（点击后三项可筛选左侧列表） */}
      {summary && (
        <div className="px-6 py-4 grid grid-cols-2 md:grid-cols-4 gap-3 flex-shrink-0">
          <SummaryCard
            icon={<Activity className="h-4 w-4 text-blue-500" />}
            label={t('gitBoard.summaryActive')}
            value={summary.total}
            toneClass="bg-blue-500/10"
            onClick={() => setFilter('all')}
          />
          <SummaryCard
            icon={<FolderGit2 className="h-4 w-4 text-orange-500" />}
            label={t('gitBoard.summaryRepos')}
            value={summary.repo_count}
            toneClass="bg-orange-500/10"
          />
          <SummaryCard
            icon={<GitCommit className="h-4 w-4 text-amber-500" />}
            label={t('gitBoard.summaryDirty')}
            value={summary.dirty_count}
            toneClass="bg-amber-500/10"
            onClick={() => toggleFilter('dirty')}
          />
          <SummaryCard
            icon={<Send className="h-4 w-4 text-emerald-500" />}
            label={t('gitBoard.summaryAhead')}
            value={summary.ahead_count}
            toneClass="bg-emerald-500/10"
            onClick={() => toggleFilter('ahead')}
          />
        </div>
      )}

      {/* 主体：左侧项目列表 + 右侧详情 */}
      <div className="flex-1 flex overflow-hidden px-6 pb-6 min-h-0">
        {error ? (
          <div className="flex-1 flex flex-col items-center justify-center py-16 gap-2 text-center">
            <AlertCircle className="h-8 w-8 text-red-400" />
            <div className="text-sm theme-text-muted">{t('gitBoard.error')}</div>
            <div className="text-xs theme-text-sub font-mono max-w-md truncate">{error}</div>
          </div>
        ) : loading && !data ? (
          <div className="flex-1 flex flex-col items-center justify-center py-16 gap-2">
            <Loader2 className="h-6 w-6 animate-spin text-orange-500" />
            <div className="text-xs theme-text-muted">{t('common.loading')}</div>
          </div>
        ) : data && data.entries.length === 0 ? (
          <div className="flex-1 flex flex-col items-center justify-center py-16 gap-2 text-center">
            <GitBranch className="h-8 w-8 theme-text-sub opacity-50" />
            <div className="text-sm theme-text-muted">
              {t('gitBoard.empty', { d: summary?.days ?? days })}
            </div>
            <div className="text-xs theme-text-sub">{t('gitBoard.emptyHint')}</div>
          </div>
        ) : (
          <>
            {/* 左：项目列表 */}
            <div className="w-56 lg:w-60 flex-shrink-0 flex flex-col min-h-0 pr-1">
              {filter !== 'all' && (
                <div className="flex items-center gap-2 mb-2 px-0.5 flex-shrink-0">
                  <span className="text-[11px] theme-text-muted truncate">
                    {filter === 'dirty'
                      ? t('gitBoard.summaryDirty')
                      : t('gitBoard.summaryAhead')}
                  </span>
                  <button
                    onClick={() => setFilter('all')}
                    className="ml-auto flex items-center gap-1 px-2 py-0.5 rounded-md text-[10px] theme-bg-sub border theme-border theme-text-muted hover:theme-text-main transition-colors cursor-pointer flex-shrink-0"
                  >
                    <X className="h-3 w-3" />
                    {t('gitBoard.filterClear')}
                  </button>
                </div>
              )}
              {filteredEntries.length === 0 ? (
                <div className="flex-1 flex flex-col items-center justify-center gap-2 text-center rounded-xl border theme-border theme-bg-card py-10 px-4">
                  <GitBranch className="h-6 w-6 theme-text-sub opacity-50" />
                  <div className="text-xs theme-text-muted">
                    {filter === 'dirty'
                      ? t('gitBoard.filterDirtyEmpty')
                      : t('gitBoard.filterAheadEmpty')}
                  </div>
                </div>
              ) : (
                <div className="flex-1 overflow-y-auto min-h-0 rounded-xl border theme-border theme-bg-card">
                  {filteredEntries.map((entry, i) => (
                    <GitBoardListItem
                      key={entry.workspace_path}
                      entry={entry}
                      active={entry.workspace_path === selected?.workspace_path}
                      divider={i > 0}
                      onSelect={() => setSelectedPath(entry.workspace_path)}
                    />
                  ))}
                </div>
              )}
            </div>

            {/* 右：详情面板 */}
            <div className="flex-1 min-w-0 pl-1">
              {selected ? (
                <GitBoardDetail entry={selected} onChanged={() => void load(days)} />
              ) : (
                <div className="h-full rounded-xl border theme-border theme-bg-card flex flex-col items-center justify-center gap-2 text-center">
                  <FolderGit2 className="h-8 w-8 theme-text-sub opacity-50" />
                  <div className="text-sm theme-text-muted">{t('gitBoard.selectHint')}</div>
                </div>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}

/* ===================== 文件 diff 查看器 ===================== */

interface ParsedDiffLine {
  kind: 'meta' | 'hunk' | 'add' | 'del' | 'ctx';
  text: string;
  oldNo: number | null;
  newNo: number | null;
}

/** 解析 unified diff 文本为带新旧行号的行数组 */
function parseDiffText(text: string): ParsedDiffLine[] {
  const out: ParsedDiffLine[] = [];
  let oldNo = 0;
  let newNo = 0;
  for (const raw of text.split('\n')) {
    if (!raw) continue;
    if (raw.startsWith('@@')) {
      const m = raw.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
      if (m) {
        oldNo = Number(m[1]);
        newNo = Number(m[2]);
      }
      out.push({ kind: 'hunk', text: raw, oldNo: null, newNo: null });
    } else if (
      /^(diff --git|index |--- |\+\+\+ |new file|deleted file|similarity index|rename (from|to)|old mode|new mode|Binary files|\\ No newline)/.test(
        raw
      )
    ) {
      out.push({ kind: 'meta', text: raw, oldNo: null, newNo: null });
    } else if (raw.startsWith('+')) {
      out.push({ kind: 'add', text: raw.slice(1), oldNo: null, newNo: newNo++ });
    } else if (raw.startsWith('-')) {
      out.push({ kind: 'del', text: raw.slice(1), oldNo: oldNo++, newNo: null });
    } else {
      out.push({
        kind: 'ctx',
        text: raw.startsWith(' ') ? raw.slice(1) : raw,
        oldNo: oldNo++,
        newNo: newNo++,
      });
    }
  }
  return out;
}

function DiffBody({ text }: { text: string }) {
  const lines = useMemo(() => parseDiffText(text), [text]);
  return (
    <div className="font-mono text-xs leading-5">
      {lines.map((l, i) => {
        const cls =
          l.kind === 'add'
            ? 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
            : l.kind === 'del'
              ? 'bg-red-500/10 text-red-700 dark:text-red-300'
              : l.kind === 'hunk'
                ? 'bg-sky-500/10 text-sky-600 dark:text-sky-400'
                : l.kind === 'meta'
                  ? 'theme-text-sub'
                  : 'theme-text-muted';
        return (
          <div key={i} className={`flex ${cls}`}>
            <span className="w-12 shrink-0 text-right pr-2 select-none opacity-40">
              {l.oldNo ?? ''}
            </span>
            <span className="w-12 shrink-0 text-right pr-2 select-none opacity-40 border-r theme-border-sub mr-3">
              {l.newNo ?? ''}
            </span>
            <span className="flex-1 min-w-0 whitespace-pre-wrap break-all pr-4">
              {l.text || '\u00A0'}
            </span>
          </div>
        );
      })}
    </div>
  );
}

/**
 * 未提交文件 diff 审查器：左侧文件列表 + 右侧 diff 视图。
 * 点击列表或 ↑↓ 键切换文件；同时有暂存与工作区变更时可切换查看视角。
 */
function GitDiffViewer({
  workspacePath,
  files,
  initialIndex,
  onClose,
}: {
  workspacePath: string;
  files: GitStatusFile[];
  initialIndex: number;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [index, setIndex] = useState(() => Math.min(initialIndex, files.length - 1));

  // 文件列表在外部刷新后可能变短，索引越界时收敛
  useEffect(() => {
    if (index > files.length - 1) setIndex(Math.max(0, files.length - 1));
  }, [files, index]);

  const file = files[index] ?? null;

  // 可查看的变更类型：同时有暂存与未暂存变更时可切换
  const modes = useMemo(() => {
    const m: ('staged' | 'unstaged')[] = [];
    if (file?.staged) m.push('staged');
    if (file?.worktree) m.push('unstaged');
    return m;
  }, [file]);
  const [modePref, setModePref] = useState<'staged' | 'unstaged' | null>(null);
  const mode = modePref && modes.includes(modePref) ? modePref : (modes[0] ?? 'unstaged');

  const [data, setData] = useState<GitFileDiffResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!file) return;
    let cancelled = false;
    setData(null);
    setError(null);
    setLoading(true);
    api.gitBoard
      .fileDiff(workspacePath, file.path, mode === 'staged')
      .then((res) => {
        if (!cancelled) setData(res);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [workspacePath, file, mode]);

  // Esc 关闭，↑↓ 切换文件
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        setIndex((i) => Math.min(i + 1, files.length - 1));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setIndex((i) => Math.max(i - 1, 0));
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose, files.length]);

  if (!file) return null;

  const navBtn =
    'p-1 rounded-md theme-text-muted hover:theme-text-main hover:bg-black/5 dark:hover:bg-white/5 transition-colors cursor-pointer disabled:opacity-30 disabled:cursor-default shrink-0';

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm animate-in fade-in duration-150"
      onClick={onClose}
    >
      <div
        className="w-full max-w-6xl h-[85vh] theme-bg-card border theme-border rounded-2xl shadow-2xl overflow-hidden flex animate-in zoom-in-95 duration-150"
        onClick={(e) => e.stopPropagation()}
      >
        {/* 左：文件列表 */}
        <div className="w-72 flex-shrink-0 border-r theme-border flex flex-col min-h-0">
          <div className="px-4 py-3 border-b theme-border flex items-center justify-between gap-2 flex-shrink-0">
            <span className="text-xs font-semibold theme-text-muted">
              {t('gitBoard.filesTitle')}
            </span>
            <span className="text-[10px] theme-text-sub tabular-nums">
              {index + 1} / {files.length}
            </span>
          </div>
          <div className="flex-1 overflow-y-auto min-h-0 py-1">
            {files.map((f, i) => (
              <div
                key={`${f.path}-${f.staged ?? ''}-${f.worktree ?? ''}`}
                onClick={() => setIndex(i)}
                className={`flex items-center gap-2 px-3 py-1.5 cursor-pointer transition-colors ${
                  i === index
                    ? 'bg-black/[0.07] dark:bg-white/[0.09]'
                    : 'hover:bg-black/5 dark:hover:bg-white/5'
                }`}
              >
                <FileStatusGlyph f={f} />
                <span
                  className={`text-xs truncate font-mono ${
                    i === index ? 'theme-text-main' : 'theme-text-muted'
                  }`}
                  title={f.old_path ? `${f.old_path} → ${f.path}` : f.path}
                >
                  {f.path}
                </span>
              </div>
            ))}
          </div>
        </div>

        {/* 右：diff 视图 */}
        <div className="flex-1 min-w-0 flex flex-col">
          <div className="px-4 py-3 border-b theme-border flex items-center gap-2 flex-shrink-0">
            <button
              onClick={() => setIndex((i) => Math.max(i - 1, 0))}
              disabled={index === 0}
              title={t('gitBoard.diffPrevTitle')}
              className={navBtn}
            >
              <ChevronUp className="h-4 w-4" />
            </button>
            <button
              onClick={() => setIndex((i) => Math.min(i + 1, files.length - 1))}
              disabled={index === files.length - 1}
              title={t('gitBoard.diffNextTitle')}
              className={navBtn}
            >
              <ChevronDown className="h-4 w-4" />
            </button>
            <FileDiff className="h-4 w-4 text-orange-500 shrink-0 ml-1" />
            <span
              className="text-sm font-mono font-bold theme-text-main truncate"
              title={file.old_path ? `${file.old_path} → ${file.path}` : file.path}
            >
              {file.path}
            </span>
            {modes.length > 1 && (
              <div className="flex gap-1 ml-1 shrink-0">
                {modes.map((m) => (
                  <button
                    key={m}
                    onClick={() => setModePref(m)}
                    className={`px-2 py-0.5 rounded-md text-[11px] font-semibold transition-colors cursor-pointer ${
                      mode === m
                        ? 'bg-orange-500/15 text-orange-500'
                        : 'theme-text-muted hover:theme-text-main'
                    }`}
                  >
                    {m === 'staged' ? t('gitBoard.diffStagedTab') : t('gitBoard.diffUnstagedTab')}
                  </button>
                ))}
              </div>
            )}
            <button
              onClick={onClose}
              className="ml-auto p-1 rounded-md theme-text-muted hover:theme-text-main hover:bg-black/5 dark:hover:bg-white/5 transition-colors cursor-pointer shrink-0"
            >
              <X className="h-4 w-4" />
            </button>
          </div>

          {/* 内容区 */}
          <div className="flex-1 overflow-auto min-h-0">
            {loading ? (
              <div className="flex items-center justify-center py-16 gap-2">
                <Loader2 className="h-5 w-5 animate-spin text-orange-500" />
                <span className="text-xs theme-text-muted">{t('gitBoard.diffLoading')}</span>
              </div>
            ) : error ? (
              <div className="flex flex-col items-center justify-center py-16 gap-1 text-center">
                <AlertCircle className="h-6 w-6 text-red-400" />
                <div className="text-xs theme-text-muted">{t('gitBoard.diffError')}</div>
                <div className="text-[10px] theme-text-sub font-mono max-w-md truncate">{error}</div>
              </div>
            ) : data?.binary ? (
              <div className="flex items-center justify-center py-16 text-xs theme-text-sub">
                {t('gitBoard.diffBinary')}
              </div>
            ) : data && data.diff ? (
              <div>
                <DiffBody text={data.diff} />
                {data.truncated && (
                  <div className="text-[10px] theme-text-sub text-center py-2 border-t theme-border-sub">
                    {t('gitBoard.diffTruncated')}
                  </div>
                )}
              </div>
            ) : (
              <div className="flex items-center justify-center py-16 text-xs theme-text-sub">
                {t('gitBoard.diffEmpty')}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
