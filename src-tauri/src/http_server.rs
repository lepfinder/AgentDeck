use crate::db::{
    allowed_prompt_category_values, create_prompt, delete_prompt, fetch_conversation_messages,
    fetch_conversations, fetch_dashboard_stats, fetch_workspace_detail_stats, fetch_workspaces,
    get_database_path, get_prompt, list_prompts, prompt_category_options, search_global_messages,
    update_prompt, PromptAgentItem, PromptInput,
};
use crate::sync::execute_sync;
use rusqlite::Connection;
use serde_json::json;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use url::Url;

struct HttpRequest {
    method: String,
    path: String,
    query_params: HashMap<String, String>,
    body: String,
}

enum PromptRoute {
    Collection,
    Item(i64),
}

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
    let req = match read_http_request(&mut stream) {
        Some(r) => r,
        None => return,
    };

    if req.method == "OPTIONS" {
        send_options_response(&mut stream);
        return;
    }

    let method = req.method.as_str();
    let path = req.path.as_str();
    let query_params = req.query_params;
    let body = req.body;

    // 路由分发
    match method {
        "GET" | "HEAD" => {
            let conn = match open_read_conn() {
                Ok(c) => c,
                Err(e) => {
                    send_json(&mut stream, 500, json!({"ok": false, "error": e.to_string()}));
                    return;
                }
            };
            route_get(&mut stream, method, path, &query_params, &conn);
        }
        "POST" | "PUT" | "DELETE" => {
            if let Some(prompt_route) = match_prompt_route(path) {
                route_prompt_mut(&mut stream, method, prompt_route, &body);
                return;
            }
            if method == "POST" && (path == "/sync" || path == "/api/sync") {
                let res = execute_sync(false);
                send_json(
                    &mut stream,
                    200,
                    json!({
                        "ok": res.success,
                        "new_count": res.new_count,
                        "updated_count": res.updated_count,
                        "message": res.message
                    }),
                );
                return;
            }
            send_json(
                &mut stream,
                404,
                json!({"ok": false, "error": format!("Route not found: {} {}", method, path)}),
            );
        }
        _ => {
            send_json(
                &mut stream,
                404,
                json!({"ok": false, "error": format!("Route not found: {} {}", method, path)}),
            );
        }
    }
}

fn read_http_request(stream: &mut TcpStream) -> Option<HttpRequest> {
    let mut reader = BufReader::new(stream);
    let mut first_line = String::new();
    reader.read_line(&mut first_line).ok()?;
    if first_line.is_empty() {
        return None;
    }

    let parts: Vec<&str> = first_line.trim().split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let method = parts[0].to_string();
    let full_path = parts[1];
    let parsed_url = Url::parse(&format!("http://127.0.0.1{}", full_path)).ok()?;
    let path = parsed_url.path().to_string();
    let query_params: HashMap<String, String> = parsed_url.query_pairs().into_owned().collect();

    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        if line == "\r\n" || line.trim().is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            if key.trim().eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
    }

    let mut body = String::new();
    if content_length > 0 {
        let mut buf = vec![0u8; content_length];
        if reader.read_exact(&mut buf).is_ok() {
            body = String::from_utf8_lossy(&buf).into_owned();
        }
    }

    Some(HttpRequest {
        method,
        path,
        query_params,
        body,
    })
}

fn open_read_conn() -> Result<Connection, String> {
    let db_path = get_database_path();
    Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map(|c| {
        crate::db::apply_read_pragmas(&c);
        c
    })
    .map_err(|e| format!("Database connection error: {}", e))
}

fn open_write_conn() -> Result<Connection, String> {
    let db_path = get_database_path();
    Connection::open(&db_path)
        .map(|c| {
            crate::db::apply_write_pragmas(&c);
            c
        })
        .map_err(|e| format!("Database connection error: {}", e))
}

fn match_prompt_route(path: &str) -> Option<PromptRoute> {
    if path == "/api/prompts" {
        return Some(PromptRoute::Collection);
    }
    let rest = path.strip_prefix("/api/prompts/")?;
    if rest.is_empty() {
        return None;
    }
    let parts: Vec<&str> = rest.split('/').collect();
    let id: i64 = parts.first()?.parse().ok()?;
    match parts.get(1).copied() {
        None => Some(PromptRoute::Item(id)),
        _ => None,
    }
}

fn parse_prompt_input(body: &str) -> Result<PromptInput, String> {
    serde_json::from_str(body).map_err(|e| format!("Invalid JSON body: {}", e))
}

