//! 技能目录扫描：枚举各 Agent 与本机公共技能目录，维护 skills / skill_instances；
//! 并从 messages 全量重建 skill_usages（显式 Skill 调用 + SKILL.md 读取两类证据）。

use chrono::{Duration, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// 本机公共技能库：agent 目录下的 symlink 指向这里即归为 public
pub const PUBLIC_AGENT: &str = "public";

const AGENT_SLUGS: &[(&str, &str)] = &[
    (".claude", "claude"),
    (".codex", "codex"),
    (".gemini", "gemini"),
    (".qoder", "qoder"),
    (".codebuddy", "codebuddy"),
    (".hermes", "hermes"),
    (".cursor", "cursor"),
    (".workbuddy", "workbuddy"),
    (".grok", "grok"),
    (".openclaw", "openclaw"),
    (".trae", "trae"),
    (".agents", PUBLIC_AGENT),
];

pub fn skill_dir_candidates() -> Vec<(String, PathBuf)> {
    let Some(home) = dirs::home_dir() else {
        return vec![];
    };
    AGENT_SLUGS
        .iter()
        .map(|(dir, agent)| {
            ((*agent).to_string(), home.join(dir).join("skills"))
        })
        .filter(|(_, p)| p.is_dir())
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillItem {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub scope: String,
    pub path: String,
    pub agents: Vec<String>,
    pub use_count: i64,
    pub use_count_30d: i64,
    pub last_used_at: Option<String>,
    pub discovered_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillsSummary {
    pub total: i64,
    pub public_count: i64,
    pub private_count: i64,
    pub used_count: i64,
    pub unused_count: i64,
    pub usage_events: i64,
    pub usage_events_30d: i64,
    pub agents: Vec<String>,
    pub last_scanned_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillUsageRow {
    pub agent: String,
    pub conversation_id: String,
    pub conversation_title: String,
    pub evidence: String,
    pub occurred_at: Option<String>,
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn beijing_start_of_days_ago(days: i64) -> String {
    let tz = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
    let today = Utc::now().with_timezone(&tz).date_naive();
    let start = today - Duration::days(days);
    start
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_local_timezone(tz)
        .single()
        .map(|d| d.to_rfc3339())
        .unwrap_or_default()
}

fn content_hash(bytes: &[u8]) -> String {
    format!("{:016x}", {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut h);
        h.finish()
    })
}

fn parse_front_matter(text: &str) -> (Option<String>, String) {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return (None, String::new());
    }
    let mut name = None;
    let mut desc_parts: Vec<String> = vec![];
    let mut pending_key: Option<String> = None;
    for line in lines {
        let trimmed = line.trim_end();
        if trimmed == "---" || trimmed == "..." {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            let k = k.trim();
            let v = v.trim();
            if k == "name" && !v.is_empty() && !v.starts_with('|') && !v.starts_with('>') {
                name = Some(v.trim_matches('"').trim_matches('\'').to_string());
                pending_key = None;
            } else if k == "description" {
                pending_key = Some("description".to_string());
                if !v.is_empty() && !v.starts_with('|') && !v.starts_with('>') {
                    desc_parts.push(v.trim_matches('"').trim_matches('\'').to_string());
                }
            } else {
                pending_key = None;
            }
        } else if pending_key.as_deref() == Some("description") {
            let cont = trimmed.trim();
            if !cont.is_empty() {
                desc_parts.push(cont.to_string());
            }
        }
    }
    (name, desc_parts.join(" ").chars().take(500).collect())
}

/// 扫描一次：upsert 各 agent 目录实例、归并 canonical、清理失效项、重建使用统计。
pub fn sync_skills(conn: &Connection) -> rusqlite::Result<usize> {
    let now = now_iso();
    let mut total = 0usize;
    let public_home = dirs::home_dir().map(|h| h.join(".agents").join("skills"));
    for (agent, dir) in skill_dir_candidates() {
        total += sync_one_dir(conn, &agent, &dir, public_home.as_deref(), &now)?;
    }
    cleanup_missing_skills(conn)?;
    rebuild_usages(conn)?;
    Ok(total)
}

fn is_public_symlink(md_path: &Path, is_symlink: bool, public_home: Option<&Path>) -> bool {
    if !is_symlink {
        return false;
    }
    match (md_path.canonicalize().ok(), public_home.and_then(|p| p.canonicalize().ok())) {
        (Some(target), Some(ph)) => target.starts_with(&ph),
        _ => false,
    }
}

fn sync_one_dir(
    conn: &Connection,
    agent: &str,
    dir: &Path,
    public_home: Option<&Path>,
    now: &str,
) -> rusqlite::Result<usize> {
    let mut count = 0usize;
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(0),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_symlink = entry.file_type().map(|t| t.is_symlink()).unwrap_or(false);
        let md_path = if path.is_dir() {
            path.join("SKILL.md")
        } else if path.is_file()
            && path
                .file_name()
                .map(|n| n.to_string_lossy().eq_ignore_ascii_case("SKILL.md"))
                .unwrap_or(false)
        {
            path.clone()
        } else {
            continue;
        };
        let Ok(bytes) = fs::read(&md_path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes).to_string();
        let (fm_name, description) = parse_front_matter(&text);
        let dir_name = path
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .filter(|n| !n.is_empty());
        let Some(name) = fm_name.or(dir_name) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let hash = content_hash(&bytes);
        let scope = if is_public_symlink(&md_path, is_symlink, public_home) {
            PUBLIC_AGENT
        } else {
            agent
        };
        let shown_path = path.to_string_lossy().to_string();
        let md_str = md_path.to_string_lossy().to_string();

        conn.execute(
            r#"
            INSERT INTO skill_instances (skill_name, agent, scope, path, skill_md_path, content_hash, description, discovered_at, last_scanned_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
            ON CONFLICT(agent, skill_name, path) DO UPDATE SET
                scope = excluded.scope,
                content_hash = excluded.content_hash,
                description = excluded.description,
                last_scanned_at = excluded.last_scanned_at
            "#,
            params![name, agent, scope, shown_path, md_str, hash, description, now],
        )?;
        count += 1;
    }
    Ok(count)
}

fn cleanup_missing_skills(conn: &Connection) -> rusqlite::Result<()> {
    // 目录已消失的 agent 实例整体失效
    let live_agents: Vec<String> = skill_dir_candidates()
        .into_iter()
        .map(|(a, _)| a)
        .collect();
    if live_agents.is_empty() {
        conn.execute("DELETE FROM skill_instances", [])?;
    } else {
        let quoted = live_agents
            .iter()
            .map(|a| format!("'{}'", a.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(",");
        conn.execute(
            &format!("DELETE FROM skill_instances WHERE agent NOT IN ({})", quoted),
            [],
        )?;
    }
    // path 已消失的实例
    let mut stmt = conn.prepare("SELECT id, skill_md_path FROM skill_instances")?;
    let rows: Vec<(i64, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .flatten()
        .collect();
    for (id, md_path) in rows {
        if !Path::new(&md_path).exists() {
            conn.execute("DELETE FROM skill_instances WHERE id = ?1", params![id])?;
        }
    }
    // canonical 与实例对齐
    conn.execute(
        r#"
        INSERT INTO skills (name, description, primary_path, discovered_at, last_scanned_at)
        SELECT i.skill_name, i.description, i.path, i.last_scanned_at, i.last_scanned_at
        FROM skill_instances i
        JOIN (SELECT skill_name, MIN(id) AS mid FROM skill_instances GROUP BY skill_name) m
          ON i.id = m.mid
        ON CONFLICT(name) DO UPDATE SET
            description = excluded.description,
            primary_path = excluded.primary_path,
            last_scanned_at = excluded.last_scanned_at
        "#,
        [],
    )?;
    conn.execute(
        "DELETE FROM skills WHERE name NOT IN (SELECT DISTINCT skill_name FROM skill_instances)",
        [],
    )?;
    Ok(())
}

fn known_skill_names(conn: &Connection) -> rusqlite::Result<HashSet<String>> {
    let mut stmt = conn.prepare("SELECT name FROM skills")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    Ok(rows.flatten().collect())
}

/// 会话里出现的技能名 → 已登记技能。技能目录名常与 SKILL.md 的 name 不一致
/// （如目录 chatcut-music、name: music），调用参数多用目录名，故按别名归并。
struct NameIndex {
    known: HashSet<String>,
    aliases: HashMap<String, String>,
}

impl NameIndex {
    fn load(conn: &Connection) -> rusqlite::Result<Self> {
        let known = known_skill_names(conn)?;
        let mut aliases: HashMap<String, String> = HashMap::new();
        let mut stmt = conn.prepare("SELECT skill_name, path FROM skill_instances")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for (name, path) in rows.flatten() {
            let dir = Path::new(&path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            if dir.is_empty() || dir == name || known.contains(&dir) {
                continue;
            }
            aliases.entry(dir).or_insert_with(|| name.clone());
        }
        Ok(Self { known, aliases })
    }

    fn contains(&self, name: &str) -> bool {
        self.known.contains(name)
    }

    /// 归并且仅接受已登记技能；已删除技能的历史调用无对应行，丢弃。
    fn resolve(&self, raw: &str) -> Option<String> {
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }
        if self.known.contains(raw) {
            return Some(raw.to_string());
        }
        self.aliases.get(raw).filter(|n| self.known.contains(*n)).cloned()
    }
}

fn agent_from_path(path: &str) -> String {
    for (dir, agent) in AGENT_SLUGS {
        if path.contains(&format!("/{}/", dir)) {
            return (*agent).to_string();
        }
    }
    String::new()
}

fn name_from_skill_md_path(path: &str, index: &NameIndex) -> Option<String> {
    let clean = path.trim_end_matches(['"', '\\', '\'', ' ']);
    let cut = clean.strip_suffix("/SKILL.md").or_else(|| {
        clean
            .split_once("/SKILL.md")
            .map(|(before, _)| before)
    })?;
    let segs: Vec<&str> = cut.split('/').collect();
    let last = *segs.last()?;
    if last.is_empty() || last.starts_with('.') || last == "skills" {
        return None;
    }
    // SKILL.md 的父目录就是技能目录；只有该目录名未登记、而 skills 后一级已登记时
    // 才改用它，避免把 skills/<分类>/<技能> 里的分类名当成技能。
    if index.contains(last) {
        return Some(last.to_string());
    }
    if let Some(idx) = segs.iter().rposition(|s| *s == "skills") {
        if let Some(n) = segs.get(idx + 1) {
            if let Some(resolved) = index.resolve(n) {
                return Some(resolved);
            }
        }
    }
    index.resolve(last)
}

fn explicit_names_from_json(value: &serde_json::Value, index: &NameIndex, out: &mut Vec<String>) {
    let push = |raw: &str, out: &mut Vec<String>| {
        let base = raw.rsplit('/').next().unwrap_or(raw).trim();
        if let Some(name) = index.resolve(base) {
            out.push(name);
        }
    };
    match value {
        serde_json::Value::Object(map) => {
            for key in ["skill", "skill_name", "skillName"] {
                if let Some(serde_json::Value::String(s)) = map.get(key) {
                    push(s, out);
                }
            }
            if let Some(serde_json::Value::String(name)) = map.get("name") {
                push(name, out);
            }
            // 参数以转义 JSON 字符串内嵌（WorkBuddy 等）
            for v in map.values() {
                if let serde_json::Value::String(inner) = v {
                    if inner.trim_start().starts_with('{') || inner.trim_start().starts_with('[') {
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(inner) {
                            explicit_names_from_json(&parsed, index, out);
                        }
                    }
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                explicit_names_from_json(v, index, out);
            }
        }
        _ => {}
    }
}

/// 从 messages 全量重建 skill_usages。证据两类：
/// explicit = Skill/skill/use_skill 工具调用；read = 工具参数中出现 SKILL.md 路径。
pub fn rebuild_usages(conn: &Connection) -> rusqlite::Result<usize> {
    let index = NameIndex::load(conn)?;

    conn.execute("DELETE FROM skill_usages", [])?;

    let mut seen_titles: HashMap<String, String> = HashMap::new();

    let mut title_of = |cid: &str| -> String {
        seen_titles.entry(cid.to_string()).or_insert_with(|| {
            conn.query_row(
                "SELECT title FROM conversations WHERE id = ?1",
                params![cid],
                |r| r.get::<_, String>(0),
            )
            .unwrap_or_default()
        })
        .clone()
    };

    let mut events = 0usize;

    macro_rules! record {
        ($skill:expr, $agent:expr, $cid:expr, $evidence:expr, $occurred:expr) => {
            let title = title_of($cid);
            conn.execute(
                "INSERT INTO skill_usages (skill_name, agent, conversation_id, conversation_title, evidence, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![$skill, $agent, $cid, title, $evidence, $occurred],
            )?;
            events += 1;
        };
    }

    // 1) 显式 Skill 工具调用
    {
        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, source, tool_args, content, created_at FROM messages \
             WHERE tool_name IN ('Skill', 'skill', 'use_skill', 'Skills')",
        )?;
        let rows: Vec<(i64, String, String, Option<String>, String, Option<String>)> = stmt
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })?
            .flatten()
            .collect();
        let mut seen: HashSet<(i64, String)> = HashSet::new();
        for (mid, cid, source, tool_args, content, occurred) in rows {
            let mut names: Vec<String> = vec![];
            if let Some(args) = tool_args.as_deref() {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(args) {
                    explicit_names_from_json(&parsed, &index, &mut names);
                }
            }
            if names.is_empty() && !content.is_empty() {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                    explicit_names_from_json(&parsed, &index, &mut names);
                }
            }
            for n in names {
                if seen.insert((mid, n.clone())) {
                    record!(&n, &source, &cid, "explicit", &occurred);
                }
            }
        }
    }

    // 2) SKILL.md 路径读取
    {
        let re_path = regex::Regex::new(r"/[^\s\x22\x27<>|]*?/SKILL\.md").unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, source, tool_args, created_at FROM messages \
             WHERE tool_args LIKE '%SKILL.md%'",
        )?;
        let rows: Vec<(i64, String, String, Option<String>, Option<String>)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
            .flatten()
            .collect();
        let mut seen: HashSet<(i64, String)> = HashSet::new();
        for (mid, cid, source, tool_args, occurred) in rows {
            let Some(args) = tool_args.as_deref() else {
                continue;
            };
            for cap in re_path.find_iter(args) {
                let path = cap.as_str();
                let Some(name) = name_from_skill_md_path(path, &index) else {
                    continue;
                };
                let agent = {
                    let a = agent_from_path(path);
                    if a.is_empty() { source.clone() } else { a }
                };
                if seen.insert((mid, name.clone())) {
                    record!(&name, &agent, &cid, "read", &occurred);
                }
            }
        }
    }

    let _ = conn.execute("PRAGMA optimize", []);
    Ok(events)
}

pub fn list_skills_ex(
    conn: &Connection,
    search: Option<&str>,
    agent: Option<&str>,
) -> rusqlite::Result<Vec<SkillItem>> {
    let cutoff = beijing_start_of_days_ago(30);
    let like = format!("%{}%", search.unwrap_or("").trim());
    let agent_filter = agent.filter(|a| !a.is_empty()).map(|a| a.to_string());

    let mut items: Vec<SkillItem> = vec![];
    {
        let mut stmt = conn.prepare(
            r#"
            SELECT s.id, s.name, s.description, s.primary_path, s.discovered_at
            FROM skills s
            WHERE (?1 = '%%' OR s.name LIKE ?1 OR s.description LIKE ?1)
              AND (?2 IS NULL OR EXISTS (
                    SELECT 1 FROM skill_instances i
                    WHERE i.skill_name = s.name AND i.agent = ?2))
            ORDER BY s.name COLLATE NOCASE
            "#,
        )?;
        let rows = stmt.query_map(params![like, agent_filter], |r| {
            Ok(SkillItem {
                id: r.get(0)?,
                name: r.get(1)?,
                description: r.get(2)?,
                scope: String::new(),
                path: r.get(3)?,
                agents: vec![],
                use_count: 0,
                use_count_30d: 0,
                last_used_at: None,
                discovered_at: r.get(4)?,
            })
        })?;
        for row in rows.flatten() {
            items.push(row);
        }
    }

    for item in &mut items {
        {
            let mut stmt = conn.prepare(
                "SELECT agent, scope FROM skill_instances WHERE skill_name = ?1 ORDER BY scope = 'public' DESC, agent",
            )?;
            let rows = stmt.query_map(params![item.name], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            let mut agents = vec![];
            let mut public = false;
            for row in rows.flatten() {
                agents.push(row.0);
                if row.1 == PUBLIC_AGENT {
                    public = true;
                }
            }
            item.agents = agents;
            item.scope = if public {
                PUBLIC_AGENT.to_string()
            } else {
                "private".to_string()
            };
        }
        let mut stmt = conn.prepare(
            r#"
            SELECT COUNT(*),
                   SUM(CASE WHEN occurred_at >= ?2 THEN 1 ELSE 0 END),
                   MAX(occurred_at)
            FROM skill_usages WHERE skill_name = ?1
            "#,
        )?;
        let (cnt, cnt30, last): (i64, Option<i64>, Option<String>) = stmt
            .query_row(params![item.name, cutoff], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap_or((0, None, None));
        item.use_count = cnt;
        item.use_count_30d = cnt30.unwrap_or(0);
        item.last_used_at = last.and_then(|s| crate::db::to_beijing_iso(Some(s)));
    }
    Ok(items)
}

pub fn get_skills_summary(conn: &Connection) -> rusqlite::Result<SkillsSummary> {
    let cutoff = beijing_start_of_days_ago(30);
    let mut stmt = conn.prepare("SELECT COUNT(*), MAX(last_scanned_at) FROM skills")?;
    let (total, last_scanned): (i64, Option<String>) =
        stmt.query_row([], |r| Ok((r.get(0)?, r.get(1)?)))?;

    let public_count: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT skill_name) FROM skill_instances WHERE scope = 'public'",
        [],
        |r| r.get(0),
    )?;
    let used_count: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT skill_name) FROM skill_usages",
        [],
        |r| r.get(0),
    )?;
    let usage_events: i64 = conn.query_row("SELECT COUNT(*) FROM skill_usages", [], |r| r.get(0))?;
    let usage_events_30d: i64 = conn.query_row(
        "SELECT COUNT(*) FROM skill_usages WHERE occurred_at >= ?1",
        params![cutoff],
        |r| r.get(0),
    )?;

    let mut agents: BTreeSet<String> = BTreeSet::new();
    {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT agent FROM skill_instances ORDER BY scope = 'public' DESC, agent",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        for row in rows.flatten() {
            agents.insert(row);
        }
    }

    Ok(SkillsSummary {
        total,
        public_count,
        private_count: (total - public_count).max(0),
        used_count,
        unused_count: (total - used_count).max(0),
        usage_events,
        usage_events_30d,
        agents: agents.into_iter().collect(),
        last_scanned_at: last_scanned,
    })
}

