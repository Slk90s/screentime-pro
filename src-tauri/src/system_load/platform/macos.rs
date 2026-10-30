//! system_load/platform/macos.rs
//! macOS 内存/磁盘 FFI（v0.9.7 从 metrics.rs 迁出）。
//!
//! ⚠️ 以下全部保留 v0.7.x 的崩溃修复与布局断言——这是历史踩坑重灾区，勿改布局：
//! - v0.7.8：`sysctlbyname`（按名字）而非 `sysctl`（按 MIB 数组），
//!   否则 hw.memsize 恒 0 → 内存总量恒 0、浮窗 MEM 恒 0%、菜单栏不显示 M 段
//! - v0.7.7：statfs 必须按真实 2168 字节结构体接收（旧 `[0u8; 256]` 栈越界写 → 崩溃）
//! - v0.7.7：vm_statistics64 按字段名读取 + `size_of` 静态断言锁死 152 字节布局
//!   布局若漂移，编译期断言直接失败，而不是用户机器上再次越界写

use super::{DiskVolume, MetricsBackend};
use crate::system_load::metrics::MetricsInner;

pub(crate) struct Backend;

impl MetricsBackend for Backend {
    fn sample_memory(inner: &MetricsInner) -> (u64, u64) {
        let (page_size, memory_total) = (inner.page_size, inner.memory_total);
        // read_vm_stats 是模块级自由函数（见下方），禁止 Self:: 前缀——
        // 该错误在 Windows/Linux 被 cfg 编译掉，本地 cargo check 无法发现，
        // 只有 macOS CI 会以 E0599 暴露（v0.7.5 / v0.7.6 首次打 tag 均因此失败）。
        let stats = unsafe { ffi_read_vm_stats() };
        if let Some(st) = stats {
            let avail_pages = st.free_count + st.inactive_count + st.speculative_count;
            let avail_bytes = avail_pages.saturating_mul(page_size);
            let used = memory_total.saturating_sub(avail_bytes);
            (used, memory_total)
        } else {
            (0, memory_total)
        }
    }

    fn read_disk_usage() -> (u64, u64) {
        ffi_read_disk_usage()
    }

    fn read_disks() -> Vec<DiskVolume> {
        ffi_read_disks()
    }

    fn init_memory_constants(inner: &mut MetricsInner) {
        let (page_size, memory_total) = unsafe { (ffi_read_page_size(), ffi_read_memory_total()) };
        inner.page_size = page_size.max(4096);
        inner.memory_total = memory_total;
    }
}

extern "C" {
    /// ⚠️ v0.7.8 修复：这里必须是 **`sysctlbyname`（按名字查）**，不是 `sysctl`（按 MIB 查）。
    /// `sysctl(2)` 的 `name` 参数是 **`int[]` MIB 数组**（如 `[CTL_HW=6, HW_MEMSIZE=24]`），
    /// 传名字字符串进去会被当成整数字节解释 → 必然失败。
    /// 旧实现声明成 `sysctl` 却传 `b"hw.memsize"`，导致 `memory_total` 恒为 0
    /// （→ 菜单栏不显示 M 段、浮窗 MEM 恒 0%），页大小也一直走 4096 兜底。
    fn sysctlbyname(
        name: *const i8,
        oldp: *mut std::ffi::c_void,
        oldlenp: *mut usize,
        newp: *const std::ffi::c_void,
        newlen: usize,
    ) -> i32;
    fn mach_host_self() -> u32;
    fn host_statistics64(host: u32, flavor: u32, info: *mut u8, count: *mut u32) -> i32;
    fn statfs(path: *const i8, buf: *mut u8) -> i32;
    /// v1.2.12 多盘枚举用。`bufsize` 单位是**字节**（不是条目数），
    /// 传 0 时返回「如果给了足够空间，能放下多少个文件系统」，即总数。
    fn getfsstat(buf: *mut StatFs, bufsize: i32, flags: i32) -> i32;
}

