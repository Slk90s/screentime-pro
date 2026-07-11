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
//!
//! ## x11rb 0.13 API 适配说明
//!
//! x11rb 0.13 对 `GetPropertyReply` 做了 breaking change：
//! - `value_len` / `value` 字段变为**私有**
//! - 必须使用类型化访问器：`value8()` / `value16()` / `value32()`
//!   这些方法返回 `Option<Impl Iterator>`（format 不匹配时返回 None）
//! - 所有请求函数（get_property / intern_atom / xss_query_info）返回 `Result<Cookie, E>`
//!   需先解开 Result 再调用 cookie.reply()

use std::fs;
use std::path::Path;

use crate::error::TrackerError;
use crate::tracker::platform::{PlatformTracker, RawApp};

// x11rb 0.13 路径：
//   - Connection trait 在 x11rb::connection
//   - 协议方法（get_property 等）在 x11rb::protocol::xproto::ConnectionExt trait
//   - Atom 类型在 x11rb::protocol::xproto
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{Atom, ConnectionExt};

/// X11 EWMH/ICCCM 协议所需 atoms（连接时一次性 intern 缓存）
struct X11Atoms {
    net_active_window: Atom,
    net_wm_name: Atom,
    net_wm_pid: Atom,
    utf8_string: Atom,
    wm_name: Atom,
    string: Atom,
    cardinal: Atom,
    window: Atom,
}

impl X11Atoms {
    fn intern(conn: &impl Connection) -> Result<Self, TrackerError> {
        let names: &[&[u8]] = &[
            b"NET_ACTIVE_WINDOW",
            b"NET_WM_NAME",
            b"NET_WM_PID",
            b"UTF8_STRING",
            b"WM_NAME",
            b"STRING",
            b"CARDINAL",
            b"WINDOW",
        ];
        // intern_atom 返回 Result<Cookie, ConnectionError>
        let mut cookies = Vec::with_capacity(names.len());
        for name in names {
            let c = conn
                .intern_atom(false, *name)
                .map_err(|e| TrackerError::Platform(format!("intern_atom send: {}", e)))?;
            cookies.push(c);
        }
        // 每个 cookie reply 解析为 atom
        let mut atoms = Vec::with_capacity(cookies.len());
        for c in cookies {
            let r = c
                .reply()
                .map_err(|e| TrackerError::Platform(format!("intern_atom reply: {}", e)))?;
            atoms.push(r.atom);
        }
        Ok(Self {
            net_active_window: atoms[0],
            net_wm_name: atoms[1],
            net_wm_pid: atoms[2],
            utf8_string: atoms[3],
            wm_name: atoms[4],
            string: atoms[5],
            cardinal: atoms[6],
            window: atoms[7],
        })
    }
}

/// X11 连接 + 资源封装（RAII 自动断开）
struct X11Connection {
    conn: x11rb::rust_connection::RustConnection,
    screen_num: usize,
    atoms: X11Atoms,
}

impl X11Connection {
    fn connect() -> Result<Self, TrackerError> {
        let (conn, screen_num) =
            x11rb::connect(None).map_err(|e| TrackerError::Platform(format!("X11 connect failed: {}", e)))?;
        let atoms = X11Atoms::intern(&conn)?;
        Ok(Self { conn, screen_num, atoms })
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

        // 1. 读取 _NET_ACTIVE_WINDOW（EWMH 标准属性）→ WINDOW 类型 (u32)
        let root = x.root();
        let active_cookie = x
            .conn
            .get_property(false, root, x.atoms.net_active_window, x.atoms.window, 0, 1)
            .map_err(|e| TrackerError::Platform(format!("_NET_ACTIVE_WINDOW send: {}", e)))?;
        let active_reply = active_cookie
            .reply()
            .map_err(|e| TrackerError::Platform(format!("_NET_ACTIVE_WINDOW query failed: {}", e)))?;

        // x11rb 0.13: value32() 返回 Option<Iterator<Item=u32>>（format != 32 时 None）
        let window_id = active_reply
            .value32()
            .and_then(|mut iter| iter.next())
            .ok_or(TrackerError::NoForeground)?;

        // 2. 读取窗口标题（优先 UTF-8 的 _NET_WM_NAME，fallback WM_NAME）
        let window_title = get_window_title(&x.conn, window_id, &x.atoms);

        // 3. 读取进程 PID（_NET_WM_PID）→ CARDINAL 类型 (u32)
        let pid = get_window_pid(&x.conn, window_id, &x.atoms)?;

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
        match get_xss_idle() {
            Some(secs) => Ok(secs),
            None => Ok(0),
        }
    }
}

