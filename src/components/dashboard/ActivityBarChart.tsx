import React, { useEffect, useMemo, useRef, useState } from 'react';

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

export const ActivityBarChart: React.FC<Props> = ({
  items,
  emptyHint,
  onBarClick,
  showLine = false,
}) => {
  const plotRef = useRef<HTMLDivElement>(null);
  const [plotSize, setPlotSize] = useState({ width: 0, height: 0 });
  const max = Math.max(1, ...items.map((item) => item.count));
  const total = items.reduce((sum, item) => sum + item.count, 0);
  const barScale = showLine ? LINE_BAR_SCALE : 1;

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
        {emptyHint || '暂无数据'}
      </div>
    );
  }

  return (
    <div className="relative">
      <div className="absolute left-0 top-0 text-[10px] theme-text-muted font-mono tabular-nums">
        {max.toLocaleString()}
      </div>
      <div className="relative h-40 pt-4">
        <div ref={plotRef} className="relative h-full">
          <div className="flex items-end h-full" style={{ gap: `${BAR_GAP}px` }}>
            {items.map((item) => {
              const barPct = item.count > 0 ? Math.max(6, (item.count / max) * 100 * barScale) : 0;
              return (
                <div
                  key={item.key}
                  className={`flex-1 min-w-0 h-full flex flex-col items-center justify-end group relative ${
                    onBarClick ? 'cursor-pointer' : ''
                  }`}
                  onClick={onBarClick ? () => onBarClick(item.key) : undefined}
                >
                  <div
                    className="pointer-events-none absolute left-1/2 z-20 mb-1.5 -translate-x-1/2 whitespace-nowrap rounded-md px-2 py-1.5 text-[10px] leading-4 theme-bg-card theme-text-main border theme-border opacity-0 group-hover:opacity-100 transition-opacity shadow-md"
                    style={{ bottom: `${Math.max(barPct, 12)}%` }}
                  >
                    <div className="font-medium">{item.tooltipTitle}</div>
                    <div className="theme-text-muted font-mono mt-0.5">
                      消息 {item.messageCount.toLocaleString()}
                    </div>
                    <div className="theme-text-muted font-mono">
                      用户 {item.userMessageCount.toLocaleString()}
                    </div>
                    <div className="theme-text-muted font-mono">
                      会话 {item.conversationCount.toLocaleString()}
                    </div>
                  </div>
                  <div
                    className={`w-full max-w-6 rounded-t-[3px] transition-all duration-300 ${
                      item.emphasize
                        ? 'bg-blue-500/80 shadow-[0_0_0_1px_rgba(59,130,246,0.35)]'
                        : item.muted
                          ? 'bg-slate-400/40 hover:bg-slate-400/65'
                          : 'bg-blue-500/55 hover:bg-blue-500/80'
                    }`}
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
    </div>
  );
};
