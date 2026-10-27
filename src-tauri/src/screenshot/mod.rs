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
//!   - 2026-09-18 @v0.9.1: 修复 - macOS「屏幕录制」权限闸门（P0）。
//!     无授权时 xcap 的 `CGWindowListCreateImage` **不报错**，返回一张「只有桌面壁纸」的图
//!     （macOS 隐私软化行为，xcap#123 有完整复现）→ 现象是「截图后应用全消失、只剩桌面」，
//!     而旧代码把这幅图当正常结果走完了整条链路（冻结帧 / 归档 / 剪贴板全都"成功"）。
//!     现在 `begin_capture_inner` 先过 `tracker::macos::ensure_screen_capture_ready()`
//!     （预检 → 主动 `CGRequestScreenCaptureAccess()`：把本应用登记进系统设置列表 + 弹系统
//!     授权框 → 复查；整段放在 `spawn_blocking` 里，因为首次授权会同步等用户点击）→ 未授权即
//!     返回可读原因，由前端弹窗提示 + 一键跳「屏幕录制」面板。
//!     配套：`open_privacy_settings` 加 `pane` 参数（缺省仍是辅助功能）；前端补上
//!     原本完全缺失的 `screenshot-error` 监听（此前失败是静默的）。
//!   - 2026-09-20 @v0.9.3: 重构 - macOS 授权逻辑整体迁出到 **`screenshot/macos.rs`**，
//!     `begin_capture_inner` 只调 `macos::ensure_ready()`。三处实质改变：
//!     ① **修掉「授权了还报没权限」**：此前只用 `CGPreflightScreenCaptureAccess()` 当唯一闸门，
//!        而它有 Apple 已知行为 —— 一旦返回 false，**在同一进程内可能一直返回 false**
//!        （真正的抓屏其实已经生效）。现新增独立第二信号「窗口标题探针」（窗口标题与窗口内容
//!        同属一张 TCC 授权，读得到标题 = 抓得到内容），两信号**取或**放行，不再被过期预检卡死。
//!     ② 失败文案按**实际查到的证据**分档：运行路径 / App Translocation / 下载隔离标记
//!        （com.apple.quarantine）/ 代码签名状态（ad-hoc 或未签名 = 授权反复失效的根因）。
//!     ③ 新增 IPC `reset_screen_capture_permission`（`tccutil reset ScreenCapture <bundle id>`，
//!        只清本应用条目），配合界面「重置权限并重启」一键清掉指向旧指纹的脏授权记录。
//!   - 2026-10-27 @v1.2.11: 修复 - `reset_permission` 支持指定 TCC 服务。
//!     新增 `macos::reset_permission_for(pane)`：`"accessibility"` → `tccutil reset
//!     Accessibility`，其余 → `ScreenCapture`。原实现把服务名硬编码，导致权限中心
//!     「辅助功能」那一行既没有重置按钮、提示文案却劝用户点「重置并重启」—— 动作与
//!     文案错配。旧的无参函数已删除（无调用者，留着只会产出死代码警告）。
//!     IPC 层新增 `reset_permission(pane)`，旧的 `reset_screen_capture_permission`
//!     保留为别名（全项目 IPC **84 → 85**）。
//!   - 2026-09-20 @v0.9.4: 改造 - 授权 UX 重做（用户反馈弹窗「太多太丑」）：
//!     ① `ensure_ready` 失败文案精简为一句话 + 结构化原因码（`__PERM__:<reason>`），
//!        完整证据只进日志；② 新增权限 IPC 四件套 `screenshot_permission_status` /
//!        `screenshot_request_permission` / `screenshot_open_permission_settings` /
//!        `screenshot_restart_app`（全项目 IPC 84→**88**），前端紧凑弹窗轮询 status、
//!        发现翻转后引导重启；③ `begin_capture_inner` 识别原因码，权限类失败**不再发**
//!        `screenshot-error`（改由前端主动轮询引导），非权限失败照旧。
//!   - 2026-10-17 @v1.1.1: 废弃 - **移除滑动截长图**（`long.rs` 全文 + 4 个 IPC +
//!     前端面板），截图 IPC 26→**22**。
//!     **全项目 IPC 95 → 84**（净减 11 = 长图 4 + `commands.rs` 清理 7；
//!     后者是工作区既有的、与本次截图修复无关的改动 —— 本模块只负责自己那 4 个）。
//!     移除原因（三条独立证据同时成立，详见 docs/SHOTFIX-ARCHITECTURE-2026-10-17.md）：
//!     ① **交互架构自相矛盾**：遮罩窗是 `always_on_top` 全屏置顶窗，它本身就是
//!     用户要滚动的那块屏幕的遮挡物，长图模式只把挖洞阴影改透明、窗口本体
//!     依然吃掉所有滚轮事件（项目从未对遮罩窗调 `set_ignore_cursor_events`），
//!     用户**根本无法滚动** —— 这不是算法缺陷，换 FFT 模板匹配也解决不了；
//!     ② **macOS 100% 不可用**：首屏冻结帧走 SCK（当时恒为 1920×1080），
//!     后续每屏走 xcap（真实物理分辨率），尺寸守卫必然触发；
//!     ③ **三平台无法拉齐**：自动滚动需 `enigo`（MSRV 1.77→1.85）+
//!     macOS 辅助功能新权限 + Linux Wayland 完全不支持。
//!     ⚠️ 未来若要重启长图：**不要**当成 bug 直接加回来，先解决遮罩窗鼠标穿透
//!     （控制 UI 必须移出遮罩窗到独立浮窗，项目已有 `set_ignore_cursor_events`
//!     先例见 `pet/hit_mask.rs`）。
//!   - 2026-10-17 @v1.1.1: 修复 - `begin_capture_inner` 的 `scale` 变量遮蔽去歧义
//!     （`capture_scale` / `window_scale` 两个**职责不同**的量，见各自注释），
//!     并让 `capture_with_xcap` 的**宽高与原点都**归一到物理像素（三平台口径各不同，
//!     证据见该函数头）；`CaptureReadyPayload` 四字段改为由 `capture_scale` 单一派生
//!     （`physical == round(logical × scale_factor)`，加 `debug_assert_eq!` 守着）。
//!     ⚠️ 前端只读 `physical_*`，payload 其余字段属排障信息 —— 保留是为了可观测性。
//!   - 2026-10-21 @v1.2.2: 废弃 - **移除「重复上次截图」**（用户要求，功能价值低于认知成本）。
//!     删除项：IPC `screenshot_repeat_region`、`ScreenshotState::last_region` 状态、
//!     `screenshot_commit` 的 `region` 入参（它只服务于 last_region 记录）、
//!     辅助函数 `save_cropped`（唯一调用方就是被删的命令；`screenshot_commit` 里那段
//!     落盘逻辑与它**同构但未复用**，删掉不会影响正常截图归档）、设置页按钮与 3 个 i18n 键。
//!     截图 IPC 22→**21**，全项目 IPC **83→82**。
//!   - 2026-10-21 @v1.2.2: 修复 - macOS 截图遮罩缺顶部菜单栏一行 / 「全屏变画中画」。
//!     根因见 `capture_window.rs` 模块头（tao 的 `always_on_top` → `NSFloatingWindowLevel`(3)
//!     低于菜单栏 24，AppKit 把 frame 收进 `visibleFrame`）。本文件只在 `show()` 前
//!     调一次 `capture_window::raise_capture_window`、`hide` 时调 `restore_capture_window`。
//!   - 2026-10-23 @v1.2.6: 修复 - `hide_capture_window` 新增「隐藏后若前台仍卡在遮罩窗上
//!     则主动让出焦点」。配合 `tauri.conf.json` 把 capture 窗口的 `focus` 由 true 改 false
//!     （隐藏窗夺前台会导致 `GetForegroundWindow()` 长期返回一个不可见窗口 → 采集整段失效，
//!     详见 `tracker/windows.rs` 修改历史）。这里**只在确认仍卡住时**才动焦点，
//!     避免正常截完图后把用户弹到主窗口。
//!

