//!
//! pet/hit_mask.rs
//! 桌宠「按身体形状命中」的鼠标穿透（v0.8.0，原 Phase 4 提前落地）。
//!
//! 背景（问题坐实）：
//! - 修前 `pet/window.rs` 创建时固定 `set_ignore_cursor_events(false)`，即**整窗永久接收鼠标**。
//!   3D 熊猫皮肤窗口是 150×330 的竖矩形，其中大部分是透明区，双击/点击会全部被这块
//!   「隐形挡块」吃掉 → 形成明显点击死区（用户反馈的首要体验问题）。
//!
//! 方案（为什么不用「前端 mousemove 切穿透」）：
//! - ❌ 朴素做法不可行：一旦 `set_ignore_cursor_events(true)`，本窗就**收不到 mousemove**，
//!   前端再也没机会切回来，桌宠会「永久哑掉」。
//! - ✅ 正解：前端把渲染结果压成一张**粗粒度 alpha 网格掩码**交给 Rust；Rust 侧用后台线程
//!   轮询**全局光标位置**（Win GetCursorPos / macOS CGEventGetLocation / Linux query_pointer），
//!   换算到窗口本地坐标后查网格，只在「命中态翻转」时调 `set_ignore_cursor_events`。
//!   这样穿透判定不依赖本窗是否收到事件，天然规避上一条死锁。
//!
//! 其它约束：
//! - **拖拽期间必须停更**（前端 pointerdown 时 `set_pet_drag_lock(true)`）：否则光标拖出身体
//!   边缘时线程会把窗切成穿透，拖拽会中途断掉。
//! - 未上报掩码时保持旧行为（可交互），保证老皮肤/异常路径不退化。
//! - Linux 光标坐标取自 X11 `query_pointer`，逻辑与前两端一致，属「可微调」范围（未实机验证）。
//!
//! 修改历史：
//!   - 2026-09-16 @v0.8.0: 初始创建 - alpha 网格掩码 + 全局光标轮询穿透
//!

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// 轮询间隔：55ms ≈ 18Hz。够跟手，又不至于把 CPU 拉起来（单次只是查内存里的网格）。
const POLL_MS: u64 = 55;

/// 前端上报的 alpha 网格掩码（1=身体/不透明，0=透明可穿透）
pub struct HitMask {
    pub cols: u32,
    pub rows: u32,
    /// 长度 = cols*rows，行优先
    pub cells: Vec<u8>,
    /// 网格中是否存在至少一个「不透明」格。全 0（如整窗透明）时视为无效掩码，
    /// 轮询线程会跳过切换、保持旧「整窗可交互」行为，避免异常路径把桌宠切到全穿透而不可点。
    pub any_hit: bool,
}

/// 桌宠命中状态（由 lib.rs `app.manage` 注入）
pub struct PetHitState {
    pub mask: Mutex<Option<HitMask>>,
    /// 拖拽锁：前端拖拽期间置 true，线程跳过切换
    pub dragging: AtomicBool,
    /// 当前**已生效**的穿透态（用于去重，避免每帧重复调系统 API）
    pub applied_passthrough: AtomicBool,
    /// 是否已启动轮询线程（幂等）
    pub started: AtomicBool,
}

impl Default for PetHitState {
    fn default() -> Self {
        Self {
            mask: Mutex::new(None),
            dragging: AtomicBool::new(false),
            applied_passthrough: AtomicBool::new(false),
            started: AtomicBool::new(false),
        }
    }
}

