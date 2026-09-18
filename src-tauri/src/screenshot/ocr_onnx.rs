//!
//! screenshot/ocr_onnx.rs
//! 「取字」增强引擎（v0.9.0）：PaddleOCR v4 ONNX 本地推理（RapidOCR 同源模型）。
//!
//! 为什么要有第二引擎（对照 ocr.rs 的 WinRT 结论）：
//! - WinRT `Media.Ocr` 对**小字号（<14px）/低对比**场景丢字严重，且已实测
//!   「放大 / 灰度 / 留边」三种预处理全部无效（见 ocr.rs 头部否决记录）——
//!   说明是**引擎本身**的天花板，不是输入问题。
//! - PP-OCRv4 的 det（DBNet）在预处理时把**短边不足 736px 的图整体放大**
//!   后再做像素级文本检测，小字在检测阶段就被拉大了 —— 这是 WinRT 链路
//!   做不到的「先检测后识别」两段式能力，也是识别率提升的真正来源。
//! - 依旧 100% 本地：模型文件 + onnxruntime.dll 全部落盘本机，零网络调用。
//!
//! 模型（RapidOCR v3.7.0 发布源，ModelScope，SHA256 已核验）：
//! - det: ch_PP-OCRv4_det_infer.onnx   ~4.7MB  d2a7720d…49da9
//! - rec: ch_PP-OCRv4_rec_infer.onnx   ~10.9MB 48fc40f2…3683b
//! - cls: ch_ppocr_mobile_v2.0_cls_infer.onnx（本期管线未接：UI 截图文本方向恒正）
//! - 字典: ppocr_keys_v1.txt（6623 字，include_str! 编译期内嵌）
//!
//! 简化边界（如实标注）：
//! - det 后处理取**水平包围盒**（连通域 bbox + unclip 外扩），不做旋转矩形 ——
//!   UI 截图文本基本水平；倾斜文本会以 bbox 裁剪进 rec，识别质量下降但不出错。
//! - rec 跳过方向分类（cls），不做行内多段拼接。
//! - 不复用 ocr.rs 的 `tidy_text`：PP-OCR 按行输出连续文本、无 WinRT 单字空格
//!   问题，而「删任一侧为 CJK 的空格」会误删 `CPU 使用率` 这类**真实**间隔。
//!
//! onnxruntime 动态库（load-dynamic）：
//! - `ort` crate 以 `features = ["load-dynamic"]` 引入，编译期不链接 ORT。
//! - **产品态用 `ort::init_from(绝对路径)` 显式加载**（见 `ensure_env`）：随包的 dll
//!   落在安装目录 `ort/onnxruntime.dll`，路径由 `ocr_engine.rs` 探测。
//!   不用环境变量 —— 写错名字（`ONNXRUNTIME_DYLIB_PATH` 而非 `ORT_DYLIB_PATH`）
//!   会被静默忽略并回落到 PATH 搜索，本机 PATH 里那个 1.17.1 旧版 dll 会直接
//!   BadVersion abort（2026-09-17 实测踩坑）。
//! - 探针/开发：仍可用 `ORT_DYLIB_PATH` 指向 pip 包内 `capi/onnxruntime.dll`。
//! - 版本兼容：运行库版本 ≥ ort 编译锚定版本即可（C API 向后兼容，1.30 实测可加载）。
//! - 运行库自身依赖 MSVC 运行库（MSVCP140 / VCRUNTIME140），
//!   缺失时 `init_from` 返回 Err → 由 `ensure_env` 转成「请装 VC++ 运行库」的人话。
//!
//! 会话缓存与性能（v0.9.0）：
//! - 模型加载 ~2.4s、首次推理含图优化 ~0.7s，合计 3.1s —— 每次取字重来一遍不可接受。
//! - 全局 `ENGINE` 常驻 det+rec 两个 Session（`Session` 自带内部互斥，可跨线程共享），
//!   二次取字只剩推理耗时。`Mutex` 同时满足 `recognize(&mut self)` 的独占借用。
//! - 图优化取 Level3（全量优化，加载稍慢但推理更快）—— 加载是一次性的，划算。
//! - 线程数取 4（见 `ort_threads`：8 核上开满反而慢一倍，附实测表）。
//! - 实测阶段耗时（`SD_OCR_TIME=1` 打印）：
//!   - 900×520 全图（det 1280×736 + 8 行 rec）：前处理 84ms、det 1033ms、DB 后处理 14ms、
//!     rec 1306ms、合计 **2.71s**（4 线程）
//!   - 682×44 小选区（det 2720×192 + 1 行）：前处理 33ms、det 663ms、rec 193ms、
//!     合计 **0.97s** —— 放大上限（见 `DET_MAX_UPSCALE`）把小选区的 det 从 7 秒压到 0.66 秒
//!   - 结论：**耗时几乎全在 ORT 推理**；我们自己的前处理与 DB 后处理合计 <100ms，
//!     不是优化方向（这条结论同样来自实测，别再凭直觉改这两段）
//!
//! 修改历史：
//!   - 2026-09-17 @v0.9.0: 初始创建 - det/rec 两段管线 + DB 水平框后处理 + CTC 解码
//!   - 2026-09-17 @v0.9.0: 性能 - 全局会话缓存 + Level3 图优化 + 放大/面积上限 + 线程数定点 4（附实测）
//!   - 2026-09-17 @v0.9.0: 运行库 - 改为 `ort::init_from` 显式加载随包 dll，弃用环境变量
//!

