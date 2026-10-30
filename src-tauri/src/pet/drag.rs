//!
//! pet/drag.rs
//! 桌宠拖拽循环（v1.2.12 批次 2）：把「每帧 IPC」换成「Rust 侧读全局光标自己搬窗」。
//!
//! 为什么要动这一刀（v0.6.2-beta.28 那次「已根治」其实没生效）：
//! - `capabilities/pet.json` 一直没给 `core:window:allow-start-dragging`（批次 1.1 已补），
//!   所以 `usePetDrag.ts` 里的 `startDragging()` **每次都被 ACL 拒**，catch 后回落到
//!   「rAF + 每帧 `invoke('move_pet_window')`」这条它自己声称已经淘汰的旧路径。
//! - 旧路径有两个结构性毛病，都不是调参能解决的：
//!   ① 每帧一次异步 IPC + 一次 `set_position`，往返延迟叠加 → 观感「不跟手」；
//!   ② `pointermove` 监听挂在 webview 的 `window` 上、又没 `setPointerCapture`，
//!   光标甩出 140×140／150×330 这块矩形后 DOM 就**不再收到事件** → 桌宠原地掉队，
//!   光标绕回来时又跳一下（快速横甩必现）。
//!
//! 单一形态（v1.2.12 收尾修订，2026-10-30）：
//! 本模块是**三平台唯一的拖拽实现**。原先设计是「Windows / Linux 走 OS 原生 `startDragging()`，
//! 本模块只作兜底」，收尾修订时取消了那条路径 —— 原生拖拽的 Promise 在拖拽刚开始就 resolve，
//! 而真正的松手信号（`WM_LBUTTONUP`）被 DefWindowProc 的模态移动循环吃掉（tao 在
//! `WM_EXITSIZEMOVE` 里要手动补发一个，`tao-0.35.3/.../windows/event_loop.rs:1036-1041`），
//! 且 tauri 不把 `MouseEvent` 转发给 JS —— **前端永远等不到 pointerup**，收尾无从谈起。
//! 详见 `src/pet/composables/usePetDrag.ts` 文件头与 `docs/AUDIT-2026-10-30.md` P0-1。
//! 本模块的循环读的是操作系统全局光标，光标信息与「指针在不在窗口矩形内」无关，
//! 旧路径的毛病 ② 直接消失；每帧只有一次进程内 `set_position` 投递（不是 IPC），毛病 ① 也大幅缓解。
//!
//! 线程约束（批次 2.6 实测 tauri 2.11.5 + tauri-runtime-wry 2.11.4 源码的结论）：
//! - `WebviewWindow::set_position()` 在非主线程调用是**安全**的：runtime 侧统一走
//!   `send_user_message` → 若不是主线程就 `proxy.send_event(message)` 投给主线程事件循环
//!   （`tauri-runtime-wry-2.11.4/src/lib.rs:235-255`），消息处理里才真正调
//!   `window.set_outer_position`（`:3543`）。所以**不需要** `run_on_main_thread` 包一层 ——
//!   自己包反而会把「投递」变成「等待」。
//! - 代价是**投递即返回**：拿不到真实落点，也不能靠它报错。
//! - 读取类 API 正好相反：`window_getter!` 走 `send_user_message(tx)` + `rx.recv()`
//!   （`:197-211`），在非主线程会**阻塞等主线程**。因此拖拽循环里**一次 getter 都不查**，
//!   scale / 抓取偏移全部在 `begin_drag` 里读好缓存；`end_drag` 回报的位置用循环自己
//!   记录的最后一个目标值，而不是去问窗口。
//!
//! 坐标空间（v1.2.12 收尾修订，审计 P1-1）：**整个拖拽链路走「平台原生坐标」**。
//!
//! 旧实现在 `begin_drag` 缓存 scale、把光标除成逻辑值，循环里又交给
//! `set_position(LogicalPosition)` 由 tao 在**应用消息那一刻**再乘一次 live scale
//! （`tao-0.35.3/.../windows/window.rs:230`）。除时用缓存值、乘时用实时值 ——
//! 单屏恒定 DPI 下两者相等、结果精确，但**跨到不同 DPI 的显示器**时
//! `WM_DPICHANGED` 会改 live scale，位移被等比放大（1.0 → 1.5 就是 1.5 倍），窗口一跳。
//!
//! 修法不是「一律改物理」（那样在 mac 上会错）：`set_position` 收到 `PhysicalPosition`
//! 后三平台并不一致 —— Windows 直接 `cast()` 不乘 scale，macOS / Linux 却先
//! `to_logical(scale_factor())` 再交给平台（`macos/window.rs:729-730`、`linux/window.rs:458-461`）。
//! 而 `cursor_native()` 给的正是**各平台 set_position 期望的那个空间**
//! （Win = 物理像素；mac = points；Linux = X11 root 像素，见 `hit_mask.rs` 的说明）。
//! 所以做法是：
//! - `grab` 在 `begin_drag` 里用 `logical_to_native()` 转一次原生偏移；
//! - 循环内 `target = cursor_native − grab_native`，**加减法不带任何 scale**；
//! - `set_position` 按平台传 `PhysicalPosition` / `LogicalPosition`（两者在各自平台上
//!   都恰好等于「原生坐标」）；
//! - `end_drag` 用 `native_to_logical()` 转回逻辑值回报前端（前端持久化逻辑坐标）。
//!
//! 除法与乘法的来源就此唯一，跨 DPI 屏天然正确。
//!
//! 修改历史：
//!   - 2026-10-29 @v1.2.12: 新增 - 批次 2.1/2.2/2.4/2.5/2.6
//!   - 2026-10-30 @v1.2.12: 修订（审计 P0-1 / P1-1）- ① 全链路改平台原生坐标，消除跨 DPI 屏的
//!     scale 分叉；② 成为三平台唯一拖拽实现（原生档已删，见上「单一形态」）
//!

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::pet::hit_mask::cursor_native;
use crate::pet::window::PET_WINDOW_LABEL;
use crate::window_bounds::{self, FallbackCorner};

