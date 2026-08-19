use rusqlite::Connection;
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use walkdir::WalkDir;

use super::{
    needs_sync, record_sync_state, save_conversation_tx, ImporterStats, RawConversation, RawMessage,
};

pub fn sync(conn: &Connection, incremental: bool) -> ImporterStats {
    let mut stats = ImporterStats {
        app: "Codex".to_string(),
        new_count: 0,
        updated_count: 0,
        skipped_count: 0,
        error_count: 0,
    };

    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return stats,
    };

    let codex_dir = home.join(".codex/sessions");
    if !codex_dir.is_dir() {
        return stats;
    }

    let files: Vec<_> = WalkDir::new(&codex_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().is_file() && e.path().extension().and_then(|s| s.to_str()) == Some("jsonl")
        })
        .collect();

    for entry in files {
        let p = entry.path();

        if incremental && !needs_sync(conn, p, true) {
            stats.skipped_count += 1;
            continue;
        }

        let stem = p
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let session_id = if stem.starts_with("rollout-") {
            let parts: Vec<&str> = stem.split('-').collect();
            if parts.len() >= 5 {
                parts[parts.len() - 5..].join("-")
            } else {
                stem
            }
        } else {
            stem
        };

        let cid = format!("codex:{}", session_id);

        match parse_codex_file(&cid, p) {
            Ok(Some(conv)) => match save_conversation_tx(conn, &conv) {
                Ok(is_new) => {
                    record_sync_state(conn, p, &cid, "codex_jsonl");
                    if is_new {
                        stats.new_count += 1;
                    } else {
                        stats.updated_count += 1;
                    }
                }
                Err(e) => {
                    eprintln!("[Codex Importer] 保存失败 {}: {}", cid, e);
                    stats.error_count += 1;
                }
            },
            Ok(None) => {
                record_sync_state(conn, p, &cid, "codex_jsonl");
                stats.skipped_count += 1;
            }
            Err(e) => {
                eprintln!("[Codex Importer] 解析失败 {}: {}", cid, e);
                stats.error_count += 1;
            }
        }
    }

    stats
}

fn parse_codex_file(
    cid: &str,
    path: &Path,
) -> Result<Option<RawConversation>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let mut messages = Vec::new();
    let mut title = String::new();
    let mut workspace_path = String::new();
    let mut created_at = None;
    let mut updated_at = None;
    let mut step_idx = 0i64;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let val: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let ts = val
            .get("timestamp")
            .or_else(|| val.get("created_at"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if created_at.is_none() && ts.is_some() {
            created_at = ts.clone();
        }
        if ts.is_some() {
            updated_at = ts.clone();
        }

        let obj_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");

        if obj_type == "session_meta" {
            if let Some(payload) = val.get("payload") {
                if let Some(cwd) = payload.get("cwd").and_then(|v| v.as_str()) {
                    workspace_path = super::canonicalize_workspace_path(cwd);
                }
            }
            continue;
        }

        if obj_type == "response_item" {
            if let Some(payload) = val.get("payload") {
                let p_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if p_type == "message" {
                    let role = payload
                        .get("role")
                        .and_then(|v| v.as_str())
                        .unwrap_or("user");
                    let mut text_parts = Vec::new();
                    if let Some(arr) = payload.get("content").and_then(|v| v.as_array()) {
                        for b in arr {
                            if let Some(t) = b.get("text").and_then(|v| v.as_str()) {
                                if !t.starts_with("<environment_context>") {
                                    text_parts.push(t.to_string());
                                }
                            } else if let Some(t) = b.as_str() {
                                text_parts.push(t.to_string());
                            }
                        }
                    }
                    let full_text = text_parts.join("\n\n").trim().to_string();
                    if full_text.is_empty() {
                        continue;
                    }

                    if role == "user" && title.is_empty() {
                        title = full_text.chars().take(60).collect();
                    }

                    messages.push(RawMessage {
                        step_index: step_idx,
                        role: role.to_string(),
                        message_type: format!("codex_{}", role),
                        content: full_text,
                        thinking: None,
                        created_at: ts.clone(),
                        model_name: Some("Codex".to_string()),
                        tool_name: None,
                        tool_args: None,
                        duration_ms: None,
                        token_count: None,
                        images: None,
                    });
                    step_idx += 1;
                } else if p_type == "function_call" {
                    let fn_name = payload
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("function");
                    let fn_args = payload
                        .get("arguments")
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    messages.push(RawMessage {
                        step_index: step_idx,
                        role: "assistant".to_string(),
                        message_type: "codex_tool_call".to_string(),
                        content: format!("[Tool call] {}\n{}", fn_name, fn_args),
                        thinking: None,
                        created_at: ts.clone(),
                        model_name: Some("Codex".to_string()),
                        tool_name: Some(fn_name.to_string()),
                        tool_args: Some(fn_args),
                        duration_ms: None,
                        token_count: None,
                        images: None,
                    });
                    step_idx += 1;
                }
            }
            continue;
        }

        let role = val
            .get("role")
            .or_else(|| val.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("user");
        let content = val
            .get("content")
            .or_else(|| val.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if !content.is_empty() {
            if role == "user" && title.is_empty() {
                title = content.chars().take(60).collect();
            }
            let norm_role = if role == "assistant" || role == "bot" {
                "assistant"
            } else {
                "user"
            };
            messages.push(RawMessage {
                step_index: step_idx,
                role: norm_role.to_string(),
                message_type: "text".to_string(),
                content: content.to_string(),
                thinking: None,
                created_at: ts,
                model_name: Some("Codex".to_string()),
                tool_name: None,
                tool_args: None,
                duration_ms: None,
                token_count: None,
                images: None,
            });
            step_idx += 1;
        }
    }

    if messages.is_empty() {
        return Ok(None);
    }

    if title.is_empty() {
        title = format!("Codex 会话 {}", &cid[..cid.len().min(8)]);
    }

    Ok(Some(RawConversation {
        id: cid.to_string(),
        title,
        workspace_path,
        source_app: "codex".to_string(),
        created_at,
        updated_at,
        parse_status: "ok".to_string(),
        source_types: vec!["codex".to_string()],
        messages,
    }))
}