/// macOS 截图专属逻辑（屏幕录制授权闸门 / 授权失效根因诊断 / 一键重置授权）。
/// 仅在 macOS 上编译：Windows/Linux 抓屏不需要 TCC 授权，问题全在 mac 这一侧。
#[cfg(target_os = "macos")]
pub(crate) mod macos;

/// macOS SCK 截图引擎（ScreenCaptureKit 单帧截图，macOS 14+；低版本回落 xcap）。
/// 仅在 macOS 上编译。为什么引入见模块头注释 —— 核心是旧 API「未授权不报错」
/// 而 SCK「未授权真报错」，且 `SCShareableContent::get()` 是触发授权弹窗最可靠的方式。
#[cfg(target_os = "macos")]
pub(crate) mod macos_sck;

/// 全局光标坐标（v1.2.0 多屏截图「鼠标在哪截哪屏」的坐标源）。平台原生坐标。
mod cursor;

/// 截图遮罩窗的平台层修正（v1.2.2）：macOS 需把 NSWindow level 提到
/// screenSaver 级，否则始终被菜单栏压住 → 「全屏截图缺菜单栏那行」+「窗口
/// 被 AppKit 约束到 visibleFrame 导致画面纵向压扁，看起来像画中画」。
/// 非 macOS 上为空实现（Windows/Linux 的 always_on_top 已足够）。
mod capture_window;

mod ocr;
mod ocr_engine;
mod ocr_onnx;
/// macOS 系统取字引擎（Vision framework）—— 「标准」引擎的 mac 实现。
/// 仅在 macOS 上编译：Windows 走 `ocr.rs` 的 WinRT，Linux 的系统识别尚未落地。
#[cfg(target_os = "macos")]
mod ocr_vision;

/// 截图子系统跨平台分支（reveal / 权限 IPC / 抓屏）统一收口层。
/// 各平台实现见 `platform/{macos,windows,linux}.rs`，宿主只调 `Backend::*`。
pub(crate) mod platform;
use crate::screenshot::platform::{CaptureBackend, PermissionBackend, RevealBackend};

use crate::AppState;
use base64::Engine as _;
use image::codecs::png::PngEncoder;
use image::{imageops, ExtendedColorType, ImageEncoder, RgbaImage};
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder,
};

/// 遮罩窗固定 label（同时声明在 tauri.conf.json 的 windows[]，保证权限与幂等复用）
pub const CAPTURE_WINDOW_LABEL: &str = "capture";

