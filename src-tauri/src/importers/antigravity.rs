use rusqlite::Connection;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use regex::Regex;
use serde_json::Value;

use super::{needs_sync, record_sync_state, save_conversation_tx, ImporterStats, RawConversation, RawMessage};

pub fn sync(conn: &Connection, incremental: bool) -> ImporterStats {
    let mut stats = ImporterStats {
        app: "Antigravity".to_string(),
        new_count: 0,
        updated_count: 0,
        skipped_count: 0,
        error_count: 0,
    };

    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return stats,
    };

    let brain_dir = home.join(".gemini/antigravity-ide/brain");
    if !brain_dir.is_dir() {
        return stats;
    }

    let entries = match std::fs::read_dir(&brain_dir) {
        Ok(e) => e,
        Err(_) => return stats,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let cid = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) if !name.starts_with('.') => name.to_string(),
            _ => continue,
        };

        let transcript_path = path.join(".system_generated/logs/transcript.jsonl");
        if !transcript_path.is_file() {
            continue;
        }

        if incremental && !needs_sync(conn, &transcript_path, true) {
            stats.skipped_count += 1;
            continue;
        }

        match parse_antigravity_session(&cid, &transcript_path, &path) {
            Ok(Some(conv)) => {
                match save_conversation_tx(conn, &conv) {
                    Ok(is_new) => {
                        record_sync_state(conn, &transcript_path, &cid, "antigravity_jsonl");
                        if is_new {
                            stats.new_count += 1;
                        } else {
                            stats.updated_count += 1;
                        }
                    }
                    Err(e) => {
                        eprintln!("[Antigravity Importer] 保存失败 {}: {}", cid, e);
                        stats.error_count += 1;
                    }
                }
            }
            Ok(None) => {
                stats.skipped_count += 1;
            }
            Err(e) => {
                eprintln!("[Antigravity Importer] 解析失败 {}: {}", cid, e);
                stats.error_count += 1;
            }
        }
    }

    stats
}

fn parse_antigravity_session(cid: &str, transcript_path: &Path, session_dir: &Path) -> Result<Option<RawConversation>, Box<dyn std::error::Error>> {
    let file = File::open(transcript_path)?;
    let reader = BufReader::new(file);

    let user_req_re = Regex::new(r"(?s)<USER_REQUEST>\s*(.*?)\s*</USER_REQUEST>")?;
    let workspace_re = Regex::new(r"(/[^\s\n\r]+)\s*->")?;
    let active_doc_re = Regex::new(r"Active Document:\s*([^\s(]+)")?;

    let mut messages = Vec::new();
    let mut title = String::new();
    let mut workspace_path = String::new();
    let mut created_at = None;
    let mut updated_at = None;
    let mut step_idx = 0i64;

    // 优先读取 overview.txt 提取标题
    let overview_path = session_dir.join("overview.txt");
    if overview_path.is_file() {
        if let Ok(content) = std::fs::read_to_string(&overview_path) {
            let first_line = content.lines().next().unwrap_or("").trim();
            if !first_line.is_empty() {
                title = first_line.to_string();
            }
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

        let json_val: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let step_type = json_val.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let raw_content = json_val.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let timestamp = json_val.get("created_at")
            .or_else(|| json_val.get("timestamp"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if created_at.is_none() && timestamp.is_some() {
            created_at = timestamp.clone();
        }
        if timestamp.is_some() {
            updated_at = timestamp.clone();
        }

        // 推断工作区路径并归一化
        if workspace_path.is_empty() {
            if let Some(caps) = workspace_re.captures(raw_content) {
                if let Some(m) = caps.get(1) {
                    workspace_path = super::project_root_from_path(m.as_str());
                }
            } else if let Some(caps) = active_doc_re.captures(raw_content) {
                if let Some(m) = caps.get(1) {
                    workspace_path = super::project_root_from_path(m.as_str());
                }
            } else if raw_content.contains("/workspace/") {
                for line in raw_content.lines() {
                    if let Some(idx) = line.find("/workspace/") {
                        let sub = &line[idx..];
                        let cand = super::project_root_from_path(sub.split_whitespace().next().unwrap_or(""));
                        if !cand.is_empty() {
                            workspace_path = cand;
                            break;
                        }
                    }
                }
            }
        }

        match step_type {
            "USER_INPUT" => {
                let user_text = if let Some(caps) = user_req_re.captures(raw_content) {
                    caps.get(1).map(|m| m.as_str().trim().to_string()).unwrap_or_else(|| raw_content.to_string())
                } else {
                    raw_content.trim().to_string()
                };

                if title.is_empty() && !user_text.is_empty() {
                    title = user_text.chars().take(80).collect();
                }

                messages.push(RawMessage {
                    step_index: step_idx,
                    role: "user".to_string(),
                    message_type: "text".to_string(),
                    content: user_text,
                    thinking: None,
                    created_at: timestamp,
                    model_name: None,
                    tool_name: None,
                    tool_args: None,
                    duration_ms: None,
                    token_count: None,
                });
                step_idx += 1;
            }
            "PLANNER_RESPONSE" => {
                let tool_calls_json = json_val.get("tool_calls").map(|v| v.to_string());
                let thinking = json_val.get("thinking").and_then(|v| v.as_str()).map(|s| s.to_string());

                messages.push(RawMessage {
                    step_index: step_idx,
                    role: "assistant".to_string(),
                    message_type: if tool_calls_json.is_some() { "tool_call".to_string() } else { "text".to_string() },
                    content: raw_content.to_string(),
                    thinking,
                    created_at: timestamp,
                    model_name: Some("Gemini".to_string()),
                    tool_name: None,
                    tool_args: tool_calls_json,
                    duration_ms: None,
                    token_count: None,
                });
                step_idx += 1;
            }
            _ => {}
        }
    }

    if messages.is_empty() {
        return Ok(None);
    }

    // 兜底时间戳
    if created_at.is_none() || updated_at.is_none() {
        if let Ok(meta) = transcript_path.metadata() {
            if let Ok(modified) = meta.modified() {
                let dt: chrono::DateTime<chrono::Utc> = modified.into();
                let iso = dt.to_rfc3339();
                if created_at.is_none() {
                    created_at = Some(iso.clone());
                }
                if updated_at.is_none() {
                    updated_at = Some(iso);
                }
            }
        }
    }

    if title.is_empty() {
        title = format!("Antigravity 会话 {}", &cid[..cid.len().min(8)]);
    }

    Ok(Some(RawConversation {
        id: cid.to_string(),
        title,
        workspace_path,
        source_app: "antigravity".to_string(),
        created_at,
        updated_at,
        parse_status: "ok".to_string(),
        source_types: vec!["antigravity".to_string()],
        messages,
    }))
}
