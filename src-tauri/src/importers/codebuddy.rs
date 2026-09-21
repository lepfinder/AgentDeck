use rusqlite::{params, Connection};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::{
    canonicalize_workspace_path, needs_sync, record_sync_state, save_conversation_tx,
    ImporterStats, RawArtifact, RawConversation, RawMessage, RawUsageRecord,
};

/// 解析格式版本：变更后强制重新扫描
const CODEBUDDY_PARSER_REV: &str = "codebuddy-v3";
const CODEBUDDY_PARSER_REV_KEY: &str = "agentdeck:codebuddy_parser_rev";

/// 单条消息正文的截断上限，防止超大 tool 结果拖垮 UI
const CONTENT_TRUNCATE: usize = 4000;

/// vscdb 中最近会话的元数据（cwd / 标题）
struct ConvMeta {
    cwd: Option<String>,
    title: Option<String>,
}

pub fn sync(conn: &Connection, incremental: bool) -> ImporterStats {
    let mut stats = ImporterStats {
        app: "CodeBuddy".to_string(),
        new_count: 0,
        updated_count: 0,
        skipped_count: 0,
        error_count: 0,
    };

    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return stats,
    };

    // CodeBuddy 扩展的独立数据目录（注意不是 CodeBuddy CN/）
    let data_root = home.join("Library/Application Support/CodeBuddyExtension/Data");
    if !data_root.is_dir() {
        return stats;
    }

    // 1. 枚举账号域（default / <userId>）下的 CodeBuddyIDE 根
    let mut ide_roots: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&data_root) {
        for entry in rd.flatten() {
            let root = entry.path().join("CodeBuddyIDE");
            if root.is_dir() {
                ide_roots.push(root);
            }
        }
    }
    if ide_roots.is_empty() {
        return stats;
    }

    // 2. 工作区映射：history 目录名是 md5(cwd) 不可逆，
    //    用 base64 目录名 + vscdb 元数据建立 md5 -> cwd 反查表
    let mut md5_to_cwd: HashMap<String, String> = HashMap::new();
    let mut base64_to_cwd: HashMap<String, String> = HashMap::new();
    for root in &ide_roots {
        collect_base64_workspaces(root, &mut md5_to_cwd, &mut base64_to_cwd);
    }
    collect_genie_history_workspaces(&home, &mut md5_to_cwd);
    let vscdb_meta = load_sessions_vscdb(&home);
    for meta in vscdb_meta.values() {
        if let Some(cwd) = &meta.cwd {
            register_md5_workspace(cwd, &mut md5_to_cwd);
        }
    }

    // 3. 收集会话目录：**/history/**/index.json，父目录名为 32 位 convId。
    //    history 有三种布局深度（账号级 / 工作区级 / default 域），统一递归探测。
    let mut conv_dirs: Vec<(PathBuf, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for root in &ide_roots {
        for entry in walkdir::WalkDir::new(root)
            .max_depth(6)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path.file_name().and_then(|s| s.to_str()) != Some("index.json") {
                continue;
            }
            let conv_dir = match path.parent() {
                Some(p) => p,
                None => continue,
            };
            // 必须位于 history/ 下，排除 check-point 等同构目录
            if !conv_dir
                .ancestors()
                .any(|a| a.file_name().and_then(|s| s.to_str()) == Some("history"))
            {
                continue;
            }
            let conv_id = match conv_dir.file_name().and_then(|s| s.to_str()) {
                Some(n) if n.len() == 32 && n.chars().all(|c| c.is_ascii_hexdigit()) => {
                    n.to_string()
                }
                _ => continue,
            };
            if !seen.insert(conv_id.clone()) {
                continue;
            }
            conv_dirs.push((conv_dir.to_path_buf(), conv_id));
        }
    }

    // 4. 解析格式升级时强制全量重扫，否则按 index.json 增量
    let force_reparse = codebuddy_parser_rev_stale(conn);
    let incremental = incremental && !force_reparse;

    for (conv_dir, conv_id) in conv_dirs {
        let index_path = conv_dir.join("index.json");
        if incremental && !needs_sync(conn, &index_path, true) {
            stats.skipped_count += 1;
            continue;
        }

        match parse_conversation(
            &conv_dir,
            &conv_id,
            &md5_to_cwd,
            &base64_to_cwd,
            &vscdb_meta,
        ) {
            Ok(Some(conv)) => {
                let cid = conv.id.clone();
                match save_conversation_tx(conn, &conv) {
                    Ok(created) => {
                        if created {
                            stats.new_count += 1;
                        } else {
                            stats.updated_count += 1;
                        }
                    }
                    Err(_) => stats.error_count += 1,
                }
                record_sync_state(conn, &index_path, &cid, "codebuddy_index");
            }
            Ok(None) => {
                // 空会话（index.json 无消息）：记录同步状态避免每轮重复解析
                record_sync_state(
                    conn,
                    &index_path,
                    &format!("codebuddy:{}", conv_id),
                    "codebuddy_index",
                );
                stats.skipped_count += 1;
            }
            Err(_) => stats.error_count += 1,
        }
    }

    // 有错误的运行不标记 rev：否则一次不完整的运行会永久消耗强制重导
    if stats.error_count == 0 {
        mark_codebuddy_synced(conn);
    }
    stats
}

