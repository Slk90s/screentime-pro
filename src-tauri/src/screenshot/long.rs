//!
//! screenshot/long.rs
//! 滑动截长图（v0.9.7 Unreleased，2026-09-22）。
//!
//! 设计思路：
//! - **截图与拼接全在 Rust**：长图与普通截图不同，用户滚动页面时**遮罩窗自己会挡住
//!   要截的内容**（遮罩是置顶全屏窗）。所以长图模式的流程是：
//!   ① 用户先普通截屏进入遮罩，在冻结帧上框选出要截长的区域（选区宽度 = 长图宽度）；
//!   ② 点工具栏「长图」→ Rust 以选区矩形建会话并抓首屏（用已有冻结帧，零闪烁）；
//!   ③ 遮罩窗进入长图模式：显示「已截 N 屏 · 高度 H」进度，隐藏遮罩露出真实屏幕，
//!      用户**手动滚动**页面（应用无法替其他应用滚动 —— window.scrollBy 只对自家
//!      webview 有效，跨应用合成滚轮事件不可靠），对齐后点「继续截取」；
//!   ④ 每次截取：隐藏遮罩 ~180ms 等合成器稳定 → 抓屏 → 恢复遮罩 → Rust 把新图与
//!      累积图做**重叠检测**（拿新图顶部探针行在累积图底部搜索带里逐行滑动找匹配），
//!      把非重叠部分拼到累积图下方。
//! - **重叠算法**：逐行比对太慢（1920 宽 × 600 行搜索带），采样步长 COL_STEP=8 后
//!   单行 ~240 次对比；判定加容差（每通道 16/255 + 2% 离群容忍）抗抓屏噪声。
//!   探针行取新图顶部 1/8 处（第 0 行常带滚动残影）；多行命中取**最靠下的**
//!   （重叠最大 = 滚动距离最短，拼接最安全）。
//! - **拼不了的边界**（显式报错而非静默坏图）：
//!   - 找不到重叠（滚动太快跳过了搜索带 / 纯色页面无法判定）→ 提示滚动少一点；
//!   - 本屏无新内容（没滚动就点）→ 提示先滚动再截。
//! - **会话状态**：`LongShotState`（managed state）。start = 建会话；capture = 抓+拼；
//!   finish = 编码整图 PNG base64 回给前端（前端再走既有 screenshot_commit 落盘/
//!   剪贴板管道，复用「合成在前端」的既定口径，不另写一套落盘逻辑）；
//!   cancel = 丢弃会话。
//! - **首屏特殊**：start 时遮罩仍亮着，抓不到真实屏幕 → 首屏用普通截图会话已缓存的
//!   `ScreenshotState.frame`（那才是真实屏幕内容）。冻结帧缺失时现抓兜底（此时遮罩
//!   可能入画，属异常路径的降级，不阻断）。
//!
//! 修改历史：
//!   - 2026-09-22 @Unreleased: 初始创建 - 会话状态/重叠拼接/4 IPC（start/capture/finish/cancel）
//!

use super::ScreenshotState;
use base64::Engine as _;
use image::imageops;
use image::{GenericImage, ImageEncoder, RgbaImage};
use std::sync::{Mutex, MutexGuard};

/// 重叠检测搜索范围：拿新图顶部探针行在累积图底部这条带里找匹配（物理像素）。
/// 覆盖「滚动慢」的全部情形；一次滚出这个高度（>600px）会找不到重叠并提示用户慢点滚。
const OVERLAP_SEARCH_BAND: u32 = 600;

/// 行匹配的采样步长（像素列间隔）：逐列比较太慢（1920 宽 × 600 行），
/// 步长 8 后单行比较只需 ~240 次像素对比，整个搜索 ~15 万次，毫秒级。
const COL_STEP: usize = 8;

/// 单采样点每通道容差：同内容两次抓屏的像素噪声通常 0~3/255，
/// 视频子像素抖动 ~8；取 12 兼顾抗噪与不误匹配相邻内容行。
const CHANNEL_TOLERANCE: i32 = 12;

/// 每行允许 2% 的采样点超容差（离群容忍，应对局部小动图/光标闪烁）。
const ROW_BAD_RATIO: f64 = 0.02;

