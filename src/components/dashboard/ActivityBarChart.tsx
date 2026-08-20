import React, { useEffect, useMemo, useRef, useState } from 'react';
import { Crown, Bot } from 'lucide-react';
import { useI18n, weekdayLabel } from '../../i18n';

export interface ActivityBarItem {
  key: string;
  label: string;
  count: number;
  emphasize?: boolean;
  muted?: boolean;
  tooltipTitle: string;
  messageCount: number;
  userMessageCount: number;
  conversationCount: number;
}

interface Props {
  items: ActivityBarItem[];
  emptyHint?: string;
  onBarClick?: (key: string) => void;
  showLine?: boolean;
}

const BAR_GAP = 3;
const LINE_LIFT = 16;
const LINE_BAR_SCALE = 0.72;
function parseTitleContext(title: string) {
  const match = title.match(/(\d{4})-(\d{2})-(\d{2})/);
  if (match) {
    const dateStr = match[0];
    const parts = dateStr.split('-').map(Number);
    const d = new Date(parts[0], parts[1] - 1, parts[2]);
    const weekday = weekdayLabel(d.getDay());
    if (title.includes(':')) {
      const timePart = title.split(' ')[1] || '';
      const hour = timePart.split(':')[0];
      return {
        main: `${dateStr} ${hour}:00 - ${hour}:59`,
        sub: weekday ? `· ${weekday}` : '',
      };
    }
    return {
      main: dateStr,
      sub: weekday ? `· ${weekday}` : '',
    };
  }
  return {
    main: title,
    sub: '',
  };
}

