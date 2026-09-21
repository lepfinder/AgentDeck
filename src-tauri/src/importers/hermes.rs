use rusqlite::{params, Connection, OpenFlags};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use super::{
    needs_sync, record_sync_state, save_conversation_tx, ImporterStats, RawConversation, RawMessage,
    RawUsageRecord,
};

const HERMES_PARSER_REV: &str = "hermes-v3";
const HERMES_PARSER_REV_KEY: &str = "agentdeck:hermes_parser_rev";

fn hermes_parser_rev_stale(conn: &Connection) -> bool {
    let stored: Option<String> = conn
        .query_row(
            "SELECT conversation_id FROM sync_state WHERE source_path = ?",
            params![HERMES_PARSER_REV_KEY],
            |r| r.get(0),
        )
        .ok();
    stored.as_deref() != Some(HERMES_PARSER_REV)
}

fn mark_hermes_synced(conn: &Connection) {
    let now = chrono::Utc::now().to_rfc3339();
    let _ = conn.execute(
        r#"
        INSERT INTO sync_state (source_path, conversation_id, source_type, file_mtime, file_size, synced_at)
        VALUES (?1, ?2, 'hermes_parser', 0, 0, ?3)
        ON CONFLICT(source_path) DO UPDATE SET
            conversation_id = excluded.conversation_id,
            synced_at = excluded.synced_at
        "#,
        params![HERMES_PARSER_REV_KEY, HERMES_PARSER_REV, now],
    );
}

/// unix 秒（含小数）→ RFC3339
fn sec_to_rfc3339(s: f64) -> Option<String> {
    if !s.is_finite() || s <= 0.0 {
        return None;
    }
    chrono::DateTime::from_timestamp(s as i64, ((s.fract()) * 1_000_000_000.0) as u32)
        .map(|dt| dt.to_rfc3339())
}

pub fn sync(conn: &Connection, incremental: bool) -> ImporterStats {
    let mut stats = ImporterStats {
        app: "Hermes".to_string(),
        new_count: 0,
        updated_count: 0,
        skipped_count: 0,
        error_count: 0,
    };

    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return stats,
    };

    let hermes_dir = home.join(".hermes");
    if !hermes_dir.is_dir() {
        return stats;
    }

    let mut synced_cids = HashSet::new();
    let force_reparse = hermes_parser_rev_stale(conn);

    // 1. 优先读取 ~/.hermes/state.db
    let state_db = hermes_dir.join("state.db");
    if state_db.is_file() {
        if !incremental || force_reparse || needs_sync(conn, &state_db, true) {
            match sync_hermes_state_db(conn, &state_db, &mut synced_cids, force_reparse) {
                Ok((n, u)) => {
                    record_sync_state(conn, &state_db, "hermes:state_db", "hermes_state_db");
                    stats.new_count += n;
                    stats.updated_count += u;
                }
                Err(e) => {
                    eprintln!("[Hermes Importer] state.db 同步失败: {}", e);
                    stats.error_count += 1;
                }
            }
        } else {
            stats.skipped_count += 1;
        }
    }

    // 2. 补充扫描 ~/.hermes/sessions/*.jsonl
    let sessions_dir = hermes_dir.join("sessions");
    if sessions_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                    continue;
                }

                let stem = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let cid = format!("hermes:{}", stem);
                if synced_cids.contains(&cid) {
                    continue;
                }

                if incremental && !needs_sync(conn, &path, true) {
                    stats.skipped_count += 1;
                    continue;
                }

                match parse_hermes_jsonl(&cid, &path) {
                    Ok(Some(conv)) => match save_conversation_tx(conn, &conv) {
                        Ok(is_new) => {
                            record_sync_state(conn, &path, &cid, "hermes_jsonl");
                            if is_new {
                                stats.new_count += 1;
                            } else {
                                stats.updated_count += 1;
                            }
                        }
                        Err(e) => {
                            eprintln!("[Hermes Importer] 保存失败 {}: {}", cid, e);
                            stats.error_count += 1;
                        }
                    },
                    Ok(None) => {
                        stats.skipped_count += 1;
                    }
                    Err(e) => {
                        eprintln!("[Hermes Importer] 解析失败 {}: {}", cid, e);
                        stats.error_count += 1;
                    }
                }
            }
        }
    }

    // 有错误的运行不标记 rev：否则一次不完整的运行会永久消耗强制重导
    if stats.error_count == 0 {
        mark_hermes_synced(conn);
    }
    stats
}