/// 自家需要「截图时临时隐藏」的置顶窗 label
///
/// v0.9.7：加入 `capture`（遮罩窗自身）。原名单只有 pet/float，遮罩窗开着时
/// 再次触发截图（快捷键仍全局生效），第二次抓屏会把第一次的工具条/选区拍进
/// 新冻结帧 —— 用户看到「点截图后有个小窗口被截下来了」。
const SELF_OVERLAY_LABELS: [&str; 3] = ["pet", "float", CAPTURE_WINDOW_LABEL];

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
// 落 settings 表 7 个 key（v1.2.2：`screenshot_shadow` 随投影功能移除，8→7）；
// 默认值面向「QQ 式」肌肉记忆：确认即复制到剪贴板。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScreenshotConfig {
    /// 截图总开关（关掉后快捷键与托盘「截图」均不响应）
    pub enabled: bool,
    /// 全局快捷键（global-hotkey 语法，如 CmdOrCtrl+Shift+A）
    pub shortcut: String,
    /// 确认时同时落盘到历史目录
    pub auto_save: bool,
    /// 默认圆角（逻辑像素；0 = 直角）
    pub corner_radius: u32,
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
            auto_save: false,
            corner_radius: 8,
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
            auto_save: db
                .get_setting("screenshot_auto_save")
                .map(|s| s == "true")
                .unwrap_or(d.auto_save),
            corner_radius: db
                .get_setting("screenshot_corner_radius")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(d.corner_radius),
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
        db.set_setting("screenshot_auto_save", Self::b(self.auto_save))?;
        db.set_setting("screenshot_corner_radius", &self.corner_radius.to_string())?;
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
    // 恢复 tao 默认的浮动层级：遮罩窗只应在截图期间压着菜单栏。
    // 内部已做 run_on_main_thread 派发（NSWindow 属性只能在主线程改）。
    capture_window::restore_capture_window(app, CAPTURE_WINDOW_LABEL);
    if let Some(w) = app.get_webview_window(CAPTURE_WINDOW_LABEL) {
        let _ = w.hide();
        // v1.2.6：万一隐藏后前台仍停在这张遮罩窗上，主动让出去。
        //
        // 说明：Windows 在「前台窗口被 hide」时通常会自己激活下一个窗口，
        // 所以**多数情况这里什么都不做**。但被观察到的坏状态确实存在
        // （启动后 `GetForegroundWindow()` 长期返回 visible=False 的
        //   `ScreenTime Capture`），一旦卡住，采集会整段失效：
        //   A/B 实测 —— 前台=该隐藏窗 18s 新增 0 条；前台=真实应用 18s 新增 1 条。
        //
        // 因此只在「确认仍卡住」时才动焦点，避免正常截完图后被弹到主窗口打扰。
        // 判断与纠正都只在 Windows 上做（其它平台 hide 行为不同，且已有
        // tracker 侧的可见性兜底）。
        #[cfg(target_os = "windows")]
        if let Some(capture_hwnd) = windows_hwnd_of(&w) {
            if windows_foreground() == Some(capture_hwnd) {
                // 交给主窗口接手（用户下一步大概率在这里）；主窗口若被最小化，
                // set_focus 无效，此时退化为无操作，由 tracker 的 visible 检查兜住。
                if let Some(main) = app.get_webview_window("main") {
                    let _ = main.unminimize();
                    let _ = main.set_focus();
                }
            }
        }
    }
}

/// 取 Tauri 窗口在 Windows 上的 `HWND` 数值（非 Windows 恒为 `None`）。
#[cfg(target_os = "windows")]
fn windows_hwnd_of(w: &tauri::WebviewWindow) -> Option<isize> {
    w.hwnd().ok().map(|h| h.0 as isize)
}

/// 当前前台窗口的 `HWND` 数值（非 Windows 恒为 `None`）。
#[cfg(target_os = "windows")]
fn windows_foreground() -> Option<isize> {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    // Safety: 无参数、无副作用，返回当前前台窗口句柄（可能为空）。
    let h = unsafe { GetForegroundWindow() };
    if h.is_invalid() {
        None
    } else {
        Some(h.0 as isize)
    }
}

// ===== 截图主流程 =====

/// 截图会话进行中的闸门（v0.9.7）。
///
/// `begin_capture` 的三个入口（全局快捷键 / 托盘菜单 / 前端按钮）都是异步派发，
/// 遮罩窗亮着的时候快捷键依然全局生效 —— 旧实现没有任何重入保护，第二次触发
/// 会重新抓屏（把第一次的工具条拍进冻结帧）并重置遮罩窗状态。
/// RAII 守卫保证任何失败路径都会复位（手动复位在 panic 时会永久锁死）。
static CAPTURE_IN_FLIGHT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

struct CaptureInFlightGuard;

impl CaptureInFlightGuard {
    fn acquire() -> Option<Self> {
        use std::sync::atomic::Ordering;
        CAPTURE_IN_FLIGHT
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
            .then_some(Self)
    }
}

impl Drop for CaptureInFlightGuard {
    fn drop(&mut self) {
        CAPTURE_IN_FLIGHT.store(false, std::sync::atomic::Ordering::Release);
    }
}

/// 触发一次截图（全局快捷键 / 托盘菜单 / 前端调用共用）。
/// 整体异步执行：隐藏自家窗 → 等一帧 → 抓屏 → 亮遮罩。
pub fn begin_capture(app: &AppHandle) {
    begin_capture_delayed(app, 0);
}

