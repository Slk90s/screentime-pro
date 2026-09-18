//!
//! screenshot/mod.rs
//! 屏幕截图（v0.8.0，2026-09-16）：全局快捷键 → 全屏遮罩框选 → 复制剪贴板 / 落盘归档。
//!
//! 设计思路（对齐 docs/SCREENSHOT.md，Windows/macOS 优先，Linux 微调）：
//! - **冻结帧编辑器**（v0.8.0 修订）：抓屏后把整屏 PNG 交给遮罩窗显示。遮罩窗从此是「一张静止的
//!   桌面图 + 标注画布」，而不是「透出实时桌面的透明窗」——这是能做标注（文字 / 马赛克 / 箭头 /
//!   画笔）与圆角实时预览的前提。**旧版透明遮罩窗没有图像，标注根本无从下手**（画笔/OCR 只能是
//!   灰掉的占位按钮），这是"想加文字只能关掉重截"的根因。
//! - **合成在前端**：裁剪、标注、圆角、投影统一由遮罩窗的 canvas 画好后，以 PNG(dataURL) 交回
//!   本模块，本模块只负责「进剪贴板 / 落盘 / 入库 / FIFO 清理」。好处是所见即所得——屏幕上预览
//!   到的就是导出的那张图，不存在「预览一套、Rust 再算一套」的偏差。
//! - **剪贴板优先**：确认（✓ / Enter）的默认动作是「把 PNG 写入系统剪贴板」，落盘是可选项。
//! - **hide-self**：截图前隐藏 pet / float 两个自家置顶窗（否则会拍进自己的桌宠和指标条），
//!   确认或取消后**原样恢复**（只恢复本来就可见的窗口，绝不复活用户本来就关掉的窗）。
//! - **隐私**：图片只落本机 `screenshots/` 目录，零上传；删除走系统回收站而非直接 unlink。
//! - Linux：xcap 在 X11/Wayland 取屏实现细节较多，逻辑与前两端一致，属「可微调」范围。
//!
//! 修改历史：
//!   - 2026-09-16 @v0.8.0: 初始创建 - 截屏缓存/遮罩窗/裁剪合成（圆角+阴影）/剪贴板/归档/回收站删除
//!   - 2026-09-16 @v0.8.0: 修订 - 遮罩窗由「透明透桌面」改为「冻结帧 + 标注画布」；
//!     新增 `screenshot_frame` 下发整屏图；`screenshot_commit` 改为接收前端合成好的 PNG；
//!     `screenshot_set_config` 返回真实快捷键注册结果（ApplyResult），快捷键被占用时前端可提示；
//!     删除不再使用的 `compose_export` / `sd_rounded_box`（合成职责已移交前端 canvas）。
//!   - 2026-09-17 @v0.8.1: 新增 - 取字（本地离线 OCR，`ocr.rs` + `screenshot_ocr` +
//!     `screenshot_copy_text`）。裁剪用缓存帧在 Rust 侧做，前端只传**物理像素**矩形。
//!   - 2026-09-17 @v0.9.0: 新增 - 取字「引擎调度」（`ocr_engine.rs`）：
//!     ① `ScreenshotConfig` 增第 8 个键 `ocr_engine`（`system` / `enhanced`，脏值一律回 `system`）；
//!        ⚠️ 归一化**只有一个口径** `normalized_engine()`，`load` / `save` /
//!        `screenshot_set_config` 三处共用 —— 真机验证曾抓到：只归一化 DB 而把**原始入参**
//!        放进内存缓存时，`screenshot_get_config`（读缓存）会把脏值漏回前端，
//!        造成「接口报的值 ≠ 实际生效的值」。改动此处务必保持三处口径一致。
//!     ② `screenshot_ocr` 改经 `ocr_engine::recognize()` 分发，返回体增 `engine` / `lines`
//!        （同一结构承载两个引擎的输出，前端据 `engine` 显示真正生效的引擎）；
//!     ③ 新增 IPC `ocr_engine_info`（截图上交 14 个、全项目 80→**81**）；
//!     ④ 打开遮罩即后台预热增强引擎，把 ~2.5s 模型加载摊到用户框选的那几秒里。
//!     ⚠️ 增强引擎资源（`ort/onnxruntime.dll` + `models/*.onnx`，共 ~33MB）由
//!     `tauri.windows.conf.json` 声明为 bundle resources **随包分发**（不做首次下载）；
//!     路径解析一律多候选探测（安装目录 / 开发态 `src-tauri/` / `SD_OCR_MODELS`）。
//!

mod ocr;
mod ocr_engine;
mod ocr_onnx;
/// macOS 系统取字引擎（Vision framework）—— 「标准」引擎的 mac 实现。
/// 仅在 macOS 上编译：Windows 走 `ocr.rs` 的 WinRT，Linux 的系统识别尚未落地。
#[cfg(target_os = "macos")]
mod ocr_vision;

use crate::AppState;
use base64::Engine as _;
use image::codecs::png::PngEncoder;
use image::{imageops, ExtendedColorType, ImageEncoder, RgbaImage};
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
};

