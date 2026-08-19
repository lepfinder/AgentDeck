import React, { useState, useRef } from 'react';
import type { PunchcardSlot } from '../../types';
import { Clock, Crown, TrendingUp } from 'lucide-react';

interface Props {
  punchcardMsgs: PunchcardSlot[];
  punchcardConvs: PunchcardSlot[];
}

const getTimePeriodLabel = (hour: number) => {
  if (hour >= 0 && hour < 6) return '凌晨 / 深夜';
  if (hour >= 6 && hour < 9) return '清晨 / 早晨';
  if (hour >= 9 && hour < 12) return '上午黄金时段';
  if (hour >= 12 && hour < 14) return '午间休息 / 整理';
  if (hour >= 14 && hour < 18) return '下午高效专注期';
  if (hour >= 18 && hour < 21) return '傍晚 / 晚间编码';
  return '夜间灵感爆发';
};

export const HourlyPunchcardCard: React.FC<Props> = ({
  punchcardMsgs,
  punchcardConvs,
}) => {
  const [tab, setTab] = useState<'msgs' | 'convs'>('msgs');
  const [hovered, setHovered] = useState<{
    hour: number;
    msgSlot?: PunchcardSlot;
    convSlot?: PunchcardSlot;
    x: number;
    y: number;
    containerWidth: number;
  } | null>(null);

  const containerRef = useRef<HTMLDivElement>(null);

  const displayData = tab === 'msgs' ? punchcardMsgs : punchcardConvs;

  // 计算峰值 hour
  const peakHour = React.useMemo(() => {
    if (!displayData || displayData.length === 0) return -1;
    let maxCnt = -1;
    let maxH = -1;
    for (const item of displayData) {
      if (item.count > maxCnt) {
        maxCnt = item.count;
        maxH = item.hour;
      }
    }
    return maxH;
  }, [displayData]);

  const handleMouseEnter = (hour: number, e: React.MouseEvent<HTMLDivElement>) => {
    if (!containerRef.current) return;
    const containerRect = containerRef.current.getBoundingClientRect();
    const cellRect = e.currentTarget.getBoundingClientRect();

    const x = cellRect.left - containerRect.left + cellRect.width / 2;
    const y = cellRect.top - containerRect.top;

    const msgSlot = punchcardMsgs.find((s) => s.hour === hour);
    const convSlot = punchcardConvs.find((s) => s.hour === hour);

    setHovered({
      hour,
      msgSlot,
      convSlot,
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
      className="relative theme-bg-card border theme-border rounded-xl p-5 backdrop-blur-sm flex flex-col justify-between"
    >
      <div>
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-sm font-semibold theme-text-main flex items-center gap-2">
            <Clock className="h-4 w-4 text-emerald-500" />
            24 小时活跃时段分布 (Hourly Punchcard)
          </h2>

          {/* 双维切换 Tab */}
          <div className="flex theme-bg-sub p-0.5 rounded-lg border theme-border text-xs select-none">
            <button
              type="button"
              onClick={() => setTab('msgs')}
              className={`px-2.5 py-1 rounded-md font-medium transition-all cursor-pointer ${
                tab === 'msgs'
                  ? 'bg-blue-600 text-white shadow-sm'
                  : 'theme-text-muted hover:theme-text-main'
              }`}
            >
              按消息数
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
        </div>

        {/* 24 小时格子矩阵 */}
        <div className="grid grid-cols-12 gap-1.5 my-3">
          {displayData.map((slot) => {
            const isHovered = hovered?.hour === slot.hour;
            const isPeak = slot.hour === peakHour && slot.count > 0;

            return (
              <div
                key={slot.hour}
                onMouseEnter={(e) => handleMouseEnter(slot.hour, e)}
                className={`punchcard-cell lvl-${slot.level} rounded-md h-12 flex flex-col items-center justify-center transition-all cursor-pointer border theme-border-sub relative ${
                  isHovered ? 'scale-110 ring-2 ring-blue-500 z-20 shadow-lg' : 'hover:scale-105'
                }`}
              >
                <span className="text-[10px] font-bold flex items-center gap-0.5">
                  {slot.hour}h
                  {isPeak && <Crown className="h-2.5 w-2.5 text-amber-400" />}
                </span>
                <span className="text-[9px] opacity-80 font-mono">{slot.count}</span>
              </div>
            );
          })}
        </div>

        {/* 底部图例 */}
        <div className="flex items-center justify-between text-[11px] theme-text-muted pt-2 border-t theme-border-sub select-none">
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

      {/* 精致悬浮 Tooltip */}
      {hovered && (
        <PunchcardTooltip
          hour={hovered.hour}
          msgSlot={hovered.msgSlot}
          convSlot={hovered.convSlot}
          x={hovered.x}
          y={hovered.y}
          containerWidth={hovered.containerWidth}
          isPeak={hovered.hour === peakHour}
        />
      )}
    </div>
  );
};

interface TooltipProps {
  hour: number;
  msgSlot?: PunchcardSlot;
  convSlot?: PunchcardSlot;
  x: number;
  y: number;
  containerWidth: number;
  isPeak: boolean;
}

const PunchcardTooltip: React.FC<TooltipProps> = ({
  hour,
  msgSlot,
  convSlot,
  x,
  y,
  containerWidth,
  isPeak,
}) => {
  const periodLabel = getTimePeriodLabel(hour);
  const hourFormatted = `${hour.toString().padStart(2, '0')}:00 - ${hour.toString().padStart(2, '0')}:59`;
  const msgCount = msgSlot?.count ?? 0;
  const convCount = convSlot?.count ?? 0;
  const hasActivity = msgCount > 0 || convCount > 0;

  // 边界平移
  let tooltipLeft = x;
  let transform = 'translate(-50%, -100%) translateY(-10px)';

  if (x < 110) {
    tooltipLeft = Math.max(10, x - 10);
    transform = 'translate(0, -100%) translateY(-10px)';
  } else if (containerWidth - x < 110) {
    tooltipLeft = Math.min(containerWidth - 10, x + 10);
    transform = 'translate(-100%, -100%) translateY(-10px)';
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
      <div className="bg-neutral-900/80 dark:bg-neutral-950/80 text-neutral-100 backdrop-blur-xl backdrop-saturate-150 border border-white/20 shadow-[0_8px_32px_0_rgba(0,0,0,0.4),inset_0_1px_1px_0_rgba(255,255,255,0.15)] rounded-xl p-3 min-w-[200px] text-xs">
        {/* 头部时间语境 */}
        <div className="flex items-center justify-between gap-2 pb-2 mb-2 border-b border-white/10">
          <div>
            <div className="font-semibold text-white font-mono">{hourFormatted}</div>
            <div className="text-[10px] text-neutral-400 font-normal">{periodLabel}</div>
          </div>
          {isPeak ? (
            <span className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-amber-500/20 text-amber-300 border border-amber-500/30 flex items-center gap-1">
              <Crown className="h-3 w-3 text-amber-400" />
              <span>全天峰值</span>
            </span>
          ) : !hasActivity ? (
            <span className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-neutral-800 text-neutral-400">
              闲置
            </span>
          ) : null}
        </div>

        {/* 指标数据 */}
        {hasActivity ? (
          <div className="space-y-1.5">
            <div className="flex items-center justify-between text-neutral-300">
              <span className="flex items-center gap-1.5 text-neutral-400">
                <span className="w-1.5 h-1.5 rounded-full bg-blue-400" />
                消息总数
              </span>
              <span className="font-mono font-semibold text-blue-300">
                {msgCount.toLocaleString()} <span className="text-[10px] font-normal text-neutral-400">条</span>
              </span>
            </div>

            <div className="flex items-center justify-between text-neutral-300">
              <span className="flex items-center gap-1.5 text-neutral-400">
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-400" />
                活跃会话
              </span>
              <span className="font-mono font-semibold text-emerald-300">
                {convCount.toLocaleString()} <span className="text-[10px] font-normal text-neutral-400">个</span>
              </span>
            </div>

            {msgSlot && msgSlot.percent > 0 && (
              <div className="pt-1.5 mt-1.5 border-t border-white/10 flex items-center justify-between text-[11px] text-neutral-400">
                <span className="flex items-center gap-1 text-neutral-300">
                  <TrendingUp className="h-3.5 w-3.5 text-cyan-400" />
                  <span>相对全天 peak:</span>
                </span>
                <span className="font-mono text-cyan-300 font-medium">
                  {msgSlot.percent}%
                </span>
              </div>
            )}
          </div>
        ) : (
          <div className="py-1 text-center text-neutral-400 text-[11px]">
            历史上该时段暂无活跃交互
          </div>
        )}
      </div>
    </div>
  );
};
