use rusqlite::{params, Connection};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use walkdir::WalkDir;

use super::{
    needs_sync, normalize_to_iso, record_sync_state, save_conversation_tx, ImporterStats,
    RawConversation, RawMessage, RawUsageRecord,
};

/// 解析格式版本：变更后强制重新扫描
const QODER_PARSER_REV: &str = "qoder-v2";
const QODER_PARSER_REV_KEY: &str = "agentdeck:qoder_parser_rev";

/// 单条消息正文的截断上限，防止超大 tool 结果拖垮 UI
const CONTENT_TRUNCATE: usize = 4000;

/// 会话标题取首条用户输入的截断长度
const TITLE_MAX_CHARS: usize = 60;

pub fn sync(conn: &Connection, incremental: bool) -> ImporterStats {
    let mut stats = ImporterStats {
        app: "Qoder".to_string(),
        new_count: 0,
        updated_count: 0,
        skipped_count: 0,
        error_count: 0,
    };

    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return stats,
    };

    let projects_dir = home.join(".qoder/projects");
    if !projects_dir.is_dir() {
        return stats;
    }

    let force_reparse = qoder_parser_rev_stale(conn);
    let incremental = incremental && !force_reparse;

    for entry in WalkDir::new(&projects_dir)
        .max_depth(2)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        let session_id = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };
        let cid = format!("qoder:{}", session_id);

        if incremental && !needs_sync(conn, path, true) {
            stats.skipped_count += 1;
            continue;
        }

        match parse_qoder_jsonl(&cid, &session_id, path) {
            Ok(Some(conv)) => match save_conversation_tx(conn, &conv) {
                Ok(is_new) => {
                    record_sync_state(conn, path, &cid, "qoder_jsonl");
                    if is_new {
                        stats.new_count += 1;
                    } else {
                        stats.updated_count += 1;
                    }
                }
                Err(e) => {
                    eprintln!("[Qoder Importer] 保存失败 {}: {}", cid, e);
                    stats.error_count += 1;
                }
            },
            Ok(None) => {
                // 尚无消息的空会话：记录同步状态，避免每轮重复解析
                record_sync_state(conn, path, &cid, "qoder_jsonl");
                stats.skipped_count += 1;
            }
            Err(e) => {
                eprintln!("[Qoder Importer] 解析失败 {}: {}", cid, e);
                stats.error_count += 1;
            }
        }
    }

    // 有错误的运行不标记 rev：否则一次不完整的运行会永久消耗强制重导
    if stats.error_count == 0 {
        mark_qoder_synced(conn);
    }
    stats
}

/// 单次推理内的工具调用
struct ToolUse {
    id: String,
    name: String,
    input: String,
}

/// 一段连续的 assistant 输出（同一 message.id 的多行流式分片合并为一个回合）
#[derive(Default)]
struct Segment {
    message_id: String,
    model: Option<String>,
    ts: Option<String>,
    thinking: Vec<String>,
    text: Vec<String>,
    tools: Vec<ToolUse>,
    credits: Option<f64>,
    /// 本回合的 usage 快照（流式多行时后行覆盖前行，取最终值）
    usage: Option<Value>,
}

impl Segment {
    fn is_empty(&self) -> bool {
        self.thinking.is_empty() && self.text.is_empty() && self.tools.is_empty()
    }
}

