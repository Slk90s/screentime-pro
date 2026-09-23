//! system_load/platform/mod.rs
//! 跨平台指标后端抽象（v0.9.7 从 metrics.rs 拆分）。
//!
//! 设计：每个平台在各自文件里用 `impl MetricsBackend for Backend` 实现内存/磁盘采样 +
//! macOS 专属内存常量初始化；`metrics.rs` 的 `MetricsSampler` 只通过 `Backend` 别名调用，
//! 不再在 impl 内堆 `#[cfg(target_os=...)]` 分支。
//!
//! 这样平台代码独立成文件，改某一端（例如只动 macOS FFI）不会误伤 Windows/Linux，
//! 也更便于定位历史坑。各平台的崩溃修复与布局断言见对应文件头部注释。

use crate::system_load::metrics::MetricsInner;

/// 平台指标后端契约。
///
/// 所有方法均为关联函数（无 self）：平台实现内部直接调 FFI / 系统调用。
/// `inner` 仅 macOS 后端需要（读/写 page_size 与 memory_total 常量）；
/// Windows/Linux 由系统调用直接给出，对应参数为 no-op。
pub(crate) trait MetricsBackend {
    /// 返回 (已用字节, 总量字节) 物理内存
    fn sample_memory(inner: &MetricsInner) -> (u64, u64);
    /// 返回 (已用字节, 总量字节) 系统盘容量（macOS 用根卷 "/"）
    fn read_disk_usage() -> (u64, u64);
    /// macOS 用：写入 inner.page_size / inner.memory_total；其余平台为 no-op
    fn init_memory_constants(inner: &mut MetricsInner);
}

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
mod linux;

/// 当前编译目标的活跃后端类型（metrics.rs 通过此别名调用，无需逐处 cfg）
#[cfg(target_os = "macos")]
pub(crate) use macos::Backend;
#[cfg(target_os = "windows")]
pub(crate) use windows::Backend;
#[cfg(target_os = "linux")]
pub(crate) use linux::Backend;
