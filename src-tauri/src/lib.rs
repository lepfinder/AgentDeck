pub mod backup;
pub mod config;
pub mod db;
pub mod git_board;
pub mod http_server;
pub mod importers;
pub mod media_archive;
pub mod prompt_catalog;
pub mod quota;
pub mod quota_pill;
pub mod quota_tray;
pub mod service_config;
pub mod service_discovery;
pub mod service_http;
pub mod service_manager;
pub mod sync;

use db::{
    clear_conversation_ai_title, clear_workspace_analysis, create_prompt, delete_prompt,
    fetch_conversation_by_id, fetch_conversation_messages, fetch_conversations,
    fetch_daily_timeline, fetch_dashboard_stats, fetch_usage_stats,
    fetch_workspace_analysis_messages,
    fetch_workspace_detail_stats, fetch_workspaces, get_prompt, list_prompts, list_prompts_ex,
    merge_workspace, record_prompt_use,
    move_conversation,
    save_conversation_ai_summary, save_workspace_fine_blocks, save_workspace_module_blocks,
    save_workspace_report, search_global_messages, set_conversation_ai_status, toggle_prompt_star,
    toggle_star_session, update_conversation_ai_title, update_prompt, update_prompt_preview_size,
    AnalysisUserMessage,
    ArtifactItem, ConversationItem, DailyTimelineStats, DashboardStats, DbState, MessageItem,
    PromptInput, PromptItem, SearchResultItem, UsageStatsPayload, WorkspaceDetailStats,
    WorkspaceFineBlock,
    WorkspaceModuleBlock, WorkspaceStat, get_conversation_artifacts, get_workspace_artifacts,
    WorkspaceArtifactItem, WorkspaceMergeResult,
    ConversationMoveResult,
};
use serde::{Deserialize, Serialize};
use sync::{
    collect_agent_sources, execute_sync, AgentSourceInfo, SyncResultInfo,
};
use tauri::{Emitter, Manager, RunEvent, State, WindowEvent};

#[tauri::command]
async fn get_workspace_analysis_messages(
    workspace_path: String,
) -> Result<Vec<AnalysisUserMessage>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_read_connection().map_err(|e| e.to_string())?;
        fetch_workspace_analysis_messages(&conn, &workspace_path).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
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
async fn get_dashboard_stats() -> Result<DashboardStats, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_read_connection().map_err(|e| e.to_string())?;
        fetch_dashboard_stats(&conn).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_daily_timeline(date: String) -> Result<DailyTimelineStats, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_read_connection().map_err(|e| e.to_string())?;
        fetch_daily_timeline(&conn, &date).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_usage_stats(days: Option<i64>) -> Result<UsageStatsPayload, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_read_connection().map_err(|e| e.to_string())?;
        fetch_usage_stats(&conn, days).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn list_prompts_cmd(
    search: Option<String>,
    category: Option<String>,
    starred_only: Option<bool>,
    lite: Option<bool>,
) -> Result<Vec<PromptItem>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_read_connection().map_err(|e| e.to_string())?;
        list_prompts_ex(
            &conn,
            search.as_deref(),
            category.as_deref(),
            starred_only.unwrap_or(false),
            lite.unwrap_or(true),
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_prompt_cmd(id: i64) -> Result<PromptItem, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_read_connection().map_err(|e| e.to_string())?;
        get_prompt(&conn, id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn create_prompt_cmd(input: PromptInput, state: State<'_, DbState>) -> Result<PromptItem, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    create_prompt(&conn, &input).map_err(|e| e.to_string())
}

/// 校验并归档本地图片到 ~/.agentdeck/media/prompts/uploads/，返回 /media Web 路径。
fn import_prompt_preview_image_path(path: &std::path::Path) -> Result<String, String> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif") {
        return Err("仅支持 png / jpg / jpeg / webp / gif 图片".into());
    }
    media_archive::archive_image_file(&path, "prompts", "uploads")
        .ok_or_else(|| "图片导入失败，请重试".to_string())
}

