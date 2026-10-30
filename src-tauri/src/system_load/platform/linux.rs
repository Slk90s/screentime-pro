//! system_load/platform/linux.rs
//! Linux 内存 / 磁盘采样（v0.9.7 从 metrics.rs 迁出；v1.2.12 补多盘）。
//!   - 内存：/proc/meminfo 的 MemTotal / MemAvailable（纯 std 读文件，零 FFI）
//!   - 磁盘：libc::statvfs("/")，容量 = f_blocks × f_frsize，可用取 f_bavail
//!     （f_bavail 才是非 root 用户真正可写的量，与 `df -h` 的 Avail 列一致）
//!   - 多盘（v1.2.12）：读 /proc/mounts 找真实块设备挂载点，逐个 statvfs
//!
//! Linux 由系统调用直接给出 used/total，不需要 inner 里的常量，故后端方法为 no-op 忽略 inner。
//!
//! ## v1.2.12 口径
//! Linux 端**本来就**用 f_bavail，本版只是把 Windows / macOS 拉齐到这里 —— 所以
//! Linux 用户看到的数字不会变，变了的是另外两端。这一点在发版说明里要讲清楚，
//! 否则 Linux 用户会以为「这版没改 Linux」，而 Windows/macOS 用户会以为「数字怎么变小了」。

use super::{DiskVolume, MetricsBackend};
use crate::system_load::metrics::MetricsInner;

pub(crate) struct Backend;

impl MetricsBackend for Backend {
    fn sample_memory(_inner: &MetricsInner) -> (u64, u64) {
        ffi_read_linux_memory()
    }

    fn read_disk_usage() -> (u64, u64) {
        ffi_read_disk_usage()
    }

    fn read_disks() -> Vec<DiskVolume> {
        ffi_read_disks()
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
    match statvfs_one("/") {
        Some((used, total, _)) => (used, total),
        None => (0, 0),
    }
}

/// 对单个挂载点调 statvfs，返回 (已用, 总量, 可用)。
/// 三者都用 f_bavail 口径（用户可用），见文件头。
fn statvfs_one(mount: &str) -> Option<(u64, u64, u64)> {
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    let cpath = std::ffi::CString::new(mount).ok()?;
    // 用 libc::c_char 而非写死 i8——aarch64-linux 上 c_char 是 u8（同 macOS 的差异点）
    let rc = unsafe { libc::statvfs(cpath.as_ptr() as *const libc::c_char, &mut st) };
    if rc != 0 {
        return None;
    }
    let frsize = if st.f_frsize > 0 {
        u64::from(st.f_frsize)
    } else {
        u64::from(st.f_bsize)
    };
    if frsize == 0 || st.f_blocks == 0 {
        return None;
    }
    // u64::from 而非 as：兼容 32 位 Linux（c_ulong 为 u32）与 64 位（u64）两种宽度
    let total = u64::from(st.f_blocks).saturating_mul(frsize);
    let avail = u64::from(st.f_bavail).saturating_mul(frsize).min(total);
    Some((total.saturating_sub(avail), total, avail))
}

/// `/proc/mounts` 里出现的这些文件系统类型不是「本机存储」，一律跳过。
///
/// 为什么必须过滤：容器 / snap 环境下 `/proc/mounts` 动辄上百行，
/// 里面有 overlay、tmpfs、cgroup、proc、sysfs、squashfs（snap 挂载的只读镜像）等。
/// 全部列出来用户会看到一堆 64MB 的「盘」，而且 squashfs 是只读的，已用恒 100%。
const SKIP_FS: &[&str] = &[
    "proc",
    "sysfs",
    "devtmpfs",
    "devpts",
    "tmpfs",
    "cgroup",
    "cgroup2",
    "pstore",
    "bpf",
    "debugfs",
    "tracefs",
    "securityfs",
    "hugetlbfs",
    "mqueue",
    "fusectl",
    "configfs",
    "ramfs",
    "overlay",
    "aufs",
    "squashfs",
    "nsfs",
    "autofs",
    "binfmt_misc",
    "rpc_pipefs",
    "fuse.gvfsd-fuse",
];

/// 挂载点前缀黑名单：这些位置属系统内部，不是用户理解的「磁盘」。
const SKIP_MOUNT_PREFIX: &[&str] = &[
    "/snap/",
    "/var/lib/docker/",
    "/var/lib/snapd/",
    "/run/",
    "/sys/",
    "/proc/",
    "/dev/",
];

/// 枚举真实块设备挂载点。
///
/// 解析 `/proc/mounts`：每行 `设备 挂载点 类型 选项 0 0`，空格分隔，
/// 挂载点里的空格/制表符会被转义成 `\040` / `\011` / `\134`（反斜杠自己）。
///
/// **设备名过滤**：只接受 `/dev/` 开头的设备（真实块设备），
/// 排除 `none` / `tmpfs` / `overlay` / `proc` 这类伪设备。
/// LVM / LUKS / RAID 都是 `/dev/mapper/xxx` 或 `/dev/md0`，仍在 `/dev/` 下，不受影响。
///
/// **同设备去重**：bind mount（`mount --bind`）会让同一设备出现多次，
/// 用设备名去重后只保留首个（通常是真实挂载点而非 bind 视图）。
#[allow(dead_code)] // v1.2.13 接入多盘调用方后删除（详见 platform/mod.rs 的 DiskVolume 文档）
fn ffi_read_disks() -> Vec<DiskVolume> {
    let text = match std::fs::read_to_string("/proc/mounts") {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };

    let mut seen_dev: Vec<String> = Vec::new();
    let mut out = Vec::new();

    for line in text.lines() {
        let mut it = line.split_whitespace();
        let (Some(dev), Some(mnt), Some(fstype)) = (it.next(), it.next(), it.next()) else {
            continue;
        };
        if !dev.starts_with("/dev/") {
            continue;
        }
        if SKIP_FS.contains(&fstype) {
            continue;
        }
        let mount = unescape_mount(mnt);
        if SKIP_MOUNT_PREFIX.iter().any(|p| mount.starts_with(p)) {
            continue;
        }
        // bind mount 去重：同一设备只统计一次
        if seen_dev.iter().any(|d| d == dev) {
            continue;
        }
        let Some((used, total, avail)) = statvfs_one(&mount) else {
            continue;
        };
        if total == 0 {
            continue;
        }
        seen_dev.push(dev.to_string());
        out.push(DiskVolume {
            is_system: mount == "/",
            mount,
            total_bytes: total,
            used_bytes: used,
            avail_bytes: avail,
        });
    }

    // 兜底：若过滤后一个都没剩（极端容器环境），至少给出根分区
    if out.is_empty() {
        if let Some((used, total, avail)) = statvfs_one("/") {
            if total > 0 {
                out.push(DiskVolume {
                    is_system: true,
                    mount: "/".to_string(),
                    total_bytes: total,
                    used_bytes: used,
                    avail_bytes: avail,
                });
            }
        }
    }

    out.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));
    out
}

