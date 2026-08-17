use rusqlite::{params, Connection, OpenFlags};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use super::{needs_sync, record_sync_state, save_conversation_tx, ImporterStats, RawConversation, RawMessage};

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

    // 1. 优先读取 ~/.hermes/state.db
    let state_db = hermes_dir.join("state.db");
    if state_db.is_file() {
        if !incremental || needs_sync(conn, &state_db, true) {
            match sync_hermes_state_db(conn, &state_db, &mut synced_cids) {
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

                let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                let cid = format!("hermes:{}", stem);
                if synced_cids.contains(&cid) {
                    continue;
                }

                if incremental && !needs_sync(conn, &path, true) {
                    stats.skipped_count += 1;
                    continue;
                }

                match parse_hermes_jsonl(&cid, &path) {
                    Ok(Some(conv)) => {
                        match save_conversation_tx(conn, &conv) {
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
                        }
                    }
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

    stats
}

fn sync_hermes_state_db(
    conn: &Connection,
    db_path: &Path,
    synced_cids: &mut HashSet<String>,
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
            chrono::DateTime::from_timestamp(s as i64, ((s.fract()) * 1_000_000_000.0) as u32).map(|dt| dt.to_rfc3339())
        });
        let updated_at = updated_sec.and_then(|s| {
            chrono::DateTime::from_timestamp(s as i64, ((s.fract()) * 1_000_000_000.0) as u32).map(|dt| dt.to_rfc3339())
        }).or_else(|| created_at.clone());

        Ok((id, title, created_at, updated_at, cwd))
    })?;

    let sessions: Vec<_> = session_rows.flatten().collect();
    drop(stmt);

    let mut new_cnt = 0;
    let mut updated_cnt = 0;

    let mut existing_map: HashMap<String, (Option<String>, i64)> = HashMap::new();
    if let Ok(mut exist_stmt) = conn.prepare(
        "SELECT id, updated_at, message_count FROM conversations WHERE id LIKE 'hermes:%'",
    ) {
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

    let mut msg_stmt = hermes_conn.prepare(
        "SELECT role, content, timestamp, tool_calls FROM messages WHERE session_id = ? ORDER BY timestamp ASC, id ASC",
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
                chrono::DateTime::from_timestamp(s as i64, ((s.fract()) * 1_000_000_000.0) as u32).map(|dt| dt.to_rfc3339())
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
                message_type: if tool_calls.is_some() { "tool_call".to_string() } else { "text".to_string() },
                content: text,
                thinking: None,
                created_at: m_created,
                model_name: Some("Hermes".to_string()),
                tool_name: None,
                tool_args: tool_calls,
                duration_ms: None,
                token_count: None,
            });
            step_idx += 1;
        }

        if messages.is_empty() {
            continue;
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
        };

        if save_conversation_tx(conn, &conv)? {
            new_cnt += 1;
        } else {
            updated_cnt += 1;
        }
    }

    Ok((new_cnt, updated_cnt))
}

fn parse_hermes_jsonl(cid: &str, path: &Path) -> Result<Option<RawConversation>, Box<dyn std::error::Error>> {
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
            if let Some(ws) = val.get("cwd").or_else(|| val.get("workspace")).and_then(|v| v.as_str()) {
                workspace_path = super::canonicalize_workspace_path(ws);
            }
        }

        let ts = val.get("timestamp").or_else(|| val.get("created_at")).and_then(|v| v.as_str()).map(|s| s.to_string());
        if created_at.is_none() && ts.is_some() {
            created_at = ts.clone();
        }
        if ts.is_some() {
            updated_at = ts.clone();
        }

        let role = val.get("role").and_then(|v| v.as_str()).unwrap_or("user");
        let content = val.get("content").and_then(|v| v.as_str()).unwrap_or("").trim();

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
    }))
}
