export interface WorkspaceStat {
  workspace_path: string;
  cnt: number;
  ag_cnt: number;
  cursor_cnt: number;
  claude_cnt: number;
  codex_cnt: number;
  wb_cnt: number;
  hermes_cnt: number;
  mimo_cnt: number;
  windsurf_cnt: number;
  codebuddy_cnt: number;
  qoder_cnt: number;
  message_count: number;
  user_message_count: number;
  last_updated?: string;
}

export interface WorkspaceMergeResult {
  source_path: string;
  target_path: string;
  moved_conversations: number;
  moved_analysis_blocks: number;
  kept_target_analysis: boolean;
}

export interface ConversationMoveResult {
  conversation_id: string;
  target_path: string;
}

export interface ConversationItem {
  id: string;
  workspace_path: string;
  source_app: string;
  /** 展示标题：优先 AI 标题 */
  title: string;
  /** 同步源标题 */
  source_title: string;
  ai_title?: string | null;
  ai_summary?: string | null;
  ai_status?: string | null;
  ai_summary_stale?: boolean;
  ai_model?: string | null;
  ai_generated_at?: string | null;
  ai_new_message_count?: number | null;
  content_hash?: string;
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
  credit?: number;
  duration_ms?: number;
  tool_calls_json?: string;
  images?: string | Array<{ src: string; width?: number; height?: number }>;
}

