//!
//! screenshot/ocr_engine.rs
//! 取字「引擎调度」（v0.9.0）：在标准引擎与增强引擎之间做选择，并统一解析引擎资源路径。
//!
//! 两个引擎的分工：
//! - **标准（`System`）**：`ocr.rs` 的系统内置能力（Windows = WinRT `Media.Ocr`）。
//!   零体积、零依赖、随系统语言包走；短板是小字号（<14px）与低对比场景丢字，
//!   且已实测「放大 / 灰度 / 留边」三种预处理全部无效（见 `ocr.rs` 头部否决记录）。
//! - **增强（`Enhanced`）**：`ocr_onnx.rs` 的 PaddleOCR v4（det + rec 两段式）。
//!   短边不足 736px 的图会先被 det 放大再检测，小字在检测阶段就被拉大 ——
//!   这是 WinRT 单段链路做不到的，也是识别率提升的真正来源。
//!   代价：随包分发 ~33MB（onnxruntime.dll + det/rec 模型）。**依旧零联网、零上传。**
//!
//! 资源分发策略（v0.9.0，按平台分两种）：
//! - **模型（det/rec `.onnx`，跨平台通用）**：三端一律**随包分发**（提交进仓库 + 打进安装包），
//!   装机即用、断网可用，不依赖任何下载。
//! - **运行时库（`onnxruntime` 动态库，平台相关）**：
//!   - Windows：`onnxruntime.dll` **随包分发**（提交进仓库），零下载、零联网；
//!   - macOS / Linux：平台运行库**不随包**（无法在本机产出对应二进制），改为
//!     **首次使用时后台静默下载**到用户可写缓存目录（`download_runtime_lib_if_missing`），
//!     下载源默认 GitHub Release `ocr-runtime`，可用 `SD_OCR_DOWNLOAD_BASE` 覆盖。
//!     依旧**零上传**——只拉 Ryan 自己托管的那一份运行库，不传任何用户数据。
//! - 「零外部接口 / 断网可用」的红线对**截图与取字本身**始终成立；运行库下载是一次性的
//!   平台二进制获取，失败时自动回落「标准」引擎并提示用户，不会让取字崩溃。
//!
//! 路径解析一律**多候选探测**，因为「资源到底落在哪」随打包方式而变：
//! NSIS 安装（资源在安装目录）、开发直跑（资源在 target/release）、
//! cargo test（资源在 src-tauri/ort）三者目录不同，探测比猜更可靠。
//!
//! 修改历史：
//!   - 2026-09-17 @v0.9.0: 初始创建 - 引擎枚举 + 资源路径探测 + 统一 recognize 出口
//!

use super::{ocr, ocr_onnx};
use image::RgbaImage;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// 取字引擎类型（落 settings 键 `screenshot_ocr_engine`）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    /// 标准：系统内置（Windows = WinRT Media.Ocr）
    System,
    /// 增强：PaddleOCR-ONNX 本地模型
    Enhanced,
}

impl EngineKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EngineKind::System => "system",
            EngineKind::Enhanced => "enhanced",
        }
    }

    /// 解析配置值。**未知值一律回落到 `System`** —— 旧配置里没有这个键、
    /// 或用户手改坏了配置，都不该让取字直接不可用。
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "enhanced" | "onnx" => EngineKind::Enhanced,
            _ => EngineKind::System,
        }
    }
}

/// 引擎资源路径（一次性探测好，随任务 move 进 spawn_blocking）
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct EnginePaths {
    /// onnxruntime 动态库绝对路径（None = 让 ort 自己按环境变量/PATH 找）
    pub ort_lib: Option<String>,
    /// 模型目录（含 det/rec 两个 .onnx）
    pub models_dir: Option<String>,
}

impl EnginePaths {
    /// 探测本机引擎资源。`resource_dir` 由调用方从 `AppHandle::path()` 取。
    pub fn resolve(resource_dir: Option<&Path>) -> Self {
        Self {
            ort_lib: resolve_ort_lib(resource_dir).map(|p| p.to_string_lossy().to_string()),
            models_dir: resolve_models_dir(resource_dir).map(|p| p.to_string_lossy().to_string()),
        }
    }

