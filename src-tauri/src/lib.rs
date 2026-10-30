//! ScreenTime Pro —— Rust 后端入口
//!
//! 这里完成四件核心事情：
//! 1. `setup`：创建数据库、注入全局状态（State）、加载分类规则
//! 2. 系统托盘：应用关闭时最小化到托盘/菜单栏，而不是直接退出
//! 3. 菜单栏纯后台模式（macOS）：设为 Accessory 激活策略，去掉 Dock 图标
//! 4. 启动即自动追踪 + 开机自启；命令注册
//!
//! 修改历史（本文件近期变更）：
//!   - 2026-10-17 @v1.1.1: 废弃 - 移除滑动截长图：`app.manage(screenshot::long::LongShotState)`
//!     与 `generate_handler!` 内 4 个 `screenshot_long_*` 命令（截图 IPC 26→22）。
//!     原因见 `screenshot/mod.rs` 修改历史段。
//!   - 2026-10-21 @v1.2.2: 新增 - 悬浮窗液态玻璃。
//!     ① `mod float_glass;`（新模块，材质层 + 能力探测）；
//!     ② 启动恢复段：`cfg.float_glass` 为真时**在 show 之前**应用材质并把窗口高度
//!        拉到 44px。这里刻意不走 `apply_via_app`（它内部 run_on_main_thread 是异步
//!        派发，无法保证早于 show），而 setup 本身跑在主线程，直接同步调用即可；
//!     ③ `generate_handler!` 注册 `float_glass::float_glass_support`（IPC 82→83）。
//!        （CONVENTIONS 里长期写着 84，那是 v1.1.1 的旧数；v1.2.0 删掉
//!        `create_float_window`/`hide_float_window` 两个死命令后实际已是 82，
//!        本次一并按实测校准。）
//!   - 2026-10-23 @v1.2.6: 加固 - `default_window_icon().unwrap()` 降级为 `match`。
//!     图标资源异常时（打包缺失 / 解码失败）旧代码直接 panic，表现为「双击 exe 没反应」；
//!     现在记一条 warn 并用空图标兜底启动，界面其余部分照常可用。
//!   - 2026-10-24 @v1.2.8: 记录 - 本文件**无代码改动**；仅在此补正 IPC 计数。
//!     `float_glass_support` 的返回体新增 `native_blur` 字段（字段级变更，非新命令）。
//!     ⚠️ 计数校准（2026-10-24 实测 `generate_handler!` 块内注册项）：**85**。
//!     上方 v1.2.2 条目写的「82→83」在**当时**是对的，但 v1.2.4 新增
//!     `auto_check_update` / `dismiss_update` 两条命令后未同步 → 实际 83→85。
//!     逐版本实测值：v1.1.0=96 → v1.1.1=85 → v1.2.0~v1.2.2=83 → v1.2.7 起=85。
//!     后续再加命令请**实地数一遍** `generate_handler!`，不要凭文档递推。
//!   - 2026-10-26 @v1.2.10: 修复 - macOS 菜单栏图标改用 template 图。
//!     原来托盘图标取 `default_window_icon()`（= `icons/32x32.png`），那是**橙色 App 图标**
//!     （#FF7E27、alpha=255 占 90.6%，是个不透明色块），不是菜单栏素材，且 `icon_as_template`
//!     全仓零命中。浅色菜单栏下实测对比度 1.91:1（人眼即「脏灰」），深色栏 7.67:1
//!     → 用户说的「状态栏在浅色下偏灰」有一半来自这里。
//!     现在 macOS 分支嵌入 `icons/trayTemplate.png` 家族的 @2x 图 + `.icon_as_template(true)`，
//!     系统按菜单栏明暗自动反色（浅色栏纯黑 17.6:1、深色栏自动翻纯白）。
//!     ⚠️ 非 macOS 分支**仍然**用原 App 图标 —— template 是 macOS 专属语义，
//!     把黑白圆环给 Windows 托盘属无谓回归。
//!   - 2026-10-27 @v1.2.11: 修复 - 权限中心「重置并重启」只能清屏幕录制。
//!     新增 IPC `reset_permission(pane)`（`Settings.vue` 用它），旧的
//!     `reset_screen_capture_permission` 保留为薄别名（内部转发到
//!     `reset_permission(Some("screen_capture"))`），因此 `generate_handler!`
//!     里**是两条**。
//!     ⚠️ 计数（本次实测，用 `generate_handler!` 注册项数与 `#[tauri::command]`
//!     函数数双向比对，两者相等才算数）：**84 → 85**（净 +1）。
//!     注：上方 v1.2.8 条目写的「85」实为把闭合行 `])` 也数了进去，当时真实值是 **84**。
//!     本次已按实测更正，不要再引用旧值。后续改命令请同样实地数，不要凭文档递推。
//!

mod categorizer;
mod classifier;
mod commands;
// v1.2.11（2026-10-27）：应用内自动升级（Windows / AppImage 启用，macOS / deb / rpm 关闭）。
// 与 commands.rs 里的「版本告知」是两套并存的东西，别把旧的删了 —— 详见模块头注释。
mod app_update;
mod db;
mod error;
// v1.2.2（2026-10-21）：悬浮窗液态玻璃材质层（仅服务 float 窗口）
mod float_glass;
mod float_window;
mod logging;
mod pet;
mod system_load;
mod tracker;
// v0.8.0（2026-09-16）：屏幕截图（快捷键 → 遮罩框选 → 剪贴板/归档）
mod screenshot;
// v0.7.7：隐藏控制台窗口地创建子进程（修复 Windows 首次启动 reg 命令框闪烁）
mod proc;
// v1.2.12（2026-10-29）：「恢复位置还在不在可见区」的公共判定，float 与 pet 共用（整改 1.3）
mod window_bounds;

use std::sync::{Arc, Mutex};

