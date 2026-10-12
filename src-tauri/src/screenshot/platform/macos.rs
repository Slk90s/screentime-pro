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
        //
        // v1.0.1 修复「画中画」：旧代码把 scale 硬编码 1.0 —— SCK 出图尺寸随系统
        // 行为漂移（有的版本回逻辑尺寸），上层按物理像素处理再被显示器 scale 二次
        // 缩放 → 遮罩窗只盖半屏。现在 SCK 路径回传 `SCContentFilter::point_pixel_scale()`
        // （macOS 14+ 权威点→像素比，Retina = 2.0；SCDisplay 宽/frame 宽同为「点」，
        // 相除恒 1.0，不可用来自算，详见 macos_sck.rs）；xcap 回落路径本就返回
        // 真实 scale，行为不变。
        match crate::screenshot::macos_sck::capture_main_display() {
            Ok((img, scale)) => {
                let (w, h) = (img.width(), img.height());
                tracing::info!(w, h, scale, "整屏已抓取（SCK 引擎）");
                Ok((img, 0i32, 0i32, w as i32, h as i32, scale))
            }
            Err(e) => {
                tracing::warn!(error = %e, "SCK 截图失败，回落 xcap");
                crate::screenshot::capture_with_xcap()
            }
        }
    }
}
