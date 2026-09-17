//!
//! screenshot/ocr.rs
//! 截图「取字」：对选区做**本地离线** OCR（v0.8.1）。
//!
//! 设计：
//! - **离线优先、零外部接口**：Windows 直接用系统内置的 WinRT `Windows.Media.Ocr`
//!   （Win10 1507 起随系统提供，识别能力随已安装的语言包走，不上传任何数据）。
//!   macOS 计划走系统 Vision、Linux 计划走 Tesseract —— 本期未落地，
//!   这两端返回**明确可读的原因**，而不是静默失败或假成功。
//! - **裁剪留在 Rust 做**：整屏帧本来就缓存在 `ScreenshotState.frame` 里，
//!   前端只传选区矩形，避免把几 MB 的 PNG 再从 IPC 搬回来一趟。
//! - **必须跑在独立线程**：`IAsyncOperation::join()` 是同步等待，且 COM 初始化是
//!   线程级状态 —— 两者都要求「初始化 COM 与调用 WinRT 在同一线程」，
//!   所以整段逻辑放在 `spawn_blocking` 里（见 `mod.rs::screenshot_ocr`）。
//!
//! 踩过的坑（都实测过，别再踩第二遍）：
//! - WinRT 激活（`RoGetActivationFactory`）要求调用线程已初始化 COM，否则后续调用
//!   一律 `CO_E_NOTINITIALIZED (0x800401F0)`。tokio 线程池线程不带 COM，必须自己在
//!   同一线程 `CoInitializeEx(None, COINIT_MULTITHREADED)`，用完 `CoUninitialize()`。
//! - OCR **只接受 Bgra8**：喂 Rgba8 不是「识别质量差」，而是直接拒绝处理 → 自己换序。
//! - `SoftwareBitmap` 只能从 `IBuffer` 构造，而内存造 `IBuffer` 最省事的路是
//!   `DataWriter::WriteBytes` + `DetachBuffer()`（crate 没提供 from_vec 之类的捷径）。
//! - `OcrResult::Text()` 是**按单字词用空格拼接**的，中文结果里每个汉字之间都是空格
//!   （`OCR 的 识 别 率 很 低`）。文字本身没错，但复制出来是断的，观感等同「识别率低」，
//!   必须过一遍 `tidy_text()` 把紧邻 CJK 的空格删掉。
//!
//! 已实测**无效/有害**的「提识别率」手段（别再试，见 `probe_ocr_scales` 复现）：
//! - **放大**：2× 用 Nearest 结果与 1× **逐字一致**（零收益）；2× Triangle 反而在句末多出
//!   错误标点；3×/4× 更差（Lanczos 重采样的糊化会喂坏引擎）。
//!   结论：**不做缩放**。
//! - **灰度化**（去掉 ClearType 子像素彩边）：1× 灰度、2× 灰度均与 1× 一致 → 无收益。
//! - **裁剪留边**（给引擎上下文）：pad 4/8/16px 均**有害**——把左邻 UI 的文字吃进来变成
//!   `g` / `lg` / `»ng` 这类噪声前缀。结论：**按用户选区精确裁剪**，不擅自外扩。
//!
//! 真正无法消除的两类丢字（如实标注，属引擎/字体层面）：
//! 1. **反色字形**：被「文字选中高亮」覆盖的字是蓝底白字，与周围深字浅底**极性相反**，
//!    引擎二值化时直接丢掉（实测 `方案` → `案`）。用户避开高亮再取字即可。
//! 2. **形近字混淆**：等宽西文字体里 `I`/`l` 近乎同形，`Icon` 会被读成 `lcon`。
//!
//! 修改历史：
//!   - 2026-09-17 @v0.8.1: 初始创建 - Windows WinRT Media.Ocr 取字；非 Windows 明确降级
//!   - 2026-09-17 @v0.8.1: 修复 - 新增 `tidy_text()` 去掉中文单字间多余空格（附单测）
//!   - 2026-09-17 @v0.8.1: 记录 - 实测并否决「放大 / 灰度 / 留边」三种提识别率手段（附探针）
//!

use image::RgbaImage;

/// 取字结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct OcrOut {
    /// 识别出的完整文本（多行以 \n 分隔，顺序与系统识别出的行序一致）
    pub text: String,
    /// 参与识别的图像尺寸（物理像素），回给前端做范围提示
    pub width: u32,
    pub height: u32,
    /// 本次真正生效的识别语言标签（如 zh-Hans-CN），非 Windows 端为空
    pub language: Option<String>,
}

