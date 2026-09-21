use chrono::{DateTime, NaiveDate, Timelike, Utc};
use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;

pub fn to_beijing_iso(raw: Option<String>) -> Option<String> {
    let s = raw?.trim().to_string();
    if s.is_empty() {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(&s) {
        if let Some(beijing_tz) = chrono::FixedOffset::east_opt(8 * 3600) {
            return Some(dt.with_timezone(&beijing_tz).to_rfc3339());
        }
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S") {
        let dt_utc = chrono::DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc);
        if let Some(beijing_tz) = chrono::FixedOffset::east_opt(8 * 3600) {
            return Some(dt_utc.with_timezone(&beijing_tz).to_rfc3339());
        }
    }
    Some(s)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceStat {
    pub workspace_path: String,
    pub cnt: i64,
    pub ag_cnt: i64,
    pub cursor_cnt: i64,
    pub claude_cnt: i64,
    pub codex_cnt: i64,
    pub wb_cnt: i64,
    pub hermes_cnt: i64,
    pub mimo_cnt: i64,
    pub windsurf_cnt: i64,
    pub codebuddy_cnt: i64,
    pub qoder_cnt: i64,
    pub message_count: i64,
    pub user_message_count: i64,
    pub last_updated: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationItem {
    pub id: String,
    pub workspace_path: String,
    pub source_app: String,
    /// 列表展示标题：优先 AI 标题，否则同步源标题
    pub title: String,
    /// 同步源标题（每次 sync upsert 覆盖）
    pub source_title: String,
    pub ai_title: Option<String>,
    pub ai_summary: Option<String>,
    pub ai_status: Option<String>,
    pub ai_summary_stale: bool,
    pub ai_model: Option<String>,
    pub ai_generated_at: Option<String>,
    /// 总结之后新增的消息条数（当前 message_count - 总结时 message_count）；无摘要或无新增时为 None
    pub ai_new_message_count: Option<i64>,
    pub content_hash: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub message_count: i64,
    pub user_message_count: i64,
    pub parse_status: String,
    pub is_starred: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageItem {
    pub id: String,
    pub conversation_id: String,
    pub step_index: Option<i64>,
    pub sender: String,
    pub text: String,
    pub thinking: Option<String>,
    pub created_at: Option<String>,
    pub model_name: Option<String>,
    pub token_count: Option<i64>,
    pub duration_ms: Option<i64>,
    pub tool_calls_json: Option<String>,
    pub images: Option<String>,
    pub credit: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentShare {
    pub app: String,
    pub label: String,
    pub count: i64,
    pub percent: f64,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUsageStat {
    pub category: String,
    pub count: i64,
    pub percent: f64,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PunchcardSlot {
    pub hour: u32,
    pub count: i64,
    pub level: u32,
    pub percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourlyBarSlot {
    pub hour: u32,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyBarSlot {
    pub date: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayHourlyBars {
    pub date: String,
    pub hours: Vec<HourlyBarSlot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopRankItem {
    pub id: String,
    pub title: String,
    pub source_app: String,
    pub source_label: String,
    pub workspace_path: String,
    pub workspace_short: String,
    pub message_count: i64,
    pub user_message_count: i64,
    pub updated_at: Option<String>,
    pub is_starred: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopWorkspaceItem {
    pub path: String,
    pub short_name: String,
    pub count: i64,
    pub message_count: i64,
    pub user_message_count: i64,
    pub percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyTimelineItem {
    pub message_id: i64,
    pub id: String,
    pub workspace_path: String,
    pub workspace_short: String,
    pub source_app: String,
    pub source_label: String,
    pub source_color: String,
    pub conversation_title: String,
    pub prompt_content: String,
    pub prompt_preview: String,
    pub time: String,
    pub time_label: String,
    pub minute: u32,
    pub is_starred: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyConcurrencySlot {
    pub hour: u32,
    pub minute: u32,
    pub time_label: String,
    pub active_conversations: usize,
    pub active_workspaces: usize,
}

/// 会话级活动时段（甘特图标准条形：起止分钟对齐 24h 时间轴）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyActivitySpan {
    pub conversation_id: String,
    pub workspace_path: String,
    pub workspace_short: String,
    pub source_app: String,
    pub source_label: String,
    pub source_color: String,
    pub conversation_title: String,
    /// 当天第一条消息的分钟（0~1440，北京时间）
    pub start_minute: u32,
    /// 当天最后一条消息的分钟（0~1440，北京时间）
    pub end_minute: u32,
    pub start_label: String,
    pub end_label: String,
    pub prompt_count: i64,
    pub message_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyTimelineStats {
    pub date: String,
    pub total_conversations: usize,
    pub total_workspaces: usize,
    pub total_messages: i64,
    pub total_user_messages: i64,
    pub peak_concurrency: usize,
    pub items: Vec<DailyTimelineItem>,
    pub concurrency_slots: Vec<DailyConcurrencySlot>,
    #[serde(default)]
    pub spans: Vec<DailyActivitySpan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardStats {
    pub total_conversations: i64,
    pub total_messages: i64,
    pub total_user_messages: i64,
    pub total_workspaces: i64,
    pub starred_count: i64,
    pub total_tool_calls: i64,
    pub agent_comparison_convs: Vec<AgentShare>,
    pub agent_comparison_msgs: Vec<AgentShare>,
    pub punchcard_msgs: Vec<PunchcardSlot>,
    pub punchcard_convs: Vec<PunchcardSlot>,
    pub last30_hourly_msgs: Vec<DayHourlyBars>,
    pub last30_hourly_convs: Vec<DayHourlyBars>,
    pub last30_hourly_user_msgs: Vec<DayHourlyBars>,
    pub last30_daily_msgs: Vec<DailyBarSlot>,
    pub last30_daily_convs: Vec<DailyBarSlot>,
    pub last30_daily_user_msgs: Vec<DailyBarSlot>,
    pub heatmap_cells: Vec<HeatmapCell>,
    pub heatmap_cells_convs: Vec<HeatmapCell>,
    pub heatmap_cells_user: Vec<HeatmapCell>,
    pub heatmap_active_days: i64,
    pub heatmap_longest_streak: i64,
    pub heatmap_peak_day: Option<String>,
    pub heatmap_peak_count: i64,
    pub tool_usage: Vec<ToolUsageStat>,
    pub top_conversations_all: Vec<TopRankItem>,
    pub top_conversations_user: Vec<TopRankItem>,
    pub top_workspaces: Vec<TopWorkspaceItem>,
    pub last_sync_time: Option<String>,
    pub beijing_today: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultItem {
    pub message_id: String,
    pub conversation_id: String,
    pub conversation_title: String,
    pub source_app: String,
    pub workspace_path: String,
    pub sender: String,
    pub snippet: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageTotals {
    pub requests: i64,
    pub input_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub credit: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageAgentRow {
    pub agent: String,
    pub label: String,
    pub requests: i64,
    pub input_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub credit: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageModelRow {
    pub agent: String,
    pub model: String,
    pub requests: i64,
    pub input_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub credit: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageDayRow {
    pub date: String,
    pub requests: i64,
    pub input_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub credit: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageProjectRow {
    pub project: String,
    pub workspace_path: String,
    pub requests: i64,
    pub input_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub credit: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSessionRow {
    pub conversation_id: String,
    pub session: String,
    pub project: String,
    pub requests: i64,
    pub input_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub credit: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStatsPayload {
    pub days: Option<i64>,
    pub totals: UsageTotals,
    pub by_agent: Vec<UsageAgentRow>,
    pub by_model: Vec<UsageModelRow>,
    pub by_project: Vec<UsageProjectRow>,
    pub by_session: Vec<UsageSessionRow>,
    pub by_day: Vec<UsageDayRow>,
    /// 已接入但源数据不含 token 用量的 agent，如实展示而非静默为零
    pub agents_without_usage: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceFineBlock {
    pub id: i64,
    pub block_id: String,
    pub batch_index: Option<i64>,
    pub r#type: String,
    pub title: String,
    pub summary: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub status: String,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceModuleBlock {
    pub id: i64,
    pub module_id: String,
    pub r#type: String,
    pub title: String,
    pub summary: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub status: String,
    pub keywords: Vec<String>,
    pub child_fine_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarredSessionItem {
    pub conversation_id: String,
    pub workspace_path: String,
    pub source_app: String,
    pub starred_at: String,
    pub message_count: i64,
    pub conversation_title: String,
    pub created_at: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisUserMessage {
    pub id: Option<String>,
    pub conversation_id: String,
    pub conversation_title: String,
    pub created_at: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactItem {
    pub id: i64,
    pub conversation_id: String,
    pub file_name: String,
    pub file_path: String,
    pub title: String,
    pub summary: Option<String>,
    pub content: String,
    pub user_facing: bool,
    pub request_feedback: bool,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeatmapCell {
    pub date: String,
    pub count: i64,
    pub level: u32,
    #[serde(default)]
    pub user_count: i64,
    #[serde(default)]
    pub total_messages: i64,
    #[serde(default)]
    pub conv_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceArtifactItem {
    pub id: i64,
    pub conversation_id: String,
    pub conversation_title: String,
    pub source_app: String,
    pub file_name: String,
    pub file_path: String,
    pub title: String,
    pub summary: Option<String>,
    pub content: String,
    pub user_facing: bool,
    pub request_feedback: bool,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceDetailStats {
    pub workspace_path: String,
    pub workspace_short: String,
    pub conversation_count: i64,
    pub ag_conversation_count: i64,
    pub cursor_conversation_count: i64,
    pub claude_conversation_count: i64,
    pub codex_conversation_count: i64,
    pub wb_conversation_count: i64,
    pub hermes_conversation_count: i64,
    pub mimo_conversation_count: i64,
    pub windsurf_conversation_count: i64,
    pub codebuddy_conversation_count: i64,
    pub qoder_conversation_count: i64,
    pub user_message_count: i64,
    pub message_count: i64,
    pub agent_breakdown: String,
    pub first_active: Option<String>,
    pub last_active: Option<String>,
    pub active_days: i64,
    pub peak_day: Option<String>,
    pub peak_count: i64,
    pub heatmap_cells: Vec<HeatmapCell>,
    pub fine_blocks: Vec<WorkspaceFineBlock>,
    pub module_blocks: Vec<WorkspaceModuleBlock>,
    pub report_md: Option<String>,
    pub artifacts: Vec<WorkspaceArtifactItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCallLogItem {
    pub id: i64,
    pub created_at: String,
    pub scene: String,
    pub provider_name: String,
    pub model: String,
    pub system_prompt: String,
    pub user_prompt_snippet: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub latency_ms: i64,
    pub is_fallback: bool,
    pub status: String,
    pub error_msg: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmUsageSummary {
    pub total_calls: i64,
    pub success_calls: i64,
    pub total_prompt_tokens: i64,
    pub total_completion_tokens: i64,
    pub total_tokens: i64,
    pub avg_latency_ms: f64,
    pub fallback_calls: i64,
}

pub struct DbState {
    pub conn_mutex: Mutex<Connection>,
}

impl DbState {
    pub fn new() -> Result<Self> {
        let db_path = get_database_path();
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(&db_path)?;
        apply_write_pragmas(&conn);
        init_schema(&conn)?;
        Ok(Self {
            conn_mutex: Mutex::new(conn),
        })
    }
}

/// 写连接调优：WAL + synchronous=NORMAL，避免每条语句 full fsync
pub fn apply_write_pragmas(conn: &Connection) {
    let _ = conn.pragma_update(None, "journal_mode", "WAL");
    let _ = conn.pragma_update(None, "synchronous", "NORMAL");
    let _ = conn.pragma_update(None, "temp_store", "MEMORY");
    let _ = conn.pragma_update(None, "cache_size", -65536i64);
    let _ = conn.pragma_update(None, "wal_autocheckpoint", 4000i64);
    let _ = conn.busy_timeout(std::time::Duration::from_secs(30));
}

/// 只读连接调优：开启 query_only=ON，配置更合理的 busy_timeout，确保不抢写锁
pub fn apply_read_pragmas(conn: &Connection) {
    let _ = conn.pragma_update(None, "query_only", "ON");
    let _ = conn.pragma_update(None, "temp_store", "MEMORY");
    let _ = conn.pragma_update(None, "cache_size", -32768i64);
    let _ = conn.pragma_update(None, "mmap_size", 268435456i64); // 256MB mmap 加速只读
    let _ = conn.busy_timeout(std::time::Duration::from_secs(5));
}

/// 打开专属的无锁只读连接（专为 UI 查询服务，彻底解耦写锁与全局互斥锁）
pub fn open_read_connection() -> Result<Connection> {
    let db_path = get_database_path();
    let conn = Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    apply_read_pragmas(&conn);
    Ok(conn)
}

/// 打开独立写连接（后台同步任务用，避免占用 DbState 主锁过久）
pub fn open_write_connection() -> Result<Connection> {
    let db_path = get_database_path();
    let conn = Connection::open(&db_path)?;
    apply_write_pragmas(&conn);
    Ok(conn)
}

pub fn get_database_path() -> PathBuf {
    // 1. 若设置了环境变量 AGENTDECK_DB_PATH，直接遵循外部指定
    if let Ok(env_path) = std::env::var("AGENTDECK_DB_PATH") {
        if !env_path.trim().is_empty() {
            let p = PathBuf::from(env_path.trim());
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            return p;
        }
    }

    // 2. 独立规范主库路径：~/.agentdeck/agentdeck.db
    if let Some(home) = dirs::home_dir() {
        let app_dir = home.join(".agentdeck");
        let _ = std::fs::create_dir_all(&app_dir);
        return app_dir.join("agentdeck.db");
    }

    PathBuf::from("agentdeck.db")
}

pub fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS workspaces (
            workspace_path TEXT PRIMARY KEY,
            display_name TEXT,
            last_updated TEXT
        );

        -- 工作区别名：项目在磁盘上重命名后，把旧路径永久映射到新路径。
        -- 同步导入时套用该映射，避免源日志中的旧路径把已合并的工作区"复活"。
        CREATE TABLE IF NOT EXISTS workspace_aliases (
            old_path TEXT PRIMARY KEY,
            new_path TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        -- 会话级工作区覆盖：用户把单条会话移动到别的项目后，
        -- 同步重导时以此为准，不被源日志里的原工作区覆盖。
        CREATE TABLE IF NOT EXISTS conversation_workspace_overrides (
            conversation_id TEXT PRIMARY KEY,
            workspace_path TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS conversations (
            id TEXT PRIMARY KEY,
            workspace_path TEXT NOT NULL DEFAULT '',
            source_app TEXT,
            source_types TEXT NOT NULL DEFAULT '[]',
            title TEXT NOT NULL DEFAULT '',
            created_at TEXT,
            updated_at TEXT,
            message_count INTEGER NOT NULL DEFAULT 0,
            user_message_count INTEGER NOT NULL DEFAULT 0,
            parse_status TEXT NOT NULL DEFAULT 'ok',
            content_hash TEXT NOT NULL DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id TEXT NOT NULL,
            step_index INTEGER NOT NULL DEFAULT 0,
            role TEXT NOT NULL,
            message_type TEXT NOT NULL DEFAULT '',
            content TEXT NOT NULL DEFAULT '',
            thinking TEXT,
            tool_name TEXT,
            tool_args TEXT,
            created_at TEXT,
            source TEXT NOT NULL DEFAULT '',
            is_truncated INTEGER NOT NULL DEFAULT 0,
            images TEXT,
            model_name TEXT,
            token_count INTEGER,
            credit REAL,
            FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
        );

        -- 用量事实表：一行 = 一次 API 请求的 token/积分消耗。
        -- 与 messages 平行（同挂 conversation_id），不一一对应：usage 是计费事件，
        -- Codex/MiMo/Hermes 等来源的 usage 不锚定任何消息。
        CREATE TABLE IF NOT EXISTS usage_records (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id TEXT NOT NULL,
            agent TEXT NOT NULL DEFAULT '',
            identity TEXT NOT NULL,
            model TEXT,
            input_tokens INTEGER,
            cache_read_tokens INTEGER,
            cache_write_tokens INTEGER,
            output_tokens INTEGER,
            reasoning_tokens INTEGER,
            credit REAL,
            occurred_at TEXT,
            is_partial INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_usage_records_conv_identity
            ON usage_records (conversation_id, identity);

        CREATE INDEX IF NOT EXISTS idx_usage_records_occurred
            ON usage_records (occurred_at);

        CREATE TABLE IF NOT EXISTS starred_sessions (
            conversation_id TEXT PRIMARY KEY,
            starred_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS workspace_blocks_fine (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            workspace_path TEXT NOT NULL,
            message_fingerprint TEXT NOT NULL DEFAULT '',
            block_id TEXT NOT NULL,
            batch_index INTEGER,
            type TEXT NOT NULL DEFAULT 'feature',
            title TEXT NOT NULL,
            summary TEXT NOT NULL DEFAULT '',
            start_date TEXT,
            end_date TEXT,
            status TEXT NOT NULL DEFAULT 'completed',
            keywords_json TEXT NOT NULL DEFAULT '[]',
            evidence_json TEXT NOT NULL DEFAULT '[]',
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TEXT
        );

        CREATE TABLE IF NOT EXISTS workspace_blocks_modules (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            workspace_path TEXT NOT NULL,
            message_fingerprint TEXT NOT NULL DEFAULT '',
            module_id TEXT NOT NULL,
            type TEXT NOT NULL DEFAULT 'module',
            title TEXT NOT NULL,
            summary TEXT NOT NULL DEFAULT '',
            start_date TEXT,
            end_date TEXT,
            status TEXT NOT NULL DEFAULT 'completed',
            keywords_json TEXT NOT NULL DEFAULT '[]',
            evidence_json TEXT NOT NULL DEFAULT '[]',
            child_fine_ids_json TEXT NOT NULL DEFAULT '[]',
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TEXT
        );

        CREATE TABLE IF NOT EXISTS workspace_reports (
            workspace_path TEXT PRIMARY KEY,
            report_md TEXT NOT NULL DEFAULT '',
            updated_at TEXT,
            generated_at TEXT
        );

        CREATE TABLE IF NOT EXISTS sync_state (
            source_path TEXT PRIMARY KEY,
            conversation_id TEXT,
            source_type TEXT,
            file_mtime REAL,
            file_size INTEGER,
            synced_at TEXT
        );

        CREATE TABLE IF NOT EXISTS sync_runs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            started_at TEXT NOT NULL,
            finished_at TEXT,
            mode TEXT NOT NULL,
            new_count INTEGER DEFAULT 0,
            updated_count INTEGER DEFAULT 0,
            skipped_count INTEGER DEFAULT 0,
            error_count INTEGER DEFAULT 0,
            message TEXT
        );

        CREATE TABLE IF NOT EXISTS prompts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            content TEXT NOT NULL DEFAULT '',
            category TEXT NOT NULL DEFAULT 'image',
            tags_json TEXT NOT NULL DEFAULT '[]',
            source_url TEXT,
            source_note TEXT,
            notes TEXT,
            preview_url TEXT,
            preview_local TEXT,
            origin TEXT NOT NULL DEFAULT 'user',
            external_id TEXT,
            genre TEXT,
            styles_json TEXT NOT NULL DEFAULT '[]',
            scenes_json TEXT NOT NULL DEFAULT '[]',
            featured INTEGER NOT NULL DEFAULT 0,
            github_url TEXT,
            prompt_preview TEXT,
            content_hash TEXT,
            preview_width INTEGER,
            preview_height INTEGER,
            is_starred INTEGER NOT NULL DEFAULT 0,
            use_count INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            last_used_at TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_conv_workspace ON conversations(workspace_path);
        CREATE INDEX IF NOT EXISTS idx_conv_updated ON conversations(updated_at);
        CREATE INDEX IF NOT EXISTS idx_conv_created ON conversations(created_at);
        CREATE INDEX IF NOT EXISTS idx_messages_conv ON messages(conversation_id, step_index);
        CREATE INDEX IF NOT EXISTS idx_messages_created ON messages(created_at);
        CREATE INDEX IF NOT EXISTS idx_messages_role_created ON messages(role, created_at);
        CREATE INDEX IF NOT EXISTS idx_messages_tool_name ON messages(tool_name) WHERE tool_name IS NOT NULL AND tool_name != '';
        CREATE INDEX IF NOT EXISTS idx_blocks_fine_ws ON workspace_blocks_fine(workspace_path);
        CREATE INDEX IF NOT EXISTS idx_blocks_modules_ws ON workspace_blocks_modules(workspace_path);
        CREATE INDEX IF NOT EXISTS idx_prompts_category ON prompts(category);
        CREATE INDEX IF NOT EXISTS idx_prompts_starred ON prompts(is_starred);
        CREATE INDEX IF NOT EXISTS idx_prompts_updated ON prompts(updated_at);

        CREATE TABLE IF NOT EXISTS llm_call_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            created_at TEXT NOT NULL,
            scene TEXT NOT NULL DEFAULT '',
            provider_name TEXT NOT NULL DEFAULT '',
            model TEXT NOT NULL DEFAULT '',
            system_prompt TEXT NOT NULL DEFAULT '',
            user_prompt_snippet TEXT NOT NULL DEFAULT '',
            prompt_tokens INTEGER DEFAULT 0,
            completion_tokens INTEGER DEFAULT 0,
            total_tokens INTEGER DEFAULT 0,
            latency_ms INTEGER DEFAULT 0,
            is_fallback INTEGER DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'success',
            error_msg TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_llm_call_logs_created_at ON llm_call_logs(created_at);
        CREATE INDEX IF NOT EXISTS idx_llm_call_logs_scene ON llm_call_logs(scene);

        CREATE TABLE IF NOT EXISTS conversation_artifacts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id TEXT NOT NULL,
            file_name TEXT NOT NULL,
            file_path TEXT NOT NULL,
            title TEXT NOT NULL DEFAULT '',
            summary TEXT,
            content TEXT NOT NULL DEFAULT '',
            user_facing INTEGER NOT NULL DEFAULT 1,
            request_feedback INTEGER NOT NULL DEFAULT 0,
            created_at TEXT,
            updated_at TEXT,
            UNIQUE(conversation_id, file_name),
            FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_artifacts_conv_id ON conversation_artifacts(conversation_id);

        -- AgentDeck 本地 AI 元数据（与同步源隔离，sync upsert 不触碰）
        CREATE TABLE IF NOT EXISTS conversation_ai (
            conversation_id TEXT PRIMARY KEY,
            ai_title TEXT,
            summary TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'idle',
            based_on_content_hash TEXT,
            model TEXT,
            error TEXT,
            generated_at TEXT,
            updated_at TEXT,
            FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
        );
        "#
    )?;

    ensure_messages_schema(conn)?;

    // 自动兼容性迁移（防止旧表缺少新增字段）
    // messages 扩展列：模型名 / token 用量 / 积分消耗（CodeBuddy 等来源提供）
    let _ = conn.execute("ALTER TABLE messages ADD COLUMN model_name TEXT", []);
    let _ = conn.execute("ALTER TABLE messages ADD COLUMN token_count INTEGER", []);
    let _ = conn.execute("ALTER TABLE messages ADD COLUMN credit REAL", []);
    let _ = conn.execute("ALTER TABLE conversations ADD COLUMN source_app TEXT", []);
    let _ = conn.execute(
        "ALTER TABLE conversations ADD COLUMN content_hash TEXT NOT NULL DEFAULT ''",
        [],
    );

    // 会话 AI 摘要：记录总结时的消息条数，用于展示"总结后新增 N 条"
    let _ = conn.execute(
        "ALTER TABLE conversation_ai ADD COLUMN based_on_message_count INTEGER",
        [],
    );

    let _ = conn.execute("ALTER TABLE prompts ADD COLUMN preview_url TEXT", []);
    let _ = conn.execute("ALTER TABLE prompts ADD COLUMN preview_local TEXT", []);
    let _ = conn.execute(
        "ALTER TABLE prompts ADD COLUMN origin TEXT NOT NULL DEFAULT 'user'",
        [],
    );
    let _ = conn.execute("ALTER TABLE prompts ADD COLUMN external_id TEXT", []);
    let _ = conn.execute("ALTER TABLE prompts ADD COLUMN genre TEXT", []);
    let _ = conn.execute(
        "ALTER TABLE prompts ADD COLUMN styles_json TEXT NOT NULL DEFAULT '[]'",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE prompts ADD COLUMN scenes_json TEXT NOT NULL DEFAULT '[]'",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE prompts ADD COLUMN featured INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute("ALTER TABLE prompts ADD COLUMN github_url TEXT", []);
    let _ = conn.execute("ALTER TABLE prompts ADD COLUMN prompt_preview TEXT", []);
    let _ = conn.execute("ALTER TABLE prompts ADD COLUMN content_hash TEXT", []);
    let _ = conn.execute("ALTER TABLE prompts ADD COLUMN preview_width INTEGER", []);
    let _ = conn.execute("ALTER TABLE prompts ADD COLUMN preview_height INTEGER", []);
    let _ = conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_prompts_external_id ON prompts(external_id) WHERE external_id IS NOT NULL AND external_id != ''",
        [],
    );
    // 旧细分类收敛为 text / image / video
    let _ = conn.execute(
        "UPDATE prompts SET category = 'text' WHERE category IN ('coding','research','writing','product','agent','persona','meta')",
        [],
    );

    let _ = conn.execute(
        "ALTER TABLE workspace_blocks_fine ADD COLUMN created_at TEXT",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE workspace_blocks_fine ADD COLUMN batch_index INTEGER",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE workspace_blocks_fine ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE workspace_blocks_fine ADD COLUMN keywords_json TEXT NOT NULL DEFAULT '[]'",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE workspace_blocks_fine ADD COLUMN evidence_json TEXT NOT NULL DEFAULT '[]'",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE workspace_blocks_fine ADD COLUMN message_fingerprint TEXT NOT NULL DEFAULT ''",
        [],
    );

    let _ = conn.execute(
        "ALTER TABLE workspace_blocks_modules ADD COLUMN created_at TEXT",
        [],
    );
    let _ = conn.execute("ALTER TABLE workspace_blocks_modules ADD COLUMN child_fine_ids_json TEXT NOT NULL DEFAULT '[]'", []);
    let _ = conn.execute(
        "ALTER TABLE workspace_blocks_modules ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE workspace_blocks_modules ADD COLUMN keywords_json TEXT NOT NULL DEFAULT '[]'",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE workspace_blocks_modules ADD COLUMN evidence_json TEXT NOT NULL DEFAULT '[]'",
        [],
    );
    let _ = conn.execute("ALTER TABLE workspace_blocks_modules ADD COLUMN message_fingerprint TEXT NOT NULL DEFAULT ''", []);

    let _ = conn.execute(
        "ALTER TABLE workspace_reports ADD COLUMN updated_at TEXT",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE workspace_reports ADD COLUMN generated_at TEXT",
        [],
    );

    // 兼容短暂出现过的 file_path 列名（正式库为 source_path）
    let _ = conn.execute(
        "ALTER TABLE sync_state RENAME COLUMN file_path TO source_path",
        [],
    );

    migrate_workspace_aliases(conn);
    repair_dirty_workspace_paths(conn);

    Ok(())
}

/// 将历史 `//workspace/...` / `/workspace/...` 合并到本机 `~/workspace/...`
fn migrate_workspace_aliases(conn: &Connection) {
    use crate::importers::canonicalize_workspace_path;

    let Ok(mut stmt) = conn.prepare(
        "SELECT DISTINCT workspace_path FROM conversations \
         WHERE workspace_path LIKE '/workspace/%' OR workspace_path LIKE '//workspace/%'",
    ) else {
        return;
    };
    let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) else {
        return;
    };
    let paths: Vec<String> = rows.flatten().collect();
    drop(stmt);

    for old in paths {
        let new_path = canonicalize_workspace_path(&old);
        if new_path.is_empty() || new_path == old {
            continue;
        }
        let _ = conn.execute(
            "UPDATE conversations SET workspace_path = ?1 WHERE workspace_path = ?2",
            rusqlite::params![&new_path, &old],
        );
        // 旧 workspace 行直接删除，避免主键冲突；展示名会在后续 sync 时重建
        let _ = conn.execute(
            "DELETE FROM workspaces WHERE workspace_path = ?1",
            rusqlite::params![&old],
        );
    }
}

/// 修复 AG 启发式误抽导致的脏路径（含引号、反斜杠、尾逗号、JSON 碎片、@[path] 尾括号）
fn repair_dirty_workspace_paths(conn: &Connection) {
    use crate::importers::canonicalize_workspace_path;

    let Ok(mut stmt) = conn.prepare(
        "SELECT DISTINCT workspace_path FROM conversations WHERE \
         instr(workspace_path, '\"') > 0 \
         OR instr(workspace_path, char(92)) > 0 \
         OR instr(workspace_path, ',') > 0 \
         OR instr(workspace_path, '{') > 0 \
         OR instr(workspace_path, '}') > 0 \
         OR instr(workspace_path, '[') > 0 \
         OR instr(workspace_path, ']') > 0 \
         OR workspace_path LIKE '%toolAction%'",
    ) else {
        return;
    };
    let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) else {
        return;
    };
    let paths: Vec<String> = rows.flatten().collect();
    drop(stmt);

    for old in paths {
        let new_path = canonicalize_workspace_path(&old);
        // 无法洗成合理路径时清空，避免侧栏出现假工作区
        let new_path = if new_path.is_empty() {
            String::new()
        } else {
            new_path
        };
        if new_path == old {
            continue;
        }
        let _ = conn.execute(
            "UPDATE conversations SET workspace_path = ?1 WHERE workspace_path = ?2",
            rusqlite::params![&new_path, &old],
        );
        let _ = conn.execute(
            "DELETE FROM workspaces WHERE workspace_path = ?1",
            rusqlite::params![&old],
        );
        println!(
            "[AgentDeck] repaired dirty workspace_path: {:?} -> {:?}",
            old, new_path
        );
    }
}

/// 纠正历史错误的 messages 骨架（id TEXT / sender / text），迁移到真实列结构
fn ensure_messages_schema(conn: &Connection) -> Result<()> {
    let has_step_index = conn
        .prepare("SELECT step_index FROM messages LIMIT 0")
        .is_ok();
    if has_step_index {
        return Ok(());
    }

    let row_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
        .unwrap_or(0);

    conn.execute_batch(
        r#"
        ALTER TABLE messages RENAME TO messages_legacy_bad;
        CREATE TABLE messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id TEXT NOT NULL,
            step_index INTEGER NOT NULL DEFAULT 0,
            role TEXT NOT NULL,
            message_type TEXT NOT NULL DEFAULT '',
            content TEXT NOT NULL DEFAULT '',
            thinking TEXT,
            tool_name TEXT,
            tool_args TEXT,
            created_at TEXT,
            source TEXT NOT NULL DEFAULT '',
            is_truncated INTEGER NOT NULL DEFAULT 0,
            images TEXT,
            model_name TEXT,
            token_count INTEGER,
            credit REAL,
            FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_messages_conv ON messages(conversation_id, step_index);
        "#,
    )?;

    if row_count > 0 {
        // 尽力从旧骨架迁移可读字段
        let _ = conn.execute_batch(
            r#"
            INSERT INTO messages (conversation_id, step_index, role, message_type, content, created_at)
            SELECT
                COALESCE(conversation_id, ''),
                0,
                COALESCE(sender, 'assistant'),
                'text',
                COALESCE(text, ''),
                created_at
            FROM messages_legacy_bad;
            "#,
        );
    }

    let _ = conn.execute("DROP TABLE IF EXISTS messages_legacy_bad", []);
    Ok(())
}

fn source_to_label_and_color(app: &str) -> (&'static str, &'static str) {
    match app {
        "cursor" => ("Cursor", "#3b82f6"),
        "antigravity" => ("Antigravity", "#10b981"),
        "claude" => ("Claude", "#f97316"),
        "hermes" => ("Hermes", "#8b5cf6"),
        "codex" => ("Codex", "#ec4899"),
        "workbuddy" => ("WorkBuddy", "#06b6d4"),
        "mimo" => ("MiMo", "#f43f5e"),
        "windsurf" => ("Windsurf", "#0ea5e9"),
        "codebuddy" => ("CodeBuddy", "#6C4DFF"),
        "qoder" => ("Qoder", "#2ADB5C"),
        _ => ("Other", "#64748b"),
    }
}

pub(crate) fn get_short_workspace(path: &str) -> String {
    if path.is_empty() {
        return "默认工作区".to_string();
    }
    let parts: Vec<&str> = path.trim_end_matches('/').split('/').collect();
    if parts.len() >= 2 {
        format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1])
    } else {
        parts.last().unwrap_or(&path).to_string()
    }
}

fn collect_timestamp_volume(
    conn: &Connection,
    sql: &str,
    start_30: NaiveDate,
    today: NaiveDate,
) -> Result<([[i64; 24]; 30], [i64; 30])> {
    let mut hourly_by_day = [[0i64; 24]; 30];
    let mut daily = [0i64; 30];
    let start = start_30.format("%Y-%m-%d").to_string();
    let end = (today + chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    // 利用 start_raw 对 created_at 做索引粗过滤，避免全表扫描数十万条历史消息
    let start_raw = (start_30 - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![start, end, start_raw], |row| {
        let date: String = row.get(0)?;
        let hour: i64 = row.get(1)?;
        let count: i64 = row.get(2)?;
        Ok((date, hour, count))
    })?;
    for (date_str, hour, count) in rows.flatten() {
        let Ok(d) = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d") else {
            continue;
        };
        if d < start_30 || d > today {
            continue;
        }
        let idx = (d - start_30).num_days() as usize;
        if idx >= 30 {
            continue;
        }
        daily[idx] += count;
        if (0..24).contains(&hour) {
            hourly_by_day[idx][hour as usize] += count;
        }
    }
    Ok((hourly_by_day, daily))
}

fn slots_from_hourly_days(start_30: NaiveDate, counts: &[[i64; 24]; 30]) -> Vec<DayHourlyBars> {
    (0..30)
        .map(|i| {
            let date = start_30 + chrono::Duration::days(i as i64);
            DayHourlyBars {
                date: date.format("%Y-%m-%d").to_string(),
                hours: slots_from_hourly(&counts[i]),
            }
        })
        .collect()
}

fn slots_from_hourly(counts: &[i64; 24]) -> Vec<HourlyBarSlot> {
    counts
        .iter()
        .enumerate()
        .map(|(h, &count)| HourlyBarSlot {
            hour: h as u32,
            count,
        })
        .collect()
}

fn slots_from_daily(start_30: NaiveDate, counts: &[i64; 30]) -> Vec<DailyBarSlot> {
    (0..30)
        .map(|i| {
            let date = start_30 + chrono::Duration::days(i as i64);
            DailyBarSlot {
                date: date.format("%Y-%m-%d").to_string(),
                count: counts[i],
            }
        })
        .collect()
}

/// 用量聚合列：请求数 + 各 token 桶 + 积分（NULL 一律按 0 汇总）
const USAGE_AGG_COLS: &str = "COUNT(*), \
     COALESCE(SUM(input_tokens), 0), COALESCE(SUM(cache_read_tokens), 0), \
     COALESCE(SUM(cache_write_tokens), 0), COALESCE(SUM(output_tokens), 0), \
     COALESCE(SUM(reasoning_tokens), 0), COALESCE(SUM(credit), 0.0)";

pub fn fetch_usage_stats(conn: &Connection, days: Option<i64>) -> Result<UsageStatsPayload> {
    // days=0 表示「今天」：本地零点起；>0 为滚动 N 天
    let effective_days = days.filter(|d| *d >= 0);
    let cutoff = match effective_days {
        Some(0) => {
            let now_local = chrono::Local::now();
            let elapsed = now_local.time().num_seconds_from_midnight() as i64;
            Some(
                (now_local.with_timezone(&Utc) - chrono::Duration::seconds(elapsed)).to_rfc3339(),
            )
        }
        Some(d) => Some((chrono::Utc::now() - chrono::Duration::days(d)).to_rfc3339()),
        None => None,
    };
    let filter = "WHERE (?1 IS NULL OR occurred_at >= ?1)";

    let (requests, input_tokens, cache_read_tokens, cache_write_tokens, output_tokens, reasoning_tokens, credit): (
        i64, i64, i64, i64, i64, i64, f64,
    ) = conn.query_row(
        &format!("SELECT {USAGE_AGG_COLS} FROM usage_records {filter}"),
        params![cutoff],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
    )?;
    let totals = UsageTotals {
        requests,
        input_tokens,
        cache_read_tokens,
        cache_write_tokens,
        output_tokens,
        reasoning_tokens,
        credit,
    };

    let mut by_agent: Vec<UsageAgentRow> = Vec::new();
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT agent, {USAGE_AGG_COLS} FROM usage_records {filter} GROUP BY agent"
        ))?;
        let rows = stmt.query_map(params![cutoff], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, f64>(7)?,
            ))
        })?;
        for row in rows.flatten() {
            let (agent, req, inp, cr, cw, out, reason, credit) = row;
            by_agent.push(UsageAgentRow {
                label: source_to_label_and_color(&agent).0.to_string(),
                agent,
                requests: req,
                input_tokens: inp,
                cache_read_tokens: cr,
                cache_write_tokens: cw,
                output_tokens: out,
                reasoning_tokens: reason,
                credit,
            });
        }
    }
    by_agent.sort_by(|a, b| {
        let ta = a.input_tokens + a.cache_read_tokens + a.cache_write_tokens + a.output_tokens;
        let tb = b.input_tokens + b.cache_read_tokens + b.cache_write_tokens + b.output_tokens;
        tb.cmp(&ta).then(a.agent.cmp(&b.agent))
    });

    let mut by_model: Vec<UsageModelRow> = Vec::new();
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT agent, COALESCE(model, ''), {USAGE_AGG_COLS} \
             FROM usage_records {filter} GROUP BY agent, COALESCE(model, '')"
        ))?;
        let rows = stmt.query_map(params![cutoff], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, f64>(8)?,
            ))
        })?;
        for row in rows.flatten() {
            let (agent, model, req, inp, cr, cw, out, reason, credit) = row;
            by_model.push(UsageModelRow {
                agent,
                model,
                requests: req,
                input_tokens: inp,
                cache_read_tokens: cr,
                cache_write_tokens: cw,
                output_tokens: out,
                reasoning_tokens: reason,
                credit,
            });
        }
    }
    by_model.sort_by(|a, b| {
        let ta = a.input_tokens + a.cache_read_tokens + a.cache_write_tokens + a.output_tokens;
        let tb = b.input_tokens + b.cache_read_tokens + b.cache_write_tokens + b.output_tokens;
        tb.cmp(&ta).then((a.agent.clone(), a.model.clone()).cmp(&(b.agent.clone(), b.model.clone())))
    });

    // 项目/会话展示名：优先 workspaces 里用户改过的显示名，否则取路径末段
    let ws_display = |path: &str, display: &str| -> String {
        let d = display.trim();
        if !d.is_empty() {
            return d.to_string();
        }
        if path.is_empty() {
            return "—".to_string();
        }
        std::path::Path::new(path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string())
    };

    let mut by_project: Vec<UsageProjectRow> = Vec::new();
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT c.workspace_path, COALESCE(w.display_name, ''), {USAGE_AGG_COLS} \
             FROM usage_records u \
             JOIN conversations c ON c.id = u.conversation_id \
             LEFT JOIN workspaces w ON w.workspace_path = c.workspace_path \
             {filter} AND u.occurred_at IS NOT NULL \
             GROUP BY c.workspace_path"
        ))?;
        let rows = stmt.query_map(params![cutoff], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, f64>(8)?,
            ))
        })?;
        for row in rows.flatten() {
            let (path, display, req, inp, cr, cw, out, reason, credit) = row;
            by_project.push(UsageProjectRow {
                project: ws_display(&path, &display),
                workspace_path: path,
                requests: req,
                input_tokens: inp,
                cache_read_tokens: cr,
                cache_write_tokens: cw,
                output_tokens: out,
                reasoning_tokens: reason,
                credit,
            });
        }
    }
    by_project.sort_by(|a, b| {
        let ta = a.input_tokens + a.cache_read_tokens + a.cache_write_tokens + a.output_tokens;
        let tb = b.input_tokens + b.cache_read_tokens + b.cache_write_tokens + b.output_tokens;
        tb.cmp(&ta).then(a.workspace_path.cmp(&b.workspace_path))
    });
    by_project.truncate(20);

    let mut by_session: Vec<UsageSessionRow> = Vec::new();
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT u.conversation_id, COALESCE(c.title, ''), c.workspace_path, COALESCE(w.display_name, ''), {USAGE_AGG_COLS} \
             FROM usage_records u \
             JOIN conversations c ON c.id = u.conversation_id \
             LEFT JOIN workspaces w ON w.workspace_path = c.workspace_path \
             {filter} AND u.occurred_at IS NOT NULL \
             GROUP BY u.conversation_id"
        ))?;
        let rows = stmt.query_map(params![cutoff], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, i64>(8)?,
                r.get::<_, i64>(9)?,
                r.get::<_, f64>(10)?,
            ))
        })?;
        for row in rows.flatten() {
            let (cid, title, path, display, req, inp, cr, cw, out, reason, credit) = row;
            by_session.push(UsageSessionRow {
                conversation_id: cid,
                session: title,
                project: ws_display(&path, &display),
                requests: req,
                input_tokens: inp,
                cache_read_tokens: cr,
                cache_write_tokens: cw,
                output_tokens: out,
                reasoning_tokens: reason,
                credit,
            });
        }
    }
    by_session.sort_by(|a, b| {
        let ta = a.input_tokens + a.cache_read_tokens + a.cache_write_tokens + a.output_tokens;
        let tb = b.input_tokens + b.cache_read_tokens + b.cache_write_tokens + b.output_tokens;
        tb.cmp(&ta).then(a.conversation_id.cmp(&b.conversation_id))
    });
    by_session.truncate(20);

    let mut by_day: Vec<UsageDayRow> = Vec::new();
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT date(occurred_at, 'localtime'), {USAGE_AGG_COLS} \
             FROM usage_records {filter} AND occurred_at IS NOT NULL \
             GROUP BY date(occurred_at, 'localtime') ORDER BY date(occurred_at, 'localtime') DESC"
        ))?;
        let rows = stmt.query_map(params![cutoff], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, f64>(7)?,
            ))
        })?;
        for row in rows.flatten() {
            let (date, req, inp, cr, cw, out, reason, credit) = row;
            by_day.push(UsageDayRow {
                date,
                requests: req,
                input_tokens: inp,
                cache_read_tokens: cr,
                cache_write_tokens: cw,
                output_tokens: out,
                reasoning_tokens: reason,
                credit,
            });
        }
    }

    let mut agents_without_usage = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT source_app FROM conversations \
             WHERE source_app IS NOT NULL AND source_app != '' \
               AND source_app NOT IN (SELECT DISTINCT agent FROM usage_records) \
             ORDER BY source_app",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        for app in rows.flatten() {
            agents_without_usage.push(source_to_label_and_color(&app).0.to_string());
        }
    }

    Ok(UsageStatsPayload {
        days: effective_days,
        totals,
        by_agent,
        by_model,
        by_project,
        by_session,
        by_day,
        agents_without_usage,
    })
}

