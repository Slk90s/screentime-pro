//! Tauri 命令层（前端通过 `invoke` 调用）
//!
//! ## 修改历史
//!
//! - 2026-10-23 @v1.2.6:
//!   ① `sampling_loop` 采集失败改为「状态翻转才记一条」——旧实现每 tick（2s）都打 WARN，
//!      而「锁屏 / 桌面 / 自家辅助窗」都是**正常状态**，实测累计 9376 条占全部 WARN 的 98%。
//!      现在 `NoForeground` 降为 DEBUG，真异常仍 WARN 但同类不重复，恢复时补一条 INFO。
//!   ② `finalize_active_session` 的 `let _ = insert_session(...)` 改为记录 error 日志：
//!      写库失败原先完全静默，用户只会发现「今天时长比实际少」却毫无线索。

use crate::AppState;
use crate::classifier::classify_app;
use crate::db::{
    AppRankingOut, CategoryOut, CurrentForegroundOut, DailySummaryOut, DayCategoryOut, DeviceInfo,
    ExportBundle, ExportResult, HourlyBucketOut, MetricsOut, MonthSummaryOut, OverviewOut,
    PermissionStatus, PeriodStat, RuleOut, SettingsOut, TrendsOut,
};
use crate::tracker::{platform_name, RawApp};
// v1.2.6：采样循环需要区分 NoForeground（正常状态）与真实错误，故引入错误类型
use crate::error::TrackerError;
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tauri::Manager;
use tauri_plugin_autostart::ManagerExt;

/// 命令层跨平台分支（reveal / open_url / 权限 / WebView2）统一收口层。
/// 各平台实现见 `platform/{macos,windows,linux}.rs`，宿主命令只调 `Backend::*`。
mod platform;
use crate::commands::platform::{OsOpenBackend, PermissionBackend, Webview2Backend};

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
            // v1.2.6：原为 `let _ = insert_session(...)` —— 写库失败完全静默，
            // 用户只会发现「今天时长比实际少」却找不到任何线索。
            // 配合 db::open 里新设的 busy_timeout(5s)，绝大多数瞬时锁冲突会自行重试成功；
            // 到这里仍失败就是真异常（磁盘满 / 库损坏 / 权限），必须留痕。
            if let Err(e) = state.db.insert_session(
                active.app_id,
                &active.category_id,
                &seg_start.to_rfc3339(),
                &seg_end.to_rfc3339(),
                effective,
                &date,
                active.app.window_title.as_deref(),
                &state.device_id,
            ) {
                tracing::error!(
                    error = %e,
                    app = %active.app.process_name,
                    date = %date,
                    duration = effective,
                    "会话落库失败（该段时长将缺失）"
                );
            }
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
    crate::commands::platform::Backend::reveal(&path)
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
    crate::logging::audit(
        "device_backup_purge",
        &format!(
            "device_id={device_id} deleted_sessions={deleted} backup={}",
            backup_path.display()
        ),
    );
    Ok(BackupAndPruneResult { backup_path: backup_path.to_string_lossy().to_string(), deleted_count: deleted })
}

#[tauri::command]
pub fn import_data(app: tauri::AppHandle, content: String) -> Result<usize, String> {
    let bundle: ExportBundle = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let state = app.state::<Arc<AppState>>();
    let imported = state.db.import_data(&bundle).map_err(|e| e.to_string())?;
    // P2-9：导入若含**当前设备**的名字映射，同步刷新裸键 `device_name` 与内存态，
    // 消除「备份还原后 device_name 与 device_name:<id> 短暂不一致」的歧义。
    // 只有 bundle.devices 里恰好命中本机 device_id 才写（跨设备合并导入不会误改本机名）。
    if let Some(name) = bundle.devices.get(&state.device_id) {
        state
            .db
            .set_setting("device_name", name)
            .map_err(|e| e.to_string())?;
        *state.device_name.lock().unwrap_or_else(|e| e.into_inner()) = name.clone();
    }
    Ok(imported)
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
    crate::logging::audit(
        "data_pruned",
        &format!("days={days} device_scope={} deleted_rows={n}", if ids.is_empty() { "all".to_string() } else { ids.join(",") }),
    );
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
    crate::commands::platform::Backend::check_permissions()
}

/// 打开 macOS「隐私与安全性」中的指定面板。
///
/// `pane` 取值：`"screen_capture"` → 屏幕录制；其余（含缺省）→ 辅助功能。
///
/// v0.9.1：加 `pane` 参数。此前硬编码 `Privacy_Accessibility`，
/// 而**截图**要的是「屏幕录制」权限 —— 引导跳过去会落在错误的页面，
/// 用户在那个页面里怎么找都找不到本应用。
#[tauri::command]
pub fn open_privacy_settings(pane: Option<String>) {
    crate::commands::platform::Backend::open_privacy_settings(pane)
}

/// 重启本应用（退出并重新拉起进程）。
///
/// 为什么需要它：macOS 的 TCC 授权（屏幕录制 / 辅助功能）**只对本进程之后新启动的进程生效**。
/// 用户在系统设置里把开关打开后，当前正在运行的进程拿不到新授权 —— 表现是
/// 「系统设置里明明是开着的，应用却一直说没权限」，用户只能自己 Cmd+Q 再打开，很容易卡成死循环。
/// 权限提示弹窗上放一个「重启应用」按钮，一键就把这个循环打通。
///
/// 实现走 Tauri 的 `AppHandle::restart()`（触发 ExitRequested / Exit 后重新拉起进程）。
/// 它在非主线程调用时会先请求退出再重启，本命令由 IPC 线程进入，正好走这条分支。
#[tauri::command]
pub fn restart_app(app: tauri::AppHandle) {
    tracing::info!("用户从界面触发应用重启（TCC 新授权需要新进程才生效）");
    app.restart();
}

/// 重置本应用指定 TCC 服务的授权记录（仅 macOS 有意义）。
///
/// 为什么需要：授权记录一旦指向**旧版本的代码签名指纹**（未签名的包每次升级都会这样），
/// 系统设置界面是清不掉的 —— 用户把开关关了再开、或把条目按「−」移除再加，
/// 系统写进去的往往仍是旧身份，于是「开关是开的、应用还说没权限」的循环解不开。
/// `tccutil reset <service> <bundle id>` 是 Apple 官方支持的清除方式：清掉后
/// 下一次请求会重新弹授权框、重新写入一条干净的记录。
///
/// v1.2.11（2026-10-27）：加 `pane` 参数。原来只清屏幕录制（`ScreenCapture`），
/// 但界面上「辅助功能未授权」的提示也劝用户点「重置并重启」—— 那个动作清的是
/// 另一个服务，用户白忙。`pane` 与 `open_privacy_settings` 用同一套词汇：
/// `"screen_capture"` → `ScreenCapture`；`"accessibility"` → `Accessibility`；
/// 缺省按屏幕录制（保持既有调用点行为不变）。
///
/// ⚠️ 只重置本应用（显式传了 bundle id），不影响其它 App；正常无需 sudo。
/// 前端把它和「重启应用」串成一个按钮 —— 重置后必须重启进程，新授权才对运行中的进程生效。
#[tauri::command]
pub fn reset_permission(pane: Option<String>) -> Result<String, String> {
    crate::commands::platform::Backend::reset_permission(pane)
}

