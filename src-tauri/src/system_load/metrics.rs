//! system_load/metrics.rs
//! 跨平台系统指标采样器（v0.7.5 引入 → v0.7.7 补齐全平台内存/磁盘）。
//!
//! 设计：
//! - 复用 CpuMonitor 的 CPU 采样（零重复代码）
//! - 网络采样：v0.7.6 新增 NetworkSampler，跨平台
//! - 单线程统一调度：1s 采 CPU/内存/网络，磁盘采后缓存避免频繁枚举
//! - 采样结果直接驱动托盘 title / 托盘图标 / 悬浮指标条，绕开往返开销
//!
//! ## 内存口径（对齐各平台「活动监视器 / 任务管理器」）
//! - macOS：available = (free + inactive + speculative) × page_size；used = total - available
//!   （不直接用 free_count——macOS 会用空闲内存做文件缓存，free 经常接近 0）
//! - Windows：GlobalMemoryStatusEx → used = ullTotalPhys - ullAvailPhys
//! - Linux：/proc/meminfo → used = MemTotal - MemAvailable
//!
//! ## 磁盘口径（对齐 Finder「显示简介」/ Windows「此电脑」/ df）
//! - macOS：statfs("/") → total = f_blocks × f_bsize（**macOS 没有 f_frsize**）
//! - Windows：GetDiskFreeSpaceExW(系统盘) → used = total - totalFree
//! - Linux：statvfs("/") → total = f_blocks × f_frsize，free 用 f_bavail（非 root 可用量）
//!
//! ## 平台支持矩阵（v0.7.7 起）
//! | 指标 | macOS | Windows | Linux |
//! | ---- | ----- | ------- | ----- |
//! | CPU  | ✅ Mach host_statistics(HOST_CPU_LOAD_INFO) | ✅ GetSystemTimes | ✅ /proc/stat |
//! | 内存 | ✅ host_statistics64 + sysctlbyname(hw.memsize) | ✅ GlobalMemoryStatusEx | ✅ /proc/meminfo |
//! | 磁盘 | ✅ statfs | ✅ GetDiskFreeSpaceExW | ✅ statvfs |
//! | 网络 | ✅ getifaddrs | ✅ GetIfTable2 | ✅ /proc/net/dev |
//!
//! ⚠️ v0.7.8（2026-09-11）macOS「CPU / 内存恒 0%」连环根因修复：
//!   1. **CPU**：`kern.cp_time` **在 macOS 上不存在**（那是 BSD 的 OID），
//!      Darwin 只能走 Mach `host_statistics(HOST_CPU_LOAD_INFO)`——详见 `macos.rs`。
//!   2. **内存**：本文件旧代码把 `sysctl()` 当 `sysctlbyname()` 用（把名字字符串传给
//!      需要 **MIB 整型数组** 的 `sysctl(2)`）→ `hw.memsize` 永远读失败 →
//!      `memory_total` 恒 0 → 浮窗 MEM 恒 0%、菜单栏干脆不显示 M 段；
//!      `hw.pagesize` 也一直走 4096 兜底（Apple Silicon 实为 16384，会放大误差）。
//!      现已改为正确的 `sysctlbyname(3)`，并在失败时 `tracing::warn` 留痕。
//!
//! v0.7.6 及以前：内存/磁盘被文件级思路限制为「macOS-only」，非 macOS 恒返回 (0,0)，
//! 且托盘 title 用**编译期常量** MEMORY_SUPPORTED 决定是否拼 M 段。v0.7.7 改为
//! **运行时判据**（`memory_total_bytes > 0`）——采样失败/平台不支持时自动不显示，
//! 不再需要用常量硬编码平台矩阵。
//!
//! ⚠️ v0.7.6 hotfix（2026-09-10，macOS CI E0599）：文件尾部的 read_page_size /
//!    read_memory_total / read_vm_stats / read_disk_usage 是【模块级自由函数】，
//!    impl 内调用禁止加 Self:: 前缀。该错误仅 macOS 编译路径可见（其余平台被
//!    cfg 编译掉），本地 cargo check 无法拦截——改 macOS-only 代码后必须看 CI。
//!
//! ⚠️⚠️ v0.7.7 崩溃修复（2026-09-10，macOS 打开后闪退）——根因是**栈越界写**：
//!    `read_disk_usage()` 旧实现用 `let mut buf = [0u8; 256]` 接收 `statfs(2)` 输出，
//!    但 Darwin 的 `struct statfs` 是 **2168 字节**（含 f_mntonname / f_mntfromname
//!    各 1024 字节），且 `statfs(2)` **没有长度参数**——内核无条件写满 2168 字节，
//!    于是往 Rust 栈上越界写 1912 字节 → 栈破坏 → SIGSEGV / SIGABRT（`.ips` 崩溃报告）。
//!    触发条件：任何一次 `sample_all()`（启用状态栏后的托盘采样线程，或悬浮指标条
//!    前端 FloatBar 的 1Hz `get_system_metrics`）。Windows/Linux 上
//!    `sample_disk_cached()` 当时直接返回 (0,0)、**从不调用 statfs**，所以这个 bug 只炸 macOS，
//!    本地 Windows 开发 + `cargo check` 全绿也发现不了。
//!    同一轮修复还纠正了两处**字段偏移错误**（数值错但不崩）：
//!      - `vm_statistics64`：free/inactive/speculative 应取 0/8/92 字节处，
//!        旧代码取的是 8/24/32（实际是 inactive+wire / reactivations / pageins）
//!      - `statfs`：macOS **没有 `f_frsize`**（那是 Linux 字段），旧代码读的 72..80
//!        其实是 `f_fstypename`（字符串），容量应为 `f_blocks × f_bsize`
//!    两处现在都改为「真实布局的 repr(C) 结构体 + `size_of` 静态断言」，布局若变 CI 直接报错。
//!
//! ⚠️ v0.7.7 教训 2：**托盘/浮窗显示的百分比，Rust 侧一律用 0.0~1.0 分数**，
//!    前端展示时必须 ×100。FloatBar.vue 曾漏乘导致「77% 显示成 1%」。

