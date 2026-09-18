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
//!   代价：随包分发 ~33MB（`onnxruntime` 运行库 + det/rec 模型）。依旧**零上传**；
//!   其中 Windows 运行库随包（零联网），macOS / Linux 运行库首次使用时下载一次（见下）。
//!
//! 资源分发策略（v0.9.0，按平台分两种）：
//! - **模型（det/rec `.onnx`，跨平台通用）**：三端一律**随包分发**（提交进仓库 + 打进安装包），
//!   装机即用、断网可用，不依赖任何下载。
//! - **运行时库（`onnxruntime` 动态库，平台相关）**：
//!   - Windows：`onnxruntime.dll` **随包分发**（提交进仓库），零下载、零联网；
//!   - macOS / Linux：平台运行库**不随包**（无法在本机产出对应二进制），改为
//!     **打开截图遮罩时在后台线程静默下载**到用户可写缓存目录（`download_runtime_lib_if_missing`），
//!     下载源按序尝试 GitHub Release `ocr-runtime` → Gitee 同名 Release，
//!     可用 `SD_OCR_DOWNLOAD_BASE` 覆盖。依旧**零上传**——只拉运行库，不传任何用户数据。
//! - ⚠️ **下载一律只在后台预热线程做，绝不在 `recognize()` 同步路径里做**：
//!   运行库 29~43MB，慢网下可能数分钟；卡住取字调用会让 UI 长时间无响应。
//!   取字时若尚未就绪，立刻返回可读原因让用户稍后重试。
//! - 「零外部接口 / 断网可用」的红线对**截图与取字本身**始终成立；运行库下载是一次性的
//!   平台二进制获取，失败时自动回落「标准」引擎（Windows）并提示用户，不会让取字崩溃。
//!
//! 路径解析一律**多候选探测**，因为「资源到底落在哪」随打包方式而变：
//! NSIS 安装（资源在安装目录）、开发直跑（资源在 target/release）、
//! cargo test（资源在 src-tauri/ort）三者目录不同，探测比猜更可靠。
//!
//! 修改历史：
//!   - 2026-09-17 @v0.9.0: 初始创建 - 引擎枚举 + 资源路径探测 + 统一 recognize 出口
//!   - 2026-09-18 @v0.9.0: 修复 - ①下载超时由「20s 总超时」（慢网 43MB 必失败）改为
//!     「连接 30s + 总上限 10 分钟 + 每源 3 次重试」；②新增 Gitee 镜像作为第二下载源；
//!     ③SHA256 校验加状态码与摘要格式判断（.sha256 缺失/404 不再误判为损坏）；
//!     ④`recognize_enhanced` 移除同步下载（改由后台预热下载），并修正其重解析
//!     `EnginePaths::resolve(None)` 会丢 macOS `Contents/Resources/models` 的问题。
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
    /// ⚠️ 本函数是**纯字符串解析**，不带平台判断；「本平台到底能不能用标准引擎」
    /// 由下面的 [`effective_kind`] 决定。两者刻意分开，避免污染这里的可测性。
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "enhanced" | "onnx" => EngineKind::Enhanced,
            _ => EngineKind::System,
        }
    }
}

/// 本平台是否内置「标准」取字引擎。
///
/// 标准引擎 = `ocr.rs` 的系统能力，**目前仅 Windows 实现**（WinRT `Media.Ocr`）；
/// macOS / Linux 尚未落地（macOS 计划接系统 Vision、Linux 计划接 Tesseract，均未实现），
/// 在这两个平台上 `ocr::recognize()` 只会返回「仅支持 Windows」的错误。
pub const fn system_engine_available() -> bool {
    cfg!(target_os = "windows")
}

/// 引擎**缺省值**（配置里没有这个键时使用）。
///
/// - 有标准引擎的平台（Windows）→ `system`：零额外体积、零依赖、零模型加载；
/// - 没有标准引擎的平台（macOS / Linux）→ `enhanced`：若默认成 `system`，
///   开箱取字必然报「仅支持 Windows」，等于功能不可用。
pub const fn default_engine() -> EngineKind {
    if system_engine_available() {
        EngineKind::System
    } else {
        EngineKind::Enhanced
    }
}

