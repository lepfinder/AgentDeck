//! 额度悬浮条：独立透明置顶小窗（label = `quota-pill`）。
//! - 前端以竖向悬浮条渲染各 provider 额度，hover 展开明细卡
//! - 本模块负责：窗口创建、5~10 分钟随机抖动刷新循环（`quota-pill-updated` 事件推送快照）、
//!   窗口尺寸调整命令、位置持久化（含拔屏回退主屏左缘）

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};

use super::quota;

pub const PILL_LABEL: &str = "quota-pill";

/// logo 右键菜单项 id
const MENU_ITEM_HIDE: &str = "pill-hide";
const MENU_ITEM_QUIT: &str = "pill-quit";

/// 折叠态悬浮条宽度（逻辑像素），与前端 CSS 保持一致
const BAR_W: f64 = 64.0;
/// 初始高度，前端挂载后会按 provider 数量 resize
const INITIAL_H: f64 = 240.0;

const REFRESH_MIN_SECS: u64 = 300;
const REFRESH_JITTER_SECS: u64 = 300;
/// 拖拽结束后延迟保存位置的等待时间
const SAVE_DEBOUNCE: Duration = Duration::from_millis(600);

static REFRESHING: AtomicBool = AtomicBool::new(false);
/// 拖拽期间 Moved 事件高频触发，用代际计数做 trailing 保存
static SAVE_GEN: AtomicU64 = AtomicU64::new(0);
static PENDING_POS: Mutex<Option<(f64, f64)>> = Mutex::new(None);

/// logo 右键原生菜单（启动时构建一次，前端 invoke 时在鼠标位置弹出）
struct PillMenu(Menu<tauri::Wry>);

fn position_file(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("quota_pill.json"))
}

fn load_position(app: &AppHandle) -> Option<(f64, f64)> {
    let raw = std::fs::read_to_string(position_file(app)?).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    Some((v.get("x")?.as_f64()?, v.get("y")?.as_f64()?))
}

fn write_position(app: &AppHandle, x: f64, y: f64) {
    let Some(path) = position_file(app) else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let json = serde_json::json!({ "x": x, "y": y }).to_string();
    let _ = std::fs::write(path, json);
}

fn position_on_any_monitor(app: &AppHandle, x: f64, y: f64) -> bool {
    let Ok(monitors) = app.available_monitors() else {
        return false;
    };
    monitors.iter().any(|m| {
        let scale = m.scale_factor();
        let mx = m.position().x as f64 / scale;
        let my = m.position().y as f64 / scale;
        let mw = m.size().width as f64 / scale;
        let mh = m.size().height as f64 / scale;
        x >= mx - 8.0 && x <= mx + mw - BAR_W && y >= my - 8.0 && y <= my + mh - 40.0
    })
}

fn default_position(app: &AppHandle, h: f64) -> (f64, f64) {
    let monitor = app
        .primary_monitor()
        .ok()
        .flatten()
        .or_else(|| app.available_monitors().ok().and_then(|v| v.first().cloned()));
    if let Some(m) = monitor {
        let scale = m.scale_factor();
        let px = m.position().x as f64 / scale;
        let py = m.position().y as f64 / scale;
        let mh = m.size().height as f64 / scale;
        return (px + 16.0, py + (mh - h) / 2.0);
    }
    (16.0, 16.0)
}

fn schedule_save(app: AppHandle) {
    let gen = SAVE_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    tauri::async_runtime::spawn(async move {
        sleep(SAVE_DEBOUNCE).await;
        // 只有最后一代（拖拽已停）才落盘
        if SAVE_GEN.load(Ordering::SeqCst) != gen {
            return;
        }
        if let Some((x, y)) = *PENDING_POS.lock().unwrap_or_else(|e| e.into_inner()) {
            write_position(&app, x, y);
        }
    });
}

