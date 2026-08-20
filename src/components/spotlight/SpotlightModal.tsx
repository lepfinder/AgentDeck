import React, { useState, useEffect, useRef } from 'react';
import type { ConversationItem, SearchResultItem } from '../../types';
import { api } from '../../api/tauriBridge';
import { Clock3, Search, X } from 'lucide-react';
import { formatRelativeTime } from '../../utils/date';
import { useI18n } from '../../i18n';

function HighlightText({ text, query }: { text: string; query: string }) {
  const q = query.trim();
  if (!q) return <>{text}</>;
  const escaped = q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const parts = text.split(new RegExp(`(${escaped})`, 'gi'));
  return (
    <>
      {parts.map((part, i) =>
        part.toLowerCase() === q.toLowerCase() ? (
          <mark
            key={`${part}-${i}`}
            className="rounded-sm bg-amber-300/90 px-0.5 font-medium text-slate-900 dark:bg-amber-400/85"
          >
            {part}
          </mark>
        ) : (
          <span key={`${part}-${i}`}>{part}</span>
        ),
      )}
    </>
  );
}

interface Props {
  isOpen: boolean;
  onClose: () => void;
  onSelectResult: (convId: string, wsPath: string) => void;
}

export const SpotlightModal: React.FC<Props> = ({ isOpen, onClose, onSelectResult }) => {
  const { t } = useI18n();
  const [query, setQuery] = useState('');
  const [role, setRole] = useState<'user' | 'all'>('user');
  const [results, setResults] = useState<SearchResultItem[]>([]);
  const [recentConversations, setRecentConversations] = useState<ConversationItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);

  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (isOpen) {
      setTimeout(() => inputRef.current?.focus(), 50);
    } else {
      setQuery('');
      setResults([]);
      setRecentConversations([]);
    }
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen) return;

    const timer = setTimeout(async () => {
      setLoading(true);
      try {
        if (query.trim()) {
          const items = await api.searchMessages(query, role === 'user' ? 'user' : undefined, 30);
          setResults(items);
          setRecentConversations([]);
        } else {
          const items = await api.listConversations(undefined, undefined, false);
          setRecentConversations(items.slice(0, 12));
          setResults([]);
        }
        setSelectedIndex(0);
      } catch (e) {
        console.error(e);
      } finally {
        setLoading(false);
      }
    }, 250);

    return () => clearTimeout(timer);
  }, [isOpen, query, role]);

  // 键盘导航
  const handleKeyDown = (e: React.KeyboardEvent) => {
    const itemCount = query.trim() ? results.length : recentConversations.length;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelectedIndex((prev) => (itemCount > 0 ? (prev + 1) % itemCount : 0));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelectedIndex((prev) => (itemCount > 0 ? (prev - 1 + itemCount) % itemCount : 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (!query.trim() && recentConversations[selectedIndex]) {
        const item = recentConversations[selectedIndex];
        onSelectResult(item.id, item.workspace_path);
        onClose();
      } else if (results[selectedIndex]) {
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
            placeholder={t('spotlight.placeholder')}
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
              {t('spotlight.user')}
            </button>
            <button
              onClick={() => setRole('all')}
              className={`px-2.5 py-1 rounded font-medium transition-all cursor-pointer ${
                role === 'all'
                  ? 'bg-blue-600 text-white shadow-xs'
                  : 'theme-text-muted hover:theme-text-main'
              }`}
            >
              {t('spotlight.all')}
            </button>
          </div>
        </div>

        {/* 结果列表 */}
        <div className="flex-1 overflow-y-auto p-2 space-y-1">
          {loading ? (
            <div className="py-12 text-center text-xs theme-text-muted">
              {query.trim() ? t('spotlight.searching') : t('spotlight.loadingRecent')}
            </div>
          ) : !query.trim() && recentConversations.length > 0 ? (
            <>
              <div className="flex items-center gap-1.5 px-2 py-2 text-[11px] font-medium theme-text-sub">
                <Clock3 className="h-3.5 w-3.5" />
                {t('spotlight.recent')}
              </div>
              {recentConversations.map((item, idx) => {
                const isSelected = idx === selectedIndex;
                const workspaceName = item.workspace_path.split('/').filter(Boolean).pop() || t('nav.uncategorized');
                return (
                  <div
                    key={item.id}
                    onClick={() => {
                      onSelectResult(item.id, item.workspace_path);
                      onClose();
                    }}
                    className={`flex items-center justify-between gap-4 px-3 py-2.5 rounded-xl border text-xs cursor-pointer transition-all ${
                      isSelected
                        ? 'bg-blue-600/15 border-blue-500/50 theme-text-main shadow-xs'
                        : 'theme-bg-sub border-transparent hover:theme-border theme-text-muted hover:theme-text-main'
                    }`}
                  >
                    <div className="min-w-0 flex items-center gap-2">
                      <span className="h-1.5 w-1.5 rounded-full bg-blue-500 flex-shrink-0" />
                      <span className="font-medium theme-text-main truncate">{item.title || t('conv.untitled')}</span>
                    </div>
                    <div className="flex-shrink-0 flex items-center gap-2 text-[10px] theme-text-sub">
                      <span className="px-1.5 py-0.5 bg-blue-500/15 text-blue-500 rounded font-medium">
                        {item.source_app}
                      </span>
                      <span className="max-w-28 truncate">{workspaceName}</span>
                      <span>{formatRelativeTime(item.updated_at || item.created_at)}</span>
                    </div>
                  </div>
                );
              })}
            </>
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
                      <span className="truncate">
                        <HighlightText text={item.conversation_title} query={query} />
                      </span>
                    </div>
                    <span className="text-[10px] theme-text-sub flex-shrink-0">
                      {item.sender === 'user' ? 'User' : 'Assistant'}
                    </span>
                  </div>
                  <div className="theme-text-muted line-clamp-2 text-[11px] leading-relaxed">
                    <HighlightText text={item.snippet} query={query} />
                  </div>
                </div>
              );
            })
          ) : query.trim() ? (
            <div className="py-12 text-center text-xs theme-text-sub">{t('spotlight.emptyMsg')}</div>
          ) : (
            <div className="py-12 text-center text-xs theme-text-sub">
              {t('spotlight.emptyRecent')}
            </div>
          )}
        </div>

        {/* 底部快捷键提示 */}
        <div className="p-3 border-t theme-border theme-bg-sub flex items-center justify-between text-[11px] theme-text-sub">
          <div className="flex items-center gap-3">
            <span className="flex items-center gap-1">
              <kbd className="px-1.5 py-0.5 theme-bg-card rounded border theme-border font-mono">↑</kbd>
              <kbd className="px-1.5 py-0.5 theme-bg-card rounded border theme-border font-mono">↓</kbd> {t('spotlight.nav')}
            </span>
            <span className="flex items-center gap-1">
              <kbd className="px-1.5 py-0.5 theme-bg-card rounded border theme-border font-mono">↵</kbd> {t('spotlight.jump')}
            </span>
            <span className="flex items-center gap-1">
              <kbd className="px-1.5 py-0.5 theme-bg-card rounded border theme-border font-mono">Esc</kbd> {t('spotlight.esc')}
            </span>
          </div>
          <span className="text-blue-500 font-medium">AgentDeck Spotlight</span>
        </div>
      </div>
    </div>
  );
};