/// 启动穿透轮询线程（幂等；在 setup 里调一次即可）
pub fn spawn_watcher(app: AppHandle) {
    let state = match app.try_state::<PetHitState>() {
        Some(s) => s,
        None => return,
    };
    if state.started.swap(true, Ordering::SeqCst) {
        return; // 已启动
    }
    std::thread::Builder::new()
        .name("pet-hit-mask".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_millis(POLL_MS));
            let state = match app.try_state::<PetHitState>() {
                Some(s) => s,
                None => continue,
            };
            if state.dragging.load(Ordering::Relaxed) {
                continue;
            }
            // 取掩码（无掩码 / 空掩码 = 保持旧行为，不说话）
            {
                let guard = state.mask.lock().unwrap_or_else(|e| e.into_inner());
                match guard.as_ref() {
                    Some(m) if m.any_hit && m.cols > 0 && m.rows > 0 => {}
                    _ => continue,
                }
            }

            let window = match app.get_webview_window(crate::pet::PET_WINDOW_LABEL) {
                Some(w) => w,
                None => continue,
            };
            if !window.is_visible().unwrap_or(false) {
                continue;
            }
            let scale = window.scale_factor().unwrap_or(1.0);
            let pos = match window.outer_position() {
                Ok(p) => p.to_logical::<f64>(scale),
                Err(_) => continue,
            };
            let size = match window.inner_size() {
                Ok(s) => s.to_logical::<f64>(scale),
                Err(_) => continue,
            };
            let (cx, cy) = match cursor_logical(scale) {
                Some(v) => v,
                None => continue,
            };
            let lx = cx - pos.x;
            let ly = cy - pos.y;

            // 视口外 → 视为未命中（可穿透）
            let inside_window = lx >= 0.0 && ly >= 0.0 && lx < size.width && ly < size.height;
            let mut on_body = false;
            if inside_window {
                let guard = state.mask.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(m) = guard.as_ref() {
                    let col = ((lx / size.width) * m.cols as f64) as i64;
                    let row = ((ly / size.height) * m.rows as f64) as i64;
                    let col = col.clamp(0, m.cols as i64 - 1) as usize;
                    let row = row.clamp(0, m.rows as i64 - 1) as usize;
                    let idx = row * m.cols as usize + col;
                    on_body = m.cells.get(idx).copied().unwrap_or(0) == 1;
                }
            }

            let want_passthrough = !on_body;
            if state.applied_passthrough.load(Ordering::Relaxed) != want_passthrough
                && window.set_ignore_cursor_events(want_passthrough).is_ok()
            {
                state
                    .applied_passthrough
                    .store(want_passthrough, Ordering::Relaxed);
            }
        })
        .ok();
}

/// 前端上报 alpha 网格掩码（按皮肤渲染结果压缩；窗口尺寸变化/换肤后需重报）
#[tauri::command]
pub fn set_pet_hit_mask(
    state: tauri::State<'_, PetHitState>,
    cols: u32,
    rows: u32,
    cells: Vec<u8>,
) -> Result<bool, String> {
    if cols == 0 || rows == 0 || cells.len() != (cols * rows) as usize {
        return Err(format!(
            "掩码尺寸不合法: cols={cols} rows={rows} len={}",
            cells.len()
        ));
    }
    let any_hit = cells.iter().any(|&c| c != 0);
    *state.mask.lock().unwrap_or_else(|e| e.into_inner()) = Some(HitMask {
        cols,
        rows,
        cells,
        any_hit,
    });
    Ok(true)
}

/// 拖拽锁：前端 pointerdown → true（暂停穿透轮询），pointerup/cancel → false
#[tauri::command]
pub fn set_pet_drag_lock(
    state: tauri::State<'_, PetHitState>,
    locked: bool,
) -> Result<bool, String> {
    state.dragging.store(locked, Ordering::Relaxed);
    Ok(true)
}

/// 清空掩码（桌宠关闭时调用，回到旧「整窗可交互」行为）
#[tauri::command]
pub fn clear_pet_hit_mask(state: tauri::State<'_, PetHitState>) -> Result<bool, String> {
    *state.mask.lock().unwrap_or_else(|e| e.into_inner()) = None;
    Ok(true)
}

// ===== 平台光标位置（统一返回「逻辑坐标」，与 Tauri 的 LogicalPosition 同空间）=====
//
// v1.2.12：这里不再只服务穿透轮询 —— 桌宠拖拽循环（`pet/drag.rs` 的 `begin_drag`）
// 也复用同一组函数，所以四个实现都要 `pub(crate)`。三个平台里**只有 Linux 需要缓存连接**
// （Win / macOS 的查询本身就是无状态的系统调用）。
//
// v1.2.12 收尾修订（审计 P1-1）拆成两层：
// - `cursor_native()`：**平台原生坐标**，不做任何换算。Windows = 物理像素；
//   macOS = 点（points）；Linux = X11 root 像素。拖拽循环用这一层。
// - `cursor_logical(scale)`：统一换算到 Tauri 逻辑坐标，供**穿透轮询**用（窗口本地命中判定）。
//   只有 Windows 需要除 scale，mac/Linux 原生坐标本就与逻辑坐标同空间。
//
// 为什么拖拽必须用原生坐标而不是「一律换物理」：`set_position` 收到 `PhysicalPosition` 后
// 各平台的处理并不一致 —— Windows 直接 `cast()` 不乘 scale（`tao/.../windows/window.rs:230`），
// 而 macOS 与 Linux 先 `to_logical(scale_factor())` 除一次（`macos/window.rs:729-730`、
// `linux/window.rs:458-461`）再交给平台。所以「物理」在 macOS 上要额外乘 scale 才是点的对应物，
// 一刀切地全用物理值反而会算错。用原生坐标则两端都无须换算，跨 DPI 屏天然正确。

