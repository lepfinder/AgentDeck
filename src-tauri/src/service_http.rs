//! REST handlers for local service management (AgentDeck HTTP API).
//!
//! Scan / probe stay on Tauri IPC for the desktop registration wizard only —
//! they are intentionally not exposed over HTTP.

use crate::service_config::ServiceRegistrationPayload;
use crate::service_manager::{self, ServiceActionResult};
use serde_json::{json, Value};
use std::collections::HashMap;
use tauri::AppHandle;

fn ok(data: Value) -> Value {
    json!({ "ok": true, "data": data })
}

fn err(code: &str, message: impl AsRef<str>) -> Value {
    json!({
        "ok": false,
        "error": { "code": code, "message": message.as_ref() }
    })
}

fn parse_service_path(path: &str) -> Option<(String, String, Option<String>)> {
    // /api/services/{projectId}/{serviceId}[/{action}]
    let rest = path.strip_prefix("/api/services/")?;
    if rest.is_empty() || rest == "projects" || rest.starts_with("projects/") {
        return None;
    }
    let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() < 2 {
        return None;
    }
    let project_id = parts[0].to_string();
    let service_id = parts[1].to_string();
    let action = parts.get(2).map(|s| s.to_string());
    Some((project_id, service_id, action))
}

fn parse_project_path(path: &str) -> Option<String> {
    // /api/services/projects/{projectId}
    let rest = path.strip_prefix("/api/services/projects/")?;
    if rest.is_empty() || rest.contains('/') {
        return None;
    }
    Some(rest.to_string())
}

fn service_key(project_id: &str, service_id: &str) -> String {
    format!("{}/{}", project_id, service_id)
}

pub fn handle_get(
    app: &AppHandle,
    path: &str,
    query: &HashMap<String, String>,
) -> Result<(u16, Value), (u16, Value)> {
    if path == "/api/services" || path == "/api/services/" {
        return match service_manager::api_status(app, None) {
            Ok(list) => Ok((200, ok(json!({ "services": list, "total": list.len() })))),
            Err(e) => Err((500, err("STATUS_FAILED", e))),
        };
    }

    if path == "/api/services/projects" {
        return match service_manager::api_projects(app) {
            Ok(projects) => {
                Ok((200, ok(json!({ "projects": projects, "total": projects.len() }))))
            }
            Err(e) => Err((500, err("LIST_FAILED", e))),
        };
    }

    if let Some((project_id, service_id, action)) = parse_service_path(path) {
        let key = service_key(&project_id, &service_id);
        match action.as_deref() {
            None => match service_manager::api_status(app, Some(key.clone())) {
                Ok(list) if !list.is_empty() => Ok((200, ok(json!(list[0])))),
                Ok(_) => Err((
                    404,
                    err("NOT_FOUND", format!("未知服务: {}/{}", project_id, service_id)),
                )),
                Err(e) => Err((404, err("NOT_FOUND", e))),
            },
            Some("logs") => {
                let lines = query
                    .get("lines")
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(80);
                match service_manager::api_tail_log(app, &key, Some(lines)) {
                    Ok(content) => Ok((
                        200,
                        ok(json!({ "serviceKey": key, "lines": lines, "content": content })),
                    )),
                    Err(e) => Err((404, err("LOG_FAILED", e))),
                }
            }
            Some(other) => Err((404, err("NOT_FOUND", format!("未知子路径: {}", other)))),
        }
    } else {
        Err((404, err("NOT_FOUND", format!("Route not found: GET {}", path))))
    }
}

pub fn handle_mut(
    app: &AppHandle,
    method: &str,
    path: &str,
    body: &str,
) -> Result<(u16, Value), (u16, Value)> {
    if method == "POST" && (path == "/api/services" || path == "/api/services/") {
        let payload: ServiceRegistrationPayload = match serde_json::from_str(body) {
            Ok(p) => p,
            Err(e) => return Err((400, err("BAD_REQUEST", format!("无效 payload: {}", e)))),
        };
        return match service_manager::api_upsert(app, payload) {
            Ok(result) => Ok((200, action_json(result))),
            Err(e) => Err((400, err("UPSERT_FAILED", e))),
        };
    }

    if method == "PATCH" {
        if let Some(project_id) = parse_project_path(path) {
            let Some(name) = parse_json_field(body, "name") else {
                return Err((400, err("BAD_REQUEST", "需要 JSON 字段 name")));
            };
            return match service_manager::api_rename_project(app, project_id, name) {
                Ok(result) => Ok((200, action_json(result))),
                Err(e) => Err((400, err("RENAME_FAILED", e))),
            };
        }
        if let Some((project_id, service_id, action)) = parse_service_path(path) {
            if action.is_none() {
                let Some(name) = parse_json_field(body, "name") else {
                    return Err((400, err("BAD_REQUEST", "需要 JSON 字段 name")));
                };
                let key = service_key(&project_id, &service_id);
                return match service_manager::api_rename_service(app, key, name) {
                    Ok(result) => Ok((200, action_json(result))),
                    Err(e) => Err((400, err("RENAME_FAILED", e))),
                };
            }
        }
    }

    if let Some((project_id, service_id, action)) = parse_service_path(path) {
        let key = service_key(&project_id, &service_id);
        match (method, action.as_deref()) {
            ("POST", Some("start")) => match service_manager::api_start(app.clone(), key) {
                Ok(result) => Ok((202, action_json(result))),
                Err(e) => Err((400, err("START_FAILED", e))),
            },
            ("POST", Some("stop")) => {
                let force = parse_json_bool(body, "force").unwrap_or(false);
                match service_manager::api_stop(app.clone(), key, force) {
                    Ok(result) => Ok((202, action_json(result))),
                    Err(e) => Err((400, err("STOP_FAILED", e))),
                }
            },
            ("POST", Some("restart")) => match service_manager::api_restart(app.clone(), key) {
                Ok(result) => Ok((202, action_json(result))),
                Err(e) => Err((400, err("RESTART_FAILED", e))),
            },
            ("POST", Some("open")) => match service_manager::api_open(app, &key) {
                Ok(()) => Ok((200, ok(json!({ "opened": true, "serviceKey": key })))),
                Err(e) => Err((400, err("OPEN_FAILED", e))),
            },
            ("DELETE", None) => {
                let force = parse_json_bool(body, "force").unwrap_or(false);
                match service_manager::api_remove(app.clone(), key, force) {
                    Ok(result) => Ok((200, action_json(result))),
                    Err(e) => Err((400, err("REMOVE_FAILED", e))),
                }
            }
            _ => Err((
                405,
                err(
                    "METHOD_NOT_ALLOWED",
                    format!("不支持 {} {}", method, path),
                ),
            )),
        }
    } else {
        Err((
            404,
            err("NOT_FOUND", format!("Route not found: {} {}", method, path)),
        ))
    }
}

fn action_json(result: ServiceActionResult) -> Value {
    if result.success {
        json!({
            "ok": true,
            "data": {
                "success": true,
                "accepted": true,
                "message": result.message
            }
        })
    } else {
        json!({
            "ok": false,
            "data": {
                "success": false,
                "accepted": false,
                "message": result.message
            },
            "error": { "code": "ACTION_REJECTED", "message": result.message }
        })
    }
}

fn parse_json_field(body: &str, key: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    v.get(key)?.as_str().map(|s| s.to_string())
}

fn parse_json_bool(body: &str, key: &str) -> Option<bool> {
    if body.trim().is_empty() {
        return None;
    }
    let v: Value = serde_json::from_str(body).ok()?;
    v.get(key)?.as_bool()
}
