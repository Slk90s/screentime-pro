//!
//! screenshot/long.rs
//! 滑动截长图（v1.0.0，2026-09-22）。
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
//!   - 2026-09-22 @v1.0.0: 初始创建 - 会话状态/重叠拼接/4 IPC（start/capture/finish/cancel）
//!   - 2026-10-04 @v1.1.0: 拼接置信分级（借鉴 Snapzy confirmed/tentative 的本地化裁剪版）：
//!     搜索带内唯一命中 = confirmed 立即拼；多行命中（重复版式/纯色页）= tentative，
//!     取最靠下拼接并记待验证段起点；下一屏完整覆盖该段时逐采样行复核——吻合升级
//!     confirmed、错位回滚截断重拼、覆盖不到保留（尽力而为）。回滚无需快照：
//!     tentative 段恒为累积图尾部，截断即回滚。LongShotInfo 增 tentative 计数。
//!   - 2026-10-09 @v1.1.0 审计修复（长图专项检查）:
//!     ① verify_tentative 增 CannotVerify 裁决：「新屏覆盖不到段」此前被调用方当成
//!        已定案清掉待验证段、随后 append 又必然报错 → 未验证段被静默定案。
//!        现改为整体放弃本屏（报错、会话状态原样保留、段继续待验证）。
//!     ② 会话记录首屏来源帧尺寸 frame_size，每屏抓屏后守卫（与 repeat_region 同款）：
//!        分辨率/显示器/DPI 变化显式报错，不再静默 clamp 裁错区域。
//!     ③ finish 改 `#[tauri::command(async)]`（大 PNG 编码不占主线程）+ 编码失败
//!        时会话还回（可重试，不丢成果）。
//!     ④ start 拒绝覆盖进行中的会话；append 新内容段改整块单次拷贝。
//!

use super::ScreenshotState;
use base64::Engine as _;
use image::imageops;
use image::{GenericImage, ImageEncoder, RgbaImage};
use std::borrow::Cow;
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

/// 固定头/尾识别的上限：单侧最多识别到视口高度的 1/3（防止全屏静态页面被整屏误判）。
const FIXED_REGION_MAX_RATIO: u32 = 3;

/// 固定头尾识别的最小有意义高度（低于此视为噪声，不剔除）。
const FIXED_REGION_MIN: u32 = 4;

/// 长图累积会话。
///
/// v1.0.1 对齐 QQ/企业微信方案（其技术分享：滚动截图 = 滚动中多次截图 + 子线程
/// 模板匹配拼接 + **固定头尾识别裁剪，拼完再拼回**）。此前没有固定区处理，
/// 页面固定 header/footer 会随每屏重复拼进长图（内容错位、同一条导航栏出现 N 次）。
pub struct LongSession {
    /// 已拼接的内容图（宽 = 首屏选区宽；**不含**固定头尾）
    img: RgbaImage,
    /// 首屏裁剪矩形（物理像素，显示器坐标系）——后续每屏都按它裁
    rect: (i32, i32, u32, u32),
    /// 上一屏的完整视口裁剪图（固定区识别的对比基准）
    last: RgbaImage,
    /// 固定头（首次识别时记录；finish 时拼回最顶）
    header: Option<RgbaImage>,
    /// 固定尾（首次识别时记录；finish 时拼回最底）
    footer: Option<RgbaImage>,
    /// 首屏是否已按识别出的固定区剔除（只做一次；此后 img 恒为「剔除后」口径）
    first_cut_done: bool,
    /// v1.1.0 置信分级：最近一次 tentative（低置信）拼接段在 `img` 中的起始 y。
    /// None = 无待验证段（上一屏是 confirmed 拼接或段已被验证/回滚）。
    /// 段恒为 img 尾部（[tentative_from, img.height)），回滚 = 截断，无需快照。
    tentative_from: Option<u32>,
    /// v1.1.0：历史累计的 tentative 拼接次数（含已验证/已回滚），供前端提示
    tentative_count: u32,
    /// v1.1.0 审计修复：首屏来源的整屏帧尺寸 (w, h)——后续每屏抓屏后校验，
    /// 分辨率/显示器排布/DPI 变化时显式报错，防止选区矩形静默裁错区域
    /// （与 `screenshot_repeat_region` 的同款守卫对齐）。
    frame_size: (u32, u32),
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

/// 在累积图底部搜索带里找新图探针行的全部命中行；返回累积图中的 y 列表（升序）。
///
/// 探针行取新图顶部 1/8 处（第 0 行常带滚动残影）。
///
/// v1.1.0 置信分级：返回**全部**命中而非只回最靠下的——
/// - 空列表 = 无重叠（调用方报错，同旧逻辑）；
/// - 单命中 = confirmed（对齐无歧义，立即拼接）；
/// - 多命中 = tentative（重复版式/纯色页无法分辨，取最靠下拼但记待验证段，
///   下一屏复核；Snapzy 同思路：最终图只由 confirmed 切片构成，本项目串行
///   交互下简化为「先拼 + 可回滚」，用户体感一致且实现量小一个量级）。
fn find_overlap_rows(base: &RgbaImage, new_img: &RgbaImage) -> Vec<u32> {
    let (bw, bh) = (base.width() as usize, base.height() as usize);
    let (nw, nh) = (new_img.width() as usize, new_img.height() as usize);
    if bw == 0 || bh == 0 || nw == 0 || nh == 0 {
        return Vec::new();
    }
    let w = bw.min(nw);
    let probe_y = nh / 8;
    if probe_y >= nh {
        return Vec::new();
    }
    let probe_row = &new_img.as_raw()[probe_y * nw * 4..];

    let band = (OVERLAP_SEARCH_BAND as usize).min(bh);
    let band_top = bh - band;
    let mut hits: Vec<u32> = Vec::new();
    for y in band_top..bh {
        let base_row = &base.as_raw()[y * bw * 4..];
        if rows_match(base_row, probe_row, w) {
            hits.push(y as u32);
        }
    }
    hits
}

/// 识别两屏之间的固定头/尾高度：从顶部（或底部）逐行比对，连续相同的行数即固定区。
///
/// QQ/微信思路的移植：页面固定 header/footer 在滚动中逐像素不变，而滚动区内容
/// 必然变化（静止不动时用户不会点「继续截取」）。
///
/// 返回 `(header_h, footer_h)`；`max` 为单侧上限（视口 1/3，防全静态页误判）。
fn detect_fixed_regions(last: &RgbaImage, cur: &RgbaImage) -> (u32, u32) {
    let w = last.width().min(cur.width()) as usize;
    let max_h = (last.height().min(cur.height())) / FIXED_REGION_MAX_RATIO;
    if w == 0 || max_h < FIXED_REGION_MIN {
        return (0, 0);
    }
    let lraw = last.as_raw();
    let craw = cur.as_raw();
    let (lw, cw) = (last.width() as usize, cur.width() as usize);

    // 从顶部数连续相同的行
    let mut header = 0u32;
    for y in 0..max_h {
        let y = y as usize;
        if rows_match(&lraw[y * lw * 4..], &craw[y * cw * 4..], w) {
            header += 1;
        } else {
            break;
        }
    }
    // 从底部数连续相同的行
    let mut footer = 0u32;
    for dy in 0..max_h {
        let ly = (last.height() - 1 - dy) as usize;
        let cy = (cur.height() - 1 - dy) as usize;
        if rows_match(&lraw[ly * lw * 4..], &craw[cy * cw * 4..], w) {
            footer += 1;
        } else {
            break;
        }
    }
    // 低于最小有意义高度视为噪声（滚动残影/光标闪烁），不剔除
    let header = if header >= FIXED_REGION_MIN { header } else { 0 };
    let footer = if footer >= FIXED_REGION_MIN { footer } else { 0 };
    (header, footer)
}

/// 拼接置信级别（v1.1.0，借鉴 Snapzy confirmed/tentative）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StitchConfidence {
    /// 搜索带内唯一命中：对齐无歧义，立即成为最终图的一部分。
    Confirmed,
    /// 多行命中（重复版式/纯色页）：取最靠下拼接，段标记为待验证，
    /// 下一屏完整覆盖时逐行复核（吻合升级 / 错位回滚）。
    Tentative,
}

