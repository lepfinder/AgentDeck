use crate::db::{
    fetch_conversations, fetch_conversation_messages, fetch_dashboard_stats,
    fetch_workspace_detail_stats, fetch_workspaces, search_global_messages,
    get_database_path,
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
            send_response(&mut stream, 400, "application/json", &json!({"ok": false, "error": "Invalid URL"}).to_string());
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
        Ok(c) => c,
        Err(e) => {
            send_response(
                &mut stream,
                500,
                "application/json",
                &json!({"ok": false, "error": format!("Database connection error: {}", e)}).to_string(),
            );
            return;
        }
    };

    // 路由分发
    match (method, path) {
        ("GET", "/health") => {
            let stats = fetch_dashboard_stats(&conn).ok();
            let body = json!({
                "ok": true,
                "status": "ok",
                "app": "AgentDeck",
                "version": "0.1.0",
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
                    send_response(&mut stream, 500, "application/json", &json!({"ok": false, "error": e.to_string()}).to_string());
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
                    send_response(&mut stream, 500, "application/json", &json!({"ok": false, "error": e.to_string()}).to_string());
                }
            }
        }

        ("GET", "/api/conversations") => {
            let workspace = query_params.get("workspace").map(|s| s.as_str());
            let q = query_params.get("q").map(|s| s.as_str());
            let starred = query_params.get("starred").map(|s| s == "1" || s == "true").unwrap_or(false);

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
                    send_response(&mut stream, 500, "application/json", &json!({"ok": false, "error": e.to_string()}).to_string());
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
                    send_response(&mut stream, 500, "application/json", &json!({"ok": false, "error": e.to_string()}).to_string());
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
                    send_response(&mut stream, 500, "application/json", &json!({"ok": false, "error": e.to_string()}).to_string());
                }
            }
        }

        ("GET", "/api/search") | ("GET", "/api/spotlight") => {
            let q = query_params.get("q").cloned().unwrap_or_default();
            let role = query_params.get("role").map(|s| s.as_str());
            let limit = query_params.get("limit").and_then(|s| s.parse::<usize>().ok()).unwrap_or(30);

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
                    send_response(&mut stream, 500, "application/json", &json!({"ok": false, "error": e.to_string()}).to_string());
                }
            }
        }

        ("GET", "/api/user-messages") => {
            let q = query_params.get("q").cloned().unwrap_or_default();
            let workspace = query_params.get("workspace").map(|s| s.as_str());
            let limit = query_params.get("limit").and_then(|s| s.parse::<usize>().ok()).unwrap_or(50);

            let sql = r#"
                SELECT m.conversation_id, c.title, c.workspace_path, m.role, m.content, m.created_at, m.source
                FROM messages m
                LEFT JOIN conversations c ON m.conversation_id = c.id
                WHERE m.role = 'user'
                  AND (?1 IS NULL OR ?1 = '' OR c.workspace_path = ?1)
                  AND (?2 IS NULL OR ?2 = '' OR m.content LIKE '%' || ?2 || '%')
                ORDER BY m.created_at DESC
                LIMIT ?3
            "#;

            let mut stmt = match conn.prepare(sql) {
                Ok(s) => s,
                Err(e) => {
                    send_response(&mut stream, 500, "application/json", &json!({"ok": false, "error": e.to_string()}).to_string());
                    return;
                }
            };

            let rows = stmt.query_map(
                rusqlite::params![workspace.unwrap_or(""), q, limit as i64],
                |row| {
                    Ok(json!({
                        "conversation_id": row.get::<_, String>(0)?,
                        "conversation_title": row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                        "workspace_path": row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        "role": row.get::<_, String>(3)?,
                        "content": row.get::<_, String>(4)?,
                        "created_at": row.get::<_, Option<String>>(5)?,
                        "source": row.get::<_, Option<String>>(6)?.unwrap_or_else(|| "unknown".to_string())
                    }))
                },
            );

            match rows {
                Ok(mapped) => {
                    let msgs: Vec<serde_json::Value> = mapped.filter_map(Result::ok).collect();
                    let body = json!({
                        "ok": true,
                        "total": msgs.len(),
                        "messages": msgs
                    });
                    send_response(&mut stream, 200, "application/json", &body.to_string());
                }
                Err(e) => {
                    send_response(&mut stream, 500, "application/json", &json!({"ok": false, "error": e.to_string()}).to_string());
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
                    send_response(&mut stream, 500, "application/json", &json!({"ok": false, "error": e.to_string()}).to_string());
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
            send_response(&mut stream, 404, "application/json", &json!({"ok": false, "error": "Image not found"}).to_string());
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
            send_response(&mut stream, 404, "application/json", &json!({"ok": false, "error": "Cursor image not found"}).to_string());
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
    match p.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn get_ag_image_path(raw: &str) -> Option<std::path::PathBuf> {
    if raw.is_empty() {
        return None;
    }
    let path = std::path::PathBuf::from(raw);
    if path.is_file() {
        return Some(path);
    }
    if let Some(home) = dirs::home_dir() {
        if raw.starts_with('~') {
            let clean = raw.trim_start_matches('~').trim_start_matches('/');
            let p = home.join(clean);
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