fn map_prompt_db_error(err: rusqlite::Error) -> (u16, &'static str, String) {
    match err {
        rusqlite::Error::QueryReturnedNoRows => (404, "NOT_FOUND", "prompt not found".to_string()),
        rusqlite::Error::InvalidParameterName(msg) if msg.starts_with("invalid category:") => (
            400,
            "INVALID_CATEGORY",
            msg,
        ),
        rusqlite::Error::InvalidParameterName(msg) if msg.contains("title") => {
            (400, "VALIDATION_ERROR", msg)
        }
        other => (400, "VALIDATION_ERROR", other.to_string()),
    }
}

fn send_api_error(
    stream: &mut TcpStream,
    status: u16,
    code: &str,
    message: &str,
    extra: Option<serde_json::Value>,
) {
    let mut body = json!({
        "ok": false,
        "code": code,
        "error": message,
    });
    if let Some(obj) = body.as_object_mut() {
        if let Some(ext) = extra {
            if let Some(ext_obj) = ext.as_object() {
                for (k, v) in ext_obj {
                    obj.insert(k.clone(), v.clone());
                }
            }
        }
    }
    send_json(stream, status, body);
}

fn category_error_extra() -> serde_json::Value {
    json!({ "allowed_categories": allowed_prompt_category_values() })
}

fn route_prompt_mut(stream: &mut TcpStream, method: &str, route: PromptRoute, body: &str) {
    let conn = match open_write_conn() {
        Ok(c) => c,
        Err(e) => {
            send_api_error(stream, 500, "DB_ERROR", &e, None);
            return;
        }
    };

    match (method, route) {
        ("POST", PromptRoute::Collection) => match parse_prompt_input(body) {
            Ok(input) => match create_prompt(&conn, &input) {
                Ok(prompt) => send_json(
                    stream,
                    201,
                    json!({
                        "ok": true,
                        "prompt": PromptAgentItem::detail_from(&prompt)
                    }),
                ),
                Err(e) => {
                    let (status, code, msg) = map_prompt_db_error(e);
                    let extra = if code == "INVALID_CATEGORY" {
                        Some(category_error_extra())
                    } else {
                        None
                    };
                    send_api_error(stream, status, code, &msg, extra);
                }
            },
            Err(e) => send_api_error(stream, 400, "INVALID_JSON", &e, None),
        },
        ("PUT", PromptRoute::Item(id)) => match parse_prompt_input(body) {
            Ok(input) => match update_prompt(&conn, id, &input) {
                Ok(prompt) => send_json(
                    stream,
                    200,
                    json!({
                        "ok": true,
                        "prompt": PromptAgentItem::detail_from(&prompt)
                    }),
                ),
                Err(e) => {
                    let (status, code, msg) = map_prompt_db_error(e);
                    let extra = if code == "INVALID_CATEGORY" {
                        Some(category_error_extra())
                    } else {
                        None
                    };
                    send_api_error(stream, status, code, &msg, extra);
                }
            },
            Err(e) => send_api_error(stream, 400, "INVALID_JSON", &e, None),
        },
        ("DELETE", PromptRoute::Item(id)) => match delete_prompt(&conn, id) {
            Ok(true) => send_json(stream, 200, json!({"ok": true, "deleted": true, "id": id})),
            Ok(false) => send_api_error(stream, 404, "NOT_FOUND", "prompt not found", None),
            Err(e) => send_api_error(stream, 500, "DB_ERROR", &e.to_string(), None),
        },
        _ => send_api_error(
            stream,
            405,
            "METHOD_NOT_ALLOWED",
            &format!("Method {} not allowed for prompt route", method),
            None,
        ),
    }
}

