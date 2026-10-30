//! system_load/platform/windows.rs
//! Windows 内存 / 磁盘采样（v0.9.7 从 metrics.rs 迁出；v1.2.12 补多盘）。
//!
//! 为什么之前没有：v0.7.5 把内存/磁盘整体锁死在 macOS 分支，Windows 恒返回 (0,0)，
//! 于是浮窗/托盘上内存永远是 0%、也从来没有磁盘。这里用 Win32 原生 API 补齐：
//!   - 内存：GlobalMemoryStatusEx（比读 WMI/perf counter 快且零依赖）
//!   - 磁盘：GetDiskFreeSpaceExW 取**系统盘**（%SystemDrive%，通常 C:）
//!   - 多盘（v1.2.12）：GetLogicalDrives + GetDriveTypeW 过滤固定盘，逐盘取容量
//! 两者都属 Win32 系统调用，无需 proc::hidden（不创建进程，不会有黑框）。
//!
//! Windows 由系统调用直接给出 used/total，不需要 inner 里的常量，故后端方法为 no-op 忽略 inner。
//!
//! ## v1.2.12 口径变更（重要）
//! `GetDiskFreeSpaceExW` 有三个出参，v1.2.11 及以前用的是第 3 个
//! （`lpTotalNumberOfFreeBytes`，磁盘**物理**空闲），导致「已用」比资源管理器偏小。
//! 本版改用第 1 个 `lpFreeBytesAvailableToCaller`（**当前用户**可用，已扣配额），
//! 与 macOS/Linux 的 `f_bavail` 对齐。资源管理器「此电脑」显示的也是这个值。

use super::{DiskVolume, MetricsBackend};
use crate::system_load::metrics::MetricsInner;

pub(crate) struct Backend;

impl MetricsBackend for Backend {
    fn sample_memory(_inner: &MetricsInner) -> (u64, u64) {
        ffi_read_windows_memory()
    }

    fn read_disk_usage() -> (u64, u64) {
        ffi_read_disk_usage()
    }

    fn read_disks() -> Vec<DiskVolume> {
        ffi_read_disks()
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

/// 系统盘根路径，如 `C:\`。`%SystemDrive%` 缺失（极端情况）时回退 `C:`。
fn system_drive_root() -> String {
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    format!("{}\\", drive.trim_end_matches('\\'))
}

/// 查询单个卷的 (已用, 总量, 可用)。
///
/// 用 `lpFreeBytesAvailableToCaller`（用户可用）算 used，见文件头「口径变更」。
fn query_volume(root: &str) -> Option<(u64, u64, u64)> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
    let mut avail_to_caller = 0u64;
    let mut total = 0u64;
    let mut total_free = 0u64;
    let rc = unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(wide.as_ptr()),
            Some(&mut avail_to_caller),
            Some(&mut total),
            Some(&mut total_free),
        )
    };
    if rc.is_err() || total == 0 {
        return None;
    }
    let avail = avail_to_caller.min(total);
    Some((total.saturating_sub(avail), total, avail))
}

fn ffi_read_disk_usage() -> (u64, u64) {
    match query_volume(&system_drive_root()) {
        Some((used, total, _)) => (used, total),
        None => (0, 0),
    }
}