use image::{DynamicImage, RgbaImage};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// det 模型文件名（`is_ready` / 缓存键 / 资源探测三处共用，避免拼错）
pub const DET_FILE: &str = "ch_PP-OCRv4_det_infer.onnx";
/// rec 模型文件名
pub const REC_FILE: &str = "ch_PP-OCRv4_rec_infer.onnx";

/// 进程级常驻引擎（会话缓存）：`(模型目录, 引擎)`。
/// 目录变了要重载 —— 探针里换 `SD_OCR_MODELS` 就能切模型，不必重启进程。
static ENGINE: Mutex<Option<(PathBuf, OnnxOcr)>> = Mutex::new(None);

/// onnxruntime 环境初始化结果：只缓存**成功**。
/// 失败不缓存 —— 用户装完 VC++ 运行库后可以原地重试，不必重启应用。
static ENV_READY: Mutex<Option<()>> = Mutex::new(None);

/// 模型文件是否齐备（不含运行库检查 —— 那是 `ocr_engine.rs` 的职责）
pub fn is_ready(models_dir: &Path) -> bool {
    models_dir.join(DET_FILE).is_file() && models_dir.join(REC_FILE).is_file()
}

/// 初始化 onnxruntime 环境（幂等）。`lib` 为 None 时回落到 ort 的默认搜索
/// （环境变量 `ORT_DYLIB_PATH` → PATH），供开发/探针使用。
pub fn ensure_env(lib: Option<&Path>) -> Result<(), String> {
    {
        let g = ENV_READY.lock().unwrap_or_else(|e| e.into_inner());
        if g.is_some() {
            return Ok(());
        }
    }
    let builder = match lib {
        Some(p) => ort::init_from(p).map_err(|e| {
            format!(
                "加载 onnxruntime 运行库失败（{}）：{e}\n\
                 若提示缺少 DLL，请安装「Microsoft Visual C++ 2015-2022 运行库 (x64)」后重试。",
                p.display()
            )
        })?,
        None => ort::init(),
    };
    // commit() 返回 bool：true = 本次提交成功，false = 进程内已有环境（重复提交，非错误）。
    // 真正的加载失败在上面的 `init_from` 就已返回 Err（load-dynamic 在 init_from 阶段载库）。
    if !builder.commit() {
        tracing::debug!("onnxruntime 环境此前已配置，本次跳过");
    }
    let mut g = ENV_READY.lock().unwrap_or_else(|e| e.into_inner());
    *g = Some(());
    Ok(())
}

/// 预热：把会话加载到常驻缓存（不推理）。遮罩窗打开时后台调用，
/// 用户点「取字」时就不用等 2.5s 的模型加载。
pub fn warmup(models_dir: &Path, lib: Option<&Path>) -> Result<(), String> {
    ensure_env(lib)?;
    let mut g = ENGINE.lock().unwrap_or_else(|e| e.into_inner());
    ensure_loaded(&mut g, models_dir)?;
    Ok(())
}

/// 带缓存的识别：首次调用加载模型，之后复用。
pub fn recognize_cached(
    models_dir: &Path,
    lib: Option<&Path>,
    img: &RgbaImage,
) -> Result<Vec<OcrLine>, String> {
    ensure_env(lib)?;
    let mut g = ENGINE.lock().unwrap_or_else(|e| e.into_inner());
    ensure_loaded(&mut g, models_dir)?;
    // 上面 ensure_loaded 保证 Some；用 match 而非 unwrap，避免万一 panic 掉整个应用
    match g.as_mut() {
        Some((_, eng)) => eng.recognize(img),
        None => Err("增强引擎未就绪".into()),
    }
}

