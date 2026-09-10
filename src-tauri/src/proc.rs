//!
//! proc.rs
//! 子进程工具：**隐藏控制台窗口**地创建子进程（v0.7.7，2026-09-10）。
//!
//! ## 为什么需要这个模块
//! Ryan 反馈（2026-09-10）：Windows 新装后**首次打开会跳一堆命令框**（黑框接连闪现）。
//!
//! 根因：发布版是 GUI 子系统进程（`main.rs` 的 `windows_subsystem = "windows"`，
//! 进程本身**没有控制台**），而 `std::process::Command` 启动控制台程序（`reg.exe`）
//! 时，Windows 会**为子进程新建一个控制台窗口**并短暂显示。
//! 首次启动的 spawn 序列：
//!   1. `commands::check_webview2()` → `read_webview2_version()`：`reg query` **最多 3 次**
//!      （WOW6432Node ClientState / ClientState / Edge BLBeacon）
//!   2. `lib.rs::windows_machine_guid()`：`reg query MachineGuid` **1 次**
//! → 用户看到 3~4 个黑框接连闪现，观感极差（像在跑脚本）。
//!
//! ## 解法
//! 创建进程时带 `CREATE_NO_WINDOW`（0x08000000）创建标志：子进程不分配控制台窗口，
//! stdout 依然可通过管道正常读取（`output()` / `spawn()` 均适用）。
//!
//! ## 平台差异
//! - Windows：设置 `CREATE_NO_WINDOW`
//! - macOS / Linux：直通 `Command::new`，行为与之前完全一致（无控制台概念）
//!
//! ## 使用约定（重要）
//! 本程序内**所有**启动短命系统命令（`reg` / `tasklist` / `netstat` / `ioreg` …）的地方
//! 都应走 `proc::hidden()`；只有「用户主动触发的、需要终端窗口或与用户交互的」命令
//! （如 `explorer /select,` 定位文件、`open` 打开 URL）可以继续用裸 `Command::new`。
//!
//! ## 修改历史
//!   - 2026-09-10 @v0.7.7: 初始创建 - 修复 Windows 首次启动控制台黑框闪烁

use std::process::Command;

/// Windows `CREATE_NO_WINDOW` 进程创建标志（winbase.h）
///
/// 语义：子进程不继承父进程控制台，也**不新建**控制台窗口。
/// 与 `DETACHED_PROCESS` 的区别：后者会连标准句柄都不继承，导致 `output()` 拿不到数据。
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 创建「不弹控制台窗口」的命令
///
/// 用法与 `std::process::Command::new` 完全一致，可直接链式调用：
/// ```ignore
/// let out = proc::hidden("reg").args(["query", key, "/v", "pv"]).output()?;
/// ```
pub fn hidden(program: &str) -> Command {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = Command::new(program);
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    }
    #[cfg(not(target_os = "windows"))]
    {
        Command::new(program)
    }
}
