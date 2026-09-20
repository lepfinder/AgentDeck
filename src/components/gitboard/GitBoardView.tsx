/**
 * Git 看板 — 最近 N 天有会话活动的工作区 Git 状态总览
 *
 * 数据由后端 git_board 模块只读采集（分支 / ahead-behind / 变更计数 / 最近提交），
 * 本视图纯展示，不做任何 git 写操作。
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import {
  GitBranch,
  GitCommit,
  RefreshCw,
  Loader2,
  AlertCircle,
  ArrowUp,
  ArrowDown,
  ArrowLeft,
  CheckCircle2,
  FolderGit2,
  Send,
  X,
  ChevronUp,
  ChevronDown,
  ChevronRight,
  Sparkles,
  EyeOff,
  History,
  Folder,
  FileText,
  Search,
  Plus,
  Minus,
  Download,
  FolderOpen,
} from 'lucide-react';
import { api } from '../../api/tauriBridge';
import { useI18n } from '../../i18n';
import { OpenInIdeMenu } from '../browse/OpenInIdeMenu';
import {
  getHiddenPaths,
  hideWorkspace,
  onHiddenChange,
  unhideWorkspace,
} from '../../lib/gitBoardHidden';
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
  remote_name: string | null;
  remote_url: string | null;
  upstream: string | null;
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
  id: string;
  message: string;
  short_id: string;
  author: string | null;
  time: string;
}

export interface GitCommitFile {
  path: string;
  status: string;
}

export interface GitCommitShowResponse {
  short_id: string;
  message: string;
  author: string | null;
  time: string;
  files: GitCommitFile[];
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

export interface GitAddIgnoreResponse {
  pattern: string;
  added: boolean;
  tracked: boolean;
}

export interface WorkspaceDirEntry {
  name: string;
  path: string;
  is_dir: boolean;
}

export interface WorkspaceFileContent {
  content: string;
  truncated: boolean;
  binary: boolean;
  size: number;
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

function GitBoardListItem({
  entry,
  active,
  divider,
  onSelect,
  onHide,
}: {
  entry: GitBoardEntry;
  active: boolean;
  divider: boolean;
  onSelect: () => void;
  onHide: () => void;
}) {
  const { t } = useI18n();
  const name =
    entry.display_name ||
    entry.workspace_path.split('/').slice(-1)[0] ||
    entry.workspace_path;
  const dirtyCount = entry.staged + entry.unstaged + entry.untracked + entry.conflicts;

  return (
    <div
      onClick={onSelect}
      className={`group relative w-full text-left px-3 py-2.5 transition-colors cursor-pointer ${
        divider ? 'border-t theme-border-sub ' : ''
      }${
        active
          ? 'bg-black/[0.05] dark:bg-white/[0.07]'
          : 'hover:bg-black/[0.03] dark:hover:bg-white/[0.04]'
      }`}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="text-sm font-semibold truncate theme-text-main">{name}</span>
        <span className="text-[10px] theme-text-sub flex-shrink-0 group-hover:opacity-0 transition-opacity">
          {formatRelativeTime(entry.last_commit_time ?? entry.last_activity ?? undefined)}
        </span>
      </div>
      <div className="flex items-center gap-1.5 mt-1 text-[10px] theme-text-sub flex-wrap">
        {entry.detached ? (
          <span>{t('gitBoard.detached')}</span>
        ) : entry.branch ? (
          <span className="font-mono">{entry.branch}</span>
        ) : null}
        <span className="opacity-50">·</span>
        {dirtyCount > 0 ? (
          <span>{t('gitBoard.pillDirty', { n: dirtyCount })}</span>
        ) : (
          <span>{t('gitBoard.clean')}</span>
        )}
        {(entry.ahead ?? 0) > 0 && (
          <>
            <span className="opacity-50">·</span>
            <span>{t('gitBoard.listAhead', { n: entry.ahead ?? 0 })}</span>
          </>
        )}
        {(entry.behind ?? 0) > 0 && (
          <>
            <span className="opacity-50">·</span>
            <span>{t('gitBoard.listBehind', { n: entry.behind ?? 0 })}</span>
          </>
        )}
      </div>
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          onHide();
        }}
        title={t('gitBoard.hideTitle')}
        className="absolute right-2 top-2.5 hidden group-hover:flex items-center gap-1 px-1.5 py-0.5 rounded-md text-[10px] theme-bg-sub border theme-border theme-text-muted hover:theme-text-main transition-colors cursor-pointer"
      >
        <EyeOff className="h-3 w-3" />
        {t('gitBoard.hide')}
      </button>
    </div>
  );
}

/** 文件暂存/工作区状态字母（如 "A " / " M" / " U" 未跟踪 / "UU" 冲突），颜色区分状态 */
function FileStatusGlyph({ f }: { f: GitStatusFile }) {
  const { t } = useI18n();
  const wt = f.worktree;
  const isUntracked = wt === '?';
  const isConflict = wt === 'U';
  // 未跟踪对外展示为绿色 U（冲突仍为红色 U）
  const displayWt = isUntracked ? 'U' : wt;
  const cls = isConflict
    ? 'text-red-500'
    : isUntracked
      ? 'text-emerald-500'
      : wt
        ? 'text-amber-500'
        : 'text-blue-500';
  const tip = isConflict
    ? t('gitBoard.fileConflict')
    : isUntracked
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
      {displayWt ?? ' '}
    </span>
  );
}

