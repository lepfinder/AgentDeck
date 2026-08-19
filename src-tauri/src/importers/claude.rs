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
        app: "Claude Code".to_string(),
        new_count: 0,
        updated_count: 0,
        skipped_count: 0,
        error_count: 0,
    };

    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return stats,
    };

    let claude_dir = home.join(".claude/projects");
    if !claude_dir.is_dir() {
        return stats;
    }

    for entry in WalkDir::new(&claude_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            if ext != "jsonl" {
                continue;
            }
        } else {
            continue;
        }

        let cid = format!(
            "claude:{}",
            path.file_stem().unwrap_or_default().to_string_lossy()
        );

        if incremental && !needs_sync(conn, path, true) {
            stats.skipped_count += 1;
            continue;
        }

        match parse_claude_jsonl(&cid, path) {
            Ok(Some(conv)) => match save_conversation_tx(conn, &conv) {
                Ok(is_new) => {
                    record_sync_state(conn, path, &cid, "claude_jsonl");
                    if is_new {
                        stats.new_count += 1;
                    } else {
                        stats.updated_count += 1;
                    }
                }
                Err(e) => {
                    eprintln!("[Claude Importer] 保存失败 {}: {}", cid, e);
                    stats.error_count += 1;
                }
            },
            Ok(None) => {
                stats.skipped_count += 1;
            }
            Err(e) => {
                eprintln!("[Claude Importer] 解析失败 {}: {}", cid, e);
                stats.error_count += 1;
            }
        }
    }

    stats
}

fn parse_claude_jsonl(
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

    // 工作区由项目目录名称推断
    if let Some(parent) = path.parent() {
        let parent_name = parent
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if parent_name.starts_with("Users-") {
            workspace_path = format!("/{}", parent_name.replace('-', "/"));
        } else {
            workspace_path = parent_name;
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

        let msg_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");
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

        match msg_type {
            "user" | "prompt" => {
                let mut content = String::new();
                if let Some(s) = val
                    .get("text")
                    .or_else(|| val.get("content"))
                    .or_else(|| val.get("prompt"))
                    .and_then(|v| v.as_str())
                {
                    content = s.trim().to_string();
                } else if let Some(arr) = val.get("content").and_then(|v| v.as_array()) {
                    for b in arr {
                        if let Some(t) = b.get("text").and_then(|v| v.as_str()) {
                            content.push_str(t);
                            content.push('\n');
                        }
                    }
                    content = content.trim().to_string();
                }

                if content.is_empty() {
                    continue;
                }

                if let Some(cwd) = val.get("cwd").and_then(|v| v.as_str()) {
                    if workspace_path.is_empty() {
                        workspace_path = super::project_root_from_path(cwd);
                    }
                }

                if title.is_empty() {
                    title = content.chars().take(60).collect();
                }

                messages.push(RawMessage {
                    step_index: step_idx,
                    role: "user".to_string(),
                    message_type: "text".to_string(),
                    content,
                    thinking: None,
                    created_at: ts,
                    model_name: None,
                    tool_name: None,
                    tool_args: None,
                    duration_ms: None,
                    token_count: None,
                    images: None,
                });
                step_idx += 1;
            }
            "assistant" | "completion" => {
                let mut text_parts = Vec::new();
                let mut thinking_parts = Vec::new();
                let mut tool_calls = Vec::new();

                if let Some(s) = val
                    .get("text")
                    .or_else(|| val.get("content"))
                    .and_then(|v| v.as_str())
                {
                    text_parts.push(s.to_string());
                } else if let Some(arr) = val.get("content").and_then(|v| v.as_array()) {
                    for b in arr {
                        let b_type = b.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        if b_type == "text" {
                            if let Some(t) = b.get("text").and_then(|v| v.as_str()) {
                                text_parts.push(t.to_string());
                            }
                        } else if b_type == "thinking" {
                            if let Some(th) = b.get("thinking").and_then(|v| v.as_str()) {
                                thinking_parts.push(th.to_string());
                            }
                        } else if b_type == "tool_use" {
                            tool_calls.push(b.clone());
                        }
                    }
                }

                let content_str = text_parts.join("\n\n").trim().to_string();
                let thinking_str = if thinking_parts.is_empty() {
                    None
                } else {
                    Some(thinking_parts.join("\n\n"))
                };
                let tool_args = if tool_calls.is_empty() {
                    None
                } else {
                    serde_json::to_string_pretty(&tool_calls).ok()
                };
                let tool_name = if tool_calls.len() == 1 {
                    tool_calls[0]
                        .get("name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                } else if !tool_calls.is_empty() {
                    Some("multiple_tools".to_string())
                } else {
                    None
                };

                if content_str.is_empty() && thinking_str.is_none() && tool_args.is_none() {
                    continue;
                }

                messages.push(RawMessage {
                    step_index: step_idx,
                    role: "assistant".to_string(),
                    message_type: if tool_args.is_some() {
                        "tool_call".to_string()
                    } else {
                        "text".to_string()
                    },
                    content: if content_str.is_empty() {
                        tool_args.clone().unwrap_or_default()
                    } else {
                        content_str
                    },
                    thinking: thinking_str,
                    created_at: ts,
                    model_name: Some("Claude".to_string()),
                    tool_name,
                    tool_args,
                    duration_ms: None,
                    token_count: None,
                    images: None,
                });
                step_idx += 1;
            }
            _ => {}
        }
    }

    if messages.is_empty() {
        return Ok(None);
    }

    if title.is_empty() {
        title = format!("Claude 会话 {}", &cid[..cid.len().min(8)]);
    }

    Ok(Some(RawConversation {
        id: cid.to_string(),
        title,
        workspace_path,
        source_app: "claude".to_string(),
        created_at,
        updated_at,
        parse_status: "ok".to_string(),
        source_types: vec!["claude".to_string()],
        messages,
    }))
}
