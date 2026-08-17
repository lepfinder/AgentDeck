import React, { useState } from 'react';
import type { DashboardStats, TopRankItem } from '../../types';
import {
  Layers,
  MessageSquare,
  Flame,
  FolderGit2,
  Wrench,
  Sparkles,
  RefreshCw,
  Star,
  Clock,
  ArrowUpRight,
} from 'lucide-react';

interface Props {
  stats: DashboardStats | null;
  loading: boolean;
  onRefresh: () => void;
  onSelectConversation: (convId: string, workspacePath: string) => void;
}

export const DashboardView: React.FC<Props> = ({
  stats,
  loading,
  onRefresh,
  onSelectConversation,
}) => {
  const [agentTab, setAgentTab] = useState<'convs' | 'msgs'>('convs');
  const [punchcardTab, setPunchcardTab] = useState<'msgs' | 'convs'>('msgs');
  const [topRankTab, setTopRankTab] = useState<'all' | 'user'>('all');

  if (loading && !stats) {
    return (
      <div className="flex h-full flex-col items-center justify-center theme-text-muted">
        <RefreshCw className="h-8 w-8 animate-spin text-blue-500 mb-3" />
        <p className="text-sm">正在加载全景驾驶舱大盘数据…</p>
      </div>
    );
  }

  if (!stats) {
    return (
      <div className="flex h-full items-center justify-center theme-text-sub">
        暂无大盘数据，请先同步会话。
      </div>
    );
  }

  const agentData = agentTab === 'convs' ? stats.agent_comparison_convs : stats.agent_comparison_msgs;
  const punchcardData = punchcardTab === 'msgs' ? stats.punchcard_msgs : stats.punchcard_convs;
  const topRankData = topRankTab === 'all' ? stats.top_conversations_all : stats.top_conversations_user;

  return (
    <div className="h-full overflow-y-auto p-6 space-y-6">
      {/* 顶部标题与控制栏 */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-bold tracking-tight theme-text-main flex items-center gap-2.5">
            <Sparkles className="h-5 w-5 text-blue-500" />
            Agent 全景数据驾驶舱
          </h1>
          <p className="text-xs theme-text-muted mt-0.5">
            聚合全平台 AI 编码智能体会话资产、多维活动热力与代码交互透视
          </p>
        </div>
        <button
          onClick={onRefresh}
          disabled={loading}
          className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium theme-bg-sub hover:opacity-80 active:scale-95 theme-text-main rounded-lg border theme-border transition-all cursor-pointer shadow-sm"
        >
          <RefreshCw className={`h-3.5 w-3.5 ${loading ? 'animate-spin' : ''}`} />
          刷新数据
        </button>
      </div>

      {/* 4 大核心 KPI 指标卡片 */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        {/* 全量会话数 */}
        <div className="theme-bg-card border theme-border rounded-xl p-4 relative overflow-hidden backdrop-blur-sm">
          <div className="flex items-center justify-between theme-text-muted mb-2">
            <span className="text-xs font-medium">全量会话数</span>
            <Layers className="h-4 w-4 text-blue-500" />
          </div>
          <div className="text-2xl font-bold theme-text-main tracking-tight font-mono">
            {stats.total_conversations.toLocaleString()}
          </div>
          <div className="mt-2 flex items-center gap-2 text-[11px] theme-text-muted">
            <span className="text-amber-500 font-medium flex items-center gap-0.5">
              <Star className="h-3 w-3 fill-amber-400 text-amber-400" /> {stats.starred_count}
            </span>
            <span>个星标收藏</span>
          </div>
        </div>

        {/* 用户消息数 */}
        <div className="theme-bg-card border theme-border rounded-xl p-4 relative overflow-hidden backdrop-blur-sm">
          <div className="flex items-center justify-between theme-text-muted mb-2">
            <span className="text-xs font-medium">用户提问 / 提示词</span>
            <MessageSquare className="h-4 w-4 text-emerald-500" />
          </div>
          <div className="text-2xl font-bold theme-text-main tracking-tight font-mono">
            {stats.total_user_messages.toLocaleString()}
          </div>
          <div className="mt-2 text-[11px] theme-text-muted">
            人类开发者主动 Prompt 交互
          </div>
        </div>

        {/* 全部消息交互总数 */}
        <div className="theme-bg-card border theme-border rounded-xl p-4 relative overflow-hidden backdrop-blur-sm">
          <div className="flex items-center justify-between theme-text-muted mb-2">
            <span className="text-xs font-medium">交互消息总数</span>
            <Sparkles className="h-4 w-4 text-purple-500" />
          </div>
          <div className="text-2xl font-bold theme-text-main tracking-tight font-mono">
            {stats.total_messages.toLocaleString()}
          </div>
          <div className="mt-2 text-[11px] theme-text-muted">
            包含思考、回答与代码生成
          </div>
        </div>

        {/* 工具调用总数 */}
        <div className="theme-bg-card border theme-border rounded-xl p-4 relative overflow-hidden backdrop-blur-sm">
          <div className="flex items-center justify-between theme-text-muted mb-2">
            <span className="text-xs font-medium">工具调用执行 (Tool Usage)</span>
            <Wrench className="h-4 w-4 text-amber-500" />
          </div>
          <div className="text-2xl font-bold theme-text-main tracking-tight font-mono">
            {stats.total_tool_calls.toLocaleString()}
          </div>
          <div className="mt-2 text-[11px] theme-text-muted">
            覆盖 {stats.total_workspaces} 个项目工程目录
          </div>
        </div>
      </div>

      {/* 中部第一排：Agent 平台分布占比 + 24 小时活跃时段分布 */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Agent 平台分布占比 */}
        <div className="theme-bg-card border theme-border rounded-xl p-5 backdrop-blur-sm flex flex-col justify-between">
          <div>
            <div className="flex items-center justify-between mb-4">
              <h2 className="text-sm font-semibold theme-text-main flex items-center gap-2">
                <Layers className="h-4 w-4 text-blue-500" />
                Agent 平台分布占比
              </h2>
              {/* 双维切换 Tab */}
              <div className="flex theme-bg-sub p-0.5 rounded-lg border theme-border text-xs">
                <button
                  onClick={() => setAgentTab('convs')}
                  className={`flex items-center gap-1 px-2.5 py-1 rounded-md font-medium transition-all cursor-pointer ${
                    agentTab === 'convs'
                      ? 'bg-blue-600 text-white shadow-sm'
                      : 'theme-text-muted hover:theme-text-main'
                  }`}
                >
                  <Layers className="h-3 w-3" /> 按会话数
                </button>
                <button
                  onClick={() => setAgentTab('msgs')}
                  className={`flex items-center gap-1 px-2.5 py-1 rounded-md font-medium transition-all cursor-pointer ${
                    agentTab === 'msgs'
                      ? 'bg-blue-600 text-white shadow-sm'
                      : 'theme-text-muted hover:theme-text-main'
                  }`}
                >
                  <MessageSquare className="h-3 w-3" /> 按消息数
                </button>
              </div>
            </div>

            {/* 彩色复合进度条 */}
            <div className="h-3 w-full theme-bg-sub rounded-full overflow-hidden flex mb-5 border theme-border-sub">
              {agentData.map((item) => (
                <div
                  key={item.app}
                  style={{ width: `${Math.max(1, item.percent)}%`, backgroundColor: item.color }}
                  title={`${item.label}: ${item.count.toLocaleString()} (${item.percent}%)`}
                  className="h-full transition-all duration-500 first:rounded-l-full last:rounded-r-full"
                />
              ))}
            </div>

            {/* 各 Agent 明细列表 */}
            <div className="grid grid-cols-2 sm:grid-cols-3 gap-3">
              {agentData.map((item) => (
                <div
                  key={item.app}
                  className="theme-bg-sub border theme-border rounded-lg p-2.5 flex items-center justify-between"
                >
                  <div className="flex items-center gap-2">
                    <span
                      className="w-2.5 h-2.5 rounded-full flex-shrink-0"
                      style={{ backgroundColor: item.color }}
                    />
                    <span className="text-xs font-medium theme-text-main">{item.label}</span>
                  </div>
                  <div className="text-right">
                    <div className="text-xs font-bold theme-text-main font-mono">{item.count.toLocaleString()}</div>
                    <div className="text-[10px] theme-text-muted">{item.percent}%</div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>

        {/* 24 小时活跃时段分布 */}
        <div className="theme-bg-card border theme-border rounded-xl p-5 backdrop-blur-sm flex flex-col justify-between">
          <div>
            <div className="flex items-center justify-between mb-4">
              <h2 className="text-sm font-semibold theme-text-main flex items-center gap-2">
                <Clock className="h-4 w-4 text-emerald-500" />
                24 小时活跃时段分布 (Hourly Punchcard)
              </h2>
              {/* 双维切换 Tab */}
              <div className="flex theme-bg-sub p-0.5 rounded-lg border theme-border text-xs">
                <button
                  onClick={() => setPunchcardTab('msgs')}
                  className={`px-2.5 py-1 rounded-md font-medium transition-all cursor-pointer ${
                    punchcardTab === 'msgs'
                      ? 'bg-blue-600 text-white shadow-sm'
                      : 'theme-text-muted hover:theme-text-main'
                  }`}
                >
                  按消息数
                </button>
                <button
                  onClick={() => setPunchcardTab('convs')}
                  className={`px-2.5 py-1 rounded-md font-medium transition-all cursor-pointer ${
                    punchcardTab === 'convs'
                      ? 'bg-blue-600 text-white shadow-sm'
                      : 'theme-text-muted hover:theme-text-main'
                  }`}
                >
                  按会话数
                </button>
              </div>
            </div>

            {/* 24 小时格子矩阵 */}
            <div className="grid grid-cols-12 gap-1.5 my-3">
              {punchcardData.map((slot) => (
                <div
                  key={slot.hour}
                  title={`${slot.hour}:00 - ${slot.count.toLocaleString()} 次 (${slot.percent}%)`}
                  className={`punchcard-cell lvl-${slot.level} rounded-md h-12 flex flex-col items-center justify-center transition-all hover:scale-105 cursor-pointer border theme-border-sub`}
                >
                  <span className="text-[10px] font-bold">{slot.hour}h</span>
                  <span className="text-[9px] opacity-80">{slot.count}</span>
                </div>
              ))}
            </div>

            {/* 底部图例 */}
            <div className="flex items-center justify-between text-[11px] theme-text-muted pt-2 border-t theme-border-sub">
              <span>0h (午夜)</span>
              <div className="flex items-center gap-1.5">
                <span>低</span>
                <span className="w-3 h-3 rounded punchcard-cell lvl-0 inline-block border theme-border-sub" />
                <span className="w-3 h-3 rounded punchcard-cell lvl-1 inline-block" />
                <span className="w-3 h-3 rounded punchcard-cell lvl-2 inline-block" />
                <span className="w-3 h-3 rounded punchcard-cell lvl-3 inline-block" />
                <span className="w-3 h-3 rounded punchcard-cell lvl-4 inline-block" />
                <span>高</span>
              </div>
              <span>23h (深夜)</span>
            </div>
          </div>
        </div>
      </div>

      {/* 中部第二排：深度会话排行榜 Top 10 + 工具调用分布与热门项目 */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* 深度会话排行榜 Top 10（占 2 列） */}
        <div className="lg:col-span-2 theme-bg-card border theme-border rounded-xl p-5 backdrop-blur-sm">
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-sm font-semibold theme-text-main flex items-center gap-2">
              <Flame className="h-4 w-4 text-orange-500" />
              深度会话排行榜 Top 10
            </h2>
            <div className="flex theme-bg-sub p-0.5 rounded-lg border theme-border text-xs">
              <button
                onClick={() => setTopRankTab('all')}
                className={`px-2.5 py-1 rounded-md font-medium transition-all cursor-pointer ${
                  topRankTab === 'all'
                    ? 'bg-blue-600 text-white shadow-sm'
                    : 'theme-text-muted hover:theme-text-main'
                }`}
              >
                全部消息数
              </button>
              <button
                onClick={() => setTopRankTab('user')}
                className={`px-2.5 py-1 rounded-md font-medium transition-all cursor-pointer ${
                  topRankTab === 'user'
                    ? 'bg-blue-600 text-white shadow-sm'
                    : 'theme-text-muted hover:theme-text-main'
                }`}
              >
                用户提问数
              </button>
            </div>
          </div>

          <div className="space-y-2">
            {topRankData.map((item: TopRankItem, idx: number) => (
              <div
                key={item.id}
                onClick={() => onSelectConversation(item.id, item.workspace_path)}
                className="group flex items-center justify-between p-2.5 rounded-lg theme-bg-sub hover:opacity-90 border theme-border hover:theme-border-hover transition-all cursor-pointer shadow-xs"
              >
                <div className="flex items-center gap-3 min-w-0 pr-2">
                  <span
                    className={`w-5 h-5 flex-shrink-0 rounded-full flex items-center justify-center text-[11px] font-bold ${
                      idx === 0
                        ? 'bg-amber-500/20 text-amber-500 border border-amber-500/30'
                        : idx === 1
                        ? 'bg-slate-500/20 text-slate-400 border border-slate-500/30'
                        : idx === 2
                        ? 'bg-amber-700/20 text-amber-600 border border-amber-700/30'
                        : 'theme-text-sub'
                    }`}
                  >
                    {idx + 1}
                  </span>
                  <div className="min-w-0">
                    <div className="text-xs font-medium theme-text-main group-hover:text-blue-500 truncate transition-colors flex items-center gap-1.5">
                      {item.is_starred && (
                        <Star className="h-3 w-3 text-amber-400 fill-amber-400 flex-shrink-0" />
                      )}
                      <span>{item.title}</span>
                    </div>
                    <div className="text-[10px] theme-text-muted flex items-center gap-2 mt-0.5">
                      <span className="text-blue-500 font-mono">{item.source_label}</span>
                      <span>·</span>
                      <span className="truncate">{item.workspace_short}</span>
                    </div>
                  </div>
                </div>
                <div className="flex items-center gap-3 flex-shrink-0 text-right">
                  <div className="text-xs font-bold theme-text-main font-mono">
                    {item.message_count} 条
                    <span className="text-[10px] font-normal theme-text-muted block">
                      用户 {item.user_message_count}
                    </span>
                  </div>
                  <ArrowUpRight className="h-3.5 w-3.5 theme-text-muted group-hover:theme-text-main transition-colors" />
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* 右侧：工具调用分布 + 热门项目 */}
        <div className="space-y-6">
          {/* 工具调用分布 */}
          <div className="theme-bg-card border theme-border rounded-xl p-5 backdrop-blur-sm">
            <h2 className="text-sm font-semibold theme-text-main flex items-center gap-2 mb-3">
              <Wrench className="h-4 w-4 text-emerald-500" />
              Agent 工具调用分布
            </h2>
            <div className="space-y-2.5">
              {stats.tool_usage.map((tool) => (
                <div key={tool.category} className="space-y-1">
                  <div className="flex items-center justify-between text-xs">
                    <span className="theme-text-main font-medium">{tool.category}</span>
                    <span className="theme-text-muted font-mono">
                      {tool.count.toLocaleString()} ({tool.percent}%)
                    </span>
                  </div>
                  <div className="h-1.5 w-full theme-bg-sub rounded-full overflow-hidden border theme-border-sub">
                    <div
                      className="h-full rounded-full transition-all duration-500"
                      style={{ width: `${tool.percent}%`, backgroundColor: tool.color }}
                    />
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* 热门项目工作区分布 */}
          <div className="theme-bg-card border theme-border rounded-xl p-5 backdrop-blur-sm">
            <h2 className="text-sm font-semibold theme-text-main flex items-center gap-2 mb-3">
              <FolderGit2 className="h-4 w-4 text-purple-500" />
              热门项目工作区分布 Top 8
            </h2>
            <div className="space-y-2">
              {stats.top_workspaces.slice(0, 8).map((ws) => (
                <div
                  key={ws.path}
                  className="p-2 rounded theme-bg-sub border theme-border flex items-center justify-between text-xs"
                >
                  <div className="min-w-0 pr-2">
                    <div className="font-medium theme-text-main truncate">{ws.short_name}</div>
                    <div className="text-[10px] theme-text-muted truncate">{ws.path}</div>
                  </div>
                  <div className="text-right flex-shrink-0">
                    <div className="font-bold theme-text-main font-mono">{ws.count} 会话</div>
                    <div className="text-[10px] theme-text-muted">{ws.percent}%</div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