/// 枚举所有**本地固定**盘（`DRIVE_FIXED`）。
///
/// 过滤掉的东西以及原因：
/// - `DRIVE_REMOVABLE`（U 盘/SD 卡）：拔插频繁，容量抖动会让浮窗数字乱跳
/// - `DRIVE_REMOTE`（网络映射盘）：`GetDiskFreeSpaceExW` 可能阻塞数秒，1Hz 采样扛不住
/// - `DRIVE_CDROM` / `DRIVE_RAMDISK` / `DRIVE_NO_ROOT_DIR`：无意义或不可用
/// - **未就绪的卷**（`GetDiskFreeSpaceExW` 失败）：直接跳过而非记 0，
///   否则会显示「0/0」这种假数据
#[allow(dead_code)] // v1.2.13 接入多盘调用方后删除（详见 platform/mod.rs 的 DiskVolume 文档）
fn ffi_read_disks() -> Vec<DiskVolume> {
    use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};

    // `DRIVE_FIXED` 在 windows crate 里位于 `Win32::System::WindowsProgramming`，
    // 为一个 u32 常量启用整个模块不划算（该模块很大，明显拖慢增量编译）。
    // 这里的值取自 <winbase.h>，自 Windows NT 起从未变更：
    //   DRIVE_UNKNOWN=0 / DRIVE_NO_ROOT_DIR=1 / DRIVE_REMOVABLE=2
    //   DRIVE_FIXED=3 / DRIVE_REMOTE=4 / DRIVE_CDROM=5 / DRIVE_RAMDISK=6
    const DRIVE_FIXED: u32 = 3;

    let mask = unsafe { GetLogicalDrives() };
    if mask == 0 {
        return Vec::new();
    }
    let sys_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    let sys_letter = sys_drive
        .trim_end_matches('\\')
        .chars()
        .next()
        .map(|c| c.to_ascii_uppercase());

    let mut out = Vec::new();
    for i in 0..26u32 {
        // bit i 对应盘符 'A' + i
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = char::from(b'A' + i as u8);
        let root = format!("{letter}:\\");
        let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
        let ty = unsafe { GetDriveTypeW(windows::core::PCWSTR(wide.as_ptr())) };
        if ty != DRIVE_FIXED {
            continue;
        }
        let Some((used, total, avail)) = query_volume(&root) else {
            // 卷在位但查询失败（未就绪 / 无权限）——跳过，不记 0
            continue;
        };
        if total == 0 {
            continue;
        }
        out.push(DiskVolume {
            mount: format!("{letter}:"),
            total_bytes: total,
            used_bytes: used,
            avail_bytes: avail,
            is_system: Some(letter) == sys_letter,
        });
    }
    // 大容量优先，前端取第一项做「主盘」时结果稳定
    out.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_usage_matches_system_volume_in_enumeration() {
        let (used, total) = Backend::read_disk_usage();
        assert!(total > 0, "系统盘总量必须非零，实际 {total}");
        assert!(
            used <= total,
            "已用不应超过总量（used={used} total={total}）"
        );

        let vols = Backend::read_disks();
        assert!(!vols.is_empty(), "至少应枚举出系统盘");
        let sys: Vec<_> = vols.iter().filter(|v| v.is_system).collect();
        assert_eq!(sys.len(), 1, "系统盘应恰好 1 个，实际 {sys:?}");
        // 两个 API 的必然一致性：read_disk_usage 就是系统盘那一项
        assert_eq!(
            (sys[0].used_bytes, sys[0].total_bytes),
            (used, total),
            "read_disk_usage 与 read_disks 的系统盘项必须一致"
        );
    }

    #[test]
    fn every_volume_is_internally_consistent() {
        for v in Backend::read_disks() {
            assert!(v.total_bytes > 0, "{} 总量为 0", v.mount);
            assert!(
                v.used_bytes <= v.total_bytes,
                "{} 已用 {} > 总量 {}",
                v.mount,
                v.used_bytes,
                v.total_bytes
            );
            assert_eq!(
                v.avail_bytes,
                v.total_bytes - v.used_bytes,
                "{} 的 avail 必须等于 total - used",
                v.mount
            );
            // 盘符格式：单个大写字母 + 冒号
            let b = v.mount.as_bytes();
            assert_eq!(b.len(), 2, "盘符长度应为 2，实际 {:?}", v.mount);
            assert!(
                b[0].is_ascii_uppercase(),
                "盘符应为大写，实际 {:?}",
                v.mount
            );
            assert_eq!(b[1], b':', "盘符应以冒号结尾，实际 {:?}", v.mount);
        }
    }

    #[test]
    fn volumes_are_sorted_by_size_desc() {
        let vols = Backend::read_disks();
        for w in vols.windows(2) {
            assert!(
                w[0].total_bytes >= w[1].total_bytes,
                "应按总量降序：{} ({}) 排在 {} ({}) 之前",
                w[0].mount,
                w[0].total_bytes,
                w[1].mount,
                w[1].total_bytes
            );
        }
    }

    /// 人工核对用：cargo test --lib system_load -- --nocapture --ignored disk_probe
    #[test]
    #[ignore]
    fn disk_probe() {
        for v in Backend::read_disks() {
            println!(
                "{} {:<6} total={:>6.1}GB used={:>6.1}GB avail={:>6.1}GB  {:.1}%",
                if v.is_system { "*" } else { " " },
                v.mount,
                v.total_bytes as f64 / 1e9,
                v.used_bytes as f64 / 1e9,
                v.avail_bytes as f64 / 1e9,
                v.used_bytes as f64 / v.total_bytes as f64 * 100.0
            );
        }
    }
}