export interface ArtifactItem {
  id: number;
  conversation_id: string;
  file_name: string;
  file_path: string;
  title: string;
  summary?: string | null;
  content: string;
  user_facing: boolean;
  request_feedback: boolean;
  created_at?: string | null;
  updated_at?: string | null;
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

export interface HourlyBarSlot {
  hour: number;
  count: number;
}

export interface DailyBarSlot {
  date: string;
  count: number;
}

export interface DayHourlyBars {
  date: string;
  hours: HourlyBarSlot[];
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

export interface DailyTimelineItem {
  message_id: number;
  id: string;
  workspace_path: string;
  workspace_short: string;
  source_app: string;
  source_label: string;
  source_color: string;
  conversation_title: string;
  prompt_content: string;
  prompt_preview: string;
  time: string;
  time_label: string;
  minute: number;
  is_starred: boolean;
}

export interface DailyConcurrencySlot {
  hour: number;
  minute: number;
  time_label: string;
  active_conversations: number;
  active_workspaces: number;
}

export interface DailyTimelineStats {
  date: string;
  total_conversations: number;
  total_workspaces: number;
  total_messages: number;
  total_user_messages: number;
  peak_concurrency: number;
  items: DailyTimelineItem[];
  concurrency_slots: DailyConcurrencySlot[];
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
  last30_hourly_msgs: DayHourlyBars[];
  last30_hourly_convs: DayHourlyBars[];
  last30_hourly_user_msgs: DayHourlyBars[];
  last30_daily_msgs: DailyBarSlot[];
  last30_daily_convs: DailyBarSlot[];
  last30_daily_user_msgs: DailyBarSlot[];
  heatmap_cells: HeatmapCell[];
  heatmap_cells_convs: HeatmapCell[];
  heatmap_cells_user: HeatmapCell[];
  heatmap_active_days: number;
  heatmap_longest_streak: number;
  heatmap_peak_day?: string;
  heatmap_peak_count: number;
  tool_usage: ToolUsageStat[];
  top_conversations_all: TopRankItem[];
  top_conversations_user: TopRankItem[];
  top_workspaces: TopWorkspaceItem[];
  last_sync_time?: string;
  beijing_today: string;
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

export interface BlockEvidence {
  date?: string;
  conversation_title?: string;
  snippet?: string;
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
  evidence?: BlockEvidence[];
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
  evidence?: BlockEvidence[];
  child_fine_ids: string[];
}

export interface AnalysisUserMessage {
  id?: string;
  conversation_id: string;
  conversation_title?: string;
  created_at?: string;
  content: string;
}

export interface HeatmapCell {
  date: string;
  count: number;
  level: number;
  user_count?: number;
  total_messages?: number;
  conv_count?: number;
}

export interface WorkspaceArtifactItem {
  id: number;
  conversation_id: string;
  conversation_title: string;
  source_app: string;
  file_name: string;
  file_path: string;
  title: string;
  summary?: string;
  content: string;
  user_facing: boolean;
  request_feedback: boolean;
  created_at?: string;
  updated_at?: string;
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
  mimo_conversation_count: number;
  windsurf_conversation_count: number;
  codebuddy_conversation_count: number;
  qoder_conversation_count: number;
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
  artifacts?: WorkspaceArtifactItem[];
}

export interface SyncResultInfo {
  success: boolean;
  new_count: number;
  updated_count: number;
  message: string;
}

export type PromptCategory = 'image' | 'video' | 'text';

export interface PromptItem {
  id: number;
  title: string;
  content: string;
  category: PromptCategory | string;
  tags: string[];
  source_url?: string;
  source_note?: string;
  notes?: string;
  preview_url?: string;
  preview_local?: string;
  origin: string;
  external_id?: string;
  genre?: string;
  styles: string[];
  scenes: string[];
  featured: boolean;
  github_url?: string;
  prompt_preview?: string;
  content_hash?: string;
  /** 封面图原始宽高（渲染元数据，用于 Feed 占位与分列估算） */
  preview_width?: number;
  preview_height?: number;
  is_starred: boolean;
  use_count: number;
  created_at: string;
  updated_at: string;
  last_used_at?: string;
}

export interface PromptInput {
  title: string;
  content: string;
  category: string;
  tags: string[];
  source_url?: string;
  source_note?: string;
  notes?: string;
  preview_url?: string;
  preview_local?: string;
  is_starred: boolean;
}

export interface CatalogSyncResult {
  fetched: number;
  inserted: number;
  updated: number;
  unchanged: number;
  images_cached: number;
  message: string;
}

export interface CatalogSyncProgress {
  phase: string;
  message: string;
  current: number;
  total: number;
  percent: number;
  inserted: number;
  updated: number;
  unchanged: number;
  imagesCached: number;
}

export interface BackupInfo {
  file_name: string;
  file_path: string;
  file_size_bytes: number;
  file_size_formatted: string;
  created_at: string;
  conversation_count?: number;
  media_file_count?: number;
}

export interface RestoreInfo {
  success: boolean;
  message: string;
  conversation_count: number;
  media_file_count: number;
}

export interface CloudPreset {
  id: string;
  name: string;
  icon: string;
  path: string;
  available: boolean;
}

export interface IdeAppStatus {
  id: 'cursor' | 'antigravity' | 'claude' | 'codex' | string;
  label: string;
  kind: 'app' | 'cli' | string;
  installed: boolean;
}

export interface BackupProgress {
  stage: string;
  percent: number;
  message: string;
}

export interface BackupConfig {
  target_path: string;
  auto_backup_enabled: boolean;
  max_snapshots: number;
}

export interface AppConfig {
  backup: BackupConfig;
  theme?: string;
  ai_config?: Record<string, any>;
}

export interface AgentPathInfo {
  path: string;
  display_path: string;
  description: string;
  exists: boolean;
  is_dir: boolean;
}

export interface AgentSourceInfo {
  id: string;
  name: string;
  detected: boolean;
  session_count: number;
  paths: AgentPathInfo[];
}

export interface LlmCallLogItem {
  id: number;
  created_at: string;
  scene: string;
  provider_name: string;
  model: string;
  system_prompt: string;
  user_prompt_snippet: string;
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  latency_ms: number;
  is_fallback: boolean;
  status: 'success' | 'error' | string;
  error_msg?: string | null;
}

export interface LlmUsageSummary {
  total_calls: number;
  success_calls: number;
  total_prompt_tokens: number;
  total_completion_tokens: number;
  total_tokens: number;
  avg_latency_ms: number;
  fallback_calls: number;
}

export interface QuotaWindow {
  id: string;
  kind: string;
  scope?: string | null;
  used_fraction: number;
  used_percent: number;
  exhausted: boolean;
  resets_at?: string | null;
}

export interface ProviderQuota {
  id: string;
  name: string;
  available: boolean;
  status: string;
  message?: string | null;
  plan?: string | null;
  credit_balance?: string | null;
  source?: string | null;
  observed_at?: string | null;
  age_seconds?: number | null;
  windows: QuotaWindow[];
  headline: string[];
}

export interface QuotaSnapshot {
  generated_at: string;
  providers: ProviderQuota[];
}
