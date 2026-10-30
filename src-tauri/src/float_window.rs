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
//!   - 2026-10-21 @v1.2.2: 重构 - 高度改为「模式相关」动态取值，接液态玻璃。
//!     ① FLOAT_HEIGHT 保留为普通模式高度（36），新增 FLOAT_HEIGHT_GLASS（44）+ 玻璃圆角；
//!        实际取值统一走 `float_glass::height_for(glass)`，本文件不再直接读 FLOAT_HEIGHT。
//!     ② **关键**：`resize_float_window` 原先把高度写死成 FLOAT_HEIGHT，而该命令被前端
//!        `fitWidth()` 每秒调用（1Hz tick + ResizeObserver）——玻璃模式一开，下一秒就被
//!        压回 36px。现改为按 AppState 里的 `status_bar_config.float_glass` 取值。
//!     ③ `ensure_float_window` 不负责材质：conf 已预创建 float 窗口，本函数开头的
//!        `if exists { return Ok(()) }` 会早返回，材质若放这里永远不生效。
//!        材质由调用方（commands / lib）在 ensure 之后经 `float_glass::apply_via_app` 应用。
//!   - 2026-10-29 @v1.2.12: 重构 - 离屏纠偏判定搬到公共 util `window_bounds`（整改 1.3）。
//!     桌宠的位置存在**同一类** localStorage 绝对坐标里、却完全没防，v1.2.1 那个根因在它身上
//!     会原样复发；两处共用一套判定才不会各修各的。本文件保留同名私有函数作薄封装（只传
//!     「右上角 + 24px 边距」两个 float 专属参数），三个调用点不动。判定顺带变严格：从只看
//!     水平重叠改成两轴各 1/3，且枚举显示器失败时从静默 return 改成 warn 日志。

use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::float_glass;

/// 浮窗固定 label（与 tauri.conf.json 的 windows[].label 一致）
pub const FLOAT_WINDOW_LABEL: &str = "float";

/// 浮窗初始/最小尺寸（逻辑像素）。实际宽度由前端测量内容后经 resize_float_window 调整。
///
/// 初始宽度取 340（与前端 BAR_W 一致）：这是「四项指标全开」时的安全上限，
/// 保证首次布局不被裁切；FloatBar.vue 会在 show 之前先测量并收缩，用户看不到宽态。
pub const FLOAT_INITIAL_WIDTH: f64 = 340.0;
/// 普通模式高度（逻辑像素）。玻璃模式取 `float_glass::FLOAT_HEIGHT_GLASS`。
///
/// ⚠️ 不要在本文件里直接用这个常量算 set_size —— 一律走 `float_glass::height_for()`
/// 或本文件的 `current_height()`，否则玻璃模式会被压回 36px（v1.2.2 修复的坑）。
///
/// v1.2.2 起本文件内已无直接引用（全部改走 height_for），仅作为对外口径保留，
/// 供外部/未来代码读取"普通模式高度"这一语义值。
#[allow(dead_code)]
pub const FLOAT_HEIGHT: f64 = float_glass::FLOAT_HEIGHT_PLAIN;
pub const FLOAT_MIN_WIDTH: f64 = 140.0;

/// 离屏回落时贴角留的物理像素边距（v1.2.1 起就是这个值，搬进 `window_bounds` 时保持不变）
const FLOAT_OFFSCREEN_MARGIN: i32 = 24;

/// 从 AppState 读当前是否处于玻璃模式（读不到按 false，退回普通态最安全）
///
/// 为什么经 AppHandle 读而不是加参数：`resize_float_window` 由前端 1Hz 调用，
/// 让前端每次多传一个 glass 参数会把"配置真相"分散到两端；这里直接读后端缓存，
/// 单一来源，且与 `set_status_bar_config` 写入的缓存天然一致。
fn current_glass(app: &AppHandle) -> bool {
    app.try_state::<std::sync::Arc<crate::AppState>>()
        .map(|st| {
            st.status_bar_config
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .float_glass
        })
        .unwrap_or(false)
}

/// 当前模式下的窗口高度（逻辑像素）
fn current_height(app: &AppHandle) -> f64 {
    float_glass::height_for(current_glass(app))
}

