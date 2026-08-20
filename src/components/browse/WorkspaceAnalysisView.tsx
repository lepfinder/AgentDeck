import React, { useState, useEffect } from 'react';
import type { WorkspaceDetailStats, WorkspaceFineBlock } from '../../types';
import { api } from '../../api/tauriBridge';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import {
  runExtractFineBlocksPipeline,
  runMergeModulesPipeline,
  runGenerateReportPipeline,
  getAiEndpoints,
  type PipelineProgress,
} from '../../services/analysisPipeline';
import {
  Layers,
  MessageSquare,
  Calendar,
  Zap,
  Boxes,
  FileText,
  Clock,
  Sparkles,
  Flame,
  RefreshCw,
  X,
  CheckCircle2,
  AlertTriangle,
} from 'lucide-react';
import { ContributionHeatmap } from '../common/ContributionHeatmap';
import { useI18n } from '../../i18n';

interface Props {
  workspacePath: string;
}

export const WorkspaceAnalysisView: React.FC<Props> = ({ workspacePath }) => {
  const { t } = useI18n();
  const [detail, setDetail] = useState<WorkspaceDetailStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [activeTab, setActiveTab] = useState<'fine' | 'modules' | 'report'>('fine');
  const [selectedBlock, setSelectedBlock] = useState<WorkspaceFineBlock | null>(null);
  const [extracting, setExtracting] = useState(false);
  const [extractMessage, setExtractMessage] = useState<string | null>(null);
  const [progressInfo, setProgressInfo] = useState<PipelineProgress | null>(null);

  const fetchDetail = async () => {
    if (!workspacePath) return;
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

  useEffect(() => {
    fetchDetail();
  }, [workspacePath]);

  // 1. 细粒度 Blocks 智能提取
  const handleExtractBlocks = async (force: boolean) => {
    const endpoints = getAiEndpoints();
    if (!endpoints.hasKey) {
      setExtractMessage(t('analysis.needKeyExtract'));
      setTimeout(() => setExtractMessage(null), 5000);
      return;
    }

    setExtracting(true);
    setProgressInfo(null);
    setExtractMessage(force ? t('analysis.reextracting') : t('analysis.extracting'));

    try {
      const res = await runExtractFineBlocksPipeline(workspacePath, force, (p) => {
        setProgressInfo(p);
        setExtractMessage(p.detail);
      });

      if (res.success) {
        setExtractMessage(`🎉 ${res.message}`);
        await fetchDetail();
        setActiveTab('fine');
      } else {
        setExtractMessage(t('analysis.extractFail', { msg: res.message }));
      }
    } catch (err: any) {
      console.error('Extract blocks error:', err);
      setExtractMessage(t('analysis.extractErr', { msg: err?.message || err }));
    } finally {
      setExtracting(false);
      setTimeout(() => {
        setProgressInfo(null);
      }, 3000);
    }
  };

  // 2. 合并为模块总览
  const handleMergeModules = async (force: boolean) => {
    const endpoints = getAiEndpoints();
    if (!endpoints.hasKey) {
      setExtractMessage(t('analysis.needKeyMerge'));
      setTimeout(() => setExtractMessage(null), 5000);
      return;
    }

    if (!detail || detail.fine_blocks.length === 0) {
      setExtractMessage(t('analysis.needFine'));
      setTimeout(() => setExtractMessage(null), 4000);
      return;
    }

    setExtracting(true);
    setProgressInfo(null);
    setExtractMessage(t('analysis.merging'));

    try {
      const res = await runMergeModulesPipeline(workspacePath, detail.fine_blocks, detail, force, (p) => {
        setProgressInfo(p);
        setExtractMessage(p.detail);
      });

      if (res.success) {
        setExtractMessage(`🎉 ${res.message}`);
        await fetchDetail();
        setActiveTab('modules');
      } else {
        setExtractMessage(t('analysis.mergeFail', { msg: res.message }));
      }
    } catch (err: any) {
      console.error('Merge modules error:', err);
      setExtractMessage(t('analysis.mergeErr', { msg: err?.message || err }));
    } finally {
      setExtracting(false);
      setTimeout(() => {
        setProgressInfo(null);
      }, 3000);
    }
  };

  // 3. 生成 Markdown 架构演进报告
  const handleGenerateReport = async (_force: boolean) => {
    const endpoints = getAiEndpoints();
    if (!endpoints.hasKey) {
      setExtractMessage(t('analysis.needKeyReport'));
      setTimeout(() => setExtractMessage(null), 5000);
      return;
    }

    if (!detail || (detail.module_blocks.length === 0 && detail.fine_blocks.length === 0)) {
      setExtractMessage(t('analysis.needBlocks'));
      setTimeout(() => setExtractMessage(null), 4000);
      return;
    }

    setExtracting(true);
    setProgressInfo(null);
    setExtractMessage(t('analysis.reporting'));

    try {
      const res = await runGenerateReportPipeline(
        workspacePath,
        detail,
        detail.module_blocks,
        detail.fine_blocks,
        (p) => {
          setProgressInfo(p);
          setExtractMessage(p.detail);
        }
      );

      if (res.success) {
        setExtractMessage(`🎉 ${res.message}`);
        await fetchDetail();
        setActiveTab('report');
      } else {
        setExtractMessage(t('analysis.reportFail', { msg: res.message }));
      }
    } catch (err: any) {
      console.error('Generate report error:', err);
      setExtractMessage(t('analysis.reportErr', { msg: err?.message || err }));
    } finally {
      setExtracting(false);
      setTimeout(() => {
        setProgressInfo(null);
      }, 3000);
    }
  };

  if (loading) {
    return (
      <div className="flex h-full flex-col items-center justify-center theme-text-muted">
        <Clock className="h-8 w-8 animate-spin text-blue-500 mb-3" />
        <p className="text-sm">{t('analysis.loading')}</p>
      </div>
    );
  }

  if (!detail) {
    return (
      <div className="flex h-full items-center justify-center theme-text-sub">
        {t('analysis.empty')}
      </div>
    );
  }

  // 按月份对粗粒度 Blocks 进行分组
  const blocksByMonth: Record<string, WorkspaceFineBlock[]> = {};
  const undatedBlocks: WorkspaceFineBlock[] = [];

  for (const block of detail.fine_blocks) {
    const rawDate = block.start_date || block.end_date;
    if (rawDate && rawDate.length >= 7) {
      const month = rawDate.substring(0, 7);
      if (!blocksByMonth[month]) {
        blocksByMonth[month] = [];
      }
      blocksByMonth[month].push(block);
    } else {
      undatedBlocks.push(block);
    }
  }

  const sortedMonths = Object.keys(blocksByMonth).sort();

  const getBlockTypeDotColor = (type: string) => {
    switch (type.toLowerCase()) {
      case 'module':
        return 'border-purple-500 bg-purple-500/20 text-purple-400';
      case 'feature':
        return 'border-blue-500 bg-blue-500/20 text-blue-400';
      case 'refactor':
        return 'border-amber-500 bg-amber-500/20 text-amber-400';
      case 'bugfix':
      case 'fix':
        return 'border-emerald-500 bg-emerald-500/20 text-emerald-400';
      default:
        return 'border-cyan-500 bg-cyan-500/20 text-cyan-400';
    }
  };

  const getBlockTypeBadge = (type: string) => {
    switch (type.toLowerCase()) {
      case 'module':
        return <span className="px-1.5 py-0.5 text-[10px] font-semibold bg-purple-500/15 text-purple-500 border border-purple-500/30 rounded">{t('analysis.module')}</span>;
      case 'feature':
        return <span className="px-1.5 py-0.5 text-[10px] font-semibold bg-blue-500/15 text-blue-500 border border-blue-500/30 rounded">{t('analysis.feature')}</span>;
      case 'refactor':
        return <span className="px-1.5 py-0.5 text-[10px] font-semibold bg-amber-500/15 text-amber-500 border border-amber-500/30 rounded">{t('analysis.refactor')}</span>;
      case 'bugfix':
      case 'fix':
        return <span className="px-1.5 py-0.5 text-[10px] font-semibold bg-emerald-500/15 text-emerald-500 border border-emerald-500/30 rounded">{t('analysis.fix')}</span>;
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
            <span className="text-xs font-medium">{t('analysis.sessionsTotal')}</span>
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
            <span className="text-xs font-medium">{t('dashboard.kpiPrompts')}</span>
            <MessageSquare className="h-4 w-4 text-emerald-500" />
          </div>
          <div className="text-2xl font-bold theme-text-main tracking-tight font-mono">
            {detail.user_message_count.toLocaleString()}
          </div>
          <div className="mt-2 text-[11px] theme-text-muted">
            {t('analysis.allInteract', { n: detail.message_count.toLocaleString() })}
          </div>
        </div>

        {/* 活跃天数 */}
        <div className="theme-bg-card border theme-border rounded-xl p-4 relative overflow-hidden shadow-xs">
          <div className="flex items-center justify-between theme-text-muted mb-2">
            <span className="text-xs font-medium">{t('analysis.activeDaysTotal')}</span>
            <Calendar className="h-4 w-4 text-purple-500" />
          </div>
          <div className="text-2xl font-bold theme-text-main tracking-tight font-mono">
            {detail.active_days} <span className="text-xs font-normal theme-text-muted">{t('analysis.daysUnit')}</span>
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
            <span className="text-xs font-medium">{t('analysis.peakMsgs')}</span>
            <Flame className="h-4 w-4 text-orange-500" />
          </div>
          <div className="text-2xl font-bold theme-text-main tracking-tight font-mono">
            {detail.peak_count.toLocaleString()} <span className="text-xs font-normal theme-text-muted">{t('analysis.tiao')}</span>
          </div>
          <div className="mt-2 text-[11px] theme-text-muted">
            {detail.peak_day || '—'}
          </div>
        </div>
      </div>

      {/* 研发日历贡献热力图 (GitHub 风格) */}
      <ContributionHeatmap
        title={t('analysis.heatmapTitle')}
        subtitle={t('analysis.heatmapSub')}
        icon={<Calendar className="h-4 w-4 text-emerald-500" />}
        cellsAllMsgs={detail.heatmap_cells || []}
        activeDays={detail.active_days || 0}
        peakDay={detail.peak_day || undefined}
        peakCount={detail.peak_count}
        showTabs={false}
        autoScrollToEnd={true}
      />

      {/* 研发分析三级颗粒度 */}
      <div className="theme-bg-card border theme-border rounded-xl p-5 shadow-xs space-y-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="text-sm font-semibold theme-text-main flex items-center gap-2">
              <Sparkles className="h-4 w-4 text-purple-500" />
              {t('analysis.rdTitle')}
            </h2>
            <p className="text-xs theme-text-muted mt-0.5">
              {t('analysis.pipeline')}
            </p>
          </div>

          <div className="flex items-center gap-3">
            {/* 动态操作按钮（根据当前 Tab 切换：提取 Blocks/重新提取 | 合并模块/重新合并 | 生成报告/刷新报告） */}
            <div className="flex items-center gap-2">
              {activeTab === 'fine' && (
                <>
                  <button
                    onClick={() => handleExtractBlocks(false)}
                    disabled={extracting}
                    className="px-2.5 py-1 text-xs font-medium theme-bg-sub hover:opacity-80 border theme-border rounded-lg theme-text-main transition-all cursor-pointer flex items-center gap-1 shadow-2xs"
                  >
                    {extracting ? <Clock className="h-3.5 w-3.5 animate-spin text-blue-500" /> : <Sparkles className="h-3.5 w-3.5 text-purple-500" />}
                    <span>{t('analysis.extract')}</span>
                  </button>
                  <button
                    onClick={() => handleExtractBlocks(true)}
                    disabled={extracting}
                    className="px-2.5 py-1 text-xs font-medium theme-bg-sub hover:opacity-80 border theme-border rounded-lg theme-text-muted hover:theme-text-main transition-all cursor-pointer flex items-center gap-1 shadow-2xs"
                  >
                    <RefreshCw className="h-3.5 w-3.5 text-slate-400" />
                    <span>{t('analysis.reextract')}</span>
                  </button>
                </>
              )}

              {activeTab === 'modules' && (
                <>
                  <button
                    onClick={() => handleMergeModules(false)}
                    disabled={extracting}
                    className="px-2.5 py-1 text-xs font-medium theme-bg-sub hover:opacity-80 border theme-border rounded-lg theme-text-main transition-all cursor-pointer flex items-center gap-1 shadow-2xs"
                  >
                    {extracting ? <Clock className="h-3.5 w-3.5 animate-spin text-blue-500" /> : <Boxes className="h-3.5 w-3.5 text-purple-500" />}
                    <span>{t('analysis.merge')}</span>
                  </button>
                  <button
                    onClick={() => handleMergeModules(true)}
                    disabled={extracting}
                    className="px-2.5 py-1 text-xs font-medium theme-bg-sub hover:opacity-80 border theme-border rounded-lg theme-text-muted hover:theme-text-main transition-all cursor-pointer flex items-center gap-1 shadow-2xs"
                  >
                    <RefreshCw className="h-3.5 w-3.5 text-slate-400" />
                    <span>{t('analysis.remerge')}</span>
                  </button>
                </>
              )}

              {activeTab === 'report' && (
                <>
                  <button
                    onClick={() => handleGenerateReport(false)}
                    disabled={extracting}
                    className="px-2.5 py-1 text-xs font-medium theme-bg-sub hover:opacity-80 border theme-border rounded-lg theme-text-main transition-all cursor-pointer flex items-center gap-1 shadow-2xs"
                  >
                    {extracting ? <Clock className="h-3.5 w-3.5 animate-spin text-blue-500" /> : <FileText className="h-3.5 w-3.5 text-blue-500" />}
                    <span>{t('analysis.report')}</span>
                  </button>
                  <button
                    onClick={() => handleGenerateReport(true)}
                    disabled={extracting}
                    className="px-2.5 py-1 text-xs font-medium theme-bg-sub hover:opacity-80 border theme-border rounded-lg theme-text-muted hover:theme-text-main transition-all cursor-pointer flex items-center gap-1 shadow-2xs"
                  >
                    <RefreshCw className="h-3.5 w-3.5 text-slate-400" />
                    <span>{t('analysis.refreshReport')}</span>
                  </button>
                </>
              )}
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
                <span>{t('analysis.tabFine', { n: detail.fine_blocks.length })}</span>
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
                <span>{t('analysis.tabModules', { n: detail.module_blocks.length })}</span>
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
                <span>{t('analysis.tabReport')}</span>
              </button>
            </div>
          </div>
        </div>

        {/* 提取状态提示 & 进度条 */}
        {extractMessage && (
          <div className="p-3.5 text-xs bg-blue-500/10 border border-blue-500/30 rounded-xl text-blue-400 space-y-2">
            <div className="flex items-center justify-between gap-2">
              <div className="flex items-center gap-2">
                {extracting ? (
                  <Clock className="h-4 w-4 animate-spin text-blue-500 flex-shrink-0" />
                ) : extractMessage.startsWith('❌') ? (
                  <AlertTriangle className="h-4 w-4 text-rose-500 flex-shrink-0" />
                ) : (
                  <CheckCircle2 className="h-4 w-4 text-emerald-500 flex-shrink-0" />
                )}
                <span className="font-medium">{extractMessage}</span>
              </div>
              {progressInfo?.total && progressInfo.total > 0 && progressInfo.current !== undefined && (
                <span className="text-[11px] font-mono text-blue-400">
                  {progressInfo.current} / {progressInfo.total} ({Math.round((progressInfo.current / progressInfo.total) * 100)}%)
                </span>
              )}
            </div>

            {/* 进度条动画 */}
            {extracting && progressInfo?.total && progressInfo.total > 0 && (
              <div className="w-full bg-blue-950/40 rounded-full h-1.5 overflow-hidden">
                <div
                  className="bg-blue-500 h-1.5 rounded-full transition-all duration-300"
                  style={{
                    width: `${Math.min(100, Math.max(5, ((progressInfo.current || 0) / progressInfo.total) * 100))}%`,
                  }}
                />
              </div>
            )}
          </div>
        )}

        {/* Tab 1: 细粒度 Blocks (完全对齐 Python 版研发时间轴 + Blocks 卡片) */}
        {activeTab === 'fine' && (
          <div className="space-y-6 pt-2">
            {/* 研发时间轴 (Timeline) 模块 */}
            {sortedMonths.length > 0 && (
              <div className="p-5 rounded-2xl theme-bg-sub border theme-border shadow-xs space-y-4">
                <div>
                  <h3 className="text-xs font-bold theme-text-main flex items-center gap-1.5">
                    <Clock className="h-3.5 w-3.5 text-blue-500" />
                    <span>{t('analysis.timeline')}</span>
                  </h3>
                  <p className="text-[11px] theme-text-muted mt-0.5">
                    {t('analysis.timelineHint')}
                  </p>
                </div>

                {/* 按月份排布横向 Rail 轨道 */}
                <div className="space-y-6 pt-2">
                  {sortedMonths.map((month) => (
                    <div key={month} className="flex items-start gap-4">
                      {/* 月份 Label */}
                      <div className="w-16 text-right font-mono font-bold text-xs theme-text-muted pt-0.5 flex-shrink-0">
                        {month}
                      </div>

                      {/* 时间轴轨道 Track & Nodes */}
                      <div className="flex-1 relative pb-2">
                        {/* 水平背景轨线 */}
                        <div className="absolute left-0 right-0 top-2 h-0.5 bg-slate-300 dark:bg-slate-700/60 rounded-full" />

                        {/* 节点瀑布流 */}
                        <div className="relative flex flex-wrap gap-x-6 gap-y-4 pt-0">
                          {blocksByMonth[month].map((block) => (
                            <div
                              key={block.id}
                              onClick={() => setSelectedBlock(block)}
                              title={`${block.title}\n${block.summary}`}
                              className="flex flex-col items-center group cursor-pointer w-28 text-center"
                            >
                              {/* 圆点 */}
                              <div
                                className={`w-3.5 h-3.5 rounded-full border-2 ${getBlockTypeDotColor(
                                  block.type
                                )} transition-transform group-hover:scale-150 shadow-xs z-10`}
                              />
                              {/* 节点名称 */}
                              <span className="text-[11px] theme-text-main leading-tight line-clamp-2 mt-1.5 group-hover:text-blue-500 font-medium transition-colors">
                                {block.title}
                              </span>
                            </div>
                          ))}
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* 细粒度 Blocks 卡片列表（自适应响应式网格布局，对齐 Python 版） */}
            <div className="space-y-3">
              <div className="text-xs font-semibold theme-text-muted flex items-center justify-between">
                <span>{t('analysis.fineHint')}</span>
                <span>{t('analysis.nItems', { n: detail.fine_blocks.length })}</span>
              </div>

              <div className="grid grid-cols-[repeat(auto-fill,minmax(190px,1fr))] gap-2.5">
                {detail.fine_blocks.map((block) => {
                  return (
                    <div
                      key={block.id}
                      onClick={() => setSelectedBlock(block)}
                      className={`p-3 rounded-xl border flex flex-col justify-between transition-all text-xs shadow-2xs hover:shadow-md hover:-translate-y-0.5 cursor-pointer min-h-[145px] ${
                        block.type.toLowerCase() === 'module'
                          ? 'bg-purple-500/[0.04] dark:bg-purple-950/20 border-purple-500/25 hover:border-purple-500/60 hover:shadow-purple-500/10'
                          : block.type.toLowerCase() === 'feature'
                          ? 'bg-blue-500/[0.04] dark:bg-blue-950/20 border-blue-500/25 hover:border-blue-500/60 hover:shadow-blue-500/10'
                          : block.type.toLowerCase() === 'refactor'
                          ? 'bg-amber-500/[0.04] dark:bg-amber-950/20 border-amber-500/25 hover:border-amber-500/60 hover:shadow-amber-500/10'
                          : 'bg-emerald-500/[0.04] dark:bg-emerald-950/20 border-emerald-500/25 hover:border-emerald-500/60 hover:shadow-emerald-500/10'
                      }`}
                    >
                      <div>
                        {/* 顶部批次与类型 */}
                        <div className="flex items-center gap-1.5 mb-1.5">
                          {block.batch_index !== undefined && block.batch_index !== null && (
                            <span className="px-1.5 py-0.2 text-[9px] bg-black/5 dark:bg-white/10 rounded font-mono text-slate-500 dark:text-slate-400">
                              {t('analysis.batch', { n: block.batch_index + 1 })}
                            </span>
                          )}
                          {getBlockTypeBadge(block.type)}
                        </div>

                        {/* 标题 */}
                        <h5 className="font-bold text-xs theme-text-main leading-snug line-clamp-2 mb-1">
                          {block.title}
                        </h5>

                        {/* 摘要 */}
                        <p className="text-[11px] theme-text-muted leading-relaxed line-clamp-3 mb-2">
                          {block.summary}
                        </p>
                      </div>

                      <div>
                        {/* 关键词 */}
                        {block.keywords && block.keywords.length > 0 && (
                          <div className="flex items-center gap-1 flex-wrap mb-1.5">
                            {block.keywords.slice(0, 3).map((kw, i) => (
                              <span
                                key={i}
                                className="px-1.5 py-0.2 text-[9px] bg-black/5 dark:bg-white/5 border theme-border rounded text-slate-500 dark:text-slate-400 font-mono"
                              >
                                {kw}
                              </span>
                            ))}
                          </div>
                        )}

                        {/* 底部时间 */}
                        {(block.start_date || block.end_date) && (
                          <div className="text-[10px] theme-text-sub font-mono pt-1 border-t theme-border-sub">
                            {block.start_date || '—'} ~ {block.end_date || '—'}
                          </div>
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
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
                        <span>{t('analysis.relatedFine')}</span>
                        <span className="font-mono font-medium text-blue-500">
                          {t('analysis.nXiang', { n: mod.child_fine_ids.length })}
                        </span>
                      </div>
                    )}
                  </div>
                ))}
              </div>
            ) : (
              <div className="py-12 text-center text-xs theme-text-sub">
                {t('analysis.noModules')}
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
                {t('analysis.noReportYet')}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Block 详情抽屉 Drawer */}
      {selectedBlock && (
        <div
          className="fixed inset-0 z-50 flex justify-end bg-black/50 backdrop-blur-xs"
          onClick={() => setSelectedBlock(null)}
        >
          <div
            className="w-full max-w-md theme-bg-card border-l theme-border h-full p-6 overflow-y-auto space-y-4 shadow-2xl flex flex-col justify-between"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="space-y-4">
              <div className="flex items-center justify-between border-b theme-border pb-3">
                <div className="flex items-center gap-2">
                  {getBlockTypeBadge(selectedBlock.type)}
                  <span className="text-xs font-mono theme-text-muted">{t('analysis.blockDetail')}</span>
                </div>
                <button
                  onClick={() => setSelectedBlock(null)}
                  className="p-1 rounded-lg theme-text-muted hover:theme-text-main hover:theme-bg-sub cursor-pointer"
                >
                  <X className="h-4 w-4" />
                </button>
              </div>

              <div>
                <h3 className="text-base font-bold theme-text-main">{selectedBlock.title}</h3>
                {selectedBlock.start_date && (
                  <p className="text-xs theme-text-muted font-mono mt-1">
                    {t('analysis.period', { range: `${selectedBlock.start_date} ${selectedBlock.end_date ? `~ ${selectedBlock.end_date}` : ''}` })}
                  </p>
                )}
              </div>

              <div className="space-y-1.5">
                <div className="text-xs font-semibold theme-text-muted">{t('analysis.summary')}</div>
                <div className="p-3 rounded-xl theme-bg-sub border theme-border text-xs theme-text-main leading-relaxed">
                  {selectedBlock.summary}
                </div>
              </div>

              {selectedBlock.keywords && selectedBlock.keywords.length > 0 && (
                <div className="space-y-1.5">
                  <div className="text-xs font-semibold theme-text-muted">{t('analysis.keywords')}</div>
                  <div className="flex flex-wrap gap-1.5">
                    {selectedBlock.keywords.map((kw, i) => (
                      <span
                        key={i}
                        className="px-2 py-0.5 text-xs theme-bg-sub border theme-border rounded-lg font-mono theme-text-main"
                      >
                        {kw}
                      </span>
                    ))}
                  </div>
                </div>
              )}

              {selectedBlock.evidence && selectedBlock.evidence.length > 0 && (
                <div className="space-y-1.5">
                  <div className="text-xs font-semibold theme-text-muted">{t('analysis.evidence')}</div>
                  <div className="space-y-2">
                    {selectedBlock.evidence.map((ev, i) => (
                      <div key={i} className="p-2.5 rounded-lg theme-bg-sub border theme-border text-[11px] space-y-1">
                        {ev.conversation_title && (
                          <div className="font-medium text-blue-500 line-clamp-1">{ev.conversation_title}</div>
                        )}
                        {ev.snippet && (
                          <div className="theme-text-muted italic line-clamp-2">“{ev.snippet}”</div>
                        )}
                        {ev.date && (
                          <div className="text-[10px] theme-text-sub font-mono">{ev.date}</div>
                        )}
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>

            <div className="pt-4 border-t theme-border">
              <button
                onClick={() => setSelectedBlock(null)}
                className="w-full py-2 bg-blue-600 hover:bg-blue-500 text-white text-xs font-medium rounded-xl transition-colors cursor-pointer shadow-xs"
              >
                {t('analysis.closeDrawer')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
