//! Tauri 命令层（前端通过 `invoke` 调用）

use crate::AppState;
use crate::classifier::classify_app;
use crate::db::{
    AppRankingOut, CategoryOut, CurrentForegroundOut, DailySummaryOut, DayCategoryOut, DeviceInfo,
    ExportBundle, ExportResult, HourlyBucketOut, MetricsOut, MonthSummaryOut, OverviewOut,
    PermissionStatus, PeriodStat, RuleOut, SessionOut, SettingsOut, TrendsOut,
};
use crate::error::AppError;
use crate::tracker::{platform_name, RawApp};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Weekday};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tauri::Manager;
use tauri_plugin_autostart::ManagerExt;

const SAMPLE_INTERVAL: u64 = 2;
const MIN_SESSION_SECS: i64 = 10;

pub struct ActiveSession {
    pub app: RawApp,
    pub app_id: i64,
    pub category_id: String,
    pub started_at: DateTime<Local>,
    pub last_input_at: DateTime<Local>,
}

pub fn begin_tracking(state: &Arc<AppState>) {
    {
        let r = state.tracking.lock().unwrap_or_else(|e| e.into_inner());
        if *r {
            return;
        }
    }
    *state.tracking.lock().unwrap_or_else(|e| e.into_inner()) = true;
    let st = Arc::clone(state);
    tauri::async_runtime::spawn(async move {
        sampling_loop(st).await;
    });
}

#[tauri::command]
pub fn start_tracking(state: tauri::State<'_, Arc<AppState>>) -> Result<bool, String> {
    begin_tracking(&state);
    Ok(true)
}

#[tauri::command]
pub fn stop_tracking(state: tauri::State<'_, Arc<AppState>>) -> Result<bool, String> {
    let mut r = state.tracking.lock().unwrap_or_else(|e| e.into_inner());
    *r = false;
    if let Some(active) = state.current.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let now = Local::now();
        finalize_active_session(&state, &active, now);
    }
    Ok(true)
}

fn finalize_active_session(
    state: &Arc<AppState>,
    active: &ActiveSession,
    now: DateTime<Local>,
) {
    let start_date = active.started_at.date_naive();
    let now_date = now.date_naive();
    let splits: Vec<(DateTime<Local>, DateTime<Local>)> = if start_date == now_date {
        vec![(active.started_at, now)]
    } else {
        let mut out = Vec::new();
        let mut cursor_date = start_date;
        let mut cursor_dt = active.started_at;
        loop {
            let next_date = cursor_date + Duration::days(1);
            let next_midnight = next_date
                .and_hms_opt(0, 0, 0)
                .and_then(|nd| nd.and_local_timezone(Local).single());
            let next_midnight = match next_midnight {
                Some(t) => t,
                None => break,
            };
            let end = if next_midnight > now { now } else { next_midnight };
            out.push((cursor_dt, end));
            if next_midnight > now {
                break;
            }
            cursor_date = next_date;
            cursor_dt = next_midnight;
        }
        out
    };
    for (seg_start, seg_end) in splits {
        let dur = (seg_end - seg_start).num_seconds().max(0) as i64;
        let last_in_seg = if active.last_input_at < seg_start {
            seg_start
        } else {
            active.last_input_at
        };
        let idle_dur = (seg_end - last_in_seg).num_seconds().max(0) as i64;
        let effective = (dur - idle_dur).max(0);
        if effective >= MIN_SESSION_SECS {
            let date = seg_start.format("%Y-%m-%d").to_string();
            let _ = state.db.insert_session(
                active.app_id,
                &active.category_id,
                &seg_start.to_rfc3339(),
                &seg_end.to_rfc3339(),
                effective,
                &date,
                active.app.window_title.as_deref(),
                &state.device_id,
            );
        }
    }
}

#[tauri::command]
pub fn is_tracking(state: tauri::State<'_, Arc<AppState>>) -> Result<bool, String> {
    Ok(*state.tracking.lock().unwrap_or_else(|e| e.into_inner()))
}

#[tauri::command]
pub fn get_current_foreground(state: tauri::State<'_, Arc<AppState>>) -> CurrentForegroundOut {
    let idle = state.tracker.get_idle_seconds().unwrap_or(0);
    let tracking = *state.tracking.lock().unwrap_or_else(|e| e.into_inner());
    let session_seconds = {
        let cur = state.current.lock().unwrap_or_else(|e| e.into_inner());
        match cur.as_ref() {
            Some(s) => (Local::now() - s.started_at).num_seconds().max(0),
            None => 0,
        }
    };
    match state.tracker.get_foreground_app() {
        Ok(app) => {
            let rules_clone = state.rules.lock().unwrap_or_else(|e| e.into_inner()).clone();
            let cat = classify_app(&app, &rules_clone);
            CurrentForegroundOut {
                name: app.name,
                process_name: app.process_name,
                category_id: cat,
                idle_seconds: idle,
                tracking,
                window_title: app.window_title,
                bundle_id: app.bundle_id,
                session_seconds,
            }
        }
        Err(_) => CurrentForegroundOut {
            name: "无".into(),
            process_name: String::new(),
            category_id: "other".into(),
            idle_seconds: idle,
            tracking,
            window_title: None,
            bundle_id: None,
            session_seconds,
        },
    }
}

