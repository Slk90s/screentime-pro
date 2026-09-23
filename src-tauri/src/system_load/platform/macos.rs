//! system_load/platform/macos.rs
//! macOS 内存/磁盘 FFI（v0.9.7 从 metrics.rs 迁出）。
//!
//! ⚠️ 以下全部保留 v0.7.x 的崩溃修复与布局断言——这是历史踩坑重灾区，勿改布局：
//! - v0.7.8：`sysctlbyname`（按名字）而非 `sysctl`（按 MIB 数组），
//!   否则 hw.memsize 恒 0 → 内存总量恒 0、浮窗 MEM 恒 0%、菜单栏不显示 M 段
//! - v0.7.7：statfs 必须按真实 2168 字节结构体接收（旧 `[0u8; 256]` 栈越界写 → 崩溃）
//! - v0.7.7：vm_statistics64 按字段名读取 + `size_of` 静态断言锁死 152 字节布局
//!   布局若漂移，编译期断言直接失败，而不是用户机器上再次越界写

use crate::system_load::metrics::MetricsInner;
use super::MetricsBackend;

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
    fn host_statistics64(
        host: u32,
        flavor: u32,
        info: *mut u8,
        count: *mut u32,
    ) -> i32;
    fn statfs(path: *const i8, buf: *mut u8) -> i32;
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
    let mut count: u32 =
        (std::mem::size_of::<VmStats64Abi>() / std::mem::size_of::<u32>()) as u32;
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
    let rc = unsafe { statfs(path.as_ptr() as *const i8, &mut info as *mut StatFs as *mut u8) };
    if rc != 0 {
        tracing::warn!(rc, "statfs failed, returning no disk stats");
        return (0, 0);
    }
    let block = info.f_bsize as u64;
    if block == 0 || info.f_blocks == 0 {
        return (0, 0);
    }
    let total = info.f_blocks.saturating_mul(block);
    let free = info.f_bfree.saturating_mul(block);
    let used = total.saturating_sub(free);
    (used, total)
}

#[derive(Debug, Clone)]
struct VmStats64 {
    free_count: u64,
    inactive_count: u64,
    speculative_count: u64,
}
