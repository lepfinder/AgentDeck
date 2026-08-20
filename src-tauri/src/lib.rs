pub mod backup;
pub mod config;
pub mod db;
pub mod http_server;
pub mod importers;
pub mod media_archive;
pub mod sync;

use db::{
    clear_workspace_analysis, create_prompt, delete_prompt, fetch_conversation_messages,
    fetch_conversations, fetch_dashboard_stats, fetch_workspace_analysis_messages,
    fetch_workspace_detail_stats, fetch_workspaces, get_prompt, list_prompts, record_prompt_use,
    save_workspace_fine_blocks, save_workspace_module_blocks, save_workspace_report,
    search_global_messages, toggle_prompt_star, toggle_star_session, update_prompt,
    AnalysisUserMessage, ConversationItem, DashboardStats, DbState, MessageItem, PromptInput,
    PromptItem, SearchResultItem, WorkspaceDetailStats, WorkspaceFineBlock, WorkspaceModuleBlock,
    WorkspaceStat,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use sync::{execute_sync, get_agent_source_paths, SyncResultInfo};
use tauri::{Emitter, Manager, RunEvent, State, WindowEvent};

static AUTO_SYNC_INTERVAL_SECS: AtomicU64 = AtomicU64::new(60);

#[tauri::command]
fn get_workspace_analysis_messages(
    workspace_path: String,
    state: State<'_, DbState>,
) -> Result<Vec<AnalysisUserMessage>, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    fetch_workspace_analysis_messages(&conn, &workspace_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_workspace_fine_blocks_cmd(
    workspace_path: String,
    blocks: Vec<WorkspaceFineBlock>,
    clear_existing: Option<bool>,
    state: State<'_, DbState>,
) -> Result<usize, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    save_workspace_fine_blocks(
        &conn,
        &workspace_path,
        &blocks,
        clear_existing.unwrap_or(false),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn save_workspace_module_blocks_cmd(
    workspace_path: String,
    modules: Vec<WorkspaceModuleBlock>,
    clear_existing: Option<bool>,
    state: State<'_, DbState>,
) -> Result<usize, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    save_workspace_module_blocks(
        &conn,
        &workspace_path,
        &modules,
        clear_existing.unwrap_or(false),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn save_workspace_report_cmd(
    workspace_path: String,
    report_md: String,
    state: State<'_, DbState>,
) -> Result<(), String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    save_workspace_report(&conn, &workspace_path, &report_md).map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_workspace_analysis_cmd(
    workspace_path: String,
    state: State<'_, DbState>,
) -> Result<(), String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    clear_workspace_analysis(&conn, &workspace_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_dashboard_stats(state: State<'_, DbState>) -> Result<DashboardStats, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    fetch_dashboard_stats(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_prompts_cmd(
    search: Option<String>,
    category: Option<String>,
    starred_only: Option<bool>,
    state: State<'_, DbState>,
) -> Result<Vec<PromptItem>, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    list_prompts(
        &conn,
        search.as_deref(),
        category.as_deref(),
        starred_only.unwrap_or(false),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_prompt_cmd(id: i64, state: State<'_, DbState>) -> Result<PromptItem, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    get_prompt(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn create_prompt_cmd(input: PromptInput, state: State<'_, DbState>) -> Result<PromptItem, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    create_prompt(&conn, &input).map_err(|e| e.to_string())
}

#[tauri::command]
fn update_prompt_cmd(
    id: i64,
    input: PromptInput,
    state: State<'_, DbState>,
) -> Result<PromptItem, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    update_prompt(&conn, id, &input).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_prompt_cmd(id: i64, state: State<'_, DbState>) -> Result<bool, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    delete_prompt(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn toggle_prompt_star_cmd(id: i64, state: State<'_, DbState>) -> Result<bool, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    toggle_prompt_star(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn record_prompt_use_cmd(id: i64, state: State<'_, DbState>) -> Result<PromptItem, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    record_prompt_use(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_workspace_detail(
    workspace_path: String,
    state: State<'_, DbState>,
) -> Result<WorkspaceDetailStats, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    fetch_workspace_detail_stats(&conn, &workspace_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_workspaces(
    search: Option<String>,
    state: State<'_, DbState>,
) -> Result<Vec<WorkspaceStat>, String> {
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
    fetch_conversations(
        &conn,
        workspace.as_deref(),
        search.as_deref(),
        starred_only.unwrap_or(false),
    )
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
fn toggle_star(conversation_id: String, state: State<'_, DbState>) -> Result<bool, String> {
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
                    if let Some(msg) = val
                        .get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                    {
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
        .timeout(std::time::Duration::from_secs(180))
        .connect_timeout(std::time::Duration::from_secs(30))
        .tcp_keepalive(std::time::Duration::from_secs(60))
        .user_agent("OpenAI/Python 1.55.0 (AgentDeck)")
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

    // 辅助闭包：发送单次 LLM 请求
    let send_request = |endpoint: &LlmEndpointConfig, attempt_idx: usize| {
        let url = format!(
            "{}/chat/completions",
            endpoint.base_url.trim_end_matches('/')
        );
        println!(
            "[AgentDeck LLM] 🚀 [{}] POST {} (model: {}, attempt: {})",
            endpoint.provider_name,
            url,
            endpoint.model,
            attempt_idx + 1
        );
        let mut payload = serde_json::json!({
            "model": endpoint.model,
            "messages": messages,
            "temperature": 0.2,
        });
        if let Some(mt) = max_tokens {
            payload["max_tokens"] = serde_json::json!(mt);
        }

        let mut req = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&payload);

        if !endpoint.api_key.trim().is_empty() {
            req = req.header(
                "Authorization",
                format!("Bearer {}", endpoint.api_key.trim()),
            );
        }
        req
    };

    // 1. 先尝试主力模型 (支持 1 次自动重试)
    let mut primary_err_msg = String::new();
    for attempt in 0..2 {
        let req = send_request(&primary, attempt);
        let call_start = std::time::Instant::now();
        match req.send().await {
            Ok(res) if res.status().is_success() => {
                let latency_ms = call_start.elapsed().as_millis() as u64;
                if let Ok(data) = res.json::<serde_json::Value>().await {
                    if let Some(content) = data
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("message"))
                        .and_then(|m| m.get("content"))
                        .and_then(|s| s.as_str())
                    {
                        if !content.trim().is_empty() {
                            println!(
                                "[AgentDeck LLM] ✅ [{}] HTTP 200 ({}ms, content_len: {})",
                                primary.provider_name,
                                latency_ms,
                                content.len()
                            );
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
                    if let Some(reasoning) = data
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("message"))
                        .and_then(|m| m.get("reasoning_content"))
                        .and_then(|s| s.as_str())
                    {
                        if !reasoning.trim().is_empty() {
                            println!("[AgentDeck LLM] ✅ [{}] HTTP 200 via reasoning ({}ms, reasoning_len: {})", primary.provider_name, latency_ms, reasoning.len());
                            return Ok(LlmCompletionResult {
                                success: true,
                                content: reasoning.to_string(),
                                provider_used: primary.provider_name,
                                is_fallback: false,
                                latency_ms,
                                error: None,
                            });
                        }
                    }
                }
                primary_err_msg =
                    "主力模型未返回有效的 message.content 或 reasoning_content".to_string();
                println!(
                    "[AgentDeck LLM] ⚠️ [{}] HTTP 200 but content empty",
                    primary.provider_name
                );
            }
            Ok(res) => {
                let status = res.status();
                let err_txt = res.text().await.unwrap_or_default();
                let mut msg = format!("HTTP {}", status);
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&err_txt) {
                    if let Some(m) = val
                        .get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                    {
                        msg = m.to_string();
                    } else if let Some(m) = val.get("message").and_then(|m| m.as_str()) {
                        msg = m.to_string();
                    }
                }
                println!(
                    "[AgentDeck LLM] ❌ [{}] Response error: {} ({})",
                    primary.provider_name,
                    msg,
                    err_txt.chars().take(200).collect::<String>()
                );
                primary_err_msg = msg;
                // 鉴权或参数错误不重试
                if status.as_u16() == 401 || status.as_u16() == 403 || status.as_u16() == 404 {
                    break;
                }
            }
            Err(e) => {
                println!(
                    "[AgentDeck LLM] ⚠️ [{}] Network error on attempt {}: {}",
                    primary.provider_name,
                    attempt + 1,
                    e
                );
                primary_err_msg = format!("网络错误: {}", e);
                if attempt == 0 {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    continue;
                }
            }
        }
    }

    // 2. 如果主力模型失败且配置了备用模型，无缝故障转移至备用模型 (Fallback)
    if let Some(fb) = fallback {
        let fb_start = std::time::Instant::now();
        let req = send_request(&fb, 0);

        match req.send().await {
            Ok(res) if res.status().is_success() => {
                let latency_ms = fb_start.elapsed().as_millis() as u64;
                if let Ok(data) = res.json::<serde_json::Value>().await {
                    if let Some(content) = data
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("message"))
                        .and_then(|m| m.get("content"))
                        .and_then(|s| s.as_str())
                    {
                        if !content.trim().is_empty() {
                            return Ok(LlmCompletionResult {
                                success: true,
                                content: content.to_string(),
                                provider_used: fb.provider_name,
                                is_fallback: true,
                                latency_ms,
                                error: Some(format!(
                                    "主力模型失败（{}），已自动故障转移至备用模型",
                                    primary_err_msg
                                )),
                            });
                        }
                    }
                    if let Some(reasoning) = data
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("message"))
                        .and_then(|m| m.get("reasoning_content"))
                        .and_then(|s| s.as_str())
                    {
                        if !reasoning.trim().is_empty() {
                            return Ok(LlmCompletionResult {
                                success: true,
                                content: reasoning.to_string(),
                                provider_used: fb.provider_name,
                                is_fallback: true,
                                latency_ms,
                                error: Some(format!(
                                    "主力模型失败（{}），已自动故障转移至备用模型",
                                    primary_err_msg
                                )),
                            });
                        }
                    }
                }
            }
            Ok(res) => {
                let status = res.status();
                let err_txt = res.text().await.unwrap_or_default();
                let mut msg = format!("HTTP {}", status);
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&err_txt) {
                    if let Some(m) = val
                        .get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                    {
                        msg = m.to_string();
                    }
                }
                return Ok(LlmCompletionResult {
                    success: false,
                    content: String::new(),
                    provider_used: fb.provider_name,
                    is_fallback: true,
                    latency_ms: fb_start.elapsed().as_millis() as u64,
                    error: Some(format!("主力失败: {}; 备用失败: {}", primary_err_msg, msg)),
                });
            }
            Err(e) => {
                return Ok(LlmCompletionResult {
                    success: false,
                    content: String::new(),
                    provider_used: fb.provider_name,
                    is_fallback: true,
                    latency_ms: fb_start.elapsed().as_millis() as u64,
                    error: Some(format!(
                        "主力失败: {}; 备用网络错误: {}",
                        primary_err_msg, e
                    )),
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
        error: Some(format!(
            "主力模型调用失败（{}），未配置或未启用备用模型",
            primary_err_msg
        )),
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
async fn trigger_sync(
    app_handle: tauri::AppHandle,
    full: Option<bool>,
) -> Result<SyncResultInfo, String> {
    let is_full = full.unwrap_or(false);
    let _ = app_handle.emit("sync-started", ());
    let res = tauri::async_runtime::spawn_blocking(move || execute_sync(is_full))
        .await
        .map_err(|e| format!("同步任务执行失败: {}", e));

    if let Ok(result) = &res {
        let _ = app_handle.emit("sync-completed", result);
    }
    res
}

#[tauri::command]
fn set_auto_sync_interval(seconds: u64) -> Result<u64, String> {
    if !(15..=3600).contains(&seconds) {
        return Err("自动同步频率需在 15 到 3600 秒之间".to_string());
    }
    AUTO_SYNC_INTERVAL_SECS.store(seconds, Ordering::Relaxed);
    Ok(seconds)
}

#[tauri::command]
fn get_database_path_info() -> Result<String, String> {
    Ok(db::get_database_path().to_string_lossy().to_string())
}

#[tauri::command]
async fn create_backup_cmd(
    target_dir: String,
    max_snapshots: Option<usize>,
    app_handle: tauri::AppHandle,
    state: State<'_, DbState>,
) -> Result<backup::BackupInfo, String> {
    let handle = app_handle.clone();
    let callback = move |p: backup::BackupProgress| {
        let _ = handle.emit("backup-progress", p);
    };

    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    backup::create_backup_with_progress(
        &conn,
        &target_dir,
        max_snapshots.unwrap_or(3),
        Some(Box::new(callback)),
    )
}

#[tauri::command]
fn list_backups_cmd(target_dir: String) -> Result<Vec<backup::BackupInfo>, String> {
    backup::list_backups(&target_dir)
}

#[tauri::command]
fn restore_backup_cmd(backup_file: String) -> Result<backup::RestoreInfo, String> {
    backup::restore_backup(&backup_file)
}

#[tauri::command]
fn get_cloud_presets_cmd() -> Result<Vec<backup::CloudPreset>, String> {
    Ok(backup::detect_cloud_presets())
}

#[tauri::command]
async fn select_folder_dialog_cmd() -> Result<Option<String>, String> {
    let folder = rfd::AsyncFileDialog::new()
        .set_title("选择 AgentDeck 备份存储目录")
        .pick_folder()
        .await;

    Ok(folder.map(|f| f.path().to_string_lossy().to_string()))
}

#[tauri::command]
fn get_app_config_cmd() -> Result<config::AppConfig, String> {
    Ok(config::load_config())
}

#[tauri::command]
fn save_app_config_cmd(config: config::AppConfig) -> Result<(), String> {
    config::save_config(&config)
}

#[tauri::command]
fn open_url_cmd(url: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&url).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd").args(["/C", "start", &url]).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
struct IdeAppStatus {
    id: String,
    label: String,
    kind: String,
    installed: bool,
}

fn macos_app_exists(app_name: &str) -> bool {
    let app_path = format!("/Applications/{}.app", app_name);
    std::path::Path::new(&app_path).exists()
}

fn command_on_path(bin: &str) -> bool {
    std::process::Command::new("which")
        .arg(bin)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn ide_installed(id: &str) -> bool {
    match id {
        "cursor" => macos_app_exists("Cursor") || command_on_path("cursor"),
        "antigravity" => macos_app_exists("Antigravity") || command_on_path("antigravity"),
        "claude" => command_on_path("claude"),
        "codex" => command_on_path("codex"),
        _ => false,
    }
}

#[tauri::command]
fn list_ide_apps_cmd() -> Result<Vec<IdeAppStatus>, String> {
    Ok(vec![
        IdeAppStatus {
            id: "cursor".into(),
            label: "Cursor".into(),
            kind: "app".into(),
            installed: ide_installed("cursor"),
        },
        IdeAppStatus {
            id: "antigravity".into(),
            label: "Antigravity".into(),
            kind: "app".into(),
            installed: ide_installed("antigravity"),
        },
        IdeAppStatus {
            id: "claude".into(),
            label: "Claude Code".into(),
            kind: "cli".into(),
            installed: ide_installed("claude"),
        },
        IdeAppStatus {
            id: "codex".into(),
            label: "Codex".into(),
            kind: "cli".into(),
            installed: ide_installed("codex"),
        },
    ])
}

fn spawn_checked(cmd: &mut std::process::Command, fail: &str) -> Result<(), String> {
    cmd.spawn().map(|_| ()).map_err(|e| format!("{}: {}", fail, e))
}

fn open_in_macos_app(app_name: &str, workspace: &str) -> Result<(), String> {
    spawn_checked(
        std::process::Command::new("open")
            .arg("-a")
            .arg(app_name)
            .arg(workspace),
        &format!("无法打开 {}", app_name),
    )
}

fn open_in_terminal_cli(bin: &str, workspace: &str) -> Result<(), String> {
    let escaped_ws = workspace.replace('\\', "\\\\").replace('"', "\\\"");
    let escaped_bin = bin.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        r#"tell application "Terminal"
  activate
  do script "cd \"{escaped_ws}\" && {escaped_bin}"
end tell"#
    );
    spawn_checked(
        std::process::Command::new("osascript").arg("-e").arg(script),
        &format!("无法在终端启动 {}", bin),
    )
}

#[tauri::command]
fn open_workspace_in_ide_cmd(ide: String, workspace_path: String) -> Result<(), String> {
    let path = std::path::Path::new(&workspace_path);
    if !path.is_dir() {
        return Err("工作区路径不存在或不是目录".into());
    }

    #[cfg(target_os = "macos")]
    {
        match ide.as_str() {
            "cursor" => open_in_macos_app("Cursor", &workspace_path),
            "antigravity" => open_in_macos_app("Antigravity", &workspace_path),
            "claude" => open_in_terminal_cli("claude", &workspace_path),
            "codex" => open_in_terminal_cli("codex", &workspace_path),
            _ => Err(format!("不支持的 IDE: {}", ide)),
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (ide, workspace_path);
        Err("当前仅支持在 macOS 上打开 AI IDE".into())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db_state = DbState::new().expect("Failed to initialize SQLite database");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(db_state)
        .invoke_handler(tauri::generate_handler![
            get_dashboard_stats,
            list_prompts_cmd,
            get_prompt_cmd,
            create_prompt_cmd,
            update_prompt_cmd,
            delete_prompt_cmd,
            toggle_prompt_star_cmd,
            record_prompt_use_cmd,
            get_workspace_detail,
            list_workspaces,
            list_conversations,
            get_conversation_messages,
            toggle_star,
            search_messages,
            trigger_sync,
            set_auto_sync_interval,
            test_llm_connection,
            test_llm_pipeline,
            call_llm_with_fallback,
            get_database_path_info,
            get_workspace_analysis_messages,
            save_workspace_fine_blocks_cmd,
            save_workspace_module_blocks_cmd,
            save_workspace_report_cmd,
            clear_workspace_analysis_cmd,
            create_backup_cmd,
            list_backups_cmd,
            restore_backup_cmd,
            get_cloud_presets_cmd,
            select_folder_dialog_cmd,
            get_app_config_cmd,
            save_app_config_cmd,
            open_url_cmd,
            list_ide_apps_cmd,
            open_workspace_in_ide_cmd
        ])
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title("AgentDeck - AI Coding Cockpit");
            }

            // 启动嵌入式 REST API 兼容服务（监听 127.0.0.1:8788，供给前端图片与外部服务无缝调用）
            http_server::start_http_server(8788);

            // 启动后台多源智能监听线程（每 60 秒探测数据源 mtime 变动，实现无感实时同步）
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut last_mtimes: HashMap<std::path::PathBuf, std::time::SystemTime> =
                    HashMap::new();
                let mut last_sync_at = std::time::Instant::now()
                    .checked_sub(std::time::Duration::from_secs(60))
                    .unwrap_or_else(std::time::Instant::now);
                // 首次填充初始时间戳
                for src in get_agent_source_paths() {
                    if let Ok(meta) = src.metadata() {
                        if let Ok(mtime) = meta.modified() {
                            last_mtimes.insert(src, mtime);
                        }
                    }
                }

                loop {
                    let interval_secs = AUTO_SYNC_INTERVAL_SECS.load(Ordering::Relaxed);
                    std::thread::sleep(std::time::Duration::from_secs(interval_secs));
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
                                    changed = true;
                                    last_mtimes.insert(src.clone(), mtime);
                                }
                            }
                        }
                    }

                    if changed && last_sync_at.elapsed() >= std::time::Duration::from_secs(20) {
                        last_sync_at = std::time::Instant::now();
                        let _ = app_handle.emit("sync-started", ());
                        let result = execute_sync(false);
                        let _ = app_handle.emit("sync-completed", result);
                    }
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| match event {
            // 点击关闭按钮：隐藏到 Dock，不退出
            RunEvent::WindowEvent {
                label,
                event: WindowEvent::CloseRequested { api, .. },
                ..
            } => {
                api.prevent_close();
                if let Some(win) = app_handle.get_webview_window(&label) {
                    let _ = win.hide();
                }
            }
            // 所有窗口关闭时的自动退出请求：拦截，保持进程；⌘Q 带 exit code，放行
            RunEvent::ExitRequested { api, code, .. } => {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
            // macOS：从 Dock 图标点击恢复窗口
            #[cfg(target_os = "macos")]
            RunEvent::Reopen {
                has_visible_windows,
                ..
            } => {
                if !has_visible_windows {
                    if let Some(win) = app_handle.get_webview_window("main") {
                        let _ = win.show();
                        let _ = win.set_focus();
                    }
                }
            }
            _ => {}
        });
}
