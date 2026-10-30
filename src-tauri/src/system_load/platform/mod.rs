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

/// 一个可统计的磁盘卷（v1.2.12 新增，用于多盘）。
///
/// **口径统一（v1.2.12 的核心变更）**：三平台一律用「当前用户可用量」算已用，
/// 即 `used = total - available_to_user`，`available_to_user` 对应：
/// - Windows `GetDiskFreeSpaceExW` 的第 1 个出参 `lpFreeBytesAvailableToCaller`
///   （**已扣除配额**，比第 3 个出参 `lpTotalNumberOfFreeBytes` 更贴近「我还能写多少」）
/// - macOS/Linux `statfs`/`statvfs` 的 `f_bavail`（`f_bfree` 含 root 保留块，不含）
///
/// v1.2.11 及以前三端口径不一致：Windows/macOS 用「全部空闲」、Linux 用「非特权可用」，
/// 导致同一块 ext4 盘在 Linux 上显示得更满（默认 5% root 保留）。本版统一为后者。
///
/// ⚠️ **`allow(dead_code)` 的由来（v1.2.11 加，v1.2.12 接入时删除）**：
/// 本类型的消费方（前端多盘卡片 + `metrics.rs` 的 `get_system_metrics` 扩展）属于 v1.2.12，
/// 尚未落地；而 v1.2.11 是**稳定性紧急版**，要求 `cargo check` 零警告。
/// 于是这一整条多盘链（`DiskVolume` / `read_disks` / `ffi_read_disks`）在 v1.2.11 里是
/// 「已实现、无调用方」的死代码。**保留而不删**的原因：三平台的 FFI 与过滤规则
/// （`getfsstat` / `GetLogicalDrives` / `/proc/mounts`）是逐字节验过交叉探针的，
/// 删掉等于把验证过的成果丢掉。v1.2.12 接上调用方后**必须移除这个 allow**，
/// 否则将来真的写出死代码也不会再报警。
#[allow(dead_code)] // v1.2.12 接入多盘调用方后删除；保留原因见上
#[derive(Debug, Clone)]
pub(crate) struct DiskVolume {
    /// 卷标识：Windows 盘符（`C:`）、Unix 挂载点（`/`、`/home`）。
    /// **仅用于前端去重与提示，不参与计算。**
    pub mount: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    /// `total - used`，即当前用户可用量。显式存一份避免前端各算一遍。
    pub avail_bytes: u64,
    /// 是否为系统盘（Windows `%SystemDrive%` / Unix 挂载点为 `/`）
    pub is_system: bool,
}

/// 平台指标后端契约。
///
/// 所有方法均为关联函数（无 self）：平台实现内部直接调 FFI / 系统调用。
/// `inner` 仅 macOS 后端需要（读/写 page_size 与 memory_total 常量）；
/// Windows/Linux 由系统调用直接给出，对应参数为 no-op。
pub(crate) trait MetricsBackend {
    /// 返回 (已用字节, 总量字节) 物理内存
    fn sample_memory(inner: &MetricsInner) -> (u64, u64);
    /// 返回 (已用字节, 总量字节) **系统盘**容量。
    ///
    /// 保留此方法（而非只留 `read_disks`）的原因：`read_disks` 需要枚举挂载点，
    /// 在部分环境（Windows 网络盘超时、Linux 容器）上明显更慢，而浮窗只需要一个数。
    /// v1.2.12 起两者口径已统一，`read_disk_usage` 的结果恒等于
    /// `read_disks()` 里 `is_system = true` 那一项。
    fn read_disk_usage() -> (u64, u64);
    /// v1.2.12 新增：枚举所有**本地固定**卷（排除可移动盘 / 网络盘 / 虚拟盘 /
    /// 只读快照），按 `total_bytes` 降序。
    ///
    /// 返回空 Vec 表示该平台枚举不可用或全部失败 —— 调用方应回落到 `read_disk_usage`。
    /// **不保证包含系统盘**：macOS 的 APFS 卷组结构下 `/` 与 `Data` 卷共享容器，
    /// 这里按「挂载点」列出（`/` 一定是其中一项）。各平台过滤规则见各自实现。
    ///
    /// ⚠️ `allow(dead_code)` 同 `DiskVolume`：v1.2.12 接上调用方后删除。
    #[allow(dead_code)] // v1.2.12 接入多盘调用方后删除；保留原因见 `DiskVolume` 文档
    fn read_disks() -> Vec<DiskVolume>;
    /// macOS 用：写入 inner.page_size / inner.memory_total；其余平台为 no-op
    fn init_memory_constants(inner: &mut MetricsInner);
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub(crate) use linux::Backend;
/// 当前编译目标的活跃后端类型（metrics.rs 通过此别名调用，无需逐处 cfg）
#[cfg(target_os = "macos")]
pub(crate) use macos::Backend;
#[cfg(target_os = "windows")]
pub(crate) use windows::Backend;
