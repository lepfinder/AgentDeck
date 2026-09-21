use rusqlite::{params, Connection};
use serde_json::Value;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use walkdir::WalkDir;

use super::{
    needs_sync, record_sync_state, save_conversation_tx, ImporterStats, RawConversation,
    RawMessage, RawUsageRecord,
};

/// 解析格式版本：变更后强制重新扫描
const CLAUDE_PARSER_REV: &str = "claude-v1";
const CLAUDE_PARSER_REV_KEY: &str = "agentdeck:claude_parser_rev";

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

    let force_reparse = claude_parser_rev_stale(conn);
    let incremental = incremental && !force_reparse;

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

    // 有错误的运行不标记 rev：否则一次不完整的运行会永久消耗强制重导
    if stats.error_count == 0 {
        mark_claude_synced(conn);
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
    let mut usage_records = Vec::new();
    // 同一 message.id 可能因会话恢复被重放，只记首次用量，避免双计
    let mut seen_usage_idents: HashSet<String> = HashSet::new();
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
                // 现行格式正文在 message.content（字符串或块数组）；兼容旧版顶层字段
                let msg_content = val.get("message").and_then(|m| m.get("content"));
                let mut content = String::new();
                if let Some(s) = msg_content.and_then(|v| v.as_str()) {
                    content = s.trim().to_string();
                } else if let Some(s) = val
                    .get("text")
                    .or_else(|| val.get("prompt"))
                    .and_then(|v| v.as_str())
                {
                    content = s.trim().to_string();
                } else {
                    let arr = msg_content
                        .and_then(|v| v.as_array())
                        .or_else(|| val.get("content").and_then(|v| v.as_array()));
                    if let Some(arr) = arr {
                        for b in arr {
                            if let Some(t) = b.get("text").and_then(|v| v.as_str()) {
                                content.push_str(t);
                                content.push('\n');
                            }
                        }
                        content = content.trim().to_string();
                    }
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
                    credit: None,
                    images: None,
                });
                step_idx += 1;
            }
            "assistant" | "completion" => {
                let msg = val.get("message");
                let mut text_parts = Vec::new();
                let mut thinking_parts = Vec::new();
                let mut tool_calls = Vec::new();

                // 现行格式内容在 message.content 块数组；兼容旧版顶层 text/content
                if let Some(arr) = msg
                    .and_then(|m| m.get("content"))
                    .and_then(|v| v.as_array())
                    .or_else(|| val.get("content").and_then(|v| v.as_array()))
                {
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
                } else if let Some(s) = val
                    .get("text")
                    .or_else(|| val.get("content"))
                    .and_then(|v| v.as_str())
                {
                    text_parts.push(s.to_string());
                }

                let model_name = msg
                    .and_then(|m| m.get("model"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "Claude".to_string());

                // usage 附着在 assistant 消息（一次 API 响应）上：4 桶齐全，input 为新鲜值。
                // 提取放在空消息跳过之前——usage 是独立事实，不依赖消息是否入库。
                let msg_id = msg
                    .and_then(|m| m.get("id"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
                if let (Some(identity), Some(usage)) =
                    (&msg_id, msg.and_then(|m| m.get("usage")))
                {
                    if seen_usage_idents.insert(identity.clone()) {
                        let input = usage.get("input_tokens").and_then(|v| v.as_i64());
                        let cache_write =
                            usage.get("cache_creation_input_tokens").and_then(|v| v.as_i64());
                        let cache_read =
                            usage.get("cache_read_input_tokens").and_then(|v| v.as_i64());
                        let output = usage.get("output_tokens").and_then(|v| v.as_i64());
                        if input.is_some()
                            || cache_write.is_some()
                            || cache_read.is_some()
                            || output.is_some()
                        {
                            usage_records.push(RawUsageRecord {
                                identity: identity.clone(),
                                agent: "claude".to_string(),
                                model: Some(model_name.clone()),
                                input_tokens: input,
                                cache_read_tokens: cache_read,
                                cache_write_tokens: cache_write,
                                output_tokens: output,
                                reasoning_tokens: None,
                                credit: None,
                                occurred_at: ts.clone(),
                                is_partial: false,
                            });
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
                    model_name: Some(model_name),
                    tool_name,
                    tool_args,
                    duration_ms: None,
                    token_count: None,
                    credit: None,
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
        artifacts: Vec::new(),
        usage_records,
    }))
}

fn claude_parser_rev_stale(conn: &Connection) -> bool {
    let stored: Option<String> = conn
        .query_row(
            "SELECT conversation_id FROM sync_state WHERE source_path = ?",
            params![CLAUDE_PARSER_REV_KEY],
            |r| r.get(0),
        )
        .ok();
    stored.as_deref() != Some(CLAUDE_PARSER_REV)
}

fn mark_claude_synced(conn: &Connection) {
    let now = chrono::Utc::now().to_rfc3339();
    let _ = conn.execute(
        r#"
        INSERT INTO sync_state (source_path, conversation_id, source_type, file_mtime, file_size, synced_at)
        VALUES (?1, ?2, 'claude_parser', 0, 0, ?3)
        ON CONFLICT(source_path) DO UPDATE SET
            conversation_id = excluded.conversation_id,
            synced_at = excluded.synced_at
        "#,
        params![CLAUDE_PARSER_REV_KEY, CLAUDE_PARSER_REV, now],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_fixture(name: &str, lines: &[String]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("agentdeck_claude_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{}-{}.jsonl", name, std::process::id()));
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        path
    }

    #[test]
    fn parses_message_wrapped_lines_and_usage() {
        let lines = vec![
            r#"{"type":"user","timestamp":"2026-09-20T10:00:00.000Z","cwd":"/Users/xiyangxie/workspace/personal/Tappy","message":{"role":"user","content":"帮我修个 bug"}}"#.to_string(),
            r#"{"type":"assistant","timestamp":"2026-09-20T10:00:05.000Z","message":{"id":"msg_01","model":"glm-5.2","content":[{"type":"text","text":"好的，我来看。"}],"usage":{"input_tokens":68767,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":124}}}"#.to_string(),
            r#"{"type":"assistant","timestamp":"2026-09-20T10:00:10.000Z","message":{"id":"msg_01","model":"glm-5.2","content":[{"type":"text","text":"流式重放"}],"usage":{"input_tokens":99999,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":9}}}"#.to_string(),
            r#"{"type":"assistant","timestamp":"2026-09-20T10:00:15.000Z","message":{"id":"msg_02","model":"glm-5.2","content":[{"type":"tool_use","id":"tu_1","name":"Bash","input":{"command":"ls"}}],"usage":{"input_tokens":70000,"cache_creation_input_tokens":512,"cache_read_input_tokens":1024,"output_tokens":50}}}"#.to_string(),
        ];
        let path = write_fixture("wrapped", &lines);
        let conv = parse_claude_jsonl("claude:test", &path).unwrap().unwrap();

        // 1 user + 3 assistant（重放行内容不同仍按消息保留）
        assert_eq!(conv.messages.len(), 4);
        assert_eq!(conv.messages[0].role, "user");
        assert_eq!(conv.messages[0].content, "帮我修个 bug");
        assert_eq!(conv.messages[1].model_name.as_deref(), Some("glm-5.2"));
        assert_eq!(conv.messages[1].content, "好的，我来看。");
        assert_eq!(conv.messages[3].message_type, "tool_call");
        assert_eq!(conv.messages[3].tool_name.as_deref(), Some("Bash"));

        // usage：msg_01 首次出现计一次（重放行被身份去重），msg_02 正常
        assert_eq!(conv.usage_records.len(), 2);
        let r1 = &conv.usage_records[0];
        assert_eq!(r1.identity, "msg_01");
        assert_eq!(r1.input_tokens, Some(68767));
        assert_eq!(r1.output_tokens, Some(124));
        assert_eq!(r1.model.as_deref(), Some("glm-5.2"));
        assert_eq!(r1.occurred_at.as_deref(), Some("2026-09-20T10:00:05.000Z"));
        let r2 = &conv.usage_records[1];
        assert_eq!(r2.identity, "msg_02");
        assert_eq!(r2.cache_write_tokens, Some(512));
        assert_eq!(r2.cache_read_tokens, Some(1024));
    }

    #[test]
    fn parses_legacy_flat_lines() {
        let lines = vec![
            r#"{"type":"user","timestamp":"2026-09-20T10:00:00.000Z","text":"旧版顶层文本"}"#.to_string(),
            r#"{"type":"completion","timestamp":"2026-09-20T10:00:05.000Z","text":"旧版回复"}"#.to_string(),
        ];
        let path = write_fixture("legacy", &lines);
        let conv = parse_claude_jsonl("claude:legacy", &path).unwrap().unwrap();
        assert_eq!(conv.messages.len(), 2);
        assert_eq!(conv.messages[1].content, "旧版回复");
        assert!(conv.usage_records.is_empty());
    }
}
