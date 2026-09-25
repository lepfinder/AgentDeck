use rusqlite::{params, Connection};
use serde_json::Value;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use super::{
    needs_sync, record_sync_state, save_conversation_tx, ImporterStats, RawConversation, RawMessage,
};

const GROKBOT_PARSER_REV: &str = "grokbot-v2";
const GROKBOT_PARSER_REV_KEY: &str = "agentdeck:grokbot_parser_rev";

fn grokbot_parser_rev_stale(conn: &Connection) -> bool {
    let stored: Option<String> = conn
        .query_row(
            "SELECT conversation_id FROM sync_state WHERE source_path = ?",
            params![GROKBOT_PARSER_REV_KEY],
            |r| r.get(0),
        )
        .ok();
    stored.as_deref() != Some(GROKBOT_PARSER_REV)
}

fn mark_grokbot_synced(conn: &Connection) {
    let now = chrono::Utc::now().to_rfc3339();
    let _ = conn.execute(
        r#"
        INSERT INTO sync_state (source_path, conversation_id, source_type, file_mtime, file_size, synced_at)
        VALUES (?1, ?2, 'grokbot_parser', 0, 0, ?3)
        ON CONFLICT(source_path) DO UPDATE SET
            conversation_id = excluded.conversation_id,
            synced_at = excluded.synced_at
        "#,
        params![GROKBOT_PARSER_REV_KEY, GROKBOT_PARSER_REV, now],
    );
}

pub fn sync(conn: &Connection, incremental: bool) -> ImporterStats {
    let mut stats = ImporterStats {
        app: "Grok Bot".to_string(),
        new_count: 0,
        updated_count: 0,
        skipped_count: 0,
        error_count: 0,
    };

    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return stats,
    };

    let sync_dir = home.join(".grokbot/chats_sync");
    if !sync_dir.is_dir() {
        return stats;
    }

    let force_reparse = grokbot_parser_rev_stale(conn);
    let incremental = incremental && !force_reparse;

    let mut files: Vec<_> = match std::fs::read_dir(&sync_dir) {
        Ok(rd) => rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("jsonl"))
            .collect(),
        Err(_) => return stats,
    };
    files.sort();

    for p in files {
        if incremental && !needs_sync(conn, &p, true) {
            stats.skipped_count += 1;
            continue;
        }

        let agent_id = p
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let cid = format!("grokbot:{}", agent_id);

        match parse_grokbot_file(&cid, &agent_id, &p, &home) {
            Ok(Some(conv)) => match save_conversation_tx(conn, &conv) {
                Ok(is_new) => {
                    record_sync_state(conn, &p, &cid, "grokbot_jsonl");
                    if is_new {
                        stats.new_count += 1;
                    } else {
                        stats.updated_count += 1;
                    }
                }
                Err(e) => {
                    eprintln!("[GrokBot Importer] 保存失败 {}: {}", cid, e);
                    stats.error_count += 1;
                }
            },
            Ok(None) => {
                record_sync_state(conn, &p, &cid, "grokbot_jsonl");
                stats.skipped_count += 1;
            }
            Err(e) => {
                eprintln!("[GrokBot Importer] 解析失败 {}: {}", cid, e);
                stats.error_count += 1;
            }
        }
    }

    // 有错误的运行不标记 rev：否则一次不完整的运行会永久消耗强制重导
    if stats.error_count == 0 {
        mark_grokbot_synced(conn);
    }
    stats
}

fn timestamp_ms_to_iso(ms: Option<i64>) -> Option<String> {
    ms.and_then(chrono::DateTime::from_timestamp_millis)
        .map(|dt| dt.to_rfc3339())
}

/// 只收纯文本对话条目：message (user/assistant) 与 send-message (text)。
/// widget / user-form / local-tool-permission / connector / attachment / event
/// 是交互控件与元数据，不进入会话正文。
fn extract_entry(e: &Value) -> Option<(String, String)> {
    if e.get("isStreaming").and_then(|v| v.as_bool()) == Some(true) {
        return None;
    }
    match e.get("kind").and_then(|v| v.as_str()) {
        Some("message") => {
            let role = e.get("role").and_then(|v| v.as_str()).unwrap_or("user");
            let content = e.get("content").and_then(|v| v.as_str())?.trim();
            if content.is_empty() {
                return None;
            }
            Some((role.to_string(), content.to_string()))
        }
        Some("send-message") => {
            let msg = e.get("message")?;
            if msg.get("type").and_then(|v| v.as_str()) != Some("text") {
                return None;
            }
            let content = msg.get("content").and_then(|v| v.as_str())?.trim();
            if content.is_empty() {
                return None;
            }
            Some(("assistant".to_string(), content.to_string()))
        }
        _ => None,
    }
}