use crate::system_load::CpuMonitor;
use crate::system_load::network::NetworkSampler;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Default)]
pub struct MetricsSnapshot {
    pub cpu_usage: f32,
    pub memory_usage: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub disk_usage: f32,
    pub disk_used_bytes: u64,
    pub disk_total_bytes: u64,
    /// v0.7.6 新增：网络接收速率（bytes/sec）
    pub net_rx_bps: f64,
    /// v0.7.6 新增：网络发送速率（bytes/sec）
    pub net_tx_bps: f64,
}

pub struct MetricsSampler {
    cpu: Arc<CpuMonitor>,
    network: NetworkSampler,
    inner: Mutex<MetricsInner>,
}

struct MetricsInner {
    /// macOS 专用：物理页大小（sysctl hw.pagesize 一次读取）
    #[cfg(target_os = "macos")]
    page_size: u64,
    /// macOS 专用：物理内存总量（sysctl hw.memsize 一次读取）
    #[cfg(target_os = "macos")]
    memory_total: u64,
    /// 磁盘容量缓存（30s 有效）——v0.7.7 起跨平台（Windows/Linux 同样需要缓存）
    last_disk: Option<(u64, u64)>,
    last_disk_secs: u64,
    last_snapshot: Option<MetricsSnapshot>,
}

impl MetricsSampler {
    pub fn new(cpu: Arc<CpuMonitor>) -> Self {
        Self {
            cpu,
            network: NetworkSampler::new(),
            inner: Mutex::new(MetricsInner {
                #[cfg(target_os = "macos")]
                page_size: 4096,
                #[cfg(target_os = "macos")]
                memory_total: 0,
                last_disk: None,
                last_disk_secs: 0,
                last_snapshot: None,
            }),
        }
    }

