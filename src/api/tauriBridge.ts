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
};
