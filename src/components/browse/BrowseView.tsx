import React, { useState, useEffect } from 'react';
import type { WorkspaceStat, ConversationItem, MessageItem } from '../../types';
import { api } from '../../api/tauriBridge';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import {
  LayoutDashboard,
  Star,
  Search,
  MessageSquare,
  Bot,
  User,
  Wrench,
  Clock,
  Sparkles,
} from 'lucide-react';

interface Props {
  selectedWorkspace: string;
  selectedConversationId: string;
  isStarredView: boolean;
  onSelectWorkspace: (ws: string) => void;
  onSelectConversation: (id: string) => void;
  onSwitchToDashboard: () => void;
  onSwitchToStarred: () => void;
}

export const BrowseView: React.FC<Props> = ({
  selectedWorkspace,
  selectedConversationId,
  isStarredView,
  onSelectWorkspace,
  onSelectConversation,
  onSwitchToDashboard,
  onSwitchToStarred,
}) => {
  const [workspaces, setWorkspaces] = useState<WorkspaceStat[]>([]);
  const [conversations, setConversations] = useState<ConversationItem[]>([]);
  const [messages, setMessages] = useState<MessageItem[]>([]);
  const [currentConv, setCurrentConv] = useState<ConversationItem | null>(null);

  const [wsSearch, setWsSearch] = useState('');
  const [convSearch, setConvSearch] = useState('');
  const [loadingConv, setLoadingConv] = useState(false);
  const [starredCount, setStarredCount] = useState(0);

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
      // 默认选中第一个
      if (list.length > 0 && !selectedConversationId) {
        onSelectConversation(list[0].id);
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
        return <span className="px-1.5 py-0.5 text-[10px] font-medium bg-blue-500/15 text-blue-500 border border-blue-500/30 rounded">Cursor</span>;
      case 'antigravity':
        return <span className="px-1.5 py-0.5 text-[10px] font-medium bg-emerald-500/15 text-emerald-500 border border-emerald-500/30 rounded">AG</span>;
      case 'claude':
        return <span className="px-1.5 py-0.5 text-[10px] font-medium bg-orange-500/15 text-orange-500 border border-orange-500/30 rounded">Claude</span>;
      case 'hermes':
        return <span className="px-1.5 py-0.5 text-[10px] font-medium bg-purple-500/15 text-purple-500 border border-purple-500/30 rounded">Hermes</span>;
      case 'codex':
        return <span className="px-1.5 py-0.5 text-[10px] font-medium bg-pink-500/15 text-pink-500 border border-pink-500/30 rounded">Codex</span>;
      case 'workbuddy':
        return <span className="px-1.5 py-0.5 text-[10px] font-medium bg-cyan-500/15 text-cyan-500 border border-cyan-500/30 rounded">WorkBuddy</span>;
      default:
        return <span className="px-1.5 py-0.5 text-[10px] font-medium theme-bg-sub theme-text-muted rounded">{source}</span>;
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
              placeholder="搜索项目工作区…"
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
            className="w-full flex items-center justify-between px-3 py-2 text-xs font-medium text-blue-500 hover:opacity-80 rounded-lg transition-colors cursor-pointer"
          >
            <div className="flex items-center gap-2">
              <LayoutDashboard className="h-4 w-4" />
              <span>全景数据看板</span>
            </div>
            <span className="px-1.5 py-0.5 text-[10px] bg-blue-500/15 text-blue-500 rounded font-medium">大盘</span>
          </button>

          <button
            onClick={onSwitchToStarred}
            className={`w-full flex items-center justify-between px-3 py-2 text-xs font-medium rounded-lg transition-colors cursor-pointer ${
              isStarredView
                ? 'bg-amber-500/15 text-amber-500 border border-amber-500/30'
                : 'text-amber-500 hover:opacity-80'
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
        <div className="flex-1 overflow-y-auto p-2 space-y-1">
          {workspaces.map((ws) => {
            const shortName = ws.workspace_path
              ? ws.workspace_path.split('/').slice(-2).join('/')
              : '默认工作区';
            const isActive = !isStarredView && ws.workspace_path === selectedWorkspace;
            return (
              <div
                key={ws.workspace_path}
                onClick={() => onSelectWorkspace(ws.workspace_path)}
                className={`p-2.5 rounded-lg border text-xs cursor-pointer transition-all ${
                  isActive
                    ? 'bg-blue-600/15 border-blue-500/50 theme-text-main font-semibold shadow-2xs'
                    : 'bg-transparent border-transparent hover:theme-bg-card theme-text-muted hover:theme-text-main'
                }`}
              >
                <div className="truncate">{shortName}</div>
                <div className="text-[10px] theme-text-sub truncate mt-0.5">
                  {ws.workspace_path || 'Global Workspace'}
                </div>
                <div className="flex items-center gap-1.5 mt-2 text-[10px] theme-text-sub">
                  <span className="font-mono">{ws.cnt} 会话</span>
                  <span>·</span>
                  <span>用户 {ws.user_message_count}</span>
                </div>
              </div>
            );
          })}
        </div>
      </aside>

      {/* 第二栏：会话列表 */}
      <section className="w-80 border-r theme-border flex flex-col theme-bg-sub flex-shrink-0">
        <div className="p-3 border-b theme-border space-y-2">
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

        <div className="flex-1 overflow-y-auto p-2 space-y-1">
          {conversations.map((conv) => {
            const isSelected = conv.id === selectedConversationId;
            return (
              <div
                key={conv.id}
                onClick={() => onSelectConversation(conv.id)}
                className={`p-3 rounded-lg border text-xs cursor-pointer transition-all ${
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
                <div className="flex items-center justify-between mt-2.5 pt-2 border-t theme-border-sub text-[10px] theme-text-sub">
                  <div className="flex items-center gap-1.5">
                    {getSourceBadge(conv.source_app)}
                    <span>{conv.message_count} 消息</span>
                  </div>
                  <span>用户 {conv.user_message_count}</span>
                </div>
              </div>
            );
          })}
        </div>
      </section>

      {/* 第三栏：会话流详情 */}
      <main className="flex-1 flex flex-col h-full overflow-hidden theme-bg-main">
        {currentConv ? (
          <>
            {/* 顶部 Header */}
            <div className="p-4 border-b theme-border flex items-center justify-between theme-bg-header backdrop-blur-sm">
              <div className="min-w-0 pr-4">
                <h2 className="text-base font-bold theme-text-main truncate flex items-center gap-2">
                  <span>{currentConv.title || '未命名会话'}</span>
                </h2>
                <div className="flex items-center gap-3 text-xs theme-text-muted mt-1">
                  {getSourceBadge(currentConv.source_app)}
                  <span className="truncate">{currentConv.workspace_path}</span>
                  <span>·</span>
                  <span>{messages.length} 条消息记录</span>
                </div>
              </div>

              {/* 收藏按钮 */}
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

            {/* 消息滚动流 */}
            <div className="flex-1 overflow-y-auto p-6 space-y-6">
              {loadingConv ? (
                <div className="flex h-full items-center justify-center theme-text-muted">
                  <Clock className="h-6 w-6 animate-spin mr-2" /> 正在加载对话流…
                </div>
              ) : (
                messages.map((msg) => {
                  const isUser = msg.sender === 'user';
                  let toolCalls = [];
                  if (msg.tool_calls_json) {
                    try {
                      toolCalls = JSON.parse(msg.tool_calls_json);
                    } catch (e) {}
                  }

                  return (
                    <div
                      key={msg.id}
                      className={`flex gap-3.5 max-w-4xl mx-auto ${isUser ? 'flex-row-reverse' : 'flex-row'}`}
                    >
                      {/* 头像 */}
                      <div
                        className={`w-8 h-8 rounded-full flex items-center justify-center flex-shrink-0 text-white ${
                          isUser
                            ? 'bg-blue-600 shadow-md shadow-blue-500/20'
                            : 'bg-emerald-600 shadow-md shadow-emerald-500/20'
                        }`}
                      >
                        {isUser ? <User className="h-4 w-4" /> : <Bot className="h-4 w-4" />}
                      </div>

                      {/* 消息主体 */}
                      <div className={`space-y-2 max-w-[85%] ${isUser ? 'items-end' : 'items-start'}`}>
                        <div
                          className={`p-4 rounded-2xl border ${
                            isUser
                              ? 'bg-blue-600/10 border-blue-500/30 theme-text-main rounded-tr-none shadow-xs'
                              : 'theme-bg-card theme-border theme-text-main rounded-tl-none shadow-sm'
                          }`}
                        >
                          <div className="markdown-body">
                            <ReactMarkdown remarkPlugins={[remarkGfm]}>
                              {msg.text || (toolCalls.length > 0 ? '*执行工具调用…*' : '')}
                            </ReactMarkdown>
                          </div>

                          {/* 工具调用折叠展示 */}
                          {toolCalls.length > 0 && (
                            <div className="mt-3 pt-3 border-t theme-border-sub space-y-2">
                              <div className="text-[11px] font-semibold text-emerald-500 flex items-center gap-1">
                                <Wrench className="h-3 w-3" /> 工具调用 ({toolCalls.length})
                              </div>
                              {toolCalls.map((tc: any, idx: number) => (
                                <details
                                  key={idx}
                                  className="theme-bg-sub p-2.5 rounded-lg border theme-border text-xs group"
                                >
                                  <summary className="cursor-pointer font-mono theme-text-main flex items-center justify-between">
                                    <span>{tc.name || tc.tool_name || 'Tool Call'}</span>
                                    <span className="text-[10px] theme-text-sub">查看参数与结果</span>
                                  </summary>
                                  <pre className="mt-2 text-[11px] theme-text-muted theme-bg-main p-2 rounded overflow-x-auto border theme-border-sub">
                                    {JSON.stringify(tc, null, 2)}
                                  </pre>
                                </details>
                              ))}
                            </div>
                          )}
                        </div>

                        {/* 元数据：耗时、Token 与时间 */}
                        <div className="flex items-center gap-2 text-[10px] theme-text-sub px-1">
                          {msg.model_name && <span className="font-mono">{msg.model_name}</span>}
                          {msg.token_count ? <span>· {msg.token_count} tokens</span> : null}
                          {msg.created_at && <span>· {msg.created_at}</span>}
                        </div>
                      </div>
                    </div>
                  );
                })
              )}
            </div>
          </>
        ) : (
          <div className="flex-1 flex flex-col items-center justify-center theme-text-sub">
            <Sparkles className="h-10 w-10 mb-2 opacity-50" />
            <p className="text-sm">从左侧选择一个会话开始复盘</p>
          </div>
        )}
      </main>
    </div>
  );
};
