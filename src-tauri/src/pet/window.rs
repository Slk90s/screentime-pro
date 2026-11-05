//!
//! pet/window.rs
//! 桌宠窗口的创建、显示、隐藏、移动、鼠标穿透控制。
//!
//! 设计思路：
//! - `tauri.conf.json` 预创建隐藏 pet 窗，创建命令幂等复用；显隐与位置由前端持久化状态驱动。
//! - 鼠标穿透由 `pet/hit_mask.rs` 按身体 alpha 网格动态管理，本文件只保留手动开关与创建默认值。
//!
//! 修改历史：
//!   - 2026-07-17 @v0.6.0-beta.1: 初始创建 - 创建/显示/隐藏/移动/鼠标穿透 5 个命令
//!   - 2026-07-17 @v0.6.0-beta.1: 修复 - 创建时默认 set_ignore_cursor_events(false)，桌宠可交互
//!   - 2026-09-16 @v0.8.0: hide_pet_window 补发 pet-hidden 事件（前端据此暂停气泡计时）；
//!     鼠标穿透改由 pet/hit_mask.rs 按身体 alpha 网格动态管理（本文件只保留创建时默认值与手动开关）
//!   - 2026-10-29 @v1.2.12: 修复 - `move_pet_window` 新增可选 `clamp` 参数接公共 util
//!     `window_bounds`（整改 1.3）。桌宠位置与 v1.2.1 那次「状态栏消失」是同一根因
//!     （localStorage 存的绝对坐标 + 拔副屏），但 pet 侧从来没防；默认 false 是为了
//!     不让逐帧拖拽调用被中途纠偏，只有「恢复位置」这类调用显式传 true。
//!

use tauri::{AppHandle, Emitter, LogicalPosition, Manager};

/// 桌宠窗口的固定 label（与 tauri.conf.json 的 windows[].label 一致）
pub const PET_WINDOW_LABEL: &str = "pet";

/// 离屏回落贴右下角时留的物理像素边距。
///
/// 量级对齐前端 `petStore.defaultPosition()`（那里是逻辑像素：右边内缩 60、下边内缩 100，
/// 下边多留的是任务栏）。这里只需要「看得见、不被任务栏压住」，不必与前端逐像素一致，
/// 取 64 折中；真要对齐得把窗口 scale factor 传进换算，收益不值这笔复杂度。
const PET_OFFSCREEN_MARGIN: i32 = 64;

/// 幂等复用 `tauri.conf.json` 预创建的 pet 窗口；不存在时明确报错。
#[tauri::command]
pub async fn create_pet_window(app: AppHandle) -> Result<(), String> {
    app.get_webview_window(PET_WINDOW_LABEL)
        .map(|_| ())
        .ok_or_else(|| "pet 窗口未由 tauri.conf.json 预创建".to_string())
}

/// 显示桌宠窗口（不改变位置）
#[tauri::command]
pub async fn show_pet_window(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(PET_WINDOW_LABEL)
        .ok_or_else(|| "pet 窗口尚未创建，请先调用 create_pet_window".to_string())?;
    window
        .show()
        .map_err(|e| format!("显示 pet 窗口失败: {e}"))?;
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
    window
        .hide()
        .map_err(|e| format!("隐藏 pet 窗口失败: {e}"))?;
    // v0.8.0：与 show 对称地广播 pet-hidden。
    // 用途：前端据此暂停随机气泡计时（旧实现隐藏后计时仍在跑，纯空耗），
    // 以及让「全屏隐藏」逻辑知道当前是否已隐藏（避免重复 show/hide）。
    let _ = app.emit_to(PET_WINDOW_LABEL, "pet-hidden", ());
    Ok(())
}

/// 移动桌宠窗口到屏幕绝对坐标（前端拖拽结束 / 持久化位置恢复时调用）
///
/// 入参 x/y 是逻辑像素（LogicalPosition，即 CSS 像素），与前端 `clientX/screenX` 对齐，
/// Tauri 内部按设备 scale factor 自动换算为设备像素，避免高 DPI（如 Retina scale=2）屏偏移。
///
/// `clamp`（v1.2.12 整改 1.3，默认 **false**）：位置来自 localStorage 恢复时传 `true`，
/// 落点若已不在任何显示器上就贴回主屏右下角。为什么默认关：本命令在**拖拽回退路径**里
/// 是逐帧调用的，每帧都纠偏会让窗口在用户手上「弹跳」回角落（v1.2.1 给 float 做同样
/// 判定时也踩过这一点 —— float 的 `move_float_window` 只在拖拽结束时调用才没暴露）。
/// 所以「恢复」与「拖拽」两类调用必须显式区分，不能靠 Rust 侧猜。
/// 正常拖拽走 `begin_drag`（原生循环），其收尾自己调 `window_bounds`，不经过这里。
#[tauri::command]
pub async fn move_pet_window(
    app: AppHandle,
    x: f64,
    y: f64,
    clamp: Option<bool>,
) -> Result<(), String> {
    let window = app
        .get_webview_window(PET_WINDOW_LABEL)
        .ok_or_else(|| "pet 窗口尚未创建".to_string())?;
    let pos = LogicalPosition::new(x, y);
    window
        .set_position(pos)
        .map_err(|e| format!("移动 pet 窗口失败: {e}"))?;
    if clamp.unwrap_or(false) {
        // 纠偏结果不回传前端：前端那份 localStorage 仍是旧坐标，下次启动会再纠一次。
        // 可接受的重复劳动 —— 想省掉它得让 Rust 反向通知前端改写持久化（批次 4.1 的账）。
        if let Err(e) = crate::window_bounds::clamp_to_visible_area(
            &window,
            crate::window_bounds::FallbackCorner::BottomRight,
            PET_OFFSCREEN_MARGIN,
        ) {
            tracing::warn!(error = %e, "桌宠离屏校验跳过（保持恢复位置）");
        }
    }
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
