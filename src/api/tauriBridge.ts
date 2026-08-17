import { invoke } from '@tauri-apps/api/core';
import type {
  DashboardStats,
  WorkspaceStat,
  ConversationItem,
  MessageItem,
  SearchResultItem,
  WorkspaceDetailStats,
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

  async triggerSync(full?: boolean): Promise<{ success: boolean; message: string }> {
    if (isTauri()) {
      return await invoke<{ success: boolean; message: string }>('trigger_sync', { full: !!full });
    }
    const res = await fetch(`/api/sync?mode=${full ? 'full' : 'incremental'}`, {
      method: 'POST',
    });
    return await res.json();
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
    maxTokens = 2048
  ): Promise<{
    success: boolean;
    content: string;
    provider_used: string;
    is_fallback: boolean;
    latency_ms: number;
    error?: string;
  }> {
    if (isTauri()) {
      return await invoke('call_llm_with_fallback', {
        primary,
        fallback: fallback || null,
        messages,
        maxTokens,
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
};