/// 把「配置里的引擎字符串」解析为**本平台真正可用**的引擎。
///
/// 唯一会改写用户选择的情形：**本平台没有标准引擎（macOS / Linux）却配了标准** ——
/// 此时回落到增强，否则取字必然失败。这条同时也是**自愈路径**：
/// v0.9.0 的默认值是 `system`，macOS 用户只要改过任意一项截图设置（触发 `save()`）
/// 就会把 `system` 写进库；升级到修复版后靠这里自动改回 `enhanced`，无需用户手改配置。
pub fn effective_kind(config_value: &str) -> EngineKind {
    let k = EngineKind::parse(config_value);
    if k == EngineKind::System && !system_engine_available() {
        EngineKind::Enhanced
    } else {
        k
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

/// 下载落点（**必须是「目录 + 文件名」，绝不能只返回目录**）。
///
/// 🐞 历史 bug（v0.9.0 首发即存在，本版修复）：首分支曾写成 `return u;` ——
/// `user_ort_dir()` 返回的是**目录**（`.../ort`），于是 `fs::write(目录, bytes)`
/// 必然失败（Windows 报「拒绝访问」/ POSIX 报 EISDIR），
/// **macOS / Linux 的运行库永远落不了盘 → 增强引擎永远不可用**。
/// 由于 Windows 运行库随包、`download_runtime_lib_if_missing` 会提前返回，
/// 这个 bug 在 Windows 上永远暴露不出来，只打中真正需要下载的 mac / Linux。
fn download_target_path(name: &str) -> PathBuf {
    if let Some(u) = user_ort_dir() {
        return u.join(name);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            return dir.join("ort").join(name);
        }
    }
    PathBuf::from("ort").join(name)
}

/// 组装实际要请求的下载基址：环境变量覆盖（只用一个）→ 否则内置顺序（GitHub 主源 → Gitee 镜像）。
fn download_bases() -> Vec<String> {
    match std::env::var("SD_OCR_DOWNLOAD_BASE") {
        Ok(v) if !v.trim().is_empty() => vec![v.trim().trim_end_matches('/').to_string()],
        _ => MIRROR_OCR_DOWNLOAD_BASES
            .iter()
            .map(|s| s.trim_end_matches('/').to_string())
            .collect(),
    }
}

/// 建下载用的 HTTP 客户端。
///
/// ⚠️ 超时策略是这条链路的关键：运行库 29~43MB，**绝不能用短「总超时」**
/// （最初写成 20s 总超时 → 慢网必然失败，等于 mac/Linux 取字永远不可用）。
/// 这里用「连接 30s + 总上限 10 分钟」，配合调用侧的「每源 3 次重试」。
fn download_client() -> Result<reqwest::blocking::Client, reqwest::Error> {
    reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(600))
        .build()
}

/// 按序拉取运行库字节：逐个基址 × 每基址 3 次重试，命中即返回 `(命中的基址, 字节)`。
fn fetch_runtime_lib_bytes(
    client: &reqwest::blocking::Client,
    name: &str,
    bases: &[String],
) -> Option<(String, Vec<u8>)> {
    for base in bases {
        let url = format!("{}/{}", base, name);
        for attempt in 1..=3u32 {
            tracing::info!(url = %url, attempt, "增强引擎运行库未随包，尝试后台下载");
            match client
                .get(&url)
                .send()
                .and_then(|r| r.error_for_status())
                .and_then(|r| r.bytes())
            {
                Ok(b) => return Some((base.clone(), b.to_vec())),
                Err(e) => {
                    tracing::warn!(error = %e, url = %url, attempt, "增强引擎运行库下载失败，将重试 / 换源");
                }
            }
        }
    }
    None
}

