use serde::{Deserialize, Serialize};
use rusqlite::{params, Connection, Result};
use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::Mutex;
use chrono::{DateTime, Utc, Local, Timelike};

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
    pub tool_usage: Vec<ToolUsageStat>,
    pub top_conversations_all: Vec<TopRankItem>,
    pub top_conversations_user: Vec<TopRankItem>,
    pub top_workspaces: Vec<TopWorkspaceItem>,
    pub last_sync_time: Option<String>,
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
pub struct HeatmapCell {
    pub date: String,
    pub count: i64,
    pub level: u32,
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
        init_schema(&conn)?;
        Ok(Self {
            conn_mutex: Mutex::new(conn),
        })
    }
}

pub fn get_database_path() -> PathBuf {
    // 优先复用已有 aicoding-chat-viewer 的本地 SQLite 数据库（保证 700+ 会话和 11万+ 消息瞬间可用）
    if let Some(home) = dirs::home_dir() {
        let candidates = [
            home.join("workspace/personal/aicoding-chat-viewer/data/antigravity_chats.db"),
            home.join(".aicoding-chat-viewer/data/antigravity_chats.db"),
            home.join(".aicoding-chat-viewer/conversations.db"),
            home.join(".agentdeck/conversations.db"),
        ];

        for path in &candidates {
            if path.exists() {
                if let Ok(metadata) = std::fs::metadata(path) {
                    if metadata.len() > 1024 * 50 { // 大于 50KB 则认为是有实际数据的数据库
                        return path.clone();
                    }
                }
            }
        }

        // 如果上述大库不存在但有已存在路径
        for path in &candidates {
            if path.exists() {
                return path.clone();
            }
        }

        let default_app_db = home.join(".agentdeck").join("conversations.db");
        return default_app_db;
    }
    PathBuf::from("conversations.db")
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
            workspace_path TEXT,
            source_app TEXT,
            source_types TEXT DEFAULT '[]',
            title TEXT,
            created_at TEXT,
            updated_at TEXT,
            message_count INTEGER DEFAULT 0,
            user_message_count INTEGER DEFAULT 0,
            parse_status TEXT DEFAULT 'ok'
        );

        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            conversation_id TEXT,
            sender TEXT,
            text TEXT,
            created_at TEXT,
            model_name TEXT,
            token_count INTEGER,
            duration_ms INTEGER,
            tool_calls_json TEXT
        );

        CREATE TABLE IF NOT EXISTS starred_sessions (
            conversation_id TEXT PRIMARY KEY,
            starred_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_conv_workspace ON conversations(workspace_path);
        CREATE INDEX IF NOT EXISTS idx_conv_updated ON conversations(updated_at);
        "#
    )?;
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

