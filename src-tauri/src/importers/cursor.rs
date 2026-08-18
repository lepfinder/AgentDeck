use regex::Regex;
use rusqlite::{params, Connection, OpenFlags};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;
use walkdir::WalkDir;

use super::{
    canonicalize_workspace_path, needs_sync, record_sync_state, save_conversation_tx,
    ImporterStats, RawConversation, RawMessage,
};

/// 解析格式版本：变更后即使 Cursor 主库未改动，也强制重写已入库会话的消息时间戳
const CURSOR_PARSER_REV: &str = "bubble-created-at-v1";
const CURSOR_PARSER_REV_KEY: &str = "agentdeck:cursor_parser_rev";

pub fn sync(conn: &Connection, incremental: bool) -> ImporterStats {
    let mut stats = ImporterStats {
        app: "Cursor".to_string(),
        new_count: 0,
        updated_count: 0,
        skipped_count: 0,
        error_count: 0,
    };

    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return stats,
    };

    let cursor_db = home.join("Library/Application Support/Cursor/User/globalStorage/state.vscdb");
    if !cursor_db.is_file() {
        return stats;
    }

    // 增量：Cursor 主库未变则整源跳过。解析格式升级时除外，否则旧时间戳不会被重写。
    let force_reparse = cursor_parser_rev_stale(conn);
    if incremental && !force_reparse && !needs_sync(conn, &cursor_db, true) {
        stats.skipped_count += 1;
        return stats;
    }

    let ws_storage_dir = home.join("Library/Application Support/Cursor/User/workspaceStorage");
    let projects_dir = home.join(".cursor/projects");

    let cursor_conn = match Connection::open_with_flags(
        &cursor_db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[Cursor Importer] 无法只读打开 Cursor 数据库: {}", e);
            stats.error_count += 1;
            return stats;
        }
    };

    let mut existing_map: HashMap<String, (String, i64)> = HashMap::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id, COALESCE(updated_at, created_at, ''), message_count FROM conversations WHERE id LIKE 'cursor:%'",
    ) {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
        }) {
            for r in rows.flatten() {
                existing_map.insert(r.0, (r.1, r.2));
            }
        }
    }

    // 先用 JSON1 轻量字段做增量筛选，避免对未变更会话解析整段 value
    let meta_sql = r#"
        SELECT
            key,
            COALESCE(
                json_extract(value, '$.lastUpdatedAt'),
                json_extract(value, '$.conversationCheckpointLastUpdatedAt'),
                json_extract(value, '$.createdAt'),
                0
            ) as updated_ms,
            COALESCE(json_array_length(json_extract(value, '$.fullConversationHeadersOnly')), 0) as msg_cnt
        FROM cursorDiskKV
        WHERE key LIKE 'composerData:%'
    "#;

    let mut changed_keys: Vec<String> = Vec::new();
    let meta_ok = if let Ok(mut stmt) = cursor_conn.prepare(meta_sql) {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1).unwrap_or(0),
                row.get::<_, i64>(2).unwrap_or(0),
            ))
        }) {
            for (key, updated_ms, msg_cnt) in rows.flatten() {
                let composer_id = match key.split(':').nth(1) {
                    Some(id) if !id.is_empty() => id,
                    _ => continue,
                };
                if msg_cnt <= 0 {
                    continue;
                }
                let cid = format!("cursor:{}", composer_id);
                let norm_updated = if updated_ms > 0 {
                    chrono::DateTime::from_timestamp_millis(updated_ms)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_default()
                } else {
                    String::new()
                };

                if incremental && !force_reparse {
                    if let Some((prev_up, prev_cnt)) = existing_map.get(&cid) {
                        if *prev_cnt == msg_cnt && *prev_up == norm_updated {
                            stats.skipped_count += 1;
                            continue;
                        }
                    }
                }
                changed_keys.push(key);
            }
            true
        } else {
            false
        }
    } else {
        false
    };

    // 仅在确有变更会话时再构建 workspace 映射（slug/uuid 扫描很贵）
    let hash_map = OnceLock::new();
    let slug_map = OnceLock::new();
    let uuid_map_cache: OnceLock<HashMap<String, String>> = OnceLock::new();

    if !meta_ok {
        // JSON1 不可用时回退旧路径：全量拉 value
        let hash_map_v = load_hash_to_folder(&ws_storage_dir);
        let slug_map_v = load_slug_to_folder(&projects_dir);
        let _ = hash_map.set(hash_map_v);
        let _ = slug_map.set(slug_map_v);

        let mut stmt = match cursor_conn
            .prepare("SELECT key, value FROM cursorDiskKV WHERE key LIKE 'composerData:%'")
        {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[Cursor Importer] 查询 cursorDiskKV 失败: {}", e);
                stats.error_count += 1;
                return stats;
            }
        };
        let rows = match stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[Cursor Importer] 遍历会话失败: {}", e);
                stats.error_count += 1;
                return stats;
            }
        };

        for (key, val_str) in rows.flatten() {
            if let Err(e) = process_cursor_composer(
                conn,
                &cursor_conn,
                &key,
                &val_str,
                &existing_map,
                &ws_storage_dir,
                &projects_dir,
                &hash_map,
                &slug_map,
                &uuid_map_cache,
                &mut stats,
            ) {
                eprintln!("[Cursor Importer] 处理失败 {}: {}", key, e);
                stats.error_count += 1;
            }
        }

        if stats.error_count == 0 {
            mark_cursor_synced(conn, &cursor_db);
        }
        return stats;
    }

    if changed_keys.is_empty() {
        if stats.error_count == 0 {
            mark_cursor_synced(conn, &cursor_db);
        }
        return stats;
    }

    for key in changed_keys {
        let val_str: String = match cursor_conn.query_row(
            "SELECT value FROM cursorDiskKV WHERE key = ?1",
            params![&key],
            |r| r.get(0),
        ) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if let Err(e) = process_cursor_composer(
            conn,
            &cursor_conn,
            &key,
            &val_str,
            &existing_map,
            &ws_storage_dir,
            &projects_dir,
            &hash_map,
            &slug_map,
            &uuid_map_cache,
            &mut stats,
        ) {
            eprintln!("[Cursor Importer] 处理失败 {}: {}", key, e);
            stats.error_count += 1;
        }
    }

    if stats.error_count == 0 {
        mark_cursor_synced(conn, &cursor_db);
    }

    stats
}