/// 遮罩窗固定 label（同时声明在 tauri.conf.json 的 windows[]，保证权限与幂等复用）
pub const CAPTURE_WINDOW_LABEL: &str = "capture";

/// 自家需要「截图时临时隐藏」的置顶窗 label
const SELF_OVERLAY_LABELS: [&str; 2] = ["pet", "float"];

/// 缓存的整屏帧（物理像素 RGBA8），确认时按选区裁剪
pub struct CachedFrame {
    pub img: RgbaImage,
}

/// 截图运行时状态（由 lib.rs `app.manage` 注入）
pub struct ScreenshotState {
    pub frame: Mutex<Option<CachedFrame>>,
    /// 本次截图被临时隐藏的自家窗口 label（只为这些恢复可见）
    pub hidden: Mutex<Vec<String>>,
    pub config: Mutex<ScreenshotConfig>,
}

impl ScreenshotState {
    pub fn new(config: ScreenshotConfig) -> Self {
        Self {
            frame: Mutex::new(None),
            hidden: Mutex::new(Vec::new()),
            config: Mutex::new(config),
        }
    }
}

// ===== 配置 =====
// 落 settings 表 8 个 key；默认值面向「QQ 式」肌肉记忆：确认即复制到剪贴板。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScreenshotConfig {
    /// 截图总开关（关掉后快捷键与托盘「截图」均不响应）
    pub enabled: bool,
    /// 全局快捷键（global-hotkey 语法，如 CmdOrCtrl+Shift+A）
    pub shortcut: String,
    /// 确认时默认动作 = 复制到剪贴板
    pub auto_copy: bool,
    /// 确认时同时落盘到历史目录
    pub auto_save: bool,
    /// 默认圆角（逻辑像素；0 = 直角）
    pub corner_radius: u32,
    /// 默认是否给导出图加投影
    pub shadow: bool,
    /// 历史截图 FIFO 上限（超出后最旧的移入回收站）
    pub max_count: u32,
    /// 取字引擎（v0.9.0）：`system` = 系统内置 / `enhanced` = PaddleOCR-ONNX 本地模型。
    /// 存字符串而非枚举，是为了让「旧配置没有这个键」与「用户手改脏值」都能安全回落。
    /// ⚠️ 内存/库里的值**恒为归一化后的结果**（见 [`Self::normalized_engine`]）：
    /// 缺省值按平台定（Windows = `system`，macOS/Linux = `enhanced`），
    /// 且在没有标准引擎的平台上「标准」会被自愈成「增强」。
    pub ocr_engine: String,
}

impl Default for ScreenshotConfig {
    fn default() -> Self {
        // 默认快捷键取 CmdOrCtrl+Shift+A（A = 截图/Annotate 记忆点）：
        // 刻意不用 Win+Shift+S —— 那是 Windows 自带「截图工具」的系统级热键，
        // RegisterHotKey 会失败或被系统抢先，用户按下去会弹系统截图工具。
        Self {
            enabled: true,
            shortcut: "CmdOrCtrl+Shift+A".into(),
            auto_copy: true,
            auto_save: false,
            corner_radius: 8,
            shadow: false,
            max_count: 200,
            // 默认引擎**按平台定**（v0.9.0 修复）：Windows 用标准（零体积零依赖、
            // 无模型加载耗时）；macOS / Linux 没有标准引擎实现，默认必须直接给「增强」，
            // 否则开箱取字必然报「仅支持 Windows」（v0.9.0 首发就是这个毛病）。
            ocr_engine: ocr_engine::default_engine().as_str().into(),
        }
    }
}

