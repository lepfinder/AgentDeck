use rusqlite::{params, Connection, OpenFlags};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

use super::{
    needs_sync, record_sync_state, save_conversation_tx, ImporterStats, RawConversation, RawMessage,
};

/// 毫秒 epoch → RFC3339
fn ms_to_rfc3339(ms: i64) -> Option<String> {
    if ms <= 0 {
        return None;
    }
    chrono::DateTime::from_timestamp_millis(ms).map(|dt| dt.to_rfc3339())
}

fn json_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(|s| s.to_string())
}

/// 从 tool part 的 state 中提取可序列化参数（去掉超长 output）
fn tool_args_from_part(data: &Value) -> Option<String> {
    let state = data.get("state")?;
    let input = state.get("input").cloned().unwrap_or(Value::Null);
    let status = json_str(state, "status");
    let obj = serde_json::json!({
        "input": input,
        "status": status,
    });
    serde_json::to_string(&obj).ok()
}

fn tool_content_from_part(data: &Value) -> String {
    let state = data.get("state");
    if let Some(desc) = state
        .and_then(|s| s.get("input"))
        .and_then(|i| i.get("description"))
        .and_then(|d| d.as_str())
    {
        return desc.to_string();
    }
    if let Some(out) = state.and_then(|s| s.get("output")).and_then(|o| o.as_str()) {
        let t = out.trim();
        if t.chars().count() > 400 {
            let truncated: String = t.chars().take(400).collect();
            return format!("{}…", truncated);
        }
        return t.to_string();
    }
    String::new()
}

pub fn sync(conn: &Connection, incremental: bool) -> ImporterStats {
    let mut stats = ImporterStats {
        app: "MiMo".to_string(),
        new_count: 0,
        updated_count: 0,
        skipped_count: 0,
        error_count: 0,
    };

    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return stats,
    };

    let db_path = home.join(".local/share/mimocode/mimocode.db");
    if !db_path.is_file() {
        return stats;
    }

    if incremental && !needs_sync(conn, &db_path, true) {
        stats.skipped_count += 1;
        return stats;
    }

    match sync_mimocode_db(conn, &db_path) {
        Ok((n, u, s)) => {
            record_sync_state(conn, &db_path, "mimo:db", "mimocode_db");
            stats.new_count += n;
            stats.updated_count += u;
            stats.skipped_count += s;
        }
        Err(e) => {
            eprintln!("[MiMo Importer] mimocode.db 同步失败: {}", e);
            stats.error_count += 1;
        }
    }

    stats
}

