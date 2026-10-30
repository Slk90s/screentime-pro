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
//!   - 2026-10-17 @v1.1.1: 修复 - 显式设置 `SCStreamConfiguration` 出图宽高（P0，
//!     「小窗口快照」根因）；同时返回显示器真实原点（原硬编码 0,0）。详见下方
//!     「出图尺寸」段的完整推导。
//!

use screencapturekit::screenshot_manager::{CGImageExt, SCScreenshotManager};
use screencapturekit::shareable_content::SCShareableContent;
use screencapturekit::stream::configuration::{SCCaptureResolutionType, SCStreamConfiguration};
use screencapturekit::stream::content_filter::SCContentFilter;

/// SCK 路径的错误（转成 String 给上层统一处理）
pub type SckResult<T> = Result<T, String>;

/// 本机 macOS 版本是否 ≥ 14.0（SCK 截图 API 的最低运行版本）。
///
/// 用子进程查 `sw_vers -productVersion`（走 `crate::proc::hidden`，与全库约定一致）而不是链接
/// `NSProcessInfo.operatingSystemVersion`：后者要引 objc2-app-kit 的运行时调用，
/// 而这里只在**截图入口**跑一次（进程生命周期内缓存），子进程开销可忽略。
///
/// ⚠️ 结果按 `OnceLock` 缓存：一次进程只查一次，避免每次截图都 fork。
fn is_macos_14_plus() -> bool {
    static CACHE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CACHE.get_or_init(|| {
        let v = crate::proc::hidden("/usr/bin/sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .parse::<String>()
                    .ok()
            })
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

/// 用 SCK 抓取**主显示器**整屏，返回物理像素 RGBA8 图、该显示器在全局坐标系中的
/// 原点（物理像素）与该显示器的真实缩放因子。
///
/// 返回 `(img, origin_x_px, origin_y_px, scale)`。
///
/// 流程：`SCShareableContent::get()`（枚举显示器；未授权时这里报错）→
/// 选主显示器（displays()[0]；SCK 返回顺序即系统顺序，第一块即主屏）→
/// 构建全显示器过滤器 → `SCScreenshotManager::capture_image()` →
/// `rgba_data()` 提取像素 → 组装 `image::RgbaImage`。
///
/// ⚠️ **会阻塞**：`capture_image` 内部是同步等待回调（Swift Task → completion），
/// 必须从 `spawn_blocking` 里调（`begin_capture_inner` 已保证）。
///
/// ## 出图尺寸（2026-10-17 @v1.1.1 修复 P0，必须显式设置宽高）
///
/// **修改原因**：`SCStreamConfiguration` 的宽高**不设时，Apple 的默认值就是
/// 1920×1080**（官方 StackOverflow 答复 78847211 与 DTS 论坛 765360 双源确认：
/// "The SCStreamConfiguration defaults to 1920x1080. It has to be configured
/// manually"）。本文件此前未调 `with_width` / `with_height`，于是**无论显示器
/// 真实分辨率多高，冻结帧恒为 1920×1080** → 遮罩窗按 1920×1080 布局 → 用户看到
/// 「一个小窗口的快照」而不是整屏。
///
/// **注意事项**：
///   - 期望物理像素尺寸 = `SCContentFilter::content_rect().size`（**单位：点**）
///     × `point_pixel_scale()`（点→像素比）。两者都是 `SCContentFilter` 的现成
///     方法（`macos_14_0` feature，本项目 `Cargo.toml` 已启用），无需新增依赖。
///   - **向上取整**（`ceil`）而非四舍五入：Retina 下 point × 2 偶能整除，但带
///     分数缩放（如 1.5x 的外接显示器、macOS 的「放大显示」非整数档位）时
///     取整会**少 1px**，导致遮罩窗右下角露缝。
///   - **不可用** `SCScreenshotConfiguration`（`screenshot_manager.rs` 内自带
///     `with_width`/`with_height`）：它整体 gated 在 `macos_26_0` feature 之后，
///     而本项目目标是 **macOS 14+**，用它会在 14/15 上直接编译不过。
///   - 抓完做**尺寸自检**：出图与期望偏差 > 2px 即返回 `Err`，让
///     `platform/macos.rs` 回落 xcap（宁可走旧引擎，也不出半张/被缩放的坏图）。
///     这道兜底覆盖「某些系统版本上 contentRect 返回 0」等未知漂移。
///   - **旧约定失效**：本函数头注释此前声称「宽高不设时 SCK 默认按显示器原生
///     分辨率出图（Retina 下即物理像素）」—— 该断言**已被 Apple 官方答复证伪**，
///     不要再沿用。若未来看到「不设宽高更简单」的「优化」，那是在 reintroduce
///     本 bug。
///
/// ## 缩放因子与原点
///
/// - `scale` 取 `SCContentFilter::point_pixel_scale()`（macOS 14+ 权威的
///   点→像素比，Retina = 2.0）。**注意**：不能拿 `SCDisplay.width()` 除以
///   `frame().size.width` 自算 —— Apple 文档明确两者单位都是**点**，比值恒 1.0
///   （crate 文档声称 width 是像素属于错误文档，审计时已核实）。
/// - `origin_x_px / origin_y_px` 取 `SCDisplay::frame().origin`（**单位：点**）
///   × `scale` 换算到物理像素。此前上层硬编码 `0, 0`，多显示器 / 非主屏场景下
///   遮罩窗会错位（macOS 允许显示器原点为负坐标 —— 副屏在主屏左侧/上方）。
pub fn capture_main_display() -> SckResult<(image::RgbaImage, i32, i32, f64)> {
    if !sck_available() {
        return Err("SCK 需要 macOS 14+，本机版本较低".into());
    }

    // ① 枚举可共享内容 —— 未授权时这里返回 Err（SCK 相比旧 API 的核心优势）
    let content = SCShareableContent::get()
        .map_err(|e| format!("SCK 枚举屏幕内容失败（多为未授权屏幕录制）: {e}"))?;
    let displays = content.displays();
    // v1.2.0：多屏「鼠标在哪截哪屏」——优先选光标停靠的显示器，取不到光标 /
    // 点不到任何屏时回落第一块（与 xcap 路径的回落语义一致）。
    // ⚠️ SCDisplay 无 Clone/Copy，用索引取借用（后续 with_display 吃 &SCDisplay）。
    let display_idx = crate::screenshot::cursor::cursor_native()
        .and_then(|(cx, cy)| {
            displays.iter().position(|d| {
                let f = d.frame();
                cx >= f.origin.x
                    && cx < f.origin.x + f.size.width
                    && cy >= f.origin.y
                    && cy < f.origin.y + f.size.height
            })
        })
        .unwrap_or(0);
    let display = displays
        .get(display_idx)
        .ok_or_else(|| "SCK 未枚举到任何显示器".to_string())?;

    // ② 过滤器：整个显示器（不含窗口级过滤 —— 我们要整屏冻结帧）
    let filter = SCContentFilter::create()
        .with_display(display)
        .with_excluding_windows(&[])
        .build();

    // ①' 权威缩放因子（v1.0.1 审计返工）：改用 SCK 自己的
    //     `point_pixel_scale()`（macOS 14+，与截图路径版本要求一致）。
    //     ⚠️ 旧实现 `display.width() / frame.size.width` 是错的：
    //     Apple 官方文档明确 SCDisplay.width 的单位是**点**（不是
    //     crate 文档声称的像素），frame 也是点 —— 两点相除恒得 1.0，
    //     等于没修。`point_pixel_scale()` 由系统返回真实的点→像素比
    //     （Retina = 2.0），才是权威来源。
    //     异常（≤0）时回落 1.0 —— 此时出图大概率就是逻辑尺寸，
    //     与旧行为一致，不会更糟。
    let scale = {
        let s = filter.point_pixel_scale() as f64;
        if (1.0..=4.0).contains(&s) {
            s
        } else {
            1.0
        }
    };

    // ②' 期望出图物理像素尺寸：content_rect（点）× scale。
    //     ⚠️ content_rect 在 macOS < 14.0 返回零 rect，但本函数已由
    //     `sck_available()` 守住 14+ 下限；这里仍做一次显式防御 ——
    //     拿不到有效尺寸时**直接报错回落 xcap**，绝不带着 0 去构造 config
    //     （`with_width(0)` 会让 SCK 行为未定义）。
    let content_rect = filter.content_rect();
    let expected_w = (content_rect.size.width * scale).ceil();
    let expected_h = (content_rect.size.height * scale).ceil();
    if !expected_w.is_finite() || !expected_h.is_finite() || expected_w < 1.0 || expected_h < 1.0 {
        return Err(format!(
            "SCK contentRect 无效（point {}×{}，scale {scale}），无法确定出图尺寸",
            content_rect.size.width, content_rect.size.height
        ));
    }
    // 上限保护：8K（7680×4320）已远超任何现役显示器；超过说明 content_rect
    // 拿到了异常值，直接回落而不是给 SCK 塞一个荒谬的尺寸。
    const MAX_DIMENSION: f64 = 8192.0;
    if expected_w > MAX_DIMENSION || expected_h > MAX_DIMENSION {
        return Err(format!(
            "SCK contentRect 异常偏大（{expected_w:.0}×{expected_h:.0}），已放弃 SCK 路径"
        ));
    }
    let expected_w = expected_w as u32;
    let expected_h = expected_h as u32;

    // ③ 配置：**显式**指定出图宽高（Apple 默认 1920×1080，见函数头「出图尺寸」段）
    //    + 最高质量。`with_capture_resolution_type(Best)` 在显式宽高下只影响
    //    内部采样质量，不再决定出图尺寸。
    let config = SCStreamConfiguration::new()
        .with_width(expected_w)
        .with_height(expected_h)
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

    // ④' 出图尺寸自检：与期望偏差 > 2px 即视为「SCK 没按我们给的宽高出图」。
    //     宁回落 xcap（`platform/macos.rs` 的 Err 分支）也不出坏图 ——
    //     半张图 / 被系统悄悄缩放过的图会让遮罩窗与冻结帧对不上，
    //     表现为「框选位置偏移」，比整条链路失败更难排查。
    let dw = (w as i64 - expected_w as i64).abs();
    let dh = (h as i64 - expected_h as i64).abs();
    if dw > 2 || dh > 2 {
        return Err(format!(
            "SCK 出图尺寸与期望不符：期望 {expected_w}×{expected_h}，实际 {w}×{h}（偏差 {dw}×{dh}）"
        ));
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

    let img = image::RgbaImage::from_raw(w as u32, h as u32, rgba)
        .ok_or_else(|| "SCK 像素数据无法组装为图像".to_string())?;

    // ⑥ 显示器真实原点：SCDisplay.frame() 的单位是**点**（含 origin，
    //     副屏在主屏左侧/上方时为负），× scale 换算到物理像素。
    //     与上面出图尺寸同源同一个坐标系，故用同一个 scale 换算。
    let frame = display.frame();
    let origin_x_px = (frame.origin.x * scale).round() as i32;
    let origin_y_px = (frame.origin.y * scale).round() as i32;
    tracing::info!(
        expected_w,
        expected_h,
        actual_w = w,
        actual_h = h,
        scale,
        origin_x_px,
        origin_y_px,
        "SCK 出图尺寸与显示器原点已确定"
    );

    Ok((img, origin_x_px, origin_y_px, scale))
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
