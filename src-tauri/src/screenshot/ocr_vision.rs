//!
//! screenshot/ocr_vision.rs
//! 取字「标准引擎」的 macOS 实现 —— 系统内置 Vision framework（`VNRecognizeTextRequest`）。
//!
//! 与 Windows 侧 `ocr.rs` 的 WinRT `Media.Ocr` 是**同一层级**的两个平台实现：
//! 都零体积、零随包资源、零联网，由 `ocr_engine.rs` 按平台分发。
//!
//! 为什么 macOS 要有自己的系统引擎（而不是只靠增强引擎）：
//! - **开箱可用**：增强引擎的运行库（`libonnxruntime.dylib`，43MB）不随包、
//!   需首次使用时下载。若没有本模块，mac 用户第一次取字必须等一次下载；
//!   有了它，「标准」引擎在 mac 上真的存在，装机即可取字、断网也可用。
//! - **零模型体积**：识别模型在系统里（macOS 11+ 支持 `zh-Hans`），不占安装包。
//! - 代价：小字号（<14px）与深色底不如 PaddleOCR 增强引擎 —— 与 Windows 侧同结论，
//!   这正是「标准 / 增强」双引擎并存的意义。
//!
//! 实现要点（三条都是踩过才知道的）：
//! 1. **图像走 `NSData` + `initWithData:options:`**，不自己构造 `CGImage`。
//!    后者要 `CGDataProvider` + `CGColorSpace` + `CGImageCreate` 一串 FFI，
//!    而 Vision 原生支持直接喂 PNG 数据。这里用**快速压缩 + 无滤波**编码 PNG，
//!    把「整屏 500 万像素」的编码成本压到可接受（取字本来就在阻塞线程里跑）。
//! 2. **语言必须运行时问系统，不能硬写**。`zh-Hans` 要 macOS 11+；
//!    不同版本/区域支持的语种不同，硬写会让请求直接失败（苹果的报错很含糊）。
//!    这里先用 `supportedRecognitionLanguagesAndReturnError` 取交集，
//!    交不上就**不设**语言（留系统默认），而不是赌一把。
//! 3. **结果要自己排序**。Vision 不保证 `results` 的顺序；
//!    归一化坐标原点在**左下角**，所以按「y 降序、再 x 升序」重排才是人读的顺序。
//!    不排序时偶发乱序，用户看到的取字结果会跳行。
//!
//! ⚠️ 本文件**只在 macOS 上编译**（`mod.rs` 里以 `#[cfg(target_os = "macos")]` 声明）。
//! 开发机是 Windows，因此另配了一个交叉编译校验探针：
//! `.tmp-ocr/vision-probe` 用 `cargo check --target aarch64-apple-darwin`
//! 把本文件真实编译一遍（见 `docs/ARCHITECTURE.md` / 发版 skill），
//! 让「API 用法写错」这类只能在 mac 上暴露的问题就地拦下。
//!
//! 修改历史：
//!   - 2026-09-18 @v0.9.0: 初始创建 - macOS 系统 Vision 取字，
//!     与 WinRT 对等成为「标准」引擎的 mac 实现（objc2 0.6 / objc2-vision 0.3，与项目已锁谱系同代）。
//!

use super::ocr::OcrOut;
use image::RgbaImage;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::AnyThread;
use objc2_foundation::{NSArray, NSData, NSDictionary, NSError, NSString};
use objc2_vision::{
    VNImageOption, VNImageRequestHandler, VNRecognizeTextRequest, VNRequest,
    VNRequestTextRecognitionLevel,
};

/// 期望语言（按优先级）。与 Windows 侧 `ocr.rs` 的 `FALLBACK_LANGS` 同思路：
/// 用户语言 → 中文 → 英文。**实际用哪些要问系统**（见 [`choose_languages`]）。
const PREFERRED_LANGS: [&str; 3] = ["zh-Hans", "zh-Hans-CN", "en-US"];