pub fn fetch_dashboard_stats(conn: &Connection) -> Result<DashboardStats> {
    // 1. KPI 基础统计
    let total_conversations: i64 = conn
        .query_row("SELECT COUNT(*) FROM conversations", [], |r| r.get(0))
        .unwrap_or(0);

    let total_messages: i64 = conn.query_row(
        "SELECT COALESCE(SUM(message_count), (SELECT COUNT(*) FROM messages)) FROM conversations",
        [],
        |r| r.get(0),
    ).unwrap_or(0);

    let total_user_messages: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(user_message_count), 0) FROM conversations",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let total_workspaces: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT workspace_path) FROM conversations WHERE workspace_path IS NOT NULL AND workspace_path != ''",
        [],
        |r| r.get(0),
    ).unwrap_or(0);

    let starred_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM starred_sessions", [], |r| r.get(0))
        .unwrap_or(0);

    // 2. Agent 平台分布（按会话数）
    let mut agent_comparison_convs = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT
            CASE
                WHEN source_types LIKE '%claude%' THEN 'claude'
                WHEN source_types LIKE '%cursor%' THEN 'cursor'
                WHEN source_types LIKE '%codex%' THEN 'codex'
                WHEN source_types LIKE '%workbuddy%' THEN 'workbuddy'
                WHEN source_types LIKE '%hermes%' THEN 'hermes'
                WHEN source_types LIKE '%mimo%' THEN 'mimo'
                WHEN source_types LIKE '%windsurf%' THEN 'windsurf'
                WHEN source_types LIKE '%codebuddy%' THEN 'codebuddy'
                WHEN source_types LIKE '%qoder%' THEN 'qoder'
                ELSE 'antigravity'
            END as app,
            COUNT(*) as cnt
         FROM conversations
         GROUP BY app
         ORDER BY cnt DESC",
    )?;
    let conv_rows = stmt.query_map([], |row| {
        let app: String = row.get(0)?;
        let cnt: i64 = row.get(1)?;
        Ok((app, cnt))
    })?;
    for item in conv_rows.flatten() {
        let (app, count) = item;
        let (label, color) = source_to_label_and_color(&app);
        let percent = if total_conversations > 0 {
            (count as f64 / total_conversations as f64 * 1000.0).round() / 10.0
        } else {
            0.0
        };
        agent_comparison_convs.push(AgentShare {
            app,
            label: label.to_string(),
            count,
            percent,
            color: color.to_string(),
        });
    }

    // 3. Agent 平台分布（按消息数）
    let mut agent_comparison_msgs = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT
            CASE
                WHEN source_types LIKE '%claude%' THEN 'claude'
                WHEN source_types LIKE '%cursor%' THEN 'cursor'
                WHEN source_types LIKE '%codex%' THEN 'codex'
                WHEN source_types LIKE '%workbuddy%' THEN 'workbuddy'
                WHEN source_types LIKE '%hermes%' THEN 'hermes'
                WHEN source_types LIKE '%mimo%' THEN 'mimo'
                WHEN source_types LIKE '%windsurf%' THEN 'windsurf'
                WHEN source_types LIKE '%codebuddy%' THEN 'codebuddy'
                WHEN source_types LIKE '%qoder%' THEN 'qoder'
                ELSE 'antigravity'
            END as app,
            SUM(message_count) as cnt
         FROM conversations
         GROUP BY app
         ORDER BY cnt DESC",
    )?;
    let msg_rows = stmt.query_map([], |row| {
        let app: String = row.get(0)?;
        let cnt: i64 = row.get(1).unwrap_or(0);
        Ok((app, cnt))
    })?;
    for item in msg_rows.flatten() {
        let (app, count) = item;
        let (label, color) = source_to_label_and_color(&app);
        let percent = if total_messages > 0 {
            (count as f64 / total_messages as f64 * 1000.0).round() / 10.0
        } else {
            0.0
        };
        agent_comparison_msgs.push(AgentShare {
            app,
            label: label.to_string(),
            count,
            percent,
            color: color.to_string(),
        });
    }

    // 4. 24 小时活跃时段（按消息数 & 按会话数，统一以北京时间 UTC+8 统计，利用 SQLite 内核毫秒级聚合）
    let mut hourly_msgs = vec![0i64; 24];
    let mut stmt = conn.prepare(
        "SELECT CAST(strftime('%H', datetime(created_at, '+8 hours')) AS INTEGER) as h, COUNT(*)
         FROM messages
         WHERE created_at IS NOT NULL AND created_at != '' AND datetime(created_at, '+8 hours') IS NOT NULL
         GROUP BY h",
    )?;
    let msg_time_rows = stmt.query_map([], |row| {
        let h: i64 = row.get(0)?;
        let cnt: i64 = row.get(1)?;
        Ok((h as usize, cnt))
    })?;
    for r in msg_time_rows.flatten() {
        if r.0 < 24 {
            hourly_msgs[r.0] = r.1;
        }
    }

    let mut hourly_convs = vec![0i64; 24];
    let mut stmt = conn.prepare(
        "SELECT CAST(strftime('%H', datetime(COALESCE(created_at, updated_at), '+8 hours')) AS INTEGER) as h, COUNT(*)
         FROM conversations
         WHERE COALESCE(created_at, updated_at) IS NOT NULL 
           AND COALESCE(created_at, updated_at) != ''
           AND datetime(COALESCE(created_at, updated_at), '+8 hours') IS NOT NULL
         GROUP BY h",
    )?;
    let conv_time_rows = stmt.query_map([], |row| {
        let h: i64 = row.get(0)?;
        let cnt: i64 = row.get(1)?;
        Ok((h as usize, cnt))
    })?;
    for r in conv_time_rows.flatten() {
        if r.0 < 24 {
            hourly_convs[r.0] = r.1;
        }
    }

    let max_msg_hour = *hourly_msgs.iter().max().unwrap_or(&1).max(&1) as f64;
    let punchcard_msgs: Vec<PunchcardSlot> = hourly_msgs
        .iter()
        .enumerate()
        .map(|(h, &cnt)| {
            let ratio = cnt as f64 / max_msg_hour;
            let level = if cnt == 0 {
                0
            } else if ratio < 0.25 {
                1
            } else if ratio < 0.55 {
                2
            } else if ratio < 0.85 {
                3
            } else {
                4
            };
            PunchcardSlot {
                hour: h as u32,
                count: cnt,
                level,
                percent: (ratio * 100.0).round(),
            }
        })
        .collect();

    let max_conv_hour = *hourly_convs.iter().max().unwrap_or(&1).max(&1) as f64;
    let punchcard_convs: Vec<PunchcardSlot> = hourly_convs
        .iter()
        .enumerate()
        .map(|(h, &cnt)| {
            let ratio = cnt as f64 / max_conv_hour;
            let level = if cnt == 0 {
                0
            } else if ratio < 0.25 {
                1
            } else if ratio < 0.55 {
                2
            } else if ratio < 0.85 {
                3
            } else {
                4
            };
            PunchcardSlot {
                hour: h as u32,
                count: cnt,
                level,
                percent: (ratio * 100.0).round(),
            }
        })
        .collect();

    // 4.5 按日 24 小时 + 近 30 天柱状图（统一按北京时间 UTC+8 自然日/小时切分）
    let beijing_tz = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
    let today_bj = Utc::now().with_timezone(&beijing_tz).date_naive();
    let start_30 = today_bj - chrono::Duration::days(29);

    let (hourly_msg_by_day, last30_msg_counts) = collect_timestamp_volume(
        conn,
        r#"
        SELECT
            strftime('%Y-%m-%d', datetime(created_at, '+8 hours')),
            CAST(strftime('%H', datetime(created_at, '+8 hours')) AS INTEGER),
            COUNT(*)
        FROM messages
        WHERE created_at IS NOT NULL AND created_at != ''
          AND created_at >= ?3
          AND datetime(created_at, '+8 hours') >= ?1
          AND datetime(created_at, '+8 hours') < ?2
        GROUP BY 1, 2
        "#,
        start_30,
        today_bj,
    )?;
    let (hourly_conv_by_day, _) = collect_timestamp_volume(
        conn,
        r#"
        SELECT
            strftime('%Y-%m-%d', datetime(created_at, '+8 hours')),
            CAST(strftime('%H', datetime(created_at, '+8 hours')) AS INTEGER),
            COUNT(DISTINCT conversation_id)
        FROM messages
        WHERE created_at IS NOT NULL AND created_at != ''
          AND created_at >= ?3
          AND datetime(created_at, '+8 hours') >= ?1
          AND datetime(created_at, '+8 hours') < ?2
        GROUP BY 1, 2
        "#,
        start_30,
        today_bj,
    )?;
    let (hourly_user_msg_by_day, last30_user_msg_counts) = collect_timestamp_volume(
        conn,
        r#"
        SELECT
            strftime('%Y-%m-%d', datetime(created_at, '+8 hours')),
            CAST(strftime('%H', datetime(created_at, '+8 hours')) AS INTEGER),
            COUNT(*)
        FROM messages
        WHERE created_at IS NOT NULL AND created_at != ''
          AND created_at >= ?3
          AND (role LIKE '%user%' OR role = 'user')
          AND datetime(created_at, '+8 hours') >= ?1
          AND datetime(created_at, '+8 hours') < ?2
        GROUP BY 1, 2
        "#,
        start_30,
        today_bj,
    )?;
    let (_, last30_conv_counts) = collect_timestamp_volume(
        conn,
        r#"
        SELECT
            strftime('%Y-%m-%d', datetime(created_at, '+8 hours')),
            0,
            COUNT(DISTINCT conversation_id)
        FROM messages
        WHERE created_at IS NOT NULL AND created_at != ''
          AND created_at >= ?3
          AND datetime(created_at, '+8 hours') >= ?1
          AND datetime(created_at, '+8 hours') < ?2
        GROUP BY 1
        "#,
        start_30,
        today_bj,
    )?;
    let last30_hourly_msgs = slots_from_hourly_days(start_30, &hourly_msg_by_day);
    let last30_hourly_convs = slots_from_hourly_days(start_30, &hourly_conv_by_day);
    let last30_hourly_user_msgs = slots_from_hourly_days(start_30, &hourly_user_msg_by_day);
    let last30_daily_msgs = slots_from_daily(start_30, &last30_msg_counts);
    let last30_daily_convs = slots_from_daily(start_30, &last30_conv_counts);
    let last30_daily_user_msgs = slots_from_daily(start_30, &last30_user_msg_counts);

    // 5. Tool Usage 工具调用分布分析 (SQL 快速聚合)
    let mut total_tool_calls = 0i64;
    let mut tool_categories: HashMap<String, i64> = HashMap::new();

    let mut stmt = conn.prepare(
        r#"
        SELECT 
            CASE 
                WHEN LOWER(tool_name) LIKE '%read%' OR LOWER(tool_name) LIKE '%view%' OR LOWER(tool_name) LIKE '%list%' OR LOWER(tool_name) LIKE '%cat%' THEN '文件阅读'
                WHEN LOWER(tool_name) LIKE '%edit%' OR LOWER(tool_name) LIKE '%write%' OR LOWER(tool_name) LIKE '%replace%' OR LOWER(tool_name) LIKE '%patch%' THEN '代码编辑'
                WHEN LOWER(tool_name) LIKE '%bash%' OR LOWER(tool_name) LIKE '%cmd%' OR LOWER(tool_name) LIKE '%terminal%' OR LOWER(tool_name) LIKE '%run%' OR LOWER(tool_name) LIKE '%exec%' THEN '终端命令'
                WHEN LOWER(tool_name) LIKE '%search%' OR LOWER(tool_name) LIKE '%grep%' OR LOWER(tool_name) LIKE '%find%' OR LOWER(tool_name) LIKE '%query%' THEN '搜索检索'
                WHEN LOWER(tool_name) LIKE '%skill%' OR LOWER(tool_name) LIKE '%mcp%' OR LOWER(tool_name) LIKE '%plugin%' OR LOWER(tool_name) LIKE '%image%' OR LOWER(tool_name) LIKE '%schedule%' OR LOWER(tool_name) LIKE '%task%' THEN '技能扩展'
                WHEN LOWER(tool_name) LIKE '%browser%' OR LOWER(tool_name) LIKE '%web%' OR LOWER(tool_name) LIKE '%http%' OR LOWER(tool_name) LIKE '%fetch%' OR LOWER(tool_name) LIKE '%url%' THEN '网络与浏览器'
                ELSE '其他工具'
            END as cat,
            COUNT(*) as cnt
        FROM messages
        WHERE tool_name IS NOT NULL AND tool_name != ''
        GROUP BY cat
        "#,
    )?;
    let tool_rows = stmt.query_map([], |row| {
        let cat: String = row.get(0)?;
        let cnt: i64 = row.get(1)?;
        Ok((cat, cnt))
    })?;
    for r in tool_rows.flatten() {
        total_tool_calls += r.1;
        tool_categories.insert(r.0, r.1);
    }

    let mut tool_usage = Vec::new();
    let category_colors = [
        ("文件阅读", "#3b82f6"),
        ("代码编辑", "#10b981"),
        ("终端命令", "#f59e0b"),
        ("搜索检索", "#8b5cf6"),
        ("技能扩展", "#ec4899"),
        ("网络与浏览器", "#06b6d4"),
        ("其他工具", "#64748b"),
    ];
    for (cat, col) in category_colors {
        let cnt = *tool_categories.get(cat).unwrap_or(&0);
        if cnt > 0 || total_tool_calls > 0 {
            let percent = if total_tool_calls > 0 {
                (cnt as f64 / total_tool_calls as f64 * 1000.0).round() / 10.0
            } else {
                0.0
            };
            tool_usage.push(ToolUsageStat {
                category: cat.to_string(),
                count: cnt,
                percent,
                color: col.to_string(),
            });
        }
    }

    // 6. Top 10 深度会话排行榜
    let get_top_convs = |order_by: &str| -> Result<Vec<TopRankItem>> {
        let mut list = Vec::new();
        let query = format!(
            "SELECT c.id,
                    COALESCE(NULLIF(TRIM(ai.ai_title), ''), c.title) as title,
                    CASE
                        WHEN c.source_types LIKE '%claude%' THEN 'claude'
                        WHEN c.source_types LIKE '%cursor%' THEN 'cursor'
                        WHEN c.source_types LIKE '%codex%' THEN 'codex'
                        WHEN c.source_types LIKE '%workbuddy%' THEN 'workbuddy'
                        WHEN c.source_types LIKE '%hermes%' THEN 'hermes'
                        WHEN c.source_types LIKE '%mimo%' THEN 'mimo'
                        WHEN c.source_types LIKE '%windsurf%' THEN 'windsurf'
                        WHEN c.source_types LIKE '%codebuddy%' THEN 'codebuddy'
                        WHEN c.source_types LIKE '%qoder%' THEN 'qoder'
                        ELSE 'antigravity'
                    END as source_app,
                    c.workspace_path, c.message_count, c.user_message_count, c.updated_at,
                    (SELECT COUNT(*) FROM starred_sessions s WHERE s.conversation_id = c.id) as is_starred
             FROM conversations c
             LEFT JOIN conversation_ai ai ON ai.conversation_id = c.id
             ORDER BY {} DESC LIMIT 10",
            order_by
        );
        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let title: String = row.get(1)?;
            let source_app: String = row.get(2)?;
            let workspace_path: String = row.get(3)?;
            let message_count: i64 = row.get(4)?;
            let user_message_count: i64 = row.get(5)?;
            let raw_updated: Option<String> = row.get(6)?;
            let is_starred_cnt: i64 = row.get(7)?;
            let (label, _) = source_to_label_and_color(&source_app);
            let ws_short = get_short_workspace(&workspace_path);
            Ok(TopRankItem {
                id,
                title: if title.is_empty() {
                    "未命名会话".to_string()
                } else {
                    title
                },
                source_app,
                source_label: label.to_string(),
                workspace_path,
                workspace_short: ws_short,
                message_count,
                user_message_count,
                updated_at: to_beijing_iso(raw_updated),
                is_starred: is_starred_cnt > 0,
            })
        })?;
        for r in rows.flatten() {
            list.push(r);
        }
        Ok(list)
    };

    let top_conversations_all = get_top_convs("c.message_count")?;
    let top_conversations_user = get_top_convs("c.user_message_count")?;

    // 7. Top 8 热门项目工作区分布
    let mut top_workspaces = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT workspace_path, COUNT(*) as cnt, SUM(message_count) as total_msg, SUM(user_message_count) as total_user
         FROM conversations
         WHERE workspace_path IS NOT NULL AND workspace_path != ''
         GROUP BY workspace_path
         ORDER BY cnt DESC
         LIMIT 8"
    )?;
    let ws_rows = stmt.query_map([], |row| {
        let path: String = row.get(0)?;
        let count: i64 = row.get(1)?;
        let message_count: i64 = row.get(2).unwrap_or(0);
        let user_message_count: i64 = row.get(3).unwrap_or(0);
        let percent = if total_conversations > 0 {
            (count as f64 / total_conversations as f64 * 1000.0).round() / 10.0
        } else {
            0.0
        };
        let short_name = get_short_workspace(&path);
        Ok(TopWorkspaceItem {
            path,
            short_name,
            count,
            message_count,
            user_message_count,
            percent,
        })
    })?;
    for r in ws_rows.flatten() {
        top_workspaces.push(r);
    }

    // 5.5 全景 365 天日历热力图（按北京时间自然日，与柱状图一致）
    let mut heatmap_cells = Vec::new();
    let mut heatmap_cells_convs = Vec::new();
    let mut heatmap_cells_user = Vec::new();
    let start_date = today_bj - chrono::Duration::days(364);

    let mut day_msg_counts: HashMap<String, i64> = HashMap::new();
    let mut day_user_msg_counts: HashMap<String, i64> = HashMap::new();
    let mut stmt_hm_msg = conn.prepare(
        "SELECT strftime('%Y-%m-%d', datetime(m.created_at, '+8 hours')) as d,
                COUNT(*),
                COUNT(CASE WHEN m.role = 'user' THEN 1 END)
         FROM messages m
         WHERE m.created_at IS NOT NULL AND m.created_at != ''
           AND datetime(m.created_at, '+8 hours') IS NOT NULL
         GROUP BY d",
    )?;
    let hm_msg_rows = stmt_hm_msg.query_map([], |r| {
        let d: String = r.get(0)?;
        let total_c: i64 = r.get(1)?;
        let user_c: i64 = r.get(2)?;
        Ok((d, total_c, user_c))
    })?;
    for r in hm_msg_rows.flatten() {
        day_msg_counts.insert(r.0.clone(), r.1);
        day_user_msg_counts.insert(r.0, r.2);
    }

    let mut day_conv_counts: HashMap<String, i64> = HashMap::new();
    let mut stmt_hm_conv = conn.prepare(
        "SELECT strftime('%Y-%m-%d', datetime(COALESCE(c.created_at, c.updated_at), '+8 hours')) as d, COUNT(*)
         FROM conversations c
         WHERE COALESCE(c.created_at, c.updated_at) IS NOT NULL
           AND COALESCE(c.created_at, c.updated_at) != ''
           AND datetime(COALESCE(c.created_at, c.updated_at), '+8 hours') IS NOT NULL
         GROUP BY d"
    )?;
    let hm_conv_rows = stmt_hm_conv.query_map([], |r| {
        let d: String = r.get(0)?;
        let c: i64 = r.get(1)?;
        Ok((d, c))
    })?;
    for r in hm_conv_rows.flatten() {
        day_conv_counts.insert(r.0, r.1);
    }

    let mut heatmap_active_days = 0i64;
    let mut heatmap_longest_streak = 0i64;
    let mut current_streak = 0i64;
    let mut heatmap_peak_day: Option<String> = None;
    let mut heatmap_peak_count = 0i64;

    for i in 0..365 {
        let curr = start_date + chrono::Duration::days(i);
        let date_str = curr.format("%Y-%m-%d").to_string();
        let msg_cnt = *day_msg_counts.get(&date_str).unwrap_or(&0);
        let user_msg_cnt = *day_user_msg_counts.get(&date_str).unwrap_or(&0);
        let conv_cnt = *day_conv_counts.get(&date_str).unwrap_or(&0);

        if msg_cnt > 0 {
            heatmap_active_days += 1;
            current_streak += 1;
            if current_streak > heatmap_longest_streak {
                heatmap_longest_streak = current_streak;
            }
            if msg_cnt > heatmap_peak_count {
                heatmap_peak_count = msg_cnt;
                heatmap_peak_day = Some(date_str.clone());
            }
        } else {
            current_streak = 0;
        }

        let level_msg = match msg_cnt {
            0 => 0,
            1..=10 => 1,
            11..=40 => 2,
            41..=100 => 3,
            _ => 4,
        };
        let level_user_msg = match user_msg_cnt {
            0 => 0,
            1..=3 => 1,
            4..=10 => 2,
            11..=25 => 3,
            _ => 4,
        };
        let level_conv = match conv_cnt {
            0 => 0,
            1..=2 => 1,
            3..=5 => 2,
            6..=12 => 3,
            _ => 4,
        };

        heatmap_cells.push(HeatmapCell {
            date: date_str.clone(),
            count: msg_cnt,
            level: level_msg,
            user_count: user_msg_cnt,
            total_messages: msg_cnt,
            conv_count: conv_cnt,
        });
        heatmap_cells_user.push(HeatmapCell {
            date: date_str.clone(),
            count: user_msg_cnt,
            level: level_user_msg,
            user_count: user_msg_cnt,
            total_messages: msg_cnt,
            conv_count: conv_cnt,
        });
        heatmap_cells_convs.push(HeatmapCell {
            date: date_str,
            count: conv_cnt,
            level: level_conv,
            user_count: user_msg_cnt,
            total_messages: msg_cnt,
            conv_count: conv_cnt,
        });
    }

    let raw_sync: Option<String> = conn
        .query_row(
            "SELECT COALESCE(finished_at, created_at) FROM sync_runs ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .ok();
    let last_sync_time = to_beijing_iso(raw_sync);

    Ok(DashboardStats {
        total_conversations,
        total_messages,
        total_user_messages,
        total_workspaces,
        starred_count,
        total_tool_calls,
        agent_comparison_convs,
        agent_comparison_msgs,
        punchcard_msgs,
        punchcard_convs,
        last30_hourly_msgs,
        last30_hourly_convs,
        last30_hourly_user_msgs,
        last30_daily_msgs,
        last30_daily_convs,
        last30_daily_user_msgs,
        heatmap_cells,
        heatmap_cells_convs,
        heatmap_cells_user,
        heatmap_active_days,
        heatmap_longest_streak,
        heatmap_peak_day,
        heatmap_peak_count,
        tool_usage,
        top_conversations_all,
        top_conversations_user,
        top_workspaces,
        last_sync_time,
        beijing_today: today_bj.format("%Y-%m-%d").to_string(),
    })
}

