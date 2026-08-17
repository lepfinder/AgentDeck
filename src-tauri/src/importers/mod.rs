use serde::{Deserialize, Serialize};
use rusqlite::{params, Connection, Result};
use std::path::Path;
use std::time::Instant;

pub mod antigravity;
pub mod cursor;
pub mod claude;
pub mod codex;
pub mod hermes;
pub mod workbuddy;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawMessage {
    pub step_index: i64,
    pub role: String, // "user" | "assistant" | "system" | "tool"
    pub message_type: String, // "text" | "tool_call" | "tool_result"
    pub content: String,
    pub thinking: Option<String>,
    pub created_at: Option<String>,
    pub model_name: Option<String>,
    pub tool_name: Option<String>,
    pub tool_args: Option<String>,
    pub duration_ms: Option<i64>,
    pub token_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawConversation {
    pub id: String,
    pub title: String,
    pub workspace_path: String,
    pub source_app: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub parse_status: String,
    pub source_types: Vec<String>,
    pub messages: Vec<RawMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImporterStats {
    pub app: String,
    pub new_count: u32,
    pub updated_count: u32,
    pub skipped_count: u32,
    pub error_count: u32,
}

/// 将文件/目录路径归一到项目根：在 workspace 下保留 bucket/project 两级 (对齐 parser.py project_root_from_path)
pub fn project_root_from_path(path_str: &str) -> String {
    let clean = path_str.trim_start_matches("file://").trim().trim_matches('"').trim_matches('\'');
    if !clean.starts_with('/') {
        return clean.to_string();
    }

    let parts: Vec<&str> = clean.split('/').filter(|p| !p.is_empty()).collect();
    for (i, &part) in parts.iter().enumerate() {
        if part == "workspace" {
            let prefix = format!("/{}", parts[..i].join("/"));
            if i + 2 < parts.len() {
                return format!("{}/{}", prefix, parts[i..i + 3].join("/"));
            }
            if i + 1 < parts.len() {
                return format!("{}/{}", prefix, parts[i..i + 2].join("/"));
            }
            return format!("{}/workspace", prefix);
        }
    }

    // ~/.gemini/antigravity-ide 等特殊主目录
    for (i, &part) in parts.iter().enumerate() {
        if part == ".gemini" && i + 1 < parts.len() {
            return format!("/{}", parts[..i + 2].join("/"));
        }
    }

    if parts.len() >= 3 {
        return format!("/{}", parts[..3].join("/"));
    }
    clean.to_string()
}

/// 检查文件是否需要增量同步
pub fn needs_sync(conn: &Connection, file_path: &Path, incremental: bool) -> bool {
    if !incremental {
        return true;
    }
    let metadata = match file_path.metadata() {
        Ok(m) => m,
        Err(_) => return false,
    };

    let path_str = file_path.to_string_lossy();
    let mtime_sec = match metadata.modified() {
        Ok(t) => t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs_f64(),
        Err(_) => 0.0,
    };
    let file_size = metadata.len() as i64;

    let mut stmt = match conn.prepare_cached("SELECT file_mtime, file_size FROM sync_state WHERE file_path = ?") {
        Ok(s) => s,
        Err(_) => return true,
    };

    let mut rows = stmt.query(params![path_str]).ok();
    if let Some(ref mut r) = rows {
        if let Ok(Some(row)) = r.next() {
            let prev_mtime: f64 = row.get(0).unwrap_or(0.0);
            let prev_size: i64 = row.get(1).unwrap_or(0);
            if (prev_mtime - mtime_sec).abs() < 0.001 && prev_size == file_size {
                return false;
            }
        }
    }
    true
}

/// 记录同步状态
pub fn record_sync_state(conn: &Connection, file_path: &Path, cid: &str, source_type: &str) {
    let path_str = file_path.to_string_lossy();
    let metadata = match file_path.metadata() {
        Ok(m) => m,
        Err(_) => return,
    };
    let mtime_sec = match metadata.modified() {
        Ok(t) => t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs_f64(),
        Err(_) => 0.0,
    };
    let file_size = metadata.len() as i64;
    let now = chrono::Utc::now().to_rfc3339();

    let _ = conn.execute(
        r#"
        INSERT INTO sync_state (file_path, conversation_id, source_type, file_mtime, file_size, synced_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(file_path) DO UPDATE SET
            conversation_id = excluded.conversation_id,
            source_type = excluded.source_type,
            file_mtime = excluded.file_mtime,
            file_size = excluded.file_size,
            synced_at = excluded.synced_at
        "#,
        params![path_str, cid, source_type, mtime_sec, file_size, now],
    );
}

/// 将各类时间字符串统一标准化为 ISO-8601 UTC 格式，彻底解决字符串排序导致的 8 小时时差倒挂
pub fn normalize_to_iso(raw: Option<String>) -> Option<String> {
    let s = raw?.trim().to_string();
    if s.is_empty() {
        return None;
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
        return Some(dt.with_timezone(&chrono::Utc).to_rfc3339());
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S") {
        let dt = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(naive, chrono::Utc);
        return Some(dt.to_rfc3339());
    }
    Some(s)
}

/// 保存单条会话及其所有消息入库
pub fn save_conversation_tx(conn: &Connection, conv: &RawConversation) -> Result<bool> {
    let source_types_json = serde_json::to_string(&conv.source_types).unwrap_or_else(|_| "[]".to_string());
    let user_msg_count = conv.messages.iter().filter(|m| m.role == "user").count() as i64;
    let total_msg_count = conv.messages.len() as i64;

    let norm_created = normalize_to_iso(conv.created_at.clone());
    let norm_updated = normalize_to_iso(conv.updated_at.clone()).or_else(|| norm_created.clone());

    // 检查是否存在
    let mut exists = false;
    if let Ok(mut stmt) = conn.prepare_cached("SELECT 1 FROM conversations WHERE id = ?") {
        if let Ok(mut rows) = stmt.query(params![&conv.id]) {
            if let Ok(Some(_)) = rows.next() {
                exists = true;
            }
        }
    }

    // 确保 conversations 的 source_app 列存在
    let _ = conn.execute("ALTER TABLE conversations ADD COLUMN source_app TEXT", []);

    conn.execute(
        r#"
        INSERT INTO conversations (
            id, workspace_path, source_app, source_types, title, created_at, updated_at,
            message_count, user_message_count, parse_status
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        ON CONFLICT(id) DO UPDATE SET
            workspace_path = excluded.workspace_path,
            source_app = excluded.source_app,
            source_types = excluded.source_types,
            title = excluded.title,
            created_at = excluded.created_at,
            updated_at = excluded.updated_at,
            message_count = excluded.message_count,
            user_message_count = excluded.user_message_count,
            parse_status = excluded.parse_status
        "#,
        params![
            &conv.id,
            &conv.workspace_path,
            &conv.source_app,
            &source_types_json,
            &conv.title,
            &norm_created,
            &norm_updated,
            total_msg_count,
            user_msg_count,
            &conv.parse_status,
        ],
    )?;

    // 删除并重建消息
    conn.execute("DELETE FROM messages WHERE conversation_id = ?", params![&conv.id])?;

    for (idx, msg) in conv.messages.iter().enumerate() {
        conn.execute(
            r#"
            INSERT INTO messages (
                conversation_id, step_index, role, message_type, content, thinking,
                tool_name, tool_args, created_at, source, is_truncated, images
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, NULL)
            "#,
            params![
                &conv.id,
                msg.step_index.max(idx as i64),
                &msg.role,
                &msg.message_type,
                &msg.content,
                &msg.thinking,
                &msg.tool_name,
                &msg.tool_args,
                &msg.created_at,
                &conv.source_app,
            ],
        )?;
    }

    // 维护 workspaces 记录
    if !conv.workspace_path.is_empty() {
        let display_name = Path::new(&conv.workspace_path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| conv.workspace_path.clone());
        let _ = conn.execute(
            r#"
            INSERT INTO workspaces (workspace_path, display_name, last_updated)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(workspace_path) DO UPDATE SET
                last_updated = CASE WHEN excluded.last_updated > workspaces.last_updated OR workspaces.last_updated IS NULL
                               THEN excluded.last_updated ELSE workspaces.last_updated END
            "#,
            params![&conv.workspace_path, display_name, &conv.updated_at],
        );
    }

    Ok(!exists)
}

/// 统一同步引擎调度器
pub struct SyncEngine;

impl SyncEngine {
    pub fn run_all(conn: &Connection, incremental: bool) -> (u32, u32, u32, u32, Vec<ImporterStats>) {
        let start = Instant::now();
        let mut total_new = 0;
        let mut total_updated = 0;
        let mut total_skipped = 0;
        let mut total_errors = 0;
        let mut all_stats = Vec::new();

        println!("[AgentDeck SyncEngine] 🚀 开始纯 Rust 原生全源扫描同步 (incremental: {})...", incremental);

        // 1. Antigravity
        let ag_stat = antigravity::sync(conn, incremental);
        total_new += ag_stat.new_count;
        total_updated += ag_stat.updated_count;
        total_skipped += ag_stat.skipped_count;
        total_errors += ag_stat.error_count;
        all_stats.push(ag_stat);

        // 2. Cursor
        let cursor_stat = cursor::sync(conn, incremental);
        total_new += cursor_stat.new_count;
        total_updated += cursor_stat.updated_count;
        total_skipped += cursor_stat.skipped_count;
        total_errors += cursor_stat.error_count;
        all_stats.push(cursor_stat);

        // 3. Claude Code
        let claude_stat = claude::sync(conn, incremental);
        total_new += claude_stat.new_count;
        total_updated += claude_stat.updated_count;
        total_skipped += claude_stat.skipped_count;
        total_errors += claude_stat.error_count;
        all_stats.push(claude_stat);

        // 4. Codex
        let codex_stat = codex::sync(conn, incremental);
        total_new += codex_stat.new_count;
        total_updated += codex_stat.updated_count;
        total_skipped += codex_stat.skipped_count;
        total_errors += codex_stat.error_count;
        all_stats.push(codex_stat);

        // 5. Hermes
        let hermes_stat = hermes::sync(conn, incremental);
        total_new += hermes_stat.new_count;
        total_updated += hermes_stat.updated_count;
        total_skipped += hermes_stat.skipped_count;
        total_errors += hermes_stat.error_count;
        all_stats.push(hermes_stat);

        // 6. WorkBuddy
        let wb_stat = workbuddy::sync(conn, incremental);
        total_new += wb_stat.new_count;
        total_updated += wb_stat.updated_count;
        total_skipped += wb_stat.skipped_count;
        total_errors += wb_stat.error_count;
        all_stats.push(wb_stat);

        println!(
            "[AgentDeck SyncEngine] ✅ 同步完成 (耗时: {}ms) => 新增: {}, 更新: {}, 跳过: {}, 错误: {}",
            start.elapsed().as_millis(),
            total_new,
            total_updated,
            total_skipped,
            total_errors
        );

        (total_new, total_updated, total_skipped, total_errors, all_stats)
    }
}
