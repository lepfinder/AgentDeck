import React, { useState, useEffect, useRef } from 'react';
import type { SearchResultItem } from '../../types';
import { api } from '../../api/tauriBridge';
import { Search, X } from 'lucide-react';

interface Props {
  isOpen: boolean;
  onClose: () => void;
  onSelectResult: (convId: string, wsPath: string) => void;
}

export const SpotlightModal: React.FC<Props> = ({ isOpen, onClose, onSelectResult }) => {
  const [query, setQuery] = useState('');
  const [role, setRole] = useState<'user' | 'all'>('user');
  const [results, setResults] = useState<SearchResultItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);

  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (isOpen) {
      setTimeout(() => inputRef.current?.focus(), 50);
    } else {
      setQuery('');
      setResults([]);
    }
  }, [isOpen]);

  useEffect(() => {
    if (!query.trim()) {
      setResults([]);
      return;
    }
    const timer = setTimeout(async () => {
      setLoading(true);
      try {
        const items = await api.searchMessages(query, role === 'user' ? 'user' : undefined, 30);
        setResults(items);
        setSelectedIndex(0);
      } catch (e) {
        console.error(e);
      } finally {
        setLoading(false);
      }
    }, 250);

    return () => clearTimeout(timer);
  }, [query, role]);

  // 键盘导航
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelectedIndex((prev) => (results.length > 0 ? (prev + 1) % results.length : 0));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelectedIndex((prev) => (results.length > 0 ? (prev - 1 + results.length) % results.length : 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (results[selectedIndex]) {
        const item = results[selectedIndex];
        onSelectResult(item.conversation_id, item.workspace_path);
        onClose();
      }
    } else if (e.key === 'Escape') {
      onClose();
    }
  };

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-20 bg-black/60 backdrop-blur-sm p-4"
      onClick={onClose}
    >
      <div
        className="w-full max-w-2xl theme-bg-card border theme-border rounded-2xl shadow-2xl overflow-hidden flex flex-col max-h-[75vh]"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={handleKeyDown}
      >
        {/* 顶部搜索输入框 */}
        <div className="p-4 border-b theme-border flex items-center gap-3">
          <Search className="h-5 w-5 text-blue-500 flex-shrink-0" />
          <input
            ref={inputRef}
            type="text"
            placeholder="搜索全库会话提问、代码片段与知识..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="flex-1 bg-transparent text-sm theme-text-main placeholder-slate-400 focus:outline-none"
          />
          {query && (
            <button onClick={() => setQuery('')} className="theme-text-sub hover:theme-text-main cursor-pointer">
              <X className="h-4 w-4" />
            </button>
          )}

          {/* 角色范围过滤 */}
          <div className="flex theme-bg-sub p-0.5 rounded-lg border theme-border text-xs">
            <button
              onClick={() => setRole('user')}
              className={`px-2.5 py-1 rounded font-medium transition-all cursor-pointer ${
                role === 'user'
                  ? 'bg-blue-600 text-white shadow-xs'
                  : 'theme-text-muted hover:theme-text-main'
              }`}
            >
              用户提问
            </button>
            <button
              onClick={() => setRole('all')}
              className={`px-2.5 py-1 rounded font-medium transition-all cursor-pointer ${
                role === 'all'
                  ? 'bg-blue-600 text-white shadow-xs'
                  : 'theme-text-muted hover:theme-text-main'
              }`}
            >
              全部消息
            </button>
          </div>
        </div>

        {/* 结果列表 */}
        <div className="flex-1 overflow-y-auto p-2 space-y-1">
          {loading ? (
            <div className="py-12 text-center text-xs theme-text-muted">正在搜索全库会话记录…</div>
          ) : results.length > 0 ? (
            results.map((item, idx) => {
              const isSelected = idx === selectedIndex;
              return (
                <div
                  key={item.message_id}
                  onClick={() => {
                    onSelectResult(item.conversation_id, item.workspace_path);
                    onClose();
                  }}
                  className={`p-3 rounded-xl border text-xs cursor-pointer transition-all ${
                    isSelected
                      ? 'bg-blue-600/15 border-blue-500/50 theme-text-main shadow-xs'
                      : 'theme-bg-sub border-transparent hover:theme-border theme-text-muted hover:theme-text-main'
                  }`}
                >
                  <div className="flex items-center justify-between gap-2 mb-1.5">
                    <div className="flex items-center gap-1.5 font-semibold theme-text-main truncate">
                      <span className="px-1.5 py-0.2 text-[10px] bg-blue-500/15 text-blue-500 rounded font-medium">
                        {item.source_app}
                      </span>
                      <span className="truncate">{item.conversation_title}</span>
                    </div>
                    <span className="text-[10px] theme-text-sub flex-shrink-0">
                      {item.sender === 'user' ? 'User' : 'Assistant'}
                    </span>
                  </div>
                  <div className="theme-text-muted line-clamp-2 text-[11px] leading-relaxed">
                    {item.snippet}
                  </div>
                </div>
              );
            })
          ) : query.trim() ? (
            <div className="py-12 text-center text-xs theme-text-sub">未找到匹配的消息记录</div>
          ) : (
            <div className="py-12 text-center text-xs theme-text-sub">
              输入关键词，快速检索 700+ 会话与代码
            </div>
          )}
        </div>

        {/* 底部快捷键提示 */}
        <div className="p-3 border-t theme-border theme-bg-sub flex items-center justify-between text-[11px] theme-text-sub">
          <div className="flex items-center gap-3">
            <span className="flex items-center gap-1">
              <kbd className="px-1.5 py-0.5 theme-bg-card rounded border theme-border font-mono">↑</kbd>
              <kbd className="px-1.5 py-0.5 theme-bg-card rounded border theme-border font-mono">↓</kbd> 导航
            </span>
            <span className="flex items-center gap-1">
              <kbd className="px-1.5 py-0.5 theme-bg-card rounded border theme-border font-mono">↵</kbd> 跳转
            </span>
            <span className="flex items-center gap-1">
              <kbd className="px-1.5 py-0.5 theme-bg-card rounded border theme-border font-mono">Esc</kbd> 退出
            </span>
          </div>
          <span className="text-blue-500 font-medium">AgentDeck Spotlight</span>
        </div>
      </div>
    </div>
  );
};