fn sync_mimocode_db(
    conn: &Connection,
    db_path: &Path,
) -> Result<(u32, u32, u32), Box<dyn std::error::Error>> {
    let mimo = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    let mut skipped = 0u32;

    let mut existing_map: HashMap<String, (Option<String>, i64)> = HashMap::new();
    if let Ok(mut exist_stmt) =
        conn.prepare("SELECT id, updated_at, message_count FROM conversations WHERE id LIKE 'mimo:%'")
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

    let mut session_stmt = mimo.prepare(
        "SELECT id, title, directory, time_created, time_updated
         FROM session
         ORDER BY time_updated DESC",
    )?;
    let sessions: Vec<(String, String, String, i64, i64)> = session_stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1).unwrap_or_default(),
                row.get::<_, String>(2).unwrap_or_default(),
                row.get::<_, i64>(3).unwrap_or(0),
                row.get::<_, i64>(4).unwrap_or(0),
            ))
        })?
        .flatten()
        .collect();
    drop(session_stmt);

    let mut new_cnt = 0u32;
    let mut updated_cnt = 0u32;

    for (raw_id, title, directory, created_ms, updated_ms) in sessions {
        let cid = format!("mimo:{}", raw_id);

        let created_at = ms_to_rfc3339(created_ms);
        let updated_at = ms_to_rfc3339(updated_ms).or_else(|| created_at.clone());

        if let Some((prev_up, _)) = existing_map.get(&cid) {
            if prev_up == &updated_at {
                skipped += 1;
                continue;
            }
        }

        let messages = match load_messages(&mimo, &raw_id) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[MiMo Importer] 读取消息失败 {}: {}", cid, e);
                continue;
            }
        };
        if messages.is_empty() {
            skipped += 1;
            continue;
        }

        let mut derived_title = title.trim().to_string();
        if derived_title.is_empty() {
            if let Some(first_user) = messages.iter().find(|m| m.role == "user") {
                derived_title = first_user.content.chars().take(60).collect();
            }
        }
        if derived_title.is_empty() {
            derived_title = format!("MiMo 会话 {}", &raw_id[..raw_id.len().min(8)]);
        }

        let workspace_path = super::canonicalize_workspace_path(&directory);

        let conv = RawConversation {
            id: cid.clone(),
            title: derived_title,
            workspace_path,
            source_app: "mimo".to_string(),
            created_at,
            updated_at,
            parse_status: "ok".to_string(),
            source_types: vec!["mimo".to_string()],
            messages,
            artifacts: Vec::new(),
        };

        match save_conversation_tx(conn, &conv) {
            Ok(is_new) => {
                if is_new {
                    new_cnt += 1;
                } else {
                    updated_cnt += 1;
                }
            }
            Err(e) => {
                eprintln!("[MiMo Importer] 保存失败 {}: {}", cid, e);
            }
        }
    }

    Ok((new_cnt, updated_cnt, skipped))
}

