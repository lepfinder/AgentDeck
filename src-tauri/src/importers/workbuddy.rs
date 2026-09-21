use rusqlite::{params, Connection};
use serde_json::Value;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use walkdir::WalkDir;

use super::{
    needs_sync, record_sync_state, save_conversation_tx, ImporterStats, RawConversation, RawMessage,
    RawUsageRecord,
};

const WORKBUDDY_PARSER_REV: &str = "workbuddy-v2";
const WORKBUDDY_PARSER_REV_KEY: &str = "agentdeck:workbuddy_parser_rev";

fn workbuddy_parser_rev_stale(conn: &Connection) -> bool {
    let stored: Option<String> = conn
        .query_row(
            "SELECT conversation_id FROM sync_state WHERE source_path = ?",
            params![WORKBUDDY_PARSER_REV_KEY],
            |r| r.get(0),
        )
        .ok();
    stored.as_deref() != Some(WORKBUDDY_PARSER_REV)
}

fn mark_workbuddy_synced(conn: &Connection) {
    let now = chrono::Utc::now().to_rfc3339();
    let _ = conn.execute(
        r#"
        INSERT INTO sync_state (source_path, conversation_id, source_type, file_mtime, file_size, synced_at)
        VALUES (?1, ?2, 'workbuddy_parser', 0, 0, ?3)
        ON CONFLICT(source_path) DO UPDATE SET
            conversation_id = excluded.conversation_id,
            synced_at = excluded.synced_at
        "#,
        params![WORKBUDDY_PARSER_REV_KEY, WORKBUDDY_PARSER_REV, now],
    );
}

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

    let force_reparse = workbuddy_parser_rev_stale(conn);
    let incremental = incremental && !force_reparse;

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

    // 有错误的运行不标记 rev：否则一次不完整的运行会永久消耗强制重导
    if stats.error_count == 0 {
        mark_workbuddy_synced(conn);
    }
    stats
}

fn parse_workbuddy_timestamp(val: &Value) -> Option<String> {
    if let Some(v) = val.get("timestamp").or_else(|| val.get("created_at")) {
        if let Some(ms) = v.as_i64() {
            if let Some(dt) = chrono::DateTime::from_timestamp_millis(ms) {
                return Some(dt.to_rfc3339());
            }
        }
        if let Some(f) = v.as_f64() {
            if let Some(dt) = chrono::DateTime::from_timestamp_millis(f as i64) {
                return Some(dt.to_rfc3339());
            }
        }
        if let Some(s) = v.as_str() {
            if let Ok(ms) = s.parse::<i64>() {
                if let Some(dt) = chrono::DateTime::from_timestamp_millis(ms) {
                    return Some(dt.to_rfc3339());
                }
            }
            return Some(s.to_string());
        }
    }
    None
}

fn extract_text_from_content(val: &Value) -> String {
    if let Some(s) = val.get("content").and_then(|v| v.as_str()) {
        return s.trim().to_string();
    }
    if let Some(s) = val.get("text").and_then(|v| v.as_str()) {
        return s.trim().to_string();
    }
    if let Some(arr) = val.get("content").and_then(|v| v.as_array()) {
        let mut texts = Vec::new();
        for item in arr {
            if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                if !t.trim().is_empty() {
                    texts.push(t.trim());
                }
            } else if let Some(s) = item.as_str() {
                if !s.trim().is_empty() {
                    texts.push(s.trim());
                }
            }
        }
        if !texts.is_empty() {
            return texts.join("\n\n");
        }
    }
    if let Some(arr) = val.get("rawContent").and_then(|v| v.as_array()) {
        let mut texts = Vec::new();
        for item in arr {
            if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                if !t.trim().is_empty() {
                    texts.push(t.trim());
                }
            } else if let Some(s) = item.as_str() {
                if !s.trim().is_empty() {
                    texts.push(s.trim());
                }
            }
        }
        if !texts.is_empty() {
            return texts.join("\n\n");
        }
    }
    String::new()
}

fn extract_user_query(raw: &str) -> String {
    if raw.contains("<user_query>") && raw.contains("</user_query>") {
        let mut queries = Vec::new();
        let mut remaining = raw;
        while let Some(start) = remaining.find("<user_query>") {
            let after_start = &remaining[start + "<user_query>".len()..];
            if let Some(end) = after_start.find("</user_query>") {
                let q = after_start[..end].trim();
                if !q.is_empty() {
                    queries.push(q);
                }
                remaining = &after_start[end + "</user_query>".len()..];
            } else {
                break;
            }
        }
        if !queries.is_empty() {
            return queries.join("\n\n");
        }
    }

    // 若无 user_query 标签，剥离 <system-reminder> 块以防提示词污染
    let mut cleaned = raw.to_string();
    while let Some(s_start) = cleaned.find("<system-reminder") {
        if let Some(s_end) = cleaned[s_start..].find("</system-reminder>") {
            cleaned.replace_range(s_start..s_start + s_end + "</system-reminder>".len(), "");
        } else {
            break;
        }
    }
    let trimmed = cleaned.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }

    raw.trim().to_string()
}