unsafe fn ffi_read_page_size() -> u64 {
    let name = b"hw.pagesize\0";
    let mut page_size: u32 = 0;
    let mut len = std::mem::size_of::<u32>();
    let rc = sysctlbyname(
        name.as_ptr() as *const i8,
        &mut page_size as *mut u32 as *mut std::ffi::c_void,
        &mut len,
        std::ptr::null(),
        0,
    );
    // Apple Silicon 的物理页是 16KB（不是 4KB），读不到会直接让内存占用算错，故只兜底不静默
    if rc == 0 && page_size > 0 {
        page_size as u64
    } else {
        tracing::warn!(rc, "sysctlbyname(hw.pagesize) 失败，回落到 4096");
        4096
    }
}

unsafe fn ffi_read_memory_total() -> u64 {
    let name = b"hw.memsize\0";
    let mut total: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    let rc = sysctlbyname(
        name.as_ptr() as *const i8,
        &mut total as *mut u64 as *mut std::ffi::c_void,
        &mut len,
        std::ptr::null(),
        0,
    );
    if rc == 0 && total > 0 {
        total
    } else {
        tracing::warn!(rc, "sysctlbyname(hw.memsize) 失败，本次内存总量记为 0");
        0
    }
}

/// macOS `struct vm_statistics64`（Darwin，64 位，共 152 字节）
///
/// 只声明到 `speculative_count` 之后的**全部**字段——`host_statistics64` 要求
/// `*count` 不小于 `sizeof(vm_statistics64_data_t)/4 = 38`，否则返回 KERN_FAILURE
/// （不会越界写，但会拿不到数据）。所以这里完整对齐 152 字节。
///
/// ⚠️ v0.7.7：旧实现按手写字节偏移读取，偏移量全错（详见文件头注释）。
/// 现在按字段名读取，并由下方 `size_of` 静态断言锁死布局。
#[repr(C)]
#[derive(Default)]
struct VmStats64Abi {
    free_count: u32,
    active_count: u32,
    inactive_count: u32,
    wire_count: u32,
    zero_fill_count: u64,
    reactivations: u64,
    pageins: u64,
    pageouts: u64,
    faults: u64,
    cow_faults: u64,
    lookups: u64,
    hits: u64,
    purges: u64,
    purgeable_count: u32,
    speculative_count: u32,
    decompressions: u64,
    compressions: u64,
    swapins: u64,
    swapouts: u64,
    compressor_page_count: u32,
    throttled_count: u32,
    external_page_count: u32,
    internal_page_count: u32,
    total_uncompressed_pages_in_compressor: u64,
}

// 编译期护栏：布局漂移 → CI 直接编译失败，而不是用户机器上再次越界写
const _: () = assert!(
    std::mem::size_of::<VmStats64Abi>() == 152,
    "macOS vm_statistics64 布局与本定义不一致，请重新核对 <mach/vm_statistics.h>"
);

unsafe fn ffi_read_vm_stats() -> Option<VmStats64> {
    const HOST_VM_INFO64: u32 = 4;
    let mut stats = VmStats64Abi::default();
    // count 单位是 4 字节 integer_t（不是字节数）
    let mut count: u32 = (std::mem::size_of::<VmStats64Abi>() / std::mem::size_of::<u32>()) as u32;
    let host = mach_host_self();
    let rc = host_statistics64(
        host,
        HOST_VM_INFO64,
        &mut stats as *mut VmStats64Abi as *mut u8,
        &mut count,
    );
    if rc != 0 {
        tracing::warn!(rc, "host_statistics64 failed, returning no memory stats");
        return None;
    }
    Some(VmStats64 {
        free_count: stats.free_count as u64,
        inactive_count: stats.inactive_count as u64,
        speculative_count: stats.speculative_count as u64,
    })
}