pub fn fetch_workspaces(
    conn: &Connection,
    search: Option<&str>,
    date: Option<&str>,
) -> Result<Vec<WorkspaceStat>> {
    let target_date = match date {
        Some("today") => Some(
            chrono::Utc::now()
                .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
                .format("%Y-%m-%d")
                .to_string(),
        ),
        Some(d) if !d.trim().is_empty() => Some(d.trim().to_string()),
        _ => None,
    };

    let mut list = Vec::new();
    let sql = r#"
        SELECT
            workspace_path,
            COUNT(*) as cnt,
            SUM(CASE WHEN COALESCE(source_app, '') = 'antigravity'
                OR source_types LIKE '%antigravity%'
                OR source_types LIKE '%transcript%'
                OR source_types LIKE '%sqlite_db%'
                OR source_types LIKE '%overview%'
                OR (
                    (source_app IS NULL OR source_app = '')
                    AND (source_types IS NULL OR source_types = '' OR source_types = '[]')
                    AND id NOT LIKE '%:%'
                )
                THEN 1 ELSE 0 END) as ag_cnt,
            SUM(CASE WHEN COALESCE(source_app, '') = 'cursor' OR source_types LIKE '%cursor%' THEN 1 ELSE 0 END) as cursor_cnt,
            SUM(CASE WHEN COALESCE(source_app, '') = 'claude' OR source_types LIKE '%claude%' THEN 1 ELSE 0 END) as claude_cnt,
            SUM(CASE WHEN COALESCE(source_app, '') = 'codex' OR source_types LIKE '%codex%' THEN 1 ELSE 0 END) as codex_cnt,
            SUM(CASE WHEN COALESCE(source_app, '') = 'workbuddy' OR source_types LIKE '%workbuddy%' THEN 1 ELSE 0 END) as wb_cnt,
            SUM(CASE WHEN COALESCE(source_app, '') = 'hermes' OR source_types LIKE '%hermes%' THEN 1 ELSE 0 END) as hermes_cnt,
            SUM(CASE WHEN COALESCE(source_app, '') = 'mimo' OR source_types LIKE '%mimo%' THEN 1 ELSE 0 END) as mimo_cnt,
            SUM(CASE WHEN COALESCE(source_app, '') = 'windsurf' OR source_types LIKE '%windsurf%' THEN 1 ELSE 0 END) as windsurf_cnt,
            SUM(CASE WHEN COALESCE(source_app, '') = 'codebuddy' OR source_types LIKE '%codebuddy%' THEN 1 ELSE 0 END) as codebuddy_cnt,
            SUM(CASE WHEN COALESCE(source_app, '') = 'qoder' OR source_types LIKE '%qoder%' THEN 1 ELSE 0 END) as qoder_cnt,
            SUM(message_count) as message_count,
            SUM(user_message_count) as user_message_count,
            MAX(updated_at) as last_updated
        FROM conversations
        WHERE (?1 IS NULL OR ?1 = '' OR workspace_path LIKE '%' || ?1 || '%')
          AND (?2 IS NULL OR ?2 = '' OR strftime('%Y-%m-%d', datetime(COALESCE(created_at, updated_at), '+8 hours')) = ?2)
        GROUP BY workspace_path
        ORDER BY last_updated DESC
    "#;

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(
        params![search.unwrap_or(""), target_date.as_deref().unwrap_or("")],
        |row| {
            Ok(WorkspaceStat {
                workspace_path: row.get(0)?,
                cnt: row.get(1)?,
                ag_cnt: row.get(2).unwrap_or(0),
                cursor_cnt: row.get(3).unwrap_or(0),
                claude_cnt: row.get(4).unwrap_or(0),
                codex_cnt: row.get(5).unwrap_or(0),
                wb_cnt: row.get(6).unwrap_or(0),
                hermes_cnt: row.get(7).unwrap_or(0),
                mimo_cnt: row.get(8).unwrap_or(0),
                windsurf_cnt: row.get(9).unwrap_or(0),
                codebuddy_cnt: row.get(10).unwrap_or(0),
                qoder_cnt: row.get(11).unwrap_or(0),
                message_count: row.get(12).unwrap_or(0),
                user_message_count: row.get(13).unwrap_or(0),
                last_updated: to_beijing_iso(row.get(14)?),
            })
        },
    )?;

    for r in rows.flatten() {
        list.push(r);
    }
    Ok(list)
}

