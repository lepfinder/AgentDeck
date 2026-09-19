use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::Instant;

pub mod antigravity;
pub mod claude;
pub mod codebuddy;
pub mod codex;
pub mod cursor;
pub mod hermes;
pub mod mimo;
pub mod qoder;
pub mod windsurf;
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
    #[serde(default)]
    pub credit: Option<f64>,
    pub images: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawArtifact {
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
    #[serde(default)]
    pub artifacts: Vec<RawArtifact>,
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

/// 清洗启发式抽取的路径候选，剔除 JSON 转义/字段碎片
pub fn sanitize_extracted_path(raw: &str) -> Option<String> {
    let mut s = raw.trim().trim_start_matches("file://").trim();
    if s.is_empty() {
        return None;
    }

    // 截断到第一个明显非路径字符（JSON 残留、@[path] 尾部括号等）
    let cut = s
        .find(|c: char| matches!(c, '"' | '\'' | '\\' | '{' | '}' | '[' | ']' | '\n' | '\r'))
        .unwrap_or(s.len());
    s = s[..cut].trim();
    // @[/Users/.../proj] 一类引用：路径本身不应以括号结尾
    s = s.trim_end_matches(|c: char| {
        matches!(c, ',' | ';' | ':' | ')' | '(' | ']' | '[' | '`' | ' ' | '/')
    });

    // 路径至少保留根斜杠 + 一段
    if !s.starts_with('/') || s.len() < 2 {
        return None;
    }
    if s.contains("toolAction") || s.contains("Active Document") {
        return None;
    }
    // 拒绝源码 import 误抽（/components/ui/button、/assets/...）
    if !is_plausible_workspace_path(s) {
        return None;
    }

    Some(s.to_string())
}

fn is_plausible_workspace_path(s: &str) -> bool {
    s.starts_with("/Users/")
        || s.starts_with("/home/")
        || s.starts_with("/workspace/")
        || s.contains("/workspace/")
}

/// 将远端 `/workspace/...` 与本机 `~/workspace/...` 合并为同一身份
pub fn canonicalize_workspace_path(path_str: &str) -> String {
    let cleaned = match sanitize_extracted_path(path_str) {
        Some(s) => s,
        None => {
            // 非绝对路径（如 slug）仍走原逻辑
            let t = path_str.trim();
            if t.is_empty() || t.starts_with('/') {
                return String::new();
            }
            t.to_string()
        }
    };
    let root = project_root_from_path(&cleaned);
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
        msg.model_name.hash(&mut hasher);
        msg.token_count.hash(&mut hasher);
        msg.credit.map(|c| c.to_bits()).hash(&mut hasher);
    }
    for art in &conv.artifacts {
        art.file_name.hash(&mut hasher);
        art.content.hash(&mut hasher);
        art.updated_at.hash(&mut hasher);
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
    // 套用工作区别名：项目重命名合并后，源日志中的旧路径自动映射到新路径，
    // 防止后续同步把已合并的工作区重新拆分出来
    let workspace_path =
        crate::db::resolve_workspace_alias(conn, &workspace_path).unwrap_or(workspace_path);
    // 会话级覆盖优先：用户把单条会话移到别的项目后，以此为准不被源日志覆盖
    let workspace_path = match crate::db::conversation_workspace_override(conn, &conv.id) {
        Ok(Some(override_path)) => override_path,
        _ => workspace_path,
    };

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

    // 增量同步消息：保留已有消息的创建时间，避免后续聊天导致历史时间漂移
    struct ExistingMsgSnapshot {
        id: i64,
        created_at: Option<String>,
        role: String,
        message_type: String,
        content: String,
        thinking: Option<String>,
        tool_name: Option<String>,
        tool_args: Option<String>,
        images: Option<String>,
        model_name: Option<String>,
        token_count: Option<i64>,
        credit: Option<f64>,
    }

    let mut existing_map: HashMap<i64, ExistingMsgSnapshot> = HashMap::new();
    {
        let mut stmt = tx.prepare_cached(
            r#"
            SELECT id, step_index, created_at, role, message_type, content,
                   thinking, tool_name, tool_args, images, model_name, token_count, credit
            FROM messages
            WHERE conversation_id = ?
            "#,
        )?;
        let rows = stmt.query_map(params![&conv.id], |r| {
            Ok((
                r.get::<_, i64>(1)?, // step_index
                ExistingMsgSnapshot {
                    id: r.get(0)?,
                    created_at: r.get(2).ok(),
                    role: r.get(3).unwrap_or_default(),
                    message_type: r.get(4).unwrap_or_default(),
                    content: r.get(5).unwrap_or_default(),
                    thinking: r.get(6).ok(),
                    tool_name: r.get(7).ok(),
                    tool_args: r.get(8).ok(),
                    images: r.get(9).ok(),
                    model_name: r.get(10).ok(),
                    token_count: r.get(11).ok(),
                    credit: r.get(12).ok(),
                },
            ))
        })?;
        for item in rows.flatten() {
            existing_map.insert(item.0, item.1);
        }
    }

    let mut max_incoming_step: i64 = -1;
    let now_iso = chrono::Utc::now().to_rfc3339();

    {
        let mut insert_stmt = tx.prepare_cached(
            r#"
            INSERT INTO messages (
                conversation_id, step_index, role, message_type, content, thinking,
                tool_name, tool_args, created_at, source, is_truncated, images,
                model_name, token_count, credit
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, ?11, ?12, ?13, ?14)
            "#,
        )?;

        let mut update_stmt = tx.prepare_cached(
            r#"
            UPDATE messages
            SET role = ?1, message_type = ?2, content = ?3, thinking = ?4,
                tool_name = ?5, tool_args = ?6, images = ?7,
                created_at = CASE WHEN created_at IS NOT NULL AND created_at != '' THEN created_at ELSE ?8 END,
                model_name = ?10, token_count = ?11, credit = ?12
            WHERE id = ?9
            "#,
        )?;

        for (idx, msg) in conv.messages.iter().enumerate() {
            let step = msg.step_index.max(idx as i64);
            if step > max_incoming_step {
                max_incoming_step = step;
            }

            if let Some(existing) = existing_map.get(&step) {
                let existing_time = existing
                    .created_at
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let existing_created_empty = existing_time.is_none();

                let needs_update = existing.role != msg.role
                    || existing.message_type != msg.message_type
                    || existing.content != msg.content
                    || existing.thinking != msg.thinking
                    || existing.tool_name != msg.tool_name
                    || existing.tool_args != msg.tool_args
                    || existing.images != msg.images
                    || existing.model_name != msg.model_name
                    || existing.token_count != msg.token_count
                    || existing.credit != msg.credit
                    || (existing_created_empty && msg.created_at.is_some());

                if needs_update {
                    let incoming_time = msg
                        .created_at
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty());
                    let fallback_created = existing_time.or(incoming_time).unwrap_or(&now_iso);

                    update_stmt.execute(params![
                        &msg.role,
                        &msg.message_type,
                        &msg.content,
                        &msg.thinking,
                        &msg.tool_name,
                        &msg.tool_args,
                        &msg.images,
                        fallback_created,
                        existing.id,
                        &msg.model_name,
                        &msg.token_count,
                        &msg.credit,
                    ])?;
                }
            } else {
                let final_created = msg.created_at.as_deref().unwrap_or(&now_iso);
                insert_stmt.execute(params![
                    &conv.id,
                    step,
                    &msg.role,
                    &msg.message_type,
                    &msg.content,
                    &msg.thinking,
                    &msg.tool_name,
                    &msg.tool_args,
                    final_created,
                    &conv.source_app,
                    &msg.images,
                    &msg.model_name,
                    &msg.token_count,
                    &msg.credit,
                ])?;
            }
        }
    }

    // 若客户端截断或回滚了消息，清理多余的旧消息
    if max_incoming_step >= 0 {
        tx.execute(
            "DELETE FROM messages WHERE conversation_id = ? AND step_index > ?",
            params![&conv.id, max_incoming_step],
        )?;
    } else if conv.messages.is_empty() {
        tx.execute(
            "DELETE FROM messages WHERE conversation_id = ?",
            params![&conv.id],
        )?;
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

    // 保存关联的产物文档 (Artifacts)
    if !conv.artifacts.is_empty() {
        let mut artifact_stmt = tx.prepare_cached(
            r#"
            INSERT INTO conversation_artifacts (
                conversation_id, file_name, file_path, title, summary, content,
                user_facing, request_feedback, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(conversation_id, file_name) DO UPDATE SET
                file_path = excluded.file_path,
                title = excluded.title,
                summary = excluded.summary,
                content = excluded.content,
                user_facing = excluded.user_facing,
                request_feedback = excluded.request_feedback,
                updated_at = excluded.updated_at
            "#,
        )?;

        for art in &conv.artifacts {
            artifact_stmt.execute(params![
                &conv.id,
                &art.file_name,
                &art.file_path,
                &art.title,
                &art.summary,
                &art.content,
                if art.user_facing { 1 } else { 0 },
                if art.request_feedback { 1 } else { 0 },
                &art.created_at,
                &art.updated_at,
            ])?;
        }
    }

    tx.commit()?;

    Ok(!exists)
}

/// 统一同步引擎调度器
pub struct SyncEngine;

fn sync_log_ts() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

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
            "[{}] [AgentDeck SyncEngine] 🚀 开始纯 Rust 原生全源扫描同步 (incremental: {})...",
            sync_log_ts(),
            incremental
        );

        type ImporterFn = fn(&Connection, bool) -> ImporterStats;
        let importers: [(&str, ImporterFn); 10] = [
            ("Antigravity", antigravity::sync),
            ("Cursor", cursor::sync),
            ("Claude", claude::sync),
            ("Codex", codex::sync),
            ("Hermes", hermes::sync),
            ("WorkBuddy", workbuddy::sync),
            ("MiMo", mimo::sync),
            ("Windsurf", windsurf::sync),
            ("CodeBuddy", codebuddy::sync),
            ("Qoder", qoder::sync),
        ];

        for (name, importer) in importers {
            let src_start = Instant::now();
            let stat = importer(conn, incremental);
            println!(
                "[{}] [AgentDeck SyncEngine] └─ {} 完成 ({}ms) => 新增 {}, 更新 {}, 跳过 {}, 错误 {}",
                sync_log_ts(),
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
            "[{}] [AgentDeck SyncEngine] ✅ 同步完成 (耗时: {}ms) => 新增: {}, 更新: {}, 跳过: {}, 错误: {}",
            sync_log_ts(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_incremental_save_locks_created_at() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();

        let conv_id = "test:conv1".to_string();
        let old_time = "2026-09-01T10:00:00+00:00".to_string();
        let new_time = "2026-09-03T10:00:00+00:00".to_string();

        let make_msg = |idx: i64, content: &str, time: Option<String>| RawMessage {
            step_index: idx,
            role: "user".to_string(),
            message_type: "text".to_string(),
            content: content.to_string(),
            thinking: None,
            created_at: time,
            model_name: None,
            tool_name: None,
            tool_args: None,
            duration_ms: None,
            token_count: None,
            credit: None,
            images: None,
        };

        // 1. 首次保存 2 条历史消息
        let conv_v1 = RawConversation {
            id: conv_id.clone(),
            title: "测试会话".to_string(),
            workspace_path: "/test/ws".to_string(),
            source_app: "cursor".to_string(),
            created_at: Some(old_time.clone()),
            updated_at: Some(old_time.clone()),
            parse_status: "ok".to_string(),
            source_types: vec!["cursor".to_string()],
            messages: vec![
                make_msg(0, "历史消息0", Some(old_time.clone())),
                make_msg(1, "历史消息1", Some(old_time.clone())),
            ],
            artifacts: vec![],
        };
        let is_new = save_conversation_tx(&conn, &conv_v1).unwrap();
        assert!(is_new);

        // 2. 第二次保存：模拟两天后追加了一条消息，并且旧消息被传入了今天的时间（试图漂移）
        let conv_v2 = RawConversation {
            id: conv_id.clone(),
            title: "测试会话".to_string(),
            workspace_path: "/test/ws".to_string(),
            source_app: "cursor".to_string(),
            created_at: Some(old_time.clone()),
            updated_at: Some(new_time.clone()),
            parse_status: "ok".to_string(),
            source_types: vec!["cursor".to_string()],
            messages: vec![
                make_msg(0, "历史消息0", Some(new_time.clone())), // 试图用新时间覆盖
                make_msg(1, "历史消息1 (流式补全)", Some(new_time.clone())), // 模拟内容微调且带新时间
                make_msg(2, "今天追加的新消息", Some(new_time.clone())),
            ],
            artifacts: vec![],
        };
        let is_new2 = save_conversation_tx(&conn, &conv_v2).unwrap();
        assert!(!is_new2);

        // 查询数据库验证时间戳
        let mut stmt = conn
            .prepare("SELECT step_index, content, created_at FROM messages WHERE conversation_id = ? ORDER BY step_index ASC")
            .unwrap();
        let rows: Vec<(i64, String, Option<String>)> = stmt
            .query_map(params![&conv_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap()
            .flatten()
            .collect();

        assert_eq!(rows.len(), 3);
        // step 0 时间必须严格锁定为 old_time
        assert_eq!(rows[0].0, 0);
        assert_eq!(rows[0].2.as_deref(), Some(old_time.as_str()));

        // step 1 内容已更新，但时间必须依然严格锁定为 old_time
        assert_eq!(rows[1].0, 1);
        assert_eq!(rows[1].1, "历史消息1 (流式补全)");
        assert_eq!(rows[1].2.as_deref(), Some(old_time.as_str()));

        // step 2 为全新插入，时间为 new_time
        assert_eq!(rows[2].0, 2);
        assert_eq!(rows[2].2.as_deref(), Some(new_time.as_str()));

        // 3. 第三次保存：测试截断/回滚，只剩 step 0
        let conv_v3 = RawConversation {
            id: conv_id.clone(),
            title: "测试会话".to_string(),
            workspace_path: "/test/ws".to_string(),
            source_app: "cursor".to_string(),
            created_at: Some(old_time.clone()),
            updated_at: Some(new_time.clone()),
            parse_status: "ok".to_string(),
            source_types: vec!["cursor".to_string()],
            messages: vec![make_msg(0, "历史消息0", Some(old_time.clone()))],
            artifacts: vec![],
        };
        save_conversation_tx(&conn, &conv_v3).unwrap();

        let remaining_cnt: i64 = conn
            .query_row(
                "SELECT count(*) FROM messages WHERE conversation_id = ?",
                params![&conv_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(remaining_cnt, 1);
    }

    #[test]
    fn test_sanitize_extracted_path_strips_json_junk() {
        let dirty = r#"/Users/xiyangxie/workspace/chuhai/notix\"","toolAction":"\"Listing"#;
        let clean = sanitize_extracted_path(dirty).unwrap();
        assert_eq!(clean, "/Users/xiyangxie/workspace/chuhai/notix");

        let trailing = "/Users/xiyangxie/workspace/github/deepseek-harness,";
        assert_eq!(
            sanitize_extracted_path(trailing).as_deref(),
            Some("/Users/xiyangxie/workspace/github/deepseek-harness")
        );

        let with_bracket = "/Users/xiyangxie/workspace/personal/xiaonuan-web]";
        assert_eq!(
            sanitize_extracted_path(with_bracket).as_deref(),
            Some("/Users/xiyangxie/workspace/personal/xiaonuan-web")
        );
        assert_eq!(
            canonicalize_workspace_path(with_bracket),
            "/Users/xiyangxie/workspace/personal/xiaonuan-web"
        );

        assert!(sanitize_extracted_path("/components/ui/button'\\nimport").is_none());

        let canon = canonicalize_workspace_path(dirty);
        assert_eq!(canon, "/Users/xiyangxie/workspace/chuhai/notix");
    }
}