/// 将累计的 assistant 回合展开为消息：正文消息在前，工具调用逐条展开，积分挂到末条。
/// usage 是独立事实，先于空段判断发射——纯 usage 行（无正文无工具）也要记账。
fn flush_segment(
    seg: &mut Segment,
    messages: &mut Vec<RawMessage>,
    usage_records: &mut Vec<RawUsageRecord>,
    step_idx: &mut i64,
    tool_names: &mut HashMap<String, String>,
) {
    if let Some(usage) = seg.usage.take() {
        let input = usage.get("input_tokens").and_then(|v| v.as_i64());
        let cache_write = usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_i64());
        let cache_read = usage
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_i64());
        let output = usage.get("output_tokens").and_then(|v| v.as_i64());
        let credit = usage
            .get("credits")
            .and_then(|v| v.as_f64())
            .filter(|c| *c > 0.0);
        if input.is_some()
            || cache_write.is_some()
            || cache_read.is_some()
            || output.is_some()
            || credit.is_some()
        {
            let identity = usage
                .get("request_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .or_else(|| {
                    if seg.message_id.is_empty() {
                        None
                    } else {
                        Some(seg.message_id.clone())
                    }
                })
                .unwrap_or_default();
            if !identity.is_empty() {
                usage_records.push(RawUsageRecord {
                    identity,
                    agent: "qoder".to_string(),
                    model: seg.model.clone(),
                    input_tokens: input,
                    cache_read_tokens: cache_read,
                    cache_write_tokens: cache_write,
                    output_tokens: output,
                    reasoning_tokens: None,
                    credit,
                    occurred_at: seg.ts.clone(),
                    is_partial: false,
                });
            }
        }
    }

    if seg.is_empty() {
        *seg = Segment::default();
        return;
    }

    let thinking = if seg.thinking.is_empty() {
        None
    } else {
        Some(truncate(&seg.thinking.join("\n\n"), CONTENT_TRUNCATE))
    };
    let text = seg.text.join("\n\n").trim().to_string();
    let mut thinking_used = thinking.is_none();

    if !text.is_empty() {
        messages.push(RawMessage {
            step_index: *step_idx,
            role: "assistant".to_string(),
            message_type: "text".to_string(),
            content: truncate(&text, CONTENT_TRUNCATE),
            thinking: thinking.clone(),
            created_at: seg.ts.clone(),
            model_name: seg.model.clone(),
            tool_name: None,
            tool_args: None,
            duration_ms: None,
            token_count: None,
            credit: None,
            images: None,
        });
        *step_idx += 1;
        thinking_used = true;
    }

    for tool in seg.tools.drain(..) {
        let tool_thinking = if !thinking_used {
            thinking_used = true;
            thinking.clone()
        } else {
            None
        };
        let input_json = truncate(&tool.input, CONTENT_TRUNCATE);
        tool_names.insert(tool.id.clone(), tool.name.clone());
        messages.push(RawMessage {
            step_index: *step_idx,
            role: "assistant".to_string(),
            message_type: "tool_call".to_string(),
            content: input_json.clone(),
            thinking: tool_thinking,
            created_at: seg.ts.clone(),
            model_name: seg.model.clone(),
            tool_name: Some(tool.name),
            tool_args: Some(input_json),
            duration_ms: None,
            token_count: None,
            credit: None,
            images: None,
        });
        *step_idx += 1;
    }

    // 仅有思考过程、没有正文与工具调用的回合
    if !thinking_used {
        messages.push(RawMessage {
            step_index: *step_idx,
            role: "assistant".to_string(),
            message_type: "text".to_string(),
            content: String::new(),
            thinking,
            created_at: seg.ts.clone(),
            model_name: seg.model.clone(),
            tool_name: None,
            tool_args: None,
            duration_ms: None,
            token_count: None,
            credit: None,
            images: None,
        });
        *step_idx += 1;
    }

    if let Some(credit) = seg.credits {
        if let Some(last) = messages.last_mut() {
            last.credit = Some(credit);
        }
    }

    *seg = Segment::default();
}