fn sync_hermes_state_db(
    conn: &Connection,
    db_path: &Path,
    synced_cids: &mut HashSet<String>,
    force: bool,
) -> Result<(u32, u32), Box<dyn std::error::Error>> {
    let hermes_conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    let mut stmt = hermes_conn.prepare(
        "SELECT id, title, started_at, COALESCE(last_activity_at, ended_at, started_at) as updated_at, cwd FROM sessions ORDER BY started_at DESC"
    )?;

    let session_rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let title: Option<String> = row.get(1)?;
        let started_sec: Option<f64> = row.get(2)?;
        let updated_sec: Option<f64> = row.get(3)?;
        let cwd: Option<String> = row.get(4)?;

        let created_at = started_sec.and_then(|s| {
            chrono::DateTime::from_timestamp(s as i64, ((s.fract()) * 1_000_000_000.0) as u32)
                .map(|dt| dt.to_rfc3339())
        });
        let updated_at = updated_sec
            .and_then(|s| {
                chrono::DateTime::from_timestamp(s as i64, ((s.fract()) * 1_000_000_000.0) as u32)
                    .map(|dt| dt.to_rfc3339())
            })
            .or_else(|| created_at.clone());

        Ok((id, title, created_at, updated_at, cwd))
    })?;

    let sessions: Vec<_> = session_rows.flatten().collect();
    drop(stmt);

    let mut new_cnt = 0;
    let mut updated_cnt = 0;

    let mut existing_map: HashMap<String, (Option<String>, i64)> = HashMap::new();
    // force 全量重解析时不读跳表，让 usage 变化也能触发重写
    if !force {
        if let Ok(mut exist_stmt) = conn
            .prepare("SELECT id, updated_at, message_count FROM conversations WHERE id LIKE 'hermes:%'")
        {
            if let Ok(rows) = exist_stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, i64>(2).unwrap_or(0),
                ))
            }) {
                for row in rows.flatten() {
                    existing_map.insert(row.0, (row.1, row.2));
                }
            }
        }
    }

    let mut msg_stmt = hermes_conn.prepare(
        "SELECT role, content, timestamp, tool_calls FROM messages WHERE session_id = ? ORDER BY timestamp ASC, id ASC",
    )?;

    // session_model_usage：会话级聚合用量，一行 = 一个 (model, billing, task) 组合
    let mut usage_stmt = hermes_conn.prepare(
        "SELECT model, billing_provider, billing_base_url, billing_mode, task,
                input_tokens, output_tokens, cache_read_tokens, cache_write_tokens,
                reasoning_tokens, estimated_cost_usd, actual_cost_usd, last_seen
         FROM session_model_usage WHERE session_id = ?",
    )?;

    for s_row in sessions {
        let (raw_id, raw_title, created_at, updated_at, raw_cwd) = s_row;
        let cid = format!("hermes:{}", raw_id);
        synced_cids.insert(cid.clone());
        let workspace_path = raw_cwd
            .map(|p| super::canonicalize_workspace_path(&p))
            .unwrap_or_default();

        if let Some((prev_up, _)) = existing_map.get(&cid) {
            if prev_up == &updated_at {
                continue;
            }
        }

        let mut messages = Vec::new();
        let mut step_idx = 0i64;
        let mut derived_title = raw_title.unwrap_or_default();

        let msg_rows = msg_stmt.query_map(params![&raw_id], |m_row| {
            let role: String = m_row.get(0)?;
            let content: Option<String> = m_row.get(1)?;
            let ts_sec: Option<f64> = m_row.get(2)?;
            let tool_calls: Option<String> = m_row.get(3)?;

            let m_created = ts_sec.and_then(|s| {
                chrono::DateTime::from_timestamp(s as i64, ((s.fract()) * 1_000_000_000.0) as u32)
                    .map(|dt| dt.to_rfc3339())
            });

            Ok((role, content, m_created, tool_calls))
        })?;

        for m in msg_rows.flatten() {
            let (role, content_opt, m_created, tool_calls) = m;
            let text = content_opt.unwrap_or_default();
            if text.trim().is_empty() && tool_calls.is_none() {
                continue;
            }

            if role == "user" && derived_title.is_empty() {
                derived_title = text.chars().take(60).collect();
            }

            messages.push(RawMessage {
                step_index: step_idx,
                role,
                message_type: if tool_calls.is_some() {
                    "tool_call".to_string()
                } else {
                    "text".to_string()
                },
                content: text,
                thinking: None,
                created_at: m_created,
                model_name: Some("Hermes".to_string()),
                tool_name: None,
                tool_args: tool_calls,
                duration_ms: None,
                token_count: None,
                credit: None,
                images: None,
            });
            step_idx += 1;
        }

        if messages.is_empty() {
            continue;
        }

        // 会话级聚合用量：input 不含缓存读（实测 input < cache_read），各桶独立
        let mut usage_records = Vec::new();
        let usage_rows = usage_stmt.query_map(params![&raw_id], |u_row| {
            Ok((
                u_row.get::<_, String>(0)?,
                u_row.get::<_, String>(1)?,
                u_row.get::<_, String>(2)?,
                u_row.get::<_, String>(3)?,
                u_row.get::<_, String>(4)?,
                u_row.get::<_, i64>(5)?,
                u_row.get::<_, i64>(6)?,
                u_row.get::<_, i64>(7)?,
                u_row.get::<_, i64>(8)?,
                u_row.get::<_, i64>(9)?,
                u_row.get::<_, f64>(10)?,
                u_row.get::<_, f64>(11)?,
                u_row.get::<_, Option<f64>>(12)?,
            ))
        })?;
        for u_row in usage_rows.flatten() {
            let (model, provider, base_url, mode, task, input, output, cache_read, cache_write, reasoning, est, actual, last_seen) = u_row;
            let identity = format!("{}|{}|{}|{}|{}", model, provider, base_url, mode, task);
            let credit = if actual > 0.0 {
                Some(actual)
            } else if est > 0.0 {
                Some(est)
            } else {
                None
            };
            usage_records.push(RawUsageRecord {
                identity,
                agent: "hermes".to_string(),
                model: Some(model),
                input_tokens: Some(input),
                cache_read_tokens: Some(cache_read),
                cache_write_tokens: Some(cache_write),
                output_tokens: Some(output),
                reasoning_tokens: Some(reasoning),
                credit,
                occurred_at: last_seen.and_then(sec_to_rfc3339),
                is_partial: false,
            });
        }

        if derived_title.is_empty() {
            derived_title = format!("Hermes 会话 {}", &raw_id[..raw_id.len().min(8)]);
        }

        let conv = RawConversation {
            id: cid,
            title: derived_title,
            workspace_path,
            source_app: "hermes".to_string(),
            created_at,
            updated_at,
            parse_status: "ok".to_string(),
            source_types: vec!["hermes".to_string()],
            messages,
            artifacts: Vec::new(),
            usage_records,
        };

        if save_conversation_tx(conn, &conv)? {
            new_cnt += 1;
        } else {
            updated_cnt += 1;
        }
    }

    Ok((new_cnt, updated_cnt))
}