/// macOS `struct statfs`（Darwin，64 位，共 **2168** 字节）
///
/// ⚠️ v0.7.7 崩溃根因：`statfs(2)` 没有输出长度参数，内核**无条件写满**这个结构体。
/// 旧实现只给了 `[0u8; 256]` 的栈缓冲 → 越界写 1912 字节 → 栈破坏 → 崩溃。
///
/// 字段与 `<sys/mount.h>` 一一对应；**注意 macOS 没有 `f_frsize`**（Linux 才有），
/// 块大小是 `f_bsize`，容量 = `f_blocks × f_bsize`。
#[repr(C)]
struct StatFs {
    f_bsize: u32,
    f_iosize: i32,
    f_blocks: u64,
    f_bfree: u64,
    f_bavail: u64,
    f_files: u64,
    f_ffree: u64,
    f_fsid: [i32; 2],
    f_owner: u32,
    f_type: u32,
    f_flags: u32,
    f_fssubtype: u32,
    f_fstypename: [u8; 16],
    f_mntonname: [u8; 1024],
    f_mntfromname: [u8; 1024],
    f_reserved: [u32; 8],
}

const _: () = assert!(
    std::mem::size_of::<StatFs>() == 2168,
    "macOS struct statfs 布局与本定义不一致，请重新核对 <sys/mount.h>"
);

fn ffi_read_disk_usage() -> (u64, u64) {
    // 用结构体本身做输出缓冲：大小由 repr(C) 布局保证，不再手写魔法数字
    let mut info: StatFs = unsafe { std::mem::zeroed() };
    let path = b"/\0";
    let rc = unsafe {
        statfs(
            path.as_ptr() as *const i8,
            &mut info as *mut StatFs as *mut u8,
        )
    };
    if rc != 0 {
        tracing::warn!(rc, "statfs failed, returning no disk stats");
        return (0, 0);
    }
    let block = info.f_bsize as u64;
    if block == 0 || info.f_blocks == 0 {
        return (0, 0);
    }
    let total = info.f_blocks.saturating_mul(block);
    // v1.2.12 口径统一：用 f_bavail（**非 root 用户**可用）而非 f_bfree（含 root 保留块
    // 与快照回收空间）。与 Linux 的 statvfs 口径对齐，也与 Finder「显示简介」的
    // 「可用」一栏一致。旧实现用 f_bfree 会让 macOS 显示的已用比 Finder 偏小。
    let avail = info.f_bavail.min(info.f_blocks).saturating_mul(block);
    (total.saturating_sub(avail), total)
}

/// macOS 上要跳过的伪文件系统（`f_fstypename`）。
///
/// APFS 的结构特点：同一容器的多个卷（`/`、`/System/Volumes/Data`、`/private/var/vm`…）
/// **各自 statfs 会返回同一个容器容量**。若全都列出来，用户会看到「3 个 500GB 的盘」，
/// 实际只有 500GB。所以这里必须按文件系统类型 + 挂载点双重过滤。
///
/// 保留规则：只留 `apfs` / `hfs` / `exfat` / `msdos` / `ntfs`，
/// 且挂载点**不以** `/System/Volumes/` 开头（排除 Data/VM/Preboot/Update 等 APFS 子卷）、
/// 不以 `/private/` 开头（排除虚拟内存卷）。
const SKIP_FS_TYPES: &[&str] = &[
    "devfs", "autofs", "map", "tmpfs", "procfs", "kernfs", "volfs", "fdesc", "nullfs",
];

