//! 额度悬浮条：独立透明置顶小窗（label = `quota-pill`）。
//! - 前端以竖向悬浮条渲染各 provider 额度，hover 展开明细卡
//! - 本模块负责：窗口创建、5~10 分钟随机抖动刷新循环（`quota-pill-updated` 事件推送快照）、
//!   窗口高度调整命令（宽度恒定，收拢/展开只是前端 CSS 过渡）、
//!   拖拽边缘吸附（左/右缘 32pt 磁吸，floating 态强制离缘）、
//!   位置持久化（显示器名 + 停靠态 + 工作区比例，旧绝对坐标格式自动迁移）、
//!   指针监视器（非前台 hover 靠轮询补齐；不可见区域点击穿透，不误触下层 app）

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Monitor, PhysicalPosition,
    WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

use super::quota;

pub const PILL_LABEL: &str = "quota-pill";

/// logo 右键菜单项 id
const MENU_ITEM_HIDE: &str = "pill-hide";
const MENU_ITEM_QUIT: &str = "pill-quit";

/// 折叠态悬浮条宽度（逻辑像素），与前端 CSS 保持一致
const BAR_W: f64 = 64.0;
/// 停靠收拢后的细条宽度（逻辑像素），与前端 SLIVER_W 保持一致
const SLIVER_W: f64 = 8.0;
/// 细条高度（逻辑像素），垂直居中于窗口；与前端 SLIVER_H 保持一致。
/// 收拢时窗口仍是 bar 高，只有这一段算可交互区，其余继续点击穿透
const SLIVER_H: f64 = 96.0;
/// 窗口固定宽度：悬浮条 + 间距 + 明细卡（与前端 BAR_W+POPOVER_GAP+POPOVER_W 一致）。
/// 窗口永不横向改尺寸——透明区域 macOS 自动点击穿透，
/// 改宽度会移动窗口左缘、跳坐标系（Pulse 注释里点名的顽固 glitch 来源）
const PANEL_W: f64 = 354.0;
/// 初始高度，前端挂载后会按 provider 数量 resize
const INITIAL_H: f64 = 240.0;
/// 磁吸距离：拖拽中距工作区左/右缘 ≤ 此值即吸附；floating 态也强制保持这个距离
const DOCK_DISTANCE: f64 = 32.0;
/// 原生拖拽没有 mouseup 事件，Moved 间隔超过此值视为一次拖拽会话结束
const DRAG_SESSION_GAP: Duration = Duration::from_millis(300);

const REFRESH_MIN_SECS: u64 = 300;
const REFRESH_JITTER_SECS: u64 = 300;
/// 拖拽结束后延迟保存位置的等待时间
const SAVE_DEBOUNCE: Duration = Duration::from_millis(600);

static REFRESHING: AtomicBool = AtomicBool::new(false);
/// 拖拽期间 Moved 事件高频触发，用代际计数做 trailing 保存
static SAVE_GEN: AtomicU64 = AtomicU64::new(0);
/// 自己 set_position / set_size 触发的 Moved 事件标记（重入 guard）
static HANDLING: AtomicBool = AtomicBool::new(false);
/// 拖拽会话：指针相对窗口左上角的 grab 偏移 + 最近事件时刻
static DRAG: Mutex<Option<DragSession>> = Mutex::new(None);
/// 当前停靠态与最新位置（setup 后恒有值，防抖落盘的数据源）
static PLACEMENT: Mutex<Option<Placement>> = Mutex::new(None);
/// 明细卡是否向左展开（docked 右缘 / 悬浮在右半屏）
static MIRROR: AtomicBool = AtomicBool::new(false);
/// 是否处于停靠态（左/右缘）；前端据此在指针离开后收成细条
static DOCKED: AtomicBool = AtomicBool::new(false);
/// 前端收拢态；指针监视器据此圈定可交互区
static COLLAPSED: AtomicBool = AtomicBool::new(false);
/// 明细卡是否展开；展开时整窗都可交互（bar→卡的过渡间隙不能断 hover）
static POPOVER_OPEN: AtomicBool = AtomicBool::new(false);
/// 上次向前端广播的布局值 (mirror, docked)，避免重复 emit
static LAYOUT_SENT: Mutex<Option<(bool, bool)>> = Mutex::new(None);