pub fn fetch_dashboard_stats(conn: &Connection) -> Result<DashboardStats> {
    // 1. KPI 基础统计
    let total_conversations: i64 = conn.query_row(
        "SELECT COUNT(*) FROM conversations",
        [],
        |r| r.get(0),
    ).unwrap_or(0);

    let total_messages: i64 = conn.query_row(
        "SELECT COALESCE(SUM(message_count), (SELECT COUNT(*) FROM messages)) FROM conversations",
        [],
        |r| r.get(0),
    ).unwrap_or(0);

    let total_user_messages: i64 = conn.query_row(
        "SELECT COALESCE(SUM(user_message_count), 0) FROM conversations",
        [],
        |r| r.get(0),
    ).unwrap_or(0);

    let total_workspaces: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT workspace_path) FROM conversations WHERE workspace_path IS NOT NULL AND workspace_path != ''",
        [],
        |r| r.get(0),
    ).unwrap_or(0);

    let starred_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM starred_sessions",
        [],
        |r| r.get(0),
    ).unwrap_or(0);

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
         ORDER BY cnt DESC"
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
         ORDER BY cnt DESC"
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

    // 4. 24 小时活跃时段（按消息数 & 按会话数）
    let mut hourly_msgs = vec![0i64; 24];
    let mut stmt = conn.prepare(
        "SELECT created_at FROM messages WHERE created_at IS NOT NULL AND created_at != '' LIMIT 50000"
    )?;
    let time_rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    for t in time_rows.flatten() {
        if let Ok(dt) = DateTime::parse_from_rfc3339(&t) {
            let hour = dt.with_timezone(&Local).hour() as usize;
            if hour < 24 {
                hourly_msgs[hour] += 1;
            }
        } else if t.len() >= 13 && t.contains(' ') {
            if let Some(h_str) = t.split(' ').nth(1).and_then(|s| s.split(':').next()) {
                if let Ok(h) = h_str.parse::<usize>() {
                    if h < 24 {
                        hourly_msgs[h] += 1;
                    }
                }
            }
        }
    }

    let mut hourly_convs = vec![0i64; 24];
    let mut stmt = conn.prepare(
        "SELECT updated_at FROM conversations WHERE updated_at IS NOT NULL AND updated_at != ''"
    )?;
    let conv_time_rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    for t in conv_time_rows.flatten() {
        if let Ok(dt) = DateTime::parse_from_rfc3339(&t) {
            let hour = dt.with_timezone(&Local).hour() as usize;
            if hour < 24 {
                hourly_convs[hour] += 1;
            }
        } else if t.len() >= 13 && t.contains(' ') {
            if let Some(h_str) = t.split(' ').nth(1).and_then(|s| s.split(':').next()) {
                if let Ok(h) = h_str.parse::<usize>() {
                    if h < 24 {
                        hourly_convs[h] += 1;
                    }
                }
            }
        }
    }

    let max_msg_hour = *hourly_msgs.iter().max().unwrap_or(&1).max(&1) as f64;
    let punchcard_msgs: Vec<PunchcardSlot> = hourly_msgs.iter().enumerate().map(|(h, &cnt)| {
        let ratio = cnt as f64 / max_msg_hour;
        let level = if cnt == 0 { 0 } else if ratio < 0.25 { 1 } else if ratio < 0.55 { 2 } else if ratio < 0.85 { 3 } else { 4 };
        PunchcardSlot {
            hour: h as u32,
            count: cnt,
            level,
            percent: (ratio * 100.0).round(),
        }
    }).collect();

    let max_conv_hour = *hourly_convs.iter().max().unwrap_or(&1).max(&1) as f64;
    let punchcard_convs: Vec<PunchcardSlot> = hourly_convs.iter().enumerate().map(|(h, &cnt)| {
        let ratio = cnt as f64 / max_conv_hour;
        let level = if cnt == 0 { 0 } else if ratio < 0.25 { 1 } else if ratio < 0.55 { 2 } else if ratio < 0.85 { 3 } else { 4 };
        PunchcardSlot {
            hour: h as u32,
            count: cnt,
            level,
            percent: (ratio * 100.0).round(),
        }
    }).collect();

    // 5. Tool Usage 工具调用分布分析
    let mut total_tool_calls = 0i64;
    let mut tool_categories: HashMap<&'static str, i64> = HashMap::new();

    // 优先从 messages.tool_name 直接查询
    let mut stmt = conn.prepare(
        "SELECT tool_name FROM messages WHERE tool_name IS NOT NULL AND tool_name != ''"
    )?;
    let tool_rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    for raw_name in tool_rows.flatten() {
        total_tool_calls += 1;
        let name = raw_name.to_lowercase();
        if name.contains("read") || name.contains("view") || name.contains("list") || name.contains("cat") {
            *tool_categories.entry("文件阅读").or_insert(0) += 1;
        } else if name.contains("edit") || name.contains("write") || name.contains("replace") || name.contains("patch") {
            *tool_categories.entry("代码编辑").or_insert(0) += 1;
        } else if name.contains("bash") || name.contains("cmd") || name.contains("terminal") || name.contains("run") || name.contains("exec") {
            *tool_categories.entry("终端命令").or_insert(0) += 1;
        } else if name.contains("search") || name.contains("grep") || name.contains("find") || name.contains("query") {
            *tool_categories.entry("搜索检索").or_insert(0) += 1;
        } else if name.contains("skill") || name.contains("mcp") || name.contains("plugin") || name.contains("image") || name.contains("schedule") || name.contains("task") {
            *tool_categories.entry("技能扩展").or_insert(0) += 1;
        } else if name.contains("browser") || name.contains("web") || name.contains("http") || name.contains("fetch") || name.contains("url") {
            *tool_categories.entry("网络与浏览器").or_insert(0) += 1;
        } else {
            *tool_categories.entry("其他工具").or_insert(0) += 1;
        }
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
            let updated_at: Option<String> = row.get(6)?;
            let is_starred_cnt: i64 = row.get(7)?;
            let (label, _) = source_to_label_and_color(&source_app);
            let ws_short = get_short_workspace(&workspace_path);
            Ok(TopRankItem {
                id,
                title: if title.is_empty() { "未命名会话".to_string() } else { title },
                source_app,
                source_label: label.to_string(),
                workspace_path,
                workspace_short: ws_short,
                message_count,
                user_message_count,
                updated_at,
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

    let last_sync_time: Option<String> = conn.query_row(
        "SELECT COALESCE(finished_at, created_at) FROM sync_runs ORDER BY id DESC LIMIT 1",
        [],
        |r| r.get(0),
    ).ok();

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
        tool_usage,
        top_conversations_all,
        top_conversations_user,
        top_workspaces,
        last_sync_time,
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
            last_updated: row.get(10)?,
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
            Ok(ConversationItem {
                id: row.get(0)?,
                workspace_path: row.get(1)?,
                source_app: row.get(2)?,
                title: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
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

pub fn fetch_conversation_messages(conn: &Connection, conversation_id: &str) -> Result<Vec<MessageItem>> {
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
                CASE WHEN tool_name IS NOT NULL AND tool_name != '' THEN json_array(json_object('name', tool_name, 'args', tool_args)) ELSE NULL END as tool_calls_json
         FROM messages
         WHERE conversation_id = ?1
         ORDER BY step_index ASC, id ASC"
    )?;

    let rows = stmt.query_map(params![conversation_id], |row| {
        let id_val: i64 = row.get(0)?;
        Ok(MessageItem {
            id: id_val.to_string(),
            conversation_id: row.get(1)?,
            step_index: row.get(2)?,
            sender: row.get(3)?,
            text: row.get(4)?,
            thinking: row.get(5)?,
            created_at: row.get(6)?,
            model_name: row.get(7)?,
            token_count: row.get(8)?,
            duration_ms: row.get(9)?,
            tool_calls_json: row.get(10)?,
        })
    })?;

    for r in rows.flatten() {
        list.push(r);
    }
    Ok(list)
}

pub fn toggle_star_session(conn: &Connection, conversation_id: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM starred_sessions WHERE conversation_id = ?1",
        params![conversation_id],
        |r| r.get(0),
    ).unwrap_or(0);

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
    let rows = stmt.query_map(params![if is_user { 1 } else { 0 }, query, limit as i64], |row| {
        let id_val: i64 = row.get(0)?;
        let text: String = row.get(6)?;
        let snippet = if text.chars().count() > 180 {
            let s: String = text.chars().take(180).collect();
            format!("{}...", s)
        } else {
            text
        };
        Ok(SearchResultItem {
            message_id: id_val.to_string(),
            conversation_id: row.get(1)?,
            conversation_title: row.get(2)?,
            source_app: row.get(3)?,
            workspace_path: row.get(4)?,
            sender: row.get(5)?,
            snippet,
            created_at: row.get(7)?,
        })
    })?;

    for r in rows.flatten() {
        list.push(r);
    }
    Ok(list)
}

pub fn fetch_workspace_detail_stats(conn: &Connection, workspace_path: &str) -> Result<WorkspaceDetailStats> {
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
    if ag_cnt > 0 { breakdown_parts.push(format!("AG {}", ag_cnt)); }
    if cursor_cnt > 0 { breakdown_parts.push(format!("Cursor {}", cursor_cnt)); }
    if claude_cnt > 0 { breakdown_parts.push(format!("Claude {}", claude_cnt)); }
    if hermes_cnt > 0 { breakdown_parts.push(format!("Hermes {}", hermes_cnt)); }
    if wb_cnt > 0 { breakdown_parts.push(format!("WorkBuddy {}", wb_cnt)); }
    if codex_cnt > 0 { breakdown_parts.push(format!("Codex {}", codex_cnt)); }
    let agent_breakdown = if breakdown_parts.is_empty() {
        format!("共 {} 会话", conversation_count)
    } else {
        breakdown_parts.join(" · ")
    };

    // 每日活跃消息统计
    let mut daily_counts: HashMap<String, i64> = HashMap::new();
    let mut stmt = conn.prepare(
        r#"
        SELECT substr(m.created_at, 1, 10) as day, COUNT(*)
        FROM messages m
        JOIN conversations c ON c.id = m.conversation_id
        WHERE c.workspace_path = ?1 AND m.created_at IS NOT NULL AND length(m.created_at) >= 10
        GROUP BY day
        "#
    )?;
    let rows = stmt.query_map(params![workspace_path], |r| {
        let day: String = r.get(0)?;
        let count: i64 = r.get(1)?;
        Ok((day, count))
    })?;
    for item in rows.flatten() {
        daily_counts.insert(item.0, item.1);
    }

    let active_days = daily_counts.len() as i64;
    let peak_item = daily_counts.iter().max_by_key(|(_, &cnt)| cnt);
    let (peak_day, peak_count) = match peak_item {
        Some((d, &cnt)) => (Some(d.clone()), cnt),
        None => (None, 0),
    };

    // 生成 52 周 (364天) 热力图格子
    let mut heatmap_cells = Vec::new();
    let today = Local::now().date_naive();
    let max_count = *daily_counts.values().max().unwrap_or(&1).max(&1) as f64;
    for i in (0..364).rev() {
        let d = today - chrono::Duration::days(i);
        let date_str = d.format("%Y-%m-%d").to_string();
        let cnt = *daily_counts.get(&date_str).unwrap_or(&0);
        let ratio = cnt as f64 / max_count;
        let level = if cnt == 0 { 0 } else if ratio <= 0.25 { 1 } else if ratio <= 0.5 { 2 } else if ratio <= 0.75 { 3 } else { 4 };
        heatmap_cells.push(HeatmapCell {
            date: date_str,
            count: cnt,
            level,
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
    let has_rep_table: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='workspace_reports'",
        [],
        |r| r.get(0)
    ).unwrap_or(0);
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
        first_active,
        last_active,
        active_days,
        peak_day,
        peak_count,
        heatmap_cells,
        fine_blocks,
        module_blocks,
        report_md,
    })
}