fn cstr_to_string(buf: &[u8]) -> String {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

/// 这个挂载点该不该进「本机磁盘」列表（v1.2.11 整改 0.5；抽成纯函数是为了能单测）。
///
/// 三道判据，顺序即理由：
/// 1. **文件系统类型**：先按 `SKIP_FS_TYPES` 排伪文件系统，再要求落在真实磁盘格式白名单里
///    （nfs/smbfs/webdav 这类网络卷的 statfs 可能阻塞，且不属于「本机存储」）；
/// 2. **挂载点前缀**：`/System/Volumes/*` 与 `/private/*` 都是**同一 APFS 容器**的其它视图，
///    statfs 数字与 `/` 一模一样，全列出来用户会看到「3 个 500GB 的盘」；
/// 3. **只读标志**：Time Machine 本地快照会挂成只读卷，容量与主卷重复 → 排除。
///
/// ⚠️ 第 3 条**必须给 `/` 开后门**（这正是整改 0.5 修的东西）：macOS 11 (Big Sur) 起
/// 系统卷是**已签名只读的密封快照**，`statfs("/")` 恒带 `MNT_RDONLY`。旧实现无差别地
/// 「只读即丢弃」，于是内置磁盘从列表里**整个消失**——`is_system` 永远为 `false`，
/// 而 `read_disk_usage()`（它本身就是 `statfs("/")`）与「`read_disks()` 里
/// `is_system=true` 那一项恒等」的契约（见 `platform/mod.rs`）也一起落空。
/// 外接过只读卷、快照卷仍按原规则排除。
fn macos_volume_selected(fs_type: &str, mount: &str, flags: u32) -> bool {
    const MNT_RDONLY: u32 = 0x0000_0001;
    if SKIP_FS_TYPES.contains(&fs_type) {
        return false;
    }
    if !matches!(fs_type, "apfs" | "hfs" | "exfat" | "msdos" | "ntfs") {
        return false;
    }
    if mount.starts_with("/System/Volumes/") || mount.starts_with("/private/") {
        return false;
    }
    mount == "/" || flags & MNT_RDONLY == 0
}

/// 枚举本地固定卷。
///
/// 实现选择：用 `getfsstat` 两次调用（第一次问数量、第二次取数据）而不是逐个 `statfs`。
/// 逐盘 `statfs` 需要先知道挂载点列表，那本身就要枚举；`getfsstat` 一步到位。
///
/// ⚠️ `getfsstat` 的 `bufsize` 参数是**字节数**，且结构体是 **2168 字节**（见 `StatFs` 断言）。
/// 传 `[0u8; 256]` 之类的裸缓冲会越界写（v0.7.7 崩溃根因），这里用 `Vec<StatFs>` 保证尺寸。
#[allow(dead_code)] // v1.2.13 接入多盘调用方后删除（详见 platform/mod.rs 的 DiskVolume 文档）
fn ffi_read_disks() -> Vec<DiskVolume> {
    const MNT_NOWAIT: i32 = 2;

    // 第一次：传 null 取数量
    let count = unsafe { getfsstat(std::ptr::null_mut(), 0, MNT_NOWAIT) };
    if count <= 0 {
        return Vec::new();
    }
    // 留一点余量：两次调用之间可能有挂载变化
    let cap = (count as usize).saturating_add(8);
    let mut buf: Vec<StatFs> = Vec::with_capacity(cap);
    let bufsize = (cap * std::mem::size_of::<StatFs>()) as i32;
    let n = unsafe { getfsstat(buf.as_mut_ptr(), bufsize, MNT_NOWAIT) };
    if n <= 0 {
        return Vec::new();
    }
    let n = (n as usize).min(cap);
    // SAFETY: getfsstat 已成功写入前 n 项，且 cap 不小于 n
    unsafe { buf.set_len(n) };

    let mut out = Vec::new();
    for info in &buf {
        let fs_type = cstr_to_string(&info.f_fstypename);
        let mount = cstr_to_string(&info.f_mntonname);
        if !macos_volume_selected(&fs_type, &mount, info.f_flags) {
            continue;
        }
        let block = info.f_bsize as u64;
        if block == 0 || info.f_blocks == 0 {
            continue;
        }
        let total = info.f_blocks.saturating_mul(block);
        if total == 0 {
            continue;
        }
        let avail = info.f_bavail.min(info.f_blocks).saturating_mul(block);
        out.push(DiskVolume {
            is_system: mount == "/",
            mount,
            total_bytes: total,
            used_bytes: total.saturating_sub(avail),
            avail_bytes: avail,
        });
    }
    // 兜底（与 linux.rs 同构，v1.2.11 整改 0.5）：过滤后一个不剩时至少给出系统卷。
    // 消费方（浮窗 / 多盘卡片）要的是「系统盘」这一项，空列表等于功能整体消失；
    // 而这里已经有过一次「过滤器把 `/` 自己滤掉」的真实事故，不留这条兜底就是
    // 等下一次规则改动重演。
    if out.is_empty() {
        let (used, total) = ffi_read_disk_usage();
        if total > 0 {
            out.push(DiskVolume {
                is_system: true,
                mount: "/".to_string(),
                total_bytes: total,
                used_bytes: used,
                avail_bytes: total.saturating_sub(used),
            });
        }
    }
    out.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));
    out
}