fn process_cursor_composer(
    conn: &Connection,
    cursor_conn: &Connection,
    key: &str,
    val_str: &str,
    _existing_map: &HashMap<String, (String, i64)>,
    ws_storage_dir: &Path,
    projects_dir: &Path,
    hash_map: &OnceLock<HashMap<String, String>>,
    slug_map: &OnceLock<HashMap<String, String>>,
    uuid_map_cache: &OnceLock<HashMap<String, String>>,
    stats: &mut ImporterStats,
) -> Result<(), Box<dyn std::error::Error>> {
    let composer_id = match key.split(':').nth(1) {
        Some(id) if !id.is_empty() => id,
        _ => return Ok(()),
    };

    let data: Value = serde_json::from_str(val_str)?;
    let headers = match data
        .get("fullConversationHeadersOnly")
        .and_then(|v| v.as_array())
    {
        Some(h) if !h.is_empty() => h,
        _ => {
            stats.skipped_count += 1;
            return Ok(());
        }
    };

    let cid = format!("cursor:{}", composer_id);
    let created_ms = data.get("createdAt").and_then(|v| v.as_i64()).unwrap_or(0);
    let updated_ms = data
        .get("lastUpdatedAt")
        .or_else(|| data.get("conversationCheckpointLastUpdatedAt"))
        .and_then(|v| v.as_i64())
        .unwrap_or(created_ms);

    let created_at = if created_ms > 0 {
        chrono::DateTime::from_timestamp_millis(created_ms).map(|dt| dt.to_rfc3339())
    } else {
        None
    };
    let updated_at = if updated_ms > 0 {
        chrono::DateTime::from_timestamp_millis(updated_ms).map(|dt| dt.to_rfc3339())
    } else {
        created_at.clone()
    };

    let workspace_path = infer_workspace(
        cursor_conn,
        composer_id,
        &data,
        ws_storage_dir,
        projects_dir,
        hash_map,
        slug_map,
        uuid_map_cache,
    );

    let mut title = data
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    let messages = extract_messages(
        cursor_conn,
        composer_id,
        headers,
        updated_at.clone(),
        &mut title,
    );
    if messages.is_empty() {
        stats.skipped_count += 1;
        return Ok(());
    }

    if title.is_empty() {
        title = format!("Cursor 会话 {}", &composer_id[..composer_id.len().min(8)]);
    }

    let conv = RawConversation {
        id: cid,
        title,
        workspace_path,
        source_app: "cursor".to_string(),
        created_at,
        updated_at,
        parse_status: "ok".to_string(),
        source_types: vec!["cursor".to_string()],
        messages,
    };

    match save_conversation_tx(conn, &conv) {
        Ok(is_new) => {
            if is_new {
                stats.new_count += 1;
            } else {
                stats.updated_count += 1;
            }
        }
        Err(e) => {
            eprintln!("[Cursor Importer] 写入会话失败 {}: {}", conv.id, e);
            stats.error_count += 1;
        }
    }

    Ok(())
}

