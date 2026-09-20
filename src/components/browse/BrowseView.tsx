import React, { useState, useEffect, useRef } from 'react';
import type { WorkspaceStat, ConversationItem, MessageItem, DashboardStats, ArtifactItem } from '../../types';
import { api, isTauri } from '../../api/tauriBridge';
import { formatBeijingTime, formatRelativeTime } from '../../utils/date';
import { useI18n } from '../../i18n';
import { listen } from '@tauri-apps/api/event';
import { WorkspaceAnalysisView } from './WorkspaceAnalysisView';
import { ProjectHeader } from './ProjectHeader';
import { OpenInIdeMenu } from './OpenInIdeMenu';
import { WorkspaceMoreMenu } from './WorkspaceMoreMenu';
import { DashboardView } from '../dashboard/DashboardView';
import { PromptLibraryView } from '../prompts/PromptLibraryView';
import { ArtifactsModal } from './ArtifactsModal';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import {
  LayoutDashboard,
  Star,
  BookMarked,
  Server,
  Search,
  MessageSquare,
  Wrench,
  Clock,
  Sparkles,
  ArrowLeft,
  ArrowUpDown,
  Maximize2,
 Minimize2,
 X,
  FileText,
  Copy,
  Check,
  Pencil,
  AlignLeft,
  Layers,
  Loader2,
  StopCircle,
  CheckCircle2,
  FolderInput,
  GitBranch,
} from 'lucide-react';
import { ServicesView } from '../services/ServicesView';
import { GitBoardView } from '../gitboard/GitBoardView';
import {
  clearConversationAiTitle,
  renameConversationTitle,
  summarizeConversation,
} from '../../lib/conversationAi';
import {
  DEFAULT_BATCH_SUMMARIZE_OPTIONS,
  estimateBatchSeconds,
  fetchConversationMessages,
  getBatchConcurrency,
  runBatchSummarize,
  selectConversationsForBatch,
  type BatchProgress,
  type BatchSummarizeOptions,
} from '../../lib/batchSummarizeConversations';
import { getServiceLlmConfig } from '../../lib/serviceLlm';

interface Props {
  selectedWorkspace: string;
  selectedConversationId: string;
  isStarredView: boolean;
  isPromptLibraryView: boolean;
  isServicesView: boolean;
  isGitBoardView: boolean;
  promptLibraryCount: number;
  onSelectWorkspace: (ws: string) => void;
  onSelectConversation: (id: string) => void;
  onSwitchToDashboard: () => void;
  onSwitchToStarred: () => void;
  onSwitchToPromptLibrary: () => void;
  onSwitchToServices: () => void;
  /** 从项目分析页跳到该项目的 Git 看板详情 */
  onOpenGitBoard: (wsPath: string) => void;
  onPromptLibraryCountChange: (count: number) => void;
  stats: DashboardStats | null;
  loadingStats: boolean;
  onRefreshStats: () => void;
}

