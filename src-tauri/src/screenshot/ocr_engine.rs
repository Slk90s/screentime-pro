//!
//! screenshot/ocr_engine.rs
//! 取字「引擎调度」（v0.9.0）：在标准引擎与增强引擎之间做选择，并统一解析引擎资源路径。
//!
//! 两个引擎的分工：
//! - **标准（`System`）**：**操作系统内置**的识别能力 —— 零体积、零联网、随系统语言包走。
//!   - Windows = WinRT `Media.Ocr`（`ocr.rs`）
//!   - macOS = Vision framework `VNRecognizeTextRequest`（`ocr_vision.rs`）
//!
//!   两端短板同源：小字号（<14px）与低对比场景丢字，且已实测
//!   「放大 / 灰度 / 留边」三种预处理全部无效（见 `ocr.rs` 头部否决记录）。
//!   Linux 的系统识别尚未落地，该平台上「标准」不可用。
//! - **增强（`Enhanced`）**：`ocr_onnx.rs` 的 PaddleOCR v4（det + rec 两段式）。
//!   短边不足 736px 的图会先被 det 放大再检测，小字在检测阶段就被拉大 ——
//!   这是单段链路（WinRT / Vision）做不到的，也是识别率提升的真正来源。
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
//!   平台二进制获取。**未就绪时不阻塞**：`recognize()` 立刻返回可读原因（提示稍后重试、
//!   或切回「标准」引擎），由用户决定，不做静默自动回落 —— Windows / macOS 有「标准」可退，
//!   Linux 本机没有，所以「回落」不是对所有平台都成立的说法。
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
use std::sync::atomic::{AtomicBool, Ordering};

/// 取字引擎类型（落 settings 键 `screenshot_ocr_engine`）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    /// 标准：操作系统内置（Windows = WinRT `Media.Ocr`；macOS = Vision）
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
/// 标准引擎 = 操作系统自带的识别能力：**Windows = WinRT `Media.Ocr`、
/// macOS = Vision framework**（`ocr_vision.rs`）。
/// Linux 尚未落地（系统识别计划接 Tesseract），在它上面 `ocr::recognize()`
/// 只会返回「暂不支持本平台」的错误。
pub const fn system_engine_available() -> bool {
    cfg!(target_os = "windows") || cfg!(target_os = "macos")
}

/// 标准引擎的**具体实现名**（仅用于设置页/排障展示，不参与任何逻辑分支）。
///
/// 有这个字段的原因：macOS 只能靠 CI 或用户反馈验证，出问题时能一眼看出
/// 「本机识别走的是 Vision 还是 WinRT」，省掉一轮来回猜。
pub const fn system_engine_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "winrt"
    } else if cfg!(target_os = "macos") {
        "vision"
    } else {
        ""
    }
}

/// 引擎**缺省值**（配置里没有这个键时使用）。
///
/// - 有标准引擎的平台（Windows / macOS）→ `system`：零额外体积、零依赖、
///   零模型加载，**且无需任何下载**（macOS 选它就不必等 43MB 运行库）；
/// - 没有标准引擎的平台（Linux）→ `enhanced`：若默认成 `system`，
///   开箱取字必然报「暂不支持本平台」，等于功能不可用。
pub const fn default_engine() -> EngineKind {
    if system_engine_available() {
        EngineKind::System
    } else {
        EngineKind::Enhanced
    }
}

/// 把「配置里的引擎字符串」解析为**本平台真正可用**的引擎。
///
/// 唯一会改写用户选择的情形：**本平台没有标准引擎（Linux）却配了标准** ——
/// 此时回落到增强，否则取字必然失败。这条同时也是**自愈路径**：
/// v0.9.0 **首发**那版在 macOS 上没有标准引擎（默认值却是 `system`），用户只要改过任意一项
/// 截图设置（触发 `save()`）就会被写成 `system`；本版 macOS 真的有了标准引擎
/// （Vision），这个 `system` 不再需要被改写，直接生效；Linux 上仍会被自愈成 `enhanced`。
/// 反过来，若用户在修复版里被自愈成了 `enhanced`，那是有效选择，**不再改动**。
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

    /// 模型是否齐备（det + rec 两个 `.onnx`）。
    ///
    /// 三端**一律随包**，所以这里为 `false` 只说明一件事：**安装不完整**
    /// （models/ 被删、或用了残缺的绿色包）。它是前端「增强」选项能否被选中的判据。
    pub fn models_ready(&self) -> bool {
        self.models_path().is_some_and(ocr_onnx::is_ready)
    }

    /// 运行库是否就绪（`onnxruntime` 动态库）。
    ///
    /// ⚠️ 与 [`Self::models_ready`]**必须分开看**：Windows 运行库随包，缺了是安装问题；
    /// 而 **macOS / Linux 运行库不随包**，缺了是**正常初始状态**（首次使用后台下载）。
    /// 一旦把两者揉成一个布尔值，mac 用户会掉进死锁：运行库没下载 → `enhanced_ready=false`
    /// → 「增强」置灰选不中 → 而下载**只在选中增强后才触发** → 永远选不中
    /// （这正是 v0.9.0 在 mac 上的实际表现，前端与后端各占一半责任）。
    pub fn runtime_ready(&self) -> bool {
        self.ort_lib_path().is_some_and(|p| p.is_file())
    }

    /// 增强引擎是否**真正可用**（运行库 + 两个模型都在）
    pub fn enhanced_ready(&self) -> bool {
        self.runtime_ready() && self.models_ready()
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
///
/// 🔴 2026-10-29 @v1.2.11（P0）：**下载落点必须通过完整性闸门才算「就绪」**。
///
/// 旧实现只判断 `cand.is_file()`。v1.2.10 可能已把一份短读的残缺 dylib 写进了
/// `用户缓存目录/ort/`，那些文件在升级后依然会被这里命中 → `ort::init_from` →
/// `Session::builder()` 内部 `expect("Failed to initialize ORT API")` →
/// 在 v1.2.10 及以前（release 还是 `panic = "abort"`）那是**每次启动、每次截图都必然
/// 整进程崩溃**；本版虽然改回了 `"unwind"`，但坏库依然会污染 ort 的全局状态，
/// 所以这里仍要把坏文件「认成不存在」，让它们走重新下载的路径。
/// 只把下载修好（写 .tmp + rename）救不了这批存量用户，必须在这里把坏文件
/// 「认成不存在」，让它们走重新下载的路径。
///
/// 判据用 [`probe_ort_lib`]（体积 + 平台魔数 + ORT 自检），不做完整 SHA256
/// （那需要网络，而本函数在取字同步路径上被调用）。
fn resolve_ort_lib(resource_dir: Option<&Path>) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("ORT_DYLIB_PATH") {
        let p = PathBuf::from(p);
        // 环境变量是开发者/探针显式指定的，不做完整性闸门：
        // 探针场景可能指向 pip 装的库，误杀会让人以为是代码坏了。
        if p.is_file() {
            return Some(p);
        }
    }
    scan_ort_roots(&candidate_roots(resource_dir), user_ort_dir().as_deref())
}