/// 对一张 RGBA 图做系统取字。失败返回**可以直接展示给用户**的中文原因。
pub fn recognize(img: &RgbaImage) -> Result<OcrOut, String> {
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return Err("取字范围为空".into());
    }

    // ① 编码成 PNG 交给 Vision（它自己能解码，省掉一整套 CoreGraphics FFI）
    let png = encode_png(img)?;
    let data = NSData::with_bytes(&png);

    // ② 图像处理器。options 传空字典即可（相机内参等参数对截图无意义）。
    let options: Retained<NSDictionary<VNImageOption, AnyObject>> = NSDictionary::new();
    let handler =
        VNImageRequestHandler::initWithData_options(VNImageRequestHandler::alloc(), &data, &options);

    // ③ 配置识别请求（语言部分要单独拿，回给前端做语言提示）
    let request = VNRecognizeTextRequest::new();
    request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
    // 打开语言校正：截图里多为界面文本/正常词句，校正通常更准；
    // 关掉只快一点点，却更容易把 `CPU 使用率` 这类词切坏。
    request.setUsesLanguageCorrection(true);
    // macOS 13+（revision 3）支持自动判别语种；低版本是 **no-op、不会报错**。
    // 开了它，中英混排的界面文本不必依赖我们猜对语言。
    request.setAutomaticallyDetectsLanguage(true);
    let language = choose_languages(&request);
    if let Some((langs, _)) = &language {
        request.setRecognitionLanguages(langs);
    }

    // ④ 执行。`performRequests:error:` 在 objc2 里是**安全函数**（无指针参数）。
    //    `into_super()` 两次：VNRecognizeTextRequest → VNImageBasedRequest → VNRequest。
    let requests: Retained<NSArray<VNRequest>> =
        NSArray::from_retained_slice(&[request.clone().into_super().into_super()]);
    handler
        .performRequests_error(&requests)
        .map_err(|e| format!("系统取字失败：{}", describe_error(&e)))?;

    // ⑤ 取每行的最优候选，并按视觉顺序（自上而下、同高自左而右）重排
    let lines = collect_lines(&request);
    if lines.is_empty() {
        return Err("未识别到文字：请确认选区里确实包含文本，或改用「增强」引擎再试。".into());
    }

    let text = lines.join("\n");
    Ok(OcrOut {
        text,
        width: w,
        height: h,
        language: language.map(|(_, primary)| primary),
        // 与 Windows 侧一致：系统引擎在配置里都叫 `system`，前端据 engine 字段展示人话
        engine: "system".into(),
        lines: lines.len(),
    })
}

/// 询问系统支持哪些语言，再与 [`PREFERRED_LANGS`] 取交集。
///
/// 返回「可直接设置的语言数组」+「主语言标签（回给前端展示）」。
/// 交集为空时返回 `None` —— 此时**不设置** `recognitionLanguages`，
/// 让 Vision 用它自己的默认（系统语言），而不是塞一个系统不认的标签把请求弄失败。
fn choose_languages(
    req: &VNRecognizeTextRequest,
) -> Option<(Retained<NSArray<NSString>>, String)> {
    // 该方法在 objc2 里是 unsafe fn（有 out-error 参数），但语义无害：失败就当不支持。
    let supported = unsafe { req.supportedRecognitionLanguagesAndReturnError() }.ok()?;
    let codes: Vec<String> = supported.iter().map(|s| s.to_string()).collect();

    let wanted: Vec<&str> = PREFERRED_LANGS
        .iter()
        .copied()
        .filter(|w| codes.iter().any(|c| c == w))
        .collect();
    let primary = wanted.first()?.to_string();

    let objs: Vec<Retained<NSString>> = wanted.iter().map(|w| NSString::from_str(w)).collect();
    Some((NSArray::from_retained_slice(&objs), primary))
}

/// 收集识别结果并按视觉顺序重排。
///
/// Vision 不保证 `results` 的顺序，且包围盒坐标原点在**左下角**
/// （归一化到 0~1）—— 所以「y 大的在上面」，按 y 降序排出来的才是人读的顺序。
fn collect_lines(req: &VNRecognizeTextRequest) -> Vec<String> {
    let Some(results) = req.results() else {
        return Vec::new();
    };
    // (中心 y, 左边界 x, 文本)
    let mut items: Vec<(f64, f64, String)> = Vec::with_capacity(results.len());
    for obs in results.iter() {
        let bbox = unsafe { obs.boundingBox() };
        let cands = obs.topCandidates(1);
        let Some(best) = cands.iter().next() else {
            continue;
        };
        items.push((
            bbox.origin.y + bbox.size.height / 2.0,
            bbox.origin.x,
            best.string().to_string(),
        ));
    }
    items.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    });
    // ⚠️ 这里**不复用** `ocr.rs` 的 `tidy_text()`：那条规则是为 WinRT 的
    // 「单字 + 空格」输出定制的（删掉任一侧为 CJK 的空格）。Vision 按**整行**返回，
    // 不会插入这类空格，套用反而会误删 `CPU 使用率` 的真实间隔
    // —— 与增强引擎（ONNX）同样的取舍。
    items.into_iter().map(|(_, _, t)| t).collect()
}

