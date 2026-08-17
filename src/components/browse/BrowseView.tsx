import React, { useState, useEffect } from 'react';
import type { WorkspaceStat, ConversationItem, MessageItem, DashboardStats } from '../../types';
import { api } from '../../api/tauriBridge';
import { WorkspaceAnalysisView } from './WorkspaceAnalysisView';
import { DashboardView } from '../dashboard/DashboardView';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import {
  LayoutDashboard,
  Star,
  Search,
  MessageSquare,
  Wrench,
  Clock,
  Sparkles,
  BarChart3,
  ArrowLeft,
} from 'lucide-react';

interface Props {
  selectedWorkspace: string;
  selectedConversationId: string;
  isStarredView: boolean;
  onSelectWorkspace: (ws: string) => void;
  onSelectConversation: (id: string) => void;
  onSwitchToDashboard: () => void;
  onSwitchToStarred: () => void;
  stats: DashboardStats | null;
  loadingStats: boolean;
  onRefreshStats: () => void;
}

export const BrowseView: React.FC<Props> = ({
  selectedWorkspace,
  selectedConversationId,
  isStarredView,
  onSelectWorkspace,
  onSelectConversation,
  onSwitchToDashboard,
  onSwitchToStarred,
  stats,
  loadingStats,
  onRefreshStats,
}) => {
  const [workspaces, setWorkspaces] = useState<WorkspaceStat[]>([]);
  const [conversations, setConversations] = useState<ConversationItem[]>([]);
  const [messages, setMessages] = useState<MessageItem[]>([]);
  const [currentConv, setCurrentConv] = useState<ConversationItem | null>(null);

  const [wsSearch, setWsSearch] = useState('');
  const [convSearch, setConvSearch] = useState('');
  const [loadingConv, setLoadingConv] = useState(false);
  const [starredCount, setStarredCount] = useState(0);
  const [expandedTurns, setExpandedTurns] = useState<Record<number, boolean>>({});

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
      if (isStarredView) {
        setStarredCount(list.length);
      }
    } catch (e) {
      console.error(e);
    }
  };

  useEffect(() => {
    loadConversations();
  }, [selectedWorkspace, convSearch, isStarredView]);

  // 加载会话详情
  useEffect(() => {
    if (!selectedConversationId) {
      setMessages([]);
      setCurrentConv(null);
      return;
    }
    const fetchMsgs = async () => {
      setLoadingConv(true);
      try {
        const msgs = await api.getConversationMessages(selectedConversationId);
        setMessages(msgs);
        const found = conversations.find((c) => c.id === selectedConversationId);
        if (found) setCurrentConv(found);
      } catch (e) {
        console.error(e);
      } finally {
        setLoadingConv(false);
      }
    };
    fetchMsgs();
  }, [selectedConversationId, conversations]);

  // 切换星标
  const handleToggleStar = async () => {
    if (!selectedConversationId) return;
    try {
      const isNowStarred = await api.toggleStar(selectedConversationId);
      if (currentConv) {
        setCurrentConv({ ...currentConv, is_starred: isNowStarred });
      }
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

  const formatRelativeTime = (timeStr?: string) => {
    if (!timeStr) return '';
    try {
      const d = new Date(timeStr.replace('Z', '+00:00'));
      if (isNaN(d.getTime())) return timeStr;
      const now = new Date();
      const diffSec = Math.floor((now.getTime() - d.getTime()) / 1000);
      if (diffSec < 0 || diffSec < 60) return '刚刚';
      const diffMin = Math.floor(diffSec / 60);
      if (diffMin < 60) return `${diffMin} 分钟前`;
      const diffHour = Math.floor(diffMin / 60);
      if (diffHour < 24) return `${diffHour} 小时前`;
      const diffDay = Math.floor(diffHour / 24);
      if (diffDay < 30) return `${diffDay} 天前`;
      const diffMonth = Math.floor(diffDay / 30);
      if (diffMonth < 12) return `${diffMonth} 个月前`;
      return `${Math.floor(diffDay / 365)} 年前`;
    } catch {
      return timeStr;
    }
  };

  const formatTime = (timeStr?: string) => {
    if (!timeStr) return '';
    if (timeStr.length >= 16) {
      return timeStr.substring(0, 16).replace('T', ' ');
    }
    return timeStr;
  };

  // 会话消息按 User Turn 轮次进行分组
  interface MessageTurn {
    user?: MessageItem;
    replies: MessageItem[];
  }
  const messageTurns: MessageTurn[] = [];
  let curTurn: MessageTurn | null = null;
  for (const msg of messages) {
    if (msg.sender === 'user') {
      curTurn = { user: msg, replies: [] };
      messageTurns.push(curTurn);
    } else {
      if (!curTurn) {
        curTurn = { replies: [] };
        messageTurns.push(curTurn);
      }
      curTurn.replies.push(msg);
    }
  }

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
              placeholder="搜索目录…"
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
              !selectedWorkspace && !isStarredView
                ? 'bg-blue-600/15 text-blue-500 border border-blue-500/30 shadow-xs font-semibold'
                : 'theme-text-muted hover:text-blue-500 hover:theme-bg-card'
            }`}
          >
            <div className="flex items-center gap-2">
              <LayoutDashboard className="h-4 w-4" />
              <span>全景数据大盘</span>
            </div>
            <span className="px-1.5 py-0.5 text-[10px] bg-blue-500/15 text-blue-500 rounded font-medium">大盘</span>
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
              <span>我的收藏</span>
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
              : '未分类';
            const isActive = !isStarredView && ws.workspace_path === selectedWorkspace;
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
                  {ws.workspace_path || '未分类目录'}
                </div>

                {/* Agent 来源标签 */}
                <div className="flex items-center gap-1.5 flex-wrap mt-2">
                  <span className="text-[10px] font-mono theme-text-muted">{ws.cnt} 会话</span>
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
                  <span>· 用户 {ws.user_message_count}</span>
                  <span>·</span>
                  <span>全部 {ws.message_count}</span>
                </div>
              </div>
            );
          })}
        </div>
      </aside>

      {/* 右侧主内容区域 */}
      {!selectedWorkspace && !isStarredView ? (
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
                      <span>我的收藏</span>
                    </>
                  ) : (
                    <>
                      <MessageSquare className="h-3.5 w-3.5 text-blue-500" />
                      <span>会话列表 ({conversations.length})</span>
                    </>
                  )}
                </div>

                {selectedWorkspace && !isStarredView && (
                  <button
                    onClick={() => onSelectConversation('')}
                    className="text-[11px] text-blue-500 hover:underline flex items-center gap-1 cursor-pointer"
                  >
                    <BarChart3 className="h-3 w-3" />
                    <span>查看项目分析</span>
                  </button>
                )}
              </div>

              <div className="relative">
                <Search className="absolute left-2.5 top-2.5 h-3.5 w-3.5 theme-text-sub" />
                <input
                  type="text"
                  placeholder="搜索会话标题…"
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
                        <span>{conv.title || '未命名会话'}</span>
                      </div>
                    </div>

                    <div className="flex items-center justify-between mt-2 pt-2 border-t theme-border-sub text-[10px] theme-text-sub">
                      <div className="flex items-center gap-1.5">
                        {getSourceBadge(conv.source_app)}
                        <span>{formatTime(conv.updated_at || conv.created_at)}</span>
                      </div>
                      <div className="flex items-center gap-1">
                        <span>用户 {conv.user_message_count}</span>
                        <span>·</span>
                        <span>全部 {conv.message_count}</span>
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
                        <span>返回项目分析</span>
                      </button>
                    </div>
                    <h2 className="text-base font-bold theme-text-main truncate flex items-center gap-2">
                      <span>{currentConv.title || '未命名会话'}</span>
                    </h2>
                    <div className="flex items-center gap-3 text-xs theme-text-muted mt-1">
                      {getSourceBadge(currentConv.source_app)}
                      <span className="truncate font-mono">{currentConv.workspace_path}</span>
                      <span>·</span>
                      <span>{messages.length} 条消息记录</span>
                    </div>
                  </div>

                  {/* 顶部操作区：展开/折叠全部 + 收藏按钮 */}
                  <div className="flex items-center gap-2">
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
                        ? '全部折叠'
                        : '全部展开'}
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
                      <span>{currentConv.is_starred ? '已收藏' : '收藏'}</span>
                    </button>
                  </div>
                </div>

                {/* 消息轮次滚动流（默认显示用户消息，Agent 消息折叠在用户轮次内） */}
                <div className="flex-1 overflow-y-auto p-6 space-y-4">
                  {loadingConv ? (
                    <div className="flex h-full items-center justify-center theme-text-muted">
                      <Clock className="h-6 w-6 animate-spin mr-2" /> 正在加载对话流…
                    </div>
                  ) : messageTurns.length === 0 ? (
                    <div className="py-12 text-center text-xs theme-text-sub">此会话暂无消息记录</div>
                  ) : (
                    messageTurns.map((turn, turnIdx) => {
                      const isExpanded = !!expandedTurns[turnIdx];
                      const userMsg = turn.user;
                      const replyCount = turn.replies.length;

                      return (
                        <div
                          key={turnIdx}
                          className="border theme-border rounded-2xl theme-bg-card p-4 transition-all shadow-xs max-w-4xl mx-auto space-y-3"
                        >
                          {/* 用户消息头部信息栏（点击可切换展开/折叠本轮 Agent 回复） */}
                          <div
                            onClick={() => toggleTurn(turnIdx)}
                            className="flex items-center justify-between gap-2 cursor-pointer select-none text-xs pb-2 border-b theme-border-sub"
                          >
                            <div className="flex items-center gap-2">
                              <span className="w-5 h-5 rounded-md bg-blue-600/15 text-blue-500 flex items-center justify-center font-bold text-[11px]">
                                {isExpanded ? '▼' : '▶'}
                              </span>
                              <span className="px-1.5 py-0.5 text-[10px] font-bold bg-blue-600 text-white rounded">
                                用户
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
                                <span>{replyCount} 条 Agent 回复</span>
                                <span className="text-[10px] theme-text-muted">
                                  {isExpanded ? '(点击收起)' : '(点击展开)'}
                                </span>
                              </div>
                            )}
                          </div>

                          {/* 用户 Prompt 提问内容（直接清晰展示） */}
                          {userMsg && (
                            <div className="text-xs leading-relaxed theme-text-main whitespace-pre-wrap font-sans pl-1">
                              {userMsg.text}
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
                                          {m.sender === 'system' ? '系统' : 'Agent / 模型'}
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
                                          <span>思考与推理过程 (Thinking)</span>
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
                                                <span>Tool: {tc.tool_name || tc.name || '工具调用'}</span>
                                              </div>
                                              <span className="text-[10px] theme-text-sub">查看参数</span>
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
                <p className="text-sm">从左侧选择一个项目工作区查看全景分析</p>
              </div>
            )}
          </main>
        </>
      )}
    </div>
  );
};