/// 触发一次截图，可带倒计时（v0.10.0 延时截图，借鉴 Snapzy）。
///
/// `delay_secs = 0` 即普通即时截图（同 [`begin_capture`]）；大于 0 时先倒计时再抓屏。
/// 倒计时设计（与 Snapzy 一致的两个关键点）：
/// - **不抢焦点**：倒计时期间用户可以在别的应用里摆界面 / 展开菜单，抓屏时拍到的
///   就是摆好的状态。为此每秒只向 main 窗发 `screenshot-countdown` 事件（前端渲染
///   悬浮倒计时），后端不弹任何窗、不 set_focus；
/// - **可取消**：`screenshot_cancel_countdown` 置取消标志，倒计时循环每秒检查，
///   取消后静默退出（闸门复位、无任何窗态变化）。审计补充：**倒计时期间再按
///   截图快捷键 = 取消**——倒计时不抢焦点，主窗的取消按钮 / Esc 在用户焦点
///   位于其他应用时收不到（全局 keydown 也只对自家窗生效）；快捷键是全局
///   注册的，是唯一保证可达的取消通道。实现见下方重入分支。
///
/// 重入语义：倒计时全程持有 CaptureInFlight 闸门（延时截图本身也是一种进行中的会话）。
/// 闸门被占时不再一律忽略——先看是不是「延时倒计时进行中」，是则**置取消标志**
/// （快捷键 = 取消，全局可达）；否则维持旧行为忽略（遮罩亮着时重复触发会拍进
/// 自家工具条，取消反而引发事故）。
pub fn begin_capture_delayed(app: &AppHandle, delay_secs: u32) {
    // v0.9.7：重入保护 —— 已有一次截图会话在跑（遮罩窗亮着 / 抓屏进行中）时，
    // 忽略本次触发。修复「遮罩窗开着再按快捷键 → 工具条被截进新冻结帧」。
    // v0.10.0（审计补充）：倒计时期间重入 = 取消（详见函数头「可取消」段）。
    let Some(_guard) = CaptureInFlightGuard::acquire() else {
        if COUNTDOWN_ACTIVE.load(std::sync::atomic::Ordering::Acquire) {
            tracing::info!("延时截图倒计时中收到再次触发，按取消处理");
            CANCEL_COUNTDOWN.store(true, std::sync::atomic::Ordering::Release);
        } else {
            tracing::debug!("截图会话进行中，忽略重入触发");
        }
        return;
    };
    // v0.9.0：选了增强引擎就在后台先把模型加载好（~2.5s）。
    // 用户框完选区再点「取字」通常要好几秒，这段时间足够把加载摊掉；
    // 不预热的话第一次取字要多等一次模型加载（3.1s → 体感「卡住」）。
    warm_ocr_engine(app);
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        // 守卫 move 进任务：无论 begin_capture_inner 成功 / 失败 / 提前 return，
        // 任务结束时 Drop 都会把闸门复位。守卫覆盖的是**启动阶段**（权限闸门 +
        // 抓屏 + 亮遮罩，mac 首次授权时可达数秒）—— 这是重入最危险的窗口；
        // 遮罩亮起后的框选期间再触发，靠 SELF_OVERLAY_LABELS 含 capture 兜底
        //（hide-self 先把旧遮罩藏掉再抓屏，新冻结帧必然干净）。
        let _guard = _guard;

        // v0.10.0：倒计时段（delay_secs > 0 时）。每秒发事件给 main 窗渲染倒计时，
        // 取消标志一旦置位立即退出。倒计时在权限闸门之前——没授权的用户提前 3 秒
        // 发现取消不了也没意义，授权引导留在真正抓屏时走既有链路。
        if delay_secs > 0 {
            CANCEL_COUNTDOWN.store(false, std::sync::atomic::Ordering::Release);
            COUNTDOWN_ACTIVE.store(true, std::sync::atomic::Ordering::Release);
            for remain in (1..=delay_secs).rev() {
                let _ = app2.emit_to("main", "screenshot-countdown", remain);
                tokio::time::sleep(Duration::from_secs(1)).await;
                if CANCEL_COUNTDOWN.load(std::sync::atomic::Ordering::Acquire) {
                    tracing::info!("延时截图已取消");
                    let _ = app2.emit_to("main", "screenshot-countdown", 0);
                    // 撤标记后再退（否则下次触发会被误判为「取消」而非正常流程）
                    COUNTDOWN_ACTIVE.store(false, std::sync::atomic::Ordering::Release);
                    return;
                }
            }
            // 倒计时结束：清零事件让前端撤掉倒计时 UI（0 = 结束信号）；
            // 先撤「倒计时进行中」标记再走抓屏（此后重入恢复「忽略」语义）
            let _ = app2.emit_to("main", "screenshot-countdown", 0);
            COUNTDOWN_ACTIVE.store(false, std::sync::atomic::Ordering::Release);
        }

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

/// 延时截图取消标志（v0.10.0）。只在倒计时循环里消费，抓屏开始后无效。
static CANCEL_COUNTDOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 「倒计时正在进行」标记（审计新增）。区分重入该取消还是该忽略：
/// - true  → 倒计时中，重入触发 = 取消（快捷键全局可达，见函数头）；
/// - false → 抓屏/框选进行中或无会话，重入维持旧行为（忽略）。
/// 与 CANCEL_COUNTDOWN 分开：取消标志每轮倒计时先清零，若复用它判断会
/// 把「上一轮已取消」误判为「本轮倒计时中」。
static COUNTDOWN_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 取消进行中的延时截图倒计时（v0.10.0）。没有倒计时时是 no-op。
#[tauri::command]
pub fn screenshot_cancel_countdown() -> bool {
    let had = CANCEL_COUNTDOWN.swap(true, std::sync::atomic::Ordering::AcqRel);
    // 交换前的值只说明「有没有人取消过」，不代表倒计时是否存在；
    // 返回值仅用于日志/调试，前端以 countdown=0 事件为准撤 UI。
    !had
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

    // v0.9.1：macOS「屏幕录制」权限闸门（P0）。
    //
    // 没有这张 TCC 授权时，xcap 底层的 `CGWindowListCreateImage` **不会报错** ——
    // 它会返回一张「只有桌面壁纸」的图（macOS 的隐私软化行为，xcap#123 有完整复现）。
    // 现象就是「截图后应用全消失、只剩桌面」，且用户无从判断原因：此前这段代码
    // 把这幅图当正常结果一路往下走（冻结帧、归档、进剪贴板全都"成功"）。
    //
    // 所以提前拦下：先主动请求一次授权（把本应用登记进系统设置列表 + 弹系统授权框），
    // 仍未拿到就返回可读原因，由前端提示 + 一键跳转「屏幕录制」面板。
    //
    // v0.9.4：失败文案改为结构化原因码 `__PERM__:<reason>`，且权限类失败**不再走**
    // `screenshot-error` 事件 —— 那条链路弹的是「错误 toast」，而权限未授权是**常态
    // 引导**而非错误：前端收到原因码后亮紧凑授权弹窗（轮询 + 引导重启，Task 81）。
    // 非权限失败（线程异常等）仍走原 toast 链路。
    // v0.9.4：macOS「屏幕录制」权限闸门（P0）—— 收口到 `platform::Backend::ensure_ready`。
    // 宿主负责把它挪到阻塞线程：首次授权会弹系统授权框并同步等用户点击，
    // 直接跑在 async 任务里会占住 runtime 线程。非 macOS 平台恒 Ok（无 TCC 概念）。
    if let Err(reason_code) = tauri::async_runtime::spawn_blocking(crate::screenshot::platform::Backend::ensure_ready)
        .await
        .unwrap_or_else(|e| Err(format!("__THREAD__:{e}")))
    {
        if let Some(reason) = reason_code.strip_prefix("__PERM__:") {
            // 权限未授权：发结构化事件给主窗，紧凑授权弹窗据此渲染（Task 81）
            tracing::info!(reason, "截图被权限闸门拦下，已通知前端亮授权引导弹窗");
            let _ = app.emit_to("main", "screenshot-permission-blocked", reason);
        } else {
            // 线程异常等真错误：仍走错误 toast
            return Err(reason_code
                .strip_prefix("__THREAD__:")
                .map(|e| format!("屏幕录制权限检查线程异常: {e}"))
                .unwrap_or(reason_code));
        }
        return Ok(()); // 权限引导已交给前端，本次截图流程到此为止（不算错误）
    }

    // ① 先隐藏自家置顶窗（pet/float），等合成器稳定后再抓屏
    hide_self(app, &state);
    tokio::time::sleep(Duration::from_millis(160)).await;

    // ② 抓**光标所在显示器**整屏（v1.2.0 起；单屏时即主屏，行为零变化）。
    //    统一为物理像素口径（v1.1.1 契约）。
    //
    // v0.9.4（Task 80）：macOS 优先走 SCK 引擎（ScreenCaptureKit，macOS 14+），
    // 失败或低版本回落 xcap；Windows / Linux 直走 xcap。抓屏逻辑收口到
    // `platform::Backend::capture`（在阻塞线程跑，SCK 内部同步等回调）。
    //
    // v1.1.1：契约收紧 —— 返回的 `mx/my/mw/mh` **一律是物理像素**。
    // 旧契约下 macOS 的 `mw/mh` 来自 `xcap::Monitor::width()`，那是
    // `CGDisplayBounds().size.width` = **逻辑点**（Retina 下 = 物理/2），
    // 与同一元组里代表物理像素的 `img` 构成混口径 → 遮罩窗按点数布局、
    // 冻结帧是物理像素，两者对不上（`xcap-0.9.8/src/macos/impl_monitor.rs:159`
    // vs `windows/impl_monitor.rs:236`，同一行代码两平台语义不同）。
    // 现统一改用 `img` 自身尺寸（`img` 就是这块屏的完整抓屏，天然自洽、零换算误差）。
    let captured = tauri::async_runtime::spawn_blocking(crate::screenshot::platform::Backend::capture)
        .await
        .map_err(|e| format!("截图线程异常: {e}"))??;
    // `capture_scale` = **抓屏侧**缩放因子（来自 `platform::Backend::capture`，
    // macOS 上是 SCK 的 `point_pixel_scale`、其它平台是 xcap 的 `scale_factor`）。
    // ⚠️ v1.1.1 之前这里叫 `scale`，与下方 `window_scale`（显示器侧）同名遮蔽，
    //    导致 payload 里 `width/height` 用抓屏 scale、`scale_factor` 用窗口 scale，
    //    读代码时极易误判为自洽。单显示器下两者相等故无可见错误，但多屏
    //    / 抓屏回落等场景会发散 —— 现在靠改名消除歧义。
    let (img, mx, my, mw, mh, capture_scale) = captured;

    tracing::info!(w = img.width(), h = img.height(), x = mx, y = my, "整屏已抓取");
    let phys_w = img.width();
    let phys_h = img.height();
    // 用**抓屏侧** scale 把物理像素换成 CSS 逻辑像素：这才是前端
    // `<img>` / canvas 所处的坐标系（冻结帧的物理像素 ÷ 冻结帧自带的缩放比）。
    // ⚠️ 必须与下面 payload 的 `scale_factor` 同源，否则 payload 三字段互相矛盾
    //   （历史上正是这个不一致引发「按点数布局、按像素裁剪」的错位）。
    let logical_w = phys_w as f64 / capture_scale;
    let logical_h = phys_h as f64 / capture_scale;
    debug_assert_eq!(
        phys_w as f64,
        (logical_w * capture_scale).round(),
        "capture-ready payload 不自洽：physical_width 必须等于 round(width × scale_factor)。\
         若此断言触发，说明 phys_w 与 scale_factor 取自不同口径的 scale"
    );
    debug_assert_eq!(
        phys_h as f64,
        (logical_h * capture_scale).round(),
        "capture-ready payload 不自洽：physical_height 必须等于 round(height × scale_factor)"
    );
    // ⚠️ 这两条断言**只在真机跑截图时**才可能触发：`begin_capture_inner` 需要真实
    // `AppHandle` + 显示器，现有单测不覆盖它（已实测：故意破坏断言条件后
    // `cargo test` 仍全绿，因为没有任何测试执行到这行）。它们的价值是
    // 「开发/真机调试构建下把口径发散当场炸出来」，而非 CI 门禁 ——
    // 别指望 `cargo test` 能验证它们。

    *state.frame.lock().unwrap_or_else(|e| e.into_inner()) = Some(CachedFrame { img });

    // ③ 亮遮罩窗，覆盖整个显示器
    //
    // ⚠️ macOS Retina 坐标陷阱（v0.9.8 修复「截图变成画中画」）：
    // tao 在 macOS 上对 `set_outer_position(Physical)` / `set_inner_size(Physical)`
    // 的处理是 **`to_logical(window.scale_factor())`** —— 即先除以「窗口当前」的
    // scale_factor 再交给 AppKit。而窗口此时是新建的（还停在 800×600 默认尺寸、
    // 未上屏），它的 scale_factor 不一定等于目标显示器的真实缩放（新建瞬间读到 1.0
    // 或旧屏值都会发生）。物理值被错除后又×2 还原 → 窗口只有屏幕一半大，
    // 且缩在左上角 —— 用户看到的是「全屏画中画，框选只能在画中画上框」。
    //
    // 解法：取**目标显示器的** scale_factor，把物理几何转成逻辑值，
    // 用 `LogicalPosition` / `LogicalSize` 显式传入，绕开 tao 的隐式换算。
    // 逻辑值在任何 scale_factor 下语义唯一（AppKit 再按显示器真实缩放还原），
    // Windows/Linux 上 `to_logical(s)` 用同显示器 scale 除回再乘，逐值等价，不受影响。
    let win = ensure_capture_window(app)?;
    // `window_scale` = **目标显示器**缩放因子，只用于把物理几何转成「逻辑」值
    // 喂给 `LogicalPosition` / `LogicalSize`（绕开 tao 的隐式换算，见上）。
    //
    // ⚠️ v1.2.0 起**直接用 `capture_scale`**：抓屏侧 scale（Windows per-monitor 比 /
    //   macOS 点→像素比 / Linux 像素/逻辑比）就是「目标屏」的 scale。旧实现用
    //   `win.current_monitor()` 取窗口**当前所在屏**的 scale，但此时窗口尚未 move
    //   到目标屏，它返回的是默认创建位置所在屏（通常主屏）——多屏且各屏 DPI 不同时，
    //   抓副屏会拿主屏 scale 去算副屏几何，遮罩窗尺寸/位置全错。既然现在抓的就是
    //   光标所在屏，`capture_scale` 与目标屏 scale 恒等，二者合一不再有歧义。
    let window_scale = capture_scale;
    tracing::info!(
        window_scale,
        capture_scale,
        phys = format!("{}x{}", mw, mh),
        "遮罩窗几何：目标显示器 scale_factor"
    );
    win.set_position(tauri::LogicalPosition::new(
        mx as f64 / window_scale,
        my as f64 / window_scale,
    ))
    .map_err(|e| format!("定位遮罩窗失败: {e}"))?;
    win.set_size(tauri::LogicalSize::new(
        mw as f64 / window_scale,
        mh as f64 / window_scale,
    ))
    .map_err(|e| format!("调整遮罩窗尺寸失败: {e}"))?;

    // ③' 提升窗口层级（v1.2.2，macOS 专有修复，非 macOS 为空实现）
    //
    // ⚠️ 必须在 `show()` **之前**：tao 的 `always_on_top` 在 macOS 上只映射到
    // `NSFloatingWindowLevel`(3)，低于菜单栏 `NSMainMenuWindowLevel`(24) →
    // AppKit 把窗口 frame 约束进 `visibleFrame`（扣掉菜单栏那条），造成
    //   ① 顶部菜单栏那一行盖不到，"全屏"也选不进去；
    //   ② 窗口高度被砍 ~25pt，而冻结帧是整屏物理像素、`.shot` 用 100% 拉伸 →
    //      画面纵向压扁，视觉上像「全屏变画中画」。
    // 提到 `NSScreenSaverWindowLevel`(1000) 后可覆盖菜单栏 / Dock / 全屏 App。
    // 失败只记 warn 不阻断：截图本身仍可用，只是顶部那行盖不到。
    if let Err(e) = capture_window::raise_capture_window(app, CAPTURE_WINDOW_LABEL) {
        tracing::warn!(error = %e, "提升遮罩窗层级失败，顶部菜单栏可能盖不到");
    }

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
            scale_factor: capture_scale,
            default_radius: cfg.corner_radius,
        },
    );
    Ok(())
}