/// 拼接结果：新累积图 + 总高 + 本次置信 + tentative 段起点。
///
/// `tentative_from` 仅在 `confidence == Tentative` 时有意义（Tentative 段
/// 恒为返回图的尾部，起点 = 拼接前的 base.height）；Confirmed 时为 0 占位。
pub struct AppendOutcome {
    pub img: RgbaImage,
    pub total_h: u32,
    pub confidence: StitchConfidence,
    /// Tentative 段在返回图中的起始 y（恒 = 拼接前的 base.height，段到图尾）
    pub tentative_from: u32,
}

/// 把新图的非重叠部分拼到 base 下方；返回拼接结果（含置信分级）。
///
/// v1.0.1：`fixed` 传「本屏识别到的固定区高度」——先从新图**剔除**头尾固定像素，
/// 再做重叠检测与拼接（固定区不参与内容累积，否则每屏重复一次）。
/// 宽不一致时把新图等比缩放到 base 宽（防御性：同显示器理论恒等宽），
/// 固定区高度同步等比换算（审计修正：识别值是缩放前的像素，缩放后坐标系不同）。
///
/// v1.1.0 置信分级：`find_overlap_rows` 返回全部命中行——
/// - 1 个命中（Confirmed）：直接拼，`tentative_from` 无意义（0 占位）；
/// - ≥2 个命中（Tentative）：仍取最靠下（重叠最大 = 滚动最短，最安全）拼接，
///   但调用方必须把返回的 `tentative_from` 记入会话，下一屏复核（见
///   `verify_tentative`）。拼接前调用方应先对**上一轮** tentative 段做验证/回滚。
fn append_frame(base: &RgbaImage, new_img: RgbaImage, fixed: (u32, u32)) -> Result<AppendOutcome, String> {
    let (new_img, fixed) = if new_img.width() != base.width() {
        let ratio = base.width() as f64 / new_img.width() as f64;
        let nh = ((new_img.height() as f64) * ratio).max(1.0) as u32;
        let scaled = (
            ((fixed.0 as f64) * ratio) as u32,
            ((fixed.1 as f64) * ratio) as u32,
        );
        (imageops::resize(&new_img, base.width(), nh, imageops::FilterType::Triangle), scaled)
    } else {
        (new_img, fixed)
    };

    // 先剔除本屏固定头尾（在缩放后的坐标系里；宽一致时即原坐标系）
    let cut_top = fixed.0.min(new_img.height() / 2);
    let cut_bottom = fixed.1.min(new_img.height().saturating_sub(cut_top) / 2);
    let content = imageops::crop_imm(
        &new_img,
        0,
        cut_top,
        new_img.width(),
        new_img.height() - cut_top - cut_bottom,
    )
    .to_image();

    // 重叠换算：探针行在 new 里位于 probe_y，在 base 里匹配到 match_y
    // → new 的第 0 行对应 base 的第 (match_y - probe_y) 行
    // → base 中已被 new 覆盖的行数 = base.height - (match_y - probe_y)
    // → new 顶部要跳过同样多的行，剩下的才是新内容。
    // （多命中取最靠下的 match_y = 重叠最大、滚动最短，拼接最安全）
    let probe_y = content.height() / 8;
    let hits = find_overlap_rows(base, &content);
    let match_y = *hits.last().ok_or_else(|| {
        "没找到重叠区域：请滚动少一点，确保新画面与上一屏有重合（纯色页面无法判定重叠）"
    })?;
    let new_origin_in_base = match_y as i64 - probe_y as i64; // new y=0 对应的 base y
    let skip = base.height() as i64 - new_origin_in_base;
    // 越界防御：匹配点使 skip 为负（new 内容反超 base 底）或超 new 高度 → 视为无重叠
    if skip < 0 || skip > content.height() as i64 {
        return Err(
            "没找到重叠区域：请滚动少一点，确保新画面与上一屏有重合（纯色页面无法判定重叠）"
                .into(),
        );
    }
    let skip_in_new = skip as u32;
    let new_h = content.height() - skip_in_new;
    if new_h < MIN_NEW_CONTENT {
        return Err("本屏没有新内容：请先滚动页面再截取".into());
    }

    // 拼接：旧图原样 + 新图跳过重叠段的部分（宽已一致）。
    // v1.1.0 审计优化：`imageops::overlay` 带目标偏移，整块单次合成——
    // 此前逐行 crop+copy 在高分屏（new_h 上千行）要循环上千次。
    // overlay(&mut out, &content, 0, base.height() as i64 - skip_in_new as i64)
    // 的语义：content 的第 0 行落在 out 的 y = base.height - skip_in_new，
    // 即 content 的 skip_in_new 行恰好对齐 out 的 base.height 行 ✓
    let total_h = base.height() + new_h;
    let mut out = RgbaImage::new(base.width(), total_h);
    out.copy_from(base, 0, 0)
        .map_err(|e| format!("拼接失败（基准段）: {e}"))?;
    imageops::overlay(
        &mut out,
        &content,
        0,
        base.height() as i64 - skip_in_new as i64,
    );
    let confidence = if hits.len() == 1 {
        StitchConfidence::Confirmed
    } else {
        StitchConfidence::Tentative
    };
    Ok(AppendOutcome {
        img: out,
        total_h,
        confidence,
        tentative_from: base.height(),
    })
}

