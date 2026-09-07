use crate::db::{
    allowed_prompt_category_values, create_prompt, delete_prompt, fetch_conversation_messages,
    fetch_conversations, fetch_daily_timeline, fetch_dashboard_stats, fetch_workspace_detail_stats,
    fetch_workspaces, get_database_path, get_prompt, get_short_workspace, list_prompts, prompt_category_options,
    search_global_messages, update_prompt, PromptAgentItem, PromptInput,
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

        "/api/agent-sources" | "/api/sources" => {
            let sources = crate::sync::collect_agent_sources(conn);
            send_json(stream, 200, json!({ "ok": true, "sources": sources }))
        },

        "/api/daily-timeline" | "/api/timeline" => {
            let date = query_params.get("date").cloned().unwrap_or_else(|| {
                chrono::Utc::now()
                    .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
                    .format("%Y-%m-%d")
                    .to_string()
            });
            match fetch_daily_timeline(conn, &date) {
                Ok(timeline) => send_json(stream, 200, json!(timeline)),
                Err(e) => send_json(stream, 500, json!({"ok": false, "error": e.to_string()})),
            }
        }

        "/api/daily-summary" | "/api/daily-digest" => {
            route_daily_summary(stream, query_params, conn);
        }

        "/api/recent-activity" | "/api/recent" | "/api/hourly-activity" => {
            route_recent_activity(stream, query_params, conn);
        }

        "/api/workspaces" => {
            let q = query_params.get("q").map(|s| s.as_str());
            let date = query_params.get("date").map(|s| s.as_str());
            match fetch_workspaces(conn, q, date) {
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

fn route_daily_summary(
    stream: &mut TcpStream,
    query_params: &HashMap<String, String>,
    conn: &Connection,
) {
    match build_daily_summary(query_params, conn) {
        Ok(body) => send_json(stream, 200, body),
        Err(e) => send_json(stream, 500, json!({"ok": false, "error": e})),
    }
}

pub(crate) fn build_daily_summary(
    query_params: &HashMap<String, String>,
    conn: &Connection,
) -> Result<serde_json::Value, String> {
    let date = query_params
        .get("date")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            chrono::Utc::now()
                .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
                .format("%Y-%m-%d")
                .to_string()
        });
    let workspace = query_params
        .get("workspace")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let role = query_params
        .get("role")
        .map(|s| s.trim().to_lowercase())
        .unwrap_or_else(|| "all".to_string());
    let fmt = query_params
        .get("format")
        .map(|s| s.trim().to_lowercase())
        .unwrap_or_else(|| "compact".to_string());
    let max_len = query_params
        .get("max_len")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1000);
    let order = query_params
        .get("order")
        .map(|s| s.trim().to_lowercase())
        .unwrap_or_else(|| "asc".to_string());

    let role_filter = match role.as_str() {
        "user" => "AND m.role = 'user'",
        "assistant" => "AND m.role = 'assistant'",
        _ => "AND m.role IN ('user', 'assistant')",
    };

    let order_clause = if order == "desc" { "DESC" } else { "ASC" };

    let sql = format!(
        r#"
        SELECT
            c.workspace_path,
            m.conversation_id,
            c.title,
            CASE
                WHEN c.source_types LIKE '%claude%' THEN 'claude'
                WHEN c.source_types LIKE '%cursor%' THEN 'cursor'
                WHEN c.source_types LIKE '%codex%' THEN 'codex'
                WHEN c.source_types LIKE '%workbuddy%' THEN 'workbuddy'
                WHEN c.source_types LIKE '%hermes%' THEN 'hermes'
                ELSE 'antigravity'
            END as source_app,
            m.role,
            m.content,
            m.created_at,
            datetime(m.created_at, '+8 hours') as bj_created_at
        FROM messages m
        JOIN conversations c ON m.conversation_id = c.id
        WHERE strftime('%Y-%m-%d', datetime(m.created_at, '+8 hours')) = ?1
          AND (?2 IS NULL OR ?2 = '' OR c.workspace_path = ?2 OR c.workspace_path LIKE '%' || ?2 || '%')
          {}
        ORDER BY c.workspace_path ASC, m.conversation_id ASC, datetime(m.created_at, '+8 hours') {}
        "#,
        role_filter, order_clause
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(rusqlite::params![date, workspace.unwrap_or("")], |row| {
            let ws_path: String = row.get(0)?;
            let conv_id: String = row.get(1)?;
            let title: Option<String> = row.get(2)?;
            let source_app: Option<String> = row.get(3)?;
            let msg_role: String = row.get(4)?;
            let content: String = row.get(5)?;
            let raw_created: Option<String> = row.get(6)?;
            let bj_created: Option<String> = row.get(7)?;

            Ok((
                ws_path,
                conv_id,
                title.unwrap_or_default(),
                source_app.unwrap_or_else(|| "unknown".to_string()),
                msg_role,
                content,
                raw_created,
                bj_created,
            ))
        })
        .map_err(|e| e.to_string())?;

    struct ConvEntry {
        id: String,
        title: String,
        source: String,
        created_at: Option<String>,
        messages: Vec<serde_json::Value>,
    }

    struct WsEntry {
        workspace_path: String,
        workspace_short: String,
        conv_order: Vec<String>,
        conv_map: HashMap<String, ConvEntry>,
    }

    let mut ws_order: Vec<String> = Vec::new();
    let mut ws_map: HashMap<String, WsEntry> = HashMap::new();
    let mut total_messages = 0usize;
    let mut total_conversations = 0usize;

    for row_res in rows.filter_map(Result::ok) {
        let (ws_path, conv_id, title, source, msg_role, content, raw_created, bj_created) = row_res;
        total_messages += 1;

        // 提取时间标签 [HH:MM:SS]
        let time_label = if let Some(ref bj) = bj_created {
            if let Some(t_part) = bj.split_whitespace().nth(1) {
                t_part.to_string()
            } else {
                bj.clone()
            }
        } else if let Some(ref raw) = raw_created {
            format_beijing_tag(Some(raw), true)
                .trim()
                .trim_matches('[')
                .trim_matches(']')
                .to_string()
        } else {
            "00:00:00".to_string()
        };

        // 消息文本截断，超出部分附带 ... [truncated]
        let content_trimmed = content.trim();
        let final_content = if max_len > 0 && content_trimmed.chars().count() > max_len {
            let truncated: String = content_trimmed.chars().take(max_len).collect();
            format!("{}... [truncated]", truncated)
        } else {
            content_trimmed.to_string()
        };

        // 格式化单条消息
        let formatted_msg = if fmt == "json" {
            json!({
                "time": time_label,
                "role": msg_role,
                "content": final_content
            })
        } else {
            json!(format!("[{}] [{}] {}", time_label, msg_role, final_content))
        };

        let ws_entry = ws_map.entry(ws_path.clone()).or_insert_with(|| {
            ws_order.push(ws_path.clone());
            WsEntry {
                workspace_short: get_short_workspace(&ws_path),
                workspace_path: ws_path.clone(),
                conv_order: Vec::new(),
                conv_map: HashMap::new(),
            }
        });

        if !ws_entry.conv_map.contains_key(&conv_id) {
            total_conversations += 1;
            ws_entry.conv_order.push(conv_id.clone());
            let beijing_conv_time = convert_to_beijing_iso(raw_created);
            ws_entry.conv_map.insert(
                conv_id.clone(),
                ConvEntry {
                    id: conv_id.clone(),
                    title: if title.trim().is_empty() {
                        "未命名会话".to_string()
                    } else {
                        title
                    },
                    source,
                    created_at: beijing_conv_time,
                    messages: vec![formatted_msg],
                },
            );
        } else if let Some(conv) = ws_entry.conv_map.get_mut(&conv_id) {
            conv.messages.push(formatted_msg);
        }
    }

    let mut workspaces_json = Vec::new();
    for ws_path in ws_order {
        if let Some(mut ws) = ws_map.remove(&ws_path) {
            let mut convs_json = Vec::new();
            let mut ws_msg_count = 0usize;
            for cid in ws.conv_order {
                if let Some(c) = ws.conv_map.remove(&cid) {
                    let count = c.messages.len();
                    ws_msg_count += count;
                    convs_json.push(json!({
                        "id": c.id,
                        "title": c.title,
                        "source": c.source,
                        "message_count": count,
                        "created_at": c.created_at,
                        "messages": c.messages
                    }));
                }
            }
            workspaces_json.push(json!({
                "workspace_path": ws.workspace_path,
                "workspace_short": ws.workspace_short,
                "conversation_count": convs_json.len(),
                "message_count": ws_msg_count,
                "conversations": convs_json
            }));
        }
    }

    Ok(json!({
        "ok": true,
        "date": date,
        "total_workspaces": workspaces_json.len(),
        "total_conversations": total_conversations,
        "total_messages": total_messages,
        "workspaces": workspaces_json
    }))
}

fn route_recent_activity(
    stream: &mut TcpStream,
    query_params: &HashMap<String, String>,
    conn: &Connection,
) {
    match build_recent_activity(query_params, conn) {
        Ok(body) => send_json(stream, 200, body),
        Err(e) => send_json(stream, 500, json!({"ok": false, "error": e})),
    }
}

pub(crate) fn build_recent_activity(
    query_params: &HashMap<String, String>,
    conn: &Connection,
) -> Result<serde_json::Value, String> {
    let minutes: i64 = if let Some(m) = query_params.get("minutes").and_then(|s| s.parse::<i64>().ok()) {
        m.max(1)
    } else if let Some(h) = query_params.get("hours").and_then(|s| s.parse::<f64>().ok()) {
        ((h * 60.0).round() as i64).max(1)
    } else {
        60
    };

    let now_utc = chrono::Utc::now();
    let since_utc = now_utc - chrono::Duration::minutes(minutes);
    let beijing_offset = chrono::FixedOffset::east_opt(8 * 3600).unwrap();

    let since_param = query_params
        .get("since")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    let (since_sql_val, since_bj_str, since_utc_str) = if let Some(s) = since_param {
        (s.to_string(), s.to_string(), s.to_string())
    } else {
        let utc_str = since_utc.format("%Y-%m-%d %H:%M:%S").to_string();
        let bj_str = since_utc.with_timezone(&beijing_offset).format("%Y-%m-%d %H:%M:%S").to_string();
        (utc_str.clone(), bj_str, utc_str)
    };

    let until_bj_str = now_utc.with_timezone(&beijing_offset).format("%Y-%m-%d %H:%M:%S").to_string();
    let until_utc_str = now_utc.format("%Y-%m-%d %H:%M:%S").to_string();

    let workspace = query_params
        .get("workspace")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let source = query_params
        .get("source")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let role = query_params
        .get("role")
        .map(|s| s.trim().to_lowercase())
        .unwrap_or_else(|| "all".to_string());
    let fmt = query_params
        .get("format")
        .map(|s| s.trim().to_lowercase())
        .unwrap_or_else(|| "compact".to_string());
    let max_len = query_params
        .get("max_len")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1000);
    let order = query_params
        .get("order")
        .map(|s| s.trim().to_lowercase())
        .unwrap_or_else(|| "desc".to_string());
    let limit = query_params
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(200);

    let role_filter = match role.as_str() {
        "user" => "AND m.role = 'user'",
        "assistant" => "AND m.role = 'assistant'",
        _ => "AND m.role IN ('user', 'assistant')",
    };

    let order_clause = if order == "asc" { "ASC" } else { "DESC" };
    let limit_clause = if limit > 0 {
        format!("LIMIT {}", limit)
    } else {
        "".to_string()
    };

    let sql = format!(
        r#"
        SELECT
            c.workspace_path,
            m.conversation_id,
            c.title,
            CASE
                WHEN c.source_types LIKE '%claude%' THEN 'claude'
                WHEN c.source_types LIKE '%cursor%' THEN 'cursor'
                WHEN c.source_types LIKE '%codex%' THEN 'codex'
                WHEN c.source_types LIKE '%workbuddy%' THEN 'workbuddy'
                WHEN c.source_types LIKE '%hermes%' THEN 'hermes'
                ELSE 'antigravity'
            END as source_app,
            m.role,
            m.content,
            m.created_at,
            datetime(m.created_at, '+8 hours') as bj_created_at
        FROM messages m
        JOIN conversations c ON m.conversation_id = c.id
        WHERE datetime(m.created_at) >= datetime(?1)
          AND (?2 IS NULL OR ?2 = '' OR c.workspace_path = ?2 OR c.workspace_path LIKE '%' || ?2 || '%')
          AND (?3 IS NULL OR ?3 = '' OR c.source_types LIKE '%' || ?3 || '%')
          {}
        ORDER BY datetime(m.created_at, '+8 hours') {}
        {}
        "#,
        role_filter, order_clause, limit_clause
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(
            rusqlite::params![since_sql_val, workspace.unwrap_or(""), source.unwrap_or("")],
            |row| {
                let ws_path: String = row.get(0)?;
                let conv_id: String = row.get(1)?;
                let title: Option<String> = row.get(2)?;
                let source_app: Option<String> = row.get(3)?;
                let msg_role: String = row.get(4)?;
                let content: String = row.get(5)?;
                let raw_created: Option<String> = row.get(6)?;
                let bj_created: Option<String> = row.get(7)?;

                Ok((
                    ws_path,
                    conv_id,
                    title.unwrap_or_default(),
                    source_app.unwrap_or_else(|| "unknown".to_string()),
                    msg_role,
                    content,
                    raw_created,
                    bj_created,
                ))
            },
        )
        .map_err(|e| e.to_string())?;

    let time_window = json!({
        "minutes": minutes,
        "since_beijing": since_bj_str,
        "until_beijing": until_bj_str,
        "since_utc": since_utc_str,
        "until_utc": until_utc_str,
    });

    if fmt == "timeline" || fmt == "flat" {
        let mut timeline = Vec::new();
        for row_res in rows.filter_map(Result::ok) {
            let (ws_path, conv_id, title, source, msg_role, content, raw_created, bj_created) = row_res;
            let time_label = if let Some(ref bj) = bj_created {
                if let Some(t_part) = bj.split_whitespace().nth(1) {
                    t_part.to_string()
                } else {
                    bj.clone()
                }
            } else {
                "00:00:00".to_string()
            };

            let content_trimmed = content.trim();
            let final_content = if max_len > 0 && content_trimmed.chars().count() > max_len {
                let truncated: String = content_trimmed.chars().take(max_len).collect();
                format!("{}... [truncated]", truncated)
            } else {
                content_trimmed.to_string()
            };

            timeline.push(json!({
                "time": time_label,
                "workspace": get_short_workspace(&ws_path),
                "workspace_path": ws_path,
                "conversation_id": conv_id,
                "conversation_title": if title.trim().is_empty() { "未命名会话".to_string() } else { title },
                "source": source,
                "role": msg_role,
                "content": final_content,
                "created_at": convert_to_beijing_iso(raw_created)
            }));
        }

        return Ok(json!({
            "ok": true,
            "time_window": time_window,
            "total_messages": timeline.len(),
            "timeline": timeline
        }));
    }

    struct RecentConvEntry {
        id: String,
        title: String,
        source: String,
        last_activity_at: Option<String>,
        messages: Vec<serde_json::Value>,
    }

    struct RecentWsEntry {
        workspace_path: String,
        workspace_short: String,
        conv_order: Vec<String>,
        conv_map: HashMap<String, RecentConvEntry>,
    }

    let mut ws_order: Vec<String> = Vec::new();
    let mut ws_map: HashMap<String, RecentWsEntry> = HashMap::new();
    let mut total_messages = 0usize;
    let mut total_conversations = 0usize;

    for row_res in rows.filter_map(Result::ok) {
        let (ws_path, conv_id, title, source, msg_role, content, raw_created, bj_created) = row_res;
        total_messages += 1;

        let time_label = if let Some(ref bj) = bj_created {
            if let Some(t_part) = bj.split_whitespace().nth(1) {
                t_part.to_string()
            } else {
                bj.clone()
            }
        } else {
            "00:00:00".to_string()
        };

        let content_trimmed = content.trim();
        let final_content = if max_len > 0 && content_trimmed.chars().count() > max_len {
            let truncated: String = content_trimmed.chars().take(max_len).collect();
            format!("{}... [truncated]", truncated)
        } else {
            content_trimmed.to_string()
        };

        let formatted_msg = if fmt == "json" {
            json!({
                "time": time_label,
                "role": msg_role,
                "content": final_content
            })
        } else {
            json!(format!("[{}] [{}] {}", time_label, msg_role, final_content))
        };

        let ws_entry = ws_map.entry(ws_path.clone()).or_insert_with(|| {
            ws_order.push(ws_path.clone());
            RecentWsEntry {
                workspace_short: get_short_workspace(&ws_path),
                workspace_path: ws_path.clone(),
                conv_order: Vec::new(),
                conv_map: HashMap::new(),
            }
        });

        if !ws_entry.conv_map.contains_key(&conv_id) {
            total_conversations += 1;
            ws_entry.conv_order.push(conv_id.clone());
            let beijing_conv_time = convert_to_beijing_iso(raw_created);
            ws_entry.conv_map.insert(
                conv_id.clone(),
                RecentConvEntry {
                    id: conv_id.clone(),
                    title: if title.trim().is_empty() {
                        "未命名会话".to_string()
                    } else {
                        title
                    },
                    source,
                    last_activity_at: beijing_conv_time,
                    messages: vec![formatted_msg],
                },
            );
        } else if let Some(conv) = ws_entry.conv_map.get_mut(&conv_id) {
            conv.messages.push(formatted_msg);
        }
    }

    let mut workspaces_json = Vec::new();
    for ws_path in ws_order {
        if let Some(mut ws) = ws_map.remove(&ws_path) {
            let mut convs_json = Vec::new();
            let mut ws_msg_count = 0usize;
            for cid in ws.conv_order {
                if let Some(c) = ws.conv_map.remove(&cid) {
                    let count = c.messages.len();
                    ws_msg_count += count;
                    convs_json.push(json!({
                        "id": c.id,
                        "title": c.title,
                        "source": c.source,
                        "message_count": count,
                        "last_activity_at": c.last_activity_at,
                        "messages": c.messages
                    }));
                }
            }
            workspaces_json.push(json!({
                "workspace_path": ws.workspace_path,
                "workspace_short": ws.workspace_short,
                "conversation_count": convs_json.len(),
                "message_count": ws_msg_count,
                "conversations": convs_json
            }));
        }
    }

    Ok(json!({
        "ok": true,
        "time_window": time_window,
        "total_workspaces": workspaces_json.len(),
        "total_conversations": total_conversations,
        "total_messages": total_messages,
        "workspaces": workspaces_json
    }))
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

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_daily_summary_empty() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();
        let query_params = HashMap::new();
        let res = build_daily_summary(&query_params, &conn).unwrap();
        assert_eq!(res["ok"], true);
        assert_eq!(res["total_workspaces"], 0);
        assert_eq!(res["total_conversations"], 0);
        assert_eq!(res["total_messages"], 0);
    }

    #[test]
    fn test_daily_summary_with_data() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();

        conn.execute(
            "INSERT INTO conversations (id, workspace_path, title, source_types) VALUES ('c1', '/Users/test/ws1', 'Test Conv 1', '[\"antigravity\"]')",
            [],
        ).unwrap();

        conn.execute(
            "INSERT INTO messages (conversation_id, role, content, created_at) VALUES ('c1', 'user', 'Hello Agent', '2026-09-05 02:00:00')",
            [],
        ).unwrap();

        conn.execute(
            "INSERT INTO messages (conversation_id, role, content, created_at) VALUES ('c1', 'assistant', 'Hello User, how can I help you today?', '2026-09-05 02:01:00')",
            [],
        ).unwrap();

        let mut query_params = HashMap::new();
        query_params.insert("date".to_string(), "2026-09-05".to_string());

        // 测试默认 compact 模式
        let res = build_daily_summary(&query_params, &conn).unwrap();
        assert_eq!(res["ok"], true);
        assert_eq!(res["total_workspaces"], 1);
        assert_eq!(res["total_conversations"], 1);
        assert_eq!(res["total_messages"], 2);

        let ws = &res["workspaces"][0];
        assert_eq!(ws["workspace_path"], "/Users/test/ws1");
        assert_eq!(ws["workspace_short"], "test/ws1");
        assert_eq!(ws["conversation_count"], 1);
        assert_eq!(ws["message_count"], 2);

        let conv = &ws["conversations"][0];
        assert_eq!(conv["title"], "Test Conv 1");
        assert_eq!(conv["messages"].as_array().unwrap().len(), 2);
        let msg0 = conv["messages"][0].as_str().unwrap();
        assert!(msg0.contains("[user] Hello Agent"));

        // 测试 format=json 模式
        query_params.insert("format".to_string(), "json".to_string());
        let res_json = build_daily_summary(&query_params, &conn).unwrap();
        let conv_json = &res_json["workspaces"][0]["conversations"][0];
        let msg_obj0 = &conv_json["messages"][0];
        assert_eq!(msg_obj0["role"], "user");
        assert_eq!(msg_obj0["content"], "Hello Agent");

        // 测试 max_len 截断
        query_params.insert("max_len".to_string(), "10".to_string());
        let res_trunc = build_daily_summary(&query_params, &conn).unwrap();
        let conv_trunc = &res_trunc["workspaces"][0]["conversations"][0];
        let msg_obj1 = &conv_trunc["messages"][1];
        assert!(msg_obj1["content"].as_str().unwrap().ends_with("... [truncated]"));

        // 测试 role=user 过滤
        query_params.insert("role".to_string(), "user".to_string());
        let res_user_only = build_daily_summary(&query_params, &conn).unwrap();
        assert_eq!(res_user_only["total_messages"], 1);
        let conv_user_only = &res_user_only["workspaces"][0]["conversations"][0];
        assert_eq!(conv_user_only["messages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_recent_activity_empty() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();
        let query_params = HashMap::new();
        let res = build_recent_activity(&query_params, &conn).unwrap();
        assert_eq!(res["ok"], true);
        assert_eq!(res["total_workspaces"], 0);
        assert_eq!(res["total_conversations"], 0);
        assert_eq!(res["total_messages"], 0);
    }

    #[test]
    fn test_recent_activity_with_data() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();

        conn.execute(
            "INSERT INTO conversations (id, workspace_path, title, source_types) VALUES ('c1', '/Users/test/ws1', 'Recent Conv 1', '[\"antigravity\"]')",
            [],
        ).unwrap();

        let now = chrono::Utc::now();
        let min_20_ago = (now - chrono::Duration::minutes(20)).format("%Y-%m-%d %H:%M:%S").to_string();
        let min_10_ago = (now - chrono::Duration::minutes(10)).format("%Y-%m-%d %H:%M:%S").to_string();
        let hours_3_ago = (now - chrono::Duration::hours(3)).format("%Y-%m-%d %H:%M:%S").to_string();

        // 20 分钟前 user 消息
        conn.execute(
            "INSERT INTO messages (conversation_id, role, content, created_at) VALUES ('c1', 'user', 'What are you doing?', ?1)",
            rusqlite::params![min_20_ago],
        ).unwrap();

        // 10 分钟前 assistant 消息
        conn.execute(
            "INSERT INTO messages (conversation_id, role, content, created_at) VALUES ('c1', 'assistant', 'I am coding right now.', ?1)",
            rusqlite::params![min_10_ago],
        ).unwrap();

        // 3 小时前旧消息
        conn.execute(
            "INSERT INTO messages (conversation_id, role, content, created_at) VALUES ('c1', 'user', 'An old question from 3 hours ago', ?1)",
            rusqlite::params![hours_3_ago],
        ).unwrap();

        // 1. 默认查询最近 60 分钟 (minutes=60)，应该只返回 2 条新消息
        let mut query_params = HashMap::new();
        let res = build_recent_activity(&query_params, &conn).unwrap();
        assert_eq!(res["ok"], true);
        assert_eq!(res["total_workspaces"], 1);
        assert_eq!(res["total_conversations"], 1);
        assert_eq!(res["total_messages"], 2);

        let ws = &res["workspaces"][0];
        assert_eq!(ws["workspace_short"], "test/ws1");
        let conv = &ws["conversations"][0];
        assert_eq!(conv["title"], "Recent Conv 1");
        assert_eq!(conv["message_count"], 2);
        let msg0 = conv["messages"][0].as_str().unwrap();
        assert!(msg0.contains("I am coding right now.") || msg0.contains("What are you doing?"));

        // 2. 测试 format=timeline
        query_params.insert("format".to_string(), "timeline".to_string());
        let res_timeline = build_recent_activity(&query_params, &conn).unwrap();
        assert_eq!(res_timeline["total_messages"], 2);
        let timeline_arr = res_timeline["timeline"].as_array().unwrap();
        assert_eq!(timeline_arr.len(), 2);
        assert_eq!(timeline_arr[0]["workspace"], "test/ws1");

        // 3. 扩大时间窗口到 240 分钟，应该能获取全部 3 条消息
        query_params.insert("format".to_string(), "compact".to_string());
        query_params.insert("minutes".to_string(), "240".to_string());
        let res_expanded = build_recent_activity(&query_params, &conn).unwrap();
        assert_eq!(res_expanded["total_messages"], 3);
    }
}