/// 文件选择器方式导入；用户取消时返回 None。
#[tauri::command]
fn pick_prompt_preview_image_cmd() -> Result<Option<String>, String> {
    let picked = rfd::FileDialog::new()
        .add_filter("图片", &["png", "jpg", "jpeg", "webp", "gif"])
        .pick_file();
    let Some(path) = picked else {
        return Ok(None);
    };
    import_prompt_preview_image_path(&path).map(Some)
}

/// 拖拽方式导入：接收 Tauri 拖放事件给出的本地文件路径。
#[tauri::command]
fn import_prompt_preview_image_cmd(path: String) -> Result<String, String> {
    let path = std::path::PathBuf::from(path);
    if !path.is_file() {
        return Err("文件不存在或不可访问".into());
    }
    import_prompt_preview_image_path(&path)
}

/// 剪贴板粘贴方式导入：接收前端 paste 事件取出的图片字节（base64 编码）。
#[tauri::command]
fn import_prompt_preview_image_bytes_cmd(data: String, ext: String) -> Result<String, String> {
    use base64::Engine as _;

    let ext = ext.trim_start_matches('.').to_lowercase();
    if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif") {
        return Err("仅支持 png / jpg / jpeg / webp / gif 图片".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.as_bytes())
        .map_err(|e| format!("图片数据解码失败: {e}"))?;
    media_archive::archive_image_bytes(&bytes, &ext, "prompts", "uploads", Some("pasted"))
        .ok_or_else(|| "图片导入失败，请重试".to_string())
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
fn update_prompt_preview_size_cmd(
    id: i64,
    width: i64,
    height: i64,
    state: State<'_, DbState>,
) -> Result<bool, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    update_prompt_preview_size(&conn, id, width, height).map_err(|e| e.to_string())
}