/// v1.1.0 置信分级核心：验证上一轮 tentative 段，决定升级 / 回滚 / 无法验证。
///
/// 原理：tentative 段是靠「多行命中取最靠下」猜出来的对齐，可能把重复版式
/// （如表格行）拼重或拼漏。新屏到来时，若其滚动位移合理，新屏的**视口内容**
/// 会完整覆盖 tentative 段——此时用新屏对应区段逐采样行比对：
/// - 全部吻合 → 段内容正确（只是当初匹配有歧义），升级为 confirmed；
/// - 不吻合 → 段对齐错了，**截断回滚**（tentative 段恒在图尾，截断即回滚），
///   返回回滚后的图与高度，调用方用新屏重新拼接；
/// - 新屏覆盖不到段（滚动太快，重叠不够 / 对齐参数异常）→ **无法验证**。
///   v1.1.0 审计修正：此前此情形与「吻合」共用同一返回（不回滚 + 调用方清段），
///   而后续 append 在这种几何下必然报「无重叠/无新内容」——结果是未验证（可能
///   拼错）的段被**静默定案**，用户 finish 后拿到坏图却没有任何提示。
///   现在显式返回 `CannotVerify`，调用方应**整体放弃本屏**（报错、保留会话与
///   待验证段原状），让用户滚动到位后下一屏再验证。
///
/// 返回裁决 + （回滚时的）验证后图与高度。升级/无法验证时图原样借用（零拷贝）。
/// `new_origin_in_base` = 新屏 y=0 在 base 坐标系中的 y（与
/// `append_frame` 内的换算同式）；`new_content` = 新屏剔除固定区后的内容图。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VerifyVerdict {
    /// 段内容与新屏逐行吻合 → 升级 confirmed（清待验证段）。
    Confirmed,
    /// 段内容不吻合 → 已截断回滚（图/高度取返回值，段消失，本屏重拼）。
    RolledBack,
    /// 新屏覆盖不到段（或对齐参数异常）→ 无法验证，base 原样，段保持待验证。
    CannotVerify,
}

