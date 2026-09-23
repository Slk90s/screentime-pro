//! system_load/metrics.rs
//! 跨平台系统指标采样器（v0.7.5 引入 → v0.7.7 补齐全平台内存/磁盘）。
//!
//! 设计：
//! - 复用 CpuMonitor 的 CPU 采样（零重复代码）
//! - 网络采样：v0.7.6 新增 NetworkSampler，跨平台
//! - 单线程统一调度：1s 采 CPU/内存/网络，磁盘采后缓存避免频繁枚举
//! - 采样结果只驱动「悬浮指标条」——float 窗口前端（FloatBar.vue）1Hz 调
//!   `get_system_metrics` 拉取原始 MetricsSnapshot 自行渲染；Rust 侧不再拼文字。
//!
//! ## 跨平台后端（v0.9.7 拆分）
//! 内存/磁盘的平台差异实现（含 macOS FFI / Windows Win32 / Linux /proc）已迁到
//! `system_load/platform/{macos,windows,linux}.rs`，统一实现 `platform::MetricsBackend`
//! trait；本文件只保留 `MetricsSampler` 的统一调度，不再在 impl 内堆
//! `#[cfg(target_os=...)]` 分支。平台代码独立成文件后，改某一端不会误伤另一端，
//! 也更便于定位历史坑。各平台具体的崩溃修复与布局断言见对应 platform 文件头部。
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
//! ⚠️ v0.7.11（2026-09-13）：macOS「菜单栏文字指标」整套移除——原
//! `TrayTitleConfig` / `tray_title_with` / `tray_title` / `should_update` /
//! `cache_snapshot` / `format_bps` 及 `MetricsInner.last_snapshot` 已删除。
//! 三端统一只用悬浮指标条。旧约定（macOS 用 `tray.set_title` 显示指标）作废。
//!
//! v0.7.6 及以前：内存/磁盘被文件级思路限制为「macOS-only」，非 macOS 恒返回 (0,0)，
//! 且托盘 title 用**编译期常量** MEMORY_SUPPORTED 决定是否拼 M 段。v0.7.7 改为
//! **运行时判据**（`memory_total_bytes > 0`）——采样失败/平台不支持时自动不显示，
//! 不再需要用常量硬编码平台矩阵。
//!
//! ⚠️ v0.7.7 教训：托盘/浮窗显示的百分比，Rust 侧一律用 0.0~1.0 分数，
//!    前端展示时必须 ×100。FloatBar.vue 曾漏乘导致「77% 显示成 1%」。

use crate::system_load::CpuMonitor;
use crate::system_load::network::NetworkSampler;
use crate::system_load::platform::{Backend, MetricsBackend};
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

/// 采样器内部状态。
/// - `page_size` / `memory_total`：仅 macOS 需要（sysctl 一次读入的常量）；
///   其余平台由系统调用直接给出，字段不存在。
/// - `last_disk*`：磁盘容量缓存（30s 有效），三平台共用。
pub(crate) struct MetricsInner {
    #[cfg(target_os = "macos")]
    pub(crate) page_size: u64,
    #[cfg(target_os = "macos")]
    pub(crate) memory_total: u64,
    pub(crate) last_disk: Option<(u64, u64)>,
    pub(crate) last_disk_secs: u64,
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

    // v0.7.11（2026-09-13）：原 tray_title_with / tray_title / should_update / cache_snapshot
    // 四个方法随 macOS 菜单栏文字指标（tray.set_title）**整体移除**——三端已统一只用
    // 「悬浮指标条」展示系统指标，不再有任何平台把指标拼成文字写进托盘/菜单栏。
    // ⚠️ 不要在此重新引入「拼文字给托盘/菜单栏」的逻辑；指标展示只走 float_window。

    // ============================================================
    // 内存采样（v0.9.7 起统一委托给平台后端，本方法不再有 cfg 分支）
    // ============================================================

    /// 锁 inner、交给当前平台后端采样内存（macOS 需要 inner 里的 page_size/memory_total）
    fn sample_memory(&self) -> (u64, u64) {
        match self.inner.lock() {
            Ok(inner) => Backend::sample_memory(&inner),
            Err(poisoned) => Backend::sample_memory(&poisoned.into_inner()),
        }
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
        let (used, total) = Backend::read_disk_usage();
        if used > 0 && total > 0 {
            if let Ok(mut inner) = self.inner.lock() {
                inner.last_disk = Some((used, total));
                inner.last_disk_secs = now_secs;
            }
        }
        (used, total)
    }

    /// v0.7.5 公开 `new` 时一次性初始化内存常量；v0.7.6 改为在首次 sample_all 之前调用 init
    /// （仅 macOS 有实际工作，其余平台为 no-op）
    pub fn init(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            Backend::init_memory_constants(&mut inner);
        }
    }
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
    use crate::system_load::platform::{Backend, MetricsBackend};

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
        // 内存后端不需要 inner 状态（Windows 由系统调用直接给出），传占位实例即可
        let inner = super::MetricsInner {
            last_disk: None,
            last_disk_secs: 0,
        };
        let (used, total) = Backend::sample_memory(&inner);
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
        let (used, total) = Backend::read_disk_usage();
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
        let (disk_used, disk_total) = Backend::read_disk_usage();
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