/// 缓存命中则复用，目录变了 / 首次则重新加载
fn ensure_loaded(slot: &mut Option<(PathBuf, OnnxOcr)>, models_dir: &Path) -> Result<(), String> {
    let hit = matches!(slot.as_ref(), Some((p, _)) if p == models_dir);
    if hit {
        return Ok(());
    }
    let t0 = std::time::Instant::now();
    let eng = OnnxOcr::load(models_dir)?;
    tracing::info!(
        cost_ms = t0.elapsed().as_millis() as u64,
        dir = %models_dir.display(),
        "增强引擎模型加载完成（常驻缓存）"
    );
    *slot = Some((models_dir.to_path_buf(), eng));
    Ok(())
}

/// ppocr_keys_v1.txt：6623 行，每行一个字符（编译期内嵌，零运行时路径依赖）
const DICT_TXT: &str = include_str!("ppocr_keys_v1.txt");

/// det 预处理参数（对齐 RapidOCR 默认：limit_type=min, limit_side_len=736）
const DET_LIMIT_SIDE: f32 = 736.0;
/// det 放大倍数上限（v0.9.0 性能，实测确定）。
///
/// RapidOCR 默认把「短边不足 736px 的图」放大到短边 736 —— 选区越小放大越狠：
/// 一块 682×44 的文字选区要放大 **16.7 倍**，det 输入变成 11392×736，单次检测 **7 秒**。
/// 实测（`.tmp-ocr/probe_ratio.py`，两组小选区 × 7 档上限）：
///
/// | 上限 | 682×44 选区 det 输入 | det 耗时 | 识别结果 |
/// |---|---|---|---|
/// | 1.0x | 672×32 | 38ms | ❌ 直接漏检（字太小） |
/// | 2.0x | 1376×96 | 153ms | ✅ 正确 |
/// | 3.0x | 2048×128 | 270ms | ⚠️ 正确但有 1 处形近字混淆 |
/// | 4.0x | 2720×192 | 479ms | ✅ 正确 |
/// | 16.7x（原行为） | 11392×736 | **7066ms** | ✅ 正确（且深色小字反而被切成两行） |
///
/// 结论：**4 倍是准确率与耗时的拐点** —— 再往上耗时线性涨、准确率不再提升。
/// 取 4.0 而非 3.0 是留余量（3.0 在实测里出现过一次 `引擎`→`引l擎`）。
/// 注意这只影响**小选区**：全图级选区本来就不放大（ratio < 1.0），行为与 v0.8.1 探针基线完全一致。
const DET_MAX_UPSCALE: f32 = 4.0;
/// DB 二值化阈值（RapidOCR 默认 0.3）
const DET_THRESH: f32 = 0.3;
/// 文本框得分阈值（连通域内概率均值，RapidOCR 默认 0.5）
const DET_BOX_THRESH: f32 = 0.5;
/// unclip 扩张系数（RapidOCR 默认 1.6）
const DET_UNCLIP: f32 = 1.6;
/// rec 输入高度（PP-OCRv4 rec 为 48，v3 及以前是 32 —— 别写错）
const REC_H: u32 = 48;
/// rec 单行最大宽度（对应约 120 字符，超出截断；UI 文本行远达不到）
const REC_MAX_W: u32 = 960;
/// det 输入的总像素上限（约 2.6MP）。
/// 取字选区通常远小于此；但用户可能框住整屏（1920×1080 → 2.07MP，还算可控），
/// 4K 屏整屏就是 8.3MP —— det 推理耗时与面积成正比，不设上限会让「手滑框全屏」
/// 变成十几秒的卡顿。超过上限就等比缩回（牺牲一点极小字，换可预期的最坏耗时）。
const DET_MAX_PIXELS: f32 = 2_640_000.0;

/// det 输入尺寸与放大比例。**纯函数**，便于单测钉死策略（避免只有耗时断言时随机器噪声假失败）。
///
/// 三层约束，按顺序施加：
/// 1. **下限**：短边不足 736px 就放大（小字受益，RapidOCR 默认策略）
/// 2. **放大上限**：不超过 `DET_MAX_UPSCALE` 倍（小选区的主要成本来源）
/// 3. **面积上限**：不超过 `DET_MAX_PIXELS`（防 4K 整屏框选把 det 拖成十几秒）
///
/// ⚠️ 32 对齐的运算顺序：先取「块数」再 ×32，最后才 `max(1)` ——
/// 若写成 `.max(32) * 32` 会把**块数**顶到 32（=1024px），图像被纵向拉伸 1.39 倍、
/// 全部下游坐标错乱（2026-09-17 探针实测踩坑，别再改回去）。
fn det_input_size(w0: f32, h0: f32) -> (u32, u32, f32) {
    let short = w0.min(h0).max(1.0);
    let mut ratio = if short < DET_LIMIT_SIDE {
        DET_LIMIT_SIDE / short
    } else {
        1.0
    };
    ratio = ratio.min(DET_MAX_UPSCALE);
    let area = (w0 * ratio) * (h0 * ratio);
    if area > DET_MAX_PIXELS {
        ratio *= (DET_MAX_PIXELS / area).sqrt();
    }
    let rw = ((w0 * ratio / 32.0).round() as u32).max(1) * 32;
    let rh = ((h0 * ratio / 32.0).round() as u32).max(1) * 32;
    (rw, rh, ratio)
}