#[derive(Debug, Clone)]
struct VmStats64 {
    free_count: u64,
    inactive_count: u64,
    speculative_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `MNT_RDONLY`（sys/mount.h）。系统卷在 Big Sur 起恒带这个位。
    const RO: u32 = 0x0000_0001;

    /// 🐞 回归（P0，2026-10-29 @v1.2.11 整改 0.5）：**只读的系统卷必须留在列表里**。
    ///
    /// macOS 11+ 的 `/` 是已签名只读的密封系统卷，`statfs` 一定带 `MNT_RDONLY`。
    /// 旧过滤「只读一律丢」把它连同 `/System/Volumes/Data`（被前缀规则挡掉）一起排除，
    /// 内置磁盘于是**整块消失**：`read_disks()` 里没有 `is_system = true` 那一项，
    /// 而 `platform/mod.rs` 承诺 `read_disk_usage()` 的结果恒等于那一项 —— 契约落空，
    /// 多盘卡片也不会显示系统盘。
    ///
    /// ⚠️ 本模块只在 macOS 参与编译，Windows 本机与 CI（不跑 `cargo test`）都验不到，
    /// 需在 Apple 机器上跑 `cargo test --lib system_load::platform::macos`。
    #[test]
    fn sealed_read_only_system_volume_is_kept() {
        assert!(
            macos_volume_selected("apfs", "/", RO),
            "只读的密封系统卷必须保留：它是唯一与 read_disk_usage() 对应的挂载点"
        );
        assert!(
            macos_volume_selected("apfs", "/", 0),
            "macOS 10 的可写形态同样保留"
        );
        assert!(
            macos_volume_selected("hfs", "/", RO),
            "老式 HFS+ 装的系统也一样只读挂载"
        );
    }

    /// 上面那个后门不能带偏其它判据：同一 APFS 容器的其它视图、只读快照、
    /// 网络卷与伪文件系统都要留在门外；可写的真实卷要能进来。
    #[test]
    fn duplicate_snapshot_and_network_volumes_stay_out() {
        let excluded = [
            ("apfs", "/System/Volumes/Data", 0u32), // 同一容器，容量与 / 重复
            ("apfs", "/System/Volumes/Preboot", RO),
            ("apfs", "/private/var/vm", 0),       // 交换分区
            ("apfs", "/Volumes/TM-Snapshot", RO), // Time Machine 本地快照
            ("nfs", "/Volumes/nas", 0),
            ("smbfs", "/Volumes/share", 0),
            ("webdav", "/Volumes/cloud", 0),
            ("devfs", "/dev", 0),
            ("tmpfs", "/tmp", 0),
            ("apfs", "/System/Volumes/Data/private", 0),
        ];
        for (fs_type, mount, flags) in excluded {
            assert!(
                !macos_volume_selected(fs_type, mount, flags),
                "{fs_type} @ {mount}（flags={flags}）不该进「本机磁盘」列表"
            );
        }
        let included = [
            ("apfs", "/Volumes/USB", 0u32),
            ("msdos", "/Volumes/SD", 0),
            ("exfat", "/Volumes/Ex", 0),
            ("ntfs", "/Volumes/Win", 0),
        ];
        for (fs_type, mount, flags) in included {
            assert!(
                macos_volume_selected(fs_type, mount, flags),
                "{fs_type} @ {mount} 是可写的真实外置卷，必须进来"
            );
        }
    }
}
