//! 统一错误类型
//!
//! 用 `thiserror` 派生 `Display`，错误信息直接作为字符串返回给前端命令。
//!
//! 修改历史：
//!   - 2026-10-18 @v1.2.0: 清理 - 移除 `AppError` 枚举 + `From<String> for AppError`
//!     与 `TrackerError::Unsupported` 变体。三者均为死代码：`AppError` 无任何函数返回、
//!     无任何 `.into()` 消费（`commands.rs` 曾为它 `impl From<AppError> for String`，但
//!     该转换亦无调用方）；`Unsupported` 全库无构造点（`tracker/linux.rs` 头注释
//!     「Wayland 返回 Unsupported」是过时注释，实际已无该分支）。命令层统一返回
//!     `Result<_, String>`，本模块仅保留 `TrackerError`（平台采集的错误类型）。
use thiserror::Error;

/// 平台活动采集相关错误
#[derive(Debug, Error)]
pub enum TrackerError {
    #[error("当前无前台应用")]
    NoForeground,
    #[error("平台 API 调用失败: {0}")]
    Platform(String),
}