/// logo 右键原生菜单（启动时构建一次，前端 invoke 时在鼠标位置弹出）
struct PillMenu(Menu<tauri::Wry>);

#[derive(Clone, Copy, PartialEq, Eq)]
enum DockSide {
    Left,
    Right,
    Floating,
}

#[derive(Clone)]
struct Placement {
    x: f64,
    y: f64,
    dock: DockSide,
    display: Option<String>,
}

#[derive(Clone, Copy)]
struct DragSession {
    grab: (f64, f64),
    last: Instant,
}

/// 显示器工作区（逻辑像素，已排除菜单栏 / Dock）
struct WorkArea {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl WorkArea {
    fn of(monitor: &Monitor) -> WorkArea {
        let scale = monitor.scale_factor();
        let wa = monitor.work_area();
        let pos = wa.position;
        let size = wa.size;
        WorkArea {
            min_x: pos.x as f64 / scale,
            min_y: pos.y as f64 / scale,
            max_x: (pos.x + size.width as i32) as f64 / scale,
            max_y: (pos.y + size.height as i32) as f64 / scale,
        }
    }
}

fn primary_monitor(app: &AppHandle) -> Option<Monitor> {
    app.primary_monitor()
        .ok()
        .flatten()
        .or_else(|| app.available_monitors().ok().and_then(|v| v.first().cloned()))
}

fn monitor_by_name(app: &AppHandle, name: &str) -> Option<Monitor> {
    app.available_monitors()
        .ok()?
        .into_iter()
        .find(|m| m.name().map(|n| n == name).unwrap_or(false))
}

fn monitor_containing(app: &AppHandle, point: PhysicalPosition<f64>) -> Option<Monitor> {
    app.available_monitors().ok()?.into_iter().find(|m| {
        let pos = m.position();
        let size = m.size();
        point.x >= pos.x as f64
            && point.x < (pos.x + size.width as i32) as f64
            && point.y >= pos.y as f64
            && point.y < (pos.y + size.height as i32) as f64
    })
}

/// 由期望位置判定停靠并吸附/收拢：docked 贴缘，floating 离缘 ≥ dock_dist，y 始终留在工作区内。
/// 坐标系由调用方决定（拖拽路径传物理坐标 + 物理 dock_dist，恢复路径传逻辑坐标）。
fn resolve_placement(
    desired_x: f64,
    desired_y: f64,
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    w: f64,
    h: f64,
    dock_dist: f64,
) -> (f64, f64, DockSide) {
    let dock = if desired_x - min_x <= dock_dist {
        DockSide::Left
    } else if max_x - (desired_x + w) <= dock_dist {
        DockSide::Right
    } else {
        DockSide::Floating
    };
    let x = match dock {
        DockSide::Left => min_x,
        DockSide::Right => (max_x - w).max(min_x),
        DockSide::Floating => {
            let lo = min_x + dock_dist;
            let hi = (max_x - w - dock_dist).max(lo);
            desired_x.clamp(lo, hi)
        }
    };
    let y_hi = (max_y - h).max(min_y);
    (x, desired_y.clamp(min_y, y_hi), dock)
}

/// 明细卡展开方向：bar 中心在右半屏（含 docked 右缘）则向左展开
fn mirror_for(bar_x: f64, wa: &WorkArea) -> bool {
    bar_x + BAR_W / 2.0 > (wa.min_x + wa.max_x) / 2.0
}

fn set_layout(app: &AppHandle, mirror: bool, docked: bool) {
    MIRROR.store(mirror, Ordering::Relaxed);
    DOCKED.store(docked, Ordering::Relaxed);
    let mut sent = LAYOUT_SENT.lock().unwrap_or_else(|e| e.into_inner());
    if *sent != Some((mirror, docked)) {
        *sent = Some((mirror, docked));
        let _ = app.emit("quota-pill-layout", PillLayout { mirror, docked });
    }
}

fn record_placement(app: &AppHandle, x: f64, y: f64, dock: DockSide, display: Option<String>) {
    *PLACEMENT.lock().unwrap_or_else(|e| e.into_inner()) = Some(Placement { x, y, dock, display });
    schedule_save(app.clone());
}

fn current_dock() -> DockSide {
    PLACEMENT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|p| p.dock)
        .unwrap_or(DockSide::Floating)
}