/// 阶段耗时打点开关（`SD_OCR_TIME=1`）。用 OnceLock 缓存，避免每次识别都读一次环境变量。
fn time_probe() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("SD_OCR_TIME").map(|v| v == "1").unwrap_or(false))
}

/// 推理线程数：默认 4，可用 `SD_OCR_THREADS` 覆盖（调优/探针用）。
///
/// **线程数是这台机器上最敏感的参数**，同一张 900×520 图（det 1280×736 + 8 行 rec）实测：
///
/// | 线程 | det | rec | 合计 |
/// |---|---|---|---|
/// | 1 | 1525ms | 3861ms | 5723ms |
/// | 2 | 940ms | 2637ms | 3887ms |
/// | **4** | **1033ms** | **1306ms** | **2710ms** ← 最优 |
/// | 8 | 2031ms | 2796ms | 5173ms |
///
/// 8 核机器上开满反而**慢一倍**：ORT 的 intra-op 线程池是**自旋等任务**的，
/// 超配会与自身、以及本应用的采样线程互相抢核。（一度顺手改成「用满可用核」，
/// 结果整体从 2.7s 掉到 5.2s —— 这条实测曲线就是为拦住这种「直觉优化」留的。）
/// 留出环境变量口子是为了调参时不必反复重新编译。
fn ort_threads() -> usize {
    if let Ok(v) = std::env::var("SD_OCR_THREADS") {
        if let Ok(n) = v.trim().parse::<usize>() {
            return n.clamp(1, 32);
        }
    }
    std::thread::available_parallelism()
        .map(|n| n.get().clamp(2, 4))
        .unwrap_or(4)
}

/// 识别出的单行文本
#[derive(Debug, Clone)]
pub struct OcrLine {
    pub text: String,
    /// 原图坐标系（物理像素）水平包围盒
    pub x: i32,
    pub y: i32,
    // 下面两个字段当前只被探针（cfg(test)）读取，产品代码走的是「整段文本」出口。
    // 保留它们是为了后续做「逐行高亮 / 低置信度提示」时不必回头改管线，故显式允许未使用。
    #[allow(dead_code)]
    pub w: i32,
    pub h: i32,
    /// det 置信度（连通域概率均值，0~1）
    #[allow(dead_code)]
    pub score: f32,
}

/// ONNX 增强引擎（det + rec 两段）。线程安全：`Session` 内部有互斥，可跨线程共享。
pub struct OnnxOcr {
    det: ort::session::Session,
    rec: ort::session::Session,
    /// CTC 字符表：索引 0 = blank，1..=len = 字典字符，len+1 = 空格
    charset: Vec<String>,
}

impl OnnxOcr {
    /// 从模型目录加载 det + rec。
    pub fn load(models_dir: &Path) -> Result<Self, String> {
        let det_path = models_dir.join(DET_FILE);
        let rec_path = models_dir.join(REC_FILE);
        if !det_path.is_file() {
            return Err(format!("缺少检测模型：{}", det_path.display()));
        }
        if !rec_path.is_file() {
            return Err(format!("缺少识别模型：{}", rec_path.display()));
        }
        let det = ort::session::Session::builder()
            .map_err(|e| e.to_string())?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
            .map_err(|e| e.to_string())?
            .with_intra_threads(ort_threads())
            .map_err(|e| e.to_string())?
            .commit_from_file(&det_path)
            .map_err(|e| format!("加载 det 模型失败：{e}"))?;
        let rec = ort::session::Session::builder()
            .map_err(|e| e.to_string())?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
            .map_err(|e| e.to_string())?
            .with_intra_threads(ort_threads())
            .map_err(|e| e.to_string())?
            .commit_from_file(&rec_path)
            .map_err(|e| format!("加载 rec 模型失败：{e}"))?;
        Ok(Self {
            det,
            rec,
            charset: build_charset(),
        })
    }

