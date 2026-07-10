//! Linux 平台采集器（X11 实现）
//!
//! 通过 XCB 协议（x11rb 纯 Rust 库）获取前台窗口信息：
//! - `_NET_ACTIVE_WINDOW` EWMH 属性 → 前台窗口 ID
//! - `_NET_WM_NAME`（UTF-8）/ `WM_NAME` → 窗口标题
//! - `_NET_WM_PID` / `WM_CLIENT_MACHINE` → 进程 PID
//! - `/proc/[pid]/cmdline` → 可执行路径 + 进程名
//! - `/proc/[pid]/comm` → 进程短名
//! - `XScreenSaverQueryInfo` → 空闲秒数（需 X ScreenSaver 扩展）
//!
//! Wayland 环境：当前返回 Unsupported，上层采样循环跳过不崩溃。
//! 注：本文件仅在 `target_os = "linux"` 下参与编译。编译时无需系统 X11 库
//! （x11rb 是纯 Rust XCB 协议实现）；运行时需要 X11 Display 服务。

use std::fs;
use std::path::Path;

use crate::error::TrackerError;
use crate::tracker::platform::{PlatformTracker, RawApp};

// x11rb 0.13 API 路径迁移（v0.4.3 修复）：
//   - Atom 常量从 `x11rb::protocol::Atom` → `x11rb::protocol::xproto::Atom`
//   - Connection 从 `x11rb::protocol::xcb::Connection` → `x11rb::connection::Connection`（trait）
use x11rb::connection::Connection;
use x11rb::protocol::xproto::Atom;

/// X11 连接 + 资源封装（RAII 自动断开）
struct X11Connection {
    conn: x11rb::rust_connection::RustConnection,
    screen_num: usize,
}

impl X11Connection {
    fn connect() -> Result<Self, TrackerError> {
        let (conn, screen_num) =
            x11rb::connect(None).map_err(|e| TrackerError::Platform(format!("X11 connect failed: {}", e)))?;
        Ok(Self { conn, screen_num })
    }

    fn root(&self) -> u32 {
        self.conn.setup().roots[self.screen_num].root
    }
}

/// Linux 采集器：X11 EWMH 前台窗口 + procfs 进程信息 + ScreenSaver 空闲检测
pub struct LinuxTracker;

impl PlatformTracker for LinuxTracker {
    fn get_foreground_app(&self) -> Result<RawApp, TrackerError> {
        let x = X11Connection::connect()?;

        // 1. 读取 _NET_ACTIVE_WINDOW（EWMH 标准属性）
        let root = x.root();
        let active_cookie =
            x.conn.get_property(false, root, Atom::NET_ACTIVE_WINDOW.into(), Atom::WINDOW.into(), 0, 1);
        let active_reply = active_cookie
            .reply()
            .map_err(|e| TrackerError::Platform(format!("_NET_ACTIVE_WINDOW query failed: {}", e)))?;

        if active_reply.value_len() == 0 {
            return Err(TrackerError::NoForeground);
        }
        let window_id: u32 = active_reply
            .value32()
            .map_err(|e| TrackerError::Platform(format!("_NET_ACTIVE_WINDOW value32: {}", e)))?
            .first()
            .copied()
            .ok_or_else(|| TrackerError::Platform("_NET_ACTIVE_WINDOW 空值".into()))?;

        // 避免记录自身窗口（ScreenTime Pro 自身）
        // 注意：无法可靠获取自身窗口 ID，此处不做过滤；
        // 上层采样循环已对自身做排除逻辑

        // 2. 读取窗口标题（优先 UTF-8 的 _NET_WM_NAME，fallback WM_NAME）
        let window_title = get_window_title(&x.conn, window_id);

        // 3. 读取进程 PID（_NET_WM_PID）
        let pid = get_window_pid(&x.conn, window_id)?;

        // 4. 从 /proc 读取进程详细信息
        let process_info = read_proc_pid(pid as i32);

        // 5. 组装 RawApp
        let name = window_title
            .as_ref()
            .filter(|t| !t.is_empty())
            .cloned()
            .unwrap_or_else(|| process_info.name.clone());
        let exe_path = process_info.exe_path;

        Ok(RawApp {
            name,
            process_name: process_info.name,
            exe_path,
            bundle_id: None,
            window_title,
        })
    }