/// 只导入主对话（agent_id = main），子 Agent 轨迹不进入时间线
fn load_messages(mimo: &Connection, session_id: &str) -> Result<Vec<RawMessage>, rusqlite::Error> {
    let mut msg_rows = Vec::new();
    {
        let mut stmt = mimo.prepare(
            "SELECT id, agent_id, time_created, data
             FROM message
             WHERE session_id = ?1
             ORDER BY time_created ASC, id ASC",
        )?;
        let rows = stmt.query_map(params![session_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1).unwrap_or_else(|_| "main".into()),
                r.get::<_, i64>(2).unwrap_or(0),
                r.get::<_, String>(3).unwrap_or_else(|_| "{}".into()),
            ))
        })?;
        for row in rows.flatten() {
            msg_rows.push(row);
        }
    }

    let mut parts_by_msg: HashMap<String, Vec<Value>> = HashMap::new();
    {
        let mut stmt = mimo.prepare(
            "SELECT message_id, data
             FROM part
             WHERE session_id = ?1
             ORDER BY time_created ASC, id ASC",
        )?;
        let rows = stmt.query_map(params![session_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1).unwrap_or_else(|_| "{}".into()),
            ))
        })?;
        for (mid, raw) in rows.flatten() {
            if let Ok(v) = serde_json::from_str::<Value>(&raw) {
                parts_by_msg.entry(mid).or_default().push(v);
            }
        }
    }

    let mut out = Vec::new();
    let mut step_idx = 0i64;

    for (msg_id, agent_id, time_created, raw_data) in msg_rows {
        if agent_id != "main" {
            continue;
        }
        let data: Value = serde_json::from_str(&raw_data).unwrap_or(Value::Null);
        let role = json_str(&data, "role").unwrap_or_else(|| "assistant".to_string());
        let model = json_str(&data, "modelID")
            .or_else(|| data.get("model").and_then(|m| json_str(m, "modelID")))
            .or_else(|| json_str(&data, "providerID"));
        let created_at = ms_to_rfc3339(time_created);

        let parts = parts_by_msg.get(&msg_id).cloned().unwrap_or_default();

        let mut text_chunks: Vec<String> = Vec::new();
        let mut thinking_chunks: Vec<String> = Vec::new();
        let mut tools: Vec<(String, Option<String>, String)> = Vec::new();

        for p in &parts {
            match json_str(p, "type").as_deref() {
                Some("text") => {
                    if let Some(t) = json_str(p, "text") {
                        let t = t.trim();
                        if !t.is_empty() {
                            text_chunks.push(t.to_string());
                        }
                    }
                }
                Some("reasoning") => {
                    if let Some(t) = json_str(p, "text") {
                        let t = t.trim();
                        if !t.is_empty() {
                            thinking_chunks.push(t.to_string());
                        }
                    }
                }
                Some("tool") => {
                    let name = json_str(p, "tool").unwrap_or_else(|| "tool".to_string());
                    let args = tool_args_from_part(p);
                    let content = tool_content_from_part(p);
                    tools.push((name, args, content));
                }
                // step-start / step-finish / patch / file 暂不映射
                _ => {}
            }
        }

        let content = text_chunks.join("\n\n");
        let thinking = if thinking_chunks.is_empty() {
            None
        } else {
            Some(thinking_chunks.join("\n\n"))
        };

        if content.is_empty() && thinking.is_none() && tools.is_empty() {
            continue;
        }

        // 主消息：正文 + thinking
        if !content.is_empty() || thinking.is_some() {
            out.push(RawMessage {
                step_index: step_idx,
                role: role.clone(),
                message_type: "text".to_string(),
                content,
                thinking,
                created_at: created_at.clone(),
                model_name: model.clone(),
                tool_name: None,
                tool_args: None,
                duration_ms: None,
                token_count: None,
                credit: None,
                images: None,
            });
            step_idx += 1;
        }

        // 工具调用各自成行
        for (name, args, tool_content) in tools {
            out.push(RawMessage {
                step_index: step_idx,
                role: if role == "user" {
                    "assistant".to_string()
                } else {
                    role.clone()
                },
                message_type: "tool_call".to_string(),
                content: tool_content,
                thinking: None,
                created_at: created_at.clone(),
                model_name: model.clone(),
                tool_name: Some(name),
                tool_args: args,
                duration_ms: None,
                token_count: None,
                credit: None,
                images: None,
            });
            step_idx += 1;
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ms_to_rfc3339() {
        let s = ms_to_rfc3339(1789110437838).unwrap();
        assert!(s.starts_with("2026-") || s.starts_with("202"));
        assert!(ms_to_rfc3339(0).is_none());
    }

    #[test]
    fn test_tool_args_shape() {
        let v: Value = serde_json::from_str(
            r#"{"type":"tool","tool":"Bash","state":{"status":"completed","input":{"command":"ls"},"output":"a\nb"}}"#,
        )
        .unwrap();
        let args = tool_args_from_part(&v).unwrap();
        assert!(args.contains("ls"));
        assert!(args.contains("completed"));
        let content = tool_content_from_part(&v);
        assert_eq!(content, "a\nb");
    }

    /// 对本机真实 mimocode.db 做一次端到端试跑（默认跳过）
    #[test]
    #[ignore]
    fn test_sync_real_mimocode_db() {
        let home = dirs::home_dir().unwrap();
        let db_path = home.join(".local/share/mimocode/mimocode.db");
        assert!(db_path.is_file(), "mimocode.db not found at {:?}", db_path);

        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();

        let stats = super::sync(&conn, false);
        println!(
            "sync stats: new={} updated={} skipped={} errors={}",
            stats.new_count, stats.updated_count, stats.skipped_count, stats.error_count
        );
        assert_eq!(stats.error_count, 0);

        let (total, user_msgs, mimo_cnt): (i64, i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(user_message_count),0),
                        SUM(CASE WHEN source_app='mimo' THEN 1 ELSE 0 END)
                 FROM conversations",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        println!("archived conversations={} user_msgs={} mimo={}", total, user_msgs, mimo_cnt);
        assert!(mimo_cnt > 0, "expected at least one mimo conversation");

        let sample: Option<(String, String, String)> = conn
            .query_row(
                "SELECT title, workspace_path, source_app FROM conversations WHERE id LIKE 'mimo:%' LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok();
        println!("sample: {:?}", sample);
        assert!(sample.is_some());
    }
}