fn parse_qoder_jsonl(
    cid: &str,
    session_id: &str,
    path: &Path,
) -> Result<Option<RawConversation>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let mut messages: Vec<RawMessage> = Vec::new();
    let mut usage_records: Vec<RawUsageRecord> = Vec::new();
    let mut tool_names: HashMap<String, String> = HashMap::new();
    let mut segment = Segment::default();
    let mut step_idx = 0i64;

    let mut title: Option<String> = None;
    let mut workspace_dir: Option<String> = None;
    let mut cwd_hint: Option<String> = None;
    let mut runtime_model: Option<String> = None;
    let mut created_at: Option<String> = None;
    let mut updated_at: Option<String> = None;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        match value.get("type").and_then(|v| v.as_str()).unwrap_or("") {
            "workspace-directories" => {
                if workspace_dir.is_none() {
                    if let Some(d) = value
                        .get("directories")
                        .and_then(|v| v.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|v| v.as_str())
                    {
                        if !d.is_empty() {
                            workspace_dir = Some(d.to_string());
                        }
                    }
                }
            }
            "runtime-config" => {
                // timestamp 为 epoch 毫秒；model 为运行时选择（auto 等）
                if let Some(m) = value
                    .get("model")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                {
                    runtime_model = Some(m.to_string());
                }
                if created_at.is_none() {
                    if let Some(ms) = value.get("timestamp").and_then(|v| v.as_i64()) {
                        if let Some(dt) = chrono::DateTime::from_timestamp_millis(ms) {
                            created_at = Some(dt.to_rfc3339());
                        }
                    }
                }
            }
            "assistant" => {
                let msg = value.get("message").unwrap_or(&Value::Null);
                let mid = msg
                    .get("id")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .or_else(|| {
                        value
                            .get("uuid")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_default();

                // message.id 变化即新回合，先结束上一段再累计
                if !segment.message_id.is_empty() && segment.message_id != mid {
                    flush_segment(&mut segment, &mut messages, &mut usage_records, &mut step_idx, &mut tool_names);
                }
                if segment.message_id.is_empty() {
                    segment.message_id = mid;
                }

                let ts = normalize_to_iso(
                    value.get("timestamp").and_then(|v| v.as_str()).map(|s| s.to_string()),
                );
                if let Some(t) = ts {
                    if created_at.is_none() {
                        created_at = Some(t.clone());
                    }
                    updated_at = Some(t.clone());
                    segment.ts = Some(t);
                }
                if segment.model.is_none() {
                    segment.model = msg
                        .get("model")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .or_else(|| runtime_model.clone());
                }

                if let Some(blocks) = msg.get("content").and_then(|v| v.as_array()) {
                    for b in blocks {
                        match b.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                            "thinking" => {
                                if let Some(t) = b.get("thinking").and_then(|v| v.as_str()) {
                                    if !t.trim().is_empty() {
                                        segment.thinking.push(t.to_string());
                                    }
                                }
                            }
                            "text" => {
                                if let Some(t) = b.get("text").and_then(|v| v.as_str()) {
                                    if !t.trim().is_empty() {
                                        segment.text.push(t.to_string());
                                    }
                                }
                            }
                            "tool_use" => {
                                let name = b
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("tool")
                                    .to_string();
                                let input = b
                                    .get("input")
                                    .map(|v| {
                                        serde_json::to_string_pretty(v)
                                            .unwrap_or_else(|_| v.to_string())
                                    })
                                    .unwrap_or_default();
                                segment.tools.push(ToolUse {
                                    id: b
                                        .get("id")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or_default()
                                        .to_string(),
                                    name,
                                    input,
                                });
                            }
                            _ => {}
                        }
                    }
                }

                // 每个回合仅一行携带 usage：积分挂消息末条做内联展示，
                // 完整快照存入 segment，flush 时作为独立用量事实发射（流式后行覆盖前行）
                if let Some(usage) = msg.get("usage") {
                    if let Some(c) = usage.get("credits").and_then(|v| v.as_f64()) {
                        if c > 0.0 {
                            segment.credits = Some(c);
                        }
                    }
                    segment.usage = Some(usage.clone());
                }
            }
            "user" => {
                // 用户消息是回合边界，先结束可能未收尾的 assistant 段
                flush_segment(&mut segment, &mut messages, &mut usage_records, &mut step_idx, &mut tool_names);

                if value
                    .get("isCompactSummary")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                    || value.get("isMeta").and_then(|v| v.as_bool()).unwrap_or(false)
                {
                    continue;
                }
                if cwd_hint.is_none() {
                    if let Some(c) = value.get("cwd").and_then(|v| v.as_str()) {
                        if !c.is_empty() {
                            cwd_hint = Some(c.to_string());
                        }
                    }
                }

                let is_human = value
                    .get("origin")
                    .and_then(|o| o.get("kind"))
                    .and_then(|v| v.as_str())
                    == Some("human");

                let ts = normalize_to_iso(
                    value.get("timestamp").and_then(|v| v.as_str()).map(|s| s.to_string()),
                );
                if let Some(ref t) = ts {
                    if created_at.is_none() {
                        created_at = Some(t.clone());
                    }
                    updated_at = Some(t.clone());
                }

                if is_human {
                    let blocks = value
                        .get("message")
                        .and_then(|m| m.get("content"))
                        .and_then(|v| v.as_array());
                    let text = value
                        .get("humanInput")
                        .and_then(|h| h.get("text"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| {
                            blocks.map(|arr| {
                                arr.iter()
                                    .filter_map(|b| {
                                        if b.get("type").and_then(|v| v.as_str()) == Some("text") {
                                            b.get("text").and_then(|v| v.as_str())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n\n")
                            })
                        })
                        .unwrap_or_default();

                    let local_refs = extract_local_image_refs(&text);
                    let remote_urls: Vec<String> = blocks
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|b| {
                                    if b.get("type").and_then(|v| v.as_str()) != Some("image") {
                                        return None;
                                    }
                                    b.get("source")
                                        .and_then(|s| s.get("url"))
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string())
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                    let images = build_images_json(&local_refs, &remote_urls, session_id);

                    let content = strip_attachment_refs(&text);
                    if title.is_none() {
                        let first_line = content.lines().find(|l| !l.trim().is_empty());
                        if let Some(l) = first_line {
                            title = Some(truncate(l.trim(), TITLE_MAX_CHARS));
                        }
                    }
                    if content.trim().is_empty() && images.is_none() {
                        continue;
                    }

                    messages.push(RawMessage {
                        step_index: step_idx,
                        role: "user".to_string(),
                        message_type: "text".to_string(),
                        content: truncate(&content, CONTENT_TRUNCATE),
                        thinking: None,
                        created_at: ts,
                        model_name: None,
                        tool_name: None,
                        tool_args: None,
                        duration_ms: None,
                        token_count: None,
                        credit: None,
                        images,
                    });
                    step_idx += 1;
                } else if let Some(block) = value
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|v| v.as_array())
                    .and_then(|arr| {
                        arr.iter()
                            .find(|b| b.get("type").and_then(|v| v.as_str()) == Some("tool_result"))
                    })
                {
                    let tool_use_id = block
                        .get("tool_use_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    let tool_name = tool_names
                        .get(tool_use_id)
                        .cloned()
                        .unwrap_or_else(|| "tool".to_string());
                    let content = tool_result_text(block.get("content"));

                    messages.push(RawMessage {
                        step_index: step_idx,
                        role: "tool".to_string(),
                        message_type: "tool_result".to_string(),
                        content: truncate(&content, CONTENT_TRUNCATE),
                        thinking: None,
                        created_at: ts,
                        model_name: None,
                        tool_name: Some(tool_name),
                        tool_args: None,
                        duration_ms: None,
                        token_count: None,
                        credit: None,
                        images: None,
                    });
                    step_idx += 1;
                }
            }
            "system" => {
                flush_segment(&mut segment, &mut messages, &mut usage_records, &mut step_idx, &mut tool_names);
            }
            // attachment / active-leaf / last-prompt / file-history-snapshot 等辅助行不产生消息
            _ => {}
        }
    }

    flush_segment(&mut segment, &mut messages, &mut usage_records, &mut step_idx, &mut tool_names);

    if messages.is_empty() {
        return Ok(None);
    }

    let workspace_path = workspace_dir
        .or(cwd_hint)
        .unwrap_or_else(String::new);

    let title = title.unwrap_or_else(|| {
        let short: String = session_id.chars().take(8).collect();
        format!("Qoder 会话 {}", short)
    });

    Ok(Some(RawConversation {
        id: cid.to_string(),
        title,
        workspace_path,
        source_app: "qoder".to_string(),
        created_at,
        updated_at,
        parse_status: "ok".to_string(),
        source_types: vec!["qoder".to_string()],
        messages,
        artifacts: Vec::new(),
        usage_records,
    }))
}

/// tool_result 的 content 可能是字符串或内容块数组（含图片），统一转为文本
fn tool_result_text(raw: Option<&Value>) -> String {
    match raw {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(arr)) => {
            let mut parts: Vec<String> = Vec::new();
            for b in arr {
                match b.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                    "text" => {
                        if let Some(t) = b.get("text").and_then(|v| v.as_str()) {
                            parts.push(t.to_string());
                        }
                    }
                    "image" => parts.push("[图片]".to_string()),
                    _ => {}
                }
            }
            parts.join("\n")
        }
        _ => String::new(),
    }
}

/// 解析 humanInput 文本中的本地附件引用路径（"附件引用：\n- 文件：<abs path>"）
fn extract_local_image_refs(text: &str) -> Vec<String> {
    let mut refs = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.trim().strip_prefix("- 文件：") {
            let p = rest.trim();
            if !p.is_empty() {
                refs.push(p.to_string());
            }
        }
    }
    refs
}