    pub fn ort_lib_path(&self) -> Option<&Path> {
        self.ort_lib.as_ref().map(Path::new)
    }

    pub fn models_path(&self) -> Option<&Path> {
        self.models_dir.as_ref().map(Path::new)
    }

    /// 增强引擎是否**真正可用**（运行库 + 两个模型都在）
    pub fn enhanced_ready(&self) -> bool {
        self.ort_lib_path().is_some_and(|p| p.is_file())
            && self.models_path().is_some_and(ocr_onnx::is_ready)
    }
}

/// 资源根目录候选（按优先级）：
/// 1. exe 同级（NSIS/MSI 安装后资源就在安装目录）
/// 2. exe 同级 `resources/`（部分打包器的约定）
/// 3. Tauri 报告的 resource_dir（等价于 1，但 MSI 下可能是别的路径）
/// 4. `src-tauri/`（开发直跑 / cargo test：资源就放在仓库里）
fn candidate_roots(resource_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.to_path_buf());
            roots.push(dir.join("resources"));
        }
    }
    if let Some(rd) = resource_dir {
        roots.push(rd.to_path_buf());
        roots.push(rd.join("resources"));
    }
    // 编译期路径：开发态（cargo run / cargo test）资源在 src-tauri/ 下
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    // 用户可写缓存目录（macOS /Applications 只读，运行时下载的运行库必须落这里）
    if let Some(u) = user_ort_dir() {
        roots.push(u.clone());
        if let Some(p) = u.parent() {
            roots.push(p.to_path_buf());
        }
    }
    roots
}

/// 定位 onnxruntime 动态库。
///
/// 注意 `ORT_DYLIB_PATH` 环境变量优先级最高 —— 探针/开发靠它指向 pip 装的那份；
/// 产品态不设它，走下面的资源探测。（ort 2.x 认的就是这个名字，
/// 写成 `ONNXRUNTIME_DYLIB_PATH` 会被静默忽略并从 PATH 搜到旧版 dll → BadVersion abort。）
fn resolve_ort_lib(resource_dir: Option<&Path>) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("ORT_DYLIB_PATH") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let name = ort_lib_name();
    for root in candidate_roots(resource_dir) {
        for sub in ["ort", ""] {
            let cand = if sub.is_empty() {
                root.join(name)
            } else {
                root.join(sub).join(name)
            };
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

/// 各平台运行库文件名
fn ort_lib_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    }
}

/// 定位模型目录（含 det/rec 两个 .onnx 的目录）。
/// `SD_OCR_MODELS` 优先（探针/CI 显式指定），其次 exe 同级 `models/`、
/// 资源目录 `models/`、应用数据目录 `models/`（未来「下载模式」的落点）。
fn resolve_models_dir(resource_dir: Option<&Path>) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("SD_OCR_MODELS") {
        if !p.is_empty() {
            let p = PathBuf::from(p);
            if ocr_onnx::is_ready(&p) {
                return Some(p);
            }
        }
    }
    for root in candidate_roots(resource_dir) {
        let cand = root.join("models");
        if ocr_onnx::is_ready(&cand) {
            return Some(cand);
        }
    }
    None
}

/// 用户可写目录下的 `ort` 子目录（跨平台）。
///
/// 关键：macOS 的 `.app` 在 `/Applications` 下是**只读**的，运行时下载的运行库若写进
/// `.app` 内部会静默失败、OCR 永远不可用。所以下载落点统一用用户缓存目录：
/// - Windows: `%LOCALAPPDATA%\ScreenTime Pro\ort`（NSIS 安装目录本就在这里，可读写）
/// - macOS:   `~/Library/Caches/com.screentime.pro/ort`
/// - Linux:   `~/.cache/screentime-pro/ort`
/// 该目录同时被 `resolve_ort_lib` 探测，下载后即被命中。
fn user_ort_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        std::env::var("LOCALAPPDATA")
            .ok()
            .map(|p| PathBuf::from(p).join("ScreenTime Pro").join("ort"))
    } else if let Ok(home) = std::env::var("HOME") {
        if cfg!(target_os = "macos") {
            Some(
                PathBuf::from(home)
                    .join("Library")
                    .join("Caches")
                    .join("com.screentime.pro")
                    .join("ort"),
            )
        } else {
            Some(
                PathBuf::from(home)
                    .join(".cache")
                    .join("screentime-pro")
                    .join("ort"),
            )
        }
    } else {
        None
    }
}