fn verify_tentative<'a>(
    base: &'a RgbaImage,
    tentative_from: u32,
    new_origin_in_base: i64,
    new_content: &RgbaImage,
) -> (VerifyVerdict, Cow<'a, RgbaImage>, u32) {
    let seg_top = tentative_from as usize;
    let seg_bottom = base.height() as usize;
    // 新屏在 base 坐标系中覆盖的行区间下界。
    // new_origin 为负（新屏顶在 base 顶之上）时 clamp 为 0——但段内比对要求
    // 每行都有对应新屏行，负 origin 会使段顶行 ny<0 → 走 mismatch 回滚，
    // 语义正确（新屏没盖住段顶 = 无法确证段顶，宁回滚不保留错位）。
    let cov_bottom = (new_origin_in_base + new_content.height() as i64).max(0) as usize;
    // 覆盖不到段尾 → 无法验证，原样保留（v1.1.0 审计：显式 CannotVerify，
    // 调用方不得清段、也不得用本屏继续 append——该几何下 append 必然出错）
    if cov_bottom < seg_bottom {
        return (VerifyVerdict::CannotVerify, Cow::Borrowed(base), base.height());
    }
    // 逐采样行比对：段内每一行都要求与新屏对应行吻合
    let w = base.width().min(new_content.width()) as usize;
    for y in seg_top..seg_bottom {
        let ny = y as i64 - new_origin_in_base;
        if ny < 0 || ny >= new_content.height() as i64 {
            // 段顶超出新屏视野（对齐参数异常）：与「覆盖不足」同族，无法验证
            return (VerifyVerdict::CannotVerify, Cow::Borrowed(base), base.height());
        }
        let a = &base.as_raw()[y * base.width() as usize * 4..];
        let b = &new_content.as_raw()[ny as usize * new_content.width() as usize * 4..];
        if !rows_match(a, b, w) {
            // 不吻合：截断回滚（tentative 段恒为尾部）
            let cut = tentative_from;
            let trimmed = imageops::crop_imm(base, 0, 0, base.width(), cut).to_image();
            return (VerifyVerdict::RolledBack, Cow::Owned(trimmed), cut);
        }
    }
    // 全部吻合：升级 confirmed，段起点清除（返回借用，零拷贝）
    (VerifyVerdict::Confirmed, Cow::Borrowed(base), base.height())
}

/// 长图信息（capture / start 共用返回）
#[derive(Clone, serde::Serialize)]
pub struct LongShotInfo {
    /// 已拼接总高度（物理像素）
    pub height: u32,
    /// 宽度（物理像素，恒 = 首屏选区宽）
    pub width: u32,
    /// v1.1.0：当前仍处于 tentative（低置信待验证）状态的拼接段计数
    /// （0 或 1——同一时刻最多一个待验证段）。前端可据此提示「建议多截一屏复核」。
    pub tentative: u32,
}

