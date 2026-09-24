//! Linux 实现：reveal 退化为「打开所在目录」（无跨 DE 的选中文件标准）；
//! 权限三件套给安全兜底（TCC 是 macOS 概念）；WebView2 是 Windows 概念，给兜底。

use super::{OsOpenBackend, PermissionBackend, Webview2Backend};

use crate::commands::Webview2Status;
use crate::db::PermissionStatus;

pub(crate) struct Backend;

impl OsOpenBackend for Backend {
    fn reveal(path: &str) -> Result<(), String> {
        let dir = std::path::Path::new(path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());
        std::process::Command::new("xdg-open")
            .arg(dir)
            .status()
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn open_url(url: &str) -> Result<(), String> {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("打开 URL 失败：{}", e))?;
        Ok(())
    }
}

impl PermissionBackend for Backend {
    fn check_permissions() -> PermissionStatus {
        PermissionStatus {
            accessibility: true,
            screen_capture: true,
        }
    }

    fn open_privacy_settings(_pane: Option<String>) {}

    fn reset_screen_capture_permission() -> Result<String, String> {
        Err("仅 macOS 需要「屏幕录制」授权".to_string())
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
        // Linux 不需要 WebView2，保持接口一致
        Ok(())
    }
}