/// 循环间隔：8ms ≈ 125Hz 上限。
/// 实际频率由系统调用耗时决定（Win `GetCursorPos` / macOS `CGEventGetLocation` 都是亚 µs 级），
/// 而 `set_position` 只是往主线程队列投消息，不构成背压 —— 靠下面的节流把无效帧滤掉。
const DRAG_TICK_MS: u64 = 8;

/// 位移合并节流：两轴**都**小于该值（**原生坐标单位**）就不发这次 `set_position`。
/// 光标静止时循环一帧都不动窗口，避免 125Hz 空转把主线程事件队列灌满。
///
/// 注：Windows 上这是物理像素，高 DPI 屏下实际比 4 逻辑像素更细 —— 只会更跟手，不会更卡。
const DRAG_EPS: f64 = 4.0;

/// 拖拽期离屏纠偏的贴角边距（与 `window.rs` 的恢复路径同值，保持手感一致）
const DRAG_MARGIN: i32 = 64;

/// 逻辑值 → 本平台原生坐标。只有 Windows 的原生坐标是物理像素，需要乘 scale。
#[cfg(target_os = "windows")]
fn logical_to_native(v: f64, scale: f64) -> f64 {
    v * scale
}
#[cfg(not(target_os = "windows"))]
fn logical_to_native(v: f64, _scale: f64) -> f64 {
    // macOS 的 points 与 Linux 的 root 像素本就与 Tauri 逻辑坐标同空间
    v
}

/// 本平台原生坐标 → 逻辑值。与 `logical_to_native` 严格互逆（单测里对拍）。
#[cfg(target_os = "windows")]
fn native_to_logical(v: f64, scale: f64) -> f64 {
    if scale > 0.0 {
        v / scale
    } else {
        v
    }
}
#[cfg(not(target_os = "windows"))]
fn native_to_logical(v: f64, _scale: f64) -> f64 {
    v
}