/// 幂等创建浮窗（已存在则直接复用）
///
/// visible=false 创建；显隐统一走 set_float_visible（透明空窗即使 show 也不可见，
/// 因此不存在「先 show 再加载」的白闪问题）
pub fn ensure_float_window(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window(FLOAT_WINDOW_LABEL).is_some() {
        return Ok(());
    }
    // v1.2.2：创建时就按当前模式定高，避免"先 36 后 44"的二次调整
    let h = current_height(app);
    let window = WebviewWindowBuilder::new(
        app,
        FLOAT_WINDOW_LABEL,
        WebviewUrl::App("index.html".into()),
    )
    .title("ScreenTime Float")
    .inner_size(FLOAT_INITIAL_WIDTH, h)
    .min_inner_size(FLOAT_MIN_WIDTH, h)
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

/// **离屏纠偏**（v1.2.1）：窗口若与所有显示器都「几乎不相交」，则移回主屏右上默认位。
///
/// v1.2.12 整改 1.3：判定逻辑整体搬到公共 util `window_bounds`（桌宠原来没防，同一个
/// 根因会再犯一次）。本函数只剩 float 的两个参数：回落角 = 右上、边距 = 24 物理像素，
/// 与 v1.2.1 起的既有行为一致。事故背景与判定口径见 `window_bounds.rs` 模块头。
///
/// 与旧实现的差异（都是往严格方向走，不会把屏内窗口误挪走）：
/// - 旧实现只查**水平**重叠（36px 高的条子垂直不会丢），新 util 两轴各要求 1/3 ——
///   桌宠这种高窗口必须查垂直，float 顺带一起受益（比如被拖到已消失的上方副屏）；
/// - 旧实现用 `inner_size`，新 util 用 `outer_size`（无边框透明窗两者相同）；
/// - 旧实现枚举失败时静默 `return`，新 util 返回 `Err` → 这里降级成一条 warn 日志，
///   可观测性补上（原来「为什么没纠偏」在日志里查不到）。
fn clamp_to_visible_area(window: &tauri::WebviewWindow) {
    if let Err(e) = crate::window_bounds::clamp_to_visible_area(
        window,
        crate::window_bounds::FallbackCorner::TopRight,
        FLOAT_OFFSCREEN_MARGIN,
    ) {
        tracing::warn!(error = %e, "悬浮指标条离屏校验跳过（保持当前位置）");
    }
}

/// 显隐浮窗（不存在则先幂等创建再切换；托盘菜单 / 设置页 / 全屏检测三条路径共用）
pub fn set_float_visible(app: &AppHandle, show: bool) -> Result<(), String> {
    ensure_float_window(app)?;
    let window = app
        .get_webview_window(FLOAT_WINDOW_LABEL)
        .ok_or_else(|| "float 窗口缺失".to_string())?;
    if show {
        // v1.2.2：**补材质**。用户可能是「先开玻璃开关（浮窗当时关着，apply 无处可落）
        // → 再打开浮窗」的顺序；也可能全屏自动隐藏后恢复、上一轮材质已被系统回收。
        // 这两条路径都绕开了 commands.rs 的翻转分支，所以必须在每次 show 前补一次。
        //
        // 顺序上放在 clamp/show **之前**：apply_via_app 与 show 都会派发到主线程，
        // 先入队先执行，材质先于窗口可见落定 → 不会出现「先露出深色底再变玻璃」的闪跳。
        // 失败只记 warn（apply_via_app 内部已记），绝不阻断显示。
        if current_glass(app) {
            if let Err(e) = float_glass::apply_via_app(app, true) {
                tracing::warn!(error = %e, "显示浮窗前应用液态玻璃失败，退化为 CSS 兜底样式");
            }
        }
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
///
/// **右边缘锚定**：宽度变化时同步平移窗口，使右上角位置保持不动
/// （浮窗默认贴在屏幕右上，若只改宽度会看起来往右「长」或往左「缩」）。
/// 锚定计算放在 Rust 侧完成，前端不需要额外申请 window 读取权限。
///
/// v1.2.2 修复：高度**不再写死**。原实现 `set_size(w, FLOAT_HEIGHT)` 会把玻璃模式的
/// 44px 每秒压回 36px（本命令被前端 fitWidth() 以 1Hz + ResizeObserver 调用）。
/// 现按 `status_bar_config.float_glass` 取 `float_glass::height_for()`。
/// 高度变化时**顶部锚定**（只改 x 与 size，不改 y），窗口向下生长，符合"贴屏幕右上"预期。
#[tauri::command]
pub async fn resize_float_window(app: AppHandle, width: f64) -> Result<(), String> {
    let window = app
        .get_webview_window(FLOAT_WINDOW_LABEL)
        .ok_or_else(|| "float 窗口尚未创建".to_string())?;
    let target_w = width.max(FLOAT_MIN_WIDTH);
    let target_h = current_height(&app);
    let scale = window.scale_factor().unwrap_or(1.0);
    // 当前物理尺寸/位置 → 换算成逻辑坐标做锚定
    let (cur_w, cur_h) = window
        .inner_size()
        .map(|s| {
            let l = s.to_logical::<f64>(scale);
            (l.width, l.height)
        })
        .unwrap_or((FLOAT_INITIAL_WIDTH, target_h));
    // 宽高都无变化才免调整，避免无谓的 set_size/set_position 抖动
    if (cur_w - target_w).abs() < 0.5 && (cur_h - target_h).abs() < 0.5 {
        return Ok(());
    }
    if let Ok(pos) = window.outer_position() {
        let logical = pos.to_logical::<f64>(scale);
        // 右锚定：宽度变小时窗口左边缘右移，保持右上角不动
        let new_x = logical.x + (cur_w - target_w);
        let _ = window.set_position(LogicalPosition::new(new_x, logical.y));
    }
    window
        .set_size(LogicalSize::new(target_w, target_h))
        .map_err(|e| format!("调整悬浮指标条尺寸失败: {e}"))?;
    // 锚定后再兜底一次：异常初始位置/尺寸组合下右锚定可能把窗口送出屏（v1.2.1）
    clamp_to_visible_area(&window);
    Ok(())
}
