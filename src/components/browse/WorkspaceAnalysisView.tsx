import React, { useState, useEffect } from 'react';
import type { WorkspaceDetailStats, WorkspaceFineBlock } from '../../types';
import { api } from '../../api/tauriBridge';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import {
  Layers,
  MessageSquare,
  Calendar,
  Zap,
  Boxes,
  FileText,
  Clock,
  ChevronDown,
  ChevronUp,
  Tag,
  Sparkles,
  Flame,
} from 'lucide-react';

interface Props {
  workspacePath: string;
}

export const WorkspaceAnalysisView: React.FC<Props> = ({ workspacePath }) => {
  const [detail, setDetail] = useState<WorkspaceDetailStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [activeTab, setActiveTab] = useState<'fine' | 'modules' | 'report'>('fine');
  const [expandedBlocks, setExpandedBlocks] = useState<Record<string, boolean>>({});

  useEffect(() => {
    if (!workspacePath) return;
    const fetchDetail = async () => {
      setLoading(true);
      try {
        const res = await api.getWorkspaceDetail(workspacePath);
        setDetail(res);
      } catch (e) {
        console.error('Failed to load workspace detail:', e);
      } finally {
        setLoading(false);
      }
    };
    fetchDetail();
  }, [workspacePath]);

  const toggleExpand = (id: string) => {
    setExpandedBlocks((prev) => ({ ...prev, [id]: !prev[id] }));
  };

  if (loading) {
    return (
      <div className="flex h-full flex-col items-center justify-center theme-text-muted">
        <Clock className="h-8 w-8 animate-spin text-blue-500 mb-3" />
        <p className="text-sm">正在加载工作区研发分析数据…</p>
      </div>
    );
  }

  if (!detail) {
    return (
      <div className="flex h-full items-center justify-center theme-text-sub">
        暂无该工作区的分析数据。
      </div>
    );
  }

  // 按月份对粗粒度 Blocks 进行分组
  const blocksByMonth: Record<string, WorkspaceFineBlock[]> = {};
  for (const block of detail.fine_blocks) {
    const month = block.start_date ? block.start_date.substring(0, 7) : '其他时段';
    if (!blocksByMonth[month]) {
      blocksByMonth[month] = [];
    }
    blocksByMonth[month].push(block);
  }

  const sortedMonths = Object.keys(blocksByMonth).sort();

  const getBlockTypeBadge = (type: string) => {
    switch (type.toLowerCase()) {
      case 'module':
        return <span className="px-1.5 py-0.5 text-[10px] font-semibold bg-purple-500/15 text-purple-500 border border-purple-500/30 rounded">模块</span>;
      case 'feature':
        return <span className="px-1.5 py-0.5 text-[10px] font-semibold bg-blue-500/15 text-blue-500 border border-blue-500/30 rounded">功能</span>;
      case 'refactor':
        return <span className="px-1.5 py-0.5 text-[10px] font-semibold bg-amber-500/15 text-amber-500 border border-amber-500/30 rounded">重构</span>;
      case 'bugfix':
      case 'fix':
        return <span className="px-1.5 py-0.5 text-[10px] font-semibold bg-emerald-500/15 text-emerald-500 border border-emerald-500/30 rounded">修复</span>;
      default:
        return <span className="px-1.5 py-0.5 text-[10px] font-semibold theme-bg-sub theme-text-muted border theme-border rounded">{type}</span>;
    }
  };

  return (
    <div className="h-full overflow-y-auto p-6 space-y-6 theme-bg-main theme-text-main">
      {/* 顶部标题 */}
      <div>
        <h1 className="text-xl font-bold tracking-tight theme-text-main flex items-center gap-2">
          <span>{detail.workspace_short}</span>
        </h1>
        <p className="text-xs theme-text-muted font-mono mt-0.5 break-all">{detail.workspace_path}</p>
      </div>

      {/* 4 大核心 KPI 卡片 */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        {/* 会话数 */}
        <div className="theme-bg-card border theme-border rounded-xl p-4 relative overflow-hidden shadow-xs">
          <div className="flex items-center justify-between theme-text-muted mb-2">
            <span className="text-xs font-medium">会话总数</span>
            <Layers className="h-4 w-4 text-blue-500" />
          </div>
          <div className="text-2xl font-bold theme-text-main tracking-tight font-mono">
            {detail.conversation_count}
          </div>
          <div className="mt-2 text-[11px] theme-text-muted truncate">
            {detail.agent_breakdown}
          </div>
        </div>

        {/* 用户提问 */}
        <div className="theme-bg-card border theme-border rounded-xl p-4 relative overflow-hidden shadow-xs">
          <div className="flex items-center justify-between theme-text-muted mb-2">
            <span className="text-xs font-medium">用户提问 / 提示词</span>
            <MessageSquare className="h-4 w-4 text-emerald-500" />
          </div>
          <div className="text-2xl font-bold theme-text-main tracking-tight font-mono">
            {detail.user_message_count.toLocaleString()}
          </div>
          <div className="mt-2 text-[11px] theme-text-muted">
            全部交互 {detail.message_count.toLocaleString()} 条
          </div>
        </div>

        {/* 活跃天数 */}
        <div className="theme-bg-card border theme-border rounded-xl p-4 relative overflow-hidden shadow-xs">
          <div className="flex items-center justify-between theme-text-muted mb-2">
            <span className="text-xs font-medium">累计活跃天数</span>
            <Calendar className="h-4 w-4 text-purple-500" />
          </div>
          <div className="text-2xl font-bold theme-text-main tracking-tight font-mono">
            {detail.active_days} <span className="text-xs font-normal theme-text-muted">天</span>
          </div>
          <div className="mt-2 text-[11px] theme-text-muted truncate">
            {detail.first_active && detail.last_active
              ? `${detail.first_active.substring(0, 10)} ~ ${detail.last_active.substring(0, 10)}`
              : '—'}
          </div>
        </div>

        {/* 峰值日消息 */}
        <div className="theme-bg-card border theme-border rounded-xl p-4 relative overflow-hidden shadow-xs">
          <div className="flex items-center justify-between theme-text-muted mb-2">
            <span className="text-xs font-medium">单日峰值消息</span>
            <Flame className="h-4 w-4 text-orange-500" />
          </div>
          <div className="text-2xl font-bold theme-text-main tracking-tight font-mono">
            {detail.peak_count.toLocaleString()} <span className="text-xs font-normal theme-text-muted">条</span>
          </div>
          <div className="mt-2 text-[11px] theme-text-muted">
            {detail.peak_day || '—'}
          </div>
        </div>
      </div>

      {/* 研发日历贡献热力图 */}
      <div className="theme-bg-card border theme-border rounded-xl p-5 shadow-xs">
        <div className="flex items-center justify-between mb-3">
          <div>
            <h2 className="text-sm font-semibold theme-text-main flex items-center gap-2">
              <Calendar className="h-4 w-4 text-blue-500" />
              研发日历 (52-Week Activity Heatmap)
            </h2>
            <p className="text-xs theme-text-muted mt-0.5">
              按用户消息与会话滚动统计，颜色越深表示当天研发活跃度越高
            </p>
          </div>
        </div>

        {/* 水平 52 周热力图格子 */}
        <div className="overflow-x-auto pb-2">
          <div className="flex gap-1 min-w-[700px]">
            {Array.from({ length: 52 }).map((_, weekIdx) => (
              <div key={weekIdx} className="flex flex-col gap-1">
                {Array.from({ length: 7 }).map((_, dayIdx) => {
                  const cellIdx = weekIdx * 7 + dayIdx;
                  const cell = detail.heatmap_cells[cellIdx];
                  if (!cell) return null;
                  return (
                    <div
                      key={cell.date}
                      title={`${cell.date}: ${cell.count} 条消息`}
                      className={`w-3 h-3 rounded-[2.5px] punchcard-cell lvl-${cell.level} transition-transform hover:scale-125 cursor-pointer`}
                    />
                  );
                })}
              </div>
            ))}
          </div>
        </div>

        {/* 底部图例 */}
        <div className="flex items-center justify-between text-[11px] theme-text-muted pt-2 border-t theme-border-sub">
          <span>滚动 52 周研发轨迹</span>
          <div className="flex items-center gap-1.5">
            <span>少</span>
            <span className="w-2.5 h-2.5 rounded-[2px] punchcard-cell lvl-0 inline-block border theme-border-sub" />
            <span className="w-2.5 h-2.5 rounded-[2px] punchcard-cell lvl-1 inline-block" />
            <span className="w-2.5 h-2.5 rounded-[2px] punchcard-cell lvl-2 inline-block" />
            <span className="w-2.5 h-2.5 rounded-[2px] punchcard-cell lvl-3 inline-block" />
            <span className="w-2.5 h-2.5 rounded-[2px] punchcard-cell lvl-4 inline-block" />
            <span>多</span>
          </div>
        </div>
      </div>

      {/* 研发分析三级颗粒度 */}
      <div className="theme-bg-card border theme-border rounded-xl p-5 shadow-xs space-y-4">
        <div className="flex items-center justify-between">
          <div>
            <h2 className="text-sm font-semibold theme-text-main flex items-center gap-2">
              <Sparkles className="h-4 w-4 text-purple-500" />
              研发分析 (R&D Intelligence)
            </h2>
            <p className="text-xs theme-text-muted mt-0.5">
              三级颗粒度：粗粒度 Blocks → 模块总览 → Markdown 架构报告
            </p>
          </div>

          {/* 三级颗粒度切换 Tab */}
          <div className="flex theme-bg-sub p-0.5 rounded-lg border theme-border text-xs">
            <button
              onClick={() => setActiveTab('fine')}
              className={`flex items-center gap-1 px-3 py-1 rounded-md font-medium transition-all cursor-pointer ${
                activeTab === 'fine'
                  ? 'bg-blue-600 text-white shadow-xs'
                  : 'theme-text-muted hover:theme-text-main'
              }`}
            >
              <Zap className="h-3 w-3" />
              <span>粗粒度 Blocks ({detail.fine_blocks.length})</span>
            </button>

            <button
              onClick={() => setActiveTab('modules')}
              className={`flex items-center gap-1 px-3 py-1 rounded-md font-medium transition-all cursor-pointer ${
                activeTab === 'modules'
                  ? 'bg-blue-600 text-white shadow-xs'
                  : 'theme-text-muted hover:theme-text-main'
              }`}
            >
              <Boxes className="h-3 w-3" />
              <span>模块总览 ({detail.module_blocks.length})</span>
            </button>

            <button
              onClick={() => setActiveTab('report')}
              className={`flex items-center gap-1 px-3 py-1 rounded-md font-medium transition-all cursor-pointer ${
                activeTab === 'report'
                  ? 'bg-blue-600 text-white shadow-xs'
                  : 'theme-text-muted hover:theme-text-main'
              }`}
            >
              <FileText className="h-3 w-3" />
              <span>Markdown 报告</span>
            </button>
          </div>
        </div>

        {/* Tab 1: 粗粒度 Blocks (带研发时间轴) */}
        {activeTab === 'fine' && (
          <div className="space-y-6 pt-2">
            {sortedMonths.length > 0 ? (
              sortedMonths.map((month) => (
                <div key={month} className="space-y-3">
                  <div className="flex items-center gap-2">
                    <span className="px-2.5 py-1 text-xs font-bold font-mono bg-blue-500/10 text-blue-500 border border-blue-500/20 rounded-lg">
                      {month}
                    </span>
                    <div className="h-px flex-1 theme-bg-sub" />
                    <span className="text-[11px] theme-text-sub">
                      {blocksByMonth[month].length} 个研发要点
                    </span>
                  </div>

                  <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                    {blocksByMonth[month].map((block) => {
                      const isExpanded = !!expandedBlocks[block.block_id || block.id.toString()];
                      return (
                        <div
                          key={block.id}
                          className="p-3.5 rounded-xl theme-bg-sub border theme-border hover:theme-border-hover transition-all text-xs space-y-2 shadow-2xs"
                        >
                          <div className="flex items-start justify-between gap-2">
                            <div className="flex items-center gap-2 flex-wrap">
                              {getBlockTypeBadge(block.type)}
                              <span className="font-bold theme-text-main">{block.title}</span>
                            </div>
                            {block.start_date && (
                              <span className="text-[10px] theme-text-sub font-mono flex-shrink-0">
                                {block.start_date}
                              </span>
                            )}
                          </div>

                          <p className={`theme-text-muted text-[11px] leading-relaxed ${isExpanded ? '' : 'line-clamp-2'}`}>
                            {block.summary}
                          </p>

                          {block.summary.length > 80 && (
                            <button
                              onClick={() => toggleExpand(block.block_id || block.id.toString())}
                              className="text-[10px] text-blue-500 hover:underline flex items-center gap-0.5 cursor-pointer pt-0.5"
                            >
                              <span>{isExpanded ? '收起详情' : '展开完整摘要'}</span>
                              {isExpanded ? <ChevronUp className="h-3 w-3" /> : <ChevronDown className="h-3 w-3" />}
                            </button>
                          )}

                          {block.keywords && block.keywords.length > 0 && (
                            <div className="flex items-center gap-1.5 flex-wrap pt-1">
                              <Tag className="h-3 w-3 theme-text-sub" />
                              {block.keywords.map((kw, i) => (
                                <span
                                  key={i}
                                  className="px-1.5 py-0.2 text-[9px] theme-bg-card border theme-border rounded text-slate-400 font-mono"
                                >
                                  {kw}
                                </span>
                              ))}
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                </div>
              ))
            ) : (
              <div className="py-12 text-center text-xs theme-text-sub">
                当前工作区暂无粗粒度 Blocks 数据，可通过 AI 供应商配置发起智能提取。
              </div>
            )}
          </div>
        )}

        {/* Tab 2: 模块总览 */}
        {activeTab === 'modules' && (
          <div className="space-y-3 pt-2">
            {detail.module_blocks.length > 0 ? (
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                {detail.module_blocks.map((mod) => (
                  <div
                    key={mod.id}
                    className="p-4 rounded-xl theme-bg-sub border theme-border hover:theme-border-hover transition-all space-y-2.5 shadow-2xs"
                  >
                    <div className="flex items-center justify-between">
                      <div className="flex items-center gap-2">
                        <Boxes className="h-4 w-4 text-purple-500" />
                        <span className="font-bold text-xs theme-text-main">{mod.title}</span>
                      </div>
                      {mod.start_date && (
                        <span className="text-[10px] font-mono theme-text-sub">{mod.start_date}</span>
                      )}
                    </div>

                    <p className="text-xs theme-text-muted leading-relaxed">{mod.summary}</p>

                    {mod.child_fine_ids && mod.child_fine_ids.length > 0 && (
                      <div className="pt-2 border-t theme-border-sub text-[11px] theme-text-sub flex items-center justify-between">
                        <span>关联细粒度 Blocks:</span>
                        <span className="font-mono font-medium text-blue-500">
                          {mod.child_fine_ids.length} 项
                        </span>
                      </div>
                    )}
                  </div>
                ))}
              </div>
            ) : (
              <div className="py-12 text-center text-xs theme-text-sub">
                当前工作区暂无模块总览数据。
              </div>
            )}
          </div>
        )}

        {/* Tab 3: Markdown 报告 */}
        {activeTab === 'report' && (
          <div className="pt-2">
            {detail.report_md ? (
              <div className="p-6 rounded-xl theme-bg-sub border theme-border markdown-body max-w-4xl mx-auto">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{detail.report_md}</ReactMarkdown>
              </div>
            ) : (
              <div className="py-12 text-center text-xs theme-text-sub">
                当前工作区暂未生成 Markdown 架构报告。
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
};