    /// 全管线识别：RGBA 图 → 文本行（按 y 主序、x 次序）。
    pub fn recognize(&mut self, img: &RgbaImage) -> Result<Vec<OcrLine>, String> {
        let t_all = std::time::Instant::now();
        // ---------- 1. det：短边放大到 736 → 归一化 → DB 概率图 ----------
        let rgb = DynamicImage::ImageRgba8(img.clone()).to_rgb8();
        let (rw, rh, ratio) = det_input_size(img.width() as f32, img.height() as f32);
        let resized = image::imageops::resize(
            &rgb,
            rw,
            rh,
            image::imageops::FilterType::Triangle,
        );
        // 探针对照：dump det 输入图与 Python 版逐像素 diff（SD_OCR_DUMP=path.png）
        #[cfg(test)]
        if let Ok(p) = std::env::var("SD_OCR_DUMP") {
            let _ = resized.save(&p);
            println!("[dump] det 输入已存 {p} ({}x{})", resized.width(), resized.height());
        }
        let t_pre = std::time::Instant::now();
        let det_in = normalize_nchw(&resized, &DET_MEAN, &DET_STD);
        let norm_ms = t_pre.elapsed().as_millis() as u64;
        let t_det = std::time::Instant::now();
        let prob = self.run_det(det_in)?;
        let det_ms = t_det.elapsed().as_millis() as u64;

        // ---------- 2. DB 后处理：连通域 → 水平框 → unclip → 映射回原图 ----------
        let t_db = std::time::Instant::now();
        let boxes = db_boxes(&prob.0, prob.1, prob.2, ratio);
        let db_ms = t_db.elapsed().as_millis() as u64;

        // ---------- 3. rec：逐行裁剪 → 归一化 → CTC 解码 ----------
        let t_rec = std::time::Instant::now();
        let mut lines = Vec::with_capacity(boxes.len());
        for (bx, by, bw, bh, score) in boxes {
            let crop = image::imageops::crop_imm(
                &rgb,
                bx.max(0) as u32,
                by.max(0) as u32,
                bw.min(rgb.width() as i32 - bx) as u32,
                bh.min(rgb.height() as i32 - by) as u32,
            )
            .to_image();
            let text = self.run_rec(&crop)?;
            if !text.is_empty() {
                lines.push(OcrLine {
                    text,
                    x: bx,
                    y: by,
                    w: bw,
                    h: bh,
                    score,
                });
            }
        }
        let rec_ms = t_rec.elapsed().as_millis() as u64;
        // 排序：y 中心为主序（同行阈值 0.6×行高内按 x 排）
        lines.sort_by(|a, b| {
            let ac = (a.y as f32 + a.h as f32 / 2.0) as i32;
            let bc = (b.y as f32 + b.h as f32 / 2.0) as i32;
            let tol = ((a.h + b.h) / 2).max(1) * 3 / 5;
            if (ac - bc).abs() <= tol {
                a.x.cmp(&b.x)
            } else {
                ac.cmp(&bc)
            }
        });
        // 阶段耗时（SD_OCR_TIME=1 时打印）：一眼看清「慢在 det、rec 还是我们自己的前处理」。
        // 用 println 而非 tracing：探针（cargo test --nocapture）默认没有订阅器，
        // tracing 的 debug 日志根本不会输出，调优时会以为「没走这段代码」。
        if time_probe() {
            println!(
                "[time] det输入 {rw}x{rh} (x{ratio:.2})  前处理 {}ms  det {}ms  DB后处理 {}ms  rec {}ms  合计 {}ms  {} 行",
                norm_ms,
                det_ms,
                db_ms,
                rec_ms,
                t_all.elapsed().as_millis(),
                lines.len()
            );
        }
        Ok(lines)
    }