#[tauri::command]
pub fn get_overview(
    state: tauri::State<'_, Arc<AppState>>,
    days: u32,
    date: String,
    device: Option<String>,
) -> Result<OverviewOut, String> {
    state.db.get_overview(days, &date, &device).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_daily_summaries(
    state: tauri::State<'_, Arc<AppState>>,
    days: u32,
    device: Option<String>,
) -> Result<Vec<DailySummaryOut>, String> {
    state.db.get_daily_summaries(days, &device).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_daily_categories(
    state: tauri::State<'_, Arc<AppState>>,
    days: u32,
    device: Option<String>,
) -> Result<Vec<DayCategoryOut>, String> {
    state.db.get_daily_categories(days, &device).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_month_summary(
    state: tauri::State<'_, Arc<AppState>>,
    year: i32,
    month: i32,
    device: Option<String>,
) -> Result<MonthSummaryOut, String> {
    state.db.get_month_summary(year, month, &device).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg("-R").arg(&path).status().map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer").arg(format!("/select,{}", path)).status().map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        let dir = std::path::Path::new(&path).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| path.clone());
        std::process::Command::new("xdg-open").arg(dir).status().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn export_logs(app: tauri::AppHandle) -> Result<ExportResult, String> {
    use std::io::Read;
    let log_dir = app.path().app_log_dir().map_err(|e| format!("无法定位日志目录: {}", e))?;
    if !log_dir.exists() {
        return Err(format!("日志目录不存在: {}", log_dir.display()));
    }
    let desktop = app.path().desktop_dir().or_else(|_| app.path().home_dir().map(|p| p.join("Desktop"))).map_err(|e| format!("无法定位桌面目录: {}", e))?;
    std::fs::create_dir_all(&desktop).ok();
    let ts = Local::now().format("%Y%m%d_%H%M%S");
    let txt_path = desktop.join(format!("screentime-pro-logs-{}.txt", ts));
    let mut out = std::fs::File::create(&txt_path).map_err(|e| format!("创建日志导出文件失败: {}", e))?;
    use std::io::Write;
    writeln!(out, "ScreenTime Pro 日志导出\n生成时间: {}\n日志目录: {}\n\n--- 文件列表 ---\n", Local::now().format("%Y-%m-%d %H:%M:%S"), log_dir.display()).ok();
    let mut entries: Vec<_> = std::fs::read_dir(&log_dir).map_err(|e| format!("读取日志目录失败: {}", e))?.filter_map(|e| e.ok()).filter(|e| {
        let name = e.file_name().to_string_lossy().to_string();
        crate::logging::is_our_log_file(&name)
    }).collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.file_name()));
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        writeln!(out, "\n=== {} ({} bytes) ===\n", name, entry.metadata().map(|m| m.len()).unwrap_or(0)).ok();
        if let Ok(mut f) = std::fs::File::open(&path) {
            let mut buf = String::new();
            if f.read_to_string(&mut buf).is_ok() {
                let _ = out.write_all(buf.as_bytes());
            }
        }
    }
    tracing::info!(path = %txt_path.display(), "用户导出日志");
    Ok(ExportResult { path: txt_path.to_string_lossy().to_string() })
}

#[tauri::command]
pub fn get_log_size(app: tauri::AppHandle) -> Result<u64, String> {
    let log_dir = app.path().app_log_dir().map_err(|e| format!("无法定位日志目录: {}", e))?;
    crate::logging::dir_size(&log_dir).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_log_dir(app: tauri::AppHandle) -> Result<String, String> {
    let log_dir = app.path().app_log_dir().map_err(|e| format!("无法定位日志目录: {}", e))?;
    Ok(log_dir.to_string_lossy().to_string())
}

#[tauri::command]
pub fn get_hourly_buckets(state: tauri::State<'_, Arc<AppState>>, date: String, device: Option<String>) -> Result<Vec<HourlyBucketOut>, String> {
    state.db.get_hourly_buckets(&date, &device).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_app_ranking(state: tauri::State<'_, Arc<AppState>>, days: u32, date: String, device: Option<String>) -> Result<Vec<AppRankingOut>, String> {
    state.db.get_app_ranking(days, &date, &device).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_categories(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<CategoryOut>, String> {
    state.db.get_categories().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_sessions(state: tauri::State<'_, Arc<AppState>>, date: String) -> Result<Vec<SessionOut>, String> {
    state.db.get_sessions(&date).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_trends(state: tauri::State<'_, Arc<AppState>>, period: String, device: Option<String>) -> Result<TrendsOut, String> {
    let (cur_start, cur_end, prev_start, prev_end, yoy_start, yoy_end, cur_label, prev_label) = period_ranges(&period);
    let current = state.db.period_summary(&cur_start, &cur_end, &device, &cur_label).map_err(|e| e.to_string())?;
    let prev = state.db.period_summary(&prev_start, &prev_end, &device, &prev_label).map_err(|e| e.to_string())?;
    let yoy: Option<PeriodStat> = if period == "month" {
        Some(state.db.period_summary(&yoy_start, &yoy_end, &device, "去年同期").map_err(|e| e.to_string())?)
    } else {
        None
    };
    let delta_total_pct = if prev.total_seconds > 0 {
        (current.total_seconds - prev.total_seconds) as f64 / prev.total_seconds as f64 * 100.0
    } else {
        0.0
    };
    Ok(TrendsOut { period, current, prev, yoy, delta_total_pct })
}

#[tauri::command]
pub fn set_idle_threshold(state: tauri::State<'_, Arc<AppState>>, secs: u64) -> Result<bool, String> {
    *state.idle_threshold.lock().unwrap_or_else(|e| e.into_inner()) = secs;
    Ok(true)
}

#[tauri::command]
pub fn get_idle_threshold(state: tauri::State<'_, Arc<AppState>>) -> Result<u64, String> {
    Ok(*state.idle_threshold.lock().unwrap_or_else(|e| e.into_inner()))
}

#[tauri::command]
pub fn export_data(app: tauri::AppHandle, state: tauri::State<'_, Arc<AppState>>, date: String, format: String) -> Result<ExportResult, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let exports = dir.join("exports");
    std::fs::create_dir_all(&exports).ok();
    let ext = if format == "json" { "json" } else { "csv" };
    let path = exports.join(format!("screentime_{}.{}", date, ext));
    if ext == "csv" {
        state.db.export_csv(&path, &date).map_err(|e| e.to_string())?;
    } else {
        let ranking = state.db.get_app_ranking(0, &date, &None).map_err(|e| e.to_string())?;
        let json = serde_json::to_string_pretty(&ranking).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())?;
    }
    Ok(ExportResult { path: path.to_string_lossy().to_string() })
}

#[tauri::command]
pub fn export_all(app: tauri::AppHandle, device_id: Option<String>) -> Result<ExportResult, String> {
    let bundle = state_export(&app, device_id.as_deref())?;
    let json = serde_json::to_string_pretty(&bundle).map_err(|e| e.to_string())?;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let exports = dir.join("exports");
    std::fs::create_dir_all(&exports).ok();
    let suffix = match device_id.as_ref().filter(|s| !s.is_empty()) {
        Some(id) => format!("_{}", &id[..id.len().min(12)]),
        None => String::new(),
    };
    let file = format!("screentime_backup{}_{}.json", suffix, today_str());
    let path = exports.join(file);
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(ExportResult { path: path.to_string_lossy().to_string() })
}

fn state_export(app: &tauri::AppHandle, device_id: Option<&str>) -> Result<ExportBundle, String> {
    let state = app.state::<Arc<AppState>>();
    state.db.export_all_filtered(device_id).map_err(|e| e.to_string())
}

#[derive(serde::Serialize, Clone)]
pub struct BackupAndPruneResult {
    pub backup_path: String,
    pub deleted_count: usize,
}

#[tauri::command]
pub fn backup_and_prune_device(app: tauri::AppHandle, device_id: String) -> Result<BackupAndPruneResult, String> {
    if device_id.trim().is_empty() {
        return Err("device_id 不能为空".to_string());
    }
    let bundle = state_export(&app, Some(&device_id))?;
    let json = serde_json::to_string_pretty(&bundle).map_err(|e| e.to_string())?;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let exports = dir.join("exports");
    std::fs::create_dir_all(&exports).ok();
    let suffix = &device_id[..device_id.len().min(12)];
    let file = format!("screentime_backup_{}_{}_pre_purge.json", suffix, today_str());
    let backup_path = exports.join(file);
    std::fs::write(&backup_path, &json).map_err(|e| e.to_string())?;
    let state = app.state::<Arc<AppState>>();
    let deleted = state.db.delete_all_sessions_for_device(&device_id).map_err(|e| e.to_string())?;
    Ok(BackupAndPruneResult { backup_path: backup_path.to_string_lossy().to_string(), deleted_count: deleted })
}

#[tauri::command]
pub fn import_data(app: tauri::AppHandle, content: String) -> Result<usize, String> {
    let bundle: ExportBundle = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let state = app.state::<Arc<AppState>>();
    state.db.import_data(&bundle).map_err(|e| e.to_string())
}

#[derive(serde::Serialize, Clone)]
pub struct BackupConfig {
    pub enabled: bool,
    pub path: String,
    pub keep_days: u32,
    pub last_date: String,
}

fn perform_backup(app: &tauri::AppHandle, path_override: Option<&str>) -> Result<String, String> {
    let state = app.state::<Arc<AppState>>();
    let bundle = state.db.export_all_filtered(None).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&bundle).map_err(|e| e.to_string())?;
    let dir = match path_override {
        Some(p) if !p.trim().is_empty() => PathBuf::from(p),
        _ => match state.db.get_setting("backup_path").filter(|s| !s.is_empty()) {
            Some(p) => PathBuf::from(p),
            None => {
                let d = app.path().app_data_dir().map_err(|e| e.to_string())?;
                d.join("exports")
            }
        },
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let today = today_str();
    let file = format!("screentime_backup_{}.json", today);
    let path = dir.join(&file);
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    let _ = state.db.set_setting("backup_last_date", &today);
    recycle_previous_backup(&dir, &today);
    if let Some(kd) = state.db.get_setting("backup_keep_days") {
        if let Ok(kd) = kd.parse::<u32>() {
            prune_backup_files(&dir, kd);
        }
    }
    Ok(path.to_string_lossy().to_string())
}

fn recycle_previous_backup(dir: &Path, today: &str) {
    let mut prev_path: Option<PathBuf> = None;
    let mut prev_date: Option<String> = None;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("screentime_backup_") && name.ends_with(".json") {
                let date_part = name.trim_start_matches("screentime_backup_").trim_end_matches(".json").to_string();
                if date_part.as_str() < today {
                    let newer = match &prev_date {
                        Some(pd) => date_part > *pd,
                        None => true,
                    };
                    if newer {
                        prev_date = Some(date_part);
                        prev_path = Some(entry.path());
                    }
                }
            }
        }
    }
    if let Some(p) = prev_path {
        match trash::delete(&p) {
            Ok(()) => eprintln!("[backup] 已将上一份备份移入回收站: {}", p.display()),
            Err(e) => eprintln!("[backup] 移入回收站失败（保留于原目录）: {} ({})", p.display(), e),
        }
    }
}

fn prune_backup_files(dir: &Path, keep_days: u32) {
    let cutoff = (Local::now() - Duration::days(keep_days as i64)).format("%Y-%m-%d").to_string();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("screentime_backup_") && name.ends_with(".json") {
                let date_part = name.trim_start_matches("screentime_backup_").trim_end_matches(".json");
                if date_part < cutoff.as_str() {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
}

pub fn auto_backup_tick(app: &tauri::AppHandle) {
    let state = match app.try_state::<Arc<AppState>>() {
        Some(s) => s,
        None => return,
    };
    let enabled = state.db.get_setting("backup_enabled").map(|v| v == "true").unwrap_or(false);
    if !enabled { return; }
    let today = today_str();
    let last = state.db.get_setting("backup_last_date").unwrap_or_default();
    if last == today { return; }
    if let Err(e) = perform_backup(app, None) {
        eprintln!("[auto-backup] 自动备份失败: {}", e);
    }
}

#[tauri::command]
pub fn get_backup_config(state: tauri::State<'_, Arc<AppState>>) -> BackupConfig {
    let enabled = state.db.get_setting("backup_enabled").map(|v| v == "true").unwrap_or(false);
    let path = state.db.get_setting("backup_path").unwrap_or_default();
    let keep_days = state.db.get_setting("backup_keep_days").and_then(|v| v.parse::<u32>().ok()).unwrap_or(30);
    let last_date = state.db.get_setting("backup_last_date").unwrap_or_default();
    BackupConfig { enabled, path, keep_days, last_date }
}

#[tauri::command]
pub fn save_backup_config(state: tauri::State<'_, Arc<AppState>>, enabled: bool, path: String, keep_days: u32) -> Result<(), String> {
    let _ = state.db.set_setting("backup_enabled", if enabled { "true" } else { "false" });
    if path.trim().is_empty() {
        let _ = state.db.set_setting("backup_path", "");
    } else {
        std::fs::create_dir_all(&path).map_err(|e| format!("无法创建备份目录：{}", e))?;
        let _ = state.db.set_setting("backup_path", &path);
    }
    let _ = state.db.set_setting("backup_keep_days", &keep_days.to_string());
    Ok(())
}

#[tauri::command]
pub fn run_backup_now(app: tauri::AppHandle) -> Result<ExportResult, String> {
    let path = perform_backup(&app, None)?;
    Ok(ExportResult { path })
}

#[tauri::command]
pub fn prune_data(state: tauri::State<'_, Arc<AppState>>, days: u32, device_ids: Option<Vec<String>>) -> Result<usize, String> {
    let ids = device_ids.unwrap_or_default();
    let n = if ids.is_empty() {
        state.db.prune_old(days, None).map_err(|e| e.to_string())?
    } else {
        let mut total = 0usize;
        for id in &ids {
            total += state.db.prune_old(days, Some(id.as_str())).map_err(|e| e.to_string())?;
        }
        total
    };
    if ids.is_empty() {
        let _ = state.db.set_setting("data_retention_days", &days.to_string());
    }
    Ok(n)
}

#[tauri::command]
pub fn list_devices_with_stats(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<crate::db::DeviceStats>, String> {
    let current_id = state.device_id.clone();
    let current_name = state.db.get_setting("device_name").unwrap_or_else(|| current_id.clone());
    state.db.list_devices_with_stats(&current_id, &current_name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn check_permissions() -> PermissionStatus {
    #[cfg(target_os = "macos")]
    {
        PermissionStatus {
            accessibility: crate::tracker::macos::is_accessibility_trusted(),
            screen_capture: crate::tracker::macos::is_screen_capture_trusted(),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        PermissionStatus { accessibility: true, screen_capture: true }
    }
}

#[tauri::command]
pub fn open_privacy_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility").spawn();
    }
}

#[tauri::command]
pub fn get_rules(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<RuleOut>, String> {
    state.db.get_rules_out().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_rule(state: tauri::State<'_, Arc<AppState>>, field: String, match_type: String, pattern: String, category_id: String, priority: i32) -> Result<i64, String> {
    let id = state.db.insert_rule(&field, &match_type, &pattern, &category_id, priority).map_err(|e| e.to_string())?;
    reload_rules(&state);
    Ok(id)
}

#[tauri::command]
pub fn update_rule(state: tauri::State<'_, Arc<AppState>>, id: i64, field: String, match_type: String, pattern: String, category_id: String, priority: i32, enabled: bool) -> Result<bool, String> {
    state.db.update_rule(id, &field, &match_type, &pattern, &category_id, priority, enabled).map_err(|e| e.to_string())?;
    reload_rules(&state);
    Ok(true)
}

#[tauri::command]
pub fn delete_rule(state: tauri::State<'_, Arc<AppState>>, id: i64) -> Result<bool, String> {
    state.db.delete_rule(id).map_err(|e| e.to_string())?;
    reload_rules(&state);
    Ok(true)
}

#[tauri::command]
pub fn reclassify_all(state: tauri::State<'_, Arc<AppState>>) -> Result<usize, String> {
    let rules = state.rules.lock().unwrap_or_else(|e| e.into_inner());
    state.db.reclassify_all(&rules).map_err(|e| e.to_string())
}

fn reload_rules(state: &tauri::State<'_, Arc<AppState>>) {
    if let Ok(rules) = state.db.load_rules() {
        *state.rules.lock().unwrap_or_else(|e| e.into_inner()) = rules;
    }
}

#[tauri::command]
pub fn set_autostart(app: tauri::AppHandle, state: tauri::State<'_, Arc<AppState>>, enabled: bool) -> Result<bool, String> {
    let mgr = app.autolaunch();
    if enabled { mgr.enable().map_err(|e| e.to_string())?; } else { mgr.disable().map_err(|e| e.to_string())?; }
    state.db.set_setting("autostart", if enabled { "true" } else { "false" }).map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
pub fn is_autostart(app: tauri::AppHandle) -> Result<bool, String> {
    Ok(app.autolaunch().is_enabled().unwrap_or(false))
}

#[tauri::command]
pub fn get_autostart_pref(state: tauri::State<'_, Arc<AppState>>) -> Option<bool> {
    state.db.get_setting("autostart").map(|v| v == "true")
}

#[tauri::command]
pub fn get_devices(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<DeviceInfo>, String> {
    let id = state.device_id.clone();
    let name = state.db.get_setting("device_name").unwrap_or_else(|| id.clone());
    state.db.get_devices(&id, &name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_settings(state: tauri::State<'_, Arc<AppState>>) -> Result<SettingsOut, String> {
    let device_id = state.device_id.clone();
    let device_name = state.db.get_setting("device_name").unwrap_or_else(|| device_id.clone());
    let idle = *state.idle_threshold.lock().unwrap_or_else(|e| e.into_inner());
    let retention: u32 = state.db.get_setting("data_retention_days").and_then(|v| v.parse().ok()).unwrap_or(365);
    let autostart = state.db.get_setting("autostart").map(|v| v == "true").unwrap_or(false);
    Ok(SettingsOut { device_id, device_name, idle_threshold: idle, data_retention_days: retention, sample_interval: SAMPLE_INTERVAL, autostart })
}

#[tauri::command]
pub fn save_settings(state: tauri::State<'_, Arc<AppState>>, idle_threshold: u64, device_name: String, data_retention_days: u32) -> Result<bool, String> {
    *state.idle_threshold.lock().unwrap_or_else(|e| e.into_inner()) = idle_threshold;
    *state.device_name.lock().unwrap_or_else(|e| e.into_inner()) = device_name.clone();
    state.db.set_setting("idle_threshold", &idle_threshold.to_string()).map_err(|e| e.to_string())?;
    state.db.set_setting("device_name", &device_name).map_err(|e| e.to_string())?;
    state.db.set_setting(&format!("device_name:{}", state.device_id), &device_name).map_err(|e| e.to_string())?;
    state.db.set_setting("data_retention_days", &data_retention_days.to_string()).map_err(|e| e.to_string())?;
    Ok(true)
}

async fn sampling_loop(state: Arc<AppState>) {
    let mut ticker = tokio::time::interval(StdDuration::from_secs(SAMPLE_INTERVAL));
    let mut last_summary_minute: Option<i64> = None;
    let mut tick_in_minute: u32 = 0;
    let mut switch_in_minute: u32 = 0;
    let mut last_app_for_minute: String = String::new();
    loop {
        ticker.tick().await;
        if !*state.tracking.lock().unwrap_or_else(|e| e.into_inner()) { break; }
        let fg = match state.tracker.get_foreground_app() {
            Ok(a) => a,
            Err(e) => { tracing::warn!(error = %e, "采集前台应用失败"); continue; }
        };
        let idle = state.tracker.get_idle_seconds().unwrap_or(0);
        tracing::debug!(app = %fg.process_name, idle = idle, "foreground tick");
        let threshold = *state.idle_threshold.lock().unwrap_or_else(|e| e.into_inner());
        let now = Local::now();
        let current_minute = now.timestamp() / 60;
        if last_summary_minute != Some(current_minute) {
            if let Some(_m) = last_summary_minute {
                tracing::info!(ticks = tick_in_minute, switches = switch_in_minute, last_app = %last_app_for_minute, "采样循环分钟摘要");
            }
            last_summary_minute = Some(current_minute);
            tick_in_minute = 0;
            switch_in_minute = 0;
        }
        tick_in_minute += 1;
        if !last_app_for_minute.is_empty() && last_app_for_minute != fg.process_name { switch_in_minute += 1; }
        last_app_for_minute = fg.process_name.clone();
        let platform = platform_name();
        let category = {
            let user_cat = classify_app(&fg, &state.rules.lock().unwrap_or_else(|e| e.into_inner()));
            if user_cat != "other" {
                user_cat
            } else {
                if let Some(c) = crate::categorizer::lookup_local_only(&fg.process_name, fg.exe_path.as_deref(), &fg.name) {
                    c
                } else {
                    let pn = fg.process_name.clone();
                    let nm = fg.name.clone();
                    let ep = fg.exe_path.clone();
                    let cache = state.category_cache.clone();
                    let other = tauri::async_runtime::spawn_blocking(move || crate::categorizer::lookup_category(&pn, ep.as_deref(), &nm, &cache)).await.unwrap_or_else(|e| { tracing::error!(error = %e, "spawn_blocking(lookup_category) 失败"); "other".to_string() });
                    other
                }
            }
        };
        let to_finalize: Option<ActiveSession> = {
            let mut cur = state.current.lock().unwrap_or_else(|e| e.into_inner());
            let finalize = match cur.as_ref() {
                None => false,
                Some(a) => {
                    let same = a.app.process_name == fg.process_name && a.app.exe_path == fg.exe_path;
                    let date_changed = a.started_at.date_naive() != now.date_naive();
                    !same || idle >= threshold || date_changed
                }
            };
            if finalize { cur.take() } else { if let Some(a) = cur.as_mut() { a.last_input_at = now; } None }
        };
        if let Some(active) = to_finalize { finalize_active_session(&state, &active, now); }
        {
            let mut cur = state.current.lock().unwrap_or_else(|e| e.into_inner());
            if cur.is_none() {
                let process_name = fg.process_name.clone();
                let app_id = match state.db.upsert_app(&fg.name, &fg.process_name, fg.exe_path.as_deref(), &category, platform) {
                    Ok(id) => id,
                    Err(e) => { tracing::error!(error = %e, process = %fg.process_name, name = %fg.name, "upsert_app 失败，跳过本 tick"); continue; }
                };
                *cur = Some(ActiveSession { app: fg, app_id, category_id: category.clone(), started_at: now, last_input_at: now });
                let already_covered = state.rules.lock().unwrap().iter().any(|r| r.enabled && r.field == "process_name" && r.pattern == process_name);
                if !already_covered {
                    let st = Arc::clone(&state);
                    let pn = process_name.clone();
                    let cat = category.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Ok(rule_id) = st.db.insert_rule("process_name", "equals", &pn, &cat, 0) {
                            if let Ok(new_rules) = st.db.load_rules() {
                                if let Ok(mut guard) = st.rules.lock() { *guard = new_rules; }
                            }
                            let _ = rule_id;
                        }
                    });
                }
            }
        }
    }
}

impl From<AppError> for String {
    fn from(e: AppError) -> Self { e.to_string() }
}

fn period_ranges(period: &str) -> (String, String, String, String, String, String, String, String) {
    let now = Local::now();
    let today = now.date_naive();
    let to_local = |d: NaiveDate, h: u32, mi: u32, s: u32| -> DateTime<Local> { d.and_hms_opt(h, mi, s).and_then(|dt| dt.and_local_timezone(Local).latest()).unwrap_or_else(|| now) };
    let ymd = |y: i32, m: u32, d: u32| -> NaiveDate { NaiveDate::from_ymd_opt(y, m, d).unwrap_or(today) };
    if period == "month" {
        let y = today.year();
        let m = today.month();
        let cur_start = to_local(ymd(y, m, 1), 0, 0, 0);
        let cur_end = now;
        let (py, pm) = if m == 1 { (y - 1, 12) } else { (y, m - 1) };
        let prev_start = to_local(ymd(py, pm, 1), 0, 0, 0);
        let prev_last = last_day_of_month(py, pm);
        let prev_end = to_local(ymd(py, pm, prev_last), 23, 59, 59);
        let yoy_y = y - 1;
        let yoy_start = to_local(ymd(yoy_y, m, 1), 0, 0, 0);
        let yoy_last = last_day_of_month(yoy_y, m);
        let yoy_end = to_local(ymd(yoy_y, m, yoy_last), 23, 59, 59);
        (cur_start.to_rfc3339(), cur_end.to_rfc3339(), prev_start.to_rfc3339(), prev_end.to_rfc3339(), yoy_start.to_rfc3339(), yoy_end.to_rfc3339(), format!("{y}年{m}月"), format!("{py}年{pm}月"))
    } else {
        let wd = today.weekday().num_days_from_monday() as i64;
        let this_mon = today - Duration::days(wd);
        let cur_start = to_local(this_mon, 0, 0, 0);
        let cur_end = now;
        let prev_mon = this_mon - Duration::days(7);
        let prev_start = to_local(prev_mon, 0, 0, 0);
        let prev_end = to_local(this_mon, 0, 0, 0);
        (cur_start.to_rfc3339(), cur_end.to_rfc3339(), prev_start.to_rfc3339(), prev_end.to_rfc3339(), String::new(), String::new(), "本周".to_string(), "上周".to_string())
    }
}

fn last_day_of_month(year: i32, month: u32) -> u32 {
    let next_month = if month == 12 { 1 } else { month + 1 };
    let next_year = if month == 12 { year + 1 } else { year };
    let first_of_next = NaiveDate::from_ymd_opt(next_year, next_month, 1).unwrap();
    (first_of_next - Duration::days(1)).day()
}

fn today_str() -> String {
    let d = Local::now();
    format!("{}-{:02}-{:02}", d.year(), d.month(), d.day())
}

#[allow(dead_code)]
fn _weekday_marker(_: Weekday) {}

#[derive(serde::Serialize, Clone)]
pub struct Webview2Status {
    pub os: String,
    pub available: bool,
    pub version: String,
    pub hint: String,
}

#[tauri::command]
pub fn check_webview2(app: tauri::AppHandle) -> Webview2Status {
    #[cfg(target_os = "windows")]
    {
        let _ = app;
        let version = read_webview2_version();
        if let Some(v) = version {
            return Webview2Status { os: "windows".to_string(), available: true, version: v, hint: String::new() };
        }
        Webview2Status { os: "windows".to_string(), available: false, version: String::new(), hint: "未检测到 WebView2 运行时，请先安装 Microsoft Edge WebView2 Runtime（永驻版）后再运行本应用：\nhttps://developer.microsoft.com/en-us/microsoft-edge/webview2/".to_string() }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        Webview2Status { os: std::env::consts::OS.to_string(), available: true, version: "n/a".to_string(), hint: String::new() }
    }
}

#[cfg(target_os = "windows")]
fn read_webview2_version() -> Option<String> {
    let keys = [
        r"HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\ClientState\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        r"HKLM\SOFTWARE\Microsoft\EdgeUpdate\ClientState\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
    ];
    for key in keys {
        // v0.7.7：全部走 proc::hidden——否则首次启动会连续闪 3 个 reg 控制台黑框
        let output = crate::proc::hidden("reg")
            .args(["query", key, "/v", "pv"])
            .output()
            .ok()?;
        if !output.status.success() { continue; }
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            if line.trim_start().starts_with("pv") {
                if let Some(v) = line.split_whitespace().nth(2) { return Some(v.to_string()); }
            }
        }
    }
    let output = crate::proc::hidden("reg")
        .args(["query", r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Edge\BLBeacon", "/v", "version"])
        .output()
        .ok()?;
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            if line.trim_start().starts_with("version") {
                if let Some(v) = line.split_whitespace().nth(2) { return Some(v.to_string()); }
            }
        }
    }
    None
}

#[tauri::command]
pub fn open_webview2_download() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    { open::that("https://developer.microsoft.com/en-us/microsoft-edge/webview2/").map_err(|e| e.to_string())?; }
    Ok(())
}

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(format!("不允许的 URL 协议：{}", url));
    }
    #[cfg(target_os = "windows")]
    { open::that(&url).map_err(|e| format!("打开 URL 失败：{}", e)) }
    #[cfg(target_os = "macos")]
    { std::process::Command::new("open").arg(&url).spawn().map_err(|e| format!("打开 URL 失败：{}", e))?; Ok(()) }
    #[cfg(target_os = "linux")]
    { std::process::Command::new("xdg-open").arg(&url).spawn().map_err(|e| format!("打开 URL 失败：{}", e))?; Ok(()) }
}

#[derive(serde::Serialize, Clone)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub has_update: bool,
    pub url: String,
    pub notes: String,
}

#[tauri::command]
pub async fn check_for_update(app: tauri::AppHandle) -> Result<UpdateInfo, String> {
    let current = app.config().version.clone().unwrap_or_else(|| "0.0.0".to_string());
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).user_agent("ScreenTime-Pro-Update-Checker").build().map_err(|e| format!("构建 HTTP 客户端失败: {}", e))?;
    let mut latest_info: Option<(String, String, String)> = None;
    let mut last_err: Option<String> = None;
    match client.get("https://github.com/Slk90s/screentime-pro/releases.atom").send().await {
        Ok(resp) if resp.status().is_success() => {
            match resp.text().await {
                Ok(xml) => {
                    if let Some(info) = parse_atom_latest_release(&xml) { latest_info = Some(info); }
                    else { last_err = Some("Atom feed 解析失败（未找到 release 条目）".to_string()); }
                }
                Err(e) => last_err = Some(format!("Atom feed 读取失败: {}", e)),
            }
        }
        Ok(resp) => last_err = Some(format!("Atom feed 返回 HTTP {}", resp.status().as_u16())),
        Err(e) => last_err = Some(format!("Atom feed 请求失败: {}", e)),
    }
    if latest_info.is_none() {
        match client.get("https://api.github.com/repos/Slk90s/screentime-pro/releases/latest").header("Accept", "application/vnd.github+json").send().await {
            Ok(resp) if resp.status().is_success() => {
                #[derive(serde::Deserialize)] struct GhRelease { tag_name: String, html_url: String, body: Option<String> }
                match resp.json::<GhRelease>().await {
                    Ok(gh) => {
                        let latest = gh.tag_name.trim_start_matches('v').to_string();
                        latest_info = Some((latest, gh.html_url, gh.body.unwrap_or_default()));
                    }
                    Err(e) => last_err = Some(format!("REST API 解析失败: {}", e)),
                }
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let body_short: String = body.chars().take(200).collect();
                let body_short = if body.len() > body_short.len() { format!("{}…", body_short) } else { body };
                last_err = Some(format!("REST API 返回 HTTP {}：{}", status.as_u16(), body_short));
            }
            Err(e) => last_err = Some(format!("REST API 请求失败: {}", e)),
        }
    }
    let (latest, url, notes) = match latest_info {
        Some(v) => v,
        None => return Err(last_err.unwrap_or_else(|| "未知错误：所有检查更新策略均失败".to_string())),
    };
    let has_update = semver_gt(&latest, &current);
    Ok(UpdateInfo { current, latest, has_update, url, notes })
}

fn parse_atom_latest_release(xml: &str) -> Option<(String, String, String)> {
    use regex::Regex;
    let entry_re = Regex::new(r"<entry[\s\S]*?</entry>").ok()?;
    let entry = entry_re.find(xml)?;
    let block = entry.as_str();
    let title_re = Regex::new(r"<title[^>]*>([\s\S]*?)</title>").ok()?;
    let raw_title = title_re.captures(block)?.get(1)?.as_str().trim().to_string();
    let tag = raw_title.trim_start_matches("ScreenTime Pro ").trim_start_matches("Release ").trim_start_matches("release ").trim().trim_start_matches('v').to_string();
    let link_re = Regex::new(r#"<link[^>]*href="([^"]+)""#).ok()?;
    let url = link_re.captures(block)?.get(1)?.as_str().to_string();
    let sum_re = Regex::new(r"<summary[^>]*>([\s\S]*?)</summary>").ok()?;
    let notes = sum_re.captures(block).and_then(|c| c.get(1)).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
    if tag.is_empty() { return None; }
    Some((tag, url, notes))
}

fn semver_gt(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> { s.split('.').filter_map(|p| p.split('-').next().unwrap_or("").parse::<u32>().ok()).collect() };
    let l = parse(latest);
    let c = parse(current);
    for i in 0..3 {
        let lv = l.get(i).copied().unwrap_or(0);
        let cv = c.get(i).copied().unwrap_or(0);
        if lv > cv { return true; }
        if lv < cv { return false; }
    }
    false
}

// ===== v0.7.6：状态栏配置（取代 v0.7.5 的 metrics_enabled 总开关）=====
// 4 个 key 落 settings 表：
//   - status_bar_enabled   → 启用状态栏（替换旧 statusbar_metrics_enabled）
//   - status_bar_show_cpu  → 是否显示 CPU 占用
//   - status_bar_show_mem  → 是否显示内存占用
//   - status_bar_show_net  → 是否显示网速
// 默认值：enabled 默认**关闭**（首启用户需手动打开状态栏；见 load()，其从旧
//   statusbar_metrics_enabled 迁移且缺省 false）；CPU/内存/网速三个子项默认全部开启。
//   旧 statusbar_metrics_enabled 用户的选择会迁移到 status_bar_enabled。
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct StatusBarConfig {
    pub enabled: bool,
    pub show_cpu: bool,
    pub show_mem: bool,
    // v0.7.7（2026-09-10）：磁盘占用开关。此前磁盘采样只有 macOS 有，且没有任何开关，
    // 界面上完全不可见；补齐三平台采样后新增本项（默认关，避免浮窗默认过长）
    pub show_disk: bool,
    pub show_net: bool,
    // v0.7.6（2026-09-10）：悬浮指标条独立开关（与托盘状态栏总开关互不依赖，
    // 全屏时由采样线程自动隐藏，见 system_load/fullscreen.rs）
    pub float_enabled: bool,
}

impl Default for StatusBarConfig {
    fn default() -> Self {
        // enabled=false 与 load() 首启行为一致（状态栏默认关闭，用户主动开启）
        Self {
            enabled: false,
            show_cpu: true,
            show_mem: true,
            show_disk: false,
            show_net: true,
            float_enabled: false,
        }
    }
}

impl StatusBarConfig {
    pub fn load(db: &crate::db::AppDb) -> Self {
        // 迁移：优先读新 key `status_bar_enabled`；缺失时回退旧 key `statusbar_metrics_enabled`
        // （旧 v0.7.5 仅写旧 key）。两者都缺失 → 默认 false（状态栏默认关闭，用户主动开启）。
        let enabled = db
            .get_setting("status_bar_enabled")
            .or_else(|| db.get_setting("statusbar_metrics_enabled"))
            .map(|s| s == "true")
            .unwrap_or(false);
        let show_cpu = db
            .get_setting("status_bar_show_cpu")
            .map(|s| s == "true")
            .unwrap_or(true);
        let show_mem = db
            .get_setting("status_bar_show_mem")
            .map(|s| s == "true")
            .unwrap_or(true);
        // v0.7.7 新增：旧版本 DB 无此 key → 默认 false（磁盘默认不显示）
        let show_disk = db
            .get_setting("status_bar_show_disk")
            .map(|s| s == "true")
            .unwrap_or(false);
        let show_net = db
            .get_setting("status_bar_show_net")
            .map(|s| s == "true")
            .unwrap_or(true);
        let float_enabled = db
            .get_setting("status_bar_float_enabled")
            .map(|s| s == "true")
            .unwrap_or(false);
        Self {
            enabled,
            show_cpu,
            show_mem,
            show_disk,
            show_net,
            float_enabled,
        }
    }

    pub fn save(&self, db: &crate::db::AppDb) -> rusqlite::Result<()> {
        let b = |v: bool| if v { "true" } else { "false" };
        db.set_setting("statusbar_metrics_enabled", b(self.enabled))?;
        db.set_setting("status_bar_enabled", b(self.enabled))?;
        db.set_setting("status_bar_show_cpu", b(self.show_cpu))?;
        db.set_setting("status_bar_show_mem", b(self.show_mem))?;
        db.set_setting("status_bar_show_disk", b(self.show_disk))?;
        db.set_setting("status_bar_show_net", b(self.show_net))?;
        db.set_setting("status_bar_float_enabled", b(self.float_enabled))?;
        Ok(())
    }
}

#[tauri::command]
pub fn get_status_bar_config(state: tauri::State<'_, Arc<AppState>>) -> StatusBarConfig {
    *state.status_bar_config.lock().unwrap_or_else(|e| e.into_inner())
}

#[tauri::command]
pub fn set_status_bar_config(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    config: StatusBarConfig,
) -> Result<bool, String> {
    // v0.7.6：记录旧可见态，判断是否需要显隐浮窗。
    // 2026-09-10 总开关统管：浮窗可见 = enabled && float_enabled，
    // 任一翻转导致可见态变化都要动窗口（含「关总开关 → 浮窗立即隐藏」）
    let (old_enabled, old_float) = {
        let c = state
            .status_bar_config
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        (c.enabled, c.float_enabled)
    };
    config.save(&state.db).map_err(|e| e.to_string())?;
    *state.status_bar_config.lock().unwrap_or_else(|e| e.into_inner()) = config;
    // v0.7.6：设置页改动 → 托盘菜单勾选态即时同步（防「页面改了、菜单还挂旧勾」）。
    // 2026-09-10 精简：托盘菜单只剩悬浮指标条一个勾选项，sync 只刷它
    if let Some(toggles) = app.try_state::<crate::TrayFloatToggle>() {
        toggles.sync(&config);
    }
    // v0.7.6：浮窗可见态翻转 → 显隐浮窗（幂等创建；透明空窗 show 不可见，无白闪）
    let old_shown = old_enabled && old_float;
    let new_shown = config.enabled && config.float_enabled;
    if old_shown != new_shown {
        crate::float_window::set_float_visible(&app, new_shown)
            .map_err(|e| e.to_string())?;
    }
    Ok(true)
}

#[tauri::command]
pub fn get_system_metrics(state: tauri::State<'_, Arc<AppState>>) -> MetricsOut {
    let cfg = *state.status_bar_config.lock().unwrap_or_else(|e| e.into_inner());
    let snap = state.metrics_sampler.sample_all();
    MetricsOut {
        supported: true,
        enabled: cfg.enabled,
        cpu_usage: snap.cpu_usage,
        memory_usage: snap.memory_usage,
        memory_used_bytes: snap.memory_used_bytes,
        memory_total_bytes: snap.memory_total_bytes,
        disk_usage: snap.disk_usage,
        disk_used_bytes: snap.disk_used_bytes,
        disk_total_bytes: snap.disk_total_bytes,
        net_rx_bps: snap.net_rx_bps,
        net_tx_bps: snap.net_tx_bps,
    }
}

// ===== v0.7.5 兼容 IPC：set/get_system_metrics_enabled 改为操作新 config（enabled 字段）=====
#[tauri::command]
pub fn set_system_metrics_enabled(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    enabled: bool,
) -> Result<bool, String> {
    // 2026-09-10 总开关统管：enabled 翻转影响浮窗可见态（关总开关 → 开着的浮窗立即隐藏）
    let (old_enabled, old_float) = {
        let c = state.status_bar_config.lock().unwrap_or_else(|e| e.into_inner());
        (c.enabled, c.float_enabled)
    };
    let mut cfg = *state.status_bar_config.lock().unwrap_or_else(|e| e.into_inner());
    cfg.enabled = enabled;
    cfg.save(&state.db).map_err(|e| e.to_string())?;
    *state.status_bar_config.lock().unwrap_or_else(|e| e.into_inner()) = cfg;
    // v0.7.6：旧兼容命令同样同步托盘菜单勾选态（2026-09-10 精简后仅悬浮指标条）
    if let Some(toggles) = app.try_state::<crate::TrayFloatToggle>() {
        toggles.sync(&cfg);
    }
    let old_shown = old_enabled && old_float;
    let new_shown = cfg.enabled && cfg.float_enabled;
    if old_shown != new_shown {
        crate::float_window::set_float_visible(&app, new_shown).map_err(|e| e.to_string())?;
    }
    Ok(true)
}

#[tauri::command]
pub fn get_system_metrics_enabled(state: tauri::State<'_, Arc<AppState>>) -> bool {
    state.status_bar_config.lock().unwrap_or_else(|e| e.into_inner()).enabled
}
