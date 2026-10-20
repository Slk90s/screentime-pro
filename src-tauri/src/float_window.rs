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
//!   - 2026-09-10 @v0.7.6: 初始创建 - 创建/显示/隐藏/移动 4 个命令 + 显隐 helper
//!   - 2026-09-10 @v0.7.7: 尺寸 340×44 → 200×36，新增 resize_float_window 命令。
//!     原因：窗口固定 340px 而内容（如「● CPU 77% ↓1.4M ↑3.5M」）只有 ~200px，
//!     右侧一大片空白，用户反馈「浮窗过长」。改为由 FloatBar.vue 测量内容宽度后
//!     调本命令自适应收缩，窗口始终贴合内容（也顺带消除了透明窗口右侧的无效点击热区）。
//!   - 2026-10-18 @v1.2.0: 清理 - 移除 `create_float_window` / `hide_float_window` 两个
//!     死 IPC 命令（前端零调用，唯一调用方是 Rust 内部 `set_float_visible`，直接调
//!     `ensure_float_window` 不经过 IPC）。命令由 5 个减为 3 个（show/move/resize）。
//!   - 2026-10-19 @v1.2.1: 修复 - 「状态栏开了却看不到」。根因：localStorage 里的
//!     `float_pos` 是副屏还在时（或高分辨率机器上）保存的负坐标，副屏移除/分辨率变小后
//!     被无校验地恢复，窗口整个落到屏幕外（实测物理坐标 (-695,159)，单屏 1536×864）。
//!     新增 clamp_to_visible_area：show 前用物理像素枚举显示器，窗口与任一屏重叠不足
//!     1/3 宽时回落到主屏右上角。纯 Rust 兜底，不依赖前端能否枚举显示器。

use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, PhysicalPosition, WebviewUrl,
    WebviewWindowBuilder,
};

/// 浮窗固定 label（与 tauri.conf.json 的 windows[].label 一致）
pub const FLOAT_WINDOW_LABEL: &str = "float";

/// 浮窗初始/最小尺寸（逻辑像素）。实际宽度由前端测量内容后经 resize_float_window 调整。
///
/// 初始宽度取 340（与前端 BAR_W 一致）：这是「四项指标全开」时的安全上限，
/// 保证首次布局不被裁切；FloatBar.vue 会在 show 之前先测量并收缩，用户看不到宽态。
pub const FLOAT_INITIAL_WIDTH: f64 = 340.0;
pub const FLOAT_HEIGHT: f64 = 36.0;
pub const FLOAT_MIN_WIDTH: f64 = 140.0;

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
    .inner_size(FLOAT_INITIAL_WIDTH, FLOAT_HEIGHT)
    .min_inner_size(FLOAT_MIN_WIDTH, FLOAT_HEIGHT)
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

/// 窗口（物理像素矩形）与某显示器（物理像素矩形）的重叠宽度
fn overlap_x(
    win_l: i32,
    win_r: i32,
    mon_l: i32,
    mon_r: i32,
) -> i32 {
    win_r.min(mon_r) - win_l.max(mon_l)
}

/// **离屏纠偏**（v1.2.1）：窗口若与所有显示器都「几乎不相交」，则移回主屏右上默认位。
///
/// 背景：位置由前端 localStorage `float_pos` 恢复，保存的是**逻辑像素**绝对坐标。
/// 当副屏被移除、显示器布局变化或在不同分辨率的机器间迁移后，旧坐标会落到已不存在
/// 的屏幕上（实测：单屏 1536×864 机器上窗口被恢复到物理坐标 (-695,159)，整个在
/// 屏幕左侧之外，用户看到的就是「状态栏没了」）。不做校验时，只要陈旧位置不覆盖屏内，
/// 用户连拖回来的抓手都没有。
///
/// 判定与修正全部用**物理像素**，天然规避多屏混合 DPI 下逻辑/物理换算的歧义：
/// - 枚举所有显示器，只要窗口与某一屏的水平重叠 ≥ 窗口宽度的 1/3（36px 高下垂直
///   一般不会丢，只看水平即可），就认为仍在可见区域，不动用户位置；
/// - 否则贴到主屏右上角（物理坐标，右侧/顶部各留 24px 物理像素的边距近似）。
fn clamp_to_visible_area(window: &tauri::WebviewWindow) {
    let monitors: Vec<tauri::Monitor> = match window.available_monitors() {
        Ok(list) if !list.is_empty() => list,
        _ => match window.current_monitor() {
            Ok(Some(m)) => vec![m],
            _ => return,
        },
    };

    if let Ok(pos) = window.outer_position() {
        if let Ok(size) = window.inner_size() {
            let win_l = pos.x;
            let win_r = pos.x + size.width as i32;
            let min_visible = (size.width as i32 / 3).max(1);
            let on_any = monitors.iter().any(|m| {
                overlap_x(win_l, win_r, m.position().x, m.position().x + m.size().width as i32)
                    >= min_visible
            });
            if on_any {
                return;
            }
            tracing::warn!(
                saved_x = pos.x,
                saved_y = pos.y,
                "悬浮指标条恢复位置不在任何显示器上，回落到主屏右上角"
            );
            // 以主屏为基准放置（拿不到 primary 信息则用第一块显示器）
            let primary = match window.primary_monitor() {
                Ok(Some(m)) => m,
                _ => monitors[0].clone(),
            };
            let margin = 24;
            let target_x =
                primary.position().x + primary.size().width as i32 - size.width as i32 - margin;
            let target_y = primary.position().y + margin;
            let _ = window.set_position(PhysicalPosition::new(target_x, target_y));
        }
    }
}

