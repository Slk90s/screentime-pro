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
mod logging;
mod tracker;
mod pet;
mod system_load;

use std::process::Command;
use std::sync::{Arc, Mutex};

use classifier::Rule;
use db::AppDb;
use tauri::menu::{Menu, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
#[cfg(target_os = "macos")]
use tauri::tray::TrayIconEvent;
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
    // ===== v0.7.5：托盘系统指标监测 =====
    pub metrics_enabled: Mutex<bool>,
    #[cfg(target_os = "macos")]
    pub metrics_sampler: Option<Arc<system_load::MetricsSampler>>,
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
    let out = Command::new("ioreg")
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
    let out = Command::new("reg")
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
            let metrics_enabled = db
                .get_setting("statusbar_metrics_enabled")
                .and_then(|s| s.parse::<bool>().ok())
                .unwrap_or(false);
            #[cfg(target_os = "macos")]
            let metrics_sampler = {
                let cpu_monitor = Arc::new(system_load::CpuMonitor::new());
                Some(Arc::new(system_load::MetricsSampler::new(cpu_monitor)))
            };
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
                metrics_enabled: Mutex::new(metrics_enabled),
                #[cfg(target_os = "macos")]
                metrics_sampler,
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
            let show_item = MenuItemBuilder::with_id("show", "显示主窗口")
                .enabled(true)
                .build(app)?;
            let quit_item =
                MenuItemBuilder::with_id("quit", "退出").enabled(true).build(app)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;
            let icon = app.default_window_icon().unwrap().clone();
            let tray_icon = TrayIconBuilder::with_id("statusbar")
                .icon(icon)
                .menu(&tray_menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
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
                })
                .build(app)?;
            #[cfg(target_os = "macos")]
            {
                use std::time::Duration;
                let tray_clone = tray_icon.clone();
                let sampler_opt = app_state.metrics_sampler.clone();
                let state_clone = app_state.clone();
                if let Some(ref sampler) = sampler_opt {
                    let _ = sampler.sample_all();
                }
                std::thread::Builder::new()
                    .name("metrics-sampler".into())
                    .spawn(move || {
                        const POLL_INTERVAL_MS: u64 = 1000;
                        let mut slow_tick = 0u64;
                        loop {
                            let enabled = state_clone
                                .metrics_enabled
                                .lock()
                                .map(|g| *g)
                                .unwrap_or(false);
                            if enabled {
                                if let Some(ref sampler) = sampler_opt {
                                    let snap = sampler.sample_all();
                                    if sampler.should_update(&snap) {
                                        let title = sampler.tray_title(&snap);
                                        let _ = tray_clone.set_title(title);
                                        sampler.cache_snapshot(snap);
                                    }
                                }
                                std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
                            } else {
                                let _ = tray_clone.set_title("");
                                std::thread::sleep(Duration::from_secs(5));
                            }
                            slow_tick = slow_tick.wrapping_add(1);
                            if slow_tick % 60 == 0 && enabled {
                                if let Some(ref sampler) = sampler_opt {
                                    let snap = sampler.sample_all();
                                    let title = sampler.tray_title(&snap);
                                    let _ = tray_clone.set_title(title);
                                    sampler.cache_snapshot(snap);
                                }
                            }
                        }
                    })
                    .ok();
            }
            #[cfg(not(target_os = "macos"))]
            let _ = tray_icon;
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
