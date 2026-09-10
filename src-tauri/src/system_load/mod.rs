//! system_load/mod.rs
//! 系统负载与托盘指标监测（v0.7.5 引入 / v0.7.6 跨平台扩展）。
//!
//! 设计：
//! - `CpuMonitor`：跨平台 CPU 使用率（v0.6.2-beta.15）
//!   - macOS：sysctlbyname("kern.cp_time") 取 ticks，差分计算使用率
//!   - Windows：GetSystemTimes 取 idle+kernel+user，差分
//!   - Linux：/proc/stat 取 total/idle，差分
//! - `NetworkSampler`：v0.7.6 新增，跨平台网络速率采样
//!   - macOS：getifaddrs + 解析 if_data.ifi_ibytes/ifi_obytes
//!   - Windows：GetIfTable2 累加 InOctets/OutOctets
//!   - Linux：读 /proc/net/dev 累加 rx/tx bytes
//! - `MetricsSampler`：统一指标采样器（v0.7.5 引入，仅 macOS 含内存/磁盘）
//!   - 复用 CpuMonitor 的 CPU 采样
//!   - 复用 NetworkSampler 的网络采样（v0.7.6）
//!   - macOS 内存（host_statistics64 + hw.memsize）
//!   - macOS 磁盘容量（statfs）
//!   - 单线程 1Hz 采样，结果直接驱动托盘 title
//! - `tray_icon`（v0.7.6 新增）：在 Windows/Linux 托盘图标上**手绘**系统指标文字
//!   - 5x7 位图字体 + 32x32 RGBA 渲染，2 行 × ≤3 字符
//!   - 弥补 `tray.set_title` 在 Windows/Linux 不显示的缺口（macOS 仍走 set_title）

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
// v0.7.6：在 Windows/Linux 托盘图标上手绘指标文字（macOS 走 set_title，详见 tray_icon.rs）
pub mod tray_icon;
// v0.7.6：前台窗口全屏检测（悬浮指标条「全屏自动隐藏」用；Windows 实装，其余平台 stub）
pub mod fullscreen;

pub use metrics::{MetricsSampler, TrayTitleConfig};
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
