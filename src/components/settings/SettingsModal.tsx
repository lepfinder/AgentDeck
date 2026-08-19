import React, { useState, useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
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
  Archive,
  Cloud,
  CloudRain,
  Folder,
  RotateCcw,
  FileArchive,
  Check,
  Server,
  Copy,
  Terminal,
  FileText,
  Lock,
} from 'lucide-react';
import { CustomSelect } from '../common/CustomSelect';
import { getApiDocsMarkdown } from '../../utils/apiDocsMarkdown';
import type { CloudPreset, BackupInfo, BackupProgress } from '../../types';

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
    baseUrl: 'https://ark.cn-beijing.volces.com/api/coding/v3',
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
  autoSyncIntervalSec: number;
  onAutoSyncIntervalChange: (seconds: number) => void;
  totalConversations?: number;
  totalMessages?: number;
  appVersion?: string;
}

export const SettingsModal: React.FC<Props> = ({
  isOpen,
  onClose,
  theme,
  onToggleTheme,
  autoSyncIntervalSec,
  onAutoSyncIntervalChange,
  totalConversations = 0,
  totalMessages = 0,
  appVersion = '0.2.2',
}) => {
  const [activeTab, setActiveTab] = useState<'ai' | 'storage' | 'backup' | 'api' | 'appearance' | 'about'>('ai');

  // API 服务状态与检测
  const [apiHealth, setApiHealth] = useState<{ status: string; latencyMs: number; ok: boolean; version?: string } | null>(null);
  const [checkingApi, setCheckingApi] = useState<boolean>(false);
  const [copiedEndpoint, setCopiedEndpoint] = useState<string | null>(null);
  const [showMarkdownPreview, setShowMarkdownPreview] = useState<boolean>(false);
  const [copiedFullDoc, setCopiedFullDoc] = useState<boolean>(false);

  const checkApiHealth = async () => {
    setCheckingApi(true);
    const start = performance.now();
    try {
      const res = await fetch('http://127.0.0.1:8788/health');
      const data = await res.json();
      const latency = Math.round(performance.now() - start);
      if (data && data.ok) {
        setApiHealth({ status: '服务正常运行中 (Active)', latencyMs: latency, ok: true, version: data.version });
      } else {
        setApiHealth({ status: '服务响应异常', latencyMs: latency, ok: false });
      }
    } catch {
      setApiHealth({ status: '连接失败 (服务未启动或端口被占用)', latencyMs: 0, ok: false });
    } finally {
      setCheckingApi(false);
    }
  };

  const handleCopyMarkdownDoc = () => {
    const md = getApiDocsMarkdown();
    navigator.clipboard.writeText(md);
    setCopiedFullDoc(true);
    setTimeout(() => setCopiedFullDoc(false), 2200);
  };

  useEffect(() => {
    if (isOpen && activeTab === 'api') {
      checkApiHealth();
    }
  }, [isOpen, activeTab]);

  const copyToClipboard = (text: string, id: string) => {
    navigator.clipboard.writeText(text);
    setCopiedEndpoint(id);
    setTimeout(() => setCopiedEndpoint(null), 2000);
  };

  // 主力与备用模型配置
  const [primaryProviderId, setPrimaryProviderId] = useState<string>(() => {
    return localStorage.getItem('agentdeck_primary_ai_provider') || 'bailian';
  });
  const [fallbackProviderId, setFallbackProviderId] = useState<string>(() => {
    return localStorage.getItem('agentdeck_fallback_ai_provider') || 'deepseek';
  });
  const [autoFallbackEnabled, setAutoFallbackEnabled] = useState<boolean>(() => {
    return localStorage.getItem('agentdeck_auto_fallback') !== 'false';
  });

  const [selectedProviderId, setSelectedProviderId] = useState<string>(() => {
    return localStorage.getItem('agentdeck_primary_ai_provider') || 'bailian';
  });

  const [dbPath, setDbPath] = useState<string>('~/.agentdeck/agentdeck.db');

  // 备份相关状态
  const [backupTargetPath, setBackupTargetPath] = useState<string>(() => {
    return localStorage.getItem('agentdeck_backup_target_path') || '~/Documents/AgentDeck_Backups';
  });
  const [autoBackupEnabled, setAutoBackupEnabled] = useState<boolean>(() => {
    return localStorage.getItem('agentdeck_auto_backup_enabled') !== 'false';
  });
  const [cloudPresets, setCloudPresets] = useState<CloudPreset[]>([]);
  const [backupList, setBackupList] = useState<BackupInfo[]>([]);
  const [loadingBackups, setLoadingBackups] = useState<boolean>(false);
  const [isBackingUp, setIsBackingUp] = useState<boolean>(false);
  const [backupFeedback, setBackupFeedback] = useState<{ success: boolean; msg: string } | null>(null);
  const [backupProgress, setBackupProgress] = useState<BackupProgress | null>(null);
  const [isRestoring, setIsRestoring] = useState<boolean>(false);
  const [confirmRestoreFile, setConfirmRestoreFile] = useState<string | null>(null);
  const [restoreFeedback, setRestoreFeedback] = useState<{ success: boolean; msg: string } | null>(null);

  const refreshBackups = async (targetPath: string) => {
    if (!targetPath) return;
    setLoadingBackups(true);
    try {
      const list = await api.listBackups(targetPath);
      setBackupList(list);
    } catch {
      setBackupList([]);
    } finally {
      setLoadingBackups(false);
    }
  };

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<BackupProgress>('backup-progress', (event) => {
      setBackupProgress(event.payload);
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {});

    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const persistBackupSettings = async (targetPath: string, autoEnabled: boolean) => {
    try {
      localStorage.setItem('agentdeck_backup_target_path', targetPath);
      localStorage.setItem('agentdeck_auto_backup_enabled', String(autoEnabled));
      const currentConfig = (await api.getAppConfig()) || {
        backup: { target_path: targetPath, auto_backup_enabled: autoEnabled, max_snapshots: 3 },
      };
      currentConfig.backup = {
        ...currentConfig.backup,
        target_path: targetPath,
        auto_backup_enabled: autoEnabled,
        max_snapshots: 3,
      };
      await api.saveAppConfig(currentConfig);
    } catch (e) {
      console.error('保存配置文件失败', e);
    }
  };

  useEffect(() => {
    if (isOpen) {
      api.getDatabasePathInfo().then((p) => {
        if (p) setDbPath(p);
      });

      api.getAppConfig().then((cfg) => {
        if (cfg && cfg.backup?.target_path) {
          setBackupTargetPath(cfg.backup.target_path);
          if (typeof cfg.backup.auto_backup_enabled === 'boolean') {
            setAutoBackupEnabled(cfg.backup.auto_backup_enabled);
          }
          refreshBackups(cfg.backup.target_path);
        } else {
          const savedPath = localStorage.getItem('agentdeck_backup_target_path');
          if (savedPath) {
            setBackupTargetPath(savedPath);
            refreshBackups(savedPath);
          }
        }
      });

      api.getCloudPresets().then((presets) => {
        setCloudPresets(presets);
        const currentSaved = localStorage.getItem('agentdeck_backup_target_path');
        if (!currentSaved) {
          const gdrive = presets.find((p) => p.id === 'gdrive' && p.available);
          const defaultPath = gdrive
            ? gdrive.path
            : (presets.find((p) => p.id === 'documents')?.path || '~/Documents/AgentDeck_Backups');
          setBackupTargetPath(defaultPath);
          persistBackupSettings(defaultPath, true);
          refreshBackups(defaultPath);
        }
      });
    }
  }, [isOpen]);

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

  // 主备双链路测试状态
  const [testingPipeline, setTestingPipeline] = useState(false);
  const [pipelineResult, setPipelineResult] = useState<{
    primary: { success: boolean; message: string; latency_ms: number };
    fallback?: { success: boolean; message: string; latency_ms: number };
    overall_success: boolean;
    message: string;
  } | null>(null);

  const activeProvider = AI_PROVIDERS.find((p) => p.id === selectedProviderId) || AI_PROVIDERS[0];
  const currentKey = apiKeys[activeProvider.id] || '';
  const currentBaseUrl = baseUrls[activeProvider.id] || activeProvider.baseUrl;
  const currentModel = customModels[activeProvider.id] || activeProvider.defaultModel;

  // 主力与备用 Provider 对象
  const primaryProvider = AI_PROVIDERS.find((p) => p.id === primaryProviderId) || AI_PROVIDERS[0];
  const fallbackProvider = AI_PROVIDERS.find((p) => p.id === fallbackProviderId) || AI_PROVIDERS[1];

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

  const handleSetAsPrimary = (id: string) => {
    setPrimaryProviderId(id);
    localStorage.setItem('agentdeck_primary_ai_provider', id);
    if (id === fallbackProviderId) {
      // 避免主备相同，自动为备用切换
      const alt = AI_PROVIDERS.find((p) => p.id !== id)?.id || 'custom';
      setFallbackProviderId(alt);
      localStorage.setItem('agentdeck_fallback_ai_provider', alt);
    }
  };

  const handleSetAsFallback = (id: string) => {
    setFallbackProviderId(id);
    localStorage.setItem('agentdeck_fallback_ai_provider', id);
    if (id === primaryProviderId) {
      // 避免主备相同，自动为主力切换
      const alt = AI_PROVIDERS.find((p) => p.id !== id)?.id || 'bailian';
      setPrimaryProviderId(alt);
      localStorage.setItem('agentdeck_primary_ai_provider', alt);
    }
  };

  const handleToggleAutoFallback = () => {
    const next = !autoFallbackEnabled;
    setAutoFallbackEnabled(next);
    localStorage.setItem('agentdeck_auto_fallback', String(next));
  };

  // 单个模型测试连接（通过 Rust 原生端点发起，彻底避免 WebKit/浏览器 CORS 拦截）
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

  // 一键测试主备链路连通性
  const handleTestPipeline = async () => {
    setTestingPipeline(true);
    setPipelineResult(null);

    const primaryCfg = {
      provider_name: primaryProvider.name,
      base_url: baseUrls[primaryProvider.id] || primaryProvider.baseUrl,
      api_key: apiKeys[primaryProvider.id] || '',
      model: customModels[primaryProvider.id] || primaryProvider.defaultModel,
    };

    const fallbackCfg = autoFallbackEnabled
      ? {
        provider_name: fallbackProvider.name,
        base_url: baseUrls[fallbackProvider.id] || fallbackProvider.baseUrl,
        api_key: apiKeys[fallbackProvider.id] || '',
        model: customModels[fallbackProvider.id] || fallbackProvider.defaultModel,
      }
      : undefined;

    try {
      const res = await api.testLlmPipeline(primaryCfg, fallbackCfg);
      setPipelineResult(res);
    } catch (e: any) {
      setPipelineResult({
        primary: { success: false, message: e.message || String(e), latency_ms: 0 },
        overall_success: false,
        message: '主备链路测试调用异常',
      });
    } finally {
      setTestingPipeline(false);
    }
  };

  const handleCreateBackup = async () => {
    if (!backupTargetPath.trim()) {
      setBackupFeedback({ success: false, msg: '请先填写或选择有效的备份存储路径' });
      return;
    }
    setIsBackingUp(true);
    setBackupFeedback(null);
    setBackupProgress({ stage: 'init', percent: 5, message: '正在准备备份...' });

    try {
      const res = await api.createBackup(backupTargetPath.trim(), 3);
      setBackupFeedback({
        success: true,
        msg: `备份成功！生成快照 ${res.file_name} (${res.file_size_formatted})，已保留最近 3 份。`,
      });
      localStorage.setItem('agentdeck_backup_target_path', backupTargetPath.trim());
      await refreshBackups(backupTargetPath.trim());
    } catch (err: any) {
      setBackupFeedback({
        success: false,
        msg: `备份失败: ${err?.message || err || '未知错误'}`,
      });
    } finally {
      setIsBackingUp(false);
      setTimeout(() => {
        setBackupProgress(null);
      }, 2500);
    }
  };

  const handleRestoreBackup = async (filePath: string) => {
    setIsRestoring(true);
    setRestoreFeedback(null);
    try {
      const res = await api.restoreBackup(filePath);
      setRestoreFeedback({
        success: true,
        msg: `${res.message} (已恢复 ${res.conversation_count} 个会话, ${res.media_file_count} 个媒体文件)`,
      });
      setConfirmRestoreFile(null);
    } catch (err: any) {
      setRestoreFeedback({
        success: false,
        msg: `还原失败: ${err?.message || err || '未知错误'}`,
      });
    } finally {
      setIsRestoring(false);
    }
  };

  const handleSelectFolder = async () => {
    try {
      const selected = await api.selectFolderDialog();
      if (selected) {
        setBackupTargetPath(selected);
        await persistBackupSettings(selected, autoBackupEnabled);
        refreshBackups(selected);
      }
    } catch (e) {
      console.error('选择目录失败', e);
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
        className="w-full max-w-4xl h-[620px] max-h-[85vh] min-h-[500px] theme-bg-main border theme-border rounded-2xl shadow-2xl flex flex-col overflow-hidden animate-in zoom-in-95 duration-150"
        onClick={(e) => e.stopPropagation()}
      >
        {/* 顶部标题栏 */}
        <div className="flex items-center justify-between px-6 py-4 border-b theme-border flex-shrink-0 theme-bg-header">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-xl bg-blue-600/10 text-blue-500 border border-blue-500/20">
              <Sliders className="h-5 w-5" />
            </div>
            <div>
              <h2 className="text-sm font-bold theme-text-main">应用设置 (Settings)</h2>
              <p className="text-xs theme-text-muted">配置 AI 模型高可用主备架构、多数据源增量同步与全局偏好</p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 rounded-lg hover:theme-bg-sub theme-text-muted hover:theme-text-main transition-colors cursor-pointer"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* 主体左右分栏 */}
        <div className="flex flex-1 overflow-hidden">
          {/* 左侧 Tab 切换 */}
          <div className="w-48 border-r theme-border p-3 space-y-1 flex-shrink-0 theme-bg-sub">
            <button
              onClick={() => setActiveTab('ai')}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-medium transition-all cursor-pointer ${activeTab === 'ai'
                  ? 'bg-blue-600 text-white shadow-xs'
                  : 'theme-text-muted hover:theme-text-main hover:theme-bg-card'
                }`}
            >
              <Cpu className="h-4 w-4" />
              <span>AI 供应商与主备</span>
            </button>

            <button
              onClick={() => setActiveTab('storage')}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-medium transition-all cursor-pointer ${activeTab === 'storage'
                  ? 'bg-blue-600 text-white shadow-xs'
                  : 'theme-text-muted hover:theme-text-main hover:theme-bg-card'
                }`}
            >
              <Database className="h-4 w-4" />
              <span>数据存储与源</span>
            </button>

            <button
              onClick={() => setActiveTab('backup')}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-medium transition-all cursor-pointer ${activeTab === 'backup'
                  ? 'bg-blue-600 text-white shadow-xs'
                  : 'theme-text-muted hover:theme-text-main hover:theme-bg-card'
                }`}
            >
              <Archive className="h-4 w-4" />
              <span>数据备份与恢复</span>
            </button>

            <button
              onClick={() => setActiveTab('api')}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-medium transition-all cursor-pointer ${activeTab === 'api'
                  ? 'bg-blue-600 text-white shadow-xs'
                  : 'theme-text-muted hover:theme-text-main hover:theme-bg-card'
                }`}
            >
              <Server className="h-4 w-4" />
              <span>REST API 服务</span>
            </button>

            <button
              onClick={() => setActiveTab('appearance')}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-medium transition-all cursor-pointer ${activeTab === 'appearance'
                  ? 'bg-blue-600 text-white shadow-xs'
                  : 'theme-text-muted hover:theme-text-main hover:theme-bg-card'
                }`}
            >
              <Sun className="h-4 w-4" />
              <span>外观与主题</span>
            </button>

            <button
              onClick={() => setActiveTab('about')}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-medium transition-all cursor-pointer ${activeTab === 'about'
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
                {/* 顶部高可用主备架构说明卡片 */}
                <div className="border border-blue-500/30 bg-blue-500/5 rounded-2xl p-4 space-y-3">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <Sparkles className="h-4 w-4 text-blue-500" />
                      <span className="font-bold text-xs theme-text-main">高可用主备模型架构 (HA Dual-LLM Pipeline)</span>
                    </div>
                    <label className="flex items-center gap-2 text-xs cursor-pointer select-none">
                      <span className="theme-text-muted text-[11px]">自动故障降级 (Failover)</span>
                      <input
                        type="checkbox"
                        checked={autoFallbackEnabled}
                        onChange={handleToggleAutoFallback}
                        className="rounded border-slate-600 text-blue-600 focus:ring-0 cursor-pointer"
                      />
                    </label>
                  </div>

                  <div className="grid grid-cols-2 gap-3 text-xs">
                    {/* 主力模型指示 */}
                    <div className="p-2.5 rounded-xl border theme-border theme-bg-card flex items-center justify-between">
                      <div className="flex items-center gap-2">
                        <span className="px-1.5 py-0.5 text-[10px] font-bold bg-blue-600 text-white rounded">
                          主力 Primary
                        </span>
                        <span className="font-medium theme-text-main truncate">{primaryProvider.name}</span>
                      </div>
                      <span className="text-[10px] font-mono theme-text-sub">
                        {customModels[primaryProvider.id] || primaryProvider.defaultModel}
                      </span>
                    </div>

                    {/* 备用模型指示 */}
                    <div className={`p-2.5 rounded-xl border theme-border theme-bg-card flex items-center justify-between ${!autoFallbackEnabled ? 'opacity-50' : ''}`}>
                      <div className="flex items-center gap-2">
                        <span className="px-1.5 py-0.5 text-[10px] font-bold bg-purple-600 text-white rounded">
                          备用 Fallback
                        </span>
                        <span className="font-medium theme-text-main truncate">{fallbackProvider.name}</span>
                      </div>
                      <span className="text-[10px] font-mono theme-text-sub">
                        {customModels[fallbackProvider.id] || fallbackProvider.defaultModel}
                      </span>
                    </div>
                  </div>

                  <div className="flex items-center justify-between pt-1 text-[11px] theme-text-muted">
                    <span>当主力模型遇到超时、限流 (429) 或服务器异常时，系统将无缝自动切换至备用模型。</span>
                    <button
                      onClick={handleTestPipeline}
                      disabled={testingPipeline}
                      className="flex items-center gap-1 text-blue-500 hover:underline font-medium cursor-pointer flex-shrink-0"
                    >
                      {testingPipeline ? <Loader2 className="h-3 w-3 animate-spin" /> : <RefreshCw className="h-3 w-3" />}
                      <span>一键测试主备链路</span>
                    </button>
                  </div>

                  {/* 主备链路测试结果反馈 */}
                  {pipelineResult && (
                    <div className={`p-3 rounded-xl border text-xs space-y-1.5 ${pipelineResult.overall_success
                        ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-600 dark:text-emerald-400'
                        : 'bg-red-500/10 border-red-500/30 text-red-600 dark:text-red-400'
                      }`}>
                      <div className="flex items-center gap-1.5 font-bold">
                        {pipelineResult.overall_success ? <CheckCircle2 className="h-4 w-4" /> : <AlertCircle className="h-4 w-4" />}
                        <span>{pipelineResult.message}</span>
                      </div>
                      <div className="grid grid-cols-2 gap-2 text-[11px] font-mono opacity-90">
                        <div>
                          主力 [{primaryProvider.name}]: {pipelineResult.primary.success ? `✅ 正常 (${pipelineResult.primary.latency_ms}ms)` : `❌ 失败 (${pipelineResult.primary.message})`}
                        </div>
                        {pipelineResult.fallback && (
                          <div>
                            备用 [{fallbackProvider.name}]: {pipelineResult.fallback.success ? `✅ 正常 (${pipelineResult.fallback.latency_ms}ms)` : `❌ 失败 (${pipelineResult.fallback.message})`}
                          </div>
                        )}
                      </div>
                    </div>
                  )}
                </div>

                <div>
                  <h3 className="text-sm font-bold theme-text-main">AI 供应商列表</h3>
                  <p className="text-xs theme-text-muted mt-0.5">
                    点击供应商卡片可配置 API 密钥并指定为主力或备用模型。
                  </p>
                </div>

                {/* 供应商选择网格 */}
                <div className="grid grid-cols-2 sm:grid-cols-3 gap-2">
                  {AI_PROVIDERS.map((provider) => {
                    const isEditing = provider.id === selectedProviderId;
                    const isPrimary = provider.id === primaryProviderId;
                    const isFallback = provider.id === fallbackProviderId;
                    const hasKey = Boolean(apiKeys[provider.id]);

                    return (
                      <button
                        key={provider.id}
                        onClick={() => {
                          setSelectedProviderId(provider.id);
                          setTestResult(null);
                        }}
                        className={`flex flex-col p-2.5 rounded-xl border text-xs font-medium transition-all cursor-pointer relative ${isEditing
                            ? 'bg-blue-600/15 border-blue-500/50 theme-text-main shadow-xs'
                            : 'theme-bg-sub theme-border hover:theme-border-hover theme-text-muted hover:theme-text-main'
                          }`}
                      >
                        <div className="flex items-center justify-between w-full mb-1">
                          <div className="flex items-center gap-1.5">
                            {renderProviderIcon(provider.iconName)}
                            <span className="truncate font-semibold">{provider.name}</span>
                          </div>
                          {hasKey && (
                            <span className="w-1.5 h-1.5 rounded-full bg-emerald-500 flex-shrink-0" title="已配置 API Key" />
                          )}
                        </div>

                        <div className="flex items-center gap-1 mt-1">
                          {isPrimary && (
                            <span className="px-1.5 py-0.2 text-[9px] font-bold bg-blue-600 text-white rounded">
                              主力模型
                            </span>
                          )}
                          {isFallback && (
                            <span className="px-1.5 py-0.2 text-[9px] font-bold bg-purple-600 text-white rounded">
                              备用模型
                            </span>
                          )}
                          {!isPrimary && !isFallback && (
                            <span className="text-[10px] theme-text-sub font-mono truncate">
                              {customModels[provider.id] || provider.defaultModel}
                            </span>
                          )}
                        </div>
                      </button>
                    );
                  })}
                </div>

                {/* 当前供应商配置详情卡片 */}
                <div className="theme-bg-sub border theme-border rounded-xl p-4 space-y-4">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      {renderProviderIcon(activeProvider.iconName)}
                      <span className="font-bold text-xs theme-text-main">{activeProvider.name} 配置详情</span>
                    </div>

                    <div className="flex items-center gap-2">
                      {activeProvider.id !== primaryProviderId && (
                        <button
                          onClick={() => handleSetAsPrimary(activeProvider.id)}
                          className="px-2 py-1 text-[11px] rounded-lg border theme-border bg-blue-600/10 text-blue-500 hover:bg-blue-600 hover:text-white transition-all cursor-pointer"
                        >
                          设为主力模型
                        </button>
                      )}
                      {activeProvider.id !== fallbackProviderId && (
                        <button
                          onClick={() => handleSetAsFallback(activeProvider.id)}
                          className="px-2 py-1 text-[11px] rounded-lg border theme-border bg-purple-600/10 text-purple-500 hover:bg-purple-600 hover:text-white transition-all cursor-pointer"
                        >
                          设为备用模型
                        </button>
                      )}
                      {activeProvider.apiKeyLink && (
                        <a
                          href={activeProvider.apiKeyLink}
                          target="_blank"
                          rel="noreferrer"
                          className="text-[11px] text-blue-500 hover:underline flex items-center gap-1 ml-1"
                        >
                          <span>获取 Key</span>
                          <ExternalLink className="h-3 w-3" />
                        </a>
                      )}
                    </div>
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
                        <CustomSelect<string>
                          value={activeProvider.models.includes(currentModel) ? currentModel : ''}
                          onChange={(val) => {
                            if (val) handleModelChange(val);
                          }}
                          placeholder="常用预设..."
                          options={activeProvider.models.map((m) => ({
                            value: m,
                            label: m,
                          }))}
                          className="min-w-[140px]"
                        />
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
                      <span>测试当前端点连通性</span>
                    </button>

                    {testResult && (
                      <div
                        className={`text-xs flex items-center gap-1.5 ${testResult.success ? 'text-emerald-500 font-medium' : 'text-red-500 font-medium'
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

                  <div className="text-xs font-mono theme-text-muted bg-black/10 dark:bg-black/40 p-2.5 rounded-lg border theme-border break-all select-all">
                    {dbPath}
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

                <div className="theme-bg-sub border theme-border rounded-xl p-4 space-y-3">
                  <div>
                    <div className="text-xs font-semibold theme-text-main">自动同步频率</div>
                    <p className="text-[11px] theme-text-muted mt-1">
                      后台监听检测到源文件变化后，会按这里的频率轮询检查。无变化时不会弹通知。
                    </p>
                  </div>

                  <div className="flex items-center gap-3">
                    <CustomSelect<number>
                      value={autoSyncIntervalSec}
                      onChange={(val) => onAutoSyncIntervalChange(val)}
                      options={[
                        { value: 30, label: '30 秒', subLabel: '高频探测' },
                        { value: 60, label: '60 秒', subLabel: '推荐默认' },
                        { value: 120, label: '120 秒', subLabel: '省电模式' },
                        { value: 300, label: '300 秒', subLabel: '低频模式' },
                      ]}
                      className="min-w-[130px]"
                    />
                    <span className="text-[11px] theme-text-sub">
                      默认 60 秒，建议在频繁切换 Agent 时保持 60 秒或更高。
                    </span>
                  </div>
                </div>
              </div>
            )}

            {activeTab === 'backup' && (
              <div className="space-y-5">
                <div>
                  <h3 className="text-sm font-bold theme-text-main">数据备份与灾备恢复 (Backup & Disaster Recovery)</h3>
                  <p className="text-xs theme-text-muted mt-0.5">
                    自动将 SQLite 会话库（热快照）与图片媒体资产打包为压缩包（.tar.gz），支持定期自动备份与跨端迁移。
                  </p>
                </div>

                {/* 备份存储目录配置 */}
                <div className="theme-bg-sub border theme-border rounded-xl p-4 space-y-3">
                  <div className="flex items-center justify-between">
                    <span className="text-xs font-semibold theme-text-main">备份存储目标路径</span>
                    <span className="text-[11px] theme-text-muted">支持本地、Google Drive、iCloud 或 NAS 挂载路径</span>
                  </div>

                  <div className="flex items-center gap-2">
                    <input
                      type="text"
                      value={backupTargetPath}
                      onChange={(e) => {
                        const val = e.target.value;
                        setBackupTargetPath(val);
                        persistBackupSettings(val, autoBackupEnabled);
                      }}
                      placeholder="~/Documents/AgentDeck_Backups 或云盘挂载路径..."
                      className="flex-1 text-xs font-mono theme-bg-card border theme-border rounded-lg px-3 py-2 theme-text-main focus:outline-none focus:border-blue-500"
                    />
                    <button
                      onClick={handleSelectFolder}
                      className="px-3 py-2 text-xs font-medium bg-blue-600/10 hover:bg-blue-600/20 text-blue-500 border border-blue-500/30 rounded-lg transition-all cursor-pointer flex items-center gap-1.5 shrink-0 shadow-xs"
                      title="打开系统原生文件夹选择器"
                    >
                      <Folder className="h-3.5 w-3.5" />
                      <span>浏览...</span>
                    </button>
                    <button
                      onClick={() => refreshBackups(backupTargetPath)}
                      className="px-3 py-2 text-xs font-medium theme-bg-card hover:theme-bg-sub border theme-border theme-text-main rounded-lg transition-all cursor-pointer flex items-center gap-1.5 shrink-0"
                      title="刷新快照列表"
                    >
                      <RefreshCw className={`h-3.5 w-3.5 ${loadingBackups ? 'animate-spin' : ''}`} />
                      <span>刷新</span>
                    </button>
                  </div>

                  {/* 快捷预设胶囊按钮 */}
                  <div className="pt-1">
                    <div className="text-[11px] theme-text-muted mb-1.5 flex items-center gap-1">
                      <span>常用预设:</span>
                    </div>
                    <div className="flex flex-wrap gap-2">
                      {cloudPresets.map((preset) => (
                        <button
                          key={preset.id}
                          onClick={() => {
                            setBackupTargetPath(preset.path);
                            persistBackupSettings(preset.path, autoBackupEnabled);
                            refreshBackups(preset.path);
                          }}
                          className={`flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs border transition-all cursor-pointer ${
                            backupTargetPath === preset.path
                              ? 'bg-blue-600/15 border-blue-500 text-blue-500 font-medium'
                              : 'theme-bg-card border-theme-border theme-text-muted hover:theme-text-main hover:border-blue-500/50'
                          }`}
                        >
                          {preset.id === 'gdrive' && <Cloud className="h-3.5 w-3.5 text-blue-400" />}
                          {preset.id === 'icloud' && <CloudRain className="h-3.5 w-3.5 text-indigo-400" />}
                          {preset.id === 'documents' && <Folder className="h-3.5 w-3.5 text-amber-400" />}
                          <span>{preset.name}</span>
                          {preset.available && (
                            <span className="w-1.5 h-1.5 rounded-full bg-emerald-500" title="已检测到该应用" />
                          )}
                        </button>
                      ))}
                    </div>
                  </div>
                </div>

                {/* 备份策略与操作区 */}
                <div className="theme-bg-sub border theme-border rounded-xl p-4 space-y-4">
                  <div className="flex items-center justify-between">
                    <div>
                      <div className="text-xs font-semibold theme-text-main">每日自动备份</div>
                      <p className="text-[11px] theme-text-muted mt-0.5">
                        在后台静默执行热快照并压缩归档，自动保留最新的 3 份历史备份（超期旧备份自动修剪）。
                      </p>
                    </div>
                    <label className="relative inline-flex items-center cursor-pointer">
                      <input
                        type="checkbox"
                        checked={autoBackupEnabled}
                        onChange={(e) => {
                          const checked = e.target.checked;
                          setAutoBackupEnabled(checked);
                          persistBackupSettings(backupTargetPath, checked);
                        }}
                        className="sr-only peer"
                      />
                      <div className="w-9 h-5 bg-neutral-700 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-neutral-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-blue-600"></div>
                    </label>
                  </div>

                  <div className="pt-3 border-t theme-border-sub flex items-center justify-between">
                    <button
                      onClick={handleCreateBackup}
                      disabled={isBackingUp || !backupTargetPath}
                      className="flex items-center gap-1.5 px-4 py-2 text-xs font-medium bg-blue-600 hover:bg-blue-500 disabled:opacity-50 text-white rounded-lg transition-all cursor-pointer shadow-xs"
                    >
                      {isBackingUp ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Archive className="h-3.5 w-3.5" />}
                      <span>{isBackingUp ? '正在执行备份...' : '立即执行备份 (Backup Now)'}</span>
                    </button>

                    {backupFeedback && !isBackingUp && (
                      <div
                        className={`text-xs flex items-center gap-1.5 max-w-md ${
                          backupFeedback.success ? 'text-emerald-500 font-medium' : 'text-red-500 font-medium'
                        }`}
                      >
                        {backupFeedback.success ? <CheckCircle2 className="h-4 w-4 shrink-0" /> : <AlertCircle className="h-4 w-4 shrink-0" />}
                        <span className="truncate">{backupFeedback.msg}</span>
                      </div>
                    )}
                  </div>

                  {/* 进度条与实时状态展示 */}
                  {isBackingUp && backupProgress && (
                    <div className="space-y-1.5 pt-1">
                      <div className="flex items-center justify-between text-xs">
                        <span className="theme-text-main font-medium flex items-center gap-1.5">
                          <Loader2 className="h-3.5 w-3.5 animate-spin text-blue-500 shrink-0" />
                          <span>{backupProgress.message}</span>
                        </span>
                        <span className="font-mono font-bold text-blue-500">{backupProgress.percent}%</span>
                      </div>
                      <div className="w-full h-1.5 bg-black/20 dark:bg-white/10 rounded-full overflow-hidden">
                        <div
                          className="h-full bg-gradient-to-r from-blue-600 via-indigo-500 to-cyan-400 rounded-full transition-all duration-300 shadow-[0_0_8px_rgba(59,130,246,0.6)]"
                          style={{ width: `${backupProgress.percent}%` }}
                        />
                      </div>
                    </div>
                  )}
                </div>

                {/* 历史快照列表 (Top 3) */}
                <div className="theme-bg-sub border theme-border rounded-xl p-4 space-y-3">
                  <div className="flex items-center justify-between">
                    <span className="text-xs font-semibold theme-text-main flex items-center gap-1.5">
                      <FileArchive className="h-4 w-4 text-blue-500" />
                      <span>现有快照备份 (最近 {backupList.length} / 3 份)</span>
                    </span>
                    <span className="text-[11px] theme-text-muted">单文件自包含归档包</span>
                  </div>

                  {restoreFeedback && (
                    <div
                      className={`p-3 rounded-lg text-xs flex items-center gap-2 border ${
                        restoreFeedback.success
                          ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-500'
                          : 'bg-red-500/10 border-red-500/30 text-red-500'
                      }`}
                    >
                      {restoreFeedback.success ? <CheckCircle2 className="h-4 w-4 shrink-0" /> : <AlertCircle className="h-4 w-4 shrink-0" />}
                      <span>{restoreFeedback.msg}</span>
                    </div>
                  )}

                  {backupList.length === 0 ? (
                    <div className="text-center py-6 text-xs theme-text-muted border border-dashed theme-border rounded-lg">
                      暂无备份文件，点击上方「立即执行备份」生成首份快照。
                    </div>
                  ) : (
                    <div className="space-y-2">
                      {backupList.map((item, index) => (
                        <div
                          key={item.file_name}
                          className="p-3 rounded-lg theme-bg-card border theme-border flex items-center justify-between hover:theme-border-hover transition-all"
                        >
                          <div className="space-y-1">
                            <div className="flex items-center gap-2">
                              <span className="px-1.5 py-0.5 rounded text-[10px] font-mono bg-blue-500/10 text-blue-400 border border-blue-500/20">
                                快照 #{index + 1}
                              </span>
                              <span className="text-xs font-mono font-medium theme-text-main">{item.file_name}</span>
                            </div>
                            <div className="text-[11px] theme-text-muted flex items-center gap-3">
                              <span>大小: {item.file_size_formatted}</span>
                              <span>时间: {item.created_at ? new Date(item.created_at).toLocaleString() : '未知'}</span>
                            </div>
                          </div>

                          <div>
                            {confirmRestoreFile === item.file_path ? (
                              <div className="flex items-center gap-2">
                                <span className="text-[11px] text-amber-400 font-medium">确认还原覆盖当前库?</span>
                                <button
                                  onClick={() => handleRestoreBackup(item.file_path)}
                                  disabled={isRestoring}
                                  className="px-2.5 py-1 text-xs bg-amber-600 hover:bg-amber-500 text-white rounded-md cursor-pointer flex items-center gap-1"
                                >
                                  {isRestoring ? <Loader2 className="h-3 w-3 animate-spin" /> : <Check className="h-3 w-3" />}
                                  <span>确认</span>
                                </button>
                                <button
                                  onClick={() => setConfirmRestoreFile(null)}
                                  disabled={isRestoring}
                                  className="px-2 py-1 text-xs theme-bg-sub theme-text-muted hover:theme-text-main rounded-md cursor-pointer"
                                >
                                  取消
                                </button>
                              </div>
                            ) : (
                              <button
                                onClick={() => {
                                  setConfirmRestoreFile(item.file_path);
                                  setRestoreFeedback(null);
                                }}
                                disabled={isRestoring}
                                className="px-3 py-1.5 text-xs font-medium theme-bg-sub hover:bg-blue-600 hover:text-white border theme-border hover:border-blue-600 rounded-lg theme-text-main transition-all cursor-pointer flex items-center gap-1.5 shadow-xs"
                              >
                                <RotateCcw className="h-3.5 w-3.5" />
                                <span>从此快照还原</span>
                              </button>
                            )}
                          </div>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              </div>
            )}

            {activeTab === 'api' && (
              <div className="space-y-5">
                <div>
                  <h3 className="text-sm font-bold theme-text-main">REST API 服务与集成 (API Services & Integration)</h3>
                  <p className="text-xs theme-text-muted mt-0.5">
                    嵌入式轻量级 HTTP 服务，提供标准 RESTful 接口用于外部脚本、Raycast、Alfred、CLI 终端与第三方大盘接入。
                  </p>
                </div>

                {/* 鉴权与网络安全提示横幅 */}
                <div className="border border-blue-500/30 bg-blue-500/10 rounded-xl p-3.5 flex items-start justify-between gap-3">
                  <div className="flex items-start gap-2.5">
                    <div className="p-1 rounded-md bg-blue-500/20 text-blue-400 mt-0.5">
                      <Lock className="h-4 w-4" />
                    </div>
                    <div className="text-xs space-y-0.5">
                      <div className="font-bold text-blue-400">免 Token 鉴权说明 (No Token Required)</div>
                      <p className="theme-text-muted text-[11px] leading-relaxed">
                        当前 API 严格监听在当前 Mac 本机回环网卡 <code className="font-mono text-blue-300">127.0.0.1:8788</code>，仅供本机进程、脚本或插件直接调用，外部网络无法直连。因此无需在请求头传递 Authorization Token。
                      </p>
                    </div>
                  </div>
                </div>

                {/* 服务运行状态卡片 */}
                <div className="theme-bg-sub border theme-border rounded-xl p-4 space-y-3">
                  <div className="flex items-center justify-between">
                    <span className="text-xs font-semibold theme-text-main">服务运行状态</span>
                    <div className="flex items-center gap-2">
                      <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-emerald-500/10 text-emerald-500 border border-emerald-500/20">
                        <span className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse" />
                        <span>{apiHealth?.ok ? '服务运行中 (Active)' : '服务已就绪 (127.0.0.1:8788)'}</span>
                      </span>
                    </div>
                  </div>

                  <div className="flex flex-wrap items-center justify-between gap-3 pt-1">
                    <div className="flex items-center gap-2 flex-wrap">
                      <div className="px-3 py-1.5 rounded-lg bg-black/20 dark:bg-white/5 border theme-border font-mono text-xs text-blue-400 font-semibold select-all">
                        http://127.0.0.1:8788
                      </div>
                      <button
                        onClick={() => copyToClipboard('http://127.0.0.1:8788', 'base-url')}
                        className="px-2.5 py-1.5 text-xs theme-bg-card hover:theme-bg-sub border theme-border theme-text-muted hover:theme-text-main rounded-lg transition-all cursor-pointer flex items-center gap-1"
                        title="复制 Base URL"
                      >
                        {copiedEndpoint === 'base-url' ? <Check className="h-3.5 w-3.5 text-emerald-500" /> : <Copy className="h-3.5 w-3.5" />}
                        <span>{copiedEndpoint === 'base-url' ? '已复制' : '复制'}</span>
                      </button>

                      <button
                        onClick={checkApiHealth}
                        disabled={checkingApi}
                        className="px-2.5 py-1.5 text-xs theme-bg-card hover:theme-bg-sub border theme-border theme-text-muted hover:theme-text-main rounded-lg transition-all cursor-pointer flex items-center gap-1"
                        title="测试接口连通性"
                      >
                        <RefreshCw className={`h-3.5 w-3.5 ${checkingApi ? 'animate-spin' : ''}`} />
                        <span>{checkingApi ? '测试中...' : '测试连通性'}</span>
                      </button>

                      {apiHealth && (
                        <span className="text-[11px] font-mono text-emerald-400">
                          ({apiHealth.latencyMs}ms)
                        </span>
                      )}
                    </div>

                    {/* 文档操作按钮组 */}
                    <div className="flex items-center gap-2 flex-wrap">
                      <button
                        onClick={handleCopyMarkdownDoc}
                        className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium theme-bg-card hover:theme-bg-sub border theme-border theme-text-main rounded-lg transition-all cursor-pointer shadow-xs"
                        title="一键复制完整 Markdown 格式 API 规范文档"
                      >
                        {copiedFullDoc ? <Check className="h-3.5 w-3.5 text-emerald-500" /> : <FileText className="h-3.5 w-3.5 text-blue-400" />}
                        <span>{copiedFullDoc ? '已复制 Markdown！' : '复制 Markdown 文档'}</span>
                      </button>

                      <button
                        onClick={() => setShowMarkdownPreview(!showMarkdownPreview)}
                        className={`flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg border transition-all cursor-pointer shadow-xs ${
                          showMarkdownPreview
                            ? 'bg-blue-600/20 border-blue-500 text-blue-400'
                            : 'theme-bg-card hover:theme-bg-sub border-theme-border theme-text-muted hover:theme-text-main'
                        }`}
                        title="在当前弹窗中直接展开预览 Markdown 文档"
                      >
                        <Terminal className="h-3.5 w-3.5" />
                        <span>{showMarkdownPreview ? '收起预览' : '预览 Markdown'}</span>
                      </button>

                      <button
                        onClick={() => api.openUrl('http://127.0.0.1:8788/docs')}
                        className="flex items-center gap-1.5 px-3.5 py-1.5 text-xs font-semibold bg-blue-600 hover:bg-blue-500 text-white rounded-lg transition-all cursor-pointer shadow-xs"
                        title="在系统默认浏览器中打开交互式 API 文档"
                      >
                        <ExternalLink className="h-3.5 w-3.5" />
                        <span>在线文档 (Docs)</span>
                      </button>
                    </div>
                  </div>

                  {/* 展开 Markdown 预览区 */}
                  {showMarkdownPreview && (
                    <div className="pt-3 border-t theme-border-sub space-y-2">
                      <div className="flex items-center justify-between text-xs">
                        <span className="font-semibold theme-text-main">Markdown 文档内容预览</span>
                        <button
                          onClick={handleCopyMarkdownDoc}
                          className="text-[11px] text-blue-400 hover:underline flex items-center gap-1 cursor-pointer"
                        >
                          <Copy className="h-3 w-3" />
                          <span>一键全选复制</span>
                        </button>
                      </div>
                      <pre className="p-3 rounded-lg bg-black/40 dark:bg-black/70 border theme-border font-mono text-[11px] text-neutral-300 max-h-60 overflow-y-auto whitespace-pre-wrap select-all">
                        {getApiDocsMarkdown()}
                      </pre>
                    </div>
                  )}
                </div>

                {/* 开放接口清单 */}
                <div className="theme-bg-sub border theme-border rounded-xl p-4 space-y-3">
                  <div className="flex items-center justify-between">
                    <span className="text-xs font-semibold theme-text-main">开放接口概览</span>
                    <span className="text-[11px] theme-text-muted">点击端点或右侧按钮可直接复制 cURL 调用命令</span>
                  </div>

                  <div className="space-y-2 text-xs">
                    {[
                      { method: 'GET', path: '/health', desc: '服务健康度与数据源可用性探测', curl: 'curl http://127.0.0.1:8788/health' },
                      { method: 'GET', path: '/api/stats', desc: '大盘总览指标、总消息数与多智能体分布', curl: 'curl http://127.0.0.1:8788/api/stats' },
                      { method: 'GET', path: '/api/workspaces', desc: '项目工作区列表与消息统计 (支持 ?q= 搜索)', curl: 'curl http://127.0.0.1:8788/api/workspaces' },
                      { method: 'GET', path: '/api/workspaces/detail', desc: '指定工作区 365 天研发热力图与打卡明细', curl: 'curl "http://127.0.0.1:8788/api/workspaces/detail?workspace=YOUR_PATH"' },
                      { method: 'GET', path: '/api/conversations', desc: '智能体会话历史 (支持 ?source= &limit=20)', curl: 'curl "http://127.0.0.1:8788/api/conversations?limit=20"' },
                      { method: 'GET', path: '/api/conversations/:id', desc: '指定会话完整消息流、思考链与附图资源', curl: 'curl http://127.0.0.1:8788/api/conversations/CONV_ID' },
                      { method: 'GET', path: '/api/search?q=...', desc: '全局全文搜索与代码关键词定位', curl: 'curl "http://127.0.0.1:8788/api/search?q=hello"' },
                      { method: 'POST', path: '/api/sync', desc: '触发全量 AI 智能体数据源增量扫描同步', curl: 'curl -X POST http://127.0.0.1:8788/api/sync' },
                      { method: 'GET', path: '/media/:source/:id/:file', desc: '本地持久化媒体图片静态分发', curl: 'curl -I http://127.0.0.1:8788/media/antigravity/ID/pic.png' },
                    ].map((item, idx) => (
                      <div
                        key={idx}
                        className="p-2.5 rounded-lg theme-bg-card border theme-border flex items-center justify-between hover:theme-border-hover transition-all"
                      >
                        <div className="flex items-center gap-2.5 flex-1 min-w-0">
                          <span
                            className={`px-1.5 py-0.5 rounded text-[10px] font-mono font-bold shrink-0 ${
                              item.method === 'GET'
                                ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20'
                                : 'bg-indigo-500/10 text-indigo-400 border border-indigo-500/20'
                            }`}
                          >
                            {item.method}
                          </span>
                          <span className="font-mono font-medium theme-text-main text-xs">{item.path}</span>
                          <span className="text-[11px] theme-text-muted truncate hidden sm:inline">{item.desc}</span>
                        </div>

                        <button
                          onClick={() => copyToClipboard(item.curl, `endpoint-${idx}`)}
                          className="px-2 py-1 text-[11px] theme-bg-sub hover:theme-bg-main border theme-border theme-text-muted hover:theme-text-main rounded-md transition-all cursor-pointer flex items-center gap-1 shrink-0 ml-2"
                          title="复制 cURL 命令"
                        >
                          {copiedEndpoint === `endpoint-${idx}` ? (
                            <Check className="h-3 w-3 text-emerald-500" />
                          ) : (
                            <Copy className="h-3 w-3" />
                          )}
                          <span>{copiedEndpoint === `endpoint-${idx}` ? '已复制' : '复制 cURL'}</span>
                        </button>
                      </div>
                    ))}
                  </div>
                </div>

                {/* 快速调用与脚本集成卡片 */}
                <div className="border border-blue-500/20 bg-blue-500/5 rounded-xl p-4 space-y-2 text-xs">
                  <div className="flex items-center gap-2 font-semibold theme-text-main">
                    <Terminal className="h-4 w-4 text-blue-500" />
                    <span>快速调用示例 (Quick Start)</span>
                  </div>
                  <div className="p-2.5 rounded-lg bg-black/40 dark:bg-black/60 font-mono text-[11px] text-blue-300 overflow-x-auto select-all border border-white/5">
                    curl http://127.0.0.1:8788/api/stats | jq .
                  </div>
                  <p className="text-[11px] theme-text-muted">
                    支持在 Raycast、Alfred、CLI 脚本、自动化管线或局域网 HomeAssistant 等场景中无缝获取会话资产。
                  </p>
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
                    className={`p-4 rounded-xl border text-xs cursor-pointer transition-all flex flex-col items-center gap-2 ${theme === 'dark'
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
                    className={`p-4 rounded-xl border text-xs cursor-pointer transition-all flex flex-col items-center gap-2 ${theme === 'light'
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
                    <span className="font-mono font-medium theme-text-main">v{appVersion}</span>
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