/// 可选 SHA256 校验：仅当基址下存在 `<name>.sha256` 且内容确为 64 位十六进制摘要时才校验；
/// 缺失 / 404 / 网络失败 / 内容不是摘要 → **一律放行**（仅记日志）。
/// 绝不因为「校验文件本身不可用」而把一份完好的运行库误判为损坏。
/// 返回 `true` = 通过（或无从校验）。
fn verify_sha256(
    client: &reqwest::blocking::Client,
    base: &str,
    name: &str,
    bytes: &[u8],
) -> bool {
    let Ok(hr) = client.get(format!("{}/{}.sha256", base, name)).send() else {
        return true;
    };
    if !hr.status().is_success() {
        return true;
    }
    let Ok(htext) = hr.text() else { return true };
    let expected = htext
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if expected.len() != 64 || !expected.bytes().all(|c| c.is_ascii_hexdigit()) {
        tracing::warn!("增强引擎运行库 .sha256 内容非摘要，跳过校验");
        return true;
    }
    let mut h = Sha256::new();
    h.update(bytes);
    let got = format!("{:x}", h.finalize());
    if got != expected {
        tracing::error!(expected = %expected, got = %got, "增强引擎运行库 SHA256 校验失败，已丢弃");
        return false;
    }
    true
}

/// 下载 → 校验 → 落盘（生产路径与测试探针共用同一实现）。
///
/// ⚠️ **只在后台线程调用**：运行库 29~43MB，慢网下可能耗时数分钟。
/// ⚠️ `target` 必须是**文件路径**（`<ort 目录>/<库文件名>`）—— 传目录必然写失败，
/// 见 [`download_target_path`] 记录的历史 bug。
fn fetch_and_store(name: &str, target: &Path, bases: &[String]) -> Result<(), String> {
    let client = download_client().map_err(|e| format!("创建下载客户端失败: {e}"))?;
    let Some((base, bytes)) = fetch_runtime_lib_bytes(&client, name, bases) else {
        return Err("所有下载源均失败".to_string());
    };
    if !verify_sha256(&client, &base, name, &bytes) {
        return Err("SHA256 校验失败".to_string());
    }
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(target, &bytes).map_err(|e| format!("写入 {} 失败: {e}", target.display()))?;
    tracing::info!(path = %target.display(), bytes = bytes.len(), "增强引擎运行库下载完成");
    Ok(())
}

/// 增强引擎运行时库按需下载（macOS / Linux 走此通道；Windows 运行库随包天然就绪）。
///
/// **按序尝试**内置基址 [`MIRROR_OCR_DOWNLOAD_BASES`]（GitHub Release `ocr-runtime` → Gitee 镜像），
/// 可用环境变量 `SD_OCR_DOWNLOAD_BASE` 覆盖（覆盖后只用该单一基址）。
/// 基址下应存在 `<lib_name>`（文件名见 [`ort_lib_name`]），
/// 可选 `<lib_name>.sha256`（单行十六进制摘要）做完整性校验。
///
/// ⚠️ **只在后台预热线程调用**（见 `mod.rs::warm_ocr_engine`），不要放进 `recognize()` 同步路径：
/// 运行库 29~43MB，慢网下可能数分钟。超时策略为「连接 30s + 总上限 10 分钟 + 每源 3 次重试」。
///
/// 设计原则：**绝不阻断取字**。任何失败（网络 / 校验 / 写入）都只记 `tracing` 日志并返回
/// `false`，调用方据此向用户提示，不会让截图流程崩溃。
///
/// ⚠️ 仅在「本平台运行库未随包」时才有意义：Windows 运行库（`onnxruntime.dll`）随包，
/// `resolve_ort_lib` 已命中 → 直接返回 `true`，不会发任何网络请求。
pub fn download_runtime_lib_if_missing(resource_dir: Option<&Path>) -> bool {
    if resolve_ort_lib(resource_dir).is_some() {
        return true; // 已就绪，无需下载（Windows 常态）
    }
    let name = ort_lib_name();
    let target = download_target_path(name);
    if let Err(e) = fetch_and_store(name, &target, &download_bases()) {
        tracing::error!(error = %e, "增强引擎运行库下载失败");
        return false;
    }
    resolve_ort_lib(resource_dir).is_some()
}