#[tauri::command]
async fn sync_gpt_image_catalog_cmd(
    app_handle: tauri::AppHandle,
) -> Result<prompt_catalog::CatalogSyncResult, String> {
    let handle = app_handle.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_write_connection().map_err(|e| e.to_string())?;
        let progress = Box::new(move |p: prompt_catalog::CatalogSyncProgress| {
            let _ = handle.emit("prompt-catalog-sync-progress", p);
        });
        prompt_catalog::sync_gpt_image_catalog_with_progress(&conn, Some(progress))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn cache_prompt_previews_cmd(limit: Option<u32>) -> Result<usize, String> {
    let lim = limit.unwrap_or(40) as usize;
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_write_connection().map_err(|e| e.to_string())?;
        prompt_catalog::cache_missing_previews(&conn, lim)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn warm_prompt_preview_cache_cmd(
    batch_size: Option<u32>,
    max_batches: Option<u32>,
) -> Result<usize, String> {
    let batch = batch_size.unwrap_or(40) as usize;
    let batches = max_batches.unwrap_or(20) as usize;
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_write_connection().map_err(|e| e.to_string())?;
        prompt_catalog::warm_preview_cache(&conn, batch, batches)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn count_uncached_prompt_previews_cmd() -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let conn = db::open_read_connection().map_err(|e| e.to_string())?;
        prompt_catalog::count_uncached_previews(&conn)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_workspace_detail(
    workspace_path: String,
) -> Result<WorkspaceDetailStats, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_read_connection().map_err(|e| e.to_string())?;
        fetch_workspace_detail_stats(&conn, &workspace_path).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn list_workspaces(
    search: Option<String>,
) -> Result<Vec<WorkspaceStat>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_read_connection().map_err(|e| e.to_string())?;
        fetch_workspaces(&conn, search.as_deref(), None).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn merge_workspace_cmd(
    source_path: String,
    target_path: String,
    state: State<'_, DbState>,
) -> Result<WorkspaceMergeResult, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    merge_workspace(&conn, &source_path, &target_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn move_conversation_cmd(
    conversation_id: String,
    target_path: String,
    state: State<'_, DbState>,
) -> Result<ConversationMoveResult, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    move_conversation(&conn, &conversation_id, &target_path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_conversations(
    workspace: Option<String>,
    search: Option<String>,
    starred_only: Option<bool>,
) -> Result<Vec<ConversationItem>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_read_connection().map_err(|e| e.to_string())?;
        fetch_conversations(
            &conn,
            workspace.as_deref(),
            search.as_deref(),
            starred_only.unwrap_or(false),
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_conversation_messages(
    conversation_id: String,
) -> Result<Vec<MessageItem>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_read_connection().map_err(|e| e.to_string())?;
        fetch_conversation_messages(&conn, &conversation_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_conversation_artifacts_cmd(
    conversation_id: String,
) -> Result<Vec<ArtifactItem>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_read_connection().map_err(|e| e.to_string())?;
        get_conversation_artifacts(&conn, &conversation_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_workspace_artifacts_cmd(
    workspace_path: String,
) -> Result<Vec<WorkspaceArtifactItem>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_read_connection().map_err(|e| e.to_string())?;
        get_workspace_artifacts(&conn, &workspace_path).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn toggle_star(conversation_id: String, state: State<'_, DbState>) -> Result<bool, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    toggle_star_session(&conn, &conversation_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn update_conversation_ai_title_cmd(
    conversation_id: String,
    ai_title: String,
    state: State<'_, DbState>,
) -> Result<ConversationItem, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    update_conversation_ai_title(&conn, &conversation_id, &ai_title).map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_conversation_ai_title_cmd(
    conversation_id: String,
    state: State<'_, DbState>,
) -> Result<ConversationItem, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    clear_conversation_ai_title(&conn, &conversation_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_conversation_ai_summary_cmd(
    conversation_id: String,
    ai_title: Option<String>,
    summary: String,
    status: String,
    based_on_content_hash: Option<String>,
    based_on_message_count: Option<i64>,
    model: Option<String>,
    error: Option<String>,
    state: State<'_, DbState>,
) -> Result<ConversationItem, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    save_conversation_ai_summary(
        &conn,
        &conversation_id,
        ai_title.as_deref(),
        &summary,
        &status,
        based_on_content_hash.as_deref(),
        based_on_message_count,
        model.as_deref(),
        error.as_deref(),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_conversation_ai_status_cmd(
    conversation_id: String,
    status: String,
    error: Option<String>,
    state: State<'_, DbState>,
) -> Result<(), String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    set_conversation_ai_status(&conn, &conversation_id, &status, error.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_conversation_item(
    conversation_id: String,
) -> Result<Option<ConversationItem>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_read_connection().map_err(|e| e.to_string())?;
        fetch_conversation_by_id(&conn, &conversation_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn search_messages(
    query: String,
    role: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<SearchResultItem>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_read_connection().map_err(|e| e.to_string())?;
        search_global_messages(&conn, &query, role.as_deref(), limit.unwrap_or(30))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
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
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
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
    disable_thinking: Option<bool>,
    scene: Option<String>,
    state: State<'_, DbState>,
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
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
                error: Some(format!("HTTP客户端构建失败: {}", e)),
            });
        }
    };

    let should_disable_thinking = disable_thinking.unwrap_or(true);
    let scene_str = scene.unwrap_or_else(|| "general".to_string());

    // 提取系统提示词和用户输入片段用于审计
    let mut system_prompt = String::new();
    let mut user_prompt_snippet = String::new();
    for msg in &messages {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
        let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
        if role == "system" && system_prompt.is_empty() {
            system_prompt = content.to_string();
        } else if role == "user" && user_prompt_snippet.is_empty() {
            user_prompt_snippet = content.chars().take(800).collect();
        }
    }

    let parse_usage = |data: &serde_json::Value| -> (i64, i64, i64) {
        if let Some(usage) = data.get("usage") {
            let pt = usage.get("prompt_tokens").and_then(|t| t.as_i64()).unwrap_or(0);
            let ct = usage.get("completion_tokens").and_then(|t| t.as_i64()).unwrap_or(0);
            let tt = usage.get("total_tokens").and_then(|t| t.as_i64()).unwrap_or(pt + ct);
            (pt, ct, tt)
        } else {
            (0, 0, 0)
        }
    };

    // 辅助闭包：发送单次 LLM 请求（支持注入关闭思考参数）
    let send_request = |endpoint: &LlmEndpointConfig, attempt_idx: usize, include_thinking_params: bool| {
        let url = format!(
            "{}/chat/completions",
            endpoint.base_url.trim_end_matches('/')
        );
        println!(
            "[AgentDeck LLM] 🚀 [{}] POST {} (model: {}, attempt: {}, disable_thinking: {}, scene: {})",
            endpoint.provider_name,
            url,
            endpoint.model,
            attempt_idx + 1,
            include_thinking_params,
            scene_str
        );
        let mut payload = serde_json::json!({
            "model": endpoint.model,
            "messages": messages,
            "temperature": 0.2,
        });
        if let Some(mt) = max_tokens {
            payload["max_tokens"] = serde_json::json!(mt);
        }

        if include_thinking_params {
            payload["enable_thinking"] = serde_json::json!(false);
            payload["thinking_config"] = serde_json::json!({ "thinking_budget": 0 });
            payload["reasoning_effort"] = serde_json::json!("none");
            payload["thinking"] = serde_json::json!({ "type": "disabled" });
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
        let req = send_request(&primary, attempt, should_disable_thinking);
        let call_start = std::time::Instant::now();
        match req.send().await {
            Ok(res) if res.status().is_success() => {
                let latency_ms = call_start.elapsed().as_millis() as u64;
                if let Ok(data) = res.json::<serde_json::Value>().await {
                    let (prompt_tokens, completion_tokens, total_tokens) = parse_usage(&data);

                    if let Some(content) = data
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("message"))
                        .and_then(|m| m.get("content"))
                        .and_then(|s| s.as_str())
                    {
                        if !content.trim().is_empty() {
                            println!(
                                "[AgentDeck LLM] ✅ [{}] HTTP 200 ({}ms, content_len: {}, tokens: {}/{})",
                                primary.provider_name,
                                latency_ms,
                                content.len(),
                                prompt_tokens,
                                completion_tokens
                            );
                            if let Ok(conn) = state.conn_mutex.lock() {
                                let _ = db::save_llm_call_log(
                                    &conn,
                                    &scene_str,
                                    &primary.provider_name,
                                    &primary.model,
                                    &system_prompt,
                                    &user_prompt_snippet,
                                    prompt_tokens,
                                    completion_tokens,
                                    total_tokens,
                                    latency_ms as i64,
                                    false,
                                    "success",
                                    None,
                                );
                            }
                            return Ok(LlmCompletionResult {
                                success: true,
                                content: content.to_string(),
                                provider_used: primary.provider_name,
                                is_fallback: false,
                                latency_ms,
                                prompt_tokens: Some(prompt_tokens),
                                completion_tokens: Some(completion_tokens),
                                total_tokens: Some(total_tokens),
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
                            if let Ok(conn) = state.conn_mutex.lock() {
                                let _ = db::save_llm_call_log(
                                    &conn,
                                    &scene_str,
                                    &primary.provider_name,
                                    &primary.model,
                                    &system_prompt,
                                    &user_prompt_snippet,
                                    prompt_tokens,
                                    completion_tokens,
                                    total_tokens,
                                    latency_ms as i64,
                                    false,
                                    "success",
                                    None,
                                );
                            }
                            return Ok(LlmCompletionResult {
                                success: true,
                                content: reasoning.to_string(),
                                provider_used: primary.provider_name,
                                is_fallback: false,
                                latency_ms,
                                prompt_tokens: Some(prompt_tokens),
                                completion_tokens: Some(completion_tokens),
                                total_tokens: Some(total_tokens),
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

                // 若由于严格校验未知字段导致 400，自动剥离 thinking 参数降级重发一次
                if status.as_u16() == 400 && should_disable_thinking {
                    println!("[AgentDeck LLM] ⚠️ [{}] 检测到 400 错误，剥离 thinking 参数降级重发...", primary.provider_name);
                    let fallback_req = send_request(&primary, attempt, false);
                    if let Ok(fallback_res) = fallback_req.send().await {
                        if fallback_res.status().is_success() {
                            let latency_ms = call_start.elapsed().as_millis() as u64;
                            if let Ok(data) = fallback_res.json::<serde_json::Value>().await {
                                let (prompt_tokens, completion_tokens, total_tokens) = parse_usage(&data);
                                if let Some(content) = data
                                    .get("choices")
                                    .and_then(|c| c.get(0))
                                    .and_then(|c| c.get("message"))
                                    .and_then(|m| m.get("content"))
                                    .and_then(|s| s.as_str())
                                {
                                    if !content.trim().is_empty() {
                                        if let Ok(conn) = state.conn_mutex.lock() {
                                            let _ = db::save_llm_call_log(
                                                &conn,
                                                &scene_str,
                                                &primary.provider_name,
                                                &primary.model,
                                                &system_prompt,
                                                &user_prompt_snippet,
                                                prompt_tokens,
                                                completion_tokens,
                                                total_tokens,
                                                latency_ms as i64,
                                                false,
                                                "success",
                                                None,
                                            );
                                        }
                                        return Ok(LlmCompletionResult {
                                            success: true,
                                            content: content.to_string(),
                                            provider_used: primary.provider_name,
                                            is_fallback: false,
                                            latency_ms,
                                            prompt_tokens: Some(prompt_tokens),
                                            completion_tokens: Some(completion_tokens),
                                            total_tokens: Some(total_tokens),
                                            error: None,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }

                primary_err_msg = msg;
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
        let req = send_request(&fb, 0, should_disable_thinking);

        match req.send().await {
            Ok(res) if res.status().is_success() => {
                let latency_ms = fb_start.elapsed().as_millis() as u64;
                if let Ok(data) = res.json::<serde_json::Value>().await {
                    let (prompt_tokens, completion_tokens, total_tokens) = parse_usage(&data);

                    if let Some(content) = data
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("message"))
                        .and_then(|m| m.get("content"))
                        .and_then(|s| s.as_str())
                    {
                        if !content.trim().is_empty() {
                            if let Ok(conn) = state.conn_mutex.lock() {
                                let _ = db::save_llm_call_log(
                                    &conn,
                                    &scene_str,
                                    &fb.provider_name,
                                    &fb.model,
                                    &system_prompt,
                                    &user_prompt_snippet,
                                    prompt_tokens,
                                    completion_tokens,
                                    total_tokens,
                                    latency_ms as i64,
                                    true,
                                    "success",
                                    None,
                                );
                            }
                            return Ok(LlmCompletionResult {
                                success: true,
                                content: content.to_string(),
                                provider_used: fb.provider_name,
                                is_fallback: true,
                                latency_ms,
                                prompt_tokens: Some(prompt_tokens),
                                completion_tokens: Some(completion_tokens),
                                total_tokens: Some(total_tokens),
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
                            if let Ok(conn) = state.conn_mutex.lock() {
                                let _ = db::save_llm_call_log(
                                    &conn,
                                    &scene_str,
                                    &fb.provider_name,
                                    &fb.model,
                                    &system_prompt,
                                    &user_prompt_snippet,
                                    prompt_tokens,
                                    completion_tokens,
                                    total_tokens,
                                    latency_ms as i64,
                                    true,
                                    "success",
                                    None,
                                );
                            }
                            return Ok(LlmCompletionResult {
                                success: true,
                                content: reasoning.to_string(),
                                provider_used: fb.provider_name,
                                is_fallback: true,
                                latency_ms,
                                prompt_tokens: Some(prompt_tokens),
                                completion_tokens: Some(completion_tokens),
                                total_tokens: Some(total_tokens),
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
                if status.as_u16() == 400 && should_disable_thinking {
                    let fallback_req = send_request(&fb, 0, false);
                    if let Ok(fallback_res) = fallback_req.send().await {
                        if fallback_res.status().is_success() {
                            let latency_ms = fb_start.elapsed().as_millis() as u64;
                            if let Ok(data) = fallback_res.json::<serde_json::Value>().await {
                                let (prompt_tokens, completion_tokens, total_tokens) = parse_usage(&data);
                                if let Some(content) = data
                                    .get("choices")
                                    .and_then(|c| c.get(0))
                                    .and_then(|c| c.get("message"))
                                    .and_then(|m| m.get("content"))
                                    .and_then(|s| s.as_str())
                                {
                                    if !content.trim().is_empty() {
                                        if let Ok(conn) = state.conn_mutex.lock() {
                                            let _ = db::save_llm_call_log(
                                                &conn,
                                                &scene_str,
                                                &fb.provider_name,
                                                &fb.model,
                                                &system_prompt,
                                                &user_prompt_snippet,
                                                prompt_tokens,
                                                completion_tokens,
                                                total_tokens,
                                                latency_ms as i64,
                                                true,
                                                "success",
                                                None,
                                            );
                                        }
                                        return Ok(LlmCompletionResult {
                                            success: true,
                                            content: content.to_string(),
                                            provider_used: fb.provider_name,
                                            is_fallback: true,
                                            latency_ms,
                                            prompt_tokens: Some(prompt_tokens),
                                            completion_tokens: Some(completion_tokens),
                                            total_tokens: Some(total_tokens),
                                            error: Some(format!(
                                                "主力模型失败（{}），已自动故障转移至备用模型",
                                                primary_err_msg
                                            )),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                let err_txt = res.text().await.unwrap_or_default();
                println!(
                    "[AgentDeck LLM] ❌ Fallback model failed: HTTP {} ({})",
                    status,
                    err_txt.chars().take(200).collect::<String>()
                );
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
                let final_err = format!("主力失败: {}; 备用失败: {}", primary_err_msg, msg);
                if let Ok(conn) = state.conn_mutex.lock() {
                    let _ = db::save_llm_call_log(
                        &conn,
                        &scene_str,
                        &fb.provider_name,
                        &fb.model,
                        &system_prompt,
                        &user_prompt_snippet,
                        0,
                        0,
                        0,
                        fb_start.elapsed().as_millis() as i64,
                        true,
                        "error",
                        Some(&final_err),
                    );
                }
                return Ok(LlmCompletionResult {
                    success: false,
                    content: String::new(),
                    provider_used: fb.provider_name,
                    is_fallback: true,
                    latency_ms: fb_start.elapsed().as_millis() as u64,
                    prompt_tokens: None,
                    completion_tokens: None,
                    total_tokens: None,
                    error: Some(final_err),
                });
            }
            Err(e) => {
                let final_err = format!("主力失败: {}; 备用网络错误: {}", primary_err_msg, e);
                if let Ok(conn) = state.conn_mutex.lock() {
                    let _ = db::save_llm_call_log(
                        &conn,
                        &scene_str,
                        &fb.provider_name,
                        &fb.model,
                        &system_prompt,
                        &user_prompt_snippet,
                        0,
                        0,
                        0,
                        fb_start.elapsed().as_millis() as i64,
                        true,
                        "error",
                        Some(&final_err),
                    );
                }
                return Ok(LlmCompletionResult {
                    success: false,
                    content: String::new(),
                    provider_used: fb.provider_name,
                    is_fallback: true,
                    latency_ms: fb_start.elapsed().as_millis() as u64,
                    prompt_tokens: None,
                    completion_tokens: None,
                    total_tokens: None,
                    error: Some(final_err),
                });
            }
        }
    }

    let final_err = format!(
        "主力模型调用失败（{}），未配置或未启用备用模型",
        primary_err_msg
    );
    if let Ok(conn) = state.conn_mutex.lock() {
        let _ = db::save_llm_call_log(
            &conn,
            &scene_str,
            &primary.provider_name,
            &primary.model,
            &system_prompt,
            &user_prompt_snippet,
            0,
            0,
            0,
            start.elapsed().as_millis() as i64,
            false,
            "error",
            Some(&final_err),
        );
    }

    Ok(LlmCompletionResult {
        success: false,
        content: String::new(),
        provider_used: primary.provider_name,
        is_fallback: false,
        latency_ms: start.elapsed().as_millis() as u64,
        prompt_tokens: None,
        completion_tokens: None,
        total_tokens: None,
        error: Some(final_err),
    })
}

#[tauri::command]
async fn get_llm_call_logs_cmd(
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<db::LlmCallLogItem>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_read_connection().map_err(|e| e.to_string())?;
        db::get_llm_call_logs(&conn, limit.unwrap_or(50), offset.unwrap_or(0))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_llm_usage_summary_cmd() -> Result<db::LlmUsageSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_read_connection().map_err(|e| e.to_string())?;
        db::get_llm_usage_summary(&conn).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn clear_llm_call_logs_cmd(
    state: State<'_, DbState>,
) -> Result<bool, String> {
    let conn = state.conn_mutex.lock().map_err(|e| e.to_string())?;
    db::clear_llm_call_logs(&conn).map_err(|e| e.to_string())?;
    Ok(true)
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
async fn get_quota_snapshot(force: Option<bool>) -> Result<quota::QuotaSnapshot, String> {
    let force = force.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || quota::fetch_quota_snapshot(force))
        .await
        .map_err(|e| format!("额度查询失败: {e}"))
}

#[tauri::command]
fn get_database_path_info() -> Result<String, String> {
    Ok(db::get_database_path().to_string_lossy().to_string())
}

#[tauri::command]
async fn get_agent_sources_cmd() -> Result<Vec<AgentSourceInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_read_connection().map_err(|e| e.to_string())?;
        Ok(collect_agent_sources(&conn))
    })
    .await
    .map_err(|e| e.to_string())?
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

/// 在系统文件管理器中定位目录（macOS: Finder 选中；Windows: 资源管理器选中；Linux: 打开父目录）
#[tauri::command]
fn reveal_in_folder(path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err("路径不存在".into());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", p.display()))
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(p.parent().unwrap_or(p))
            .spawn()
            .map_err(|e| e.to_string())?;
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

fn home_data_dir_exists(rel: &str) -> bool {
    dirs::home_dir()
        .map(|h| h.join("Library/Application Support").join(rel).exists())
        .unwrap_or(false)
}

fn ide_installed(id: &str) -> bool {
    match id {
        "cursor" => macos_app_exists("Cursor") || command_on_path("cursor"),
        "antigravity" => {
            macos_app_exists("Antigravity")
                || macos_app_exists("Antigravity IDE")
                || command_on_path("antigravity")
        }
        "claude" => command_on_path("claude"),
        "codex" => command_on_path("codex"),
        "mimo" => macos_app_exists("Xiaomi MiMo"),
        "codebuddy" => {
            macos_app_exists("CodeBuddy CN")
                || macos_app_exists("CodeBuddy")
                || home_data_dir_exists("CodeBuddyExtension")
        }
        "qoder" => {
            macos_app_exists("Qoder IDE")
                || macos_app_exists("Qoder")
                || command_on_path("qodercli")
        }
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
        IdeAppStatus {
            id: "mimo".into(),
            label: "Xiaomi MiMo".into(),
            kind: "app".into(),
            installed: ide_installed("mimo"),
        },
        IdeAppStatus {
            id: "codebuddy".into(),
            label: "CodeBuddy".into(),
            kind: "app".into(),
            installed: ide_installed("codebuddy"),
        },
        IdeAppStatus {
            id: "qoder".into(),
            label: "Qoder".into(),
            kind: "app".into(),
            installed: ide_installed("qoder"),
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
    if !path.exists() {
        return Err("路径不存在".into());
    }
    // 传入文件时：GUI 应用直接打开文件；CLI 工具在其所在目录启动
    let cli_dir = if path.is_file() {
        path.parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| ".".to_string())
    } else {
        workspace_path.clone()
    };

    #[cfg(target_os = "macos")]
    {
        match ide.as_str() {
            "cursor" => open_in_macos_app("Cursor", &workspace_path),
            "antigravity" => {
                if macos_app_exists("Antigravity") {
                    open_in_macos_app("Antigravity", &workspace_path)
                } else if macos_app_exists("Antigravity IDE") {
                    open_in_macos_app("Antigravity IDE", &workspace_path)
                } else {
                    open_in_macos_app("Antigravity", &workspace_path)
                }
            }
            "claude" => open_in_terminal_cli("claude", &cli_dir),
            "codex" => open_in_terminal_cli("codex", &cli_dir),
            "mimo" => open_in_macos_app("Xiaomi MiMo", &workspace_path),
            "codebuddy" => {
                if macos_app_exists("CodeBuddy CN") {
                    open_in_macos_app("CodeBuddy CN", &workspace_path)
                } else {
                    open_in_macos_app("CodeBuddy", &workspace_path)
                }
            }
            "qoder" => {
                if macos_app_exists("Qoder IDE") {
                    open_in_macos_app("Qoder IDE", &workspace_path)
                } else {
                    open_in_macos_app("Qoder", &workspace_path)
                }
            }
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
            get_daily_timeline,
            get_usage_stats,
            list_prompts_cmd,
            get_prompt_cmd,
            create_prompt_cmd,
            pick_prompt_preview_image_cmd,
            import_prompt_preview_image_cmd,
            import_prompt_preview_image_bytes_cmd,
            update_prompt_cmd,
            delete_prompt_cmd,
            toggle_prompt_star_cmd,
            record_prompt_use_cmd,
            update_prompt_preview_size_cmd,
            sync_gpt_image_catalog_cmd,
            cache_prompt_previews_cmd,
            warm_prompt_preview_cache_cmd,
            count_uncached_prompt_previews_cmd,
            get_workspace_detail,
            list_workspaces,
            merge_workspace_cmd,
            move_conversation_cmd,
            list_conversations,
            get_conversation_messages,
            get_conversation_artifacts_cmd,
            get_workspace_artifacts_cmd,
            toggle_star,
            update_conversation_ai_title_cmd,
            clear_conversation_ai_title_cmd,
            save_conversation_ai_summary_cmd,
            set_conversation_ai_status_cmd,
            get_conversation_item,
            search_messages,
            trigger_sync,
            get_quota_snapshot,
            quota_pill::quota_pill_resize,
            quota_pill::quota_pill_show_menu,
            quota_pill::quota_pill_show,
            git_board::get_git_board,
            git_board::get_git_workspace_entry,
            git_board::get_git_commits,
            git_board::get_git_status_files,
            git_board::get_git_file_diff,
            git_board::git_stage_all,
            git_board::git_commit,
            git_board::git_push,
            git_board::get_git_pending_diff,
            git_board::get_git_commit_show,
            git_board::get_git_commit_file_diff,
            git_board::add_git_ignore,
            git_board::list_workspace_dir,
            git_board::read_workspace_file,
            git_board::search_workspace_files,
            git_board::git_stage_file,
            git_board::git_unstage_file,
            git_board::git_fetch,
            test_llm_connection,
            test_llm_pipeline,
            call_llm_with_fallback,
            get_llm_call_logs_cmd,
            get_llm_usage_summary_cmd,
            clear_llm_call_logs_cmd,
            get_database_path_info,
            get_agent_sources_cmd,
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
            open_workspace_in_ide_cmd,
            service_manager::service_list,
            service_manager::service_status,
            service_manager::service_start,
            service_manager::service_stop,
            service_manager::service_restart,
            service_manager::service_open,
            service_manager::service_open_in_ide,
            service_manager::service_detect_ides,
            service_manager::service_tail_log,
            service_manager::service_pick_folder,
            service_manager::service_scan_project,
            service_manager::service_probe_candidate,
            service_manager::service_upsert,
            service_manager::service_rename_project,
            service_manager::service_rename_service,
            service_manager::service_remove,
            reveal_in_folder
        ])
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title("AgentDeck - AI Coding Cockpit");
            }

            // 启动嵌入式 REST API 兼容服务（监听 127.0.0.1:8788，供给前端图片与外部服务无缝调用）
            http_server::start_http_server(app.handle().clone(), 8788);

            // 系统菜单栏入口（仅显示主窗口 / 退出）
            if let Err(e) = quota_tray::setup_tray(&app.handle().clone()) {
                log::warn!("[tray] 菜单栏组件初始化失败: {}", e);
            }

            // 额度悬浮条（独立透明置顶窗）
            if let Err(e) = quota_pill::setup_pill(&app.handle().clone()) {
                log::warn!("[quota-pill] 悬浮条初始化失败: {}", e);
            }

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
            // macOS：点击 Dock 图标恢复主窗口。
            // 注意：额度悬浮条等子窗可能仍可见，has_visible_windows 为 true，
            // 但主窗可能已被 ⌘W hide —— 因此不依赖该标志，始终尝试恢复 main。
            #[cfg(target_os = "macos")]
            RunEvent::Reopen {
                has_visible_windows,
                ..
            } => {
                if let Some(win) = app_handle.get_webview_window("main") {
                    // 主窗隐藏 / 最小化时都要能从 Dock 拉回来
                    let _ = win.unminimize();
                    let _ = win.show();
                    let _ = win.set_focus();
                } else if !has_visible_windows {
                    log::warn!("[dock] Reopen: main window missing");
                }
            }
            _ => {}
        });
}