/// 兼容旧名：等价于 `reset_permission(Some("screen_capture"))`。
///
/// 为什么保留：v1.2.10 及更早的前端只认这个名字。前端与后端在桌面端是**同包发布**
/// 的，理论上不会错配；但开发期 `cargo run` 配旧 dist、或用户手工替换二进制时会，
/// 保留一个薄别名比让按钮静默报「命令不存在」好。
#[tauri::command]
pub fn reset_screen_capture_permission() -> Result<String, String> {
    reset_permission(Some("screen_capture".to_string()))
}

#[tauri::command]
pub fn get_rules(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<RuleOut>, String> {
    state.db.get_rules_out().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_rule(state: tauri::State<'_, Arc<AppState>>, field: String, match_type: String, pattern: String, category_id: String, priority: i32, enabled: bool) -> Result<i64, String> {
    let id = state.db.insert_rule(&field, &match_type, &pattern, &category_id, priority, enabled).map_err(|e| e.to_string())?;
    crate::logging::audit("rule_added", &format!("field={field} match_type={match_type} pattern={pattern} category={category_id} enabled={enabled}"));
    reload_rules(&state);
    Ok(id)
}

#[tauri::command]
pub fn update_rule(state: tauri::State<'_, Arc<AppState>>, id: i64, field: String, match_type: String, pattern: String, category_id: String, priority: i32, enabled: bool) -> Result<bool, String> {
    state.db.update_rule(id, &field, &match_type, &pattern, &category_id, priority, enabled).map_err(|e| e.to_string())?;
    crate::logging::audit("rule_updated", &format!("id={id} field={field} pattern={pattern}"));
    reload_rules(&state);
    Ok(true)
}

#[tauri::command]
pub fn delete_rule(state: tauri::State<'_, Arc<AppState>>, id: i64) -> Result<bool, String> {
    state.db.delete_rule(id).map_err(|e| e.to_string())?;
    crate::logging::audit("rule_deleted", &format!("id={id}"));
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
pub fn get_autostart_pref(state: tauri::State<'_, Arc<AppState>>) -> Option<bool> {
    state.db.get_setting("autostart").map(|v| v == "true")
}

/// 列出所有设备（首页「全部设备」切换器用）。
///
/// v0.7.9（2026-09-12）加固（Ryan 反馈 Windows 首页只剩「全部设备」一个按钮）：
///   ① **名称兜底**：旧写法 `get_setting(..).unwrap_or_else(|| id.clone())` 只区分
///      `Some/None`——若设置里存的是**空串/纯空白**（用户把「本机设备名称」清空后保存过），
///      会原样返回空字符串，前端按钮就渲染成一块空白。现把空白也视为「没名字」，回落设备 id。
///   ② **加日志**：成功/失败都记一行。出问题时能在应用日志里区分
///      「前端根本没调用」（无此行）与「调用了但查询报错」（有 error 行），无需再靠猜。
#[tauri::command]
pub fn get_devices(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<DeviceInfo>, String> {
    let id = state.device_id.clone();
    let name = state
        .db
        .get_setting("device_name")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| id.clone());
    let res = state.db.get_devices(&id, &name);
    match &res {
        Ok(list) => tracing::info!(
            device_id = %id,
            device_name = %name,
            count = list.len(),
            "get_devices 返回"
        ),
        Err(e) => tracing::error!(device_id = %id, error = %e, "get_devices 查询失败"),
    }
    res.map_err(|e| e.to_string())
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
    crate::logging::audit("settings_saved", &format!("idle_threshold={idle_threshold} data_retention_days={data_retention_days}"));
    Ok(true)
}

async fn sampling_loop(state: Arc<AppState>) {
    let mut ticker = tokio::time::interval(StdDuration::from_secs(SAMPLE_INTERVAL));
    let mut last_summary_minute: Option<i64> = None;
    let mut tick_in_minute: u32 = 0;
    let mut switch_in_minute: u32 = 0;
    let mut last_app_for_minute: String = String::new();
    // v1.2.6：采集失败「只在状态翻转时记一条」。
    //
    // 旧实现在每 tick 失败都打 WARN，而采样间隔是 2 秒 → 锁屏 / 无前台窗口 /
    // 自家辅助窗抢前台（v1.2.1 的 SELF_HELPER_TITLES 过滤）这些**正常状态**
    // 会以 1800 条/小时的速度灌日志：实测累计 8288 条「采集前台应用失败」
    // 占全部 WARN 的 99%。真正需要告警的是「非预期错误」，不是「暂时没有前台」。
    //
    // 现在：`NoForeground` 降为 debug（属于正常状态）；其它错误仍 WARN；
    // 两种都做去重 —— 连续同类失败只记第一条，恢复时补一条「已恢复」。
    let mut last_fg_err: Option<String> = None;
    loop {
        ticker.tick().await;
        if !*state.tracking.lock().unwrap_or_else(|e| e.into_inner()) { break; }
        let fg = match state.tracker.get_foreground_app() {
            Ok(a) => {
                if let Some(prev) = last_fg_err.take() {
                    tracing::info!(recovered_from = %prev, "前台采集已恢复");
                }
                a
            }
            Err(TrackerError::NoForeground) => {
                // 正常状态（锁屏 / 桌面 / 自家辅助窗）：仅首次记 debug
                if last_fg_err.is_none() {
                    tracing::debug!("当前无前台应用（锁屏或桌面），暂停采集");
                    last_fg_err = Some("no_foreground".to_string());
                }
                continue;
            }
            Err(e) => {
                // 真异常：首次 WARN，之后同类连续失败只 debug，不刷屏
                let key = e.to_string();
                if last_fg_err.as_deref() != Some(key.as_str()) {
                    tracing::warn!(error = %key, "采集前台应用失败");
                    last_fg_err = Some(key);
                }
                continue;
            }
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
                let already_covered = state.rules.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|r| r.enabled && r.field == "process_name" && r.pattern == process_name);
                if !already_covered {
                    let st = Arc::clone(&state);
                    let pn = process_name.clone();
                    let cat = category.clone();
                    tauri::async_runtime::spawn(async move {
                        // v0.9.2：改用 ensure_rule（存在则跳过插入）——
                        // 既天然幂等（并发也不会重复），又不会把用户手动禁用的自动规则重新启用。
                        if let Ok(_inserted) = st.db.ensure_rule("process_name", "equals", &pn, &cat, 0) {
                            if let Ok(new_rules) = st.db.load_rules() {
                                if let Ok(mut guard) = st.rules.lock() { *guard = new_rules; }
                            }
                        }
                    });
                }
            }
        }
    }
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

#[derive(serde::Serialize, Clone)]
pub struct Webview2Status {
    pub os: String,
    pub available: bool,
    pub version: String,
    pub hint: String,
}

#[tauri::command]
pub fn check_webview2(app: tauri::AppHandle) -> Webview2Status {
    crate::commands::platform::Backend::check(&app)
}

#[tauri::command]
pub fn open_webview2_download() -> Result<(), String> {
    crate::commands::platform::Backend::open_download()
}

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(format!("不允许的 URL 协议：{}", url));
    }
    crate::commands::platform::Backend::open_url(&url)
}

