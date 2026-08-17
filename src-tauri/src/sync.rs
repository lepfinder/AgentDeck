use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResultInfo {
    pub success: bool,
    pub new_count: u32,
    pub updated_count: u32,
    pub message: String,
}

/// 获取用户主目录下各 Agent 的关键路径
pub fn get_agent_source_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = dirs::home_dir() {
        // 1. Cursor
        let cursor_db = home.join("Library/Application Support/Cursor/User/globalStorage/state.vscdb");
        if cursor_db.exists() {
            paths.push(cursor_db);
        }

        // 2. Antigravity
        let ag_dir = home.join(".gemini/antigravity-ide/brain");
        if ag_dir.exists() {
            paths.push(ag_dir);
        }

        // 3. Claude Code
        let claude_dir = home.join(".claude/projects");
        if claude_dir.exists() {
            paths.push(claude_dir);
        }

        // 4. Codex
        let codex_dir = home.join(".codex/sessions");
        if codex_dir.exists() {
            paths.push(codex_dir);
        }

        // 5. Hermes
        let hermes_dir = home.join(".hermes");
        if hermes_dir.exists() {
            paths.push(hermes_dir);
        }

        // 6. WorkBuddy
        let wb_dir = home.join(".workbuddy/projects");
        if wb_dir.exists() {
            paths.push(wb_dir);
        }
    }
    paths
}

/// 执行多源同步（调用已验证的同步引擎脚本）
pub fn execute_sync(full: bool) -> SyncResultInfo {
    // 定位 python sync 脚本路径
    let mut script_path = PathBuf::from("/Users/xiyangxie/workspace/personal/aicoding-chat-viewer/sync.py");
    if !script_path.exists() {
        if let Some(home) = dirs::home_dir() {
            let alt = home.join("workspace/personal/aicoding-chat-viewer/sync.py");
            if alt.exists() {
                script_path = alt;
            }
        }
    }

    if !script_path.exists() {
        return SyncResultInfo {
            success: false,
            new_count: 0,
            updated_count: 0,
            message: format!("找不到同步脚本: {:?}", script_path),
        };
    }

    let mut cmd = Command::new("python3");
    cmd.arg(&script_path);
    if full {
        cmd.arg("--full");
    } else {
        cmd.arg("--incremental");
    }

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let msg = if !stdout.trim().is_empty() {
                stdout.trim().to_string()
            } else if !stderr.trim().is_empty() {
                stderr.trim().to_string()
            } else {
                "同步完成".to_string()
            };

            SyncResultInfo {
                success: output.status.success(),
                new_count: 0,
                updated_count: 0,
                message: msg,
            }
        }
        Err(e) => SyncResultInfo {
            success: false,
            new_count: 0,
            updated_count: 0,
            message: format!("同步执行失败: {}", e),
        },
    }
}