/// 逐个候选根目录验库。与 [`resolve_ort_lib`] 拆开的原因：根目录与「下载目录」
/// 都要能注入，否则「删不删这份文件」这条破坏性分支在单测里根本碰不到。
///
/// 三种结论对应三种处置（整改 0.1）：
/// - **可用** → 直接返回，优先级最高的那份就是它；
/// - **环境阻塞**（文件完好，但本机缺前置依赖，典型是 Windows 没装 VC++ 运行库）
///   → **保留文件**，且在没有其它可用库时把它返回出去：它比「什么都没有」更有价值 ——
///   `ort::init_from` 会原样带回系统错误码，[`ocr_onnx::ensure_env`] 据此提示装运行库。
///   这里删它是最坏的选择：随包库一旦消失，Windows 上没有下载通道可以补回来；
/// - **不可用**（体积/魔数/符号/版本不合格）→ 跳过；**仅当它躺在下载目录里**
///   （即「我们自己下载的那份」）才剔除，等待重新下载。
fn scan_ort_roots(roots: &[PathBuf], download_dir: Option<&Path>) -> Option<PathBuf> {
    let name = ort_lib_name();
    let mut env_blocked: Option<PathBuf> = None;
    for root in roots {
        for sub in ["ort", ""] {
            let cand = if sub.is_empty() {
                root.join(name)
            } else {
                root.join(sub).join(name)
            };
            if !cand.is_file() {
                continue;
            }
            match probe_ort_lib(&cand) {
                OrtProbe::Usable => return Some(cand),
                OrtProbe::EnvBlocked(reason) => {
                    tracing::error!(
                        path = %cand.display(),
                        reason = %reason,
                        "onnxruntime 运行库完好但本机缺前置依赖（未删除；取字时会给出安装提示）"
                    );
                    if env_blocked.is_none() {
                        env_blocked = Some(cand);
                    }
                }
                OrtProbe::Unusable(reason) => {
                    let ours = is_our_download_copy(&cand, download_dir);
                    tracing::error!(
                        path = %cand.display(),
                        bytes = std::fs::metadata(&cand).map(|m| m.len()).unwrap_or(0),
                        reason = %reason,
                        pruned = ours,
                        "onnxruntime 运行库文件不可用（体积/格式/符号/版本不合格）"
                    );
                    // 删不掉（只读安装目录等）也无妨，下一轮探测仍会跳过。
                    if ours {
                        let _ = std::fs::remove_file(&cand);
                    }
                }
            }
        }
    }
    env_blocked
}

/// 这份文件是不是「我们自己下载到缓存目录的那份」—— 唯一允许被剔除的情况。
/// 判据用父目录相等，不做前缀匹配：下载目录的父级也在用户缓存里，
/// 但那里的文件不是本程序写的。
fn is_our_download_copy(path: &Path, download_dir: Option<&Path>) -> bool {
    download_dir.is_some_and(|d| path.parent() == Some(d))
}

/// 动态库自检结论。
///
/// 拆成三态而不是 `bool` 的原因（整改 0.1）：布尔值会把「文件坏了」和
/// 「文件好好的、但这台机器加载不了」压成同一件事，而这两件事的**处置完全相反** ——
/// 前者该删掉重下，后者删了就把随包库永久毁掉，还会把「装 VC++ 运行库」这句
/// 唯一有用的提示挤掉。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OrtProbe {
    /// 文件完好且本机可加载
    Usable,
    /// 文件完好，但本机环境缺前置依赖（Windows = 未装 VC++ 运行库；POSIX = 缺 `@rpath` 依赖库）
    EnvBlocked(String),
    /// 文件本身不合格（体积异常 / 不是本平台动态库 / 截断 / 没有 ORT 符号 / API 版本不匹配）
    Unusable(String),
}

