//! ScreenTime Pro —— Rust 后端入口
//!
//! 这里完成四件核心事情：
//! 1. `setup`：创建数据库、注入全局状态（State）、加载分类规则
//! 2. 系统托盘：应用关闭时最小化到托盘/菜单栏，而不是直接退出
//! 3. 菜单栏纯后台模式（macOS）：设为 Accessory 激活策略，去掉 Dock 图标
//! 4. 启动即自动追踪 + 开机自启；命令注册：把 Rust 函数暴露给前端（Vue）通过 `invoke` 调用

mod categorizer;
mod classifier;
mod commands;
mod db;
mod error;
mod tracker;

use std::sync::{Arc, Mutex};

use classifier::Rule;
use db::AppDb;
use tauri::menu::{Menu, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
// TrayIconEvent 仅 macOS 菜单栏模式（左键切换窗口）使用，按平台条件导入避免 Windows/Linux 告警
#[cfg(target_os = "macos")]
use tauri::tray::TrayIconEvent;
use tauri::{Emitter, Manager};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tracker::{create_tracker, PlatformTracker};

use crate::commands::ActiveSession;

/// 全局共享状态（通过 Tauri `manage` 注入，命令函数用 `tauri::State` 取出）
pub struct AppState {
    /// SQLite 封装（本地存储，零上传，隐私优先）
    pub db: AppDb,
    /// 当前平台的采集器（macOS/Windows/Linux 自动选择）
    pub tracker: Arc<dyn PlatformTracker>,
    /// 设备唯一标识（首次运行生成并持久化，多设备合并时区分数据来源）
    pub device_id: String,
    /// 设备名（与 db.settings.device_name 同步，save_settings 时内存立即更新，无需重启）
    pub device_name: Mutex<String>,
    /// 是否正在追踪（防止重复启动采样循环）
    pub tracking: Mutex<bool>,
    /// 空闲阈值（秒）：超过该时长无操作视为「离开」，不计入有效时长
    pub idle_threshold: Mutex<u64>,
    /// 当前进行中的使用时段（跨采样周期保存）
    pub current: Mutex<Option<ActiveSession>>,
    /// 内存缓存的分类规则（采样循环匹配用，规则变更时刷新）
    pub rules: Mutex<Vec<Rule>>,
    /// 自动归类缓存（LRU 256 容量，避免每次都查 Wikipedia）
    pub category_cache: categorizer::CategoryCache,
}

/// 生成稳定的设备唯一 ID（首次运行时调用，之后持久化到 settings，不再变化）
///
/// 用「纳秒时间戳 + 进程 PID」做 FNV-1a 哈希，输出 12 位十六进制串，
/// 仅用于在本机与其他设备的导出数据之间做区分，不含有任何用户隐私。
fn gen_device_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let raw = format!("{}{}", nanos, pid);
    // FNV-1a 64 位哈希，压缩成固定长度十六进制
    let mut h: u64 = 0xcbf29ce484222325; // FNV offset basis
    for b in raw.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3); // FNV prime
    }
    format!("{:012x}", h)
}