/// 解析工作区别名链（old → new → newer…）；无别名时原样返回。
/// 迭代上限防御异常数据成环。
pub fn resolve_workspace_alias(conn: &Connection, path: &str) -> Result<String> {
    let mut current = path.to_string();
    for _ in 0..16 {
        let next: Option<String> = match conn.query_row(
            "SELECT new_path FROM workspace_aliases WHERE old_path = ?1",
            params![&current],
            |r| r.get(0),
        ) {
            Ok(v) => Some(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(e),
        };
        match next {
            Some(n) if n != current => current = n,
            _ => break,
        }
    }
    Ok(current)
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceMergeResult {
    pub source_path: String,
    pub target_path: String,
    pub moved_conversations: usize,
    pub moved_analysis_blocks: usize,
    /// true = 目标已有 AI 分析，保留目标并丢弃源侧产物
    pub kept_target_analysis: bool,
}

/// 合并 / 重命名工作区：把 source 的全部会话与 AI 分析产物并入 target，
/// 并写入持久别名，保证后续同步导入旧路径时自动落到 target。
pub fn merge_workspace(
    conn: &Connection,
    source_path: &str,
    target_path: &str,
) -> Result<WorkspaceMergeResult> {
    use crate::importers::canonicalize_workspace_path;

    let source = canonicalize_workspace_path(source_path);
    let mut target = canonicalize_workspace_path(target_path);
    if source.is_empty() || target.is_empty() {
        return Err(rusqlite::Error::InvalidParameterName(
            "workspace path cannot be empty".into(),
        ));
    }

    // target 自身可能已是别名（曾被合并过），解析到最终真实路径
    target = resolve_workspace_alias(conn, &target)?;
    if source == target {
        return Err(rusqlite::Error::InvalidParameterName(
            "source and target workspace are the same".into(),
        ));
    }

    let source_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM conversations WHERE workspace_path = ?1",
        params![&source],
        |r| r.get(0),
    )?;
    if source_count == 0 {
        return Err(rusqlite::Error::InvalidParameterName(
            "source workspace has no conversations".into(),
        ));
    }

    let tx = conn.unchecked_transaction()?;

    let moved_conversations = tx.execute(
        "UPDATE conversations SET workspace_path = ?1 WHERE workspace_path = ?2",
        params![&target, &source],
    )? as usize;

    // AI 分析产物冲突策略：目标已有则保留目标（可重新生成），否则整体搬运
    let target_fine: i64 = tx.query_row(
        "SELECT COUNT(*) FROM workspace_blocks_fine WHERE workspace_path = ?1",
        params![&target],
        |r| r.get(0),
    )?;
    let target_modules: i64 = tx.query_row(
        "SELECT COUNT(*) FROM workspace_blocks_modules WHERE workspace_path = ?1",
        params![&target],
        |r| r.get(0),
    )?;
    let target_report: i64 = tx.query_row(
        "SELECT COUNT(*) FROM workspace_reports WHERE workspace_path = ?1",
        params![&target],
        |r| r.get(0),
    )?;
    let kept_target_analysis = target_fine > 0 || target_modules > 0 || target_report > 0;

    let mut moved_analysis_blocks = 0usize;
    if kept_target_analysis {
        tx.execute(
            "DELETE FROM workspace_blocks_fine WHERE workspace_path = ?1",
            params![&source],
        )?;
        tx.execute(
            "DELETE FROM workspace_blocks_modules WHERE workspace_path = ?1",
            params![&source],
        )?;
        tx.execute(
            "DELETE FROM workspace_reports WHERE workspace_path = ?1",
            params![&source],
        )?;
    } else {
        moved_analysis_blocks += tx.execute(
            "UPDATE workspace_blocks_fine SET workspace_path = ?1 WHERE workspace_path = ?2",
            params![&target, &source],
        )? as usize;
        moved_analysis_blocks += tx.execute(
            "UPDATE workspace_blocks_modules SET workspace_path = ?1 WHERE workspace_path = ?2",
            params![&target, &source],
        )? as usize;
        tx.execute(
            "UPDATE workspace_reports SET workspace_path = ?1 WHERE workspace_path = ?2",
            params![&target, &source],
        )?;
    }

    // 持久别名：同步导入旧路径时自动映射（链式别名在 resolve 时逐跳解析）
    let now = chrono::Utc::now().to_rfc3339();
    tx.execute(
        r#"
        INSERT INTO workspace_aliases (old_path, new_path, created_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(old_path) DO UPDATE SET
            new_path = excluded.new_path,
            created_at = excluded.created_at
        "#,
        params![&source, &target, &now],
    )?;

    // workspaces 表已退化为兼容残留，同步清理源路径
    tx.execute(
        "DELETE FROM workspaces WHERE workspace_path = ?1",
        params![&source],
    )?;

    tx.commit()?;

    Ok(WorkspaceMergeResult {
        source_path: source,
        target_path: target,
        moved_conversations,
        moved_analysis_blocks,
        kept_target_analysis,
    })
}

/// 读取会话级工作区覆盖（同步导入时以此为准）。无覆盖返回 None。
pub fn conversation_workspace_override(conn: &Connection, conversation_id: &str) -> Result<Option<String>> {
    match conn.query_row(
        "SELECT workspace_path FROM conversation_workspace_overrides WHERE conversation_id = ?1",
        params![conversation_id],
        |r| r.get(0),
    ) {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversationMoveResult {
    pub conversation_id: String,
    pub target_path: String,
}

/// 把单条会话移动到目标工作区，并写入覆盖记录，保证后续同步不回退。
pub fn move_conversation(
    conn: &Connection,
    conversation_id: &str,
    target_path: &str,
) -> Result<ConversationMoveResult> {
    use crate::importers::canonicalize_workspace_path;

    let target = canonicalize_workspace_path(target_path);
    if target.is_empty() {
        return Err(rusqlite::Error::InvalidParameterName(
            "target workspace path cannot be empty".into(),
        ));
    }
    // 目标自身可能已被合并过，解析到最终真实路径
    let target = resolve_workspace_alias(conn, &target)?;

    let changed = conn.execute(
        "UPDATE conversations SET workspace_path = ?1 WHERE id = ?2",
        params![&target, conversation_id],
    )?;
    if changed == 0 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }

    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        r#"
        INSERT INTO conversation_workspace_overrides (conversation_id, workspace_path, updated_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(conversation_id) DO UPDATE SET
            workspace_path = excluded.workspace_path,
            updated_at = excluded.updated_at
        "#,
        params![conversation_id, &target, &now],
    )?;

    Ok(ConversationMoveResult {
        conversation_id: conversation_id.to_string(),
        target_path: target,
    })
}

pub fn fetch_conversations(
    conn: &Connection,
    workspace: Option<&str>,
    search: Option<&str>,
    starred_only: bool,
) -> Result<Vec<ConversationItem>> {
    let mut list = Vec::new();
    let sql = r#"
        SELECT
            c.id,
            c.workspace_path,
            CASE
                WHEN c.source_types LIKE '%claude%' THEN 'claude'
                WHEN c.source_types LIKE '%cursor%' THEN 'cursor'
                WHEN c.source_types LIKE '%codex%' THEN 'codex'
                WHEN c.source_types LIKE '%workbuddy%' THEN 'workbuddy'
                WHEN c.source_types LIKE '%hermes%' THEN 'hermes'
                WHEN c.source_types LIKE '%mimo%' THEN 'mimo'
                WHEN c.source_types LIKE '%windsurf%' THEN 'windsurf'
                WHEN c.source_types LIKE '%codebuddy%' THEN 'codebuddy'
                WHEN c.source_types LIKE '%qoder%' THEN 'qoder'
                ELSE 'antigravity'
            END as source_app,
            c.title as source_title,
            NULLIF(TRIM(COALESCE(ai.ai_title, '')), '') as ai_title,
            COALESCE(ai.summary, '') as ai_summary,
            COALESCE(ai.status, 'idle') as ai_status,
            ai.based_on_content_hash,
            ai.model as ai_model,
            ai.generated_at as ai_generated_at,
            COALESCE(c.content_hash, '') as content_hash,
            c.created_at,
            c.updated_at,
            c.message_count,
            c.user_message_count,
            c.parse_status,
            (SELECT COUNT(*) FROM starred_sessions s WHERE s.conversation_id = c.id) as is_starred,
            ai.based_on_message_count
        FROM conversations c
        LEFT JOIN conversation_ai ai ON ai.conversation_id = c.id
        WHERE (?1 = 0 OR (SELECT COUNT(*) FROM starred_sessions s WHERE s.conversation_id = c.id) > 0)
          AND (?2 IS NULL OR ?2 = '' OR c.workspace_path = ?2)
          AND (
            ?3 IS NULL OR ?3 = ''
            OR c.title LIKE '%' || ?3 || '%'
            OR COALESCE(ai.ai_title, '') LIKE '%' || ?3 || '%'
          )
        ORDER BY c.updated_at DESC
        LIMIT 200
    "#;

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(
        params![
            if starred_only { 1 } else { 0 },
            workspace.unwrap_or(""),
            search.unwrap_or("")
        ],
        |row| {
            let source_title: String = row.get(3)?;
            let ai_title: Option<String> = row.get(4)?;
            let based_on: Option<String> = row.get(7)?;
            let content_hash: String = row.get(10)?;
            let is_starred_cnt: i64 = row.get(16)?;
            let based_on_msg_count: Option<i64> = row.get(17)?;
            let msg_count: i64 = row.get(13)?;
            let ai_new_message_count = match based_on_msg_count {
                Some(b) if b > 0 => {
                    let diff = msg_count - b;
                    if diff > 0 { Some(diff) } else { None }
                }
                _ => None,
            };
            let raw_created: Option<String> = row.get(11)?;
            let raw_updated: Option<String> = row.get(12)?;
            let raw_ai_generated: Option<String> = row.get(9)?;
            let display_title = ai_title
                .as_ref()
                .filter(|t| !t.is_empty())
                .cloned()
                .unwrap_or_else(|| source_title.clone());
            let summary_stale = match (&based_on, content_hash.as_str()) {
                (Some(h), ch) if !h.is_empty() && !ch.is_empty() => h != ch,
                _ => false,
            };
            let ai_summary: String = row.get(5)?;
            Ok(ConversationItem {
                id: row.get(0)?,
                workspace_path: row.get(1)?,
                source_app: row.get(2)?,
                title: display_title,
                source_title,
                ai_title,
                ai_summary: if ai_summary.is_empty() {
                    None
                } else {
                    Some(ai_summary)
                },
                ai_status: Some(row.get::<_, String>(6)?),
                ai_summary_stale: summary_stale,
                ai_model: row.get(8)?,
                ai_generated_at: to_beijing_iso(raw_ai_generated),
                content_hash,
                created_at: to_beijing_iso(raw_created),
                updated_at: to_beijing_iso(raw_updated),
                message_count: msg_count,
                user_message_count: row.get(14)?,
                parse_status: row.get(15)?,
                is_starred: is_starred_cnt > 0,
                ai_new_message_count,
            })
        },
    )?;

    for r in rows.flatten() {
        list.push(r);
    }
    Ok(list)
}

pub fn fetch_conversation_messages(
    conn: &Connection,
    conversation_id: &str,
) -> Result<Vec<MessageItem>> {
    let mut list = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT id, conversation_id, step_index,
                CASE WHEN role LIKE '%user%' THEN 'user' WHEN role LIKE '%tool%' THEN 'tool' ELSE 'assistant' END as sender,
                content as text,
                thinking,
                created_at,
                COALESCE(model_name, source) as model_name,
                token_count,
                NULL as duration_ms,
                credit,
                CASE WHEN tool_name IS NOT NULL AND tool_name != '' THEN json_array(json_object('name', tool_name, 'args', tool_args)) ELSE NULL END as tool_calls_json,
                images
         FROM messages
         WHERE conversation_id = ?1
         ORDER BY step_index ASC, id ASC"
    )?;

    let rows = stmt.query_map(params![conversation_id], |row| {
        let id_val: i64 = row.get(0)?;
        let raw_created: Option<String> = row.get(6)?;
        Ok(MessageItem {
            id: id_val.to_string(),
            conversation_id: row.get(1)?,
            step_index: row.get(2)?,
            sender: row.get(3)?,
            text: row.get(4)?,
            thinking: row.get(5)?,
            created_at: to_beijing_iso(raw_created),
            model_name: row.get(7)?,
            token_count: row.get(8)?,
            duration_ms: row.get(9)?,
            tool_calls_json: row.get(11)?,
            images: row.get(12)?,
            credit: row.get(10)?,
        })
    })?;

    for r in rows.flatten() {
        list.push(r);
    }
    Ok(list)
}

pub fn toggle_star_session(conn: &Connection, conversation_id: &str) -> Result<bool> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM starred_sessions WHERE conversation_id = ?1",
            params![conversation_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    if count > 0 {
        conn.execute(
            "DELETE FROM starred_sessions WHERE conversation_id = ?1",
            params![conversation_id],
        )?;
        Ok(false)
    } else {
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO starred_sessions (conversation_id, starred_at) VALUES (?1, ?2)",
            params![conversation_id, now],
        )?;
        Ok(true)
    }
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

/// 手动重命名：只写 ai_title，不影响同步源 title
pub fn update_conversation_ai_title(
    conn: &Connection,
    conversation_id: &str,
    ai_title: &str,
) -> Result<ConversationItem> {
    let title = ai_title.trim();
    if title.is_empty() {
        return Err(rusqlite::Error::InvalidParameterName(
            "title cannot be empty".into(),
        ));
    }
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM conversations WHERE id = ?1",
        params![conversation_id],
        |r| r.get(0),
    )?;
    if exists == 0 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }

    let now = now_rfc3339();
    conn.execute(
        r#"
        INSERT INTO conversation_ai (conversation_id, ai_title, summary, status, updated_at)
        VALUES (?1, ?2, '', 'idle', ?3)
        ON CONFLICT(conversation_id) DO UPDATE SET
            ai_title = excluded.ai_title,
            updated_at = excluded.updated_at
        "#,
        params![conversation_id, title, now],
    )?;

    fetch_conversation_by_id(conn, conversation_id)?
        .ok_or(rusqlite::Error::QueryReturnedNoRows)
}