// ===================== 辅助函数 =====================

/// 从指定窗口获取标题字符串
///
/// x11rb 0.13: GetPropertyReply 的 value 字段私有化；
/// UTF-8 字符串用 `value8()` 读取字节迭代器再收集为 Vec<u8>。
fn get_window_title(
    conn: &impl Connection,
    window: u32,
    atoms: &X11Atoms,
) -> Option<String> {
    // 优先尝试 _NET_WM_NAME（UTF-8 编码，现代桌面环境标准）
    if let Ok(cookie) = conn.get_property(false, window, atoms.net_wm_name, atoms.utf8_string, 0, 256) {
        if let Ok(reply) = cookie.reply() {
            // value8(): format==8 时 Some(Iterator<u8>)，否则 None
            if let Some(bytes) = reply.value8() {
                let vec: Vec<u8> = bytes.collect();
                if let Ok(s) = String::from_utf8(vec) {
                    let trimmed = s.trim_end('\0').trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
            }
        }
    }

    // Fallback: WM_NAME（传统 Latin-1/STRING 编码）
    if let Ok(cookie) = conn.get_property(false, window, atoms.wm_name, atoms.string, 0, 256) {
        if let Ok(reply) = cookie.reply() {
            if let Some(bytes) = reply.value8() {
                let vec: Vec<u8> = bytes.collect();
                let s = String::from_utf8_lossy(&vec);
                let trimmed = s.trim_end('\0').trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }

    None
}

/// 从指定窗口获取 PID（_NET_WM_PID）→ CARDINAL/u32
///
/// x11rb 0.13: value32() 返回 Option<Iterator<Item=u32>>
fn get_window_pid(
    conn: &impl Connection,
    window: u32,
    atoms: &X11Atoms,
) -> Result<u32, TrackerError> {
    let cookie = conn
        .get_property(false, window, atoms.net_wm_pid, atoms.cardinal, 0, 1)
        .map_err(|e| TrackerError::Platform(format!("_NET_WM_PID send: {}", e)))?;
    let reply = cookie
        .reply()
        .map_err(|e| TrackerError::Platform(format!("_NET_WM_PID query failed: {}", e)))?;

    // value32() 返回 Option<Iterator>；无值或格式不匹配均为 None
    reply
        .value32()
        .and_then(|mut iter| iter.next())
        .ok_or_else(|| TrackerError::Platform("No _NET_WM_PID on active window".into()))
}

/// 从 /proc/[pid]/ 读取进程可执行路径和名称
struct ProcInfo {
    name: String,
    exe_path: Option<String>,
}

fn read_proc_pid(pid: i32) -> ProcInfo {
    let proc_dir = format!("/proc/{}", pid);

    let exe_path = fs::read_link(format!("{}/exe", proc_dir))
        .ok()
        .and_then(|p| p.to_str().map(String::from));

    let name = fs::read_to_string(format!("{}/comm", proc_dir))
        .ok()
        .map(|s| s.trim_end_matches('\n').to_string())
        .unwrap_or_else(|| {
            exe_path
                .as_ref()
                .and_then(|p| Path::new(p).file_name())
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| format!("pid-{}", pid))
        });

    ProcInfo { name, exe_path }
}

/// 通过 X ScreenSaver 扩展获取用户空闲毫秒数
#[cfg(feature = "xss")]
fn get_xss_idle() -> Option<u64> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xss::ConnectionExt as _;

    let (conn, _) = x11rb::connect(None).ok()?;
    let screen = conn.setup().screens.first()?;
    // xss_query_info 返回 Result<Cookie, _>
    let cookie = conn.xss_query_info(screen.root).ok()?;
    let reply = cookie.reply().ok()?;
    Some((reply.ms_since_user_input / 1000) as u64)
}

/// 无 xss feature 时直接返回 None
#[cfg(not(feature = "xss"))]
fn get_xss_idle() -> Option<u64> {
    None
}
