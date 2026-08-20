import React, { useState, useEffect } from 'react';
import type { WorkspaceStat, ConversationItem, MessageItem, DashboardStats } from '../../types';
import { api, isTauri } from '../../api/tauriBridge';
import { formatBeijingTime, formatRelativeTime } from '../../utils/date';
import { useI18n } from '../../i18n';
import { listen } from '@tauri-apps/api/event';
import { WorkspaceAnalysisView } from './WorkspaceAnalysisView';
import { OpenInIdeMenu } from './OpenInIdeMenu';
import { DashboardView } from '../dashboard/DashboardView';
import { PromptLibraryView } from '../prompts/PromptLibraryView';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import {
  LayoutDashboard,
  Star,
  BookMarked,
  Search,
  MessageSquare,
  Wrench,
  Clock,
  Sparkles,
  BarChart3,
  ArrowLeft,
  ArrowUpDown,
  Maximize2,
  X,
} from 'lucide-react';

interface Props {
  selectedWorkspace: string;
  selectedConversationId: string;
  isStarredView: boolean;
  isPromptLibraryView: boolean;
  promptLibraryCount: number;
  onSelectWorkspace: (ws: string) => void;
  onSelectConversation: (id: string) => void;
  onSwitchToDashboard: () => void;
  onSwitchToStarred: () => void;
  onSwitchToPromptLibrary: () => void;
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
  promptLibraryCount,
  onSelectWorkspace,
  onSelectConversation,
  onSwitchToDashboard,
  onSwitchToStarred,
  onSwitchToPromptLibrary,
  onPromptLibraryCountChange,
  stats,
  loadingStats,
  onRefreshStats,
}) => {
  const { t } = useI18n();
  const [workspaces, setWorkspaces] = useState<WorkspaceStat[]>([]);
  const [conversations, setConversations] = useState<ConversationItem[]>([]);
  const [messages, setMessages] = useState<MessageItem[]>([]);
  const [currentConv, setCurrentConv] = useState<ConversationItem | null>(null);

  const [wsSearch, setWsSearch] = useState('');
  const [convSearch, setConvSearch] = useState('');
  const [loadingConv, setLoadingConv] = useState(false);
  const [starredCount, setStarredCount] = useState(0);
  const [expandedTurns, setExpandedTurns] = useState<Record<number, boolean>>({});
  const [sortOrder, setSortOrder] = useState<'asc' | 'desc'>('desc');
  const [lightboxImg, setLightboxImg] = useState<string | null>(null);

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

  // 切换会话时重置折叠状态
  useEffect(() => {
    setExpandedTurns({});
  }, [selectedConversationId]);

  // 加载工作区列表
  const loadWorkspaces = async () => {
    try {
      const list = await api.listWorkspaces(wsSearch);
      setWorkspaces(list);
    } catch (e) {
      console.error(e);
    }
  };

  useEffect(() => {
    loadWorkspaces();
  }, [wsSearch]);

  // 加载会话列表
  const loadConversations = async () => {
    try {
      const list = await api.listConversations(
        isStarredView ? undefined : selectedWorkspace,
        convSearch,
        isStarredView
      );
      setConversations(list);
    } catch (e) {
      console.error(e);
    }
  };

  useEffect(() => {
    loadConversations();
  }, [selectedWorkspace, convSearch, isStarredView]);

  // 加载收藏会话总数
  const loadStarredCount = async () => {
    try {
      const list = await api.listConversations(undefined, undefined, true);
      setStarredCount(list.length);
    } catch (e) {
      console.error(e);
    }
  };

  // 监听后台实时同步完成事件，自动刷新工作区与会话列表
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    listen('sync-completed', () => {
      loadWorkspaces();
      loadConversations();
      loadStarredCount();
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      if (unlisten) unlisten();
    };
  }, [selectedWorkspace, convSearch, isStarredView]);

  // 加载会话消息流
  useEffect(() => {
    if (!selectedConversationId) {
      setMessages([]);
      setCurrentConv(null);
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
  }, [selectedConversationId]);

  // 切换收藏状态
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
      default:
        return <span className="px-1.5 py-0.5 text-[10px] font-semibold theme-bg-sub theme-text-muted rounded">{source}</span>;
    }
  };

  const formatTime = formatBeijingTime;

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
              !selectedWorkspace && !isStarredView && !isPromptLibraryView
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
            const isActive = !isStarredView && !isPromptLibraryView && ws.workspace_path === selectedWorkspace;
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
                }`}
              >
                <div className="flex items-center justify-between">
                  <span className="font-bold theme-text-main text-xs truncate pr-1">{shortName}</span>
                  {ws.last_updated && (
                    <span className="text-[10px] theme-text-sub flex-shrink-0">
                      {formatRelativeTime(ws.last_updated)}
                    </span>
                  )}
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
      {isPromptLibraryView ? (
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
      ) : (
        <>
          {/* 第二栏：会话列表 */}
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
                    <button
                      onClick={() => onSelectConversation('')}
                      className="text-[11px] text-blue-500 hover:underline flex items-center gap-1 cursor-pointer"
                    >
                      <BarChart3 className="h-3 w-3" />
                      <span>{t('conv.analysis')}</span>
                    </button>
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
                return (
                  <div
                    key={conv.id}
                    onClick={() => onSelectConversation(conv.id)}
                    className={`p-3 rounded-xl border text-xs cursor-pointer transition-all ${
                      isSelected
                        ? 'bg-blue-600/15 border-blue-500/50 theme-text-main shadow-xs'
                        : 'theme-bg-card border-transparent hover:theme-border theme-text-muted hover:theme-text-main shadow-2xs'
                    }`}
                  >
                    <div className="flex items-start justify-between gap-1.5">
                      <div className="font-medium line-clamp-2 theme-text-main flex items-center gap-1">
                        {conv.is_starred && (
                          <Star className="h-3 w-3 fill-amber-400 text-amber-400 flex-shrink-0" />
                        )}
                        <span>{conv.title || t('conv.untitled')}</span>
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

          {/* 第三栏：主内容区域（按用户轮次折叠展示） */}
          <main className="flex-1 flex flex-col h-full overflow-hidden theme-bg-main">
            {selectedConversationId && currentConv ? (
              <>
                {/* 会话顶部 Header */}
                <div className="p-4 border-b theme-border flex items-center justify-between theme-bg-header backdrop-blur-sm">
                  <div className="min-w-0 pr-4">
                    <div className="flex items-center gap-2 mb-1">
                      <button
                        onClick={() => onSelectConversation('')}
                        className="text-xs text-blue-500 hover:underline flex items-center gap-1 cursor-pointer"
                      >
                        <ArrowLeft className="h-3.5 w-3.5" />
                        <span>{t('conv.backAnalysis')}</span>
                      </button>
                    </div>
                    <h2 className="text-base font-bold theme-text-main truncate flex items-center gap-2">
                      <span>{currentConv.title || t('conv.untitled')}</span>
                    </h2>
                    <div className="flex items-center gap-3 text-xs theme-text-muted mt-1">
                      {getSourceBadge(currentConv.source_app)}
                      <span className="truncate font-mono">{currentConv.workspace_path}</span>
                      <span>·</span>
                      <span>{t('conv.messages', { n: messages.length })}</span>
                    </div>
                  </div>

                  {/* 顶部操作区：正序/倒序 + 展开/折叠全部 + 收藏按钮 */}
                  <div className="flex items-center gap-2">
                    {/* 正序 / 倒序切换按钮 */}
                    <button
                      onClick={() => setSortOrder((prev) => (prev === 'asc' ? 'desc' : 'asc'))}
                      title={sortOrder === 'asc' ? t('conv.sortTitleAsc') : t('conv.sortTitleDesc')}
                      className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg border theme-border theme-bg-sub hover:opacity-80 text-xs font-medium theme-text-muted hover:theme-text-main transition-colors cursor-pointer shadow-xs"
                    >
                      <ArrowUpDown className="h-3.5 w-3.5 text-blue-500" />
                      <span>{sortOrder === 'asc' ? t('conv.sortAsc') : t('conv.sortDesc')}</span>
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
                      className="px-2.5 py-1.5 rounded-lg border theme-border theme-bg-sub hover:opacity-80 text-xs font-medium theme-text-muted hover:theme-text-main transition-colors cursor-pointer shadow-xs"
                    >
                      {messageTurns.length > 0 &&
                      Object.keys(expandedTurns).length === messageTurns.length &&
                      Object.values(expandedTurns).every(Boolean)
                        ? t('conv.collapseAll')
                        : t('conv.expandAll')}
                    </button>

                    <button
                      onClick={handleToggleStar}
                      className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg border text-xs font-medium transition-all cursor-pointer shadow-xs ${
                        currentConv.is_starred
                          ? 'bg-amber-500/15 border-amber-500/40 text-amber-500'
                          : 'theme-bg-sub hover:opacity-80 theme-border theme-text-main'
                      }`}
                    >
                      <Star
                        className={`h-3.5 w-3.5 ${
                          currentConv.is_starred ? 'fill-amber-400 text-amber-400' : 'theme-text-sub'
                        }`}
                      />
                      <span>{currentConv.is_starred ? t('conv.starredBtn') : t('conv.star')}</span>
                    </button>
                  </div>
                </div>

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
                                        <ReactMarkdown remarkPlugins={[remarkGfm]}>
                                          {m.text}
                                        </ReactMarkdown>
                                      </div>
                                    )}

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
            ) : selectedWorkspace ? (
              /* 工作区全景研发分析 */
              <WorkspaceAnalysisView workspacePath={selectedWorkspace} />
            ) : (
              <div className="flex-1 flex flex-col items-center justify-center theme-text-sub">
                <Sparkles className="h-10 w-10 mb-2 opacity-50" />
                <p className="text-sm">{t('conv.pickWorkspace')}</p>
              </div>
            )}
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
    </div>
  );
};
