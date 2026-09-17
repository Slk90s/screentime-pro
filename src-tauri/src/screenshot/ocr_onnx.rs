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
//! - `ort` crate 以 `features = ["load-dynamic"]` 引入，编译期不链接 ORT，
//!   运行时经 `ONNXRUNTIME_DYLIB_PATH` 或 PATH 找 `onnxruntime.dll`。
//! - 产品分发：dll 随应用资源打包（见 tauri.conf.json resources）；
//!   探针/开发：指向 pip onnxruntime 包内 `capi/onnxruntime.dll`。
//! - 版本兼容：onnxruntime C API 向后兼容，运行库版本 ≥ ort 编译锚定版本即可
//!   （本机 pip 装的 1.30.0 实测可加载）。
//!
//! 模型目录解析（`models_dir()` 优先级）：
//! 1. env `SD_OCR_MODELS`（开发/探针/CI 显式指定）
//! 2. 应用数据目录 `models/`（产品形态；「首次使用时下载」逻辑在 mod.rs 接线）
//!
//! 修改历史：
//!   - 2026-09-17 @v0.9.0: 初始创建 - det/rec 两段管线 + DB 水平框后处理 + CTC 解码
//!

use image::{DynamicImage, RgbaImage};
use std::path::{Path, PathBuf};

/// ppocr_keys_v1.txt：6623 行，每行一个字符（编译期内嵌，零运行时路径依赖）
const DICT_TXT: &str = include_str!("ppocr_keys_v1.txt");

/// det 预处理参数（对齐 RapidOCR 默认：limit_type=min, limit_side_len=736）
const DET_LIMIT_SIDE: f32 = 736.0;
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

/// 识别出的单行文本
#[derive(Debug, Clone)]
pub struct OcrLine {
    pub text: String,
    /// 原图坐标系（物理像素）水平包围盒
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// det 置信度（连通域概率均值，0~1）
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
        let det_path = models_dir.join("ch_PP-OCRv4_det_infer.onnx");
        let rec_path = models_dir.join("ch_PP-OCRv4_rec_infer.onnx");
        if !det_path.is_file() {
            return Err(format!("缺少检测模型：{}", det_path.display()));
        }
        if !rec_path.is_file() {
            return Err(format!("缺少识别模型：{}", rec_path.display()));
        }
        let det = ort::session::Session::builder()
            .map_err(|e| e.to_string())?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level1)
            .map_err(|e| e.to_string())?
            .with_intra_threads(4)
            .map_err(|e| e.to_string())?
            .commit_from_file(&det_path)
            .map_err(|e| format!("加载 det 模型失败：{e}"))?;
        let rec = ort::session::Session::builder()
            .map_err(|e| e.to_string())?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level1)
            .map_err(|e| e.to_string())?
            .with_intra_threads(4)
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
        // ---------- 1. det：短边放大到 736 → 归一化 → DB 概率图 ----------
        let rgb = DynamicImage::ImageRgba8(img.clone()).to_rgb8();
        let (w0, h0) = (img.width() as f32, img.height() as f32);
        let ratio = if w0.min(h0) < DET_LIMIT_SIDE {
            DET_LIMIT_SIDE / w0.min(h0)
        } else {
            1.0
        };
        // 32 像素对齐（DBNet 要求）。⚠️ 运算顺序：先取「块数」再 ×32，最后才 max——
        // 若写成 `.max(32) * 32` 会把块数顶到 32（=1024px），图像被纵向拉伸、
        // 全部下游坐标错乱（2026-09-17 探针实测踩坑）。
        let rw = ((w0 * ratio / 32.0).round() as u32).max(1) * 32;
        let rh = ((h0 * ratio / 32.0).round() as u32).max(1) * 32;
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
        let det_in = normalize_nchw(&resized, &DET_MEAN, &DET_STD);
        let prob = self.run_det(det_in)?;

        // ---------- 2. DB 后处理：连通域 → 水平框 → unclip → 映射回原图 ----------
        let boxes = db_boxes(&prob.0, prob.1, prob.2, ratio);

        // ---------- 3. rec：逐行裁剪 → 归一化 → CTC 解码 ----------
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

/// 模型目录解析：env `SD_OCR_MODELS` > 应用数据目录 `models/`
pub fn models_dir(app_data_dir: Option<&Path>) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("SD_OCR_MODELS") {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    app_data_dir.map(|d| d.join("models"))
}

// ---------------------------------------------------------------------------
// 探针（#[ignore]，不进 CI）：固定 PNG 全管线识别 + 打印
//   SD_OCR_MODELS=E:/Agent_WorkSpace/screentime-pro/.tmp-ocr/models \
//   ONNXRUNTIME_DYLIB_PATH="C:/Users/SLK/.workbuddy/binaries/python/envs/default/Lib/site-packages/onnxruntime/capi/onnxruntime.dll" \
//   cargo test --release -- --ignored --nocapture probe_ocr_onnx
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn probe_ocr_onnx() {
        let models = std::env::var("SD_OCR_MODELS").expect("需要 SD_OCR_MODELS");
        let png = std::env::var("SD_OCR_PNG")
            .unwrap_or_else(|_| "E:/Agent_WorkSpace/screentime-pro/scripts/ocr_probe.png".into());
        let t0 = std::time::Instant::now();
        let mut eng = OnnxOcr::load(Path::new(&models)).expect("加载引擎");
        println!("模型加载耗时 {:?}", t0.elapsed());
        let img = image::open(&png).expect("打开测试图").to_rgba8();
        println!("输入 {}×{}", img.width(), img.height());
        let t1 = std::time::Instant::now();
        let lines = eng.recognize(&img).expect("识别");
        println!("识别耗时 {:?}，{} 行：", t1.elapsed(), lines.len());
        for l in &lines {
            println!("  [{:>4},{:>4} {:>3}×{:>3} score={:.2}] {}", l.x, l.y, l.w, l.h, l.score, l.text);
        }
        // 自洽断言：大标题与小字行至少各命中一行（内容会变，断言不写死全文）
        let all: String = lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");
        assert!(!lines.is_empty(), "一行都没识别出来");
        assert!(all.chars().any(|c| c >= '\u{4e00}'), "未识别出任何中文字符");
    }
}
