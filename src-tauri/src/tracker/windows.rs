//! Windows 平台采集器
//!
//! 通过 Win32 API 获取前台窗口与进程信息：
//! - `GetForegroundWindow` 取前台窗口句柄 → PID
//! - `GetModuleFileNameExW` / `QueryFullProcessImageNameW` 取可执行文件路径
//! - 空闲检测用 `GetLastInputInfo`
//!
//! 注：本文件仅在 `target_os = "windows"` 下参与编译。
//! 对 GetModuleFileNameExW / QueryFullProcessImageNameW 使用 raw FFI 声明，
//! 避免 windows crate 0.58 的 Param trait 严格类型约束导致交叉编译失败。

use crate::error::TrackerError;
use crate::tracker::platform::{PlatformTracker, RawApp};
use std::path::Path;
use std::os::raw::{c_uint, c_void};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

/// Windows 采集器
pub struct WindowsTracker;

impl PlatformTracker for WindowsTracker {
    fn get_foreground_app(&self) -> Result<RawApp, TrackerError> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_invalid() {
                return Err(TrackerError::NoForeground);
            }
            let mut pid: u32 = 0;
            let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));

            // 前台窗口标题（用于「按窗口标题分类」）
            let mut title_buf = [0u16; 1024];
            let len = GetWindowTextW(hwnd, &mut title_buf);
            let window_title = if len > 0 {
                Some(String::from_utf16_lossy(&title_buf[..len as usize]))
            } else {
                None
            };

            // 获取 exe 全路径 + 进程名
            let (exe_path, process_name) = get_process_path(pid);

            // 展示名用进程名（v0.9.7 修复：原实现 name=window_title，而 UI 渲染
            // 「name · window_title」，窗口有标题时两者必然相同 → 「ScreenTime Pro ·
            // ScreenTime Pro」式重复。窗口标题的匹配职责已由 window_title 字段承担，
            // name 只需是稳定的「应用名」；取 exe 文件名并去掉 .exe 后缀更友好，
            // 拿不到 exe 路径时回退 get_process_path 给出的 pid-N）。
            let name = process_name
                .trim_end_matches(".exe")
                .trim_end_matches(".EXE")
                .to_string();

            Ok(RawApp {
                name,
                process_name,
                exe_path, // 可能是 None（系统进程拒绝查询时）
                bundle_id: None,
                window_title,
            })
        }
    }

    fn get_idle_seconds(&self) -> Result<u64, TrackerError> {
        unsafe {
            let mut lii = LASTINPUTINFO {
                cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
                dwTime: 0,
            };
            let _ = GetLastInputInfo(&mut lii);
            let now = windows::Win32::System::SystemInformation::GetTickCount();
            Ok(((now as u64).saturating_sub(lii.dwTime as u64)) / 1000)
        }
    }
}

// ===================== raw FFI 声明（绕过 windows crate Param trait）=====================

// 注意：函数名必须与 Windows 导入库导出的符号一致（x64 下无 @N 装饰），
// 否则链接器报 undefined reference。原始名即 `GetModuleFileNameExW` /
// `QueryFullProcessImageNameW`，链接器会从 psapi / kernel32 导入库解析。
#[link(name = "psapi")]
extern "system" {
    /// 获取进程模块文件全路径（PSAPI 函数）
    fn GetModuleFileNameExW(
        hprocess: *mut c_void,
        hmodule: *mut c_void,
        lpfilename: *mut u16,
        nsize: c_uint,
    ) -> c_uint;
}

#[link(name = "kernel32")]
extern "system" {
    /// 获取进程镜像路径完整名称（支持受限权限查询）
    fn QueryFullProcessImageNameW(
        hprocess: *mut c_void,
        flags: u32,
        lpexename: *mut u16,
        lpdwsize: *mut u32,
    ) -> i32; // BOOL
}

/// 通过 OpenProcess + PSAPI/Kernel32 获取进程路径
/// - 返回 (Option<path>, process_name)：当两条策略都失败时，path 为 None（避免污染
///   分类规则的「pid-N」假路径入库）；process_name 始终有值（fallback 用 pid-N 形式）
unsafe fn get_process_path(pid: u32) -> (Option<String>, String) {
    // 策略1：标准权限 + PSAPI GetModuleFileNameExW
    // v0.8.0（2026-09-16）：windows crate 0.58 → 0.62 后 OpenProcess 的 bInheritHandle
    // 形参类型由 windows_core::BOOL 改为原生 bool，故 `FALSE` 常量改传 `false`。
    if let Ok(handle) = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid) {
        let mut buf = [0u16; 1024];
        let hproc = handle.0 as *mut c_void; // HANDLE → 原始指针
        let n = GetModuleFileNameExW(hproc, std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32);
        if n > 0 {
            let path = String::from_utf16_lossy(&buf[..n as usize]);
            let name = Path::new(&path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| format!("pid-{}", pid));
            return (Some(path), name);
        }
    }

    // 策略2：受限权限 + Kernel32 QueryFullProcessImageNameW
    // v0.8.0：同上，bInheritHandle 传 bool
    if let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
        let mut buf = [0u16; 1024];
        let mut size: u32 = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle.0 as *mut c_void,
            0, // PROCESS_NAME_WIN32
            buf.as_mut_ptr(),
            &mut size,
        );
        if ok != 0 && size > 0 {
            let path = String::from_utf16_lossy(&buf[..size as usize]);
            let name = Path::new(&path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| format!("pid-{}", pid));
            return (Some(path), name);
        }
    }

    // 两条策略都失败：path 为 None（让上游知道这是"未知路径"），process_name 仍给 fallback
    (None, format!("pid-{}", pid))
}
