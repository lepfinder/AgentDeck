use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::Instant;

pub mod antigravity;
pub mod claude;
pub mod codex;
pub mod cursor;
pub mod hermes;
pub mod workbuddy;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawMessage {
    pub step_index: i64,
    pub role: String,         // "user" | "assistant" | "system" | "tool"
    pub message_type: String, // "text" | "tool_call" | "tool_result"
    pub content: String,
    pub thinking: Option<String>,
    pub created_at: Option<String>,
    pub model_name: Option<String>,
    pub tool_name: Option<String>,
    pub tool_args: Option<String>,
    pub duration_ms: Option<i64>,
    pub token_count: Option<i64>,
    pub images: Option<String>,
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
    let clean = path_str
        .trim_start_matches("file://")
        .trim()
        .trim_matches('"')
        .trim_matches('\'');
    if clean.is_empty() {
        return String::new();
    }
    if !clean.starts_with('/') {
        return clean.to_string();
    }

    let parts: Vec<&str> = clean.split('/').filter(|p| !p.is_empty()).collect();
    for (i, &part) in parts.iter().enumerate() {
        if part == "workspace" {
            // 空前缀会产生 "//workspace/..."，统一成 "/workspace/..."
            if i == 0 {
                if parts.len() >= 3 {
                    return format!("/{}", parts[..3].join("/"));
                }
                if parts.len() >= 2 {
                    return format!("/{}", parts[..2].join("/"));
                }
                return "/workspace".to_string();
            }
            let prefix = format!("/{}", parts[..i].join("/"));
            if i + 2 < parts.len() {
                return format!("{}/{}", prefix, parts[i..=i + 2].join("/"));
            }
            if i + 1 < parts.len() {
                return format!("{}/{}", prefix, parts[i..=i + 1].join("/"));
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

/// 将远端 `/workspace/...` 与本机 `~/workspace/...` 合并为同一身份
pub fn canonicalize_workspace_path(path_str: &str) -> String {
    let root = project_root_from_path(path_str);
    if root.is_empty() {
        return root;
    }

    let parts: Vec<&str> = root.split('/').filter(|p| !p.is_empty()).collect();
    if parts.first() == Some(&"workspace") {
        if let Some(home) = dirs::home_dir() {
            return format!(
                "{}/{}",
                home.to_string_lossy().trim_end_matches('/'),
                parts.join("/")
            );
        }
        return format!("/{}", parts.join("/"));
    }

    root
}

/// 基于消息内容生成稳定指纹，避免仅靠时间戳/条数漏更新
pub fn conversation_content_hash(conv: &RawConversation) -> String {
    let mut hasher = DefaultHasher::new();
    conv.id.hash(&mut hasher);
    conv.title.hash(&mut hasher);
    for msg in &conv.messages {
        msg.step_index.hash(&mut hasher);
        msg.role.hash(&mut hasher);
        msg.message_type.hash(&mut hasher);
        msg.content.hash(&mut hasher);
        msg.thinking.hash(&mut hasher);
        msg.tool_name.hash(&mut hasher);
        msg.tool_args.hash(&mut hasher);
        msg.created_at.hash(&mut hasher);
        msg.images.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
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
        Ok(t) => t
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64(),
        Err(_) => 0.0,
    };
    let file_size = metadata.len() as i64;

    let mut stmt = match conn
        .prepare_cached("SELECT file_mtime, file_size FROM sync_state WHERE source_path = ?")
    {
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
        Ok(t) => t
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64(),
        Err(_) => 0.0,
    };
    let file_size = metadata.len() as i64;
    let now = chrono::Utc::now().to_rfc3339();

    let _ = conn.execute(
        r#"
        INSERT INTO sync_state (source_path, conversation_id, source_type, file_mtime, file_size, synced_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(source_path) DO UPDATE SET
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
    let source_types_json =
        serde_json::to_string(&conv.source_types).unwrap_or_else(|_| "[]".to_string());
    let user_msg_count = conv.messages.iter().filter(|m| m.role == "user").count() as i64;
    let total_msg_count = conv.messages.len() as i64;
    let content_hash = conversation_content_hash(conv);
    let workspace_path = canonicalize_workspace_path(&conv.workspace_path);

    let norm_created = normalize_to_iso(conv.created_at.clone());
    let norm_updated = normalize_to_iso(conv.updated_at.clone()).or_else(|| norm_created.clone());

    // 检查是否存在，并判断内容指纹是否有变化
    let mut exists = false;
    let mut unchanged = false;
    if let Ok(mut stmt) = conn.prepare_cached(
        "SELECT updated_at, message_count, user_message_count, COALESCE(content_hash, '') FROM conversations WHERE id = ?",
    ) {
        if let Ok(mut rows) = stmt.query(params![&conv.id]) {
            if let Ok(Some(row)) = rows.next() {
                exists = true;
                let old_updated: Option<String> = row.get(0).ok();
                let old_total: i64 = row.get(1).unwrap_or(-1);
                let old_user: i64 = row.get(2).unwrap_or(-1);
                let old_hash: String = row.get(3).unwrap_or_default();
                unchanged = if !old_hash.is_empty() {
                    old_hash == content_hash
                } else {
                    old_updated == norm_updated
                        && old_total == total_msg_count
                        && old_user == user_msg_count
                };
            }
        }
    }

    // 时间戳、条数与内容哈希均未变化时无需重写消息
    if unchanged {
        return Ok(false);
    }

    let tx = conn.unchecked_transaction()?;

    tx.execute(
        r#"
        INSERT INTO conversations (
            id, workspace_path, source_app, source_types, title, created_at, updated_at,
            message_count, user_message_count, parse_status, content_hash
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        ON CONFLICT(id) DO UPDATE SET
            workspace_path = excluded.workspace_path,
            source_app = excluded.source_app,
            source_types = excluded.source_types,
            title = excluded.title,
            created_at = excluded.created_at,
            updated_at = excluded.updated_at,
            message_count = excluded.message_count,
            user_message_count = excluded.user_message_count,
            parse_status = excluded.parse_status,
            content_hash = excluded.content_hash
        "#,
        params![
            &conv.id,
            &workspace_path,
            &conv.source_app,
            &source_types_json,
            &conv.title,
            &norm_created,
            &norm_updated,
            total_msg_count,
            user_msg_count,
            &conv.parse_status,
            &content_hash,
        ],
    )?;

    // 删除并重建消息
    tx.execute(
        "DELETE FROM messages WHERE conversation_id = ?",
        params![&conv.id],
    )?;

    {
        let mut msg_stmt = tx.prepare_cached(
            r#"
            INSERT INTO messages (
                conversation_id, step_index, role, message_type, content, thinking,
                tool_name, tool_args, created_at, source, is_truncated, images
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, ?11)
            "#,
        )?;

        for (idx, msg) in conv.messages.iter().enumerate() {
            msg_stmt.execute(params![
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
                &msg.images,
            ])?;
        }
    }

    // 维护 workspaces 记录
    if !workspace_path.is_empty() {
        let display_name = Path::new(&workspace_path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| workspace_path.clone());
        let _ = tx.execute(
            r#"
            INSERT INTO workspaces (workspace_path, display_name, last_updated)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(workspace_path) DO UPDATE SET
                last_updated = CASE WHEN excluded.last_updated > workspaces.last_updated OR workspaces.last_updated IS NULL
                               THEN excluded.last_updated ELSE workspaces.last_updated END
            "#,
            params![&workspace_path, display_name, &norm_updated],
        );
    }

    tx.commit()?;

    Ok(!exists)
}

/// 统一同步引擎调度器
pub struct SyncEngine;

impl SyncEngine {
    pub fn run_all(
        conn: &Connection,
        incremental: bool,
    ) -> (u32, u32, u32, u32, Vec<ImporterStats>) {
        let start = Instant::now();
        let mut total_new = 0;
        let mut total_updated = 0;
        let mut total_skipped = 0;
        let mut total_errors = 0;
        let mut all_stats = Vec::new();

        println!(
            "[AgentDeck SyncEngine] 🚀 开始纯 Rust 原生全源扫描同步 (incremental: {})...",
            incremental
        );

        type ImporterFn = fn(&Connection, bool) -> ImporterStats;
        let importers: [(&str, ImporterFn); 6] = [
            ("Antigravity", antigravity::sync),
            ("Cursor", cursor::sync),
            ("Claude", claude::sync),
            ("Codex", codex::sync),
            ("Hermes", hermes::sync),
            ("WorkBuddy", workbuddy::sync),
        ];

        for (name, importer) in importers {
            let src_start = Instant::now();
            let stat = importer(conn, incremental);
            println!(
                "[AgentDeck SyncEngine] └─ {} 完成 ({}ms) => 新增 {}, 更新 {}, 跳过 {}, 错误 {}",
                name,
                src_start.elapsed().as_millis(),
                stat.new_count,
                stat.updated_count,
                stat.skipped_count,
                stat.error_count
            );

            total_new += stat.new_count;
            total_updated += stat.updated_count;
            total_skipped += stat.skipped_count;
            total_errors += stat.error_count;
            all_stats.push(stat);
        }

        println!(
            "[AgentDeck SyncEngine] ✅ 同步完成 (耗时: {}ms) => 新增: {}, 更新: {}, 跳过: {}, 错误: {}",
            start.elapsed().as_millis(),
            total_new,
            total_updated,
            total_skipped,
            total_errors
        );

        (
            total_new,
            total_updated,
            total_skipped,
            total_errors,
            all_stats,
        )
    }
}