/// 按平台投递位置。传入的 `(x, y)` 是**原生坐标**，而 `set_position` 在各平台期望的
/// 恰好就是原生坐标 —— Windows 收 `PhysicalPosition`（cast，不乘 scale），
/// macOS / Linux 收 `LogicalPosition`（内部再 `to_logical(scale)` 转成点 / 逻辑值）。
#[cfg(target_os = "windows")]
fn apply_position(window: &tauri::WebviewWindow, x: f64, y: f64) -> tauri::Result<()> {
    window.set_position(tauri::PhysicalPosition::new(
        x.round() as i32,
        y.round() as i32,
    ))
}
#[cfg(not(target_os = "windows"))]
fn apply_position(window: &tauri::WebviewWindow, x: f64, y: f64) -> tauri::Result<()> {
    window.set_position(tauri::LogicalPosition::new(x, y))
}

/// 桌宠拖拽状态（由 lib.rs `app.manage` 注入）
#[derive(Default)]
pub struct PetDragState {
    /// 会话号：`begin_drag` 自增；每个循环线程带自己的号进来，发现不等于当前号就退出
    /// —— 前端漏发 `end_drag`（webview 被刷新／指针事件被系统吃掉）时，下一次 begin 能**接管**，
    /// 不会留下一个永远在搬窗口的孤儿线程。
    generation: AtomicU64,
    /// 当前是否有会话在跑：`end_drag` 置 false，循环据此收尾
    running: AtomicBool,
    /// 抓取偏移 = 光标 − 窗口左上角（**原生坐标**）。前端给的是逻辑像素
    /// （`clientX/clientY`），在 `begin_drag` 里用 `logical_to_native` 转一次。
    grab_native: Mutex<(f64, f64)>,
    /// 窗口 scale factor：begin 时读一次。**循环内禁止再查 getter**（会阻塞等主线程）。
    /// 只在两头用到：begin 换算 grab、end 换算回报值。
    scale: Mutex<f64>,
    /// 循环最后一次应用的窗口左上角（**原生坐标**）；`end_drag` 据此回报，不去查询窗口
    last_target: Mutex<Option<(f64, f64)>>,
    /// 窗口尺寸（逻辑像素，begin 时读一次）：收尾做离屏判定要用
    size_logical: Mutex<(f64, f64)>,
}

/// 目标窗口左上角 = 全局光标 − 抓取偏移（纯函数，便于单测）。两侧同为原生坐标。
fn target_from_cursor(cursor: (f64, f64), grab: (f64, f64)) -> (f64, f64) {
    (cursor.0 - grab.0, cursor.1 - grab.1)
}

/// 这一帧值不值得发：与上一次已发送的目标相比两轴位移都 < `DRAG_EPS` 则不发。
/// 首帧（`last == None`）永远发 —— 否则「按下就松手」会变成窗口纹丝不动。
fn should_send(last: Option<(f64, f64)>, next: (f64, f64)) -> bool {
    match last {
        None => true,
        Some((lx, ly)) => (next.0 - lx).abs() >= DRAG_EPS || (next.1 - ly).abs() >= DRAG_EPS,
    }
}

