pub mod db;
pub mod sync;

use db::{
    DbState, DashboardStats, WorkspaceStat, ConversationItem, MessageItem, SearchResultItem,
    WorkspaceDetailStats, fetch_dashboard_stats, fetch_workspaces, fetch_conversations, fetch_conversation_messages,
    toggle_star_session, search_global_messages, fetch_workspace_detail_stats
};
use serde::{Deserialize, Serialize};
use sync::{SyncResultInfo, execute_sync, get_agent_source_paths};
use tauri::{State, Manager, Emitter};
use std::collections::HashMap;

#[tauri::command]
fn get_dashboard_stats(state: State<'_, DbState>) -> Result<DashboardStats, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    fetch_dashboard_stats(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_workspace_detail(workspace_path: String, state: State<'_, DbState>) -> Result<WorkspaceDetailStats, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    fetch_workspace_detail_stats(&conn, &workspace_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_workspaces(search: Option<String>, state: State<'_, DbState>) -> Result<Vec<WorkspaceStat>, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    fetch_workspaces(&conn, search.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_conversations(
    workspace: Option<String>,
    search: Option<String>,
    starred_only: Option<bool>,
    state: State<'_, DbState>,
) -> Result<Vec<ConversationItem>, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    fetch_conversations(&conn, workspace.as_deref(), search.as_deref(), starred_only.unwrap_or(false))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_conversation_messages(
    conversation_id: String,
    state: State<'_, DbState>,
) -> Result<Vec<MessageItem>, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    fetch_conversation_messages(&conn, &conversation_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn toggle_star(
    conversation_id: String,
    state: State<'_, DbState>,
) -> Result<bool, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    toggle_star_session(&conn, &conversation_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn search_messages(
    query: String,
    role: Option<String>,
    limit: Option<usize>,
    state: State<'_, DbState>,
) -> Result<Vec<SearchResultItem>, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    search_global_messages(&conn, &query, role.as_deref(), limit.unwrap_or(30))
        .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmTestResult {
    pub success: bool,
    pub message: String,
    pub latency_ms: u64,
}

#[tauri::command]
async fn test_llm_connection(
    base_url: String,
    api_key: String,
    model: String,
) -> Result<LlmTestResult, String> {
    let start = std::time::Instant::now();
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return Ok(LlmTestResult {
                success: false,
                message: format!("HTTP客户端初始化失败: {}", e),
                latency_ms: 0,
            });
        }
    };

    let payload = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "Ping: reply 1 word"}],
        "max_tokens": 5
    });

    let mut req = client.post(&url).json(&payload);
    if !api_key.trim().is_empty() {
        req = req.header("Authorization", format!("Bearer {}", api_key.trim()));
    }

    match req.send().await {
        Ok(res) => {
            let latency_ms = start.elapsed().as_millis() as u64;
            let status = res.status();
            if status.is_success() {
                Ok(LlmTestResult {
                    success: true,
                    message: "连接成功！模型响应正常".to_string(),
                    latency_ms,
                })
            } else {
                let err_body = res.text().await.unwrap_or_default();
                let mut err_msg = format!("HTTP {}", status);
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&err_body) {
                    if let Some(msg) = val.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
                        err_msg = msg.to_string();
                    } else if let Some(msg) = val.get("message").and_then(|m| m.as_str()) {
                        err_msg = msg.to_string();
                    }
                }
                Ok(LlmTestResult {
                    success: false,
                    message: format!("连接失败: {}", err_msg),
                    latency_ms,
                })
            }
        }
        Err(e) => {
            let latency_ms = start.elapsed().as_millis() as u64;
            Ok(LlmTestResult {
                success: false,
                message: format!("网络连接异常: {}", e),
                latency_ms,
            })
        }
    }
}

#[tauri::command]
fn trigger_sync(full: Option<bool>) -> Result<SyncResultInfo, String> {
    Ok(execute_sync(full.unwrap_or(false)))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db_state = DbState::new().expect("Failed to initialize SQLite database");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(db_state)
        .invoke_handler(tauri::generate_handler![
            get_dashboard_stats,
            get_workspace_detail,
            list_workspaces,
            list_conversations,
            get_conversation_messages,
            toggle_star,
            search_messages,
            trigger_sync,
            test_llm_connection
        ])
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title("AgentDeck - AI Coding Cockpit");
            }

            // 启动后台多源智能监听线程（每 10 秒探测数据源 mtime 变动）
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut last_mtimes: HashMap<std::path::PathBuf, std::time::SystemTime> = HashMap::new();
                // 首次填充初始时间戳
                for src in get_agent_source_paths() {
                    if let Ok(meta) = src.metadata() {
                        if let Ok(mtime) = meta.modified() {
                            last_mtimes.insert(src, mtime);
                        }
                    }
                }

                loop {
                    std::thread::sleep(std::time::Duration::from_secs(10));
                    let sources = get_agent_source_paths();
                    let mut changed = false;

                    for src in sources {
                        if let Ok(meta) = src.metadata() {
                            if let Ok(mtime) = meta.modified() {
                                if let Some(prev) = last_mtimes.get(&src) {
                                    if *prev != mtime {
                                        changed = true;
                                        last_mtimes.insert(src.clone(), mtime);
                                    }
                                } else {
                                    last_mtimes.insert(src.clone(), mtime);
                                }
                            }
                        }
                    }

                    if changed {
                        let _res = execute_sync(false);
                        let _ = app_handle.emit("sync-completed", ());
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
