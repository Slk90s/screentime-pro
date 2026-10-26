//! macOS 实现：reveal / open_url 走 `open`；权限三件套委托 `tracker::macos` 与
//! `screenshot::macos`；WebView2 是 Windows 概念，这里给「可用 / 无需安装」兜底。

use super::{OsOpenBackend, PermissionBackend, Webview2Backend};

use crate::commands::Webview2Status;
use crate::db::PermissionStatus;

pub(crate) struct Backend;

impl OsOpenBackend for Backend {
    fn reveal(path: &str) -> Result<(), String> {
        crate::proc::hidden("open")
            .arg("-R")
            .arg(path)
            .status()
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn open_url(url: &str) -> Result<(), String> {
        crate::proc::hidden("open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("打开 URL 失败：{}", e))?;
        Ok(())
    }
}

impl PermissionBackend for Backend {
    fn check_permissions() -> PermissionStatus {
        PermissionStatus {
            accessibility: crate::tracker::macos::is_accessibility_trusted(),
            // v1.2.9（2026-10-26）：**改用双信号口径**，与截图侧 `ensure_ready` /
            // `status_snapshot` 保持一致。
            //
            // 原写法直接调 `tracker::macos::is_screen_capture_trusted()`（= 单信号
            // `CGPreflightScreenCaptureAccess`）。该 API 有「进程内一旦返回 false
            // 就一直 false」的已知行为（见 tracker/macos.rs、screenshot/macos.rs
            // 两处模块头的说明），于是「其实已授权」的机器会被报成 false。
            //
            // 后果落到前端是一处**文案误判**：App.vue 的
            // `accessibilityNeverGranted = !accessibility && !screen_capture`
            // 会把「升级后失效」（录屏已授权）错判成「从未授权」，横幅文案从
            // 「授权已失效，关掉再开一次」变成「从未授权，去勾选」—— 两者的
            // 处置动作完全不同，用户按错的指引操作会白忙一场。
            //
            // 为什么读 status_snapshot 而不是再写一份双信号判断：那是本仓库唯一
            // 的双信号实现（预检 OR 窗口标题可读），复制一份迟早漂移（v1.2.7 的
            // 三次「初始化竞态」教训同源）。它返回的是快照结构体，取 .permitted 即可。
            //
            // 代价：status_snapshot 在未放行时会跑 codesign / xattr 两个子进程做
            // 诊断（见 screenshot/macos.rs collect）。本函数由 App.vue 在挂载时
            // 调一次、窗口每次 focus 再调一次，属低频，可接受；**不要**把它接进
            // 1Hz/2Hz 轮询（那样才会每 2 秒 fork 两个进程）。
            screen_capture: crate::screenshot::macos::status_snapshot().permitted,
        }
    }

    fn open_privacy_settings(pane: Option<String>) {
        let anchor = match pane.as_deref() {
            Some("screen_capture") => "Privacy_ScreenCapture",
            _ => "Privacy_Accessibility",
        };
        let _ = crate::proc::hidden("open")
            .arg(format!(
                "x-apple.systempreferences:com.apple.preference.security?{anchor}"
            ))
            .spawn();
    }

    fn reset_screen_capture_permission() -> Result<String, String> {
        crate::screenshot::macos::reset_permission()
    }
}

impl Webview2Backend for Backend {
    fn check(_app: &tauri::AppHandle) -> Webview2Status {
        Webview2Status {
            os: std::env::consts::OS.to_string(),
            available: true,
            version: "n/a".to_string(),
            hint: String::new(),
        }
    }

    fn read_version() -> Option<String> {
        None
    }

    fn open_download() -> Result<(), String> {
        // macOS 不需要 WebView2，保持接口一致
        Ok(())
    }
}