    /// det 推理：输入 [1,3,H,W]，输出 [1,1,H,W] 概率图（展平为 Vec + 尺寸）
    fn run_det(&mut self, input: Array4<f32>) -> Result<(Vec<f32>, usize, usize), String> {
        let tensor = ort::value::Tensor::from_array(input)
            .map_err(|e| format!("det 输入构造失败：{e}"))?;
        let outputs = self
            .det
            .run(ort::inputs![tensor])
            .map_err(|e| format!("det 推理失败：{e}"))?;
        let (name, value) = outputs
            .iter()
            .next()
            .ok_or("det 无输出")?;
        let _ = name;
        // ort 2.0.0-rc.13：try_extract_tensor 返回 (&Shape, &[f32])；Shape Deref 到 [i64]
        let (shape, data) = value
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("det 输出解析失败：{e}"))?;
        let oh = *shape.get(2).ok_or("det 输出 shape 异常")? as usize;
        let ow = *shape.get(3).ok_or("det 输出 shape 异常")? as usize;
        Ok((data.to_vec(), oh, ow))
    }

    /// rec 推理：行图（RGB）→ 文本（CTC 解码）
    fn run_rec(&mut self, crop: &image::RgbImage) -> Result<String, String> {
        if crop.width() < 2 || crop.height() < 2 {
            return Ok(String::new());
        }
        let rgb = crop;
        // 等比缩放到高 48；宽 clamp
        let scale = REC_H as f32 / rgb.height() as f32;
        let mut rw = (rgb.width() as f32 * scale).round() as u32;
        rw = rw.clamp(8, REC_MAX_W);
        let resized = image::imageops::resize(rgb, rw, REC_H, image::imageops::FilterType::Triangle);
        let input = normalize_nchw_05(&resized, rw as usize, REC_H as usize);
        let tensor = ort::value::Tensor::from_array(input)
            .map_err(|e| format!("rec 输入构造失败：{e}"))?;
        let outputs = self
            .rec
            .run(ort::inputs![tensor])
            .map_err(|e| format!("rec 推理失败：{e}"))?;
        let (_, value) = outputs
            .iter()
            .next()
            .ok_or("rec 无输出")?;
        // [1, T, C]：T = 时间步，C = 类别数（6625）
        let (shape, data) = value
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("rec 输出解析失败：{e}"))?;
        let t_len = *shape.get(1).ok_or("rec 输出 shape 异常")? as usize;
        let c_len = *shape.get(2).ok_or("rec 输出 shape 异常")? as usize;
        ctc_decode(data, t_len, c_len, &self.charset)
    }
}

/// det 归一化常量（ImageNet 均值/方差，PaddleOCR det 预处理标准值）
const DET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const DET_STD: [f32; 3] = [0.229, 0.224, 0.225];

use ndarray::Array4;

/// RGB 图 → NCHW f32，`(x/255 - mean) / std`（det 用）
fn normalize_nchw(img: &image::RgbImage, mean: &[f32; 3], std: &[f32; 3]) -> Array4<f32> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let px = img.as_raw();
    let mut out = Array4::<f32>::zeros((1, 3, h, w));
    let plane = h * w;
    for c in 0..3 {
        let (m, s) = (mean[c], std[c]);
        for i in 0..plane {
            out[[0, c, i / w, i % w]] = (px[i * 3 + c] as f32 / 255.0 - m) / s;
        }
    }
    out
}

/// RGB 图 → NCHW f32，`(x/255 - 0.5) / 0.5`（rec 用：归一化到 [-1,1]，pad 区即 -1）
fn normalize_nchw_05(img: &image::RgbImage, w: usize, h: usize) -> Array4<f32> {
    let mut out = Array4::<f32>::ones((1, 3, h, w)) * -1.0;
    let px = img.as_raw();
    for y in 0..h {
        for x in 0..w {
            let (xi, yi) = (x.min(img.width() as usize - 1), y.min(img.height() as usize - 1));
            let i = (yi * img.width() as usize + xi) * 3;
            for c in 0..3 {
                out[[0, c, y, x]] = px[i + c] as f32 / 255.0 - 0.5;
            }
        }
    }
    out
}

/// CTC 解码：argmax → 去 blank → 去相邻重复。data 为 [T, C] 展平。
fn ctc_decode(data: &[f32], t_len: usize, c_len: usize, charset: &[String]) -> Result<String, String> {
    let mut text = String::new();
    let mut last = 0usize;
    for t in 0..t_len {
        let row = &data[t * c_len..(t + 1) * c_len];
        let (mut best_i, mut best_v) = (0usize, f32::MIN);
        for (c, &v) in row.iter().enumerate() {
            if v > best_v {
                best_v = v;
                best_i = c;
            }
        }
        if best_i != 0 && best_i != last {
            // 类别布局 = [blank(0), 6623 字典字符(1..6623), space(6624)]——已用 Python
            // 旁路实验实测（charset[i] 直接映射输出正确文本；charset[i-1] 会整体
            // 偏移一位输出乱码，见 2026-09-17 探针记录）。charset[0] 为占位空串，
            // 由 i != 0 条件挡住，永不命中。
            match charset.get(best_i) {
                Some(ch) => text.push_str(ch),
                None => return Err(format!("CTC 索引越界：{best_i} / 字典 {}", charset.len())),
            }
        }
        last = best_i;
    }
    Ok(text)
}

