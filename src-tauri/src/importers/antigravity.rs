use regex::Regex;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use super::{
    needs_sync, record_sync_state, save_conversation_tx, ImporterStats, RawArtifact,
    RawConversation, RawMessage, RawUsageRecord,
};

const AG_PARSER_REV: &str = "ag-dual-dir-v7";
const AG_PARSER_REV_KEY: &str = "agentdeck:ag_parser_rev";

struct SessionCandidate {
    cid: String,
    session_dir: std::path::PathBuf,
    transcript_path: std::path::PathBuf,
    mtime: f64,
    size: u64,
    is_ide: bool,
    shadowed_paths: Vec<std::path::PathBuf>,
}

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

    let brain_dirs = [
        (home.join(".gemini/antigravity/brain"), false),
        (home.join(".gemini/antigravity-ide/brain"), true),
    ];

    let mut candidates: std::collections::HashMap<String, SessionCandidate> =
        std::collections::HashMap::new();

    for (brain_dir, is_ide) in &brain_dirs {
        if !brain_dir.is_dir() {
            continue;
        }

        let entries = match std::fs::read_dir(brain_dir) {
            Ok(e) => e,
            Err(_) => continue,
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

            let metadata = match transcript_path.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            let mtime = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);
            let size = metadata.len();

            if let Some(existing) = candidates.get_mut(&cid) {
                // 如果已存在同一 cid（跨目录重复），比较两者，择优选取更新/更大的
                let is_newer =
                    mtime > existing.mtime || (mtime == existing.mtime && size > existing.size);
                if is_newer {
                    let old_path = existing.transcript_path.clone();
                    existing.session_dir = path;
                    existing.transcript_path = transcript_path;
                    existing.mtime = mtime;
                    existing.size = size;
                    existing.is_ide = *is_ide;
                    existing.shadowed_paths.push(old_path);
                } else {
                    existing.shadowed_paths.push(transcript_path);
                }
            } else {
                candidates.insert(
                    cid.clone(),
                    SessionCandidate {
                        cid,
                        session_dir: path,
                        transcript_path,
                        mtime,
                        size,
                        is_ide: *is_ide,
                        shadowed_paths: Vec::new(),
                    },
                );
            }
        }
    }

    if candidates.is_empty() {
        return stats;
    }

    let force_reparse = ag_parser_rev_stale(conn);

    for cand in candidates.into_values() {
        if incremental && !force_reparse && !needs_sync(conn, &cand.transcript_path, true) {
            stats.skipped_count += 1;
            for shadowed in &cand.shadowed_paths {
                record_sync_state(conn, shadowed, &cand.cid, "antigravity_jsonl");
            }
            continue;
        }

        match parse_antigravity_session(
            &cand.cid,
            &cand.transcript_path,
            &cand.session_dir,
            cand.is_ide,
        ) {
            Ok(Some(conv)) => match save_conversation_tx(conn, &conv) {
                Ok(is_new) => {
                    record_sync_state(conn, &cand.transcript_path, &cand.cid, "antigravity_jsonl");
                    for shadowed in &cand.shadowed_paths {
                        record_sync_state(conn, shadowed, &cand.cid, "antigravity_jsonl");
                    }
                    if is_new {
                        stats.new_count += 1;
                    } else {
                        stats.updated_count += 1;
                    }
                }
                Err(e) => {
                    eprintln!("[Antigravity Importer] 保存失败 {}: {}", cand.cid, e);
                    stats.error_count += 1;
                }
            },
            Ok(None) => {
                stats.skipped_count += 1;
            }
            Err(e) => {
                eprintln!("[Antigravity Importer] 解析失败 {}: {}", cand.cid, e);
                stats.error_count += 1;
            }
        }
    }

    // 有错误的运行不标记 rev：否则一次不完整的运行会永久消耗强制重导
    if stats.error_count == 0 {
        mark_ag_synced(conn);
    }

    stats
}

fn ag_parser_rev_stale(conn: &Connection) -> bool {
    let stored: Option<String> = conn
        .query_row(
            "SELECT conversation_id FROM sync_state WHERE source_path = ?",
            rusqlite::params![AG_PARSER_REV_KEY],
            |r| r.get(0),
        )
        .ok();
    stored.as_deref() != Some(AG_PARSER_REV)
}

fn mark_ag_synced(conn: &Connection) {
    let now = chrono::Utc::now().to_rfc3339();
    let _ = conn.execute(
        r#"
        INSERT INTO sync_state (source_path, conversation_id, source_type, file_mtime, file_size, synced_at)
        VALUES (?1, ?2, 'ag_parser', 0, 0, ?3)
        ON CONFLICT(source_path) DO UPDATE SET
            conversation_id = excluded.conversation_id,
            synced_at = excluded.synced_at
        "#,
        rusqlite::params![AG_PARSER_REV_KEY, AG_PARSER_REV, now],
    );
}

/// 极简 protobuf wire 解码器。Antigravity 会话库的 token 数据藏在 protobuf blob 里，
/// 上游不随包发布 .proto，字段含义只能实测确立（与 Pulse AntigravityWire 同规则）：
/// 越界、超长 varint、废弃 group 类型任一出现，整条消息解码失败，绝不返回半截消息。
mod ag_wire {
    use std::collections::BTreeMap;

    #[derive(Debug, Clone)]
    pub enum WireValue {
        Varint(u64),
        Fixed64(u64),
        Length(Vec<u8>),
        Fixed32(u32),
    }

    #[derive(Debug, Clone, Default)]
    pub struct WireMessage {
        fields: BTreeMap<u64, Vec<WireValue>>,
    }

    impl WireMessage {
        fn first(&self, number: u64) -> Option<&WireValue> {
            self.fields.get(&number).and_then(|v| v.first())
        }

        pub fn varint(&self, number: u64) -> Option<u64> {
            match self.first(number) {
                Some(WireValue::Varint(v)) => Some(*v),
                _ => None,
            }
        }

        fn bytes(&self, number: u64) -> Option<&[u8]> {
            match self.first(number) {
                Some(WireValue::Length(b)) => Some(b),
                _ => None,
            }
        }

        pub fn string(&self, number: u64) -> Option<String> {
            let text = String::from_utf8_lossy(self.bytes(number)?).trim().to_string();
            if text.is_empty() { None } else { Some(text) }
        }

        pub fn nested(&self, number: u64) -> Option<WireMessage> {
            decode(self.bytes(number)?)
        }
    }