use classifier::Rule;
use db::AppDb;
// v0.7.6（2026-09-10）：托盘菜单新增 CheckMenuItem 快捷开关 + 分隔线
// v0.7.9（2026-09-12）：两个平台的托盘点击处理都要按「按键 + 按下/抬起」过滤，
//   故 MouseButton / MouseButtonState 不再限定非 mac（原 cfg 门控会让 macOS 侧少这两个符号）
use tauri::menu::{CheckMenuItemBuilder, Menu, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tracker::{create_tracker, PlatformTracker};

use crate::commands::ActiveSession;

pub struct AppState {
    pub db: AppDb,
    pub tracker: Arc<dyn PlatformTracker>,
    pub device_id: String,
    pub device_name: Mutex<String>,
    pub tracking: Mutex<bool>,
    pub idle_threshold: Mutex<u64>,
    pub current: Mutex<Option<ActiveSession>>,
    pub rules: Mutex<Vec<Rule>>,
    pub category_cache: categorizer::CategoryCache,
    // ===== v0.7.5 → v0.7.6：托盘系统指标监测 =====
    // v0.7.5：仅 macOS，metrics_enabled + Option<MetricsSampler>；v0.7.6 拆 cfg，跨平台始终存在
    pub metrics_sampler: Arc<system_load::MetricsSampler>,
    pub status_bar_config: Mutex<commands::StatusBarConfig>,
}

/// v0.7.6（2026-09-10）：托盘快捷开关菜单项句柄（setup 构建 tray 菜单后 manage）。
/// 用途：设置页 `set_status_bar_config` 保存成功后调用 `sync()` 同步菜单勾选态，
/// 防止「设置页改了、托盘菜单还挂旧勾」的显示不一致。
/// CheckMenuItem 句柄是廉价 clone（内部同一 Arc），与菜单事件闭包里持有的 clone 互不冲突。
/// 托盘菜单唯一的快捷开关句柄（**启用状态栏**总开关）。
/// v0.7.6 初版曾含 状态栏/CPU/网速/内存 四项勾选；2026-09-10 按 Ryan 反馈精简为
/// 「悬浮指标条」单项；v0.7.11（2026-09-14）再按 Ryan 反馈改为「启用状态栏」——
/// 总开关才是用户心智里唯一的开关，浮窗开关退化为设置页内的细项。
/// v0.7.7：旧的「内存子项仅 macOS 菜单存在」注释作废——内存/磁盘已三平台齐备，
/// 且托盘菜单本身不再承载任何指标勾选。
pub struct TrayStatusBarToggle {
    pub status_bar: tauri::menu::CheckMenuItem<tauri::Wry>,
}

impl TrayStatusBarToggle {
    /// 设置页改 `enabled` 后同步托盘菜单勾选态（反向：托盘勾选 → emit 事件刷设置页）
    pub fn sync(&self, cfg: &commands::StatusBarConfig) {
        let _ = self.status_bar.set_checked(cfg.enabled);
    }
}

fn gen_device_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let raw = format!("{}{}", nanos, pid);
    let mut h: u64 = 0xcbf29ce484222325;
    for b in raw.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:012x}", h)
}

fn hardware_device_id() -> String {
    #[cfg(target_os = "macos")]
    if let Some(id) = mac_hardware_uuid() {
        return id;
    }
    #[cfg(target_os = "windows")]
    if let Some(id) = windows_machine_guid() {
        return id;
    }
    #[cfg(target_os = "linux")]
    if let Some(id) = linux_machine_id() {
        return id;
    }
    gen_device_id()
}

/// macOS 菜单栏 template 图标（v1.2.10 新增）。
///
/// ## 为什么不用 `tauri::image::Image::from_bytes`
///
/// `from_bytes` / `from_path` 都被 `#[cfg(any(feature = "image-ico", feature = "image-png"))]`
/// 门控（tauri-2.11.5/src/image/mod.rs:74），而本项目的 tauri 特性里**没有**开
/// `image-png`（`cargo tree -p tauri -f "{p} {f}" --depth 0` 实测：
/// `wry,compression,common-controls-v6,custom-protocol,dbus,default,dynamic-acl,
/// macos-private-api,tray-icon,...`，无 image-png）。直接调用会 E0599 找不到方法。
///
/// 而 `image` crate 本身已在依赖里（`features = ["png"]`，OCR/截图链路在用），
/// 所以这里自己解码 PNG → RGBA → `Image::new_owned`，零新增依赖、零 feature 变更。
///
/// ## 为什么返回 Option 而不是直接 Result
///
/// 调用点已经有「图标拿不到就用 App 图标兜底」的分支（v1.2.6 的教训：图标问题
/// 绝不能让应用起不来）。返回 Option 让那个兜底天然复用，不必在调用点再 match 两层。
#[cfg(target_os = "macos")]
fn macos_tray_template_icon() -> Option<tauri::image::Image<'static>> {
    // 内嵌 @2x（44×44）。理由见调用点注释：系统会把图标高度缩到 18pt，
    // 44px 下采样比 22px 上采样锐利；`include_bytes!` 也无法表达 1x/2x 多表示。
    const PNG: &[u8] = include_bytes!("../icons/trayTemplate@2x.png");
    let img = image::load_from_memory(PNG).ok()?;
    let rgba = img.into_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    Some(tauri::image::Image::new_owned(rgba.into_raw(), w, h))
}

