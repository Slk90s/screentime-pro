//! 统一错误类型
//!
//! 用 `thiserror` 派生 `Display`，错误信息直接作为字符串返回给前端命令。
use thiserror::Error;

/// 平台活动采集相关错误
#[derive(Debug, Error)]
pub enum TrackerError {
    #[error("当前无前台应用")]
    NoForeground,
    #[error("平台 API 调用失败: {0}")]
    Platform(String),
    #[error("该平台功能尚未实现: {0}")]
    Unsupported(String),
}

/// 全局应用错误（Tauri command 统一返回 String）
#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Tracker(#[from] TrackerError),
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
    #[error("{0}")]
    Msg(String),
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Msg(s)
    }
}

#[allow(dead_code)]
pub type AppResult<T> = Result<T, AppError>;
