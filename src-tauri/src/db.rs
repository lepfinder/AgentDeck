use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    pub message_count: i64,
    pub user_message_count: i64,
    pub last_updated: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationItem {
    pub id: String,
    pub workspace_path: String,
    pub source_app: String,
    pub title: String,
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

/// 只读连接调优：避免与同步任务争锁时立即失败
pub fn apply_read_pragmas(conn: &Connection) {
    let _ = conn.pragma_update(None, "temp_store", "MEMORY");
    let _ = conn.pragma_update(None, "cache_size", -32768i64);
    let _ = conn.busy_timeout(std::time::Duration::from_secs(15));
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
            FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
        );

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
            updated_at TEXT
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
            category TEXT NOT NULL DEFAULT 'coding',
            tags_json TEXT NOT NULL DEFAULT '[]',
            source_url TEXT,
            source_note TEXT,
            notes TEXT,
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
        "#
    )?;

    ensure_messages_schema(conn)?;

    // 自动兼容性迁移（防止旧表缺少新增字段）
    let _ = conn.execute("ALTER TABLE conversations ADD COLUMN source_app TEXT", []);
    let _ = conn.execute(
        "ALTER TABLE conversations ADD COLUMN content_hash TEXT NOT NULL DEFAULT ''",
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

    // 兼容短暂出现过的 file_path 列名（正式库为 source_path）
    let _ = conn.execute(
        "ALTER TABLE sync_state RENAME COLUMN file_path TO source_path",
        [],
    );

    migrate_workspace_aliases(conn);

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
        _ => ("Other", "#64748b"),
    }
}

