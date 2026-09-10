//! system_load/metrics.rs
//! 跨平台系统托盘指标采样器（v0.7.5 macOS / v0.7.6 跨平台扩展）。
//!
//! 设计：
//! - 复用 CpuMonitor 的 CPU 采样（零重复代码）
//! - 内存/磁盘采样：macOS 用 host_statistics64 + hw.memsize + statfs（其他平台返回 0，UI 上不显示）
//! - 网络采样：v0.7.6 新增 NetworkSampler，跨平台
//! - 单线程统一调度：1s 采 CPU/内存/网络，磁盘采后缓存避免频繁 statfs
//! - 采样结果直接驱动托盘 title，绕开 IPC 往返，零前端开销
//!
//! macOS 内存口径说明（对齐活动监视器）：
//!   available = (free + inactive + speculative) * page_size
//!   used%     = (total - available) / total
//!   不直接用 free_count（macOS 会用空闲内存做文件缓存，free 经常接近 0）
//!
//! 磁盘口径（对齐 Finder「读取信息」）：
//!   total = f_blocks * f_frsize
//!   free  = f_bfree * f_frsize
//!   used  = total - free
//!
//! 跨平台说明（v0.7.6 引入）：
//! - v0.7.5 整个文件 #[cfg(target_os = "macos")]，导致 MetricsSampler 类型在 Win/Linux 不存在
//! - v0.7.6 拆掉文件级 cfg，FFI 块与 macOS-only 内部函数加 cfg 门
//! - 非 macOS 平台上，memory/disk 字段恒为 0；CPU 与网络三平台一致工作
//! - 托盘 title 根据 StatusBarConfig 动态拼接（见 tray_title_with）

use crate::system_load::CpuMonitor;
use crate::system_load::network::NetworkSampler;
use std::sync::{Arc, Mutex};