export const ActivityBarChart: React.FC<Props> = ({
  items,
  emptyHint,
  onBarClick,
  showLine = false,
}) => {
  const { t } = useI18n();
  const containerRef = useRef<HTMLDivElement>(null);
  const plotRef = useRef<HTMLDivElement>(null);
  const [plotSize, setPlotSize] = useState({ width: 0, height: 0 });
  const [hovered, setHovered] = useState<{
    item: ActivityBarItem;
    x: number;
    y: number;
    containerWidth: number;
    isPeak: boolean;
  } | null>(null);

  const max = Math.max(1, ...items.map((item) => item.count));
  const total = items.reduce((sum, item) => sum + item.count, 0);
  const barScale = showLine ? LINE_BAR_SCALE : 1;

  const peakItem = useMemo(() => {
    if (items.length === 0) return null;
    return items.reduce((best, item) => (item.count > best.count ? item : best));
  }, [items]);

  useEffect(() => {
    if (!showLine) return;
    const el = plotRef.current;
    if (!el) return;
    const update = () => {
      setPlotSize({ width: el.clientWidth, height: el.clientHeight });
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(el);
    return () => observer.disconnect();
  }, [items.length, showLine]);

  const line = useMemo(() => {
    const { width, height } = plotSize;
    if (width <= 0 || height <= 0 || items.length === 0) {
      return { points: '', dots: [] as { x: number; y: number; emphasize?: boolean }[] };
    }
    const n = items.length;
    const slot = (width - BAR_GAP * (n - 1)) / n;
    const dots = items.map((item, idx) => {
      const x = idx * (slot + BAR_GAP) + slot / 2;
      const y = Math.max(2, height - (item.count / max) * height * barScale - LINE_LIFT);
      return { x, y, emphasize: item.emphasize };
    });
    return {
      points: dots.map((dot) => `${dot.x},${dot.y}`).join(' '),
      dots,
    };
  }, [items, max, plotSize, barScale]);

  if (items.length === 0 || total === 0) {
    return (
      <div className="h-40 flex items-center justify-center text-xs theme-text-muted border border-dashed theme-border rounded-lg">
        {emptyHint || t('bar.empty')}
      </div>
    );
  }

  const handleMouseEnter = (item: ActivityBarItem, e: React.MouseEvent<HTMLDivElement>) => {
    if (!containerRef.current) return;
    const containerRect = containerRef.current.getBoundingClientRect();
    const cellRect = e.currentTarget.getBoundingClientRect();
    const x = cellRect.left - containerRect.left + cellRect.width / 2;
    const y = cellRect.top - containerRect.top;
    setHovered({
      item,
      x,
      y,
      containerWidth: containerRect.width,
      isPeak: Boolean(peakItem && peakItem.count > 0 && peakItem.key === item.key),
    });
  };

  const handleMouseLeave = () => {
    setHovered(null);
  };

  return (
    <div ref={containerRef} onMouseLeave={handleMouseLeave} className="relative">
      <div className="absolute left-0 top-0 text-[10px] theme-text-muted font-mono tabular-nums">
        {max.toLocaleString()}
      </div>
      <div className="relative h-40 pt-4">
        <div ref={plotRef} className="relative h-full">
          <div className="flex items-end h-full" style={{ gap: `${BAR_GAP}px` }}>
            {items.map((item) => {
              const barPct = item.count > 0 ? Math.max(6, (item.count / max) * 100 * barScale) : 0;
              const isHovered = hovered?.item.key === item.key;
              return (
                <div
                  key={item.key}
                  className={`flex-1 min-w-0 h-full flex flex-col items-center justify-end relative ${
                    onBarClick ? 'cursor-pointer' : ''
                  }`}
                  onMouseEnter={(e) => handleMouseEnter(item, e)}
                  onClick={onBarClick ? () => onBarClick(item.key) : undefined}
                >
                  <div
                    className={`w-full max-w-6 rounded-t-[3px] transition-all duration-200 ${
                      item.emphasize
                        ? 'bg-blue-500/80 shadow-[0_0_0_1px_rgba(59,130,246,0.35)]'
                        : item.muted
                          ? 'bg-slate-400/40 hover:bg-slate-400/70'
                          : 'bg-blue-500/55 hover:bg-blue-500/85'
                    } ${isHovered ? 'ring-2 ring-blue-500/80 shadow-md scale-y-105 origin-bottom' : ''}`}
                    style={{ height: `${barPct}%` }}
                  />
                </div>
              );
            })}
          </div>
          {showLine && plotSize.width > 0 && (
            <svg
              className="pointer-events-none absolute inset-0 z-10"
              width={plotSize.width}
              height={plotSize.height}
              viewBox={`0 0 ${plotSize.width} ${plotSize.height}`}
              aria-hidden="true"
            >
              <polyline
                points={line.points}
                fill="none"
                stroke="rgb(37 99 235)"
                strokeWidth="2"
                strokeLinejoin="round"
                strokeLinecap="round"
              />
              {line.dots.map((dot, idx) => (
                <circle
                  key={items[idx]?.key ?? idx}
                  cx={dot.x}
                  cy={dot.y}
                  r={dot.emphasize ? 3 : 2.25}
                  fill={dot.emphasize ? 'rgb(37 99 235)' : 'rgb(59 130 246)'}
                  stroke="white"
                  strokeWidth="1"
                />
              ))}
            </svg>
          )}
        </div>
      </div>
      <div className="flex mt-1.5" style={{ gap: `${BAR_GAP}px` }}>
        {items.map((item) => (
          <div key={`${item.key}-label`} className="flex-1 min-w-0 text-center">
            <span
              className={`block text-[9px] leading-none truncate ${
                item.emphasize
                  ? 'theme-text-main font-semibold'
                  : item.muted
                    ? 'theme-text-sub'
                    : 'theme-text-muted'
              }`}
            >
              {item.label}
            </span>
          </div>
        ))}
      </div>

      {/* 精致悬浮 Tooltip（对齐 ContributionHeatmap 视觉体系） */}
      {hovered && (
        <ActivityBarTooltip
          item={hovered.item}
          x={hovered.x}
          y={hovered.y}
          containerWidth={hovered.containerWidth}
          isPeak={hovered.isPeak}
        />
      )}
    </div>
  );
};

interface TooltipProps {
  item: ActivityBarItem;
  x: number;
  y: number;
  containerWidth: number;
  isPeak: boolean;
}

const ActivityBarTooltip: React.FC<TooltipProps> = ({
  item,
  x,
  y,
  containerWidth,
  isPeak,
}) => {
  const { t } = useI18n();
  const { main, sub } = parseTitleContext(item.tooltipTitle);
  const userCount = item.userMessageCount;
  const totalMsgs = item.messageCount;
  const convCount = item.conversationCount;
  const hasActivity = totalMsgs > 0 || userCount > 0 || convCount > 0 || item.count > 0;

  const multiplier =
    userCount > 0 && totalMsgs >= userCount
      ? (totalMsgs / userCount).toFixed(1)
      : null;

  let tooltipLeft = x;
  let transform = 'translate(-50%, -100%) translateY(-10px)';

  if (x < 130) {
    tooltipLeft = Math.max(10, x - 10);
    transform = 'translate(0, -100%) translateY(-10px)';
  } else if (containerWidth - x < 130) {
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
      <div className="bg-neutral-900/80 dark:bg-neutral-950/80 text-neutral-100 backdrop-blur-xl backdrop-saturate-150 border border-white/20 shadow-[0_8px_32px_0_rgba(0,0,0,0.4),inset_0_1px_1px_0_rgba(255,255,255,0.15)] rounded-xl p-3 min-w-[210px] text-xs">
        {/* Header: 日期/时间 + 星期 + 状态 Badge */}
        <div className="flex items-center justify-between gap-2 pb-2 mb-2 border-b border-white/10">
          <div className="font-semibold text-white flex items-center gap-1.5 font-mono">
            <span>{main}</span>
            {sub && (
              <span className="text-[11px] text-neutral-400 font-normal">
                {sub}
              </span>
            )}
          </div>
          {isPeak ? (
            <span className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-amber-500/20 text-amber-300 border border-amber-500/30 flex items-center gap-1">
              <Crown className="h-3 w-3 text-amber-400" />
              <span>{t('bar.peak')}</span>
            </span>
          ) : !hasActivity ? (
            <span className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-neutral-800 text-neutral-400">
              {t('bar.rest')}
            </span>
          ) : item.emphasize ? (
            <span className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-blue-500/20 text-blue-300 border border-blue-500/30">
              {t('bar.current')}
            </span>
          ) : null}
        </div>

        {/* 内容指标 */}
        {hasActivity ? (
          <div className="space-y-1.5">
            {/* 用户提问 */}
            <div className="flex items-center justify-between text-neutral-300">
              <span className="flex items-center gap-1.5 text-neutral-400">
                <span className="w-1.5 h-1.5 rounded-full bg-cyan-400" />
                {t('heatmap.userQ')}
              </span>
              <span className="font-mono font-semibold text-cyan-300">
                {userCount.toLocaleString()}{' '}
                <span className="text-[10px] font-normal text-neutral-400">{t('heatmap.unitTiao')}</span>
              </span>
            </div>

            {/* 全部消息 */}
            <div className="flex items-center justify-between text-neutral-300">
              <span className="flex items-center gap-1.5 text-neutral-400">
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-400" />
                {t('heatmap.allMsg')}
              </span>
              <span className="font-mono font-semibold text-emerald-300">
                {totalMsgs.toLocaleString()}{' '}
                <span className="text-[10px] font-normal text-neutral-400">{t('heatmap.unitTiao')}</span>
              </span>
            </div>

            {/* 会话数 */}
            <div className="flex items-center justify-between text-neutral-300">
              <span className="flex items-center gap-1.5 text-neutral-400">
                <span className="w-1.5 h-1.5 rounded-full bg-blue-400" />
                {t('heatmap.batches')}
              </span>
              <span className="font-mono font-semibold text-blue-300">
                {convCount.toLocaleString()}{' '}
                <span className="text-[10px] font-normal text-neutral-400">{t('heatmap.unitGe')}</span>
              </span>
            </div>

            {/* 交互倍率 */}
            {multiplier && parseFloat(multiplier) > 1 && (
              <div className="pt-1.5 mt-1.5 border-t border-white/10 flex items-center justify-between text-[11px] text-neutral-400">
                <span className="flex items-center gap-1 text-neutral-300">
                  <Bot className="h-3.5 w-3.5 text-purple-400" />
                  <span>{t('heatmap.leverage')}</span>
                </span>
                <span className="font-mono text-purple-300 font-medium">
                  {t('heatmap.rounds', { n: multiplier })}
                </span>
              </div>
            )}
          </div>
        ) : (
          <div className="py-1 text-center text-neutral-400 text-[11px]">
            {t('bar.noRecord')}
          </div>
        )}
      </div>
    </div>
  );
};
