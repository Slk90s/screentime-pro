//!
//! screenshot/macos_sck.rs
//! macOS SCK 截图引擎（v0.9.4，2026-09-20）：ScreenCaptureKit 单帧截图 + 版本回落。
//!
//! ## 为什么引入（Task 80）
//! v0.9.3 及以前，抓屏走 xcap → 底层 `CGWindowListCreateImage`。这条旧链路有两个
//! macOS 特有的坑：
//!   ① **API 已被 Apple 废弃**（10.15 起标记 deprecated），行为随系统版本漂移；
//!   ② **未授权时不报错** —— 返回一张「只有桌面壁纸」的图（隐私软化），调用方
//!      无从分辨「截到了」还是「被系统抹掉了」。
//! ScreenCaptureKit（macOS 14+）是 Apple 的现代替代：
//!   - `SCShareableContent::get()` 未授权时**真报错**，且是触发系统授权弹窗
//!     最可靠的方式（比 `CGRequestScreenCaptureAccess` 更新列表更及时）；
//!   - `SCScreenshotManager::capture_image()` 单帧截图，未授权同样真报错；
//!   - 原生支持 Retina 物理像素、按显示器枚举。
//!
//! ## 版本回落策略
//! SCK 截图 API 需要 **macOS 14+**；crate 本体最低 macOS 13。本模块在运行时检测：
//!   - macOS 14+  → 走 SCK（本文件）；
//!   - macOS 13   → `SCShareableContent::get()` 可用（授权登记/弹窗仍走 SCK），
//!                  但截图回落 xcap；
//!   - macOS 12-  → 全部回落 xcap（SCK framework 不存在，crate 的 Swift 桥
//!                  以 13.0 为部署目标，12 上加载会直接 dyld 报错 —— 所以
//!                  **调用前必须先查版本**，见 `sck_available()`）。
//!
//! ## 与 mod.rs 的接线
//! `begin_capture_inner` 在 macOS 上先调 [`capture_main_display`]：
//!   - `Ok(img)` → 拿到物理像素 RGBA 帧，后续流程与 xcap 路径完全一致；
//!   - `Err(SckError::PermissionDenied)` → 权限闸门已拦，正常不会走到这里；
//!   - `Err(_)` / macOS < 14 → 回落 xcap（`mod.rs` 原有路径）。
//!
//! 修改历史：
//!   - 2026-09-20 @v0.9.4: 初始创建 —— SCK 单帧截图引擎 + macOS 版本回落。
//!

use screencapturekit::screenshot_manager::{CGImageExt, SCScreenshotManager};
use screencapturekit::shareable_content::SCShareableContent;
use screencapturekit::stream::configuration::{
    SCCaptureResolutionType, SCStreamConfiguration,
};
use screencapturekit::stream::content_filter::SCContentFilter;

/// SCK 路径的错误（转成 String 给上层统一处理）
pub type SckResult<T> = Result<T, String>;

