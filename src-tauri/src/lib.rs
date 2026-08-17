pub mod db;
pub mod sync;

use db::{
    DbState, DashboardStats, WorkspaceStat, ConversationItem, MessageItem, SearchResultItem,
    WorkspaceDetailStats, fetch_dashboard_stats, fetch_workspaces, fetch_conversations, fetch_conversation_messages,
    toggle_star_session, search_global_messages, fetch_workspace_detail_stats
};
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
            trigger_sync
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