/// 运行库「像不像一份可用的 ORT 动态库」—— 纯本地判定，不联网。
///
/// 三道闸门，任何一道不过即判 [`OrtProbe::Unusable`]：
/// 1. **体积** ≥ [`RUNTIME_MIN_BYTES`]：拦住短读、错误页、空文件；
/// 2. **平台魔数**：拦住「HTML 错误页被存成 .dylib」这类完全不是库的文件；
/// 3. **符号与版本自检**：真正 dlopen 一次并调 `OrtGetApiBase` /
///    `GetApi(ORT_API_VERSION)` —— 这两步正是 `ort::setup_api` 里会
///    `expect` panic 的位置（`"`OrtGetApiBase` must be present"` 与
///    `"Failed to initialize ORT API"`），在这里提前做完并转成 `Unusable`，
///    就等于**把那条 abort 路径彻底堵死**。
///
/// 第 3 道闸门里 dlopen 失败要再分一次岔（见 [`is_missing_dependency`]）：
/// 文案指向「依赖库找不到」时文件是**好的**，坏的是本机环境 → [`OrtProbe::EnvBlocked`]。
///
/// 可用的结果按 `(路径, 长度, mtime)` 缓存：同一份文件只 dlopen 一次，
/// 避免每张截图重复映射 30MB 动态库。文件被替换（长度或 mtime 变）会自动重验，
/// 因此「先下坏包 → 再下好包」也能正确翻案。
pub(crate) fn probe_ort_lib(path: &Path) -> OrtProbe {
    let Ok(meta) = std::fs::metadata(path) else {
        return OrtProbe::Unusable("文件不存在".to_string());
    };
    let len = meta.len();
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 只缓存「可用」：坏文件必须每次重判（可能刚被重新下载覆盖）；
    // 「环境阻塞」也不能缓存 —— 用户装完 VC++ 运行库后不重启应用就该直接可用。
    if let Ok(g) = VERIFIED_LIBS.lock() {
        if let Some((p, l, m)) = g.as_ref() {
            if p == path && *l == len && *m == mtime {
                return OrtProbe::Usable;
            }
        }
    }
    if len < RUNTIME_MIN_BYTES as u64 {
        return OrtProbe::Unusable(format!("体积 {} 字节，低于下限 {}", len, RUNTIME_MIN_BYTES));
    }
    let Ok(mut f) = std::fs::File::open(path) else {
        return OrtProbe::Unusable("打不开该文件".to_string());
    };
    let mut head = [0u8; 8];
    if std::io::Read::read_exact(&mut f, &mut head).is_err() {
        return OrtProbe::Unusable("读不出文件头".to_string());
    }
    if !magic_matches(&head) {
        return OrtProbe::Unusable("文件头不是本平台动态库".to_string());
    }
    match probe_ort_api(path) {
        OrtProbe::Usable => {
            if let Ok(mut g) = VERIFIED_LIBS.lock() {
                *g = Some((path.to_path_buf(), len, mtime));
            }
            OrtProbe::Usable
        }
        other => other,
    }
}

/// 加载失败的原因是不是「本机缺前置依赖」（而不是文件本身坏）。
///
/// 认得出来的依据只有错误文案，所以这张表要按各平台加载器的实际输出列：
/// - **Windows**：`LoadLibraryExW` 对「依赖的 DLL 找不到」返回 `ERROR_MOD_NOT_FOUND`(126)，
///   对「依赖的 DLL 里少了这个导出函数」返回 `ERROR_PROC_NOT_FOUND`(127)。
///   126 正是没装 VC++ 2015-2022 运行库（`vcruntime140.dll` / `msvcp140.dll` 缺失）的表现。
///   文案随系统语言变（中文系统给「找不到指定的模块。」），所以错误码和文案都匹配。
/// - **macOS**：`dyld` 报 `Library not loaded: @rpath/... Reason: image not found`。
/// - **Linux**：缺 C++ 运行库时是 `version 'GLIBCXX_x.y.z' not found` / `CXXABI_...`。
///
/// ⚠️ 反过来，`os error 193`（「不是有效的 Win32 应用程序」）**不算**环境缺依赖：
/// 它是 PE 头不合格 / 位宽不符，对本程序而言等价于「这份文件不行」。
/// Linux 的 `cannot open shared object file` 也不列进来 —— 该文案对「文件自己在哪」
/// 和「依赖找不到」是歧义的，宁可判成「不可用」（不删随包库，只是不选用它）。
fn is_missing_dependency(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    const NEEDLE: &[&str] = &[
        "os error 126",
        "os error 127",
        "module could not be found",
        "procedure could not be found",
        "找不到指定的模块",
        "找不到指定的程序",
        "image not found",
        "library not loaded",
        "glibcxx_",
        "cxxabi_",
    ];
    NEEDLE.iter().any(|n| m.contains(n))
}

/// 最近一次通过自检的运行库指纹 `(路径, 长度, mtime 秒)`。
static VERIFIED_LIBS: std::sync::Mutex<Option<(PathBuf, u64, u64)>> = std::sync::Mutex::new(None);

/// 各平台动态库的魔数。
///
/// 只认「确实是本平台动态库」的文件，与后续能否加载解耦：
/// 一份 HTML 错误页 / JSON 报错体即使大于 10MB 也不会带这些魔数。
fn magic_matches(head: &[u8; 8]) -> bool {
    if cfg!(target_os = "windows") {
        // PE：'MZ'
        head.starts_with(b"MZ")
    } else if cfg!(target_os = "macos") {
        // Mach-O 64 小端 / FAT（通用二进制）/ FAT64
        head.starts_with(&[0xCF, 0xFA, 0xED, 0xFE])
            || head.starts_with(&[0xCE, 0xFA, 0xED, 0xFE])
            || head.starts_with(&[0xCA, 0xFE, 0xBA, 0xBE])
    } else {
        // ELF
        head.starts_with(&[0x7F, b'E', b'L', b'F'])
    }
}

