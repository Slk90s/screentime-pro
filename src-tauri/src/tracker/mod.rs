//! 采集器模块总入口
//!
//! 负责：选择当前平台的采集器 + 暴露分类规则引擎。
//! 其余命令层只依赖这里的公开类型，不直接耦合具体 OS 实现。
//!
//! 修改历史：
//!   - 2026-07-08 @v0.2.0: 新增 - 按编译平台选择采集器并统一暴露采集接口。

pub mod platform;
pub use platform::{platform_name, PlatformTracker, RawApp};

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub use macos::MacOSTracker as ActiveTracker;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::WindowsTracker as ActiveTracker;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::LinuxTracker as ActiveTracker;

use std::sync::Arc;

/// 构造当前平台的采集器
pub fn create_tracker() -> Arc<dyn PlatformTracker> {
    Arc::new(ActiveTracker)
}