fn parse_hermes_jsonl(
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

        if workspace_path.is_empty() {
            if let Some(ws) = val
                .get("cwd")
                .or_else(|| val.get("workspace"))
                .and_then(|v| v.as_str())
            {
                workspace_path = super::canonicalize_workspace_path(ws);
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

        let role = val.get("role").and_then(|v| v.as_str()).unwrap_or("user");
        let content = val
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();

        if content.is_empty() {
            continue;
        }

        if role == "user" && title.is_empty() {
            title = content.chars().take(60).collect();
        }

        messages.push(RawMessage {
            step_index: step_idx,
            role: role.to_string(),
            message_type: "text".to_string(),
            content: content.to_string(),
            thinking: None,
            created_at: ts,
            model_name: Some("Hermes".to_string()),
            tool_name: None,
            tool_args: None,
            duration_ms: None,
            token_count: None,
            credit: None,
            images: None,
        });
        step_idx += 1;
    }

    if messages.is_empty() {
        return Ok(None);
    }

    if title.is_empty() {
        title = format!("Hermes 会话 {}", &cid[..cid.len().min(8)]);
    }

    Ok(Some(RawConversation {
        id: cid.to_string(),
        title,
        workspace_path,
        source_app: "hermes".to_string(),
        created_at,
        updated_at,
        parse_status: "ok".to_string(),
        source_types: vec!["hermes".to_string()],
        messages,
        artifacts: Vec::new(),
        usage_records: Vec::new(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syncs_session_model_usage_as_records() {
        let tmp = std::env::temp_dir().join(format!(
            "hermes_usage_test_{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&tmp);
        let hermes = Connection::open(&tmp).unwrap();
        hermes.execute_batch(
            r#"
            CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT, started_at REAL, last_activity_at REAL, ended_at REAL, cwd TEXT);
            CREATE TABLE messages (id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT, role TEXT, content TEXT, timestamp REAL, tool_calls TEXT);
            CREATE TABLE session_model_usage (
                session_id TEXT NOT NULL, model TEXT NOT NULL,
                billing_provider TEXT NOT NULL DEFAULT '', billing_base_url TEXT NOT NULL DEFAULT '',
                billing_mode TEXT NOT NULL DEFAULT '', task TEXT NOT NULL DEFAULT '',
                api_call_count INTEGER NOT NULL DEFAULT 0,
                input_tokens INTEGER NOT NULL DEFAULT 0, output_tokens INTEGER NOT NULL DEFAULT 0,
                cache_read_tokens INTEGER NOT NULL DEFAULT 0, cache_write_tokens INTEGER NOT NULL DEFAULT 0,
                reasoning_tokens INTEGER NOT NULL DEFAULT 0,
                estimated_cost_usd REAL NOT NULL DEFAULT 0, actual_cost_usd REAL NOT NULL DEFAULT 0,
                cost_status TEXT, cost_source TEXT, first_seen REAL, last_seen REAL,
                PRIMARY KEY (session_id, model, billing_provider, billing_base_url, billing_mode, task)
            );
            INSERT INTO sessions VALUES ('s1', 'demo', 1775629586.0, 1775631452.0, NULL, '/tmp/demo');
            INSERT INTO messages (session_id, role, content, timestamp) VALUES ('s1', 'user', 'hi', 1775629587.0);
            INSERT INTO session_model_usage (session_id, model, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens, actual_cost_usd, last_seen)
                VALUES ('s1', 'qwen3-coder-plus', 270154, 4188, 1342272, 0, 0, 0.0, 1775631452.7);
            "#,
        )
        .unwrap();
        drop(hermes);

        let deck = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&deck).unwrap();

        let mut synced = HashSet::new();
        let (n, _u) = sync_hermes_state_db(&deck, &tmp, &mut synced, true).unwrap();
        assert_eq!(n, 1);

        let row: (String, String, i64, i64, i64, Option<f64>, Option<String>) = deck
            .query_row(
                "SELECT identity, model, input_tokens, cache_read_tokens, output_tokens, credit, occurred_at
                 FROM usage_records WHERE conversation_id = 'hermes:s1'",
                [],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(row.0, "qwen3-coder-plus||||");
        assert_eq!(row.1, "qwen3-coder-plus");
        assert_eq!(row.2, 270154);
        assert_eq!(row.3, 1342272);
        assert_eq!(row.4, 4188);
        assert_eq!(row.5, None);
        assert!(row.6.as_deref().unwrap().starts_with("2026-"));

        let _ = std::fs::remove_file(&tmp);
    }
}