fn route_get(
    stream: &mut TcpStream,
    method: &str,
    path: &str,
    query_params: &HashMap<String, String>,
    conn: &Connection,
) {
    if method == "HEAD" {
        send_response(stream, 200, "application/json", "");
        return;
    }

    match path {
        "/" | "/docs" | "/api/docs" => {
            let html = include_str!("api_docs.html");
            send_response(stream, 200, "text/html; charset=utf-8", html);
        }

        "/api/docs/markdown" => {
            let md = include_str!("api_docs.generated.md");
            send_response(stream, 200, "text/markdown; charset=utf-8", md);
        }

        "/health" => {
            let stats = fetch_dashboard_stats(conn).ok();
            send_json(
                stream,
                200,
                json!({
                    "ok": true,
                    "status": "ok",
                    "app": "AgentDeck",
                    "version": env!("CARGO_PKG_VERSION"),
                    "cursor_available": true,
                    "ai_available": true,
                    "stats": stats
                }),
            );
        }

        "/api/prompts/categories" => {
            send_json(
                stream,
                200,
                json!({
                    "ok": true,
                    "categories": prompt_category_options(),
                    "hint": "Call this before POST /api/prompts to pick a category value."
                }),
            );
        }

        "/api/prompts" => {
            let q = query_params.get("q").map(|s| s.as_str());
            let category = query_params.get("category").map(|s| s.as_str());
            let limit = query_params
                .get("limit")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(50)
                .clamp(1, 200);
            match list_prompts(conn, q, category, false) {
                Ok(prompts) => {
                    let total = prompts.len();
                    let items: Vec<PromptAgentItem> = prompts
                        .iter()
                        .take(limit)
                        .map(PromptAgentItem::list_from)
                        .collect();
                    send_json(
                        stream,
                        200,
                        json!({
                            "ok": true,
                            "total": total,
                            "limit": limit,
                            "categories": prompt_category_options(),
                            "prompts": items
                        }),
                    );
                }
                Err(e) => send_api_error(stream, 500, "DB_ERROR", &e.to_string(), None),
            }
        }

        p if p.starts_with("/api/prompts/") => {
            if let Some(PromptRoute::Item(id)) = match_prompt_route(p) {
                match get_prompt(conn, id) {
                    Ok(prompt) => send_json(
                        stream,
                        200,
                        json!({
                            "ok": true,
                            "prompt": PromptAgentItem::detail_from(&prompt)
                        }),
                    ),
                    Err(_) => {
                        send_api_error(stream, 404, "NOT_FOUND", "prompt not found", None)
                    }
                }
            } else {
                send_api_error(stream, 404, "NOT_FOUND", "prompt route not found", None);
            }
        }

        "/api/stats" | "/api/dashboard-stats" => match fetch_dashboard_stats(conn) {
            Ok(stats) => send_json(
                stream,
                200,
                json!({
                    "ok": true,
                    "stats": stats,
                    "cursor_available": true,
                    "ai_available": true
                }),
            ),
            Err(e) => send_json(stream, 500, json!({"ok": false, "error": e.to_string()})),
        },

        "/api/workspaces" => {
            let q = query_params.get("q").map(|s| s.as_str());
            match fetch_workspaces(conn, q) {
                Ok(workspaces) => {
                    let total = workspaces.len();
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
                    send_json(
                        stream,
                        200,
                        json!({"ok": true, "total": total, "workspaces": mapped_workspaces}),
                    );
                }
                Err(e) => send_json(stream, 500, json!({"ok": false, "error": e.to_string()})),
            }
        }

        "/api/conversations" => {
            let workspace = query_params.get("workspace").map(|s| s.as_str());
            let q = query_params.get("q").map(|s| s.as_str());
            let starred = query_params
                .get("starred")
                .map(|s| s == "1" || s == "true")
                .unwrap_or(false);
            match fetch_conversations(conn, workspace, q, starred) {
                Ok(conversations) => send_json(
                    stream,
                    200,
                    json!({
                        "ok": true,
                        "total": conversations.len(),
                        "conversations": conversations
                    }),
                ),
                Err(e) => send_json(stream, 500, json!({"ok": false, "error": e.to_string()})),
            }
        }

        p if p.starts_with("/api/conversations/") => {
            let cid = &p["/api/conversations/".len()..];
            match fetch_conversation_messages(conn, cid) {
                Ok(msgs) => send_json(
                    stream,
                    200,
                    json!({"ok": true, "conversation_id": cid, "messages": msgs}),
                ),
                Err(e) => send_json(stream, 500, json!({"ok": false, "error": e.to_string()})),
            }
        }

        p if p.starts_with("/api/conversation/") && p.ends_with("/messages") => {
            let mid = &p["/api/conversation/".len()..p.len() - "/messages".len()];
            match fetch_conversation_messages(conn, mid) {
                Ok(msgs) => send_json(
                    stream,
                    200,
                    json!({"ok": true, "conversation_id": mid, "messages": msgs}),
                ),
                Err(e) => send_json(stream, 500, json!({"ok": false, "error": e.to_string()})),
            }
        }

        "/api/search" | "/api/spotlight" => {
            let q = query_params.get("q").cloned().unwrap_or_default();
            let role = query_params.get("role").map(|s| s.as_str());
            let limit = query_params
                .get("limit")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(30);
            match search_global_messages(conn, &q, role, limit) {
                Ok(items) => send_json(
                    stream,
                    200,
                    json!({"ok": true, "total": items.len(), "items": items}),
                ),
                Err(e) => send_json(stream, 500, json!({"ok": false, "error": e.to_string()})),
            }
        }

        "/api/user-messages" => route_user_messages(stream, query_params, conn),

        "/api/workspace/stats" | "/api/workspace/analysis" => {
            let ws = query_params.get("workspace").cloned().unwrap_or_default();
            match fetch_workspace_detail_stats(conn, &ws) {
                Ok(detail) => send_json(
                    stream,
                    200,
                    json!({"ok": true, "workspace_path": ws, "stats": detail}),
                ),
                Err(e) => send_json(stream, 500, json!({"ok": false, "error": e.to_string()})),
            }
        }

        "/ag-image" => {
            let path_param = query_params.get("path").cloned().unwrap_or_default();
            if let Some(img_path) = get_ag_image_path(&path_param) {
                if let Ok(bytes) = std::fs::read(&img_path) {
                    let mime = get_mime_from_path(&img_path);
                    send_binary_response(stream, 200, mime, &bytes);
                    return;
                }
            }
            send_json(stream, 404, json!({"ok": false, "error": "Image not found"}));
        }

        p if p.starts_with("/cursor-image/") => {
            let uuid = &p["/cursor-image/".len()..];
            if let Some(img_path) = get_cursor_image_path(uuid) {
                if let Ok(bytes) = std::fs::read(&img_path) {
                    let mime = get_mime_from_path(&img_path);
                    send_binary_response(stream, 200, mime, &bytes);
                    return;
                }
            }
            send_json(stream, 404, json!({"ok": false, "error": "Cursor image not found"}));
        }

        p if p.starts_with("/media/") => {
            let rel_path = &p["/media/".len()..];
            if !rel_path.contains("..") {
                if let Some(media_root) = crate::media_archive::get_media_root() {
                    let target = media_root.join(rel_path);
                    if target.is_file() {
                        if let Ok(bytes) = std::fs::read(&target) {
                            let mime = get_mime_from_path(&target);
                            send_binary_response(stream, 200, mime, &bytes);
                            return;
                        }
                    }
                }
            }
            send_json(stream, 404, json!({"ok": false, "error": "Media asset not found"}));
        }

        _ => send_json(
            stream,
            404,
            json!({"ok": false, "error": format!("Route not found: GET {}", path)}),
        ),
    }
}