/// 本机 macOS 版本是否 ≥ 14.0（SCK 截图 API 的最低运行版本）。
///
/// 用 `std::process::Command` 查 `sw_vers -productVersion` 而不是链接
/// `NSProcessInfo.operatingSystemVersion`：后者要引 objc2-app-kit 的运行时调用，
/// 而这里只在**截图入口**跑一次（进程生命周期内缓存），子进程开销可忽略。
///
/// ⚠️ 结果按 `OnceLock` 缓存：一次进程只查一次，避免每次截图都 fork。
fn is_macos_14_plus() -> bool {
    static CACHE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CACHE.get_or_init(|| {
        let v = std::process::Command::new("/usr/bin/sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<String>().ok())
            .map(|s| {
                // "14.5" / "15.0" / "26.1" —— 取主版本号比较即可（macOS 26 是 Tahoe 的正式版本号）
                s.split('.')
                    .next()
                    .and_then(|major| major.parse::<u32>().ok())
                    .map(|major| major >= 14)
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        tracing::debug!(macos_14_plus = v, "SCK 可用性检测（sw_vers）");
        v
    })
}

/// SCK 引擎是否可用于**截图**（macOS 14+）。
///
/// macOS 13 上 crate 能加载（Swift 桥部署目标 13.0），但 `SCScreenshotManager`
/// 的 `capture_image` 需要 14+（Swift 侧有 `if #available(macOS 14.0, *)` 守卫，
/// 低版本会返回错误字符串而不是崩溃）—— 不过为省一次注定失败的 FFI 往返，
/// 13 上直接回落 xcap。
pub fn sck_available() -> bool {
    is_macos_14_plus()
}

/// 用 SCK 抓取**主显示器**整屏，返回物理像素 RGBA8 图。
///
/// 流程：`SCShareableContent::get()`（枚举显示器；未授权时这里报错）→
/// 选主显示器（displays()[0]；SCK 返回顺序即系统顺序，第一块即主屏）→
/// 构建全显示器过滤器 → `SCScreenshotManager::capture_image()` →
/// `rgba_data()` 提取像素 → 组装 `image::RgbaImage`。
///
/// ⚠️ **会阻塞**：`capture_image` 内部是同步等待回调（Swift Task → completion），
/// 必须从 `spawn_blocking` 里调（`begin_capture_inner` 已保证）。
///
/// 尺寸说明：`SCStreamConfiguration` 的宽高**不设**时 SCK 默认按显示器原生
/// 分辨率出图（Retina 下即物理像素）；显式设宽高反而可能触发缩放。
/// 这里只设 `capture_resolution_type(Best)` 保证拿最高分辨率。
pub fn capture_main_display() -> SckResult<image::RgbaImage> {
    if !sck_available() {
        return Err("SCK 需要 macOS 14+，本机版本较低".into());
    }

    // ① 枚举可共享内容 —— 未授权时这里返回 Err（SCK 相比旧 API 的核心优势）
    let content = SCShareableContent::get()
        .map_err(|e| format!("SCK 枚举屏幕内容失败（多为未授权屏幕录制）: {e}"))?;
    let displays = content.displays();
    let display = displays
        .first()
        .ok_or_else(|| "SCK 未枚举到任何显示器".to_string())?;

    // ② 过滤器：整个显示器（不含窗口级过滤 —— 我们要整屏冻结帧）
    let filter = SCContentFilter::create()
        .with_display(display)
        .with_excluding_windows(&[])
        .build();

    // ③ 配置：原生分辨率 + 最高质量；宽高不设 = 按显示器原生出图
    let config = SCStreamConfiguration::new()
        .with_capture_resolution_type(SCCaptureResolutionType::Best)
        .with_shows_cursor(false); // 与 xcap 路径行为一致：冻结帧不含鼠标指针

    // ④ 单帧截图（阻塞等待）
    let cg_image = SCScreenshotManager::capture_image(&filter, &config)
        .map_err(|e| format!("SCK 单帧截图失败: {e}"))?;

    let w = cg_image.width();
    let h = cg_image.height();
    if w == 0 || h == 0 {
        return Err("SCK 返回了空图像".into());
    }

    // ⑤ 提取 RGBA 像素（CGImageExt::rgba_data；BGRA 更快但我们后续链路全是 RGBA）
    let rgba = cg_image
        .rgba_data()
        .map_err(|e| format!("SCK 提取像素数据失败: {e}"))?;
    let expected = w * h * 4;
    if rgba.len() < expected {
        return Err(format!(
            "SCK 像素数据不足: 期望 {expected} 字节，实际 {}",
            rgba.len()
        ));
    }

    image::RgbaImage::from_raw(w as u32, h as u32, rgba)
        .ok_or_else(|| "SCK 像素数据无法组装为图像".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 版本检测在 Windows 上必然失败（sw_vers 不存在）→ 返回 false。
    /// 这个测试在 macOS CI 上会真跑 sw_vers（结果 ≥14 → true），两端都应通过。
    #[test]
    fn version_probe_never_panics() {
        let _ = is_macos_14_plus();
    }
}
