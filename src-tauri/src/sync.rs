use crate::importers::{ImporterStats, SyncEngine};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResultInfo {
    pub success: bool,
    pub new_count: u32,
    pub updated_count: u32,
    pub skipped_count: u32,
    pub error_count: u32,
    pub message: String,
    pub details: Vec<ImporterStats>,
}

/// 获取用户主目录下各 Agent 的关键路径与活跃文件（用于高灵敏度变动检测）
pub fn get_agent_source_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = dirs::home_dir() {
        // 1. Cursor
        let cursor_db =
            home.join("Library/Application Support/Cursor/User/globalStorage/state.vscdb");
        if cursor_db.exists() {
            paths.push(cursor_db);
        }
        let cursor_wal =
            home.join("Library/Application Support/Cursor/User/globalStorage/state.vscdb-wal");
        if cursor_wal.exists() {
            paths.push(cursor_wal);
        }

        // 2. Antigravity: 探测 Antigravity (桌面端) 与 Antigravity IDE 的 brain 目录及最新活跃 transcript.jsonl
        let ag_brain_dirs = [
            home.join(".gemini/antigravity/brain"),
            home.join(".gemini/antigravity-ide/brain"),
        ];
        for brain_dir in ag_brain_dirs {
            if brain_dir.is_dir() {
                paths.push(brain_dir.clone());
                if let Ok(entries) = std::fs::read_dir(&brain_dir) {
                    for e in entries.flatten().take(50) {
                        let transcript = e.path().join(".system_generated/logs/transcript.jsonl");
                        if transcript.is_file() {
                            paths.push(transcript);
                        }
                    }
                }
            }
        }

        // 3. Claude Code
        let claude_dir = home.join(".claude/projects");
        if claude_dir.is_dir() {
            paths.push(claude_dir);
        }

        // 4. Codex
        let codex_dir = home.join(".codex/sessions");
        if codex_dir.is_dir() {
            paths.push(codex_dir);
        }

        // 5. Hermes
        let hermes_db = home.join(".hermes/state.db");
        if hermes_db.exists() {
            paths.push(hermes_db);
        }
        let hermes_wal = home.join(".hermes/state.db-wal");
        if hermes_wal.exists() {
            paths.push(hermes_wal);
        }

        // 6. WorkBuddy
        let wb_dir = home.join(".workbuddy/projects");
        if wb_dir.is_dir() {
            paths.push(wb_dir);
        }
    }
    paths
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPathInfo {
    pub path: String,
    pub display_path: String,
    pub description: String,
    pub exists: bool,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSourceInfo {
    pub id: String,
    pub name: String,
    pub detected: bool,
    pub session_count: i64,
    pub paths: Vec<AgentPathInfo>,
}

/// 收集所有受支持的 Coding Agent、扫描路径及本机存在与会话统计
pub fn collect_agent_sources(conn: &rusqlite::Connection) -> Vec<AgentSourceInfo> {
    let mut session_counts = std::collections::HashMap::new();
    if let Ok(mut stmt) =
        conn.prepare("SELECT source_app, COUNT(*) FROM conversations GROUP BY source_app")
    {
        if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        {
            for item in rows.flatten() {
                session_counts.insert(item.0, item.1);
            }
        }
    }

    let home = dirs::home_dir();

    let make_info = |rel: &str, desc: &str| -> AgentPathInfo {
        let full = match &home {
            Some(h) => h.join(rel),
            None => std::path::PathBuf::from(format!("/{}", rel)),
        };
        let exists = full.exists();
        let is_dir = full.is_dir();
        AgentPathInfo {
            path: full.to_string_lossy().to_string(),
            display_path: format!("~/{}", rel),
            description: desc.to_string(),
            exists,
            is_dir,
        }
    };

    let agents = vec![
        (
            "cursor",
            "Cursor",
            vec![
                make_info(
                    "Library/Application Support/Cursor/User/globalStorage/state.vscdb",
                    "全局对话数据库 (SQLite)",
                ),
                make_info(
                    "Library/Application Support/Cursor/User/workspaceStorage",
                    "工作区状态与元数据缓存",
                ),
                make_info(".cursor/projects", "项目会话索引"),
            ],
        ),
        (
            "antigravity",
            "Google Antigravity",
            vec![
                make_info(".gemini/antigravity/brain", "桌面端会话记录 (brain)"),
                make_info(".gemini/antigravity-ide/brain", "IDE 会话记录 (brain)"),
                make_info(".gemini/antigravity/conversations", "桌面端媒体与步进数据"),
                make_info(".gemini/antigravity-ide/conversations", "IDE 媒体与步进数据"),
            ],
        ),
        (
            "claude",
            "Claude Code",
            vec![make_info(".claude/projects", "项目与会话历史 (JSONL)")],
        ),
        (
            "codex",
            "Codex",
            vec![make_info(".codex/sessions", "CLI 交互会话存储")],
        ),
        (
            "hermes",
            "Hermes",
            vec![
                make_info(".hermes", "Hermes Agent 会话主目录"),
                make_info(".hermes/state.db", "SQLite 状态数据库"),
            ],
        ),
        (
            "workbuddy",
            "WorkBuddy",
            vec![make_info(".workbuddy/projects", "任务与交互会话 (.jsonl)")],
        ),
    ];

    agents
        .into_iter()
        .map(|(id, name, paths)| {
            let detected = paths.iter().any(|p| p.exists);
            let session_count = *session_counts.get(id).unwrap_or(&0);
            AgentSourceInfo {
                id: id.to_string(),
                name: name.to_string(),
                detected,
                session_count,
                paths,
            }
        })
        .collect()
}

static IS_SYNCING: AtomicBool = AtomicBool::new(false);
static PENDING_SYNC: AtomicBool = AtomicBool::new(false);
static PENDING_FULL: AtomicBool = AtomicBool::new(false);

struct SyncGuard;

impl Drop for SyncGuard {
    fn drop(&mut self) {
        IS_SYNCING.store(false, Ordering::SeqCst);
    }
}

fn queued_result() -> SyncResultInfo {
    SyncResultInfo {
        success: true,
        new_count: 0,
        updated_count: 0,
        skipped_count: 0,
        error_count: 0,
        message: "同步任务已排队，将在当前任务结束后自动执行".to_string(),
        details: vec![],
    }
}

fn merge_results(into: &mut SyncResultInfo, from: SyncResultInfo) {
    into.success = into.success && from.success;
    into.new_count += from.new_count;
    into.updated_count += from.updated_count;
    into.skipped_count += from.skipped_count;
    into.error_count += from.error_count;
    into.details.extend(from.details);
    into.message = from.message;
}

fn record_sync_run(
    conn: &rusqlite::Connection,
    mode: &str,
    result: &SyncResultInfo,
    started_at: &str,
) {
    let finished_at = chrono::Utc::now().to_rfc3339();
    let _ = conn.execute(
        r#"
        INSERT INTO sync_runs (
            started_at, finished_at, mode, new_count, updated_count, skipped_count, error_count, message
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        rusqlite::params![
            started_at,
            finished_at,
            mode,
            result.new_count as i64,
            result.updated_count as i64,
            result.skipped_count as i64,
            result.error_count as i64,
            &result.message,
        ],
    );
}

fn run_once(full: bool) -> SyncResultInfo {
    let started_at = chrono::Utc::now().to_rfc3339();
    let db_path = crate::db::get_database_path();
    let conn = match rusqlite::Connection::open(&db_path) {
        Ok(c) => c,
        Err(e) => {
            return SyncResultInfo {
                success: false,
                new_count: 0,
                updated_count: 0,
                skipped_count: 0,
                error_count: 1,
                message: format!("无法连接数据库: {}", e),
                details: vec![],
            };
        }
    };

    crate::db::apply_write_pragmas(&conn);
    let _ = crate::db::init_schema(&conn);

    let incremental = !full;
    let (new_cnt, updated_cnt, skipped_cnt, error_cnt, details) =
        SyncEngine::run_all(&conn, incremental);

    let msg = format!(
        "原生同步完成: 新增 {} 条会话, 更新 {} 条, 跳过 {} 条, 错误 {} 条",
        new_cnt, updated_cnt, skipped_cnt, error_cnt
    );

    let result = SyncResultInfo {
        success: error_cnt == 0 || (new_cnt + updated_cnt > 0),
        new_count: new_cnt,
        updated_count: updated_cnt,
        skipped_count: skipped_cnt,
        error_count: error_cnt,
        message: msg,
        details,
    };

    record_sync_run(
        &conn,
        if full { "full" } else { "incremental" },
        &result,
        &started_at,
    );
    result
}

/// 纯 Rust 原生执行多源同步（冲突请求会排队，不静默丢弃）
pub fn execute_sync(full: bool) -> SyncResultInfo {
    if IS_SYNCING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        PENDING_SYNC.store(true, Ordering::SeqCst);
        if full {
            PENDING_FULL.store(true, Ordering::SeqCst);
        }
        return queued_result();
    }
    let _guard = SyncGuard;

    let mut want_full = full;
    let mut aggregate: Option<SyncResultInfo> = None;

    loop {
        let result = run_once(want_full);
        match aggregate.as_mut() {
            Some(acc) => merge_results(acc, result),
            None => aggregate = Some(result),
        }

        let has_pending = PENDING_SYNC.swap(false, Ordering::SeqCst);
        if !has_pending {
            break;
        }
        want_full = PENDING_FULL.swap(false, Ordering::SeqCst);
        println!(
            "[AgentDeck SyncEngine] 检测到排队请求，继续执行 (full: {})...",
            want_full
        );
    }

    aggregate.unwrap_or_else(queued_result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_collect_agent_sources_counts_and_paths() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            r#"
            CREATE TABLE conversations (
                id TEXT PRIMARY KEY,
                source_app TEXT
            );
            "#,
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO conversations (id, source_app) VALUES ('c1', 'cursor'), ('c2', 'cursor'), ('ag1', 'antigravity');",
            [],
        )
        .unwrap();

        let sources = collect_agent_sources(&conn);
        assert_eq!(sources.len(), 6);

        let cursor_info = sources.iter().find(|s| s.id == "cursor").unwrap();
        assert_eq!(cursor_info.name, "Cursor");
        assert_eq!(cursor_info.session_count, 2);
        assert!(!cursor_info.paths.is_empty());

        let ag_info = sources.iter().find(|s| s.id == "antigravity").unwrap();
        assert_eq!(ag_info.name, "Google Antigravity");
        assert_eq!(ag_info.session_count, 1);
        assert_eq!(ag_info.paths.len(), 4);

        let claude_info = sources.iter().find(|s| s.id == "claude").unwrap();
        assert_eq!(claude_info.session_count, 0);
    }
}