fn parse_conversation(
    conv_dir: &Path,
    conv_id: &str,
    md5_to_cwd: &HashMap<String, String>,
    base64_to_cwd: &HashMap<String, String>,
    vscdb_meta: &HashMap<String, ConvMeta>,
) -> Result<Option<RawConversation>, String> {
    let index_raw = std::fs::read_to_string(conv_dir.join("index.json")).map_err(|e| e.to_string())?;
    let index: Value = serde_json::from_str(&index_raw).map_err(|e| e.to_string())?;

    // index.messages 的顺序即消息顺序
    let order: Vec<String> = index
        .get("messages")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    if order.is_empty() {
        return Ok(None); // 空会话
    }

    // requests[].usage（token / 积分）挂到该轮最后一条消息上
    let mut usage_by_msg: HashMap<String, (Option<i64>, Option<f64>)> = HashMap::new();
    // 同一份数据同时落 usage_records 事实表；requestId → modelName 由消息 extra 补齐
    let mut usage_records: Vec<RawUsageRecord> = Vec::new();
    let mut req_idents: HashSet<String> = HashSet::new();
    if let Some(reqs) = index.get("requests").and_then(|r| r.as_array()) {
        for req in reqs {
            let usage = req.get("usage");
            let tokens = usage
                .and_then(|u| u.get("totalTokens"))
                .and_then(|v| v.as_i64());
            let credit = usage
                .and_then(|u| u.get("credit"))
                .and_then(|v| v.as_f64());
            if tokens.is_none() && credit.is_none() {
                continue;
            }
            if let Some(last) = req
                .get("messages")
                .and_then(|m| m.as_array())
                .and_then(|arr| arr.last())
                .and_then(|v| v.as_str())
            {
                usage_by_msg.insert(last.to_string(), (tokens, credit));
            }

            let ident = req
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if ident.is_empty() || !req_idents.insert(ident.clone()) {
                continue;
            }
            let u = usage.unwrap();
            // inputTokens 含缓存命中，cachedMissTokens 才是新鲜输入
            let fresh_input = u
                .get("cachedMissTokens")
                .and_then(|v| v.as_i64())
                .or_else(|| {
                    let input = u.get("inputTokens").and_then(|v| v.as_i64())?;
                    let cached = u.get("cacheTokens").and_then(|v| v.as_i64()).unwrap_or(0);
                    Some((input - cached).max(0))
                });
            let output = u.get("outputTokens").and_then(|v| v.as_i64());
            let cache_read = u.get("cacheTokens").and_then(|v| v.as_i64());
            let cache_write = u.get("cachedWriteTokens").and_then(|v| v.as_i64());
            let reasoning = u.get("thinkingTokens").and_then(|v| v.as_i64());
            let occurred_at = req
                .get("startedAt")
                .and_then(|v| v.as_i64())
                .and_then(|ms| chrono::DateTime::from_timestamp_millis(ms))
                .map(|dt| dt.to_rfc3339());
            if fresh_input.is_some()
                || output.is_some()
                || cache_read.is_some()
                || cache_write.is_some()
            {
                usage_records.push(RawUsageRecord {
                    identity: ident,
                    agent: "codebuddy".to_string(),
                    model: None,
                    input_tokens: fresh_input,
                    cache_read_tokens: cache_read,
                    cache_write_tokens: cache_write,
                    output_tokens: output,
                    reasoning_tokens: reasoning,
                    credit: credit.filter(|c| *c > 0.0),
                    occurred_at,
                    is_partial: false,
                });
            }
        }
    }

    let mut messages: Vec<RawMessage> = Vec::new();
    let mut step: i64 = 0;
    let mut first_user_text: Option<String> = None;
    let mut created_at: Option<String> = None;
    let mut updated_at: Option<String> = None;
    let mut req_model: HashMap<String, String> = HashMap::new();

    for msg_id in &order {
        let msg_path = conv_dir.join("messages").join(format!("{}.json", msg_id));
        // index 与消息文件可能短暂不一致（IDE 正在写入），读不到就跳过
        let msg_raw = match std::fs::read_to_string(&msg_path) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let outer: Value = match serde_json::from_str(&msg_raw) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let role = outer.get("role").and_then(|v| v.as_str()).unwrap_or("");
        let created = outer
            .get("createdAt")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if created_at.is_none() {
            created_at = created.clone();
        }
        if created.is_some() {
            updated_at = created.clone();
        }

        // message / extra 字段本身是序列化的 JSON 字符串，需要二次解析
        let inner: Value = match outer
            .get("message")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
        {
            Some(v) => v,
            None => continue,
        };
        let extra: Value = outer
            .get("extra")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .unwrap_or(Value::Null);
        let model_name = extra
            .get("modelName")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        if let (Some(rid), Some(mn)) = (
            extra.get("requestId").and_then(|v| v.as_str()),
            extra.get("modelName").and_then(|v| v.as_str()),
        ) {
            if !mn.is_empty() {
                req_model.entry(rid.to_string()).or_insert_with(|| mn.to_string());
            }
        }
        let content_items: Vec<Value> = inner
            .get("content")
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();

        let (tokens, credit) = usage_by_msg.get(msg_id).cloned().unwrap_or((None, None));
        let range_start = messages.len();

        match role {
            "user" => {
                let raw_text = content_items
                    .iter()
                    .filter_map(|c| {
                        if c.get("type").and_then(|t| t.as_str()) == Some("text") {
                            c.get("text").and_then(|t| t.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<&str>>()
                    .join("\n");
                let text = extract_user_text(&raw_text);
                if text.is_empty() {
                    continue;
                }
                if first_user_text.is_none() {
                    first_user_text = Some(text.chars().take(200).collect());
                }
                messages.push(RawMessage {
                    step_index: step,
                    role: "user".to_string(),
                    message_type: "text".to_string(),
                    content: text,
                    thinking: None,
                    created_at: created.clone(),
                    model_name: None,
                    tool_name: None,
                    tool_args: None,
                    duration_ms: None,
                    token_count: None,
                    credit: None,
                    images: None,
                });
                step += 1;
            }
            "assistant" => {
                let mut text_parts: Vec<String> = Vec::new();
                let mut thinking_parts: Vec<String> = Vec::new();
                for c in &content_items {
                    match c.get("type").and_then(|t| t.as_str()) {
                        Some("text") => {
                            if let Some(t) = c.get("text").and_then(|v| v.as_str()) {
                                if !t.is_empty() {
                                    text_parts.push(t.to_string());
                                }
                            }
                        }
                        Some("reasoning") => {
                            if let Some(t) = c.get("text").and_then(|v| v.as_str()) {
                                if !t.is_empty() {
                                    thinking_parts.push(t.to_string());
                                }
                            }
                        }
                        _ => {}
                    }
                }
                let text = text_parts.join("\n");
                let thinking = if thinking_parts.is_empty() {
                    None
                } else {
                    Some(thinking_parts.join("\n"))
                };
                if !text.is_empty() || thinking.is_some() {
                    messages.push(RawMessage {
                        step_index: step,
                        role: "assistant".to_string(),
                        message_type: "text".to_string(),
                        content: text,
                        thinking,
                        created_at: created.clone(),
                        model_name: model_name.clone(),
                        tool_name: None,
                        tool_args: None,
                        duration_ms: None,
                        token_count: None,
                        credit: None,
                        images: None,
                    });
                    step += 1;
                }
                // 每个工具调用单独成一条消息，与后续 tool_result 对齐
                for c in &content_items {
                    if c.get("type").and_then(|t| t.as_str()) != Some("tool-call") {
                        continue;
                    }
                    let tool_name = c
                        .get("toolName")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let args_json = c
                        .get("args")
                        .map(|a| serde_json::to_string_pretty(a).unwrap_or_default())
                        .unwrap_or_default();
                    messages.push(RawMessage {
                        step_index: step,
                        role: "assistant".to_string(),
                        message_type: "tool_call".to_string(),
                        content: truncate(&args_json, CONTENT_TRUNCATE),
                        thinking: None,
                        created_at: created.clone(),
                        model_name: model_name.clone(),
                        tool_name: Some(tool_name),
                        tool_args: Some(truncate(&args_json, CONTENT_TRUNCATE)),
                        duration_ms: None,
                        token_count: None,
                        credit: None,
                        images: None,
                    });
                    step += 1;
                }
            }
            "tool" => {
                for c in &content_items {
                    if c.get("type").and_then(|t| t.as_str()) != Some("tool-result") {
                        continue;
                    }
                    let tool_name = c
                        .get("toolName")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let result_json = c
                        .get("result")
                        .map(|r| serde_json::to_string(r).unwrap_or_default())
                        .unwrap_or_default();
                    messages.push(RawMessage {
                        step_index: step,
                        role: "tool".to_string(),
                        message_type: "tool_result".to_string(),
                        content: truncate(&result_json, CONTENT_TRUNCATE),
                        thinking: None,
                        created_at: created.clone(),
                        model_name: None,
                        tool_name: Some(tool_name),
                        tool_args: None,
                        duration_ms: None,
                        token_count: None,
                        credit: None,
                        images: None,
                    });
                    step += 1;
                }
            }
            _ => {}
        }

        // 该轮 usage 挂到本条消息展开出的最后一条上
        if messages.len() > range_start {
            if let Some(last) = messages.last_mut() {
                last.token_count = tokens;
                last.credit = credit;
            }
        }
    }

    if messages.is_empty() {
        return Ok(None);
    }

    // 工作区反查：vscdb 显式 cwd > 父目录 md5 > base64 祖先目录 > file-tree 路径兜底
    let file_tree = load_file_tree(conv_dir);
    let file_tree_cwd = file_tree
        .as_ref()
        .and_then(|items| items.first())
        .and_then(|f| f.get("filePath"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let meta = vscdb_meta.get(conv_id);
    let cwd = meta
        .and_then(|m| m.cwd.clone())
        .or_else(|| {
            conv_dir
                .parent()
                .and_then(|p| p.file_name().and_then(|s| s.to_str()))
                .filter(|n| n.len() == 32 && n.chars().all(|c| c.is_ascii_hexdigit()))
                .and_then(|n| md5_to_cwd.get(n).cloned())
        })
        .or_else(|| {
            conv_dir
                .ancestors()
                .filter_map(|a| a.file_name().and_then(|s| s.to_str()))
                .find_map(|n| base64_to_cwd.get(n).cloned())
        })
        .or_else(|| file_tree_cwd.as_deref().map(canonicalize_workspace_path));

    let title = meta
        .and_then(|m| m.title.clone())
        .filter(|t| !t.trim().is_empty())
        .or_else(|| {
            first_user_text.as_ref().map(|t| {
                let line = t.lines().next().unwrap_or(t).trim();
                let mut s: String = line.chars().take(60).collect();
                if line.chars().count() > 60 {
                    s.push('…');
                }
                s
            })
        })
        .unwrap_or_else(|| format!("CodeBuddy 会话 {}", &conv_id[..8]));

    // file-tree.json：会话变更的文件树，作为产物记录（含增删行统计）
    let mut artifacts: Vec<RawArtifact> = Vec::new();
    if let Some(items) = &file_tree {
        for f in items {
            let file_path = match f.get("filePath").and_then(|v| v.as_str()) {
                Some(p) if !p.is_empty() => p.to_string(),
                _ => continue,
            };
            let name = f
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    Path::new(&file_path)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("file")
                        .to_string()
                });
            let (mut added, mut removed) = (0i64, 0i64);
            if let Some(versions) = f.get("versions").and_then(|v| v.as_array()) {
                for ver in versions {
                    if let Some(diff) = ver.get("diff") {
                        added += diff.get("addedLines").and_then(|v| v.as_i64()).unwrap_or(0);
                        removed += diff.get("removedLines").and_then(|v| v.as_i64()).unwrap_or(0);
                    }
                }
            }
            artifacts.push(RawArtifact {
                file_name: name.clone(),
                file_path,
                title: name.clone(),
                summary: Some(format!("+{} / -{} 行", added, removed)),
                content: format!("{}\n\n本次会话新增 {} 行、删除 {} 行。", name, added, removed),
                user_facing: true,
                request_feedback: false,
                created_at: created_at.clone(),
                updated_at: updated_at.clone(),
            });
        }
    }

    // 消息 extra 里的 requestId → modelName 补齐 usage_records 的模型列
    for u in &mut usage_records {
        if u.model.is_none() {
            if let Some(m) = req_model.get(&u.identity) {
                u.model = Some(m.clone());
            }
        }
    }

    Ok(Some(RawConversation {
        id: format!("codebuddy:{}", conv_id),
        title,
        workspace_path: cwd.unwrap_or_default(),
        source_app: "codebuddy".to_string(),
        created_at,
        updated_at,
        parse_status: "ok".to_string(),
        source_types: vec!["codebuddy".to_string()],
        messages,
        artifacts,
        usage_records,
    }))
}

/// user 正文被 <additional_data> / <user_query> 标签包裹，抽取真实提问
fn extract_user_text(raw: &str) -> String {
    if let Some(start) = raw.find("<user_query>") {
        let s = start + "<user_query>".len();
        if let Some(end) = raw[s..].find("</user_query>") {
            return raw[s..s + end].trim().to_string();
        }
    }
    let mut cleaned = raw.to_string();
    loop {
        let Some(start) = cleaned.find("<additional_data>") else {
            break;
        };
        let s = start + "<additional_data>".len();
        match cleaned[s..].find("</additional_data>") {
            Some(end) => {
                let stop = s + end + "</additional_data>".len();
                cleaned.replace_range(start..stop, "");
            }
            None => {
                cleaned.truncate(start);
                break;
            }
        }
    }
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        raw.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push_str("\n…(已截断)");
    out
}

/// file-tree 与 history 同级：.../history/<...>/<convId> -> .../file-tree/<...>/<convId>
fn load_file_tree(conv_dir: &Path) -> Option<Vec<Value>> {
    let mut new_path = PathBuf::new();
    let mut replaced = false;
    for comp in conv_dir.components() {
        if !replaced && comp.as_os_str().to_string_lossy() == "history" {
            new_path.push("file-tree");
            replaced = true;
        } else {
            new_path.push(comp);
        }
    }
    if !replaced {
        return None;
    }
    let raw = std::fs::read_to_string(new_path.join("file-tree.json")).ok()?;
    serde_json::from_str::<Value>(&raw)
        .ok()?
        .as_array()
        .cloned()
}

/// CodeBuddyIDE 下的 base64(cwd) 目录名直接给出明文工作区路径
fn collect_base64_workspaces(
    ide_root: &Path,
    md5_to_cwd: &mut HashMap<String, String>,
    base64_to_cwd: &mut HashMap<String, String>,
) {
    let Ok(rd) = std::fs::read_dir(ide_root) else {
        return;
    };
    for entry in rd.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if let Some(cwd) = decode_base64_path(&name) {
            base64_to_cwd.insert(name.clone(), cwd.clone());
            register_md5_workspace(&cwd, md5_to_cwd);
        }
    }
}

/// genie-history/<base64(cwd)>/ 同样给出明文工作区路径
fn collect_genie_history_workspaces(home: &Path, md5_to_cwd: &mut HashMap<String, String>) {
    for variant in ["CodeBuddy CN", "CodeBuddy"] {
        let dir = home.join(format!(
            "Library/Application Support/{}/User/globalStorage/tencent-cloud.coding-copilot/genie-history",
            variant
        ));
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if let Some(cwd) = decode_base64_path(&name) {
                register_md5_workspace(&cwd, md5_to_cwd);
            }
        }
    }
}

fn register_md5_workspace(cwd: &str, md5_to_cwd: &mut HashMap<String, String>) {
    if cwd.is_empty() {
        return;
    }
    let digest = md5::compute(cwd.as_bytes());
    md5_to_cwd
        .entry(format!("{:x}", digest))
        .or_insert_with(|| cwd.to_string());
}

fn decode_base64_path(name: &str) -> Option<String> {
    if name.len() < 12
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=' || c == '-' || c == '_')
    {
        return None;
    }
    use base64::Engine;
    let engines = [
        &base64::engine::general_purpose::STANDARD,
        &base64::engine::general_purpose::STANDARD_NO_PAD,
        &base64::engine::general_purpose::URL_SAFE,
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ];
    for engine in engines {
        if let Ok(bytes) = engine.decode(name) {
            if let Ok(s) = String::from_utf8(bytes) {
                if is_plausible_path(&s) {
                    return Some(s);
                }
            }
        }
    }
    None
}

fn is_plausible_path(s: &str) -> bool {
    s.starts_with('/') && s.matches('/').count() >= 3 && !s.contains('\u{0}')
}

/// codebuddy-sessions.vscdb：最近会话的 title / cwd 元数据
fn load_sessions_vscdb(home: &Path) -> HashMap<String, ConvMeta> {
    let mut map = HashMap::new();
    for variant in ["CodeBuddy CN", "CodeBuddy"] {
        let db = home.join(format!(
            "Library/Application Support/{}/codebuddy-sessions.vscdb",
            variant
        ));
        if !db.is_file() {
            continue;
        }
        let Ok(conn) =
            rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            continue;
        };
        let Ok(mut stmt) =
            conn.prepare("SELECT key, value FROM ItemTable WHERE key LIKE 'session:%'")
        else {
            continue;
        };
        let Ok(rows) =
            stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        else {
            continue;
        };
        for (key, value) in rows.flatten() {
            let Ok(j) = serde_json::from_str::<Value>(&value) else {
                continue;
            };
            let conv_id = j
                .get("conversationId")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| key.trim_start_matches("session:").to_string());
            let cwd = j
                .get("cwd")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let title = j
                .get("title")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.to_string());
            map.insert(conv_id, ConvMeta { cwd, title });
        }
    }
    map
}