pub fn get_skill_detail(conn: &Connection, name: &str) -> rusqlite::Result<Vec<SkillUsageRow>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT agent, conversation_id, conversation_title, evidence, occurred_at
        FROM skill_usages WHERE skill_name = ?1
        ORDER BY occurred_at DESC LIMIT 50
        "#,
    )?;
    let rows = stmt.query_map(params![name], |r| {
        Ok(SkillUsageRow {
            agent: r.get(0)?,
            conversation_id: r.get(1)?,
            conversation_title: r.get(2)?,
            evidence: r.get(3)?,
            occurred_at: r
                .get::<_, Option<String>>(4)?
                .and_then(|s| crate::db::to_beijing_iso(Some(s))),
        })
    })?;
    Ok(rows.flatten().collect())
}

/// 读取技能 SKILL.md 原文（列表 lite 查询不带 content 时的按需拉取）
pub fn get_skill_content(conn: &Connection, id: i64) -> rusqlite::Result<Option<String>> {
    let path: Option<String> = conn
        .query_row(
            "SELECT i.skill_md_path FROM skills s \
             JOIN skill_instances i ON i.skill_name = s.name \
             WHERE s.id = ?1 ORDER BY i.scope = 'public' DESC, i.id LIMIT 1",
            params![id],
            |r| r.get(0),
        )
        .ok();
    let Some(p) = path else {
        return Ok(None);
    };
    let content = fs::read_to_string(&p).ok();
    Ok(content.map(|c| {
        const CAP: usize = 200_000;
        if c.len() > CAP {
            let mut end = CAP;
            while !c.is_char_boundary(end) {
                end -= 1;
            }
            c[..end].to_string()
        } else {
            c
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_front_matter_parse() {
        let md = "---\nname: demo\ndescription: >-\n  first line\n  second line\nmetadata:\n  version: \"1\"\n---\n# body\n";
        let (name, desc) = parse_front_matter(md);
        assert_eq!(name.as_deref(), Some("demo"));
        assert_eq!(desc, "first line second line");
    }

    fn index_of(names: &[&str], aliases: &[(&str, &str)]) -> NameIndex {
        NameIndex {
            known: names.iter().map(|s| s.to_string()).collect(),
            aliases: aliases
                .iter()
                .map(|(from, to)| (from.to_string(), to.to_string()))
                .collect(),
        }
    }

    #[test]
    fn test_name_from_path() {
        let index = index_of(&["ego-browser", "openhue", "music"], &[("chatcut-music", "music")]);
        assert_eq!(
            name_from_skill_md_path("/Users/x/.claude/skills/ego-browser/SKILL.md", &index)
                .as_deref(),
            Some("ego-browser")
        );
        assert_eq!(
            name_from_skill_md_path(
                "/Users/x/workspace/hermes-agent/skills/smart-home/openhue/SKILL.md\"",
                &index
            )
            .as_deref(),
            Some("openhue")
        );
        // 目录名与 name 不一致时按别名归并
        assert_eq!(
            name_from_skill_md_path("/Users/x/.claude/skills/chatcut-music/SKILL.md", &index)
                .as_deref(),
            Some("music")
        );
        // 未登记技能（已删除）不产生统计
        assert_eq!(
            name_from_skill_md_path("/Users/x/.claude/skills/gone-skill/SKILL.md", &index),
            None
        );
    }

    #[test]
    fn test_explicit_names_nested_json() {
        let index = index_of(&["aihot"], &[("chatcut-aihot", "aihot")]);
        let v: serde_json::Value = serde_json::from_str(
            r#"[{"args": "{\"skill\": \"chatcut-aihot\"}", "name": "Skill"}]"#,
        )
        .unwrap();
        let mut out = vec![];
        explicit_names_from_json(&v, &index, &mut out);
        assert_eq!(out, vec!["aihot".to_string()]);
    }

    #[test]
    fn test_sync_skills_on_real_machine() {
        let conn = Connection::open_in_memory().unwrap();
        // 手工建表（与 db.rs init_schema 保持同名同列）
        conn.execute_batch(
            "CREATE TABLE skills (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, description TEXT NOT NULL DEFAULT '', primary_path TEXT NOT NULL DEFAULT '', discovered_at TEXT NOT NULL, last_scanned_at TEXT NOT NULL);
             CREATE TABLE skill_instances (id INTEGER PRIMARY KEY AUTOINCREMENT, skill_name TEXT NOT NULL, agent TEXT NOT NULL, scope TEXT NOT NULL, path TEXT NOT NULL, skill_md_path TEXT NOT NULL, content_hash TEXT NOT NULL DEFAULT '', description TEXT NOT NULL DEFAULT '', discovered_at TEXT NOT NULL, last_scanned_at TEXT NOT NULL, UNIQUE(agent, skill_name, path));
             CREATE TABLE skill_usages (id INTEGER PRIMARY KEY AUTOINCREMENT, skill_name TEXT NOT NULL, agent TEXT NOT NULL DEFAULT '', conversation_id TEXT NOT NULL DEFAULT '', conversation_title TEXT NOT NULL DEFAULT '', evidence TEXT NOT NULL, occurred_at TEXT);
             CREATE TABLE conversations (id TEXT PRIMARY KEY, title TEXT NOT NULL DEFAULT '');
             CREATE TABLE messages (id INTEGER PRIMARY KEY AUTOINCREMENT, conversation_id TEXT, tool_name TEXT, tool_args TEXT, content TEXT, source TEXT, created_at TEXT);",
        )
        .unwrap();
        let n = sync_skills(&conn).unwrap();
        println!("scanned {} skill instances", n);
        let items = list_skills_ex(&conn, None, None).unwrap();
        assert!(items.iter().any(|i| !i.name.is_empty()));
    }
}
