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
mod logging;
mod tracker;

// v0.6.0-beta 桌宠子系统（独立模块，零侵入既有逻辑）
mod pet;

// v0.6.2-beta.15：系统 CPU 负载监测（跨平台；驱动桌宠"暴躁升温"状态）
mod system_load;

use std::process::Command;
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

/// 生成基于硬件的稳定设备标识，跨重装/卸载保持不变。
///
/// 优先读取机器级硬件标识（与重装无关），全部失败才回退到随机 ID（旧行为）：
/// - macOS：`ioreg` 读 `IOPlatformUUID`
/// - Windows：注册表 `HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid`
/// - Linux：`/etc/machine-id`
/// 返回统一加 `hw-` 前缀，便于与旧随机 ID 区分。
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
    // 所有平台特定方法都失败（理论上不会），回退到随机 ID，保证可用
    gen_device_id()
}

#[cfg(target_os = "macos")]
fn mac_hardware_uuid() -> Option<String> {
    let out = Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    for line in s.lines() {
        if line.contains("IOPlatformUUID") {
            // 形如：    "IOPlatformUUID" = "XXXX-XXXX-..."
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
    let out = Command::new("reg")
        .args(["query", "HKLM\\SOFTWARE\\Microsoft\\Cryptography", "/v", "MachineGuid"])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    for line in s.lines() {
        if line.contains("MachineGuid") {
            // 形如：    MachineGuid    REG_SZ    xxxxxxxx-xxxx-...
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
        // ===== 日志插件（前端 Vue 通过 @tauri-apps/plugin-log 写入同一文件）=====
        .plugin(tauri_plugin_log::Builder::default().build())
        // ===== 对话框插件（备份路径选择文件夹对话框）=====
        .plugin(tauri_plugin_dialog::init())
        // ===== 初始化：建库、注入状态 =====
        .setup(move |app| {
            // ---- 1. 初始化日志系统（最优先，其他模块才可埋点）----
            let log_dir = app
                .path()
                .app_log_dir()
                .unwrap_or_else(|_| dir_for_log_fallback());
            // WorkerGuard 必须存活到进程结束，否则非阻塞写入会被强制 flush
            // → 通过 app.manage 挂到全局 State，AppHandle 拥有它
            let log_guard = logging::init(&log_dir, cfg!(debug_assertions))
                .unwrap_or_else(|e| {
                    eprintln!("[main] 日志初始化失败: {}，降级到 stderr", e);
                    None
                });
            // 启动日志：用 tauri.conf.json 的 version（业务语义版本）而非 Cargo.toml 的
            // （CARGO_PKG_VERSION 始终是 lib crate 的初版 0.1.0，不随发版变化）
            let app_version = app.package_info().version.to_string();
            tracing::info!(
                version = %app_version,
                debug = cfg!(debug_assertions),
                "ScreenTime Pro 启动"
            );

            // 取应用数据目录（macOS: ~/Library/Application Support/com.screentime.pro）
            let dir = app.path().app_data_dir()?;
            let db = AppDb::open(&dir)?;
            // 稳定的设备唯一标识：首次运行生成并写入 settings，之后复用（多设备合并依赖它）
            // v0.7.2 起：新安装改用「基于硬件」的稳定 ID（跨重装/卸载保持不变），
            // 避免卸载重装后生成全新 device_id 导致数据碎片化。已存有旧随机 ID 的存量用户保持不变。
            let device_id = db
                .get_setting("device_id")
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    let id = hardware_device_id();
                    let _ = db.set_setting("device_id", &id);
                    id
                });
            // 迁移回填：旧版本（无 device 列）写入的 session 会被 ALTER 落到 'default'，
            // 在 device_id 已知后纠正为本机真实设备，避免幽灵「default」设备。
            let _ = db.backfill_device_column(&device_id);
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
            // 日志 guard 也挂到全局 State，保证其生命周期 = 程序生命周期
            // （否则 setup 闭包结束时 guard 被 drop，pending 的日志会强制 flush 并丢失）
            if let Some(g) = log_guard {
                app.manage(LogGuardHolder(Some(g)));
            }

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

            // ===== v0.7.2：自动备份定时线程（参考微信桌面版逻辑）=====
            // 每 30 分钟检查一次：若「自动备份」开启且今天尚未备份，则生成一份 JSON 到用户指定目录。
            // 启动后延迟 60 秒做首次检查（避免与初始化抢占 IO）。
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

            // ===== v0.6.2-beta.15：系统 CPU 负载监测线程（v0.7.0 调参）=====
            // 5s 间隔采样一次；连续 4 次（=20s）> 90% 时才判过热（向 pet 窗口发 overloading 事件）；
            // 连续 3 次（=15s）降至 < 55% 才恢复冷却。
            // 调高阈值 + 加长持续判定：避免日常编译/转码等瞬时高负载把桌宠频繁打入「升温抖动」，
            // 这是 v0.6 桌宠在桌面「抖动很频繁不丝滑」的主因（0.18s 高频抖动动画被反复触发）。
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
                    // 预热：先读一次让 monitor 拿到基线
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
                .ok(); // spawn 失败仅 log，跳过 system-load monitor；不阻塞 setup

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
            // v0.7.2：本地自动备份（微信桌面版式：本地落盘 JSON，用户自行拷到云盘）
            commands::get_backup_config,
            commands::save_backup_config,
            commands::run_backup_now,
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
            // 日志（v0.4.2）：用户报 bug 时一键导出
            commands::export_logs,
            commands::get_log_size,
            commands::get_log_dir,
            // 桌宠（v0.6.0-beta）：pet 窗口生命周期 + 鼠标穿透
            pet::create_pet_window,
            pet::show_pet_window,
            pet::hide_pet_window,
            pet::move_pet_window,
            pet::set_pet_cursor_passthrough,
            // v0.6.2-beta.17：桌宠右键菜单独立窗口（解决 teleport 模式菜单无法跨桌面拖拽）
            pet::create_pet_menu_window,
            pet::show_pet_menu_window,
            pet::hide_pet_menu_window,
            pet::move_pet_menu_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// 日志 WorkerGuard 的占位类型（v0.4.2 引入）
///
/// 把 `WorkerGuard` 通过 `app.manage` 挂到全局 State，AppHandle 拥有它直至进程结束。
/// 千万不要让 guard 在 setup 闭包结束时被 drop，否则非阻塞后台线程会强制 flush，
/// pending 的日志会丢失。
pub struct LogGuardHolder(pub Option<tracing_appender::non_blocking::WorkerGuard>);

/// app_log_dir() 失败时的兜底目录（按平台惯例）
fn dir_for_log_fallback() -> std::path::PathBuf {
    let base = match std::env::var("HOME") {
        Ok(h) => std::path::PathBuf::from(h),
        Err(_) => std::env::temp_dir(),
    };
    let dir = base.join(".screentime-pro").join("logs");
    let _ = std::fs::create_dir_all(&dir);
    dir
}
