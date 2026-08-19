use crate::db::{
    fetch_conversation_messages, fetch_conversations, fetch_dashboard_stats,
    fetch_workspace_detail_stats, fetch_workspaces, get_database_path, search_global_messages,
};
use crate::sync::execute_sync;
use rusqlite::Connection;
use serde_json::json;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use url::Url;

pub fn start_http_server(port: u16) {
    thread::spawn(move || {
        let addr = format!("127.0.0.1:{}", port);
        let listener = match TcpListener::bind(&addr) {
            Ok(l) => {
                log::info!("AgentDeck REST API server listening on http://{}", addr);
                println!("AgentDeck REST API server listening on http://{}", addr);
                l
            }
            Err(e) => {
                log::warn!("Could not bind REST API server to {}: {}", addr, e);
                eprintln!("Warning: Could not bind REST API server to {}: {}", addr, e);
                return;
            }
        };

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    thread::spawn(move || {
                        handle_connection(stream);
                    });
                }
                Err(e) => {
                    log::warn!("Incoming connection error: {}", e);
                }
            }
        }
    });
}

fn handle_connection(mut stream: TcpStream) {
    let mut reader = BufReader::new(&stream);
    let mut first_line = String::new();
    if reader.read_line(&mut first_line).is_err() || first_line.is_empty() {
        return;
    }

    let parts: Vec<&str> = first_line.trim().split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }

    let method = parts[0];
    let full_path = parts[1];

    // 解析 URL 和 Query 参数
    let parsed_url = match Url::parse(&format!("http://127.0.0.1{}", full_path)) {
        Ok(u) => u,
        Err(_) => {
            send_response(
                &mut stream,
                400,
                "application/json",
                &json!({"ok": false, "error": "Invalid URL"}).to_string(),
            );
            return;
        }
    };

    let path = parsed_url.path();
    let query_params: HashMap<String, String> = parsed_url.query_pairs().into_owned().collect();

    // 打开只读数据库连接
    let db_path = get_database_path();
    let conn = match Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => {
            crate::db::apply_read_pragmas(&c);
            c
        }
        Err(e) => {
            send_response(
                &mut stream,
                500,
                "application/json",
                &json!({"ok": false, "error": format!("Database connection error: {}", e)})
                    .to_string(),
            );
            return;
        }
    };

    // 路由分发
    match (method, path) {
        ("GET", "/") | ("GET", "/docs") | ("GET", "/api/docs") => {
            let html = include_str!("api_docs.html");
            send_response(&mut stream, 200, "text/html; charset=utf-8", html);
        }

        ("GET", "/health") => {
            let stats = fetch_dashboard_stats(&conn).ok();
            let body = json!({
                "ok": true,
                "status": "ok",
                "app": "AgentDeck",
                "version": "0.2.4",
                "cursor_available": true,
                "ai_available": true,
                "stats": stats
            });
            send_response(&mut stream, 200, "application/json", &body.to_string());
        }

        ("GET", "/api/stats") | ("GET", "/api/dashboard-stats") => {
            match fetch_dashboard_stats(&conn) {
                Ok(stats) => {
                    let body = json!({
                        "ok": true,
                        "stats": stats,
                        "cursor_available": true,
                        "ai_available": true
                    });
                    send_response(&mut stream, 200, "application/json", &body.to_string());
                }
                Err(e) => {
                    send_response(
                        &mut stream,
                        500,
                        "application/json",
                        &json!({"ok": false, "error": e.to_string()}).to_string(),
                    );
                }
            }
        }

        ("GET", "/api/workspaces") => {
            let q = query_params.get("q").map(|s| s.as_str());
            match fetch_workspaces(&conn, q) {
                Ok(workspaces) => {
                    let total = workspaces.len();
                    // 兼容 HomeCore 的字段映射
                    let mapped_workspaces: Vec<serde_json::Value> = workspaces
                        .into_iter()
                        .map(|w| {
                            json!({
                                "workspace_path": w.workspace_path,
                                "cnt": w.cnt,
                                "total_conversations": w.cnt,
                                "total_user_messages": w.user_message_count,
                                "message_count": w.message_count,
                                "user_message_count": w.user_message_count,
                                "last_updated": w.last_updated,
                                "ag_cnt": w.ag_cnt,
                                "cursor_cnt": w.cursor_cnt,
                                "claude_cnt": w.claude_cnt,
                                "codex_cnt": w.codex_cnt,
                                "wb_cnt": w.wb_cnt,
                                "hermes_cnt": w.hermes_cnt
                            })
                        })
                        .collect();

                    let body = json!({
                        "ok": true,
                        "total": total,
                        "workspaces": mapped_workspaces
                    });
                    send_response(&mut stream, 200, "application/json", &body.to_string());
                }
                Err(e) => {
                    send_response(
                        &mut stream,
                        500,
                        "application/json",
                        &json!({"ok": false, "error": e.to_string()}).to_string(),
                    );
                }
            }
        }

        ("GET", "/api/conversations") => {
            let workspace = query_params.get("workspace").map(|s| s.as_str());
            let q = query_params.get("q").map(|s| s.as_str());
            let starred = query_params
                .get("starred")
                .map(|s| s == "1" || s == "true")
                .unwrap_or(false);

            match fetch_conversations(&conn, workspace, q, starred) {
                Ok(conversations) => {
                    let total = conversations.len();
                    let body = json!({
                        "ok": true,
                        "total": total,
                        "conversations": conversations
                    });
                    send_response(&mut stream, 200, "application/json", &body.to_string());
                }
                Err(e) => {
                    send_response(
                        &mut stream,
                        500,
                        "application/json",
                        &json!({"ok": false, "error": e.to_string()}).to_string(),
                    );
                }
            }
        }

        ("GET", p) if p.starts_with("/api/conversations/") => {
            let cid = &p["/api/conversations/".len()..];
            match fetch_conversation_messages(&conn, cid) {
                Ok(msgs) => {
                    let body = json!({
                        "ok": true,
                        "conversation_id": cid,
                        "messages": msgs
                    });
                    send_response(&mut stream, 200, "application/json", &body.to_string());
                }
                Err(e) => {
                    send_response(
                        &mut stream,
                        500,
                        "application/json",
                        &json!({"ok": false, "error": e.to_string()}).to_string(),
                    );
                }
            }
        }

        ("GET", p) if p.starts_with("/api/conversation/") && p.ends_with("/messages") => {
            let mid = &p["/api/conversation/".len()..p.len() - "/messages".len()];
            match fetch_conversation_messages(&conn, mid) {
                Ok(msgs) => {
                    let body = json!({
                        "ok": true,
                        "conversation_id": mid,
                        "messages": msgs
                    });
                    send_response(&mut stream, 200, "application/json", &body.to_string());
                }
                Err(e) => {
                    send_response(
                        &mut stream,
                        500,
                        "application/json",
                        &json!({"ok": false, "error": e.to_string()}).to_string(),
                    );
                }
            }
        }

        ("GET", "/api/search") | ("GET", "/api/spotlight") => {
            let q = query_params.get("q").cloned().unwrap_or_default();
            let role = query_params.get("role").map(|s| s.as_str());
            let limit = query_params
                .get("limit")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(30);

            match search_global_messages(&conn, &q, role, limit) {
                Ok(items) => {
                    let body = json!({
                        "ok": true,
                        "total": items.len(),
                        "items": items
                    });
                    send_response(&mut stream, 200, "application/json", &body.to_string());
                }
                Err(e) => {
                    send_response(
                        &mut stream,
                        500,
                        "application/json",
                        &json!({"ok": false, "error": e.to_string()}).to_string(),
                    );
                }
            }
        }

        ("GET", "/api/user-messages") => {
            let q = query_params.get("q").cloned().unwrap_or_default();
            let workspace = query_params.get("workspace").map(|s| s.as_str());
            let date = query_params
                .get("date")
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());
            let source = query_params
                .get("source")
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());
            let order = query_params
                .get("order")
                .map(|s| s.as_str())
                .unwrap_or("desc");
            let fmt = query_params
                .get("format")
                .map(|s| s.as_str())
                .unwrap_or("compact");
            let limit = query_params
                .get("limit")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(50);
            let offset = query_params
                .get("offset")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0);
            let is_single_date = date.map(|d| d.len() == 10).unwrap_or(false);

            let order_clause = if order == "asc" { "ASC" } else { "DESC" };

            let sql = format!(
                r#"
                SELECT m.conversation_id, c.title, c.workspace_path, m.role, m.content, m.created_at, m.source
                FROM messages m
                LEFT JOIN conversations c ON m.conversation_id = c.id
                WHERE m.role = 'user'
                  AND (?1 IS NULL OR ?1 = '' OR c.workspace_path = ?1)
                  AND (?2 IS NULL OR ?2 = '' OR m.content LIKE '%' || ?2 || '%')
                  AND (?3 IS NULL OR ?3 = '' OR strftime('%Y-%m-%d', datetime(m.created_at, '+8 hours')) = ?3)
                  AND (?4 IS NULL OR ?4 = '' OR m.source LIKE '%' || ?4 || '%')
                ORDER BY m.created_at {}
                LIMIT ?5 OFFSET ?6
                "#,
                order_clause
            );

            let mut stmt = match conn.prepare(&sql) {
                Ok(s) => s,
                Err(e) => {
                    send_response(
                        &mut stream,
                        500,
                        "application/json",
                        &json!({"ok": false, "error": e.to_string()}).to_string(),
                    );
                    return;
                }
            };

            let rows = stmt.query_map(
                rusqlite::params![
                    workspace.unwrap_or(""),
                    q,
                    date.unwrap_or(""),
                    source.unwrap_or(""),
                    limit as i64,
                    offset as i64
                ],
                |row| {
                    let raw_created: Option<String> = row.get(5)?;
                    let beijing_created = convert_to_beijing_iso(raw_created);
                    Ok(json!({
                        "conversation_id": row.get::<_, String>(0)?,
                        "conversation_title": row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                        "workspace_path": row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        "role": row.get::<_, String>(3)?,
                        "content": row.get::<_, String>(4)?,
                        "created_at": beijing_created,
                        "source": row.get::<_, Option<String>>(6)?.unwrap_or_else(|| "unknown".to_string())
                    }))
                },
            );

            match rows {
                Ok(mapped) => {
                    let flat_msgs: Vec<serde_json::Value> = mapped.filter_map(Result::ok).collect();
                    let total = flat_msgs.len();

                    // 按会话构建 compact 聚合结构
                    let mut conv_map: HashMap<String, serde_json::Value> = HashMap::new();
                    let mut conv_order: Vec<String> = Vec::new();

                    for msg in &flat_msgs {
                        let cid = msg
                            .get("conversation_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let title = msg
                            .get("conversation_title")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&cid);
                        let src = msg
                            .get("source")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        let ws = msg
                            .get("workspace_path")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let created = msg.get("created_at").and_then(|v| v.as_str());
                        let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");

                        let time_tag = format_beijing_tag(created, is_single_date);
                        let line = format!("{}{}", time_tag, content);

                        if !conv_map.contains_key(&cid) {
                            conv_order.push(cid.clone());
                            conv_map.insert(
                                cid.clone(),
                                json!({
                                    "id": cid,
                                    "title": title,
                                    "source": src,
                                    "workspace_path": ws,
                                    "messages": [line]
                                }),
                            );
                        } else if let Some(item) = conv_map.get_mut(&cid) {
                            if let Some(arr) =
                                item.get_mut("messages").and_then(|v| v.as_array_mut())
                            {
                                arr.push(json!(line));
                            }
                        }
                    }

                    let compact_convs: Vec<serde_json::Value> = conv_order
                        .into_iter()
                        .filter_map(|cid| conv_map.remove(&cid))
                        .collect();

                    if fmt == "flat" || fmt == "raw" {
                        let body = json!({
                            "ok": true,
                            "total": total,
                            "limit": limit,
                            "offset": offset,
                            "date": date,
                            "workspace": workspace,
                            "messages": flat_msgs
                        });
                        send_response(&mut stream, 200, "application/json", &body.to_string());
                    } else {
                        let body = json!({
                            "ok": true,
                            "total_messages": total,
                            "total_conversations": compact_convs.len(),
                            "limit": limit,
                            "offset": offset,
                            "date": date,
                            "workspace": workspace,
                            "conversations": compact_convs,
                            "messages": flat_msgs
                        });
                        send_response(&mut stream, 200, "application/json", &body.to_string());
                    }
                }
                Err(e) => {
                    send_response(
                        &mut stream,
                        500,
                        "application/json",
                        &json!({"ok": false, "error": e.to_string()}).to_string(),
                    );
                }
            }
        }

        ("GET", "/api/workspace/stats") | ("GET", "/api/workspace/analysis") => {
            let ws = query_params.get("workspace").cloned().unwrap_or_default();
            match fetch_workspace_detail_stats(&conn, &ws) {
                Ok(detail) => {
                    let body = json!({
                        "ok": true,
                        "workspace_path": ws,
                        "stats": detail
                    });
                    send_response(&mut stream, 200, "application/json", &body.to_string());
                }
                Err(e) => {
                    send_response(
                        &mut stream,
                        500,
                        "application/json",
                        &json!({"ok": false, "error": e.to_string()}).to_string(),
                    );
                }
            }
        }

        ("GET", "/ag-image") => {
            let path_param = query_params.get("path").cloned().unwrap_or_default();
            if let Some(img_path) = get_ag_image_path(&path_param) {
                if let Ok(bytes) = std::fs::read(&img_path) {
                    let mime = get_mime_from_path(&img_path);
                    send_binary_response(&mut stream, 200, mime, &bytes);
                    return;
                }
            }
            send_response(
                &mut stream,
                404,
                "application/json",
                &json!({"ok": false, "error": "Image not found"}).to_string(),
            );
        }

        ("GET", p) if p.starts_with("/cursor-image/") => {
            let uuid = &p["/cursor-image/".len()..];
            if let Some(img_path) = get_cursor_image_path(uuid) {
                if let Ok(bytes) = std::fs::read(&img_path) {
                    let mime = get_mime_from_path(&img_path);
                    send_binary_response(&mut stream, 200, mime, &bytes);
                    return;
                }
            }
            send_response(
                &mut stream,
                404,
                "application/json",
                &json!({"ok": false, "error": "Cursor image not found"}).to_string(),
            );
        }

        ("GET", p) if p.starts_with("/media/") => {
            let rel_path = &p["/media/".len()..];
            if !rel_path.contains("..") {
                if let Some(media_root) = crate::media_archive::get_media_root() {
                    let target = media_root.join(rel_path);
                    if target.is_file() {
                        if let Ok(bytes) = std::fs::read(&target) {
                            let mime = get_mime_from_path(&target);
                            send_binary_response(&mut stream, 200, mime, &bytes);
                            return;
                        }
                    }
                }
            }
            send_response(
                &mut stream,
                404,
                "application/json",
                &json!({"ok": false, "error": "Media asset not found"}).to_string(),
            );
        }

        ("POST", "/sync") => {
            let res = execute_sync(false);
            let body = json!({
                "ok": res.success,
                "new_count": res.new_count,
                "updated_count": res.updated_count,
                "message": res.message
            });
            send_response(&mut stream, 200, "application/json", &body.to_string());
        }

        _ => {
            let body = json!({
                "ok": false,
                "error": format!("Route not found: {} {}", method, path)
            });
            send_response(&mut stream, 404, "application/json", &body.to_string());
        }
    }
}

