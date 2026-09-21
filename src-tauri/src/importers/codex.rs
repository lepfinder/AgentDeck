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

const CODEX_PARSER_REV: &str = "codex-v3";
const CODEX_PARSER_REV_KEY: &str = "agentdeck:codex_parser_rev";

fn codex_parser_rev_stale(conn: &Connection) -> bool {
    let stored: Option<String> = conn
        .query_row(
            "SELECT conversation_id FROM sync_state WHERE source_path = ?",
            params![CODEX_PARSER_REV_KEY],
            |r| r.get(0),
        )
        .ok();
    stored.as_deref() != Some(CODEX_PARSER_REV)
}

fn mark_codex_synced(conn: &Connection) {
    let now = chrono::Utc::now().to_rfc3339();
    let _ = conn.execute(
        r#"
        INSERT INTO sync_state (source_path, conversation_id, source_type, file_mtime, file_size, synced_at)
        VALUES (?1, ?2, 'codex_parser', 0, 0, ?3)
        ON CONFLICT(source_path) DO UPDATE SET
            conversation_id = excluded.conversation_id,
            synced_at = excluded.synced_at
        "#,
        params![CODEX_PARSER_REV_KEY, CODEX_PARSER_REV, now],
    );
}

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

    let force_reparse = codex_parser_rev_stale(conn);
    let incremental = incremental && !force_reparse;

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

    // 有错误的运行不标记 rev：否则一次不完整的运行会永久消耗强制重导
    if stats.error_count == 0 {
        mark_codex_synced(conn);
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
    let mut usage_records = Vec::new();
    // 同一 response_id 理论上只出现一次，重放时只记首次用量避免双计
    let mut seen_usage_idents: HashSet<String> = HashSet::new();
    let mut current_model: Option<String> = None;
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

        if obj_type == "turn_context" {
            if let Some(model) = val
                .get("payload")
                .and_then(|p| p.get("model"))
                .and_then(|v| v.as_str())
            {
                if !model.trim().is_empty() {
                    current_model = Some(model.trim().to_string());
                }
            }
            continue;
        }

        if obj_type == "token_usage_record" {
            // payload.usage 是单次请求用量；turn/thread_token_usage 才是累计值，不可用
            if let Some(usage) = val
                .get("payload")
                .and_then(|p| p.get("usage"))
                .and_then(|u| u.as_object())
            {
                let ident = val
                    .get("payload")
                    .and_then(|p| p.get("response_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if ident.is_empty() || seen_usage_idents.insert(ident.clone()) {
                    let input = usage.get("input_tokens").and_then(|v| v.as_i64());
                    let cached = usage.get("cached_input_tokens").and_then(|v| v.as_i64());
                    let cache_write = usage.get("cache_write_input_tokens").and_then(|v| v.as_i64());
                    let output = usage.get("output_tokens").and_then(|v| v.as_i64());
                    let reasoning = usage.get("reasoning_output_tokens").and_then(|v| v.as_i64());
                    // Codex 的 input_tokens 含缓存命中与写入，扣除后才是新鲜输入
                    let fresh_input = input.map(|i| {
                        (i - cached.unwrap_or(0) - cache_write.unwrap_or(0)).max(0)
                    });
                    if fresh_input.is_some()
                        || output.is_some()
                        || cached.is_some()
                        || cache_write.is_some()
                    {
                        usage_records.push(RawUsageRecord {
                            identity: ident,
                            agent: "codex".to_string(),
                            model: current_model.clone(),
                            input_tokens: fresh_input,
                            cache_read_tokens: cached,
                            cache_write_tokens: cache_write,
                            output_tokens: output,
                            reasoning_tokens: reasoning,
                            credit: None,
                            occurred_at: ts.clone(),
                            is_partial: false,
                        });
                    }
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
                        credit: None,
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
                        credit: None,
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
                credit: None,
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
        artifacts: Vec::new(),
        usage_records,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_fixture(name: &str, lines: &[&str]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "codex_test_{}_{}.jsonl",
            name,
            std::process::id()
        ));
        std::fs::write(&path, lines.join("\n")).unwrap();
        path
    }

    #[test]
    fn parses_usage_records_with_cache_subtraction() {
        let lines = [
            r#"{"timestamp":"2026-09-18T12:00:00.000Z","type":"session_meta","payload":{"cwd":"/tmp/demo"}}"#,
            r#"{"timestamp":"2026-09-18T12:00:01.000Z","type":"turn_context","payload":{"turn_id":"t1","model":"glm-5.3"}}"#,
            r#"{"timestamp":"2026-09-18T12:00:02.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"text":"hi"}]}}"#,
            r#"{"timestamp":"2026-09-18T12:00:03.000Z","type":"token_usage_record","payload":{"response_id":"resp_a","usage":{"input_tokens":18987,"cached_input_tokens":13184,"cache_write_input_tokens":0,"output_tokens":207,"reasoning_output_tokens":54,"total_tokens":19194}}}"#,
            // 同一 response_id 重放，只记首次
            r#"{"timestamp":"2026-09-18T12:00:04.000Z","type":"token_usage_record","payload":{"response_id":"resp_a","usage":{"input_tokens":99999,"cached_input_tokens":0,"cache_write_input_tokens":0,"output_tokens":1,"reasoning_output_tokens":0,"total_tokens":100000}}}"#,
            r#"{"timestamp":"2026-09-18T12:00:05.000Z","type":"token_usage_record","payload":{"response_id":"resp_b","usage":{"input_tokens":20007,"cached_input_tokens":0,"cache_write_input_tokens":500,"output_tokens":231,"reasoning_output_tokens":45,"total_tokens":20238}}}"#,
        ];
        let path = write_fixture("usage", &lines);
        let conv = parse_codex_file("codex:test", &path).unwrap().unwrap();

        assert_eq!(conv.messages.len(), 1);
        assert_eq!(conv.usage_records.len(), 2);

        let a = &conv.usage_records[0];
        assert_eq!(a.identity, "resp_a");
        assert_eq!(a.model.as_deref(), Some("glm-5.3"));
        // 18987 - 13184(缓存读) = 5803 新鲜输入
        assert_eq!(a.input_tokens, Some(5803));
        assert_eq!(a.cache_read_tokens, Some(13184));
        assert_eq!(a.cache_write_tokens, Some(0));
        assert_eq!(a.output_tokens, Some(207));
        assert_eq!(a.reasoning_tokens, Some(54));
        assert!(!a.is_partial);

        let b = &conv.usage_records[1];
        assert_eq!(b.identity, "resp_b");
        // 20007 - 0 - 500(缓存写) = 19507
        assert_eq!(b.input_tokens, Some(19507));
        assert_eq!(b.cache_write_tokens, Some(500));
        assert_eq!(b.output_tokens, Some(231));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn empty_session_returns_none() {
        let path = write_fixture("empty", &[r#"{"timestamp":"2026-09-18T12:00:00.000Z","type":"session_meta","payload":{"cwd":"/tmp/demo"}}"#]);
        assert!(parse_codex_file("codex:empty", &path).unwrap().is_none());
        std::fs::remove_file(&path).ok();
    }

    /// 对本机真实 ~/.codex/sessions 做一次解析试跑
    #[test]
    fn test_parse_real_codex_sessions() {
        let home = dirs::home_dir().unwrap();
        let dir = home.join(".codex/sessions");
        assert!(dir.is_dir(), "no codex sessions dir");

        let mut total_usage = 0usize;
        let mut convs_with_usage = 0usize;
        for entry in WalkDir::new(&dir).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            if !p.is_file() || p.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                continue;
            }
            let stem = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
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
            if let Ok(Some(conv)) = parse_codex_file(&format!("codex:{}", session_id), p) {
                if !conv.usage_records.is_empty() {
                    convs_with_usage += 1;
                }
                total_usage += conv.usage_records.len();
            }
        }
        println!(
            "real codex: {} usage records across {} conversations",
            total_usage, convs_with_usage
        );
        assert!(total_usage > 0, "expected usage records from real codex data");
    }
}
