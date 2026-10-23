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
//!
//! 修改历史：
//! - 2026-10-19 @v1.2.1: 修复「记录中 screentime-pro · ScreenTime Float」。根因：float/pet/
//!   pet-menu/capture 与主窗口同一进程同一 exe，在这些辅助窗口上按下（浮窗 startDragging）
//!   会使其短暂成为前台窗口，旧实现无差别采集 → 自家悬浮窗被录成被追踪应用（macOS 浮窗不夺
//!   frontmost 故无此问题）。新增 SELF_HELPER_TITLES 标题过滤，命中即返回 NoForeground 跳过；
//!   主窗口 ScreenTime Pro 不过滤（操作设置面板时记录自身属预期）。
//! - 2026-10-23 @v1.2.6: 新增「前台窗口不可见即跳过」的通用兜底（`IsWindowVisible`）。
//!   病根：`GetForegroundWindow()` 返回的是**前台**窗口，而前台 ≠ 可见 —— `tauri.conf.json`
//!   里 `capture` 窗口配了 `visible:false` 却 `focus:true`，隐藏窗创建时仍会成为前台且此后
//!   **不会自行让出**，导致采样每次都命中上面的标题白名单并跳过，整段不落库。
//!   A/B 实测（同机同库、均持续发真实鼠标事件保持非空闲）：前台=该隐藏窗 18s 新增 0 条，
//!   前台=真实应用 18s 新增 1 条。根治见 `tauri.conf.json` 把该窗口 `focus` 改回 false；
//!   这里是通用防御层（第三方/系统也可能造出隐藏前台窗），判据是「用户在看的东西
//!   才可能有使用时长」。

use crate::error::TrackerError;
use crate::tracker::platform::{PlatformTracker, RawApp};
use std::path::Path;
use std::os::raw::{c_uint, c_void};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

/// Windows 采集器
pub struct WindowsTracker;

/// 本程序自家的**辅助窗口**标题（与 tauri.conf.json 各窗口 title 一致）。
///
/// v1.2.1 修复「记录 screentime-pro · ScreenTime Float」：float / pet / pet-menu / capture
/// 与主窗口同属一个进程、同一个 exe，差别只在窗口标题。在这些窗口上按下（浮窗整条是
/// 原生 startDragging 拖拽区）会让它们短暂成为 `GetForegroundWindow` 的前台窗口，
/// 旧实现无差别采集 → 把自家悬浮窗当成被追踪应用 upsert 入库。
///
/// 这里按标题把辅助窗口识别出来并当作「无前台」跳过（采样循环忽略该 tick、不建会话）。
/// **主窗口 `ScreenTime Pro` 刻意不在列表中**：用户真正在操作设置面板时记录本程序是
/// 预期行为（v0.9.7 仅修了其「名称重复展示」，未排除自身）。
const SELF_HELPER_TITLES: [&str; 4] = [
    "ScreenTime Float",
    "ScreenTime Pet",
    "ScreenTime Pet Menu",
    "ScreenTime Capture",
];

impl PlatformTracker for WindowsTracker {
    fn get_foreground_app(&self) -> Result<RawApp, TrackerError> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_invalid() {
                return Err(TrackerError::NoForeground);
            }
            let mut pid: u32 = 0;
            let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));

            // ── v1.2.6：不可见窗口不能算「用户正在使用的应用」 ──
            //
            // 病根：`GetForegroundWindow()` 返回的是「**前台**窗口」，而前台 ≠ 可见。
            // 一个 hidden 窗口仍可持有前台状态（Tauri 创建时带 focus=true 即如此），
            // 之后**不会自行退出**前台，只有用户点别的窗口才会换掉。
            //
            // 实测（本机 Windows 11，标题为 `ScreenTime Capture` 的遮罩窗）：
            //   hwnd=8717262  visible=False  class=Tauri Window
            //   → GetForegroundWindow() 的返回值就是它，且长期不变。
            // 而 `ScreenTime Capture` 在 `SELF_HELPER_TITLES` 里 → 每次采样都返回
            // NoForeground → 采样循环 continue → **整段时间一条记录都不落库**。
            //
            // A/B 验证（同一台机器、同一份 DB、都持续发真实鼠标事件保持非空闲）：
            //   前台=ScreenTime Capture(隐藏) → 18s 新增 0 条
            //   前台=WorkBuddy(真实应用)     → 18s 新增 1 条
            //
            // 这条 visible 检查是**通用兜底**：根治靠 `tauri.conf.json` 里把该窗口的
            // `focus` 改回 false（与 pet / pet-menu / float 三个隐藏窗保持一致）。
            // 但配置只影响「以后新建的窗口」，且第三方/系统也可能造出隐藏前台窗，
            // 所以这里再判一次可见性——判据本身是「用户在看的东西才可能有使用时长」。
            if !IsWindowVisible(hwnd).as_bool() {
                tracing::debug!(
                    hwnd = hwnd.0 as usize,
                    "前台窗口不可见（隐藏窗持有前台），跳过采集"
                );
                return Err(TrackerError::NoForeground);
            }

            // 前台窗口标题（用于「按窗口标题分类」）
            let mut title_buf = [0u16; 1024];
            let len = GetWindowTextW(hwnd, &mut title_buf);
            let window_title = if len > 0 {
                Some(String::from_utf16_lossy(&title_buf[..len as usize]))
            } else {
                None
            };

            // 自家辅助窗口抢前台（点浮窗/拖桌宠/截图遮罩）→ 当作无前台跳过，
            // 不把 ScreenTime Float 之类录成被追踪应用（v1.2.1）
            if let Some(t) = window_title.as_deref() {
                if SELF_HELPER_TITLES.contains(&t) {
                    tracing::debug!(title = t, "前台为本程序辅助窗口，跳过采集");
                    return Err(TrackerError::NoForeground);
                }
            }

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