impl ScreenshotConfig {
    fn b(v: bool) -> &'static str {
        if v {
            "true"
        } else {
            "false"
        }
    }

    /// 引擎值的**唯一**归一化口径。
    ///
    /// 走 [`ocr_engine::effective_kind`]（而非裸 `EngineKind::parse`），因此不仅是
    /// 「脏值 / 空值 → 标准」，还带**平台自愈**：本平台没有标准引擎（macOS / Linux）
    /// 却配了标准时，归一化结果直接是「增强」。
    ///
    /// 为什么必须自愈：v0.9.0 的默认值是 `system`，macOS 用户只要改过任意一项截图
    /// 设置（触发 `save()` 就会把 `system` 写进 `screenshot_ocr_engine`），
    /// 升级后若只改默认值不治存量，取字仍会一直报「仅支持 Windows」。
    ///
    /// `load` / `save` / `screenshot_set_config` 三处共用，避免「一处归一化、
    /// 另一处原样透传」的口径分裂。
    fn normalized_engine(&self) -> &'static str {
        ocr_engine::effective_kind(&self.ocr_engine).as_str()
    }

    /// 就地归一化全部「有规范形式」的字段（目前只有 `ocr_engine`）。
    ///
    /// 为什么必须做：`screenshot_set_config` 把**入参**放进内存缓存，而
    /// `screenshot_get_config` 读的正是这份缓存 —— 只在 `save()` 里归一化 DB 的话，
    /// 脏值会从读接口漏回前端（设置页据此渲染会「两个引擎都不选中」）。
    /// 取字功能本身不受影响（所有调用点都会 `EngineKind::parse` 兜底），
    /// 但读接口报出的值必须与真正生效的一致。
    pub fn normalize(&mut self) {
        self.ocr_engine = self.normalized_engine().to_string();
    }

    pub fn load(db: &crate::db::AppDb) -> Self {
        let d = Self::default();
        let mut cfg = Self {
            enabled: db
                .get_setting("screenshot_enabled")
                .map(|s| s == "true")
                .unwrap_or(d.enabled),
            shortcut: db
                .get_setting("screenshot_shortcut")
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(d.shortcut),
            auto_copy: db
                .get_setting("screenshot_auto_copy")
                .map(|s| s == "true")
                .unwrap_or(d.auto_copy),
            auto_save: db
                .get_setting("screenshot_auto_save")
                .map(|s| s == "true")
                .unwrap_or(d.auto_save),
            corner_radius: db
                .get_setting("screenshot_corner_radius")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(d.corner_radius),
            shadow: db
                .get_setting("screenshot_shadow")
                .map(|s| s == "true")
                .unwrap_or(d.shadow),
            max_count: db
                .get_setting("screenshot_max_count")
                .and_then(|s| s.parse::<u32>().ok())
                .filter(|v| *v >= 10)
                .unwrap_or(d.max_count),
            // 原样取库值，随后统一走 normalize() 归一化
            ocr_engine: db
                .get_setting("screenshot_ocr_engine")
                .unwrap_or(d.ocr_engine),
        };
        // 未知/空值一律回 system（normalize 内置兜底），保证取字永远有个能用的引擎
        cfg.normalize();
        cfg
    }

    pub fn save(&self, db: &crate::db::AppDb) -> rusqlite::Result<()> {
        db.set_setting("screenshot_enabled", Self::b(self.enabled))?;
        db.set_setting("screenshot_shortcut", &self.shortcut)?;
        db.set_setting("screenshot_auto_copy", Self::b(self.auto_copy))?;
        db.set_setting("screenshot_auto_save", Self::b(self.auto_save))?;
        db.set_setting("screenshot_corner_radius", &self.corner_radius.to_string())?;
        db.set_setting("screenshot_shadow", Self::b(self.shadow))?;
        db.set_setting("screenshot_max_count", &self.max_count.to_string())?;
        // 存「归一化后」的引擎值，脏值不落库（读侧也有兜底，这里是第二道保险）
        db.set_setting("screenshot_ocr_engine", self.normalized_engine())?;
        Ok(())
    }
}

/// 截图目录：`<app_data_dir>/screenshots/`（v0.8.0 固定，不做可配置路径，
/// 避免「设置里能改路径、但 rel_path 设计跟不上」的口径分裂）
pub fn screenshots_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法定位应用数据目录: {e}"))?;
    let dir = base.join("screenshots");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建截图目录失败: {e}"))?;
    Ok(dir)
}

fn default_file_name() -> String {
    format!(
        "ScreenShot_{}.png",
        chrono::Local::now().format("%Y%m%d_%H%M%S_%3f")
    )
}

// ===== 图像编解码 =====
//
// 圆角 / 投影 / 标注的合成已整体移交遮罩窗的 canvas 完成（见模块头「合成在前端」），
// 本模块只保留编解码：thumbnail 用 encode_png，提交时直接落前端传来的 PNG 字节。
// v0.8.0 初版曾在此用 SDF 做圆角+投影（`compose_export` / `sd_rounded_box`），
// 因「预览一套、导出另一套」的偏差风险已删除。

fn encode_png(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut buf: Vec<u8> = Vec::new();
    PngEncoder::new(&mut buf)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("PNG 编码失败: {e}"))?;
    Ok(buf)
}

fn write_clipboard(img: &RgbaImage) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("剪贴板不可用: {e}"))?;
    cb.set_image(arboard::ImageData {
        width: img.width() as usize,
        height: img.height() as usize,
        bytes: Cow::Borrowed(img.as_raw()),
    })
    .map_err(|e| format!("写入剪贴板失败: {e}"))
}

/// 把文件移入系统回收站（失败时退化为直接删除并记日志，保证不阻断主流程）
fn move_to_trash(path: &std::path::Path) {
    match trash::delete(path) {
        Ok(()) => tracing::info!(path = %path.display(), "截图已移入回收站"),
        Err(e) => {
            tracing::warn!(error = %e, path = %path.display(), "移入回收站失败，退化为直接删除");
            let _ = std::fs::remove_file(path);
        }
    }
}

// ===== 自家窗口 hide-self / 恢复 =====

fn hide_self(app: &AppHandle, state: &ScreenshotState) {
    let mut hidden = state.hidden.lock().unwrap_or_else(|e| e.into_inner());
    hidden.clear();
    for label in SELF_OVERLAY_LABELS {
        if let Some(w) = app.get_webview_window(label) {
            if w.is_visible().unwrap_or(false) {
                let _ = w.hide();
                hidden.push(label.to_string());
            }
        }
    }
}

