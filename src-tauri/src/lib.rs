//! ScreenTime Pro —— Rust 后端入口
//!
//! 这里完成四件核心事情：
//! 1. `setup`：创建数据库、注入全局状态（State）、加载分类规则
//! 2. 系统托盘：应用关闭时最小化到托盘/菜单栏，而不是直接退出
//! 3. 菜单栏纯后台模式（macOS）：设为 Accessory 激活策略，去掉 Dock 图标
//! 4. 启动即自动追踪 + 开机自启；命令注册

mod categorizer;
mod classifier;
mod commands;
mod db;
mod error;
mod float_window;
mod logging;
mod tracker;
mod pet;
mod system_load;
// v0.7.7：隐藏控制台窗口地创建子进程（修复 Windows 首次启动 reg 命令框闪烁）
mod proc;

use std::sync::{Arc, Mutex};

use classifier::Rule;
use db::AppDb;
// v0.7.6（2026-09-10）：托盘菜单新增 CheckMenuItem 快捷开关 + 分隔线；
// 非 mac 平台也需要 TrayIconEvent/MouseButton（左键点击显示主窗口，右键弹菜单）
use tauri::menu::{CheckMenuItemBuilder, Menu, MenuItemBuilder, PredefinedMenuItem};
#[cfg(not(target_os = "macos"))]
use tauri::tray::{MouseButton, MouseButtonState};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
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
/// 用途：设置页 `set_status_bar_config` / `set_system_metrics_enabled` 保存成功后
/// 调用 `sync()` 同步菜单勾选态，防止「设置页改了、托盘菜单还挂旧勾」的显示不一致。
/// CheckMenuItem 句柄是廉价 clone（内部同一 Arc），与菜单事件闭包里持有的 clone 互不冲突。
/// 托盘菜单唯一的快捷开关句柄（悬浮指标条）。
/// v0.7.6 初版曾含 状态栏/CPU/网速/内存 四项勾选；2026-09-10 按 Ryan 反馈精简：
/// 指标勾选只保留在设置页「状态栏」卡片，托盘右键菜单仅留浮窗开关，避免重复冗长。
/// v0.7.7：旧的「内存子项仅 macOS 菜单存在」注释作废——内存/磁盘已三平台齐备，
/// 且托盘菜单本身不再承载任何指标勾选。
pub struct TrayFloatToggle {
    pub float: tauri::menu::CheckMenuItem<tauri::Wry>,
}