fn get_mime_from_path(p: &std::path::Path) -> &'static str {
    match p
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn get_ag_image_path(raw: &str) -> Option<std::path::PathBuf> {
    let unescaped = urlencoding::decode(raw).unwrap_or_else(|_| std::borrow::Cow::Borrowed(raw));
    let clean = unescaped
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_start_matches("file://");
    if clean.is_empty() {
        return None;
    }
    let path = std::path::PathBuf::from(clean);
    if path.is_file() {
        return Some(path);
    }
    if let Some(home) = dirs::home_dir() {
        if clean.starts_with('~') {
            let p = home.join(clean.trim_start_matches('~').trim_start_matches('/'));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

fn get_cursor_image_path(image_uuid: &str) -> Option<std::path::PathBuf> {
    if image_uuid.is_empty() {
        return None;
    }
    let home = dirs::home_dir()?;
    let ws_storage = home.join("Library/Application Support/Cursor/User/workspaceStorage");
    if !ws_storage.exists() {
        return None;
    }
    let clean_uuid = image_uuid.trim().to_lowercase();
    for entry in walkdir::WalkDir::new(&ws_storage).max_depth(3) {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file() {
                if let Some(fname) = path.file_name() {
                    let name = fname.to_string_lossy().to_lowercase();
                    if name.contains(&clean_uuid) {
                        return Some(path.to_path_buf());
                    }
                }
            }
        }
    }
    None
}

fn send_binary_response(stream: &mut TcpStream, status_code: u16, content_type: &str, body: &[u8]) {
    let header = format!(
        "HTTP/1.1 {} OK\r\n\
        Content-Type: {}\r\n\
        Content-Length: {}\r\n\
        Access-Control-Allow-Origin: *\r\n\
        Cache-Control: public, max-age=86400\r\n\
        Connection: close\r\n\
        \r\n",
        status_code,
        content_type,
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn send_response(stream: &mut TcpStream, status_code: u16, content_type: &str, body: &str) {
    let status_text = match status_code {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    };

    let response = format!(
        "HTTP/1.1 {} {}\r\n\
        Content-Type: {}\r\n\
        Content-Length: {}\r\n\
        Access-Control-Allow-Origin: *\r\n\
        Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
        Access-Control-Allow-Headers: *\r\n\
        Connection: close\r\n\
        \r\n\
        {}",
        status_code,
        status_text,
        content_type,
        body.as_bytes().len(),
        body
    );

    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn convert_to_beijing_iso(raw: Option<String>) -> Option<String> {
    let s = raw?.trim().to_string();
    if s.is_empty() {
        return None;
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
        let beijing_tz = chrono::FixedOffset::east_opt(8 * 3600)?;
        return Some(dt.with_timezone(&beijing_tz).to_rfc3339());
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S") {
        let dt_utc = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(naive, chrono::Utc);
        let beijing_tz = chrono::FixedOffset::east_opt(8 * 3600)?;
        return Some(dt_utc.with_timezone(&beijing_tz).to_rfc3339());
    }
    Some(s)
}

fn format_beijing_tag(raw: Option<&str>, is_single_date: bool) -> String {
    let s = match raw {
        Some(v) if !v.trim().is_empty() => v.trim(),
        _ => return String::new(),
    };
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        if let Some(beijing_tz) = chrono::FixedOffset::east_opt(8 * 3600) {
            let b = dt.with_timezone(&beijing_tz);
            if is_single_date {
                return format!("[{}] ", b.format("%H:%M:%S"));
            } else {
                return format!("[{}] ", b.format("%Y-%m-%d %H:%M:%S"));
            }
        }
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        let dt_utc = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(naive, chrono::Utc);
        if let Some(beijing_tz) = chrono::FixedOffset::east_opt(8 * 3600) {
            let b = dt_utc.with_timezone(&beijing_tz);
            if is_single_date {
                return format!("[{}] ", b.format("%H:%M:%S"));
            } else {
                return format!("[{}] ", b.format("%Y-%m-%d %H:%M:%S"));
            }
        }
    }
    if s.contains('T') {
        let parts: Vec<&str> = s.split('T').collect();
        let time_clean = parts[1]
            .split('.')
            .next()
            .unwrap_or(parts[1])
            .trim_end_matches('Z');
        if is_single_date {
            format!("[{}] ", time_clean)
        } else {
            format!("[{} {}] ", parts[0], time_clean)
        }
    } else {
        format!("[{}] ", s)
    }
}
