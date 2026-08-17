import React, { useState } from 'react';
import { api } from '../../api/tauriBridge';
import {
  X,
  Bot,
  Flame,
  Sparkles,
  Zap,
  Globe,
  Key,
  Cpu,
  CheckCircle2,
  AlertCircle,
  Loader2,
  ExternalLink,
  Database,
  Sliders,
  Info,
  RefreshCw,
  Sun,
  Moon,
} from 'lucide-react';

export interface AiProviderConfig {
  id: string;
  name: string;
  iconName: 'Flame' | 'Bot' | 'Sparkles' | 'Zap' | 'Globe';
  baseUrl: string;
  defaultModel: string;
  models: string[];
  apiKeyLink: string;
  apiKeyPlaceholder: string;
}

export const AI_PROVIDERS: AiProviderConfig[] = [
  {
    id: 'bailian',
    name: '通义千问（百炼）',
    iconName: 'Flame',
    baseUrl: 'https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1',
    defaultModel: 'qwen3.8-max',
    models: ['qwen3.8-max', 'qwen3.6-flash', 'kimi-k2.7-code', 'kimi-k2.6', 'deepseek-v4-pro'],
    apiKeyLink: 'https://bailian.console.aliyun.com/cn-beijing?tab=plan#/efm/subscription/token-plan/enterprise',
    apiKeyPlaceholder: 'sk-...',
  },
  {
    id: 'deepseek',
    name: 'DeepSeek',
    iconName: 'Bot',
    baseUrl: 'https://api.deepseek.com',
    defaultModel: 'deepseek-v4-flash',
    models: ['deepseek-v4-flash', 'deepseek-v4-pro', 'deepseek-chat', 'deepseek-reasoner'],
    apiKeyLink: 'https://platform.deepseek.com/api_keys',
    apiKeyPlaceholder: 'sk-...',
  },
  {
    id: 'volcano_ark_coding_plan',
    name: '火山方舟 Coding Plan',
    iconName: 'Sparkles',
    baseUrl: 'https://ark.cn-beijing.volces.com/api/coding',
    defaultModel: 'deepseek-v4-flash',
    models: ['deepseek-v4-flash', 'doubao-seed-2.0-mini', 'doubao-seed-evolving', 'minimax-m3', 'kimi-k2.7-code', 'kimi-k3'],
    apiKeyLink: 'https://console.volcengine.com/ark/region:cn-beijing/subscription/coding-plan?projectName=default',
    apiKeyPlaceholder: 'sk-...',
  },
  {
    id: 'zhipu',
    name: '智谱 GLM',
    iconName: 'Zap',
    baseUrl: 'https://open.bigmodel.cn/api/paas/v4',
    defaultModel: 'glm-4-plus',
    models: ['glm-4-plus', 'glm-4-air', 'glm-4-flash', 'codegeex-4'],
    apiKeyLink: 'https://open.bigmodel.cn/usercenter/apikeys',
    apiKeyPlaceholder: 'API Key...',
  },
  {
    id: 'minimax',
    name: 'MiniMax',
    iconName: 'Bot',
    baseUrl: 'https://api.minimax.chat/v1',
    defaultModel: 'MiniMax-Text-01',
    models: ['MiniMax-Text-01', 'abab6.5s-chat'],
    apiKeyLink: 'https://platform.minimaxi.com/user-center/basic-information/interface-key',
    apiKeyPlaceholder: 'sk-...',
  },
  {
    id: 'gemini',
    name: 'Google Gemini',
    iconName: 'Sparkles',
    baseUrl: 'https://generativelanguage.googleapis.com/v1beta/openai',
    defaultModel: 'gemini-2.0-flash',
    models: ['gemini-2.0-flash', 'gemini-2.0-pro-exp-02-05', 'gemini-1.5-pro', 'gemini-1.5-flash'],
    apiKeyLink: 'https://aistudio.google.com/app/apikey',
    apiKeyPlaceholder: 'AIzaSy...',
  },
  {
    id: 'custom',
    name: '自定义 OpenAI 兼容接口',
    iconName: 'Globe',
    baseUrl: 'http://localhost:11434/v1',
    defaultModel: 'llama3.3',
    models: ['llama3.3', 'qwen2.5-coder:32b', 'deepseek-r1:14b'],
    apiKeyLink: '',
    apiKeyPlaceholder: 'sk-...（本地 Ollama 可选留空）',
  },
];