    /// 预热一次（让 CpuMonitor / NetworkSampler 拿到首帧基准）
    pub fn warmup(&self) {
        let _ = self.cpu.cpu_usage();
        let _ = self.network.sample_rate();
    }

    pub fn sample_all(&self) -> MetricsSnapshot {
        let cpu_usage = self.cpu.cpu_usage().unwrap_or(0.0);
        let (mem_used, mem_total) = self.sample_memory();
        let mem_pct = if mem_total > 0 {
            (mem_used as f64 / mem_total as f64).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let (disk_used, disk_total) = self.sample_disk_cached();
        let disk_pct = if disk_total > 0 {
            (disk_used as f64 / disk_total as f64).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let net = self.network.sample_rate();
        MetricsSnapshot {
            cpu_usage: cpu_usage.clamp(0.0, 1.0),
            memory_usage: mem_pct,
            memory_used_bytes: mem_used,
            memory_total_bytes: mem_total,
            disk_usage: disk_pct,
            disk_used_bytes: disk_used,
            disk_total_bytes: disk_total,
            net_rx_bps: net.rx_bps,
            net_tx_bps: net.tx_bps,
        }
    }

    /// 拼装托盘 title；v0.7.6 起支持按 config 选择显示哪些项
    ///
    /// v0.7.7：内存段由「编译期平台常量」改为**运行时判据**（`memory_total_bytes > 0`），
    /// 采样失败（系统调用被拒）时自动隐藏，不再出现无意义的 "M0"。
    pub fn tray_title_with(&self, snap: &MetricsSnapshot, cfg: &TrayTitleConfig) -> String {
        if !cfg.enabled {
            return String::new();
        }
        let mut parts: Vec<String> = Vec::new();
        if cfg.show_cpu {
            let cpu = (snap.cpu_usage * 100.0).round() as u32;
            parts.push(format!("C{}", cpu));
        }
        if cfg.show_mem && snap.memory_total_bytes > 0 {
            let mem = (snap.memory_usage * 100.0).round() as u32;
            parts.push(format!("M{}", mem));
        }
        if cfg.show_disk && snap.disk_total_bytes > 0 {
            let disk = (snap.disk_usage * 100.0).round() as u32;
            parts.push(format!("D{}", disk));
        }
        if cfg.show_net {
            // 方向语义：rx=下行/接收(in)，tx=上行/发送(out)。通用惯例 下载↓ 上传↑。
            parts.push(format!(
                "↓{} ↑{}",
                format_bps(snap.net_rx_bps),
                format_bps(snap.net_tx_bps)
            ));
        }
        parts.join(" ")
    }

    /// 兼容旧调用（v0.7.5 形式）：三项全显示
    pub fn tray_title(&self, snap: &MetricsSnapshot) -> String {
        self.tray_title_with(
            snap,
            &TrayTitleConfig {
                enabled: true,
                show_cpu: true,
                show_mem: true,
                show_disk: false,
                show_net: true,
            },
        )
    }

    pub fn should_update(&self, snap: &MetricsSnapshot) -> bool {
        let inner = match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(ref last) = inner.last_snapshot {
            let d_cpu = ((snap.cpu_usage - last.cpu_usage).abs() * 100.0).round() as u32;
            let d_mem = ((snap.memory_usage - last.memory_usage).abs() * 100.0).round() as u32;
            // 网速差 > 2KB/s 才视为变化（避免空闲态 0.1K 抖动引发频繁重绘）
            let d_net_rx = (snap.net_rx_bps - last.net_rx_bps).abs() as u64;
            let d_net_tx = (snap.net_tx_bps - last.net_tx_bps).abs() as u64;
            if d_cpu < 1 && d_mem < 1 && d_net_rx < 2048 && d_net_tx < 2048 {
                return false;
            }
        }
        true
    }

    pub fn cache_snapshot(&self, snap: MetricsSnapshot) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.last_snapshot = Some(snap);
        }
    }