/// 程序入口（桌面端 `main.rs` 调用，也为后续移动端预留）
pub fn run() {
    // 必须在 setup 之前创建采集器（构造本身依赖平台 API，无副作用）
    let tracker = create_tracker();

    tauri::Builder::default()
        // ===== 开机自启插件（macOS 用 LaunchAgent，跨平台）=====
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        // ===== 初始化：建库、注入状态 =====
        .setup(move |app| {
            // 取应用数据目录（macOS: ~/Library/Application Support/com.screentime.pro）
            let dir = app.path().app_data_dir()?;
            let db = AppDb::open(&dir)?;
            // 稳定的设备唯一标识：首次运行生成并写入 settings，之后复用（多设备合并依赖它）
            let device_id = db
                .get_setting("device_id")
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    let id = gen_device_id();
                    let _ = db.set_setting("device_id", &id);
                    id
                });
            // 设备显示名称：首次运行（或仍为默认设备 ID）时取本机电脑名（hostname），
            // 否则复用用户已保存的名称。这样默认展示「我的电脑」之类可读名，而不是一长串哈希 ID。
            // （仅用于落库；get_settings 直接从 DB 读取，故用 _ 前缀避免未使用告警）
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
            // 首次运行默认开启「开机自启」：若从未设置过偏好，则启用系统自启项并写入设置
            if db.get_setting("autostart").is_none() {
                let _ = app.autolaunch().enable();
                let _ = db.set_setting("autostart", "true");
            }
            // 加载分类规则到内存缓存（采样循环据此自动归类）
            let rules = db.load_rules().unwrap_or_default();
            // 从 settings 表加载空闲阈值（保存的设置下次启动必须生效，否则用户会觉得「没保存」）
            let idle_threshold = db
                .get_setting("idle_threshold")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(300); // 默认 5 分钟

            // 从 db 读取本机设备名（与 v0.4.0 的 device_name 内存字段同步：保存时无需重启）
            let device_name_from_db = db
                .get_setting("device_name")
                .unwrap_or_else(|| device_id.clone());

            // 把数据库与采集器等放入全局状态，供命令使用
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
            });
            app.manage(app_state.clone());

            // ===== 菜单栏纯后台模式（仅 macOS）：去掉 Dock 图标 =====
            // 设为 Accessory 激活策略后，应用不出现在 Dock 与 Cmd+Tab，
            // 仅以菜单栏/托盘常驻，成为纯状态栏应用。
            #[cfg(target_os = "macos")]
            {
                use objc2::MainThreadMarker;
                use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
                // setup 由 Tauri 在主线程调用，可安全获取主线程标记
                if let Some(mtm) = MainThreadMarker::new() {
                    let ns_app = NSApplication::sharedApplication(mtm);
                    // Accessory：去掉 Dock 图标与 Cmd+Tab 条目，仅驻留菜单栏/托盘
                    ns_app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
                }
            }

            // ===== 启动即自动追踪（无需手动触发）=====
            commands::begin_tracking(&app_state);

            // ===== 构建系统托盘（macOS 显示在菜单栏右上角）=====
            // 菜单：显示主窗口 / 退出
            let show_item = MenuItemBuilder::with_id("show", "显示主窗口")
                .enabled(true)
                .build(app)?;
            let quit_item =
                MenuItemBuilder::with_id("quit", "退出").enabled(true).build(app)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            // 复用应用图标作为托盘图标（无需额外资源）
            let icon = app.default_window_icon().unwrap().clone();

            TrayIconBuilder::new()
                .icon(icon)
                .menu(&tray_menu)
                .show_menu_on_left_click(true) // macOS/Windows 左键点击也弹出菜单
                .on_menu_event(|app, event| match event.id.as_ref() {
                    // 显示主窗口并聚焦
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                            // 通知前端立即拉取一次最新数据（避免看到 stale 的「已记录 Xh」）
                            let _ = app.emit_to("main", "tray-shown", ());
                        }
                    }
                    // 真正退出程序（区别于「关闭窗口到托盘」）
                    // Windows 上先销毁主窗口再退出，确保进程彻底终止（避免托盘右键退出无效）
                    "quit" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.destroy();
                        }
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|_tray, _event| {
                    // 仅 macOS 菜单栏模式：左键点击在「显示/隐藏」间切换
                    // Windows/Linux 右键本就弹菜单，不在此处处理，避免与菜单/退出冲突
                    #[cfg(target_os = "macos")]
                    {
                        if let TrayIconEvent::Click { .. } = _event {
                            if let Some(w) = _tray.app_handle().get_webview_window("main") {
                                if w.is_visible().unwrap_or(false) {
                                    let _ = w.hide();
                                } else {
                                    let _ = w.show();
                                    let _ = w.set_focus();
                                    // 同菜单「显示主窗口」：唤起后立刻通知前端刷新一次
                                    let _ = _tray.app_handle().emit_to("main", "tray-shown", ());
                                }
                            }
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        // ===== 拦截窗口关闭：最小化到托盘而非退出 =====
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // 阻止默认行为（默认会退出整个应用）
                api.prevent_close();
                // 仅隐藏窗口，程序与后台采样继续运行
                let _ = window.hide();
            }
        })
        // ===== 注册前端可调用的命令 =====
        .invoke_handler(tauri::generate_handler![
            commands::start_tracking,
            commands::stop_tracking,
            commands::is_tracking,
            commands::get_current_foreground,
            commands::get_overview,
            commands::get_daily_summaries,
            commands::get_daily_categories,
            commands::get_hourly_buckets,
            commands::get_app_ranking,
            commands::get_categories,
            commands::get_sessions,
            commands::set_idle_threshold,
            commands::get_idle_threshold,
            commands::export_data,
            commands::check_permissions,
            commands::open_privacy_settings,
            // 分类规则引擎
            commands::get_rules,
            commands::add_rule,
            commands::update_rule,
            commands::delete_rule,
            commands::reclassify_all,
            // 开机自启
            commands::set_autostart,
            commands::is_autostart,
            commands::get_autostart_pref,
            // 周/月同比分析
            commands::get_trends,
            // 全量导出 / 导入合并
            commands::export_all,
            commands::import_data,
            commands::prune_data,
            commands::backup_and_prune_device,
            // 多设备合并
            commands::get_devices,
            commands::list_devices_with_stats,
            commands::get_settings,
            commands::save_settings,
            // 文件管理器定位（导出后打开所在目录）
            commands::reveal_path,
            // WebView2 运行时检测（仅 Windows 真正生效）
            commands::check_webview2,
            commands::open_webview2_download,
            // 检查更新（拉 GitHub Releases API）
            commands::check_for_update,
            commands::open_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
