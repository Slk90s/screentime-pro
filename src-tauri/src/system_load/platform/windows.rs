//! system_load/platform/windows.rs
//! Windows 内存 / 磁盘采样（v0.9.7 从 metrics.rs 迁出）。
//!
//! 为什么之前没有：v0.7.5 把内存/磁盘整体锁死在 macOS 分支，Windows 恒返回 (0,0)，
//! 于是浮窗/托盘上内存永远是 0%、也从来没有磁盘。这里用 Win32 原生 API 补齐：
//!   - 内存：GlobalMemoryStatusEx（比读 WMI/perf counter 快且零依赖）
//!   - 磁盘：GetDiskFreeSpaceExW 取**系统盘**（%SystemDrive%，通常 C:）
//!     —— 与资源管理器「此电脑」中系统盘的口径一致
//! 两者都属 Win32 系统调用，无需 proc::hidden（不创建进程，不会有黑框）。
//!
//! Windows 由系统调用直接给出 used/total，不需要 inner 里的常量，故后端方法为 no-op 忽略 inner。

use crate::system_load::metrics::MetricsInner;
use super::MetricsBackend;

pub(crate) struct Backend;

impl MetricsBackend for Backend {
    fn sample_memory(_inner: &MetricsInner) -> (u64, u64) {
        ffi_read_windows_memory()
    }

    fn read_disk_usage() -> (u64, u64) {
        ffi_read_disk_usage()
    }

    fn init_memory_constants(_inner: &mut MetricsInner) {}
}

fn ffi_read_windows_memory() -> (u64, u64) {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    if unsafe { GlobalMemoryStatusEx(&mut status) }.is_err() {
        return (0, 0);
    }
    let total = status.ullTotalPhys;
    if total == 0 {
        return (0, 0);
    }
    // ullAvailPhys 是「物理内存可用量」（含 standby/cached，可被立即再利用），
    // 与任务管理器「可用」一栏同口径。
    let avail = status.ullAvailPhys.min(total);
    (total - avail, total)
}

fn ffi_read_disk_usage() -> (u64, u64) {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    // 系统盘根路径，如 "C:\"。%SystemDrive% 缺失（极端情况）时回退 C:
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    let root: Vec<u16> = format!("{}\\", drive.trim_end_matches('\\'))
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let mut free_to_caller = 0u64;
    let mut total = 0u64;
    let mut total_free = 0u64;
    let rc = unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(root.as_ptr()),
            Some(&mut free_to_caller),
            Some(&mut total),
            Some(&mut total_free),
        )
    };
    if rc.is_err() || total == 0 {
        return (0, 0);
    }
    (total.saturating_sub(total_free), total)
}