#[cfg(target_os = "macos")]
fn mac_hardware_uuid() -> Option<String> {
    let out = crate::proc::hidden("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    for line in s.lines() {
        if line.contains("IOPlatformUUID") {
            if let Some(start) = line.find('"') {
                let rest = &line[start + 1..];
                if let Some(end) = rest.find('"') {
                    let uuid = rest[..end].trim();
                    if !uuid.is_empty() {
                        return Some(format!("hw-{}", uuid.to_lowercase()));
                    }
                }
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn windows_machine_guid() -> Option<String> {
    // v0.7.7：必须走 proc::hidden——GUI 进程直接 spawn reg.exe 会闪一个控制台黑框
    let out = crate::proc::hidden("reg")
        .args([
            "query",
            "HKLM\\SOFTWARE\\Microsoft\\Cryptography",
            "/v",
            "MachineGuid",
        ])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    for line in s.lines() {
        if line.contains("MachineGuid") {
            if let Some(start) = line.rfind("REG_SZ") {
                let guid = line[start + "REG_SZ".len()..].trim();
                if !guid.is_empty() {
                    return Some(format!("hw-{}", guid.to_lowercase()));
                }
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn linux_machine_id() -> Option<String> {
    let content = std::fs::read_to_string("/etc/machine-id").ok()?;
    let id = content.trim();
    if !id.is_empty() {
        return Some(format!("hw-{}", id));
    }
    None
}

pub fn run() {
    let tracker = create_tracker();
    tauri::Builder::default()
        // v1.0.1：单实例锁 —— 修复 macOS「关机自启动已拉起实例后，用户再手动点
        // 图标又起一个 → 两个进程同时记录、时长翻倍」。⚠️ 官方要求本插件必须
        // **第一个**注册（在其他插件/setup 之前），它内部在初始化时做进程互斥，
        // 重复启动的进程会直接退出，并把已运行实例的参数转发过去。
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 第二实例启动时走到这里的是**第一实例**：把主窗从托盘/菜单栏拉回前台。
            // mac 上是 Accessory 激活策略（无 Dock 图标），show + set_focus 足够。
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.unminimize();
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_log::Builder::new()
                .format(|out, message, record| {
                    // 前端日志（Vue → plugin-log）每行也带版本戳，与后端主日志一致，
                    // 便于只凭一行日志判断「哪个版本出的问题」。
                    out.finish(format_args!(
                        "{} [{}] {}",
                        crate::logging::app_version(),
                        record.level(),
                        message
                    ))
                })
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        // v1.2.11：应用内升级。pubkey / endpoints 走 tauri.conf.json 的 plugins.updater，
        // 这里只做默认初始化 —— 不覆盖配置，配置文件是唯一真源。
        //
        // ⚠️ 即使当前平台不支持升级（macOS / deb / rpm），也必须注册：
        // 命令层 `install_supported()` 会先拦掉，注册本身不做任何网络或文件动作。
        // 反之若不注册，`app.updater()` 会因缺少 state 而报错，错误信息与「平台不支持」
        // 混在一起，排查时难以区分。
        .plugin(tauri_plugin_updater::Builder::new().build())
        // v0.8.0：全局快捷键（截图触发）。handler 只认「按下」态，
        // 避免 macOS/Windows 一次按键派发 down+up 导致重复截图。
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        crate::screenshot::begin_capture(app);
                    }
                })
                .build(),
        )
        .setup(move |app| {
            let log_dir = app
                .path()
                .app_log_dir()
                .unwrap_or_else(|_| dir_for_log_fallback());
            let app_version = app.package_info().version.to_string();
            let log_guard = logging::init(&log_dir, cfg!(debug_assertions), &app_version)
                .unwrap_or_else(|e| {
                    eprintln!("[main] 日志初始化失败: {}，降级到 stderr", e);
                    None
                });
            tracing::info!(
                version = %app_version,
                debug = cfg!(debug_assertions),
                "ScreenTime Pro 启动"
            );
            // v0.9.2 诊断：把「当前运行的可执行文件路径」写进启动日志。
            //
            // 起因：macOS 上「系统设置里授权开关明明是开的，应用却说没权限，重启也无效」
            // 这类问题，根因几乎都是**授权记在另一个二进制身份上**（TCC 按路径 + 二进制指纹记账）：
            // - 从 DMG / 下载目录直接双击运行 → Gatekeeper 把 App 挪到随机只读路径
            //   （`/private/var/folders/…/AppTranslocation/<uuid>/d/…`），系统不会把授权
            //   保留在这份临时副本上，且**每次启动路径还会变**，怎么重启都没用；
            // - 无代码签名的应用**升级后指纹变化** → 旧授权看着还开着，实际已不匹配。
            //
            // 有这一行日志，下次就不用再猜了。用纯 `std`（不依赖 mac 专属模块），全平台通用。
            {
                let exe = std::env::current_exe()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|e| format!("<无法获取: {e}>"));
                if exe.contains("/AppTranslocation/") {
                    tracing::warn!(
                        exe = %exe,
                        "当前处于 App Translocation（从 DMG / 下载目录直接运行）—— \
                         系统不会把权限授权保留在这份临时副本上，且每次启动路径都会变；\
                         请先把 App 拖进「应用程序」文件夹，再从那里启动"
                    );
                } else {
                    tracing::info!(exe = %exe, "运行位置");
                }
            }
            // v0.7.7（2026-09-10）：panic 兜底钩子。
            // 背景：release 是 GUI 进程（Windows 无控制台 / macOS .app 双击无 stderr 终端），
            // 一旦 panic，用户只看到「闪退」而拿不到任何信息（macOS 反馈的现场即如此）。
            // 这里把 panic 内容写进日志文件，让闪退可被诊断：
            //   macOS   ~/Library/Logs/com.screentime.pro/app.YYYY-MM-DD.log
            //   Windows %LOCALAPPDATA%\com.screentime.pro\logs\app.YYYY-MM-DD.log
            // 必须装在 logging::init 之后（否则 tracing 无 subscriber，日志会被丢弃）。
            {
                let default_hook = std::panic::take_hook();
                std::panic::set_hook(Box::new(move |info| {
                    let location = info
                        .location()
                        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
                        .unwrap_or_else(|| "<unknown>".to_string());
                    tracing::error!(target: "panic", location = %location, "PANIC: {info}");
                    default_hook(info);
                }));
            }
            let dir = app.path().app_data_dir()?;
            let db = AppDb::open(&dir)?;
            let device_id = db
                .get_setting("device_id")
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    let id = hardware_device_id();
                    let _ = db.set_setting("device_id", &id);
                    id
                });
            let _ = db.backfill_device_column(&device_id);
            let _device_name = {
                let raw = db.get_setting("device_name").unwrap_or_default();
                let is_default_id = raw.trim().is_empty() || raw.trim() == device_id;
                if is_default_id {
                    let host = gethostname::gethostname()
                        .into_string()
                        .ok()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| device_id.clone());
                    let _ = db.set_setting("device_name", &host);
                    host
                } else {
                    raw
                }
            };
            if db.get_setting("autostart").is_none() {
                let _ = app.autolaunch().enable();
                let _ = db.set_setting("autostart", "true");
            }
            let rules = db.load_rules().unwrap_or_default();
            let idle_threshold = db
                .get_setting("idle_threshold")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(300);
            let device_name_from_db = db
                .get_setting("device_name")
                .unwrap_or_else(|| device_id.clone());
            let status_bar_config = commands::StatusBarConfig::load(&db);
            // v0.7.6：sampler 跨平台始终存在
            let cpu_monitor_for_sampler = Arc::new(system_load::CpuMonitor::new());
            let metrics_sampler =
                Arc::new(system_load::MetricsSampler::new(cpu_monitor_for_sampler));
            metrics_sampler.warmup();
            metrics_sampler.init();
            let app_state = Arc::new(AppState {
                db,
                tracker,
                device_id: device_id.clone(),
                device_name: Mutex::new(device_name_from_db),
                tracking: Mutex::new(false),
                idle_threshold: Mutex::new(idle_threshold),
                current: Mutex::new(None),
                rules: Mutex::new(rules),
                category_cache: categorizer::CategoryCache::new(),
                metrics_sampler,
                status_bar_config: Mutex::new(status_bar_config),
            });
            app.manage(app_state.clone());
            // ===== v0.8.0（2026-09-16）：截图 + 桌宠命中状态 =====
            // 截图配置从 settings 表读（首启即默认值：快捷键 CmdOrCtrl+Shift+A、确认即复制剪贴板）
            let screenshot_cfg = screenshot::ScreenshotConfig::load(&app_state.db);
            app.manage(screenshot::ScreenshotState::new(screenshot_cfg.clone()));
            app.manage(pet::PetHitState::default());
            // v1.2.12（批次 2）：Rust 侧拖拽循环的会话状态（grab 偏移 / 会话号 / 最后落点）
            app.manage(pet::PetDragState::default());
            // 注册截图全局快捷键。失败不致命：可能被系统/其他软件占用，
            // 用户仍可用托盘菜单「截图」或设置页按钮触发（此处只记日志；
            // 用户在设置页改键时由 screenshot_set_config 把失败原因带回 UI）。
            if let Err(e) = screenshot::shortcut::apply(app.handle(), &screenshot_cfg) {
                tracing::warn!(error = %e, "启动期注册截图快捷键失败");
            }
            // v1.2.10（2026-10-26）：**删除启动 3 秒预请求「屏幕录制」授权**
            // （原 v0.9.4 Task 79 的 `schedule_startup_permission_request`）。
            //
            // 为什么删（用户反馈「每次打开都弹权限框，弹窗太多」）：
            //   ① 该预请求的判据是**单信号** `is_screen_capture_trusted()`
            //      （= CGPreflightScreenCaptureAccess），而该 API 有「进程内一旦
            //      返回 false 就一直 false」的已知行为（本模块与
            //      screenshot/macos.rs 模块头各记了一遍）。于是**已授权用户每次
            //      启动都可能被再弹一次系统授权框** —— 这是「反复弹」的主因。
            //   ② 它无条件在每次 setup 执行，与「新授权只对新进程生效」叠加后，
            //      首启用户会陷入「弹框 → 授权 → 提示重启 → 重启 → 再弹框」循环。
            //   ③ 时长统计**不依赖**这张授权（NSWorkspace.frontmostApplication
            //      无需任何 TCC），只有「窗口标题粒度」和「截图取字」需要。启动时
            //      就为它弹框，属于「无明确用户意图时索要权限」。
            //
            // 现在改为**按需请求**：用户第一次真正截图时，由
            // `screenshot::macos::ensure_ready()`（同样是双信号判据，且已有完整
            // 引导弹窗）发起请求。启动阶段只做**静默查询**，不 request、不 emit。
            //
            // ⚠️ 旧约定失效：`screenshot-permission-changed` 事件不再有发出方
            // （唯一的 emit 点随本函数删除）。前端 App.vue 仍保留了监听以防旧版
            // 后端，但正常路径下不会再触发；启动首屏的权限说明由
            // `tracker.checkPermissions()`（横幅）承担。
            // 桌宠「按身体 alpha 命中」穿透轮询线程（修透明区点击死区）
            pet::hit_mask::spawn_watcher(app.handle().clone());
            app.manage(LogGuardHolder(log_guard.unwrap_or_default()));
            #[cfg(target_os = "macos")]
            {
                use objc2::MainThreadMarker;
                use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
                if let Some(mtm) = MainThreadMarker::new() {
                    let ns_app = NSApplication::sharedApplication(mtm);
                    ns_app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
                }
            }
            commands::begin_tracking(&app_state);
            let backup_handle = app.handle().clone();
            std::thread::Builder::new()
                .name("auto-backup".into())
                .spawn(move || {
                    use std::time::Duration;
                    std::thread::sleep(Duration::from_secs(60));
                    loop {
                        commands::auto_backup_tick(&backup_handle);
                        std::thread::sleep(Duration::from_secs(30 * 60));
                    }
                })
                .ok();
            let monitor = Arc::new(system_load::CpuMonitor::new());
            let app_handle = app.handle().clone();
            std::thread::Builder::new()
                .name("system-load-monitor".into())
                .spawn(move || {
                    use std::time::Duration;
                    const OVERHEAT_PCT: f32 = 0.90;
                    const COOL_PCT: f32 = 0.55;
                    const SUSTAIN_OVERHEAT: u32 = 4;
                    const SUSTAIN_COOL: u32 = 3;
                    let mut hot_streak = 0u32;
                    let mut cool_streak = 0u32;
                    let mut is_overheating = false;
                    let _ = monitor.cpu_usage();
                    loop {
                        std::thread::sleep(Duration::from_secs(5));
                        let usage = match monitor.cpu_usage() {
                            Some(u) => u,
                            None => continue,
                        };
                        if !is_overheating {
                            if usage >= OVERHEAT_PCT {
                                hot_streak += 1;
                                cool_streak = 0;
                                if hot_streak >= SUSTAIN_OVERHEAT {
                                    is_overheating = true;
                                    let _ = app_handle.emit_to("pet", "pet-system-overload", usage);
                                    tracing::warn!(usage, "system overheating → pet 升温");
                                }
                            } else {
                                hot_streak = 0;
                            }
                        } else {
                            if usage < COOL_PCT {
                                cool_streak += 1;
                                hot_streak = 0;
                                if cool_streak >= SUSTAIN_COOL {
                                    is_overheating = false;
                                    let _ = app_handle.emit_to("pet", "pet-system-cool", usage);
                                    tracing::info!(usage, "system cooled → pet 恢复");
                                }
                            } else {
                                cool_streak = 0;
                            }
                        }
                    }
                })
                .ok();
            // ===== v0.7.11（2026-09-14）：托盘右键菜单 =「启用状态栏 ✓ ─ 显示主窗口 / 退出」=====
            // - 历史沿革：v0.7.6 初版曾把 状态栏/CPU/网速/内存 四项勾选放进托盘菜单；
            //   同日精简为单项「悬浮指标条」；2026-09-14 Ryan 反馈——托盘快捷项应当是
            //   「启用状态栏」总开关（用户心智里的唯一开关），且勾选态必须与设置页一致。
            // - 与设置页同源：写 AppState.status_bar_config + SQLite 持久化，
            //   toggle 后 emit status-bar-config-changed → 设置页即时刷新（双向同步）
            // - 菜单文案沿用硬编码中文（原生菜单不做 i18n，与既有约定一致）
            let sb_cfg = *app_state
                .status_bar_config
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let status_item = CheckMenuItemBuilder::with_id("toggle_status_bar", "启用状态栏")
                .checked(sb_cfg.enabled)
                .build(app)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let show_item = MenuItemBuilder::with_id("show", "显示主窗口")
                .enabled(true)
                .build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "退出")
                .enabled(true)
                .build(app)?;
            // ⚠️ with_items 要求元素统一为 &dyn IsMenuItem；CheckMenuItem /
            // PredefinedMenuItem / MenuItem 混排必须显式 Vec<&dyn ...> 注解，
            // 否则数组字面量推断出单一具体类型而编译失败
            let tray_items: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
                vec![&status_item, &sep, &show_item, &quit_item];
            let tray_menu = Menu::with_items(app, &tray_items)?;
            // 勾选态句柄：CheckMenuItem 点击后不自动翻转，菜单事件里手动 set_checked
            let status_toggle = status_item.clone();
            // 设置页 → 托盘菜单方向的同步句柄：manage 后 commands 里 try_state 取用
            // （原始 item move 进 struct，菜单事件闭包用的是上面的 clone，互不影响）
            app.manage(TrayStatusBarToggle {
                status_bar: status_item,
            });
            // v1.2.6：原写法 `.unwrap()` —— 一旦 bundle 图标缺失/损坏（打包配置被改、
            // 平台覆盖配置没带上 icon），整个应用会在启动时 panic，用户看到的是「双击没反应」，
            // 而日志里只有一行 panic，极难定位。这里降级为「无图标托盘」继续运行：
            // 托盘仍可用（Windows 会显示系统默认图标），主功能不受影响。
            //
            // v1.2.10（2026-10-26）：macOS 改取 template 图，不再用 App 图标。
            //
            // 为什么：`default_window_icon()` 是 `icons/32x32.png` —— 橙色 App 图标
            // （#FF7E27，alpha=255 占 90.6%），在菜单栏里是一枚不透明色块，
            // 浅色栏对比度仅 1.91:1（「脏灰」），深色栏 7.67:1（所以深色下看着正常）。
            // template 图是「纯黑 + alpha」的剪影，系统按菜单栏明暗自动着色
            // （浅色栏画黑、深色栏画白、点击自动反色），浅色栏对比度直接到 17.6:1。
            //
            // ⚠️ 素材**不写进** `tauri.conf.json` 的 `bundle.icon`：那里的 `find_icon`
            // 取列表里第一个 `.png` 当 Windows 默认图标，塞进来会污染 exe 图标。
            // 这里用 `include_bytes!` 编译期内嵌，素材只服务托盘。
            //
            // ⚠️ 关于 `trayTemplate@2x.png`：`include_bytes!` 只能嵌单张位图，
            // 无法把两张图作为一个 NSImage 的「1x/2x 表示」交给系统 —— tray-icon 0.24
            // 的 macOS 实现是 `PlatformIcon::to_png()` → `NSImage::initWithData`
            // 单张数据源（见 tray-icon/src/platform_impl/macos/icon.rs:20）。
            // 且系统会把图标按高度缩到固定的 18pt（macos/mod.rs:296 硬编码），
            // 缩放在 Retina 上由 NSImage 从位图重建，故这里嵌 **@2x（44×44）**：
            // 44×44 是 18pt@2x 的最近原生尺寸，下采样比 22px 的 2 倍上采样锐利得多
            // ——这正是报告里「32→36 上采样导致边缘发虚」的同类问题。
            // @2x 文件仍入库，留给将来 tray-icon 支持多表示或改用 iconPath 时直接用。
            #[cfg(target_os = "macos")]
            let icon = match macos_tray_template_icon() {
                Some(ic) => ic,
                None => {
                    // 素材损坏属构建期问题，但绝不能因此让应用起不来（同 v1.2.6 教训）
                    tracing::warn!("托盘 template 图解码失败，回退到 App 图标");
                    app.default_window_icon()
                        .cloned()
                        .unwrap_or_else(|| tauri::image::Image::new_owned(Vec::new(), 0, 0))
                }
            };
            #[cfg(not(target_os = "macos"))]
            let icon = match app.default_window_icon() {
                Some(ic) => ic.clone(),
                None => {
                    tracing::warn!("未找到默认窗口图标，托盘将使用系统默认图标（功能不受影响）");
                    tauri::image::Image::new_owned(Vec::new(), 0, 0)
                }
            };
            let tray_icon = TrayIconBuilder::with_id("statusbar")
                .icon(icon)
                // v1.2.10：macOS 上把图标声明为 template，系统据此按菜单栏明暗反色。
                // ⚠️ 该 API 无 cfg 门控（tauri/src/tray/mod.rs:295 直通
                // tray-icon 的 with_icon_as_template），非 macOS 平台是**空操作**
                // （只有 macos 的 platform_impl 有实现），故无需再包一层 cfg；
                // 但为了语义清晰仍只在 macOS 置真 —— 万一将来 tray-icon 给
                // Windows 也加上 template 语义，这里不会误伤。
                .icon_as_template(cfg!(target_os = "macos"))
                .menu(&tray_menu)
                // v0.7.6：左键不再弹菜单——非 mac 左键=显示主窗口、右键=快捷菜单；
                // macOS 左键沿用「点击切换主窗口显隐」（见下方 on_tray_icon_event）
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                            let _ = app.emit_to("main", "tray-shown", ());
                        }
                    }
                    "quit" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.destroy();
                        }
                        app.exit(0);
                    }
                    // v0.7.11（2026-09-14）：托盘「启用状态栏」总开关（与设置页
                    // set_status_bar_config 同一套持久化）。
                    // - 三端已统一为「指标只由悬浮指标条展示」，故打开总开关时顺手打开
                    //   浮窗子项，避免「开了状态栏却什么都不显示」的死开关体验；
                    //   关闭总开关则浮窗一并隐藏（可见态 = enabled && float_enabled）。
                    // - emit status-bar-config-changed → 设置页即时刷新，勾选态与页面一致。
                    "toggle_status_bar" => {
                        let state = app.state::<Arc<AppState>>();
                        let mut cfg = *state
                            .status_bar_config
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        cfg.enabled = !cfg.enabled;
                        if cfg.enabled && !cfg.float_enabled {
                            cfg.float_enabled = true;
                        }
                        match cfg.save(&state.db) {
                            Ok(()) => {
                                *state
                                    .status_bar_config
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner()) = cfg;
                                // 菜单勾选态不自动翻转，手动同步（勾选 = enabled）
                                let _ = status_toggle.set_checked(cfg.enabled);
                                // 通知前端设置页刷新（托盘 ↔ 页面双向一致）
                                let _ = app.emit_to("main", "status-bar-config-changed", cfg);
                                // 同步显隐浮窗（不存在则幂等创建）
                                let want = cfg.enabled && cfg.float_enabled;
                                if let Err(e) = float_window::set_float_visible(app, want) {
                                    tracing::error!(error = %e, "切换悬浮指标条失败");
                                }
                            }
                            Err(e) => {
                                tracing::error!(error = %e, "状态栏开关持久化失败");
                            }
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|_tray, _event| {
                    // ===== v0.7.9（2026-09-12）macOS 左键点击「只闪一下」修复 =====
                    // 根因：此处原写 `TrayIconEvent::Click { .. }`——既不筛按键、也不筛 button_state。
                    // macOS 上一次左键单击会先后派发 Click{Left,Down} 与 Click{Left,Up} 两个事件，
                    // 于是 toggle 被执行两次（显示后又立刻隐藏）→ 界面一闪而过；右键同理也会误切显隐。
                    // 现在与下方非 mac 分支一致：只认「左键 + 抬起」，单击刚好一次 toggle。
                    // （右键不再触发 toggle，只由系统弹出快捷菜单 → 「显示主窗口」菜单项兜底）
                    #[cfg(target_os = "macos")]
                    {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = _event
                        {
                            if let Some(w) = _tray.app_handle().get_webview_window("main") {
                                if w.is_visible().unwrap_or(false) {
                                    let _ = w.hide();
                                } else {
                                    let _ = w.show();
                                    let _ = w.set_focus();
                                    let _ = _tray.app_handle().emit_to("main", "tray-shown", ());
                                }
                            }
                        }
                    }
                    // v0.7.6：非 mac 左键单击 → 显示主窗口（右键由系统自动弹快捷菜单）
                    #[cfg(not(target_os = "macos"))]
                    {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = _event
                        {
                            if let Some(w) = _tray.app_handle().get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                                let _ = _tray.app_handle().emit_to("main", "tray-shown", ());
                            }
                        }
                    }
                })
                .build(app)?;
            // ===== 后台线程：悬浮指标条显隐管理（三端统一）=====
            // v0.7.11（2026-09-13）按产品决策收敛（Ryan 反馈）：
            // - macOS 原「菜单栏文字指标」（`tray.set_title` 那一整套：TrayTitleConfig /
            //   tray_title_with / tray_title / should_update / cache_snapshot）**整体移除**；
            // - 三端现在完全一致：系统指标只由「悬浮指标条」（float 窗口）显示，托盘恒为品牌图标。
            // - 旧约定失效：v0.7.8 的「仅 macOS 用原生菜单栏显示指标」作废——不再有任何平台
            //   把指标写进托盘/菜单栏文字。
            // 本线程只剩一件事：按 enabled && float_enabled 管理浮窗显隐（含全屏自动隐藏）。
            use std::time::Duration;
            let tray_clone = tray_icon.clone();
            let state_clone = app_state.clone();
            std::thread::Builder::new()
                .name("float-visibility".into())
                .spawn(move || {
                    const POLL_INTERVAL_MS: u64 = 1000;
                    const IDLE_INTERVAL_MS: u64 = 5000;
                    // 浮窗显隐状态缓存（只在翻转时调 show/hide，避免每秒无效调用）
                    let mut float_shown = false;
                    // v0.8.0：全屏态缓存（只在翻转时通知 pet 窗口，避免每秒刷事件）
                    let mut last_fullscreen = false;
                    loop {
                        let cfg = *state_clone
                            .status_bar_config
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        // 悬浮指标条显隐管理（全平台）。
                        // 受「启用状态栏」总开关统管——enabled=false 时浮窗一并隐藏，
                        // 消除「总开关关了浮窗还显示」的死开关困惑。
                        // 前台全屏 → 自动隐藏，退出全屏恢复
                        // （fullscreen 检测 Windows 实装，macOS/Linux 为 false stub）。
                        if cfg.enabled && cfg.float_enabled {
                            let want_show = !system_load::fullscreen::foreground_is_fullscreen();
                            if want_show != float_shown {
                                float_shown = want_show;
                                let handle = tray_clone.app_handle().clone();
                                if let Err(e) = float_window::set_float_visible(&handle, want_show)
                                {
                                    tracing::warn!(error = %e, "悬浮指标条显隐切换失败");
                                }
                            }
                        } else if float_shown {
                            float_shown = false;
                            let handle = tray_clone.app_handle().clone();
                            let _ = float_window::set_float_visible(&handle, false);
                        }
                        // v0.8.0（2026-09-16）：前台全屏翻转 → 通知桌宠窗口。
                        // 本线程只知道「有没有全屏」，不知道用户是否开了桌宠（那是前端
                        // localStorage 的事），故只发事件；由 PetWindow 自己决定隐藏/恢复，
                        // 且只在「本来就可见」时才恢复，绝不复活用户关掉的桌宠。
                        let fs_now = system_load::fullscreen::foreground_is_fullscreen();
                        if fs_now != last_fullscreen {
                            last_fullscreen = fs_now;
                            let handle = tray_clone.app_handle();
                            let evt = if fs_now {
                                "pet-fullscreen-enter"
                            } else {
                                "pet-fullscreen-exit"
                            };
                            let _ = handle.emit_to("pet", evt, ());
                        }
                        // 轮询节奏：浮窗可见 → 1s（前台全屏检测需要及时响应）；否则 5s 低频。
                        let fast = cfg.enabled && cfg.float_enabled;
                        std::thread::sleep(Duration::from_millis(if fast {
                            POLL_INTERVAL_MS
                        } else {
                            IDLE_INTERVAL_MS
                        }));
                    }
                })
                .ok();
            // ===== v0.7.6：启动时若「悬浮指标条」可见（enabled + float_enabled 双条件）
            // → 创建并显示浮窗 =====
            // 位置由前端 FloatBar.vue 从 localStorage 恢复；透明空窗 show 不可见，无白闪
            // v1.2.2：同时取出 float_glass，材质与 44px 高度都要在 show 之前落定
            //
            // ===== v1.2.10（2026-10-26）：conf 几何定标与「哪个 builder 生效」=====
            //
            // 事实核查（读 tauri 源码 + 实测，不是推测）：`app.config().app.windows`
            // 里的窗口由 tauri 自己在**用户 setup 闭包之前**逐个建好
            // （tauri-2.11.5/src/app.rs:2524 `for window_config in ... .filter(|w| w.create)`），
            // 而 `float_window::ensure_float_window` 第一行就是
            // `if app.get_webview_window(LABEL).is_some() { return Ok(()) }`。
            // → 对 float 窗口，**conf 才是唯一生效的构造参数**；`ensure_float_window`
            // 里那份 `.inner_size(340, 44)` / `.min_inner_size(140, h)` 属于**死代码路径**
            // （只有"窗口被销毁后重建"才会走到，当前没有任何路径 destroy float 窗口）。
            //
            // 这条事实直接决定 conf 该怎么改（报告 §2.4 方案 3 三项的最终裁决）：
            //
            //   - `x/y: 24`：**已加**。conf 无 x/y 时 tao 会兜底 `ns_window.center()`
            //     （tao-0.35.3/platform_impl/macos/window.rs:331
            //     `if attrs.position.is_none() { ns_window.center(); }`），
            //     而 float 窗口 340px 宽、主窗口 1040px 宽且自身 center —— 于是残影
            //     正好压在主窗口正中央（报告 §2.2 T0），最显眼的位置。
            //     给 x/y 后残影落到屏幕左上角 24px 处，用户几乎不会注意。
            //     ⚠️ 前端 applyPosition 随后会用 localStorage 或「主屏右上角」覆盖它，
            //     所以 x/y 只影响「show 到 applyPosition 之间那几帧」—— 正是残影窗口期。
            //     ⚠️ tauri 要求 x/y **同时给**才生效
            //     （tauri-runtime-wry-2.11.4/src/lib.rs:974 `if let (Some(x), Some(y))`），
            //     只给一个等于没给。
            //
            //   - `height: 36 → 44`：**不改，维持 36**。这条我先前判"可以且应当改"，
            //     是**错的**，现按实际时序推翻：
            //     ① 它对玻璃用户**没有收益** —— 玻璃档的 44 已由本文件下方
            //        `float_glass::apply` 后的 `set_size(cur_w, 44)` 在 show **之前**
            //        落定，且 FloatBar 的 `height:100%` 会自动跟随；conf 写 44 只是重复。
            //     ② 它对非玻璃用户**有实际损害** —— 时序是
            //        `set_float_visible(true)` 里的 `window.show()`（lib.rs:835）**早于**
            //        webview 加载完成后的 `onMounted → fitWidth()`（FloatBar.vue:545）。
            //        中间这段时间窗口已上屏、CSS 已按 `height:100%` 画出 44px 胶囊，
            //        等 fitWidth 调 `resize_float_window` 压回 36 时才跳一下。
            //        即「为了消掉一个本来就已消除的 8px 差，制造一个新的可见跳动」。
            //     ③ 真正消掉「窗口 44 / 胶囊 36」的是 v1.2.10 把 CSS 改成
            //        `height:100%`（单一来源），不是改 conf —— 两者不要混为一谈。
            //
            //   - `minHeight: 36 → 44`：**绝不改，维持 36**。`resize_float_window` 在
            //     玻璃关闭时会 `set_size(w, 36)`（float_window.rs:256 `current_height`），
            //     而 minHeight 44 会让 OS 直接拒绝这次缩高 → 高度差 8px 永久消不掉，
            //     且前端 1Hz 每秒重试一次 set_size/set_position → **每秒抖动**。
            //     即「conf 的 minHeight 必须等于**两档里较小的那个**」：
            //     minHeight = FLOAT_HEIGHT_PLAIN = 36。
            //
            // ⚠️ 上述说明无法写进 tauri.conf.json（JSON 不支持注释），故落在本文件。
            let (float_resume, float_glass_on) = {
                let sb = app_state
                    .status_bar_config
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                (sb.enabled && sb.float_enabled, sb.float_glass)
            };
            if float_resume {
                let handle = app.handle().clone();
                // v1.2.2：**先建窗 → 上材质 → 定高 → 再 show**。
                // ⚠️ 这里不走 `float_glass::apply_via_app()`：它内部用 run_on_main_thread
                // 异步派发，回调可能在 show() 之后才执行，用户会先看到 36px 无材质的窗
                // 再跳成 44px 玻璃。而 setup 本身就跑在主线程（tauri app.rs::setup 由
                // 事件循环主线程驱动），可以直接同步调用，保证时序。
                if let Err(e) = float_window::ensure_float_window(&handle) {
                    tracing::warn!(error = %e, "启动恢复悬浮指标条失败");
                } else if float_glass_on {
                    if let Some(win) = handle.get_webview_window(float_window::FLOAT_WINDOW_LABEL) {
                        if let Err(e) = float_glass::apply(&win, true) {
                            tracing::warn!(error = %e, "启动恢复液态玻璃失败，退化为 CSS 兜底样式");
                        }
                        // 宽度沿用 conf 的 340，前端 fitsWidth 后会自动收缩
                        let scale = win.scale_factor().unwrap_or(1.0);
                        let cur_w = win
                            .inner_size()
                            .map(|s| s.to_logical::<f64>(scale).width)
                            .unwrap_or(float_window::FLOAT_INITIAL_WIDTH);
                        let _ = win.set_size(tauri::LogicalSize::new(
                            cur_w,
                            float_glass::FLOAT_HEIGHT_GLASS,
                        ));
                    }
                }
                if let Err(e) = float_window::set_float_visible(&handle, true) {
                    tracing::warn!(error = %e, "启动恢复悬浮指标条失败");
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::start_tracking,
            commands::stop_tracking,
            commands::is_tracking,
            commands::get_current_foreground,
            commands::get_overview,
            commands::get_daily_summaries,
            commands::get_daily_categories,
            commands::get_month_summary,
            commands::get_hourly_buckets,
            commands::get_app_ranking,
            commands::get_categories,
            commands::check_permissions,
            commands::open_privacy_settings,
            commands::restart_app,
            commands::reset_permission,
            commands::reset_screen_capture_permission,
            commands::get_rules,
            commands::add_rule,
            commands::update_rule,
            commands::delete_rule,
            commands::reclassify_all,
            commands::set_autostart,
            commands::get_autostart_pref,
            commands::get_trends,
            commands::export_all,
            commands::import_data,
            commands::prune_data,
            commands::backup_and_prune_device,
            commands::get_backup_config,
            commands::save_backup_config,
            commands::run_backup_now,
            commands::get_devices,
            commands::list_devices_with_stats,
            commands::get_settings,
            commands::save_settings,
            commands::reveal_path,
            commands::check_webview2,
            commands::open_webview2_download,
            commands::check_for_update,
            commands::auto_check_update,
            commands::dismiss_update,
            commands::open_url,
            commands::get_system_metrics,
            commands::get_status_bar_config,
            commands::set_status_bar_config,
            commands::export_logs,
            commands::get_log_size,
            commands::get_log_dir,
            // ===== v0.8.0（2026-09-16）：屏幕截图 =====
            screenshot::screenshot_get_config,
            screenshot::screenshot_set_config,
            screenshot::screenshot_trigger,
            screenshot::screenshot_trigger_delayed,
            screenshot::screenshot_cancel_countdown,
            // v1.2.2：`screenshot_repeat_region` 已移除（详见 screenshot/mod.rs 模块头）
            screenshot::screenshot_frame,
            screenshot::take_capture_ready,
            screenshot::screenshot_commit,
            screenshot::screenshot_cancel,
            screenshot::screenshot_list,
            screenshot::screenshot_delete,
            screenshot::screenshot_delete_many,
            screenshot::screenshot_reveal,
            screenshot::screenshot_thumbnail,
            screenshot::screenshot_dir,
            screenshot::screenshot_ocr,
            screenshot::screenshot_copy_text,
            // v0.9.0：取字引擎信息（设置页展示当前引擎 + 增强引擎资源齐备情况）
            screenshot::ocr_engine_info,
            // v0.9.4：macOS 屏幕录制授权四件套（紧凑授权弹窗轮询 + 引导重启）
            screenshot::screenshot_permission_status,
            screenshot::screenshot_request_permission,
            screenshot::screenshot_open_permission_settings,
            screenshot::screenshot_restart_app,
            // v1.1.1：滑动截长图四件套（screenshot_long_start/capture/finish/cancel）
            // 已**移除** —— 遮罩窗是置顶窗，用户无法滚动它下方的目标窗口，
            // 功能在现有遮罩窗架构内不可达。详见 screenshot/mod.rs 修改历史。
            pet::create_pet_window,
            pet::show_pet_window,
            pet::hide_pet_window,
            pet::move_pet_window,
            pet::set_pet_cursor_passthrough,
            // v0.8.0：按身体 alpha 命中的穿透控制（修透明区点击死区）
            pet::set_pet_hit_mask,
            pet::set_pet_drag_lock,
            pet::clear_pet_hit_mask,
            // v1.2.12（批次 2）：Rust 侧拖拽循环（macOS 主方案 / Win·Linux 兜底）
            // IPC 实测 **91**（本文件 `generate_handler!` 内 `mod::cmd,` 行计数；v1.2.11 是 89，
            // 本版 +begin_drag +end_drag。别照文档递推 —— 见本文件头 2026-10-25 那条记账纠正）
            pet::begin_drag,
            pet::end_drag,
            pet::create_pet_menu_window,
            pet::show_pet_menu_window,
            pet::hide_pet_menu_window,
            pet::move_pet_menu_window,
            float_window::show_float_window,
            float_window::move_float_window,
            float_window::resize_float_window,
            // v1.2.2（2026-10-21）：液态玻璃能力探测（设置页开关可用性依据），IPC 82→83
            float_glass::float_glass_support,
            // v1.2.11（2026-10-27）：应用内自动升级，IPC 85→88
            app_update::app_update_supported,
            app_update::check_app_update,
            app_update::install_app_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub struct LogGuardHolder(pub Vec<tracing_appender::non_blocking::WorkerGuard>);

fn dir_for_log_fallback() -> std::path::PathBuf {
    let base = match std::env::var("HOME") {
        Ok(h) => std::path::PathBuf::from(h),
        Err(_) => std::env::temp_dir(),
    };
    let dir = base.join(".screentime-pro").join("logs");
    let _ = std::fs::create_dir_all(&dir);
    dir
}
