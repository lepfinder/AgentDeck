use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::importers::{ImporterStats, SyncEngine};

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
        let cursor_db = home.join("Library/Application Support/Cursor/User/globalStorage/state.vscdb");
        if cursor_db.exists() {
            paths.push(cursor_db);
        }
        let cursor_wal = home.join("Library/Application Support/Cursor/User/globalStorage/state.vscdb-wal");
        if cursor_wal.exists() {
            paths.push(cursor_wal);
        }

        // 2. Antigravity: 探测 brain 目录及其最新的活跃 transcript.jsonl
        let brain_dir = home.join(".gemini/antigravity-ide/brain");
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

    record_sync_run(&conn, if full { "full" } else { "incremental" }, &result, &started_at);
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
        println!("[AgentDeck SyncEngine] 检测到排队请求，继续执行 (full: {})...", want_full);
    }

    aggregate.unwrap_or_else(queued_result)
}