/// 对一张 RGBA 图做 OCR。失败返回**可以直接展示给用户**的中文原因。
pub fn recognize(img: &RgbaImage) -> Result<OcrOut, String> {
    if img.width() == 0 || img.height() == 0 {
        return Err("取字范围为空".into());
    }
    platform::recognize(img)
}

// ===== 结果后处理（跨平台，与 WinRT 用法解耦，便于单测） =====

/// 字符是否属于「CJK 语境」——汉字、中文标点、全角符号。
fn is_cjk(c: char) -> bool {
    matches!(
        c as u32,
        0x2E80..=0x2EFF      // CJK 部首补充
            | 0x3000..=0x303F // CJK 标点（、。《》「」等）
            | 0x3400..=0x4DBF // 扩展 A
            | 0x4E00..=0x9FFF // 基本区（最常用汉字）
            | 0xF900..=0xFAFF // 兼容汉字
            | 0xFE30..=0xFE4F // CJK 兼容形式
            | 0xFF00..=0xFFEF // 全角字符（，？！：；等）
            | 0x20000..=0x2FA1F // 扩展 B~F
    )
}

/// 整理 OCR 原始文本：**只在「紧邻 CJK」的位置丢掉空格**。
///
/// 为什么必须做：WinRT `OcrResult::Text()` 是「按行内的词、用空格拼接」得到的，
/// 而中文会被切成**单字词**，于是每个汉字之间都会多出一个空格 ——
/// 实测 `OCR的Icon里面的那个A不居中` 会变成
/// `OCR 的 lcon 里 面 的 那 个 A 不 居 中`。文字本身是对的，
/// 但复制出来是断开的，看起来就像「识别率很低」。
///
/// 规则：某个空格只要**前一个非空格字符或后一个非空格字符**是 CJK，就删掉它。
/// 纯拉丁文本（`Hello World`、`OCR Engine`）两侧都不是 CJK，空格原样保留。
fn tidy_text(raw: &str) -> String {
    // 统一换行，避免 \r\n 混进来
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let chars: Vec<char> = normalized.chars().collect();
    let mut out = String::with_capacity(normalized.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == ' ' {
            let prev = out.chars().last();
            let next = chars[i + 1..].iter().find(|x| **x != ' ');
            let cjk_ctx = prev.is_some_and(is_cjk) || next.is_some_and(|n| is_cjk(*n));
            if cjk_ctx {
                i += 1;
                continue; // 丢弃这个空格
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

// ===== Windows：系统 WinRT OCR =====

#[cfg(target_os = "windows")]
mod platform {
    use super::{tidy_text, OcrOut};
    use image::RgbaImage;
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::Streams::DataWriter;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

    /// 语言包兜底顺序：用户语言 → 中文 → 英文
    const FALLBACK_LANGS: [&str; 3] = ["zh-Hans-CN", "zh-Hans", "en-US"];

    pub fn recognize(img: &RgbaImage) -> Result<OcrOut, String> {
        let (w, h) = (img.width(), img.height());

        // COM 是线程级状态，必须在真正做 WinRT 调用的这个线程里初始化。
        // 重复初始化返回 S_FALSE、模式冲突返回 RPC_E_CHANGED_MODE —— 这两种都说明
        // 该线程已经有可用的 COM，**不该由本次调用去 CoUninitialize**，所以只在
        // 「本次初始化成功（S_OK）」时收尾。
        let inited_here = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
        let out = recognize_inner(img, w, h);
        if inited_here {
            unsafe { CoUninitialize() };
        }
        out
    }

    fn recognize_inner(img: &RgbaImage, w: u32, h: u32) -> Result<OcrOut, String> {
        // 系统上限（通常 10000）。超限直接给一句人话，别让引擎抛一句看不懂的 HRESULT。
        if let Ok(max) = OcrEngine::MaxImageDimension() {
            if max > 0 && (w > max || h > max) {
                return Err(format!("取字范围过大（系统上限 {max}px），请缩小选区后重试"));
            }
        }

        // RGBA → BGRA：OCR 只吃 Bgra8。截图整屏不透明，alpha 原样搬运即可。
        let mut bgra: Vec<u8> = Vec::with_capacity((w as usize) * (h as usize) * 4);
        for px in img.as_raw().chunks_exact(4) {
            bgra.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }

        let writer = DataWriter::new().map_err(|e| format!("准备图像缓冲失败：{e}"))?;
        writer
            .WriteBytes(&bgra)
            .map_err(|e| format!("写入图像缓冲失败：{e}"))?;
        let buffer = writer
            .DetachBuffer()
            .map_err(|e| format!("取出图像缓冲失败：{e}"))?;

        let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
            &buffer,
            BitmapPixelFormat::Bgra8,
            w as i32,
            h as i32,
        )
        .map_err(|e| format!("构造位图失败：{e}"))?;

        let (engine, lang) = create_engine()?;

        // IAsyncOperation::join()：同步等到识别结束（windows-future >=0.3 的固有方法；
        // 0.2 时代叫 get()，已被改名）。内部靠 SetCompleted + Waiter，不做忙等 ——
        // 所以本函数必须在非 UI、非 async 线程跑（见 mod.rs 的 spawn_blocking）。
        let result = engine
            .RecognizeAsync(&bitmap)
            .map_err(|e| format!("启动识别失败：{e}"))?
            .join()
            .map_err(|e| format!("识别失败：{e}"))?;

        let raw = result
            .Text()
            .map_err(|e| format!("读取识别结果失败：{e}"))?
            .to_string_lossy();
        // 去掉中文单字之间的多余空格（见 tidy_text 的说明）
        let text = tidy_text(&raw);

        Ok(OcrOut {
            text,
            width: w,
            height: h,
            language: Some(lang),
        })
    }

    /// 选识别语言。引擎按调用创建（实测 1–5ms），不做全局缓存 ——
    /// 缓存要把 `OcrEngine` 塞进全局状态，而它绑定 COM 线程，收益不抵复杂度。
    fn create_engine() -> Result<(OcrEngine, String), String> {
        // ① 用户配置的语言：多数情况命中，且是「用户本来就看得懂」的那一种
        if let Ok(engine) = OcrEngine::TryCreateFromUserProfileLanguages() {
            let tag = engine
                .RecognizerLanguage()
                .and_then(|l| l.LanguageTag())
                .map(|s| s.to_string())
                .unwrap_or_default();
            return Ok((engine, tag));
        }
        // ② 兜底：中文 → 英文
        for tag in FALLBACK_LANGS {
            let hstr = windows::core::HSTRING::from(tag);
            if let Ok(lang) = windows::Globalization::Language::CreateLanguage(&hstr) {
                if let Ok(engine) = OcrEngine::TryCreateFromLanguage(&lang) {
                    return Ok((engine, tag.to_string()));
                }
            }
        }
        Err("系统里没有可用的 OCR 识别语言包：请到「设置 → 时间和语言 → 语言和区域」\
             给中文或英文语言添加可选功能「光学字符识别(OCR)」后重试"
            .into())
    }
}

// ===== 其它平台：明确降级 =====

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::OcrOut;
    use image::RgbaImage;

    pub fn recognize(_img: &RgbaImage) -> Result<OcrOut, String> {
        // 不假装成功、也不静默失败：把「为什么不行 + 后续计划」直接说清楚。
        Err(if cfg!(target_os = "macos") {
            "当前版本的取字功能仅支持 Windows（macOS 计划接入系统 Vision，尚未落地）".into()
        } else {
            "当前版本的取字功能仅支持 Windows".to_string()
        })
    }
}

// ===== 单测（与平台无关，可在任意平台跑） =====

#[cfg(test)]
mod tests {
    use super::{is_cjk, tidy_text};

    #[test]
    fn drops_spaces_between_cjk() {
        // WinRT 真实输出形态：每个汉字之间都有一个空格
        assert_eq!(
            tidy_text("OCR 的 lcon 里 面 的 那 个 A 不 居 中"),
            "OCR的lcon里面的那个A不居中"
        );
    }

    #[test]
    fn keeps_spaces_between_latin_words() {
        // 纯拉丁词之间的空格必须保留，否则 "Hello World" 会被粘成一个词
        assert_eq!(tidy_text("Hello World"), "Hello World");
        assert_eq!(tidy_text("OCR Engine v0.8.1"), "OCR Engine v0.8.1");
    }

    #[test]
    fn glues_cjk_punctuation() {
        assert_eq!(
            tidy_text("有 什 么 优 化 方 案 吗 ？ 截 图"),
            "有什么优化方案吗？截图"
        );
    }

    #[test]
    fn keeps_newlines_and_normalizes_crlf() {
        assert_eq!(tidy_text("第一 行\r\n第二 行"), "第一行\n第二行");
    }

    #[test]
    fn mixed_latin_in_cjk() {
        assert_eq!(tidy_text("使 用 OCR 提 取 文 字"), "使用OCR提取文字");
    }

    #[test]
    fn is_cjk_covers_common_ranges() {
        assert!(is_cjk('汉'));
        assert!(is_cjk('，'));
        assert!(is_cjk('？'));
        assert!(!is_cjk('A'));
        assert!(!is_cjk(' '));
        assert!(!is_cjk('1'));
    }

    /// 真机调参探针（默认 `#[ignore]`，不进 CI）：同一块裁剪图在多组预处理下的识别差异。
    ///
    /// 用法（Windows，需已装中文 OCR 语言包）：
    /// ```text
    /// SD_OCR_PROBE=scripts/_frame.png SD_OCR_RECT="760,798,860,48" \
    ///   cargo test --release -- --ignored --nocapture probe_ocr_scales
    /// ```
    /// 用来回答「放大 / 留边 / 灰度 到底有没有用」——**先测再定策略**，不拍脑袋。
    #[test]
    #[ignore]
    fn probe_ocr_scales() {
        use image::imageops::FilterType;
        use image::RgbaImage;

        let Ok(path) = std::env::var("SD_OCR_PROBE") else {
            println!("未设置 SD_OCR_PROBE，跳过");
            return;
        };
        let rect = std::env::var("SD_OCR_RECT").unwrap_or_else(|_| "0,0,600,100".into());
        let n: Vec<u32> = rect
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        let (x, y, w, h) = (n[0], n[1], n[2], n[3]);

        let img = image::open(&path).expect("打开探针图失败").to_rgba8();
        println!("原图 {}x{}，裁剪 ({x},{y}) {w}x{h}", img.width(), img.height());
        let crop = image::imageops::crop_imm(&img, x, y, w, h).to_image();

        let scale = |src: &RgbaImage, s: u32, f: FilterType| -> RgbaImage {
            if s == 1 {
                src.clone()
            } else {
                image::imageops::resize(src, src.width() * s, src.height() * s, f)
            }
        };
        // 转灰：OCR 面向「墨迹/背景」二值化，子像素彩边（ClearType）有时是干扰源
        let gray = |src: &RgbaImage| -> RgbaImage {
            RgbaImage::from_fn(src.width(), src.height(), |px, py| {
                let p = src.get_pixel(px, py).0;
                let l = ((p[0] as u32 * 299 + p[1] as u32 * 587 + p[2] as u32 * 114) / 1000) as u8;
                image::Rgba([l, l, l, p[3]])
            })
        };

        let variants: Vec<(&str, RgbaImage)> = vec![
            ("1x 原样", scale(&crop, 1, FilterType::Nearest)),
            ("2x Nearest", scale(&crop, 2, FilterType::Nearest)),
            ("2x Triangle", scale(&crop, 2, FilterType::Triangle)),
            ("1x 灰度", gray(&crop)),
            ("2x Nearest 灰度", gray(&scale(&crop, 2, FilterType::Nearest))),
        ];

        for (name, im) in variants {
            let tag = match super::platform::recognize(&im) {
                Ok(o) => format!("{} 字: {}", o.text.chars().count(), o.text),
                Err(e) => format!("失败: {e}"),
            };
            println!("\n--- {name} ({}x{}) ---\n{tag}", im.width(), im.height());
        }

        // 留边：给引擎一点上下文再裁回来（左右各 pad px）
        for pad in [4u32, 8, 16] {
            let x0 = x.saturating_sub(pad);
            let y0 = y.saturating_sub(pad);
            let x1 = (x + w + pad).min(img.width());
            let y1 = (y + h + pad).min(img.height());
            let padded = image::imageops::crop_imm(&img, x0, y0, x1 - x0, y1 - y0).to_image();
            let tag = match super::platform::recognize(&padded) {
                Ok(o) => format!("{} 字: {}", o.text.chars().count(), o.text),
                Err(e) => format!("失败: {e}"),
            };
            println!("\n--- 留边 {pad}px ---\n{tag}");
        }
    }
}
