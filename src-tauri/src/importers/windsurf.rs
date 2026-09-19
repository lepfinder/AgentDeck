//! Windsurf Cascade 历史导入
//!
//! 读取 `~/.codeium/windsurf/cascade/*.pb`（及 `implicit/`）：
//! AES-256-GCM 解密 → 无 schema 的 protobuf wire 解析 → `RawConversation`。
//!
//! 密钥与字段映射参考：
//! https://github.com/dayearleo/windsurf-local-user-data-decryption

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use regex::Regex;
use rusqlite::Connection;
use std::path::Path;
use std::sync::OnceLock;

use super::{
    canonicalize_workspace_path, needs_sync, record_sync_state, save_conversation_tx, ImporterStats,
    RawConversation, RawMessage,
};

/// Windsurf language_server 内嵌 AES-256-GCM 密钥（全用户共用）
const WINDSURF_AES_KEY: &[u8; 32] = b"safeCodeiumworldKeYsecretBalloon";

const VARIANT_FILE_CONTEXT: u64 = 15;
const VARIANT_USER_INPUT: u64 = 19;
const VARIANT_PLANNER_RESPONSE: u64 = 20;
const VARIANT_RUN_COMMAND: u64 = 28;
const VARIANT_CHECKPOINT: u64 = 30;
const VARIANT_COMMAND_RESULT: u64 = 37;

pub fn sync(conn: &Connection, incremental: bool) -> ImporterStats {
    let mut stats = ImporterStats {
        app: "Windsurf".to_string(),
        new_count: 0,
        updated_count: 0,
        skipped_count: 0,
        error_count: 0,
    };

    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return stats,
    };

    for dir in [
        home.join(".codeium/windsurf/cascade"),
        home.join(".codeium/windsurf/implicit"),
    ] {
        if !dir.is_dir() {
            continue;
        }
        let source_type = if dir.ends_with("implicit") {
            "windsurf_implicit_pb"
        } else {
            "windsurf_cascade_pb"
        };
        sync_dir(conn, &dir, source_type, incremental, &mut stats);
    }

    stats
}

fn sync_dir(
    conn: &Connection,
    dir: &Path,
    source_type: &str,
    incremental: bool,
    stats: &mut ImporterStats,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("[Windsurf Importer] 无法读取 {}: {}", dir.display(), e);
            stats.error_count += 1;
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("pb") {
            continue;
        }
        if incremental && !needs_sync(conn, &path, true) {
            stats.skipped_count += 1;
            continue;
        }

        match parse_cascade_file(&path) {
            Ok(Some(conv)) => match save_conversation_tx(conn, &conv) {
                Ok(is_new) => {
                    record_sync_state(conn, &path, &conv.id, source_type);
                    if is_new {
                        stats.new_count += 1;
                    } else {
                        stats.updated_count += 1;
                    }
                }
                Err(e) => {
                    eprintln!("[Windsurf Importer] 保存失败 {}: {}", conv.id, e);
                    stats.error_count += 1;
                }
            },
            Ok(None) => stats.skipped_count += 1,
            Err(e) => {
                eprintln!("[Windsurf Importer] 解析失败 {}: {}", path.display(), e);
                stats.error_count += 1;
            }
        }
    }
}