/// 保存 AI 总结结果（标题 + 摘要）
pub fn save_conversation_ai_summary(
    conn: &Connection,
    conversation_id: &str,
    ai_title: Option<&str>,
    summary: &str,
    status: &str,
    based_on_content_hash: Option<&str>,
    based_on_message_count: Option<i64>,
    model: Option<&str>,
    error: Option<&str>,
) -> Result<ConversationItem> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM conversations WHERE id = ?1",
        params![conversation_id],
        |r| r.get(0),
    )?;
    if exists == 0 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }

    let now = now_rfc3339();
    let title_opt = ai_title.map(|t| t.trim()).filter(|t| !t.is_empty());
    let generated_at = if status == "ok" {
        Some(now.as_str())
    } else {
        None
    };

    conn.execute(
        r#"
        INSERT INTO conversation_ai (
           conversation_id, ai_title, summary, status,
            based_on_content_hash, based_on_message_count, model, error, generated_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        ON CONFLICT(conversation_id) DO UPDATE SET
            ai_title = COALESCE(excluded.ai_title, conversation_ai.ai_title),
            summary = excluded.summary,
            status = excluded.status,
            based_on_content_hash = excluded.based_on_content_hash,
            based_on_message_count = excluded.based_on_message_count,
            model = COALESCE(excluded.model, conversation_ai.model),
            error = excluded.error,
            generated_at = COALESCE(excluded.generated_at, conversation_ai.generated_at),
            updated_at = excluded.updated_at
        "#,
        params![
            conversation_id,
            title_opt,
            summary,
            status,
           based_on_content_hash,
            based_on_message_count,
           model,
           error,
           generated_at,
           now,
       ],
   )?;

    fetch_conversation_by_id(conn, conversation_id)?
        .ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub fn set_conversation_ai_status(
    conn: &Connection,
    conversation_id: &str,
    status: &str,
    error: Option<&str>,
) -> Result<()> {
    let now = now_rfc3339();
    conn.execute(
        r#"
        INSERT INTO conversation_ai (conversation_id, ai_title, summary, status, error, updated_at)
        VALUES (?1, NULL, '', ?2, ?3, ?4)
        ON CONFLICT(conversation_id) DO UPDATE SET
            status = excluded.status,
            error = excluded.error,
            updated_at = excluded.updated_at
        "#,
        params![conversation_id, status, error, now],
    )?;
    Ok(())
}

pub fn clear_conversation_ai_title(conn: &Connection, conversation_id: &str) -> Result<ConversationItem> {
    conn.execute(
        r#"
        UPDATE conversation_ai
        SET ai_title = NULL, updated_at = ?2
        WHERE conversation_id = ?1
        "#,
        params![conversation_id, now_rfc3339()],
    )?;
    fetch_conversation_by_id(conn, conversation_id)?
        .ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub fn fetch_conversation_by_id(
    conn: &Connection,
    conversation_id: &str,
) -> Result<Option<ConversationItem>> {
    let sql = r#"
        SELECT
            c.id,
            c.workspace_path,
            CASE
                WHEN c.source_types LIKE '%claude%' THEN 'claude'
                WHEN c.source_types LIKE '%cursor%' THEN 'cursor'
                WHEN c.source_types LIKE '%codex%' THEN 'codex'
                WHEN c.source_types LIKE '%workbuddy%' THEN 'workbuddy'
                WHEN c.source_types LIKE '%hermes%' THEN 'hermes'
                WHEN c.source_types LIKE '%mimo%' THEN 'mimo'
                WHEN c.source_types LIKE '%windsurf%' THEN 'windsurf'
                WHEN c.source_types LIKE '%codebuddy%' THEN 'codebuddy'
                WHEN c.source_types LIKE '%qoder%' THEN 'qoder'
                ELSE 'antigravity'
            END as source_app,
            c.title as source_title,
            NULLIF(TRIM(COALESCE(ai.ai_title, '')), '') as ai_title,
            COALESCE(ai.summary, '') as ai_summary,
            COALESCE(ai.status, 'idle') as ai_status,
            ai.based_on_content_hash,
            ai.model as ai_model,
            ai.generated_at as ai_generated_at,
            COALESCE(c.content_hash, '') as content_hash,
            c.created_at,
            c.updated_at,
            c.message_count,
            c.user_message_count,
            c.parse_status,
            (SELECT COUNT(*) FROM starred_sessions s WHERE s.conversation_id = c.id) as is_starred,
            ai.based_on_message_count
        FROM conversations c
        LEFT JOIN conversation_ai ai ON ai.conversation_id = c.id
        WHERE c.id = ?1
    "#;
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query_map(params![conversation_id], |row| {
        let source_title: String = row.get(3)?;
        let ai_title: Option<String> = row.get(4)?;
        let based_on: Option<String> = row.get(7)?;
        let content_hash: String = row.get(10)?;
        let is_starred_cnt: i64 = row.get(16)?;
        let based_on_msg_count: Option<i64> = row.get(17)?;
        let msg_count: i64 = row.get(13)?;
        let ai_new_message_count = match based_on_msg_count {
            Some(b) if b > 0 => {
                let diff = msg_count - b;
                if diff > 0 { Some(diff) } else { None }
            }
            _ => None,
        };
        let raw_created: Option<String> = row.get(11)?;
        let raw_updated: Option<String> = row.get(12)?;
        let raw_ai_generated: Option<String> = row.get(9)?;
        let display_title = ai_title
            .as_ref()
            .filter(|t| !t.is_empty())
            .cloned()
            .unwrap_or_else(|| source_title.clone());
        let summary_stale = match (&based_on, content_hash.as_str()) {
            (Some(h), ch) if !h.is_empty() && !ch.is_empty() => h != ch,
            _ => false,
        };
        let ai_summary: String = row.get(5)?;
        Ok(ConversationItem {
            id: row.get(0)?,
            workspace_path: row.get(1)?,
            source_app: row.get(2)?,
            title: display_title,
            source_title,
            ai_title,
            ai_summary: if ai_summary.is_empty() {
                None
            } else {
                Some(ai_summary)
            },
            ai_status: Some(row.get::<_, String>(6)?),
            ai_summary_stale: summary_stale,
            ai_model: row.get(8)?,
            ai_generated_at: to_beijing_iso(raw_ai_generated),
            content_hash,
            created_at: to_beijing_iso(raw_created),
            updated_at: to_beijing_iso(raw_updated),
            message_count: msg_count,
            user_message_count: row.get(14)?,
            parse_status: row.get(15)?,
            is_starred: is_starred_cnt > 0,
            ai_new_message_count,
        })
    })?;

    if let Some(row) = rows.next() {
        return Ok(Some(row?));
    }
    Ok(None)
}

fn snippet_around(text: &str, query: &str, radius: usize) -> String {
    let q = query.trim();
    if q.is_empty() {
        return clip_chars(text, radius * 2);
    }
    let lower_text = text.to_lowercase();
    let lower_q = q.to_lowercase();
    let Some(byte_idx) = lower_text.find(&lower_q) else {
        return clip_chars(text, radius * 2);
    };
    let char_idx = text.get(..byte_idx).map(|s| s.chars().count()).unwrap_or(0);
    let match_len = q.chars().count();
    let total = text.chars().count();
    let start = char_idx.saturating_sub(radius);
    let end = (char_idx + match_len + radius).min(total);
    let snippet: String = text.chars().skip(start).take(end.saturating_sub(start)).collect();
    let prefix = if start > 0 { "..." } else { "" };
    let suffix = if end < total { "..." } else { "" };
    format!("{}{}{}", prefix, snippet, suffix)
}

fn clip_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let s: String = text.chars().take(max_chars).collect();
    format!("{}...", s)
}

pub fn search_global_messages(
    conn: &Connection,
    query: &str,
    role: Option<&str>,
    limit: usize,
) -> Result<Vec<SearchResultItem>> {
    let mut list = Vec::new();
    let is_user = role == Some("user");
    let sql = r#"
        SELECT m.id, m.conversation_id,
               COALESCE(NULLIF(TRIM(ai.ai_title), ''), c.title) as title,
               CASE
                   WHEN c.source_types LIKE '%claude%' THEN 'claude'
                   WHEN c.source_types LIKE '%cursor%' THEN 'cursor'
                   WHEN c.source_types LIKE '%codex%' THEN 'codex'
                   WHEN c.source_types LIKE '%workbuddy%' THEN 'workbuddy'
                   WHEN c.source_types LIKE '%hermes%' THEN 'hermes'
                   WHEN c.source_types LIKE '%mimo%' THEN 'mimo'
                   WHEN c.source_types LIKE '%windsurf%' THEN 'windsurf'
                   WHEN c.source_types LIKE '%codebuddy%' THEN 'codebuddy'
                   WHEN c.source_types LIKE '%qoder%' THEN 'qoder'
                   ELSE 'antigravity'
               END as source_app,
               c.workspace_path,
               CASE WHEN m.role LIKE '%user%' THEN 'user' WHEN m.role LIKE '%tool%' THEN 'tool' ELSE 'assistant' END as sender,
               m.content as text,
               m.created_at
        FROM messages m
        JOIN conversations c ON m.conversation_id = c.id
        LEFT JOIN conversation_ai ai ON ai.conversation_id = c.id
        WHERE (?1 = 0 OR m.role LIKE '%user%')
          AND m.content LIKE '%' || ?2 || '%'
        ORDER BY m.id DESC
        LIMIT ?3
    "#;

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(
        params![if is_user { 1 } else { 0 }, query, limit as i64],
        |row| {
            let id_val: i64 = row.get(0)?;
            let raw_created: Option<String> = row.get(7)?;
            let text: String = row.get(6)?;
            let snippet = snippet_around(&text, query, 70);
            Ok(SearchResultItem {
                message_id: id_val.to_string(),
                conversation_id: row.get(1)?,
                conversation_title: row
                    .get::<_, Option<String>>(2)?
                    .unwrap_or_else(|| "未命名会话".to_string()),
                source_app: row.get(3)?,
                workspace_path: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                sender: row.get(5)?,
                snippet,
                created_at: to_beijing_iso(raw_created),
            })
        },
    )?;

    for r in rows.flatten() {
        list.push(r);
    }
    Ok(list)
}

pub fn fetch_workspace_detail_stats(
    conn: &Connection,
    workspace_path: &str,
) -> Result<WorkspaceDetailStats> {
    let ws_short = get_short_workspace(workspace_path);

    let (
        conversation_count,
        ag_cnt,
        cursor_cnt,
        claude_cnt,
        codex_cnt,
        wb_cnt,
        hermes_cnt,
        mimo_cnt,
        windsurf_cnt,
        codebuddy_cnt,
        qoder_cnt,
        user_message_count,
        message_count,
        first_active,
        last_active
    ): (i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, Option<String>, Option<String>) = conn.query_row(
        r#"
        SELECT
            COUNT(*),
            SUM(CASE WHEN COALESCE(source_app, '') = 'antigravity'
                OR source_types LIKE '%antigravity%'
                OR source_types LIKE '%transcript%'
                OR source_types LIKE '%sqlite_db%'
                OR source_types LIKE '%overview%'
                OR (
                    (source_app IS NULL OR source_app = '')
                    AND (source_types IS NULL OR source_types = '' OR source_types = '[]')
                    AND id NOT LIKE '%:%'
                )
                THEN 1 ELSE 0 END),
            SUM(CASE WHEN COALESCE(source_app, '') = 'cursor' OR source_types LIKE '%cursor%' THEN 1 ELSE 0 END),
            SUM(CASE WHEN COALESCE(source_app, '') = 'claude' OR source_types LIKE '%claude%' THEN 1 ELSE 0 END),
            SUM(CASE WHEN COALESCE(source_app, '') = 'codex' OR source_types LIKE '%codex%' THEN 1 ELSE 0 END),
            SUM(CASE WHEN COALESCE(source_app, '') = 'workbuddy' OR source_types LIKE '%workbuddy%' THEN 1 ELSE 0 END),
            SUM(CASE WHEN COALESCE(source_app, '') = 'hermes' OR source_types LIKE '%hermes%' THEN 1 ELSE 0 END),
            SUM(CASE WHEN COALESCE(source_app, '') = 'mimo' OR source_types LIKE '%mimo%' THEN 1 ELSE 0 END),
            SUM(CASE WHEN COALESCE(source_app, '') = 'windsurf' OR source_types LIKE '%windsurf%' THEN 1 ELSE 0 END),
            SUM(CASE WHEN COALESCE(source_app, '') = 'codebuddy' OR source_types LIKE '%codebuddy%' THEN 1 ELSE 0 END),
            SUM(CASE WHEN COALESCE(source_app, '') = 'qoder' OR source_types LIKE '%qoder%' THEN 1 ELSE 0 END),
            COALESCE(SUM(user_message_count), 0),
            COALESCE(SUM(message_count), 0),
            MIN(created_at),
            MAX(updated_at)
        FROM conversations
        WHERE workspace_path = ?1
        "#,
        params![workspace_path],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1).unwrap_or(0),
                r.get(2).unwrap_or(0),
                r.get(3).unwrap_or(0),
                r.get(4).unwrap_or(0),
                r.get(5).unwrap_or(0),
                r.get(6).unwrap_or(0),
                r.get(7).unwrap_or(0),
                r.get(8).unwrap_or(0),
                r.get(9).unwrap_or(0),
                r.get(10).unwrap_or(0),
                r.get(11)?,
                r.get(12)?,
                r.get(13)?,
                r.get(14)?,
            ))
        }
    ).unwrap_or((0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, None, None));

    let mut breakdown_parts = Vec::new();
    if ag_cnt > 0 {
        breakdown_parts.push(format!("AG {}", ag_cnt));
    }
    if cursor_cnt > 0 {
        breakdown_parts.push(format!("Cursor {}", cursor_cnt));
    }
    if claude_cnt > 0 {
        breakdown_parts.push(format!("Claude {}", claude_cnt));
    }
    if hermes_cnt > 0 {
        breakdown_parts.push(format!("Hermes {}", hermes_cnt));
    }
    if wb_cnt > 0 {
        breakdown_parts.push(format!("WorkBuddy {}", wb_cnt));
    }
    if codex_cnt > 0 {
        breakdown_parts.push(format!("Codex {}", codex_cnt));
    }
    if mimo_cnt > 0 {
        breakdown_parts.push(format!("MiMo {}", mimo_cnt));
    }
    if windsurf_cnt > 0 {
        breakdown_parts.push(format!("Windsurf {}", windsurf_cnt));
    }
    if codebuddy_cnt > 0 {
        breakdown_parts.push(format!("CodeBuddy {}", codebuddy_cnt));
    }
    if qoder_cnt > 0 {
        breakdown_parts.push(format!("Qoder {}", qoder_cnt));
    }
    let agent_breakdown = if breakdown_parts.is_empty() {
        format!("共 {} 会话", conversation_count)
    } else {
        breakdown_parts.join(" · ")
    };

    // 每日活跃消息统计（按北京时间自然日切分）
    struct DayStat {
        total_msgs: i64,
        user_msgs: i64,
        convs: i64,
    }
    let mut daily_stats: HashMap<String, DayStat> = HashMap::new();
    let mut stmt = conn.prepare(
        r#"
        SELECT strftime('%Y-%m-%d', datetime(m.created_at, '+8 hours')) as day, 
               COUNT(*) as total_msgs,
               COUNT(CASE WHEN m.role = 'user' THEN 1 END) as user_msgs,
               COUNT(DISTINCT c.id) as conv_cnt
        FROM messages m
        JOIN conversations c ON c.id = m.conversation_id
        WHERE c.workspace_path = ?1 AND m.created_at IS NOT NULL AND length(m.created_at) >= 10
        GROUP BY day
        "#,
    )?;
    let rows = stmt.query_map(params![workspace_path], |r| {
        let day: String = r.get(0)?;
        let total_msgs: i64 = r.get(1)?;
        let user_msgs: i64 = r.get(2)?;
        let convs: i64 = r.get(3)?;
        Ok((day, DayStat { total_msgs, user_msgs, convs }))
    })?;
    for item in rows.flatten() {
        daily_stats.insert(item.0, item.1);
    }

    let active_days = daily_stats.len() as i64;
    let peak_item = daily_stats.iter().max_by_key(|(_, stat)| stat.total_msgs);
    let (peak_day, peak_count) = match peak_item {
        Some((d, stat)) => (Some(d.clone()), stat.total_msgs),
        None => (None, 0),
    };

    // 生成 52 周 (364天) 热力图格子（基于北京时间自然日）
    let mut heatmap_cells = Vec::new();
    let beijing_tz = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
    let today = Utc::now().with_timezone(&beijing_tz).date_naive();
    let max_count = daily_stats.values().map(|s| s.total_msgs).max().unwrap_or(1).max(1) as f64;
    for i in (0..364).rev() {
        let d = today - chrono::Duration::days(i);
        let date_str = d.format("%Y-%m-%d").to_string();
        let stat = daily_stats.get(&date_str);
        let cnt = stat.map(|s| s.total_msgs).unwrap_or(0);
        let user_cnt = stat.map(|s| s.user_msgs).unwrap_or(0);
        let conv_cnt = stat.map(|s| s.convs).unwrap_or(0);
        let ratio = cnt as f64 / max_count;
        let level = if cnt == 0 {
            0
        } else if ratio <= 0.25 {
            1
        } else if ratio <= 0.5 {
            2
        } else if ratio <= 0.75 {
            3
        } else {
            4
        };
        heatmap_cells.push(HeatmapCell {
            date: date_str,
            count: cnt,
            level,
            user_count: user_cnt,
            total_messages: cnt,
            conv_count: conv_cnt,
        });
    }

    // 粗粒度 Blocks 查询
    let mut fine_blocks = Vec::new();
    let has_fine_table: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='workspace_blocks_fine'",
        [],
        |r| r.get(0)
    ).unwrap_or(0);

    if has_fine_table > 0 {
        let mut stmt = conn.prepare(
            r#"
            SELECT id, block_id, batch_index, type, title, summary, start_date, end_date, status, keywords_json
            FROM workspace_blocks_fine
            WHERE workspace_path = ?1
            ORDER BY start_date ASC, sort_order ASC, id ASC
            "#
        )?;
        let f_rows = stmt.query_map(params![workspace_path], |r| {
            let id: i64 = r.get(0)?;
            let block_id: String = r.get(1)?;
            let batch_index: Option<i64> = r.get(2)?;
            let b_type: String = r.get(3)?;
            let title: String = r.get(4)?;
            let summary: String = r.get(5)?;
            let start_date: Option<String> = r.get(6)?;
            let end_date: Option<String> = r.get(7)?;
            let status: String = r.get(8)?;
            let kw_json: String = r.get(9).unwrap_or_else(|_| "[]".to_string());
            let keywords: Vec<String> = serde_json::from_str(&kw_json).unwrap_or_default();
            Ok(WorkspaceFineBlock {
                id,
                block_id,
                batch_index,
                r#type: b_type,
                title,
                summary,
                start_date,
                end_date,
                status,
                keywords,
            })
        })?;
        for b in f_rows.flatten() {
            fine_blocks.push(b);
        }
    }

    // 模块总览 Blocks 查询
    let mut module_blocks = Vec::new();
    let has_mod_table: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='workspace_blocks_modules'",
        [],
        |r| r.get(0)
    ).unwrap_or(0);

    if has_mod_table > 0 {
        let mut stmt = conn.prepare(
            r#"
            SELECT id, module_id, type, title, summary, start_date, end_date, status, keywords_json, child_fine_ids_json
            FROM workspace_blocks_modules
            WHERE workspace_path = ?1
            ORDER BY sort_order ASC, id ASC
            "#
        )?;
        let m_rows = stmt.query_map(params![workspace_path], |r| {
            let id: i64 = r.get(0)?;
            let module_id: String = r.get(1)?;
            let b_type: String = r.get(2)?;
            let title: String = r.get(3)?;
            let summary: String = r.get(4)?;
            let start_date: Option<String> = r.get(5)?;
            let end_date: Option<String> = r.get(6)?;
            let status: String = r.get(7)?;
            let kw_json: String = r.get(8).unwrap_or_else(|_| "[]".to_string());
            let child_json: String = r.get(9).unwrap_or_else(|_| "[]".to_string());
            let keywords: Vec<String> = serde_json::from_str(&kw_json).unwrap_or_default();
            let child_fine_ids: Vec<String> = serde_json::from_str(&child_json).unwrap_or_default();
            Ok(WorkspaceModuleBlock {
                id,
                module_id,
                r#type: b_type,
                title,
                summary,
                start_date,
                end_date,
                status,
                keywords,
                child_fine_ids,
            })
        })?;
        for m in m_rows.flatten() {
            module_blocks.push(m);
        }
    }

    // Markdown 架构报告
    let mut report_md = None;
    let has_rep_table: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='workspace_reports'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if has_rep_table > 0 {
        let res: rusqlite::Result<String> = conn.query_row(
            "SELECT report_md FROM workspace_reports WHERE workspace_path = ?1 LIMIT 1",
            params![workspace_path],
            |r| r.get(0),
        );
        if let Ok(md) = res {
            if !md.is_empty() {
                report_md = Some(md);
            }
        }
    }

    let artifacts = get_workspace_artifacts(conn, workspace_path).unwrap_or_default();

    Ok(WorkspaceDetailStats {
        workspace_path: workspace_path.to_string(),
        workspace_short: ws_short,
        conversation_count,
        ag_conversation_count: ag_cnt,
        cursor_conversation_count: cursor_cnt,
        claude_conversation_count: claude_cnt,
        codex_conversation_count: codex_cnt,
        wb_conversation_count: wb_cnt,
        hermes_conversation_count: hermes_cnt,
        mimo_conversation_count: mimo_cnt,
        windsurf_conversation_count: windsurf_cnt,
        codebuddy_conversation_count: codebuddy_cnt,
        qoder_conversation_count: qoder_cnt,
        user_message_count,
        message_count,
        agent_breakdown,
        first_active: to_beijing_iso(first_active),
        last_active: to_beijing_iso(last_active),
        active_days,
        peak_day,
        peak_count,
        heatmap_cells,
        fine_blocks,
        module_blocks,
        report_md,
        artifacts,
    })
}

