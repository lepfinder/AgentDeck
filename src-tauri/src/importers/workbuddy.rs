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
        app: "WorkBuddy".to_string(),
        new_count: 0,
        updated_count: 0,
        skipped_count: 0,
        error_count: 0,
    };

    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return stats,
    };

    let wb_dir = home.join(".workbuddy/projects");
    if !wb_dir.is_dir() {
        return stats;
    }

    let files: Vec<_> = WalkDir::new(&wb_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path();
            if p.components().any(|c| c.as_os_str() == "subagents") {
                return false;
            }
            p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("jsonl")
        })
        .collect();

    for entry in files {
        let p = entry.path();

        if incremental && !needs_sync(conn, p, true) {
            stats.skipped_count += 1;
            continue;
        }

        let session_id = p
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let cid = format!("workbuddy:{}", session_id);

        match parse_workbuddy_file(&cid, p) {
            Ok(Some(conv)) => match save_conversation_tx(conn, &conv) {
                Ok(is_new) => {
                    record_sync_state(conn, p, &cid, "workbuddy_jsonl");
                    if is_new {
                        stats.new_count += 1;
                    } else {
                        stats.updated_count += 1;
                    }
                }
                Err(e) => {
                    eprintln!("[WorkBuddy Importer] 保存失败 {}: {}", cid, e);
                    stats.error_count += 1;
                }
            },
            Ok(None) => {
                record_sync_state(conn, p, &cid, "workbuddy_jsonl");
                stats.skipped_count += 1;
            }
            Err(e) => {
                eprintln!("[WorkBuddy Importer] 解析失败 {}: {}", cid, e);
                stats.error_count += 1;
            }
        }
    }

    stats
}

fn parse_workbuddy_file(
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

    if let Some(parent) = path.parent() {
        let parent_name = parent
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if parent_name.starts_with("Users-") {
            let slug = format!("/{}", parent_name.replace('-', "/"));
            workspace_path = super::canonicalize_workspace_path(&slug);
        } else if !parent_name.is_empty() {
            workspace_path = super::canonicalize_workspace_path(&parent_name);
        }
    }

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

        if let Some(cwd) = val
            .get("cwd")
            .or_else(|| val.get("workspace"))
            .and_then(|v| v.as_str())
        {
            if workspace_path.is_empty() {
                workspace_path = super::canonicalize_workspace_path(cwd);
            }
        }

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

        if obj_type == "ai-title" {
            if let Some(t) = val.get("aiTitle").and_then(|v| v.as_str()) {
                if !t.trim().is_empty() {
                    title = t.trim().to_string();
                }
            }
            continue;
        }

        if obj_type == "reasoning" {
            let mut thinking_parts = Vec::new();
            if let Some(arr) = val.get("rawContent").and_then(|v| v.as_array()) {
                for b in arr {
                    if let Some(t) = b.get("text").and_then(|v| v.as_str()) {
                        thinking_parts.push(t);
                    } else if let Some(t) = b.as_str() {
                        thinking_parts.push(t);
                    }
                }
            }
            let thinking_text = thinking_parts.join("\n\n").trim().to_string();
            if !thinking_text.is_empty() {
                messages.push(RawMessage {
                    step_index: step_idx,
                    role: "assistant".to_string(),
                    message_type: "workbuddy_reasoning".to_string(),
                    content: String::new(),
                    thinking: Some(thinking_text),
                    created_at: ts,
                    model_name: Some("WorkBuddy".to_string()),
                    tool_name: None,
                    tool_args: None,
                    duration_ms: None,
                    token_count: None,
                });
                step_idx += 1;
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

        if content.is_empty() {
            continue;
        }

        if (role == "user" || role == "human") && title.is_empty() {
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
            model_name: Some("WorkBuddy".to_string()),
            tool_name: None,
            tool_args: None,
            duration_ms: None,
            token_count: None,
        });
        step_idx += 1;
    }

    if messages.is_empty() {
        return Ok(None);
    }

    if title.is_empty() {
        title = format!("WorkBuddy 会话 {}", &cid[..cid.len().min(8)]);
    }

    Ok(Some(RawConversation {
        id: cid.to_string(),
        title,
        workspace_path,
        source_app: "workbuddy".to_string(),
        created_at,
        updated_at,
        parse_status: "ok".to_string(),
        source_types: vec!["workbuddy".to_string()],
        messages,
    }))
}