interface Props {
  isOpen: boolean;
  onClose: () => void;
  theme: 'dark' | 'light';
  onToggleTheme: () => void;
  totalConversations?: number;
  totalMessages?: number;
}

export const SettingsModal: React.FC<Props> = ({
  isOpen,
  onClose,
  theme,
  onToggleTheme,
  totalConversations = 0,
  totalMessages = 0,
}) => {
  const [activeTab, setActiveTab] = useState<'ai' | 'storage' | 'appearance' | 'about'>('ai');
  const [selectedProviderId, setSelectedProviderId] = useState<string>(() => {
    return localStorage.getItem('agentdeck_active_ai_provider') || 'bailian';
  });

  // 每个供应商的配置状态
  const [apiKeys, setApiKeys] = useState<Record<string, string>>(() => {
    try {
      const saved = localStorage.getItem('agentdeck_ai_api_keys');
      return saved ? JSON.parse(saved) : {};
    } catch {
      return {};
    }
  });

  const [baseUrls, setBaseUrls] = useState<Record<string, string>>(() => {
    try {
      const saved = localStorage.getItem('agentdeck_ai_base_urls');
      return saved ? JSON.parse(saved) : {};
    } catch {
      return {};
    }
  });

  const [customModels, setCustomModels] = useState<Record<string, string>>(() => {
    try {
      const saved = localStorage.getItem('agentdeck_ai_models');
      return saved ? JSON.parse(saved) : {};
    } catch {
      return {};
    }
  });

  // 测试连接状态
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ success: boolean; msg: string; latency?: number } | null>(null);

  const activeProvider = AI_PROVIDERS.find((p) => p.id === selectedProviderId) || AI_PROVIDERS[0];
  const currentKey = apiKeys[activeProvider.id] || '';
  const currentBaseUrl = baseUrls[activeProvider.id] || activeProvider.baseUrl;
  const currentModel = customModels[activeProvider.id] || activeProvider.defaultModel;

  // 保存到 LocalStorage
  const handleKeyChange = (val: string) => {
    const next = { ...apiKeys, [activeProvider.id]: val };
    setApiKeys(next);
    localStorage.setItem('agentdeck_ai_api_keys', JSON.stringify(next));
  };

  const handleBaseUrlChange = (val: string) => {
    const next = { ...baseUrls, [activeProvider.id]: val };
    setBaseUrls(next);
    localStorage.setItem('agentdeck_ai_base_urls', JSON.stringify(next));
  };

  const handleModelChange = (val: string) => {
    const next = { ...customModels, [activeProvider.id]: val };
    setCustomModels(next);
    localStorage.setItem('agentdeck_ai_models', JSON.stringify(next));
  };

  const handleSetActiveProvider = (id: string) => {
    setSelectedProviderId(id);
    localStorage.setItem('agentdeck_active_ai_provider', id);
    setTestResult(null);
  };

  // 测试连接（通过 Rust 原生端点发起，彻底避免 WebKit/浏览器 CORS 拦截）
  const handleTestConnection = async () => {
    setTesting(true);
    setTestResult(null);

    try {
      const res = await api.testLlmConnection(currentBaseUrl, currentKey, currentModel);
      setTestResult({
        success: res.success,
        msg: res.message,
        latency: res.latency_ms,
      });
    } catch (e: any) {
      setTestResult({
        success: false,
        msg: `请求异常: ${e.message || e}`,
        latency: 0,
      });
    } finally {
      setTesting(false);
    }
  };

  const renderProviderIcon = (iconName: string) => {
    switch (iconName) {
      case 'Flame':
        return <Flame className="h-4 w-4 text-orange-500" />;
      case 'Bot':
        return <Bot className="h-4 w-4 text-blue-500" />;
      case 'Sparkles':
        return <Sparkles className="h-4 w-4 text-purple-500" />;
      case 'Zap':
        return <Zap className="h-4 w-4 text-amber-500" />;
      default:
        return <Globe className="h-4 w-4 text-cyan-500" />;
    }
  };

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4"
      onClick={onClose}
    >
      <div
        className="w-full max-w-3xl theme-bg-card border theme-border rounded-2xl shadow-2xl overflow-hidden flex flex-col h-[80vh] max-h-[680px]"
        onClick={(e) => e.stopPropagation()}
      >
        {/* 顶部标题栏 */}
        <div className="px-6 py-4 border-b theme-border flex items-center justify-between theme-bg-header">
          <div className="flex items-center gap-2.5">
            <div className="p-1.5 rounded-lg bg-blue-500/10 text-blue-500 border border-blue-500/20">
              <Sliders className="h-4 w-4" />
            </div>
            <div>
              <h2 className="text-sm font-bold theme-text-main">应用设置 (Settings)</h2>
              <p className="text-[11px] theme-text-muted">配置 AI 模型供应商、数据同步与全局偏好</p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 rounded-lg theme-text-sub hover:theme-text-main hover:theme-bg-sub transition-colors cursor-pointer"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* 主体左右分栏 */}
        <div className="flex-1 flex overflow-hidden">
          {/* 左侧导航菜单 */}
          <div className="w-48 border-r theme-border theme-bg-sub p-3 space-y-1 flex-shrink-0">
            <button
              onClick={() => setActiveTab('ai')}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-medium transition-all cursor-pointer ${
                activeTab === 'ai'
                  ? 'bg-blue-600 text-white shadow-xs'
                  : 'theme-text-muted hover:theme-text-main hover:theme-bg-card'
              }`}
            >
              <Cpu className="h-4 w-4" />
              <span>AI 供应商配置</span>
            </button>

            <button
              onClick={() => setActiveTab('storage')}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-medium transition-all cursor-pointer ${
                activeTab === 'storage'
                  ? 'bg-blue-600 text-white shadow-xs'
                  : 'theme-text-muted hover:theme-text-main hover:theme-bg-card'
              }`}
            >
              <Database className="h-4 w-4" />
              <span>数据存储与源</span>
            </button>

            <button
              onClick={() => setActiveTab('appearance')}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-medium transition-all cursor-pointer ${
                activeTab === 'appearance'
                  ? 'bg-blue-600 text-white shadow-xs'
                  : 'theme-text-muted hover:theme-text-main hover:theme-bg-card'
              }`}
            >
              <Sun className="h-4 w-4" />
              <span>外观与主题</span>
            </button>

            <button
              onClick={() => setActiveTab('about')}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-medium transition-all cursor-pointer ${
                activeTab === 'about'
                  ? 'bg-blue-600 text-white shadow-xs'
                  : 'theme-text-muted hover:theme-text-main hover:theme-bg-card'
              }`}
            >
              <Info className="h-4 w-4" />
              <span>关于 AgentDeck</span>
            </button>
          </div>

          {/* 右侧设置面板详情 */}
          <div className="flex-1 overflow-y-auto p-6">
            {activeTab === 'ai' && (
              <div className="space-y-5">
                <div>
                  <h3 className="text-sm font-bold theme-text-main">AI 供应商 (LLM Providers)</h3>
                  <p className="text-xs theme-text-muted mt-0.5">
                    选择激活的 AI 大模型，用于自动化会话洞察、深度复盘与代码报告生成。
                  </p>
                </div>

                {/* 供应商选择网格 */}
                <div className="grid grid-cols-2 sm:grid-cols-3 gap-2">
                  {AI_PROVIDERS.map((provider) => {
                    const isSelected = provider.id === selectedProviderId;
                    const hasKey = Boolean(apiKeys[provider.id]);
                    return (
                      <button
                        key={provider.id}
                        onClick={() => handleSetActiveProvider(provider.id)}
                        className={`flex items-center justify-between p-2.5 rounded-xl border text-xs font-medium transition-all cursor-pointer ${
                          isSelected
                            ? 'bg-blue-600/15 border-blue-500/50 theme-text-main shadow-xs'
                            : 'theme-bg-sub theme-border hover:theme-border-hover theme-text-muted hover:theme-text-main'
                        }`}
                      >
                        <div className="flex items-center gap-2">
                          {renderProviderIcon(provider.iconName)}
                          <span className="truncate">{provider.name}</span>
                        </div>
                        {hasKey && (
                          <span className="w-1.5 h-1.5 rounded-full bg-emerald-500 flex-shrink-0" title="已配置 API Key" />
                        )}
                      </button>
                    );
                  })}
                </div>

                {/* 当前供应商配置详情卡片 */}
                <div className="theme-bg-sub border theme-border rounded-xl p-4 space-y-4">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      {renderProviderIcon(activeProvider.iconName)}
                      <span className="font-bold text-xs theme-text-main">{activeProvider.name} 配置</span>
                    </div>
                    {activeProvider.apiKeyLink && (
                      <a
                        href={activeProvider.apiKeyLink}
                        target="_blank"
                        rel="noreferrer"
                        className="text-[11px] text-blue-500 hover:underline flex items-center gap-1"
                      >
                        <span>获取 API Key</span>
                        <ExternalLink className="h-3 w-3" />
                      </a>
                    )}
                  </div>

                  {/* API Key */}
                  <div className="space-y-1.5">
                    <label className="text-[11px] font-medium theme-text-muted flex items-center gap-1">
                      <Key className="h-3 w-3" />
                      <span>API 密钥 (API Key)</span>
                    </label>
                    <input
                      type="password"
                      placeholder={activeProvider.apiKeyPlaceholder}
                      value={currentKey}
                      onChange={(e) => handleKeyChange(e.target.value)}
                      className="w-full px-3 py-2 text-xs theme-bg-card border theme-border rounded-lg theme-text-main placeholder-slate-400 focus:outline-none focus:border-blue-500 font-mono"
                    />
                  </div>

                  {/* Base URL */}
                  <div className="space-y-1.5">
                    <label className="text-[11px] font-medium theme-text-muted flex items-center gap-1">
                      <Globe className="h-3 w-3" />
                      <span>接口端点 (Base URL)</span>
                    </label>
                    <input
                      type="text"
                      value={currentBaseUrl}
                      onChange={(e) => handleBaseUrlChange(e.target.value)}
                      className="w-full px-3 py-2 text-xs theme-bg-card border theme-border rounded-lg theme-text-main placeholder-slate-400 focus:outline-none focus:border-blue-500 font-mono"
                    />
                  </div>

                  {/* 模型选择 */}
                  <div className="space-y-1.5">
                    <label className="text-[11px] font-medium theme-text-muted flex items-center gap-1">
                      <Cpu className="h-3 w-3" />
                      <span>模型名称 (Model)</span>
                    </label>
                    <div className="flex gap-2">
                      <input
                        type="text"
                        value={currentModel}
                        onChange={(e) => handleModelChange(e.target.value)}
                        className="flex-1 px-3 py-2 text-xs theme-bg-card border theme-border rounded-lg theme-text-main placeholder-slate-400 focus:outline-none focus:border-blue-500 font-mono"
                      />
                      {activeProvider.models.length > 0 && (
                        <select
                          value={activeProvider.models.includes(currentModel) ? currentModel : ''}
                          onChange={(e) => {
                            if (e.target.value) handleModelChange(e.target.value);
                          }}
                          className="px-2 py-1.5 text-xs theme-bg-card border theme-border rounded-lg theme-text-main focus:outline-none cursor-pointer"
                        >
                          <option value="">常用预设...</option>
                          {activeProvider.models.map((m) => (
                            <option key={m} value={m}>
                              {m}
                            </option>
                          ))}
                        </select>
                      )}
                    </div>
                  </div>

                  {/* 连接测试与状态 */}
                  <div className="pt-2 border-t theme-border-sub flex items-center justify-between">
                    <button
                      onClick={handleTestConnection}
                      disabled={testing || !currentKey}
                      className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium bg-blue-600 hover:bg-blue-500 disabled:opacity-50 text-white rounded-lg transition-all cursor-pointer shadow-xs"
                    >
                      {testing ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <RefreshCw className="h-3.5 w-3.5" />}
                      <span>测试接口连通性</span>
                    </button>

                    {testResult && (
                      <div
                        className={`text-xs flex items-center gap-1.5 ${
                          testResult.success ? 'text-emerald-500 font-medium' : 'text-red-500 font-medium'
                        }`}
                      >
                        {testResult.success ? <CheckCircle2 className="h-4 w-4" /> : <AlertCircle className="h-4 w-4" />}
                        <span>
                          {testResult.msg} {testResult.latency ? `(${testResult.latency}ms)` : ''}
                        </span>
                      </div>
                    )}
                  </div>
                </div>
              </div>
            )}

            {activeTab === 'storage' && (
              <div className="space-y-5">
                <div>
                  <h3 className="text-sm font-bold theme-text-main">数据存储与同步 (Data Source)</h3>
                  <p className="text-xs theme-text-muted mt-0.5">
                    AgentDeck 直接通过 Rust 本地 IPC 读取 SQLite 会话大库，数据全本地持久化。
                  </p>
                </div>

                <div className="theme-bg-sub border theme-border rounded-xl p-4 space-y-3">
                  <div className="flex items-center justify-between">
                    <span className="text-xs font-semibold theme-text-main">当前主会话库</span>
                    <span className="px-2 py-0.5 text-[10px] bg-emerald-500/15 text-emerald-500 border border-emerald-500/30 rounded font-medium">
                      正常挂载
                    </span>
                  </div>

                  <div className="text-xs font-mono theme-text-muted bg-black/10 dark:bg-black/40 p-2.5 rounded-lg border theme-border break-all">
                    /Users/xiyangxie/workspace/personal/aicoding-chat-viewer/data/antigravity_chats.db
                  </div>

                  <div className="grid grid-cols-2 gap-3 pt-2">
                    <div className="p-2.5 rounded-lg theme-bg-card border theme-border text-xs">
                      <div className="theme-text-muted">聚合会话总数</div>
                      <div className="text-base font-bold theme-text-main font-mono mt-0.5">
                        {totalConversations.toLocaleString()}
                      </div>
                    </div>
                    <div className="p-2.5 rounded-lg theme-bg-card border theme-border text-xs">
                      <div className="theme-text-muted">历史交互消息总数</div>
                      <div className="text-base font-bold theme-text-main font-mono mt-0.5">
                        {totalMessages.toLocaleString()}
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            )}

            {activeTab === 'appearance' && (
              <div className="space-y-5">
                <div>
                  <h3 className="text-sm font-bold theme-text-main">外观与主题偏好 (Appearance)</h3>
                  <p className="text-xs theme-text-muted mt-0.5">
                    切换应用界面色彩风格，支持深色沉浸模式与清爽浅色模式。
                  </p>
                </div>

                <div className="grid grid-cols-2 gap-3">
                  <div
                    onClick={() => {
                      if (theme !== 'dark') onToggleTheme();
                    }}
                    className={`p-4 rounded-xl border text-xs cursor-pointer transition-all flex flex-col items-center gap-2 ${
                      theme === 'dark'
                        ? 'bg-blue-600/15 border-blue-500 theme-text-main shadow-xs'
                        : 'theme-bg-sub theme-border hover:theme-border-hover theme-text-muted'
                    }`}
                  >
                    <Moon className="h-6 w-6 text-indigo-400" />
                    <span className="font-semibold">深色模式 (Dark)</span>
                    <span className="text-[10px] theme-text-sub">极客黑蓝科技质感</span>
                  </div>

                  <div
                    onClick={() => {
                      if (theme !== 'light') onToggleTheme();
                    }}
                    className={`p-4 rounded-xl border text-xs cursor-pointer transition-all flex flex-col items-center gap-2 ${
                      theme === 'light'
                        ? 'bg-blue-600/15 border-blue-500 theme-text-main shadow-xs'
                        : 'theme-bg-sub theme-border hover:theme-border-hover theme-text-muted'
                    }`}
                  >
                    <Sun className="h-6 w-6 text-amber-500" />
                    <span className="font-semibold">浅色模式 (Light)</span>
                    <span className="text-[10px] theme-text-sub">清爽高对比度现代风格</span>
                  </div>
                </div>
              </div>
            )}

            {activeTab === 'about' && (
              <div className="space-y-5">
                <div>
                  <h3 className="text-sm font-bold theme-text-main">关于 AgentDeck</h3>
                  <p className="text-xs theme-text-muted mt-0.5">
                    AI Coding 智能体全景数据资产与会话交互驾驶舱
                  </p>
                </div>

                <div className="theme-bg-sub border theme-border rounded-xl p-4 space-y-3 text-xs">
                  <div className="flex justify-between py-1 border-b theme-border-sub">
                    <span className="theme-text-muted">应用版本</span>
                    <span className="font-mono font-medium theme-text-main">v0.1.0</span>
                  </div>
                  <div className="flex justify-between py-1 border-b theme-border-sub">
                    <span className="theme-text-muted">底层框架</span>
                    <span className="font-mono font-medium theme-text-main">Tauri v2 + Rust + React 19</span>
                  </div>
                  <div className="flex justify-between py-1 border-b theme-border-sub">
                    <span className="theme-text-muted">支持智能体平台</span>
                    <span className="font-medium theme-text-main">Antigravity, Cursor, Claude Code, Codex, Hermes, WorkBuddy</span>
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