pub fn fetch_workspace_analysis_messages(
    conn: &Connection,
    workspace_path: &str,
) -> Result<Vec<AnalysisUserMessage>> {
    let mut list = Vec::new();
    let sql = r#"
        SELECT m.id, m.conversation_id, COALESCE(c.title, ''), m.created_at, m.content
        FROM messages m
        JOIN conversations c ON m.conversation_id = c.id
        WHERE c.workspace_path = ?1
          AND (m.role LIKE '%user%' OR m.role = 'user')
          AND m.content IS NOT NULL AND TRIM(m.content) != ''
        ORDER BY m.created_at ASC, m.id ASC
    "#;

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![workspace_path], |row| {
        let id_val: i64 = row.get(0)?;
        let raw_created: Option<String> = row.get(3)?;
        Ok(AnalysisUserMessage {
            id: Some(id_val.to_string()),
            conversation_id: row.get(1)?,
            conversation_title: row.get(2)?,
            created_at: to_beijing_iso(raw_created),
            content: row.get(4)?,
        })
    })?;

    for r in rows.flatten() {
        list.push(r);
    }
    Ok(list)
}

pub fn save_workspace_fine_blocks(
    conn: &Connection,
    workspace_path: &str,
    blocks: &[WorkspaceFineBlock],
    clear_existing: bool,
) -> Result<usize> {
    // 确保旧表缺少列时平滑自愈
    let _ = conn.execute(
        "ALTER TABLE workspace_blocks_fine ADD COLUMN created_at TEXT",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE workspace_blocks_fine ADD COLUMN batch_index INTEGER",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE workspace_blocks_fine ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE workspace_blocks_fine ADD COLUMN keywords_json TEXT NOT NULL DEFAULT '[]'",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE workspace_blocks_fine ADD COLUMN evidence_json TEXT NOT NULL DEFAULT '[]'",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE workspace_blocks_fine ADD COLUMN message_fingerprint TEXT NOT NULL DEFAULT ''",
        [],
    );

    if clear_existing {
        conn.execute(
            "DELETE FROM workspace_blocks_fine WHERE workspace_path = ?1",
            params![workspace_path],
        )?;
    }

    let now = Utc::now().to_rfc3339();
    let mut count = 0;
    let mut stmt = conn.prepare(
        r#"
        INSERT INTO workspace_blocks_fine (
            workspace_path, message_fingerprint, block_id, batch_index, type, title, summary,
            start_date, end_date, status, keywords_json, evidence_json, sort_order, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
        "#,
    )?;

    for (idx, block) in blocks.iter().enumerate() {
        let kw_json = serde_json::to_string(&block.keywords).unwrap_or_else(|_| "[]".to_string());
        stmt.execute(params![
            workspace_path,
            "",
            block.block_id,
            block.batch_index,
            block.r#type,
            block.title,
            block.summary,
            block.start_date,
            block.end_date,
            block.status,
            kw_json,
            "[]",
            idx as i64,
            now,
        ])?;
        count += 1;
    }

    Ok(count)
}

pub fn save_workspace_module_blocks(
    conn: &Connection,
    workspace_path: &str,
    modules: &[WorkspaceModuleBlock],
    clear_existing: bool,
) -> Result<usize> {
    // 确保旧表缺少列时平滑自愈
    let _ = conn.execute(
        "ALTER TABLE workspace_blocks_modules ADD COLUMN created_at TEXT",
        [],
    );
    let _ = conn.execute("ALTER TABLE workspace_blocks_modules ADD COLUMN child_fine_ids_json TEXT NOT NULL DEFAULT '[]'", []);
    let _ = conn.execute(
        "ALTER TABLE workspace_blocks_modules ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE workspace_blocks_modules ADD COLUMN keywords_json TEXT NOT NULL DEFAULT '[]'",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE workspace_blocks_modules ADD COLUMN evidence_json TEXT NOT NULL DEFAULT '[]'",
        [],
    );
    let _ = conn.execute("ALTER TABLE workspace_blocks_modules ADD COLUMN message_fingerprint TEXT NOT NULL DEFAULT ''", []);

    if clear_existing {
        conn.execute(
            "DELETE FROM workspace_blocks_modules WHERE workspace_path = ?1",
            params![workspace_path],
        )?;
    }

    let now = Utc::now().to_rfc3339();
    let mut count = 0;
    let mut stmt = conn.prepare(
        r#"
        INSERT INTO workspace_blocks_modules (
            workspace_path, message_fingerprint, module_id, type, title, summary,
            start_date, end_date, status, keywords_json, evidence_json, child_fine_ids_json, sort_order, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
        "#,
    )?;

    for (idx, m) in modules.iter().enumerate() {
        let kw_json = serde_json::to_string(&m.keywords).unwrap_or_else(|_| "[]".to_string());
        let child_json =
            serde_json::to_string(&m.child_fine_ids).unwrap_or_else(|_| "[]".to_string());
        stmt.execute(params![
            workspace_path,
            "",
            m.module_id,
            m.r#type,
            m.title,
            m.summary,
            m.start_date,
            m.end_date,
            m.status,
            kw_json,
            "[]",
            child_json,
            idx as i64,
            now,
        ])?;
        count += 1;
    }

    Ok(count)
}

pub fn save_workspace_report(
    conn: &Connection,
    workspace_path: &str,
    report_md: &str,
) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS workspace_reports (
            workspace_path TEXT PRIMARY KEY,
            report_md TEXT NOT NULL DEFAULT '',
            updated_at TEXT,
            generated_at TEXT
        );
        "#,
    )?;
    let _ = conn.execute(
        "ALTER TABLE workspace_reports ADD COLUMN updated_at TEXT",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE workspace_reports ADD COLUMN generated_at TEXT",
        [],
    );

    let now = Utc::now().to_rfc3339();
    conn.execute(
        r#"
        INSERT INTO workspace_reports (workspace_path, report_md, updated_at, generated_at)
        VALUES (?1, ?2, ?3, ?3)
        ON CONFLICT(workspace_path) DO UPDATE SET
            report_md = excluded.report_md,
            updated_at = excluded.updated_at,
            generated_at = excluded.generated_at
        "#,
        params![workspace_path, report_md, now],
    )?;
    Ok(())
}