/// 开始由 Rust 接管的拖拽：`grab_dx/grab_dy` 是「光标相对窗口左上角」的抓取点（**逻辑像素**，
/// 即前端 `clientX/clientY`）。
///
/// 前端职责只剩三件事：按下时算出这个偏移、越过 4px 阈值调本命令、松手调 `end_drag`。
/// 每帧 IPC 从这里开始就不存在了（批次 2.3）。
#[tauri::command]
pub fn begin_drag(
    app: AppHandle,
    state: tauri::State<'_, PetDragState>,
    grab_dx: f64,
    grab_dy: f64,
) -> Result<u64, String> {
    let window = app
        .get_webview_window(PET_WINDOW_LABEL)
        .ok_or_else(|| "pet 窗口尚未创建".to_string())?;

    // begin 是同步命令（macOS 上跑在主线程），这里的 getter 开销可以接受，
    // 但也**只此一次**：循环里再查就会变成每帧阻塞主线程。
    let scale = window.scale_factor().unwrap_or(1.0);
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let size = window
        .outer_size()
        .map(|s| s.to_logical::<f64>(scale))
        .map_err(|e| format!("读取桌宠窗口尺寸失败: {e}"))?;

    if state.running.load(Ordering::SeqCst) {
        tracing::warn!("begin_drag 时上一轮拖拽未收尾，按新会话接管（旧循环会自行退出）");
    }
    // 抓取偏移在这里**一次性**转成原生坐标：前端给逻辑值，循环里的光标是原生坐标。
    // 转换只做这一次，跨 DPI 屏时 live scale 再怎么变，循环内的加减法都不受影响
    // （旧实现在每帧换算，而除用的 scale 与乘用的 live scale 不同源 → 位移被放大）。
    *state.grab_native.lock().unwrap_or_else(|e| e.into_inner()) = (
        logical_to_native(grab_dx, scale),
        logical_to_native(grab_dy, scale),
    );
    *state.scale.lock().unwrap_or_else(|e| e.into_inner()) = scale;
    *state.size_logical.lock().unwrap_or_else(|e| e.into_inner()) = (size.width, size.height);
    *state.last_target.lock().unwrap_or_else(|e| e.into_inner()) = None;
    // 先自增会话号、再置 running：反过来会给旧线程留一个「看到旧号 + 新旗子」的窗口，
    // 它会在收尾时把新会话的 running 抹成 false（拖拽当场失效）。
    let my_gen = state.generation.fetch_add(1, Ordering::SeqCst) + 1;
    state.running.store(true, Ordering::SeqCst);

    let app_for_thread = app.clone();
    std::thread::Builder::new()
        .name(format!("pet-drag-{my_gen}"))
        .spawn(move || drag_loop(app_for_thread, my_gen))
        .map_err(|e| {
            // 起线程失败（线程资源耗尽）：只有「我还是最新会话」时才回撤旗子
            if state.generation.load(Ordering::SeqCst) == my_gen {
                state.running.store(false, Ordering::SeqCst);
            }
            format!("启动桌宠拖拽循环失败: {e}")
        })?;
    Ok(my_gen)
}