/// 构建字符表：blank + 6623 字典字符 + space（索引 = 类别号 - 1）
fn build_charset() -> Vec<String> {
    let mut v = Vec::with_capacity(6626);
    v.push(String::new()); // [0] = blank 占位（CTC 类别 1 对应这里即 get(0)）
    for line in DICT_TXT.lines() {
        // 字典文件每行一个字符；\r\n 兼容（trim_end 只去 \r 不动内容字符）
        let ch = line.trim_end_matches('\r');
        if ch.is_empty() {
            continue; // 文件可能以空行结尾
        }
        v.push(ch.to_string());
    }
    v.push(" ".to_string()); // use_space_char=true：末尾追加空格类别
    v
}

/// DB 后处理：概率图 → 连通域 → 水平框（含 unclip 外扩）→ 映射回原图坐标。
/// 返回 (x, y, w, h, score) 列表，原图坐标系。
fn db_boxes(
    data: &[f32],
    oh: usize,
    ow: usize,
    ratio: f32,
) -> Vec<(i32, i32, i32, i32, f32)> {
    // 二值化
    let mask: Vec<bool> = data.iter().map(|&p| p > DET_THRESH).collect();
    let (w, h) = (ow, oh);
    // 连通域标注（8 邻接 BFS）
    let mut label = vec![0u32; w * h];
    let mut boxes: Vec<(i32, i32, i32, i32, f32)> = Vec::new();
    let mut cur = 0u32;
    let mut queue: Vec<usize> = Vec::with_capacity(4096);
    for start in 0..w * h {
        if !mask[start] || label[start] != 0 {
            continue;
        }
        cur += 1;
        queue.clear();
        queue.push(start);
        label[start] = cur;
        let (mut x0, mut y0, mut x1, mut y1) = (w as i32, h as i32, -1, -1);
        let (mut sum, mut cnt) = (0f32, 0u32);
        while let Some(p) = queue.pop() {
            let (px, py) = (p % w, p / w);
            (x0, y0) = (x0.min(px as i32), y0.min(py as i32));
            (x1, y1) = (x1.max(px as i32), y1.max(py as i32));
            sum += data[p];
            cnt += 1;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = px as i32 + dx;
                    let ny = py as i32 + dy;
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    let n = ny as usize * w + nx as usize;
                    if mask[n] && label[n] == 0 {
                        label[n] = cur;
                        queue.push(n);
                    }
                }
            }
        }
        if cnt == 0 {
            continue;
        }
        let score = sum / cnt as f32;
        #[cfg(test)]
        println!(
            "[db] det图 region=({x0},{y0})-({x1},{y1}) score={score:.3} px={cnt} map=({mx:.1},{my:.1}) {mw:.1}x{mh:.1}",
            mx = x0 as f32 / ratio,
            my = y0 as f32 / ratio,
            mw = (x1 - x0 + 1) as f32 / ratio,
            mh = (y1 - y0 + 1) as f32 / ratio,
        );
        // 过滤：得分过低 / 尺寸过小（<3px 的碎片噪声）
        let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
        if score < DET_BOX_THRESH || bw < 3 || bh < 3 {
            continue;
        }
        // unclip：标准公式 distance = area * unclip_ratio / perimeter（对水平框等效四边外扩）
        let area = (bw * bh) as f32;
        let perimeter = 2.0 * (bw + bh) as f32;
        let dist = (area * DET_UNCLIP / perimeter).round() as i32;
        // 映射回原图（除以 det 放大比例）并外扩
        let inv = 1.0 / ratio;
        let ux = ((x0 as f32 - dist as f32) * inv).floor() as i32;
        let uy = ((y0 as f32 - dist as f32) * inv).floor() as i32;
        let ux1 = ((x1 as f32 + 1.0 + dist as f32) * inv).ceil() as i32;
        let uy1 = ((y1 as f32 + 1.0 + dist as f32) * inv).ceil() as i32;
        boxes.push((ux, uy, ux1 - ux, uy1 - uy, score));
    }
    boxes
}