impl TrayFloatToggle {
    /// 设置页改 float_enabled 后同步托盘菜单勾选态（反向：托盘勾选 → emit 事件刷设置页）
    pub fn sync(&self, cfg: &commands::StatusBarConfig) {
        let _ = self.float.set_checked(cfg.float_enabled);
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
        .args(["query", "HKLM\\SOFTWARE\\Microsoft\\Cryptography", "/v", "MachineGuid"])
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
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let log_dir = app
                .path()
                .app_log_dir()
                .unwrap_or_else(|_| dir_for_log_fallback());
            let log_guard = logging::init(&log_dir, cfg!(debug_assertions))
                .unwrap_or_else(|e| {
                    eprintln!("[main] 日志初始化失败: {}，降级到 stderr", e);
                    None
                });
            let app_version = app.package_info().version.to_string();
            tracing::info!(
                version = %app_version,
                debug = cfg!(debug_assertions),
                "ScreenTime Pro 启动"
            );
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
            let status_bar_config =
                commands::StatusBarConfig::load(&db);
            // v0.7.6：sampler 跨平台始终存在
            let cpu_monitor_for_sampler = Arc::new(system_load::CpuMonitor::new());
            let metrics_sampler = Arc::new(system_load::MetricsSampler::new(cpu_monitor_for_sampler));
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
            if let Some(g) = log_guard {
                app.manage(LogGuardHolder(Some(g)));
            }
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
                                    let _ = app_handle.emit_to(
                                        "pet",
                                        "pet-system-overload",
                                        usage,
                                    );
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
                                    let _ = app_handle.emit_to(
                                        "pet",
                                        "pet-system-cool",
                                        usage,
                                    );
                                    tracing::info!(usage, "system cooled → pet 恢复");
                                }
                            } else {
                                cool_streak = 0;
                            }
                        }
                    }
                })
                .ok();
            // ===== v0.7.6（2026-09-10 精简）：托盘右键菜单 =「悬浮指标条 ✓ ─ 显示主窗口 / 退出」=====
            // - 初版曾把 状态栏/CPU/网速/内存 四项勾选放进托盘菜单；Ryan 反馈（2026-09-10）：
            //   托盘只留浮窗开关，指标勾选在设置页「状态栏」卡片里设计即可
            // - 浮窗开关与设置页同源：写 AppState.status_bar_config + SQLite 持久化，
            //   toggle 后 emit status-bar-config-changed → 设置页即时刷新（双向同步）
            // - 菜单文案沿用硬编码中文（原生菜单不做 i18n，与既有约定一致）
            let sb_cfg = *app_state
                .status_bar_config
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let float_item = CheckMenuItemBuilder::with_id("toggle_float", "悬浮指标条")
                .checked(sb_cfg.float_enabled)
                .build(app)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let show_item = MenuItemBuilder::with_id("show", "显示主窗口")
                .enabled(true)
                .build(app)?;
            let quit_item =
                MenuItemBuilder::with_id("quit", "退出").enabled(true).build(app)?;
            // ⚠️ with_items 要求元素统一为 &dyn IsMenuItem；CheckMenuItem /
            // PredefinedMenuItem / MenuItem 混排必须显式 Vec<&dyn ...> 注解，
            // 否则数组字面量推断出单一具体类型而编译失败
            let tray_items: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
                vec![&float_item, &sep, &show_item, &quit_item];
            let tray_menu = Menu::with_items(app, &tray_items)?;
            // 勾选态句柄：CheckMenuItem 点击后不自动翻转，菜单事件里手动 set_checked
            let float_toggle = float_item.clone();
            // 设置页 → 托盘菜单方向的同步句柄：manage 后 commands 里 try_state 取用
            // （原始 item move 进 struct，菜单事件闭包用的是上面的 clone，互不影响）
            app.manage(TrayFloatToggle { float: float_item });
            let icon = app.default_window_icon().unwrap().clone();
            // v0.7.6：非 mac 平台需要在状态栏关闭时把托盘图标还原回品牌图。
            // ⚠️ app.default_window_icon() 返回的 &Image 生命周期绑在 &mut App 上，
            // .clone() 仍带借用，不能 move 进 'static 线程；必须拷出原始 RGBA 字节，
            // 用 new_owned 重建一个 owned Image<'static>。
            #[cfg(not(target_os = "macos"))]
            let default_tray_icon: tauri::image::Image<'static> = {
                let raw = app
                    .default_window_icon()
                    .expect("default window icon");
                let rgba = raw.rgba().to_vec();
                tauri::image::Image::new_owned(rgba, raw.width(), raw.height())
            };
            let tray_icon = TrayIconBuilder::with_id("statusbar")
                .icon(icon)
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
                    // v0.7.6：悬浮指标条开关（与设置页 set_status_bar_config 同一套持久化）。
                    // 2026-09-10 精简：指标勾选已从托盘菜单移除，此处只处理浮窗开关
                    "toggle_float" => {
                        let state = app.state::<Arc<AppState>>();
                        let mut cfg = *state
                            .status_bar_config
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        cfg.float_enabled = !cfg.float_enabled;
                        // 2026-09-10：总开关统管后，托盘菜单勾浮窗时若「启用状态栏」还关着
                        // → 顺手打开总开关（否则会出现「勾了浮窗却什么都不显示」的死开关体验；
                        // emit 的 status-bar-config-changed 会把设置页一并刷成 enabled=true）
                        if cfg.float_enabled && !cfg.enabled {
                            cfg.enabled = true;
                        }
                        match cfg.save(&state.db) {
                            Ok(()) => {
                                *state
                                    .status_bar_config
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner()) = cfg;
                                // 菜单勾选态不自动翻转，手动同步
                                let _ = float_toggle.set_checked(cfg.float_enabled);
                                // 通知前端设置页刷新（托盘 ↔ 页面双向一致）
                                let _ =
                                    app.emit_to("main", "status-bar-config-changed", cfg);
                                // 同步显隐浮窗（不存在则幂等创建）
                                if let Err(e) =
                                    float_window::set_float_visible(app, cfg.float_enabled)
                                {
                                    tracing::error!(error = %e, "切换悬浮指标条失败");
                                }
                            }
                            Err(e) => {
                                tracing::error!(error = %e, "悬浮指标条开关持久化失败");
                            }
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|_tray, _event| {
                    #[cfg(target_os = "macos")]
                    {
                        if let TrayIconEvent::Click { .. } = _event {
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
            // ===== v0.7.6：跨平台托盘指标采样线程 =====
            // - macOS：set_title 到菜单栏（菜单栏原生支持完整文字）
            // - Windows/Linux：tray.set_title 在 Win 上仅写 tooltip，Linux 多数 DE 也不显示
            //   → 必须每 tick 把缩写文字画进 32x32 RGBA 图标 set_icon，
            //   同时 set_title(完整) 作为 tooltip（Windows 悬停可见）
            //   关闭时 set_icon(默认品牌图) + set_title(None) 还原
            //   详见 system_load/tray_icon.rs 的「画布约束」段
            use std::time::Duration;
            let tray_clone = tray_icon.clone();
            let sampler = app_state.metrics_sampler.clone();
            let state_clone = app_state.clone();
            #[cfg(not(target_os = "macos"))]
            let default_tray_icon_for_thread = default_tray_icon.clone();
            std::thread::Builder::new()
                .name("metrics-sampler".into())
                .spawn(move || {
                    const POLL_INTERVAL_MS: u64 = 1000;
                    #[cfg(target_os = "macos")]
                    let mut slow_tick: u64 = 0;
                    // v0.7.6：浮窗显隐状态缓存（只在翻转时调 show/hide，避免每秒无效调用）
                    let mut float_shown = false;
                    // 浮窗优先（用户反馈 2026-09-10）：开了悬浮指标条后托盘不再绘制指标，
                    // 仅浮窗显示；此标记记录托盘当前是否处于「画指标」态，翻转时才还原品牌图
                    let mut tray_shown_metrics = false;
                    loop {
                        let cfg = *state_clone
                            .status_bar_config
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        let title_cfg: system_load::TrayTitleConfig = cfg.into();
                        // v0.7.6：悬浮指标条显隐管理。
                        // 2026-09-10 语义修正（Ryan 反馈）：受「启用状态栏」总开关统管——
                        // enabled=false 时浮窗与托盘指标一并隐藏，消除「总开关关了浮窗还显示」
                        // 造成的死开关困惑（旧设计「浮窗独立于总开关」作废）。
                        // Windows 上前台全屏 → 自动隐藏，退出全屏恢复；设置页/菜单关闭时兜底隐藏。
                        if cfg.enabled && cfg.float_enabled {
                            let want_show =
                                !system_load::fullscreen::foreground_is_fullscreen();
                            if want_show != float_shown {
                                float_shown = want_show;
                                let handle = tray_clone.app_handle().clone();
                                if let Err(e) =
                                    float_window::set_float_visible(&handle, want_show)
                                {
                                    tracing::warn!(error = %e, "悬浮指标条显隐切换失败");
                                }
                            }
                        } else if float_shown {
                            float_shown = false;
                            let handle = tray_clone.app_handle().clone();
                            let _ = float_window::set_float_visible(&handle, false);
                        }
                        // 浮窗优先：悬浮指标条开启 → 托盘不画指标（还原品牌图 + 清 tooltip），仅浮窗显示
                        let want_tray_metrics = cfg.enabled && !cfg.float_enabled;
                        if want_tray_metrics {
                            let snap = sampler.sample_all();
                            let full_title =
                                sampler.tray_title_with(&snap, &title_cfg);
                            #[cfg(target_os = "macos")]
                            {
                                if sampler.should_update(&snap) {
                                    let _ = tray_clone.set_title(Some(full_title));
                                    sampler.cache_snapshot(snap);
                                }
                            }
                            #[cfg(not(target_os = "macos"))]
                            {
                                // 画缩写文字进 32x32 图标；同时 set_title(完整) 作为 tooltip
                                let lines = system_load::tray_icon::format_icon_lines(
                                    &snap, &title_cfg,
                                );
                                let refs: Vec<&str> =
                                    lines.iter().map(|s| s.as_str()).collect();
                                let img = system_load::tray_icon::render_tray_icon(&refs);
                                let _ = tray_clone.set_icon(Some(img));
                                let _ = tray_clone.set_title(Some(full_title));
                            }
                            tray_shown_metrics = true;
                            std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
                        } else {
                            // 还原品牌图 / 清 tooltip 只在「刚从画字态翻转过来」时执行一次，
                            // 避免每秒重设图标（set_icon 每次都重建句柄）
                            if tray_shown_metrics {
                                #[cfg(target_os = "macos")]
                                {
                                    let _ = tray_clone.set_title(Some(String::new()));
                                }
                                #[cfg(not(target_os = "macos"))]
                                {
                                    // 还原品牌图标 + 清除 tooltip
                                    let _ = tray_clone
                                        .set_icon(Some(default_tray_icon_for_thread.clone()));
                                    let _ = tray_clone.set_title(None::<&str>);
                                }
                                tray_shown_metrics = false;
                            }
                            // 浮窗可见时保持 1s 轮询（前台全屏检测的响应性依赖此循环）；
                            // 托盘指标与浮窗都不可见 → 5s 低频轮询省 CPU
                            if cfg.enabled && cfg.float_enabled {
                                std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
                            } else {
                                std::thread::sleep(Duration::from_secs(5));
                            }
                        }
                        #[cfg(target_os = "macos")]
                        {
                            slow_tick = slow_tick.wrapping_add(1);
                            if slow_tick % 60 == 0 && want_tray_metrics {
                                let snap = sampler.sample_all();
                                let title =
                                    sampler.tray_title_with(&snap, &title_cfg);
                                let _ = tray_clone.set_title(Some(title));
                                sampler.cache_snapshot(snap);
                            }
                        }
                    }
                })
                .ok();
            // ===== v0.7.6：启动时若「悬浮指标条」可见（enabled + float_enabled 双条件）
            // → 创建并显示浮窗 =====
            // 位置由前端 FloatBar.vue 从 localStorage 恢复；透明空窗 show 不可见，无白闪
            let float_resume = {
                let sb = app_state
                    .status_bar_config
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                sb.enabled && sb.float_enabled
            };
            if float_resume {
                let handle = app.handle().clone();
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
            commands::get_sessions,
            commands::set_idle_threshold,
            commands::get_idle_threshold,
            commands::export_data,
            commands::check_permissions,
            commands::open_privacy_settings,
            commands::get_rules,
            commands::add_rule,
            commands::update_rule,
            commands::delete_rule,
            commands::reclassify_all,
            commands::set_autostart,
            commands::is_autostart,
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
            commands::open_url,
            commands::get_system_metrics,
            commands::set_system_metrics_enabled,
            commands::get_system_metrics_enabled,
            commands::get_status_bar_config,
            commands::set_status_bar_config,
            commands::export_logs,
            commands::get_log_size,
            commands::get_log_dir,
            pet::create_pet_window,
            pet::show_pet_window,
            pet::hide_pet_window,
            pet::move_pet_window,
            pet::set_pet_cursor_passthrough,
            pet::create_pet_menu_window,
            pet::show_pet_menu_window,
            pet::hide_pet_menu_window,
            pet::move_pet_menu_window,
            float_window::create_float_window,
            float_window::show_float_window,
            float_window::hide_float_window,
            float_window::move_float_window,
            float_window::resize_float_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub struct LogGuardHolder(pub Option<tracing_appender::non_blocking::WorkerGuard>);

fn dir_for_log_fallback() -> std::path::PathBuf {
    let base = match std::env::var("HOME") {
        Ok(h) => std::path::PathBuf::from(h),
        Err(_) => std::env::temp_dir(),
    };
    let dir = base.join(".screentime-pro").join("logs");
    let _ = std::fs::create_dir_all(&dir);
    dir
}