/// 一屏至少要比累积图多出这么多非重叠像素才算「新内容」，
/// 防止用户没滚动就连续点击导致图高虚涨。
const MIN_NEW_CONTENT: u32 = 8;

/// 长图累积会话
pub struct LongSession {
    /// 已拼接的完整图（宽 = 首屏选区宽）
    img: RgbaImage,
    /// 首屏裁剪矩形（物理像素，显示器坐标系）——后续每屏都按它裁
    rect: (i32, i32, u32, u32),
}

/// 长图运行时状态（lib.rs `app.manage` 注入）
#[derive(Default)]
pub struct LongShotState {
    session: Mutex<Option<LongSession>>,
}

impl LongShotState {
    fn lock(&self) -> MutexGuard<'_, Option<LongSession>> {
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// 抓主显示器整屏（直接复用 xcap 路径；SCK 引擎仅在普通截图流程优先使用，
/// 长图会话的重复抓屏走 xcap 足够，且 mac 上 SCK 每次调用开销更大）
fn capture_full_primary() -> Result<RgbaImage, String> {
    let monitors = xcap::Monitor::all().map_err(|e| format!("枚举显示器失败: {e}"))?;
    if monitors.is_empty() {
        return Err("未找到可用显示器".into());
    }
    let m = monitors
        .iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .unwrap_or(&monitors[0]);
    m.capture_image().map_err(|e| format!("屏幕捕获失败: {e}"))
}

/// 按选区矩形裁剪（物理像素、显示器坐标系；越界部分收紧）
fn crop_by_rect(full: RgbaImage, rect: (i32, i32, u32, u32)) -> Result<RgbaImage, String> {
    let (rx, ry, rw, rh) = rect;
    let (fw, fh) = (full.width() as i32, full.height() as i32);
    let x1 = rx.clamp(0, fw);
    let y1 = ry.clamp(0, fh);
    let x2 = (rx + rw as i32).clamp(0, fw);
    let y2 = (ry + rh as i32).clamp(0, fh);
    if x2 - x1 < 1 || y2 - y1 < 1 {
        return Err("选区不在当前显示器范围内".into());
    }
    Ok(imageops::crop_imm(
        &full,
        x1 as u32,
        y1 as u32,
        (x2 - x1) as u32,
        (y2 - y1) as u32,
    )
    .to_image())
}

/// 两行像素是否视为相同（逐像素采样 + 每通道容差 + 2% 离群容忍）
fn rows_match(a: &[u8], b: &[u8], w: usize) -> bool {
    let need = w * 4;
    if a.len() < need || b.len() < need {
        return false;
    }
    let step = COL_STEP.max(1);
    let samples = w.div_ceil(step);
    let mut bad = 0usize;
    for i in (0..w).step_by(step) {
        let pa = &a[i * 4..i * 4 + 3];
        let pb = &b[i * 4..i * 4 + 3];
        let over = (pa[0] as i32 - pb[0] as i32).abs() > CHANNEL_TOLERANCE
            || (pa[1] as i32 - pb[1] as i32).abs() > CHANNEL_TOLERANCE
            || (pa[2] as i32 - pb[2] as i32).abs() > CHANNEL_TOLERANCE;
        if over {
            bad += 1;
        }
    }
    (bad as f64) <= (samples as f64) * ROW_BAD_RATIO
}

/// 在累积图底部搜索带里找新图探针行的最佳匹配行；返回累积图中的 y。
///
/// 探针行取新图顶部 1/8 处（第 0 行常带滚动残影）；多行命中取**最靠下的**
/// （重叠最大 = 滚动距离最短，拼接最安全）。
fn find_overlap_row(base: &RgbaImage, new_img: &RgbaImage) -> Option<u32> {
    let (bw, bh) = (base.width() as usize, base.height() as usize);
    let (nw, nh) = (new_img.width() as usize, new_img.height() as usize);
    if bw == 0 || bh == 0 || nw == 0 || nh == 0 {
        return None;
    }
    let w = bw.min(nw);
    let probe_y = nh / 8;
    if probe_y >= nh {
        return None;
    }
    let probe_row = &new_img.as_raw()[probe_y * nw * 4..];

    let band = (OVERLAP_SEARCH_BAND as usize).min(bh);
    let band_top = bh - band;
    let mut best: Option<u32> = None;
    for y in band_top..bh {
        let base_row = &base.as_raw()[y * bw * 4..];
        if rows_match(base_row, probe_row, w) {
            best = Some(y as u32); // 持续覆盖 → 循环结束保留最靠下的命中
        }
    }
    best
}

/// 把新图的非重叠部分拼到 base 下方；返回拼接后的新图与总高度。
/// 宽不一致时把新图等比缩放到 base 宽（防御性：同显示器理论恒等宽）。
fn append_frame(base: &RgbaImage, new_img: RgbaImage) -> Result<(RgbaImage, u32), String> {
    let new_img = if new_img.width() != base.width() {
        let ratio = base.width() as f64 / new_img.width() as f64;
        let nh = ((new_img.height() as f64) * ratio).max(1.0) as u32;
        imageops::resize(&new_img, base.width(), nh, imageops::FilterType::Triangle)
    } else {
        new_img
    };

    // 重叠换算：探针行在 new 里位于 probe_y，在 base 里匹配到 match_y
    // → new 的第 0 行对应 base 的第 (match_y - probe_y) 行
    // → base 中已被 new 覆盖的行数 = base.height - (match_y - probe_y)
    // → new 顶部要跳过同样多的行，剩下的才是新内容。
    // （find_overlap_row 多行命中取最靠下的 match_y = 重叠最大、滚动最短，拼接最安全）
    let probe_y = new_img.height() / 8;
    let overlap_row = find_overlap_row(base, &new_img);
    let skip_in_new = match overlap_row {
        Some(match_y) => {
            let new_origin_in_base = match_y as i64 - probe_y as i64; // new y=0 对应的 base y
            let skip = base.height() as i64 - new_origin_in_base;
            // 越界防御：匹配点使 skip 为负（new 内容反超 base 底）或超 new 高度 → 视为无重叠
            if skip < 0 || skip > new_img.height() as i64 {
                return Err(
                    "没找到重叠区域：请滚动少一点，确保新画面与上一屏有重合（纯色页面无法判定重叠）"
                        .into(),
                );
            }
            skip as u32
        }
        None => {
            return Err(
                "没找到重叠区域：请滚动少一点，确保新画面与上一屏有重合（纯色页面无法判定重叠）"
                    .into(),
            )
        }
    };
    let new_h = new_img.height() - skip_in_new;
    if new_h < MIN_NEW_CONTENT {
        return Err("本屏没有新内容：请先滚动页面再截取".into());
    }

    // 拼接：旧图原样 + 新图跳过重叠段的部分（逐行拷贝，宽已一致）
    let total_h = base.height() + new_h;
    let mut out = RgbaImage::new(base.width(), total_h);
    out.copy_from(base, 0, 0)
        .map_err(|e| format!("拼接失败（基准段）: {e}"))?;
    for dy in 0..new_h {
        let row = imageops::crop_imm(&new_img, 0, skip_in_new + dy, new_img.width(), 1).to_image();
        out.copy_from(&row, 0, base.height() + dy)
            .map_err(|e| format!("拼接失败（新内容段）: {e}"))?;
    }
    Ok((out, total_h))
}

/// 长图信息（capture / start 共用返回）
#[derive(Clone, serde::Serialize)]
pub struct LongShotInfo {
    /// 已拼接总高度（物理像素）
    pub height: u32,
    /// 宽度（物理像素，恒 = 首屏选区宽）
    pub width: u32,
}

/// 开始长图会话：以遮罩窗当前选区矩形为裁剪依据抓首屏。
///
/// ⚠️ 首屏**不能用实时抓屏** —— 调用时遮罩窗还亮着，抓到的是自家遮罩。
/// 首屏用普通截图会话已缓存的冻结帧（真实屏幕内容）；冻结帧缺失（异常路径）
/// 才现抓兜底，此时遮罩可能入画，属降级不阻断。
#[tauri::command]
pub fn screenshot_long_start(
    ss: tauri::State<'_, ScreenshotState>,
    ls: tauri::State<'_, LongShotState>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<LongShotInfo, String> {
    if width < 16 || height < 16 {
        return Err("选区太小，无法截长图".into());
    }
    let rect = (x, y, width, height);

    let first = {
        let guard = ss.frame.lock().unwrap_or_else(|e| e.into_inner());
        match guard.as_ref() {
            Some(f) => crop_by_rect(f.img.clone(), rect)?,
            None => {
                tracing::warn!("长图首屏：冻结帧缺失，降级为实时抓屏（遮罩可能入画）");
                let full = capture_full_primary()?;
                crop_by_rect(full, rect)?
            }
        }
    };
    let info = LongShotInfo {
        width: first.width(),
        height: first.height(),
    };
    *ls.lock() = Some(LongSession { img: first, rect });
    tracing::info!(
        w = info.width,
        h = info.height,
        "长图会话开始（首屏已捕获）"
    );
    Ok(info)
}

/// 截下一屏：隐藏遮罩 → 抓屏 → 恢复遮罩 → 裁剪 → 拼接。
///
/// 遮罩窗在抓屏期间隐藏约 260ms，用户视觉上是一次轻闪，属预期行为
/// （参照市面长截图工具同类交互）。无论成败都把遮罩还回来 —— 用户还要继续操作。
#[tauri::command]
pub async fn screenshot_long_capture(
    app: tauri::AppHandle,
    ls: tauri::State<'_, LongShotState>,
) -> Result<LongShotInfo, String> {
    // ① 锁内取出会话快照（rect + 当前累积图克隆），立刻放锁
    //    （base 克隆只为校验会话存活；拼接直接在锁内对 sess.img 做，避免整份克隆过锁）
    let (rect, _base_alive) = {
        let mut guard = ls.lock();
        let sess = guard.as_mut().ok_or_else(|| "长图会话未开始".to_string())?;
        (sess.rect, sess.img.width())
    };

    // ② 隐藏遮罩 → 等合成器稳定 → 抓屏 → 恢复遮罩（全在阻塞线程）
    let app2 = app.clone();
    let full = tauri::async_runtime::spawn_blocking(move || {
        let shot = (|| -> Result<RgbaImage, String> {
            super::hide_capture_window(&app2);
            std::thread::sleep(std::time::Duration::from_millis(180));
            let img = capture_full_primary();
            std::thread::sleep(std::time::Duration::from_millis(80));
            img
        })();
        // 无论成败都把遮罩还回来（用户还要继续操作）
        super::show_capture_window(&app2);
        shot
    })
    .await
    .map_err(|e| format!("长图抓屏线程异常: {e}"))??;
    let new_img = crop_by_rect(full, rect)?;

    // ③ 拼接（毫秒级 CPU 操作，同步做）
    let mut guard = ls.lock();
    let sess = guard.as_mut().ok_or_else(|| "长图会话已结束".to_string())?;
    let (stitched, total_h) = append_frame(&sess.img, new_img)?;
    sess.img = stitched;
    tracing::info!(h = total_h, "长图已拼接新屏");
    Ok(LongShotInfo {
        width: sess.img.width(),
        height: total_h,
    })
}

/// 结束长图：把整图编码为 PNG base64 回给前端。
///
/// 前端拿到后走既有 `screenshot_commit`（剪贴板/落盘/入库/FIFO 全复用），
/// 本命令只负责编码并**清掉会话**（finish 语义 = 一次性）。
#[tauri::command]
pub fn screenshot_long_finish(ls: tauri::State<'_, LongShotState>) -> Result<String, String> {
    let sess = ls
        .lock()
        .take()
        .ok_or_else(|| "长图会话未开始".to_string())?;
    let (w, h) = (sess.img.width(), sess.img.height());
    let mut buf: Vec<u8> = Vec::new();
    image::codecs::png::PngEncoder::new(&mut buf)
        .write_image(sess.img.as_raw(), w, h, image::ExtendedColorType::Rgba8)
        .map_err(|e| format!("长图 PNG 编码失败: {e}"))?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(buf);
    tracing::info!(w, h, "长图会话完成，已编码 PNG 交回前端走 commit 管道");
    Ok(format!("data:image/png;base64,{b64}"))
}

/// 取消长图：丢弃会话（不产任何图，遮罩窗由前端负责回到普通模式或关闭）
#[tauri::command]
pub fn screenshot_long_cancel(ls: tauri::State<'_, LongShotState>) -> Result<(), String> {
    let dropped = ls.lock().take().is_some();
    tracing::info!(dropped, "长图会话已取消");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造测试图：**双向散列**像素色 —— R=(y0 + y*97 + x*29)%251、
    /// G=(y0*3 + y*59 + x*17)%249。
    /// 三个设计要点（都来自第一版测试的失败教训）：
    /// ① 不用线性渐变 —— 相邻行差恒定且小于容差，探针会大面积假命中；
    /// ② 行内必须随 x 变化 —— 行内恒色时「行间相似性」被放大，整行采样全同步；
    /// ③ 高度 ≤ 220 —— mod 251 的乘法阶是 250，行数超过必然周期碰撞（鸽笼）。
    /// 三项组合经逐行枚举验证：任意两行在 2% 容差内都判「不同」，仅复制行全同。
    fn grad(w: u32, h: u32, y0: u32) -> RgbaImage {
        let mut img = RgbaImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let r = ((y0 + y * 97 + x * 29) % 251) as u8;
                let g = ((y0 * 3 + y * 59 + x * 17) % 249) as u8;
                img.put_pixel(x, y, image::Rgba([r, g, 7, 255]));
            }
        }
        img
    }

    /// 重叠拼接：新屏顶部 80px 与累积图底部 80px 重合，应正确接出 120px 新内容。
    #[test]
    fn append_stitches_with_overlap() {
        let base = grad(200, 220, 0);
        let mut new_img = RgbaImage::new(200, 200);
        // 模拟「向下滚动 80px」：新图顶部 80px = base 的 y∈[140,220)
        for dy in 0..80 {
            let row = imageops::crop_imm(&base, 0, 140 + dy, 200, 1).to_image();
            new_img.copy_from(&row, 0, dy).unwrap();
        }
        // 新图 y∈[80,200) 是 120px 新内容（行色延续散列序列：虚拟第 220..340 行）
        for dy in 80..200 {
            let y_abs = 220 + dy - 80;
            let r = ((y_abs * 97) % 251) as u16;
            let g = ((y_abs * 59) % 249) as u16;
            for x in 0..200 {
                let xr = ((x * 29) % 251) as u16;
                let xg = ((x * 17) % 249) as u16;
                // u16 中转避免 u8 加法溢出 panic（散列和最大 250+250）
                let rr = ((r + xr) % 251) as u8;
                let gg = ((g + xg) % 249) as u8;
                new_img.put_pixel(x, dy, image::Rgba([rr, gg, 7, 255]));
            }
        }

        let (out, total) = append_frame(&base, new_img).expect("应能拼接");
        assert_eq!(total, 340, "220 + 120 新内容");
        assert_eq!(out.width(), 200);
        assert_eq!(out.height(), 340);
    }

    /// 无重叠（内容完全不同）必须显式报错，不允许静默产出坏图。
    /// 探针行（new y=25，y0=100）与 base 全部 220 行均不匹配（已逐行枚举验证）。
    #[test]
    fn append_fails_without_overlap() {
        let base = grad(200, 220, 0);
        let new_img = grad(200, 200, 100);
        assert!(append_frame(&base, new_img).is_err(), "找不到重叠时应报错");
    }

    /// 没滚动就点截取（新图整体是累积图底部的复制）→ 应报「无新内容」。
    #[test]
    fn append_fails_without_new_content() {
        let base = grad(200, 220, 0);
        let mut new_img = RgbaImage::new(200, 160);
        // 视口没动：新图 160px 全部是 base 底部 y∈[60,220) 的原样内容
        for dy in 0..160 {
            let row = imageops::crop_imm(&base, 0, 60 + dy, 200, 1).to_image();
            new_img.copy_from(&row, 0, dy).unwrap();
        }
        assert!(append_frame(&base, new_img).is_err(), "无新内容时应报错");
    }

    /// 宽不一致的防御路径：新图先被等比缩放，不再 panic 即可（重叠与否取决于缩放结果）。
    #[test]
    fn append_handles_mismatched_width() {
        let base = grad(200, 220, 0);
        let new_img = grad(400, 200, 100);
        let scaled = imageops::resize(&new_img, 200, 100, imageops::FilterType::Triangle);
        let _ = append_frame(&base, scaled);
    }
}