/// 1. 加载 workspaceStorage 中的 workspace.json 映射
fn load_hash_to_folder(ws_storage_dir: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if !ws_storage_dir.is_dir() {
        return map;
    }

    if let Ok(entries) = std::fs::read_dir(ws_storage_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let ws_json = path.join("workspace.json");
            if ws_json.is_file() {
                if let Ok(content) = std::fs::read_to_string(&ws_json) {
                    if let Ok(val) = serde_json::from_str::<Value>(&content) {
                        let folder = val
                            .get("folder")
                            .or_else(|| val.get("workspace"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if !folder.is_empty() {
                            let root = canonicalize_workspace_path(folder);
                            let hash = path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();
                            map.insert(hash, root);
                        }
                    }
                }
            }
        }
    }
    map
}

/// 2. 加载 ~/.cursor/projects 中的 slug 映射
fn load_slug_to_folder(projects_dir: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if !projects_dir.is_dir() {
        return map;
    }

    if let Ok(entries) = std::fs::read_dir(projects_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if !name.starts_with("Users-") {
                continue;
            }
            let slug_path = format!("/{}", name.replace('-', "/"));
            let root = canonicalize_workspace_path(&slug_path);

            for file_entry in WalkDir::new(path.join("agent-transcripts"))
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let p = file_entry.path();
                if p.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        map.insert(stem.to_string(), root.clone());
                    }
                }
            }
        }
    }
    map
}

/// 3. 加载 workspaceStorage 下 state.vscdb 中的 UUID 映射
fn load_uuid_to_folder(
    ws_storage_dir: &Path,
    hash_map: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let uuid_re = match Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
    {
        Ok(r) => r,
        Err(_) => return map,
    };

    if !ws_storage_dir.is_dir() {
        return map;
    }

    for (hash, root) in hash_map {
        let db_path = ws_storage_dir.join(hash).join("state.vscdb");
        if !db_path.is_file() {
            continue;
        }

        if let Ok(conn) = Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            if let Ok(mut stmt) = conn.prepare(
                "SELECT value FROM ItemTable WHERE value IS NOT NULL AND length(value) BETWEEN 10 AND 500000",
            ) {
                if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                    for val_str in rows.flatten() {
                        for cap in uuid_re.find_iter(&val_str) {
                            map.insert(cap.as_str().to_string(), root.clone());
                        }
                    }
                }
            }
        }
    }
    map
}