/// 真正加载一次并做 ORT API 自检。
///
/// 只 `dlopen` + 取两个符号 + 调 `GetApi`，**不创建 Environment / Session**，
/// 因此不会与随后 `ort::init_from` 产生的全局状态冲突：
/// POSIX 下同一个库被 dlopen 两次返回同一 handle（引用计数），
/// 这里 drop 掉本地 handle 只是减一次引用，ort 自己持有的那份仍然有效。
///
/// 加载失败分两类（整改 0.1）：文案指向「依赖库找不到」→ [`OrtProbe::EnvBlocked`]
/// （文件是好的，**不许删**，也不许谎报「下载不完整」）；其余 → [`OrtProbe::Unusable`]。
/// 取到符号之后 `GetApi` 返回 null 属于 **API 版本不匹配** —— 那正是 ort 内部
/// `expect("Failed to initialize ORT API")` 的位置，必须在这里判死，绝不让调用方走到那步。
fn probe_ort_api(path: &Path) -> OrtProbe {
    // SAFETY: 只在本地已通过体积与魔数检查的文件上调用；
    // 符号签名取自 `onnxruntime_c_api.h` 的公开 ABI：
    //   `const OrtApiBase* OrtGetApiBase(void)`
    //   `struct OrtApiBase { const OrtApi* (*GetApi)(uint32_t version);
    //                        const char* (*GetVersionString)(void); }`
    unsafe {
        let lib = match libloading::Library::new(path) {
            Ok(l) => l,
            Err(e) => {
                let msg = e.to_string();
                return if is_missing_dependency(&msg) {
                    OrtProbe::EnvBlocked(msg)
                } else {
                    OrtProbe::Unusable(msg)
                };
            }
        };
        let Ok(get_base) =
            lib.get::<unsafe extern "C" fn() -> *const OrtApiBase>(b"OrtGetApiBase\0")
        else {
            return OrtProbe::Unusable(
                "库内没有 OrtGetApiBase 符号（这不是 onnxruntime 运行库）".to_string(),
            );
        };
        let base = get_base();
        if base.is_null() {
            return OrtProbe::Unusable("OrtGetApiBase 返回空指针".to_string());
        }
        let base = &*base;
        let api = (base.get_api)(ORT_API_VERSION as i32);
        if api.is_null() {
            return OrtProbe::Unusable(format!(
                "该库不支持 ORT API 版本 {ORT_API_VERSION}（与 ort 编译期版本不匹配）"
            ));
        }
        // 版本串只用于诊断日志（也是 GetVersionString 的唯一用途，
        // 顺带证明这个字段的偏移没错位 —— 若结构体布局错了，这里会拿到空指针）。
        let vs = (base.get_version_string)();
        if !vs.is_null() {
            let s = std::ffi::CStr::from_ptr(vs).to_string_lossy();
            tracing::debug!(version = %s, path = %path.display(), "onnxruntime 运行库自检通过");
        }
        OrtProbe::Usable
    }
}

/// `onnxruntime_c_api.h` 中的 `ORT_API_VERSION`。
///
/// 必须与 `ort-sys` 编译期使用的值一致，否则自检可能放行一个 ort 自己会拒绝的库。
/// 这里直接引用 `ort::sys`（`ort` 已 `pub use ort_sys as sys`）而不是本地写死数字：
/// 升级 `ort` 时 API 版本若变化，下面这行断言会在**编译期**报错，
/// 而不是等到线上自检与 ort 实际行为不一致。
const ORT_API_VERSION: u32 = ort::sys::ORT_API_VERSION;

