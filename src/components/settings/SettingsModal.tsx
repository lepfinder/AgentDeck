import React, { useState, useEffect, useRef } from 'react';
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
  Lock,
} from 'lucide-react';
import { CustomSelect } from '../common/CustomSelect';
import { useI18n } from '../../i18n';
import type { CloudPreset, BackupInfo, BackupProgress } from '../../types';
import { AI_PROVIDERS } from '../../config/aiProviders';

export type { AiProviderConfig } from '../../config/aiProviders';
export { AI_PROVIDERS } from '../../config/aiProviders';

interface Props {
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
  onClose,
  theme,
  onToggleTheme,
  autoSyncIntervalSec,
  onAutoSyncIntervalChange,
  totalConversations = 0,
  totalMessages = 0,
  appVersion = 'unknown',
}) => {
  const { t, locale, setLocale } = useI18n();
  const [activeTab, setActiveTab] = useState<'ai' | 'storage' | 'backup' | 'api' | 'appearance' | 'about'>('appearance');

  // API 服务状态与检测
  const [apiHealth, setApiHealth] = useState<{ status: string; latencyMs: number; ok: boolean; version?: string } | null>(null);
  const [copiedEndpoint, setCopiedEndpoint] = useState<string | null>(null);

  const checkApiHealth = async () => {
    const start = performance.now();
    try {
      const res = await fetch('http://127.0.0.1:8788/health');
      const data = await res.json();
      const latency = Math.round(performance.now() - start);
      if (data && data.ok) {
        setApiHealth({ status: t('settings.apiOk'), latencyMs: latency, ok: true, version: data.version });
      } else {
        setApiHealth({ status: t('settings.apiBad'), latencyMs: latency, ok: false });
      }
    } catch {
      setApiHealth({ status: t('settings.apiFail'), latencyMs: 0, ok: false });
    }
  };

  useEffect(() => {
    if (activeTab === 'api') {
      checkApiHealth();
    }
  }, [activeTab]);

  const storageBootstrapped = useRef(false);
  const backupBootstrapped = useRef(false);

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
    if (activeTab !== 'storage' || storageBootstrapped.current) return;
    storageBootstrapped.current = true;
    api.getDatabasePathInfo().then((p) => {
      if (p) setDbPath(p);
    });
  }, [activeTab]);

  useEffect(() => {
    if (activeTab !== 'backup' || backupBootstrapped.current) return;
    backupBootstrapped.current = true;

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
  }, [activeTab]);

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

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
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
              <h2 className="text-sm font-bold theme-text-main">{t('settings.title')}</h2>
              <p className="text-xs theme-text-muted">{t('settings.subtitle')}</p>
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
              onClick={() => setActiveTab('appearance')}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-medium transition-all cursor-pointer ${activeTab === 'appearance'
                  ? 'bg-blue-600 text-white shadow-xs'
                  : 'theme-text-muted hover:theme-text-main hover:theme-bg-card'
                }`}
            >
              <Sun className="h-4 w-4" />
              <span>{t('settings.tabAppearance')}</span>
            </button>

            <button
              onClick={() => setActiveTab('ai')}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-medium transition-all cursor-pointer ${activeTab === 'ai'
                  ? 'bg-blue-600 text-white shadow-xs'
                  : 'theme-text-muted hover:theme-text-main hover:theme-bg-card'
                }`}
            >
              <Cpu className="h-4 w-4" />
              <span>{t('settings.tabAi')}</span>
            </button>

            <button
              onClick={() => setActiveTab('storage')}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-medium transition-all cursor-pointer ${activeTab === 'storage'
                  ? 'bg-blue-600 text-white shadow-xs'
                  : 'theme-text-muted hover:theme-text-main hover:theme-bg-card'
                }`}
            >
              <Database className="h-4 w-4" />
              <span>{t('settings.tabStorage')}</span>
            </button>

            <button
              onClick={() => setActiveTab('backup')}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-medium transition-all cursor-pointer ${activeTab === 'backup'
                  ? 'bg-blue-600 text-white shadow-xs'
                  : 'theme-text-muted hover:theme-text-main hover:theme-bg-card'
                }`}
            >
              <Archive className="h-4 w-4" />
              <span>{t('settings.tabBackup')}</span>
            </button>

            <button
              onClick={() => setActiveTab('api')}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-medium transition-all cursor-pointer ${activeTab === 'api'
                  ? 'bg-blue-600 text-white shadow-xs'
                  : 'theme-text-muted hover:theme-text-main hover:theme-bg-card'
                }`}
            >
              <Server className="h-4 w-4" />
              <span>{t('settings.tabApi')}</span>
            </button>

            <button
              onClick={() => setActiveTab('about')}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-medium transition-all cursor-pointer ${activeTab === 'about'
                  ? 'bg-blue-600 text-white shadow-xs'
                  : 'theme-text-muted hover:theme-text-main hover:theme-bg-card'
                }`}
            >
              <Info className="h-4 w-4" />
              <span>{t('settings.tabAbout')}</span>
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
                      <span className="font-bold text-xs theme-text-main">{t('settings.haTitle')}</span>
                    </div>
                    <label className="flex items-center gap-2 text-xs cursor-pointer select-none">
                      <span className="theme-text-muted text-[11px]">{t('settings.failover')}</span>
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
                          {t('settings.primary')}
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
                          {t('settings.fallback')}
                        </span>
                        <span className="font-medium theme-text-main truncate">{fallbackProvider.name}</span>
                      </div>
                      <span className="text-[10px] font-mono theme-text-sub">
                        {customModels[fallbackProvider.id] || fallbackProvider.defaultModel}
                      </span>
                    </div>
                  </div>

                  <div className="flex items-center justify-between pt-1 text-[11px] theme-text-muted">
                    <span>{t('settings.haHint')}</span>
                    <button
                      onClick={handleTestPipeline}
                      disabled={testingPipeline}
                      className="flex items-center gap-1 text-blue-500 hover:underline font-medium cursor-pointer flex-shrink-0"
                    >
                      {testingPipeline ? <Loader2 className="h-3 w-3 animate-spin" /> : <RefreshCw className="h-3 w-3" />}
                      <span>{t('settings.testPipeline')}</span>
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
                          {pipelineResult.primary.success
                            ? t('settings.primaryOk', { name: primaryProvider.name, ms: pipelineResult.primary.latency_ms })
                            : t('settings.primaryFail', { name: primaryProvider.name, msg: pipelineResult.primary.message })}
                        </div>
                        {pipelineResult.fallback && (
                          <div>
                            {pipelineResult.fallback.success
                              ? t('settings.fallbackOk', { name: fallbackProvider.name, ms: pipelineResult.fallback.latency_ms })
                              : t('settings.fallbackFail', { name: fallbackProvider.name, msg: pipelineResult.fallback.message })}
                          </div>
                        )}
                      </div>
                    </div>
                  )}
                </div>

                <div>
                  <h3 className="text-sm font-bold theme-text-main">{t('settings.providerList')}</h3>
                  <p className="text-xs theme-text-muted mt-0.5">
                    {t('settings.providerListHint')}
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
                            <span className="w-1.5 h-1.5 rounded-full bg-emerald-500 flex-shrink-0" title={t('settings.keyConfigured')} />
                          )}
                        </div>

                        <div className="flex items-center gap-1 mt-1">
                          {isPrimary && (
                            <span className="px-1.5 py-0.2 text-[9px] font-bold bg-blue-600 text-white rounded">
                              {t('settings.primaryModel')}
                            </span>
                          )}
                          {isFallback && (
                            <span className="px-1.5 py-0.2 text-[9px] font-bold bg-purple-600 text-white rounded">
                              {t('settings.fallbackModel')}
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
                      <span className="font-bold text-xs theme-text-main">{t('settings.configDetail', { name: activeProvider.name })}</span>
                    </div>

                    <div className="flex items-center gap-2">
                      {activeProvider.id !== primaryProviderId && (
                        <button
                          onClick={() => handleSetAsPrimary(activeProvider.id)}
                          className="px-2 py-1 text-[11px] rounded-lg border theme-border bg-blue-600/10 text-blue-500 hover:bg-blue-600 hover:text-white transition-all cursor-pointer"
                        >
                          {t('settings.setPrimary')}
                        </button>
                      )}
                      {activeProvider.id !== fallbackProviderId && (
                        <button
                          onClick={() => handleSetAsFallback(activeProvider.id)}
                          className="px-2 py-1 text-[11px] rounded-lg border theme-border bg-purple-600/10 text-purple-500 hover:bg-purple-600 hover:text-white transition-all cursor-pointer"
                        >
                          {t('settings.setFallback')}
                        </button>
                      )}
                      {activeProvider.apiKeyLink && (
                        <a
                          href={activeProvider.apiKeyLink}
                          target="_blank"
                          rel="noreferrer"
                          className="text-[11px] text-blue-500 hover:underline flex items-center gap-1 ml-1"
                        >
                          <span>{t('settings.getKey')}</span>
                          <ExternalLink className="h-3 w-3" />
                        </a>
                      )}
                    </div>
                  </div>

                  {/* API Key */}
                  <div className="space-y-1.5">
                    <label className="text-[11px] font-medium theme-text-muted flex items-center gap-1">
                      <Key className="h-3 w-3" />
                      <span>{t('settings.apiKey')}</span>
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
                      <span>{t('settings.baseUrl')}</span>
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
                      <span>{t('settings.model')}</span>
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
                          placeholder={t('settings.presetPh')}
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
                      <span>{t('settings.testEndpoint')}</span>
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
                  <h3 className="text-sm font-bold theme-text-main">{t('settings.storageTitle')}</h3>
                  <p className="text-xs theme-text-muted mt-0.5">
                    {t('settings.storageHint')}
                  </p>
                </div>

                <div className="theme-bg-sub border theme-border rounded-xl p-4 space-y-3">
                  <div className="flex items-center justify-between">
                    <span className="text-xs font-semibold theme-text-main">{t('settings.dbTitle')}</span>
                    <span className="px-2 py-0.5 text-[10px] bg-emerald-500/15 text-emerald-500 border border-emerald-500/30 rounded font-medium">
                      {t('settings.mounted')}
                    </span>
                  </div>

                  <div className="text-xs font-mono theme-text-muted bg-black/10 dark:bg-black/40 p-2.5 rounded-lg border theme-border break-all select-all">
                    {dbPath}
                  </div>

                  <div className="grid grid-cols-2 gap-3 pt-2">
                    <div className="p-2.5 rounded-lg theme-bg-card border theme-border text-xs">
                      <div className="theme-text-muted">{t('settings.aggSessions')}</div>
                      <div className="text-base font-bold theme-text-main font-mono mt-0.5">
                        {totalConversations.toLocaleString()}
                      </div>
                    </div>
                    <div className="p-2.5 rounded-lg theme-bg-card border theme-border text-xs">
                      <div className="theme-text-muted">{t('settings.histMessages')}</div>
                      <div className="text-base font-bold theme-text-main font-mono mt-0.5">
                        {totalMessages.toLocaleString()}
                      </div>
                    </div>
                  </div>
                </div>

                <div className="theme-bg-sub border theme-border rounded-xl p-4 space-y-3">
                  <div>
                    <div className="text-xs font-semibold theme-text-main">{t('settings.syncFreq')}</div>
                    <p className="text-[11px] theme-text-muted mt-1">
                      {t('settings.syncFreqHint')}
                    </p>
                  </div>

                  <div className="flex items-center gap-3">
                    <CustomSelect<number>
                      value={autoSyncIntervalSec}
                      onChange={(val) => onAutoSyncIntervalChange(val)}
                      options={[
                        { value: 30, label: t('settings.sec30'), subLabel: t('settings.highFreq') },
                        { value: 60, label: t('settings.sec60'), subLabel: t('settings.recommended') },
                        { value: 120, label: t('settings.sec120'), subLabel: t('settings.powerSave') },
                        { value: 300, label: t('settings.sec300'), subLabel: t('settings.lowFreq') },
                      ]}
                      className="min-w-[130px]"
                    />
                    <span className="text-[11px] theme-text-sub">
                      {t('settings.syncDefaultHint')}
                    </span>
                  </div>
                </div>
              </div>
            )}

            {activeTab === 'backup' && (
              <div className="space-y-5">
                <div>
                  <h3 className="text-sm font-bold theme-text-main">{t('settings.backupTitle')}</h3>
                  <p className="text-xs theme-text-muted mt-0.5">
                    {t('settings.backupHint')}
                  </p>
                </div>

                {/* 备份存储目录配置 */}
                <div className="theme-bg-sub border theme-border rounded-xl p-4 space-y-3">
                  <div className="flex items-center justify-between">
                    <span className="text-xs font-semibold theme-text-main">{t('settings.backupPath')}</span>
                    <span className="text-[11px] theme-text-muted">{t('settings.backupPathHint')}</span>
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
                      title={t('settings.openFolder')}
                    >
                      <Folder className="h-3.5 w-3.5" />
                      <span>{t('settings.browse')}</span>
                    </button>
                    <button
                      onClick={() => refreshBackups(backupTargetPath)}
                      className="px-3 py-2 text-xs font-medium theme-bg-card hover:theme-bg-sub border theme-border theme-text-main rounded-lg transition-all cursor-pointer flex items-center gap-1.5 shrink-0"
                      title={t('settings.refreshList')}
                    >
                      <RefreshCw className={`h-3.5 w-3.5 ${loadingBackups ? 'animate-spin' : ''}`} />
                      <span>{t('settings.refresh')}</span>
                    </button>
                  </div>

                  {/* 快捷预设胶囊按钮 */}
                  <div className="pt-1">
                    <div className="text-[11px] theme-text-muted mb-1.5 flex items-center gap-1">
                      <span>{t('settings.presets')}</span>
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
                            <span className="w-1.5 h-1.5 rounded-full bg-emerald-500" title={t('settings.detected')} />
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
                      <div className="text-xs font-semibold theme-text-main">{t('settings.autoBackup')}</div>
                      <p className="text-[11px] theme-text-muted mt-0.5">
                        {t('settings.autoBackupHint')}
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
                      <span>{isBackingUp ? t('settings.backingUp') : t('settings.backupNow')}</span>
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
                      <span>{t('settings.snapshots', { n: backupList.length })}</span>
                    </span>
                    <span className="text-[11px] theme-text-muted">{t('settings.selfContained')}</span>
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
                      {t('settings.noBackup')}
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
                                {t('settings.snapshotN', { n: index + 1 })}
                              </span>
                              <span className="text-xs font-mono font-medium theme-text-main">{item.file_name}</span>
                            </div>
                            <div className="text-[11px] theme-text-muted flex items-center gap-3">
                              <span>{t('settings.size', { size: item.file_size_formatted })}</span>
                              <span>{t('settings.time', { time: item.created_at ? new Date(item.created_at).toLocaleString() : t('settings.unknown') })}</span>
                            </div>
                          </div>

                          <div>
                            {confirmRestoreFile === item.file_path ? (
                              <div className="flex items-center gap-2">
                                <span className="text-[11px] text-amber-400 font-medium">{t('settings.confirmRestore')}</span>
                                <button
                                  onClick={() => handleRestoreBackup(item.file_path)}
                                  disabled={isRestoring}
                                  className="px-2.5 py-1 text-xs bg-amber-600 hover:bg-amber-500 text-white rounded-md cursor-pointer flex items-center gap-1"
                                >
                                  {isRestoring ? <Loader2 className="h-3 w-3 animate-spin" /> : <Check className="h-3 w-3" />}
                                  <span>{t('settings.confirm')}</span>
                                </button>
                                <button
                                  onClick={() => setConfirmRestoreFile(null)}
                                  disabled={isRestoring}
                                  className="px-2 py-1 text-xs theme-bg-sub theme-text-muted hover:theme-text-main rounded-md cursor-pointer"
                                >
                                  {t('settings.cancel')}
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
                                <span>{t('settings.restoreFrom')}</span>
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
                  <h3 className="text-sm font-bold theme-text-main">{t('settings.apiTitle')}</h3>
                  <p className="text-xs theme-text-muted mt-0.5">
                    {t('settings.apiHint')}
                  </p>
                </div>

                {/* 核心服务状态与在线文档主入口卡片 */}
                <div className="theme-bg-sub border theme-border rounded-2xl p-6 space-y-6">
                  {/* 状态与地址栏 */}
                  <div className="flex items-center justify-between flex-wrap gap-4">
                    <div className="space-y-1.5">
                      <div className="text-xs font-semibold theme-text-muted">{t('settings.listenAddr')}</div>
                      <div className="flex items-center gap-2">
                        <div className="px-3.5 py-2 rounded-xl bg-black/20 dark:bg-white/5 border theme-border font-mono text-sm text-blue-400 font-bold select-all">
                          http://127.0.0.1:8788
                        </div>
                        <button
                          onClick={() => copyToClipboard('http://127.0.0.1:8788', 'base-url')}
                          className="px-3 py-2 text-xs theme-bg-card hover:theme-bg-sub border theme-border theme-text-muted hover:theme-text-main rounded-xl transition-all cursor-pointer flex items-center gap-1.5 shadow-xs"
                          title={t('settings.copyBase')}
                        >
                          {copiedEndpoint === 'base-url' ? <Check className="h-4 w-4 text-emerald-500" /> : <Copy className="h-4 w-4" />}
                          <span>{copiedEndpoint === 'base-url' ? t('settings.copied') : t('settings.copy')}</span>
                        </button>
                      </div>
                    </div>

                    <div className="flex flex-col items-end gap-1.5">
                      <div className="text-xs font-semibold theme-text-muted">{t('settings.apiStatus')}</div>
                      <div className="inline-flex items-center gap-2 px-3 py-1.5 rounded-full text-xs font-semibold bg-emerald-500/10 text-emerald-500 border border-emerald-500/20">
                        <span className="w-2.5 h-2.5 rounded-full bg-emerald-500 animate-pulse" />
                        <span>{apiHealth?.ok ? t('settings.apiActive') : t('settings.apiReady')}</span>
                        {apiHealth && (
                          <span className="text-[11px] font-mono text-emerald-400/80">
                            ({apiHealth.latencyMs}ms)
                          </span>
                        )}
                      </div>
                    </div>
                  </div>

                  {/* 鉴权说明 */}
                  <div className="border border-blue-500/20 bg-blue-500/5 rounded-xl p-4 flex items-start gap-3">
                    <div className="p-1.5 rounded-lg bg-blue-500/20 text-blue-400 mt-0.5 shrink-0">
                      <Lock className="h-4 w-4" />
                    </div>
                    <div className="text-xs space-y-1">
                      <div className="font-bold text-blue-400">{t('settings.noToken')}</div>
                      <p className="theme-text-muted text-xs leading-relaxed">
                        {t('settings.noTokenHint')}
                      </p>
                    </div>
                  </div>

                  {/* 唯一主入口按钮：打开精美交互式文档网页 */}
                  <button
                    onClick={() => api.openUrl('http://127.0.0.1:8788/docs')}
                    className="w-full py-3 px-4 bg-blue-600 hover:bg-blue-500 text-white font-semibold text-sm rounded-xl transition-all duration-150 cursor-pointer shadow-xs flex items-center justify-center gap-2"
                  >
                    <span>{t('settings.viewDocs')}</span>
                    <ExternalLink className="h-4 w-4 opacity-90" />
                  </button>
                </div>
              </div>
            )}

            {activeTab === 'appearance' && (
              <div className="space-y-5">
                <div>
                  <h3 className="text-sm font-bold theme-text-main">{t('settings.appearanceTitle')}</h3>
                  <p className="text-xs theme-text-muted mt-0.5">
                    {t('settings.appearanceSub')}
                  </p>
                </div>

                <div>
                  <div className="text-xs font-semibold theme-text-main mb-1">{t('settings.language')}</div>
                  <p className="text-[11px] theme-text-muted mb-2">{t('settings.langAutoHint')}</p>
                  <div className="grid grid-cols-2 gap-3">
                    <button
                      type="button"
                      onClick={() => setLocale('zh')}
                      className={`p-3 rounded-xl border text-xs cursor-pointer transition-all ${
                        locale === 'zh'
                          ? 'bg-blue-600/15 border-blue-500 theme-text-main shadow-xs'
                          : 'theme-bg-sub theme-border hover:theme-border-hover theme-text-muted'
                      }`}
                    >
                      {t('settings.langZh')}
                    </button>
                    <button
                      type="button"
                      onClick={() => setLocale('en')}
                      className={`p-3 rounded-xl border text-xs cursor-pointer transition-all ${
                        locale === 'en'
                          ? 'bg-blue-600/15 border-blue-500 theme-text-main shadow-xs'
                          : 'theme-bg-sub theme-border hover:theme-border-hover theme-text-muted'
                      }`}
                    >
                      {t('settings.langEn')}
                    </button>
                  </div>
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
                    <span className="font-semibold">{t('settings.dark')}</span>
                    <span className="text-[10px] theme-text-sub">{t('settings.darkHint')}</span>
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
                    <span className="font-semibold">{t('settings.light')}</span>
                    <span className="text-[10px] theme-text-sub">{t('settings.lightHint')}</span>
                  </div>
                </div>
              </div>
            )}

            {activeTab === 'about' && (
              <div className="space-y-5">
                <div>
                  <h3 className="text-sm font-bold theme-text-main">{t('settings.aboutTitle')}</h3>
                  <p className="text-xs theme-text-muted mt-0.5">
                    {t('settings.aboutSub')}
                  </p>
                </div>

                <div className="theme-bg-sub border theme-border rounded-xl p-4 space-y-3 text-xs">
                  <div className="flex justify-between py-1 border-b theme-border-sub">
                    <span className="theme-text-muted">{t('settings.version')}</span>
                    <span className="font-mono font-medium theme-text-main">v{appVersion}</span>
                  </div>
                  <div className="flex justify-between py-1 border-b theme-border-sub">
                    <span className="theme-text-muted">{t('settings.stack')}</span>
                    <span className="font-mono font-medium theme-text-main">Tauri v2 + Rust + React 19</span>
                  </div>
                  <div className="flex justify-between py-1 border-b theme-border-sub">
                    <span className="theme-text-muted">{t('settings.agents')}</span>
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
