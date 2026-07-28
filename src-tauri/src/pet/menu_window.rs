//!
//! pet/menu_window.rs
//! 桌宠右键菜单独立窗口（v0.6.2-beta.17 引入）。
//!
//! 设计动机：
//! - 原 PetContextMenu 用 `Teleport to="body"` 嵌在 pet webview 内，
//!   `position:fixed` 受限于 pet webview 视口（150×330），菜单无法拖到全桌面。
//! - 解决：新建独立 Tauri WebviewWindow（透明、置顶、装饰关、跨整个桌面可拖）。
//!
//! 行为：
//! - create 时给个默认位置（用户右键处），由前端自由移动
//! - show/hide 切换显隐，move 改位置
//! - 前端组件 PetMenuWindow.vue 作为该窗口的根（label "pet-menu"）
//!
//! 注意：与 pet 本体窗口不同，菜单窗口**默认鼠标穿透关闭**——菜单要接收事件
//! （拖动 + 按钮点击）；窗口外区域由前端用 document.pointerdown 监听器
//! 把"外部点击"事件经 Tauri emit 回主逻辑处理。
//!
//! 修改历史：
//!   - 2026-07-25 @v0.6.2-beta.17: 初始创建 - 解决菜单无法在桌面自由拖拽

use tauri::{AppHandle, Emitter, LogicalPosition, Manager, WebviewUrl, WebviewWindowBuilder};

/// 桌宠右键菜单窗口的固定 label（与 tauri.conf.json 的 windows[].label 一致）
pub const PET_MENU_WINDOW_LABEL: &str = "pet-menu";

/// 创建 pet-menu 窗口（若已存在则忽略并返回 Ok，幂等）
///
/// 窗口初始大小 280×600（兼容现有菜单内容高度），按需可由前端 move_pet_menu_window 改位。
/// `transparent`+`decorations(false)`+`always_on_top`+`skip_taskbar`，与 pet 本体一致。
#[tauri::command]
pub async fn create_pet_menu_window(app: AppHandle) -> Result<(), String> {
    if app.get_webview_window(PET_MENU_WINDOW_LABEL).is_some() {
        return Ok(());
    }

    let window = WebviewWindowBuilder::new(
        &app,
        PET_MENU_WINDOW_LABEL,
        WebviewUrl::App("index.html".into()),
    )
    .title("ScreenTime Pet Menu")
    .inner_size(300.0, 620.0)
    // 不设 min/max 限制，让前端按需调整
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .shadow(false)
    .focused(false)
    .visible(false) // 默认隐藏，等前端 show_pet_menu_window 才显示
    .build()
    .map_err(|e| format!("创建 pet 菜单窗口失败: {e}"))?;

    // 菜单窗口必须接收事件（拖动 + 按钮），所以默认**不穿透**
    let _ = window.set_ignore_cursor_events(false);
    tracing::info!("pet-menu 窗口创建成功");
    Ok(())
}

/// 显示 pet-menu 窗口（不改变位置）
#[tauri::command]
pub async fn show_pet_menu_window(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(PET_MENU_WINDOW_LABEL)
        .ok_or_else(|| "pet-menu 窗口尚未创建，请先调用 create_pet_menu_window".to_string())?;
    window
        .show()
        .map_err(|e| format!("显示 pet-menu 窗口失败: {e}"))?;
    let _ = app.emit_to(PET_MENU_WINDOW_LABEL, "pet-menu-shown", ());
    Ok(())
}

/// 隐藏 pet-menu 窗口（不销毁，下次 show 零延迟）
#[tauri::command]
pub async fn hide_pet_menu_window(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(PET_MENU_WINDOW_LABEL)
        .ok_or_else(|| "pet-menu 窗口尚未创建".to_string())?;
    window
        .hide()
        .map_err(|e| format!("隐藏 pet-menu 窗口失败: {e}"))?;
    Ok(())
}

/// 移动 pet-menu 窗口到屏幕绝对坐标
///
/// 入参 x/y 是逻辑像素（LogicalPosition = CSS 像素），与 Tauri setPosition 对齐。
/// 在用户拖动菜单时每帧调用（但配合 rAF 合并，实际频率 ≤ 60fps）。
#[tauri::command]
pub async fn move_pet_menu_window(
    app: AppHandle,
    x: f64,
    y: f64,
) -> Result<(), String> {
    let window = app
        .get_webview_window(PET_MENU_WINDOW_LABEL)
        .ok_or_else(|| "pet-menu 窗口尚未创建".to_string())?;
    let pos = LogicalPosition::new(x, y);
    window
        .set_position(pos)
        .map_err(|e| format!("移动 pet-menu 窗口失败: {e}"))?;
    Ok(())
}
