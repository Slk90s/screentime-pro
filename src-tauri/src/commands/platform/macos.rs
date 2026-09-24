//! macOS 实现：reveal / open_url 走 `open`；权限三件套委托 `tracker::macos` 与
//! `screenshot::macos`；WebView2 是 Windows 概念，这里给「可用 / 无需安装」兜底。

use super::{OsOpenBackend, PermissionBackend, Webview2Backend};

use crate::commands::Webview2Status;
use crate::db::PermissionStatus;

pub(crate) struct Backend;

impl OsOpenBackend for Backend {
    fn reveal(path: &str) -> Result<(), String> {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .status()
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn open_url(url: &str) -> Result<(), String> {
        std::process::Command::new("open")
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
            screen_capture: crate::tracker::macos::is_screen_capture_trusted(),
        }
    }

    fn open_privacy_settings(pane: Option<String>) {
        let anchor = match pane.as_deref() {
            Some("screen_capture") => "Privacy_ScreenCapture",
            _ => "Privacy_Accessibility",
        };
        let _ = std::process::Command::new("open")
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
