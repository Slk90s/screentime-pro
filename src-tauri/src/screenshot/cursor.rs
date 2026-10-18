//!
//! screenshot/cursor.rs
//! 全局光标坐标（v1.2.0，2026-10-17）：多屏截图「鼠标在哪截哪屏」的坐标源。
//!
//! ## 为什么独立成模块
//! v1.2.0 引入「鼠标所在屏截图」（微信式：Alt+A 截鼠标停靠的那块屏）。判断
//! 「光标落在哪块屏」需要全局光标坐标，而 `pet/hit_mask.rs` 里已有一份平台
//! 专属实现 —— 但那套的返回口径是**统一换算到逻辑坐标**（供窗口本地命中用），
//! 与本模块需要的**平台原生坐标系**不同：
//!   - `Monitor::x()/y()` / `Monitor::from_point()` 在三平台各自用**原生**坐标：
//!     Windows `GetCursorPos`/`dmPosition` = 物理像素；macOS `CGEventGetLocation`/
//!     `CGDisplayBounds` = 点（points）；Linux X11 root 窗口 = 逻辑像素。
//!   - `hit_mask.rs` 的 `cursor_logical` 为了「除以 scale 对齐 Tauri 逻辑坐标」，
//!     Windows 分支会被多除一次 scale，与 `Monitor` 的物理像素口径对不上。
//! 这里新开一份**不除 scale、原样返回平台原生坐标**的实现，专供「点→屏」判定；
//! `pet/hit_mask.rs` 那份保持不变（职责不同，别合并）。
//!
//! ## 坐标系约定（关键）
//! 本模块返回的值必须与 `xcap::Monitor` / screencapturekit `SCDisplay` 的几何
//! **同空间**，才能用「点在矩形内」正确判定：
//!   - Windows：`GetCursorPos` 返回**物理像素**；`Monitor::x/y/width/height` 来自
//!     `dmPosition`/`dmPelsWidth` = **物理像素**。同空间 ✓（无需换算）。
//!   - macOS：`CGEventGetLocation` 返回**点**；`SCDisplay::frame()` 是 `CGRect` =
//!     **点**。同空间 ✓（无需换算）。
//!   - Linux：X11 `query_pointer` 的 `root_x/root_y` 是**根窗口像素**（= 逻辑像素）；
//!     `xcap::Monitor::from_point` 内部 `x * scale_factor` 再与 RandR 输出（像素）
//!     比对，说明它期望**逻辑像素**入参。同空间 ✓（无需换算）。
//! 三平台都是「原样透传」即可，这正是本模块与 `hit_mask.rs` 的本质区别。
//!
//! 修改历史：
//!   - 2026-10-17 @v1.2.0: 初始创建 —— 平台原生坐标的全局光标位置。
//!

/// 全局光标位置（**平台原生坐标**，见模块头「坐标系约定」）。
///
/// 返回 `None` 表示取不到（平台 API 失败 / 未实现），调用方自行回落「主屏」。
pub fn cursor_native() -> Option<(f64, f64)> {
    cursor_native_inner()
}

#[cfg(target_os = "windows")]
fn cursor_native_inner() -> Option<(f64, f64)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = POINT::default();
    // GetCursorPos 返回物理像素（本进程 per-monitor DPI aware），
    // 与 Monitor 的 dmPosition/dmPelsWidth 同空间，原样返回。
    unsafe {
        GetCursorPos(&mut p).ok()?;
    }
    Some((f64::from(p.x), f64::from(p.y)))
}

#[cfg(target_os = "macos")]
fn cursor_native_inner() -> Option<(f64, f64)> {
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
        // CoreGraphics 全局显示坐标是点（points），与 SCDisplay::frame()（CGRect）同空间。
        Some((p.x, p.y))
    }
}

#[cfg(target_os = "linux")]
fn cursor_native_inner() -> Option<(f64, f64)> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::ConnectionExt;
    let (conn, screen_num) = x11rb::connect(None).ok()?;
    let root = conn.setup().roots.get(screen_num)?.root;
    let reply = conn.query_pointer(root).ok()?.reply().ok()?;
    // root_x/root_y 是根窗口逻辑像素，与 xcap::Monitor::from_point 期望同空间。
    Some((f64::from(reply.root_x), f64::from(reply.root_y)))
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn cursor_native_inner() -> Option<(f64, f64)> {
    None
}
