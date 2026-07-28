//! system_load/mod.rs
//! 系统 CPU 负载监测（v0.6.2-beta.15）。
//!
//! 设计：
//! - 提供跨平台统一的 `cpu_usage(&mut self) -> Option<f32>`（0.0~1.0）
//! - macOS：host_processor_info 取 ticks，差分计算使用率
//! - Windows：GetSystemTimes 取 idle+kernel+user，差分
//! - Linux：/proc/stat 取 total/idle，差分
//! - 各实现内部用「上一次调用保留值」做差分，所以 `cpu_usage` 必须可变持有状态
//!   （返回 `&mut self`，调用方一般把它装到 mutex 里共享）

use std::sync::Mutex;

/// 跨平台 CPU 使用率监测器
///
/// 内部持有上一次系统 ticks 的快照，每次 `tick()` 计算「自上次起的平均使用率」。
/// 通过 `Arc<Mutex<CpuMonitor>>` 在多线程间共享。空实现留给 `tauri::State` 注入使用方。
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
    /// 取一次系统 CPU 使用率（0.0~1.0）。第一次调用返回 None（需要 2 次差分）。
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