/// 恢复本次被临时隐藏的自家窗口（**只恢复本来就可见的**，不复活用户关掉的窗）
pub fn restore_self(app: &AppHandle, state: &ScreenshotState) {
    let mut hidden = state.hidden.lock().unwrap_or_else(|e| e.into_inner());
    for label in hidden.drain(..) {
        if let Some(w) = app.get_webview_window(&label) {
            let _ = w.show();
        }
    }
}

// ===== 遮罩窗 =====

fn ensure_capture_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    if let Some(w) = app.get_webview_window(CAPTURE_WINDOW_LABEL) {
        return Ok(w);
    }
    WebviewWindowBuilder::new(
        app,
        CAPTURE_WINDOW_LABEL,
        WebviewUrl::App("index.html".into()),
    )
    .title("ScreenTime Capture")
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .shadow(false)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .closable(false)
    .focused(true)
    .visible(false)
    .build()
    .map_err(|e| format!("创建截图遮罩窗失败: {e}"))
}

fn hide_capture_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(CAPTURE_WINDOW_LABEL) {
        let _ = w.hide();
    }
}

// ===== 截图主流程 =====

/// 触发一次截图（全局快捷键 / 托盘菜单 / 前端调用共用）。
/// 整体异步执行：隐藏自家窗 → 等一帧 → 抓屏 → 亮遮罩。
pub fn begin_capture(app: &AppHandle) {
    // v0.9.0：选了增强引擎就在后台先把模型加载好（~2.5s）。
    // 用户框完选区再点「取字」通常要好几秒，这段时间足够把加载摊掉；
    // 不预热的话第一次取字要多等一次模型加载（3.1s → 体感「卡住」）。
    warm_ocr_engine(app);
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = begin_capture_inner(&app2).await {
            tracing::error!(error = %e, "启动截图失败");
            // 失败路径：遮罩窗不亮，但必须把 hide-self 隐藏的窗还回去
            if let Some(state) = app2.try_state::<ScreenshotState>() {
                restore_self(&app2, &state);
            }
            hide_capture_window(&app2);
            let _ = app2.emit_to("main", "screenshot-error", e);
        }
    });
}

/// 后台预热增强引擎（非阻塞；已加载则是一次锁检查，几乎零成本）
fn warm_ocr_engine(app: &AppHandle) {
    let Some(state) = app.try_state::<ScreenshotState>() else {
        return;
    };
    let kind = ocr_engine::effective_kind(
        &state
            .config
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .ocr_engine,
    );
    if kind != ocr_engine::EngineKind::Enhanced {
        return;
    }
    let rd = resource_dir(app);
    tauri::async_runtime::spawn_blocking(move || {
        // 运行库缺失（macOS / Linux 不随包）→ 先尝试后台静默下载，再预热
        ocr_engine::download_runtime_lib_if_missing(rd.as_deref());
        let paths = ocr_engine::EnginePaths::resolve(rd.as_deref());
        let (Some(models), lib) = (
            paths.models_path().map(Path::to_path_buf),
            paths.ort_lib_path().map(Path::to_path_buf),
        ) else {
            return;
        };
        if let Err(e) = ocr_onnx::warmup(&models, lib.as_deref()) {
            // 预热失败不致命：真正取字时会再试一次，并把原因展示给用户
            tracing::warn!(error = %e, "增强引擎预热失败");
        }
    });
}

async fn begin_capture_inner(app: &AppHandle) -> Result<(), String> {
    let state = app
        .try_state::<ScreenshotState>()
        .ok_or_else(|| "截图状态未初始化".to_string())?;
    let cfg = state
        .config
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    if !cfg.enabled {
        tracing::debug!("截图总开关关闭，忽略触发");
        return Ok(());
    }

    // ① 先隐藏自家置顶窗（pet/float），等合成器稳定后再抓屏
    hide_self(app, &state);
    tokio::time::sleep(Duration::from_millis(160)).await;

    // ② 抓主显示器整屏（物理像素）
    let monitors = xcap::Monitor::all().map_err(|e| format!("枚举显示器失败: {e}"))?;
    if monitors.is_empty() {
        return Err("未找到可用显示器".into());
    }
    let m = monitors
        .iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .unwrap_or(&monitors[0]);
    let img = m.capture_image().map_err(|e| format!("屏幕捕获失败: {e}"))?;
    let mx = m.x().unwrap_or(0);
    let my = m.y().unwrap_or(0);
    let mw = m.width().unwrap_or(img.width());
    let mh = m.height().unwrap_or(img.height());
    tracing::info!(w = img.width(), h = img.height(), x = mx, y = my, "整屏已抓取");

    let scale = m.scale_factor().unwrap_or(1.0) as f64;
    let phys_w = img.width();
    let phys_h = img.height();
    let logical_w = phys_w as f64 / scale;
    let logical_h = phys_h as f64 / scale;

    *state.frame.lock().unwrap_or_else(|e| e.into_inner()) = Some(CachedFrame { img });

    // ③ 亮遮罩窗，覆盖整个显示器
    let win = ensure_capture_window(app)?;
    win.set_position(PhysicalPosition::new(mx, my))
        .map_err(|e| format!("定位遮罩窗失败: {e}"))?;
    win.set_size(PhysicalSize::new(mw, mh))
        .map_err(|e| format!("调整遮罩窗尺寸失败: {e}"))?;
    win.show().map_err(|e| format!("显示遮罩窗失败: {e}"))?;
    let _ = win.set_focus();
    // 事件只带元信息（尺寸/DPI/默认样式），整屏图由前端在收到事件后调
    // `screenshot_frame` 取回来——把几 MB 的图从事件通道里挪开，避免事件体过大。
    let _ = app.emit_to(
        CAPTURE_WINDOW_LABEL,
        "capture-ready",
        CaptureReadyPayload {
            width: logical_w,
            height: logical_h,
            physical_width: phys_w,
            physical_height: phys_h,
            scale_factor: scale,
            default_radius: cfg.corner_radius,
            default_shadow: cfg.shadow,
        },
    );
    Ok(())
}