fn route_user_messages(
    stream: &mut TcpStream,
    query_params: &HashMap<String, String>,
    conn: &Connection,
) {
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
            send_json(stream, 500, json!({"ok": false, "error": e.to_string()}));
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
                    if let Some(arr) = item.get_mut("messages").and_then(|v| v.as_array_mut()) {
                        arr.push(json!(line));
                    }
                }
            }

            let compact_convs: Vec<serde_json::Value> = conv_order
                .into_iter()
                .filter_map(|cid| conv_map.remove(&cid))
                .collect();

            if fmt == "flat" || fmt == "raw" {
                send_json(
                    stream,
                    200,
                    json!({
                        "ok": true,
                        "total": total,
                        "limit": limit,
                        "offset": offset,
                        "date": date,
                        "workspace": workspace,
                        "messages": flat_msgs
                    }),
                );
            } else {
                send_json(
                    stream,
                    200,
                    json!({
                        "ok": true,
                        "total_messages": total,
                        "total_conversations": compact_convs.len(),
                        "limit": limit,
                        "offset": offset,
                        "date": date,
                        "workspace": workspace,
                        "conversations": compact_convs,
                        "messages": flat_msgs
                    }),
                );
            }
        }
        Err(e) => send_json(stream, 500, json!({"ok": false, "error": e.to_string()})),
    }
}

fn send_json(stream: &mut TcpStream, status_code: u16, body: serde_json::Value) {
    send_response(stream, status_code, "application/json", &body.to_string());
}

fn send_options_response(stream: &mut TcpStream) {
    let response = "HTTP/1.1 204 No Content\r\n\
        Access-Control-Allow-Origin: *\r\n\
        Access-Control-Allow-Methods: GET, POST, PUT, PATCH, DELETE, OPTIONS\r\n\
        Access-Control-Allow-Headers: Content-Type, Authorization\r\n\
        Content-Length: 0\r\n\
        Connection: close\r\n\
        \r\n";
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
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
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "OK",
    };

    let response = format!(
        "HTTP/1.1 {} {}\r\n\
        Content-Type: {}\r\n\
        Content-Length: {}\r\n\
        Access-Control-Allow-Origin: *\r\n\
        Access-Control-Allow-Methods: GET, POST, PUT, PATCH, DELETE, OPTIONS\r\n\
        Access-Control-Allow-Headers: Content-Type, Authorization\r\n\
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
