export interface WorkspaceStat {
  workspace_path: string;
  cnt: number;
  ag_cnt: number;
  cursor_cnt: number;
  claude_cnt: number;
  codex_cnt: number;
  wb_cnt: number;
  hermes_cnt: number;
  message_count: number;
  user_message_count: number;
  last_updated?: string;
}

export interface ConversationItem {
  id: string;
  workspace_path: string;
  source_app: string;
  title: string;
  created_at?: string;
  updated_at?: string;
  message_count: number;
  user_message_count: number;
  parse_status: string;
  is_starred: boolean;
}

export interface ToolCallItem {
  name?: string;
  tool_name?: string;
  arguments?: any;
  result?: any;
  error?: string;
}

export interface MessageItem {
  id: string;
  conversation_id: string;
  step_index?: number;
  sender: 'user' | 'assistant' | 'tool' | 'system' | string;
  text: string;
  thinking?: string;
  created_at?: string;
  model_name?: string;
  token_count?: number;
  duration_ms?: number;
  tool_calls_json?: string;
  images?: string | Array<{ src: string; width?: number; height?: number }>;
}

export interface AgentShare {
  app: string;
  label: string;
  count: number;
  percent: number;
  color: string;
}

export interface ToolUsageStat {
  category: string;
  count: number;
  percent: number;
  color: string;
}

export interface PunchcardSlot {
  hour: number;
  count: number;
  level: number;
  percent: number;
}

export interface TopRankItem {
  id: string;
  title: string;
  source_app: string;
  source_label: string;
  workspace_path: string;
  workspace_short: string;
  message_count: number;
  user_message_count: number;
  updated_at?: string;
  is_starred: boolean;
}

export interface TopWorkspaceItem {
  path: string;
  short_name: string;
  count: number;
  message_count: number;
  user_message_count: number;
  percent: number;
}

export interface DashboardStats {
  total_conversations: number;
  total_messages: number;
  total_user_messages: number;
  total_workspaces: number;
  starred_count: number;
  total_tool_calls: number;
  agent_comparison_convs: AgentShare[];
  agent_comparison_msgs: AgentShare[];
  punchcard_msgs: PunchcardSlot[];
  punchcard_convs: PunchcardSlot[];
  tool_usage: ToolUsageStat[];
  top_conversations_all: TopRankItem[];
  top_conversations_user: TopRankItem[];
  top_workspaces: TopWorkspaceItem[];
  last_sync_time?: string;
}

export interface SearchResultItem {
  message_id: string;
  conversation_id: string;
  conversation_title: string;
  source_app: string;
  workspace_path: string;
  sender: string;
  snippet: string;
  created_at?: string;
}

export interface WorkspaceFineBlock {
  id: number;
  block_id: string;
  batch_index?: number;
  type: string;
  title: string;
  summary: string;
  start_date?: string;
  end_date?: string;
  status: string;
  keywords: string[];
}

export interface WorkspaceModuleBlock {
  id: number;
  module_id: string;
  type: string;
  title: string;
  summary: string;
  start_date?: string;
  end_date?: string;
  status: string;
  keywords: string[];
  child_fine_ids: string[];
}

export interface HeatmapCell {
  date: string;
  count: number;
  level: number;
}

export interface WorkspaceDetailStats {
  workspace_path: string;
  workspace_short: string;
  conversation_count: number;
  ag_conversation_count: number;
  cursor_conversation_count: number;
  claude_conversation_count: number;
  codex_conversation_count: number;
  wb_conversation_count: number;
  hermes_conversation_count: number;
  user_message_count: number;
  message_count: number;
  agent_breakdown: string;
  first_active?: string;
  last_active?: string;
  active_days: number;
  peak_day?: string;
  peak_count: number;
  heatmap_cells: HeatmapCell[];
  fine_blocks: WorkspaceFineBlock[];
  module_blocks: WorkspaceModuleBlock[];
  report_md?: string;
}

export interface SyncResultInfo {
  success: boolean;
  new_count: number;
  updated_count: number;
  message: string;
}