/// 开始长图会话：以遮罩窗当前选区矩形为裁剪依据抓首屏。
///
/// ⚠️ 首屏**不能用实时抓屏** —— 调用时遮罩窗还亮着，抓到的是自家遮罩。
/// 首屏用普通截图会话已缓存的冻结帧（真实屏幕内容）；冻结帧缺失（异常路径）
/// 才现抓兜底，此时遮罩可能入画，属降级不阻断。
///
/// v1.1.0 审计修复：已有会话未结束时再次 start → 显式报错（此前会静默覆盖
/// 旧会话，旧累积成果无提示丢失）；会话记录首屏来源帧尺寸 frame_size，
/// 后续每屏校验（分辨率/显示器/DPI 变化 → 显式报错而非静默裁错）。
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

    // 已有会话未结束：拒绝新会话（前端须先完成/取消），不静默覆盖
    if ls.lock().is_some() {
        return Err("已有长图会话进行中，请先完成或取消".into());
    }

    let (first, frame_size) = {
        let guard = ss.frame.lock().unwrap_or_else(|e| e.into_inner());
        match guard.as_ref() {
            Some(f) => (crop_by_rect(f.img.clone(), rect)?, (f.img.width(), f.img.height())),
            None => {
                tracing::warn!("长图首屏：冻结帧缺失，降级为实时抓屏（遮罩可能入画）");
                let full = capture_full_primary()?;
                let size = (full.width(), full.height());
                (crop_by_rect(full, rect)?, size)
            }
        }
    };
    let info = LongShotInfo {
        width: first.width(),
        height: first.height(),
        tentative: 0,
    };
    // v1.0.1：首屏整幅视口先原样保存为 img 与 last（对比基准）；
    // 固定头尾此时未知（没有第二屏做对比），首次 capture 时统一识别。
    // first_cut_done 只在**真正剔除过首屏**后才置位（审计修正）：首次对比
    // 若未识别到固定区（fixed=(0,0)，如噪声/未滚动到位），后续识别到时
    // 仍要对首屏补剔除，否则 base（含固定区）与新屏（已剔除）口径不一致。
    *ls.lock() = Some(LongSession {
        img: first.clone(),
        rect,
        last: first,
        header: None,
        footer: None,
        first_cut_done: false,
        tentative_from: None,
        tentative_count: 0,
        frame_size,
    });
    tracing::info!(
        w = info.width,
        h = info.height,
        frame = ?frame_size,
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

    // v1.1.0 审计修复：帧尺寸守卫（与 repeat_region 同款）——分辨率/显示器排布/DPI
    // 变化后选区矩形会裁到完全错位的区域，显式报错让用户重新框选，不静默出坏图。
    {
        let guard = ls.lock();
        let sess = guard.as_ref().ok_or_else(|| "长图会话未开始".to_string())?;
        let size = (full.width(), full.height());
        if size != sess.frame_size {
            return Err(format!(
                "屏幕配置已变化（分辨率/显示器），长图已中止：请重新框选后再截长图（{}×{} → {}×{}）",
                sess.frame_size.0, sess.frame_size.1, size.0, size.1
            ));
        }
    }
    let new_img = crop_by_rect(full, rect)?;

    // ③ 拼接（毫秒级 CPU 操作，同步做）
    let mut guard = ls.lock();
    let sess = guard.as_mut().ok_or_else(|| "长图会话已结束".to_string())?;

    // v1.0.1：与上一屏对比识别固定头尾 → 剔除后再拼接（QQ/微信方案）。
    // 首次 capture（last = 首屏完整视口）识别出的固定区记进会话并**一次性**
    // 对首屏做剔除；后续每屏按当次识别值剔除（固定区滚出视口后自动停止剔除）。
    let fixed = detect_fixed_regions(&sess.last, &new_img);
    if sess.header.is_none() && fixed.0 > 0 {
        sess.header = Some(imageops::crop_imm(&new_img, 0, 0, new_img.width(), fixed.0).to_image());
    }
    if sess.footer.is_none() && fixed.1 > 0 {
        let fy = new_img.height() - fixed.1;
        sess.footer =
            Some(imageops::crop_imm(&new_img, 0, fy, new_img.width(), fixed.1).to_image());
    }
    if !sess.first_cut_done {
        // 首屏 img 是完整视口：按本次识别值剔除一次，对齐后续「剔除后」口径。
        // （首屏与本屏的固定区行必然相同 —— 固定区本就不随滚动变）
        // 审计修正：只有真正剔除才置位 —— 首次对比 fixed=(0,0) 时保持
        // false，等后续对比识别到固定区再补剔除（口径延迟对齐）。
        let cut_top = fixed.0.min(sess.img.height() / 2);
        let cut_bottom = fixed.1.min(sess.img.height().saturating_sub(cut_top) / 2);
        if cut_top > 0 || cut_bottom > 0 {
            sess.img = imageops::crop_imm(
                &sess.img,
                0,
                cut_top,
                sess.img.width(),
                sess.img.height() - cut_top - cut_bottom,
            )
            .to_image();
            sess.first_cut_done = true;
        }
    }
    let (stitched, total_h) = {
        // v1.1.0 置信分级主流程（顺序敏感，三步）：
        // ① 先算出本屏的对齐参数（在「验证前 base 坐标系」里），
        // ② 用它验证/回滚上一轮 tentative 段（CannotVerify 时整体报错返回），
        // ③ 在验证后的 base 上重新对齐并拼接，按命中数标记新段的置信。
        //
        // 注意：append_frame 内部的对齐计算与 verify_tentative 共用同一套
        // 「探针行 → match_y → new_origin_in_base」换算，因此 ① 直接用
        // find_overlap_rows 独立算一份（探针行不受 fixed 剔除影响的口径
        // 与 append_frame 一致：都是剔除固定区后的 content）。

        // —— ① 本屏对齐参数（基于当前 sess.img，可能与 ② 之前一致）——
        // 先构造与 append_frame 相同口径的 content（缩放 + 剔除固定区）
        let content = if new_img.width() != sess.img.width() {
            let ratio = sess.img.width() as f64 / new_img.width() as f64;
            let nh = ((new_img.height() as f64) * ratio).max(1.0) as u32;
            imageops::resize(&new_img, sess.img.width(), nh, imageops::FilterType::Triangle)
        } else {
            new_img.clone()
        };
        let cut_top = fixed.0.min(content.height() / 2);
        let cut_bottom = fixed.1.min(content.height().saturating_sub(cut_top) / 2);
        let content = imageops::crop_imm(
            &content,
            0,
            cut_top,
            content.width(),
            content.height() - cut_top - cut_bottom,
        )
        .to_image();
        let probe_y = content.height() / 8;
        let hits = find_overlap_rows(&sess.img, &content);
        let match_y = hits.last().copied();

        // —— ② 验证/回滚上一轮 tentative 段 ——
        if let (Some(seg_from), Some(my)) = (sess.tentative_from, match_y) {
            let new_origin = my as i64 - probe_y as i64;
            let (verdict, verified, verified_h) =
                verify_tentative(&sess.img, seg_from, new_origin, &content);
            match verdict {
                VerifyVerdict::RolledBack => {
                    tracing::warn!(
                        seg_from,
                        old_h = sess.img.height(),
                        new_h = verified_h,
                        "长图 tentative 段复核不吻合，已回滚（本屏重新拼接补回正确内容）"
                    );
                    sess.img = verified.into_owned();
                    sess.tentative_from = None;
                }
                VerifyVerdict::Confirmed => {
                    sess.tentative_from = None; // 升级 confirmed，段定案
                }
                VerifyVerdict::CannotVerify => {
                    // v1.1.0 审计修复：新屏覆盖不到待验证段（滚动太快）——
                    // ① 无法验证段（可能是拼错的），② 该几何下本屏 append 也必然
                    // 报「无重叠/无新内容」。整体放弃本屏：报错把状态原样还给用户
                    // （会话、待验证段都不动），用户滚动到位后下一屏再验证。
                    // 此前此分支被当作「已定案」清掉段 + 照常 append → 未验证段
                    // 被静默定案，finish 后才坏图。
                    tracing::warn!(
                        seg_from,
                        "本屏覆盖不到 tentative 段，无法验证：放弃本屏，段继续待验证"
                    );
                    return Err(
                        "本屏滚动幅度过大，覆盖不到上一段待验证内容，未做拼接：请滚动少一点（保证与上一屏重合）再截取".into(),
                    );
                }
            }
        } else if sess.tentative_from.is_some() {
            // 本屏连重叠都找不到（后续 append_frame 会报错），段保持待验证
            tracing::debug!("本屏无法用于验证 tentative 段（无重叠命中）");
        }

        // —— ③ 在（可能已回滚的）base 上对齐并拼接 ——
        let outcome = append_frame(&sess.img, new_img.clone(), fixed)?;
        let tentative_now = outcome.confidence == StitchConfidence::Tentative;
        if tentative_now {
            sess.tentative_from = Some(outcome.tentative_from);
            sess.tentative_count += 1;
            tracing::info!(
                seg_from = outcome.tentative_from,
                hits = hits.len(),
                "长图本屏为 tentative 拼接（多行命中），待下一屏验证"
            );
        }
        (outcome.img, outcome.total_h)
    };
    sess.img = stitched;
    sess.last = new_img;
    tracing::info!(h = total_h, fixed_h = fixed.0, fixed_f = fixed.1, "长图已拼接新屏");
    Ok(LongShotInfo {
        width: sess.img.width(),
        height: total_h,
        tentative: if sess.tentative_from.is_some() { 1 } else { 0 },
    })
}

