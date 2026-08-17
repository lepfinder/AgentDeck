import { useState, useEffect } from 'react';
import { api } from './api/tauriBridge';
import type { DashboardStats } from './types';
import { DashboardView } from './components/dashboard/DashboardView';
import { BrowseView } from './components/browse/BrowseView';
import { SpotlightModal } from './components/spotlight/SpotlightModal';
import {
  LayoutDashboard,
  MessageSquare,
  Search,
  Sparkles,
  Command,
  Star,
  Sun,
  Moon,
} from 'lucide-react';

export function App() {
  const [currentTab, setCurrentTab] = useState<'dashboard' | 'browse'>('dashboard');
  const [isStarredView, setIsStarredView] = useState(false);
  const [selectedWorkspace, setSelectedWorkspace] = useState('');
  const [selectedConversationId, setSelectedConversationId] = useState('');
  const [isSpotlightOpen, setIsSpotlightOpen] = useState(false);

  // 主题模式支持 (Dark / Light)
  const [theme, setTheme] = useState<'dark' | 'light'>(() => {
    const saved = localStorage.getItem('agentdeck_theme');
    if (saved === 'light' || saved === 'dark') return saved;
    return window.matchMedia && window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
  });

  useEffect(() => {
    localStorage.setItem('agentdeck_theme', theme);
    const root = document.documentElement;
    if (theme === 'light') {
      root.classList.remove('dark');
      root.classList.add('light');
      root.setAttribute('data-theme', 'light');
    } else {
      root.classList.remove('light');
      root.classList.add('dark');
      root.setAttribute('data-theme', 'dark');
    }
  }, [theme]);

  const toggleTheme = () => {
    setTheme((prev) => (prev === 'dark' ? 'light' : 'dark'));
  };

  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [loadingStats, setLoadingStats] = useState(true);

  // 加载大盘数据
  const loadStats = async () => {
    setLoadingStats(true);
    try {
      const data = await api.getDashboardStats();
      setStats(data);
    } catch (e) {
      console.error('Failed to load dashboard stats:', e);
    } finally {
      setLoadingStats(false);
    }
  };

  useEffect(() => {
    loadStats();
  }, []);

  // 监听全局 Cmd+K / Ctrl+K 快捷键
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setIsSpotlightOpen((prev) => !prev);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  // 从大盘或全局搜索直达会话
  const handleSelectConversation = (convId: string, wsPath: string) => {
    setSelectedWorkspace(wsPath);
    setSelectedConversationId(convId);
    setIsStarredView(false);
    setCurrentTab('browse');
  };

  return (
    <div className="flex flex-col h-screen w-screen theme-bg-main theme-text-main overflow-hidden font-sans select-none">
      {/* 顶部全局导航栏（自定义 macOS 红绿灯留白与原生拖拽区） */}
      <header
        data-tauri-drag-region
        className="h-12 border-b theme-border theme-bg-header backdrop-blur-md pl-20 pr-4 flex items-center justify-between flex-shrink-0 z-10"
      >
        {/* 左侧：Logo 与应用标题 */}
        <div className="flex items-center gap-6">
          <div className="flex items-center gap-2">
            <div className="w-6 h-6 rounded-lg bg-gradient-to-br from-blue-500 to-indigo-600 flex items-center justify-center text-white shadow-md shadow-blue-500/20">
              <Sparkles className="h-3.5 w-3.5" />
            </div>
            <span className="font-bold text-sm tracking-tight theme-text-main">AgentDeck</span>
            <span className="text-[10px] text-blue-500 bg-blue-500/10 px-1.5 py-0.2 rounded border border-blue-500/20 font-mono">
              Desktop
            </span>
          </div>

          {/* 中间核心视图 Tab 切换 */}
          <nav className="flex items-center theme-bg-sub p-0.5 rounded-lg border theme-border text-xs">
            <button
              onClick={() => {
                setCurrentTab('dashboard');
                setIsStarredView(false);
              }}
              className={`flex items-center gap-1.5 px-3 py-1 rounded-md font-medium transition-all cursor-pointer ${
                currentTab === 'dashboard'
                  ? 'bg-blue-600 text-white shadow-sm'
                  : 'theme-text-muted hover:theme-text-main'
              }`}
            >
              <LayoutDashboard className="h-3.5 w-3.5" />
              <span>全景大盘</span>
            </button>

            <button
              onClick={() => {
                setCurrentTab('browse');
                setIsStarredView(false);
              }}
              className={`flex items-center gap-1.5 px-3 py-1 rounded-md font-medium transition-all cursor-pointer ${
                currentTab === 'browse' && !isStarredView
                  ? 'bg-blue-600 text-white shadow-sm'
                  : 'theme-text-muted hover:theme-text-main'
              }`}
            >
              <MessageSquare className="h-3.5 w-3.5" />
              <span>会话浏览</span>
            </button>

            <button
              onClick={() => {
                setCurrentTab('browse');
                setIsStarredView(true);
              }}
              className={`flex items-center gap-1.5 px-3 py-1 rounded-md font-medium transition-all cursor-pointer ${
                currentTab === 'browse' && isStarredView
                  ? 'bg-amber-500/20 text-amber-500 dark:text-amber-300 border border-amber-500/40 shadow-sm'
                  : 'theme-text-muted hover:text-amber-500'
              }`}
            >
              <Star className="h-3.5 w-3.5 fill-amber-400 text-amber-400" />
              <span>我的收藏</span>
            </button>
          </nav>
        </div>

        {/* 右侧：Spotlight 搜索唤起入口 + 亮暗色切换 */}
        <div className="flex items-center gap-2.5">
          <button
            onClick={() => setIsSpotlightOpen(true)}
            className="flex items-center gap-2 px-3 py-1 text-xs theme-bg-sub hover:opacity-90 border theme-border rounded-lg theme-text-muted hover:theme-text-main transition-colors cursor-pointer shadow-sm"
          >
            <Search className="h-3.5 w-3.5 text-blue-500" />
            <span>搜索全量会话…</span>
            <div className="flex items-center gap-0.5 text-[10px] theme-text-sub theme-bg-input px-1.5 py-0.5 rounded border theme-border">
              <Command className="h-2.5 w-2.5" />
              <span>K</span>
            </div>
          </button>

          {/* 浅色 / 深色模式切换按钮 */}
          <button
            onClick={toggleTheme}
            title={theme === 'dark' ? '切换为浅色模式' : '切换为深色模式'}
            className="p-1.5 rounded-lg border theme-border theme-bg-sub hover:opacity-80 theme-text-muted hover:theme-text-main transition-colors cursor-pointer"
          >
            {theme === 'dark' ? (
              <Sun className="h-4 w-4 text-amber-400" />
            ) : (
              <Moon className="h-4 w-4 text-slate-700" />
            )}
          </button>
        </div>
      </header>

      {/* 主视图区域 */}
      <div className="flex-1 overflow-hidden">
        {currentTab === 'dashboard' ? (
          <DashboardView
            stats={stats}
            loading={loadingStats}
            onRefresh={loadStats}
            onSelectConversation={handleSelectConversation}
          />
        ) : (
          <BrowseView
            selectedWorkspace={selectedWorkspace}
            selectedConversationId={selectedConversationId}
            isStarredView={isStarredView}
            onSelectWorkspace={(ws) => {
              setSelectedWorkspace(ws);
              setIsStarredView(false);
            }}
            onSelectConversation={setSelectedConversationId}
            onSwitchToDashboard={() => {
              setCurrentTab('dashboard');
              setIsStarredView(false);
            }}
            onSwitchToStarred={() => {
              setIsStarredView(true);
            }}
          />
        )}
      </div>

      {/* Spotlight 全局浮窗搜索 */}
      <SpotlightModal
        isOpen={isSpotlightOpen}
        onClose={() => setIsSpotlightOpen(false)}
        onSelectResult={handleSelectConversation}
      />
    </div>
  );
}

export default App;
