//! macOS 实现：reveal 走 `open -R`；权限 IPC 三件套委托 `screenshot/macos` 模块
//! （纯 mac 模块，已被 vision-probe 交叉编译覆盖）；抓屏走 SCK 引擎、回落 xcap。

use super::{CaptureBackend, PermissionBackend, RevealBackend};

pub(crate) struct Backend;

impl RevealBackend for Backend {
    fn reveal(path: &str) {
        let _ = crate::proc::hidden("open").args(["-R", path]).status();
    }
}

impl PermissionBackend for Backend {
    fn status_snapshot() -> crate::screenshot::ScreenshotPermissionStatus {
        let s = crate::screenshot::macos::status_snapshot();
        crate::screenshot::ScreenshotPermissionStatus {
            preflight: s.preflight,
            titles: s.titles.to_string(),
            permitted: s.permitted,
            reason: s.reason.to_string(),
        }
    }

    fn request_permission() -> crate::screenshot::ScreenshotPermissionStatus {
        let s = crate::screenshot::macos::request_permission();
        crate::screenshot::ScreenshotPermissionStatus {
            preflight: s.preflight,
            titles: s.titles.to_string(),
            permitted: s.permitted,
            reason: s.reason.to_string(),
        }
    }

    fn open_settings() {
        crate::screenshot::macos::open_permission_settings();
    }
}

impl CaptureBackend for Backend {
    fn ensure_ready() -> Result<(), String> {
        crate::screenshot::macos::ensure_ready()
    }

    fn capture() -> Result<(image::RgbaImage, i32, i32, i32, i32, f64), String> {
        // 优先 SCK（macOS 14+）；未授权 / 低版本回落 xcap。
        match crate::screenshot::macos_sck::capture_main_display() {
            Ok(img) => {
                let (w, h) = (img.width(), img.height());
                tracing::info!(w, h, "整屏已抓取（SCK 引擎）");
                Ok((img, 0i32, 0i32, w as i32, h as i32, 1.0f64))
            }
            Err(e) => {
                tracing::warn!(error = %e, "SCK 截图失败，回落 xcap");
                crate::screenshot::capture_with_xcap()
            }
        }
    }
}