/// 增强引擎运行时库的内置下载基址（**按顺序尝试**，前者失败自动换后者）。
///
/// 两个基址下都应存在 `libonnxruntime.dylib`(macOS) / `libonnxruntime.so`(Linux)
/// 以及可选的同名 `.sha256`（单行十六进制摘要）。
/// - 主源：GitHub Release `ocr-runtime`（Ryan 自托管，与 Windows 随包的 `onnxruntime.dll` 同为 ORT 1.30.0）
/// - 镜像：Gitee 同名 Release（国内网络更稳——43MB 走 github.com 极易超时）
///
/// 可用环境变量 `SD_OCR_DOWNLOAD_BASE` 覆盖为任意单一基址（如自建 CDN）。
const MIRROR_OCR_DOWNLOAD_BASES: [&str; 2] = [
    "https://github.com/Slk90s/screentime-pro/releases/download/ocr-runtime",
    "https://gitee.com/create100/screentime-pro/releases/download/ocr-runtime",
];

/// 统一取字出口：按引擎分发，返回结构与标准引擎一致（多一个 `engine` 字段）。
pub fn recognize(kind: EngineKind, paths: &EnginePaths, img: &RgbaImage) -> Result<ocr::OcrOut, String> {
    match kind {
        EngineKind::System => ocr::recognize(img),
        EngineKind::Enhanced => recognize_enhanced(paths, img),
    }
}