fn get_short_workspace(path: &str) -> String {
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
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![start, end], |row| {
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
            "SELECT c.id, c.title,
                    CASE
                        WHEN c.source_types LIKE '%claude%' THEN 'claude'
                        WHEN c.source_types LIKE '%cursor%' THEN 'cursor'
                        WHEN c.source_types LIKE '%codex%' THEN 'codex'
                        WHEN c.source_types LIKE '%workbuddy%' THEN 'workbuddy'
                        WHEN c.source_types LIKE '%hermes%' THEN 'hermes'
                        ELSE 'antigravity'
                    END as source_app,
                    c.workspace_path, c.message_count, c.user_message_count, c.updated_at,
                    (SELECT COUNT(*) FROM starred_sessions s WHERE s.conversation_id = c.id) as is_starred
             FROM conversations c
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

pub fn fetch_workspaces(conn: &Connection, search: Option<&str>) -> Result<Vec<WorkspaceStat>> {
    let mut list = Vec::new();
    let sql = r#"
        SELECT
            workspace_path,
            COUNT(*) as cnt,
            SUM(CASE WHEN source_types LIKE '%transcript%' OR source_types LIKE '%sqlite_db%' OR source_types LIKE '%overview%' THEN 1 ELSE 0 END) as ag_cnt,
            SUM(CASE WHEN source_types LIKE '%cursor%' THEN 1 ELSE 0 END) as cursor_cnt,
            SUM(CASE WHEN source_types LIKE '%claude%' THEN 1 ELSE 0 END) as claude_cnt,
            SUM(CASE WHEN source_types LIKE '%codex%' THEN 1 ELSE 0 END) as codex_cnt,
            SUM(CASE WHEN source_types LIKE '%workbuddy%' THEN 1 ELSE 0 END) as wb_cnt,
            SUM(CASE WHEN source_types LIKE '%hermes%' THEN 1 ELSE 0 END) as hermes_cnt,
            SUM(message_count) as message_count,
            SUM(user_message_count) as user_message_count,
            MAX(updated_at) as last_updated
        FROM conversations
        WHERE (?1 IS NULL OR ?1 = '' OR workspace_path LIKE '%' || ?1 || '%')
        GROUP BY workspace_path
        ORDER BY last_updated DESC
    "#;

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![search.unwrap_or("")], |row| {
        Ok(WorkspaceStat {
            workspace_path: row.get(0)?,
            cnt: row.get(1)?,
            ag_cnt: row.get(2).unwrap_or(0),
            cursor_cnt: row.get(3).unwrap_or(0),
            claude_cnt: row.get(4).unwrap_or(0),
            codex_cnt: row.get(5).unwrap_or(0),
            wb_cnt: row.get(6).unwrap_or(0),
            hermes_cnt: row.get(7).unwrap_or(0),
            message_count: row.get(8).unwrap_or(0),
            user_message_count: row.get(9).unwrap_or(0),
            last_updated: to_beijing_iso(row.get(10)?),
        })
    })?;

    for r in rows.flatten() {
        list.push(r);
    }
    Ok(list)
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
                ELSE 'antigravity'
            END as source_app,
            c.title,
            c.created_at,
            c.updated_at,
            c.message_count,
            c.user_message_count,
            c.parse_status,
            (SELECT COUNT(*) FROM starred_sessions s WHERE s.conversation_id = c.id) as is_starred
        FROM conversations c
        WHERE (?1 = 0 OR (SELECT COUNT(*) FROM starred_sessions s WHERE s.conversation_id = c.id) > 0)
          AND (?2 IS NULL OR ?2 = '' OR c.workspace_path = ?2)
          AND (?3 IS NULL OR ?3 = '' OR c.title LIKE '%' || ?3 || '%')
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
            let is_starred_cnt: i64 = row.get(9)?;
            let raw_created: Option<String> = row.get(4)?;
            let raw_updated: Option<String> = row.get(5)?;
            Ok(ConversationItem {
                id: row.get(0)?,
                workspace_path: row.get(1)?,
                source_app: row.get(2)?,
                title: row.get(3)?,
                created_at: to_beijing_iso(raw_created),
                updated_at: to_beijing_iso(raw_updated),
                message_count: row.get(6)?,
                user_message_count: row.get(7)?,
                parse_status: row.get(8)?,
                is_starred: is_starred_cnt > 0,
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
                source as model_name,
                NULL as token_count,
                NULL as duration_ms,
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
            tool_calls_json: row.get(10)?,
            images: row.get(11)?,
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
        SELECT m.id, m.conversation_id, c.title,
               CASE
                   WHEN c.source_types LIKE '%claude%' THEN 'claude'
                   WHEN c.source_types LIKE '%cursor%' THEN 'cursor'
                   WHEN c.source_types LIKE '%codex%' THEN 'codex'
                   WHEN c.source_types LIKE '%workbuddy%' THEN 'workbuddy'
                   WHEN c.source_types LIKE '%hermes%' THEN 'hermes'
                   ELSE 'antigravity'
               END as source_app,
               c.workspace_path,
               CASE WHEN m.role LIKE '%user%' THEN 'user' WHEN m.role LIKE '%tool%' THEN 'tool' ELSE 'assistant' END as sender,
               m.content as text,
               m.created_at
        FROM messages m
        JOIN conversations c ON m.conversation_id = c.id
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
        user_message_count,
        message_count,
        first_active,
        last_active
    ): (i64, i64, i64, i64, i64, i64, i64, i64, i64, Option<String>, Option<String>) = conn.query_row(
        r#"
        SELECT
            COUNT(*),
            SUM(CASE WHEN source_types LIKE '%transcript%' OR source_types LIKE '%sqlite_db%' OR source_types LIKE '%overview%' THEN 1 ELSE 0 END),
            SUM(CASE WHEN source_types LIKE '%cursor%' THEN 1 ELSE 0 END),
            SUM(CASE WHEN source_types LIKE '%claude%' THEN 1 ELSE 0 END),
            SUM(CASE WHEN source_types LIKE '%codex%' THEN 1 ELSE 0 END),
            SUM(CASE WHEN source_types LIKE '%workbuddy%' THEN 1 ELSE 0 END),
            SUM(CASE WHEN source_types LIKE '%hermes%' THEN 1 ELSE 0 END),
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
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
                r.get(10)?,
            ))
        }
    ).unwrap_or((0, 0, 0, 0, 0, 0, 0, 0, 0, None, None));

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
    let now = Utc::now().to_rfc3339();
    conn.execute(
        r#"
        INSERT INTO workspace_reports (workspace_path, report_md, updated_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(workspace_path) DO UPDATE SET
            report_md = excluded.report_md,
            updated_at = excluded.updated_at
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptCategoryOption {
    pub value: String,
    pub label: String,
}

pub fn default_prompt_category() -> String {
    "coding".to_string()
}

pub fn prompt_category_options() -> Vec<PromptCategoryOption> {
    vec![
        PromptCategoryOption {
            value: "coding".into(),
            label: "编程".into(),
        },
        PromptCategoryOption {
            value: "research".into(),
            label: "研究".into(),
        },
        PromptCategoryOption {
            value: "writing".into(),
            label: "写作".into(),
        },
        PromptCategoryOption {
            value: "product".into(),
            label: "产品 / 设计".into(),
        },
        PromptCategoryOption {
            value: "agent".into(),
            label: "Agent / 自动化".into(),
        },
        PromptCategoryOption {
            value: "image".into(),
            label: "生图".into(),
        },
        PromptCategoryOption {
            value: "video".into(),
            label: "生视频".into(),
        },
        PromptCategoryOption {
            value: "persona".into(),
            label: "角色 / 人格".into(),
        },
        PromptCategoryOption {
            value: "meta".into(),
            label: "元提示词".into(),
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
        trimmed.to_string()
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

impl PromptAgentItem {
    pub fn list_from(item: &PromptItem) -> Self {
        Self {
            id: item.id,
            title: item.title.clone(),
            content: None,
            content_preview: Some(prompt_content_preview(&item.content)),
            category: item.category.clone(),
            tags: item.tags.clone(),
            source_url: item.source_url.clone(),
            source_note: item.source_note.clone(),
            notes: item.notes.clone(),
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
            source_url: item.source_url.clone(),
            source_note: item.source_note.clone(),
            notes: item.notes.clone(),
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
    pub is_starred: bool,
}

fn row_to_prompt_item(row: &rusqlite::Row<'_>) -> Result<PromptItem> {
    let tags_json: String = row.get(4)?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
    Ok(PromptItem {
        id: row.get(0)?,
        title: row.get(1)?,
        content: row.get(2)?,
        category: row.get(3)?,
        tags,
        source_url: row.get(5)?,
        source_note: row.get(6)?,
        notes: row.get(7)?,
        is_starred: row.get::<_, i64>(8)? != 0,
        use_count: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        last_used_at: row.get(12)?,
    })
}

const PROMPT_SELECT: &str = r#"
    SELECT id, title, content, category, tags_json, source_url, source_note, notes,
           is_starred, use_count, created_at, updated_at, last_used_at
    FROM prompts
"#;

pub fn list_prompts(
    conn: &Connection,
    search: Option<&str>,
    category: Option<&str>,
    starred_only: bool,
) -> Result<Vec<PromptItem>> {
    let mut list = Vec::new();
    let sql = format!(
        r#"{PROMPT_SELECT}
        WHERE (?1 IS NULL OR ?1 = '' OR title LIKE '%' || ?1 || '%' OR content LIKE '%' || ?1 || '%' OR tags_json LIKE '%' || ?1 || '%')
          AND (?2 IS NULL OR ?2 = '' OR category = ?2)
          AND (?3 = 0 OR is_starred = 1)
        ORDER BY is_starred DESC, updated_at DESC, id DESC"#
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
        list.push(r);
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
    conn.execute(
        r#"
        INSERT INTO prompts (
            title, content, category, tags_json, source_url, source_note, notes,
            is_starred, use_count, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, ?9)
        "#,
        params![
            title,
            input.content,
            category,
            tags_json,
            input.source_url,
            input.source_note,
            input.notes,
            if input.is_starred { 1 } else { 0 },
            now,
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
            is_starred = ?8,
            updated_at = ?9
        WHERE id = ?10
        "#,
        params![
            title,
            input.content,
            category,
            tags_json,
            input.source_url,
            input.source_note,
            input.notes,
            if input.is_starred { 1 } else { 0 },
            now,
            id,
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