/// 去掉展示正文尾部的"附件引用"段落，图片已单独归档
fn strip_attachment_refs(text: &str) -> String {
    match text.find("附件引用：") {
        Some(idx) => text[..idx].trim().to_string(),
        None => text.trim().to_string(),
    }
}

/// 本地临时副本优先归档到 ~/.agentdeck/media，缺失时回退到会过期的 OSS 签名 URL
fn build_images_json(
    local_refs: &[String],
    remote_urls: &[String],
    session_id: &str,
) -> Option<String> {
    let count = local_refs.len().max(remote_urls.len());
    if count == 0 {
        return None;
    }
    let mut list = Vec::new();
    for i in 0..count {
        let local = local_refs.get(i);
        let remote = remote_urls.get(i);
        let src = local
            .and_then(|p| {
                crate::media_archive::archive_image_file(Path::new(p), "qoder", session_id)
            })
            .or_else(|| remote.cloned());
        if let Some(src) = src {
            list.push(serde_json::json!({ "src": src }));
        }
    }
    if list.is_empty() {
        None
    } else {
        serde_json::to_string(&list).ok()
    }
}

fn truncate(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{}\n…(已截断 {} 字符)", cut, count - max)
}

fn qoder_parser_rev_stale(conn: &Connection) -> bool {
    let stored: Option<String> = conn
        .query_row(
            "SELECT conversation_id FROM sync_state WHERE source_path = ?",
            params![QODER_PARSER_REV_KEY],
            |r| r.get(0),
        )
        .ok();
    stored.as_deref() != Some(QODER_PARSER_REV)
}

