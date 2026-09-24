//! Linux 实现：reveal 退化为「打开所在目录」（无跨 DE 的选中文件标准）；
//! TCC 授权是 macOS 专属概念，给安全兜底值；抓屏直走 xcap。

use super::{CaptureBackend, PermissionBackend, RevealBackend};

use crate::screenshot::ScreenshotPermissionStatus;

pub(crate) struct Backend;

impl RevealBackend for Backend {
    fn reveal(path: &str) {
        // Linux 无跨 DE 的「选中文件」标准，退化为打开所在目录
        let dir = std::path::Path::new(path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());
        let _ = crate::proc::hidden("xdg-open").arg(dir).status();
    }
}

impl PermissionBackend for Backend {
    fn status_snapshot() -> ScreenshotPermissionStatus {
        ScreenshotPermissionStatus {
            preflight: true,
            titles: "readable".into(),
            permitted: true,
            reason: "ok".into(),
        }
    }

    fn request_permission() -> ScreenshotPermissionStatus {
        ScreenshotPermissionStatus {
            preflight: true,
            titles: "readable".into(),
            permitted: true,
            reason: "ok".into(),
        }
    }

    fn open_settings() {}
}

impl CaptureBackend for Backend {
    fn ensure_ready() -> Result<(), String> {
        Ok(())
    }

    fn capture() -> Result<(image::RgbaImage, i32, i32, i32, i32, f64), String> {
        crate::screenshot::capture_with_xcap()
    }
}