    pub fn decode(buf: &[u8]) -> Option<WireMessage> {
        let mut msg = WireMessage::default();
        let mut i = 0usize;
        while i < buf.len() {
            let tag = varint(buf, &mut i)?;
            let number = tag >> 3;
            if number == 0 {
                return None;
            }
            let value = match tag & 7 {
                0 => WireValue::Varint(varint(buf, &mut i)?),
                1 => {
                    if buf.len() - i < 8 { return None; }
                    let mut v = 0u64;
                    for off in 0..8 {
                        v |= (buf[i + off] as u64) << (8 * off);
                    }
                    i += 8;
                    WireValue::Fixed64(v)
                }
                2 => {
                    let len = varint(buf, &mut i)? as usize;
                    if len > buf.len() - i { return None; }
                    let v = buf[i..i + len].to_vec();
                    i += len;
                    WireValue::Length(v)
                }
                5 => {
                    if buf.len() - i < 4 { return None; }
                    let mut v = 0u32;
                    for off in 0..4 {
                        v |= (buf[i + off] as u32) << (8 * off);
                    }
                    i += 4;
                    WireValue::Fixed32(v)
                }
                // 3/4 是废弃的 group 类型
                _ => return None,
            };
            msg.fields.entry(number).or_default().push(value);
        }
        Some(msg)
    }

    fn varint(buf: &[u8], i: &mut usize) -> Option<u64> {
        let mut result = 0u64;
        let mut shift = 0u32;
        for _ in 0..10 {
            if *i >= buf.len() { return None; }
            let byte = buf[*i];
            *i += 1;
            if shift == 63 && byte > 1 { return None; }
            result |= ((byte & 0x7F) as u64) << shift;
            if byte & 0x80 == 0 {
                return Some(result);
            }
            shift += 7;
        }
        None
    }
}

/// {#1 秒, #2 纳秒}；非正或荒谬秒数不算时间
fn ag_wire_timestamp(msg: &ag_wire::WireMessage) -> Option<String> {
    let seconds = msg.varint(1)?;
    if seconds == 0 || seconds >= 1_000_000_000_000 {
        return None;
    }
    let nanos = msg.varint(2).unwrap_or(0).min(999_999_999) as u32;
    chrono::DateTime::from_timestamp(seconds as i64, nanos).map(|dt| dt.to_rfc3339())
}

/// 路由占位模型名：真实身份被路由层吞掉，需按 label / 单模型文件找回
const AG_ROUTING_MODEL: &str = "gemini-default";

/// Pulse 实测验证过的显示名 → 机器名映射。label 是服务端下发且可本地化的，
/// 未验证的 label 不猜。
fn ag_label_model(label: &str) -> Option<&'static str> {
    match label {
        "Gemini 3.5 Flash (Low)" => Some("gemini-3.5-flash-extra-low"),
        "Gemini 3.5 Flash (Medium)" => Some("gemini-3.5-flash-medium"),
        "Gemini 3.5 Flash (High)" => Some("gemini-3.5-flash-high"),
        _ => None,
    }
}

fn antigravity_db_path(cid: &str, home: &Path, is_ide: bool) -> Option<std::path::PathBuf> {
    let primary_dir = if is_ide {
        home.join(".gemini/antigravity-ide/conversations")
    } else {
        home.join(".gemini/antigravity/conversations")
    };
    let fallback_dir = if is_ide {
        home.join(".gemini/antigravity/conversations")
    } else {
        home.join(".gemini/antigravity-ide/conversations")
    };
    let primary = primary_dir.join(format!("{}.db", cid));
    if primary.is_file() {
        return Some(primary);
    }
    let fallback = fallback_dir.join(format!("{}.db", cid));
    if fallback.is_file() {
        return Some(fallback);
    }
    None
}

struct AgGeneration {
    idx: i64,
    model: Option<String>,
    label: Option<String>,
    explicit_ts: Option<String>,
    usage: Option<ag_wire::WireMessage>,
}

/// 从单个会话库解析 usage。字段含义经本机实测 + Pulse AntigravityCLIReader 交叉验证：
/// gen_metadata.data 的 chat(1).usage(4) 中 #2=新鲜输入、#5=缓存读、#9=输出、#10=thinking
/// （#9+#10 才是总输出，两者独立）、#11=responseId。#1 全文件恒为常量（疑似固定系统提示，
/// 含义未确立），坚决不读。proto3 语义下 0 值不序列化，字段缺失按 0 记。
/// 无整体 total 可对账，全部 is_partial。
fn load_antigravity_usage_from_db(
    db_path: &Path,
    fallback_created_at: Option<&str>,
) -> Vec<RawUsageRecord> {
    let mut records = Vec::new();
    let conn = match Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(_) => return records,
    };

    // steps(step_type=15) 提供 resp→时间 与 idx→时间 两张映射；显式时间戳常缺
    let mut ts_by_resp: HashMap<String, String> = HashMap::new();
    let mut ts_by_idx: HashMap<i64, String> = HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT metadata FROM steps WHERE step_type = 15") {
        if let Ok(rows) = stmt.query_map([], |row| {
            let blob: Vec<u8> = row.get(0)?;
            Ok(blob)
        }) {
            for blob in rows.flatten() {
                let Some(msg) = ag_wire::decode(&blob) else { continue };
                let Some(ts) = msg.nested(1).as_ref().and_then(ag_wire_timestamp) else {
                    continue;
                };
                if let Some(resp) = msg.nested(9).and_then(|m| m.string(11)) {
                    ts_by_resp.insert(resp, ts.clone());
                }
                if let Some(idx) = msg.nested(20).and_then(|m| m.varint(3)) {
                    ts_by_idx.insert(idx as i64, ts);
                }
            }
        }
    }

    let mut generations: Vec<AgGeneration> = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT idx, data FROM gen_metadata ORDER BY idx") {
        if let Ok(rows) = stmt.query_map([], |row| {
            let idx: i64 = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            Ok((idx, blob))
        }) {
            for (idx, blob) in rows.flatten() {
                let Some(msg) = ag_wire::decode(&blob) else { continue };
                let Some(chat) = msg.nested(1) else { continue };
                let explicit_ts = chat
                    .nested(9)
                    .and_then(|m| m.nested(4))
                    .as_ref()
                    .and_then(ag_wire_timestamp);
                generations.push(AgGeneration {
                    idx,
                    model: chat.string(19),
                    label: chat.string(21),
                    explicit_ts,
                    usage: chat.nested(4),
                });
            }
        }
    }

    // label→具体模型 与全文件模型集合，用于找回被路由占位吞掉的身份
    let mut models_by_label: HashMap<String, HashSet<String>> = HashMap::new();
    let mut models: HashSet<String> = HashSet::new();
    for g in &generations {
        let Some(m) = &g.model else { continue };
        if m == AG_ROUTING_MODEL {
            continue;
        }
        models.insert(m.clone());
        if let Some(label) = &g.label {
            models_by_label
                .entry(label.clone())
                .or_default()
                .insert(m.clone());
        }
    }
    let sole_model = if models.len() == 1 {
        models.into_iter().next()
    } else {
        None
    };

    let mut seen: HashSet<String> = HashSet::new();
    for g in generations {
        let Some(usage) = g.usage else { continue };
        let counter =
            |n: u64| -> i64 { usage.varint(n).unwrap_or(0).min(i64::MAX as u64) as i64 };
        let fresh = counter(2);
        let cache_read = counter(5);
        let output = counter(9);
        let reasoning = counter(10);
        if fresh + cache_read + output + reasoning == 0 {
            continue;
        }
        let resp = usage.string(11);
        let identity = match &resp {
            Some(r) => {
                if !seen.insert(r.clone()) {
                    continue;
                }
                r.clone()
            }
            None => format!("gen:{}", g.idx),
        };

        let occurred_at = g
            .explicit_ts
            .or_else(|| resp.as_ref().and_then(|r| ts_by_resp.get(r).cloned()))
            .or_else(|| ts_by_idx.get(&g.idx).cloned())
            .or_else(|| fallback_created_at.map(|s| s.to_string()));

        let model = if let Some(m) = &g.model {
            if m != AG_ROUTING_MODEL {
                Some(m.clone())
            } else {
                None
            }
        } else {
            None
        }
        .or_else(|| {
            g.label
                .as_ref()
                .and_then(|l| models_by_label.get(l))
                .filter(|ms| ms.len() == 1)
                .and_then(|ms| ms.iter().next().cloned())
        })
        .or_else(|| sole_model.clone())
        .or_else(|| g.label.as_deref().and_then(ag_label_model).map(|s| s.to_string()));

        records.push(RawUsageRecord {
            identity,
            agent: "antigravity".to_string(),
            model,
            input_tokens: Some(fresh),
            cache_read_tokens: Some(cache_read),
            cache_write_tokens: None,
            output_tokens: Some(output),
            reasoning_tokens: Some(reasoning),
            credit: None,
            occurred_at,
            is_partial: true,
        });
    }

    records
}