#[derive(serde::Serialize, Clone)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub has_update: bool,
    pub url: String,
    pub notes: String,
}

/// REST API 非 2xx 响应正文写进日志时的截断长度（**字符**数，非字节）。
///
/// 只用于 `tracing::warn!` 的错误摘要 —— GitHub 的 403 正文可能带整页 HTML
/// （限流页约 1.2 KB），原样打进日志会刷屏。
/// ⚠️ 按**字符**截断（`chars().take`）而不是字节：正文可能是中文或含代理对，
/// 按字节切会从 UTF-8 序列中间断开，产出非法字符串。
/// （Rust 的 `String::truncate` 正是按字节切且非 char 边界会 **panic**，不能用。）
const API_ERROR_BODY_CHARS: usize = 200;

/// 抓取远端最新版本，返回 `(latest, url, notes)`。
///
/// 三通道，全部失败才算失败：
///   ① GitHub releases.atom（不走 API 限额）→ ② GitHub REST API → ③ Gitee 镜像。
///
/// 抽成独立函数是为了让 `check_for_update`（手动）与 `auto_check_update`（自动）
/// 共用同一份逻辑——两份拷贝迟早会漂移（比如只给其中一处加 Rustls 代理）。
///
/// v1.2.10（2026-10-26）：加 ③ 与 `connect_timeout`。GitHub 在国内是**连不上**
/// （TCP 阶段就被丢包），而 ①② 都指向 GitHub —— v1.2.7 → v1.2.8 那次「没检测到
/// 更新」的事故里，两条通道同时不可达是并列主因。Gitee 是本项目既有的发布落点
/// （OCR 运行库的下载源也在这里），可达性有保障。
/// 镜像只是**回落**：GitHub 任一条成功就不碰它，避免多一次跨境请求的延迟。
///
/// ⚠️ 「国内连不上」这个结论只对**直连**成立。第三个成因是代理：
/// `Cargo.toml` 里 reqwest 关掉了 `default-features`，连带丢了 `system-proxy`，
/// 于是它只认 `HTTPS_PROXY` 环境变量、**不读系统代理设置**。Clash/Surge 这类
/// GUI 客户端默认改的是系统代理而不是环境变量 → 用户以为"开着代理"，进程实际
/// 直连出境。已在 v1.2.10 的 Cargo.toml 补上 `system-proxy`。
/// 因此若再收到「检测不到更新」，先看日志里的通道来源：走 Gitee 说明是网络，
/// 三条都 `请求失败` 且用户开了代理 → 大概率是代理没被读到（旧版本）。
async fn fetch_latest_release() -> Result<(String, String, String), String> {
    // 为什么单独设 connect_timeout：只有整体 timeout 时，GitHub 被墙要**干等满 10 秒**
    // 才开始试下一条通道，三条串起来就是 30 秒的黑屏等待。连接阶段 5 秒就放弃，
    // 把时间留给真正可能成功的通道；读写阶段仍由整体 10 秒兜底。
    let client = reqwest::Client::builder()
        .connect_timeout(StdDuration::from_secs(5))
        .timeout(StdDuration::from_secs(10))
        .user_agent("ScreenTime-Pro-Update-Checker")
        .build()
        .map_err(|e| format!("构建 HTTP 客户端失败: {}", e))?;

    // ===== ① GitHub releases.atom =====
    // 每个通道用「match 求出 Option<错误>」而不是往同一个 mut 变量里赋值 ——
    // 后者会被编译器判为「初值从未被读」，也更容易漏掉分支。
    let atom_err: Option<String> =
        match client.get("https://github.com/Slk90s/screentime-pro/releases.atom").send().await {
            Ok(resp) if resp.status().is_success() => match resp.text().await {
                Ok(xml) => match parse_atom_latest_release(&xml) {
                    Some(info) => {
                        // 来源必须留痕：用户报「检测不到更新」时，日志一行就能区分
                        // 「走的哪条通道」—— 走 Atom 说明网络通、问题在版本比较；
                        // 走镜像说明 GitHub 不可达、问题在网络。两者处置完全不同。
                        tracing::info!(version = %info.0, "更新来源：GitHub Atom");
                        return Ok(info);
                    }
                    None => Some("Atom feed 解析失败（未找到合法版本条目）".to_string()),
                },
                Err(e) => Some(format!("Atom feed 读取失败: {}", e)),
            },
            Ok(resp) => Some(format!("Atom feed 返回 HTTP {}", resp.status().as_u16())),
            Err(e) => Some(format!("Atom feed 请求失败: {}", e)),
        };
    // v1.2.10：必须 warn 才能落盘（release 默认级别 info）。用户反馈「没检测到更新」时，
    // 这条日志是第一现场 —— 原来整条链路失败是静默的，根本无法判断请求发没发出去。
    tracing::warn!(error = atom_err.as_deref().unwrap_or("未知"), "检查更新：GitHub atom 通道失败，改试 GitHub API");

    // ===== ② GitHub REST API =====
    let api_err: Option<String> =
        match client.get("https://api.github.com/repos/Slk90s/screentime-pro/releases/latest").header("Accept", "application/vnd.github+json").send().await {
            Ok(resp) if resp.status().is_success() => {
                #[derive(serde::Deserialize)] struct GhRelease { tag_name: String, html_url: String, body: Option<String> }
                match resp.json::<GhRelease>().await {
                    Ok(gh) => {
                        // v1.2.10：与 atom 同口径校验。API 的 /releases/latest 会跳过 prerelease，
                        // 但「没标 prerelease 的资源型 release」仍会漏过来 —— 一旦漏过来前端就弹
                        // 「发现新版本 ocr-runtime」的死链。校验不过就当本通道失败，继续走镜像。
                        if is_version_tag(&gh.tag_name) {
                            let latest = gh.tag_name.trim().trim_start_matches(['v', 'V']).to_string();
                            tracing::info!(
                                version = %latest,
                                atom_error = atom_err.as_deref().unwrap_or("未知"),
                                "更新来源：GitHub API（Atom 通道已失败，原因见 atom_error）"
                            );
                            return Ok((latest, gh.html_url, gh.body.unwrap_or_default()));
                        }
                        Some(format!("REST API 返回的 tag 不是版本号：{}", gh.tag_name))
                    }
                    Err(e) => Some(format!("REST API 解析失败: {}", e)),
                }
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                // 先按字符截断再判是否真的截过 —— 判定用 `body.len() > body_short.len()`
                // 是安全的：截断后字节数必 ≤ 原字节数，等号只在「未截断」时成立。
                let body_short: String = body.chars().take(API_ERROR_BODY_CHARS).collect();
                let api_body =
                    if body.len() > body_short.len() { format!("{body_short}…") } else { body };
                Some(format!("REST API 返回 HTTP {}：{}", status.as_u16(), api_body))
            }
            Err(e) => Some(format!("REST API 请求失败: {}", e)),
        };
    tracing::warn!(error = api_err.as_deref().unwrap_or("未知"), "检查更新：GitHub API 通道失败，改试 Gitee 镜像");

    // ===== ③ Gitee 镜像 =====
    match fetch_gitee_latest(&client).await {
        Ok(v) => {
            // 用 warn 而不是 info：这条路径意味着「本应走 GitHub，实际没走成」，
            // 是网络异常的现场证据，排查时需要一眼看到。
            tracing::warn!(
                version = %v.0,
                url = %v.1,
                atom_error = atom_err.as_deref().unwrap_or("未知"),
                api_error = api_err.as_deref().unwrap_or("未知"),
                "GitHub 更新源不可达，已回落 Gitee 镜像"
            );
            Ok(v)
        }
        Err(e) => {
            tracing::warn!(
                atom_error = atom_err.as_deref().unwrap_or("未知"),
                api_error = api_err.as_deref().unwrap_or("未知"),
                gitee_error = %e,
                "检查更新：GitHub 与 Gitee 三个通道全部失败，本次无结果"
            );
            Err(format!(
                "GitHub atom 失败：{}；GitHub API 失败：{}；Gitee 镜像失败：{}",
                atom_err.as_deref().unwrap_or("未知"),
                api_err.as_deref().unwrap_or("未知"),
                e
            ))
        }
    }
}