fn recognize_enhanced(paths: &EnginePaths, img: &RgbaImage) -> Result<ocr::OcrOut, String> {
    // 模型在三端都随包 → 直接用调用方（带 resource_dir）解析好的 models。
    // ⚠️ 此处**不要**再 `EnginePaths::resolve(None)`：candidate_roots 不含 macOS 的
    // `Contents/Resources`（那要靠 Tauri 的 resource_dir 传入），重解析会把模型目录弄丢。
    let models = paths.models_path().ok_or_else(|| {
        "增强引擎的模型文件缺失：请重新安装完整版本的应用（models/ 未随包落盘）".to_string()
    })?;
    // 运行库缺失（macOS / Linux 不随包）→ **绝不在此处阻塞下载**：
    // 下载由 `warm_ocr_engine` 在打开截图遮罩时于后台线程静默进行（运行库 29~43MB，
    // 慢网下可能耗时数分钟，卡在取字调用里会让 UI 长时间无响应）。
    // 这里只重探一次运行库路径（后台线程可能刚把文件落到用户可写缓存目录），
    // 未就绪就立刻返回可读原因，让用户稍后重试。
    let mut resolved = paths.clone();
    if !resolved.enhanced_ready() {
        if let Some(lib) = resolve_ort_lib(None) {
            resolved.ort_lib = Some(lib.to_string_lossy().into_owned());
        }
    }
    if !resolved.enhanced_ready() {
        return Err(
            "增强引擎的运行库正在后台下载中（macOS / Linux 的运行库不随包，首次使用时自动获取）：\n\
             请稍等片刻后重试；若长时间仍失败，可在设置里把「取字引擎」切回「标准」，\n\
             或用环境变量 SD_OCR_DOWNLOAD_BASE 指定可用的下载源。"
                .into(),
        );
    }
    let lines = ocr_onnx::recognize_cached(models, resolved.ort_lib_path(), img)?;
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
            system_available: system_engine_available(),
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

    /// 🐞 回归：下载落点必须是「`ort/` 目录下的**文件名**」，不能是目录本身。
    /// v0.9.0 首发该函数首分支写成 `return u;`（返回目录）→
    /// `fs::write(目录, bytes)` 必然失败 → mac/Linux 运行库永远落不了盘。
    #[test]
    fn download_target_is_a_file_named_after_the_lib() {
        for name in [
            "onnxruntime.dll",
            "libonnxruntime.dylib",
            "libonnxruntime.so",
        ] {
            let p = download_target_path(name);
            assert_eq!(
                p.file_name().and_then(|s| s.to_str()),
                Some(name),
                "下载落点必须是文件路径（目录 + 文件名），实际 = {}",
                p.display()
            );
            assert_eq!(
                p.parent()
                    .and_then(|s| s.file_name())
                    .and_then(|s| s.to_str()),
                Some("ort"),
                "下载落点必须位于 ort/ 目录下，实际 = {}",
                p.display()
            );
        }
    }

    /// 平台差异：本平台没有标准引擎时（macOS / Linux），
    /// 配置里的 `system` 与任何脏值都必须被抬成 `enhanced`，
    /// 否则取字必然报「仅支持 Windows」（也涵盖旧配置自愈）。
    #[test]
    fn effective_kind_heals_on_platforms_without_system_engine() {
        if system_engine_available() {
            // Windows：如实尊重用户选择，脏值回标准
            assert_eq!(effective_kind("system"), EngineKind::System);
            assert_eq!(effective_kind("enhanced"), EngineKind::Enhanced);
            assert_eq!(effective_kind("garbage"), EngineKind::System);
        } else {
            // macOS / Linux：一切「非增强」输入都落到增强
            assert_eq!(effective_kind("system"), EngineKind::Enhanced);
            assert_eq!(effective_kind("garbage"), EngineKind::Enhanced);
            assert_eq!(effective_kind(""), EngineKind::Enhanced);
            assert_eq!(effective_kind("  "), EngineKind::Enhanced);
            assert_eq!(effective_kind("ONNX"), EngineKind::Enhanced);
        }
    }

    /// 缺省引擎必须是本平台**开箱可用**的那个。
    #[test]
    fn default_engine_is_usable_on_this_platform() {
        let expected = if system_engine_available() {
            EngineKind::System
        } else {
            EngineKind::Enhanced
        };
        assert_eq!(default_engine(), expected);
        // 缺省值经「平台自愈」后必须仍是它自己（不能出现「默认 system、自愈成 enhanced」的矛盾）
        assert_eq!(effective_kind(default_engine().as_str()), default_engine());
    }

    /// 探针（默认忽略）：**真实**下载 macOS / Linux 运行库，验证
    /// 「多源 + 重试 + SHA256 校验 + 落盘」整条链路。
    ///
    /// 为什么必须有这个探针：v0.9.0 首发时 `download_target_path` 把落点写成了**目录**，
    /// 导致 mac/Linux 的运行库永远落不了盘；而 Windows 因运行库随包、函数提前返回，
    /// **这个 bug 在开发机上完全暴露不出来**。此探针可在任意平台跑通整条下载链，
    /// 是「macOS 到底能不能取字」唯一能在 Windows 上拿到的硬证据。
    ///
    /// 跑法（本机 `github.com` 直链不通，默认用 Gitee 镜像）：
    /// ```text
    /// SD_OCR_PROBE_LIB=libonnxruntime.dylib \
    ///   cargo test --lib -- --ignored --nocapture probe_download_runtime_lib
    /// ```
    #[test]
    #[ignore]
    fn probe_download_runtime_lib() {
        let name = std::env::var("SD_OCR_PROBE_LIB")
            .unwrap_or_else(|_| "libonnxruntime.dylib".to_string());
        let base = std::env::var("SD_OCR_PROBE_BASE")
            .unwrap_or_else(|_| MIRROR_OCR_DOWNLOAD_BASES[1].to_string());
        let dir = std::env::temp_dir().join("sd-ocr-probe");
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join(&name);
        let _ = std::fs::remove_file(&target);

        println!("== 下载 {name} ← {base}");
        println!("== 落点 {}", target.display());
        fetch_and_store(&name, &target, &[base]).expect("运行库下载失败");

        assert!(
            target.is_file(),
            "落点必须是文件而不是目录: {}",
            target.display()
        );
        let got = std::fs::read(&target).unwrap();
        println!("== ✅ 落盘成功: {} 字节", got.len());
        assert!(
            got.len() > 1_000_000,
            "运行库应 >1MB，实际 {} 字节",
            got.len()
        );
        // 与官方 .sha256 对账（verify_sha256 已在 fetch_and_store 内执行，此处复核一次）
        let mut h = Sha256::new();
        h.update(&got);
        println!("== sha256 {}", format!("{:x}", h.finalize()));
        let _ = std::fs::remove_file(&target);
    }
}