    fn get_idle_seconds(&self) -> Result<u64, TrackerError> {
        // 尝试通过 X ScreenSaver 扩展获取空闲时间
        match get_xss_idle() {
            Some(secs) => Ok(secs),
            None => {
                // 无 ScreenSaver 扩展时回退到 0（不影响主流程，只是空闲检测不可用）
                Ok(0)
            }
        }
    }
}

// ===================== 辅助函数 =====================

/// 从指定窗口获取标题字符串
fn get_window_title(
    conn: &impl Connection,
    window: u32,
) -> Option<String> {
    // 优先尝试 _NET_WM_NAME（UTF-8 编码，现代桌面环境标准）
    let cookie = conn.get_property(false, window, Atom::NET_WM_NAME.into(), Atom::UTF8_STRING.into(), 0, 256);
    if let Ok(reply) = cookie.reply() {
        if reply.value_len() > 0 {
            if let Ok(s) = String::from_utf8(reply.value().to_vec()) {
                if !s.trim().is_empty() {
                    return Some(s);
                }
            }
        }
    }

    // Fallback: WM_NAME（传统 Latin-1 编码）
    let cookie2 = conn.get_property(false, window, Atom::WM_NAME.into(), Atom::STRING.into(), 0, 256);
    if let Ok(reply) = cookie2.reply() {
        if reply.value_len() > 0 {
            // WM_NAME 可能是 COMPOUND_TEXT 或 STRING；尝试按 Latin-1 解码
            return Some(String::from_utf8_lossy(reply.value()).trim_end('\0').to_string());
        }
    }

    None
}

/// 从指定窗口获取 PID（_NET_WM_PID）
fn get_window_pid(
    conn: &impl Connection,
    window: u32,
) -> Result<u32, TrackerError> {
    let cookie = conn.get_property(false, window, Atom::NET_WM_PID.into(), Atom::CARDINAL.into(), 0, 1);
    let reply = cookie
        .reply()
        .map_err(|e| TrackerError::Platform(format!("_NET_WM_PID query failed: {}", e)))?;

    if reply.value_len() == 0 {
        return Err(TrackerError::Platform("No _NET_WM_PID on active window".into()));
    }
    Ok(reply
        .value32()
        .map_err(|e| TrackerError::Platform(format!("_NET_WM_PID value32: {}", e)))?
        .first()
        .copied()
        .ok_or_else(|| TrackerError::Platform("_NET_WM_PID 空值".into()))?)
}

/// 从 /proc/[pid]/ 读取进程可执行路径和名称
struct ProcInfo {
    name: String,
    exe_path: Option<String>,
}

fn read_proc_pid(pid: i32) -> ProcInfo {
    let proc_dir = format!("/proc/{}", pid);

    // 读取 /proc/[pid]/exe 符号链接 → 可执行文件绝对路径
    let exe_path = fs::read_link(format!("{}/exe", proc_dir))
        .ok()
        .and_then(|p| p.to_str().map(String::from));

    // 读取 /proc/[pid]/comm → 进程短名（如 "chrome", "code"）
    let name = fs::read_to_string(format!("{}/comm", proc_dir))
        .ok()
        .map(|s| s.trim_end_matches('\n').to_string())
        .unwrap_or_else(|| {
            // comm 不可用则从 exe_path 提取文件名
            exe_path
                .as_ref()
                .and_then(|p| Path::new(p).file_name())
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| format!("pid-{}", pid))
        });

    ProcInfo { name, exe_path }
}

/// 通过 X ScreenSaver 扩展获取用户空闲毫秒数
///
/// 使用 x11rb 的 screensaver 模块发送 XScreenSaverQueryInfo 请求。
/// 返回 Some(秒数) 或 None（扩展不可用）。
#[cfg(feature = "xss")]
fn get_xss_idle() -> Option<u64> {
    use x11rb::protocol::xss::{self, ConnectionExt as _};
    use x11rb::connection::Connection;

    let (conn, _) = x11rb::connect(None).ok()?;
    let screen = conn.setup().screens.first()?;
    let cookie = conn.xss_query_info(screen.root);
    let reply = cookie.reply().ok()?;
    Some((reply.ms_since_user_input / 1000) as u64)
}

/// 无 xss feature 时直接返回 None
#[cfg(not(feature = "xss"))]
fn get_xss_idle() -> Option<u64> {
    // 不启用 xss feature 时跳过 ScreenSaver 检测
    None
}
