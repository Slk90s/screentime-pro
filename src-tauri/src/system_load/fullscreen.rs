//!
//! system_load/fullscreen.rs
//! 前台窗口全屏检测（v0.7.6 悬浮指标条「全屏自动隐藏」用，2026-09-10）。
//!
//! 设计：
//! - Windows：GetForegroundWindow 的 GetWindowRect 与其所在显示器（MonitorFromWindow
//!   + GetMonitorInfoW 的 rcMonitor）逐点比较，相等即视为全屏（无边框全屏/独占全屏
//!   游戏均命中）。
//! - 排除桌面 shell（Progman / WorkerW / Shell_TrayWnd）：点击桌面时前台窗口矩形
//!   恰好等于整屏，会造成「没全屏却隐藏浮窗」的假阳性。
//! - macOS：CGWindowListCopyWindowInfo 取**最前台普通窗口**（kCGWindowLayer == 0）的
//!   kCGWindowBounds，与 CGDisplayBounds(主显示器) 逐值比较（容差 2pt）。关键判别点：
//!   普通「最大化」窗口不会盖住菜单栏（bounds.y 从菜单栏下方开始），只有真正的全屏窗口
//!   （绿色按钮全屏 / 视频全屏 / 游戏独占）其 bounds 才等于整屏 → 天然区分，无需额外判据。
//!   v0.8.0（2026-09-16）实装。旧取舍「macOS 恒 false、不做全屏隐藏」作废。
//! - Linux：v1 恒返回 false（EWMH 需逐 WM 适配），后续可按需补。
//!
//! FFI 风格与 network.rs 一致：裸 `extern "system"` 声明，不引入 windows crate。
//!
//! 修改历史：
//!   - 2026-09-10 @Unreleased: 初始创建 - Windows 实装，其余平台 stub。
//!   - 2026-09-16 @v0.8.0: macOS 实装（前台普通窗口 bounds vs 主显示器 bounds）。

#[cfg(target_os = "windows")]
mod windows_impl {
    use std::mem::size_of;

    #[repr(C)]
    #[derive(Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[repr(C)]
    struct MonitorInfo {
        cb_size: u32,
        rc_monitor: Rect,
        rc_work: Rect,
        dw_flags: u32,
    }

    extern "system" {
        fn GetForegroundWindow() -> isize;
        fn GetWindowRect(hwnd: isize, rect: *mut Rect) -> i32;
        fn MonitorFromWindow(hwnd: isize, dw_flags: u32) -> isize;
        fn GetMonitorInfoW(hmon: isize, info: *mut MonitorInfo) -> i32;
        fn GetClassNameW(hwnd: isize, buf: *mut u16, max_count: i32) -> i32;
    }

    const MONITOR_DEFAULTTONEAREST: u32 = 2;

    pub(super) fn detect() -> bool {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd == 0 {
                return false;
            }
            // 排除 shell 桌面窗口（点桌面时前台矩形==整屏的假阳性）
            let mut buf = [0u16; 32];
            let n = GetClassNameW(hwnd, buf.as_mut_ptr(), 32);
            if n > 0 {
                let name = String::from_utf16_lossy(&buf[..(n as usize).min(31)]);
                let name = name.trim_end_matches('\0');
                if matches!(name, "Progman" | "WorkerW" | "Shell_TrayWnd") {
                    return false;
                }
            }
            let mut rect = Rect::default();
            if GetWindowRect(hwnd, &mut rect) == 0 {
                return false;
            }
            let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if hmon == 0 {
                return false;
            }
            let mut info = MonitorInfo {
                cb_size: size_of::<MonitorInfo>() as u32,
                rc_monitor: Rect::default(),
                rc_work: Rect::default(),
                dw_flags: 0,
            };
            if GetMonitorInfoW(hmon, &mut info) == 0 {
                return false;
            }
            let m = &info.rc_monitor;
            rect.left == m.left
                && rect.top == m.top
                && rect.right == m.right
                && rect.bottom == m.bottom
        }
    }
}

#[cfg(target_os = "windows")]
pub fn foreground_is_fullscreen() -> bool {
    windows_impl::detect()
}