// ---------------------------------------------------------------------------
// 探针（#[ignore]，不进 CI）：固定 PNG 全管线识别 + 打印
//   SD_OCR_MODELS=E:/Agent_WorkSpace/screentime-pro/src-tauri/models \
//   cargo test --release -- --ignored --nocapture probe_ocr_onnx
//   （开发态资源就在 src-tauri/ 下，探测会自动命中，SD_OCR_MODELS 可省）
//   ORT_DYLIB_PATH=... 指向 pip onnxruntime 的 capi/onnxruntime.dll
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn probe_ocr_onnx() {
        // 模型目录：SD_OCR_MODELS 优先；默认仓库内 src-tauri/models（开发态就在这儿）
        let models = match std::env::var("SD_OCR_MODELS") {
            Ok(p) if !p.is_empty() => PathBuf::from(p),
            _ => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models"),
        };
        let lib = std::env::var("ORT_DYLIB_PATH").ok().map(PathBuf::from);
        let png = std::env::var("SD_OCR_PNG").unwrap_or_else(|_| {
            format!("{}/../scripts/ocr_probe.png", env!("CARGO_MANIFEST_DIR"))
        });
        println!("模型目录 {}（齐备={}）", models.display(), is_ready(&models));

        // 冷启动：环境初始化 + 模型加载 + 首次推理
        if let Err(e) = ensure_env(lib.as_deref()) {
            panic!("运行库初始化失败：{e}");
        }
        let t0 = std::time::Instant::now();
        let mut eng = OnnxOcr::load(&models).expect("加载引擎");
        println!("模型加载耗时 {:?}", t0.elapsed());

        let img = image::open(&png).expect("打开测试图").to_rgba8();
        println!("输入 {}×{}", img.width(), img.height());
        let t1 = std::time::Instant::now();
        let lines = eng.recognize(&img).expect("识别");
        println!("冷启动首次识别 {:?}，{} 行：", t1.elapsed(), lines.len());
        for l in &lines {
            println!("  [{:>4},{:>4} {:>3}×{:>3} score={:.2}] {}", l.x, l.y, l.w, l.h, l.score, l.text);
        }
        // 会话复用：常驻缓存后的二次识别（v0.9.0 性能验收口径）
        let t2 = std::time::Instant::now();
        let again = eng.recognize(&img).expect("二次识别");
        println!("会话复用二次识别 {:?}（{} 行）", t2.elapsed(), again.len());

        // 小选区回归：放大上限政策（DET_MAX_UPSCALE）的守门测试。
        // 用**确定性**断言（det 输入尺寸）而不是耗时 —— 耗时随机器负载飘，
        // 会变成假失败；尺寸是这个政策唯一的对外可见后果。
        let small = image::imageops::crop_imm(&img, 18, 222, 682, 44).to_image();
        let t3 = std::time::Instant::now();
        let slines = eng.recognize(&small).expect("小选区识别");
        let sall: String = slines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join(" | ");
        println!("小选区(682×44) {:?}：{}", t3.elapsed(), sall);
        assert!(
            sall.contains("PaddleOCR"),
            "小选区识别失败（放大上限政策被改坏？）：{sall}"
        );

        // 自洽断言：大标题与小字行至少各命中一行（内容会变，断言不写死全文）
        let all: String = lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");
        assert!(!lines.is_empty(), "一行都没识别出来");
        assert!(all.chars().any(|c| c >= '\u{4e00}'), "未识别出任何中文字符");
    }

    /// det 输入尺寸策略（纯函数，可确定性断言 —— 不依赖计时）
    #[test]
    fn det_input_size_policy() {
        // ① 全图级选区（900×520）：短边 520 < 736 会放大 1.415 倍，
        //    但 1.415 < 上限 4，所以与 v0.8.1 探针基线**逐像素一致**（1280×736）
        assert_eq!((det_input_size(900.0, 520.0).0, det_input_size(900.0, 520.0).1), (1280, 736));
        // ② 小选区（682×44）：不加上限时要放大 16.7 倍 → 11392×736（实测 det 要 7 秒）；
        //    加 4 倍上限后是 2720×192（实测 det 479ms、识别结果一样准）
        assert_eq!((det_input_size(682.0, 44.0).0, det_input_size(682.0, 44.0).1), (2720, 192));
        // ③ 大选区（1920×1080）：短边 > 736 不放大，面积 2.09MP < 2.64MP 上限 → 原样
        assert_eq!(
            (det_input_size(1920.0, 1080.0).0, det_input_size(1920.0, 1080.0).1),
            (1920, 1088)
        );
        // ④ 4K 整屏（3840×2160）：面积 8.29MP 超上限 → 缩回约 2.64MP（比例 0.564）
        let (w, h, ratio) = det_input_size(3840.0, 2160.0);
        assert!(ratio < 1.0, "4K 整屏必须缩回，实际 ratio={ratio}");
        assert!(
            (w as f32 * h as f32) < DET_MAX_PIXELS * 1.15,
            "4K 整屏 det 输入面积仍超上限：{w}×{h}"
        );
        // ⑤ 32 对齐：任何输入都必须是 32 的整数倍（DBNet 要求）
        for (w0, h0) in [(900.0, 520.0), (682.0, 44.0), (37.0, 11.0), (1920.0, 1080.0)] {
            let (w, h, _) = det_input_size(w0, h0);
            assert_eq!(w % 32, 0, "{w0}×{h0} → 宽 {w} 未 32 对齐");
            assert_eq!(h % 32, 0, "{w0}×{h0} → 高 {h} 未 32 对齐");
        }
    }
}