/// `/proc/mounts` 的八进制转义解码（目前只处理挂载点里会出现的三种）。
fn unescape_mount(s: &str) -> String {
    if !s.contains('\\') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() {
            let oct = &s[i + 1..i + 4];
            if let Ok(v) = u8::from_str_radix(oct, 8) {
                out.push(v as char);
                i += 4;
                continue;
            }
        }
        out.push(s.as_bytes()[i] as char);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescape_handles_octal_escapes() {
        assert_eq!(unescape_mount("/mnt/foo"), "/mnt/foo");
        assert_eq!(unescape_mount("/mnt/with\\040space"), "/mnt/with space");
        assert_eq!(unescape_mount("/mnt/with\\011tab"), "/mnt/with\ttab");
        // 反斜杠自身：\134 → '\'
        assert_eq!(unescape_mount("/mnt/back\\134slash"), "/mnt/back\\slash");
        // 孤立反斜杠不应 panic
        assert_eq!(unescape_mount("/mnt/tail\\"), "/mnt/tail\\");
    }

    #[test]
    fn disks_are_internally_consistent() {
        let vols = Backend::read_disks();
        assert!(!vols.is_empty(), "至少应回落到根分区");
        for v in &vols {
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
                "{} avail 必须等于 total - used",
                v.mount
            );
            assert!(!v.mount.contains(' '), "{} 未解码转义", v.mount);
        }
        // 恰好一个系统盘（挂载点 "/"）
        assert_eq!(
            vols.iter().filter(|v| v.is_system).count(),
            1,
            "系统盘应恰好 1 个：{:?}",
            vols.iter().map(|v| &v.mount).collect::<Vec<_>>()
        );
    }

    #[test]
    fn read_disk_usage_matches_root_volume() {
        let (used, total) = Backend::read_disk_usage();
        assert!(total > 0, "根分区总量必须非零");
        let vols = Backend::read_disks();
        let root = vols.iter().find(|v| v.mount == "/").expect("应有 / 卷");
        assert_eq!((root.used_bytes, root.total_bytes), (used, total));
    }

    #[test]
    fn no_pseudo_filesystems_leak_in() {
        for v in Backend::read_disks() {
            assert!(
                !SKIP_MOUNT_PREFIX.iter().any(|p| v.mount.starts_with(p)),
                "伪挂载点泄漏：{}",
                v.mount
            );
        }
    }

    #[test]
    #[ignore]
    fn disk_probe() {
        for v in Backend::read_disks() {
            println!(
                "{} {:<24} total={:>8.1}GB used={:>8.1}GB avail={:>8.1}GB  {:.1}%",
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