fn parse_cascade_file(path: &Path) -> Result<Option<RawConversation>, String> {
    let encrypted = std::fs::read(path).map_err(|e| format!("读文件失败: {}", e))?;
    let plaintext = decrypt_pb(&encrypted)?;

    let file_stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".into());

    let traj = parse_trajectory(&plaintext);
    let id_stem = traj
        .cascade_id
        .filter(|s| !s.is_empty())
        .unwrap_or(file_stem);
    let cid = format!("windsurf:{}", id_stem);

    let mut messages: Vec<RawMessage> = Vec::new();
    let mut step_index: i64 = 0;
    let mut title: Option<String> = None;
    let mut first_user: Option<String> = None;
    let mut created_at: Option<String> = None;
    let mut updated_at: Option<String> = None;
    let mut workspace_hint: Option<String> = None;

    for step_buf in &traj.steps {
        let step = parse_step(step_buf);
        let ts = step.timestamp.clone();
        if let Some(ref t) = ts {
            if created_at.is_none() {
                created_at = Some(t.clone());
            }
            updated_at = Some(t.clone());
        }

        match step.variant_field {
            Some(VARIANT_USER_INPUT) => {
                let prompt =
                    read_string_field(step.variant_data.as_deref().unwrap_or(&[]), 2).unwrap_or_default();
                if prompt.trim().is_empty() {
                    continue;
                }
                if first_user.is_none() {
                    first_user = Some(prompt.clone());
                }
                messages.push(text_msg(step_index, "user", prompt, None, ts));
                step_index += 1;
            }
            Some(VARIANT_PLANNER_RESPONSE) => {
                let data = step.variant_data.as_deref().unwrap_or(&[]);
                let facing = read_string_field(data, 1)
                    .or_else(|| read_string_field(data, 8))
                    .unwrap_or_default();
                let thinking = read_string_field(data, 3);
                let tools = collect_tool_calls(data);

                if !facing.trim().is_empty()
                    || thinking
                        .as_ref()
                        .map(|s| !s.trim().is_empty())
                        .unwrap_or(false)
                {
                    messages.push(text_msg(
                        step_index,
                        "assistant",
                        facing,
                        thinking,
                        ts.clone(),
                    ));
                    step_index += 1;
                }

                for (tool_name, tool_args) in tools {
                    messages.push(RawMessage {
                        step_index,
                        role: "assistant".into(),
                        message_type: "tool_call".into(),
                        content: tool_name.clone(),
                        thinking: None,
                        created_at: ts.clone(),
                        model_name: None,
                        tool_name: Some(tool_name),
                        tool_args,
                        duration_ms: None,
                        token_count: None,
                        credit: None,
                        images: None,
                    });
                    step_index += 1;
                }
            }
            Some(VARIANT_RUN_COMMAND) => {
                let data = step.variant_data.as_deref().unwrap_or(&[]);
                let cmd = read_string_field(data, 23)
                    .or_else(|| read_string_field(data, 25))
                    .unwrap_or_default();
                let output = read_bytes_as_utf8(data, 24);
                if !cmd.trim().is_empty() {
                    messages.push(RawMessage {
                        step_index,
                        role: "assistant".into(),
                        message_type: "tool_call".into(),
                        content: cmd.clone(),
                        thinking: None,
                        created_at: ts.clone(),
                        model_name: None,
                        tool_name: Some("run_command".into()),
                        tool_args: Some(serde_json::json!({ "command": cmd }).to_string()),
                        duration_ms: None,
                        token_count: None,
                        credit: None,
                        images: None,
                    });
                    step_index += 1;
                }
                if let Some(out) = output.filter(|s| !s.trim().is_empty()) {
                    messages.push(RawMessage {
                        step_index,
                        role: "tool".into(),
                        message_type: "tool_result".into(),
                        content: truncate_chars(&out, 8000),
                        thinking: None,
                        created_at: ts,
                        model_name: None,
                        tool_name: Some("run_command".into()),
                        tool_args: None,
                        duration_ms: None,
                        token_count: None,
                        credit: None,
                        images: None,
                    });
                    step_index += 1;
                }
            }
            Some(VARIANT_COMMAND_RESULT) => {
                let data = step.variant_data.as_deref().unwrap_or(&[]);
                let output = read_string_field(data, 9).unwrap_or_default();
                if output.trim().is_empty() {
                    continue;
                }
                messages.push(RawMessage {
                    step_index,
                    role: "tool".into(),
                    message_type: "tool_result".into(),
                    content: truncate_chars(&output, 8000),
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
                step_index += 1;
            }
            Some(VARIANT_CHECKPOINT) => {
                let data = step.variant_data.as_deref().unwrap_or(&[]);
                if title.is_none() {
                    if let Some(t) = read_string_field(data, 10).filter(|s| !s.trim().is_empty()) {
                        title = Some(t);
                    }
                }
                for field in ProtoField::iter(data) {
                    if field.number == 7 && field.wire == 2 {
                        if let Some(tool) = field.bytes() {
                            if let Some(fp) = read_string_field(tool, 1) {
                                if let Some(ws) = workspace_from_path_str(&fp) {
                                    workspace_hint.get_or_insert(ws);
                                }
                            }
                        }
                    }
                }
            }
            Some(VARIANT_FILE_CONTEXT) => {
                let data = step.variant_data.as_deref().unwrap_or(&[]);
                if let Some(uri) = read_string_field(data, 1) {
                    if let Some(ws) = workspace_from_path_str(&uri) {
                        workspace_hint.get_or_insert(ws);
                    }
                }
            }
            _ => {
                if workspace_hint.is_none() {
                    if let Some(data) = step.variant_data.as_deref() {
                        if let Some(ws) = scan_workspace_in_bytes(data) {
                            workspace_hint = Some(ws);
                        }
                    }
                }
            }
        }
    }

    if messages.is_empty() {
        return Ok(None);
    }

    if workspace_hint.is_none() {
        workspace_hint = scan_workspace_in_bytes(&plaintext);
    }

    let final_title = title
        .or_else(|| {
            first_user.map(|u| {
                let line = u.lines().next().unwrap_or(&u).trim();
                truncate_chars(line, 80)
            })
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("Windsurf {}", &id_stem[..id_stem.len().min(8)]));

    Ok(Some(RawConversation {
        id: cid,
        title: final_title,
        workspace_path: workspace_hint.unwrap_or_default(),
        source_app: "windsurf".into(),
        created_at,
        updated_at,
        parse_status: "ok".into(),
        source_types: vec!["windsurf".into()],
        messages,
        artifacts: vec![],
    }))
}

fn text_msg(
    step_index: i64,
    role: &str,
    content: String,
    thinking: Option<String>,
    created_at: Option<String>,
) -> RawMessage {
    RawMessage {
        step_index,
        role: role.into(),
        message_type: "text".into(),
        content,
        thinking,
        created_at,
        model_name: None,
        tool_name: None,
        tool_args: None,
        duration_ms: None,
        token_count: None,
        credit: None,
        images: None,
    }
}

fn decrypt_pb(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 12 + 16 {
        return Err("密文过短".into());
    }
    let cipher =
        Aes256Gcm::new_from_slice(WINDSURF_AES_KEY).map_err(|e| format!("AES key 无效: {}", e))?;
    let nonce = Nonce::from_slice(&data[..12]);
    cipher
        .decrypt(nonce, &data[12..])
        .map_err(|_| "AES-GCM 解密失败（密钥或文件损坏）".into())
}

// --- protobuf wire ---

struct TrajectoryInfo {
    cascade_id: Option<String>,
    steps: Vec<Vec<u8>>,
}

struct StepInfo {
    variant_field: Option<u64>,
    variant_data: Option<Vec<u8>>,
    timestamp: Option<String>,
}

enum FieldValue<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    #[allow(dead_code)]
    Fixed64(&'a [u8]),
    #[allow(dead_code)]
    Fixed32(&'a [u8]),
}

struct ProtoField<'a> {
    number: u64,
    wire: u8,
    value: FieldValue<'a>,
}

impl<'a> ProtoField<'a> {
    fn bytes(&self) -> Option<&'a [u8]> {
        match self.value {
            FieldValue::Bytes(b) => Some(b),
            _ => None,
        }
    }

    fn varint(&self) -> Option<u64> {
        match self.value {
            FieldValue::Varint(v) => Some(v),
            _ => None,
        }
    }

    fn iter(buf: &'a [u8]) -> ProtoFieldIter<'a> {
        ProtoFieldIter { buf, pos: 0 }
    }
}

struct ProtoFieldIter<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Iterator for ProtoFieldIter<'a> {
    type Item = ProtoField<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.buf.len() {
            return None;
        }
        let (tag, np) = read_varint(self.buf, self.pos)?;
        self.pos = np;
        let number = tag >> 3;
        let wire = (tag & 7) as u8;
        let value = match wire {
            0 => {
                let (v, np) = read_varint(self.buf, self.pos)?;
                self.pos = np;
                FieldValue::Varint(v)
            }
            1 => {
                if self.pos + 8 > self.buf.len() {
                    return None;
                }
                let slice = &self.buf[self.pos..self.pos + 8];
                self.pos += 8;
                FieldValue::Fixed64(slice)
            }
            2 => {
                let (len, np) = read_varint(self.buf, self.pos)?;
                self.pos = np;
                let len = len as usize;
                if self.pos + len > self.buf.len() {
                    return None;
                }
                let slice = &self.buf[self.pos..self.pos + len];
                self.pos += len;
                FieldValue::Bytes(slice)
            }
            5 => {
                if self.pos + 4 > self.buf.len() {
                    return None;
                }
                let slice = &self.buf[self.pos..self.pos + 4];
                self.pos += 4;
                FieldValue::Fixed32(slice)
            }
            _ => return None,
        };
        Some(ProtoField {
            number,
            wire,
            value,
        })
    }
}

fn parse_trajectory(buf: &[u8]) -> TrajectoryInfo {
    let mut info = TrajectoryInfo {
        cascade_id: None,
        steps: Vec::new(),
    };
    for field in ProtoField::iter(buf) {
        match (field.number, field.wire) {
            (2, 2) => {
                if let Some(b) = field.bytes() {
                    info.steps.push(b.to_vec());
                }
            }
            (6, 2) => {
                if let Some(b) = field.bytes() {
                    if let Ok(s) = std::str::from_utf8(b) {
                        if !s.is_empty() {
                            info.cascade_id = Some(s.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    info
}

fn parse_step(step_buf: &[u8]) -> StepInfo {
    let mut info = StepInfo {
        variant_field: None,
        variant_data: None,
        timestamp: None,
    };
    for field in ProtoField::iter(step_buf) {
        if field.number == 5 && field.wire == 2 {
            if let Some(meta) = field.bytes() {
                info.timestamp = timestamp_from_metadata(meta);
            }
        } else if (7..=110).contains(&field.number) && field.wire == 2 && info.variant_field.is_none()
        {
            info.variant_field = Some(field.number);
            info.variant_data = field.bytes().map(|b| b.to_vec());
        }
    }
    info
}

fn timestamp_from_metadata(meta: &[u8]) -> Option<String> {
    for field in ProtoField::iter(meta) {
        if field.number == 1 && field.wire == 2 {
            let ts_msg = field.bytes()?;
            let mut secs: Option<i64> = None;
            let mut nanos: u32 = 0;
            for sf in ProtoField::iter(ts_msg) {
                if sf.number == 1 && sf.wire == 0 {
                    secs = sf.varint().map(|v| v as i64);
                } else if sf.number == 2 && sf.wire == 0 {
                    nanos = sf.varint().unwrap_or(0) as u32;
                }
            }
            if let Some(s) = secs {
                return chrono::DateTime::from_timestamp(s, nanos).map(|dt| dt.to_rfc3339());
            }
        }
    }
    None
}

fn read_string_field(buf: &[u8], target: u64) -> Option<String> {
    for field in ProtoField::iter(buf) {
        if field.number == target && field.wire == 2 {
            let b = field.bytes()?;
            let s = std::str::from_utf8(b).ok()?;
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

fn read_bytes_as_utf8(buf: &[u8], target: u64) -> Option<String> {
    for field in ProtoField::iter(buf) {
        if field.number == target && field.wire == 2 {
            let b = field.bytes()?;
            let s = String::from_utf8_lossy(b).into_owned();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
}

fn collect_tool_calls(data: &[u8]) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    for field in ProtoField::iter(data) {
        if field.number == 7 && field.wire == 2 {
            let tool = field.bytes().unwrap_or(&[]);
            let name = read_string_field(tool, 2).unwrap_or_else(|| "tool".into());
            let args = read_string_field(tool, 3);
            out.push((name, args));
        }
    }
    out
}

fn read_varint(buf: &[u8], mut pos: usize) -> Option<(u64, usize)> {
    let mut val: u64 = 0;
    let mut shift = 0u32;
    while pos < buf.len() {
        let b = buf[pos];
        pos += 1;
        val |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return Some((val, pos));
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
    None
}

fn workspace_from_path_str(raw: &str) -> Option<String> {
    let cleaned = raw
        .trim()
        .trim_start_matches("file://")
        .trim_start_matches("*I")
        .trim_start_matches("*P");
    let cut = cleaned
        .find(|c: char| c.is_control() || c == '\0')
        .unwrap_or(cleaned.len());
    let path = cleaned[..cut].trim();
    if !path.starts_with('/') {
        return None;
    }
    let canon = canonicalize_workspace_path(path);
    if canon.is_empty() {
        None
    } else {
        Some(canon)
    }
}

fn scan_workspace_in_bytes(buf: &[u8]) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?:file://)?(/Users/[^/\s\x00]+/workspace/[^/\s\x00]+/[^/\s\x00]+)").unwrap()
    });
    let s = String::from_utf8_lossy(buf);
    let caps = re.captures(&s)?;
    workspace_from_path_str(&caps[1])
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max).collect();
    format!("{}…", truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decrypt_and_parse_real_cascade_if_present() {
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => return,
        };
        let cascade = home.join(".codeium/windsurf/cascade");
        if !cascade.is_dir() {
            return;
        }
        let mut parsed = 0u32;
        let mut with_msgs = 0u32;
        for entry in std::fs::read_dir(&cascade).unwrap().flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("pb") {
                continue;
            }
            parsed += 1;
            let conv = parse_cascade_file(&path).expect("parse ok");
            if let Some(c) = conv {
                assert!(c.id.starts_with("windsurf:"));
                assert_eq!(c.source_app, "windsurf");
                assert!(!c.messages.is_empty());
                with_msgs += 1;
            }
        }
        assert!(parsed > 0);
        assert!(with_msgs > 0);
    }

    #[test]
    fn sync_into_memory_db() {
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => return,
        };
        if !home.join(".codeium/windsurf/cascade").is_dir() {
            return;
        }
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();
        let stats = sync(&conn, false);
        println!(
            "windsurf sync => new={} updated={} skipped={} errors={}",
            stats.new_count, stats.updated_count, stats.skipped_count, stats.error_count
        );
        assert_eq!(stats.error_count, 0);
        assert!(stats.new_count > 0);

        let (cnt, users): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(user_message_count),0) FROM conversations WHERE source_app='windsurf'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(cnt as u32, stats.new_count);
        assert!(users > 0);

        let sample: (String, String, String) = conn
            .query_row(
                "SELECT id, title, workspace_path FROM conversations WHERE source_app='windsurf' AND workspace_path != '' LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        println!("sample => id={} title={} ws={}", sample.0, sample.1, sample.2);
        assert!(sample.2.contains("/workspace/") || sample.2.starts_with("/Users/"));
    }
}
