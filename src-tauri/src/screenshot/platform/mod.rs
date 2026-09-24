//! 截图子系统跨平台分支收口层。
//!
//! 把 `screenshot/mod.rs` 里散落的 `#[cfg(target_os = "...")]` 平台专属体统一收口到
//! 这里：每个平台一个文件（`macos.rs` / `windows.rs` / `linux.rs`），通过 cfg 门控的
//! 模块声明 + 同名 `Backend` 再导出接入；宿主模块只调用 `Backend::method()`，
//! 自身**不出现任何 cfg 分支**（与 `system_load/platform` 同模式）。
//!
//! 三大平台专属能力，各自一个 trait：
//! - `RevealBackend`：`reveal` —— 在文件管理器里选中截图文件（各 OS 命令不同）
//! - `PermissionBackend`：macOS TCC「屏幕录制」授权闸门相关（状态 / 请求 / 打开设置面板）
//! - `CaptureBackend`：整屏抓取（macOS 走 SCK 引擎、回落 xcap；Windows/Linux 直走 xcap）
//!
//! ⚠️ `begin_capture_inner` 的「权限闸门」与「抓屏」两块原本各有一段 macos cfg，
//! 现在由 `CaptureBackend::ensure_ready` / `capture` 顶替；宿主负责在阻塞线程里
//! 调它们（首次授权要弹系统框并同步等用户点击，不能占 async runtime 线程）。

use image::RgbaImage;

use crate::screenshot::ScreenshotPermissionStatus;

/// 各平台「在文件管理器里选中文件」
pub(crate) trait RevealBackend {
    fn reveal(path: &str);
}

/// macOS TCC「屏幕录制」授权相关（非 mac 平台给安全兜底值）
pub(crate) trait PermissionBackend {
    /// 查询授权状态（前端紧凑弹窗 2s 轮询）
    fn status_snapshot() -> ScreenshotPermissionStatus;
    /// 主动请求授权（阻塞 → 宿主用 spawn_blocking 包裹）
    fn request_permission() -> ScreenshotPermissionStatus;
    /// 打开「系统设置 → 隐私与安全性 → 屏幕录制」面板
    fn open_settings();
}

/// 整屏抓取（物理像素 RGBA8 + 显示器几何 + 缩放因子）
pub(crate) trait CaptureBackend {
    /// 抓屏前的授权闸门（macOS 走 TCC 预检，其它平台恒 Ok）
    fn ensure_ready() -> Result<(), String>;
    /// 抓主显示器整屏，返回 `(img, mx, my, mw, mh, scale)`
    fn capture() -> Result<(RgbaImage, i32, i32, i32, i32, f64), String>;
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