#[cfg(target_os = "windows")]
pub(crate) fn cursor_native() -> Option<(f64, f64)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = POINT::default();
    // GetCursorPos 返回**物理像素**（本进程 per-monitor DPI aware）
    unsafe {
        GetCursorPos(&mut p).ok()?;
    }
    Some((f64::from(p.x), f64::from(p.y)))
}

#[cfg(target_os = "macos")]
pub(crate) fn cursor_native() -> Option<(f64, f64)> {
    #[repr(C)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    extern "C" {
        fn CGEventCreate(source: *const std::ffi::c_void) -> *mut std::ffi::c_void;
        fn CGEventGetLocation(event: *mut std::ffi::c_void) -> CGPoint;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: *const std::ffi::c_void);
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {}

    unsafe {
        let ev = CGEventCreate(std::ptr::null());
        if ev.is_null() {
            return None;
        }
        let p = CGEventGetLocation(ev);
        CFRelease(ev as *const std::ffi::c_void);
        // CoreGraphics 的全局显示坐标本来就是**点（points）**且原点在主屏左上
        Some((p.x, p.y))
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn cursor_native() -> Option<(f64, f64)> {
    // X11：root 窗口下的指针坐标即全屏像素坐标
    use std::cell::RefCell;
    use std::sync::Arc;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::ConnectionExt;
    use x11rb::rust_connection::RustConnection;

    // v1.2.12（整改 1.4）**连接缓存**：本函数被穿透轮询以 ~18Hz 调用，批次 2 的拖拽
    // 循环还会再加一条以 125Hz 调用的线程。旧写法每次都 `x11rb::connect(None)` ——
    // 开一条新 X socket、读一遍 setup、用完即弃，几百次/秒的建连既烧 CPU，也会在
    // X server（尤其 XWayland）侧留下大量瞬时连接。
    //
    // 形态选择（都经 `.tmp-probe-linux-cursor` 交叉编译探针实证，见该目录 Cargo.toml）：
    // - `RustConnection` 内部是「一条 socket + 若干 Mutex」（x11rb-0.13.2 的
    //   rust_connection/mod.rs:65），**没有实现 `Clone`** —— 所以不能把连接本体存进缓存
    //   再 `clone()` 出来用（首轮探针即 E0599）。改成缓存 `Arc<RustConnection>`，
    //   取用时克隆的是引用计数。
    // - 缓存用 `thread_local!` 而不是全局 `static`：`Connection` 的方法只吃 `&self`，
    //   跨线程共享本身安全，但「轮询线程一条、拖拽线程一条」互不阻塞，不必为了一次
    //   `query_pointer` 去抢全局锁；且 TLS 不要求 `Send`，少一层约束。
    // - `RefCell` 的借用只覆盖「取出 Arc」这一瞬，X 往返（可能阻塞）期间不持有借用，
    //   避免同线程重入时 borrow 冲突。
    thread_local! {
        static CACHED: RefCell<Option<(Arc<RustConnection>, usize)>> = RefCell::new(None);
    }

    let cached = CACHED.with(|slot| slot.borrow().clone());
    let (conn, screen_num) = match cached {
        Some(pair) => pair,
        None => {
            let (conn, num) = x11rb::connect(None).ok()?;
            let pair = (Arc::new(conn), num);
            CACHED.with(|slot| *slot.borrow_mut() = Some(pair.clone()));
            pair
        }
    };

    let read = || -> Option<(f64, f64)> {
        let root = conn.setup().roots.get(screen_num)?.root;
        let reply = conn.query_pointer(root).ok()?.reply().ok()?;
        Some((f64::from(reply.root_x), f64::from(reply.root_y)))
    };

    match read() {
        Some(v) => Some(v),
        None => {
            // 连接失效（X server 重启、显示切换、seat 换了）→ 丢掉缓存，下一轮重连。
            // 不这么做的话缓存里那条死连接会让桌宠从此不再响应穿透，且永不自愈。
            // 连不上 server 时每次调用仍会重试一次 connect（与旧写法同量级开销），
            // 换掉它需要引入「重连退避」计时器，收益不抵复杂度。
            CACHED.with(|slot| *slot.borrow_mut() = None);
            None
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub(crate) fn cursor_native() -> Option<(f64, f64)> {
    None
}

/// 统一换算到 Tauri **逻辑坐标**（穿透轮询用）。
///
/// 只有 Windows 的原生坐标（物理像素）需要除 scale；macOS 的 points 与 Linux 的
/// root 像素本就与逻辑坐标同空间，原样透传（`scale` 在这两端是未使用参数）。
pub(crate) fn cursor_logical(scale: f64) -> Option<(f64, f64)> {
    let (x, y) = cursor_native()?;
    #[cfg(target_os = "windows")]
    {
        let s = if scale > 0.0 { scale } else { 1.0 };
        Some((x / s, y / s))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = scale;
        Some((x, y))
    }
}