pub fn clear_workspace_analysis(conn: &Connection, workspace_path: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM workspace_blocks_fine WHERE workspace_path = ?1",
        params![workspace_path],
    )?;
    conn.execute(
        "DELETE FROM workspace_blocks_modules WHERE workspace_path = ?1",
        params![workspace_path],
    )?;
    conn.execute(
        "DELETE FROM workspace_reports WHERE workspace_path = ?1",
        params![workspace_path],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptItem {
    pub id: i64,
    pub title: String,
    pub content: String,
    pub category: String,
    pub tags: Vec<String>,
    pub source_url: Option<String>,
    pub source_note: Option<String>,
    pub notes: Option<String>,
    pub preview_url: Option<String>,
    pub preview_local: Option<String>,
    pub origin: String,
    pub external_id: Option<String>,
    pub genre: Option<String>,
    pub styles: Vec<String>,
    pub scenes: Vec<String>,
    pub featured: bool,
    pub github_url: Option<String>,
    pub prompt_preview: Option<String>,
    pub content_hash: Option<String>,
    /// 封面图原始宽高（渲染元数据，用于 Feed 占位与分列估算）
    pub preview_width: Option<i64>,
    pub preview_height: Option<i64>,
    pub is_starred: bool,
    pub use_count: i64,
    pub created_at: String,
    pub updated_at: String,
    pub last_used_at: Option<String>,
}

/// 对外 Agent API 使用的精简视图（不含星标、计数、时间戳）
#[derive(Debug, Clone, Serialize)]
pub struct PromptAgentItem {
    pub id: i64,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_preview: Option<String>,
    pub category: String,
    pub tags: Vec<String>,
    pub origin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub scenes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptCategoryOption {
    pub value: String,
    pub label: String,
}

pub fn default_prompt_category() -> String {
    "image".to_string()
}

pub fn prompt_category_options() -> Vec<PromptCategoryOption> {
    vec![
        PromptCategoryOption {
            value: "image".into(),
            label: "生图".into(),
        },
        PromptCategoryOption {
            value: "video".into(),
            label: "生视频".into(),
        },
        PromptCategoryOption {
            value: "text".into(),
            label: "文本".into(),
        },
    ]
}

pub fn allowed_prompt_category_values() -> Vec<String> {
    prompt_category_options()
        .into_iter()
        .map(|c| c.value)
        .collect()
}

pub fn normalize_prompt_category(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    let cat = if trimmed.is_empty() {
        default_prompt_category()
    } else {
        // 兼容旧细分类
        match trimmed {
            "coding" | "research" | "writing" | "product" | "agent" | "persona" | "meta" => {
                "text".to_string()
            }
            other => other.to_string(),
        }
    };
    if allowed_prompt_category_values().iter().any(|v| v == &cat) {
        Ok(cat)
    } else {
        Err(format!("invalid category: {}", cat))
    }
}

const PROMPT_PREVIEW_CHARS: usize = 120;

fn prompt_content_preview(content: &str) -> String {
    if content.chars().count() <= PROMPT_PREVIEW_CHARS {
        return content.to_string();
    }
    format!(
        "{}…",
        content.chars().take(PROMPT_PREVIEW_CHARS).collect::<String>()
    )
}

fn parse_json_str_list(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

impl PromptAgentItem {
    pub fn list_from(item: &PromptItem) -> Self {
        Self {
            id: item.id,
            title: item.title.clone(),
            content: None,
            content_preview: Some(
                item.prompt_preview
                    .clone()
                    .unwrap_or_else(|| prompt_content_preview(&item.content)),
            ),
            category: item.category.clone(),
            tags: item.tags.clone(),
            origin: item.origin.clone(),
            source_url: item.source_url.clone(),
            source_note: item.source_note.clone(),
            notes: item.notes.clone(),
            preview_url: item.preview_url.clone(),
            genre: item.genre.clone(),
            scenes: item.scenes.clone(),
        }
    }

    pub fn detail_from(item: &PromptItem) -> Self {
        Self {
            id: item.id,
            title: item.title.clone(),
            content: Some(item.content.clone()),
            content_preview: None,
            category: item.category.clone(),
            tags: item.tags.clone(),
            origin: item.origin.clone(),
            source_url: item.source_url.clone(),
            source_note: item.source_note.clone(),
            notes: item.notes.clone(),
            preview_url: item.preview_url.clone(),
            genre: item.genre.clone(),
            scenes: item.scenes.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptInput {
    pub title: String,
    #[serde(default)]
    pub content: String,
    #[serde(default = "default_prompt_category")]
    pub category: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub source_note: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub preview_url: Option<String>,
    /// 本地归档预览图（/media/prompts/uploads/…）。
    /// None = 调用方未提供（更新时保留原值）；Some("") = 显式清除。
    #[serde(default)]
    pub preview_local: Option<String>,
    #[serde(default)]
    pub is_starred: bool,
}

fn row_to_prompt_item(row: &rusqlite::Row<'_>) -> Result<PromptItem> {
    let tags_json: String = row.get(4)?;
    let styles_json: String = row.get(13)?;
    let scenes_json: String = row.get(14)?;
    Ok(PromptItem {
        id: row.get(0)?,
        title: row.get(1)?,
        content: row.get(2)?,
        category: row.get(3)?,
        tags: parse_json_str_list(&tags_json),
        source_url: row.get(5)?,
        source_note: row.get(6)?,
        notes: row.get(7)?,
        preview_url: row.get(8)?,
        preview_local: row.get(9)?,
        origin: row.get::<_, Option<String>>(10)?.unwrap_or_else(|| "user".into()),
        external_id: row.get(11)?,
        genre: row.get(12)?,
        styles: parse_json_str_list(&styles_json),
        scenes: parse_json_str_list(&scenes_json),
        featured: row.get::<_, i64>(15)? != 0,
        github_url: row.get(16)?,
        prompt_preview: row.get(17)?,
        content_hash: row.get(18)?,
        preview_width: row.get(19)?,
        preview_height: row.get(20)?,
        is_starred: row.get::<_, i64>(21)? != 0,
        use_count: row.get(22)?,
        created_at: row.get(23)?,
        updated_at: row.get(24)?,
        last_used_at: row.get(25)?,
    })
}

const PROMPT_SELECT: &str = r#"
    SELECT id, title, content, category, tags_json, source_url, source_note, notes,
           preview_url, preview_local, origin, external_id, genre, styles_json, scenes_json,
           featured, github_url, prompt_preview, content_hash,
           preview_width, preview_height,
           is_starred, use_count, created_at, updated_at, last_used_at
    FROM prompts
"#;

pub fn list_prompts(
    conn: &Connection,
    search: Option<&str>,
    category: Option<&str>,
    starred_only: bool,
) -> Result<Vec<PromptItem>> {
    list_prompts_ex(conn, search, category, starred_only, false)
}

/// lite=true 时清空正文，Feed 列表更轻；详情请用 get_prompt。
pub fn list_prompts_ex(
    conn: &Connection,
    search: Option<&str>,
    category: Option<&str>,
    starred_only: bool,
    lite: bool,
) -> Result<Vec<PromptItem>> {
    let mut list = Vec::new();
    let sql = format!(
        r#"{PROMPT_SELECT}
        WHERE (?1 IS NULL OR ?1 = '' OR title LIKE '%' || ?1 || '%' OR content LIKE '%' || ?1 || '%' OR tags_json LIKE '%' || ?1 || '%' OR genre LIKE '%' || ?1 || '%' OR scenes_json LIKE '%' || ?1 || '%')
          AND (?2 IS NULL OR ?2 = '' OR category = ?2)
          AND (?3 = 0 OR is_starred = 1)
        ORDER BY is_starred DESC, featured DESC, updated_at DESC, id DESC"#
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params![
            search.unwrap_or(""),
            category.unwrap_or(""),
            if starred_only { 1 } else { 0 }
        ],
        row_to_prompt_item,
    )?;
    for r in rows.flatten() {
        let mut item = r;
        if lite {
            item.content = String::new();
        }
        list.push(item);
    }
    Ok(list)
}

pub fn get_prompt(conn: &Connection, id: i64) -> Result<PromptItem> {
    let sql = format!("{PROMPT_SELECT} WHERE id = ?1");
    conn.query_row(&sql, params![id], row_to_prompt_item)
}

pub fn create_prompt(conn: &Connection, input: &PromptInput) -> Result<PromptItem> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(rusqlite::Error::InvalidParameterName(
            "title cannot be empty".into(),
        ));
    }
    let category = normalize_prompt_category(&input.category).map_err(|e| {
        rusqlite::Error::InvalidParameterName(e)
    })?;
    let now = Utc::now().to_rfc3339();
    let tags_json = serde_json::to_string(&input.tags).unwrap_or_else(|_| "[]".to_string());
    let preview_local = input
        .preview_local
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    conn.execute(
        r#"
        INSERT INTO prompts (
            title, content, category, tags_json, source_url, source_note, notes,
            preview_url, preview_local, origin, external_id, genre, styles_json, scenes_json,
            featured, github_url, prompt_preview, content_hash,
            is_starred, use_count, created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7,
            ?8, ?11, 'user', NULL, NULL, '[]', '[]',
            0, NULL, NULL, NULL,
            ?9, 0, ?10, ?10
        )
        "#,
        params![
            title,
            input.content,
            category,
            tags_json,
            input.source_url,
            input.source_note,
            input.notes,
            input.preview_url,
            if input.is_starred { 1 } else { 0 },
            now,
            preview_local,
        ],
    )?;
    let id = conn.last_insert_rowid();
    get_prompt(conn, id)
}

pub fn update_prompt(conn: &Connection, id: i64, input: &PromptInput) -> Result<PromptItem> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(rusqlite::Error::InvalidParameterName(
            "title cannot be empty".into(),
        ));
    }
    let category = normalize_prompt_category(&input.category).map_err(|e| {
        rusqlite::Error::InvalidParameterName(e)
    })?;
    let now = Utc::now().to_rfc3339();
    let tags_json = serde_json::to_string(&input.tags).unwrap_or_else(|_| "[]".to_string());
    // 用户编辑不碰 origin / external_id / catalog 同步字段；
    // preview_local 仅在调用方显式提供时更新（None 保留，空串清除）
    let changed = conn.execute(
        r#"
        UPDATE prompts SET
            title = ?1,
            content = ?2,
            category = ?3,
            tags_json = ?4,
            source_url = ?5,
            source_note = ?6,
            notes = ?7,
            preview_url = ?8,
            preview_local = CASE WHEN ?12 IS NULL THEN preview_local ELSE NULLIF(?12, '') END,
            is_starred = ?9,
            updated_at = ?10
        WHERE id = ?11
        "#,
        params![
            title,
            input.content,
            category,
            tags_json,
            input.source_url,
            input.source_note,
            input.notes,
            input.preview_url,
            if input.is_starred { 1 } else { 0 },
            now,
            id,
            input.preview_local.as_deref().map(str::trim),
        ],
    )?;
    if changed == 0 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    get_prompt(conn, id)
}

pub fn delete_prompt(conn: &Connection, id: i64) -> Result<bool> {
    let changed = conn.execute("DELETE FROM prompts WHERE id = ?1", params![id])?;
    Ok(changed > 0)
}

pub fn toggle_prompt_star(conn: &Connection, id: i64) -> Result<bool> {
    let now = Utc::now().to_rfc3339();
    let current: i64 = conn.query_row(
        "SELECT is_starred FROM prompts WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    let next = if current != 0 { 0 } else { 1 };
    conn.execute(
        "UPDATE prompts SET is_starred = ?1, updated_at = ?2 WHERE id = ?3",
        params![next, now, id],
    )?;
    Ok(next != 0)
}

/// 写回封面图原始宽高（渲染元数据）。仅当值变化时更新；不触碰 updated_at，
/// 避免纯渲染信息把记录顶到「最近更新」。
pub fn update_prompt_preview_size(
    conn: &Connection,
    id: i64,
    width: i64,
    height: i64,
) -> Result<bool> {
    if width <= 0 || height <= 0 {
        return Ok(false);
    }
    let changed = conn.execute(
        r#"
        UPDATE prompts
        SET preview_width = ?1, preview_height = ?2
        WHERE id = ?3
          AND (preview_width IS NOT ?1 OR preview_height IS NOT ?2)
        "#,
        params![width, height, id],
    )?;
    Ok(changed > 0)
}

pub fn record_prompt_use(conn: &Connection, id: i64) -> Result<PromptItem> {
    let now = Utc::now().to_rfc3339();
    let changed = conn.execute(
        "UPDATE prompts SET use_count = use_count + 1, last_used_at = ?1, updated_at = ?1 WHERE id = ?2",
        params![now, id],
    )?;
    if changed == 0 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    get_prompt(conn, id)
}

fn parse_beijing_time_str(s: &str) -> (u32, String) {
    let time_part = if let Some(pos) = s.find('T') {
        &s[pos + 1..]
    } else if let Some(pos) = s.find(' ') {
        &s[pos + 1..]
    } else {
        s
    };
    let parts: Vec<&str> = time_part.split(':').collect();
    if parts.len() >= 2 {
        let h: u32 = parts[0].parse().unwrap_or(0);
        let m: u32 = parts[1].parse().unwrap_or(0);
        let h = h.min(23);
        let m = m.min(59);
        let total_minutes = h * 60 + m;
        (total_minutes, format!("{:02}:{:02}", h, m))
    } else {
        (0, "00:00".to_string())
    }
}

pub fn fetch_daily_timeline(conn: &Connection, date: &str) -> Result<DailyTimelineStats> {
    let trimmed_date = date.trim();
    if trimmed_date.is_empty() {
        return Ok(DailyTimelineStats {
            date: date.to_string(),
            total_conversations: 0,
            total_workspaces: 0,
            total_messages: 0,
            total_user_messages: 0,
            peak_concurrency: 0,
            items: Vec::new(),
            concurrency_slots: Vec::new(),
            spans: Vec::new(),
        });
    }

    // 1. 查询当天所有的用户提示词 (User Prompts)
    let sql_prompts = r#"
        SELECT
            m.id,
            m.conversation_id,
            c.workspace_path,
            CASE
                WHEN c.source_types LIKE '%claude%' THEN 'claude'
                WHEN c.source_types LIKE '%cursor%' THEN 'cursor'
                WHEN c.source_types LIKE '%codex%' THEN 'codex'
                WHEN c.source_types LIKE '%workbuddy%' THEN 'workbuddy'
                WHEN c.source_types LIKE '%hermes%' THEN 'hermes'
                WHEN c.source_types LIKE '%mimo%' THEN 'mimo'
                WHEN c.source_types LIKE '%windsurf%' THEN 'windsurf'
                WHEN c.source_types LIKE '%codebuddy%' THEN 'codebuddy'
                WHEN c.source_types LIKE '%qoder%' THEN 'qoder'
                ELSE 'antigravity'
            END as source_app,
            c.title as conv_title,
            datetime(m.created_at, '+8 hours') as prompt_time,
            m.content as prompt_content,
            (SELECT COUNT(*) FROM starred_sessions s WHERE s.conversation_id = c.id) as is_starred
        FROM messages m
        JOIN conversations c ON m.conversation_id = c.id
        WHERE strftime('%Y-%m-%d', datetime(m.created_at, '+8 hours')) = ?1
          AND m.role = 'user'
        ORDER BY datetime(m.created_at, '+8 hours') ASC
    "#;

    let mut stmt = conn.prepare(sql_prompts)?;
    let rows = stmt.query_map(params![trimmed_date], |row| {
        let message_id: i64 = row.get(0)?;
        let id: String = row.get(1)?;
        let workspace_path: String = row.get(2)?;
        let source_app: String = row.get(3)?;
        let conversation_title: String = row.get(4)?;
        let prompt_time: Option<String> = row.get(5)?;
        let prompt_content: String = row.get(6)?;
        let is_starred_cnt: i64 = row.get(7)?;

        let raw_time = prompt_time.unwrap_or_else(|| format!("{} 00:00:00", trimmed_date));
        let (minute, time_label) = parse_beijing_time_str(&raw_time);
        let (source_label, source_color) = source_to_label_and_color(&source_app);
        let workspace_short = get_short_workspace(&workspace_path);

        let prompt_preview = prompt_content
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or(&prompt_content)
            .trim()
            .chars()
            .take(60)
            .collect::<String>();

        Ok(DailyTimelineItem {
            message_id,
            id,
            workspace_path,
            workspace_short,
            source_app,
            source_label: source_label.to_string(),
            source_color: source_color.to_string(),
            conversation_title: if conversation_title.is_empty() {
                "未命名会话".to_string()
            } else {
                conversation_title
            },
            prompt_content,
            prompt_preview,
            time: raw_time,
            time_label,
            minute,
            is_starred: is_starred_cnt > 0,
        })
    })?;

    let mut items = Vec::new();
    let mut unique_conv_ids = HashSet::new();
    let mut unique_workspaces = HashSet::new();

    for r in rows.flatten() {
        unique_conv_ids.insert(r.id.clone());
        unique_workspaces.insert(r.workspace_path.clone());
        items.push(r);
    }

    // 2. 统计当天全局总消息数与总用户消息数
    let (total_messages, total_user_messages): (i64, i64) = conn
        .query_row(
            r#"
            SELECT
                COUNT(*),
                SUM(CASE WHEN role = 'user' THEN 1 ELSE 0 END)
            FROM messages
            WHERE strftime('%Y-%m-%d', datetime(created_at, '+8 hours')) = ?1
            "#,
            params![trimmed_date],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or((items.len() as i64, items.len() as i64));

    // 3. 计算 96 个 15 分钟切片的并发度指标
    let mut concurrency_slots = Vec::with_capacity(96);
    let mut peak_concurrency = 0usize;

    for slot_idx in 0..96 {
        let slot_start_minute = (slot_idx * 15) as u32;
        let slot_end_minute = slot_start_minute + 15;
        let hour = slot_start_minute / 60;
        let minute = slot_start_minute % 60;
        let time_label = format!("{:02}:{:02}", hour, minute);

        let mut slot_workspaces = HashSet::new();
        let mut active_convs = HashSet::new();

        for item in &items {
            if item.minute >= slot_start_minute && item.minute < slot_end_minute {
                active_convs.insert(&item.id);
                slot_workspaces.insert(&item.workspace_path);
            }
        }

        let active_ws_count = slot_workspaces.len();
        if active_ws_count > peak_concurrency {
            peak_concurrency = active_ws_count;
        }

        concurrency_slots.push(DailyConcurrencySlot {
            hour,
            minute,
            time_label,
            active_conversations: active_convs.len(),
            active_workspaces: active_ws_count,
        });
    }

    // 4. 会话级活动时段：按消息时间序列，间隔 > 20 分钟则拆成多段甘特条
    const ACTIVITY_GAP_MINUTES: u32 = 20;
    let mut spans: Vec<DailyActivitySpan> = Vec::new();
    {
        #[derive(Default)]
        struct MsgPoint {
            minute: u32,
            is_prompt: bool,
        }
        struct ConvMeta {
            conversation_id: String,
            workspace_path: String,
            source_app: String,
            conversation_title: String,
            points: Vec<MsgPoint>,
        }

        let sql_msgs = r#"
            SELECT
                m.conversation_id,
                c.workspace_path,
                CASE
                    WHEN c.source_types LIKE '%claude%' THEN 'claude'
                    WHEN c.source_types LIKE '%cursor%' THEN 'cursor'
                    WHEN c.source_types LIKE '%codex%' THEN 'codex'
                    WHEN c.source_types LIKE '%workbuddy%' THEN 'workbuddy'
                    WHEN c.source_types LIKE '%hermes%' THEN 'hermes'
                    WHEN c.source_types LIKE '%mimo%' THEN 'mimo'
                    WHEN c.source_types LIKE '%windsurf%' THEN 'windsurf'
                    WHEN c.source_types LIKE '%codebuddy%' THEN 'codebuddy'
                    WHEN c.source_types LIKE '%qoder%' THEN 'qoder'
                    ELSE 'antigravity'
                END as source_app,
                c.title as conv_title,
                datetime(m.created_at, '+8 hours') as msg_at,
                m.role as msg_role
            FROM messages m
            JOIN conversations c ON m.conversation_id = c.id
            WHERE strftime('%Y-%m-%d', datetime(m.created_at, '+8 hours')) = ?1
            ORDER BY m.conversation_id ASC, m.created_at ASC, m.id ASC
        "#;
        let mut msg_stmt = conn.prepare(sql_msgs)?;
        let msg_rows = msg_stmt.query_map(params![trimmed_date], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;

        let mut ordered: Vec<ConvMeta> = Vec::new();
        let mut current_id: Option<String> = None;
        for row in msg_rows.flatten() {
            let (cid, wspath, sapp, ctitle, msg_at, role) = row;
            let (minute, _) = parse_beijing_time_str(&msg_at);
            let is_prompt = role == "user";
            match current_id.as_deref() {
                Some(id) if id == cid => {
                    if let Some(last) = ordered.last_mut() {
                        last.points.push(MsgPoint { minute, is_prompt });
                    }
                }
                _ => {
                    ordered.push(ConvMeta {
                        conversation_id: cid.clone(),
                        workspace_path: wspath,
                        source_app: sapp,
                        conversation_title: ctitle,
                        points: vec![MsgPoint { minute, is_prompt }],
                    });
                    current_id = Some(cid);
                }
            }
        }

        for conv in ordered {
            if conv.points.is_empty() {
                continue;
            }
            let (source_label, source_color) = source_to_label_and_color(&conv.source_app);
            let workspace_short = get_short_workspace(&conv.workspace_path);
            let conversation_title = if conv.conversation_title.is_empty() {
                "未命名会话".to_string()
            } else {
                conv.conversation_title.clone()
            };

            // 按 ACTIVITY_GAP 拆段
            let mut seg_start = conv.points[0].minute;
            let mut seg_end = conv.points[0].minute;
            let mut seg_prompts = if conv.points[0].is_prompt { 1 } else { 0 };
            let mut seg_msgs = 1i64;

            let mut flush_seg = |start: u32,
                                 end: u32,
                                 prompts: i64,
                                 msgs: i64,
                                 conv: &ConvMeta,
                                 title: &str,
                                 source_label: &str,
                                 source_color: &str,
                                 workspace_short: &str,
                                 spans: &mut Vec<DailyActivitySpan>| {
                let end = end.max(start);
                let (_, start_label) = parse_beijing_time_str(&format!("{} {:02}:{:02}:00", trimmed_date, start / 60, start % 60));
                let (_, end_label) = parse_beijing_time_str(&format!("{} {:02}:{:02}:00", trimmed_date, end / 60, end % 60));
                spans.push(DailyActivitySpan {
                    conversation_id: conv.conversation_id.clone(),
                    workspace_path: conv.workspace_path.clone(),
                    workspace_short: workspace_short.to_string(),
                    source_app: conv.source_app.clone(),
                    source_label: source_label.to_string(),
                    source_color: source_color.to_string(),
                    conversation_title: title.to_string(),
                    start_minute: start,
                    end_minute: end,
                    start_label,
                    end_label,
                    prompt_count: prompts,
                    message_count: msgs,
                });
            };

            for p in conv.points.iter().skip(1) {
                if p.minute.saturating_sub(seg_end) > ACTIVITY_GAP_MINUTES {
                    flush_seg(
                        seg_start,
                        seg_end,
                        seg_prompts,
                        seg_msgs,
                        &conv,
                        &conversation_title,
                        source_label,
                        source_color,
                        &workspace_short,
                        &mut spans,
                    );
                    seg_start = p.minute;
                    seg_end = p.minute;
                    seg_prompts = if p.is_prompt { 1 } else { 0 };
                    seg_msgs = 1;
                } else {
                    seg_end = p.minute;
                    if p.is_prompt {
                        seg_prompts += 1;
                    }
                    seg_msgs += 1;
                }
            }
            flush_seg(
                seg_start,
                seg_end,
                seg_prompts,
                seg_msgs,
                &conv,
                &conversation_title,
                source_label,
                source_color,
                &workspace_short,
                &mut spans,
            );
        }
    }

    Ok(DailyTimelineStats {
        date: trimmed_date.to_string(),
        total_conversations: unique_conv_ids.len(),
        total_workspaces: unique_workspaces.len(),
        total_messages,
        total_user_messages,
        peak_concurrency,
        items,
        concurrency_slots,
        spans,
    })
}

pub fn save_llm_call_log(
    conn: &Connection,
    scene: &str,
    provider_name: &str,
    model: &str,
    system_prompt: &str,
    user_prompt_snippet: &str,
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
    latency_ms: i64,
    is_fallback: bool,
    status: &str,
    error_msg: Option<&str>,
) -> Result<i64> {
    let now = chrono::Local::now().to_rfc3339();
    conn.execute(
        r#"
        INSERT INTO llm_call_logs (
            created_at, scene, provider_name, model, system_prompt, user_prompt_snippet,
            prompt_tokens, completion_tokens, total_tokens, latency_ms, is_fallback, status, error_msg
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        params![
            now,
            scene,
            provider_name,
            model,
            system_prompt,
            user_prompt_snippet,
            prompt_tokens,
            completion_tokens,
            total_tokens,
            latency_ms,
            if is_fallback { 1 } else { 0 },
            status,
            error_msg,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_llm_call_logs(
    conn: &Connection,
    limit: i64,
    offset: i64,
) -> Result<Vec<LlmCallLogItem>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, created_at, scene, provider_name, model, system_prompt, user_prompt_snippet,
               prompt_tokens, completion_tokens, total_tokens, latency_ms, is_fallback, status, error_msg
        FROM llm_call_logs
        ORDER BY id DESC
        LIMIT ?1 OFFSET ?2
        "#,
    )?;

    let rows = stmt.query_map(params![limit, offset], |row| {
        let is_fb: i64 = row.get(11)?;
        Ok(LlmCallLogItem {
            id: row.get(0)?,
            created_at: row.get(1)?,
            scene: row.get(2)?,
            provider_name: row.get(3)?,
            model: row.get(4)?,
            system_prompt: row.get(5)?,
            user_prompt_snippet: row.get(6)?,
            prompt_tokens: row.get(7)?,
            completion_tokens: row.get(8)?,
            total_tokens: row.get(9)?,
            latency_ms: row.get(10)?,
            is_fallback: is_fb != 0,
            status: row.get(12)?,
            error_msg: row.get(13)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn get_llm_usage_summary(conn: &Connection) -> Result<LlmUsageSummary> {
    let mut stmt = conn.prepare(
        r#"
        SELECT 
            COUNT(*) as total_calls,
            SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END) as success_calls,
            COALESCE(SUM(prompt_tokens), 0) as total_prompt_tokens,
            COALESCE(SUM(completion_tokens), 0) as total_completion_tokens,
            COALESCE(SUM(total_tokens), 0) as total_tokens,
            COALESCE(ROUND(AVG(latency_ms), 1), 0.0) as avg_latency_ms,
            SUM(CASE WHEN is_fallback = 1 THEN 1 ELSE 0 END) as fallback_calls
        FROM llm_call_logs
        "#,
    )?;

    let summary = stmt.query_row([], |row| {
        Ok(LlmUsageSummary {
            total_calls: row.get(0)?,
            success_calls: row.get(1)?,
            total_prompt_tokens: row.get(2)?,
            total_completion_tokens: row.get(3)?,
            total_tokens: row.get(4)?,
            avg_latency_ms: row.get(5)?,
            fallback_calls: row.get(6)?,
        })
    })?;

    Ok(summary)
}

pub fn clear_llm_call_logs(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM llm_call_logs", [])?;
    Ok(())
}

pub fn get_conversation_artifacts(
    conn: &Connection,
    conversation_id: &str,
) -> Result<Vec<ArtifactItem>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, conversation_id, file_name, file_path, title, summary, content,
               user_facing, request_feedback, created_at, updated_at
        FROM conversation_artifacts
        WHERE conversation_id = ?
        ORDER BY 
            CASE 
                WHEN file_name = 'implementation_plan.md' THEN 1
                WHEN file_name LIKE 'implementation_plan%' THEN 2
                WHEN file_name = 'task.md' THEN 3
                WHEN file_name LIKE 'task%' THEN 4
                WHEN file_name = 'walkthrough.md' THEN 5
                WHEN file_name LIKE 'walkthrough%' THEN 6
                ELSE 7
            END,
            COALESCE(created_at, updated_at, '') DESC,
            file_name DESC
        "#,
    )?;

    let rows = stmt.query_map([conversation_id], |row| {
        Ok(ArtifactItem {
            id: row.get(0)?,
            conversation_id: row.get(1)?,
            file_name: row.get(2)?,
            file_path: row.get(3)?,
            title: row.get(4)?,
            summary: row.get(5)?,
            content: row.get(6)?,
            user_facing: row.get::<_, i64>(7)? != 0,
            request_feedback: row.get::<_, i64>(8)? != 0,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn get_workspace_artifacts(
    conn: &Connection,
    workspace_path: &str,
) -> Result<Vec<WorkspaceArtifactItem>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT a.id, a.conversation_id, COALESCE(c.title, '未命名会话'), COALESCE(c.source_app, 'antigravity'),
               a.file_name, a.file_path, a.title, a.summary, a.content,
               a.user_facing, a.request_feedback, a.created_at, a.updated_at
        FROM conversation_artifacts a
        JOIN conversations c ON a.conversation_id = c.id
        WHERE c.workspace_path = ?1
        ORDER BY 
            COALESCE(a.created_at, a.updated_at, c.created_at, '') DESC,
            CASE 
                WHEN a.file_name = 'implementation_plan.md' THEN 1
                WHEN a.file_name LIKE 'implementation_plan%' THEN 2
                WHEN a.file_name = 'task.md' THEN 3
                WHEN a.file_name LIKE 'task%' THEN 4
                WHEN a.file_name = 'walkthrough.md' THEN 5
                WHEN a.file_name LIKE 'walkthrough%' THEN 6
                ELSE 7
            END,
            a.file_name ASC
        "#,
    )?;

    let rows = stmt.query_map([workspace_path], |row| {
        Ok(WorkspaceArtifactItem {
            id: row.get(0)?,
            conversation_id: row.get(1)?,
            conversation_title: row.get(2)?,
            source_app: row.get(3)?,
            file_name: row.get(4)?,
            file_path: row.get(5)?,
            title: row.get(6)?,
            summary: row.get(7)?,
            content: row.get(8)?,
            user_facing: row.get::<_, i64>(9)? != 0,
            request_feedback: row.get::<_, i64>(10)? != 0,
            created_at: row.get(11)?,
            updated_at: row.get(12)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn insert_usage(conn: &Connection, cid: &str, identity: &str, occurred: &str) {
        conn.execute("INSERT OR IGNORE INTO conversations (id) VALUES (?1)", params![cid])
            .unwrap();
        conn.execute(
            "INSERT INTO usage_records (conversation_id, agent, identity, input_tokens, occurred_at) \
             VALUES (?1, 'claude', ?2, 100, ?3)",
            params![cid, identity, occurred],
        )
        .unwrap();
    }

    #[test]
    fn test_usage_stats_days_zero_means_today() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();

        let now = chrono::Local::now();
        let elapsed = now.time().num_seconds_from_midnight() as i64;
        let now_utc = now.with_timezone(&Utc);
        let fmt = |t: chrono::DateTime<Utc>| t.format("%Y-%m-%dT%H:%M:%S+00:00").to_string();
        let today = fmt(now_utc);
        // 取「本地零点之前、滚动 24h 窗口之内」两窗口交集的中点，任何时刻运行断言都成立
        let old = fmt(now_utc - chrono::Duration::seconds((86_400 + elapsed) / 2));

        insert_usage(&conn, "c:today", "u1", &today);
        insert_usage(&conn, "c:old", "u2", &old);

        // days=0：只统计本地今天零点起的记录
        let stats = fetch_usage_stats(&conn, Some(0)).unwrap();
        assert_eq!(stats.totals.requests, 1);
        assert_eq!(stats.days.unwrap_or(-1), 0);

        // days=1：滚动 24 小时，两条都在窗口内
        let stats = fetch_usage_stats(&conn, Some(1)).unwrap();
        assert_eq!(stats.totals.requests, 2);

        // 不传：全量
        let stats = fetch_usage_stats(&conn, None).unwrap();
        assert_eq!(stats.totals.requests, 2);
        assert!(stats.days.is_none());
    }
}