/// 下载落点（优先用户可写缓存，回退 exe 同级 `ort/`，再回退相对路径）
fn download_target_path(name: &str) -> PathBuf {
    if let Some(u) = user_ort_dir() {
        return u;
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            return dir.join("ort").join(name);
        }
    }
    PathBuf::from("ort").join(name)
}

/// 增强引擎运行时库按需下载（macOS / Linux 走此通道；Windows 运行库随包天然就绪）。
///
/// **默认启用**内置基址 [`DEFAULT_OCR_DOWNLOAD_BASE`]（GitHub Release `ocr-runtime`），
/// 可用环境变量 `SD_OCR_DOWNLOAD_BASE` 覆盖。基址下应存在 `<lib_name>`（文件名见 `ort_lib_name`），
/// 可选 `<lib_name>.sha256`（单行十六进制摘要）做完整性校验。
///
/// 设计原则：**绝不阻断取字**。任何失败（网络 / 校验 / 写入）都只记 `tracing` 日志并返回
/// `false`，调用方据此回落到「标准」引擎或向用户提示，不会让截图流程崩溃。
///
/// ⚠️ 仅在「本平台运行库未随包」时才有意义：Windows 运行库（`onnxruntime.dll`）随包，
/// `resolve_ort_lib` 已命中 → 直接返回 `true`，不会发任何网络请求。
pub fn download_runtime_lib_if_missing(resource_dir: Option<&Path>) -> bool {
    if resolve_ort_lib(resource_dir).is_some() {
        return true; // 已就绪，无需下载（Windows 常态）
    }
    let base = std::env::var("SD_OCR_DOWNLOAD_BASE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_OCR_DOWNLOAD_BASE.to_string());
    let name = ort_lib_name();
    let url = format!("{}/{}", base.trim_end_matches('/'), name);
    let target = download_target_path(name);
    tracing::info!(url = %url, "增强引擎运行库未随包，尝试后台下载");

    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "增强引擎运行库下载客户端创建失败");
            return false;
        }
    };

    let bytes = match client
        .get(&url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.bytes())
    {
        Ok(b) => b.to_vec(),
        Err(e) => {
            tracing::warn!(error = %e, "增强引擎运行库下载失败");
            return false;
        }
    };

    // 可选 SHA256 校验：基址下存在 `<name>.sha256` 文本才校验，缺失则跳过（仅记日志）
    if let Ok(hr) = client.get(format!("{url}.sha256")).send() {
        if let Ok(htext) = hr.text() {
            let expected = htext.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
            if !expected.is_empty() {
                let mut h = Sha256::new();
                h.update(&bytes);
                let got = format!("{:x}", h.finalize());
                if got != expected {
                    tracing::error!(expected = %expected, got = %got, "增强引擎运行库 SHA256 校验失败，已丢弃");
                    return false;
                }
            }
        }
    }

    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&target, &bytes) {
        tracing::warn!(error = %e, "增强引擎运行库写入失败");
        return false;
    }
    tracing::info!(path = %target.display(), "增强引擎运行库下载完成");
    resolve_ort_lib(resource_dir).is_some()
}

/// 增强引擎运行时库默认下载基址（Ryan 在 GitHub 建 `ocr-runtime` Release，
/// 上传 `libonnxruntime.dylib`(mac) / `libonnxruntime.so`(linux) 及可选 `.sha256`）。
/// 可用环境变量 `SD_OCR_DOWNLOAD_BASE` 覆盖（如自建镜像）。
const DEFAULT_OCR_DOWNLOAD_BASE: &str =
    "https://github.com/Slk90s/screentime-pro/releases/download/ocr-runtime";

/// 统一取字出口：按引擎分发，返回结构与标准引擎一致（多一个 `engine` 字段）。
pub fn recognize(kind: EngineKind, paths: &EnginePaths, img: &RgbaImage) -> Result<ocr::OcrOut, String> {
    match kind {
        EngineKind::System => ocr::recognize(img),
        EngineKind::Enhanced => recognize_enhanced(paths, img),
    }
}

