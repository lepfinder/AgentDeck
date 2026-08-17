pub mod db;

use db::{
    DbState, DashboardStats, WorkspaceStat, ConversationItem, MessageItem, SearchResultItem,
    fetch_dashboard_stats, fetch_workspaces, fetch_conversations, fetch_conversation_messages,
    toggle_star_session, search_global_messages
};
use tauri::{State, Manager};

#[tauri::command]
fn get_dashboard_stats(state: State<'_, DbState>) -> Result<DashboardStats, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    fetch_dashboard_stats(&conn).map_err(|e| e.to_string())
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db_state = DbState::new().expect("Failed to initialize SQLite database");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(db_state)
        .invoke_handler(tauri::generate_handler![
            get_dashboard_stats,
            list_workspaces,
            list_conversations,
            get_conversation_messages,
            toggle_star,
            search_messages
        ])
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title("AgentDeck - AI Coding Cockpit");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
