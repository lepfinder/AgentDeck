import React, { useState, useEffect, useMemo, useRef } from 'react';
import { createPortal } from 'react-dom';
import type { DailyTimelineStats, DailyTimelineItem, DailyConcurrencySlot } from '../../types';
import { api } from '../../api/tauriBridge';
import { useI18n } from '../../i18n';
import {
  FolderGit2,
  Bot,
  Flame,
  Sparkles,
  RefreshCw,
  ExternalLink,
  MessageSquare,
  Clock,
  Zap,
  X,
  ChevronLeft,
  ChevronRight,
  Calendar,
} from 'lucide-react';

interface Props {
  date: string;
  todayStr: string;
  onChangeDate?: (date: string) => void;
  onSelectConversation: (convId: string, workspacePath: string) => void;
  refreshTrigger?: number;
}

export interface PromptCluster {
  id: string;
  minute: number;
  last_minute: number;
  time_label: string;
  prompts: DailyTimelineItem[];
  color: string;
}

interface GroupedLane {
  key: string;
  title: string;
  subtitle: string;
  color?: string;
  clusters: PromptCluster[];
  totalPrompts: number;
  distinctConvs: number;
}

export const DailyActivityGantt: React.FC<Props> = ({
  date,
  todayStr,
  onChangeDate,
  onSelectConversation,
  refreshTrigger,
}) => {
  const { t } = useI18n();
  const [stats, setStats] = useState<DailyTimelineStats | null>(null);
  const [loading, setLoading] = useState(false);
  const [groupBy, setGroupBy] = useState<'workspace' | 'agent'>('workspace');
  const [hoveredCluster, setHoveredCluster] = useState<PromptCluster | null>(null);
  const [hoveredSlot, setHoveredSlot] = useState<{
    slot: DailyConcurrencySlot;
    x: number;
    y: number;
  } | null>(null);
  const [tooltipPos, setTooltipPos] = useState<{ x: number; y: number } | null>(null);

  // 点击选中的 Prompt 聚合弹窗状态
  const [activeModalCluster, setActiveModalCluster] = useState<PromptCluster | null>(null);
  const dateInputRef = useRef<HTMLInputElement>(null);

  // 获取北京时间当前分钟数（0~1440）
  const currentNowMinutes = useMemo(() => {
    const now = new Date();
    const utc = now.getTime() + now.getTimezoneOffset() * 60000;
    const beijing = new Date(utc + 3600000 * 8);
    return beijing.getHours() * 60 + beijing.getMinutes();
  }, []);

  const isViewingToday = date === todayStr;

  useEffect(() => {
    let active = true;
    setLoading(true);
    api
      .getDailyTimeline(date)
      .then((res) => {
        if (active) {
          setStats(res);
          setLoading(false);
        }
      })
      .catch((err) => {
        console.error('Failed to load daily timeline:', err);
        if (active) setLoading(false);
      });

    return () => {
      active = false;
    };
  }, [date, refreshTrigger]);

  // ESC 键关闭弹窗
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && activeModalCluster) {
        setActiveModalCluster(null);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [activeModalCluster]);

  // 日期加减与快捷切换
  const handleOffsetDay = (offset: number) => {
    const d = new Date(date + 'T00:00:00');
    d.setDate(d.getDate() + offset);
    const y = d.getFullYear();
    const m = String(d.getMonth() + 1).padStart(2, '0');
    const day = String(d.getDate()).padStart(2, '0');
    const nextDateStr = `${y}-${m}-${day}`;
    if (onChangeDate) {
      onChangeDate(nextDateStr);
    }
  };

  // 泳道分组与绝对防重叠聚类计算（Strict Anti-overlap Clustering）
  const groupedLanes = useMemo<GroupedLane[]>(() => {
    if (!stats || !stats.items || stats.items.length === 0) return [];

    const map = new Map<string, DailyTimelineItem[]>();
    for (const item of stats.items) {
      const key = groupBy === 'workspace' ? item.workspace_path : item.source_app;
      const arr = map.get(key) || [];
      arr.push(item);
      map.set(key, arr);
    }

    const result: GroupedLane[] = [];

    for (const [key, rawItems] of map.entries()) {
      // 按照 minute 升序排序
      const sorted = [...rawItems].sort((a, b) => a.minute - b.minute);
      const rawClusters: PromptCluster[] = [];
      let current: PromptCluster | null = null;

      for (const item of sorted) {
        // 30 分钟窗口内连续提问归为同一个连续时段
        if (current && item.minute - current.last_minute <= 30) {
          current.prompts.push(item);
          current.last_minute = item.minute;
        } else {
          if (current) rawClusters.push(current);
          current = {
            id: `${item.id}-${item.message_id}`,
            minute: item.minute,
            last_minute: item.minute,
            time_label: item.time_label,
            prompts: [item],
            color: item.source_color,
          };
        }
      }
      if (current) rawClusters.push(current);

      // 二次间距保护：如果相邻两个 cluster 间距 < 35 分钟（避免在 X 轴上贴得太近产生重叠），进行合并
      const finalClusters: PromptCluster[] = [];
      for (const c of rawClusters) {
        const prev = finalClusters[finalClusters.length - 1];
        if (prev && c.minute - prev.minute < 35) {
          prev.prompts.push(...c.prompts);
          prev.last_minute = Math.max(prev.last_minute, c.last_minute);
        } else {
          finalClusters.push(c);
        }
      }

      const distinctConvs = new Set(rawItems.map((it) => it.id)).size;
      const first = rawItems[0];
      const title =
        groupBy === 'workspace'
          ? first.workspace_short || '默认工作区'
          : first.source_label || first.source_app;
      const subtitle = groupBy === 'workspace' ? key : `${distinctConvs} 个会话`;
      const color = groupBy === 'agent' ? first.source_color : undefined;

      result.push({
        key,
        title,
        subtitle,
        color,
        clusters: finalClusters,
        totalPrompts: rawItems.length,
        distinctConvs,
      });
    }

    // 按提示词总数降序排列
    result.sort((a, b) => b.totalPrompts - a.totalPrompts);
    return result;
  }, [stats, groupBy]);

  // 每 3 小时主刻度标记 (00:00, 03:00, 06:00, 09:00, 12:00, 15:00, 18:00, 21:00, 24:00)
  const hourTicks = useMemo(() => {
    const ticks = [];
    for (let h = 0; h <= 24; h += 3) {
      ticks.push({
        hour: h,
        label: `${String(h).padStart(2, '0')}:00`,
        percent: (h / 24) * 100,
      });
    }
    return ticks;
  }, []);

  return (
    <div className="theme-bg-card border theme-border rounded-xl p-5 backdrop-blur-sm relative transition-all">
      {/* 头部标题与控制区 */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 mb-4">
        <div>
          <div className="flex items-center gap-2.5 flex-wrap">
            <h2 className="text-sm font-semibold theme-text-main flex items-center gap-2">
              <Sparkles className="h-4 w-4 text-blue-500" />
              {t('dashboard.ganttTitle')}
            </h2>

            {/* 日期快捷导航与选择器 */}
            <div className="flex items-center theme-bg-sub rounded-lg border theme-border text-xs shadow-2xs">
              <button
                type="button"
                onClick={() => handleOffsetDay(-1)}
                className="p-1 rounded-l-md theme-text-muted hover:theme-text-main hover:bg-slate-500/10 transition-colors cursor-pointer"
                title="前一天"
              >
                <ChevronLeft className="h-3.5 w-3.5" />
              </button>

              <div
                onClick={() => dateInputRef.current?.showPicker?.()}
                className="px-2 py-0.5 font-mono text-xs font-medium theme-text-main cursor-pointer hover:bg-slate-500/10 flex items-center gap-1.5 relative select-none"
                title="点击选择任意日期"
              >
                <Calendar className="h-3 w-3 text-blue-500" />
                <span>{date}</span>
                {isViewingToday && (
                  <span className="text-[10px] px-1 rounded bg-blue-500/15 text-blue-500 font-sans font-semibold">
                    今天
                  </span>
                )}
                {/* 隐藏的 HTML 原生日期输入框 */}
                <input
                  ref={dateInputRef}
                  type="date"
                  value={date}
                  max={todayStr}
                  onChange={(e) => {
                    if (e.target.value && onChangeDate) {
                      onChangeDate(e.target.value);
                    }
                  }}
                  className="absolute inset-0 opacity-0 w-full h-full cursor-pointer pointer-events-none"
                />
              </div>

              <button
                type="button"
                disabled={isViewingToday}
                onClick={() => handleOffsetDay(1)}
                className={`p-1 rounded-r-md transition-colors ${
                  isViewingToday
                    ? 'theme-text-sub opacity-30 cursor-not-allowed'
                    : 'theme-text-muted hover:theme-text-main hover:bg-slate-500/10 cursor-pointer'
                }`}
                title="后一天"
              >
                <ChevronRight className="h-3.5 w-3.5" />
              </button>
            </div>

            {/* 回到今天快捷键 */}
            {!isViewingToday && (
              <button
                onClick={() => onChangeDate && onChangeDate(todayStr)}
                className="text-xs px-2 py-0.5 rounded-md font-medium theme-bg-sub border theme-border hover:border-blue-500/40 text-blue-500 cursor-pointer transition-all shadow-2xs"
              >
                回到今天
              </button>
            )}

            {loading && <RefreshCw className="h-3.5 w-3.5 animate-spin text-blue-500 ml-1" />}
          </div>
          <p className="text-xs theme-text-muted mt-1">
            {t('dashboard.ganttSubtitle')}
          </p>
        </div>

        {/* 右侧：统计概览与分组切换 */}
        <div className="flex items-center gap-2 flex-wrap">
          {stats && stats.items.length > 0 && (
            <div className="flex items-center gap-2 text-xs theme-text-muted mr-1">
              <span className="flex items-center gap-1">
                <FolderGit2 className="h-3.5 w-3.5 text-blue-400" />
                {t('dashboard.ganttTotalWorkspaces')}:{' '}
                <strong className="theme-text-main font-mono">{stats.total_workspaces}</strong>
              </span>
              <span className="opacity-40">|</span>
              <span className="flex items-center gap-1">
                <MessageSquare className="h-3.5 w-3.5 text-emerald-400" />
                提示词:{' '}
                <strong className="theme-text-main font-mono">{stats.items.length}</strong> 条
              </span>
              <span className="opacity-40">|</span>
              <span className="flex items-center gap-1 text-amber-500 font-medium">
                <Flame className="h-3.5 w-3.5 fill-amber-400" />
                {t('dashboard.ganttPeakConcurrency')}:{' '}
                <strong className="font-mono text-amber-400">{stats.peak_concurrency}</strong>
              </span>
            </div>
          )}

          {/* 切换泳道视角 */}
          <div className="flex theme-bg-sub p-0.5 rounded-lg border theme-border text-xs">
            <button
              onClick={() => setGroupBy('workspace')}
              className={`flex items-center gap-1 px-2.5 py-1 rounded-md font-medium transition-all cursor-pointer ${
                groupBy === 'workspace'
                  ? 'bg-blue-600 text-white shadow-sm'
                  : 'theme-text-muted hover:theme-text-main'
              }`}
            >
              <FolderGit2 className="h-3 w-3" />
              {t('dashboard.ganttByWorkspace')}
            </button>
            <button
              onClick={() => setGroupBy('agent')}
              className={`flex items-center gap-1 px-2.5 py-1 rounded-md font-medium transition-all cursor-pointer ${
                groupBy === 'agent'
                  ? 'bg-blue-600 text-white shadow-sm'
                  : 'theme-text-muted hover:theme-text-main'
              }`}
            >
              <Bot className="h-3 w-3" />
              {t('dashboard.ganttByAgent')}
            </button>
          </div>
        </div>
      </div>

      {/* 主体甘特图绘制区域 */}
      {groupedLanes.length === 0 ? (
        <div className="py-12 text-center text-xs theme-text-sub border theme-border border-dashed rounded-lg">
          {loading ? t('dashboard.loading') : t('dashboard.ganttEmpty')}
        </div>
      ) : (
        <div className="overflow-x-auto rounded-xl border border-slate-200/80 dark:border-slate-800/80 theme-bg-card shadow-xs">
          <div className="min-w-[760px]">
            {/* 顶部时间轴标尺 */}
            <div className="flex items-center border-b border-slate-200/80 dark:border-slate-800/80 text-[11px] font-mono theme-text-muted py-2 select-none theme-bg-sub/50">
              <div className="w-52 flex-shrink-0 px-4 font-sans font-medium text-xs">
                {groupBy === 'workspace' ? '项目工程' : 'Agent 平台'}
              </div>
              <div className="flex-1 relative h-4">
                {hourTicks.map((tick) => (
                  <div
                    key={tick.hour}
                    className="absolute -translate-x-1/2 flex flex-col items-center"
                    style={{ left: `${tick.percent}%` }}
                  >
                    <span className="text-[10px] opacity-60">{tick.label}</span>
                  </div>
                ))}
              </div>
            </div>

            {/* 泳道与波形综合容器 */}
            <div className="relative">
              {/* 背景时间网格纵向辅助极浅虚线 */}
              <div className="absolute inset-0 pointer-events-none flex left-52">
                {hourTicks.map((tick) => (
                  <div
                    key={tick.hour}
                    className="absolute top-0 bottom-0 border-r border-dashed border-slate-300/40 dark:border-slate-700/30 -translate-x-px"
                    style={{ left: `${tick.percent}%` }}
                  />
                ))}

                {/* 实时时间线（仅查看当天时渲染，贯穿到底部） */}
                {isViewingToday && currentNowMinutes >= 0 && currentNowMinutes <= 1440 && (
                  <div
                    className="absolute top-0 bottom-0 z-20 border-r border-red-500 shadow-[0_0_6px_rgba(239,68,68,0.4)]"
                    style={{ left: `${(currentNowMinutes / 1440) * 100}%` }}
                  >
                    <div className="absolute -top-1 -translate-x-1/2 bg-red-500 text-white text-[8px] font-bold px-1 py-0.2 rounded shadow-xs">
                      NOW
                    </div>
                  </div>
                )}
              </div>

              {/* 循环渲染各个项目/Agent 泳道（清爽无重线，统一 38px 高度） */}
              {groupedLanes.map((lane, laneIdx) => {
                return (
                  <div
                    key={lane.key}
                    className={`flex items-stretch relative hover:bg-slate-500/5 transition-colors group h-[38px] ${
                      laneIdx > 0 ? 'border-t border-slate-100 dark:border-slate-800/40' : ''
                    }`}
                  >
                    {/* 左侧泳道标题卡 */}
                    <div className="w-52 flex-shrink-0 px-4 py-1.5 flex items-center justify-between border-r border-slate-200/60 dark:border-slate-800/60 z-10 theme-bg-card group-hover:bg-slate-500/5 transition-colors">
                      <div className="flex items-center gap-1.5 min-w-0 pr-2">
                        {lane.color ? (
                          <span
                            className="w-2 h-2 rounded-full flex-shrink-0"
                            style={{ backgroundColor: lane.color }}
                          />
                        ) : (
                          <FolderGit2 className="h-3.5 w-3.5 text-blue-400 flex-shrink-0" />
                        )}
                        <span
                          className="text-xs font-medium theme-text-main truncate"
                          title={lane.subtitle}
                        >
                          {lane.title}
                        </span>
                      </div>
                      <div className="flex items-center gap-1 text-[10px] theme-text-muted flex-shrink-0 font-mono">
                        <span className="text-emerald-500 font-medium">{lane.totalPrompts}p</span>
                      </div>
                    </div>

                    {/* 右侧时间轴脉冲点阵/聚合徽章区域 */}
                    <div className="flex-1 relative h-full flex items-center overflow-hidden">
                      {lane.clusters.map((cluster) => {
                        const leftPercent = (cluster.minute / 1440) * 100;
                        const isMultiple = cluster.prompts.length > 1;

                        return (
                          <div
                            key={cluster.id}
                            onClick={(e) => {
                              e.stopPropagation();
                              setActiveModalCluster(cluster);
                            }}
                            onMouseEnter={(e) => {
                              const rect = e.currentTarget.getBoundingClientRect();
                              setTooltipPos({
                                x: rect.left + rect.width / 2,
                                y: rect.top,
                              });
                              setHoveredCluster(cluster);
                            }}
                            onMouseLeave={() => {
                              setHoveredCluster(null);
                              setTooltipPos(null);
                            }}
                            className={`absolute -translate-x-1/2 cursor-pointer transition-all duration-150 select-none z-10 flex items-center justify-center ${
                              isMultiple
                                ? 'h-4.5 px-1.5 rounded-full border shadow-xs hover:scale-110 hover:z-30'
                                : 'w-2.5 h-2.5 rounded-full hover:scale-150 hover:z-30'
                            }`}
                            style={{
                              left: `${leftPercent}%`,
                              backgroundColor: isMultiple
                                ? `${cluster.color}20`
                                : cluster.color,
                              borderColor: isMultiple
                                ? `${cluster.color}80`
                                : undefined,
                              boxShadow: isMultiple
                                ? `0 0 6px ${cluster.color}25`
                                : `0 0 8px ${cluster.color}70`,
                            }}
                          >
                            {isMultiple ? (
                              <span
                                className="text-[9px] font-mono font-bold leading-none flex items-center gap-0.5"
                                style={{ color: cluster.color }}
                              >
                                <span className="w-1 h-1 rounded-full bg-current" />
                                {cluster.prompts.length}
                              </span>
                            ) : (
                              <span className="w-0.5 h-0.5 rounded-full bg-white opacity-80" />
                            )}
                          </div>
                        );
                      })}
                    </div>
                  </div>
                );
              })}

              {/* 底部内置泳道：并发度趋势波形（整行显式浅色背景，柔和顶部边框） */}
              {stats && stats.concurrency_slots && stats.concurrency_slots.length > 0 && (
                <div className="flex items-stretch relative theme-bg-sub border-t border-slate-200 dark:border-slate-800 min-h-[50px]">
                  {/* 左侧说明列 */}
                  <div className="w-52 flex-shrink-0 px-4 py-2 flex flex-col justify-center border-r border-slate-200/80 dark:border-slate-800/80 z-10 theme-bg-sub">
                    <div className="flex items-center gap-1.5 text-xs font-semibold theme-text-main">
                      <Zap className="h-3.5 w-3.5 text-amber-400 fill-amber-400/20" />
                      <span>{t('dashboard.ganttConcurrencyWave')}</span>
                    </div>
                    <div className="flex items-center gap-1 mt-0.5">
                      <span className="text-[10px] theme-text-muted">最高并发:</span>
                      <span className="text-[10px] px-1.5 py-0.2 rounded font-mono font-bold bg-amber-500/15 text-amber-500 border border-amber-500/25">
                        {stats.peak_concurrency} 项目
                      </span>
                    </div>
                  </div>

                  {/* 右侧波形柱状图（与上方右侧时间轴 100% 同步宽度） */}
                  <div className="flex-1 relative flex items-end h-11 py-1.5 px-0 overflow-hidden theme-bg-sub">
                    <div className="w-full h-full flex items-end gap-[1px]">
                      {stats.concurrency_slots.map((slot, idx) => {
                        const maxPeak = Math.max(1, stats.peak_concurrency);
                        const heightPercent = (slot.active_workspaces / maxPeak) * 100;
                        const isPeak =
                          slot.active_workspaces === stats.peak_concurrency &&
                          stats.peak_concurrency > 1;

                        return (
                          <div
                            key={idx}
                            onMouseEnter={(e) => {
                              if (slot.active_workspaces === 0) return;
                              const rect = e.currentTarget.getBoundingClientRect();
                              setHoveredSlot({
                                slot,
                                x: rect.left + rect.width / 2,
                                y: rect.top,
                              });
                            }}
                            onMouseLeave={() => setHoveredSlot(null)}
                            className="flex-1 h-full flex items-end group/slot relative cursor-pointer"
                          >
                            <div
                              className={`w-full rounded-t-xs transition-all ${
                                slot.active_workspaces === 0
                                  ? 'bg-slate-400/15 dark:bg-slate-600/15 h-[1.5px]'
                                  : isPeak
                                    ? 'bg-amber-400 shadow-[0_0_6px_rgba(251,191,36,0.5)]'
                                    : slot.active_workspaces > 1
                                      ? 'bg-emerald-400/80'
                                      : 'bg-blue-500/70'
                              }`}
                              style={{ height: `${Math.max(2, heightPercent)}%` }}
                            />
                          </div>
                        );
                      })}
                    </div>
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      {/* 并发度趋势悬浮气泡（Portal 挂载） */}
      {hoveredSlot && !activeModalCluster && createPortal(
        <div
          className="fixed z-[9999] pointer-events-none transform -translate-x-1/2 -translate-y-full mb-2 transition-opacity"
          style={{
            left: `${Math.max(100, Math.min(window.innerWidth - 100, hoveredSlot.x))}px`,
            top: `${hoveredSlot.y - 4}px`,
          }}
        >
          <div className="bg-slate-900/95 text-white text-[11px] rounded-xl px-3 py-2 shadow-2xl border border-slate-700/80 backdrop-blur-md whitespace-nowrap font-mono space-y-1 animate-in fade-in zoom-in-95 duration-100">
            <div className="text-slate-400 text-[10px]">{hoveredSlot.slot.time_label}</div>
            <div className="text-amber-400 font-bold text-xs flex items-center gap-1">
              <Zap className="h-3 w-3 fill-amber-400" />
              <span>{hoveredSlot.slot.active_workspaces} 个项目并行</span>
            </div>
            <div className="text-slate-300 text-[10px]">
              {hoveredSlot.slot.active_conversations} 个活跃提示词
            </div>
          </div>
        </div>,
        document.body
      )}

      {/* 全局浮动 Hover Tooltip 卡片（使用 Portal 挂载至 document.body） */}
      {hoveredCluster && tooltipPos && !activeModalCluster && createPortal(
        <div
          className="fixed z-[9999] pointer-events-none transform -translate-x-1/2 -translate-y-full mb-2.5 transition-opacity"
          style={{
            left: `${Math.max(160, Math.min(window.innerWidth - 160, tooltipPos.x))}px`,
            top: `${tooltipPos.y - 6}px`,
          }}
        >
          <div className="bg-slate-900/95 text-slate-100 p-3 rounded-xl shadow-2xl border border-slate-700/80 backdrop-blur-md w-80 space-y-1.5 text-xs animate-in fade-in zoom-in-95 duration-100">
            <div className="flex items-center justify-between gap-2">
              <div className="flex items-center gap-1.5 min-w-0">
                <span
                  className="w-2 h-2 rounded-full flex-shrink-0"
                  style={{ backgroundColor: hoveredCluster.color }}
                />
                <span className="font-semibold text-slate-100 truncate">
                  {hoveredCluster.time_label} · {hoveredCluster.prompts.length} 条提示词
                </span>
              </div>
              <span className="text-[10px] px-1.5 py-0.5 rounded font-medium bg-slate-800 text-slate-300 flex-shrink-0">
                {hoveredCluster.prompts[0]?.source_label}
              </span>
            </div>

            {/* 提示词列表摘要 */}
            <div className="space-y-1 max-h-36 overflow-hidden">
              {hoveredCluster.prompts.slice(0, 3).map((p, idx) => (
                <p
                  key={p.message_id || idx}
                  className="text-slate-200 truncate text-[11px] bg-slate-800/60 px-2 py-1 rounded border border-slate-700/40 font-sans"
                >
                  <span className="text-slate-400 font-mono mr-1">#{idx + 1}</span>
                  {p.prompt_preview}
                </p>
              ))}
              {hoveredCluster.prompts.length > 3 && (
                <div className="text-[10px] text-slate-400 text-center font-mono">
                  + 还有 {hoveredCluster.prompts.length - 3} 条提示词...
                </div>
              )}
            </div>

            <div className="text-[10px] text-slate-400 truncate flex items-center gap-1 pt-0.5">
              <FolderGit2 className="h-3 w-3 text-slate-400" />
              <span>{hoveredCluster.prompts[0]?.workspace_short}</span>
            </div>

            <div className="pt-1 text-[10px] text-blue-400 flex items-center gap-1 border-t border-slate-800">
              <Sparkles className="h-2.5 w-2.5" />
              <span>点击查看提示词详情弹窗</span>
            </div>
          </div>
        </div>,
        document.body
      )}

      {/* 提示词列表详情弹窗 (紧凑干练的时间倒序流水) */}
      {activeModalCluster && createPortal(
        <div
          className="fixed inset-0 z-[10000] flex items-center justify-center p-4 bg-slate-950/60 backdrop-blur-xs animate-in fade-in duration-150"
          onClick={() => setActiveModalCluster(null)}
        >
          <div
            className="theme-bg-card border theme-border rounded-2xl shadow-2xl w-full max-w-xl overflow-hidden flex flex-col max-h-[82vh] animate-in zoom-in-95 duration-150"
            onClick={(e) => e.stopPropagation()}
          >
            {/* 紧凑 Header */}
            <div className="px-4 py-3 border-b theme-border flex items-center justify-between theme-bg-sub/60">
              <div className="flex items-center gap-2 min-w-0">
                <div
                  className="w-7 h-7 rounded-lg flex items-center justify-center flex-shrink-0"
                  style={{
                    backgroundColor: `${activeModalCluster.color}20`,
                    color: activeModalCluster.color,
                  }}
                >
                  <MessageSquare className="h-3.5 w-3.5" />
                </div>
                <div className="min-w-0">
                  <div className="flex items-center gap-1.5">
                    <h3 className="text-xs font-bold theme-text-main truncate">
                      提示词记录 ({activeModalCluster.prompts.length} 条)
                    </h3>
                    <span
                      className="text-[10px] px-1.5 py-0.2 rounded font-medium"
                      style={{
                        backgroundColor: `${activeModalCluster.color}20`,
                        color: activeModalCluster.color,
                      }}
                    >
                      {activeModalCluster.prompts[0]?.source_label}
                    </span>
                  </div>
                  <div className="flex items-center gap-1.5 text-[11px] theme-text-muted">
                    <span className="font-mono">{activeModalCluster.time_label} 时段</span>
                    <span>·</span>
                    <span className="truncate" title={activeModalCluster.prompts[0]?.workspace_path}>
                      {activeModalCluster.prompts[0]?.workspace_short}
                    </span>
                  </div>
                </div>
              </div>

              <button
                onClick={() => setActiveModalCluster(null)}
                className="p-1 rounded-lg theme-text-muted hover:theme-text-main hover:bg-slate-500/10 transition-colors cursor-pointer"
              >
                <X className="h-4 w-4" />
              </button>
            </div>

            {/* 紧凑列表正文 */}
            <div className="p-4 overflow-y-auto flex-1 space-y-2.5">
              {[...activeModalCluster.prompts].reverse().map((p, idx) => (
                <div
                  key={p.message_id || idx}
                  className="p-3 rounded-xl border border-slate-200/80 dark:border-slate-800/80 theme-bg-sub/40 hover:theme-bg-sub/80 transition-colors space-y-1.5 group shadow-2xs"
                >
                  {/* 条目顶栏 */}
                  <div className="flex items-center justify-between gap-2 text-xs">
                    <div className="flex items-center gap-1.5 font-mono min-w-0">
                      <span className="text-[10px] px-1.5 py-0.2 rounded bg-slate-500/15 text-slate-400 font-bold">
                        #{activeModalCluster.prompts.length - idx}
                      </span>
                      <span className="font-semibold theme-text-main flex items-center gap-1 text-[11px]">
                        <Clock className="h-3 w-3 text-blue-400" />
                        {p.time.split(' ')[1] || p.time}
                      </span>
                      {p.conversation_title && (
                        <>
                          <span className="opacity-30 text-[10px]">·</span>
                          <span className="text-[11px] theme-text-muted truncate max-w-[200px]" title={p.conversation_title}>
                            {p.conversation_title}
                          </span>
                        </>
                      )}
                    </div>

                    <button
                      onClick={() => {
                        setActiveModalCluster(null);
                        onSelectConversation(p.id, p.workspace_path);
                      }}
                      className="flex items-center gap-1 text-[11px] text-blue-500 hover:text-blue-600 transition-colors cursor-pointer flex-shrink-0"
                      title="前往该会话查看完整上下文"
                    >
                      <span>前往会话</span>
                      <ExternalLink className="h-3 w-3" />
                    </button>
                  </div>

                  {/* 提示词内容正文 */}
                  <div className="text-xs theme-text-main leading-relaxed whitespace-pre-wrap select-text break-words font-sans">
                    {p.prompt_content}
                  </div>
                </div>
              ))}
            </div>

            {/* 紧凑 Footer */}
            <div className="px-4 py-2.5 border-t theme-border flex items-center justify-between theme-bg-sub/30">
              <span className="text-[10px] theme-text-muted">
                按 <kbd className="px-1.5 py-0.5 rounded bg-slate-500/20 font-mono text-[9px]">ESC</kbd> 退出
              </span>

              <button
                onClick={() => setActiveModalCluster(null)}
                className="px-3.5 py-1 rounded-lg theme-bg-sub hover:opacity-80 theme-text-main text-xs font-medium cursor-pointer transition-all border theme-border"
              >
                关闭
              </button>
            </div>
          </div>
        </div>,
        document.body
      )}
    </div>
  );
};
