//! macOS 系统菜单栏（Tray）入口：额度展示已迁移到悬浮条（quota_pill），
//! 托盘只保留系统操作：显示主窗口 / 显示·隐藏额度悬浮条（文案随状态切换）/ 退出。

use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

pub const TRAY_ID: &str = "quota-tray";

const ITEM_SHOW: &str = "tray-show";
const ITEM_TOGGLE_PILL: &str = "tray-toggle-pill";
const ITEM_QUIT: &str = "tray-quit";

const PILL_LABEL_HIDE: &str = "隐藏额度悬浮条";
const PILL_LABEL_SHOW: &str = "显示额度悬浮条";

/// 悬浮条切换项，随托盘一起构建；quota_pill 在显隐变化时调 sync_pill_label 更新文案
static TOGGLE_PILL_ITEM: Mutex<Option<MenuItem<tauri::Wry>>> = Mutex::new(None);

pub fn sync_pill_label(app: &AppHandle) {
    let visible = app
        .get_webview_window(super::quota_pill::PILL_LABEL)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(true);
    let text = if visible { PILL_LABEL_HIDE } else { PILL_LABEL_SHOW };
    if let Some(item) = TOGGLE_PILL_ITEM.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        let _ = item.set_text(text);
    }
}

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, ITEM_SHOW, "显示主窗口", true, None::<&str>)?;
    let toggle_pill_item =
        MenuItem::with_id(app, ITEM_TOGGLE_PILL, PILL_LABEL_HIDE, true, None::<&str>)?;
    *TOGGLE_PILL_ITEM.lock().unwrap_or_else(|e| e.into_inner()) = Some(toggle_pill_item.clone());
    let quit_separator = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, ITEM_QUIT, "退出 AgentDeck", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[&show_item, &toggle_pill_item, &quit_separator, &quit_item],
    )?;

    let Some(icon) = app.default_window_icon().cloned() else {
        log::warn!("[tray] 未找到窗口图标，跳过菜单栏组件");
        return Ok(());
    };

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            ITEM_SHOW => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            ITEM_TOGGLE_PILL => {
                super::quota_pill::toggle_pill(app);
            }
            ITEM_QUIT => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}