/// 结束拖拽：停循环，回报**最终位置（逻辑像素）**给前端写持久化。
///
/// 返回的是循环自己记录的最后一个目标值（经离屏纠偏后），不是查询窗口的结果 ——
/// `set_position` 是异步投递，刚投出去的那几帧可能还没落到 NSWindow 上，
/// 这时去问 `outer_position()` 会拿到历史值，写进持久化就等于「拖完回跳」的老毛病（2.5）。
#[tauri::command]
pub fn end_drag(
    app: AppHandle,
    state: tauri::State<'_, PetDragState>,
) -> Result<(f64, f64), String> {
    state.running.store(false, Ordering::SeqCst);
    let window = app
        .get_webview_window(PET_WINDOW_LABEL)
        .ok_or_else(|| "pet 窗口尚未创建".to_string())?;

    let last = *state.last_target.lock().unwrap_or_else(|e| e.into_inner());
    let scale = {
        let cached = *state.scale.lock().unwrap_or_else(|e| e.into_inner());
        if cached > 0.0 {
            cached
        } else {
            window.scale_factor().unwrap_or(1.0)
        }
    };
    // 循环记的是**原生坐标**；前端持久化的是逻辑像素，这里用 `native_to_logical` 统一换算一次
    // （与 begin 里的 `logical_to_native` 严格互逆）。
    // 一帧都没发过（按下立刻松手 / 光标读取全程失败）→ 窗口没动，问它自己在哪。
    let (mut x, mut y) = match last {
        Some((nx, ny)) => (native_to_logical(nx, scale), native_to_logical(ny, scale)),
        None => {
            // `outer_position()` 返回的是物理像素，先换成逻辑值（与上面同一个口径）
            let p = window
                .outer_position()
                .map_err(|e| format!("读取桌宠窗口位置失败: {e}"))?
                .to_logical::<f64>(scale);
            (p.x, p.y)
        }
    };

    // 收尾纠偏：允许把宠物甩出屏幕边缘，但松手后要回到看得见的位置（否则又是一次「宠物没了」）。
    // 拖拽**过程中**故意不做纠偏 —— 每帧纠偏会让窗口在用户手里弹跳。
    // 枚举显示器失败时整段跳过：宁可不纠偏，也不要凭「猜不到的屏幕布局」把窗口挪走。
    let size = {
        let cached = *state.size_logical.lock().unwrap_or_else(|e| e.into_inner());
        if cached.0 > 0.0 && cached.1 > 0.0 {
            cached
        } else {
            // 没跑过 begin_drag（例如前端只补发了一次 end_drag）→ 现查一次
            window
                .outer_size()
                .map(|s| {
                    let l = s.to_logical::<f64>(scale);
                    (l.width, l.height)
                })
                .unwrap_or((0.0, 0.0))
        }
    };
    if size.0 <= 0.0 || size.1 <= 0.0 {
        // 连尺寸都拿不到就别做判定：1×1 的假矩形会把「实际在屏外」误判成可见
        tracing::warn!("拖拽收尾跳过离屏纠偏：拿不到桌宠窗口尺寸");
        return Ok((x.round(), y.round()));
    }
    if let Ok(monitors) = window_bounds::monitor_rects(&window) {
        if let Ok(primary) = window_bounds::primary_rect(&window, &monitors) {
            let rect = window_bounds::rect_from_logical(x, y, size.0, size.1, scale);
            if let Some((rx, ry)) = window_bounds::offscreen_target(
                rect,
                &monitors,
                primary,
                FallbackCorner::BottomRight,
                DRAG_MARGIN,
            ) {
                // 回落到物理坐标 → 换算回逻辑像素回报前端（前端存的就是逻辑像素）
                x = f64::from(rx) / scale;
                y = f64::from(ry) / scale;
                tracing::info!(
                    physical_x = rx,
                    physical_y = ry,
                    "拖拽结束时桌宠落在所有显示器之外，已贴回主屏右下角"
                );
                let _ = window.set_position(tauri::PhysicalPosition::new(rx, ry));
            }
        }
    }
    Ok((x.round(), y.round()))
}