fn parse_grokbot_file(
    cid: &str,
    agent_id: &str,
    path: &Path,
    home: &Path,
) -> Result<Option<RawConversation>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let mut messages: Vec<RawMessage> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut agent_name = String::new();
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
        let val: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if agent_name.is_empty() {
            if let Some(name) = val.get("agent_name").and_then(|v| v.as_str()) {
                if !name.trim().is_empty() {
                    agent_name = name.trim().to_string();
                }
            }
        }

        let entry_id = val.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let ts = timestamp_ms_to_iso(val.get("timestampMs").and_then(|v| v.as_i64()));
        if created_at.is_none() && ts.is_some() {
            created_at = ts.clone();
        }
        if ts.is_some() {
            updated_at = ts.clone();
        }

        if !entry_id.is_empty() && !seen_ids.insert(entry_id.to_string()) {
            continue;
        }

        let Some((role, content)) = extract_entry(&val) else {
            continue;
        };

        messages.push(RawMessage {
            step_index: messages.len() as i64,
            role,
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
    }

    if messages.is_empty() {
        return Ok(None);
    }

    let title = if agent_name.is_empty() {
        format!("Grok Bot {}", &agent_id[..agent_id.len().min(8)])
    } else {
        agent_name
    };

    Ok(Some(RawConversation {
        id: cid.to_string(),
        title,
        workspace_path: home.join(".grokbot").to_string_lossy().to_string(),
        source_app: "grokbot".to_string(),
        created_at,
        updated_at,
        parse_status: "ok".to_string(),
        source_types: vec!["grokbot".to_string()],
        messages,
        artifacts: Vec::new(),
        usage_records: Vec::new(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_parse_grokbot_file() {
        let tmp_path = std::env::temp_dir().join(format!("test_grokbot_{}.jsonl", std::process::id()));
        let mut tmp = File::create(&tmp_path).unwrap();
        writeln!(
            tmp,
            r#"{{"kind":"message","id":"u1","role":"user","content":"帮我看看这个部署问题","isStreaming":false,"timestampMs":1790154142825,"agent_id":"abc-12345678","agent_name":"大主管"}}"#
        )
        .unwrap();
        // 非文本条目应被跳过
        writeln!(
            tmp,
            r#"{{"kind":"send-message","id":"s0","message":{{"type":"widget","widget":{{"prompt":"选哪个"}}}},"timestampMs":1790154150000,"agent_id":"abc-12345678","agent_name":"大主管"}}"#
        )
        .unwrap();
        writeln!(
            tmp,
            r#"{{"kind":"send-message","id":"s1","message":{{"type":"text","content":"已经修好了。"}},"timestampMs":1790154160000,"agent_id":"abc-12345678","agent_name":"大主管"}}"#
        )
        .unwrap();
        // 重复 id 去重
        writeln!(
            tmp,
            r#"{{"kind":"message","id":"u1","role":"user","content":"帮我看看这个部署问题","timestampMs":1790154142825,"agent_id":"abc-12345678","agent_name":"大主管"}}"#
        )
        .unwrap();
        // 未完成流式条目跳过
        writeln!(
            tmp,
            r#"{{"kind":"message","id":"a2","role":"assistant","content":"半截","isStreaming":true,"timestampMs":1790154170000,"agent_id":"abc-12345678","agent_name":"大主管"}}"#
        )
        .unwrap();
        drop(tmp);

        let home = std::env::temp_dir();
        let conv = parse_grokbot_file("grokbot:abc-12345678", "abc-12345678", &tmp_path, &home)
            .unwrap()
            .unwrap();
        let _ = std::fs::remove_file(&tmp_path);

        assert_eq!(conv.title, "大主管");
        assert_eq!(conv.source_app, "grokbot");
        // 归组到 <home>/.grokbot 工作区而非未分类
        assert_eq!(
            conv.workspace_path,
            home.join(".grokbot").to_string_lossy()
        );
        assert_eq!(conv.messages.len(), 2);
        assert_eq!(conv.messages[0].role, "user");
        assert_eq!(conv.messages[0].content, "帮我看看这个部署问题");
        assert_eq!(conv.messages[1].role, "assistant");
        assert_eq!(conv.messages[1].content, "已经修好了。");
        assert!(conv.messages[0].created_at.as_deref().unwrap().starts_with("2026-"));
    }

    /// 拷贝真实 DB 到临时副本后全量同步；绝不直写真实库，
    /// 避免与运行中的 dev 应用抢写锁、消耗 parser rev
    #[test]
    fn test_sync_real_grokbot_db() {
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => return,
        };
        let real = home.join(".agentdeck/agentdeck.db");
        if !real.is_file() {
            return;
        }
        let sync_dir_exists = home.join(".grokbot/chats_sync").is_dir();
        let copy = std::env::temp_dir().join(format!("agentdeck_grokbot_test_{}.db", std::process::id()));
        std::fs::copy(&real, &copy).unwrap();
        let conn = Connection::open(&copy).unwrap();
        let stats = sync(&conn, false);
        eprintln!("GrokBot sync stats: {:?}", stats);
        assert!(
            stats.error_count == 0,
            "Errors during sync: {}",
            stats.error_count
        );
        if sync_dir_exists {
            let (conv_cnt, msg_cnt): (i64, i64) = conn
                .query_row(
                    r#"SELECT COUNT(*), COALESCE(SUM(message_count), 0)
                       FROM conversations WHERE source_app = 'grokbot'"#,
                    [],
                    |r| Ok((r.get(0)?, r.get(1).unwrap_or(0))),
                )
                .unwrap();
            assert_eq!(conv_cnt, stats.new_count as i64 + stats.updated_count as i64);
            assert!(msg_cnt > 0, "grokbot conversations contain no messages");
            // 全部归到 ~/.grokbot 工作区，不允许再落到未分类
            let unfiled: i64 = conn
                .query_row(
                    r#"SELECT COUNT(*) FROM conversations
                       WHERE source_app = 'grokbot' AND COALESCE(workspace_path, '') != ?1"#,
                    params![home.join(".grokbot").to_string_lossy().to_string()],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(unfiled, 0, "grokbot conversations outside ~/.grokbot workspace");
        }
        drop(conn);
        let _ = std::fs::remove_file(&copy);
    }
}
