use serde::{Deserialize, Serialize};
use std::path::PathBuf;
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

use std::sync::atomic::{AtomicBool, Ordering};

static IS_SYNCING: AtomicBool = AtomicBool::new(false);

struct SyncGuard;

impl Drop for SyncGuard {
    fn drop(&mut self) {
        IS_SYNCING.store(false, Ordering::SeqCst);
    }
}

/// 纯 Rust 原生执行多源同步 (含全局防重入互斥锁)
pub fn execute_sync(full: bool) -> SyncResultInfo {
    // 防重入互斥：如果已有任务在运行，立即快速返回，避免 SQLite 事务争抢死锁
    if IS_SYNCING.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return SyncResultInfo {
            success: true,
            new_count: 0,
            updated_count: 0,
            skipped_count: 0,
            error_count: 0,
            message: "同步任务正在执行中，已自动合并".to_string(),
            details: vec![],
        };
    }
    let _guard = SyncGuard;

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

    // 确保 schema 已初始化
    let _ = crate::db::init_schema(&conn);

    let incremental = !full;
    let (new_cnt, updated_cnt, skipped_cnt, error_cnt, details) = SyncEngine::run_all(&conn, incremental);

    let msg = format!(
        "原生同步完成: 新增 {} 条会话, 更新 {} 条, 跳过 {} 条, 错误 {} 条",
        new_cnt, updated_cnt, skipped_cnt, error_cnt
    );

    SyncResultInfo {
        success: error_cnt == 0 || (new_cnt + updated_cnt > 0),
        new_count: new_cnt,
        updated_count: updated_cnt,
        skipped_count: skipped_cnt,
        error_count: error_cnt,
        message: msg,
        details,
    }
}