/// 拖拽循环主体（独立线程）：读全局光标 → 算位置 → 投给主线程，直到 `end_drag` 或被打断。
fn drag_loop(app: AppHandle, my_gen: u64) {
    let mut last: Option<(f64, f64)> = None;
    // `State` 的引用在整个会话里是同一个，循环外取一次就够（旧实现每帧 125Hz 重取）。
    let Some(state) = app.try_state::<PetDragState>() else {
        return; // app 已销毁
    };
    loop {
        std::thread::sleep(Duration::from_millis(DRAG_TICK_MS));
        // 被新会话接管 → 退出，不再搬窗口
        if state.generation.load(Ordering::SeqCst) != my_gen {
            break;
        }
        if !state.running.load(Ordering::Relaxed) {
            break;
        }
        let grab = *state.grab_native.lock().unwrap_or_else(|e| e.into_inner());
        // 原生坐标直取，不做任何 scale 换算（见模块头「坐标空间」）
        let cursor = match cursor_native() {
            Some(c) => c,
            // 光标读不到（锁屏 / 远程桌面刚断开）：这一帧跳过，等下一次
            None => continue,
        };
        let next = target_from_cursor(cursor, grab);
        if !should_send(last, next) {
            continue;
        }
        let window = match app.get_webview_window(PET_WINDOW_LABEL) {
            Some(w) => w,
            None => break, // 窗口没了，没什么可搬的
        };
        // 非主线程调用安全（见模块头「线程约束」）：内部是 proxy 投递，不碰 AppKit。
        // `apply_position` 按平台选 Physical/Logical，使投递值恰好等于本平台原生坐标 ——
        // 跨 DPI 屏时不会被再缩放一次。
        if apply_position(&window, next.0, next.1).is_err() {
            tracing::warn!("桌宠拖拽循环投递位置失败，提前结束本轮");
            break;
        }
        last = Some(next);
        *state.last_target.lock().unwrap_or_else(|e| e.into_inner()) = Some(next);
    }
    // 退出时**不去动 `running`**：那个旗子的唯一归属是命令侧（begin 置真 / end 置假）。
    // 线程一写就会和「上一个 begin 刚接管」形成 check-then-act 竞态，把新会话的旗子抹掉。
    tracing::debug!(generation = my_gen, "桌宠拖拽循环退出");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 纯函数常量口径：eps 与 tick 都得是正数，否则循环会退化成「每帧都发」或死循环。
    /// 用 `const` 断言（而不是 `assert!`）让它在**编译期**失败，同时满足 clippy
    /// `assertions_on_constants`（测试函数里对常量 assert 会报这条）。
    const _: () = assert!(DRAG_TICK_MS > 0, "0 间隔会让循环把 CPU 打满");
    const _: () = assert!(
        DRAG_EPS > 0.0 && DRAG_EPS < 16.0,
        "阈值过大拖起来发顿，过小则滤不掉无效帧"
    );

    /// 抓取偏移的直接含义：光标在窗口正中按下 → 落点 = 光标 − 半窗，宠物不会「跳到手心」
    #[test]
    fn target_subtracts_grab_offset() {
        assert_eq!(
            target_from_cursor((500.0, 300.0), (70.0, 165.0)),
            (430.0, 135.0)
        );
        // 偏移为 0（按在左上角）时窗口左上角应与光标重合
        assert_eq!(
            target_from_cursor((500.0, 300.0), (0.0, 0.0)),
            (500.0, 300.0)
        );
    }

    /// 🐞 回归（P1，批次 2.4）：节流必须「两轴都小才滤」，只查一轴会让斜向慢拖吃帧。
    #[test]
    fn throttle_needs_both_axes_below_eps() {
        assert!(
            should_send(None, (0.0, 0.0)),
            "首帧必发，否则按下即松手会纹丝不动"
        );
        assert!(!should_send(Some((100.0, 100.0)), (103.9, 103.9)));
        assert!(
            should_send(Some((100.0, 100.0)), (104.0, 0.0)),
            "单轴越线也要发"
        );
        assert!(should_send(Some((100.0, 100.0)), (0.0, 104.0)));
        // 反向拖动同样要过阈值（用 abs，不能只判正位移）
        assert!(!should_send(Some((100.0, 100.0)), (97.0, 97.0)));
        assert!(should_send(Some((100.0, 100.0)), (95.0, 95.0)));
    }

    /// 🐞 回归（审计 P1-1）：逻辑 ↔ 原生 的换算必须**严格互逆**。
    ///
    /// 旧实现的病灶是「除时用 `begin_drag` 缓存的 scale、乘时用 `set_position` 那一刻的
    /// live scale」，两者在跨 DPI 屏时分叉 → 位移被放大 1.0/1.5 倍。现在换算只发生在
    /// 两端（begin 转 grab、end 转回报），循环内的加减法完全不带 scale，
    /// 所以这里对拍的就是「同一个 scale 下往返无损」。
    #[test]
    fn logical_native_round_trip_is_lossless() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for v in [0.0, 70.0, 140.0, -695.0, 2560.0] {
                let native = logical_to_native(v, scale);
                let back = native_to_logical(native, scale);
                assert!(
                    (back - v).abs() < 1e-9,
                    "scale={scale} v={v} 往返后变成 {back}"
                );
            }
        }
        // 非法 scale 不能把坐标算成 NaN / 0 —— `native_to_logical` 要退化到恒等
        assert_eq!(native_to_logical(123.0, 0.0), 123.0);
    }
}
