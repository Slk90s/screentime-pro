//! system_load/mod.rs
//! 系统负载与托盘指标监测（v0.7.5 扩展）。
//!
//! 设计：
//! - `CpuMonitor`：跨平台 CPU 使用率（v0.6.2-beta.15）
//!   - macOS：sysctlbyname("kern.cp_time") 取 ticks，差分计算使用率
//!   - Windows：GetSystemTimes 取 idle+kernel+user，差分
//!   - Linux：/proc/stat 取 total/idle，差分
//! - `MetricsSampler`：统一指标采样器（v0.7.5 新增，仅 macOS）
//!   - 复用 CpuMonitor 的 CPU 采样
//!   - 新增 macOS 内存（host_statistics64 + hw.memsize）
//!   - 新增 macOS 磁盘容量（statfs）
//!   - 单线程 1Hz 采样，结果直接驱动托盘 title

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

#[cfg(target_os = "macos")]
mod metrics;

#[cfg(target_os = "macos")]
pub use metrics::{MetricsSampler, MetricsSnapshot};

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