function GitBoardDetail({
  entry,
  onChanged,
  hideHeader = false,
}: {
  entry: GitBoardEntry;
  onChanged: () => void;
  /** 品字型项目页：不渲染详情自有头部，操作按钮 Portal 到 ProjectHeader */
  hideHeader?: boolean;
}) {
  const { t } = useI18n();
  const [commits, setCommits] = useState<GitCommitInfo[] | null>(null);
  const [commitsLoading, setCommitsLoading] = useState(false);
  const [commitsError, setCommitsError] = useState<string | null>(null);
  const [statusFiles, setStatusFiles] = useState<GitStatusFilesResponse | null>(null);
  const [filesLoading, setFilesLoading] = useState(false);
  const [filesError, setFilesError] = useState<string | null>(null);
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [collapsedDirs, setCollapsedDirs] = useState<Record<string, boolean>>({});
  const [commitsOpen, setCommitsOpen] = useState(false);
  const [viewCommit, setViewCommit] = useState<GitCommitInfo | null>(null);
  const [showCommitPanel, setShowCommitPanel] = useState(false);
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
  const [actionHost, setActionHost] = useState<HTMLElement | null>(null);

  // 文件树右键菜单与忽略提示
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; path: string } | null>(null);
  const [ignoreHint, setIgnoreHint] = useState<string | null>(null);
  const [filesKey, setFilesKey] = useState(0);

  // 左侧树 Tab：Git 变更 / 全部文件（懒加载浏览）
  const [treeTab, setTreeTab] = useState<'changes' | 'files'>('changes');
  const [dirChildren, setDirChildren] = useState<Record<string, WorkspaceDirEntry[]>>({});
  const [dirLoading, setDirLoading] = useState<Record<string, boolean>>({});
  const [expandedDirs, setExpandedDirs] = useState<Record<string, boolean>>({});
  const [selectedBrowseFile, setSelectedBrowseFile] = useState<string | null>(null);
  const [browseContent, setBrowseContent] = useState<WorkspaceFileContent | null>(null);
  const [browseLoading, setBrowseLoading] = useState(false);
  const [browseError, setBrowseError] = useState<string | null>(null);
  // Markdown 文件预览模式：渲染 / 原文
  const [mdMode, setMdMode] = useState<'rendered' | 'raw'>('rendered');
  // 全部文件搜索（null = 未启用搜索，显示树）
  const [searchQuery, setSearchQuery] = useState('');
  const [searchResults, setSearchResults] = useState<string[] | null>(null);
  const searchSeq = useRef(0);
  // fetch / 单文件暂存进行中标记
  const [fetching, setFetching] = useState(false);
  const [stagingPath, setStagingPath] = useState<string | null>(null);

  const isMarkdownFile = /\.(md|markdown)$/i.test(selectedBrowseFile ?? '');

  const dirtyCount = entry.staged + entry.unstaged + entry.untracked + entry.conflicts;

  // 切换项目时重置提交/推送操作区状态与打开的弹窗
  useEffect(() => {
    setCommitMsg('');
    setCommitError(null);
    setCommitDone(false);
    setPushError(null);
    setPushDone(false);
    setPushConfirm(false);
    setSelectedFile(null);
    setCollapsedDirs({});
    setCommitsOpen(false);
    setViewCommit(null);
    setShowCommitPanel(false);
    setCtxMenu(null);
    setTreeTab('changes');
    setDirChildren({});
    setDirLoading({});
    setExpandedDirs({});
    setSelectedBrowseFile(null);
    setBrowseContent(null);
    setBrowseError(null);
    setMdMode('rendered');
    setSearchQuery('');
    setSearchResults(null);
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
  }, [entry.workspace_path, dirtyCount, filesKey]);

  const files = useMemo(() => statusFiles?.files ?? [], [statusFiles]);

  // 文件列表刷新后选中项不存在时回退到第一个
  useEffect(() => {
    if (files.length === 0) {
      if (selectedFile !== null) setSelectedFile(null);
      return;
    }
    if (!selectedFile || !files.some((f) => f.path === selectedFile)) {
      setSelectedFile(files[0].path);
    }
  }, [files, selectedFile]);

  const selectedFileObj = useMemo(
    () => files.find((f) => f.path === selectedFile) ?? null,
    [files, selectedFile]
  );
  const selectedIndex = selectedFileObj ? files.indexOf(selectedFileObj) : -1;

  // 可查看的变更类型：同时有暂存与工作区变更时可切换
  const modes = useMemo(() => {
    const m: ('staged' | 'unstaged')[] = [];
    if (selectedFileObj?.staged) m.push('staged');
    if (selectedFileObj?.worktree) m.push('unstaged');
    return m;
  }, [selectedFileObj]);
  const [modePref, setModePref] = useState<'staged' | 'unstaged' | null>(null);
  const mode = modePref && modes.includes(modePref) ? modePref : (modes[0] ?? 'unstaged');

  const [fileDiff, setFileDiff] = useState<GitFileDiffResponse | null>(null);
  const [diffLoading, setDiffLoading] = useState(false);
  const [diffError, setDiffError] = useState<string | null>(null);

  useEffect(() => {
    if (!ignoreHint) return;
    const timer = setTimeout(() => setIgnoreHint(null), 3000);
    return () => clearTimeout(timer);
  }, [ignoreHint]);

  useEffect(() => {
    if (!ctxMenu) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setCtxMenu(null);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [ctxMenu]);

  /* ---- 全部文件树（懒加载浏览） ---- */

  const loadDir = useCallback(
    async (rel: string) => {
      setDirLoading((m) => ({ ...m, [rel]: true }));
      try {
        const res = await api.gitBoard.listDir(entry.workspace_path, rel || undefined);
        setDirChildren((m) => ({ ...m, [rel]: res }));
      } catch (e) {
        console.error('Failed to list dir:', rel, e);
        setDirChildren((m) => ({ ...m, [rel]: [] }));
      } finally {
        setDirLoading((m) => {
          const next = { ...m };
          delete next[rel];
          return next;
        });
      }
    },
    [entry.workspace_path]
  );

  // 首次切到「全部文件」时加载根目录
  useEffect(() => {
    if (treeTab === 'files' && dirChildren[''] === undefined) void loadDir('');
  }, [treeTab, dirChildren, loadDir]);

  const handleToggleBrowseDir = useCallback(
    (path: string) => {
      const willExpand = !expandedDirs[path];
      setExpandedDirs((prev) => ({ ...prev, [path]: willExpand }));
      if (willExpand && dirChildren[path] === undefined) void loadDir(path);
    },
    [expandedDirs, dirChildren, loadDir]
  );

  // 忽略规则写入后刷新已加载的目录层级
  const refreshBrowseTree = useCallback(() => {
    setDirChildren({});
    const paths = ['', ...Object.keys(expandedDirs).filter((p) => expandedDirs[p])];
    paths.forEach((p) => void loadDir(p));
  }, [expandedDirs, loadDir]);

  useEffect(() => {
    if (treeTab !== 'files' || !selectedBrowseFile) return;
    let cancelled = false;
    setBrowseContent(null);
    setBrowseError(null);
    setMdMode('rendered');
    setBrowseLoading(true);
    api.gitBoard
      .readFile(entry.workspace_path, selectedBrowseFile)
      .then((res) => {
        if (!cancelled) setBrowseContent(res);
      })
      .catch((e) => {
        if (!cancelled) setBrowseError(String(e));
      })
      .finally(() => {
        if (!cancelled) setBrowseLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [treeTab, entry.workspace_path, selectedBrowseFile]);

  const handleAddIgnore = useCallback(
    async (path: string) => {
      try {
        const res = await api.gitBoard.addIgnore(entry.workspace_path, path);
        if (res.tracked) {
          setIgnoreHint(t('gitBoard.ignoreTracked'));
        } else if (res.added) {
          setIgnoreHint(t('gitBoard.ignoreAdded', { p: res.pattern }));
        } else {
          setIgnoreHint(t('gitBoard.ignoreExists', { p: res.pattern }));
        }
        setFilesKey((k) => k + 1);
        if (treeTab === 'files') refreshBrowseTree();
        onChanged();
      } catch (e) {
        setIgnoreHint(String(e));
      }
    },
    [entry.workspace_path, onChanged, t, treeTab, refreshBrowseTree]
  );

  // 搜索防抖：query 非空时展示扁平结果列表
  useEffect(() => {
    if (treeTab !== 'files') return;
    const q = searchQuery.trim();
    const seq = ++searchSeq.current;
    if (!q) {
      setSearchResults(null);
      return;
    }
    const timer = setTimeout(() => {
      api.gitBoard
        .searchFiles(entry.workspace_path, q, 100)
        .then((res) => {
          if (searchSeq.current === seq) setSearchResults(res);
        })
        .catch((e) => {
          console.error('Failed to search files:', e);
          if (searchSeq.current === seq) setSearchResults([]);
        });
    }, 250);
    return () => clearTimeout(timer);
  }, [treeTab, searchQuery, entry.workspace_path]);

  // 全部文件树叠加 Git 状态徽章（复用变更列表数据）
  const statusByPath = useMemo(() => {
    const m = new Map<string, GitStatusFile>();
    for (const f of files) m.set(f.path.replace(/\/+$/, ''), f);
    return m;
  }, [files]);

  const handleStage = useCallback(
    async (rawPath: string, stage: boolean) => {
      if (stagingPath) return;
      const path = rawPath.replace(/\/+$/, '');
      if (!path) return;
      setStagingPath(rawPath);
      try {
        if (stage) await api.gitBoard.stageFile(entry.workspace_path, path);
        else await api.gitBoard.unstageFile(entry.workspace_path, path);
        setFilesKey((k) => k + 1);
        onChanged();
      } catch (e) {
        setIgnoreHint(String(e));
      } finally {
        setStagingPath(null);
      }
    },
    [entry.workspace_path, onChanged, stagingPath]
  );

  const handleStageAll = useCallback(async () => {
    try {
      await api.gitBoard.stageAll(entry.workspace_path);
      setFilesKey((k) => k + 1);
      onChanged();
    } catch (e) {
      setIgnoreHint(String(e));
    }
  }, [entry.workspace_path, onChanged]);

  const handleFetch = useCallback(async () => {
    if (fetching) return;
    setFetching(true);
    try {
      await api.gitBoard.fetch(entry.workspace_path);
      onChanged();
    } catch (e) {
      setIgnoreHint(String(e));
    } finally {
      setFetching(false);
    }
  }, [entry.workspace_path, fetching, onChanged]);

  const revealPath = useCallback(
    (rel: string) => {
      void api.revealInFolder(`${entry.workspace_path}/${rel.replace(/\/+$/, '')}`).catch((e) => {
        setIgnoreHint(String(e));
      });
    },
    [entry.workspace_path]
  );

  useEffect(() => {
    if (!selectedFileObj) return;
    let cancelled = false;
    setFileDiff(null);
    setDiffError(null);
    setDiffLoading(true);
    api.gitBoard
      .fileDiff(entry.workspace_path, selectedFileObj.path, mode === 'staged')
      .then((res) => {
        if (!cancelled) setFileDiff(res);
      })
      .catch((e) => {
        if (!cancelled) setDiffError(String(e));
      })
      .finally(() => {
        if (!cancelled) setDiffLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [entry.workspace_path, selectedFileObj, mode]);

  const fileTree = useMemo(() => buildFileTree(files), [files]);

  const name =
    entry.display_name ||
    entry.workspace_path.split('/').slice(-1)[0] ||
    entry.workspace_path;

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

  // 项目页 Git 模式：把操作按钮挂到 ProjectHeader 的插槽
  useEffect(() => {
    if (!hideHeader) {
      setActionHost(null);
      return;
    }
    const el = document.getElementById('project-git-actions');
    setActionHost(el);
  }, [hideHeader, entry.workspace_path]);

  const headerActionButtons = (
    <div className="flex items-center gap-2 flex-shrink-0">
      <button
        onClick={() => void handleFetch()}
        disabled={fetching}
        title={t('gitBoard.fetchTitle')}
        className="flex items-center gap-1.5 px-2.5 py-1.5 text-xs border theme-border theme-bg-sub theme-text-muted hover:theme-text-main rounded-lg transition-colors cursor-pointer disabled:opacity-50"
      >
        {fetching ? (
          <Loader2 className="h-3.5 w-3.5 animate-spin text-sky-500" />
        ) : (
          <Download className="h-3.5 w-3.5 text-sky-500" />
        )}
        <span>{t('gitBoard.fetch')}</span>
      </button>
      <button
        onClick={() => setCommitsOpen(true)}
        className="flex items-center gap-1.5 px-2.5 py-1.5 text-xs border theme-border theme-bg-sub theme-text-muted hover:theme-text-main rounded-lg transition-colors cursor-pointer"
      >
        <History className="h-3.5 w-3.5" />
        <span>{t('gitBoard.commitsTitle')}</span>
      </button>
      {(dirtyCount > 0 || (entry.ahead ?? 0) > 0) && (
        <button
          onClick={() => setShowCommitPanel((v) => !v)}
          className={`flex items-center gap-1.5 px-2.5 py-1.5 text-xs border rounded-lg transition-colors cursor-pointer ${
            showCommitPanel
              ? 'theme-border bg-black/[0.06] dark:bg-white/[0.08] theme-text-main'
              : 'theme-border theme-bg-sub theme-text-muted hover:theme-text-main'
          }`}
        >
          <GitCommit className="h-3.5 w-3.5 text-orange-500" />
          <span>{t('gitBoard.commitOrPush')}</span>
        </button>
      )}
    </div>
  );

  return (
    <div className={`${hideHeader ? 'h-full' : 'h-full rounded-xl border theme-border theme-bg-card'} flex flex-col overflow-hidden`}>
      {/* 头部：项目名、路径与状态徽章（项目页 Git 模式隐藏，信息在 ProjectHeader） */}
      {!hideHeader && (
      <div className="px-4 py-3 border-b theme-border flex-shrink-0">
        <div className="flex items-start justify-between gap-2">
          <div className="flex items-center gap-1.5 flex-wrap min-w-0">
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
          {headerActionButtons}
        </div>
        <div
          className="text-[11px] theme-text-sub truncate font-mono mt-1"
          title={entry.workspace_path}
        >
          {entry.workspace_path}
        </div>
        {(entry.remote_url || entry.upstream) && (
          <div
            className="text-[10px] theme-text-sub mt-1 truncate font-mono"
            title={entry.remote_url ?? undefined}
          >
            {entry.remote_name && <span>{entry.remote_name} </span>}
            {entry.remote_url && <span>{entry.remote_url} </span>}
            {entry.upstream && <span>· {t('gitBoard.tracking', { b: entry.upstream })}</span>}
          </div>
        )}
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
      )}

      {/* 操作按钮挂到项目 Header（隐藏本地头部时） */}
      {hideHeader && actionHost && createPortal(headerActionButtons, actionHost)}

      {/* 提交 / 推送操作区（点右上角「提交 / 推送」后展示） */}
      {showCommitPanel && (dirtyCount > 0 || (entry.ahead ?? 0) > 0) && (
        <div className="px-4 py-3 border-b theme-border flex-shrink-0 flex flex-col gap-2">
          {dirtyCount > 0 && (
            <>
              <textarea
                value={commitMsg}
                onChange={(e) => setCommitMsg(e.target.value)}
                rows={4}
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
                  onClick={() => void handleStageAll()}
                  disabled={committing || aiLoading}
                  title={t('gitBoard.stageAllBtn')}
                  className={actionBtn}
                >
                  <Plus className="h-3.5 w-3.5 text-emerald-500" />
                  <span>{t('gitBoard.stageAllBtn')}</span>
                </button>
                <button
                  onClick={() => void handleCommit()}
                  disabled={committing || aiLoading || !commitMsg.trim() || entry.staged === 0}
                  title={entry.staged === 0 ? t('gitBoard.commitNothingStaged') : undefined}
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
                  <span
                    className="text-[10px] theme-text-sub font-mono truncate"
                    title={entry.remote_url ?? undefined}
                  >
                    {t('gitBoard.pushTarget', {
                      b:
                        entry.upstream ??
                        `${entry.remote_name ?? 'origin'}/${entry.branch ?? 'HEAD'}`,
                    })}
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

      {/* 主体：左文件树（Git 变更 / 全部文件）+ 右预览 */}
      <div className="flex-1 flex min-h-0">
        {/* 左：树 */}
        <div className="w-64 lg:w-72 flex-shrink-0 border-r theme-border flex flex-col min-h-0">
          <div className="px-2 pt-2 pb-1 flex-shrink-0">
            <div className="flex items-center rounded-lg border theme-border theme-bg-sub overflow-hidden">
              <button
                onClick={() => setTreeTab('changes')}
                className={`flex-1 py-1 text-[11px] font-medium transition-colors cursor-pointer ${
                  treeTab === 'changes'
                    ? 'bg-black/[0.06] dark:bg-white/[0.08] theme-text-main'
                    : 'theme-text-muted hover:theme-text-main'
                }`}
              >
                {t('gitBoard.tabChanges')}
              </button>
              <button
                onClick={() => setTreeTab('files')}
                className={`flex-1 py-1 text-[11px] font-medium transition-colors cursor-pointer ${
                  treeTab === 'files'
                    ? 'bg-black/[0.06] dark:bg-white/[0.08] theme-text-main'
                    : 'theme-text-muted hover:theme-text-main'
                }`}
              >
                {t('gitBoard.tabFiles')}
              </button>
            </div>
          </div>
          {treeTab === 'files' && (
            <div className="relative px-2 pb-1 flex-shrink-0">
              <Search className="h-3.5 w-3.5 theme-text-sub absolute left-[18px] top-1/2 -translate-y-1/2 pointer-events-none z-10" />
              <input
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder={t('gitBoard.searchPlaceholder')}
                className="w-full text-xs theme-bg-sub theme-text-main rounded-lg border theme-border pl-7 pr-6 py-1 outline-none focus:border-black/30 dark:focus:border-white/30 placeholder:theme-text-sub"
              />
              {searchQuery && (
                <button
                  type="button"
                  onClick={() => setSearchQuery('')}
                  className="absolute right-4 top-1/2 -translate-y-1/2 theme-text-sub hover:theme-text-main cursor-pointer"
                >
                  <X className="h-3 w-3" />
                </button>
              )}
            </div>
          )}
          <div className="flex-1 overflow-y-auto min-h-0 py-1">
            {treeTab === 'changes' ? (
              dirtyCount === 0 || filesLoading || filesError || files.length === 0 ? (
                filesLoading ? (
                  <div className="flex items-center justify-center gap-2 py-6">
                    <Loader2 className="h-4 w-4 animate-spin text-orange-500" />
                    <span className="text-xs theme-text-muted">{t('gitBoard.filesLoading')}</span>
                  </div>
                ) : filesError ? (
                  <div className="flex flex-col items-center justify-center gap-1 py-6 px-3 text-center">
                    <AlertCircle className="h-5 w-5 text-red-400" />
                    <div className="text-xs theme-text-muted">{t('gitBoard.filesError')}</div>
                  </div>
                ) : (
                  <div className="flex items-center justify-center py-6 text-xs theme-text-sub">
                    {t('gitBoard.filesEmpty')}
                  </div>
                )
              ) : (
                <>
                  <FileTreeNode
                    node={fileTree}
                    depth={0}
                    collapsed={collapsedDirs}
                    onToggle={(p) => setCollapsedDirs((prev) => ({ ...prev, [p]: !prev[p] }))}
                    selected={selectedFile}
                    onSelect={setSelectedFile}
                    onStage={handleStage}
                    stagingPath={stagingPath}
                    onContext={(e, p) => {
                      e.preventDefault();
                      setCtxMenu({ x: e.clientX, y: e.clientY, path: p });
                    }}
                  />
                  {statusFiles && statusFiles.total > statusFiles.files.length && (
                    <div className="text-[10px] theme-text-sub text-center py-1.5">
                      {t('gitBoard.filesMore', { n: statusFiles.total - statusFiles.files.length })}
                    </div>
                  )}
                </>
              )
            ) : searchResults !== null ? (
              searchResults.length === 0 ? (
                <div className="py-6 text-center text-xs theme-text-sub">
                  {t('gitBoard.searchEmpty')}
                </div>
              ) : (
                searchResults.map((p) => (
                  <button
                    type="button"
                    key={p}
                    onClick={() => setSelectedBrowseFile(p)}
                    className={`flex items-center gap-1.5 w-full text-left pl-2 pr-2 py-1 transition-colors cursor-pointer ${
                      selectedBrowseFile === p
                        ? 'bg-black/[0.06] dark:bg-white/[0.08]'
                        : 'hover:bg-black/[0.03] dark:hover:bg-white/[0.04]'
                    }`}
                  >
                    {statusByPath.get(p) ? (
                      <FileStatusGlyph f={statusByPath.get(p)!} />
                    ) : (
                      <span className="w-5 shrink-0" />
                    )}
                    <FileText className="h-3.5 w-3.5 theme-text-sub shrink-0" />
                    <span
                      className={`text-[11px] truncate font-mono ${
                        selectedBrowseFile === p ? 'theme-text-main' : 'theme-text-muted'
                      }`}
                      title={p}
                    >
                      {p}
                    </span>
                  </button>
                ))
              )
            ) : dirChildren[''] === undefined ? (
              <div className="flex items-center justify-center gap-2 py-6">
                <Loader2 className="h-4 w-4 animate-spin text-orange-500" />
                <span className="text-xs theme-text-muted">{t('gitBoard.filesLoading')}</span>
              </div>
            ) : (
              <BrowseTreeNode
                entries={dirChildren['']}
                depth={0}
                dirChildren={dirChildren}
                dirLoading={dirLoading}
                expandedDirs={expandedDirs}
                statusByPath={statusByPath}
                selected={selectedBrowseFile}
                onToggleDir={handleToggleBrowseDir}
                onSelectFile={setSelectedBrowseFile}
                onContext={(e, p) => {
                  e.preventDefault();
                  setCtxMenu({ x: e.clientX, y: e.clientY, path: p });
                }}
              />
            )}
          </div>
        </div>
        {/* 右：预览 */}
        <div className="flex-1 min-w-0 flex flex-col min-h-0">
          {treeTab === 'changes' ? (
            <>
              {selectedFileObj && (
                <div className="px-4 py-2 border-b theme-border flex items-center gap-2 flex-shrink-0">
                  <button
                    onClick={() => setSelectedFile(files[selectedIndex - 1]?.path ?? null)}
                    disabled={selectedIndex <= 0}
                    title={t('gitBoard.diffPrevTitle')}
                    className={iconBtn}
                  >
                    <ChevronUp className="h-4 w-4" />
                  </button>
                  <button
                    onClick={() => setSelectedFile(files[selectedIndex + 1]?.path ?? null)}
                    disabled={selectedIndex >= files.length - 1}
                    title={t('gitBoard.diffNextTitle')}
                    className={iconBtn}
                  >
                    <ChevronDown className="h-4 w-4" />
                  </button>
                  <FileStatusGlyph f={selectedFileObj} />
                  <span
                    className="text-xs font-mono font-semibold theme-text-main truncate"
                    title={
                      selectedFileObj.old_path
                        ? `${selectedFileObj.old_path} → ${selectedFileObj.path}`
                        : selectedFileObj.path
                    }
                  >
                    {selectedFileObj.path}
                  </span>
                  {modes.length > 1 && (
                    <div className="flex gap-1 ml-1 shrink-0">
                      {modes.map((m) => (
                        <button
                          key={m}
                          onClick={() => setModePref(m)}
                          className={`px-2 py-0.5 rounded-md text-[11px] font-semibold transition-colors cursor-pointer ${
                            mode === m
                              ? 'bg-black/[0.06] dark:bg-white/[0.08] theme-text-main'
                              : 'theme-text-muted hover:theme-text-main'
                          }`}
                        >
                          {m === 'staged'
                            ? t('gitBoard.diffStagedTab')
                            : t('gitBoard.diffUnstagedTab')}
                        </button>
                      ))}
                    </div>
                  )}
                  <span className="ml-auto text-[10px] theme-text-sub tabular-nums shrink-0">
                    {selectedIndex + 1} / {files.length}
                  </span>
                </div>
              )}
              <div className="flex-1 overflow-auto min-h-0">
                {diffLoading ? (
                  <div className="flex items-center justify-center py-16 gap-2">
                    <Loader2 className="h-5 w-5 animate-spin text-orange-500" />
                    <span className="text-xs theme-text-muted">{t('gitBoard.diffLoading')}</span>
                  </div>
                ) : diffError ? (
                  <div className="flex flex-col items-center justify-center py-16 gap-1 text-center">
                    <AlertCircle className="h-6 w-6 text-red-400" />
                    <div className="text-xs theme-text-muted">{t('gitBoard.diffError')}</div>
                    <div className="text-[10px] theme-text-sub font-mono max-w-md truncate">
                      {diffError}
                    </div>
                  </div>
                ) : fileDiff?.binary ? (
                  <div className="flex items-center justify-center py-16 text-xs theme-text-sub">
                    {t('gitBoard.diffBinary')}
                  </div>
                ) : fileDiff && fileDiff.diff ? (
                  <div>
                    <DiffBody text={fileDiff.diff} />
                    {fileDiff.truncated && (
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
            </>
          ) : (
            <>
              {selectedBrowseFile && (
                <div className="px-4 py-2 border-b theme-border flex items-center gap-2 flex-shrink-0">
                  <FileText className="h-3.5 w-3.5 theme-text-sub shrink-0" />
                  <span
                    className="text-xs font-mono font-semibold theme-text-main truncate"
                    title={selectedBrowseFile}
                  >
                    {selectedBrowseFile}
                  </span>
                  {isMarkdownFile && (
                    <div
                      className="flex items-center rounded-md border theme-border theme-bg-sub overflow-hidden shrink-0 ml-1"
                      title={t('gitBoard.mdModeTitle')}
                    >
                      <button
                        onClick={() => setMdMode('rendered')}
                        className={`px-2 py-0.5 text-[11px] font-medium transition-colors cursor-pointer ${
                          mdMode === 'rendered'
                            ? 'bg-black/[0.06] dark:bg-white/[0.08] theme-text-main'
                            : 'theme-text-muted hover:theme-text-main'
                        }`}
                      >
                        {t('gitBoard.mdPreview')}
                      </button>
                      <button
                        onClick={() => setMdMode('raw')}
                        className={`px-2 py-0.5 text-[11px] font-medium transition-colors cursor-pointer ${
                          mdMode === 'raw'
                            ? 'bg-black/[0.06] dark:bg-white/[0.08] theme-text-main'
                            : 'theme-text-muted hover:theme-text-main'
                        }`}
                      >
                        {t('gitBoard.mdRaw')}
                      </button>
                    </div>
                  )}
                  <div className="ml-auto flex items-center gap-1 shrink-0">
                    <button
                      onClick={() => revealPath(selectedBrowseFile)}
                      title={t('ws.revealInFolder')}
                      className={iconBtn}
                    >
                      <FolderOpen className="h-3.5 w-3.5" />
                    </button>
                    <OpenInIdeMenu
                      workspacePath={`${entry.workspace_path}/${selectedBrowseFile}`}
                    />
                  </div>
                </div>
              )}
              <div className="flex-1 overflow-auto min-h-0">
                {!selectedBrowseFile ? (
                  <div className="flex items-center justify-center py-16 text-xs theme-text-sub">
                    {t('gitBoard.browseSelectHint')}
                  </div>
                ) : browseLoading ? (
                  <div className="flex items-center justify-center py-16 gap-2">
                    <Loader2 className="h-5 w-5 animate-spin text-orange-500" />
                    <span className="text-xs theme-text-muted">{t('gitBoard.browseLoading')}</span>
                  </div>
                ) : browseError ? (
                  <div className="flex flex-col items-center justify-center py-16 gap-1 text-center">
                    <AlertCircle className="h-6 w-6 text-red-400" />
                    <div className="text-xs theme-text-muted">{t('gitBoard.browseError')}</div>
                    <div className="text-[10px] theme-text-sub font-mono max-w-md truncate">
                      {browseError}
                    </div>
                  </div>
                ) : browseContent?.binary ? (
                  <div className="flex items-center justify-center py-16 text-xs theme-text-sub">
                    {t('gitBoard.browseBinary')}
                  </div>
                ) : browseContent && browseContent.content ? (
                  <div>
                    {isMarkdownFile && mdMode === 'rendered' ? (
                      <div className="p-6 m-4 rounded-xl theme-bg-sub border theme-border markdown-body">
                        <ReactMarkdown remarkPlugins={[remarkGfm]}>
                          {browseContent.content}
                        </ReactMarkdown>
                      </div>
                    ) : (
                      <PlainBody text={browseContent.content} />
                    )}
                    {browseContent.truncated && (
                      <div className="text-[10px] theme-text-sub text-center py-2 border-t theme-border-sub">
                        {t('gitBoard.browseTruncated')}
                      </div>
                    )}
                  </div>
                ) : (
                  <div className="flex items-center justify-center py-16 text-xs theme-text-sub">
                    {t('gitBoard.browseEmpty')}
                  </div>
                )}
              </div>
            </>
          )}
        </div>
      </div>

      {/* 最近提交弹窗 */}
      {commitsOpen && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm animate-in fade-in duration-150"
          onClick={() => setCommitsOpen(false)}
        >
          <div
            className="w-full max-w-2xl max-h-[70vh] theme-bg-card border theme-border rounded-2xl shadow-2xl overflow-hidden flex flex-col animate-in zoom-in-95 duration-150"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="px-4 py-3 border-b theme-border flex items-center justify-between flex-shrink-0">
              <span className="text-sm font-bold theme-text-main">
                {t('gitBoard.commitsTitle')}
              </span>
              <button onClick={() => setCommitsOpen(false)} className={iconBtn}>
                <X className="h-4 w-4" />
              </button>
            </div>
            <div className="flex-1 overflow-y-auto min-h-0 px-4 pb-2">
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
                    <button
                      type="button"
                      key={`${c.short_id}-${c.time}`}
                      onClick={() => setViewCommit(c)}
                      title={t('gitBoard.commitViewTitle')}
                      className="w-full text-left flex items-start gap-2.5 py-2 border-b theme-border-sub last:border-b-0 cursor-pointer hover:bg-black/[0.03] dark:hover:bg-white/[0.04] transition-colors"
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
                    </button>
                  ))}
                </div>
              ) : (
                <div className="flex items-center justify-center py-10 text-xs theme-text-sub">
                  {t('gitBoard.commitsEmpty')}
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      {/* 历史提交查看器 */}
      {viewCommit && (
        <GitCommitViewer
          workspacePath={entry.workspace_path}
          commit={viewCommit}
          onClose={() => setViewCommit(null)}
        />
      )}

      {/* 文件树右键菜单 */}
      {ctxMenu &&
        createPortal(
          <div
            className="fixed inset-0 z-[60]"
            onClick={() => setCtxMenu(null)}
            onContextMenu={(e) => {
              e.preventDefault();
              setCtxMenu(null);
            }}
          >
            <div
              className="absolute theme-bg-card border theme-border rounded-lg shadow-xl py-1 min-w-44 max-w-80"
              style={{
                left: Math.min(ctxMenu.x, window.innerWidth - 200),
                top: Math.min(ctxMenu.y, window.innerHeight - 80),
              }}
            >
              <button
                onClick={() => {
                  const p = ctxMenu.path;
                  setCtxMenu(null);
                  void navigator.clipboard
                    .writeText(`${entry.workspace_path}/${p.replace(/\/+$/, '')}`)
                    .catch(() => {});
                }}
                className="w-full text-left px-3 py-1.5 text-xs theme-text-main hover:bg-black/[0.05] dark:hover:bg-white/[0.07] transition-colors cursor-pointer"
              >
                {t('ws.copyPath')}
              </button>
              <button
                onClick={() => {
                  const p = ctxMenu.path;
                  setCtxMenu(null);
                  revealPath(p);
                }}
                className="w-full text-left px-3 py-1.5 text-xs theme-text-main hover:bg-black/[0.05] dark:hover:bg-white/[0.07] transition-colors cursor-pointer"
              >
                {t('ws.revealInFolder')}
              </button>
              <button
                onClick={() => {
                  const p = ctxMenu.path;
                  setCtxMenu(null);
                  void handleAddIgnore(p);
                }}
                className="w-full text-left px-3 py-1.5 text-xs theme-text-main hover:bg-black/[0.05] dark:hover:bg-white/[0.07] transition-colors cursor-pointer"
              >
                {t('gitBoard.ctxAddIgnore')}
              </button>
              <div
                className="px-3 pt-1 pb-0.5 text-[10px] theme-text-sub font-mono truncate border-t theme-border-sub mt-1"
                title={ctxMenu.path}
              >
                {ctxMenu.path}
              </div>
            </div>
          </div>,
          document.body
        )}

      {/* 忽略操作提示 */}
      {ignoreHint &&
        createPortal(
          <div
            className="fixed bottom-6 left-1/2 -translate-x-1/2 z-[70] px-3 py-1.5 rounded-lg theme-bg-card border theme-border shadow-lg text-xs theme-text-main max-w-[80vw] truncate"
            title={ignoreHint}
          >
            {ignoreHint}
          </div>,
          document.body
        )}
    </div>
  );
}

export function GitBoardView({
  focusPath,
  embedPath,
  onExit,
  hideHeader = false,
}: {
  focusPath?: string | null;
  /** 单项目嵌入模式：只展示该仓库变更详情，隐藏看板左侧项目列表 */
  embedPath?: string | null;
  /** 退出看板，返回上一视图 */
  onExit?: () => void;
  /** 品字型项目页：Header 由 ProjectHeader 承担，这里只渲染 Git 树 */
  hideHeader?: boolean;
} = {}) {
  const { t } = useI18n();
  const isEmbed = Boolean(embedPath);
  const [days, setDays] = useState(30);
  const [data, setData] = useState<GitBoardResponse | null>(null);
  const [embedEntry, setEmbedEntry] = useState<GitBoardEntry | null>(null);
  const [embedLoading, setEmbedLoading] = useState(false);
  const [embedError, setEmbedError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedPath, setSelectedPath] = useState<string | null>(focusPath ?? null);
  const [hiddenPaths, setHiddenPathsState] = useState<string[]>(() => getHiddenPaths());
  const [showHiddenMgr, setShowHiddenMgr] = useState(false);
  const [embedRefreshKey, setEmbedRefreshKey] = useState(0);

  useEffect(() => onHiddenChange(() => setHiddenPathsState(getHiddenPaths())), []);

  // 从项目分析页跳入时，优先聚焦该工作区
  useEffect(() => {
    if (focusPath) setSelectedPath(focusPath);
  }, [focusPath]);

  // 嵌入模式：直接探测单仓库 Git 状态，不拉全量看板
  const loadEmbed = useCallback(async (path: string) => {
    setEmbedLoading(true);
    setEmbedError(null);
    try {
      const entry = await api.gitBoard.workspaceEntry(path);
      setEmbedEntry(entry);
    } catch (e) {
      console.error('Failed to load project git entry:', e);
      setEmbedError(String(e));
      setEmbedEntry(null);
    } finally {
      setEmbedLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!embedPath) return;
    void loadEmbed(embedPath);
  }, [embedPath, loadEmbed, embedRefreshKey]);

  const visibleEntries = useMemo(
    () => (data ? data.entries.filter((e) => !hiddenPaths.includes(e.workspace_path)) : []),
    [data, hiddenPaths]
  );

  // 从可见列表中选：优先保持已选项，否则回退到第一项
  const selected = useMemo(
    () =>
      visibleEntries.find((e) => e.workspace_path === selectedPath) ??
      visibleEntries[0] ??
      null,
    [visibleEntries, selectedPath]
  );

  const load = useCallback(async (d: number) => {
    setLoading(true);
    setError(null);
    try {
      const res = await api.gitBoard.get(d);
      setData(res);
      // 默认选中第一个；已选中/聚焦的项目在新列表中消失时回退到第一个
      setSelectedPath((prev) => {
        const preferred = focusPath ?? prev;
        if (preferred && res.entries.some((e) => e.workspace_path === preferred)) {
          return preferred;
        }
        return res.entries.some((e) => e.workspace_path === prev)
          ? prev
          : (res.entries[0]?.workspace_path ?? null);
      });
    } catch (e) {
      console.error('Failed to load git board:', e);
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [focusPath]);

  // 全局看板模式才加载列表；嵌入模式只探测单仓库
  useEffect(() => {
    if (isEmbed) {
      setLoading(false);
      return;
    }
    void load(days);
  }, [days, load, isEmbed]);

  // 聚焦的工作区若不在近 N 天活跃列表里，自动放宽到 365 天再找一次
  useEffect(() => {
    if (isEmbed || !data || !focusPath || loading) return;
    const found = data.entries.some((e) => e.workspace_path === focusPath);
    if (!found && days < 365) setDays(365);
  }, [isEmbed, data, focusPath, loading, days]);

  // 汇总数字按可见仓库重算（隐藏的不计入）
  const summary = useMemo(() => {
    if (!data) return null;
    return {
      ...data.summary,
      total: visibleEntries.length,
      repo_count: visibleEntries.length,
      dirty_count: visibleEntries.filter(
        (e) => e.staged + e.unstaged + e.untracked + e.conflicts > 0
      ).length,
      ahead_count: visibleEntries.filter((e) => (e.ahead ?? 0) > 0).length,
    };
  }, [data, visibleEntries]);

  const embedDisplayName =
    embedEntry?.display_name ||
    (embedPath ? embedPath.split('/').filter(Boolean).slice(-1)[0] || embedPath : '');

  // ── 单项目嵌入模式 ──
  if (isEmbed && embedPath) {
    const embedBody = (
      <>
        {embedError ? (
          <div className="h-full flex flex-col items-center justify-center gap-2 text-center">
            <AlertCircle className="h-8 w-8 text-red-400" />
            <div className="text-sm theme-text-muted">{t('gitBoard.error')}</div>
            <div className="text-xs theme-text-sub font-mono max-w-md truncate">{embedError}</div>
          </div>
        ) : embedLoading && !embedEntry ? (
          <div className="h-full flex flex-col items-center justify-center gap-2">
            <Loader2 className="h-6 w-6 animate-spin text-orange-500" />
            <div className="text-xs theme-text-muted">{t('common.loading')}</div>
          </div>
        ) : embedEntry && !embedEntry.is_repo ? (
          <div className="h-full flex flex-col items-center justify-center gap-2 text-center">
            <FolderGit2 className="h-8 w-8 theme-text-sub opacity-50" />
            <div className="text-sm theme-text-muted">{t('gitBoard.empty', { d: 30 })}</div>
            <div className="text-xs theme-text-sub font-mono max-w-md truncate">{embedPath}</div>
          </div>
        ) : embedEntry ? (
          <GitBoardDetail
            entry={embedEntry}
            onChanged={() => setEmbedRefreshKey((k) => k + 1)}
            hideHeader={hideHeader}
          />
        ) : null}
      </>
    );

    // 项目页：Header 外置，下方直接是 Git 树
    if (hideHeader) {
      return (
        <div className="flex-1 flex flex-col h-full overflow-hidden min-h-0 theme-bg-main">
          <div className="flex-1 min-h-0 overflow-hidden px-4 pt-3 pb-4">{embedBody}</div>
        </div>
      );
    }

    return (
      <div className="flex-1 flex flex-col h-full overflow-hidden theme-bg-main">
        <div className="px-6 py-3 border-b theme-border flex items-center justify-between gap-4 flex-shrink-0">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              {onExit && (
                <button
                  onClick={onExit}
                  title={t('gitBoard.back')}
                  className="p-1.5 rounded-lg theme-text-muted hover:theme-text-main hover:bg-black/5 dark:hover:bg-white/5 transition-colors cursor-pointer flex-shrink-0"
                >
                  <ArrowLeft className="h-4 w-4" />
                </button>
              )}
              <GitBranch className="h-5 w-5 text-orange-500" />
              <h1 className="text-lg font-bold theme-text-main truncate">
                {t('nav.gitBoard')} · {embedDisplayName}
              </h1>
            </div>
            <p className="text-xs theme-text-muted mt-0.5 font-mono truncate" title={embedPath}>
              {embedPath}
            </p>
          </div>
          <button
            onClick={() => setEmbedRefreshKey((k) => k + 1)}
            disabled={embedLoading}
            className="flex items-center gap-1.5 px-2.5 py-1.5 text-xs theme-bg-sub hover:opacity-90 border theme-border rounded-lg theme-text-muted hover:theme-text-main transition-colors cursor-pointer shadow-sm disabled:opacity-50 flex-shrink-0"
          >
            {embedLoading ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin text-orange-500" />
            ) : (
              <RefreshCw className="h-3.5 w-3.5 text-orange-500" />
            )}
            <span>{t('gitBoard.refresh')}</span>
          </button>
        </div>

        <div className="flex-1 min-h-0 px-6 pt-4 pb-6 overflow-hidden">{embedBody}</div>
      </div>
    );
  }

  return (
    <div className="flex-1 flex flex-col h-full overflow-hidden theme-bg-main">
      {/* 工具栏 */}
      <div className="px-6 py-4 border-b theme-border flex items-center justify-between gap-4 flex-shrink-0">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            {onExit && (
              <button
                onClick={onExit}
                title={t('gitBoard.back')}
                className="p-1.5 rounded-lg theme-text-muted hover:theme-text-main hover:bg-black/5 dark:hover:bg-white/5 transition-colors cursor-pointer flex-shrink-0"
              >
                <ArrowLeft className="h-4 w-4" />
              </button>
            )}
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
          <div
            className="flex items-center gap-1.5"
            title={t('gitBoard.daysTitle')}
          >
            <span className="text-[11px] theme-text-sub">{t('gitBoard.daysLabel')}</span>
            <div className="flex items-center rounded-lg border theme-border theme-bg-sub overflow-hidden">
              {DAY_OPTIONS.map((d) => (
                <button
                  key={d}
                  onClick={() => setDays(d)}
                  className={`px-2.5 py-1 text-xs font-medium transition-colors cursor-pointer ${
                    days === d
                      ? 'bg-black/[0.06] dark:bg-white/[0.08] theme-text-main'
                      : 'theme-text-muted hover:theme-text-main'
                  }`}
                >
                  {t('gitBoard.days', { d })}
                </button>
              ))}
            </div>
          </div>
          {hiddenPaths.length > 0 && (
            <button
              onClick={() => setShowHiddenMgr(true)}
              className="flex items-center gap-1.5 px-2.5 py-1.5 text-xs theme-bg-sub hover:opacity-90 border theme-border rounded-lg theme-text-muted hover:theme-text-main transition-colors cursor-pointer shadow-sm"
            >
              <EyeOff className="h-3.5 w-3.5" />
              <span>{t('gitBoard.hiddenN', { n: hiddenPaths.length })}</span>
            </button>
          )}
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

      {/* 主体：左侧项目列表 + 右侧详情 */}
      <div className="flex-1 flex overflow-hidden px-6 pt-4 pb-6 min-h-0">
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
        ) : data && visibleEntries.length === 0 ? (
          <div className="flex-1 flex flex-col items-center justify-center py-16 gap-2 text-center">
            <EyeOff className="h-8 w-8 theme-text-sub opacity-50" />
            <div className="text-sm theme-text-muted">{t('gitBoard.allHiddenHint')}</div>
          </div>
        ) : (
          <>
            {/* 左：项目列表 */}
            <div className="w-56 lg:w-60 flex-shrink-0 flex flex-col min-h-0 pr-1">
              <div className="flex-1 overflow-y-auto min-h-0 rounded-xl border theme-border theme-bg-card">
                {visibleEntries.map((entry, i) => (
                  <GitBoardListItem
                    key={entry.workspace_path}
                    entry={entry}
                    active={entry.workspace_path === selected?.workspace_path}
                    divider={i > 0}
                    onSelect={() => setSelectedPath(entry.workspace_path)}
                    onHide={() => hideWorkspace(entry.workspace_path)}
                  />
                ))}
              </div>
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

      {/* 隐藏工作区管理 */}
      {showHiddenMgr && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm animate-in fade-in duration-150"
          onClick={() => setShowHiddenMgr(false)}
        >
          <div
            className="w-full max-w-md max-h-[60vh] theme-bg-card border theme-border rounded-2xl shadow-2xl overflow-hidden flex flex-col animate-in zoom-in-95 duration-150"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="px-4 py-3 border-b theme-border flex items-center justify-between flex-shrink-0">
              <span className="text-sm font-bold theme-text-main">
                {t('gitBoard.hiddenTitle')}
              </span>
              <button
                onClick={() => setShowHiddenMgr(false)}
                className="p-1 rounded-md theme-text-muted hover:theme-text-main hover:bg-black/5 dark:hover:bg-white/5 transition-colors cursor-pointer"
              >
                <X className="h-4 w-4" />
              </button>
            </div>
            <div className="flex-1 overflow-y-auto min-h-0 py-1">
              {hiddenPaths.length === 0 ? (
                <div className="px-4 py-6 text-xs theme-text-sub text-center">
                  {t('gitBoard.hiddenEmpty')}
                </div>
              ) : (
                hiddenPaths.map((p) => (
                  <div key={p} className="flex items-center gap-2 px-4 py-2">
                    <span
                      className="text-xs theme-text-main truncate flex-1 font-mono"
                      title={p}
                    >
                      {p.split('/').slice(-1)[0] || p}
                    </span>
                    <button
                      onClick={() => unhideWorkspace(p)}
                      className="px-2 py-1 rounded-md text-[11px] theme-bg-sub border theme-border theme-text-muted hover:theme-text-main transition-colors cursor-pointer flex-shrink-0"
                    >
                      {t('gitBoard.unhide')}
                    </button>
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      )}
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

/** 连续未变更行数超过该阈值时折叠，仅保留首尾 EDGE 行 */
const DIFF_CTX_COLLAPSE_THRESHOLD = 10;
const DIFF_CTX_EDGE = 4;

function DiffBody({ text }: { text: string }) {
  const { t } = useI18n();
  const lines = useMemo(() => parseDiffText(text), [text]);
  const [expanded, setExpanded] = useState<Record<number, boolean>>({});
  useEffect(() => {
    setExpanded({});
  }, [text]);

  // 预计算每个折叠区段：index → { start, end }（连续 ctx 行且长度超阈值）
  const runs = useMemo(() => {
    const map = new Map<number, { start: number; end: number }>();
    let i = 0;
    while (i < lines.length) {
      if (lines[i].kind === 'ctx') {
        let j = i;
        while (j < lines.length && lines[j].kind === 'ctx') j++;
        if (j - i > DIFF_CTX_COLLAPSE_THRESHOLD) {
          for (let k = i; k < j; k++) map.set(k, { start: i, end: j });
        }
        i = j;
      } else {
        i++;
      }
    }
    return map;
  }, [lines]);

  return (
    <div className="font-mono text-xs leading-5">
      {lines.map((l, i) => {
        const run = l.kind === 'ctx' ? runs.get(i) : undefined;
        if (run && !expanded[run.start]) {
          const { start, end } = run;
          if (i > start + DIFF_CTX_EDGE - 1 && i < end - DIFF_CTX_EDGE) {
            if (i === start + DIFF_CTX_EDGE) {
              return (
                <button
                  key={i}
                  type="button"
                  onClick={() => setExpanded((p) => ({ ...p, [start]: true }))}
                  className="w-full text-center py-1 text-[10px] theme-text-sub hover:theme-text-main hover:bg-black/[0.04] dark:hover:bg-white/[0.05] cursor-pointer select-none"
                >
                  ⋯ {t('gitBoard.diffExpand', { n: end - start - 2 * DIFF_CTX_EDGE })} ⋯
                </button>
              );
            }
            return null;
          }
        }
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

/* ===================== 文件树 ===================== */

const iconBtn =
  'p-1 rounded-md theme-text-muted hover:theme-text-main hover:bg-black/5 dark:hover:bg-white/5 transition-colors cursor-pointer disabled:opacity-30 disabled:cursor-default shrink-0';

interface TreeDirNode {
  name: string;
  path: string;
  dirs: TreeDirNode[];
  files: GitStatusFile[];
}

/** 取路径显示名：兼容 `a/b/bin/`（尾斜杠未跟踪目录） */
function pathBasename(p: string): string {
  const cleaned = p.replace(/\/+$/, '');
  if (!cleaned) return p || '/';
  return cleaned.split('/').filter(Boolean).slice(-1)[0] || cleaned;
}

/** 把平铺文件列表按目录分组为树：目录按字母序，文件保留后端的状态排序。 */
function buildFileTree(files: GitStatusFile[]): TreeDirNode {
  const root: TreeDirNode = { name: '', path: '', dirs: [], files: [] };
  for (const file of files) {
    const cleaned = file.path.replace(/\/+$/, '');
    if (!cleaned) continue;
    const parts = cleaned.split('/').filter(Boolean);
    let cur = root;
    for (let i = 0; i < parts.length - 1; i++) {
      const seg = parts[i];
      const path = cur.path ? `${cur.path}/${seg}` : seg;
      let next = cur.dirs.find((d) => d.path === path);
      if (!next) {
        next = { name: seg, path, dirs: [], files: [] };
        cur.dirs.push(next);
      }
      cur = next;
    }
    cur.files.push(file);
  }
  const sortDirs = (n: TreeDirNode) => {
    n.dirs.sort((a, b) => a.name.localeCompare(b.name));
    n.dirs.forEach(sortDirs);
  };
  sortDirs(root);
  return root;
}

function FileTreeNode({
  node,
  depth,
  collapsed,
  onToggle,
  selected,
  onSelect,
  onContext,
  onStage,
  stagingPath,
}: {
  node: TreeDirNode;
  depth: number;
  collapsed: Record<string, boolean>;
  onToggle: (path: string) => void;
  selected: string | null;
  onSelect: (path: string) => void;
  onContext: (e: React.MouseEvent, path: string) => void;
  onStage?: (path: string, stage: boolean) => void;
  stagingPath?: string | null;
}) {
  const { t } = useI18n();
  return (
    <>
      {node.dirs.map((d) => (
        <div key={d.path}>
          <button
            type="button"
            style={{ paddingLeft: 8 + depth * 12 }}
            onClick={() => onToggle(d.path)}
            onContextMenu={(e) => onContext(e, d.path)}
            className="flex items-center gap-1.5 w-full text-left pr-2 py-1 hover:bg-black/[0.03] dark:hover:bg-white/[0.04] transition-colors cursor-pointer"
          >
            {collapsed[d.path] ? (
              <ChevronRight className="h-3 w-3 theme-text-sub shrink-0" />
            ) : (
              <ChevronDown className="h-3 w-3 theme-text-sub shrink-0" />
            )}
            <Folder className="h-3.5 w-3.5 theme-text-sub shrink-0" />
            <span className="text-[11px] theme-text-muted truncate font-mono">{d.name}</span>
          </button>
          {!collapsed[d.path] && (
            <FileTreeNode
              node={d}
              depth={depth + 1}
              collapsed={collapsed}
              onToggle={onToggle}
              selected={selected}
              onSelect={onSelect}
              onContext={onContext}
              onStage={onStage}
              stagingPath={stagingPath}
            />
          )}
        </div>
      ))}
      {node.files.map((file) => (
        <div
          key={file.path}
          style={{ paddingLeft: 8 + (depth + 1) * 12 }}
          onClick={() => onSelect(file.path)}
          onContextMenu={(e) => onContext(e, file.path)}
          className={`group relative flex items-center gap-1.5 w-full text-left pr-1.5 py-1 transition-colors cursor-pointer ${
            selected === file.path
              ? 'bg-black/[0.06] dark:bg-white/[0.08]'
              : 'hover:bg-black/[0.03] dark:hover:bg-white/[0.04]'
          }`}
        >
          <FileStatusGlyph f={file} />
          <span
            className={`text-[11px] truncate font-mono flex-1 min-w-0 ${
              selected === file.path ? 'theme-text-main' : 'theme-text-muted'
            }`}
            title={file.old_path ? `${file.old_path} → ${file.path}` : file.path}
          >
            {pathBasename(file.path)}
          </span>
          {onStage && (file.worktree || file.staged) && (
            <span className="hidden group-hover:flex items-center gap-0.5 shrink-0">
              {file.worktree && (
                <button
                  type="button"
                  title={t('gitBoard.stageFile')}
                  disabled={stagingPath === file.path}
                  onClick={(e) => {
                    e.stopPropagation();
                    onStage(file.path, true);
                  }}
                  className="p-0.5 rounded theme-text-muted hover:theme-text-main hover:bg-black/10 dark:hover:bg-white/10 transition-colors cursor-pointer disabled:opacity-50"
                >
                  {stagingPath === file.path ? (
                    <Loader2 className="h-3 w-3 animate-spin" />
                  ) : (
                    <Plus className="h-3 w-3 text-emerald-500" />
                  )}
                </button>
              )}
              {file.staged && (
                <button
                  type="button"
                  title={t('gitBoard.unstageFile')}
                  disabled={stagingPath === file.path}
                  onClick={(e) => {
                    e.stopPropagation();
                    onStage(file.path, false);
                  }}
                  className="p-0.5 rounded theme-text-muted hover:theme-text-main hover:bg-black/10 dark:hover:bg-white/10 transition-colors cursor-pointer disabled:opacity-50"
                >
                  {stagingPath === file.path ? (
                    <Loader2 className="h-3 w-3 animate-spin" />
                  ) : (
                    <Minus className="h-3 w-3 text-amber-500" />
                  )}
                </button>
              )}
            </span>
          )}
        </div>
      ))}
    </>
  );
}

/* ===================== 全部文件树（懒加载浏览） ===================== */

/** 纯文本内容预览：单行号栏，与 DiffBody 视觉对齐 */
function PlainBody({ text }: { text: string }) {
  const lines = useMemo(() => text.split('\n'), [text]);
  return (
    <div className="font-mono text-xs leading-5">
      {lines.map((l, i) => (
        <div key={i} className="flex theme-text-muted">
          <span className="w-12 shrink-0 text-right pr-2 select-none opacity-40 border-r theme-border-sub mr-3">
            {i + 1}
          </span>
          <span className="flex-1 min-w-0 whitespace-pre-wrap break-all pr-4">
            {l || '\u00A0'}
          </span>
        </div>
      ))}
    </div>
  );
}

/** 懒加载文件树节点：目录点击展开时才拉取子层 */
function BrowseTreeNode({
  entries,
  depth,
  dirChildren,
  dirLoading,
  expandedDirs,
  statusByPath,
  selected,
  onToggleDir,
  onSelectFile,
  onContext,
}: {
  entries: WorkspaceDirEntry[];
  depth: number;
  dirChildren: Record<string, WorkspaceDirEntry[]>;
  dirLoading: Record<string, boolean>;
  expandedDirs: Record<string, boolean>;
  statusByPath: Map<string, GitStatusFile>;
  selected: string | null;
  onToggleDir: (path: string) => void;
  onSelectFile: (path: string) => void;
  onContext: (e: React.MouseEvent, path: string) => void;
}) {
  return (
    <>
      {entries.map((e) =>
        e.is_dir ? (
          <div key={e.path}>
            <button
              type="button"
              style={{ paddingLeft: 8 + depth * 12 }}
              onClick={() => onToggleDir(e.path)}
              onContextMenu={(ev) => onContext(ev, e.path)}
              className="flex items-center gap-1.5 w-full text-left pr-2 py-1 hover:bg-black/[0.03] dark:hover:bg-white/[0.04] transition-colors cursor-pointer"
            >
              {expandedDirs[e.path] ? (
                <ChevronDown className="h-3 w-3 theme-text-sub shrink-0" />
              ) : (
                <ChevronRight className="h-3 w-3 theme-text-sub shrink-0" />
              )}
              <Folder className="h-3.5 w-3.5 theme-text-sub shrink-0" />
              <span className="text-[11px] theme-text-muted truncate font-mono">{e.name}</span>
              {dirLoading[e.path] && (
                <Loader2 className="h-3 w-3 animate-spin theme-text-sub shrink-0" />
              )}
            </button>
            {expandedDirs[e.path] && dirChildren[e.path] && (
              <BrowseTreeNode
                entries={dirChildren[e.path]}
                depth={depth + 1}
                dirChildren={dirChildren}
                dirLoading={dirLoading}
                expandedDirs={expandedDirs}
                statusByPath={statusByPath}
                selected={selected}
                onToggleDir={onToggleDir}
                onSelectFile={onSelectFile}
                onContext={onContext}
              />
            )}
          </div>
        ) : (
          <button
            type="button"
            key={e.path}
            style={{ paddingLeft: 8 + (depth + 1) * 12 }}
            onClick={() => onSelectFile(e.path)}
            onContextMenu={(ev) => onContext(ev, e.path)}
            className={`flex items-center gap-1.5 w-full text-left pr-2 py-1 transition-colors cursor-pointer ${
              selected === e.path
                ? 'bg-black/[0.06] dark:bg-white/[0.08]'
                : 'hover:bg-black/[0.03] dark:hover:bg-white/[0.04]'
            }`}
          >
            {statusByPath.get(e.path) ? (
              <FileStatusGlyph f={statusByPath.get(e.path)!} />
            ) : (
              <span className="w-5 shrink-0" />
            )}
            <FileText className="h-3.5 w-3.5 theme-text-sub shrink-0" />
            <span
              className={`text-[11px] truncate font-mono ${
                selected === e.path ? 'theme-text-main' : 'theme-text-muted'
              }`}
              title={e.path}
            >
              {e.name}
            </span>
          </button>
        )
      )}
    </>
  );
}

/* ===================== 历史提交查看器 ===================== */

const commitStatusTones: Record<string, string> = {
  A: 'text-emerald-500',
  M: 'text-amber-500',
  D: 'text-red-500',
  R: 'text-sky-500',
  T: 'text-zinc-400',
};

/**
 * 单次历史提交的变更审查器：左侧该提交变更的文件清单，右侧选中文件的 diff。
 * 布局与未提交 diff 审查器一致；↑↓ 切换文件，Esc 关闭。
 */
function GitCommitViewer({
  workspacePath,
  commit,
  onClose,
}: {
  workspacePath: string;
  commit: GitCommitInfo;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [show, setShow] = useState<GitCommitShowResponse | null>(null);
  const [showError, setShowError] = useState<string | null>(null);
  const [index, setIndex] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setShow(null);
    setShowError(null);
    setIndex(0);
    api.gitBoard
      .commitShow(workspacePath, commit.id)
      .then((res) => {
        if (!cancelled) setShow(res);
      })
      .catch((e) => {
        if (!cancelled) setShowError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [workspacePath, commit.id]);

  const files = show?.files ?? [];

  // 文件清单加载完成后索引越界时收敛
  useEffect(() => {
    if (index > files.length - 1) setIndex(Math.max(0, files.length - 1));
  }, [files.length, index]);

  const file = files[index] ?? null;

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
      .commitFileDiff(workspacePath, commit.id, file.path)
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
  }, [workspacePath, commit.id, file]);

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
        {/* 左：变更文件清单 */}
        <div className="w-72 flex-shrink-0 border-r theme-border flex flex-col min-h-0">
          <div className="px-4 py-3 border-b theme-border flex items-center justify-between gap-2 flex-shrink-0">
            <span className="text-xs font-semibold theme-text-muted">
              {t('gitBoard.commitFilesTitle')}
            </span>
            <span className="text-[10px] theme-text-sub tabular-nums">
              {files.length > 0 ? `${index + 1} / ${files.length}` : ''}
            </span>
          </div>
          <div className="flex-1 overflow-y-auto min-h-0 py-1">
            {showError ? (
              <div className="px-4 py-6 text-xs text-red-400">
                {t('gitBoard.commitShowError')}
              </div>
            ) : !show ? (
              <div className="flex items-center justify-center py-6">
                <Loader2 className="h-4 w-4 animate-spin text-orange-500" />
              </div>
            ) : files.length === 0 ? (
              <div className="px-4 py-6 text-xs theme-text-sub">
                {t('gitBoard.commitFilesEmpty')}
              </div>
            ) : (
              files.map((f, i) => (
                <div
                  key={`${f.path}-${f.status}`}
                  onClick={() => setIndex(i)}
                  className={`flex items-center gap-2 px-3 py-1.5 cursor-pointer transition-colors ${
                    i === index
                      ? 'bg-black/[0.07] dark:bg-white/[0.09]'
                      : 'hover:bg-black/5 dark:hover:bg-white/5'
                  }`}
                >
                  <span
                    className={`font-mono text-[10px] font-bold w-4 text-center flex-shrink-0 ${
                      commitStatusTones[f.status] ?? 'theme-text-muted'
                    }`}
                  >
                    {f.status}
                  </span>
                  <span
                    className={`text-xs truncate font-mono ${
                      i === index ? 'theme-text-main' : 'theme-text-muted'
                    }`}
                    title={f.path}
                  >
                    {f.path}
                  </span>
                </div>
              ))
            )}
          </div>
        </div>

        {/* 右：选中文件的 diff */}
        <div className="flex-1 min-w-0 flex flex-col">
          <div className="px-4 py-3 border-b theme-border flex-shrink-0">
            <div className="flex items-center gap-2">
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
              <span className="font-mono text-xs text-orange-500 flex-shrink-0 ml-1">
                {show?.short_id ?? commit.short_id}
              </span>
              <span
                className="text-sm font-bold theme-text-main truncate"
                title={show?.message ?? commit.message}
              >
                {show?.message ?? commit.message}
              </span>
              <button
                onClick={onClose}
                className="ml-auto p-1 rounded-md theme-text-muted hover:theme-text-main hover:bg-black/5 dark:hover:bg-white/5 transition-colors cursor-pointer shrink-0"
              >
                <X className="h-4 w-4" />
              </button>
            </div>
            {(show?.author || show?.time) && (
              <div className="text-[10px] theme-text-sub mt-1 truncate">
                {show?.author && <span>{show.author} · </span>}
                {show?.time && <span>{formatBeijingTime(show.time)}</span>}
              </div>
            )}
          </div>
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
                <div className="text-[10px] theme-text-sub font-mono max-w-md truncate">
                  {error}
                </div>
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