/// Gitee 镜像的 release 列表接口（公开仓库，无需 token）。
const GITEE_RELEASES_API: &str =
    "https://gitee.com/api/v5/repos/create100/screentime-pro/releases?page=1&per_page=20";

/// 从 Gitee 镜像取最新**应用**版本。
///
/// ⚠️ 这里有个必须防的坑：本仓库 Gitee 侧的第一个 release 是 **`ocr-runtime`**
/// （`prerelease: true`）—— 它是 OCR 运行库的下载源，不是应用版本。不过滤的话镜像
/// 会把 `ocr-runtime` 当成最新版，前端弹出「发现新版本 ocr-runtime」的死链。
/// 双重过滤：`prerelease == true` 直接跳过；`tag_name` 必须过 `is_version_tag`。
async fn fetch_gitee_latest(client: &reqwest::Client) -> Result<(String, String, String), String> {
    #[derive(serde::Deserialize)]
    struct GiteeRelease {
        #[serde(default)]
        tag_name: String,
        #[serde(default)]
        body: Option<String>,
        #[serde(default)]
        prerelease: bool,
    }

    let resp = client
        .get(GITEE_RELEASES_API)
        .send()
        .await
        .map_err(|e| format!("镜像请求失败: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("镜像返回 HTTP {}", resp.status().as_u16()));
    }
    let list: Vec<GiteeRelease> = resp
        .json()
        .await
        .map_err(|e| format!("镜像 JSON 解析失败: {}", e))?;

    for r in &list {
        if r.prerelease {
            // 资源型 release（ocr-runtime）走这条 —— 记 info 便于事后核对跳过是否符合预期
            tracing::info!(tag = %r.tag_name, "检查更新：镜像条目为预发布/资源包，跳过");
            continue;
        }
        if !is_version_tag(&r.tag_name) {
            tracing::info!(tag = %r.tag_name, "检查更新：镜像 tag 不是版本号，跳过");
            continue;
        }
        let tag = r.tag_name.trim();
        let version = tag.trim_start_matches(['v', 'V']).to_string();
        // 用 release 详情页而不是 `/download/<tag>/<file>`：后者要拼平台资产名
        // （dmg / exe / deb / rpm 各不相同），拼不出来；详情页由用户在浏览器里选平台。
        // 排查提示：该地址对 curl 默认 UA 会收到 302（Gitee 对非浏览器 UA 降级），
        // 用浏览器 UA 请求才是 200 text/html —— 本 URL 只交给系统默认浏览器打开，不受影响。
        let url = format!("https://gitee.com/create100/screentime-pro/releases/tag/{tag}");
        return Ok((version, url, r.body.clone().unwrap_or_default()));
    }
    Err(format!("镜像 {} 条 release 中没有一个合法版本号", list.len()))
}

/// 校验 release tag 是否是版本号：可选 `v`/`V` 前缀，2~3 段数字，可带 `-beta` / `+build` 后缀。
///
/// 为什么要独立成一个函数：atom 解析与 Gitee 镜像都要用同一把尺子。两份拷贝
/// 迟早漂移（本项目已经栽过三次「同一判断写了两处」的跟头）。
fn is_version_tag(tag: &str) -> bool {
    let t = tag.trim();
    let core = t.strip_prefix('v').or_else(|| t.strip_prefix('V')).unwrap_or(t);
    // 预发布后缀与构建元数据不参与判定：`1.3.0-beta` 是合法 tag，主版本仍是 1.3.0
    let core = core.split('-').next().unwrap_or("");
    let core = core.split('+').next().unwrap_or("");
    let parts: Vec<&str> = core.split('.').collect();
    // 2~3 段：接受 `1.3`（缺位补 0 由 semver_gt 负责），拒绝 `1.2.8.1` 这类四段 tag
    if parts.len() < 2 || parts.len() > 3 {
        return false;
    }
    parts.iter().all(|p| p.parse::<u32>().is_ok())
}

#[tauri::command]
pub async fn check_for_update(app: tauri::AppHandle) -> Result<UpdateInfo, String> {
    let current = app.config().version.clone().unwrap_or_else(|| "0.0.0".to_string());
    let (latest, url, notes) = fetch_latest_release().await?;
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
    // v1.2.10：tag 必须过版本号校验，否则返回 None 让上层回落到 API / 镜像。
    // 为什么不能只 trim 前缀就往上走：本项目确有「非版本型 release」的用法
    // （`ocr-runtime` 就是 OCR 运行库的下载源）。一旦它被发到 GitHub 且发生在
    // 最新位置，atom feed 第一条就是它 —— 不校验的话前端会弹
    // 「发现新版本 ocr-runtime」的死链。返回 None 比返回脏 tag 安全。
    if !is_version_tag(&tag) { return None; }
    let link_re = Regex::new(r#"<link[^>]*href="([^"]+)""#).ok()?;
    let url = link_re.captures(block)?.get(1)?.as_str().to_string();
    let sum_re = Regex::new(r"<summary[^>]*>([\s\S]*?)</summary>").ok()?;
    let notes = sum_re.captures(block).and_then(|c| c.get(1)).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
    if tag.is_empty() { return None; }
    Some((tag, url, notes))
}

/// 严格版本比较：`latest` 是否严格大于 `current`。
///
/// v1.2.10（2026-10-26）：重写。旧实现用 `filter_map` **逐段丢弃**非法片段：
///
/// ```ignore
/// s.split('.').filter_map(|p| p.split('-').next()?.parse::<u32>().ok())
/// ```
///
/// 于是 `"v1.2.8"` → 第一段 `"v1"` 解析失败被丢 → 变成 `[2, 8]` → 被当成 2.8.0，
/// 和 `1.2.8` 比出来是「有新版本」。而 `tag_name` 带 `v` 前缀是本项目的常态
/// （GitHub 上的 tag 全是 `vX.Y.Z`），这个 bug 一旦被非 GitHub 通道的原始 tag
/// 触发，就会弹出并不存在的更新。
///
/// 现在的规则：
///   ① 先剥 `v` / `V` 前缀；
///   ② **整体**校验 —— 任一段解析失败就返回 `false`（保守：不认为有新版本。
///      宁可漏报一次，也不要弹一个点开是 404 的死链）；
///   ③ 缺位补 0（`1.3` == `1.3.0`）；只比前三段（`1.2.8.1` 视作 1.2.8）；
///   ④ 预发布后缀不参与比较（`1.3.0-beta` == `1.3.0`）。
///
/// ⚠️ 本函数被三处共用：`check_for_update`（手动）、`auto_check_update`（自动）、
/// 以及后者里的 dismissed 自愈判断。改它等于同时改三处行为 ——
/// 所以下面有覆盖这三类场景的单元测试。
fn semver_gt(latest: &str, current: &str) -> bool {
    let l = parse_version(latest);
    let c = parse_version(current);
    let (l, c) = match (l, c) {
        (Some(l), Some(c)) => (l, c),
        // 任一侧不是合法版本号 → 保守返回 false。典型输入：`ocr-runtime`（资源包名）、
        // 空串、`v`（只有一个前缀）。返回 true 会让前端弹出无对应下载的更新。
        _ => return false,
    };
    for i in 0..3 {
        let lv = l.get(i).copied().unwrap_or(0);
        let cv = c.get(i).copied().unwrap_or(0);
        if lv > cv { return true; }
        if lv < cv { return false; }
    }
    false
}

/// 解析版本号为三段数字（缺位不补，交给比较侧 `unwrap_or(0)` 统一补）。
///
/// 任一段不是数字 → 整体 `None`。**不做**「跳过非法段继续解析」——那正是旧实现
/// 把 `v1.2.8` 读成 `2.8.0` 的根因。
fn parse_version(s: &str) -> Option<Vec<u32>> {
    let t = s.trim();
    let core = t.strip_prefix('v').or_else(|| t.strip_prefix('V')).unwrap_or(t);
    // 预发布后缀与构建元数据不参与比较（`1.3.0-beta` 的主版本就是 1.3.0）
    let core = core.split('-').next().unwrap_or("");
    let core = core.split('+').next().unwrap_or("");
    if core.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for p in core.split('.') {
        // 空段（`1..3` / `1.2.`）同样视为非法，不能当 0
        out.push(p.parse::<u32>().ok()?);
    }
    if out.is_empty() { None } else { Some(out) }
}

// ===== v1.2.4（2026-10-22）：自动检查更新 =====
//
// 与手动 `check_for_update` 的分工：自动 =「告知」，手动 =「问询」。
//   - 自动：静默、节流、只在发现新版本且未被忽略时给前端一个横幅信息。
//   - 手动：用户点了按钮，必须给出明确结论（含错误），由 check_for_update 负责。
//
// 两条 settings 键（复用既有 settings 表，不新建表）：
//   - update_last_check_at   上次**成功**检查的时间戳（秒）。失败不写入，
//     否则一次断网就会把接下来 24 小时全部锁死（用户再没机会知道有新版本）。
//     v1.2.10（2026-10-26）：该时间戳改为**只在「确认远端无更新」或「发现新版本」
//     之后写入**，不再在「刚抓取成功、还没比版本」时提前写 —— 见 ③ 处说明。
//   - update_dismissed_version  用户点过「关闭」的版本号。只有版本号**严格大于**
//     它才再次提示——即「这个版本我不想装」，但下个版本仍然会提示。
//
// ⚠️ 为什么失败必须静默：断网、公司网络拦 GitHub、API 限流（未认证 60 次/小时/IP）
// 都是常态。给用户弹「检查更新失败」等于把基础设施问题伪装成软件故障，
// 用户既无法处理也不需要知道。失败只写日志，UI 完全不出现任何痕迹。
// ⚠️ 但「静默」≠「不留痕」：v1.2.10 起所有跳过/失败路径都提到 info/warn 级
//    （原为 debug，而 release 构建默认过滤级别是 info，等于完全不落盘）。
//    用户反馈「没自动检测到更新」时，第一步就是看日志里有没有这条命令的记录。

/// 自动检查的最小间隔：6 小时。
///
/// v1.2.10（2026-10-26）：由 24h 收紧到 6h。原因是 24h 与「应用常驻」叠加后会出现
/// 「永远错过」：用户 T 时刻启动检查一次（无新版）→ 写时间戳 → 16 小时后发新版 →
/// 用户 17 小时后重启应用 → elapsed < 24h 直接 return，**连网络请求都不发**。
/// 6h 让最坏情况从「错过一整天」压到「最多晚 6 小时」，同时仍远低于
/// GitHub 未认证 API 的 60 次/小时/IP 限额（每 6h 一次 = 每天 4 次）。
///
/// 取 6h 而不是「每次启动」的原因：重度用户一天会开十几次应用，
/// 不节流就是十几次无效请求。
const UPDATE_CHECK_INTERVAL_SECS: i64 = 6 * 60 * 60;

#[derive(serde::Serialize, Clone)]
pub struct AutoUpdateInfo {
    /// 是否需要向用户提示。
    /// **为 false 时**涵盖三种情况：已是最新 / 检查失败（静默）/ 该版本已被用户点过关闭。
    /// 前端只需判这一个字段，无需区分——它们对用户的表现完全一致：不显示横幅。
    pub has_update: bool,
    pub current: String,
    pub latest: String,
    pub url: String,
}

/// 启动后的静默检查。**永不返回 Err**——所有失败路径都折叠成 `has_update: false`。
///
/// 静默是刻意的：这个命令由前端在启动 15 秒后自动调用，用户没有"发起"这个动作，
/// 因此不应该看到任何失败提示（见上方注释）。
#[tauri::command]
pub async fn auto_check_update(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<AutoUpdateInfo, String> {
    let current = app.config().version.clone().unwrap_or_else(|| "0.0.0".to_string());
    let no_update = AutoUpdateInfo { has_update: false, current: current.clone(), latest: String::new(), url: String::new() };

    // ① 节流：距上次成功检查不足 6h 就跳过，不发请求
    let now = Local::now().timestamp();
    if let Some(last) = state.db.get_setting("update_last_check_at").and_then(|v| v.parse::<i64>().ok()) {
        // 负值（系统时间被回拨）也视为需要重新检查，避免永久锁死
        let elapsed = now - last;
        if elapsed >= 0 && elapsed < UPDATE_CHECK_INTERVAL_SECS {
            // v1.2.10：debug → info。这是「用户说没检测到更新」时最常见的答案，
            // 必须能落盘（release 默认级别 info，debug 会被直接丢弃）。
            tracing::info!(
                elapsed_secs = elapsed,
                interval_secs = UPDATE_CHECK_INTERVAL_SECS,
                "自动检查更新：节流窗口内已检查过，跳过"
            );
            return Ok(no_update);
        }
    }

    // ② 抓取。失败静默：写日志、不写时间戳（下个周期重试）、返回 no_update
    let (latest, url, _notes) = match fetch_latest_release().await {
        Ok(v) => v,
        Err(e) => {
            // v1.2.10：debug → warn。失败是**唯一会让用户看到「什么都没发生」**的分支，
            // 必须是 warn 才能进日志（原来 debug 在 release 下完全不落盘，
            // 导致「到底请求没请求」无法事后判断）。
            tracing::warn!(error = %e, "自动检查更新失败（静默忽略，不占节流窗口）");
            return Ok(no_update);
        }
    };

    // ③ 判有无新版本（**先判、再写时间戳**）
    //
    // v1.2.10 修正：原代码把「写时间戳」放在这里之前，等于「只要抓取成功就烧掉
    // 一整个节流窗口」——包括「已是最新」这种情况。配合 24h 窗口就是那个致命序列：
    //   T 时刻检查（无新版）→ 写 T → T+16h 发新版 → T+17h 用户重启 →
    //   elapsed < 24h 直接 return，连请求都不发。
    // 现在改成「确认过一次远端状态后才写」，语义是「上次**确认**过」，与键名一致。
    // 落点放在所有 return 之前（无新版 / 已 dismissed / 有新版本 三条都要写），
    // 否则「已是最新」会变成每次启动都请求（节流失效）。
    let has_new = semver_gt(&latest, &current);

    // ④ 用户已关闭过这个版本 → 不再提示（但更高版本仍会提示）
    //    必须先于「发现新版本」的 info 日志判断，否则日志会说「发现新版本」
    //    而实际返回 has_update:false，排查时被误导。
    let dismissed_blocked = if has_new {
        match state.db.get_setting("update_dismissed_version") {
            Some(raw) => {
                let dismissed = raw.trim().to_string();
                if dismissed.is_empty() {
                    false
                } else if !semver_gt(&dismissed, &current) {
                    // v1.2.10：**记录自愈**。`dismissed <= current` 说明用户已经
                    // 升级到（或越过）当初忽略的那个版本了，这条记录再无意义 → 清空。
                    //
                    // 为什么必须清：判断用的是「严格大于才提示」，只要键留着
                    // `dismissed == "1.2.8"`，而远端 latest 仍是 1.2.8（重切同名 tag、
                    // 或用户在预发布阶段点过关闭），该用户就**永远收不到这条提示**，
                    // 且没有任何 UI 能让他撤销这个状态（横幅本身已经不显示了）。
                    // 清空后下次检查即恢复正常。
                    tracing::info!(
                        dismissed = %dismissed,
                        current = %current,
                        "更新忽略记录已过期（用户已在该版本或更高），清除"
                    );
                    let _ = state.db.set_setting("update_dismissed_version", "");
                    false
                } else if !semver_gt(&latest, &dismissed) {
                    tracing::info!(latest = %latest, dismissed = %dismissed, "自动检查更新：该版本已被用户关闭，不再提示");
                    true
                } else {
                    false
                }
            }
            None => false,
        }
    } else {
        false
    };

    // ⑤ 到这里才算「本次确认完成」，写时间戳。
    //    ⚠️ 三条出口（无新版 / 已 dismissed / 有新版本）都会执行到这里，不要提前 return。
    match state.db.set_setting("update_last_check_at", &now.to_string()) {
        Ok(_) => tracing::info!(
            current = %current,
            latest = %latest,
            has_new,
            dismissed_blocked,
            "自动检查更新完成"
        ),
        Err(e) => tracing::warn!(error = %e, "写入 update_last_check_at 失败"),
    }

    if !has_new {
        return Ok(no_update);
    }
    if dismissed_blocked {
        return Ok(no_update);
    }

    tracing::info!(current = %current, latest = %latest, "自动检查更新：发现新版本，将展示横幅");
    Ok(AutoUpdateInfo { has_update: true, current, latest, url })
}

/// 用户点「关闭」时记录该版本，不再重复提示。
/// 单独一个命令而不是复用 save_settings：这里只改一个键，
/// 走 save_settings 会顺带覆写其他设置（它要求传全部字段）。
#[tauri::command]
pub fn dismiss_update(state: tauri::State<'_, Arc<AppState>>, version: String) -> Result<bool, String> {
    let trimmed = version.trim();
    state.db.set_setting("update_dismissed_version", trimmed).map_err(|e| e.to_string())?;
    crate::logging::audit("update_dismissed", &format!("version={trimmed}"));
    Ok(true)
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
    // v0.8.2（2026-09-17）：截图总开关透传给浮窗。浮窗尾部「截图」按钮的可见条件
    // = screenshot_enabled && float_enabled；浮窗 1Hz 轮询本结构即可联动（≤1s 生效），
    // 不新增 IPC / 事件通道（与 show_disk 等开关同机制）。
    // 权威源仍是 screenshot 模块的 `screenshot_enabled` 设置键：本字段只读不存
    // （save 不落库），缓存写入点见 screenshot_set_config / set_status_bar_config。
    pub screenshot_enabled: bool,
    // v1.2.2（2026-10-21）：液态玻璃效果开关。开启后浮窗从 36px 深色条变为
    // 44px 胶囊并叠加系统材质（macOS 26 = NSGlassEffectView 含折射；
    // Win10 1809+/11 = DWM Acrylic；其余平台不支持，前端置灰）。
    // 默认 false —— 保持既有视觉，用户主动升级观感；也避免老机器默认吃材质开销。
    // 只影响 `float` 窗口，pet / capture 等窗口不受任何影响。
    pub float_glass: bool,
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
            // 与 ScreenshotConfig::default().enabled 一致（截图默认开启）
            screenshot_enabled: true,
            // v1.2.2：液态玻璃默认关闭（保持既有视觉，用户主动开启）
            float_glass: false,
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
        // v0.8.2 新增：截图总开关（与 ScreenshotConfig::load 同 key 同默认值 true）。
        // 注意：本函数只在启动时构造 AppState 缓存用；运行期变更由
        // screenshot_set_config 同步缓存，get_status_bar_config 命令读的是缓存
        let screenshot_enabled = db
            .get_setting("screenshot_enabled")
            .map(|s| s == "true")
            .unwrap_or(true);
        // v1.2.2 新增：液态玻璃开关。老版本 DB 无此 key → 默认 false（保持既有视觉）
        let float_glass = db
            .get_setting("status_bar_float_glass")
            .map(|s| s == "true")
            .unwrap_or(false);
        Self {
            enabled,
            show_cpu,
            show_mem,
            show_disk,
            show_net,
            float_enabled,
            screenshot_enabled,
            float_glass,
        }
    }

    pub fn save(&self, db: &crate::db::AppDb) -> rusqlite::Result<()> {
        let b = |v: bool| if v { "true" } else { "false" };
        // 注意：screenshot_enabled 刻意不在这里持久化——权威源是 screenshot 模块的
        // `screenshot_enabled` 键（ScreenshotConfig::save 写），本结构只是透传视图
        db.set_setting("statusbar_metrics_enabled", b(self.enabled))?;
        db.set_setting("status_bar_enabled", b(self.enabled))?;
        db.set_setting("status_bar_show_cpu", b(self.show_cpu))?;
        db.set_setting("status_bar_show_mem", b(self.show_mem))?;
        db.set_setting("status_bar_show_disk", b(self.show_disk))?;
        db.set_setting("status_bar_show_net", b(self.show_net))?;
        db.set_setting("status_bar_float_enabled", b(self.float_enabled))?;
        // v1.2.2：液态玻璃开关（本地持久化；与 screenshot_enabled 那种"权威源在别处"
        // 的透传字段不同，本字段由本结构独占）
        db.set_setting("status_bar_float_glass", b(self.float_glass))?;
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
    // v1.2.2：额外记录 old_glass，用于判断是否需要切换液态玻璃材质
    let (old_enabled, old_float, old_glass) = {
        let c = state
            .status_bar_config
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        (c.enabled, c.float_enabled, c.float_glass)
    };
    config.save(&state.db).map_err(|e| e.to_string())?;
    crate::logging::audit("status_bar_config_saved", &format!("enabled={} float={} glass={} screenshot={}", config.enabled, config.float_enabled, config.float_glass, config.screenshot_enabled));
    *state.status_bar_config.lock().unwrap_or_else(|e| e.into_inner()) = config;
    // v0.8.2：screenshot_enabled 的权威源在截图模块。设置页可能持有打开时的旧快照，
    // 保存状态栏配置时会把旧值回写缓存，导致浮窗按钮显隐与截图实际开关脱节——
    // 这里强制用 DB 权威值兜底覆盖。
    state
        .status_bar_config
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .screenshot_enabled = crate::screenshot::ScreenshotConfig::load(&state.db).enabled;
    // v0.7.6：设置页改动 → 托盘菜单勾选态即时同步（防「页面改了、菜单还挂旧勾」）。
    // v0.7.11（2026-09-14）：托盘菜单唯一勾选项已改为「启用状态栏」总开关，sync 刷 enabled
    if let Some(toggles) = app.try_state::<crate::TrayStatusBarToggle>() {
        toggles.sync(&config);
    }
    // v0.7.6：浮窗可见态翻转 → 显隐浮窗（幂等创建；透明空窗 show 不可见，无白闪）
    let old_shown = old_enabled && old_float;
    let new_shown = config.enabled && config.float_enabled;
    if old_shown != new_shown {
        crate::float_window::set_float_visible(&app, new_shown)
            .map_err(|e| e.to_string())?;
    }
    // v1.2.2：液态玻璃模式翻转 → 应用/清除系统材质 + 同步窗口高度（36 ↔ 44）。
    //
    // 三个设计点：
    // 1. **只在翻转时动**（old_glass != config.float_glass），避免每次保存四项指标开关
    //    都重跑一遍材质调用（macOS 上会反复 addSubview/removeFromSuperview）。
    // 2. 失败**不回滚**配置、不返回 Err —— 材质只是视觉增强，配置已落库。失败时
    //    apply_via_app 内部记 warn，浮窗沿用 CSS 兜底样式，用户不会看到"开关自己弹回去"。
    // 3. 窗口尚未创建（浮窗关着）时也调用：apply_via_app 会返回"尚未创建"的 Err，
    //    我们忽略它——真正生效时机是用户下次打开浮窗，lib.rs 的显隐路径会补上
    //    （见 set_float_visible 中的 ensure+材质钩子）。
    if old_glass != config.float_glass {
        if config.float_glass {
            // 仅当浮窗当前可见时才需要立刻应用材质；不可见时留给下次 show
            if new_shown {
                if let Err(e) = crate::float_glass::apply_via_app(&app, true) {
                    tracing::warn!(error = %e, "切换液态玻璃失败（浮窗可能在关闭状态）");
                }
            }
        } else if let Err(e) = crate::float_glass::apply_via_app(&app, false) {
            tracing::warn!(error = %e, "清除液态玻璃失败（浮窗可能在关闭状态）");
        }
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

// ===== v1.2.10（2026-10-26）：更新检测的版本比较 / tag 校验单测 =====
//
// 为什么这些必须有测试：`semver_gt` 同时被三条路径使用 —— `check_for_update`（手动）、
// `auto_check_update`（自动）、以及后者的 dismissed 自愈判断。改它等于同时改三处行为，
// 而任何一处的错判对用户的表现都是「弹出一个点开是 404 的更新」或「永远收不到提示」，
// 两者都不会报错、不会留日志，只能靠单测锁住。
#[cfg(test)]
mod tests {
    use super::{is_version_tag, parse_atom_latest_release, semver_gt};

    #[test]
    fn semver_gt_basic_ordering() {
        assert!(semver_gt("1.2.8", "1.2.7"));
        assert!(!semver_gt("1.2.8", "1.2.8"));
        assert!(!semver_gt("1.2.7", "1.2.8"));
        assert!(semver_gt("2.0.0", "1.999.999"));
        assert!(!semver_gt("1.999.999", "2.0.0"));
    }

    #[test]
    fn semver_gt_strips_v_prefix() {
        // 旧实现在这里会把 "v1.2.9" 解析成 [2,9] = 2.9.0，进而和 1.2.8 比出 true。
        // 结论碰巧也是 true，但那是巧合 —— 换成 "v1.1.0" vs "1.2.8" 就会误报。
        assert!(semver_gt("v1.2.9", "1.2.8"));
        assert!(semver_gt("V1.2.9", "1.2.8"));
        assert!(!semver_gt("v1.1.0", "1.2.8"), "v 前缀不能让低版本被判成新版本");
        assert!(!semver_gt("v1.2.8", "1.2.8"));
    }

    #[test]
    fn semver_gt_pads_missing_segments() {
        // 缺位补 0：1.3 应等于 1.3.0，大于 1.2.9
        assert!(semver_gt("1.3", "1.2.9"));
        assert!(!semver_gt("1.2", "1.2.0"));
        assert!(!semver_gt("1.3.0", "1.3"));
    }

    #[test]
    fn semver_gt_ignores_prerelease_suffix() {
        // 1.3.0-beta 的主版本是 1.3.0，仍高于 1.2.8
        assert!(semver_gt("1.3.0-beta", "1.2.8"));
        assert!(!semver_gt("1.2.8-beta", "1.2.8"));
    }

    #[test]
    fn semver_gt_rejects_garbage_conservatively() {
        // 脏数据必须整体判 false。旧实现会逐段丢弃，把 "abc" 解析成 []，
        // 再被「缺位补 0」当成 0.0.0 —— 对 "1.2.8" 恰好也是 false，但
        // "abc.9" 会被读成 [9] = 9.0.0 从而误报。这里把这类输入全部钉死。
        assert!(!semver_gt("abc", "1.2.8"));
        assert!(!semver_gt("abc.9", "1.2.8"), "不能跳过非法段继续解析");
        assert!(!semver_gt("ocr-runtime", "1.2.8"), "资源包名不是版本号");
        assert!(!semver_gt("", "1.2.8"));
        assert!(!semver_gt("1..3", "1.2.8"), "空段不能当 0");
        assert!(!semver_gt("1.2.8", "abc"), "current 脏也不能判成有新版本");
    }

    #[test]
    fn semver_gt_only_compares_first_three_segments() {
        // 只比前三段：第四段多出来不影响结论（1.2.8.1 视作 1.2.8）
        assert!(!semver_gt("1.2.8", "1.2.8.1"));
        assert!(!semver_gt("1.2.8.1", "1.2.8"));
        assert!(semver_gt("1.2.9.1", "1.2.8"));
    }

    #[test]
    fn semver_gt_dismissed_self_heal_case() {
        // ⚠️ 这条是 dismissed 自愈逻辑的守门人，跨批次交互的命门。
        //
        // `auto_check_update` 里的自愈写的是：
        //     } else if !semver_gt(&dismissed, &current) {   // dismissed <= current
        //         let _ = state.db.set_setting("update_dismissed_version", "");
        //     }
        // 即「用户已经升到（或越过）当初忽略的那个版本 → 这条记录没意义了 → 清空」。
        //
        // 触发条件是 `!semver_gt(...)` 为 true。若 `dismissed == current` 时 semver_gt
        // 返回 true（例如为了「加固」把相等也当新版本），自愈**静默失效**：
        // 「重切同名 tag」的用户永远收不到提示，而且没有任何 UI 能撤销
        // （横幅本身已经不显示了）—— 这正是第一批 v1.2.10 刚修掉的问题会复发。
        // 它不会报错、不会留日志，只有这条测试能拦住。
        assert!(!semver_gt("1.2.8", "1.2.8"), "相等必须返回 false，否则自愈不触发");
        // v 前缀不能让「同一个版本」被当成两个不同版本：
        // GitHub tag 带 v 前缀，而数据库里存的是裸版本号，两边经常不同写法。
        assert!(!semver_gt("v1.2.8", "1.2.8"), "dismissed 带 v、current 不带");
        assert!(!semver_gt("1.2.8", "v1.2.8"), "current 带 v、dismissed 不带");
        assert!(!semver_gt("V1.2.8", "v1.2.8"), "大写 V 与小写 v 视为同一版本");
        // 段数不同但数值相等（缺位补 0 后相等）同样必须返回 false
        assert!(!semver_gt("1.2", "1.2.0"));
        assert!(!semver_gt("1.2.0", "1.2"));
        // 反向：dismissed 确实更高 → 自愈不触发（这条记录还有意义，不能清）
        assert!(semver_gt("1.2.9", "1.2.8"));
        // 正向：dismissed 更低 → 自愈应触发（用户已越过它）
        assert!(!semver_gt("1.2.7", "1.2.8"));
    }

    #[test]
    fn missing_segments_are_legal_but_garbage_is_not() {
        // 这两件事必须分开：「段数不足」是合法版本号（补 0），「有非数字段」才非法。
        // 若把 `1.2` 也当非法整体返回 false，就会漏掉真实的升级
        // （远端 tag 写 `1.3` 时用户永远收不到提示）。
        assert!(semver_gt("1.2.8", "1.2"), "缺位的 current 补 0 后 1.2.0 < 1.2.8");
        assert!(semver_gt("1.3", "1.2.8"), "缺位的 latest 补 0 后 1.3.0 > 1.2.8");
        assert!(semver_gt("2", "1.999.999"), "单段版本号合法");
        assert!(!semver_gt("1", "1.0.0"), "1 与 1.0.0 数值相等");
        // 对照：带非数字段的才整体判非法
        assert!(!semver_gt("1.2.x", "1.2.8"));
        assert!(!semver_gt("1.2.8", "1.2.x"));
    }

    #[test]
    fn version_tag_validation() {
        assert!(is_version_tag("v1.2.8"));
        assert!(is_version_tag("V1.2.8"));
        assert!(is_version_tag("1.2.8"));
        assert!(is_version_tag("1.3"));
        assert!(is_version_tag("  v1.2.8  "), "首尾空白应容忍");
        assert!(is_version_tag("1.3.0-beta"));
        assert!(is_version_tag("1.3.0+build.5"));
        // 非法：资源包名、四段、空、只有前缀
        assert!(!is_version_tag("ocr-runtime"));
        assert!(!is_version_tag("1.2.8.1"));
        assert!(!is_version_tag(""));
        assert!(!is_version_tag("v"));
        assert!(!is_version_tag("abc"));
        assert!(!is_version_tag("release-notes"));
        assert!(!is_version_tag("1..3"));
    }

    #[test]
    fn atom_parser_rejects_non_version_release() {
        // F9：将来若把 `ocr-runtime` 这类资源型 release 发到 GitHub 且排在最前，
        // atom feed 第一条就是它。必须返回 None 让上层回落到 API / 镜像，
        // 而不是带着脏 tag 往上走弹死链。
        let xml = r#"<feed><entry>
            <title>ocr-runtime</title>
            <link href="https://github.com/x/y/releases/tag/ocr-runtime"/>
            <summary>ONNX runtime</summary>
        </entry></feed>"#;
        assert!(parse_atom_latest_release(xml).is_none());
    }

    #[test]
    fn atom_parser_accepts_release_style_titles() {
        let xml = r#"<feed><entry>
            <title>ScreenTime Pro v1.2.8</title>
            <link href="https://github.com/x/y/releases/tag/v1.2.8"/>
            <summary>notes</summary>
        </entry></feed>"#;
        let got = parse_atom_latest_release(xml).expect("合法版本 tag 应能解析");
        assert_eq!(got.0, "1.2.8", "返回的版本号不带 v 前缀");
        assert_eq!(got.1, "https://github.com/x/y/releases/tag/v1.2.8");
    }
}