    // ============================================================
    // 内存采样（v0.7.7 起三平台齐备）
    // ============================================================

    #[cfg(target_os = "macos")]
    fn sample_memory(&self) -> (u64, u64) {
        let (page_size, memory_total) = match self.inner.lock() {
            Ok(inner) => (inner.page_size, inner.memory_total),
            Err(poisoned) => {
                let inner = poisoned.into_inner();
                (inner.page_size, inner.memory_total)
            }
        };
        // v0.7.6 hotfix（macOS CI E0599）：read_vm_stats 是模块级自由函数（见文件尾部），
        // 不在 impl 内，禁用 Self:: 前缀——Windows/Linux 路径 cfg 掉该调用，本地 check 测不出
        let stats = unsafe { read_vm_stats() };
        if let Some(st) = stats {
            let avail_pages = st.free_count + st.inactive_count + st.speculative_count;
            let avail_bytes = avail_pages.saturating_mul(page_size);
            let used = memory_total.saturating_sub(avail_bytes);
            (used, memory_total)
        } else {
            (0, memory_total)
        }
    }

    #[cfg(target_os = "windows")]
    fn sample_memory(&self) -> (u64, u64) {
        read_windows_memory()
    }

    #[cfg(target_os = "linux")]
    fn sample_memory(&self) -> (u64, u64) {
        read_linux_memory()
    }

    // ============================================================
    // 磁盘采样（v0.7.7 起三平台齐备；30s 缓存避免频繁枚举挂载点）
    // ============================================================

    fn sample_disk_cached(&self) -> (u64, u64) {
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        {
            let inner = match self.inner.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            if let (Some((used, total)), last_secs) = (inner.last_disk, inner.last_disk_secs) {
                if now_secs.saturating_sub(last_secs) < 30 {
                    return (used, total);
                }
            }
        }
        let (used, total) = read_disk_usage();
        if used > 0 && total > 0 {
            if let Ok(mut inner) = self.inner.lock() {
                inner.last_disk = Some((used, total));
                inner.last_disk_secs = now_secs;
            }
        }
        (used, total)
    }

    // ============================================================
    // macOS 专属：内存常量初始化（Windows/Linux 由系统调用直接给出）
    // ============================================================

    #[cfg(target_os = "macos")]
    fn init_memory_constants(&self) {
        let (page_size, memory_total) = unsafe { (read_page_size(), read_memory_total()) };
        if let Ok(mut inner) = self.inner.lock() {
            inner.page_size = page_size.max(4096);
            inner.memory_total = memory_total;
        }
    }

    /// v0.7.5 公开 `new` 时一次性初始化内存常量；v0.7.6 改为在首次 sample_all 之前调用 init
    pub fn init(&self) {
        #[cfg(target_os = "macos")]
        {
            self.init_memory_constants();
        }
    }
}

// ============================================================
// 托盘 title 配置（v0.7.6 引入；与 StatusBarConfig 解耦，sampler 不依赖 settings 表）
// ============================================================

#[derive(Debug, Clone, Copy, Default)]
pub struct TrayTitleConfig {
    pub enabled: bool,
    pub show_cpu: bool,
    pub show_mem: bool,
    /// v0.7.7 新增：磁盘占用
    pub show_disk: bool,
    pub show_net: bool,
}

impl From<crate::commands::StatusBarConfig> for TrayTitleConfig {
    fn from(c: crate::commands::StatusBarConfig) -> Self {
        Self {
            enabled: c.enabled,
            show_cpu: c.show_cpu,
            show_mem: c.show_mem,
            show_disk: c.show_disk,
            show_net: c.show_net,
        }
    }
}

// ============================================================
// 字节/秒格式化（K / M / G）
// ============================================================