pub fn setup_pill(app: &AppHandle) -> tauri::Result<()> {
    let (x, y) = match load_position(app) {
        Some((px, py)) if position_on_any_monitor(app, px, py) => (px, py),
        _ => default_position(app, INITIAL_H),
    };

    let window = WebviewWindowBuilder::new(app, PILL_LABEL, WebviewUrl::App("index.html".into()))
        .title("AgentDeck Quota")
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .closable(false)
        .shadow(false)
        .accept_first_mouse(true)
        .visible_on_all_workspaces(true)
        .inner_size(BAR_W, INITIAL_H)
        .position(x, y)
        .build()?;

    let app_for_event = app.clone();
    let win_for_event = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Moved(pos) = event {
            let scale = win_for_event.scale_factor().unwrap_or(1.0);
            let lx = pos.x as f64 / scale;
            let ly = pos.y as f64 / scale;
            {
                let mut guard = PENDING_POS.lock().unwrap_or_else(|e| e.into_inner());
                *guard = Some((lx, ly));
            }
            schedule_save(app_for_event.clone());
        }
    });

    let hide_item = MenuItem::with_id(app, MENU_ITEM_HIDE, "隐藏额度悬浮条", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, MENU_ITEM_QUIT, "退出 AgentDeck", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&hide_item, &separator, &quit_item])?;
    app.manage(PillMenu(menu));

    app.on_menu_event(move |app, event| match event.id().as_ref() {
        MENU_ITEM_HIDE => hide_pill(app),
        MENU_ITEM_QUIT => app.exit(0),
        _ => {}
    });

    spawn_refresh_loop(app.clone());
    Ok(())
}

/// 前端在 logo 上右键时调用：在鼠标位置弹出悬浮条菜单。
#[tauri::command]
pub fn quota_pill_show_menu(app: AppHandle) -> Result<(), String> {
    let win = app
        .get_webview_window(PILL_LABEL)
        .ok_or_else(|| "quota pill window not found".to_string())?;
    let menu = app.state::<PillMenu>().inner().0.clone();
    win.popup_menu(&menu).map_err(|e| e.to_string())
}

/// 恢复显示悬浮条（托盘 / 主窗口额度弹层共用），并通知前端收起明细卡。
pub fn show_pill(app: &AppHandle) -> Result<(), String> {
    let win = app
        .get_webview_window(PILL_LABEL)
        .ok_or_else(|| "quota pill window not found".to_string())?;
    win.show().map_err(|e| e.to_string())?;
    let _ = win.set_focus();
    let _ = app.emit("quota-pill-shown", ());
    super::quota_tray::sync_pill_label(app);
    Ok(())
}

fn hide_pill(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(PILL_LABEL) {
        let _ = win.hide();
    }
    super::quota_tray::sync_pill_label(app);
}

/// 托盘切换项：显示中则隐藏，已隐藏则恢复。
pub fn toggle_pill(app: &AppHandle) {
    let visible = app
        .get_webview_window(PILL_LABEL)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(true);
    if visible {
        hide_pill(app);
    } else if let Err(e) = show_pill(app) {
        log::warn!("[quota-pill] 显示悬浮条失败: {}", e);
    }
}

/// 主窗口额度弹层调用：恢复显示悬浮条。
#[tauri::command]
pub fn quota_pill_show(app: AppHandle) -> Result<(), String> {
    show_pill(&app)
}

/// 前端 hover 展开明细卡时调整窗口大小；折叠回悬浮条尺寸。
#[tauri::command]
pub fn quota_pill_resize(app: AppHandle, width: f64, height: f64) -> Result<(), String> {
    let win = app
        .get_webview_window(PILL_LABEL)
        .ok_or_else(|| "quota pill window not found".to_string())?;
    let w = width.clamp(BAR_W, 520.0);
    let h = height.clamp(80.0, 1200.0);
    win.set_size(LogicalSize::new(w, h)).map_err(|e| e.to_string())
}

fn spawn_refresh_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // 启动后延迟几秒做首次拉取，避开应用启动高峰
        sleep(Duration::from_secs(3)).await;
        loop {
            refresh_once(&app).await;
            let secs = REFRESH_MIN_SECS + jitter_secs();
            sleep(Duration::from_secs(secs)).await;
        }
    });
}

async fn refresh_once(app: &AppHandle) {
    if REFRESHING.swap(true, Ordering::SeqCst) {
        return;
    }
    let result =
        tauri::async_runtime::spawn_blocking(|| quota::fetch_quota_snapshot(false)).await;
    if let Ok(snapshot) = result {
        let _ = app.emit("quota-pill-updated", &snapshot);
    }
    REFRESHING.store(false, Ordering::SeqCst);
}

fn jitter_secs() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(7);
    nanos % (REFRESH_JITTER_SECS + 1)
}

async fn sleep(duration: Duration) {
    let handle = tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(duration);
    });
    let _ = handle.await;
}