#[derive(Clone, serde::Serialize)]
pub struct CaptureReadyPayload {
    /// 遮罩窗的逻辑尺寸（CSS 像素），前端据此铺满
    pub width: f64,
    pub height: f64,
    /// 整屏帧的物理像素尺寸（= CSS 尺寸 × scale_factor）
    pub physical_width: u32,
    pub physical_height: u32,
    pub scale_factor: f64,
    pub default_radius: u32,
    pub default_shadow: bool,
}

/// 取本次截图的整屏帧（PNG data URL，物理像素原始分辨率）。
///
/// 前端拿到后直接当 `<img>` 显示 + 当 canvas 的绘制源：**必须是原始物理分辨率**，
/// 否则高 DPI 屏上导出的标注与文字会糊。冻结帧只在一次截图会话内有效，
/// 提交/取消后清空 → 返回空串。
#[tauri::command]
pub fn screenshot_frame(state: tauri::State<'_, ScreenshotState>) -> Result<String, String> {
    let guard = state.frame.lock().unwrap_or_else(|e| e.into_inner());
    let frame = guard.as_ref().ok_or_else(|| "截图会话已结束".to_string())?;
    let png = encode_png(&frame.img)?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    ))
}

#[derive(Clone, serde::Serialize)]
pub struct ScreenshotResult {
    pub id: Option<i64>,
    pub file_name: Option<String>,
    pub path: Option<String>,
    pub width: u32,
    pub height: u32,
    pub copied: bool,
    pub saved: bool,
}

/// 确认截图：解码前端合成好的 PNG → 复制剪贴板 / 落盘归档 → 收遮罩 → 恢复自家窗
///
/// 入参是遮罩窗 canvas 合成后的**最终图**（已含裁剪、标注、圆角、投影），
/// 本模块不再做任何像素加工——保证「屏幕上预览到的」与「导出的」逐像素一致。
#[tauri::command]
pub async fn screenshot_commit(
    app: AppHandle,
    png_base64: String,
    copy: bool,
    save: bool,
) -> Result<ScreenshotResult, String> {
    let state = app
        .try_state::<ScreenshotState>()
        .ok_or_else(|| "截图状态未初始化".to_string())?;

    // 前端可能带 `data:image/png;base64,` 前缀，两种写法都容忍
    let b64 = png_base64
        .rsplit(',')
        .next()
        .unwrap_or(png_base64.as_str())
        .trim();
    let png = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| format!("截图数据解码失败: {e}"))?;
    if png.is_empty() {
        return Err("没有要导出的图片".into());
    }
    // 解码一份 RGBA 用于写剪贴板；落盘则直接用原始 PNG 字节，避免二次编码
    let out = image::load_from_memory(&png)
        .map_err(|e| format!("截图数据解析失败: {e}"))?
        .to_rgba8();
    let (ow, oh) = (out.width(), out.height());

    let mut result = ScreenshotResult {
        id: None,
        file_name: None,
        path: None,
        width: ow,
        height: oh,
        copied: false,
        saved: false,
    };

    // ① 剪贴板（默认动作）
    if copy {
        write_clipboard(&out)?;
        result.copied = true;
        tracing::info!(w = ow, h = oh, "截图已复制到剪贴板");
    }

    // ② 落盘 + 入库 + FIFO 清理
    if save {
        let dir = screenshots_dir(&app)?;
        let file_name = default_file_name();
        let path = dir.join(&file_name);
        std::fs::write(&path, &png).map_err(|e| format!("写入截图文件失败: {e}"))?;

        let app_state = app
            .try_state::<Arc<AppState>>()
            .ok_or_else(|| "应用状态未初始化".to_string())?;
        let created_at = chrono::Local::now().to_rfc3339();
        let id = app_state
            .db
            .insert_screenshot(
                &file_name,
                ow,
                oh,
                png.len() as i64,
                &created_at,
                &app_state.device_id,
            )
            .map_err(|e| format!("写入截图索引失败: {e}"))?;
        result.id = Some(id);
        result.file_name = Some(file_name);
        result.path = Some(path.to_string_lossy().to_string());
        result.saved = true;
        tracing::info!(path = %path.display(), bytes = png.len(), "截图已归档");

        prune_history(&app, &app_state, &state);
    }

    // ③ 通知主窗口（无论只复制还是只保存都要提示，文案由前端按 copied/saved 组合）
    let _ = app.emit_to("main", "screenshot-done", result.clone());

    // ④ 收尾：清帧、收遮罩、恢复自家窗
    *state.frame.lock().unwrap_or_else(|e| e.into_inner()) = None;
    hide_capture_window(&app);
    restore_self(&app, &state);
    Ok(result)
}