fn position_file(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("quota_pill.json"))
}

enum SavedPlacement {
    /// 旧格式：绝对逻辑坐标
    Legacy(f64, f64),
    /// 新格式：显示器名 + 停靠态 + 工作区比例
    Current {
        display: Option<String>,
        dock: DockSide,
        h: f64,
        v: f64,
    },
}

fn load_placement(app: &AppHandle) -> Option<SavedPlacement> {
    let raw = std::fs::read_to_string(position_file(app)?).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    if let (Some(h), Some(vr)) = (
        v.get("h").and_then(|x| x.as_f64()),
        v.get("v").and_then(|x| x.as_f64()),
    ) {
        let dock = match v.get("dock").and_then(|x| x.as_str()) {
            Some("left") => DockSide::Left,
            Some("right") => DockSide::Right,
            _ => DockSide::Floating,
        };
        return Some(SavedPlacement::Current {
            display: v.get("display").and_then(|x| x.as_str()).map(String::from),
            dock,
            h,
            v: vr,
        });
    }
    Some(SavedPlacement::Legacy(
        v.get("x")?.as_f64()?,
        v.get("y")?.as_f64()?,
    ))
}

fn write_placement(app: &AppHandle, p: &Placement, h_ratio: f64, v_ratio: f64) {
    let Some(path) = position_file(app) else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let dock = match p.dock {
        DockSide::Left => "left",
        DockSide::Right => "right",
        DockSide::Floating => "floating",
    };
    let json = serde_json::json!({
        "display": p.display,
        "dock": dock,
        "h": h_ratio,
        "v": v_ratio,
    })
    .to_string();
    let _ = std::fs::write(path, json);
}

/// 防抖落盘：把最新位置换算成「保存时窗口所在屏」的工作区比例后写入
fn flush_placement(app: &AppHandle) {
    let guard = PLACEMENT.lock().unwrap_or_else(|e| e.into_inner());
    let Some(p) = guard.clone() else {
        return;
    };
    drop(guard);

    let monitor = p
        .display
        .as_deref()
        .and_then(|name| monitor_by_name(app, name))
        .or_else(|| primary_monitor(app));
    let (Some(monitor), Some(win)) = (monitor, app.get_webview_window(PILL_LABEL)) else {
        return;
    };
    let wa = WorkArea::of(&monitor);
    let scale = monitor.scale_factor();
    let Ok(size) = win.inner_size() else {
        return;
    };
    let h = size.height as f64 / scale;
    // p.x 是 bar 位置，横向比例按 bar 宽度折算
    let h_ratio = ((p.x - wa.min_x) / (wa.max_x - wa.min_x - BAR_W).max(1.0)).clamp(0.0, 1.0);
    let v_ratio = ((p.y - wa.min_y) / (wa.max_y - wa.min_y - h).max(1.0)).clamp(0.0, 1.0);
    write_placement(app, &p, h_ratio, v_ratio);
}