// ===== macOS 实装（v0.8.0）=====
#[cfg(target_os = "macos")]
mod macos_impl {
    use core_foundation::base::TCFType;
    use core_foundation::string::CFString;
    use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
    use core_foundation_sys::base::CFTypeRef;
    use core_foundation_sys::dictionary::{CFDictionaryGetValue, CFDictionaryRef};
    use core_foundation_sys::number::{CFNumberGetValue, CFNumberRef, kCFNumberFloat64Type};
    use core_graphics::window::{
        CGWindowListCopyWindowInfo, CGWindowListOption, kCGNullWindowID, kCGWindowBounds,
        kCGWindowLayer, kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly,
    };
    use std::os::raw::c_void;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGSize {
        width: f64,
        height: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGMainDisplayID() -> u32;
        fn CGDisplayBounds(display: u32) -> CGRect;
    }

    /// 从字典里按「原生 CFString 键指针」取 f64（CFNumber 自动转 double）
    fn num_at(dict: CFDictionaryRef, key: *const c_void) -> Option<f64> {
        if dict.is_null() {
            return None;
        }
        unsafe {
            let v: CFTypeRef = CFDictionaryGetValue(dict, key);
            if v.is_null() {
                return None;
            }
            let mut out: f64 = 0.0;
            let ok = CFNumberGetValue(
                v as CFNumberRef,
                kCFNumberFloat64Type,
                &mut out as *mut f64 as *mut c_void,
            );
            if ok {
                Some(out)
            } else {
                None
            }
        }
    }

    /// 从字典里按「字符串键」取 f64（kCGWindowBounds 的内层 X/Y/Width/Height）
    fn num_by_name(dict: CFDictionaryRef, name: &str) -> Option<f64> {
        let k = CFString::new(name);
        num_at(dict, k.as_CFTypeRef())
    }

    pub(super) fn detect() -> bool {
        unsafe {
            let option: CGWindowListOption =
                kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements;
            // 该 C 函数返回裸指针（非 Option），需手动判空
            let list: CFArrayRef = CGWindowListCopyWindowInfo(option, kCGNullWindowID);
            if list.is_null() {
                return false;
            }
            let display = CGDisplayBounds(CGMainDisplayID());
            let count = CFArrayGetCount(list);
            // 列表按「前 → 后」排序，第一个命中的即最前台全屏窗
            for i in 0..count {
                let d = CFArrayGetValueAtIndex(list, i) as CFDictionaryRef;
                if d.is_null() {
                    continue;
                }
                // 只看普通窗口层（0）：菜单栏/浮动面板/桌面背景层都会 > 0，跳过
                let layer = num_at(d, kCGWindowLayer as *const c_void).unwrap_or(-1.0);
                if layer != 0.0 {
                    continue;
                }
                let bounds_ref: CFTypeRef =
                    CFDictionaryGetValue(d, kCGWindowBounds as *const c_void);
                if bounds_ref.is_null() {
                    continue;
                }
                let bd = bounds_ref as CFDictionaryRef;
                let (x, y) = (
                    match num_by_name(bd, "X") {
                        Some(v) => v,
                        None => continue,
                    },
                    match num_by_name(bd, "Y") {
                        Some(v) => v,
                        None => continue,
                    },
                );
                let (w, h) = (
                    match num_by_name(bd, "Width") {
                        Some(v) => v,
                        None => continue,
                    },
                    match num_by_name(bd, "Height") {
                        Some(v) => v,
                        None => continue,
                    },
                );
                const TOL: f64 = 2.0;
                if (x - display.origin.x).abs() <= TOL
                    && (y - display.origin.y).abs() <= TOL
                    && (w - display.size.width).abs() <= TOL
                    && (h - display.size.height).abs() <= TOL
                {
                    return true;
                }
            }
            false
        }
    }
}

#[cfg(target_os = "macos")]
pub fn foreground_is_fullscreen() -> bool {
    macos_impl::detect()
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn foreground_is_fullscreen() -> bool {
    false
}