/// 取消截图：清帧、收遮罩、恢复自家窗
#[tauri::command]
pub fn screenshot_cancel(app: AppHandle) -> Result<(), String> {
    if let Some(state) = app.try_state::<ScreenshotState>() {
        *state.frame.lock().unwrap_or_else(|e| e.into_inner()) = None;
        restore_self(&app, &state);
    }
    hide_capture_window(&app);
    Ok(())
}

/// FIFO 清理：超出 max_count 的最旧截图移入回收站并从索引删除。
/// 触发时机：每次确认落盘后（应用启动时不需要，因为每次落盘都会收敛到上限）。
fn prune_history(app: &AppHandle, app_state: &Arc<AppState>, state: &ScreenshotState) {
    let keep = state
        .config
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .max_count;
    let dir = match screenshots_dir(app) {
        Ok(d) => d,
        Err(_) => return,
    };
    let stale = match app_state.db.screenshots_exceeding(keep) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "查询超限截图失败");
            return;
        }
    };
    for (id, file_name) in stale {
        move_to_trash(&dir.join(&file_name));
        let _ = app_state.db.delete_screenshot_by_id(id);
    }
}

/// 前端手动触发一次截图（设置页「立即截图」按钮，等价于按快捷键）
#[tauri::command]
pub fn screenshot_trigger(app: AppHandle, state: tauri::State<'_, ScreenshotState>) -> bool {
    if !state.config.lock().unwrap_or_else(|e| e.into_inner()).enabled {
        return false;
    }
    begin_capture(&app);
    true
}

#[tauri::command]
pub fn screenshot_get_config(state: tauri::State<'_, ScreenshotState>) -> ScreenshotConfig {
    state
        .config
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// 快捷键注册结果（回给设置页，用于「被占用」提示）
#[derive(Debug, Clone, serde::Serialize)]
pub struct ApplyResult {
    /// 是否真正注册成功（总开关关闭时视为「无需注册」，也算成功）
    pub applied: bool,
    /// 实际生效的快捷键字符串
    pub shortcut: String,
    /// 注册失败原因（被系统/其他软件占用、格式非法等）
    pub error: Option<String>,
}

/// 保存配置：持久化 + 重新注册全局快捷键。
///
/// 返回**真实注册结果**——初版无论成败都 `Ok(true)`，导致用户填了个被系统占用的
/// 组合（如 Ctrl+Shift+S）时界面显示"已设置"但快捷键其实是死的，
/// 用户感受就是"快捷键无法设置"。现在失败会把原因带回设置页。
#[tauri::command]
pub fn screenshot_set_config(
    app: AppHandle,
    state: tauri::State<'_, ScreenshotState>,
    mut config: ScreenshotConfig,
) -> Result<ApplyResult, String> {
    let app_state = app
        .try_state::<Arc<AppState>>()
        .ok_or_else(|| "应用状态未初始化".to_string())?;
    // v0.9.0：**先归一化再落库 + 进缓存**。只归一化 DB 是不够的 ——
    // `screenshot_get_config` 读的是内存缓存，脏值会从读接口漏回前端
    // （设置页据此渲染会「两个引擎都不选中」），且接口报的值与实际生效的不一致。
    config.normalize();
    config.save(&app_state.db).map_err(|e| e.to_string())?;
    *state.config.lock().unwrap_or_else(|e| e.into_inner()) = config.clone();
    // v0.8.2：浮窗尾部「截图」按钮的可见性走 get_status_bar_config（读 AppState 缓存、
    // 浮窗 1Hz 轮询）。截图开关变化必须同步这份缓存，否则浮窗按钮显隐与实际开关脱节。
    if let Some(app_state) = app.try_state::<Arc<AppState>>() {
        app_state
            .status_bar_config
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .screenshot_enabled = config.enabled;
    }
    let outcome = crate::screenshot::shortcut::apply(&app, &config);
    Ok(ApplyResult {
        applied: outcome.is_ok(),
        shortcut: config.shortcut.clone(),
        error: outcome.err(),
    })
}

/// 截图目录（设置页只读展示 + 「打开目录」）
#[tauri::command]
pub fn screenshot_dir(app: AppHandle) -> Result<String, String> {
    Ok(screenshots_dir(&app)?.to_string_lossy().to_string())
}

#[tauri::command]
pub fn screenshot_list(
    state: tauri::State<'_, Arc<AppState>>,
    limit: u32,
) -> Result<Vec<crate::db::ScreenshotOut>, String> {
    state
        .db
        .list_screenshots(limit.clamp(1, 500))
        .map_err(|e| e.to_string())
}

/// 删除历史截图（图片移入回收站 + 删除索引）
#[tauri::command]
pub fn screenshot_delete(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    id: i64,
) -> Result<bool, String> {
    let name = state
        .db
        .delete_screenshot(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "截图不存在".to_string())?;
    let dir = screenshots_dir(&app)?;
    let path = dir.join(&name);
    if path.exists() {
        move_to_trash(&path);
    }
    Ok(true)
}

/// 在文件管理器中定位截图文件
#[tauri::command]
pub fn screenshot_reveal(app: AppHandle, id: i64) -> Result<(), String> {
    let app_state = app
        .try_state::<Arc<AppState>>()
        .ok_or_else(|| "应用状态未初始化".to_string())?;
    let name = app_state
        .db
        .screenshot_file_name(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "截图不存在".to_string())?;
    let path = screenshots_dir(&app)?.join(name);
    reveal_in_file_manager(&path.to_string_lossy());
    Ok(())
}

/// 历史缩略图（data URL）。返回空串表示文件缺失。
/// 缩略图走小尺寸 PNG（默认宽 320），单张约 30–80KB，可安全走 IPC。
#[tauri::command]
pub fn screenshot_thumbnail(app: AppHandle, id: i64, max_width: u32) -> Result<String, String> {
    let app_state = app
        .try_state::<Arc<AppState>>()
        .ok_or_else(|| "应用状态未初始化".to_string())?;
    let name = app_state
        .db
        .screenshot_file_name(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "截图不存在".to_string())?;
    let path = screenshots_dir(&app)?.join(name);
    if !path.exists() {
        return Ok(String::new());
    }
    let img = image::open(&path).map_err(|e| format!("读取截图失败: {e}"))?;
    let (w, h) = (img.width(), img.height());
    let max_width = max_width.clamp(80, 800);
    let scale = (max_width as f64 / w as f64).min(1.0);
    let thumb = if scale < 1.0 {
        let nw = (w as f64 * scale).max(1.0) as u32;
        let nh = (h as f64 * scale).max(1.0) as u32;
        imageops::resize(&img.to_rgba8(), nw, nh, imageops::FilterType::Triangle)
    } else {
        img.to_rgba8()
    };
    let png = encode_png(&thumb)?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    ))
}