export const BrowseView: React.FC<Props> = ({
  selectedWorkspace,
  selectedConversationId,
  isStarredView,
  isPromptLibraryView,
  isServicesView,
  isGitBoardView,
  promptLibraryCount,
  onSelectWorkspace,
  onSelectConversation,
  onSwitchToDashboard,
  onSwitchToStarred,
  onSwitchToPromptLibrary,
  onSwitchToServices,
  onOpenGitBoard,
  onPromptLibraryCountChange,
  stats,
  loadingStats,
  onRefreshStats,
}) => {
  const { t } = useI18n();
  const [workspaces, setWorkspaces] = useState<WorkspaceStat[]>([]);
  /** 各工作区未提交文件数（git dirty），仅 >0 时展示 */
  const [wsGitDirty, setWsGitDirty] = useState<Record<string, number>>({});
  const [conversations, setConversations] = useState<ConversationItem[]>([]);
  const [messages, setMessages] = useState<MessageItem[]>([]);
  const [currentConv, setCurrentConv] = useState<ConversationItem | null>(null);

  const [wsSearch, setWsSearch] = useState('');
  // 工作区合并 / 重命名
  const [mergeSource, setMergeSource] = useState<WorkspaceStat | null>(null);
  const [mergeMode, setMergeMode] = useState<'existing' | 'rename'>('existing');
  const [mergeTarget, setMergeTarget] = useState('');
  const [mergeBusy, setMergeBusy] = useState(false);
  const [mergeToast, setMergeToast] = useState<string | null>(null);
  // 会话移动到其他工作区
  const [moveConv, setMoveConv] = useState<ConversationItem | null>(null);
  const [moveConvTarget, setMoveConvTarget] = useState('');
  const [moveConvBusy, setMoveConvBusy] = useState(false);
  const [convSearch, setConvSearch] = useState('');
  const [loadingConv, setLoadingConv] = useState(false);
  const [starredCount, setStarredCount] = useState(0);
  const [expandedTurns, setExpandedTurns] = useState<Record<number, boolean>>({});
  const [sortOrder, setSortOrder] = useState<'asc' | 'desc'>('desc');
  const [lightboxImg, setLightboxImg] = useState<string | null>(null);
  const [artifacts, setArtifacts] = useState<ArtifactItem[]>([]);
  const [selectedArtifact, setSelectedArtifact] = useState<ArtifactItem | null>(null);
  const [showArtifactModal, setShowArtifactModal] = useState(false);
  const [copiedTurnIndex, setCopiedTurnIndex] = useState<number | null>(null);
  const copiedResetTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [renaming, setRenaming] = useState(false);
  const [renameDraft, setRenameDraft] = useState('');
  const [renameSaving, setRenameSaving] = useState(false);
  const [summarizing, setSummarizing] = useState(false);
  const [summaryOpen, setSummaryOpen] = useState(false);
  const [aiActionError, setAiActionError] = useState<string | null>(null);
  const [batchConfirmOpen, setBatchConfirmOpen] = useState(false);
  const [batchOpts, setBatchOpts] = useState<BatchSummarizeOptions>(
    () => ({ ...DEFAULT_BATCH_SUMMARIZE_OPTIONS }),
  );
  const [batchProgress, setBatchProgress] = useState<BatchProgress | null>(null);
  const [batchPanelOpen, setBatchPanelOpen] = useState(false);
  const [batchError, setBatchError] = useState<string | null>(null);
  const [batchRunning, setBatchRunning] = useState(false);
  const batchCancelRef = useRef(false);

  const parseImages = (raw: any): Array<{ src: string; width?: number; height?: number }> => {
    if (!raw) return [];
    let list = raw;
    if (typeof raw === 'string') {
      try {
        list = JSON.parse(raw);
      } catch {
        return [];
      }
    }
    if (!Array.isArray(list)) return [];
    return list
      .map((item: any) => {
        if (typeof item === 'string') {
          const src = item.startsWith('/') ? `http://127.0.0.1:8788${item}` : item;
          return { src };
        }
        if (item && typeof item.src === 'string') {
          const src = item.src.startsWith('/') ? `http://127.0.0.1:8788${item.src}` : item.src;
          return { ...item, src };
        }
        return null;
      })
      .filter((x): x is { src: string; width?: number; height?: number } => Boolean(x && x.src));
  };

  const getMatchedArtifactsForMessage = (
    text: string | undefined,
    msgTime: string | undefined,
    allArts: ArtifactItem[]
  ): ArtifactItem[] => {
    if (!text || allArts.length === 0) return [];

    // 1. 如果文本中精确提到了某个特定产物文件名（如 implementation_plan.v1.md）
    const exactMatches = allArts.filter((art) => text.includes(art.file_name));
    if (exactMatches.length > 0) {
      return exactMatches;
    }

    // 2. 如果包含通用 implementation_plan
    if (text.includes('implementation_plan')) {
      const planArts = allArts.filter((a) => a.file_name.startsWith('implementation_plan'));
      if (planArts.length > 0) {
        if (msgTime) {
          const priorPlans = planArts
            .filter((a) => a.created_at && a.created_at <= msgTime)
            .sort((a, b) => (b.created_at || '').localeCompare(a.created_at || ''));
          if (priorPlans.length > 0) {
            return [priorPlans[0]];
          }
        }
        return [planArts[0]];
      }
    }

    // 3. 如果包含通用 walkthrough
    if (text.includes('walkthrough')) {
      const walkArts = allArts.filter((a) => a.file_name.startsWith('walkthrough'));
      if (walkArts.length > 0) {
        if (msgTime) {
          const priorWalks = walkArts
            .filter((a) => a.created_at && a.created_at <= msgTime)
            .sort((a, b) => (b.created_at || '').localeCompare(a.created_at || ''));
          if (priorWalks.length > 0) {
            return [priorWalks[0]];
          }
        }
        return [walkArts[0]];
      }
    }

    return [];
  };

  // 切换会话时重置折叠状态与 AI 编辑态
  useEffect(() => {
    setExpandedTurns({});
    setCopiedTurnIndex(null);
    setRenaming(false);
    setRenameDraft('');
    setAiActionError(null);
    setSummaryOpen(false);
  }, [selectedConversationId]);

  useEffect(() => {
    if (currentConv?.ai_summary) {
      setSummaryOpen(true);
    }
  }, [currentConv?.id, currentConv?.ai_summary]);

  useEffect(() => {
    return () => {
      if (copiedResetTimerRef.current) clearTimeout(copiedResetTimerRef.current);
    };
  }, []);

  const selectedWorkspaceRef = useRef(selectedWorkspace);
  const convSearchRef = useRef(convSearch);
  const isStarredViewRef = useRef(isStarredView);
  const wsSearchRef = useRef(wsSearch);

  useEffect(() => {
    selectedWorkspaceRef.current = selectedWorkspace;
  }, [selectedWorkspace]);

  useEffect(() => {
    convSearchRef.current = convSearch;
  }, [convSearch]);

  useEffect(() => {
    isStarredViewRef.current = isStarredView;
  }, [isStarredView]);

  useEffect(() => {
    wsSearchRef.current = wsSearch;
  }, [wsSearch]);

  const loadingWsRef = useRef(false);
  // 加载工作区列表（带并发锁）
  const loadWorkspaces = async () => {
    if (loadingWsRef.current) return;
    loadingWsRef.current = true;
    try {
      const list = await api.listWorkspaces(wsSearchRef.current);
      setWorkspaces(list);
    } catch (e) {
      console.error(e);
    } finally {
      loadingWsRef.current = false;
    }
  };

  /** 拉取各项目未提交文件数（复用 Git 看板探测，365 天窗口覆盖冷项目） */
  const loadWsGitDirty = async () => {
    try {
      const res = await api.gitBoard.get(365);
      const map: Record<string, number> = {};
      for (const e of res.entries || []) {
        const n =
          (e.staged || 0) + (e.unstaged || 0) + (e.untracked || 0) + (e.conflicts || 0);
        if (n > 0 && e.workspace_path) map[e.workspace_path] = n;
      }
      setWsGitDirty(map);
    } catch (e) {
      console.error('loadWsGitDirty failed:', e);
    }
  };

  // 离开 Git 模式后刷新侧栏 dirty 计数（提交/推送后可能已变化）
  useEffect(() => {
    if (!isGitBoardView) void loadWsGitDirty();
  }, [isGitBoardView]);

  useEffect(() => {
    void loadWorkspaces();
  }, [wsSearch]);

  // 首次进入加载侧栏未提交数
  useEffect(() => {
    void loadWsGitDirty();
  }, []);

  const handleMergeWorkspace = async () => {
    if (!mergeSource || !mergeTarget.trim()) return;
    setMergeBusy(true);
    try {
      const result = await api.mergeWorkspace(mergeSource.workspace_path, mergeTarget.trim());
      setMergeToast(
        t('ws.mergeSuccess', {
          n: result.moved_conversations,
          target: result.target_path.split('/').slice(-1)[0] || result.target_path,
        })
      );
      setMergeSource(null);
      setMergeTarget('');
      await loadWorkspaces();
      onRefreshStats?.();
      // 若正浏览源工作区，跳到合并后的目标
      if (selectedWorkspace === result.source_path) {
        onSelectWorkspace(result.target_path);
        onSelectConversation('');
      }
    } catch (e) {
      console.error(e);
      setMergeToast(t('ws.mergeFailed'));
    } finally {
      setMergeBusy(false);
    }
  };

  useEffect(() => {
    if (!mergeToast) return;
    const id = window.setTimeout(() => setMergeToast(null), 3500);
    return () => window.clearTimeout(id);
  }, [mergeToast]);

  const handleMoveConversation = async () => {
    if (!moveConv || !moveConvTarget.trim()) return;
    setMoveConvBusy(true);
    try {
      const result = await api.moveConversation(moveConv.id, moveConvTarget.trim());
      setMergeToast(
        t('conv.moveSuccess', {
          target: result.target_path.split('/').slice(-1)[0] || result.target_path,
        })
      );
      setMoveConv(null);
      setMoveConvTarget('');
      await loadConversations();
      await loadWorkspaces();
      onRefreshStats?.();
    } catch (e) {
      console.error(e);
      setMergeToast(t('conv.moveFailed'));
    } finally {
      setMoveConvBusy(false);
    }
  };

  const loadingConvsRef = useRef(false);
  // 加载会话列表（带并发锁）
  const loadConversations = async () => {
    if (loadingConvsRef.current) return;
    loadingConvsRef.current = true;
    try {
      const list = await api.listConversations(
        isStarredViewRef.current ? undefined : selectedWorkspaceRef.current,
        convSearchRef.current,
        isStarredViewRef.current
      );
      setConversations(list);
    } catch (e) {
      console.error(e);
    } finally {
      loadingConvsRef.current = false;
    }
  };

  useEffect(() => {
    loadConversations();
  }, [selectedWorkspace, convSearch, isStarredView]);

  const batchCandidates = selectConversationsForBatch(conversations, batchOpts);
  const batchConcurrency = getBatchConcurrency();
  const batchEstimateSec = estimateBatchSeconds(batchCandidates.length, batchConcurrency);

  const openBatchConfirm = () => {
    if (isStarredView || !selectedWorkspace || batchRunning) return;
    setBatchError(null);
    setBatchOpts({ ...DEFAULT_BATCH_SUMMARIZE_OPTIONS });
    setBatchConfirmOpen(true);
  };

  const cancelBatchSummarize = () => {
    batchCancelRef.current = true;
  };

  const loadingStarredRef = useRef(false);
  // 加载收藏会话总数（带并发锁）
  const loadStarredCount = async () => {
    if (loadingStarredRef.current) return;
    loadingStarredRef.current = true;
    try {
      const list = await api.listConversations(undefined, undefined, true);
      setStarredCount(list.length);
    } catch (e) {
      console.error(e);
    } finally {
      loadingStarredRef.current = false;
    }
  };

  // 监听后台实时同步完成事件（单例注册，绝对不泄漏，防并发风暴）
  useEffect(() => {
    if (!isTauri()) return;
    let isSubscribed = true;
    let unlistenFn: (() => void) | null = null;

    listen('sync-completed', () => {
      loadWorkspaces();
      loadConversations();
      loadStarredCount();
    }).then((fn) => {
      if (!isSubscribed) {
        fn();
      } else {
        unlistenFn = fn;
      }
    });

    return () => {
      isSubscribed = false;
      if (unlistenFn) {
        unlistenFn();
      }
    };
  }, []);

  // 加载会话消息流与方案产物
  useEffect(() => {
    if (!selectedConversationId) {
      setMessages([]);
      setCurrentConv(null);
      setArtifacts([]);
      setSelectedArtifact(null);
      return;
    }
    const conv = conversations.find((c) => c.id === selectedConversationId);
    if (conv) setCurrentConv(conv);

    setLoadingConv(true);
    api
      .getConversationMessages(selectedConversationId)
      .then((msgs) => {
        setMessages(msgs);
      })
      .catch((e) => {
        console.error(e);
      })
      .finally(() => {
        setLoadingConv(false);
      });

    api
      .getConversationArtifacts(selectedConversationId)
      .then((arts) => {
        setArtifacts(arts);
        if (arts.length > 0) {
          setSelectedArtifact(arts[0]);
        } else {
          setSelectedArtifact(null);
        }
      })
      .catch((e) => {
        console.error(e);
        setArtifacts([]);
      });
  }, [selectedConversationId]);

  // 列表刷新后同步当前会话的 AI 元数据（如 sync 后重新拉取）
  useEffect(() => {
    if (!selectedConversationId) return;
    const conv = conversations.find((c) => c.id === selectedConversationId);
    if (conv) setCurrentConv(conv);
  }, [conversations, selectedConversationId]);

  // 切换收藏状态
  const applyConversationUpdate = (updated: ConversationItem) => {
    setCurrentConv(updated);
    setConversations((prev) => prev.map((c) => (c.id === updated.id ? { ...c, ...updated } : c)));
  };

  const startBatchSummarize = async () => {
    if (batchRunning) return;
    if (!getServiceLlmConfig()) {
      setBatchError(t('conv.batchNeedAi'));
      return;
    }
    const targets = selectConversationsForBatch(conversations, batchOpts);
    if (targets.length === 0) {
      setBatchError(t('conv.batchNone'));
      return;
    }

    setBatchConfirmOpen(false);
    setBatchError(null);
    batchCancelRef.current = false;
    setBatchRunning(true);
    // 立刻弹出右下角进度框（不必等第一轮 onProgress）
    setBatchProgress({
      total: targets.length,
      done: 0,
      ok: 0,
      skip: 0,
      error: 0,
      currentId: null,
      items: targets.map((c) => ({
        id: c.id,
        title: c.title || c.source_title || '未命名',
        status: 'queued',
      })),
      cancelled: false,
      finished: false,
    });
    setBatchPanelOpen(true);

    try {
      await runBatchSummarize({
        conversations: targets,
       fetchMessages: fetchConversationMessages,
       shouldCancel: () => batchCancelRef.current,
       onProgress: setBatchProgress,
       onConversationUpdated: applyConversationUpdate,
       concurrency: batchConcurrency,
     });
    } catch (e) {
      setBatchError(e instanceof Error ? e.message : String(e));
    } finally {
      setBatchRunning(false);
    }
  };

  const handleToggleStar = async () => {
    if (!selectedConversationId || !currentConv) return;
    try {
      const isNowStarred = await api.toggleStar(selectedConversationId);
      setCurrentConv((prev) => (prev ? { ...prev, is_starred: isNowStarred } : null));
      setConversations((prev) =>
        prev.map((c) => (c.id === selectedConversationId ? { ...c, is_starred: isNowStarred } : c))
      );
    } catch (e) {
      console.error(e);
    }
  };

  const startRename = () => {
    if (!currentConv) return;
    setAiActionError(null);
    setRenameDraft(currentConv.ai_title || currentConv.title || '');
    setRenaming(true);
  };

  const cancelRename = () => {
    setRenaming(false);
    setRenameDraft('');
  };

  const submitRename = async () => {
    if (!currentConv || !renameDraft.trim()) return;
    setRenameSaving(true);
    setAiActionError(null);
    try {
      const updated = await renameConversationTitle(currentConv.id, renameDraft.trim());
      applyConversationUpdate(updated);
      setRenaming(false);
    } catch (e) {
      console.error(e);
      setAiActionError(e instanceof Error ? e.message : String(e));
    } finally {
      setRenameSaving(false);
    }
  };

  const handleClearAiTitle = async () => {
    if (!currentConv?.ai_title) return;
    setAiActionError(null);
    try {
      const updated = await clearConversationAiTitle(currentConv.id);
      applyConversationUpdate(updated);
    } catch (e) {
      console.error(e);
      setAiActionError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleSummarize = async () => {
    if (!currentConv) return;
    setSummarizing(true);
    setAiActionError(null);
    try {
      const updated = await summarizeConversation({
        conversation: currentConv,
        messages,
      });
      applyConversationUpdate(updated);
      setSummaryOpen(true);
    } catch (e) {
      console.error(e);
      setAiActionError(e instanceof Error ? e.message : String(e));
    } finally {
      setSummarizing(false);
    }
  };

  const getSourceBadge = (source: string) => {
    switch (source) {
      case 'cursor':
        return <span className="px-1.5 py-0.5 text-[10px] font-semibold bg-blue-500/15 text-blue-500 border border-blue-500/30 rounded">Cursor</span>;
      case 'antigravity':
        return <span className="px-1.5 py-0.5 text-[10px] font-semibold bg-emerald-500/15 text-emerald-500 border border-emerald-500/30 rounded">AG</span>;
      case 'claude':
        return <span className="px-1.5 py-0.5 text-[10px] font-semibold bg-orange-500/15 text-orange-500 border border-orange-500/30 rounded">Claude</span>;
      case 'hermes':
        return <span className="px-1.5 py-0.5 text-[10px] font-semibold bg-purple-500/15 text-purple-500 border border-purple-500/30 rounded">Hermes</span>;
      case 'codex':
        return <span className="px-1.5 py-0.5 text-[10px] font-semibold bg-pink-500/15 text-pink-500 border border-pink-500/30 rounded">Codex</span>;
      case 'workbuddy':
        return <span className="px-1.5 py-0.5 text-[10px] font-semibold bg-cyan-500/15 text-cyan-500 border border-cyan-500/30 rounded">WorkBuddy</span>;
      case 'mimo':
        return <span className="px-1.5 py-0.5 text-[10px] font-semibold bg-rose-500/15 text-rose-500 border border-rose-500/30 rounded">MiMo</span>;
      case 'windsurf':
        return <span className="px-1.5 py-0.5 text-[10px] font-semibold bg-sky-500/15 text-sky-500 border border-sky-500/30 rounded">Windsurf</span>;
      case 'codebuddy':
        return <span className="px-1.5 py-0.5 text-[10px] font-semibold bg-violet-500/15 text-violet-500 border border-violet-500/30 rounded">CodeBuddy</span>;
      case 'qoder':
        return <span className="px-1.5 py-0.5 text-[10px] font-semibold bg-green-500/15 text-green-500 border border-green-500/30 rounded">Qoder</span>;
      default:
        return <span className="px-1.5 py-0.5 text-[10px] font-semibold theme-bg-sub theme-text-muted rounded">{source}</span>;
    }
  };

  const formatTime = formatBeijingTime;

  /** 会话列表（品字型左下 / 收藏视图左侧共用） */
  const renderConversationList = () => (
    <section className="w-80 border-r theme-border flex flex-col theme-bg-sub flex-shrink-0">
      <div className="p-3 border-b theme-border space-y-2">
        <div className="flex items-center justify-between">
          <div className="text-xs font-bold theme-text-main flex items-center gap-1.5">
            {isStarredView ? (
              <>
                <Star className="h-3.5 w-3.5 fill-amber-400 text-amber-400" />
                <span>{t('nav.starred')}</span>
              </>
            ) : (
              <>
                <MessageSquare className="h-3.5 w-3.5 text-blue-500" />
                <span>{t('conv.list', { n: conversations.length })}</span>
              </>
            )}
          </div>
          {selectedWorkspace && !isStarredView && (
            <div className="flex items-center gap-1.5">
              <OpenInIdeMenu workspacePath={selectedWorkspace} />
            </div>
          )}
        </div>
        <div className="relative">
          <Search className="absolute left-2.5 top-2.5 h-3.5 w-3.5 theme-text-sub" />
          <input
            type="text"
            placeholder={t('conv.search')}
            value={convSearch}
            onChange={(e) => setConvSearch(e.target.value)}
            className="w-full pl-8 pr-3 py-1.5 text-xs theme-bg-input border theme-border rounded-lg theme-text-main placeholder-slate-400 focus:outline-none focus:border-blue-500 shadow-2xs"
          />
        </div>
      </div>
      <div className="flex-1 overflow-y-auto p-2 space-y-1.5">
        {conversations.map((conv) => {
          const isSelected = conv.id === selectedConversationId;
          const batchItem = batchProgress?.items.find((i) => i.id === conv.id);
          return (
            <div
              key={conv.id}
              onClick={() => {
                // 点击会话：进入所属项目的品字型页并定位该会话
                if (conv.workspace_path) onSelectWorkspace(conv.workspace_path);
                onSelectConversation(conv.id);
              }}
              className={`p-3 rounded-xl border text-xs cursor-pointer transition-all ${
                isSelected
                  ? 'bg-blue-600/15 border-blue-500/50 theme-text-main shadow-xs'
                  : 'theme-bg-card border-transparent hover:theme-border theme-text-muted hover:theme-text-main shadow-2xs'
              } group relative`}
            >
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  setMoveConv(conv);
                  setMoveConvTarget('');
                }}
                title={t('conv.moveTo')}
                className="absolute top-2 right-2 p-1 rounded-md theme-text-muted hover:theme-text-main hover:theme-bg-card opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer"
              >
                <FolderInput className="h-3.5 w-3.5" />
              </button>
              <div className="flex items-start justify-between gap-1.5">
                <div className="font-medium line-clamp-2 theme-text-main flex items-center gap-1">
                  {conv.is_starred && (
                    <Star className="h-3 w-3 fill-amber-400 text-amber-400 flex-shrink-0" />
                  )}
                  <span>{conv.title || t('conv.untitled')}</span>
                  {conv.ai_title && (
                    <span className="shrink-0 px-1 py-0.5 text-[9px] font-semibold rounded bg-violet-500/15 text-violet-500 border border-violet-500/30">
                      AI
                    </span>
                  )}
                  {batchItem?.status === 'running' && (
                    <Loader2 className="h-3 w-3 shrink-0 text-violet-500 animate-spin" />
                  )}
                  {batchItem?.status === 'ok' && (
                    <Check className="h-3 w-3 shrink-0 text-emerald-500" />
                  )}
                  {batchItem?.status === 'error' && (
                    <span className="shrink-0 text-[9px] font-semibold text-red-500">!</span>
                  )}
                </div>
              </div>
              <div className="flex items-center justify-between mt-2 pt-2 border-t theme-border-sub text-[10px] theme-text-sub">
                <div className="flex items-center gap-1.5">
                  {getSourceBadge(conv.source_app)}
                  <span>{formatTime(conv.updated_at || conv.created_at)}</span>
                </div>
                <div className="flex items-center gap-1">
                  <span>{t('conv.userN', { n: conv.user_message_count })}</span>
                  <span>·</span>
                  <span>{t('nav.messageCount', { n: conv.message_count })}</span>
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );

  // 会话消息按 User Turn 轮次进行分组
  interface MessageTurn {
    origIndex: number;
    user?: MessageItem;
    replies: MessageItem[];
  }
  const messageTurns: MessageTurn[] = [];
  let curTurn: MessageTurn | null = null;
  for (const msg of messages) {
    if (msg.sender === 'user') {
      curTurn = { origIndex: messageTurns.length, user: msg, replies: [] };
      messageTurns.push(curTurn);
    } else {
      if (!curTurn) {
        curTurn = { origIndex: messageTurns.length, replies: [] };
        messageTurns.push(curTurn);
      }
      curTurn.replies.push(msg);
    }
  }

  const displayTurns = sortOrder === 'desc' ? [...messageTurns].reverse() : messageTurns;

  const toggleTurn = (idx: number) => {
    setExpandedTurns((prev) => ({ ...prev, [idx]: !prev[idx] }));
  };

  const copyUserPrompt = async (turnIndex: number, text: string | undefined) => {
    if (!text?.trim()) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopiedTurnIndex(turnIndex);
      if (copiedResetTimerRef.current) clearTimeout(copiedResetTimerRef.current);
      copiedResetTimerRef.current = setTimeout(() => setCopiedTurnIndex(null), 1500);
    } catch (e) {
      console.error(e);
    }
  };

  return (
    <div className="flex h-full w-full overflow-hidden theme-bg-main">
      {/* 第一栏：工作区列表 */}
      <aside className="w-64 border-r theme-border flex flex-col theme-bg-sub flex-shrink-0">
        {/* 顶部搜索 */}
        <div className="p-3 border-b theme-border">
          <div className="relative">
            <Search className="absolute left-2.5 top-2.5 h-3.5 w-3.5 theme-text-sub" />
            <input
              type="text"
              placeholder={t('nav.searchWorkspaces')}
              value={wsSearch}
              onChange={(e) => setWsSearch(e.target.value)}
              className="w-full pl-8 pr-3 py-1.5 text-xs theme-bg-input border theme-border rounded-lg theme-text-main placeholder-slate-400 focus:outline-none focus:border-blue-500 shadow-2xs"
            />
          </div>
        </div>

        {/* 固定导航区 */}
        <div className="p-2 space-y-1 border-b theme-border">
          <button
            onClick={onSwitchToDashboard}
            className={`w-full flex items-center justify-between px-3 py-2 text-xs font-medium rounded-lg transition-colors cursor-pointer ${
              !selectedWorkspace && !isStarredView && !isPromptLibraryView && !isServicesView && !isGitBoardView
                ? 'bg-blue-600/15 text-blue-500 border border-blue-500/30 shadow-xs font-semibold'
                : 'theme-text-muted hover:text-blue-500 hover:theme-bg-card'
            }`}
          >
            <div className="flex items-center gap-2">
              <LayoutDashboard className="h-4 w-4" />
              <span>{t('nav.dashboard')}</span>
            </div>
            <span className="px-1.5 py-0.5 text-[10px] bg-blue-500/15 text-blue-500 rounded font-medium">{t('nav.dashboardBadge')}</span>
          </button>

          <button
            onClick={onSwitchToPromptLibrary}
            className={`w-full flex items-center justify-between px-3 py-2 text-xs font-medium rounded-lg transition-colors cursor-pointer ${
              isPromptLibraryView
                ? 'bg-violet-600/15 text-violet-500 border border-violet-500/30 shadow-xs font-semibold'
                : 'theme-text-muted hover:text-violet-500 hover:theme-bg-card'
            }`}
          >
            <div className="flex items-center gap-2">
              <BookMarked className="h-4 w-4" />
              <span>{t('nav.prompts')}</span>
            </div>
            {promptLibraryCount > 0 && (
              <span className="px-1.5 py-0.5 text-[10px] bg-violet-500/15 text-violet-500 rounded font-mono">
                {promptLibraryCount}
              </span>
            )}
          </button>

          <button
            onClick={onSwitchToServices}
            className={`w-full flex items-center justify-between px-3 py-2 text-xs font-medium rounded-lg transition-colors cursor-pointer ${
              isServicesView
                ? 'bg-emerald-600/15 text-emerald-500 border border-emerald-500/30 shadow-xs font-semibold'
                : 'theme-text-muted hover:text-emerald-500 hover:theme-bg-card'
            }`}
          >
            <div className="flex items-center gap-2">
              <Server className="h-4 w-4" />
              <span>{t('nav.services')}</span>
            </div>
          </button>

          <button
            onClick={onSwitchToStarred}
            className={`w-full flex items-center justify-between px-3 py-2 text-xs font-medium rounded-lg transition-colors cursor-pointer ${
              isStarredView
                ? 'bg-amber-500/15 text-amber-500 border border-amber-500/30 shadow-xs font-semibold'
                : 'theme-text-muted hover:text-amber-500 hover:theme-bg-card'
            }`}
          >
            <div className="flex items-center gap-2">
              <Star className="h-4 w-4 fill-amber-400 text-amber-400" />
              <span>{t('nav.starred')}</span>
            </div>
            {starredCount > 0 && (
              <span className="px-1.5 py-0.5 text-[10px] bg-amber-500/20 text-amber-500 rounded font-mono">
                {starredCount}
              </span>
            )}
          </button>
        </div>

        {/* 工作区列表 */}
        <div className="flex-1 overflow-y-auto p-2 space-y-1.5">
          {workspaces.map((ws) => {
            const shortName = ws.workspace_path
              ? ws.workspace_path.split('/').slice(-1)[0] || ws.workspace_path
              : t('nav.uncategorized');
            const dirtyCnt = wsGitDirty[ws.workspace_path] ?? 0;
            const isActive = !isStarredView && !isPromptLibraryView && !isServicesView && ws.workspace_path === selectedWorkspace;
            return (
              <div
                key={ws.workspace_path}
                onClick={() => {
                  onSelectWorkspace(ws.workspace_path);
                  onSelectConversation(''); // 点击工作区切换到该工作区的全景分析
                }}
                className={`p-3 rounded-xl border text-xs cursor-pointer transition-all ${
                  isActive
                    ? 'bg-blue-600/15 border-blue-500/50 theme-text-main font-semibold shadow-xs'
                    : 'bg-transparent border-transparent hover:theme-bg-card theme-text-muted hover:theme-text-main'
                } group relative`}
              >
                <WorkspaceMoreMenu
                  workspacePath={ws.workspace_path}
                  onMerge={() => {
                    setMergeSource(ws);
                    setMergeMode('existing');
                    setMergeTarget('');
                  }}
                />
                <div className="flex items-center justify-between">
                  <span className="font-bold theme-text-main text-xs truncate pr-1">{shortName}</span>
                  <div className="flex items-center gap-1.5 flex-shrink-0 transition-transform duration-200 group-hover:-translate-x-6">
                    {dirtyCnt > 0 && (
                      <span
                        title={t('ws.uncommitted', { n: dirtyCnt })}
                        className="inline-flex items-center gap-0.5 px-1.5 py-0.2 text-[9px] font-mono font-semibold rounded bg-orange-500/15 text-orange-500 border border-orange-500/30"
                      >
                        <GitBranch className="h-2.5 w-2.5" />
                        {dirtyCnt}
                      </span>
                    )}
                    {ws.last_updated && (
                      <span className="text-[10px] theme-text-sub">
                        {formatRelativeTime(ws.last_updated)}
                      </span>
                    )}
                  </div>
                </div>
                <div className="text-[10px] theme-text-sub truncate mt-0.5 font-mono">
                  {ws.workspace_path || t('nav.uncategorizedPath')}
                </div>

                {/* Agent 来源标签 */}
                <div className="flex items-center gap-1.5 flex-wrap mt-2">
                  <span className="text-[10px] font-mono theme-text-muted">{t('nav.sessionCount', { n: ws.cnt })}</span>
                  {ws.ag_cnt > 0 && (
                    <span className="px-1 py-0.2 text-[9px] bg-purple-500/15 text-purple-500 rounded font-mono">
                      AG {ws.ag_cnt}
                    </span>
                  )}
                  {ws.cursor_cnt > 0 && (
                    <span className="px-1 py-0.2 text-[9px] bg-blue-500/15 text-blue-500 rounded font-mono">
                      Cursor {ws.cursor_cnt}
                    </span>
                  )}
                  {ws.claude_cnt > 0 && (
                    <span className="px-1 py-0.2 text-[9px] bg-orange-500/15 text-orange-500 rounded font-mono">
                      Claude {ws.claude_cnt}
                    </span>
                  )}
                  {ws.codex_cnt > 0 && (
                    <span className="px-1 py-0.2 text-[9px] bg-pink-500/15 text-pink-500 rounded font-mono">
                      Codex {ws.codex_cnt}
                    </span>
                  )}
                  {ws.wb_cnt > 0 && (
                    <span className="px-1 py-0.2 text-[9px] bg-cyan-500/15 text-cyan-500 rounded font-mono">
                      WorkBuddy {ws.wb_cnt}
                    </span>
                  )}
                  {ws.hermes_cnt > 0 && (
                    <span className="px-1 py-0.2 text-[9px] bg-purple-500/15 text-purple-400 rounded font-mono">
                      Hermes {ws.hermes_cnt}
                    </span>
                  )}
                  {(ws.mimo_cnt ?? 0) > 0 && (
                    <span className="px-1 py-0.2 text-[9px] bg-rose-500/15 text-rose-500 rounded font-mono">
                      MiMo {ws.mimo_cnt}
                    </span>
                  )}
                  {(ws.windsurf_cnt ?? 0) > 0 && (
                    <span className="px-1 py-0.2 text-[9px] bg-sky-500/15 text-sky-500 rounded font-mono">
                      Windsurf {ws.windsurf_cnt}
                    </span>
                  )}
                  {(ws.codebuddy_cnt ?? 0) > 0 && (
                    <span className="px-1 py-0.2 text-[9px] bg-violet-500/15 text-violet-500 rounded font-mono">
                      CodeBuddy {ws.codebuddy_cnt}
                    </span>
                  )}
                  {(ws.qoder_cnt ?? 0) > 0 && (
                    <span className="px-1 py-0.2 text-[9px] bg-green-500/15 text-green-500 rounded font-mono">
                      Qoder {ws.qoder_cnt}
                    </span>
                  )}
                </div>

                <div className="flex items-center gap-1.5 mt-1.5 text-[10px] theme-text-sub">
                  <span>· {t('nav.userCount', { n: ws.user_message_count })}</span>
                  <span>·</span>
                  <span>{t('nav.messageCount', { n: ws.message_count })}</span>
                </div>
              </div>
            );
          })}
        </div>
      </aside>

      {/* 右侧主内容区域 */}
      {!selectedWorkspace && isGitBoardView ? (
        <main className="flex-1 flex flex-col h-full overflow-hidden theme-bg-main">
          {/* 顶栏胶囊 → 全局 Git 看板 */}
          <GitBoardView
            onExit={() => {
              onSwitchToDashboard();
            }}
          />
        </main>
      ) : isServicesView ? (
        <main className="flex-1 flex flex-col h-full overflow-hidden theme-bg-main">
          <ServicesView />
        </main>
      ) : isPromptLibraryView ? (
        <main className="flex-1 flex flex-col h-full overflow-hidden theme-bg-main">
          <PromptLibraryView onPromptCountChange={onPromptLibraryCountChange} />
        </main>
      ) : !selectedWorkspace && !isStarredView ? (
        <main className="flex-1 flex flex-col h-full overflow-hidden theme-bg-main">
          <DashboardView
            stats={stats}
            loading={loadingStats}
            onRefresh={onRefreshStats}
            onSelectConversation={(convId, wsPath) => {
              onSelectWorkspace(wsPath);
              onSelectConversation(convId);
            }}
          />
        </main>
      ) : selectedWorkspace ? (
        /* 品字型：Header 常驻；下方 = 会话+统计 或 Git 变更树 */
        <main className="flex-1 flex flex-col h-full overflow-hidden theme-bg-main">
          <ProjectHeader
            workspacePath={selectedWorkspace}
            gitBoardActive={isGitBoardView}
            onOpenGitBoard={() => onOpenGitBoard(selectedWorkspace)}
            onBatchSummarize={openBatchConfirm}
            batchSummarizeDisabled={batchRunning || conversations.length === 0}
          />
          {isGitBoardView ? (
            <div className="flex-1 min-h-0 overflow-hidden flex flex-col">
              <GitBoardView embedPath={selectedWorkspace} hideHeader />
            </div>
          ) : (
          <div className="flex-1 flex min-h-0 overflow-hidden">
            {renderConversationList()}
            <div className="flex-1 flex flex-col h-full overflow-hidden theme-bg-main min-w-0">
            {selectedConversationId && currentConv ? (
              <>
                {/* 会话顶部 Header */}
                <div className="p-4 border-b theme-border flex items-center justify-between theme-bg-header backdrop-blur-sm gap-4">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2 mb-1">
                      <button
                        onClick={() => onSelectConversation('')}
                        className="text-xs text-blue-500 hover:underline flex items-center gap-1 cursor-pointer"
                      >
                        <ArrowLeft className="h-3.5 w-3.5" />
                        <span>{t('conv.backAnalysis')}</span>
                      </button>
                    </div>
                    <h2 className="text-base font-bold theme-text-main truncate flex items-center gap-2 min-w-0">
                      {renaming ? (
                        <form
                          className="flex items-center gap-2 min-w-0 flex-1"
                          onSubmit={(e) => {
                            e.preventDefault();
                            submitRename();
                          }}
                        >
                          <input
                            autoFocus
                            value={renameDraft}
                            onChange={(e) => setRenameDraft(e.target.value)}
                            disabled={renameSaving}
                            className="flex-1 min-w-0 px-2 py-1 text-sm font-semibold theme-bg-input border theme-border rounded-md theme-text-main focus:outline-none focus:border-blue-500"
                            placeholder={t('conv.renamePlaceholder')}
                          />
                          <button
                            type="submit"
                            disabled={renameSaving || !renameDraft.trim()}
                            className="px-2 py-1 text-xs rounded-md bg-blue-600 text-white disabled:opacity-50 cursor-pointer"
                          >
                            {renameSaving ? t('conv.saving') : t('conv.save')}
                          </button>
                          <button
                            type="button"
                            onClick={cancelRename}
                            className="px-2 py-1 text-xs rounded-md border theme-border theme-text-muted cursor-pointer"
                          >
                            {t('conv.cancel')}
                          </button>
                        </form>
                      ) : (
                        <>
                          <button
                            type="button"
                            onClick={startRename}
                            disabled={summarizing}
                            title={t('conv.renameHint')}
                            className="group min-w-0 flex items-center gap-1.5 text-left cursor-pointer disabled:opacity-50"
                          >
                            <span className="truncate group-hover:text-blue-500 transition-colors">
                              {currentConv.title || t('conv.untitled')}
                            </span>
                            <Pencil className="h-3.5 w-3.5 shrink-0 theme-text-muted opacity-0 group-hover:opacity-100 transition-opacity" />
                          </button>
                          {currentConv.ai_title && (
                            <span className="shrink-0 px-1.5 py-0.5 text-[10px] font-medium rounded bg-violet-500/15 text-violet-500 border border-violet-500/30">
                              AI
                            </span>
                          )}
                          {currentConv.ai_summary_stale && (
                            <span
                              className="shrink-0 px-1.5 py-0.5 text-[10px] font-medium rounded bg-amber-500/15 text-amber-600 border border-amber-500/30"
                              title={t('conv.summaryStaleHint')}
                            >
                              {t('conv.summaryStale')}
                            </span>
                          )}
                        </>
                      )}
                    </h2>
                    <div className="flex items-center gap-2 text-xs theme-text-muted mt-1 min-w-0">
                      <span className="shrink-0">{getSourceBadge(currentConv.source_app)}</span>
                      <span
                        className="truncate font-mono min-w-0"
                        title={currentConv.workspace_path}
                      >
                        {currentConv.workspace_path}
                      </span>
                      <span className="shrink-0">·</span>
                      <span className="shrink-0 whitespace-nowrap">
                        {t('conv.messages', { n: messages.length })}
                      </span>
                    </div>
                    {currentConv.ai_title &&
                      currentConv.source_title &&
                      currentConv.ai_title !== currentConv.source_title && (
                        <div
                          className="mt-1 text-[11px] theme-text-sub leading-snug line-clamp-2"
                          title={currentConv.source_title}
                        >
                          <span className="theme-text-muted">{t('conv.sourceTitle')}：</span>
                          <span className="break-all">{currentConv.source_title}</span>
                        </div>
                      )}
                    {aiActionError && (
                      <div className="mt-1 text-[11px] text-red-500 truncate" title={aiActionError}>
                        {aiActionError}
                      </div>
                    )}
                  </div>

                  {/* 顶部操作区 */}
                  <div className="flex items-center gap-1.5 shrink-0">
                    <button
                      onClick={handleSummarize}
                      disabled={summarizing || loadingConv || messages.length === 0 || renaming}
                      title={summarizing ? t('conv.summarizing') : t('conv.summarize')}
                      className="flex items-center justify-center p-1.5 rounded-lg border transition-all cursor-pointer shadow-xs shrink-0 disabled:opacity-50 bg-violet-500/10 border-violet-500/35 text-violet-600 dark:text-violet-400 hover:bg-violet-500/20"
                    >
                      <Sparkles className={`h-4 w-4 shrink-0 ${summarizing ? 'animate-pulse' : ''}`} />
                    </button>

                    {currentConv.ai_summary && (
                      <button
                        onClick={() => setSummaryOpen((v) => !v)}
                        title={summaryOpen ? t('conv.hideSummary') : t('conv.showSummary')}
                        className="flex items-center justify-center p-1.5 rounded-lg border theme-border theme-bg-sub hover:opacity-80 text-blue-500 hover:theme-text-main transition-colors cursor-pointer shadow-xs shrink-0"
                      >
                        <AlignLeft className="h-4 w-4 shrink-0" />
                      </button>
                    )}

                    <button
                      onClick={() => setSortOrder((prev) => (prev === 'asc' ? 'desc' : 'asc'))}
                      title={sortOrder === 'asc' ? t('conv.sortTitleAsc') : t('conv.sortTitleDesc')}
                      className="flex items-center justify-center p-1.5 rounded-lg border theme-border theme-bg-sub hover:opacity-80 text-blue-500 hover:theme-text-main transition-colors cursor-pointer shadow-xs shrink-0"
                    >
                      <ArrowUpDown className="h-4 w-4 shrink-0" />
                    </button>

                    <button
                      onClick={() => {
                        const allExpanded =
                          messageTurns.length > 0 &&
                          Object.keys(expandedTurns).length === messageTurns.length &&
                          Object.values(expandedTurns).every(Boolean);
                        const next: Record<number, boolean> = {};
                        if (!allExpanded) {
                          messageTurns.forEach((_, idx) => {
                            next[idx] = true;
                          });
                        }
                        setExpandedTurns(next);
                      }}
                      title={
                        messageTurns.length > 0 &&
                        Object.keys(expandedTurns).length === messageTurns.length &&
                        Object.values(expandedTurns).every(Boolean)
                          ? t('conv.collapseAll')
                          : t('conv.expandAll')
                      }
                      className="flex items-center justify-center p-1.5 rounded-lg border theme-border theme-bg-sub hover:opacity-80 theme-text-muted hover:theme-text-main transition-colors cursor-pointer shadow-xs shrink-0"
                    >
                      {messageTurns.length > 0 &&
                      Object.keys(expandedTurns).length === messageTurns.length &&
                      Object.values(expandedTurns).every(Boolean) ? (
                        <Minimize2 className="h-4 w-4 shrink-0" />
                      ) : (
                        <Maximize2 className="h-4 w-4 shrink-0" />
                      )}
                    </button>

                    {artifacts.length > 0 && (
                      <button
                        onClick={() => {
                          if (!selectedArtifact && artifacts.length > 0) {
                            setSelectedArtifact(artifacts[0]);
                          }
                          setShowArtifactModal(true);
                        }}
                        title={t('conv.artifactsBtn', { n: artifacts.length })}
                        className="flex items-center justify-center p-1.5 rounded-lg border bg-emerald-500/10 border-emerald-500/35 text-emerald-600 dark:text-emerald-400 hover:bg-emerald-500/20 transition-all cursor-pointer shadow-xs shrink-0"
                      >
                        <FileText className="h-4 w-4 shrink-0" />
                      </button>
                    )}

                    <button
                      onClick={handleToggleStar}
                      title={currentConv.is_starred ? t('conv.starredBtn') : t('conv.star')}
                      className={`flex items-center justify-center p-1.5 rounded-lg border transition-all cursor-pointer shadow-xs shrink-0 ${
                        currentConv.is_starred
                          ? 'bg-amber-500/15 border-amber-500/40 text-amber-500'
                          : 'theme-bg-sub hover:opacity-80 theme-border theme-text-main'
                      }`}
                    >
                      <Star
                        className={`h-4 w-4 shrink-0 ${
                          currentConv.is_starred ? 'fill-amber-400 text-amber-400' : 'theme-text-sub'
                        }`}
                      />
                    </button>
                  </div>
                </div>

                {summaryOpen && currentConv.ai_summary && (
                  <div className="px-4 py-3 border-b theme-border theme-bg-sub/60">
                    <div className="max-w-4xl mx-auto">
                      <div className="flex items-center justify-between gap-2 mb-1.5">
                        <div className="flex items-center gap-2 text-xs font-semibold theme-text-main">
                          <AlignLeft className="h-3.5 w-3.5 text-violet-500" />
                          <span>{t('conv.summaryTitle')}</span>
                          {currentConv.ai_summary_stale && (
                            <span className="font-medium text-amber-600">{t('conv.summaryStale')}</span>
                          )}
                          {currentConv.ai_model && (
                            <span className="font-mono font-normal theme-text-muted">
                              · {currentConv.ai_model}
                            </span>
                          )}
                          {currentConv.ai_generated_at && (
                            <span className="font-normal theme-text-muted">
                              · {t('conv.summaryGeneratedAt', { time: formatBeijingTime(currentConv.ai_generated_at) })}
                            </span>
                          )}
                          {currentConv.ai_new_message_count != null &&
                            currentConv.ai_new_message_count > 0 && (
                            <span className="font-normal text-amber-600">
                              · {t('conv.newMessagesSince', { n: currentConv.ai_new_message_count })}
                            </span>
                          )}
                        </div>
                        <div className="flex items-center gap-2">
                          {currentConv.ai_title && (
                            <button
                              type="button"
                              onClick={handleClearAiTitle}
                              className="text-[11px] theme-text-muted hover:text-red-500 cursor-pointer"
                              title={t('conv.clearAiTitleHint')}
                            >
                              {t('conv.clearAiTitle')}
                            </button>
                          )}
                          <button
                            type="button"
                            onClick={() => setSummaryOpen(false)}
                            className="text-[11px] theme-text-muted hover:theme-text-main cursor-pointer"
                          >
                            {t('conv.hideSummary')}
                          </button>
                        </div>
                      </div>
                      <p className="text-xs leading-relaxed theme-text-main whitespace-pre-wrap">
                        {currentConv.ai_summary}
                      </p>
                    </div>
                  </div>
                )}

                {/* 消息轮次滚动流（默认显示用户消息，Agent 消息折叠在用户轮次内） */}
                <div className="flex-1 overflow-y-auto p-6 space-y-4">
                  {loadingConv ? (
                    <div className="flex h-full items-center justify-center theme-text-muted">
                      <Clock className="h-6 w-6 animate-spin mr-2" /> {t('conv.loading')}
                    </div>
                  ) : displayTurns.length === 0 ? (
                    <div className="py-12 text-center text-xs theme-text-sub">{t('conv.empty')}</div>
                  ) : (
                    displayTurns.map((turn) => {
                      const isExpanded = !!expandedTurns[turn.origIndex];
                      const userMsg = turn.user;
                      const replyCount = turn.replies.length;

                      return (
                        <div
                          key={turn.origIndex}
                          className="border theme-border rounded-2xl theme-bg-card p-4 transition-all shadow-xs max-w-4xl mx-auto space-y-3"
                        >
                          {/* 用户消息头部信息栏（点击可切换展开/折叠本轮 Agent 回复） */}
                          <div
                            onClick={() => toggleTurn(turn.origIndex)}
                            className="flex items-center justify-between gap-2 cursor-pointer select-none text-xs pb-2 border-b theme-border-sub"
                          >
                            <div className="flex items-center gap-2">
                              <span className="w-5 h-5 rounded-md bg-blue-600/15 text-blue-500 flex items-center justify-center font-bold text-[11px]">
                                {isExpanded ? '▼' : '▶'}
                              </span>
                              <span className="px-1.5 py-0.5 text-[10px] font-bold bg-blue-600 text-white rounded">
                                {t('conv.user')}
                              </span>
                              {userMsg && (
                                <span className="text-[11px] font-mono theme-text-sub">
                                  step {userMsg.step_index}
                                </span>
                              )}
                              {userMsg?.created_at && (
                                <span className="text-[10px] font-mono theme-text-muted">
                                  {formatTime(userMsg.created_at)}
                                </span>
                              )}
                              {userMsg?.text?.trim() && (
                                <button
                                  type="button"
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    copyUserPrompt(turn.origIndex, userMsg.text);
                                  }}
                                  className={`inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md border transition-all cursor-pointer ${
                                    copiedTurnIndex === turn.origIndex
                                      ? 'bg-emerald-500/15 text-emerald-500 border-emerald-500/30'
                                      : 'theme-text-muted theme-border-sub hover:theme-text-main hover:border-blue-500/40 hover:bg-blue-500/10'
                                  }`}
                                  title={
                                    copiedTurnIndex === turn.origIndex
                                      ? t('conv.copied')
                                      : t('conv.copyPrompt')
                                  }
                                >
                                  {copiedTurnIndex === turn.origIndex ? (
                                    <Check className="h-3 w-3" />
                                  ) : (
                                    <Copy className="h-3 w-3" />
                                  )}
                                  <span className="text-[10px] font-medium">
                                    {copiedTurnIndex === turn.origIndex
                                      ? t('conv.copied')
                                      : t('conv.copy')}
                                  </span>
                                </button>
                              )}
                            </div>

                            {replyCount > 0 && (
                              <div className="flex items-center gap-1 text-[11px] font-medium text-blue-500 hover:underline">
                                <span>{t('conv.agentReplyCount', { n: replyCount })}</span>
                                <span className="text-[10px] theme-text-muted">
                                  {isExpanded ? t('conv.clickCollapse') : t('conv.clickExpand')}
                                </span>
                              </div>
                            )}
                          </div>

                          {/* 用户 Prompt 提问内容与附图展示 */}
                          {userMsg && (
                            <div className="space-y-2">
                              {userMsg.text && (
                                <div className="text-xs leading-relaxed theme-text-main whitespace-pre-wrap font-sans pl-1">
                                  {userMsg.text}
                                  {(() => {
                                    if (artifacts.length === 0) return null;
                                    const matched = getMatchedArtifactsForMessage(
                                      userMsg.text,
                                      userMsg.created_at,
                                      artifacts
                                    );
                                    if (matched.length === 0) return null;
                                    return (
                                      <span className="inline-flex flex-wrap gap-1.5 ml-2">
                                        {matched.map((art) => (
                                          <button
                                            key={art.file_name}
                                            onClick={(e) => {
                                              e.stopPropagation();
                                              setSelectedArtifact(art);
                                              setShowArtifactModal(true);
                                            }}
                                            className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-medium bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 border border-emerald-500/30 hover:bg-emerald-500/25 transition-all cursor-pointer shadow-2xs"
                                          >
                                            <FileText className="h-3 w-3" />
                                            <span>查看{art.title || art.file_name}</span>
                                          </button>
                                        ))}
                                      </span>
                                    );
                                  })()}
                                </div>
                              )}

                              {/* 用户附图渲染 */}
                              {(() => {
                                const imgs = parseImages(userMsg.images);
                                if (imgs.length === 0) return null;
                                return (
                                  <div className="flex flex-wrap gap-2.5 pt-1 pl-1">
                                    {imgs.map((img, i) => (
                                      <div
                                        key={i}
                                        className="relative group rounded-xl overflow-hidden border theme-border theme-bg-sub shadow-xs cursor-pointer max-w-sm max-h-60"
                                        onClick={(e) => {
                                          e.stopPropagation();
                                          setLightboxImg(img.src);
                                        }}
                                      >
                                        <img
                                          src={img.src}
                                          alt={t('conv.imageAlt', { n: i + 1 })}
                                          loading="lazy"
                                          className="w-full h-full object-contain max-h-60 rounded-xl transition-transform group-hover:scale-105"
                                        />
                                        <div className="absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center text-white text-xs gap-1 font-medium pointer-events-none">
                                          <Maximize2 className="h-3.5 w-3.5" />
                                          <span>{t('conv.zoom')}</span>
                                        </div>
                                      </div>
                                    ))}
                                  </div>
                                );
                              })()}
                            </div>
                          )}

                          {/* 折叠区：Agent 回复流（思考过程、工具调用、模型回答） */}
                          {isExpanded && replyCount > 0 && (
                            <div className="ml-3 pl-4 border-l-2 border-blue-500/30 space-y-3 pt-2">
                              {turn.replies.map((m, mIdx) => {
                                let toolCalls = [];
                                if (m.tool_calls_json) {
                                  try {
                                    toolCalls = JSON.parse(m.tool_calls_json);
                                  } catch (e) {}
                                }

                                return (
                                  <div
                                    key={m.id || mIdx}
                                    className="p-3.5 rounded-xl theme-bg-sub border theme-border text-xs space-y-2.5 shadow-2xs"
                                  >
                                    {/* 单条回复顶栏 */}
                                    <div className="flex items-center justify-between text-[10px] theme-text-sub pb-1.5 border-b theme-border-sub">
                                      <div className="flex items-center gap-2">
                                        <span className="px-1.5 py-0.2 rounded font-semibold bg-purple-500/15 text-purple-400 border border-purple-500/30">
                                          {m.sender === 'system' ? t('conv.system') : t('conv.agent')}
                                        </span>
                                        <span className="font-mono">step {m.step_index}</span>
                                        {m.created_at && (
                                          <span className="font-mono">{formatTime(m.created_at)}</span>
                                        )}
                                      </div>
                                      {m.model_name && <span className="font-mono">{m.model_name}</span>}
                                    </div>

                                    {/* 思考推理过程 (Thinking / CoT) */}
                                    {m.thinking && (
                                      <details className="rounded-lg border theme-border theme-bg-card p-2.5 text-xs text-slate-400 group">
                                        <summary className="cursor-pointer font-medium text-[11px] select-none flex items-center gap-1.5 text-purple-400">
                                          <Sparkles className="h-3 w-3" />
                                          <span>{t('conv.thinking')}</span>
                                        </summary>
                                        <div className="mt-2 pt-2 border-t theme-border whitespace-pre-wrap font-mono text-[10.5px] leading-relaxed text-slate-400">
                                          {m.thinking}
                                        </div>
                                      </details>
                                    )}

                                    {/* Markdown 消息正文 */}
                                    {m.text && (
                                      <div className="markdown-body select-text leading-relaxed text-xs">
                                        <ReactMarkdown
                                          remarkPlugins={[remarkGfm]}
                                          components={{
                                            code({ node, className, children, ...props }) {
                                              const rawStr = String(children).trim();
                                              const matched = getMatchedArtifactsForMessage(
                                                rawStr,
                                                m.created_at,
                                                artifacts
                                              )[0];
                                              if (matched) {
                                                return (
                                                  <button
                                                    onClick={(e) => {
                                                      e.preventDefault();
                                                      e.stopPropagation();
                                                      setSelectedArtifact(matched);
                                                      setShowArtifactModal(true);
                                                    }}
                                                    className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[11px] font-mono font-semibold bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 border border-emerald-500/35 hover:bg-emerald-500/25 transition-all cursor-pointer mx-0.5"
                                                    title={`点击查看方案：${matched.title || matched.file_name}`}
                                                  >
                                                    <FileText className="h-3 w-3 inline" />
                                                    <span>{rawStr}</span>
                                                  </button>
                                                );
                                              }
                                              return <code className={className} {...props}>{children}</code>;
                                            },
                                          }}
                                        >
                                          {m.text}
                                        </ReactMarkdown>
                                      </div>
                                    )}

                                    {/* 匹配到的产物文档快捷直达入口卡片 */}
                                     {(() => {
                                       if (artifacts.length === 0 || !m.text) return null;
                                       const matched = getMatchedArtifactsForMessage(
                                         m.text,
                                         m.created_at,
                                         artifacts
                                       );
                                       if (matched.length === 0) return null;
                                      return (
                                        <div className="flex flex-wrap gap-2 pt-2 mt-2 border-t theme-border-sub">
                                          {matched.map((art) => (
                                            <button
                                              key={art.file_name}
                                              onClick={() => {
                                                setSelectedArtifact(art);
                                                setShowArtifactModal(true);
                                              }}
                                              className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-semibold bg-emerald-500/10 hover:bg-emerald-500/20 text-emerald-600 dark:text-emerald-400 border border-emerald-500/30 transition-all cursor-pointer shadow-2xs"
                                            >
                                              <FileText className="h-3.5 w-3.5" />
                                              <span>查看方案文档：{art.title || art.file_name}</span>
                                            </button>
                                          ))}
                                        </div>
                                      );
                                    })()}

                                    {/* Agent 消息附图（若有） */}
                                    {(() => {
                                      const replyImgs = parseImages(m.images);
                                      if (replyImgs.length === 0) return null;
                                      return (
                                        <div className="flex flex-wrap gap-2.5 pt-1">
                                          {replyImgs.map((img, i) => (
                                            <div
                                              key={i}
                                              className="relative group rounded-xl overflow-hidden border theme-border theme-bg-card shadow-xs cursor-pointer max-w-sm max-h-60"
                                              onClick={(e) => {
                                                e.stopPropagation();
                                                setLightboxImg(img.src);
                                              }}
                                            >
                                              <img
                                                src={img.src}
                                                alt={t('conv.replyImageAlt', { n: i + 1 })}
                                                loading="lazy"
                                                className="w-full h-full object-contain max-h-60 rounded-xl transition-transform group-hover:scale-105"
                                              />
                                              <div className="absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center text-white text-xs gap-1 font-medium pointer-events-none">
                                                <Maximize2 className="h-3.5 w-3.5" />
                                                <span>{t('conv.zoom')}</span>
                                              </div>
                                            </div>
                                          ))}
                                        </div>
                                      );
                                    })()}

                                    {/* 工具调用块 */}
                                    {toolCalls.length > 0 && (
                                      <div className="space-y-1.5 pt-1">
                                        {toolCalls.map((tc: any, tcIdx: number) => (
                                          <details
                                            key={tcIdx}
                                            className="p-2 rounded-lg border theme-border theme-bg-card text-xs font-mono space-y-1 group"
                                          >
                                            <summary className="cursor-pointer font-medium text-blue-500 flex items-center justify-between">
                                              <div className="flex items-center gap-1.5">
                                                <Wrench className="h-3 w-3" />
                                                <span>Tool: {tc.tool_name || tc.name || t('conv.toolCall')}</span>
                                              </div>
                                              <span className="text-[10px] theme-text-sub">{t('conv.viewArgs')}</span>
                                            </summary>
                                            {tc.args && (
                                              <pre className="mt-1.5 text-[10px] theme-text-muted overflow-x-auto p-2 theme-bg-main rounded border theme-border-sub">
                                                {typeof tc.args === 'string'
                                                  ? tc.args
                                                  : JSON.stringify(tc.args, null, 2)}
                                              </pre>
                                            )}
                                          </details>
                                        ))}
                                      </div>
                                    )}
                                  </div>
                                );
                              })}
                            </div>
                          )}
                        </div>
                      );
                    })
                  )}
                </div>
              </>
            ) : (
              <WorkspaceAnalysisView
                workspacePath={selectedWorkspace}
                onSelectConversation={onSelectConversation}
                embedded
              />
            )}
            </div>
          </div>
          )}
        </main>
      ) : (
        /* 收藏等无项目场景：列表 + 右侧内容 */
        <>
          {renderConversationList()}
          <main className="flex-1 flex flex-col h-full overflow-hidden theme-bg-main">
            <div className="flex-1 flex flex-col items-center justify-center theme-text-sub">
              <Sparkles className="h-10 w-10 mb-2 opacity-50" />
              <p className="text-sm">{t('conv.pickWorkspace')}</p>
            </div>
          </main>
        </>
      )}

      {/* 图片大图预览 Lightbox 模态框 */}
      {lightboxImg && (
        <div
          className="fixed inset-0 z-50 bg-black/80 backdrop-blur-sm flex items-center justify-center p-4 animate-in fade-in duration-150"
          onClick={() => setLightboxImg(null)}
        >
          <div
            className="relative max-w-5xl max-h-[90vh] flex flex-col items-center"
            onClick={(e) => e.stopPropagation()}
          >
            <button
              onClick={() => setLightboxImg(null)}
              className="absolute -top-10 right-0 text-white/80 hover:text-white p-1.5 rounded-lg bg-black/50 hover:bg-black/80 transition-all cursor-pointer"
              title={t('conv.closePreview')}
            >
              <X className="h-5 w-5" />
            </button>
            <img
              src={lightboxImg}
              alt={t('conv.previewAlt')}
              className="max-w-full max-h-[85vh] rounded-2xl object-contain shadow-2xl border border-white/15"
            />
          </div>
        </div>
      )}

      {/* 批量总结命名：确认 */}
      {batchConfirmOpen && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
          onClick={() => !batchRunning && setBatchConfirmOpen(false)}
        >
          <div
            className="w-full max-w-md rounded-2xl border theme-border theme-bg-main p-5 shadow-2xl"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-start justify-between gap-3">
              <div>
                <h3 className="text-base font-semibold theme-text-main flex items-center gap-2">
                  <Layers className="h-4 w-4 text-violet-500" />
                  {t('conv.batchSummarizeTitle')}
                </h3>
                <p className="mt-1 text-xs theme-text-muted">{t('conv.batchScope')}</p>
              </div>
              <button
                type="button"
                className="theme-text-muted hover:theme-text-main cursor-pointer"
                onClick={() => setBatchConfirmOpen(false)}
              >
                <X className="h-4 w-4" />
              </button>
            </div>

            <div className="mt-4 space-y-2.5 text-sm theme-text-main">
              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  type="checkbox"
                  checked={batchOpts.onlyMissing && !batchOpts.redoExisting}
                  disabled={batchOpts.redoExisting}
                  onChange={(e) =>
                    setBatchOpts((o) => ({ ...o, onlyMissing: e.target.checked }))
                  }
                  className="rounded border theme-border"
                />
                {t('conv.batchOnlyMissing')}
              </label>
              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  type="checkbox"
                  checked={batchOpts.includeErrors}
                  disabled={batchOpts.redoExisting}
                  onChange={(e) =>
                    setBatchOpts((o) => ({ ...o, includeErrors: e.target.checked }))
                  }
                  className="rounded border theme-border"
                />
                {t('conv.batchIncludeErrors')}
              </label>
              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  type="checkbox"
                  checked={batchOpts.redoExisting}
                  onChange={(e) =>
                    setBatchOpts((o) => ({ ...o, redoExisting: e.target.checked }))
                  }
                  className="rounded border theme-border"
                />
                {t('conv.batchRedo')}
              </label>
              <label className="flex items-center gap-2">
                <span className="theme-text-muted text-xs shrink-0">{t('conv.batchMax')}</span>
                <input
                  type="number"
                  min={1}
                  max={200}
                  value={batchOpts.maxCount}
                  onChange={(e) =>
                    setBatchOpts((o) => ({
                      ...o,
                      maxCount: Math.max(1, Math.min(200, Number(e.target.value) || 50)),
                    }))
                  }
                  className="w-20 rounded-lg border theme-border theme-bg-input px-2 py-1 text-xs"
                />
                <span className="text-xs theme-text-muted">{t('conv.batchMaxUnit')}</span>
              </label>
            </div>

            <p className="mt-3 text-xs theme-text-muted">
              {batchCandidates.length === 0
                ? t('conv.batchNone')
                : batchEstimateSec >= 60
                  ? t('conv.batchEstimate', {
                      n: batchCandidates.length,
                      min: Math.max(1, Math.round(batchEstimateSec / 60)),
                    })
                  : t('conv.batchEstimateShort', {
                      n: batchCandidates.length,
                      sec: batchEstimateSec,
                    })}
            </p>
            {batchError && (
              <p className="mt-2 text-xs text-red-500">{batchError}</p>
            )}

            <div className="mt-5 flex justify-end gap-2">
              <button
                type="button"
                className="px-3 py-1.5 text-xs rounded-lg border theme-border theme-text-muted hover:theme-text-main cursor-pointer"
                onClick={() => setBatchConfirmOpen(false)}
              >
                {t('conv.cancel')}
              </button>
              <button
                type="button"
                disabled={batchCandidates.length === 0}
                className="px-3 py-1.5 text-xs rounded-lg bg-violet-600 text-white hover:bg-violet-500 disabled:opacity-40 cursor-pointer"
                onClick={() => void startBatchSummarize()}
              >
                {t('conv.batchStart')}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 批量总结命名：进度 */}
      {batchPanelOpen && batchProgress && (
        <div className="fixed bottom-6 right-6 z-[100] w-[min(100%-2rem,22rem)] rounded-2xl border theme-border theme-bg-main shadow-2xl overflow-hidden">
          <div className="px-4 py-3 border-b theme-border flex items-center justify-between gap-2">
            <div className="min-w-0">
              <div className="text-sm font-semibold theme-text-main flex items-center gap-2">
                {batchRunning ? (
                  <Loader2 className="h-3.5 w-3.5 text-violet-500 animate-spin shrink-0" />
                ) : (
                  <Check className="h-3.5 w-3.5 text-emerald-500 shrink-0" />
                )}
                <span className="truncate">
                  {batchRunning
                    ? t('conv.batchRunning')
                    : batchProgress.cancelled
                      ? t('conv.batchCancelled')
                      : t('conv.batchDone')}
                </span>
              </div>
              <p className="mt-0.5 text-[11px] theme-text-muted">
                {t('conv.batchProgress', {
                  done: batchProgress.done,
                  total: batchProgress.total,
                })}
                {' · '}
                {t('conv.batchStats', {
                  ok: batchProgress.ok,
                  skip: batchProgress.skip,
                  error: batchProgress.error,
                })}
              </p>
            </div>
            <div className="flex items-center gap-1 shrink-0">
              {batchRunning ? (
                <button
                  type="button"
                  onClick={cancelBatchSummarize}
                  className="inline-flex items-center gap-1 px-2 py-1 text-[11px] rounded-lg border theme-border theme-text-muted hover:theme-text-main cursor-pointer"
                >
                  <StopCircle className="h-3 w-3" />
                  {t('conv.batchCancel')}
                </button>
              ) : (
                <button
                  type="button"
                  onClick={() => {
                    setBatchPanelOpen(false);
                    setBatchProgress(null);
                  }}
                  className="px-2 py-1 text-[11px] rounded-lg border theme-border theme-text-muted hover:theme-text-main cursor-pointer"
                >
                  {t('conv.batchClose')}
                </button>
              )}
            </div>
          </div>
          <div className="max-h-48 overflow-y-auto px-2 py-2 space-y-1">
            {batchProgress.items.map((item) => (
              <div
                key={item.id}
                className="flex items-start gap-2 rounded-lg px-2 py-1.5 text-[11px]"
              >
                <span className="shrink-0 mt-0.5 w-3.5">
                  {item.status === 'running' && (
                    <Loader2 className="h-3 w-3 text-violet-500 animate-spin" />
                  )}
                  {item.status === 'ok' && <Check className="h-3 w-3 text-emerald-500" />}
                  {item.status === 'error' && (
                    <span className="text-red-500 font-bold">!</span>
                  )}
                  {item.status === 'skip' && (
                    <span className="theme-text-muted">–</span>
                  )}
                  {item.status === 'queued' && (
                    <span className="theme-text-muted">·</span>
                  )}
                </span>
                <div className="min-w-0 flex-1">
                  <div className="truncate theme-text-main">{item.title}</div>
                  {item.error && (
                    <div className="truncate text-red-500/90">{item.error}</div>
                  )}
                </div>
              </div>
            ))}
          </div>
          {batchError && (
            <div className="px-4 py-2 border-t theme-border text-xs text-red-500">{batchError}</div>
          )}
        </div>
      )}

      {/* 方案与产物文档浏览弹窗 */}
      <ArtifactsModal
        isOpen={showArtifactModal}
        onClose={() => setShowArtifactModal(false)}
        artifacts={artifacts}
        currentArtifact={selectedArtifact}
        onSelectArtifact={(art) => setSelectedArtifact(art)}
      />
      {mergeSource && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-sm"
          onClick={() => !mergeBusy && setMergeSource(null)}
        >
          <div
            className="w-full max-w-md theme-bg-card border theme-border rounded-2xl shadow-2xl p-5 space-y-4"
            onClick={(e) => e.stopPropagation()}
          >
            <div>
              <h3 className="text-sm font-bold theme-text-main">{t('ws.mergeTitle')}</h3>
              <p className="text-[11px] theme-text-muted mt-1">{t('ws.mergeHint')}</p>
            </div>

            <div className="theme-bg-sub border theme-border rounded-lg p-3 space-y-1">
              <div className="text-[10px] theme-text-muted">{t('ws.mergeSource')}</div>
              <div className="text-xs theme-text-main font-mono break-all">{mergeSource.workspace_path}</div>
              <div className="text-[10px] theme-text-sub mt-1">
                {t('ws.mergeSourceCount', { n: mergeSource.cnt })}
              </div>
            </div>

            <div className="flex gap-2">
              <button
                type="button"
                onClick={() => setMergeMode('existing')}
                className={`flex-1 px-3 py-2 text-xs font-medium rounded-lg border transition-colors cursor-pointer ${
                  mergeMode === 'existing'
                    ? 'bg-blue-600/15 border-blue-500/50 text-blue-500'
                    : 'theme-bg-sub theme-border theme-text-muted hover:theme-text-main'
                }`}
              >
                {t('ws.mergeToExisting')}
              </button>
              <button
                type="button"
                onClick={() => setMergeMode('rename')}
                className={`flex-1 px-3 py-2 text-xs font-medium rounded-lg border transition-colors cursor-pointer ${
                  mergeMode === 'rename'
                    ? 'bg-blue-600/15 border-blue-500/50 text-blue-500'
                    : 'theme-bg-sub theme-border theme-text-muted hover:theme-text-main'
                }`}
              >
                {t('ws.renameToNew')}
              </button>
            </div>

            {mergeMode === 'existing' ? (
              <div className="space-y-1.5 max-h-48 overflow-y-auto">
                <label className="text-[11px] font-medium theme-text-muted block">{t('ws.mergePickTarget')}</label>
                {workspaces
                  .filter((w) => w.workspace_path !== mergeSource.workspace_path)
                  .map((w) => {
                    const name = w.workspace_path.split('/').slice(-1)[0] || w.workspace_path;
                    const selected = mergeTarget === w.workspace_path;
                    return (
                      <button
                        key={w.workspace_path}
                        type="button"
                        onClick={() => setMergeTarget(w.workspace_path)}
                        className={`w-full text-left px-3 py-2 rounded-lg border text-xs transition-colors cursor-pointer ${
                          selected
                            ? 'bg-blue-600/15 border-blue-500/50 text-blue-500 font-semibold'
                            : 'theme-bg-sub theme-border theme-text-muted hover:theme-text-main'
                        }`}
                      >
                        <div className="font-bold truncate">{name}</div>
                        <div className="text-[10px] theme-text-sub truncate font-mono">{w.workspace_path}</div>
                      </button>
                    );
                  })}
              </div>
            ) : (
              <div className="space-y-1.5">
                <label className="text-[11px] font-medium theme-text-muted block">{t('ws.mergeNewPath')}</label>
                <input
                  value={mergeTarget}
                  onChange={(e) => setMergeTarget(e.target.value)}
                  placeholder={t('ws.mergeNewPathPh')}
                  className="w-full px-3 py-2 text-sm theme-bg-input border theme-border rounded-lg theme-text-main font-mono focus:outline-none focus:border-blue-500"
                />
              </div>
            )}

            <p className="text-[10px] theme-text-sub leading-relaxed">{t('ws.mergeAnalysisNote')}</p>

            <div className="flex items-center justify-end gap-2 pt-1">
              <button
                type="button"
                onClick={() => setMergeSource(null)}
                disabled={mergeBusy}
                className="px-4 py-2 text-xs font-medium theme-bg-sub border theme-border rounded-lg theme-text-muted hover:theme-text-main transition-colors cursor-pointer disabled:opacity-50"
              >
                {t('ws.mergeCancel')}
              </button>
              <button
                type="button"
                onClick={handleMergeWorkspace}
                disabled={mergeBusy || !mergeTarget.trim()}
                className="px-4 py-2 text-xs font-medium bg-blue-600 text-white rounded-lg hover:bg-blue-500 transition-colors cursor-pointer disabled:opacity-50 flex items-center gap-1.5"
              >
                {mergeBusy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : null}
                <span>{mergeBusy ? t('ws.merging') : t('ws.mergeConfirm')}</span>
              </button>
            </div>
          </div>
        </div>
      )}
      {mergeToast && (
        <div className="fixed bottom-6 right-6 z-50 flex items-center gap-2 px-4 py-2.5 bg-slate-900/90 text-white text-xs font-medium rounded-xl shadow-2xl border border-white/10 backdrop-blur-md">
          <CheckCircle2 className="h-4 w-4 text-emerald-400 flex-shrink-0" />
          <span>{mergeToast}</span>
        </div>
      )}
      {moveConv && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-sm"
          onClick={() => !moveConvBusy && setMoveConv(null)}
        >
          <div
            className="w-full max-w-md theme-bg-card border theme-border rounded-2xl shadow-2xl p-5 space-y-4"
            onClick={(e) => e.stopPropagation()}
          >
            <div>
              <h3 className="text-sm font-bold theme-text-main">{t('conv.moveTitle')}</h3>
              <p className="text-[11px] theme-text-muted mt-1">{t('conv.moveHint')}</p>
            </div>

            <div className="theme-bg-sub border theme-border rounded-lg p-3 space-y-1">
              <div className="text-[10px] theme-text-muted">{t('conv.moveSession')}</div>
              <div className="text-xs theme-text-main truncate">{moveConv.title || t('conv.untitled')}</div>
              <div className="text-[10px] theme-text-sub font-mono truncate mt-1">{moveConv.id}</div>
            </div>

            <div className="space-y-1.5 max-h-48 overflow-y-auto">
              <label className="text-[11px] font-medium theme-text-muted block">{t('conv.movePickTarget')}</label>
              {workspaces
                .filter((w) => w.workspace_path !== selectedWorkspace)
                .map((w) => {
                  const name = w.workspace_path.split('/').slice(-1)[0] || w.workspace_path;
                  const selected = moveConvTarget === w.workspace_path;
                  return (
                    <button
                      key={w.workspace_path}
                      type="button"
                      onClick={() => setMoveConvTarget(w.workspace_path)}
                      className={`w-full text-left px-3 py-2 rounded-lg border text-xs transition-colors cursor-pointer ${
                        selected
                          ? 'bg-blue-600/15 border-blue-500/50 text-blue-500 font-semibold'
                          : 'theme-bg-sub theme-border theme-text-muted hover:theme-text-main'
                      }`}
                    >
                      <div className="font-bold truncate">{name}</div>
                      <div className="text-[10px] theme-text-sub truncate font-mono">{w.workspace_path}</div>
                    </button>
                  );
                })}
            </div>

            <div className="flex items-center justify-end gap-2 pt-1">
              <button
                type="button"
                onClick={() => setMoveConv(null)}
                disabled={moveConvBusy}
                className="px-4 py-2 text-xs font-medium theme-bg-sub border theme-border rounded-lg theme-text-muted hover:theme-text-main transition-colors cursor-pointer disabled:opacity-50"
              >
                {t('ws.mergeCancel')}
              </button>
              <button
                type="button"
                onClick={handleMoveConversation}
                disabled={moveConvBusy || !moveConvTarget.trim()}
                className="px-4 py-2 text-xs font-medium bg-blue-600 text-white rounded-lg hover:bg-blue-500 transition-colors cursor-pointer disabled:opacity-50 flex items-center gap-1.5"
              >
                {moveConvBusy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : null}
                <span>{moveConvBusy ? t('ws.merging') : t('conv.moveConfirm')}</span>
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