/// `capture-ready` 事件的载荷（Rust → 遮罩窗）。
///
/// ⚠️ v1.1.1 起**四个几何字段全部由同一个 `capture_scale` 派生**（自洽）：
/// `width/height` 与 `scale_factor` 同源、`physical_*` 是冻结帧真实像素，
/// 三者满足 `physical == round(logical × scale_factor)`（`begin_capture_inner`
/// 里有 `debug_assert_eq!` 守着）。改动前 `width/height` 用抓屏 scale、
/// `scale_factor` 用显示器 scale，两者可能不等 —— 单显示器下恰好相等所以从未暴露。
///
/// 前端当前**只读** `physical_width` / `physical_height`（canvas 尺寸与
/// CSS→物理换算系数 `k = physical_width / window.innerWidth` 都由它们派生），
/// `width`/`height`/`scale_factor` 属排障信息。保留它们是为了让「后端算了什么」
/// 可被日志与真机排障直接观测，而不是让前端去反推。
#[derive(Clone, serde::Serialize)]
pub struct CaptureReadyPayload {
    /// 冻结帧的 CSS 逻辑尺寸 = `physical_width ÷ scale_factor`（前端据此铺满）。
    pub width: f64,
    pub height: f64,
    /// 整屏帧的物理像素尺寸（冻结帧真实像素，非 CSS 尺寸 × scale_factor）。
    pub physical_width: u32,
    pub physical_height: u32,
    /// **抓屏侧**缩放因子（`platform::Backend::capture` 回传；macOS 上是 SCK 的
    /// `point_pixel_scale`）。与 `width/height` 同源，⚠️ **不是**遮罩窗所在显示器的
    /// `scale_factor`（那个值只用于窗口定位/尺寸的逻辑化换算，不进 payload）。
    pub scale_factor: f64,
    pub default_radius: u32,
}

