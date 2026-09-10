//!
//! float_window.rs
//! 悬浮指标条窗口（v0.7.6，2026-09-10）：透明置顶无边框小窗，1Hz 显示系统指标。
//!
//! 设计（照抄 pet/window.rs 模式）：
//! - 运行时幂等创建（ensure_float_window），首次创建后窗体常驻、只切显隐（不销毁 webview，
//!   重新显示零延迟）
//! - 位置由前端 FloatBar.vue 用 localStorage 持久化并恢复；Rust 只提供 move 命令
//!   （逻辑像素入参，与 usePetDrag/move_pet_window 同约定，避免高 DPI 偏移）
//! - Windows 采样线程（lib.rs）每 tick 做全屏检测 → 全屏自动隐藏（system_load/fullscreen.rs）
//! - 与 pet 的差异：整条窗口都是拖拽区（原生 startDragging，无阈值判断），无右键菜单、
//!   无鼠标穿透动态切换
//! - 窗口同时在 tauri.conf.json 中声明（visible:false），与 pet/pet-menu 双保险方式一致：
//!   conf 预创建则 ensure 幂等复用，未预创建则运行时按需创建
//!
//! 修改历史：
//!   - 2026-09-10 @Unreleased: 初始创建 - 创建/显示/隐藏/移动 4 个命令 + 显隐 helper

use tauri::{AppHandle, LogicalPosition, Manager, WebviewUrl, WebviewWindowBuilder};

/// 浮窗固定 label（与 tauri.conf.json 的 windows[].label 一致）
pub const FLOAT_WINDOW_LABEL: &str = "float";

/// 幂等创建浮窗（已存在则直接复用）
///
/// visible=false 创建；显隐统一走 set_float_visible（透明空窗即使 show 也不可见，
/// 因此不存在「先 show 再加载」的白闪问题）
pub fn ensure_float_window(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window(FLOAT_WINDOW_LABEL).is_some() {
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(
        app,
        FLOAT_WINDOW_LABEL,
        WebviewUrl::App("index.html".into()),
    )
    .title("ScreenTime Float")
    .inner_size(340.0, 44.0)
    .min_inner_size(340.0, 44.0)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .closable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .shadow(false)
    .focused(false)
    .visible(false)
    .build()
    .map_err(|e| format!("创建悬浮指标条失败: {e}"))?;
    let _ = window.set_ignore_cursor_events(false); // 可交互（整条拖拽）
    tracing::info!("悬浮指标条窗口创建成功");
    Ok(())
}

/// 显隐浮窗（不存在则先幂等创建再切换；托盘菜单 / 设置页 / 全屏检测三条路径共用）
pub fn set_float_visible(app: &AppHandle, show: bool) -> Result<(), String> {
    ensure_float_window(app)?;
    let window = app
        .get_webview_window(FLOAT_WINDOW_LABEL)
        .ok_or_else(|| "float 窗口缺失".to_string())?;
    if show {
        window
            .show()
            .map_err(|e| format!("显示悬浮指标条失败: {e}"))
    } else {
        window
            .hide()
            .map_err(|e| format!("隐藏悬浮指标条失败: {e}"))
    }
}

#[tauri::command]
pub async fn create_float_window(app: AppHandle) -> Result<(), String> {
    ensure_float_window(&app)
}

#[tauri::command]
pub async fn show_float_window(app: AppHandle) -> Result<(), String> {
    set_float_visible(&app, true)
}

#[tauri::command]
pub async fn hide_float_window(app: AppHandle) -> Result<(), String> {
    set_float_visible(&app, false)
}

/// 移动浮窗到屏幕绝对坐标（逻辑像素；前端拖拽结束 / 恢复位置时调用）
#[tauri::command]
pub async fn move_float_window(app: AppHandle, x: f64, y: f64) -> Result<(), String> {
    let window = app
        .get_webview_window(FLOAT_WINDOW_LABEL)
        .ok_or_else(|| "float 窗口尚未创建".to_string())?;
    window
        .set_position(LogicalPosition::new(x, y))
        .map_err(|e| format!("移动悬浮指标条失败: {e}"))
}