fn codebuddy_parser_rev_stale(conn: &Connection) -> bool {
    let stored: Option<String> = conn
        .query_row(
            "SELECT conversation_id FROM sync_state WHERE source_path = ?",
            params![CODEBUDDY_PARSER_REV_KEY],
            |r| r.get(0),
        )
        .ok();
    stored.as_deref() != Some(CODEBUDDY_PARSER_REV)
}

fn mark_codebuddy_synced(conn: &Connection) {
    let now = chrono::Utc::now().to_rfc3339();
    let _ = conn.execute(
        r#"
        INSERT INTO sync_state (source_path, conversation_id, source_type, file_mtime, file_size, synced_at)
        VALUES (?1, ?2, 'codebuddy_parser', 0, 0, ?3)
        ON CONFLICT(source_path) DO UPDATE SET
            conversation_id = excluded.conversation_id,
            synced_at = excluded.synced_at
        "#,
        params![CODEBUDDY_PARSER_REV_KEY, CODEBUDDY_PARSER_REV, now],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_request_usage_into_records() {
        let dir = std::env::temp_dir().join(format!(
            "codebuddy_usage_test_{}",
            std::process::id()
        ));
        let msg_dir = dir.join("messages");
        std::fs::create_dir_all(&msg_dir).unwrap();

        let index = json!({
            "messages": [{"id": "m1"}],
            "requests": [{
                "id": "req1",
                "type": "chat",
                "messages": ["m1"],
                "state": "done",
                "startedAt": 1789785611976i64,
                "usage": {
                    "inputTokens": 27539,
                    "outputTokens": 105,
                    "totalTokens": 27644,
                    "lastTokens": 27644,
                    "cacheTokens": 1000,
                    "cachedWriteTokens": 200,
                    "cachedMissTokens": 26539,
                    "credit": 0.5
                }
            }]
        });
        std::fs::write(dir.join("index.json"), serde_json::to_string(&index).unwrap()).unwrap();

        let msg = json!({
            "role": "user",
            "id": "m1",
            "createdAt": "2026-09-17T10:00:00.000Z",
            "message": serde_json::to_string(&json!({
                "content": [{"type": "text", "text": "hi"}]
            })).unwrap(),
            "extra": serde_json::to_string(&json!({
                "requestId": "req1",
                "modelName": "glm-5.3"
            })).unwrap()
        });
        std::fs::write(
            msg_dir.join("m1.json"),
            serde_json::to_string(&msg).unwrap(),
        )
        .unwrap();

        let conv = parse_conversation(&dir, "abc123", &HashMap::new(), &HashMap::new(), &HashMap::new())
            .unwrap()
            .unwrap();

        assert_eq!(conv.usage_records.len(), 1);
        let u = &conv.usage_records[0];
        assert_eq!(u.identity, "req1");
        assert_eq!(u.model.as_deref(), Some("glm-5.3"));
        // cachedMissTokens 是新鲜输入；cacheTokens/cachedWriteTokens 单列
        assert_eq!(u.input_tokens, Some(26539));
        assert_eq!(u.cache_read_tokens, Some(1000));
        assert_eq!(u.cache_write_tokens, Some(200));
        assert_eq!(u.output_tokens, Some(105));
        assert_eq!(u.credit, Some(0.5));
        assert!(u.occurred_at.as_deref().unwrap().starts_with("2026-"));
        // 消息侧的 token_count 挂载保持不变
        assert_eq!(conv.messages[0].token_count, Some(27644));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// 对本机真实 CodeBuddy 数据目录做一次端到端试跑（默认跳过）
    #[test]
    #[ignore]
    fn test_sync_real_codebuddy_data() {
        let home = dirs::home_dir().unwrap();
        let data_root = home.join("Library/Application Support/CodeBuddyExtension/Data");
        assert!(data_root.is_dir(), "CodeBuddy data root not found at {:?}", data_root);

        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();

        let stats = sync(&conn, false);
        println!(
            "sync stats: new={} updated={} skipped={} errors={}",
            stats.new_count, stats.updated_count, stats.skipped_count, stats.error_count
        );
        assert_eq!(stats.error_count, 0);

        let (total, cb_cnt): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), SUM(CASE WHEN source_app='codebuddy' THEN 1 ELSE 0 END)
                 FROM conversations",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        let (msgs, with_model, with_credit, with_tokens): (i64, i64, i64, i64) = conn
            .query_row(
                "SELECT COUNT(*),
                        SUM(CASE WHEN model_name IS NOT NULL THEN 1 ELSE 0 END),
                        SUM(CASE WHEN credit IS NOT NULL THEN 1 ELSE 0 END),
                        SUM(CASE WHEN token_count IS NOT NULL THEN 1 ELSE 0 END)
                 FROM messages",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        println!(
            "conversations={} codebuddy={} messages={} with_model={} with_credit={} with_tokens={}",
            total, cb_cnt, msgs, with_model, with_credit, with_tokens
        );
        for (wid, title, ws) in conn
            .prepare(
                "SELECT id, title, workspace_path FROM conversations WHERE source_app='codebuddy' LIMIT 5",
            )
            .unwrap()
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
            })
            .unwrap()
            .flatten()
        {
            println!("  sample: {} | {} | {}", wid, title, ws);
        }
        assert!(cb_cnt > 0, "expected at least one codebuddy conversation");
    }

    #[test]
    fn extracts_user_query_tag() {
        let raw = "<additional_data>rules...</additional_data>\n\n<user_query>帮我看下这个项目</user_query>";
        assert_eq!(extract_user_text(raw), "帮我看下这个项目");
    }

    #[test]
    fn strips_additional_data_without_query_tag() {
        let raw = "<additional_data>ctx</additional_data>真实提问";
        assert_eq!(extract_user_text(raw), "真实提问");
    }

    #[test]
    fn falls_back_to_raw_when_only_additional_data() {
        let raw = "<additional_data>只有上下文</additional_data>";
        assert_eq!(extract_user_text(raw), "<additional_data>只有上下文</additional_data>");
    }

    #[test]
    fn decodes_base64_workspace() {
        let name = "L1VzZXJzL3hpeWFuZ3hpZS93b3Jrc3BhY2UvcGVyc29uYWwvQWdlbnREZWNr";
        assert_eq!(
            decode_base64_path(name).as_deref(),
            Some("/Users/xiyangxie/workspace/personal/AgentDeck")
        );
    }

    #[test]
    fn rejects_hex_dir_as_base64() {
        assert!(decode_base64_path("badc582712cac142fd12606bb86001d4").is_none());
    }

    #[test]
    fn md5_workspace_matches_history_layout() {
        let mut map = HashMap::new();
        register_md5_workspace("/Users/xiyangxie/workspace/personal/AgentDeck", &mut map);
        assert!(map.contains_key("badc582712cac142fd12606bb86001d4"));
    }

    #[test]
    fn truncates_long_content() {
        let s = "x".repeat(5000);
        let out = truncate(&s, CONTENT_TRUNCATE);
        assert!(out.contains("已截断"));
        assert!(out.chars().count() < 4100);
    }
}