/// 显隐浮窗（不存在则先幂等创建再切换；托盘菜单 / 设置页 / 全屏检测三条路径共用）
pub fn set_float_visible(app: &AppHandle, show: bool) -> Result<(), String> {
    ensure_float_window(app)?;
    let window = app
        .get_webview_window(FLOAT_WINDOW_LABEL)
        .ok_or_else(|| "float 窗口缺失".to_string())?;
    if show {
        // 显示前先纠偏：前端随后会按 localStorage 恢复并 fitWidth，但恢复出的可能是
        // 离屏旧坐标；在此兜回可见区，避免「开了却看不到」（v1.2.1）
        clamp_to_visible_area(&window);
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
pub async fn show_float_window(app: AppHandle) -> Result<(), String> {
    set_float_visible(&app, true)
}

/// 移动浮窗到屏幕绝对坐标（逻辑像素；前端拖拽结束 / 恢复位置时调用）
#[tauri::command]
pub async fn move_float_window(app: AppHandle, x: f64, y: f64) -> Result<(), String> {
    let window = app
        .get_webview_window(FLOAT_WINDOW_LABEL)
        .ok_or_else(|| "float 窗口尚未创建".to_string())?;
    window
        .set_position(LogicalPosition::new(x, y))
        .map_err(|e| format!("移动悬浮指标条失败: {e}"))?;
    // 恢复 localStorage 旧位置时可能落在已移除的副屏上；set 完立即纠偏，
    // 否则 onMounted 的 applyPosition 会把 show 前的 clamp 成果再拉回屏幕外（v1.2.1）
    clamp_to_visible_area(&window);
    Ok(())
}

/// 按内容自适应调整浮窗宽度（逻辑像素；前端测量 DOM 宽度后调用）
///
/// v0.7.7 新增：窗口宽度 = 内容宽度，避免固定 340px 时的右侧大片空白。
/// 高度用常量（FLOAT_HEIGHT）保持稳定，只有宽度变化。
///
/// **右边缘锚定**：宽度变化时同步平移窗口，使右上角位置保持不动
/// （浮窗默认贴在屏幕右上，若只改宽度会看起来往右「长」或往左「缩」）。
/// 锚定计算放在 Rust 侧完成，前端不需要额外申请 window 读取权限。
#[tauri::command]
pub async fn resize_float_window(app: AppHandle, width: f64) -> Result<(), String> {
    let window = app
        .get_webview_window(FLOAT_WINDOW_LABEL)
        .ok_or_else(|| "float 窗口尚未创建".to_string())?;
    let target_w = width.max(FLOAT_MIN_WIDTH);
    let scale = window.scale_factor().unwrap_or(1.0);
    // 当前物理尺寸/位置 → 换算成逻辑坐标做锚定
    let cur = window
        .inner_size()
        .map(|s| s.to_logical::<f64>(scale).width)
        .unwrap_or(FLOAT_INITIAL_WIDTH);
    if (cur - target_w).abs() < 0.5 {
        return Ok(()); // 宽度无变化，免抖动
    }
    if let Ok(pos) = window.outer_position() {
        let logical = pos.to_logical::<f64>(scale);
        let new_x = logical.x + (cur - target_w);
        let _ = window.set_position(LogicalPosition::new(new_x, logical.y));
    }
    window
        .set_size(LogicalSize::new(target_w, FLOAT_HEIGHT))
        .map_err(|e| format!("调整悬浮指标条尺寸失败: {e}"))?;
    // 锚定后再兜底一次：异常初始位置/尺寸组合下右锚定可能把窗口送出屏（v1.2.1）
    clamp_to_visible_area(&window);
    Ok(())
}
