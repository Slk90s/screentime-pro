//! 平台采集抽象层
//!
//! 定义跨平台统一的「活动采集」接口 `PlatformTracker`，
//! 各操作系统提供自己的实现（macOS/Windows/Linux），
//! 上层采样循环只依赖这个 trait，不关心平台细节。
//!
//! 注意：分类逻辑已迁移到 `classifier` 模块（基于数据库可配置规则），
//! 本文件只保留「原始数据」定义与平台标识。

use crate::error::TrackerError;

/// 采集到的原始应用信息
///
/// 分类规则引擎（`classifier`）会综合这些字段（进程名 / 窗口标题 /
/// 可执行路径 / 包名 / 展示名）自动归入某个分类。
#[derive(Debug, Clone)]
pub struct RawApp {
    pub name: String,
    pub process_name: String,
    pub exe_path: Option<String>,
    pub bundle_id: Option<String>,
    /// 前台窗口标题（macOS 需屏幕录制权限、Windows 默认可取；用于更细粒度分类）
    pub window_title: Option<String>,
}

/// 平台活动采集抽象（各 OS 提供自己的实现）
pub trait PlatformTracker: Send + Sync {
    /// 获取当前前台应用（进程名 / 路径 / 包名 / 窗口标题）
    fn get_foreground_app(&self) -> Result<RawApp, TrackerError>;
    /// 获取系统空闲时长（秒），即距上次用户输入的时间
    fn get_idle_seconds(&self) -> Result<u64, TrackerError>;
}

/// 当前平台标识
pub fn platform_name() -> &'static str {
    #[cfg(target_os = "macos")]
    return "macos";
    #[cfg(target_os = "windows")]
    return "windows";
    #[cfg(target_os = "linux")]
    return "linux";
    #[allow(unreachable_code)]
    "unknown"
}
