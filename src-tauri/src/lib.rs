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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmEndpointConfig {
    pub provider_name: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCompletionResult {
    pub success: bool,
    pub content: String,
    pub provider_used: String,
    pub is_fallback: bool,
    pub latency_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineTestResult {
    pub primary: LlmTestResult,
    pub fallback: Option<LlmTestResult>,
    pub overall_success: bool,
    pub message: String,
}

#[tauri::command]
async fn call_llm_with_fallback(
    primary: LlmEndpointConfig,
    fallback: Option<LlmEndpointConfig>,
    messages: Vec<serde_json::Value>,
    max_tokens: Option<u32>,
) -> Result<LlmCompletionResult, String> {
    let start = std::time::Instant::now();
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return Ok(LlmCompletionResult {
                success: false,
                content: String::new(),
                provider_used: primary.provider_name,
                is_fallback: false,
                latency_ms: 0,
                error: Some(format!("HTTP客户端构建失败: {}", e)),
            });
        }
    };

    // 1. 先尝试主力模型 (Primary)
    let primary_url = format!("{}/chat/completions", primary.base_url.trim_end_matches('/'));
    let primary_payload = serde_json::json!({
        "model": primary.model,
        "messages": messages,
        "max_tokens": max_tokens.unwrap_or(2048),
    });

    let mut primary_req = client.post(&primary_url).json(&primary_payload);
    if !primary.api_key.trim().is_empty() {
        primary_req = primary_req.header("Authorization", format!("Bearer {}", primary.api_key.trim()));
    }

    let primary_err_msg = match primary_req.send().await {
        Ok(res) if res.status().is_success() => {
            let latency_ms = start.elapsed().as_millis() as u64;
            if let Ok(data) = res.json::<serde_json::Value>().await {
                if let Some(content) = data.get("choices")
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.get("message"))
                    .and_then(|m| m.get("content"))
                    .and_then(|s| s.as_str()) {
                    return Ok(LlmCompletionResult {
                        success: true,
                        content: content.to_string(),
                        provider_used: primary.provider_name,
                        is_fallback: false,
                        latency_ms,
                        error: None,
                    });
                }
            }
            "主力模型未返回有效的 message.content".to_string()
        }
        Ok(res) => {
            let status = res.status();
            let err_txt = res.text().await.unwrap_or_default();
            let mut msg = format!("HTTP {}", status);
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&err_txt) {
                if let Some(m) = val.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
                    msg = m.to_string();
                }
            }
            msg
        }
        Err(e) => {
            format!("网络错误: {}", e)
        }
    };

    // 2. 如果主力模型失败且配置了备用模型，无缝故障转移至备用模型 (Fallback)
    if let Some(fb) = fallback {
            let fb_start = std::time::Instant::now();
            let fb_url = format!("{}/chat/completions", fb.base_url.trim_end_matches('/'));
            let fb_payload = serde_json::json!({
                "model": fb.model,
                "messages": messages,
                "max_tokens": max_tokens.unwrap_or(2048),
            });

            let mut fb_req = client.post(&fb_url).json(&fb_payload);
            if !fb.api_key.trim().is_empty() {
                fb_req = fb_req.header("Authorization", format!("Bearer {}", fb.api_key.trim()));
            }

            match fb_req.send().await {
                Ok(res) if res.status().is_success() => {
                    let latency_ms = fb_start.elapsed().as_millis() as u64;
                    if let Ok(data) = res.json::<serde_json::Value>().await {
                        if let Some(content) = data.get("choices")
                            .and_then(|c| c.get(0))
                            .and_then(|c| c.get("message"))
                            .and_then(|m| m.get("content"))
                            .and_then(|s| s.as_str()) {
                            return Ok(LlmCompletionResult {
                                success: true,
                                content: content.to_string(),
                                provider_used: fb.provider_name,
                                is_fallback: true,
                                latency_ms,
                                error: Some(format!("主力模型失败（{}），已自动故障转移至备用模型", primary_err_msg)),
                            });
                        }
                    }
                }
                Ok(res) => {
                    let status = res.status();
                    let err_txt = res.text().await.unwrap_or_default();
                    return Ok(LlmCompletionResult {
                        success: false,
                        content: String::new(),
                        provider_used: fb.provider_name,
                        is_fallback: true,
                        latency_ms: start.elapsed().as_millis() as u64,
                        error: Some(format!("主力模型失败（{}），备用模型亦失败（HTTP {} {}）", primary_err_msg, status, err_txt)),
                    });
                }
                Err(e) => {
                    return Ok(LlmCompletionResult {
                        success: false,
                        content: String::new(),
                        provider_used: fb.provider_name,
                        is_fallback: true,
                        latency_ms: start.elapsed().as_millis() as u64,
                        error: Some(format!("主力模型失败（{}），备用模型连接异常（{}）", primary_err_msg, e)),
                    });
                }
            }
        }

    Ok(LlmCompletionResult {
        success: false,
        content: String::new(),
        provider_used: primary.provider_name,
        is_fallback: false,
        latency_ms: start.elapsed().as_millis() as u64,
        error: Some(format!("主力模型调用失败（{}），未配置或未启用备用模型", primary_err_msg)),
    })
}

#[tauri::command]
async fn test_llm_pipeline(
    primary: LlmEndpointConfig,
    fallback: Option<LlmEndpointConfig>,
) -> Result<PipelineTestResult, String> {
    let p_res = test_llm_connection(primary.base_url, primary.api_key, primary.model).await?;
    
    let mut fb_res = None;
    if let Some(fb) = fallback {
        let r = test_llm_connection(fb.base_url, fb.api_key, fb.model).await?;
        fb_res = Some(r);
    }

    let overall = p_res.success || fb_res.as_ref().map(|r| r.success).unwrap_or(false);
    let msg = match (&p_res.success, fb_res.as_ref().map(|r| r.success)) {
        (true, Some(true)) => "双重主备链路均已就绪（主力与备用均正常连通）".to_string(),
        (true, Some(false)) => "主力模型可用，但备用模型连通失败".to_string(),
        (true, None) => "主力模型可用（未配置备用模型）".to_string(),
        (false, Some(true)) => "主力模型不可用，但备用模型正常（将自动触发故障转移）".to_string(),
        (false, Some(false)) => "主力模型与备用模型均连通失败，请检查配置".to_string(),
        (false, None) => "主力模型连通失败，请检查 API Key 或地址".to_string(),
    };

    Ok(PipelineTestResult {
        primary: p_res,
        fallback: fb_res,
        overall_success: overall,
        message: msg,
    })
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
            test_llm_connection,
            test_llm_pipeline,
            call_llm_with_fallback
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