fn parse_workbuddy_file(
    cid: &str,
    path: &Path,
) -> Result<Option<RawConversation>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let mut messages = Vec::new();
    let mut usage_records = Vec::new();
    let mut seen_usage_idents: HashSet<String> = HashSet::new();
    let mut title = String::new();
    let mut workspace_path = String::new();
    let mut created_at = None;
    let mut updated_at = None;
    let mut step_idx = 0i64;

    let mut pending_thinking: Vec<String> = Vec::new();
    let mut pending_tool_calls: Vec<Value> = Vec::new();
    let mut last_model_name: Option<String> = None;

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

        let ts = parse_workbuddy_timestamp(&val);
        if created_at.is_none() && ts.is_some() {
            created_at = ts.clone();
        }
        if ts.is_some() {
            updated_at = ts.clone();
        }

        // 提取模型名称
        if let Some(model) = val
            .get("providerData")
            .and_then(|p| p.get("model").or_else(|| p.get("requestModelName")))
            .and_then(|v| v.as_str())
        {
            if !model.trim().is_empty() {
                last_model_name = Some(model.trim().to_string());
            }
        }

        // providerData.usage 是 WorkBuddy 归一化后的单次请求用量
        if let Some(usage) = val
            .get("providerData")
            .and_then(|p| p.get("usage"))
            .filter(|u| u.get("inputTokens").is_some() || u.get("outputTokens").is_some())
        {
            let pd = val.get("providerData").unwrap_or(&Value::Null);
            let ident = pd
                .get("messageId")
                .and_then(|v| v.as_str())
                .or_else(|| val.get("id").and_then(|v| v.as_str()))
                .unwrap_or("")
                .to_string();
            if !ident.is_empty() && seen_usage_idents.insert(ident.clone()) {
                let input = usage.get("inputTokens").and_then(|v| v.as_i64());
                let output = usage.get("outputTokens").and_then(|v| v.as_i64());
                // inputTokensDetails 是数组，缓存读可能分散在多个桶里
                let cached: i64 = usage
                    .get("inputTokensDetails")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|d| d.get("cached_tokens").and_then(|v| v.as_i64()))
                            .sum()
                    })
                    .unwrap_or(0);
                let reasoning: Option<i64> = usage
                    .get("outputTokensDetails")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|d| d.get("reasoning_tokens").and_then(|v| v.as_i64()))
                            .sum()
                    });
                // inputTokens 含缓存命中，扣除后才是新鲜输入
                let fresh_input = input.map(|i| (i - cached).max(0));
                usage_records.push(RawUsageRecord {
                    identity: ident,
                    agent: "workbuddy".to_string(),
                    model: last_model_name.clone(),
                    input_tokens: fresh_input,
                    cache_read_tokens: if cached > 0 { Some(cached) } else { Some(0) },
                    cache_write_tokens: None,
                    output_tokens: output,
                    reasoning_tokens: reasoning,
                    credit: None,
                    occurred_at: ts.clone(),
                    is_partial: false,
                });
            }
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
                        if !t.trim().is_empty() {
                            thinking_parts.push(t.trim());
                        }
                    } else if let Some(t) = b.as_str() {
                        if !t.trim().is_empty() {
                            thinking_parts.push(t.trim());
                        }
                    }
                }
            }
            if !thinking_parts.is_empty() {
                pending_thinking.push(thinking_parts.join("\n\n"));
            }
            continue;
        }

        if obj_type == "function_call" {
            let name = val.get("name").and_then(|v| v.as_str()).unwrap_or("tool").to_string();
            let args = val.get("arguments").cloned().unwrap_or(Value::Null);
            pending_tool_calls.push(serde_json::json!({
                "name": name,
                "args": args
            }));
            continue;
        }

        if obj_type == "function_call_result" || obj_type == "file-history-snapshot" {
            continue;
        }

        // 处理 message
        let role = val
            .get("role")
            .or_else(|| val.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("user");

        let raw_content = extract_text_from_content(&val);

        if role == "user" || role == "human" {
            let content = extract_user_query(&raw_content);
            if content.is_empty() {
                continue;
            }

            // 如果此前有未收拢的 thinking 或 tool_calls，先封装为一条 assistant 消息
            if !pending_thinking.is_empty() || !pending_tool_calls.is_empty() {
                let thinking_str = if pending_thinking.is_empty() {
                    None
                } else {
                    Some(pending_thinking.join("\n\n"))
                };
                pending_thinking.clear();

                let tool_args = if pending_tool_calls.is_empty() {
                    None
                } else {
                    serde_json::to_string_pretty(&pending_tool_calls).ok()
                };
                let tool_name = if pending_tool_calls.len() == 1 {
                    pending_tool_calls[0]
                        .get("name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                } else if !pending_tool_calls.is_empty() {
                    Some(format!("{} tools", pending_tool_calls.len()))
                } else {
                    None
                };
                let is_tool = tool_args.is_some();
                pending_tool_calls.clear();

                messages.push(RawMessage {
                    step_index: step_idx,
                    role: "assistant".to_string(),
                    message_type: if is_tool {
                        "tool_call".to_string()
                    } else {
                        "text".to_string()
                    },
                    content: String::new(),
                    thinking: thinking_str,
                    created_at: ts.clone(),
                    model_name: last_model_name.clone().or_else(|| Some("WorkBuddy".to_string())),
                    tool_name,
                    tool_args,
                    duration_ms: None,
                    token_count: None,
                    credit: None,
                    images: None,
                });
                step_idx += 1;
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
        } else {
            // assistant
            let thinking_str = if pending_thinking.is_empty() {
                None
            } else {
                Some(pending_thinking.join("\n\n"))
            };
            pending_thinking.clear();

            let tool_args = if pending_tool_calls.is_empty() {
                None
            } else {
                serde_json::to_string_pretty(&pending_tool_calls).ok()
            };
            let tool_name = if pending_tool_calls.len() == 1 {
                pending_tool_calls[0]
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            } else if !pending_tool_calls.is_empty() {
                Some(format!("{} tools", pending_tool_calls.len()))
            } else {
                None
            };
            let is_tool = tool_args.is_some();
            pending_tool_calls.clear();

            let content_str = raw_content.trim().to_string();
            if content_str.is_empty() && thinking_str.is_none() && !is_tool {
                continue;
            }

            messages.push(RawMessage {
                step_index: step_idx,
                role: "assistant".to_string(),
                message_type: if is_tool {
                    "tool_call".to_string()
                } else {
                    "text".to_string()
                },
                content: content_str,
                thinking: thinking_str,
                created_at: ts,
                model_name: last_model_name.clone().or_else(|| Some("WorkBuddy".to_string())),
                tool_name,
                tool_args,
                duration_ms: None,
                token_count: None,
                credit: None,
                images: None,
            });
            step_idx += 1;
        }
    }

    // 循环结束：若末尾仍有残留的 thinking 或 tool_calls
    if !pending_thinking.is_empty() || !pending_tool_calls.is_empty() {
        let thinking_str = if pending_thinking.is_empty() {
            None
        } else {
            Some(pending_thinking.join("\n\n"))
        };
        let tool_args = if pending_tool_calls.is_empty() {
            None
        } else {
            serde_json::to_string_pretty(&pending_tool_calls).ok()
        };
        let tool_name = if pending_tool_calls.len() == 1 {
            pending_tool_calls[0]
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        } else if !pending_tool_calls.is_empty() {
            Some(format!("{} tools", pending_tool_calls.len()))
        } else {
            None
        };
        let is_tool = tool_args.is_some();

        messages.push(RawMessage {
            step_index: step_idx,
            role: "assistant".to_string(),
            message_type: if is_tool {
                "tool_call".to_string()
            } else {
                "text".to_string()
            },
            content: String::new(),
            thinking: thinking_str,
            created_at: updated_at.clone(),
            model_name: last_model_name.or_else(|| Some("WorkBuddy".to_string())),
            tool_name,
            tool_args,
            duration_ms: None,
            token_count: None,
            credit: None,
            images: None,
        });
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
        artifacts: Vec::new(),
        usage_records,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_parse_workbuddy_file() {
        let tmp_path = std::env::temp_dir().join(format!("test_workbuddy_{}.jsonl", std::process::id()));
        let mut tmp = File::create(&tmp_path).unwrap();
        writeln!(tmp, r#"{{"type":"ai-title","aiTitle":"Excel 转网页方便打印"}}"#).unwrap();
        writeln!(
            tmp,
            r#"{{"type":"message","role":"user","timestamp":1787326330000,"content":[{{"type":"input_text","text":"<system-reminder>sys prompt</system-reminder><user_query>帮我把这个 Excel 处理下，做成一个网页</user_query>"}}]}}"#
        ).unwrap();
        writeln!(
            tmp,
            r#"{{"type":"reasoning","rawContent":[{{"text":"需要使用相关脚本处理表格"}}]}}"#
        ).unwrap();
        writeln!(
            tmp,
            r#"{{"type":"function_call","name":"Read","arguments":"{{\"file\":\"test.xlsx\"}}"}}"#
        ).unwrap();
        writeln!(
            tmp,
            r#"{{"type":"message","role":"assistant","timestamp":1787326340000,"content":[{{"type":"output_text","text":"搞定了，已生成网页。"}}]}}"#
        ).unwrap();
        drop(tmp);

        let conv = parse_workbuddy_file("workbuddy:test-123", &tmp_path)
            .unwrap()
            .unwrap();
        let _ = std::fs::remove_file(&tmp_path);
        assert_eq!(conv.title, "Excel 转网页方便打印");
        assert_eq!(conv.messages.len(), 2);

        // 验证用户消息
        let u_msg = &conv.messages[0];
        assert_eq!(u_msg.role, "user");
        assert_eq!(u_msg.content, "帮我把这个 Excel 处理下，做成一个网页");
        assert!(u_msg.created_at.is_some());

        // 验证助手消息
        let a_msg = &conv.messages[1];
        assert_eq!(a_msg.role, "assistant");
        assert_eq!(a_msg.content, "搞定了，已生成网页。");
        assert!(a_msg
            .thinking
            .as_ref()
            .unwrap()
            .contains("需要使用相关脚本处理表格"));
        assert!(a_msg.tool_args.as_ref().unwrap().contains("test.xlsx"));
    }

    #[test]
    fn test_parse_workbuddy_usage_records() {
        let tmp_path = std::env::temp_dir().join(format!(
            "test_workbuddy_usage_{}.jsonl",
            std::process::id()
        ));
        let mut tmp = File::create(&tmp_path).unwrap();
        writeln!(
            tmp,
            r#"{{"type":"message","role":"user","timestamp":1786604171145,"content":[{{"type":"input_text","text":"hi"}}]}}"#
        )
        .unwrap();
        writeln!(
            tmp,
            r#"{{"id":"rec-1","type":"message","role":"assistant","timestamp":1786604180000,"content":[{{"type":"output_text","text":"ok"}}],"providerData":{{"messageId":"msg-abc","model":"kimi-k3-1","usage":{{"requests":1,"inputTokens":30642,"outputTokens":155,"totalTokens":30797,"inputTokensDetails":[{{"cached_tokens":9472}}],"outputTokensDetails":[{{"reasoning_tokens":42}}]}}}}}}"#
        )
        .unwrap();
        // 同 messageId 重放，只记首次
        writeln!(
            tmp,
            r#"{{"id":"rec-2","type":"message","role":"assistant","timestamp":1786604190000,"content":[{{"type":"output_text","text":"again"}}],"providerData":{{"messageId":"msg-abc","model":"kimi-k3-1","usage":{{"requests":1,"inputTokens":999,"outputTokens":1,"totalTokens":1000}}}}}}"#
        )
        .unwrap();
        drop(tmp);

        let conv = parse_workbuddy_file("workbuddy:usage-test", &tmp_path)
            .unwrap()
            .unwrap();
        let _ = std::fs::remove_file(&tmp_path);

        assert_eq!(conv.usage_records.len(), 1);
        let u = &conv.usage_records[0];
        assert_eq!(u.identity, "msg-abc");
        assert_eq!(u.model.as_deref(), Some("kimi-k3-1"));
        // 30642 - 9472(缓存读) = 21170 新鲜输入
        assert_eq!(u.input_tokens, Some(21170));
        assert_eq!(u.cache_read_tokens, Some(9472));
        assert_eq!(u.output_tokens, Some(155));
        assert_eq!(u.reasoning_tokens, Some(42));
        assert!(u.occurred_at.as_deref().unwrap().starts_with("2026-"));
    }

    /// 拷贝真实 DB 到临时副本后全量同步；绝不直写真实库，
    /// 避免与运行中的 dev 应用抢写锁、消耗 parser rev
    #[test]
    fn test_sync_real_workbuddy_db() {
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => return,
        };
        let real = home.join(".agentdeck/agentdeck.db");
        if !real.is_file() {
            return;
        }
        let copy =
            std::env::temp_dir().join(format!("agentdeck_wb_test_{}.db", std::process::id()));
        std::fs::copy(&real, &copy).unwrap();
        let conn = Connection::open(&copy).unwrap();
        let stats = sync(&conn, false);
        eprintln!("WorkBuddy sync stats: {:?}", stats);
        assert!(stats.error_count == 0, "Errors during sync: {}", stats.error_count);
        drop(conn);
        let _ = std::fs::remove_file(&copy);
    }
}