/// 取字：对选区做**本地离线** OCR（v0.8.1，不联网、不上传）。
///
/// 入参是**物理像素**矩形——由前端按 `k = physical_width / innerWidth` 换算后再传，
/// 与导出合成共用同一口径；Rust 侧不再自己猜一次 DPI，避免多显示器/混合缩放下两边不一致。
/// 裁剪在 Rust 做：整屏帧本就缓存在 `state.frame`，不必让前端把几 MB 的图再传回来。
#[tauri::command]
pub async fn screenshot_ocr(
    app: AppHandle,
    state: tauri::State<'_, ScreenshotState>,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<ocr::OcrOut, String> {
    // ① 锁内只做裁剪：把选区拷成一张小图后立刻放锁，
    //    否则识别期间（几十到几百 ms）会一直占着 frame 锁。
    let crop = {
        let guard = state.frame.lock().unwrap_or_else(|e| e.into_inner());
        let frame = guard
            .as_ref()
            .ok_or_else(|| "截图会话已结束，无法取字".to_string())?;
        let img = &frame.img;
        let (iw, ih) = (img.width() as i32, img.height() as i32);
        let x1 = x.clamp(0, iw);
        let y1 = y.clamp(0, ih);
        let x2 = x.saturating_add(width).clamp(0, iw);
        let y2 = y.saturating_add(height).clamp(0, ih);
        if x2 - x1 < 1 || y2 - y1 < 1 {
            return Err("取字范围为空".into());
        }
        imageops::crop_imm(img, x1 as u32, y1 as u32, (x2 - x1) as u32, (y2 - y1) as u32).to_image()
    };

    // ② 引擎选择与资源路径在此解析（读配置 + 探测文件系统），
    //    再连同裁剪图一起 move 进阻塞任务 —— 阻塞线程里不碰 Tauri 状态。
    let kind = ocr_engine::effective_kind(
        &state
            .config
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .ocr_engine,
    );
    let paths = ocr_engine::EnginePaths::resolve(resource_dir(&app).as_deref());

    // ③ 识别必须离开 async 上下文：WinRT 的 `IAsyncOperation::join()` 是阻塞等待，
    //    且 COM 初始化属线程级状态 —— 两者都要求「初始化与调用在同一线程」。
    tauri::async_runtime::spawn_blocking(move || ocr_engine::recognize(kind, &paths, &crop))
        .await
        .map_err(|e| format!("取字任务异常：{e}"))?
}