fn format_bps(bps: f64) -> String {
    if !bps.is_finite() || bps < 0.0 {
        return "0B".to_string();
    }
    if bps < 1024.0 {
        return format!("{}B", bps.round() as u64);
    }
    if bps < 1024.0 * 1024.0 {
        return format!("{:.1}K", bps / 1024.0);
    }
    if bps < 1024.0 * 1024.0 * 1024.0 {
        return format!("{:.1}M", bps / 1024.0 / 1024.0);
    }
    format!("{:.1}G", bps / 1024.0 / 1024.0 / 1024.0)
}

// ============================================================
// macOS 专属 FFI（v0.7.5 原有代码迁移至 impl 外部，加 cfg 门）
// ⚠️ v0.7.6 hotfix 教训：这里是【模块级自由函数】不是关联函数——
//    impl 内调用必须直接写 read_vm_stats(...)，禁止 Self::read_vm_stats(...)；
//    该错误在 Windows/Linux 被 cfg 编译掉，本地 cargo check 无法发现，
//    只有 macOS CI 会以 E0599 暴露（v0.7.5 / v0.7.6 首次打 tag 均因此失败）。
// ============================================================

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
unsafe fn read_page_size() -> u64 {
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

#[cfg(target_os = "macos")]
unsafe fn read_memory_total() -> u64 {
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
#[cfg(target_os = "macos")]
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
#[cfg(target_os = "macos")]
const _: () = assert!(
    std::mem::size_of::<VmStats64Abi>() == 152,
    "macOS vm_statistics64 布局与本定义不一致，请重新核对 <mach/vm_statistics.h>"
);

#[cfg(target_os = "macos")]
unsafe fn read_vm_stats() -> Option<VmStats64> {
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
#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
const _: () = assert!(
    std::mem::size_of::<StatFs>() == 2168,
    "macOS struct statfs 布局与本定义不一致，请重新核对 <sys/mount.h>"
);

#[cfg(target_os = "macos")]
fn read_disk_usage() -> (u64, u64) {
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

// ============================================================
// Windows 专属：内存 / 磁盘（v0.7.7 新增）
//
// 为什么之前没有：v0.7.5 把内存/磁盘整体锁死在 macOS 分支，Windows 恒返回 (0,0)，
// 于是浮窗/托盘上内存永远是 0%、也从来没有磁盘。这里用 Win32 原生 API 补齐：
//   - 内存：GlobalMemoryStatusEx（比读 WMI/perf counter 快且零依赖）
//   - 磁盘：GetDiskFreeSpaceExW 取**系统盘**（%SystemDrive%，通常 C:）
//     —— 与资源管理器「此电脑」中系统盘的口径一致
// 两者都属 Win32 系统调用，无需 proc::hidden（不创建进程，不会有黑框）。
// ============================================================

#[cfg(target_os = "windows")]
fn read_windows_memory() -> (u64, u64) {
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

#[cfg(target_os = "windows")]
fn read_disk_usage() -> (u64, u64) {
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

// ============================================================
// Linux 专属：内存 / 磁盘（v0.7.7 新增）
//   - 内存：/proc/meminfo 的 MemTotal / MemAvailable（纯 std 读文件，零 FFI）
//   - 磁盘：libc::statvfs("/")，容量 = f_blocks × f_frsize，可用取 f_bavail
//     （f_bavail 才是非 root 用户真正可写的量，与 `df -h` 的 Avail 列一致）
// ============================================================

#[cfg(target_os = "linux")]
fn read_linux_memory() -> (u64, u64) {
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

#[cfg(target_os = "linux")]
fn read_disk_usage() -> (u64, u64) {
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    let path = b"/\0";
    // 用 libc::c_char 而非写死 i8——aarch64-linux 上 c_char 是 u8（同 macOS 的差异点）
    let rc = unsafe {
        libc::statvfs(path.as_ptr() as *const libc::c_char, &mut st)
    };
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

#[cfg(target_os = "macos")]
#[derive(Debug, Clone)]
struct VmStats64 {
    free_count: u64,
    inactive_count: u64,
    speculative_count: u64,
}

// ============================================================
// 单元测试（v0.7.7 新增）
// 目的：内存/磁盘/CPU 采样属于「跨平台 FFI」，出错时不会崩、只会静默给出错误数字，
// 而这正是本项目连续踩坑的地方（statfs 越界写、vm_statistics 偏移错、cp_time 宽度错）。
// 这里给出**最低成本的分辨率测试**：值必须落在物理合理区间。
// 运行：cd src-tauri && cargo test --lib system_load
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bps_scales_and_clamps() {
        assert_eq!(format_bps(0.0), "0B");
        assert_eq!(format_bps(-5.0), "0B");
        assert_eq!(format_bps(512.0), "512B");
        assert_eq!(format_bps(2048.0), "2.0K");
        assert_eq!(format_bps(3.0 * 1024.0 * 1024.0), "3.0M");
    }

    #[test]
    fn cpu_usage_is_a_fraction() {
        let cpu = CpuMonitor::new();
        let _ = cpu.cpu_usage(); // 首帧只建立基准，返回 None
        std::thread::sleep(std::time::Duration::from_millis(300));
        let v = cpu.cpu_usage().expect("第二次采样应能算出差值");
        assert!(
            (0.0..=1.0).contains(&v),
            "CPU 使用率必须是 0.0~1.0 的**分数**（前端负责 ×100），实际 {v}"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_memory_is_physically_sane() {
        let (used, total) = read_windows_memory();
        assert!(total > 0, "GlobalMemoryStatusEx 应给出非零物理内存总量");
        assert!(used <= total, "已用内存不应超过总量（used={used} total={total}）");
        // 物理内存不可能小于 256MB 或大于 4TB，越界说明字段读错
        assert!(
            (256 * 1024 * 1024..=4u64 * 1024 * 1024 * 1024 * 1024).contains(&total),
            "物理内存总量不在合理范围：{total}"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_disk_is_physically_sane() {
        let (used, total) = read_disk_usage();
        assert!(total > 0, "GetDiskFreeSpaceExW 应给出非零系统盘容量");
        assert!(used <= total, "已用不应超过总量（used={used} total={total}）");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_memory_and_disk_are_physically_sane() {
        // 先按生产路径初始化页大小/内存总量，否则 memory_total 恒为 0
        let sampler = MetricsSampler::new(Arc::new(CpuMonitor::new()));
        sampler.init();
        let (mem_used, mem_total) = sampler.sample_memory();
        assert!(mem_total > 0, "hw.memsize 应给出非零物理内存总量");
        assert!(mem_used <= mem_total, "已用内存不应超过总量");
        let (disk_used, disk_total) = read_disk_usage();
        assert!(disk_total > 0, "statfs 应给出非零根卷容量");
        assert!(disk_used <= disk_total, "已用不应超过总量");
    }

    /// 仅用于人工核对读数（cargo test --lib system_load -- --nocapture --ignored probe）
    #[test]
    #[ignore]
    fn probe_real_metrics() {
        let sampler = MetricsSampler::new(Arc::new(CpuMonitor::new()));
        sampler.init();
        sampler.warmup();
        for _ in 0..3 {
            std::thread::sleep(std::time::Duration::from_millis(1000));
            let s = sampler.sample_all();
            println!(
                "CPU={:.2}%  MEM={:.2}% ({}/{} GB)  DISK={:.2}% ({}/{} GB)  NET=↓{:.1} ↑{:.1} B/s",
                s.cpu_usage * 100.0,
                s.memory_usage * 100.0,
                s.memory_used_bytes / 1024 / 1024 / 1024,
                s.memory_total_bytes / 1024 / 1024 / 1024,
                s.disk_usage * 100.0,
                s.disk_used_bytes / 1024 / 1024 / 1024,
                s.disk_total_bytes / 1024 / 1024 / 1024,
                s.net_rx_bps,
                s.net_tx_bps,
            );
        }
    }
}
