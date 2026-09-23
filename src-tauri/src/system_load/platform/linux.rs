//! system_load/platform/linux.rs
//! Linux 内存 / 磁盘采样（v0.9.7 从 metrics.rs 迁出）。
//!   - 内存：/proc/meminfo 的 MemTotal / MemAvailable（纯 std 读文件，零 FFI）
//!   - 磁盘：libc::statvfs("/")，容量 = f_blocks × f_frsize，可用取 f_bavail
//!     （f_bavail 才是非 root 用户真正可写的量，与 `df -h` 的 Avail 列一致）
//!
//! Linux 由系统调用直接给出 used/total，不需要 inner 里的常量，故后端方法为 no-op 忽略 inner。

use crate::system_load::metrics::MetricsInner;
use super::MetricsBackend;

pub(crate) struct Backend;

impl MetricsBackend for Backend {
    fn sample_memory(_inner: &MetricsInner) -> (u64, u64) {
        ffi_read_linux_memory()
    }

    fn read_disk_usage() -> (u64, u64) {
        ffi_read_disk_usage()
    }

    fn init_memory_constants(_inner: &mut MetricsInner) {}
}

fn ffi_read_linux_memory() -> (u64, u64) {
    let text = match std::fs::read_to_string("/proc/meminfo") {
        Ok(t) => t,
        Err(_) => return (0, 0),
    };
    // 单位：kB（Linux 固定以 kB 输出）
    let mut total_kb = 0u64;
    let mut avail_kb = 0u64;
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let Some(key) = it.next() else { continue };
        let Some(value) = it.next().and_then(|v| v.parse::<u64>().ok()) else {
            continue;
        };
        match key {
            "MemTotal:" => total_kb = value,
            "MemAvailable:" => avail_kb = value,
            _ => {}
        }
        if total_kb > 0 && avail_kb > 0 {
            break;
        }
    }
    if total_kb == 0 {
        return (0, 0);
    }
    let total = total_kb.saturating_mul(1024);
    let avail = avail_kb.saturating_mul(1024).min(total);
    (total - avail, total)
}

fn ffi_read_disk_usage() -> (u64, u64) {
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    let path = b"/\0";
    // 用 libc::c_char 而非写死 i8——aarch64-linux 上 c_char 是 u8（同 macOS 的差异点）
    let rc = unsafe { libc::statvfs(path.as_ptr() as *const libc::c_char, &mut st) };
    if rc != 0 {
        return (0, 0);
    }
    let frsize = if st.f_frsize > 0 {
        u64::from(st.f_frsize)
    } else {
        u64::from(st.f_bsize)
    };
    if frsize == 0 || st.f_blocks == 0 {
        return (0, 0);
    }
    // u64::from 而非 as：兼容 32 位 Linux（c_ulong 为 u32）与 64 位（u64）两种宽度
    let total = u64::from(st.f_blocks).saturating_mul(frsize);
    let free = u64::from(st.f_bavail).saturating_mul(frsize);
    (total.saturating_sub(free), total)
}
