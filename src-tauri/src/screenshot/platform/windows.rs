//! Windows 实现：reveal 走 `explorer /select,`；TCC 授权是 macOS 专属概念，
//! 这里给安全兜底值；抓屏直走 xcap（Windows 无 SCK）。

use super::{CaptureBackend, PermissionBackend, RevealBackend};

use crate::screenshot::ScreenshotPermissionStatus;

pub(crate) struct Backend;

impl RevealBackend for Backend {
    fn reveal(path: &str) {
        let _ = crate::proc::hidden("explorer")
            .arg(format!("/select,{path}"))
            .status();
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
