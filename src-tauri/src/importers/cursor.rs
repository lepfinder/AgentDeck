use rusqlite::{params, Connection, OpenFlags};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use regex::Regex;
use walkdir::WalkDir;

use super::{save_conversation_tx, ImporterStats, RawConversation, RawMessage};

pub fn sync(conn: &Connection, force: bool) -> ImporterStats {
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

    let ws_storage_dir = home.join("Library/Application Support/Cursor/User/workspaceStorage");
    let projects_dir = home.join(".cursor/projects");

    // 1. 建立工作区三级级联映射 (对齐 cursor_reader.py)
    let hash_map = load_hash_to_folder(&ws_storage_dir);
    let slug_map = load_slug_to_folder(&projects_dir);
    let uuid_map = load_uuid_to_folder(&ws_storage_dir, &hash_map);

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

    // 2. 读取已有 cursor 会话的 (updated_at, message_count) 用于秒级增量比对 (对齐 sync_cursor_to_db.py)
    let mut existing_map: HashMap<String, (String, i64)> = HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id, COALESCE(updated_at, created_at, ''), message_count FROM conversations WHERE id LIKE 'cursor:%'") {
        if let Ok(rows) = stmt.query_map([], |r| {
            let id: String = r.get(0)?;
            let up: String = r.get(1)?;
            let cnt: i64 = r.get(2)?;
            Ok((id, (up, cnt)))
        }) {
            for r in rows.flatten() {
                existing_map.insert(r.0, r.1);
            }
        }
    }

    // 3. 全量扫描 cursorDiskKV 中的 composerData:%
    let mut stmt = match cursor_conn.prepare("SELECT key, value FROM cursorDiskKV WHERE key LIKE 'composerData:%'") {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[Cursor Importer] 查询 cursorDiskKV 失败: {}", e);
            stats.error_count += 1;
            return stats;
        }
    };

    let rows = match stmt.query_map([], |row| {
        let key: String = row.get(0)?;
        let val: String = row.get(1)?;
        Ok((key, val))
    }) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[Cursor Importer] 遍历会话失败: {}", e);
            stats.error_count += 1;
            return stats;
        }
    };

    for row in rows.flatten() {
        let (key, val_str) = row;
        let composer_id = match key.split(':').nth(1) {
            Some(id) if !id.is_empty() => id,
            _ => continue,
        };

        let data: Value = match serde_json::from_str(&val_str) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let headers = match data.get("fullConversationHeadersOnly").and_then(|v| v.as_array()) {
            Some(h) if !h.is_empty() => h,
            _ => continue,
        };

        let cid = format!("cursor:{}", composer_id);
        let msg_cnt = headers.len() as i64;

        let created_ms = data.get("createdAt").and_then(|v| v.as_i64()).unwrap_or(0);
        let updated_ms = data.get("lastUpdatedAt")
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

        let norm_updated_str = updated_at.clone().unwrap_or_default();

        // 增量检查：如果未变化直接跳过消息提取，极速完成
        if !force {
            if let Some((prev_up, prev_cnt)) = existing_map.get(&cid) {
                if *prev_cnt == msg_cnt && *prev_up == norm_updated_str {
                    stats.skipped_count += 1;
                    continue;
                }
            }
        }

        // 4. 四级工作区级联推断 (对齐 cursor_reader.py _infer_workspace)
        let workspace_path = infer_workspace(
            &cursor_conn,
            composer_id,
            &data,
            &hash_map,
            &slug_map,
            &uuid_map,
        );

        let mut title = data.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();

        // 5. 提取消息流
        let messages = extract_messages(&cursor_conn, composer_id, headers, updated_at.clone(), &mut title);
        if messages.is_empty() {
            stats.skipped_count += 1;
            continue;
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
    }

    stats
}

/// 路径归一到项目根目录 (对齐 parser.py project_root_from_path)
fn project_root_from_path(path_str: &str) -> String {
    let clean = path_str.trim_start_matches("file://").trim();
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

    clean.to_string()
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
                        let folder = val.get("folder")
                            .or_else(|| val.get("workspace"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if !folder.is_empty() {
                            let root = project_root_from_path(folder);
                            let hash = path.file_name().unwrap_or_default().to_string_lossy().to_string();
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
            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            if !name.starts_with("Users-") {
                continue;
            }
            let slug_path = format!("/{}", name.replace('-', "/"));
            let root = project_root_from_path(&slug_path);

            for file_entry in WalkDir::new(path.join("agent-transcripts")).into_iter().filter_map(|e| e.ok()) {
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
fn load_uuid_to_folder(ws_storage_dir: &Path, hash_map: &HashMap<String, String>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let uuid_re = match Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}") {
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

        if let Ok(conn) = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX) {
            if let Ok(mut stmt) = conn.prepare("SELECT value FROM ItemTable WHERE value IS NOT NULL AND length(value) BETWEEN 10 AND 500000") {
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
    hash_map: &HashMap<String, String>,
    slug_map: &HashMap<String, String>,
    uuid_map: &HashMap<String, String>,
) -> String {
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
                    let root = project_root_from_path(p);
                    if !root.is_empty() {
                        return root;
                    }
                }
            }
        }
    }

    // 2. slug map
    if let Some(folder) = slug_map.get(composer_id) {
        return folder.clone();
    }

    // 3. uuid map
    if let Some(folder) = uuid_map.get(composer_id) {
        return folder.clone();
    }

    // 4. bubbles 文本启发式扫描
    let bubble_prefix = format!("bubbleId:{}:%", composer_id);
    if let Ok(mut stmt) = cursor_conn.prepare("SELECT value FROM cursorDiskKV WHERE key LIKE ? LIMIT 100") {
        if let Ok(rows) = stmt.query_map(params![&bubble_prefix], |r| r.get::<_, String>(0)) {
            for val_str in rows.flatten() {
                if let Ok(bubble) = serde_json::from_str::<Value>(&val_str) {
                    if let Some(text) = bubble.get("text").and_then(|v| v.as_str()) {
                        if text.contains("/workspace/") {
                            for line in text.lines() {
                                if let Some(idx) = line.find("/workspace/") {
                                    let sub = &line[idx..];
                                    let candidate = project_root_from_path(sub.split_whitespace().next().unwrap_or(""));
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
    if let Ok(mut stmt) = cursor_conn.prepare("SELECT key, value FROM cursorDiskKV WHERE key LIKE ?") {
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
        let bubble_id = header.get("bubbleId").and_then(|v| v.as_str()).unwrap_or("");
        if bubble_id.is_empty() {
            continue;
        }

        let bubble = match bubble_map.get(bubble_id) {
            Some(b) => b,
            None => continue,
        };

        let raw_type = header.get("type").or_else(|| bubble.get("type")).and_then(|v| v.as_i64()).unwrap_or(1);
        let role = if raw_type == 1 { "user" } else { "assistant" };
        let text = bubble.get("text").and_then(|v| v.as_str()).unwrap_or("").trim();
        let thinking = bubble.get("allThinkingBlocks")
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
            message_type: if tool_results.is_some() { "tool_call".to_string() } else { "text".to_string() },
            content: text.to_string(),
            thinking,
            created_at: updated_at.clone(),
            model_name: Some("Cursor".to_string()),
            tool_name: if tool_results.is_some() { Some("tool".to_string()) } else { None },
            tool_args: tool_results,
            duration_ms: None,
            token_count: None,
        });
        step_idx += 1;
    }

    messages
}