/// `OrtApiBase` 的最小 ABI 视图（只取前两个字段，其余不关心）。
#[repr(C)]
struct OrtApiBase {
    get_api: unsafe extern "C" fn(version: i32) -> *const std::ffi::c_void,
    get_version_string: unsafe extern "C" fn() -> *const std::ffi::c_char,
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
///
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

/// 下载体积下限（字节）：低于此值一律视为残缺包，**不与网络交互就丢弃**。
///
/// 2026-10-29 @v1.2.11 修复（macOS 截图后崩溃的 P0 根因之一）：
/// 真机日志出现过「`attempt=1` 之后既无重试也无失败日志，紧接着进程 panic」——
/// 连接被中途切断时 `reqwest` 的 `bytes()` 在部分场景返回**短读的 Ok**，
/// 一份几 KB 的残缺 dylib 被当成下载成功写进缓存目录。
///
/// 取 8MB 的理由（**不是越大越安全**）：这是**廉价的预筛**，只需拦住
/// 「错误页 / 空文件 / 明显短读」这类一眼假的东西，真正权威的判据是
/// [`probe_ort_lib`] 里的 dlopen + ORT API 自检（截断的 PE/Mach-O
/// 一定过不了加载器校验）。
///
/// 上限方向的风险更大：Windows 的随包 `onnxruntime.dll` 实测 18,446,136 字节，
/// macOS dylib ~43MB、Linux so ~29MB。下限若贴近 18MB，就会把 Windows
/// 随包库误判为损坏并删除 —— 而 Windows 根本没有下载通道，
/// 增强引擎会直接不可用。8MB 相对最小的随包库留出 2.3 倍余量。
const RUNTIME_MIN_BYTES: usize = 8 * 1024 * 1024;

/// 可选 SHA256 校验。
///
/// ⚠️ 2026-10-29 @v1.2.11 语义变更（**安全方向**）：
/// 旧版把「拉不到 `.sha256`」判为**通过**（`return true`），即校验形同虚设 ——
/// 真正的校验只有发布方提供 `.sha256` 且网络恰好成功时才生效，而这恰恰是
/// 最不可靠的一环。现在改为**三态**：
/// - `Some(true)`  ：拿到了摘要且**匹配** → 可信
/// - `Some(false)` ：拿到了摘要但**不匹配** → 一定是坏包，必须丢弃
/// - `None`        ：摘要不可用（404 / 网络失败 / 内容非法）→ **无法判定**，
///   由调用方结合 [`RUNTIME_MIN_BYTES`] 的体积下限做兜底判断，而不是盲目放行。
///
/// 这样「网络差」只会退化为「按体积粗筛」，不会退化为「完全不校验」。
fn verify_sha256(
    client: &reqwest::blocking::Client,
    base: &str,
    name: &str,
    bytes: &[u8],
) -> Option<bool> {
    let Ok(hr) = client.get(format!("{}/{}.sha256", base, name)).send() else {
        return None;
    };
    if !hr.status().is_success() {
        return None;
    }
    let Ok(htext) = hr.text() else { return None };
    let expected = htext
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if expected.len() != 64 || !expected.bytes().all(|c| c.is_ascii_hexdigit()) {
        tracing::warn!("增强引擎运行库 .sha256 内容非摘要，无法校验");
        return None;
    }
    let mut h = Sha256::new();
    h.update(bytes);
    let got = format!("{:x}", h.finalize());
    if got != expected {
        tracing::error!(expected = %expected, got = %got, "增强引擎运行库 SHA256 校验失败，已丢弃");
        return Some(false);
    }
    Some(true)
}

/// 下载 → 校验 → 落盘（生产路径与测试探针共用同一实现）。
///
/// ⚠️ **只在后台线程调用**：运行库 29~43MB，慢网下可能耗时数分钟。
/// ⚠️ `target` 必须是**文件路径**（`<ort 目录>/<库文件名>`）—— 传目录必然写失败，
/// 见 [`download_target_path`] 记录的历史 bug。
///
/// 🔴 2026-10-29 @v1.2.11（P0）：**先写 `.tmp` → 校验 → 原子 rename**。
///
/// 旧实现是「先校验字节、再 `fs::write(target)` 直写目标路径」，有两个致命缺口：
/// 1. `verify_sha256` 拉不到 `.sha256` 时返回 `true`（等于不校验），
///    于是任何短读 / 错误页都能落盘；
/// 2. `fs::write` 是**非原子**的（内部 create + 截断 + 写），
///    进程在写入中途被 panic / 被杀 → 目标路径留下**半截 dylib**。
///
/// 而 `resolve_ort_lib` 只判断 `is_file()`，半截文件会被认定为「已就绪」，
/// 下次启动 `ort::init_from` 载入坏库 → `Session::builder()` 内部
/// `expect("Failed to initialize ORT API")` → 配上当时（v1.2.10）的
/// `panic = "abort"` → **整个进程消失**。这正是真机日志里「截完图就崩溃」的机制。
///
/// 现在的不变式：**`target` 要么不存在，要么是一份通过校验的完整文件**。
/// `.tmp` 是唯一的中间态，且每次进入都先清理，不会跨进程残留。
fn fetch_and_store(name: &str, target: &Path, bases: &[String]) -> Result<(), String> {
    let client = download_client().map_err(|e| format!("创建下载客户端失败: {e}"))?;
    let Some((base, bytes)) = fetch_runtime_lib_bytes(&client, name, bases) else {
        return Err("所有下载源均失败".to_string());
    };

    // ① 体积闸门：不看网络、不看摘要，先按绝对下限筛掉残缺包。
    if bytes.len() < RUNTIME_MIN_BYTES {
        tracing::error!(
            bytes = bytes.len(),
            floor = RUNTIME_MIN_BYTES,
            "增强引擎运行库体积异常（疑似短读 / 错误页），已丢弃"
        );
        return Err(format!(
            "运行库体积异常：{} 字节（下限 {}）",
            bytes.len(),
            RUNTIME_MIN_BYTES
        ));
    }

    // ② 摘要三态：明确不匹配 → 拒绝；无从校验 → 依赖上面的体积闸门兜底。
    match verify_sha256(&client, &base, name, &bytes) {
        Some(false) => return Err("SHA256 校验失败".to_string()),
        Some(true) => tracing::info!("增强引擎运行库 SHA256 校验通过"),
        None => tracing::warn!("增强引擎运行库无可用 .sha256 摘要，已按体积闸门放行"),
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建 {} 失败: {e}", parent.display()))?;
    }

    // ③ 先写临时文件（同目录，保证 rename 不跨文件系统）。
    let tmp = target.with_extension(format!(
        "{}.tmp",
        target.extension().and_then(|e| e.to_str()).unwrap_or("bin")
    ));
    // 上一次异常退出可能留下 .tmp；先清掉，避免磁盘上堆积大文件。
    let _ = std::fs::remove_file(&tmp);
    std::fs::write(&tmp, &bytes).map_err(|e| format!("写入 {} 失败: {e}", tmp.display()))?;

    // ④ 回读长度复核：磁盘写满 / 配额限制时 write 可能"成功"但内容截断。
    let written = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
    if written != bytes.len() as u64 {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!(
            "写入不完整：期望 {} 字节，实际 {written}",
            bytes.len()
        ));
    }

