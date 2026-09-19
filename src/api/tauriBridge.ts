import { invoke } from '@tauri-apps/api/core';
import type {
  DashboardStats,
  DailyTimelineStats,
  WorkspaceStat,
  WorkspaceMergeResult,
  ConversationMoveResult,
  ConversationItem,
  MessageItem,
  ArtifactItem,
  WorkspaceArtifactItem,
  SearchResultItem,
  SyncResultInfo,
  WorkspaceDetailStats,
  WorkspaceFineBlock,
  WorkspaceModuleBlock,
  AnalysisUserMessage,
  BackupInfo,
  RestoreInfo,
  CloudPreset,
  AppConfig,
  PromptItem,
  PromptInput,
  CatalogSyncResult,
  IdeAppStatus,
  AgentSourceInfo,
  LlmCallLogItem,
  LlmUsageSummary,
  QuotaSnapshot,
} from '../types';

export const isTauri = () => {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
};

export const api = {
  async getDashboardStats(): Promise<DashboardStats> {
    if (isTauri()) {
      return await invoke<DashboardStats>('get_dashboard_stats');
    }
    const res = await fetch('/api/dashboard');
    return await res.json();
  },

  async getDailyTimeline(date: string): Promise<DailyTimelineStats> {
    if (isTauri()) {
      return await invoke<DailyTimelineStats>('get_daily_timeline', { date });
    }
    const res = await fetch(`/api/daily-timeline?date=${encodeURIComponent(date)}`);
    return await res.json();
  },

  async getWorkspaceDetail(workspacePath: string): Promise<WorkspaceDetailStats> {
    if (isTauri()) {
      return await invoke<WorkspaceDetailStats>('get_workspace_detail', { workspacePath });
    }
    const res = await fetch(`/api/workspace?path=${encodeURIComponent(workspacePath)}`);
    return await res.json();
  },

  async listWorkspaces(search?: string): Promise<WorkspaceStat[]> {
    if (isTauri()) {
      return await invoke<WorkspaceStat[]>('list_workspaces', { search: search || null });
    }
    const res = await fetch(`/api/workspaces?q=${encodeURIComponent(search || '')}`);
    return await res.json();
  },

  async mergeWorkspace(sourcePath: string, targetPath: string): Promise<WorkspaceMergeResult> {
    if (isTauri()) {
      return await invoke<WorkspaceMergeResult>('merge_workspace_cmd', {
        sourcePath,
        targetPath,
      });
    }
    const res = await fetch('/api/workspaces/merge', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ source_path: sourcePath, target_path: targetPath }),
    });
    if (!res.ok) throw new Error(await res.text());
    const data = await res.json();
    return data.result;
  },

  async moveConversation(conversationId: string, targetPath: string): Promise<ConversationMoveResult> {
    if (isTauri()) {
      return await invoke<ConversationMoveResult>('move_conversation_cmd', {
        conversationId,
        targetPath,
      });
    }
    const res = await fetch('/api/conversations/move', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ conversation_id: conversationId, target_path: targetPath }),
    });
    if (!res.ok) throw new Error(await res.text());
    const data = await res.json();
    return data.result;
  },

  async listConversations(
    workspace?: string,
    search?: string,
    starredOnly?: boolean
  ): Promise<ConversationItem[]> {
    if (isTauri()) {
      return await invoke<ConversationItem[]>('list_conversations', {
        workspace: workspace || null,
        search: search || null,
        starredOnly: !!starredOnly,
      });
    }
    const res = await fetch(
      `/api/conversations?workspace=${encodeURIComponent(workspace || '')}&cq=${encodeURIComponent(
        search || ''
      )}&starred=${starredOnly ? '1' : '0'}`
    );
    return await res.json();
  },

  async getConversationMessages(conversationId: string): Promise<MessageItem[]> {
    if (isTauri()) {
      return await invoke<MessageItem[]>('get_conversation_messages', { conversationId });
    }
    const res = await fetch(`/api/conversation/${encodeURIComponent(conversationId)}/messages`);
    return await res.json();
  },

  async getConversationArtifacts(conversationId: string): Promise<ArtifactItem[]> {
    if (isTauri()) {
      return await invoke<ArtifactItem[]>('get_conversation_artifacts_cmd', { conversationId });
    }
    const res = await fetch(`/api/conversation/${encodeURIComponent(conversationId)}/artifacts`);
    const data = await res.json();
    return data.artifacts || [];
  },

  async getWorkspaceArtifacts(workspacePath: string): Promise<WorkspaceArtifactItem[]> {
    if (isTauri()) {
      return await invoke<WorkspaceArtifactItem[]>('get_workspace_artifacts_cmd', { workspacePath });
    }
    const res = await fetch(`/api/workspace/artifacts?path=${encodeURIComponent(workspacePath)}`);
    const data = await res.json();
    return data.artifacts || [];
  },

  async toggleStar(conversationId: string): Promise<boolean> {
    if (isTauri()) {
      return await invoke<boolean>('toggle_star', { conversationId });
    }
    const res = await fetch(`/api/conversation/${encodeURIComponent(conversationId)}/toggle-star`, {
      method: 'POST',
    });
    const data = await res.json();
    return data.starred;
  },

  async updateConversationAiTitle(
    conversationId: string,
    aiTitle: string
  ): Promise<ConversationItem> {
    if (isTauri()) {
      return await invoke<ConversationItem>('update_conversation_ai_title_cmd', {
        conversationId,
        aiTitle,
      });
    }
    throw new Error('仅在客户端环境下支持重命名会话');
  },

  async clearConversationAiTitle(conversationId: string): Promise<ConversationItem> {
    if (isTauri()) {
      return await invoke<ConversationItem>('clear_conversation_ai_title_cmd', {
        conversationId,
      });
    }
    throw new Error('仅在客户端环境下支持清除 AI 标题');
  },

  async saveConversationAiSummary(payload: {
    conversationId: string;
    aiTitle?: string | null;
    summary: string;
    status: string;
    basedOnContentHash?: string | null;
    basedOnMessageCount?: number | null;
    model?: string | null;
    error?: string | null;
  }): Promise<ConversationItem> {
    if (isTauri()) {
      return await invoke<ConversationItem>('save_conversation_ai_summary_cmd', {
        conversationId: payload.conversationId,
        aiTitle: payload.aiTitle ?? null,
        summary: payload.summary,
        status: payload.status,
        basedOnContentHash: payload.basedOnContentHash ?? null,
        basedOnMessageCount: payload.basedOnMessageCount ?? null,
        model: payload.model ?? null,
        error: payload.error ?? null,
      });
    }
    throw new Error('仅在客户端环境下支持保存会话摘要');
  },

  async setConversationAiStatus(
    conversationId: string,
    status: string,
    error?: string | null
  ): Promise<void> {
    if (isTauri()) {
      await invoke('set_conversation_ai_status_cmd', {
        conversationId,
        status,
        error: error ?? null,
      });
      return;
    }
    throw new Error('仅在客户端环境下支持更新会话摘要状态');
  },

  async getConversationItem(conversationId: string): Promise<ConversationItem | null> {
    if (isTauri()) {
      return await invoke<ConversationItem | null>('get_conversation_item', { conversationId });
    }
    return null;
  },

  async searchMessages(query: string, role?: string, limit?: number): Promise<SearchResultItem[]> {
    if (isTauri()) {
      return await invoke<SearchResultItem[]>('search_messages', {
        query,
        role: role || null,
        limit: limit || 30,
      });
    }
    const res = await fetch(
      `/api/spotlight?q=${encodeURIComponent(query)}&role=${encodeURIComponent(role || '')}&limit=${limit || 30}`
    );
    const data = await res.json();
    return data.items || [];
  },

  async triggerSync(full?: boolean): Promise<SyncResultInfo> {
    if (isTauri()) {
      return await invoke<SyncResultInfo>('trigger_sync', { full: !!full });
    }
    const res = await fetch(`/api/sync?mode=${full ? 'full' : 'incremental'}`, {
      method: 'POST',
    });
    return await res.json();
  },

  async getQuotaSnapshot(force = false): Promise<QuotaSnapshot> {
    if (isTauri()) {
      return await invoke<QuotaSnapshot>('get_quota_snapshot', { force });
    }
    return { generated_at: new Date().toISOString(), providers: [] };
  },

  async testLlmConnection(
    baseUrl: string,
    apiKey: string,
    model: string
  ): Promise<{ success: boolean; message: string; latency_ms: number }> {
    if (isTauri()) {
      return await invoke<{ success: boolean; message: string; latency_ms: number }>(
        'test_llm_connection',
        { baseUrl, apiKey, model }
      );
    }
    const start = Date.now();
    try {
      const targetUrl = `${baseUrl.replace(/\/+$/, '')}/chat/completions`;
      const res = await fetch(targetUrl, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Authorization: apiKey ? `Bearer ${apiKey}` : '',
        },
        body: JSON.stringify({
          model,
          messages: [{ role: 'user', content: 'Ping' }],
          max_tokens: 5,
        }),
      });
      const latency = Date.now() - start;
      if (res.ok) {
        return { success: true, message: '连接成功！模型响应正常', latency_ms: latency };
      }
      return { success: false, message: `HTTP ${res.status}`, latency_ms: latency };
    } catch (e: any) {
      return { success: false, message: e.message, latency_ms: Date.now() - start };
    }
  },

  async testLlmPipeline(
    primary: { provider_name: string; base_url: string; api_key: string; model: string },
    fallback?: { provider_name: string; base_url: string; api_key: string; model: string }
  ): Promise<{
    primary: { success: boolean; message: string; latency_ms: number };
    fallback?: { success: boolean; message: string; latency_ms: number };
    overall_success: boolean;
    message: string;
  }> {
    if (isTauri()) {
      return await invoke('test_llm_pipeline', { primary, fallback: fallback || null });
    }
    const p = await api.testLlmConnection(primary.base_url, primary.api_key, primary.model);
    let fb = undefined;
    if (fallback) {
      fb = await api.testLlmConnection(fallback.base_url, fallback.api_key, fallback.model);
    }
    const overall = p.success || (fb ? fb.success : false);
    return {
      primary: p,
      fallback: fb,
      overall_success: overall,
      message: p.success && fb?.success ? '主备链路双重连通正常' : p.message,
    };
  },

  async callLlmWithFallback(
    primary: { provider_name: string; base_url: string; api_key: string; model: string },
    fallback?: { provider_name: string; base_url: string; api_key: string; model: string },
    messages: Array<{ role: string; content: string }> = [],
    maxTokens?: number,
    disableThinking?: boolean,
    scene?: string
  ): Promise<{
    success: boolean;
    content: string;
    provider_used: string;
    is_fallback: boolean;
    latency_ms: number;
    prompt_tokens?: number;
    completion_tokens?: number;
    total_tokens?: number;
    error?: string;
  }> {
    if (isTauri()) {
      return await invoke('call_llm_with_fallback', {
        primary,
        fallback: fallback || null,
        messages,
        maxTokens: maxTokens ?? null,
        disableThinking: disableThinking ?? true,
        scene: scene || 'general',
      });
    }
    return {
      success: false,
      content: '',
      provider_used: primary.provider_name,
      is_fallback: false,
      latency_ms: 0,
      error: 'Web mode fallback not implemented',
    };
  },

  async getLlmCallLogs(limit = 50, offset = 0): Promise<LlmCallLogItem[]> {
    if (isTauri()) {
      try {
        return await invoke<LlmCallLogItem[]>('get_llm_call_logs_cmd', { limit, offset });
      } catch (e) {
        console.error('Failed to get LLM call logs:', e);
        return [];
      }
    }
    return [];
  },

  async getLlmUsageSummary(): Promise<LlmUsageSummary> {
    if (isTauri()) {
      try {
        return await invoke<LlmUsageSummary>('get_llm_usage_summary_cmd');
      } catch (e) {
        console.error('Failed to get LLM usage summary:', e);
      }
    }
    return {
      total_calls: 0,
      success_calls: 0,
      total_prompt_tokens: 0,
      total_completion_tokens: 0,
      total_tokens: 0,
      avg_latency_ms: 0,
      fallback_calls: 0,
    };
  },

  async clearLlmCallLogs(): Promise<boolean> {
    if (isTauri()) {
      try {
        return await invoke<boolean>('clear_llm_call_logs_cmd');
      } catch (e) {
        console.error('Failed to clear LLM call logs:', e);
        return false;
      }
    }
    return true;
  },

  async getDatabasePathInfo(): Promise<string> {
    if (isTauri()) {
      try {
        return await invoke<string>('get_database_path_info');
      } catch {
        return '~/.agentdeck/agentdeck.db';
      }
    }
    return '~/.agentdeck/agentdeck.db';
  },

  async getAgentSources(): Promise<AgentSourceInfo[]> {
    if (isTauri()) {
      try {
        return await invoke<AgentSourceInfo[]>('get_agent_sources_cmd');
      } catch (e) {
        console.error('Failed to get agent sources via Tauri invoke:', e);
      }
    }
    try {
      const res = await fetch('/api/agent-sources');
      const data = await res.json();
      return data.sources || [];
    } catch {
      return [];
    }
  },

  // 研发分析相关 API
  async getWorkspaceAnalysisMessages(workspacePath: string): Promise<AnalysisUserMessage[]> {
    if (isTauri()) {
      return await invoke<AnalysisUserMessage[]>('get_workspace_analysis_messages', { workspacePath });
    }
    const res = await fetch(`/api/user-messages?workspace=${encodeURIComponent(workspacePath)}&format=flat`);
    const data = await res.json();
    return (data.messages || []).map((m: any) => ({
      conversation_id: m.conversation_id,
      conversation_title: m.conversation_title || '',
      created_at: m.created_at,
      content: m.content || '',
    }));
  },

  async saveWorkspaceFineBlocks(
    workspacePath: string,
    blocks: WorkspaceFineBlock[],
    clearExisting = false
  ): Promise<number> {
    if (isTauri()) {
      return await invoke<number>('save_workspace_fine_blocks_cmd', {
        workspacePath,
        blocks,
        clearExisting,
      });
    }
    return blocks.length;
  },

  async saveWorkspaceModuleBlocks(
    workspacePath: string,
    modules: WorkspaceModuleBlock[],
    clearExisting = false
  ): Promise<number> {
    if (isTauri()) {
      return await invoke<number>('save_workspace_module_blocks_cmd', {
        workspacePath,
        modules,
        clearExisting,
      });
    }
    return modules.length;
  },

  async saveWorkspaceReport(workspacePath: string, reportMd: string): Promise<void> {
    if (isTauri()) {
      await invoke('save_workspace_report_cmd', {
        workspacePath,
        reportMd,
      });
    }
  },

  async clearWorkspaceAnalysis(workspacePath: string): Promise<void> {
    if (isTauri()) {
      await invoke('clear_workspace_analysis_cmd', {
        workspacePath,
      });
    }
  },

  async getAppVersion(): Promise<string> {
    if (isTauri()) {
      try {
        const { getVersion } = await import('@tauri-apps/api/app');
        return await getVersion();
      } catch {
        return 'unknown';
      }
    }
    return 'unknown';
  },

  async createBackup(targetDir: string, maxSnapshots = 3): Promise<BackupInfo> {
    if (isTauri()) {
      return await invoke<BackupInfo>('create_backup_cmd', {
        targetDir,
        maxSnapshots,
      });
    }
    throw new Error('仅在客户端环境下支持本地数据备份');
  },

  async listBackups(targetDir: string): Promise<BackupInfo[]> {
    if (isTauri()) {
      return await invoke<BackupInfo[]>('list_backups_cmd', {
        targetDir,
      });
    }
    return [];
  },

  async restoreBackup(backupFile: string): Promise<RestoreInfo> {
    if (isTauri()) {
      return await invoke<RestoreInfo>('restore_backup_cmd', {
        backupFile,
      });
    }
    throw new Error('仅在客户端环境下支持本地数据还原');
  },

  async getCloudPresets(): Promise<CloudPreset[]> {
    if (isTauri()) {
      return await invoke<CloudPreset[]>('get_cloud_presets_cmd');
    }
    return [];
  },

  async selectFolderDialog(): Promise<string | null> {
    if (isTauri()) {
      return await invoke<string | null>('select_folder_dialog_cmd');
    }
    return null;
  },

  async getAppConfig(): Promise<AppConfig | null> {
    if (isTauri()) {
      return await invoke<AppConfig>('get_app_config_cmd');
    }
    return null;
  },

  async saveAppConfig(config: AppConfig): Promise<void> {
    if (isTauri()) {
      await invoke('save_app_config_cmd', { config });
    }
  },

  async openUrl(url: string): Promise<void> {
    if (isTauri()) {
      try {
        await invoke('open_url_cmd', { url });
        return;
      } catch (e) {
        console.error('Failed to open url via Rust command:', e);
      }
    }
    window.open(url, '_blank');
  },

  async listIdeApps(): Promise<IdeAppStatus[]> {
    if (isTauri()) {
      return await invoke<IdeAppStatus[]>('list_ide_apps_cmd');
    }
    return [
      { id: 'cursor', label: 'Cursor', kind: 'app', installed: false },
      { id: 'antigravity', label: 'Antigravity', kind: 'app', installed: false },
      { id: 'claude', label: 'Claude Code', kind: 'cli', installed: false },
      { id: 'codex', label: 'Codex', kind: 'cli', installed: false },
      { id: 'mimo', label: 'Xiaomi MiMo', kind: 'app', installed: false },
    ];
  },

  async openWorkspaceInIde(ide: string, workspacePath: string): Promise<void> {
    if (isTauri()) {
      await invoke('open_workspace_in_ide_cmd', { ide, workspacePath });
      return;
    }
    throw new Error('仅在客户端环境下支持打开 AI IDE');
  },

  async listPrompts(
    search?: string,
    category?: string,
    starredOnly?: boolean,
    lite = true
  ): Promise<PromptItem[]> {
    if (isTauri()) {
      return await invoke<PromptItem[]>('list_prompts_cmd', {
        search: search || null,
        category: category || null,
        starredOnly: !!starredOnly,
        lite,
      });
    }
    return [];
  },

  async getPrompt(id: number): Promise<PromptItem> {
    if (isTauri()) {
      return await invoke<PromptItem>('get_prompt_cmd', { id });
    }
    throw new Error('仅在客户端环境下支持提示词库');
  },

  async createPrompt(input: PromptInput): Promise<PromptItem> {
    if (isTauri()) {
      return await invoke<PromptItem>('create_prompt_cmd', { input });
    }
    throw new Error('仅在客户端环境下支持提示词库');
  },

  /** 选择本地图片并归档到媒体库，返回 /media/prompts/uploads/… 路径；取消返回 null */
  async pickPromptPreviewImage(): Promise<string | null> {
    if (isTauri()) {
      return await invoke<string | null>('pick_prompt_preview_image_cmd');
    }
    throw new Error('仅在客户端环境下支持本地图片上传');
  },

  /** 拖拽导入：按本地文件路径归档图片，返回 /media/prompts/uploads/… 路径 */
  async importPromptPreviewImage(path: string): Promise<string> {
    if (isTauri()) {
      return await invoke<string>('import_prompt_preview_image_cmd', { path });
    }
    throw new Error('仅在客户端环境下支持本地图片上传');
  },

  async updatePrompt(id: number, input: PromptInput): Promise<PromptItem> {
    if (isTauri()) {
      return await invoke<PromptItem>('update_prompt_cmd', { id, input });
    }
    throw new Error('仅在客户端环境下支持提示词库');
  },

  async deletePrompt(id: number): Promise<boolean> {
    if (isTauri()) {
      return await invoke<boolean>('delete_prompt_cmd', { id });
    }
    return false;
  },

  async togglePromptStar(id: number): Promise<boolean> {
    if (isTauri()) {
      return await invoke<boolean>('toggle_prompt_star_cmd', { id });
    }
    return false;
  },

  async recordPromptUse(id: number): Promise<PromptItem> {
    if (isTauri()) {
      return await invoke<PromptItem>('record_prompt_use_cmd', { id });
    }
    throw new Error('仅在客户端环境下支持提示词库');
  },

  /** 写回封面图原始宽高（渲染元数据，仅变化时落库） */
  async updatePromptPreviewSize(id: number, width: number, height: number): Promise<boolean> {
    if (isTauri()) {
      return await invoke<boolean>('update_prompt_preview_size_cmd', { id, width, height });
    }
    return false;
  },

  async syncGptImageCatalog(): Promise<CatalogSyncResult> {
    if (isTauri()) {
      return await invoke<CatalogSyncResult>('sync_gpt_image_catalog_cmd');
    }
    throw new Error('仅在客户端环境下支持目录同步');
  },

  async cachePromptPreviews(limit = 40): Promise<number> {
    if (isTauri()) {
      return await invoke<number>('cache_prompt_previews_cmd', { limit });
    }
    return 0;
  },

  async warmPromptPreviewCache(batchSize = 40, maxBatches = 20): Promise<number> {
    if (isTauri()) {
      return await invoke<number>('warm_prompt_preview_cache_cmd', {
        batchSize,
        maxBatches,
      });
    }
    return 0;
  },

  async countUncachedPromptPreviews(): Promise<number> {
    if (isTauri()) {
      return await invoke<number>('count_uncached_prompt_previews_cmd');
    }
    return 0;
  },

  services: {
    list: () => invoke<import('../hooks/useLocalServices').ServiceMeta[]>('service_list'),
    status: (id?: string) =>
      invoke<import('../hooks/useLocalServices').ServiceStatus[]>('service_status', {
        id: id ?? null,
      }),
    start: (id: string) =>
      invoke<import('../hooks/useLocalServices').ServiceActionResult>('service_start', { id }),
    stop: (id: string, force?: boolean) =>
      invoke<import('../hooks/useLocalServices').ServiceActionResult>('service_stop', {
        id,
        force: force ?? null,
      }),
    restart: (id: string) =>
      invoke<import('../hooks/useLocalServices').ServiceActionResult>('service_restart', { id }),
    open: (id: string) => invoke<void>('service_open', { id }),
    openInIde: (path: string, ide: string) => invoke<void>('service_open_in_ide', { path, ide }),
    detectIdes: () =>
      invoke<import('../hooks/useLocalServices').InstalledIdes>('service_detect_ides'),
    tailLog: (id: string, lines?: number) =>
      invoke<string>('service_tail_log', { id, lines: lines ?? null }),
    pickFolder: () => invoke<string | null>('service_pick_folder'),
    scanProject: (projectDir: string) =>
      invoke<import('../types/serviceCandidate').CandidateConfig>('service_scan_project', {
        projectDir,
      }),
    probeCandidate: (payload: import('../types/serviceCandidate').ServiceRegistrationPayload, timeoutSecs?: number, attempt?: number) =>
      invoke<import('../types/serviceCandidate').ProbeReport>('service_probe_candidate', {
        payload,
        timeoutSecs: timeoutSecs ?? null,
        attempt: attempt ?? null,
      }),
    upsert: (payload: import('../types/serviceCandidate').ServiceRegistrationPayload) =>
      invoke<import('../hooks/useLocalServices').ServiceActionResult>('service_upsert', {
        payload,
      }),
    renameProject: (projectId: string, name: string) =>
      invoke<import('../hooks/useLocalServices').ServiceActionResult>('service_rename_project', {
        projectId,
        name,
      }),
    renameService: (id: string, name: string) =>
      invoke<import('../hooks/useLocalServices').ServiceActionResult>('service_rename_service', {
        id,
        name,
      }),
    remove: (id: string) =>
      invoke<import('../hooks/useLocalServices').ServiceActionResult>('service_remove', { id }),
  },
};
