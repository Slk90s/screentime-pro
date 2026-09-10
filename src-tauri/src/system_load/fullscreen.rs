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
//! - macOS / Linux：v1 恒返回 false（浮窗不做全屏隐藏）。macOS 菜单栏本就有完整
//!   文字，浮窗主要面向 Windows/Linux；后续可按需补 CGDisplay / EWMH 检测。
//!
//! FFI 风格与 network.rs 一致：裸 `extern "system"` 声明，不引入 windows crate。
//!
//! 修改历史：
//!   - 2026-09-10 @Unreleased: 初始创建 - Windows 实装，其余平台 stub。

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

#[cfg(not(target_os = "windows"))]
pub fn foreground_is_fullscreen() -> bool {
    false
}
