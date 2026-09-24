//! Windows 实现：reveal 走 `explorer /select,`；权限三件套给安全兜底（TCC 是 macOS 概念）；
//! WebView2 检测走注册表。

use super::{OsOpenBackend, PermissionBackend, Webview2Backend};

use crate::commands::Webview2Status;
use crate::db::PermissionStatus;

pub(crate) struct Backend;

impl OsOpenBackend for Backend {
    fn reveal(path: &str) -> Result<(), String> {
        std::process::Command::new("explorer")
            .arg(format!("/select,{path}"))
            .status()
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn open_url(url: &str) -> Result<(), String> {
        open::that(url).map_err(|e| format!("打开 URL 失败：{}", e))
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
        let version = Self::read_version();
        if let Some(v) = version {
            return Webview2Status {
                os: "windows".to_string(),
                available: true,
                version: v,
                hint: String::new(),
            };
        }
        Webview2Status {
            os: "windows".to_string(),
            available: false,
            version: String::new(),
            hint: "未检测到 WebView2 运行时，请先安装 Microsoft Edge WebView2 Runtime（永驻版）后再运行本应用：\nhttps://developer.microsoft.com/en-us/microsoft-edge/webview2/".to_string(),
        }
    }

    fn read_version() -> Option<String> {
        let keys = [
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\ClientState\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
            r"HKLM\SOFTWARE\Microsoft\EdgeUpdate\ClientState\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        ];
        for key in keys {
            // v0.7.7：全部走 proc::hidden——否则首次启动会连续闪 3 个 reg 控制台黑框
            let output = crate::proc::hidden("reg")
                .args(["query", key, "/v", "pv"])
                .output()
                .ok()?;
            if !output.status.success() {
                continue;
            }
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                if line.trim_start().starts_with("pv") {
                    if let Some(v) = line.split_whitespace().nth(2) {
                        return Some(v.to_string());
                    }
                }
            }
        }
        let output = crate::proc::hidden("reg")
            .args([
                "query",
                r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Edge\BLBeacon",
                "/v",
                "version",
            ])
            .output()
            .ok()?;
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                if line.trim_start().starts_with("version") {
                    if let Some(v) = line.split_whitespace().nth(2) {
                        return Some(v.to_string());
                    }
                }
            }
        }
        None
    }

    fn open_download() -> Result<(), String> {
        open::that("https://developer.microsoft.com/en-us/microsoft-edge/webview2/")
            .map_err(|e| e.to_string())
    }
}