fn load_antigravity_db_usage(
    cid: &str,
    home: &Path,
    is_ide: bool,
    fallback_created_at: Option<&str>,
) -> Vec<RawUsageRecord> {
    match antigravity_db_path(cid, home, is_ide) {
        Some(p) => load_antigravity_usage_from_db(&p, fallback_created_at),
        None => Vec::new(),
    }
}

fn load_antigravity_db_media(
    cid: &str,
    home: &Path,
    is_ide: bool,
) -> std::collections::HashMap<i64, Vec<String>> {
    let mut mapping = std::collections::HashMap::new();
    let db_path = match antigravity_db_path(cid, home, is_ide) {
        Some(p) => p,
        None => return mapping,
    };

    let conn = match Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(_) => return mapping,
    };

    let Ok(media_re) = Regex::new(r#"(/Users[^\x00-\x1f\s"'`<>]*?\.(?:png|jpe?g|webp|gif|svg))"#) else {
        return mapping;
    };

    if let Ok(mut stmt) = conn.prepare("SELECT idx, step_payload FROM steps WHERE step_payload IS NOT NULL ORDER BY idx") {
        if let Ok(rows) = stmt.query_map([], |row| {
            let idx: i64 = row.get(0)?;
            let payload: Vec<u8> = row.get(1)?;
            Ok((idx, payload))
        }) {
            for r in rows.flatten() {
                let text = String::from_utf8_lossy(&r.1);
                let mut list = Vec::new();
                for caps in media_re.captures_iter(&text) {
                    if let Some(m) = caps.get(1) {
                        let p = m.as_str().to_string();
                        if !list.contains(&p) {
                            list.push(p);
                        }
                    }
                }
                if !list.is_empty() {
                    mapping.insert(r.0, list);
                }
            }
        }
    }

    mapping
}

fn unwrap_json_str(val: &Value) -> String {
    match val {
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
                if let Ok(Value::String(unquoted)) = serde_json::from_str::<Value>(trimmed) {
                    return unquoted;
                }
                let inner = &trimmed[1..trimmed.len() - 1];
                inner
                    .replace("\\n", "\n")
                    .replace("\\r", "")
                    .replace("\\t", "\t")
                    .replace("\\\"", "\"")
                    .replace("\\\\", "\\")
            } else {
                s.replace("\\n", "\n").replace("\\r", "").replace("\\\"", "\"")
            }
        }
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn extract_markdown_title(content: &str, default_name: &str) -> String {
    for line in content.lines() {
        let trimmed = line.trim().trim_start_matches('"').trim();
        if trimmed.starts_with('#') {
            let title = trimmed
                .trim_start_matches('#')
                .trim()
                .trim_matches(|c| c == '*' || c == '`' || c == '"' || c == '[' || c == ']')
                .trim();
            if !title.is_empty() {
                return title.to_string();
            }
        }
    }
    match default_name {
        name if name.starts_with("implementation_plan") => "实施计划 (Implementation Plan)".to_string(),
        name if name.starts_with("walkthrough") => "成果走查 (Walkthrough)".to_string(),
        name if name.starts_with("task") => "任务清单 (Task)".to_string(),
        _ => default_name.to_string(),
    }
}

#[derive(Debug, Clone)]
struct RawArtifactEntry {
    #[allow(dead_code)]
    step_index: i64,
    base_file_name: String,
    file_path: String,
    title: String,
    summary: Option<String>,
    content: String,
    user_facing: bool,
    request_feedback: bool,
    created_at: Option<String>,
}

fn load_antigravity_artifacts(session_dir: &Path) -> Vec<RawArtifact> {
    let mut artifacts = Vec::new();
    let entries = match std::fs::read_dir(session_dir) {
        Ok(e) => e,
        Err(_) => return artifacts,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };

        // 仅收集 .md 文件，且排除隐藏文件及特定系统文件
        if !file_name.ends_with(".md") || file_name.starts_with('.') {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if content.trim().is_empty() {
            continue;
        }

        let title = extract_markdown_title(&content, &file_name);

        // 尝试读取伴生 metadata.json
        let meta_path = session_dir.join(format!("{}.metadata.json", file_name));
        let mut summary = None;
        let mut user_facing = true;
        let mut request_feedback = false;
        let mut updated_at = None;

        if meta_path.is_file() {
            if let Ok(meta_str) = std::fs::read_to_string(&meta_path) {
                if let Ok(meta_val) = serde_json::from_str::<Value>(&meta_str) {
                    summary = meta_val
                        .get("summary")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    user_facing = meta_val
                        .get("userFacing")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                    request_feedback = meta_val
                        .get("requestFeedback")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    updated_at = meta_val
                        .get("updatedAt")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                }
            }
        }

        if updated_at.is_none() {
            if let Ok(meta) = path.metadata() {
                if let Ok(mtime) = meta.modified() {
                    let dt: chrono::DateTime<chrono::Utc> = mtime.into();
                    updated_at = Some(dt.to_rfc3339());
                }
            }
        }

        artifacts.push(RawArtifact {
            file_name,
            file_path: path.to_string_lossy().to_string(),
            title,
            summary,
            content,
            user_facing,
            request_feedback,
            created_at: updated_at.clone(),
            updated_at,
        });
    }

    // 优先级排序：implementation_plan.md > task.md > walkthrough.md > 其他
    artifacts.sort_by_key(|a| match a.file_name.as_str() {
        "implementation_plan.md" => 1,
        "task.md" => 2,
        "walkthrough.md" => 3,
        _ => 4,
    });

    artifacts
}