/// 四重工作区级联推断 (对齐 cursor_reader.py _infer_workspace)
fn infer_workspace(
    cursor_conn: &Connection,
    composer_id: &str,
    data: &Value,
    ws_storage_dir: &Path,
    projects_dir: &Path,
    hash_map: &OnceLock<HashMap<String, String>>,
    slug_map: &OnceLock<HashMap<String, String>>,
    uuid_map_cache: &OnceLock<HashMap<String, String>>,
) -> String {
    let hash_map = hash_map.get_or_init(|| load_hash_to_folder(ws_storage_dir));

    // 1. workspaceIdentifier
    if let Some(wi) = data.get("workspaceIdentifier") {
        if let Some(wid) = wi.get("id").and_then(|v| v.as_str()) {
            if let Some(folder) = hash_map.get(wid) {
                return folder.clone();
            }
        }
        if let Some(cfg) = wi.get("configPath") {
            for key in ["fsPath", "path", "external"] {
                if let Some(p) = cfg.get(key).and_then(|v| v.as_str()) {
                    let root = canonicalize_workspace_path(p);
                    if !root.is_empty() {
                        return root;
                    }
                }
            }
        }
    }

    // 2. slug map
    let slug_map = slug_map.get_or_init(|| load_slug_to_folder(projects_dir));
    if let Some(folder) = slug_map.get(composer_id) {
        return folder.clone();
    }

    // 3. uuid map（懒加载）
    let uuid_map = uuid_map_cache.get_or_init(|| load_uuid_to_folder(ws_storage_dir, hash_map));
    if let Some(folder) = uuid_map.get(composer_id) {
        return folder.clone();
    }

    // 4. bubbles 文本启发式扫描
    let bubble_prefix = format!("bubbleId:{}:%", composer_id);
    if let Ok(mut stmt) =
        cursor_conn.prepare("SELECT value FROM cursorDiskKV WHERE key LIKE ? LIMIT 100")
    {
        if let Ok(rows) = stmt.query_map(params![&bubble_prefix], |r| r.get::<_, String>(0)) {
            for val_str in rows.flatten() {
                if let Ok(bubble) = serde_json::from_str::<Value>(&val_str) {
                    if let Some(text) = bubble.get("text").and_then(|v| v.as_str()) {
                        if text.contains("/workspace/") {
                            for line in text.lines() {
                                if let Some(idx) = line.find("/workspace/") {
                                    let sub = &line[idx..];
                                    let candidate = canonicalize_workspace_path(
                                        sub.split_whitespace().next().unwrap_or(""),
                                    );
                                    if !candidate.is_empty() {
                                        return candidate;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    String::new()
}

/// 提取消息流并关联 bubbles
fn extract_messages(
    cursor_conn: &Connection,
    composer_id: &str,
    headers: &[Value],
    updated_at: Option<String>,
    title: &mut String,
) -> Vec<RawMessage> {
    let mut bubble_map = HashMap::new();
    let query_prefix = format!("bubbleId:{}:%", composer_id);
    if let Ok(mut stmt) =
        cursor_conn.prepare_cached("SELECT key, value FROM cursorDiskKV WHERE key LIKE ?")
    {
        if let Ok(rows) = stmt.query_map(params![&query_prefix], |r| {
            let k: String = r.get(0)?;
            let v: String = r.get(1)?;
            Ok((k, v))
        }) {
            for r in rows.flatten() {
                let (k, v_str) = r;
                if let Some(bid) = k.split(':').nth(2) {
                    if let Ok(val) = serde_json::from_str::<Value>(&v_str) {
                        bubble_map.insert(bid.to_string(), val);
                    }
                }
            }
        }
    }

    let mut messages = Vec::new();
    let mut step_idx = 0i64;

    for header in headers {
        let bubble_id = header
            .get("bubbleId")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if bubble_id.is_empty() {
            continue;
        }

        let bubble = match bubble_map.get(bubble_id) {
            Some(b) => b,
            None => continue,
        };

        let raw_type = header
            .get("type")
            .or_else(|| bubble.get("type"))
            .and_then(|v| v.as_i64())
            .unwrap_or(1);
        let role = if raw_type == 1 { "user" } else { "assistant" };
        let text = bubble
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let thinking = bubble
            .get("allThinkingBlocks")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|b| b.get("text").and_then(|t| t.as_str()).unwrap_or(""))
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n\n")
            })
            .filter(|s| !s.is_empty());

        let tool_results = bubble.get("toolResults").map(|v| v.to_string());

        if text.is_empty() && thinking.is_none() && tool_results.is_none() {
            continue;
        }

        if role == "user" && title.is_empty() && !text.is_empty() {
            *title = text.chars().take(60).collect();
        }

        messages.push(RawMessage {
            step_index: step_idx,
            role: role.to_string(),
            message_type: if tool_results.is_some() {
                "tool_call".to_string()
            } else {
                "text".to_string()
            },
            content: text.to_string(),
            thinking,
            created_at: bubble_created_at(header, bubble).or_else(|| updated_at.clone()),
            model_name: Some("Cursor".to_string()),
            tool_name: if tool_results.is_some() {
                Some("tool".to_string())
            } else {
                None
            },
            tool_args: tool_results,
            duration_ms: None,
            token_count: None,
        });
        step_idx += 1;
    }

    messages
}

fn bubble_created_at(header: &Value, bubble: &Value) -> Option<String> {
    let raw = header
        .get("createdAt")
        .or_else(|| bubble.get("createdAt"))?;
    if let Some(s) = raw.as_str() {
        return super::normalize_to_iso(Some(s.to_string()));
    }
    let ms = raw.as_i64().or_else(|| raw.as_f64().map(|v| v as i64))?;
    chrono::DateTime::from_timestamp_millis(ms).map(|dt| dt.to_rfc3339())
}

fn cursor_parser_rev_stale(conn: &Connection) -> bool {
    let stored: Option<String> = conn
        .query_row(
            "SELECT conversation_id FROM sync_state WHERE source_path = ?",
            params![CURSOR_PARSER_REV_KEY],
            |r| r.get(0),
        )
        .ok();
    stored.as_deref() != Some(CURSOR_PARSER_REV)
}

fn mark_cursor_synced(conn: &Connection, cursor_db: &Path) {
    record_sync_state(conn, cursor_db, "cursor:state_vscdb", "cursor_db");
    let now = chrono::Utc::now().to_rfc3339();
    let _ = conn.execute(
        r#"
        INSERT INTO sync_state (source_path, conversation_id, source_type, file_mtime, file_size, synced_at)
        VALUES (?1, ?2, 'cursor_parser', 0, 0, ?3)
        ON CONFLICT(source_path) DO UPDATE SET
            conversation_id = excluded.conversation_id,
            synced_at = excluded.synced_at
        "#,
        params![CURSOR_PARSER_REV_KEY, CURSOR_PARSER_REV, now],
    );
}