    // ⑤ 原子转正。rename 覆盖同名文件在 POSIX 与 Windows 上均成立。
    if let Err(e) = std::fs::rename(&tmp, target) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("落盘 {} 失败: {e}", target.display()));
    }

    tracing::info!(path = %target.display(), bytes = bytes.len(), "增强引擎运行库下载完成");
    Ok(())
}

/// 「运行库正在下载」标记：把并发请求收敛成一次真正的下载。
///
/// 为什么必须：`warm_ocr_engine` 在**每次**触发截图时都会调用
/// [`download_runtime_lib_if_missing`]。运行库缺失时（macOS / Linux 常态，Windows 随包不受影响），
/// 用户连按几次快捷键就会**并发**拉起多个 42MB 下载，而且都写同一个目标文件。
///
/// 实测（v0.9.1 macOS 真机日志）：一分钟内连按 7 次 → **7 个 `attempt=1` 同时开跑**，
/// 日志刷屏之外，`download_target_path` 指向同一路径，存在被交叉写坏的风险。
/// 用一次性标记让「第一个真下、其余立即返回」；失败后标记复位，下次触发自然重试。
static RUNTIME_DOWNLOAD_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// 复位 [`RUNTIME_DOWNLOAD_IN_FLIGHT`]。
///
/// 用 RAII 而不是在函数末尾手动复位：`fetch_and_store` 若 panic（含解压 / IO 异常），
/// 手动复位会被跳过，标记永久为 `true` → 下载通道被一次 panic 死锁。
struct DownloadInFlightGuard;