fn mark_qoder_synced(conn: &Connection) {
    let now = chrono::Utc::now().to_rfc3339();
    let _ = conn.execute(
        r#"
        INSERT INTO sync_state (source_path, conversation_id, source_type, file_mtime, file_size, synced_at)
        VALUES (?1, ?2, 'qoder_parser', 0, 0, ?3)
        ON CONFLICT(source_path) DO UPDATE SET
            conversation_id = excluded.conversation_id,
            synced_at = excluded.synced_at
        "#,
        params![QODER_PARSER_REV_KEY, QODER_PARSER_REV, now],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 对本机真实 Qoder 数据目录做一次端到端试跑（默认跳过）
    #[test]
    #[ignore]
    fn test_sync_real_qoder_data() {
        let home = dirs::home_dir().unwrap();
        let projects_dir = home.join(".qoder/projects");
        assert!(
            projects_dir.is_dir(),
            "Qoder projects dir not found at {:?}",
            projects_dir
        );

        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();

        let stats = sync(&conn, false);
        println!(
            "sync stats: new={} updated={} skipped={} errors={}",
            stats.new_count, stats.updated_count, stats.skipped_count, stats.error_count
        );
        assert_eq!(stats.error_count, 0);

        let (total, q_cnt): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), SUM(CASE WHEN source_app='qoder' THEN 1 ELSE 0 END)
                 FROM conversations",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        let (msgs, with_credit, with_images): (i64, i64, i64) = conn
            .query_row(
                "SELECT COUNT(*),
                        SUM(CASE WHEN credit IS NOT NULL THEN 1 ELSE 0 END),
                        SUM(CASE WHEN images IS NOT NULL THEN 1 ELSE 0 END)
                 FROM messages",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        println!(
            "conversations={} qoder={} messages={} with_credit={} with_images={}",
            total, q_cnt, msgs, with_credit, with_images
        );
        for (wid, title, ws) in conn
            .prepare(
                "SELECT id, title, workspace_path FROM conversations WHERE source_app='qoder' LIMIT 5",
            )
            .unwrap()
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .unwrap()
            .flatten()
        {
            println!("  sample: {} | {} | {}", wid, title, ws);
        }
        assert!(q_cnt > 0, "expected at least one qoder conversation");
    }

    #[test]
    fn truncates_long_content() {
        let s = "x".repeat(5000);
        let out = truncate(&s, CONTENT_TRUNCATE);
        assert!(out.contains("已截断"));
        assert!(out.chars().count() < 4100);
    }

    #[test]
    fn extracts_and_strips_attachment_refs() {
        let raw = "看看这个 UI\n\n附件引用：\n- 文件：/tmp/a.png\n- 文件：/tmp/b.png";
        let refs = extract_local_image_refs(raw);
        assert_eq!(refs, vec!["/tmp/a.png".to_string(), "/tmp/b.png".to_string()]);
        assert_eq!(strip_attachment_refs(raw), "看看这个 UI");
    }

    #[test]
    fn keeps_text_without_attachment_refs() {
        assert_eq!(strip_attachment_refs("  普通提问  "), "普通提问");
    }

    #[test]
    fn tool_result_handles_string_and_blocks() {
        let s = serde_json::json!("done");
        assert_eq!(tool_result_text(Some(&s)), "done");
        let arr = serde_json::json!([
            {"type": "text", "text": "line1"},
            {"type": "image", "source": {"type": "url", "url": "https://x"}},
        ]);
        assert_eq!(tool_result_text(Some(&arr)), "line1\n[图片]");
        assert_eq!(tool_result_text(None), "");
    }

    fn write_qoder_fixture(name: &str, lines: &[String]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("agentdeck_qoder_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{}-{}.jsonl", name, std::process::id()));
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        path
    }

    #[test]
    fn emits_usage_records_with_request_id_identity() {
        let lines = vec![
            r#"{"type":"workspace-directories","directories":["/Users/xiyangxie/workspace/personal/Tappy"]}"#.to_string(),
            r#"{"type":"user","timestamp":"2026-09-20T10:00:00.000Z","cwd":"/Users/xiyangxie/workspace/personal/Tappy","origin":{"kind":"human"},"message":{"content":[{"type":"text","text":"跑一下测试"}]}}"#.to_string(),
            r#"{"type":"assistant","timestamp":"2026-09-20T10:00:05.000Z","message":{"id":"msg_a","model":"qoder-max","content":[{"type":"text","text":"第一回合"}],"usage":{"input_tokens":100,"cache_creation_input_tokens":10,"cache_read_input_tokens":20,"output_tokens":30,"request_id":"req_a"}}}"#.to_string(),
            // 同一 message.id 的流式分片：后行 usage 覆盖前行，只发一条事实
            r#"{"type":"assistant","timestamp":"2026-09-20T10:00:06.000Z","message":{"id":"msg_a","model":"qoder-max","content":[{"type":"text","text":"（续）"}],"usage":{"input_tokens":150,"cache_creation_input_tokens":10,"cache_read_input_tokens":20,"output_tokens":45,"credits":0.35,"request_id":"req_a"}}}"#.to_string(),
            r#"{"type":"user","timestamp":"2026-09-20T10:01:00.000Z","origin":{"kind":"human"},"message":{"content":[{"type":"text","text":"再来"}]}}"#.to_string(),
            r#"{"type":"assistant","timestamp":"2026-09-20T10:01:05.000Z","message":{"id":"msg_b","model":"qoder-max","content":[{"type":"text","text":"第二回合"}],"usage":{"input_tokens":200,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":60,"request_id":"req_b"}}}"#.to_string(),
        ];
        let path = write_qoder_fixture("usage", &lines);
        let conv = parse_qoder_jsonl("qoder:test", "test", &path)
            .unwrap()
            .unwrap();

        assert_eq!(conv.usage_records.len(), 2);
        let a = &conv.usage_records[0];
        assert_eq!(a.identity, "req_a");
        assert_eq!(a.input_tokens, Some(150));
        assert_eq!(a.output_tokens, Some(45));
        assert_eq!(a.cache_write_tokens, Some(10));
        assert_eq!(a.cache_read_tokens, Some(20));
        assert_eq!(a.credit, Some(0.35));
        assert_eq!(a.model.as_deref(), Some("qoder-max"));
        // timestamp 经 normalize_to_iso 重排，只断言时刻前缀
        assert!(
            a.occurred_at
                .as_deref()
                .unwrap_or("")
                .starts_with("2026-09-20T10:00:06"),
            "unexpected occurred_at: {:?}",
            a.occurred_at
        );
        let b = &conv.usage_records[1];
        assert_eq!(b.identity, "req_b");
        assert_eq!(b.input_tokens, Some(200));
        assert_eq!(b.credit, None);
    }
}