/// 取字引擎信息（设置页展示：当前引擎 / 增强引擎资源是否齐备 / 体积）
#[tauri::command]
pub fn ocr_engine_info(
    app: AppHandle,
    state: tauri::State<'_, ScreenshotState>,
) -> ocr_engine::EngineInfo {
    let kind = ocr_engine::effective_kind(
        &state
            .config
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .ocr_engine,
    );
    let paths = ocr_engine::EnginePaths::resolve(resource_dir(&app).as_deref());
    ocr_engine::EngineInfo::probe(kind, &paths)
}

/// Tauri 资源目录（失败不致命：`candidate_roots` 里还有 exe 同级等候选）
fn resource_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().resource_dir().ok()
}

/// 把取字结果写进系统剪贴板（复用截图同一条 arboard 路径，不额外引剪贴板插件）
#[tauri::command]
pub fn screenshot_copy_text(text: String) -> Result<(), String> {
    if text.is_empty() {
        return Err("没有可复制的文字".into());
    }
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("剪贴板不可用: {e}"))?;
    cb.set_text(text)
        .map_err(|e| format!("写入剪贴板失败: {e}"))
}

/// 各平台「在文件管理器中选中文件」
fn reveal_in_file_manager(path: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = crate::proc::hidden("open").args(["-R", path]).status();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = crate::proc::hidden("explorer")
            .arg(format!("/select,{path}"))
            .status();
    }
    #[cfg(target_os = "linux")]
    {
        // Linux 无跨 DE 的「选中文件」标准，退化为打开所在目录
        let dir = std::path::Path::new(path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());
        let _ = crate::proc::hidden("xdg-open").arg(dir).status();
    }
}

// ===== 全局快捷键注册 =====

pub mod shortcut {
    use super::ScreenshotConfig;
    use tauri::AppHandle;
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    /// 应用配置里的快捷键：先撤掉旧键（换键时避免旧键仍生效），再注册新键。
    ///
    /// 返回注册结果而不是只记日志——设置页需要据此告诉用户「这个组合被占用了」，
    /// 否则用户会以为设置成功但按下去毫无反应。
    pub fn apply(app: &AppHandle, cfg: &ScreenshotConfig) -> Result<(), String> {
        let gs = app.global_shortcut();
        if let Err(e) = gs.unregister_all() {
            tracing::warn!(error = %e, "注销旧快捷键失败");
        }
        if !cfg.enabled {
            tracing::info!("截图总开关关闭，不注册快捷键");
            return Ok(());
        }
        match gs.register(cfg.shortcut.as_str()) {
            Ok(()) => {
                tracing::info!(shortcut = %cfg.shortcut, "截图快捷键已注册");
                Ok(())
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    shortcut = %cfg.shortcut,
                    "截图快捷键注册失败（可能被系统或其他软件占用）"
                );
                Err(format!("{e}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v0.9.0：脏值必须在**进入内存缓存与 DB 之前**就被归一化。
    ///
    /// 这个测试来自一次真机验证抓到的真实缺陷：`save()` 只归一化了写入 DB 的值，
    /// 而 `screenshot_set_config` 把**原始入参**放进内存缓存、`screenshot_get_config`
    /// 又读那份缓存 —— 于是写入 `garbage-engine` 后读回来还是 `garbage-engine`。
    /// 取字功能本身没错（各调用点都会兜底），但接口报的值与实际生效的不一致，
    /// 设置页会「两个引擎都不选中」。
    ///
    /// ⚠️ 期望值**按平台**：归一化走 `effective_kind`，在没有标准引擎的平台
    /// （macOS / Linux）一切「非增强」输入都会被自愈成 `enhanced`。
    #[test]
    fn normalize_canonicalizes_ocr_engine() {
        let mut cfg = ScreenshotConfig::default();
        let fallback = if ocr_engine::system_engine_available() {
            "system"
        } else {
            // 本平台没有标准引擎 → 脏值必须自愈成「增强」，否则取字不可用
            "enhanced"
        };

        cfg.ocr_engine = "garbage-engine".into();
        cfg.normalize();
        assert_eq!(cfg.ocr_engine, fallback, "未知值必须回落到本平台可用引擎");

        cfg.ocr_engine = "  ".into();
        cfg.normalize();
        assert_eq!(cfg.ocr_engine, fallback, "空白值必须回落到本平台可用引擎");

        cfg.ocr_engine = "ONNX".into();
        cfg.normalize();
        assert_eq!(cfg.ocr_engine, "enhanced", "别名 ONNX 应归一化为 enhanced");

        // 幂等：归一化过的值再归一化不变
        cfg.normalize();
        assert_eq!(cfg.ocr_engine, "enhanced", "归一化必须幂等");
    }

    /// 缺省配置的引擎必须是「本平台开箱可用」的那个 ——
    /// macOS / Linux 若默认成 `system`，用户第一次取字就会看到「仅支持 Windows」。
    #[test]
    fn default_config_engine_is_usable_on_this_platform() {
        let cfg = ScreenshotConfig::default();
        let expected = if ocr_engine::system_engine_available() {
            "system"
        } else {
            "enhanced"
        };
        assert_eq!(cfg.ocr_engine, expected);
    }
}