impl Drop for DownloadInFlightGuard {
    fn drop(&mut self) {
        RUNTIME_DOWNLOAD_IN_FLIGHT.store(false, Ordering::Release);
    }
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
/// ⚠️ **可并发调用**（每次触发截图都会走一遍）：内部用 [`RUNTIME_DOWNLOAD_IN_FLIGHT`]
/// 收敛，重复请求会立即返回 `false` 而不是再开一个下载。
///
/// ⚠️ 仅在「本平台运行库未随包」时才有意义：Windows 运行库（`onnxruntime.dll`）随包，
/// `resolve_ort_lib` 已命中 → 直接返回 `true`，不会发任何网络请求。
pub fn download_runtime_lib_if_missing(resource_dir: Option<&Path>) -> bool {
    if resolve_ort_lib(resource_dir).is_some() {
        return true; // 已就绪，无需下载（Windows 常态）
    }
    // 并发收敛：已有下载在跑就直接返回（说明见 RUNTIME_DOWNLOAD_IN_FLIGHT）。
    // 返回 false 表示「本次没拿到」——调用方本来就要能处理「运行库暂不可用」。
    if RUNTIME_DOWNLOAD_IN_FLIGHT
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        tracing::debug!("增强引擎运行库正在下载中，跳过本次重复请求");
        return false;
    }
    let _guard = DownloadInFlightGuard;
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
pub fn recognize(
    kind: EngineKind,
    paths: &EnginePaths,
    img: &RgbaImage,
) -> Result<ocr::OcrOut, String> {
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
    /// 标准引擎在本平台是否可用（Windows = WinRT；macOS = Vision；Linux 暂无）
    pub system_available: bool,
    /// 标准引擎的具体实现（`winrt` / `vision`；无系统引擎时为空串）
    pub system_engine: String,
    /// 增强引擎是否可用（运行库 + 模型齐备，选了就能直接用）
    pub enhanced_ready: bool,
    /// 增强引擎**模型**是否齐备（三端随包；`false` = 安装不完整）
    pub enhanced_models_ready: bool,
    /// 增强引擎**运行库**是否就绪（mac / Linux 缺失属正常，会后台下载）
    pub enhanced_runtime_ready: bool,
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
            system_engine: system_engine_name().to_string(),
            enhanced_ready: paths.enhanced_ready(),
            enhanced_models_ready: paths.models_ready(),
            enhanced_runtime_ready: paths.runtime_ready(),
            ort_lib: paths.ort_lib.clone(),
            models_dir: paths.models_dir.clone(),
            enhanced_size_mb: (bytes as f64 / 1048576.0 * 100.0).round() / 100.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 产品路径不再需要这个布尔壳（调用方要的是三态结论，见 [`OrtProbe`]），
    /// 但回归测试要一句读得懂的断言：v1.2.10 之前的写法就是「文件存在 = 可用」。
    fn ort_lib_usable(path: &Path) -> bool {
        matches!(probe_ort_lib(path), OrtProbe::Usable)
    }

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

    /// 🐞 回归（P0，2026-10-29 @v1.2.11）：**残缺/伪造的运行库必须被判为不可用**。
    ///
    /// 背景：真机日志里「macOS 截完图就崩溃」，根因是缓存目录里躺着一份下载不完整的
    /// `libonnxruntime.dylib`。旧 `resolve_ort_lib` 只判断 `is_file()`，把它当成
    /// 「已就绪」交给 `ort::init_from`；而 ort 在 `setup_api()` 里对
    /// `GetApi` 返回 null 写的是 `expect(...)`，配合 `panic = "abort"`
    /// —— 进程被直接 abort，连 Err 都拿不到。
    ///
    /// 本测试锁定新的判据：体积、魔数、ORT API 自检，任一不过即 `false`。
    /// 若有人把自检退回「只看文件存在」，这里会立刻红。
    #[test]
    fn rejects_truncated_and_fake_runtime_libs() {
        let dir = std::env::temp_dir().join(format!("sd-ort-probe-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);

        // ① 空文件：体积闸门第一道就拦住
        let empty = dir.join("empty_lib.bin");
        std::fs::write(&empty, b"").unwrap();
        assert!(!ort_lib_usable(&empty), "空文件必须被判为不可用");

        // ② 太小：即便有正确魔数也不够（模拟短读截断）
        let tiny = dir.join("tiny_lib.bin");
        let mut tiny_bytes = vec![0u8; 1024];
        if cfg!(target_os = "windows") {
            tiny_bytes[..2].copy_from_slice(b"MZ");
        } else if cfg!(target_os = "macos") {
            tiny_bytes[..4].copy_from_slice(&[0xCF, 0xFA, 0xED, 0xFE]);
        } else {
            tiny_bytes[..4].copy_from_slice(&[0x7F, b'E', b'L', b'F']);
        }
        std::fs::write(&tiny, &tiny_bytes).unwrap();
        assert!(!ort_lib_usable(&tiny), "体积不足必须被判为不可用");

        // ③ 体积够大但内容不是动态库（HTML 错误页被存成 .dylib 的典型形态）
        let fake = dir.join("fake_lib.bin");
        let fake_bytes = vec![b'x'; RUNTIME_MIN_BYTES + 1024];
        std::fs::write(&fake, &fake_bytes).unwrap();
        assert!(
            !ort_lib_usable(&fake),
            "魔数不符（不是 PE/Mach-O/ELF）必须被判为不可用"
        );

        // ④ 不存在的路径不能 panic
        assert!(!ort_lib_usable(&dir.join("nope.bin")));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 🐞 回归（P0，2026-10-29 @v1.2.11 整改 0.1）：**自检失败的候选，只允许删我们自己下载的那份**。
    ///
    /// 旧实现对**所有**候选一律 `remove_file`，其中包含随包分发的 `onnxruntime.dll`
    /// （安装目录 / 仓库 `src-tauri/ort/`）。一旦失败原因是**环境**而非文件
    /// （典型：本机未装 VC++ 2015-2022 运行库 → dlopen 报 ERROR_MOD_NOT_FOUND 126），
    /// 一份完好的随包 dll 就会被当「坏文件」删掉。而 Windows 没有下载通道
    /// （`download_runtime_lib_if_missing` 对 Windows 提前返回），删了就是**永久不可用**。
    #[test]
    fn only_prunes_the_downloaded_copy() {
        let dir = std::env::temp_dir().join(format!("sd-ort-prune-{}", std::process::id()));
        let bundled = dir.join("install"); // 模拟安装目录里的随包库
        let download = dir.join("download"); // 模拟 user_ort_dir()
        std::fs::create_dir_all(&bundled).unwrap();
        std::fs::create_dir_all(&download).unwrap();

        let name = ort_lib_name();
        let bundled_lib = bundled.join(name);
        let download_lib = download.join(name);
        // 两份都写成几字节垃圾：体积闸门必然判坏 → 走进「剔除」分支
        std::fs::write(&bundled_lib, b"not a lib").unwrap();
        std::fs::write(&download_lib, b"not a lib").unwrap();

        let found = scan_ort_roots(&[bundled.clone(), download.clone()], Some(&download));
        assert!(found.is_none(), "两份都是坏文件，不该有命中：{found:?}");
        assert!(
            bundled_lib.exists(),
            "随包运行库自检失败时绝不能删除（Windows 无下载通道，删了即永久不可用）"
        );
        assert!(
            !download_lib.exists(),
            "下载目录里我们自己写的坏文件应被剔除，等待重新下载"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 同目录下的两份坏文件，若连下载目录都没配（取不到 HOME/LOCALAPPDATA），
    /// 一律**不删** —— 删除是破坏性操作，判据不明时宁可留着。
    #[test]
    fn prunes_nothing_without_a_known_download_dir() {
        let dir = std::env::temp_dir().join(format!("sd-ort-noprune-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let lib = dir.join(ort_lib_name());
        std::fs::write(&lib, b"not a lib").unwrap();
        assert!(scan_ort_roots(std::slice::from_ref(&dir), None).is_none());
        assert!(lib.exists(), "下载目录未知时不得删除任何候选文件");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 🐞 回归（P0，整改 0.1）：**「本机缺依赖」不等于「文件损坏」**。
    ///
    /// 这条判据决定两件事：① 随包库删不删（缺依赖 → 不删）；
    /// ② 给用户哪句提示（缺依赖 → 该说「装 VC++ 运行库」，而不是「文件损坏、稍后重试」）。
    #[test]
    fn missing_dependency_is_not_corruption() {
        let env_cases = [
            "The specified module could not be found. (os error 126)",
            "找不到指定的模块。 (os error 126)",
            "The specified procedure could not be found. (os error 127)",
            "找不到指定的程序。 (os error 127)",
            "dlopen(/tmp/x/libonnxruntime.dylib): Library not loaded: @rpath/libc++.1.dylib\n  Referenced from: ... Reason: image not found",
            "libonnxruntime.so: version `GLIBCXX_3.4.29' not found (required by libonnxruntime.so)",
        ];
        for msg in env_cases {
            assert!(is_missing_dependency(msg), "应判为「环境缺依赖」：{msg}");
        }
        // 这些才是「文件本身不行」：截断 / 位宽不符 / 不是本平台动态库
        let file_cases = [
            "%1 is not a valid Win32 application. (os error 193)",
            "不是有效的 Win32 应用程序。 (os error 193)",
            "dlopen failed: malformed mach-o image: section content out of bounds",
            "An unknown error occurred (os error -2147024704)",
            "",
        ];
        for msg in file_cases {
            assert!(
                !is_missing_dependency(msg),
                "不应判为「环境缺依赖」：{msg:?}"
            );
        }
    }

    /// 🐞 回归（P0，整改 0.1）：坏文件必须被**分类**，不能一律压成 `false`。
    #[test]
    fn probe_distinguishes_unusable_from_env_blocked() {
        let dir = std::env::temp_dir().join(format!("sd-ort-kind-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // 体积不足 → 文件不可用（不是环境问题）
        let tiny = dir.join(ort_lib_name());
        std::fs::write(&tiny, b"zip").unwrap();
        match probe_ort_lib(&tiny) {
            OrtProbe::Unusable(reason) => assert!(!reason.is_empty(), "结论要带可读原因"),
            other => panic!("小文件应判 Unusable，实际 {other:?}"),
        }
        // 不存在的路径同样不能 panic，也不能判成「环境阻塞」（否则 resolve 会把它当可用库返回）
        assert!(
            !matches!(probe_ort_lib(&dir.join("nope")), OrtProbe::EnvBlocked(_)),
            "不存在的文件不可能「完好但环境阻塞」"
        );
        assert!(!ort_lib_usable(&tiny));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 🐞 回归：体积下限必须严格小于真实运行库，否则 Windows 随包 dll 会被误杀。
    ///
    /// Windows 的 `onnxruntime.dll` 随包分发且只有 ~18MB。若 RUNTIME_MIN_BYTES
    /// 被调到超过它，`resolve_ort_lib` 会把随包库判成损坏并删除 ——
    /// 增强引擎在 Windows 上直接不可用，且下载通道也没配（Windows 本来不下载）。
    #[test]
    fn runtime_size_floor_leaves_headroom_for_bundled_dll() {
        // 随包 dll 实测 18,446,136 字节（2026-09-18）。下限必须留出足够余量。
        const BUNDLED_DLL_BYTES: u64 = 18_446_136;
        assert!(
            (RUNTIME_MIN_BYTES as u64) * 2 <= BUNDLED_DLL_BYTES,
            "下限 {} 相对随包 dll {} 余量不足，存在误杀风险",
            RUNTIME_MIN_BYTES,
            BUNDLED_DLL_BYTES
        );
    }

    /// 平台差异：本平台没有标准引擎时（当前只有 Linux），
    /// 配置里的 `system` 与任何脏值都必须被抬成 `enhanced`，
    /// 否则取字必然报「暂不支持本平台」。
    #[test]
    fn effective_kind_heals_on_platforms_without_system_engine() {
        if system_engine_available() {
            // Windows / macOS：如实尊重用户选择，脏值回标准
            assert_eq!(effective_kind("system"), EngineKind::System);
            assert_eq!(effective_kind("enhanced"), EngineKind::Enhanced);
            assert_eq!(effective_kind("garbage"), EngineKind::System);
        } else {
            // Linux：一切「非增强」输入都落到增强
            assert_eq!(effective_kind("system"), EngineKind::Enhanced);
            assert_eq!(effective_kind("garbage"), EngineKind::Enhanced);
            assert_eq!(effective_kind(""), EngineKind::Enhanced);
            assert_eq!(effective_kind("  "), EngineKind::Enhanced);
            assert_eq!(effective_kind("ONNX"), EngineKind::Enhanced);
        }
    }

    /// `system_engine_name()` 与 `system_engine_available()` 必须自洽：
    /// 设置页「标准」选项的可用性与实现名展示都读这两个值，
    /// 一旦打架（比如平台有引擎、实现名却是空串）设置页会出现自相矛盾的展示。
    #[test]
    fn system_engine_name_matches_availability() {
        let name = system_engine_name();
        assert_eq!(
            !name.is_empty(),
            system_engine_available(),
            "有系统引擎时实现名不能为空，反之亦然（实际 name={name:?}）"
        );
        assert!(
            matches!(name, "winrt" | "vision" | ""),
            "实现名只能是 winrt / vision / 空串，实际 {name:?}"
        );
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

    /// 回归：「模型就绪」与「运行库就绪」必须**各自独立**，不能揉成一个布尔值。
    /// 前端靠 `enhanced_models_ready` 决定「增强」能否被选中、靠
    /// `enhanced_runtime_ready` 决定提示「首次使用会自动下载」。
    /// 揉在一起就会重现 mac 死锁：运行库没下载 → 选项置灰 → 下载永不触发。
    #[test]
    fn models_and_runtime_readiness_are_independent() {
        let absent = EnginePaths {
            ort_lib: None,
            models_dir: Some("Z:/definitely/no/such/models/dir".into()),
        };
        assert!(!absent.models_ready());
        assert!(!absent.runtime_ready());
        assert!(!absent.enhanced_ready());

        // 构造「模型在、运行库不在」—— 这正是 mac / Linux 首次使用前的正常状态。
        let real = EnginePaths::resolve(None);
        if real.models_ready() {
            let before_download = EnginePaths {
                ort_lib: None,
                models_dir: real.models_dir.clone(),
            };
            assert!(before_download.models_ready(), "模型在就必须报 ready");
            assert!(!before_download.runtime_ready(), "没给运行库就不能报 ready");
            assert!(
                !before_download.enhanced_ready(),
                "缺运行库时整体仍未就绪（前端据此提示「下载中」）"
            );
        }
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
        println!("== sha256 {:x}", h.finalize());
        let _ = std::fs::remove_file(&target);
    }
}
