//!
//! pet/window.rs
//! 桌宠窗口的创建、显示、隐藏、移动、鼠标穿透控制。
//!
//! 设计思路：
//! - 窗口在 Tauri Builder.setup() 中**按需创建**（用户首次开启桌宠时），而非启动即创建，避免无桌宠用户多占一个 webview 进程的内存。
//! - 位置/可见性状态持久化由前端 localStorage 管理（PetStore），Rust 端只管"做动作"，不维护状态。
//! - 鼠标穿透用 `set_ignore_cursor_events(bool)`：true=鼠标穿透到下层、false=当前窗口接收事件。
//!   默认 false（桌宠需接收点击/拖拽），后续 Phase 4 可按身体 alpha 通道做精确命中。
//!
//! 修改历史：
//!   - 2026-07-17 @v0.6.0-beta.1: 初始创建 - 创建/显示/隐藏/移动/鼠标穿透 5 个命令
//!   - 2026-07-17 @v0.6.0-beta.1: 修复 - 创建时默认 set_ignore_cursor_events(false)，桌宠可交互
//!

use tauri::{AppHandle, Emitter, LogicalPosition, Manager, WebviewUrl, WebviewWindowBuilder};

/// 桌宠窗口的固定 label（与 tauri.conf.json 的 windows[].label 一致）
pub const PET_WINDOW_LABEL: &str = "pet";

/// 创建 pet 窗口（若已存在则忽略并返回 Ok，幂等）
///
/// 由前端开启桌宠时调用。第一次创建后窗体常驻，仅切换可见性。
#[tauri::command]
pub async fn create_pet_window(app: AppHandle) -> Result<(), String> {
    if app.get_webview_window(PET_WINDOW_LABEL).is_some() {
        // 已存在则直接复用，避免重复创建 webview 进程
        return Ok(());
    }

    // 初次创建时窗口位置用屏幕中央偏右下（QQ 企鹅常见位置），后续由前端持久化坐标控制
    // 用 index.html 共用主前端 dist，App.vue 通过 webview label 判断当前是主窗口还是 pet 窗口
    let window = WebviewWindowBuilder::new(&app, PET_WINDOW_LABEL, WebviewUrl::App("index.html".into()))
        .title("ScreenTime Pet")
        .inner_size(140.0, 140.0)
        .min_inner_size(140.0, 140.0)
        // v0.6.2：移除 max_inner_size 死锁，允许前端按皮肤 preferredSize 用 setSize 动态放大窗口
        // （resizable=false 仍禁止用户手动缩放，只有代码能 setSize；权限见 capabilities/pet.json 的 core:window:allow-set-size）
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .focused(false)
        .visible(false) // 默认隐藏，等前端 show_pet_window 才显示
        .build()
        .map_err(|e| format!("创建 pet 窗口失败: {e}"))?;

    // 初始鼠标穿透关闭：桌宠需接收拖拽/右键/点击事件（默认可交互）
    let _ = window.set_ignore_cursor_events(false);
    tracing::info!("pet 窗口创建成功");
    Ok(())
}

/// 显示桌宠窗口（不改变位置）
#[tauri::command]
pub async fn show_pet_window(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(PET_WINDOW_LABEL)
        .ok_or_else(|| "pet 窗口尚未创建，请先调用 create_pet_window".to_string())?;
    window.show().map_err(|e| format!("显示 pet 窗口失败: {e}"))?;
    // 通知 pet 窗口前端「已被显示」（用于触发入场动画等）
    let _ = app.emit_to(PET_WINDOW_LABEL, "pet-shown", ());
    Ok(())
}

/// 隐藏桌宠窗口（不销毁，下次 show 零延迟）
#[tauri::command]
pub async fn hide_pet_window(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(PET_WINDOW_LABEL)
        .ok_or_else(|| "pet 窗口尚未创建".to_string())?;
    window.hide().map_err(|e| format!("隐藏 pet 窗口失败: {e}"))?;
    Ok(())
}

/// 移动桌宠窗口到屏幕绝对坐标（前端拖拽结束 / 持久化位置恢复时调用）
///
/// 入参 x/y 是逻辑像素（LogicalPosition，即 CSS 像素），与前端 `clientX/screenX` 对齐，
/// Tauri 内部按设备 scale factor 自动换算为设备像素，避免高 DPI（如 Retina scale=2）屏偏移。
#[tauri::command]
pub async fn move_pet_window(app: AppHandle, x: f64, y: f64) -> Result<(), String> {
    let window = app
        .get_webview_window(PET_WINDOW_LABEL)
        .ok_or_else(|| "pet 窗口尚未创建".to_string())?;
    let pos = LogicalPosition::new(x, y);
    window
        .set_position(pos)
        .map_err(|e| format!("移动 pet 窗口失败: {e}"))?;
    Ok(())
}

/// 切换鼠标穿透（true=穿透到下层应用，false=当前窗口接收事件）
///
/// 前端根据鼠标是否在熊猫身体内动态切换：
/// - 鼠标在身体内 → passthrough=false（让桌宠响应点击）
/// - 鼠标在空白处 → passthrough=true（让下层应用响应）
#[tauri::command]
pub async fn set_pet_cursor_passthrough(app: AppHandle, passthrough: bool) -> Result<(), String> {
    let window = app
        .get_webview_window(PET_WINDOW_LABEL)
        .ok_or_else(|| "pet 窗口尚未创建".to_string())?;
    window
        .set_ignore_cursor_events(passthrough)
        .map_err(|e| format!("设置鼠标穿透失败: {e}"))?;
    Ok(())
}