// 内存采样仅 macOS 支持（见下方 sample_memory 的 cfg 分支）；其他平台 memory 恒为 0。
// 该常量用于托盘 title 拼接：非 macOS 上即使 show_mem 勾选也**不拼 M0**（无意义的 0%），
// 避免 Windows/Linux 托盘出现 "Cxx M0 ↑.. ↓.."。
#[cfg(target_os = "macos")]
const MEMORY_SUPPORTED: bool = true;
#[cfg(not(target_os = "macos"))]
const MEMORY_SUPPORTED: bool = false;

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
    #[cfg(target_os = "macos")]
    page_size: u64,
    #[cfg(target_os = "macos")]
    memory_total: u64,
    #[cfg(target_os = "macos")]
    last_disk: Option<(u64, u64)>,
    #[cfg(target_os = "macos")]
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
                #[cfg(target_os = "macos")]
                last_disk: None,
                #[cfg(target_os = "macos")]
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
    pub fn tray_title_with(&self, snap: &MetricsSnapshot, cfg: &TrayTitleConfig) -> String {
        if !cfg.enabled {
            return String::new();
        }
        let mut parts: Vec<String> = Vec::new();
        if cfg.show_cpu {
            let cpu = (snap.cpu_usage * 100.0).round() as u32;
            parts.push(format!("C{}", cpu));
        }
        if cfg.show_mem && MEMORY_SUPPORTED {
            let mem = (snap.memory_usage * 100.0).round() as u32;
            parts.push(format!("M{}", mem));
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
    // 内存采样（macOS-only，其他平台返回 (0, 0)）
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
        let stats = unsafe { Self::read_vm_stats() };
        if let Some(st) = stats {
            let avail_pages = st.free_count + st.inactive_count + st.speculative_count;
            let avail_bytes = avail_pages.saturating_mul(page_size);
            let used = memory_total.saturating_sub(avail_bytes);
            (used, memory_total)
        } else {
            (0, memory_total)
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn sample_memory(&self) -> (u64, u64) {
        (0, 0)
    }

    // ============================================================
    // 磁盘采样（macOS-only，其他平台返回 (0, 0)；30s 缓存避免频繁 statfs）
    // ============================================================

    fn sample_disk_cached(&self) -> (u64, u64) {
        #[cfg(target_os = "macos")]
        {
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
            let (used, total) = unsafe { Self::read_disk_usage() };
            if used > 0 && total > 0 {
                if let Ok(mut inner) = self.inner.lock() {
                    inner.last_disk = Some((used, total));
                    inner.last_disk_secs = now_secs;
                }
            }
            (used, total)
        }
        #[cfg(not(target_os = "macos"))]
        {
            (0, 0)
        }
    }

    // ============================================================
    // macOS 专属 FFI（v0.7.5 原有代码，v0.7.6 加 cfg 门）
    // ============================================================

    #[cfg(target_os = "macos")]
    fn init_memory_constants(&self) {
        let (page_size, memory_total) = unsafe {
            (Self::read_page_size(), Self::read_memory_total())
        };
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
    pub show_net: bool,
}

impl From<crate::commands::StatusBarConfig> for TrayTitleConfig {
    fn from(c: crate::commands::StatusBarConfig) -> Self {
        Self {
            enabled: c.enabled,
            show_cpu: c.show_cpu,
            show_mem: c.show_mem,
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
// ============================================================

#[cfg(target_os = "macos")]
extern "C" {
    fn sysctl(
        name: *const i8,
        namelen: u32,
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
    let rc = sysctl(
        name.as_ptr() as *const i8,
        name.len() as u32 - 1,
        &mut page_size as *mut u32 as *mut std::ffi::c_void,
        &mut len,
        std::ptr::null(),
        0,
    );
    if rc == 0 && page_size > 0 {
        page_size as u64
    } else {
        4096
    }
}

#[cfg(target_os = "macos")]
unsafe fn read_memory_total() -> u64 {
    let name = b"hw.memsize\0";
    let mut total: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    let rc = sysctl(
        name.as_ptr() as *const i8,
        name.len() as u32 - 1,
        &mut total as *mut u64 as *mut std::ffi::c_void,
        &mut len,
        std::ptr::null(),
        0,
    );
    if rc == 0 && total > 0 {
        total
    } else {
        0
    }
}

#[cfg(target_os = "macos")]
unsafe fn read_vm_stats() -> Option<VmStats64> {
    const HOST_VM_INFO64: u32 = 4;
    const BUF_U32_COUNT: u32 = 128;
    const BUF_SIZE: usize = (BUF_U32_COUNT as usize) * 4;
    let host = mach_host_self();
    let mut buf = vec![0u8; BUF_SIZE];
    let mut count: u32 = BUF_U32_COUNT;
    let rc = host_statistics64(host, HOST_VM_INFO64, buf.as_mut_ptr(), &mut count);
    if rc != 0 {
        tracing::warn!(rc, "host_statistics64 failed, returning no memory stats");
        return None;
    }
    if count < 36 {
        tracing::warn!(count, "host_statistics64 returned too few fields");
        return None;
    }
    let free = u64::from_le_bytes(buf[8..16].try_into().unwrap_or([0u8; 8]));
    let inactive = u64::from_le_bytes(buf[24..32].try_into().unwrap_or([0u8; 8]));
    let speculative = u64::from_le_bytes(buf[32..40].try_into().unwrap_or([0u8; 8]));
    Some(VmStats64 {
        free_count: free,
        inactive_count: inactive,
        speculative_count: speculative,
    })
}

#[cfg(target_os = "macos")]
unsafe fn read_disk_usage() -> (u64, u64) {
    let mut buf = [0u8; 256];
    let path = b"/\0";
    let rc = statfs(path.as_ptr() as *const i8, buf.as_mut_ptr());
    if rc != 0 {
        return (0, 0);
    }
    let f_blocks = u64::from_le_bytes(buf[8..16].try_into().unwrap_or([0u8; 8]));
    let f_bfree = u64::from_le_bytes(buf[16..24].try_into().unwrap_or([0u8; 8]));
    let f_frsize = u64::from_le_bytes(buf[72..80].try_into().unwrap_or([0u8; 8]));
    if f_frsize == 0 || f_blocks == 0 {
        return (0, 0);
    }
    let total = f_blocks.saturating_mul(f_frsize);
    let free = f_bfree.saturating_mul(f_frsize);
    let used = total.saturating_sub(free);
    (used, total)
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone)]
struct VmStats64 {
    free_count: u64,
    inactive_count: u64,
    speculative_count: u64,
}
