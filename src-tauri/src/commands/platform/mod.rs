//! 命令层跨平台分支收口层。
//!
//! 把 `commands.rs` 里散落的 `#[cfg(target_os = "...")]` 平台专属体统一收口到
//! 这里：每个平台一个文件（`macos.rs` / `windows.rs` / `linux.rs`），通过 cfg 门控的
//! 模块声明 + 同名 `Backend` 再导出接入；宿主命令只调用 `Backend::method()`，
//! 自身**不出现任何 cfg 分支**（与 `system_load/platform` / `screenshot/platform` 同模式）。
//!
//! 三类平台专属能力，各自一个 trait：
//! - `OsOpenBackend`：在系统壳里打开文件 / URL（reveal 与 open_url，命令路径不同）
//! - `PermissionBackend`：macOS TCC 权限查询 / 打开隐私面板 / 重置屏幕录制授权
//! - `Webview2Backend`：WebView2 运行时检测（仅 Windows 有意义）

use tauri::AppHandle;

use crate::db::PermissionStatus;
use crate::commands::Webview2Status;

/// 在系统壳里打开文件 / URL
pub(crate) trait OsOpenBackend {
    /// 在文件管理器里选中指定文件
    fn reveal(path: &str) -> Result<(), String>;
    /// 用默认程序打开 URL（调用方已校验 http/https 协议）
    fn open_url(url: &str) -> Result<(), String>;
}

/// macOS TCC 权限相关（非 mac 平台给安全兜底）
pub(crate) trait PermissionBackend {
    /// 查询辅助功能 + 屏幕录制授权状态
    fn check_permissions() -> PermissionStatus;
    /// 打开「隐私与安全性」指定面板（pane 见 `open_privacy_settings`）
    fn open_privacy_settings(pane: Option<String>);
    /// 重置本应用「屏幕录制」授权记录（仅 macOS 有意义）
    fn reset_screen_capture_permission() -> Result<String, String>;
}

/// WebView2 运行时检测（仅 Windows 有意义）
pub(crate) trait Webview2Backend {
    /// 检测 WebView2 是否可用 + 版本
    fn check(app: &AppHandle) -> Webview2Status;
    /// 读取已安装 WebView2 版本（仅 Windows 实际查询注册表）
    fn read_version() -> Option<String>;
    /// 打开 WebView2 下载页（仅 Windows 实际跳转）
    fn open_download() -> Result<(), String>;
}

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "macos")]
pub(crate) use macos::Backend;
#[cfg(target_os = "windows")]
pub(crate) use windows::Backend;
#[cfg(target_os = "linux")]
pub(crate) use linux::Backend;
