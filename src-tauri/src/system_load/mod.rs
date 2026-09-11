//! system_load/mod.rs
//! 系统负载与托盘指标监测（v0.7.5 引入 / v0.7.6 跨平台扩展）。
//!
//! 设计：
//! - `CpuMonitor`：跨平台 CPU 使用率
//!   - macOS：Mach `host_statistics(HOST_CPU_LOAD_INFO)` 取 ticks，差分计算使用率
//!     （⚠️ v0.7.8 起；旧实现的 `kern.cp_time` 在 macOS 上不存在，导致 CPU 恒 0%）
//!   - Windows：GetSystemTimes 取 idle+kernel+user，差分
//!   - Linux：/proc/stat 取 total/idle，差分
//! - `NetworkSampler`：v0.7.6 新增，跨平台网络速率采样
//!   - macOS：getifaddrs + 解析 if_data.ifi_ibytes/ifi_obytes
//!   - Windows：GetIfTable2 累加 InOctets/OutOctets
//!   - Linux：读 /proc/net/dev 累加 rx/tx bytes
//! - `MetricsSampler`：统一指标采样器（v0.7.5 引入，v0.7.7 补齐全平台内存/磁盘）
//!   - 复用 CpuMonitor 的 CPU 采样
//!   - 复用 NetworkSampler 的网络采样（v0.7.6）
//!   - 内存 / 磁盘：三平台各自实现（详见 metrics.rs 的平台矩阵）
//!   - 单线程 1Hz 采样，结果驱动 macOS 菜单栏 title
//!
//! ⚠️ v0.7.8（2026-09-11）删除 `tray_icon` 模块：
//!   它曾在 Windows / Linux 把指标文字**手绘进 32x32 托盘图标**（5x7 位图字体），
//!   用以弥补 `tray.set_title` 在非 mac 平台不可见的缺口。现按产品决策移除——
//!   **Windows / Linux 的托盘恒为品牌图标**，系统指标只在「悬浮指标条」里显示；
//!   仅 macOS 使用原生菜单栏文字（`set_title`）。旧约定（非 mac 必须 set_icon 画字）作废。

use std::sync::Mutex;

pub struct CpuMonitor {
    inner: Mutex<CpuMonitorInner>,
}

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
mod linux;

mod network;
// MetricsSampler 自 v0.7.6 起跨平台存在；macOS-only 字段（mem/disk FFI）以 cfg 门控制
mod metrics;
// v0.7.6：前台窗口全屏检测（悬浮指标条「全屏自动隐藏」用；Windows 实装，其余平台 stub）
pub mod fullscreen;

pub use metrics::MetricsSampler;
// TrayTitleConfig 只有 macOS 菜单栏（set_title）路径在用；非 mac 平台不再拼托盘文字
// （v0.7.8 起），因此该 re-export 必须同样加 cfg 门，否则 Windows 侧报 unused import。
#[cfg(target_os = "macos")]
pub use metrics::TrayTitleConfig;
// NetworkSampler / NetworkSnapshot / MetricsSnapshot 仅在 system_load 内部使用，
// 不在 crate 外部 re-export（避免 unused_imports 警告；如需外部使用可在此添加）

#[cfg(target_os = "macos")]
type CpuMonitorInner = macos::Inner;
#[cfg(target_os = "windows")]
type CpuMonitorInner = windows::Inner;
#[cfg(target_os = "linux")]
type CpuMonitorInner = linux::Inner;

impl CpuMonitor {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(CpuMonitorInner::new()),
        }
    }
    pub fn cpu_usage(&self) -> Option<f32> {
        let mut inner = self.inner.lock().ok()?;
        inner.cpu_usage()
    }
}

impl Default for CpuMonitor {
    fn default() -> Self {
        Self::new()
    }
}
