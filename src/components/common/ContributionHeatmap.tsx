import React, { useState, useRef, useMemo, useEffect } from 'react';
import type { HeatmapCell } from '../../types';

interface ContributionHeatmapProps {
  title?: React.ReactNode;
  subtitle?: React.ReactNode;
  icon?: React.ReactNode;
  cellsAllMsgs: HeatmapCell[];
  cellsUserMsgs?: HeatmapCell[];
  cellsConvs?: HeatmapCell[];
  activeDays?: number;
  longestStreak?: number;
  peakDay?: string;
  peakCount?: number;
  defaultTab?: 'user' | 'msgs' | 'convs';
  showTabs?: boolean;
  onCellClick?: (date: string) => void;
  extraHeaderChips?: React.ReactNode;
  autoScrollToEnd?: boolean;
}

const WEEKDAYS = ['周日', '周一', '周二', '周三', '周四', '周五', '周六'];

const getWeekday = (dateStr: string) => {
  if (!dateStr) return '';
  const parts = dateStr.split('-').map(Number);
  if (parts.length < 3) return '';
  const d = new Date(parts[0], parts[1] - 1, parts[2]);
  return WEEKDAYS[d.getDay()] || '';
};

export const ContributionHeatmap: React.FC<ContributionHeatmapProps> = ({
  title,
  subtitle,
  icon,
  cellsAllMsgs,
  cellsUserMsgs,
  cellsConvs,
  activeDays,
  longestStreak,
  peakDay,
  peakCount,
  defaultTab = 'msgs',
  showTabs = true,
  onCellClick,
  extraHeaderChips,
  autoScrollToEnd = false,
}) => {
  const [tab, setTab] = useState<'user' | 'msgs' | 'convs'>(() => {
    if (defaultTab === 'user' && cellsUserMsgs && cellsUserMsgs.length > 0) return 'user';
    return defaultTab;
  });

  const [hovered, setHovered] = useState<{
    cell: HeatmapCell;
    x: number;
    y: number;
    containerWidth: number;
  } | null>(null);

  const containerRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  // 默认滚动到最右侧
  useEffect(() => {
    if (autoScrollToEnd && scrollRef.current) {
      scrollRef.current.scrollLeft = scrollRef.current.scrollWidth;
    }
  }, [autoScrollToEnd, cellsAllMsgs]);

  // 根据当前选择的 Tab 获取渲染数据
  const currentData = useMemo(() => {
    if (tab === 'user' && cellsUserMsgs && cellsUserMsgs.length > 0) {
      return cellsUserMsgs;
    }
    if (tab === 'convs' && cellsConvs && cellsConvs.length > 0) {
      return cellsConvs;
    }
    return cellsAllMsgs || [];
  }, [tab, cellsAllMsgs, cellsUserMsgs, cellsConvs]);

  // 组织为 52 周 × 7 天网格
  const { weeks, monthLabels } = useMemo(() => {
    const weeksList: Array<Array<HeatmapCell | null>> = [];
    const labels: { month: string; colIndex: number }[] = [];

    if (!currentData || currentData.length === 0) {
      return { weeks: weeksList, monthLabels: labels };
    }

    let currentWeek: Array<HeatmapCell | null> = [];
    const firstDateParts = currentData[0].date.split('-').map(Number);
    const firstDate = new Date(firstDateParts[0], firstDateParts[1] - 1, firstDateParts[2]);
    const startDayOfWeek = firstDate.getDay(); // 0 is Sunday

    for (let i = 0; i < startDayOfWeek; i++) {
      currentWeek.push(null);
    }

    for (const cell of currentData) {
      currentWeek.push(cell);
      if (currentWeek.length === 7) {
        weeksList.push(currentWeek);
        currentWeek = [];
      }
    }

    if (currentWeek.length > 0) {
      while (currentWeek.length < 7) {
        currentWeek.push(null);
      }
      weeksList.push(currentWeek);
    }

    // 计算月份表头
    let lastMonth = -1;
    weeksList.forEach((week, colIdx) => {
      const validDay = week.find((d) => d !== null);
      if (validDay) {
        const parts = validDay.date.split('-').map(Number);
        const m = parts[1] - 1;
        if (m !== lastMonth) {
          labels.push({
            month: `${m + 1}月`,
            colIndex: colIdx,
          });
          lastMonth = m;
        }
      }
    });

    return { weeks: weeksList, monthLabels: labels };
  }, [currentData]);

  const handleMouseEnterCell = (cell: HeatmapCell, e: React.MouseEvent<HTMLDivElement>) => {
    if (!containerRef.current) return;
    const containerRect = containerRef.current.getBoundingClientRect();
    const cellRect = e.currentTarget.getBoundingClientRect();

    // 相对容器的中心坐标
    const x = cellRect.left - containerRect.left + cellRect.width / 2;
    const y = cellRect.top - containerRect.top;

    setHovered({
      cell,
      x,
      y,
      containerWidth: containerRect.width,
    });
  };

  const handleMouseLeave = () => {
    setHovered(null);
  };

  return (
    <div
      ref={containerRef}
      onMouseLeave={handleMouseLeave}
      className="relative theme-bg-card border theme-border rounded-xl p-5 backdrop-blur-sm shadow-xs"
    >
      {/* 头部区域 */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 mb-4">
        <div>
          {title && (
            <h2 className="text-sm font-semibold theme-text-main flex items-center gap-2">
              {icon}
              {title}
            </h2>
          )}
          {subtitle && (
            <p className="text-xs theme-text-muted mt-0.5">{subtitle}</p>
          )}
        </div>

        <div className="flex items-center gap-3">
          {/* 核心副指标 Chips */}
          <div className="hidden md:flex items-center gap-3 text-xs theme-text-muted mr-2">
            {extraHeaderChips}
            {activeDays !== undefined && (
              <div className="flex items-center gap-1.5">
                <span className="w-2 h-2 rounded-full bg-emerald-500" />
                <span>活跃天数:</span>
                <strong className="theme-text-main font-mono">{activeDays} 天</strong>
              </div>
            )}
            {longestStreak !== undefined && (
              <div className="flex items-center gap-1.5">
                <span className="w-2 h-2 rounded-full bg-blue-500" />
                <span>连续活跃:</span>
                <strong className="theme-text-main font-mono">{longestStreak} 天</strong>
              </div>
            )}
            {peakDay && peakCount !== undefined && (
              <div className="flex items-center gap-1.5">
                <span className="w-2 h-2 rounded-full bg-amber-500" />
                <span>单日最高:</span>
                <strong className="theme-text-main font-mono">
                  {peakCount.toLocaleString()} 条
                </strong>
                <span className="text-[10px] opacity-75">({peakDay})</span>
              </div>
            )}
          </div>

          {/* 切换 Tab */}
          {showTabs && (
            <div className="flex theme-bg-sub p-0.5 rounded-lg border theme-border text-xs select-none">
              {cellsUserMsgs && cellsUserMsgs.length > 0 && (
                <button
                  type="button"
                  onClick={() => setTab('user')}
                  className={`px-2.5 py-1 rounded-md font-medium transition-all cursor-pointer ${
                    tab === 'user'
                      ? 'bg-blue-600 text-white shadow-sm'
                      : 'theme-text-muted hover:theme-text-main'
                  }`}
                >
                  按用户消息
                </button>
              )}
              <button
                type="button"
                onClick={() => setTab('msgs')}
                className={`px-2.5 py-1 rounded-md font-medium transition-all cursor-pointer ${
                  tab === 'msgs'
                    ? 'bg-blue-600 text-white shadow-sm'
                    : 'theme-text-muted hover:theme-text-main'
                }`}
              >
                按全部消息
              </button>
              <button
                type="button"
                onClick={() => setTab('convs')}
                className={`px-2.5 py-1 rounded-md font-medium transition-all cursor-pointer ${
                  tab === 'convs'
                    ? 'bg-blue-600 text-white shadow-sm'
                    : 'theme-text-muted hover:theme-text-main'
                }`}
              >
                按会话数
              </button>
            </div>
          )}
        </div>
      </div>

      {/* 52 周日历热力网格 */}
      <div ref={scrollRef} className="overflow-x-auto pb-2 scrollbar-thin">
        <div className="inline-block min-w-full">
          {/* 月份表头 */}
          <div className="flex text-[11px] theme-text-muted mb-1.5 pl-7 h-4 relative select-none">
            {monthLabels.map((lbl, idx) => (
              <span
                key={idx}
                style={{
                  position: 'absolute',
                  left: `${lbl.colIndex * 15 + 28}px`,
                }}
                className="font-medium"
              >
                {lbl.month}
              </span>
            ))}
          </div>

          {/* 网格主体：左侧星期 + 52 周列 */}
          <div className="flex gap-1.5">
            {/* 左侧星期标签 */}
            <div className="flex flex-col justify-between text-[9px] theme-text-muted pr-1 py-0.5 h-[104px] select-none sticky left-0 z-10 theme-bg-card">
              <span>周日</span>
              <span>周二</span>
              <span>周四</span>
              <span>周六</span>
            </div>

            {/* 52 周列 */}
            <div className="flex gap-[3px]">
              {weeks.map((week, wIdx) => (
                <div key={wIdx} className="flex flex-col gap-[3px]">
                  {week.map((cell, dIdx) => {
                    if (!cell) {
                      return (
                        <div
                          key={dIdx}
                          className="w-3 h-3 rounded-[2.5px] opacity-0 pointer-events-none"
                        />
                      );
                    }

                    const isHovered = hovered?.cell.date === cell.date;

                    return (
                      <div
                        key={dIdx}
                        onMouseEnter={(e) => handleMouseEnterCell(cell, e)}
                        onClick={() => onCellClick?.(cell.date)}
                        className={`w-3 h-3 rounded-[2.5px] gh-heatmap-cell lvl-${cell.level} border border-black/5 dark:border-white/5 cursor-pointer transition-transform duration-100 ${
                          isHovered ? 'scale-135 ring-2 ring-blue-500 z-20 shadow-md' : ''
                        }`}
                      />
                    );
                  })}
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>

      {/* 底部 Legend 与说明 */}
      <div className="flex items-center justify-between text-[11px] theme-text-muted mt-3 pt-2.5 border-t theme-border-sub">
        <span className="text-[11px]">
          过去 365 天共活跃{' '}
          <strong className="theme-text-main font-mono">{activeDays ?? 0}</strong> 天
        </span>
        <div className="flex items-center gap-1.5 select-none">
          <span>少</span>
          <span className="w-2.5 h-2.5 rounded-[2px] gh-heatmap-cell lvl-0 border theme-border-sub inline-block" />
          <span className="w-2.5 h-2.5 rounded-[2px] gh-heatmap-cell lvl-1 inline-block" />
          <span className="w-2.5 h-2.5 rounded-[2px] gh-heatmap-cell lvl-2 inline-block" />
          <span className="w-2.5 h-2.5 rounded-[2px] gh-heatmap-cell lvl-3 inline-block" />
          <span className="w-2.5 h-2.5 rounded-[2px] gh-heatmap-cell lvl-4 inline-block" />
          <span>多</span>
        </div>
      </div>

      {/* 精致单例浮动 Tooltip */}
      {hovered && (
        <HeatmapTooltip
          cell={hovered.cell}
          x={hovered.x}
          y={hovered.y}
          containerWidth={hovered.containerWidth}
          isPeak={Boolean(peakDay && hovered.cell.date === peakDay)}
        />
      )}
    </div>
  );
};

interface TooltipProps {
  cell: HeatmapCell;
  x: number;
  y: number;
  containerWidth: number;
  isPeak: boolean;
}

const HeatmapTooltip: React.FC<TooltipProps> = ({
  cell,
  x,
  y,
  containerWidth,
  isPeak,
}) => {
  const weekday = getWeekday(cell.date);
  const userCount = cell.user_count ?? 0;
  const totalMsgs = cell.total_messages ?? (cell.user_count !== undefined ? cell.count : cell.count);
  const convCount = cell.conv_count ?? 0;
  const hasActivity = totalMsgs > 0 || userCount > 0 || convCount > 0 || cell.count > 0;

  // 交互倍率
  const multiplier =
    userCount > 0 && totalMsgs >= userCount
      ? (totalMsgs / userCount).toFixed(1)
      : null;

  // 水平边界防御：如果靠近最右侧，向左偏移；靠近最左侧，向右偏移
  let tooltipLeft = x;
  let transform = 'translate(-50%, -100%) translateY(-10px)';

  if (x < 130) {
    tooltipLeft = Math.max(10, x - 16);
    transform = 'translate(0, -100%) translateY(-10px)';
  } else if (containerWidth - x < 130) {
    tooltipLeft = Math.min(containerWidth - 10, x + 16);
    transform = 'translate(-100%, -100%) translateY(-10px)';
  }

  // 垂直边界防御：若位于顶部第一排（y 较小），Tooltip 显示在下方
  const isTopRow = y < 110;
  if (isTopRow) {
    if (x < 130) {
      transform = 'translate(0, 0) translateY(14px)';
    } else if (containerWidth - x < 130) {
      transform = 'translate(-100%, 0) translateY(14px)';
    } else {
      transform = 'translate(-50%, 0) translateY(14px)';
    }
  }

  return (
    <div
      style={{
        position: 'absolute',
        left: `${tooltipLeft}px`,
        top: `${y}px`,
        transform,
      }}
      className="z-50 pointer-events-none transition-all duration-75 ease-out"
    >
      <div className="bg-neutral-900/95 dark:bg-neutral-900/95 text-neutral-100 backdrop-blur-md border border-white/15 shadow-2xl rounded-lg p-3 min-w-[210px] text-xs">
        {/* Header: 日期 + 星期 + 状态 Badge */}
        <div className="flex items-center justify-between gap-2 pb-2 mb-2 border-b border-white/10">
          <div className="font-semibold text-white flex items-center gap-1.5">
            <span>{cell.date}</span>
            <span className="text-[11px] text-neutral-400 font-normal">
              · {weekday}
            </span>
          </div>
          {isPeak ? (
            <span className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-amber-500/20 text-amber-300 border border-amber-500/30 flex items-center gap-0.5">
              👑 峰值
            </span>
          ) : !hasActivity ? (
            <span className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-neutral-800 text-neutral-400">
              休息
            </span>
          ) : cell.level === 4 ? (
            <span className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-emerald-500/20 text-emerald-300 border border-emerald-500/30">
              极度活跃
            </span>
          ) : null}
        </div>

        {/* 内容指标 */}
        {hasActivity ? (
          <div className="space-y-1.5">
            {/* 用户消息 */}
            <div className="flex items-center justify-between text-neutral-300">
              <span className="flex items-center gap-1.5 text-neutral-400">
                <span className="w-1.5 h-1.5 rounded-full bg-cyan-400" />
                用户提问
              </span>
              <span className="font-mono font-semibold text-cyan-300">
                {userCount.toLocaleString()} <span className="text-[10px] font-normal text-neutral-400">条</span>
              </span>
            </div>

            {/* 全部消息 */}
            <div className="flex items-center justify-between text-neutral-300">
              <span className="flex items-center gap-1.5 text-neutral-400">
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-400" />
                全部消息
              </span>
              <span className="font-mono font-semibold text-emerald-300">
                {totalMsgs.toLocaleString()} <span className="text-[10px] font-normal text-neutral-400">条</span>
              </span>
            </div>

            {/* 会话数 */}
            <div className="flex items-center justify-between text-neutral-300">
              <span className="flex items-center gap-1.5 text-neutral-400">
                <span className="w-1.5 h-1.5 rounded-full bg-blue-400" />
                会话批次
              </span>
              <span className="font-mono font-semibold text-blue-300">
                {convCount.toLocaleString()} <span className="text-[10px] font-normal text-neutral-400">个</span>
              </span>
            </div>

            {/* 交互倍率 */}
            {multiplier && parseFloat(multiplier) > 1 && (
              <div className="pt-1.5 mt-1.5 border-t border-white/10 flex items-center justify-between text-[11px] text-neutral-400">
                <span>🤖 交互杠杆:</span>
                <span className="font-mono text-purple-300 font-medium">
                  1 : {multiplier} 轮
                </span>
              </div>
            )}
          </div>
        ) : (
          <div className="py-1 text-center text-neutral-400 text-[11px]">
            暂无智能体交互记录
          </div>
        )}
      </div>
    </div>
  );
};