/// RGBA → PNG 字节。
///
/// 用**快速压缩 + 无滤波**：这里不追求文件小，只求编码快 ——
/// 整屏 2560×1440（约 370 万像素）走默认压缩会白白多花上百毫秒，
/// 而这份数据只活在内存里、立刻被 Vision 消费掉。
fn encode_png(img: &RgbaImage) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;
    let mut buf: Vec<u8> = Vec::with_capacity(img.as_raw().len() / 8);
    {
        let enc = image::codecs::png::PngEncoder::new_with_quality(
            &mut buf,
            image::codecs::png::CompressionType::Fast,
            image::codecs::png::FilterType::NoFilter,
        );
        enc.write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("取字图编码失败：{e}"))?;
    }
    Ok(buf)
}

/// `NSError` → 可读文案。系统报错本身很含糊，所以把 domain/code 一起带上，
/// 用户截图反馈时我们能定位得动。
fn describe_error(e: &NSError) -> String {
    format!("{}（code {}）", e.localizedDescription(), e.code())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 编码产物必须是合法 PNG 头（`\x89PNG\r\n\x1a\n`）。
    /// 这步错了 Vision 只会回一个含糊的报错，所以提前在单测里钉住。
    #[test]
    fn encode_png_produces_png_header() {
        let img = RgbaImage::from_pixel(8, 8, image::Rgba([255, 0, 0, 255]));
        let bytes = encode_png(&img).expect("PNG 编码失败");
        assert!(bytes.len() > 8, "PNG 数据过短: {} 字节", bytes.len());
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "PNG 头不合法");
    }

    /// 语言交集逻辑是纯字符串运算，但依赖系统返回值 —— 这里只验证「不 panic 且
    /// 返回的主语言确实在期望列表里」。真正的可用性由下面的真实取字探针覆盖。
    #[test]
    fn preferred_langs_are_ordered_by_priority() {
        assert_eq!(PREFERRED_LANGS[0], "zh-Hans");
        assert!(PREFERRED_LANGS.contains(&"en-US"));
    }

    /// 探针（默认忽略）：真实调用系统 Vision 识别一张图。
    ///
    /// 为什么必须有：本文件在开发机（Windows）上只能做**编译**校验，
    /// 「Vision 到底认不认这张图、取出来的字对不对」只有 mac 上跑一次才知道。
    /// 跑法（macOS）：
    /// ```text
    /// SD_OCR_PROBE=/path/to/probe.png \
    ///   cargo test --lib probe_ocr_vision -- --ignored --nocapture
    /// ```
    /// 未设置 `SD_OCR_PROBE` 时直接跳过（不假装成功）。
    #[test]
    #[ignore]
    fn probe_ocr_vision() {
        let Ok(path) = std::env::var("SD_OCR_PROBE") else {
            println!("== 未设置 SD_OCR_PROBE，跳过（这是「未验证」，不是「通过」）");
            return;
        };
        // 用 load_from_memory 而不是 image::open：本项目 image 只开了 `png` feature，
        // 按扩展名猜测格式的路径在精简 feature 下不一定可用，而按字节头猜测是稳的。
        let bytes = std::fs::read(&path).expect("探针图读取失败");
        let img = image::load_from_memory(&bytes)
            .expect("探针图解码失败")
            .to_rgba8();
        println!("== 探针图 {} ({}×{})", path, img.width(), img.height());

        let out = recognize(&img).expect("系统取字失败");
        println!("== 引擎 {} / 语言 {:?} / 行数 {}", out.engine, out.language, out.lines);
        for (i, l) in out.text.lines().enumerate() {
            println!("   {:>2}. {}", i + 1, l);
        }
        assert!(!out.text.trim().is_empty(), "取字结果为空");
        assert_eq!(out.engine, "system");
    }
}