/// 结束长图：把整图编码为 PNG base64 回给前端。
///
/// 前端拿到后走既有 `screenshot_commit`（剪贴板/落盘/入库/FIFO 全复用），
/// 本命令只负责编码并**清掉会话**（finish 语义 = 一次性）。
///
/// v1.1.0 审计修复：
/// - 标 `#[tauri::command(async)]`——长图可达数万像素高，PNG 编码 + base64 是
///   秒级 CPU 操作，同步命令会阻塞 IPC 主线程（期间其他命令全部排队）；
/// - 编码失败时会话**还回**状态（take → 失败 → restore），用户重试 finish
///   不丢累积成果（此前 take 语义下失败即丢，只能 cancel 整段重来）。
#[tauri::command(async)]
pub fn screenshot_long_finish(ls: tauri::State<'_, LongShotState>) -> Result<String, String> {
    let sess = ls
        .lock()
        .take()
        .ok_or_else(|| "长图会话未开始".to_string())?;
    // v1.0.1：把识别出的固定头尾拼回（QQ/微信「裁掉的固定区最后拼回」步骤）——
    // 头在最顶、尾在最底，长图视觉上与页面结构一致。
    let header = sess.header;
    let footer = sess.footer;
    let head_h = header.as_ref().map(|i| i.height()).unwrap_or(0);
    let foot_h = footer.as_ref().map(|i| i.height()).unwrap_or(0);
    let content_h = sess.img.height();
    let total_h = head_h + content_h + foot_h;
    let final_img = if total_h > content_h {
        let mut out = RgbaImage::new(sess.img.width(), total_h);
        let mut y = 0u32;
        if let Some(hd) = &header {
            out.copy_from(hd, 0, y).map_err(|e| format!("拼回固定头失败: {e}"))?;
            y += hd.height();
        }
        out.copy_from(&sess.img, 0, y)
            .map_err(|e| format!("拼回内容段失败: {e}"))?;
        if let Some(ft) = &footer {
            out.copy_from(ft, 0, y + content_h)
                .map_err(|e| format!("拼回固定尾失败: {e}"))?;
        }
        out
    } else {
        sess.img
    };
    let (w, h) = (final_img.width(), final_img.height());
    // 立即执行闭包（临时值随 let 语句结束析构）：借用只在编码期间存在，
    // 下方 Err 分支才能把 final_img 还回会话。
    let encoded = (|| -> Result<String, String> {
        let mut buf: Vec<u8> = Vec::new();
        image::codecs::png::PngEncoder::new(&mut buf)
            .write_image(final_img.as_raw(), w, h, image::ExtendedColorType::Rgba8)
            .map_err(|e| format!("长图 PNG 编码失败: {e}"))?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(buf);
        Ok(format!("data:image/png;base64,{b64}"))
    })();
    match encoded {
        Ok(data) => {
            tracing::info!(
                w,
                h,
                tentative_total = sess.tentative_count,
                pending = sess.tentative_from.is_some(),
                "长图会话完成，已编码 PNG 交回前端走 commit 管道"
            );
            Ok(data)
        }
        Err(e) => {
            // 编码失败：会话还回，用户可重试 finish（不丢累积成果）。
            // ⚠️ 还回的必须是「内容段」而非合成图——final_img 已含拼回的头尾，
            // 直接还回会让重试 finish 二次拼接固定区。从合成图裁回内容段
            //（else 分支无头尾，final_img 即内容段，裁剪是恒等操作）。
            let content_back = imageops::crop_imm(
                &final_img,
                0,
                head_h,
                final_img.width(),
                content_h,
            )
            .to_image();
            *ls.lock() = Some(LongSession {
                img: content_back,
                rect: sess.rect,
                last: sess.last,
                header,
                footer,
                first_cut_done: sess.first_cut_done,
                tentative_from: sess.tentative_from,
                tentative_count: sess.tentative_count,
                frame_size: sess.frame_size,
            });
            Err(e)
        }
    }
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
    /// v1.0.1：append_frame 增加 fixed 参数，无固定区时传 (0,0)。
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

        let outcome = append_frame(&base, new_img, (0, 0)).expect("应能拼接");
        assert_eq!(outcome.total_h, 340, "220 + 120 新内容");
        assert_eq!(outcome.img.width(), 200);
        assert_eq!(outcome.img.height(), 340);
        assert_eq!(
            outcome.confidence,
            StitchConfidence::Confirmed,
            "唯一命中应为 confirmed"
        );
    }

    /// 带固定头尾的拼接：新屏顶部 20px / 底部 16px 与上一屏同位行相同（固定区），
    /// 应被剔除后再拼接 —— 重复的固定区不得进入累积图。
    #[test]
    fn append_strips_fixed_regions() {
        // base：底部 600px 搜索带内放 140..220 为「上一屏视口底部」的模拟
        let base = grad(200, 220, 0);
        // 构造 new：顶部 20 行 = base 的 y∈[140,160)（固定头不随滚动变），
        // 中间是滚动内容，底部 16 行 = base 的 y∈[204,220)（固定尾）。
        // 探针（new 剔除固定头后 y=h/8）应能在 base 底部带内匹配。
        let mut new_img = RgbaImage::new(200, 200);
        // 固定头 20 行：直接复制 base 顶部内容（任何稳定内容都行）
        for dy in 0..20 {
            let row = imageops::crop_imm(&base, 0, dy, 200, 1).to_image();
            new_img.copy_from(&row, 0, dy).unwrap();
        }
        // 滚动内容：新图 y∈[20,184) = base 的 y∈[140,220)+后续新行，取 164px
        // 重叠段：y∈[20,100) = base y∈[140,220)（80px 重合）
        for dy in 20..100 {
            let row = imageops::crop_imm(&base, 0, 140 + (dy - 20), 200, 1).to_image();
            new_img.copy_from(&row, 0, dy).unwrap();
        }
        // 新内容：y∈[100,184)，延续散列序列
        for dy in 100..184 {
            let y_abs = 220 + (dy - 100);
            let r = ((y_abs * 97) % 251) as u16;
            let g = ((y_abs * 59) % 249) as u16;
            for x in 0..200 {
                let xr = ((x * 29) % 251) as u16;
                let xg = ((x * 17) % 249) as u16;
                let rr = ((r + xr) % 251) as u8;
                let gg = ((g + xg) % 249) as u8;
                new_img.put_pixel(x, dy, image::Rgba([rr, gg, 7, 255]));
            }
        }
        // 固定尾 16 行：复制 base 底部（稳定内容）
        for dy in 0..16 {
            let row = imageops::crop_imm(&base, 0, 204 + dy, 200, 1).to_image();
            new_img.copy_from(&row, 0, 184 + dy).unwrap();
        }

        let outcome = append_frame(&base, new_img, (20, 16)).expect("应能拼接");
        // 剔除 20+16 固定行后内容高 164，与 base 220 有 80px 重合 → 新增 84
        assert_eq!(outcome.total_h, 304, "220 + 84 新内容（固定区已剔除）");
        assert_eq!(outcome.img.height(), 304);
    }

    /// 固定区识别：顶部/底部连续相同行被识别、变化行处截断。
    #[test]
    fn detect_fixed_regions_counts_matching_rows() {
        // 两图顶部 30 行相同、底部 20 行相同、中间不同
        let top = grad(200, 30, 500);
        let bottom = grad(200, 20, 600);
        let mut a = grad(200, 220, 0);
        let mut b = grad(200, 220, 100);
        for dy in 0..30 {
            let row = imageops::crop_imm(&top, 0, dy, 200, 1).to_image();
            a.copy_from(&row, 0, dy).unwrap();
            b.copy_from(&row, 0, dy).unwrap();
        }
        for dy in 0..20 {
            let row = imageops::crop_imm(&bottom, 0, dy, 200, 1).to_image();
            a.copy_from(&row, 0, 200 + dy).unwrap();
            b.copy_from(&row, 0, 200 + dy).unwrap();
        }
        let (h, f) = detect_fixed_regions(&a, &b);
        // max_h = min(220,220)/3 = 73；顶部 30 行连续相同 → 识别 30（≥4 计入）
        assert!(h >= 28, "固定头应识别出 ~30 行，实际 {h}");
        assert!(f >= 18, "固定尾应识别出 ~20 行，实际 {f}");
    }

    /// 无固定区（两图完全不同）→ 识别结果应为 0。
    #[test]
    fn detect_fixed_regions_none_when_all_different() {
        let a = grad(200, 220, 0);
        let b = grad(200, 220, 100);
        let (h, f) = detect_fixed_regions(&a, &b);
        assert_eq!((h, f), (0, 0), "完全不同的两图不应识别出固定区");
    }

    /// 无重叠（内容完全不同）必须显式报错，不允许静默产出坏图。
    /// 探针行（new y=25，y0=100）与 base 全部 220 行均不匹配（已逐行枚举验证）。
    #[test]
    fn append_fails_without_overlap() {
        let base = grad(200, 220, 0);
        let new_img = grad(200, 200, 100);
        assert!(append_frame(&base, new_img, (0, 0)).is_err(), "找不到重叠时应报错");
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
        assert!(append_frame(&base, new_img, (0, 0)).is_err(), "无新内容时应报错");
    }

    /// 宽不一致的防御路径：新图先被等比缩放，不再 panic 即可（重叠与否取决于缩放结果）。
    #[test]
    fn append_handles_mismatched_width() {
        let base = grad(200, 220, 0);
        let new_img = grad(400, 200, 100);
        let scaled = imageops::resize(&new_img, 200, 100, imageops::FilterType::Triangle);
        let _ = append_frame(&base, scaled, (0, 0));
    }

    /// v1.1.0 置信分级：探针行在搜索带内**多行命中**（模拟重复版式——把 base 的
    /// 第 150 行复制到 180..190）时应判 Tentative，且仍取最靠下命中拼接。
    #[test]
    fn append_marks_tentative_on_multi_hit() {
        let mut base = grad(200, 220, 0);
        // 重复版式：把 row 150 复制到 rows 180..190（含），制造 11 个潜在命中
        let dup = imageops::crop_imm(&base, 0, 150, 200, 1).to_image();
        for dy in 0..11u32 {
            base.copy_from(&dup, 0, 180 + dy).unwrap();
        }
        // 真实对齐：new row dy = base row 125+dy（滚动 95px），probe_y=25 → row 150
        let mut new_img = RgbaImage::new(200, 200);
        for dy in 0..55u32 {
            let row = imageops::crop_imm(&base, 0, 125 + dy, 200, 1).to_image();
            new_img.copy_from(&row, 0, dy).unwrap();
        }
        // 新内容（虚拟行 220..365，与 base 无关的散列延续）
        for dy in 55..200u32 {
            let y_abs = 220 + (dy - 55);
            let r = ((y_abs * 97) % 251) as u16;
            let g = ((y_abs * 59) % 249) as u16;
            for x in 0..200u32 {
                let xr = ((x * 29) % 251) as u16;
                let xg = ((x * 17) % 249) as u16;
                let rr = ((r + xr) % 251) as u8;
                let gg = ((g + xg) % 249) as u8;
                new_img.put_pixel(x, dy, image::Rgba([rr, gg, 7, 255]));
            }
        }
        let outcome = append_frame(&base, new_img, (0, 0)).expect("应能拼接");
        assert_eq!(
            outcome.confidence,
            StitchConfidence::Tentative,
            "探针行 150 在 base 有 11 处命中（150 与 180..190 复制行），应判 tentative"
        );
        assert_eq!(outcome.tentative_from, 220, "tentative 段起点 = 拼 base 高度");
    }

    /// v1.1.0 verify_tentative：段内容与新屏对应行**不吻合** → 截断回滚。
    /// 构造：base 尾部 40 行是被错误拼接的内容（与「真实页面」不符），
    /// 新屏覆盖该段且行行可对上「真实页面」→ 复核发现不吻合 → 回滚到段起点。
    #[test]
    fn verify_rolls_back_on_mismatch() {
        let truth = grad(200, 220, 0); // 「真实页面」
        let mut base = truth.clone();
        // 尾部 40 行是错误内容（tentative 拼错的部分）
        let wrong = grad(200, 40, 900);
        for dy in 0..40u32 {
            let row = imageops::crop_imm(&wrong, 0, dy, 200, 1).to_image();
            base.copy_from(&row, 0, 180 + dy).unwrap();
        }
        // 新屏：new_origin_in_base = 100，行内容 = truth 行 100+ny（覆盖段 180..220）
        let mut new_content = RgbaImage::new(200, 200);
        for ny in 0..120u32 {
            let row = imageops::crop_imm(&truth, 0, 100 + ny, 200, 1).to_image();
            new_content.copy_from(&row, 0, ny).unwrap();
        }
        // 其余 80 行随意（不影响段内比对）
        for ny in 120..200u32 {
            let row = imageops::crop_imm(&truth, 0, ny - 120, 200, 1).to_image();
            new_content.copy_from(&row, 0, ny).unwrap();
        }
        let (verdict, img, h) = verify_tentative(&base, 180, 100, &new_content);
        assert_eq!(verdict, VerifyVerdict::RolledBack, "段内容与新屏不吻合，必须回滚");
        assert_eq!(h, 180, "回滚到段起点");
        assert_eq!(img.height(), 180);
        // 回滚后头部应与「真实页面」前 180 行一致
        for y in [0u32, 90, 179] {
            let a = img.get_pixel(0, y);
            let b = truth.get_pixel(0, y);
            assert_eq!(a, b, "回滚保留的部分应原样来自 base");
        }
    }

    /// v1.1.0 verify_tentative：段内容与新屏**吻合** → 升级 confirmed（不回滚）。
    #[test]
    fn verify_keeps_on_match() {
        let truth = grad(200, 220, 0);
        let base = truth.clone(); // 尾部 40 行是正确内容
        let mut new_content = RgbaImage::new(200, 200);
        for ny in 0..120u32 {
            let row = imageops::crop_imm(&truth, 0, 100 + ny, 200, 1).to_image();
            new_content.copy_from(&row, 0, ny).unwrap();
        }
        for ny in 120..200u32 {
            let row = imageops::crop_imm(&truth, 0, ny - 120, 200, 1).to_image();
            new_content.copy_from(&row, 0, ny).unwrap();
        }
        let (verdict, img, h) = verify_tentative(&base, 180, 100, &new_content);
        assert_eq!(verdict, VerifyVerdict::Confirmed, "吻合时应升级 confirmed");
        assert_eq!(h, 220);
        assert_eq!(img.height(), 220);
    }

    /// v1.1.0 verify_tentative：新屏覆盖不到段尾（滚动太快）→ CannotVerify。
    /// v1.1.0 审计语义修正：不再「原样保留并当已定案」，调用方应整体放弃本屏；
    /// verify_tentative 本身保持 base 原样（借用返回，零拷贝）。
    #[test]
    fn verify_keeps_when_coverage_insufficient() {
        let truth = grad(200, 220, 0);
        let base = truth.clone();
        // 新屏只有 100 高：new_origin=100 → 覆盖 [100,200)，盖不到段尾 220
        let mut new_content = RgbaImage::new(200, 100);
        for ny in 0..100u32 {
            let row = imageops::crop_imm(&truth, 0, 100 + ny, 200, 1).to_image();
            new_content.copy_from(&row, 0, ny).unwrap();
        }
        let (verdict, img, h) = verify_tentative(&base, 180, 100, &new_content);
        assert_eq!(verdict, VerifyVerdict::CannotVerify, "覆盖不足时无法验证");
        assert_eq!(h, 220);
        assert_eq!(img.height(), 220);
    }
}