fn parse_antigravity_session(
    cid: &str,
    transcript_path: &Path,
    session_dir: &Path,
    is_ide: bool,
) -> Result<Option<RawConversation>, Box<dyn std::error::Error>> {
    let file = File::open(transcript_path)?;
    let reader = BufReader::new(file);

    let home = dirs::home_dir().unwrap_or_default();
    let db_media = load_antigravity_db_media(cid, &home, is_ide);

    let user_req_re = Regex::new(r"(?s)<USER_REQUEST>\s*(.*?)\s*</USER_REQUEST>")?;
    let artifact_uri_re = Regex::new(r#"Comments on artifact URI:\s*file://([^\s\n\r"']+)"#)?;
    // 禁止吃进 JSON 引号/反斜杠/方括号，避免 toolAction 或 @[path] 尾 ] 拼进路径
    let workspace_re = Regex::new(r#"(/[^\s\n\r"'\\\[\]]+)\s*->"#)?;
    let active_doc_re = Regex::new(r#"Active Document:\s*([^\s("'\\\[\]]+)"#)?;
    let at_img_re = Regex::new(r"@\[?(/[^\s\]\)]+\.(?:png|jpe?g|gif|webp|svg))\]?")?;
    let file_uri_img_re = Regex::new(r#"file://(/[^\s"'\n\r\(\)]+\.(?:png|jpe?g|gif|webp|svg))"#)?;
    let media_re = Regex::new(r#"(/Users[^\x00-\x1f\s"'`<>]*?media[_\-\w]*\.(?:png|jpe?g|webp|gif|svg))"#)?;
    let user_uploaded_re = Regex::new(r#"(/Users[^\x00-\x1f\s"'`<>]*?/\.user_uploaded/[^\s"'`<>]+)"#)?;

    let mut messages = Vec::new();
    let mut title = String::new();
    let mut workspace_path = String::new();
    let mut created_at = None;
    let mut updated_at = None;
    let mut step_idx = 0i64;
    let mut transcript_artifact_entries: Vec<RawArtifactEntry> = Vec::new();

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
        let raw_step_index = json_val.get("step_index").and_then(|v| v.as_i64()).unwrap_or(-1);
        let raw_content = json_val
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let timestamp = json_val
            .get("created_at")
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
                    if let Some(clean) = super::sanitize_extracted_path(m.as_str()) {
                        workspace_path = super::canonicalize_workspace_path(&clean);
                    }
                }
            } else if let Some(caps) = active_doc_re.captures(raw_content) {
                if let Some(m) = caps.get(1) {
                    if let Some(clean) = super::sanitize_extracted_path(m.as_str()) {
                        workspace_path = super::canonicalize_workspace_path(&clean);
                    }
                }
            } else if raw_content.contains("/workspace/") {
                for line in raw_content.lines() {
                    if let Some(idx) = line.find("/workspace/") {
                        let sub = &line[idx..];
                        let token = sub
                            .split(|c: char| c.is_whitespace() || matches!(c, ']' | '[' | '"' | '\'' | ')' | '('))
                            .next()
                            .unwrap_or("");
                        if let Some(clean) = super::sanitize_extracted_path(token) {
                            let cand = super::canonicalize_workspace_path(&clean);
                            if !cand.is_empty() {
                                workspace_path = cand;
                                break;
                            }
                        }
                    }
                }
            }
        }

        match step_type {
            "USER_INPUT" => {
                let mut user_text = if let Some(caps) = user_req_re.captures(raw_content) {
                    caps.get(1)
                        .map(|m| m.as_str().trim().to_string())
                        .unwrap_or_default()
                } else {
                    raw_content.trim().to_string()
                };

                // 处理 Artifact 审批动作 (Proceed / Approve)
                let is_approved = raw_content.contains("The user has approved this document.");
                let artifact_name = artifact_uri_re.captures(raw_content).and_then(|c| {
                    c.get(1).map(|m| {
                        Path::new(m.as_str())
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("实施计划")
                            .to_string()
                    })
                });

                if is_approved {
                    let doc_name = artifact_name.as_deref().unwrap_or("实施计划");
                    if user_text.is_empty() {
                        let mut comment_lines = Vec::new();
                        let body = if let Some(idx) = raw_content.find("<USER_REQUEST>") {
                            &raw_content[..idx]
                        } else {
                            raw_content
                        };
                        for line in body.lines() {
                            let s = line.trim();
                            if s.is_empty()
                                || s.starts_with("Comments on artifact URI:")
                                || s == "The user has approved this document."
                            {
                                continue;
                            }
                            comment_lines.push(s);
                        }
                        let comment = comment_lines.join(" ").trim().to_string();
                        if !comment.is_empty() {
                            user_text = format!("✅ 已批准 {}：{}", doc_name, comment);
                        } else {
                            user_text = format!("✅ 已批准 {}", doc_name);
                        }
                    } else {
                        user_text = format!("✅ 已批准 {}: {}", doc_name, user_text);
                    }
                }

                if title.is_empty() && !user_text.is_empty() {
                    title = user_text.chars().take(80).collect();
                }

                // 提取附图 (结合文本正则与 conversations db step_payload)
                let mut img_paths = Vec::new();

                // 1. 从 conversations/{cid}.db 中关联此 step_index 的图片
                if let Some(db_imgs) = db_media.get(&raw_step_index) {
                    for p in db_imgs {
                        if !img_paths.contains(p) {
                            img_paths.push(p.clone());
                        }
                    }
                }

                // 2. 从 raw_content 文本中正则匹配
                for caps in at_img_re.captures_iter(raw_content) {
                    if let Some(m) = caps.get(1) {
                        let p = m.as_str().to_string();
                        if !img_paths.contains(&p) {
                            img_paths.push(p);
                        }
                    }
                }
                for caps in file_uri_img_re.captures_iter(raw_content) {
                    if let Some(m) = caps.get(1) {
                        let p = m.as_str().to_string();
                        if !img_paths.contains(&p) {
                            img_paths.push(p);
                        }
                    }
                }
                for caps in media_re.captures_iter(raw_content) {
                    if let Some(m) = caps.get(1) {
                        let p = m.as_str().to_string();
                        if !img_paths.contains(&p) {
                            img_paths.push(p);
                        }
                    }
                }
                for caps in user_uploaded_re.captures_iter(raw_content) {
                    if let Some(m) = caps.get(1) {
                        let p = m.as_str().to_string();
                        if !img_paths.contains(&p) {
                            img_paths.push(p);
                        }
                    }
                }

                let mut image_entries = Vec::new();
                for p in img_paths {
                    let path_obj = Path::new(&p);
                    if path_obj.is_file() {
                        let src_uri = crate::media_archive::archive_image_file(&path_obj, "antigravity", cid)
                            .unwrap_or_else(|| {
                                let encoded = urlencoding::encode(&p);
                                format!("/ag-image?path={}", encoded)
                            });
                        image_entries.push(serde_json::json!({
                            "src": src_uri,
                            "original_path": p,
                            "width": null,
                            "height": null
                        }));
                    }
                }

                let images_json = if !image_entries.is_empty() {
                    Some(serde_json::to_string(&image_entries).unwrap_or_default())
                } else {
                    None
                };

                // 若用户输入仍为空且无附图，尝试剥离 <ADDITIONAL_METADATA> 提取有效前置内容
                if user_text.is_empty() && image_entries.is_empty() {
                    let fallback = if let Some(idx) = raw_content.find("<ADDITIONAL_METADATA>") {
                        raw_content[..idx].trim()
                    } else {
                        raw_content.trim()
                    };
                    let clean_fallback = fallback
                        .replace("<USER_REQUEST>", "")
                        .replace("</USER_REQUEST>", "")
                        .trim()
                        .to_string();
                    if !clean_fallback.is_empty() {
                        user_text = clean_fallback;
                    }
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
                    credit: None,
                    images: images_json,
                });
                step_idx += 1;
            }
            "PLANNER_RESPONSE" => {
                let tool_calls_json = json_val.get("tool_calls").map(|v| v.to_string());
                let thinking = json_val
                    .get("thinking")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                // 提取本次回复中可能生成的实施方案或交付产物 (write_to_file)
                if let Some(tool_calls_arr) = json_val.get("tool_calls").and_then(|v| v.as_array()) {
                    for tc in tool_calls_arr {
                        let fn_name = tc.get("name").and_then(|v| v.as_str()).unwrap_or_default();
                        if fn_name == "write_to_file" {
                            if let Some(args_obj) = tc.get("args") {
                                let target_file_val = args_obj.get("TargetFile").unwrap_or(&Value::Null);
                                let target_file = unwrap_json_str(target_file_val);
                                let file_name = Path::new(&target_file)
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or_default()
                                    .to_string();

                                let has_artifact_meta = args_obj.get("ArtifactMetadata").is_some();
                                let is_target_artifact = file_name.ends_with(".md") && (
                                    has_artifact_meta
                                        || file_name.starts_with("implementation_plan")
                                        || file_name.starts_with("walkthrough")
                                        || file_name.starts_with("task")
                                        || target_file.contains("/brain/")
                                        || target_file.contains("/.gemini/")
                                );

                                if is_target_artifact {
                                    let code_val = args_obj.get("CodeContent").unwrap_or(&Value::Null);
                                    let content = unwrap_json_str(code_val);
                                    if !content.trim().is_empty() {
                                        let mut summary = None;
                                        let mut user_facing = true;
                                        let mut request_feedback = false;

                                        if let Some(meta_val) = args_obj.get("ArtifactMetadata") {
                                            let meta_obj = match meta_val {
                                                Value::Object(_) => Some(meta_val.clone()),
                                                Value::String(_) => {
                                                    let unquoted = unwrap_json_str(meta_val);
                                                    serde_json::from_str::<Value>(&unquoted).ok()
                                                }
                                                _ => None,
                                            };
                                            if let Some(m) = meta_obj {
                                                summary = m
                                                    .get("Summary")
                                                    .or_else(|| m.get("summary"))
                                                    .and_then(|v| v.as_str())
                                                    .map(|s| s.to_string());
                                                user_facing = m
                                                    .get("UserFacing")
                                                    .or_else(|| m.get("userFacing"))
                                                    .and_then(|v| v.as_bool())
                                                    .unwrap_or(true);
                                                request_feedback = m
                                                    .get("RequestFeedback")
                                                    .or_else(|| m.get("requestFeedback"))
                                                    .and_then(|v| v.as_bool())
                                                    .unwrap_or(false);
                                            }
                                        }

                                        let title = extract_markdown_title(&content, &file_name);
                                        let step_created_at = json_val
                                            .get("created_at")
                                            .or_else(|| json_val.get("timestamp"))
                                            .and_then(|v| v.as_str())
                                            .map(|s| s.to_string());

                                        transcript_artifact_entries.push(RawArtifactEntry {
                                            step_index: step_idx,
                                            base_file_name: file_name,
                                            file_path: target_file,
                                            title,
                                            summary,
                                            content,
                                            user_facing,
                                            request_feedback,
                                            created_at: step_created_at,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }

                messages.push(RawMessage {
                    step_index: step_idx,
                    role: "assistant".to_string(),
                    message_type: if tool_calls_json.is_some() {
                        "tool_call".to_string()
                    } else {
                        "text".to_string()
                    },
                    content: raw_content.to_string(),
                    thinking,
                    created_at: timestamp,
                    model_name: Some("Gemini".to_string()),
                    tool_name: None,
                    tool_args: tool_calls_json,
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

    let source_types = if is_ide {
        vec!["antigravity".to_string(), "antigravity-ide".to_string()]
    } else {
        vec!["antigravity".to_string()]
    };

    // 将 transcript_artifact_entries 按 base_file_name 组织并生成版本化产物列表
    let mut artifacts_by_name: std::collections::BTreeMap<String, Vec<RawArtifactEntry>> =
        std::collections::BTreeMap::new();

    for entry in transcript_artifact_entries {
        let list = artifacts_by_name
            .entry(entry.base_file_name.clone())
            .or_default();
        // 若相邻连续写入的内容完全相同，过滤冗余
        if let Some(last) = list.last() {
            if last.content == entry.content {
                continue;
            }
        }
        list.push(entry);
    }

    let mut final_artifacts: Vec<RawArtifact> = Vec::new();
    let mut seen_file_names = std::collections::HashSet::new();

    for (base_name, entries) in artifacts_by_name {
        let total = entries.len();
        let stem = base_name.strip_suffix(".md").unwrap_or(&base_name);

        for (idx, entry) in entries.into_iter().enumerate() {
            let file_name = if total == 1 || idx == total - 1 {
                // 最新版本或唯一定义保留原生文件名
                base_name.clone()
            } else {
                // 历史版本加上 .v{序号}.md 后缀
                format!("{}.v{}.md", stem, idx + 1)
            };

            seen_file_names.insert(file_name.clone());
            final_artifacts.push(RawArtifact {
                file_name,
                file_path: entry.file_path,
                title: entry.title,
                summary: entry.summary,
                content: entry.content,
                user_facing: entry.user_facing,
                request_feedback: entry.request_feedback,
                created_at: entry.created_at.clone(),
                updated_at: entry.created_at,
            });
        }
    }

    // 补充物理磁盘上存在但 transcript 中未记录的其它独立 artifact 文件
    for disk_art in load_antigravity_artifacts(session_dir) {
        if !seen_file_names.contains(&disk_art.file_name) {
            seen_file_names.insert(disk_art.file_name.clone());
            final_artifacts.push(disk_art);
        }
    }

    // 按业务类型与时序排序：实施计划(最新>历史倒序) > 任务清单 > 成果复盘(最新>历史倒序) > 其它
    final_artifacts.sort_by(|a, b| {
        let type_score = |name: &str| -> i32 {
            if name == "implementation_plan.md" {
                1
            } else if name.starts_with("implementation_plan") {
                2
            } else if name == "task.md" {
                3
            } else if name.starts_with("task") {
                4
            } else if name == "walkthrough.md" {
                5
            } else if name.starts_with("walkthrough") {
                6
            } else {
                7
            }
        };

        let sa = type_score(&a.file_name);
        let sb = type_score(&b.file_name);
        if sa != sb {
            return sa.cmp(&sb);
        }

        let time_a = a.created_at.as_deref().unwrap_or("");
        let time_b = b.created_at.as_deref().unwrap_or("");
        if time_a != time_b {
            return time_b.cmp(time_a);
        }

        b.file_name.cmp(&a.file_name)
    });

    let usage_records = load_antigravity_db_usage(cid, &home, is_ide, created_at.as_deref());

    Ok(Some(RawConversation {
        id: cid.to_string(),
        title,
        workspace_path,
        source_app: "antigravity".to_string(),
        created_at,
        updated_at,
        parse_status: "ok".to_string(),
        source_types,
        messages,
        artifacts: final_artifacts,
        usage_records,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_parse_antigravity_session() {
        let tmp_dir = std::env::temp_dir().join(format!("test_ag_session_{}", std::process::id()));
        let logs_dir = tmp_dir.join(".system_generated/logs");
        std::fs::create_dir_all(&logs_dir).unwrap();

        let transcript_path = logs_dir.join("transcript.jsonl");
        let mut file = File::create(&transcript_path).unwrap();
        writeln!(
            file,
            r#"{{"type":"USER_INPUT","step_index":0,"content":"<USER_REQUEST>测试提问内容</USER_REQUEST>","timestamp":"2026-09-07T10:00:00Z"}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"type":"PLANNER_RESPONSE","step_index":1,"content":"这是助手的回答","timestamp":"2026-09-07T10:00:05Z"}}"#
        )
        .unwrap();
        drop(file);

        let conv = parse_antigravity_session("test-cid-123", &transcript_path, &tmp_dir, true)
            .unwrap()
            .unwrap();

        let _ = std::fs::remove_dir_all(&tmp_dir);

        assert_eq!(conv.id, "test-cid-123");
        assert_eq!(conv.source_app, "antigravity");
        assert!(conv.source_types.contains(&"antigravity-ide".to_string()));
        assert_eq!(conv.messages.len(), 2);
        assert_eq!(conv.messages[0].role, "user");
        assert_eq!(conv.messages[0].content, "测试提问内容");
        assert_eq!(conv.messages[1].role, "assistant");
        assert_eq!(conv.messages[1].content, "这是助手的回答");
    }

    #[test]
    fn test_parse_antigravity_approval_message() {
        let tmp_dir = std::env::temp_dir().join(format!("test_ag_approval_{}", std::process::id()));
        let logs_dir = tmp_dir.join(".system_generated/logs");
        std::fs::create_dir_all(&logs_dir).unwrap();

        let transcript_path = logs_dir.join("transcript.jsonl");
        let mut file = File::create(&transcript_path).unwrap();
        // 模拟批准 implementation_plan.md 且 <USER_REQUEST> 为空
        writeln!(
            file,
            r#"{{"type":"USER_INPUT","step_index":0,"content":"Comments on artifact URI: file:///Users/test/.gemini/brain/cid/implementation_plan.md\n\nThe user has approved this document.\n\n<USER_REQUEST>\n\n</USER_REQUEST>\n<ADDITIONAL_METADATA>\nlocal time\n</ADDITIONAL_METADATA>","timestamp":"2026-09-08T10:00:00Z"}}"#
        )
        .unwrap();
        drop(file);

        let conv = parse_antigravity_session("test-cid-456", &transcript_path, &tmp_dir, true)
            .unwrap()
            .unwrap();

        let _ = std::fs::remove_dir_all(&tmp_dir);

        assert_eq!(conv.messages.len(), 1);
        assert_eq!(conv.messages[0].role, "user");
        assert_eq!(conv.messages[0].content, "✅ 已批准 implementation_plan.md");
    }

    #[test]
    fn test_parse_antigravity_artifacts() {
        let tmp_dir = std::env::temp_dir().join(format!("test_ag_art_{}", std::process::id()));
        let logs_dir = tmp_dir.join(".system_generated/logs");
        std::fs::create_dir_all(&logs_dir).unwrap();

        let transcript_path = logs_dir.join("transcript.jsonl");
        let mut file = File::create(&transcript_path).unwrap();
        writeln!(
            file,
            r#"{{"type":"USER_INPUT","step_index":0,"content":"<USER_REQUEST>查看方案</USER_REQUEST>","timestamp":"2026-09-08T10:00:00Z"}}"#
        )
        .unwrap();
        drop(file);

        // 创建 implementation_plan.md 及 implementation_plan.md.metadata.json
        let plan_md_path = tmp_dir.join("implementation_plan.md");
        std::fs::write(&plan_md_path, "# 架构重构实施计划\n\n这是正文内容").unwrap();

        let meta_path = tmp_dir.join("implementation_plan.md.metadata.json");
        std::fs::write(&meta_path, r#"{"summary":"实施方案摘要说明","updatedAt":"2026-09-08T10:05:00Z","requestFeedback":true,"userFacing":true}"#).unwrap();

        let conv = parse_antigravity_session("test-cid-art", &transcript_path, &tmp_dir, true)
            .unwrap()
            .unwrap();

        let _ = std::fs::remove_dir_all(&tmp_dir);

        assert_eq!(conv.artifacts.len(), 1);
        let art = &conv.artifacts[0];
        assert_eq!(art.file_name, "implementation_plan.md");
        assert_eq!(art.title, "架构重构实施计划");
        assert_eq!(art.summary.as_deref(), Some("实施方案摘要说明"));
        assert!(art.content.contains("这是正文内容"));
        assert!(art.request_feedback);
        assert!(art.user_facing);
    }

    #[test]
    fn test_dual_dir_candidate_deduplication() {
        let mut candidates: std::collections::HashMap<String, SessionCandidate> =
            std::collections::HashMap::new();

        let cid = "shared-uuid-001".to_string();

        // 模拟从 IDE 目录扫描到的旧版本
        let ide_cand = SessionCandidate {
            cid: cid.clone(),
            session_dir: std::path::PathBuf::from("/mock/ide/brain/shared-uuid-001"),
            transcript_path: std::path::PathBuf::from("/mock/ide/brain/shared-uuid-001/transcript.jsonl"),
            mtime: 1000.0,
            size: 500,
            is_ide: true,
            shadowed_paths: Vec::new(),
        };
        candidates.insert(cid.clone(), ide_cand);

        // 模拟从桌面端目录扫描到的更新版本（mtime 更大）
        let app_cand_mtime = 2000.0;
        let app_cand_size = 800;
        let app_transcript = std::path::PathBuf::from("/mock/app/brain/shared-uuid-001/transcript.jsonl");
        let app_session = std::path::PathBuf::from("/mock/app/brain/shared-uuid-001");

        let existing = candidates.get_mut(&cid).unwrap();
        let is_newer = app_cand_mtime > existing.mtime
            || (app_cand_mtime == existing.mtime && app_cand_size > existing.size);
        assert!(is_newer);

        let old_path = existing.transcript_path.clone();
        existing.session_dir = app_session;
        existing.transcript_path = app_transcript;
        existing.mtime = app_cand_mtime;
        existing.size = app_cand_size;
        existing.is_ide = false;
        existing.shadowed_paths.push(old_path);

        // 验证去重结果：只有一个 candidate，指向更新的桌面端，且 shadowed_paths 包含 IDE 旧路径
        assert_eq!(candidates.len(), 1);
        let selected = candidates.get(&cid).unwrap();
        assert_eq!(selected.mtime, 2000.0);
        assert!(!selected.is_ide);
        assert_eq!(selected.shadowed_paths.len(), 1);
        assert_eq!(
            selected.shadowed_paths[0],
            std::path::PathBuf::from("/mock/ide/brain/shared-uuid-001/transcript.jsonl")
        );
    }

    #[test]
    fn test_parse_antigravity_multiversion_artifacts() {
        let tmp_dir = std::env::temp_dir().join(format!("test_ag_multiversion_{}", std::process::id()));
        let logs_dir = tmp_dir.join(".system_generated/logs");
        std::fs::create_dir_all(&logs_dir).unwrap();

        let transcript_path = logs_dir.join("transcript.jsonl");
        let mut file = File::create(&transcript_path).unwrap();

        // Step 0: 用户提问
        let line0 = serde_json::json!({
            "type": "USER_INPUT",
            "step_index": 0,
            "content": "<USER_REQUEST>设计第一版方案</USER_REQUEST>",
            "created_at": "2026-09-08T10:00:00Z"
        });
        writeln!(file, "{}", line0).unwrap();

        // Step 1: AI 生成第一版 implementation_plan.md
        let line1 = serde_json::json!({
            "type": "PLANNER_RESPONSE",
            "step_index": 1,
            "content": "已生成第一版",
            "created_at": "2026-09-08T10:01:00Z",
            "tool_calls": [{
                "name": "write_to_file",
                "args": {
                    "TargetFile": format!("{}/implementation_plan.md", tmp_dir.display()),
                    "CodeContent": "# 第一版: AI 翻译与笔记润色\n\n正文1",
                    "ArtifactMetadata": serde_json::json!({
                        "Summary": "第一版摘要",
                        "RequestFeedback": true,
                        "UserFacing": true
                    }).to_string()
                }
            }]
        });
        writeln!(file, "{}", line1).unwrap();

        // Step 2: 用户提问增加新需求
        let line2 = serde_json::json!({
            "type": "USER_INPUT",
            "step_index": 2,
            "content": "<USER_REQUEST>设计第二版监控方案</USER_REQUEST>",
            "created_at": "2026-09-08T10:10:00Z"
        });
        writeln!(file, "{}", line2).unwrap();

        // Step 3: AI 生成第二版 implementation_plan.md
        let line3 = serde_json::json!({
            "type": "PLANNER_RESPONSE",
            "step_index": 3,
            "content": "已生成第二版",
            "created_at": "2026-09-08T10:11:00Z",
            "tool_calls": [{
                "name": "write_to_file",
                "args": {
                    "TargetFile": format!("{}/implementation_plan.md", tmp_dir.display()),
                    "CodeContent": "# 第二版: LLM 监控看板\n\n正文2",
                    "ArtifactMetadata": serde_json::json!({
                        "Summary": "第二版摘要",
                        "RequestFeedback": true,
                        "UserFacing": true
                    }).to_string()
                }
            }]
        });
        writeln!(file, "{}", line3).unwrap();

        let conv = parse_antigravity_session("test-cid-multi", &transcript_path, &tmp_dir, true)
            .unwrap()
            .unwrap();

        let _ = std::fs::remove_dir_all(&tmp_dir);

        // 验证提取出的产物包含 2 个版本
        assert_eq!(conv.artifacts.len(), 2);
        // 按照排序规则，最新版 (implementation_plan.md) 排在最前，历史版 (implementation_plan.v1.md) 在后
        let latest = &conv.artifacts[0];
        let v1 = &conv.artifacts[1];

        assert_eq!(latest.file_name, "implementation_plan.md");
        assert_eq!(latest.title, "第二版: LLM 监控看板");
        assert_eq!(latest.summary.as_deref(), Some("第二版摘要"));

        assert_eq!(v1.file_name, "implementation_plan.v1.md");
        assert_eq!(v1.title, "第一版: AI 翻译与笔记润色");
        assert_eq!(v1.summary.as_deref(), Some("第一版摘要"));
    }

    /// 拷贝真实 DB 到临时副本后全量同步验证产物提取；绝不直写真实库，
    /// 避免与运行中的 dev 应用抢写锁、消耗 parser rev
    #[test]
    fn test_sync_real_antigravity_db() {
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => return,
        };
        let real = home.join(".agentdeck/agentdeck.db");
        if !real.is_file() {
            return;
        }
        let copy =
            std::env::temp_dir().join(format!("agentdeck_ag_test_{}.db", std::process::id()));
        std::fs::copy(&real, &copy).unwrap();
        let conn = Connection::open(&copy).unwrap();
        crate::db::init_schema(&conn).unwrap();
        let stats = sync(&conn, false);
        eprintln!("Antigravity sync stats: {:?}", stats);
        assert_eq!(stats.error_count, 0, "sync should not error on real data");

        let artifacts = crate::db::get_conversation_artifacts(
            &conn,
            "68613a2f-279c-4299-bf94-b75a4b7f71e0",
        )
        .unwrap();
        eprintln!("Session 68613a2f artifacts count: {}", artifacts.len());
        assert!(
            artifacts.len() >= 10,
            "Should have extracted all historical plan artifacts, got {}",
            artifacts.len()
        );
        drop(conn);
        let _ = std::fs::remove_file(&copy);
    }

    // ---- protobuf wire 编码辅助（测试专用） ----
    fn enc_varint(mut v: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let b = (v & 0x7F) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                break;
            }
            out.push(b | 0x80);
        }
        out
    }

    fn enc_field_varint(num: u64, v: u64) -> Vec<u8> {
        let mut out = enc_varint((num << 3) | 0);
        out.extend(enc_varint(v));
        out
    }

    fn enc_field_bytes(num: u64, payload: &[u8]) -> Vec<u8> {
        let mut out = enc_varint((num << 3) | 2);
        out.extend(enc_varint(payload.len() as u64));
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn test_ag_wire_decode_rules() {
        let msg = ag_wire::decode(&enc_field_varint(2, 300)).unwrap();
        assert_eq!(msg.varint(2), Some(300));

        let inner = enc_field_varint(2, 42);
        let msg = ag_wire::decode(&enc_field_bytes(4, &inner)).unwrap();
        assert_eq!(msg.nested(4).unwrap().varint(2), Some(42));

        // 截断 → 整条失败
        let full = enc_field_bytes(4, &[1, 2, 3]);
        assert!(ag_wire::decode(&full[..full.len() - 1]).is_none());
        // 超长 varint → 失败
        assert!(ag_wire::decode(&[0x80; 11]).is_none());
        // 废弃 group 类型 → 失败
        assert!(ag_wire::decode(&[0x0B]).is_none());
        // 字段号 0 → 失败
        assert!(ag_wire::decode(&[0x00]).is_none());
    }

    #[test]
    fn test_load_antigravity_usage_from_db() {
        let db_path =
            std::env::temp_dir().join(format!("test_ag_usage_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db_path);
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE steps (step_type INTEGER, metadata BLOB);
             CREATE TABLE gen_metadata (idx INTEGER, data BLOB);",
        )
        .unwrap();

        // steps: resp r1 → ts
        let mut step1 = enc_field_bytes(1, &enc_field_varint(1, 1_800_000_000));
        step1.extend(enc_field_bytes(9, &enc_field_bytes(11, b"r1")));
        conn.execute(
            "INSERT INTO steps (step_type, metadata) VALUES (15, ?1)",
            rusqlite::params![step1],
        )
        .unwrap();

        let usage_msg = |fresh: u64, cache: u64, out: u64, think: u64, resp: &[u8]| {
            let mut u = enc_field_varint(2, fresh);
            u.extend(enc_field_varint(5, cache));
            u.extend(enc_field_varint(9, out));
            u.extend(enc_field_varint(10, think));
            u.extend(enc_field_bytes(11, resp));
            u
        };
        let gen_msg = |usage: Option<Vec<u8>>,
                       model: &str,
                       label: Option<&str>,
                       explicit_ts: Option<u64>| {
            let mut chat = Vec::new();
            if let Some(u) = usage {
                chat.extend(enc_field_bytes(4, &u));
            }
            chat.extend(enc_field_bytes(19, model.as_bytes()));
            if let Some(l) = label {
                chat.extend(enc_field_bytes(21, l.as_bytes()));
            }
            if let Some(secs) = explicit_ts {
                chat.extend(enc_field_bytes(
                    9,
                    &enc_field_bytes(4, &enc_field_varint(1, secs)),
                ));
            }
            enc_field_bytes(1, &chat)
        };

        // gen 0: 真实模型，无显式时间戳 → 走 steps 的 resp 映射
        conn.execute(
            "INSERT INTO gen_metadata (idx, data) VALUES (0, ?1)",
            rusqlite::params![gen_msg(
                Some(usage_msg(100, 0, 10, 5, b"r1")),
                "gemini-3.6-flash",
                None,
                None
            )],
        )
        .unwrap();
        // gen 1: 路由占位 + 已验证 label → labelTable
        conn.execute(
            "INSERT INTO gen_metadata (idx, data) VALUES (1, ?1)",
            rusqlite::params![gen_msg(
                Some(usage_msg(50, 200, 8, 2, b"r2")),
                AG_ROUTING_MODEL,
                Some("Gemini 3.5 Flash (Low)"),
                Some(1_800_000_100)
            )],
        )
        .unwrap();
        // gen 2: 全零 → 丢弃
        conn.execute(
            "INSERT INTO gen_metadata (idx, data) VALUES (2, ?1)",
            rusqlite::params![gen_msg(
                Some(usage_msg(0, 0, 0, 0, b"r3")),
                "gemini-3.6-flash",
                None,
                None
            )],
        )
        .unwrap();
        // gen 3: 无 usage 消息 → 丢弃
        conn.execute(
            "INSERT INTO gen_metadata (idx, data) VALUES (3, ?1)",
            rusqlite::params![gen_msg(None, "gemini-3.6-flash", None, None)],
        )
        .unwrap();
        // gen 4: resp 重放 → 去重
        conn.execute(
            "INSERT INTO gen_metadata (idx, data) VALUES (4, ?1)",
            rusqlite::params![gen_msg(
                Some(usage_msg(999, 0, 1, 1, b"r1")),
                "gemini-3.6-flash",
                None,
                None
            )],
        )
        .unwrap();
        // gen 5: 路由占位 + 未知 label，但同 label 兄弟行有具体模型 → label 找回
        conn.execute(
            "INSERT INTO gen_metadata (idx, data) VALUES (5, ?1)",
            rusqlite::params![gen_msg(
                Some(usage_msg(30, 0, 3, 1, b"r5")),
                AG_ROUTING_MODEL,
                Some("Custom Label"),
                None
            )],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO gen_metadata (idx, data) VALUES (6, ?1)",
            rusqlite::params![gen_msg(
                Some(usage_msg(40, 0, 4, 1, b"r6")),
                "gemini-3.9-pro",
                Some("Custom Label"),
                None
            )],
        )
        .unwrap();
        drop(conn);

        let records = load_antigravity_usage_from_db(&db_path, Some("2020-01-01T00:00:00Z"));
        let _ = std::fs::remove_file(&db_path);

        assert_eq!(records.len(), 4);
        let r1 = records.iter().find(|r| r.identity == "r1").unwrap();
        assert_eq!(r1.model.as_deref(), Some("gemini-3.6-flash"));
        assert_eq!(r1.input_tokens, Some(100));
        assert_eq!(r1.cache_read_tokens, Some(0));
        assert_eq!(r1.output_tokens, Some(10));
        assert_eq!(r1.reasoning_tokens, Some(5));
        assert!(r1.occurred_at.as_deref().unwrap().starts_with("2027-"));
        assert!(r1.is_partial);

        let r2 = records.iter().find(|r| r.identity == "r2").unwrap();
        assert_eq!(r2.model.as_deref(), Some("gemini-3.5-flash-extra-low"));
        assert_eq!(r2.input_tokens, Some(50));
        assert_eq!(r2.cache_read_tokens, Some(200));
        assert!(r2.occurred_at.as_deref().unwrap().starts_with("2027-"));

        let r5 = records.iter().find(|r| r.identity == "r5").unwrap();
        assert_eq!(r5.model.as_deref(), Some("gemini-3.9-pro"));
        // 无任何时间线索 → 会话 created_at 兜底
        assert_eq!(r5.occurred_at.as_deref(), Some("2020-01-01T00:00:00Z"));
    }

    /// 对本机真实会话库做一次解析试跑（字段映射经实测验证）
    #[test]
    fn test_load_real_antigravity_usage() {
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => return,
        };
        let db = home
            .join(".gemini/antigravity/conversations")
            .join("1b42fbc9-55c3-41f5-80e4-9cafc6cfd0ff.db");
        if !db.is_file() {
            return;
        }
        let records = load_antigravity_usage_from_db(&db, None);
        assert_eq!(records.len(), 44);

        let first = records
            .iter()
            .find(|r| r.identity.starts_with("xziKaoKd"))
            .unwrap();
        assert_eq!(first.model.as_deref(), Some("gemini-3.6-flash"));
        assert_eq!(first.input_tokens, Some(22361));
        assert_eq!(first.cache_read_tokens, Some(0));
        assert_eq!(first.output_tokens, Some(190));
        assert_eq!(first.reasoning_tokens, Some(59));
        assert!(first
            .occurred_at
            .as_deref()
            .unwrap_or("")
            .starts_with("2026-08-23"));
        assert!(records.iter().all(|r| r.is_partial));
    }
}