/// xcap 抓**光标所在**显示器整屏（v0.9.4 从 `begin_capture_inner` 提取；
/// v1.2.0 起由「主屏」改为「光标所在屏」）。
///
/// 返回 `(整屏图, 显示器原点x, 原点y, 宽, 高, 缩放因子)`。
/// Windows / Linux 的唯一路径；macOS 上作为 SCK 的回落（macOS < 14 或 SCK 失败）。
///
/// ## 多屏选屏（v1.2.0）
/// 用 `cursor::cursor_native()` 拿全局光标（**平台原生坐标**，与 `Monitor` 几何同空间），
/// 交给 `xcap::Monitor::from_point(x,y)` 直接命中「光标停靠的显示器」。三平台的
/// `from_point` 各自用原生坐标匹配（Windows `MonitorFromPoint` 物理像素 / macOS
/// `CGGetDisplaysWithPoint` 点 / Linux 遍历 RandR 输出 × scale），无需本函数换算。
/// 光标取不到 / 点不到任何屏时回落主屏（`is_primary`），主屏也取不到则取第 0 块。
/// 单屏场景 `from_point` 恒命中唯一屏 → 行为与旧「直接挑主屏」完全一致。
///
/// ⚠️ v1.1.1：**宽高原点一律取 `img` 自身尺寸，物理像素口径**。
///
/// 修改原因：`xcap::Monitor::width()/height()` 的语义**三平台不一致**——
///   - Windows：`dmPelsWidth` = **物理像素**（`windows/impl_monitor.rs:236`）
///   - macOS：`CGDisplayBounds().size.width` = **逻辑点**，Retina 下 = 物理/2
///     （`macos/impl_monitor.rs:159`；注意其 `scale_factor` = `pixel_width/width`
///     恰好反证了 width 是点）
///   - Linux：`X11Monitor` 的 mm→点后**再除以 scale_factor** = **逻辑像素**
///     （`linux/impl_monitor.rs:301`）
/// 旧代码把 `m.width()` 与物理像素的 `img` 塞进同一个元组，在 macOS 上构成
/// 混口径几何（遮罩窗按点数布局、冻结帧是物理像素）→ 窗口与冻结帧对不上。
///
/// 注意事项：`img` 是 `m.capture_image()` 的返回，就是**这块显示器**的完整抓屏，
/// 尺寸与该屏天然自洽 —— 用它做宽高，三平台都正确且无需任何换算。
/// ⚠️ 旧约定失效：`mw`/`mh` **不再**来自 `xcap::Monitor`，若未来有人想「改用
///   Monitor 的宽高以支持非整屏抓屏」，必须先补上三平台的口径换算。
///
/// ## 原点 `mx/my` 同样做了口径归一（2026-10-17 补充）
///
/// `x()/y()` 的单位与 `width()/height()` **是同一个坑**（都源自 `CGDisplayBounds` 等
/// 平台 API 对「点」与「像素」的混用）：
///   - Windows：`dmPosition.x/y` = **物理像素**（`windows/impl_monitor.rs:225-233`）
///   - macOS：`CGDisplayBounds().origin.x/y` = **逻辑点**（`macos/impl_monitor.rs:146-158`）
///   - Linux：`monitor.x() / scale_factor` = **逻辑像素**（`linux/impl_monitor.rs:287-299`）
///
/// 故 macOS / Linux 侧乘 `scale` 归一到物理像素，Windows 侧原样（已是物理像素）。
/// 单显示器恒为 `(0,0)`，乘 0 仍是 0 → **单屏行为零变化**，只在多屏下纠正错位。
///
/// 为什么 tao 要的是「物理像素」：tao 的 `util::window_position`（tao-0.37.1
/// `src/platform_impl/macos/util/mod.rs:94-99`）做 Y 轴翻转时用的是
/// `CGDisplay::main().pixels_high()` —— **物理像素**。它用物理像素量纲去减我们传入的
/// y，传入的也必须是物理像素这一量纲，反推才成立。传入逻辑点会在 Retina 主屏上
/// 让翻转量错一倍（`pixels_high` 是逻辑高的 2倍）。
fn capture_with_xcap() -> Result<(RgbaImage, i32, i32, i32, i32, f64), String> {
    let monitors = xcap::Monitor::all().map_err(|e| format!("枚举显示器失败: {e}"))?;
    if monitors.is_empty() {
        return Err("未找到可用显示器".into());
    }
    // v1.2.0：多屏截图「鼠标在哪截哪屏」——优先抓光标停靠的显示器，
    // 取不到光标 / 点不到任何屏时回落主屏（单屏行为零变化）。
    let m = match cursor::cursor_native() {
        Some((cx, cy)) => xcap::Monitor::from_point(cx as i32, cy as i32)
            .ok()
            .or_else(|| {
                monitors
                    .iter()
                    .find(|m| m.is_primary().unwrap_or(false))
                    .cloned()
            })
            .unwrap_or_else(|| monitors[0].clone()),
        None => monitors
            .iter()
            .find(|m| m.is_primary().unwrap_or(false))
            .cloned()
            .unwrap_or_else(|| monitors[0].clone()),
    };
    let img = m.capture_image().map_err(|e| format!("屏幕捕获失败: {e}"))?;
    // scale 取目标屏自身的 scale_factor：Windows 是 per-monitor 物理/逻辑比、
    // macOS 是 pixel_width/width、Linux 是像素/逻辑像素比，与遮罩窗定位的
    // `window_scale`（改用 capture_scale 后）同源，保证逻辑尺寸换算一致。
    let scale = m.scale_factor().unwrap_or(1.0) as f64;
    // 宽高一律用 `img` 的真实像素尺寸（见函数头「三平台口径不一致」段）
    let (mw, mh) = (img.width(), img.height());
    let (mx_raw, my_raw) = (m.x().unwrap_or(0), m.y().unwrap_or(0));
    // 原点归一到物理像素：macOS / Linux 的 `x()/y()` 是逻辑量，需乘 scale；
    // Windows 的 `dmPosition` 已是物理像素，原样透传。
    // ⚠️ `cfg` 必须精确到 windows —— 用 `not(windows)` 会把未来任何新平台
    // 默认打上「需换算」，而新平台的语义无从得知，宁可编译期报错也不猜。
    #[cfg(target_os = "windows")]
    let (mx, my) = (mx_raw, my_raw);
    #[cfg(not(target_os = "windows"))]
    let (mx, my) = (
        (f64::from(mx_raw) * scale).round() as i32,
        (f64::from(my_raw) * scale).round() as i32,
    );
    tracing::debug!(
        mx,
        my,
        mw,
        mh,
        scale,
        "xcap 抓屏几何（已归一到物理像素）"
    );
    Ok((img, mx, my, mw as i32, mh as i32, scale))
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

/// 延时截图（v0.10.0，借鉴 Snapzy）：倒计时 `seconds` 秒后进入正常截图流程。
///
/// 倒计时不抢焦点——用户可先把目标界面/菜单摆好。期间每秒向 main 窗发
/// `screenshot-countdown`（剩余秒数，0 = 结束/取消），前端负责渲染与取消按钮。
/// seconds 会被钳制到 1..=30（防误传 0 或超大值卡住会话闸门）。
#[tauri::command]
pub fn screenshot_trigger_delayed(
    app: AppHandle,
    state: tauri::State<'_, ScreenshotState>,
    seconds: u32,
) -> bool {
    if !state.config.lock().unwrap_or_else(|e| e.into_inner()).enabled {
        return false;
    }
    let seconds = seconds.clamp(1, 30);
    begin_capture_delayed(&app, seconds);
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
    offset: u32,
) -> Result<Vec<crate::db::ScreenshotOut>, String> {
    state
        .db
        .list_screenshots(limit.clamp(1, 500), offset)
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

/// 批量删除历史截图（图片移入回收站 + 删除索引）
///
/// 前端「截图历史」多选后一次性删除用。逐条复用 `delete_screenshot`（返回文件名）+ `move_to_trash`，
/// 返回**实际删除的条数**（传入的 id 里可能含已不存在的，按 0 跳过，不报错）。
#[tauri::command]
pub fn screenshot_delete_many(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    ids: Vec<i64>,
) -> Result<u32, String> {
    let dir = screenshots_dir(&app)?;
    let requested = ids.len();
    let mut deleted = 0u32;
    for id in ids {
        if let Some(name) = state
            .db
            .delete_screenshot(id)
            .map_err(|e| e.to_string())?
        {
            let path = dir.join(&name);
            if path.exists() {
                move_to_trash(&path);
            }
            deleted += 1;
        }
    }
    crate::logging::audit(
        "screenshots_deleted",
        &format!("requested={requested} deleted={deleted}"),
    );
    Ok(deleted)
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
    crate::logging::audit("ocr_run", &format!("engine={kind:?} region={width}x{height}"));
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

// ===== v0.9.4：macOS 屏幕录制授权 —— 权限 IPC 四件套 =====
//
// 为什么收进 screenshot 模块：v0.9.3 的权限命令散在 commands.rs（open_privacy_settings /
// restart_app / reset_screen_capture_permission），且错误文案是一大段诊断文字。
// v0.9.4 前端改用「紧凑授权弹窗 + 轮询」，需要的是**结构化**的权限状态与原子动作，
// 四件套全部挂在 screenshot:: 命名空间下，前端 api/screenshot.ts 一处封装。
//
// ⚠️ 非 macOS 平台的兜底语义：Windows/Linux 抓屏不需要 TCC 授权，
// status 恒 `permitted=true`（弹窗永不出现）；request 返回 true；open/reset 是 no-op。

/// macOS 权限状态快照的跨平台兜底（非 macOS 平台恒「已授权」）
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScreenshotPermissionStatus {
    pub preflight: bool,
    pub titles: String,
    pub permitted: bool,
    pub reason: String,
}

/// ① 查询屏幕录制授权状态（前端紧凑弹窗 2s 轮询用）
#[tauri::command]
pub fn screenshot_permission_status() -> ScreenshotPermissionStatus {
    crate::screenshot::platform::Backend::status_snapshot()
}

/// ② 主动请求屏幕录制授权（弹系统框；**阻塞** → spawn_blocking 包裹）
#[tauri::command]
pub async fn screenshot_request_permission() -> Result<ScreenshotPermissionStatus, String> {
    let s = tauri::async_runtime::spawn_blocking(crate::screenshot::platform::Backend::request_permission)
        .await
        .map_err(|e| format!("权限请求线程异常: {e}"))?;
    Ok(s)
}

/// ③ 打开「系统设置 → 隐私与安全性 → 屏幕录制」面板
#[tauri::command]
pub fn screenshot_open_permission_settings() {
    crate::screenshot::platform::Backend::open_settings();
}

/// ④ 重启应用（TCC 新授权只对新进程生效 —— 轮询发现授权翻转后引导用户点这个）
#[tauri::command]
pub fn screenshot_restart_app(app: AppHandle) {
    tracing::info!("用户从截图授权弹窗触发应用重启（TCC 新授权需要新进程才生效）");
    app.restart();
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
    crate::screenshot::platform::Backend::reveal(path);
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