fn schedule_save(app: AppHandle) {
    let gen = SAVE_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    tauri::async_runtime::spawn(async move {
        sleep(SAVE_DEBOUNCE).await;
        // 只有最后一代（拖拽已停）才落盘
        if SAVE_GEN.load(Ordering::SeqCst) != gen {
            return;
        }
        flush_placement(&app);
    });
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

/// 计算启动位置：新格式按屏名+比例还原（docked 直接贴缘），旧格式原样使用，
/// 都无效则默认停靠主屏左缘垂直居中
fn restore_placement(app: &AppHandle) -> Placement {
    if let Some(saved) = load_placement(app) {
        match saved {
            SavedPlacement::Current { display, dock, h, v } => {
                let monitor = display
                    .as_deref()
                    .and_then(|name| monitor_by_name(app, name))
                    .or_else(|| primary_monitor(app));
                if let Some(m) = monitor {
                    let wa = WorkArea::of(&m);
                    let x = match dock {
                        DockSide::Left => wa.min_x,
                        DockSide::Right => (wa.max_x - BAR_W).max(wa.min_x),
                        DockSide::Floating => {
                            let lo = wa.min_x + DOCK_DISTANCE;
                            let hi = (wa.max_x - BAR_W - DOCK_DISTANCE).max(lo);
                            (wa.min_x + h * (wa.max_x - wa.min_x - BAR_W).max(0.0)).clamp(lo, hi)
                        }
                    };
                    let y_hi = (wa.max_y - INITIAL_H).max(wa.min_y);
                    let y = (wa.min_y + v * (wa.max_y - wa.min_y - INITIAL_H).max(0.0))
                        .clamp(wa.min_y, y_hi);
                    return Placement {
                        x,
                        y,
                        dock,
                        display: display.or_else(|| m.name().cloned()),
                    };
                }
            }
            SavedPlacement::Legacy(x, y) => {
                if position_on_any_monitor(app, x, y) {
                    return Placement { x, y, dock: DockSide::Floating, display: None };
                }
            }
        }
    }
    match primary_monitor(app) {
        Some(m) => {
            let wa = WorkArea::of(&m);
            Placement {
                x: wa.min_x,
                y: wa.min_y + ((wa.max_y - wa.min_y) - INITIAL_H) / 2.0,
                dock: DockSide::Left,
                display: m.name().cloned(),
            }
        }
        None => Placement { x: 16.0, y: 16.0, dock: DockSide::Floating, display: None },
    }
}

/// Moved 事件：拖拽会话内按指针位置逐帧吸附（Pulse 式），
/// 会话外（我们自己的 set_position / resize 锚定）只记录位置。
///
/// 拖拽以 **bar 几何**为准：用户抓住的是 bar，吸附/停靠按 bar 位置判定，
/// 窗口位置由 bar 推导（镜像时窗口左缘 = bar 左缘 - 卡片区宽度），
/// 这样拖过屏幕中线、镜像翻转时 bar 屏幕位置不跳。
///
/// 全程用物理坐标：事件位置、指针、工作区天然同系，
/// 混合 DPI 多屏下各除以不同 scale 会把坐标系串掉。
fn handle_moved(app: &AppHandle, win: &WebviewWindow, pos: PhysicalPosition<i32>) {
    let lx = pos.x as f64;
    let ly = pos.y as f64;

    // 自己触发的移动：只记录，不判定（防止重入，也防止把 resize 锚定当成拖拽）
    if HANDLING.load(Ordering::Relaxed) {
        let scale = win.scale_factor().unwrap_or(1.0);
        let display = win
            .current_monitor()
            .ok()
            .flatten()
            .and_then(|m| m.name().cloned());
        let win_w = win
            .inner_size()
            .map(|s| s.width as f64)
            .unwrap_or(PANEL_W * scale);
        let bar_lx = if MIRROR.load(Ordering::Relaxed) {
            lx + win_w - BAR_W * scale
        } else {
            lx
        };
        record_placement(app, bar_lx / scale, ly / scale, current_dock(), display);
        return;
    }

    let Ok(size) = win.inner_size() else {
        return;
    };
    let w = size.width as f64;
    let h = size.height as f64;
    let win_scale = win.scale_factor().unwrap_or(1.0);

    // 按指针所在屏判定：按窗口所在屏会把拖向副屏的条每帧拉回原屏
    let Ok(pointer) = win.cursor_position() else {
        return;
    };
    let monitor = monitor_containing(app, pointer)
        .or_else(|| win.current_monitor().ok().flatten())
        .or_else(|| primary_monitor(app));
    let Some(monitor) = monitor else {
        return;
    };
    let m_scale = monitor.scale_factor();
    let wa = monitor.work_area();
    let (wa_min_x, wa_min_y) = (wa.position.x as f64, wa.position.y as f64);
    let (wa_max_x, wa_max_y) = (
        (wa.position.x + wa.size.width as i32) as f64,
        (wa.position.y + wa.size.height as i32) as f64,
    );
    let dock_dist = DOCK_DISTANCE * m_scale;
    let display = monitor.name().cloned();

    // bar 当前屏幕位置与物理宽度（镜像展开时 bar 在窗口右端）
    let bar_w = BAR_W * win_scale;
    let bar_lx = if MIRROR.load(Ordering::Relaxed) {
        lx + w - bar_w
    } else {
        lx
    };

    let now = Instant::now();
    let mut drag = DRAG.lock().unwrap_or_else(|e| e.into_inner());
    let grab = match *drag {
        Some(session) if now.duration_since(session.last) < DRAG_SESSION_GAP => {
            *drag = Some(DragSession { grab: session.grab, last: now });
            session.grab
        }
        _ => {
            // 新会话首帧：捕获指针相对 bar 左上角的 grab 偏移，并就地吸附/收拢一次
            let grab = (pointer.x - bar_lx, pointer.y - ly);
            *drag = Some(DragSession { grab, last: now });
            drop(drag);
            let (bar_fx, fy, dock) = resolve_placement(
                bar_lx, ly, wa_min_x, wa_min_y, wa_max_x, wa_max_y, bar_w, h, dock_dist,
            );
            let mirror = bar_fx + bar_w / 2.0 > (wa_min_x + wa_max_x) / 2.0;
            set_layout(app, mirror, dock != DockSide::Floating);
            let win_fx = if mirror { bar_fx - (w - bar_w) } else { bar_fx };
            apply_dragged(app, win, lx, ly, win_fx, fy, bar_fx, dock, m_scale, display);
            return;
        }
    };
    drop(drag);

    let (bar_fx, fy, dock) = resolve_placement(
        pointer.x - grab.0,
        pointer.y - grab.1,
        wa_min_x,
        wa_min_y,
        wa_max_x,
        wa_max_y,
        bar_w,
        h,
        dock_dist,
    );
    let mirror = bar_fx + bar_w / 2.0 > (wa_min_x + wa_max_x) / 2.0;
    set_layout(app, mirror, dock != DockSide::Floating);
    let win_fx = if mirror { bar_fx - (w - bar_w) } else { bar_fx };

    apply_dragged(app, win, lx, ly, win_fx, fy, bar_fx, dock, m_scale, display);
}

/// 吸附结果落地：需要时移动窗口（窗口坐标），记录 bar 位置（换算成目标屏的逻辑坐标）
#[allow(clippy::too_many_arguments)]
fn apply_dragged(
    app: &AppHandle,
    win: &WebviewWindow,
    lx: f64,
    ly: f64,
    win_fx: f64,
    win_fy: f64,
    bar_fx: f64,
    dock: DockSide,
    scale: f64,
    display: Option<String>,
) {
    if (win_fx - lx).abs() > 0.5 || (win_fy - ly).abs() > 0.5 {
        HANDLING.store(true, Ordering::Relaxed);
        let _ = win.set_position(PhysicalPosition::new(
            win_fx.round() as i32,
            win_fy.round() as i32,
        ));
        HANDLING.store(false, Ordering::Relaxed);
    }
    record_placement(app, bar_fx / scale, win_fy / scale, dock, display);
}

pub fn setup_pill(app: &AppHandle) -> tauri::Result<()> {
    let placement = restore_placement(app);
    let (bar_x, y) = (placement.x, placement.y);

    // 初始布局：前端挂载时通过 quota_pill_layout 查询
    if let Some(m) = placement
        .display
        .as_deref()
        .and_then(|name| monitor_by_name(app, name))
        .or_else(|| primary_monitor(app))
    {
        let wa = WorkArea::of(&m);
        MIRROR.store(mirror_for(bar_x, &wa), Ordering::Relaxed);
    }
    DOCKED.store(placement.dock != DockSide::Floating, Ordering::Relaxed);
    *LAYOUT_SENT.lock().unwrap_or_else(|e| e.into_inner()) = None;
    *PLACEMENT.lock().unwrap_or_else(|e| e.into_inner()) = Some(placement);

    // 窗口宽度恒定 = bar + 间距 + 明细卡；镜像时窗口左缘向左让出卡片区，bar 贴窗口右端
    let win_x = if MIRROR.load(Ordering::Relaxed) {
        bar_x - (PANEL_W - BAR_W)
    } else {
        bar_x
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
        .inner_size(PANEL_W, INITIAL_H)
        .position(win_x, y)
        .build()?;

    let app_for_event = app.clone();
    let win_for_event = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Moved(pos) = event {
            handle_moved(&app_for_event, &win_for_event, *pos);
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
    spawn_pointer_watcher(app.clone());
    Ok(())
}

/// 前端在 logo 上右键时调用：在鼠标位置弹出悬浮条菜单。
/// popup_menu 在 NSMenu tracking 的嵌套事件循环里阻塞，期间 IPC 仍会被处理；
/// muda 的菜单内部 RefCell 非重入，跟踪期间再次进入会 panic_already_borrowed 直接 abort。
static MENU_SHOWING: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn quota_pill_show_menu(app: AppHandle) -> Result<(), String> {
    if MENU_SHOWING.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let result = (|| {
        let win = app
            .get_webview_window(PILL_LABEL)
            .ok_or_else(|| "quota pill window not found".to_string())?;
        let menu = app.state::<PillMenu>().inner().0.clone();
        win.popup_menu(&menu).map_err(|e| e.to_string())
    })();
    MENU_SHOWING.store(false, Ordering::SeqCst);
    result
}

/// 前端布局状态：明细卡展开方向 + 是否停靠（停靠态指针离开后收成细条）
#[derive(Clone, Copy, serde::Serialize)]
pub struct PillLayout {
    pub mirror: bool,
    pub docked: bool,
}

/// 前端挂载时查询布局状态。
#[tauri::command]
pub fn quota_pill_layout() -> PillLayout {
    PillLayout {
        mirror: MIRROR.load(Ordering::Relaxed),
        docked: DOCKED.load(Ordering::Relaxed),
    }
}

/// 前端同步收拢态与明细卡开合；指针监视器据此圈定可交互区（细条 / bar / 整窗）
#[tauri::command]
pub fn quota_pill_set_hover_state(collapsed: bool, popover_open: bool) {
    COLLAPSED.store(collapsed, Ordering::Relaxed);
    POPOVER_OPEN.store(popover_open, Ordering::Relaxed);
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
    // 隐藏期间循环被 refresh_if_visible 跳过，恢复显示立即补一次探测
    let app_for_refresh = app.clone();
    tauri::async_runtime::spawn(async move {
        refresh_now(&app_for_refresh).await;
    });
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

/// 前端按内容调整窗口高度；宽度恒定 PANEL_W（bar + 间距 + 明细卡），横向永不改尺寸
/// （透明区域 macOS 自动点击穿透，改宽度会移动窗口左缘、跳坐标系）。
/// 高度增长不许探出工作区底部。
#[tauri::command]
pub fn quota_pill_resize(app: AppHandle, height: f64) -> Result<(), String> {
    let win = app
        .get_webview_window(PILL_LABEL)
        .ok_or_else(|| "quota pill window not found".to_string())?;
    let h = height.clamp(80.0, 1200.0);

    let scale = win.scale_factor().unwrap_or(1.0);
    let Ok(pos) = win.outer_position() else {
        return Err("quota pill window position unavailable".into());
    };
    let mut x = pos.x as f64 / scale;
    let mut y = pos.y as f64 / scale;

    let monitor = win
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| primary_monitor(&app));
    if let Some(m) = &monitor {
        let wa = WorkArea::of(m);
        x = x.max(wa.min_x);
        y = y.min((wa.max_y - h).max(wa.min_y)).max(wa.min_y);
    }

    HANDLING.store(true, Ordering::Relaxed);
    let _ = win.set_size(LogicalSize::new(PANEL_W, h));
    let _ = win.set_position(LogicalPosition::new(x, y));
    HANDLING.store(false, Ordering::Relaxed);

    // 记录 bar 位置：镜像时 bar 在窗口右端
    let display = monitor.as_ref().and_then(|m| m.name().cloned());
    let bar_x = if MIRROR.load(Ordering::Relaxed) {
        x + PANEL_W - BAR_W
    } else {
        x
    };
    record_placement(&app, bar_x, y, current_dock(), display);
    Ok(())
}

/// 指针监视器：macOS 不给非前台 app 派发 hover 事件，轮询全局指针位置。
/// 按可交互区（收拢=细条，明细卡开=整窗，否则=bar）判定指针进出：
/// - 进出沿广播 `quota-pill-hover`（payload = 是否在区内），非前台 hover 展开/收起靠它
/// - 区内广播 `quota-pill-pointer`（窗口相对逻辑坐标，离开时 null）：非前台窗口的
///   mousemove macOS 只发给 key window，明细卡命中只能靠这里报的坐标算
/// - 区外整窗 `set_ignore_cursor_events(true)`：恒宽窗口的透明区域 macOS 不做像素级
///   点击穿透，必须显式忽略，否则整块隐形矩形都在吃下层 app 的点击、误激活本应用
fn spawn_pointer_watcher(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut inside = false;
        let mut ignoring = false;
        let mut last_ptr: Option<(f64, f64)> = None;
        loop {
            sleep(Duration::from_millis(30)).await;
            let Some(win) = app.get_webview_window(PILL_LABEL) else {
                continue;
            };
            if !win.is_visible().unwrap_or(false) {
                // 隐藏期间不判定，恢复显示后按指针实际位置重新出发沿
                inside = false;
                last_ptr = None;
                continue;
            }
            let (Ok(pos), Ok(size), Ok(pointer)) =
                (win.outer_position(), win.inner_size(), win.cursor_position())
            else {
                continue;
            };
            let scale = win.scale_factor().unwrap_or(1.0);
            let (win_x, win_y) = (pos.x as f64, pos.y as f64);
            let (win_w, win_h) = (size.width as f64, size.height as f64);
            let mirror = MIRROR.load(Ordering::Relaxed);
            let (zone_x, zone_w, zone_y, zone_h) = if COLLAPSED.load(Ordering::Relaxed) {
                let sw = SLIVER_W * scale;
                let sh = (SLIVER_H * scale).min(win_h);
                let x = if mirror { win_x + win_w - sw } else { win_x };
                (x, sw, win_y + (win_h - sh) / 2.0, sh)
            } else if POPOVER_OPEN.load(Ordering::Relaxed) {
                (win_x, win_w, win_y, win_h)
            } else {
                let bw = BAR_W * scale;
                let x = if mirror { win_x + win_w - bw } else { win_x };
                (x, bw, win_y, win_h)
            };
            let now_inside = pointer.x >= zone_x
                && pointer.x < zone_x + zone_w
                && pointer.y >= zone_y
                && pointer.y < zone_y + zone_h;
            // 拖拽进行中整窗保持可交互，防止指针甩出 bar 范围时 ignore 切换打断拖拽
            let dragging = drag_active();
            let want_ignore = !now_inside && !dragging;
            if want_ignore != ignoring {
                ignoring = want_ignore;
                let _ = win.set_ignore_cursor_events(want_ignore);
            }
            if now_inside != inside {
                inside = now_inside;
                let _ = app.emit("quota-pill-hover", now_inside);
            }
            // 拖拽期间窗口在指针底下跑，相对坐标没有意义：原样不动，不广播也不制造离开沿
            let rel = if dragging {
                last_ptr
            } else if now_inside {
                Some(((pointer.x - win_x) / scale, (pointer.y - win_y) / scale))
            } else {
                None
            };
            let moved = match (last_ptr, rel) {
                (Some((px, py)), Some((nx, ny))) => (px - nx).abs() > 1.5 || (py - ny).abs() > 1.5,
                (a, b) => a.is_some() != b.is_some(),
            };
            if moved {
                last_ptr = rel;
                let payload = rel.map(|(x, y)| PillPointer { x, y });
                let _ = app.emit("quota-pill-pointer", payload);
            }
        }
    });
}

/// 指针在悬浮条窗口内的位置（逻辑像素，相对窗口左上角）
#[derive(Clone, Copy, serde::Serialize)]
pub struct PillPointer {
    pub x: f64,
    pub y: f64,
}

/// 原生拖拽进行中（Moved 事件还新鲜）
fn drag_active() -> bool {
    DRAG.lock()
        .unwrap_or_else(|e| e.into_inner())
        .map(|s| s.last.elapsed() < Duration::from_millis(500))
        .unwrap_or(false)
}

fn spawn_refresh_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // 启动后延迟几秒做首次拉取，避开应用启动高峰
        sleep(Duration::from_secs(3)).await;
        loop {
            refresh_if_visible(&app).await;
            let secs = REFRESH_MIN_SECS + jitter_secs();
            sleep(Duration::from_secs(secs)).await;
        }
    });
}

/// 悬浮球隐藏期间不探测（关了就真的停）；恢复显示由 show_pill 立即补一次。
async fn refresh_if_visible(app: &AppHandle) {
    let visible = app
        .get_webview_window(PILL_LABEL)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    if !visible {
        return;
    }
    refresh_now(app).await;
}

async fn refresh_now(app: &AppHandle) {
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
