import React from 'react';

export interface ActivityBarItem {
  key: string;
  label: string;
  count: number;
  emphasize?: boolean;
  muted?: boolean;
  tooltipTitle: string;
  messageCount: number;
  conversationCount: number;
}

interface Props {
  items: ActivityBarItem[];
  emptyHint?: string;
  onBarClick?: (key: string) => void;
}

export const ActivityBarChart: React.FC<Props> = ({ items, emptyHint, onBarClick }) => {
  const max = Math.max(1, ...items.map((item) => item.count));
  const total = items.reduce((sum, item) => sum + item.count, 0);

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
      <div className="flex items-end gap-[3px] h-40 pt-4 pb-0">
        {items.map((item) => {
          const pct = item.count > 0 ? Math.max(6, (item.count / max) * 100) : 0;
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
                style={{ bottom: `${Math.max(pct, 12)}%` }}
              >
                <div className="font-medium">{item.tooltipTitle}</div>
                <div className="theme-text-muted font-mono mt-0.5">
                  消息 {item.messageCount.toLocaleString()}
                </div>
                <div className="theme-text-muted font-mono">
                  会话 {item.conversationCount.toLocaleString()}
                </div>
              </div>
              <div
                className={`w-full max-w-6 rounded-t-[3px] transition-all duration-300 ${
                  item.emphasize
                    ? 'bg-blue-500 shadow-[0_0_0_1px_rgba(59,130,246,0.35)]'
                    : item.muted
                    ? 'bg-slate-400/45 hover:bg-slate-400/70'
                    : 'bg-blue-500/70 hover:bg-blue-500'
                }`}
                style={{ height: `${pct}%` }}
              />
            </div>
          );
        })}
      </div>
      <div className="flex gap-[3px] mt-1.5">
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