fn recognize_enhanced(paths: &EnginePaths, img: &RgbaImage) -> Result<ocr::OcrOut, String> {
    let models = paths.models_path().ok_or_else(|| {
        "增强引擎的模型文件缺失：请重新安装完整版本的应用（models/ 未随包落盘）".to_string()
    })?;
    // 运行库缺失（macOS / Linux 不随包）→ 先尝试按需下载，下载后重新探测
    let paths = if paths.enhanced_ready() {
        paths.clone()
    } else {
        download_runtime_lib_if_missing(None);
        EnginePaths::resolve(None)
    };
    if !paths.enhanced_ready() {
        return Err(
            "增强引擎的运行库缺失：本平台运行库未随包，需联网后首次使用时后台下载；\n\
             若已联网仍失败，请在设置中将「取字引擎」切回「标准」，或重新安装包含运行库的版本。"
                .into(),
        );
    }
    let lines = ocr_onnx::recognize_cached(models, paths.ort_lib_path(), img)?;
    // PP-OCR 按行输出连续文本（不是 WinRT 的「单字 + 空格」），
    // 所以**不能**复用 ocr.rs 的 tidy_text —— 那个规则会误删 `CPU 使用率` 的真实间隔。
    let text = lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    Ok(ocr::OcrOut {
        text,
        width: img.width(),
        height: img.height(),
        // PP-OCRv4 的 ch 模型覆盖「简体中文 + 英文 + 数字」，没有 WinRT 那种语言标签，
        // 这里用 PaddleOCR 自己的语言代号，前端据 engine 字段换成人话展示。
        language: Some("ch".into()),
        engine: EngineKind::Enhanced.as_str().to_string(),
        lines: lines.len(),
    })
}

/// 引擎信息（设置页展示用）
#[derive(Debug, Clone, serde::Serialize)]
pub struct EngineInfo {
    /// 当前选中的引擎（system / enhanced）
    pub kind: String,
    /// 标准引擎在本平台是否可用（仅 Windows 实现）
    pub system_available: bool,
    /// 增强引擎是否可用（运行库 + 模型齐备）
    pub enhanced_ready: bool,
    /// 增强引擎运行库路径（缺失为 null，便于排障）
    pub ort_lib: Option<String>,
    /// 增强引擎模型目录（缺失为 null）
    pub models_dir: Option<String>,
    /// 增强引擎随包体积（MB，两位小数；0 表示资源缺失）
    pub enhanced_size_mb: f64,
}

impl EngineInfo {
    pub fn probe(kind: EngineKind, paths: &EnginePaths) -> Self {
        let mut bytes: u64 = 0;
        if let Some(p) = paths.ort_lib_path() {
            bytes += std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        }
        if let Some(d) = paths.models_path() {
            for f in ["ch_PP-OCRv4_det_infer.onnx", "ch_PP-OCRv4_rec_infer.onnx"] {
                bytes += std::fs::metadata(d.join(f)).map(|m| m.len()).unwrap_or(0);
            }
        }
        Self {
            kind: kind.as_str().to_string(),
            system_available: cfg!(target_os = "windows"),
            enhanced_ready: paths.enhanced_ready(),
            ort_lib: paths.ort_lib.clone(),
            models_dir: paths.models_dir.clone(),
            enhanced_size_mb: (bytes as f64 / 1048576.0 * 100.0).round() / 100.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_falls_back_to_system() {
        assert_eq!(EngineKind::parse("enhanced"), EngineKind::Enhanced);
        assert_eq!(EngineKind::parse("ONNX"), EngineKind::Enhanced);
        assert_eq!(EngineKind::parse("system"), EngineKind::System);
        // 空值 / 脏值都不能让取字不可用
        assert_eq!(EngineKind::parse(""), EngineKind::System);
        assert_eq!(EngineKind::parse("   "), EngineKind::System);
        assert_eq!(EngineKind::parse("garbage"), EngineKind::System);
    }

    #[test]
    fn roundtrip_as_str() {
        for k in [EngineKind::System, EngineKind::Enhanced] {
            assert_eq!(EngineKind::parse(k.as_str()), k);
        }
    }
}
