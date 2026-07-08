//! Tauri 命令层（前端通过 `invoke` 调用）
//!
//! 每个 `#[tauri::command]` 对应前端的一个 API：
//! - 追踪控制：start/stop/is_tracking（启动即自动追踪，无需手动触发）
//! - 实时查询：get_current_foreground、check_permissions
//! - 统计查询：overview / daily_summaries / hourly_buckets / app_ranking / sessions（均支持按 device 过滤）
//! - 趋势对比：get_trends（周/月 环比 + 同比）
//! - 配置与导出：set/get_idle_threshold、export_data、export_all、import_data、prune_data
//! - 分类规则：get_rules / add_rule / update_rule / delete_rule / reclassify_all
//! - 开机自启：set_autostart / is_autostart / get_autostart_pref
//! - 多设备合并：get_devices / get_settings / save_settings
//!
//! 底层逻辑在 `sampling_loop`（后台采样循环）与 `db` 层；分类由 `classifier` 规则引擎完成。

use crate::AppState;
use crate::classifier::classify_app;
use crate::db::{
    AppRankingOut, CategoryOut, CurrentForegroundOut, DailySummaryOut, DayCategoryOut, DeviceInfo,
    ExportBundle, ExportResult, HourlyBucketOut, OverviewOut, PermissionStatus, PeriodStat,
    RuleOut, SessionOut, SettingsOut, TrendsOut,
};
use crate::error::AppError;
use crate::tracker::{platform_name, RawApp};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Weekday};
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tauri::Manager;
use tauri_plugin_autostart::ManagerExt;

const SAMPLE_INTERVAL: u64 = 2; // 采样间隔（秒）
const MIN_SESSION_SECS: i64 = 10; // 最小有效 session 时长

/// 当前进行中的使用时段
pub struct ActiveSession {
    pub app: RawApp,
    pub app_id: i64,
    pub category_id: String,
    pub started_at: DateTime<Local>,
    pub last_input_at: DateTime<Local>,
}

/// 启动后台采样循环（幂等），供「命令」与「启动即自动追踪」共用
///
/// 仅在尚未追踪时启动一次，避免重复 spawn。内部用独立异步任务运行采样循环。
pub fn begin_tracking(state: &Arc<AppState>) {
    {
        let r = state.tracking.lock().unwrap();
        if *r {
            return; // 已在追踪，直接返回
        }
    }
    // 标记开始再释放锁，随后异步执行采样循环
    *state.tracking.lock().unwrap() = true;
    // 克隆 Arc（State 内部已是 Arc<AppState>），移入异步任务
    let st = Arc::clone(state);
    tauri::async_runtime::spawn(async move {
        sampling_loop(st).await;
    });
}

#[tauri::command]
pub fn start_tracking(state: tauri::State<'_, Arc<AppState>>) -> Result<bool, String> {
    // 启动即自动追踪，手动调用同样幂等
    begin_tracking(&state);
    Ok(true)
}

#[tauri::command]
pub fn stop_tracking(state: tauri::State<'_, Arc<AppState>>) -> Result<bool, String> {
    let mut r = state.tracking.lock().unwrap();
    *r = false;
    // 关闭最后一个 session
    if let Some(active) = state.current.lock().unwrap().take() {
        let now = Local::now();
        let dur = (now - active.started_at).num_seconds().max(0) as i64;
        let idle_dur = (now - active.last_input_at).num_seconds().max(0) as i64;
        let effective = (dur - idle_dur).max(0);
        if effective >= MIN_SESSION_SECS {
            let date = active.started_at.format("%Y-%m-%d").to_string();
            let _ = state.db.insert_session(
                active.app_id,
                &active.category_id,
                &active.started_at.to_rfc3339(),
                &now.to_rfc3339(),
                effective,
                &date,
                active.app.window_title.as_deref(),
                &state.device_id,
            );
        }
    }
    Ok(true)
}

#[tauri::command]
pub fn is_tracking(state: tauri::State<'_, Arc<AppState>>) -> Result<bool, String> {
    Ok(*state.tracking.lock().unwrap())
}

#[tauri::command]
pub fn get_current_foreground(state: tauri::State<'_, Arc<AppState>>) -> CurrentForegroundOut {
    let idle = state.tracker.get_idle_seconds().unwrap_or(0);
    let tracking = *state.tracking.lock().unwrap();
    // 当前进行中时段已连续运行的时长（与菜单栏「已记录 XhYm」一致）
    let session_seconds = {
        let cur = state.current.lock().unwrap();
        match cur.as_ref() {
            Some(s) => (Local::now() - s.started_at).num_seconds().max(0),
            None => 0,
        }
    };
    match state.tracker.get_foreground_app() {
        Ok(app) => {
            // 用内存中的规则引擎实时分类（无需导出后人工整理）
            let cat = classify_app(&app, &state.rules.lock().unwrap());
            CurrentForegroundOut {
                name: app.name,
                process_name: app.process_name,
                category_id: cat,
                idle_seconds: idle,
                tracking,
                window_title: app.window_title,
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
            session_seconds,
        },
    }
}

#[tauri::command]
pub fn get_overview(
    state: tauri::State<'_, Arc<AppState>>,
    date: String,
    device: Option<String>,
) -> Result<OverviewOut, String> {
    state.db.get_overview(&date, &device).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_daily_summaries(
    state: tauri::State<'_, Arc<AppState>>,
    days: u32,
    device: Option<String>,
) -> Result<Vec<DailySummaryOut>, String> {
    state
        .db
        .get_daily_summaries(days, &device)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_daily_categories(
    state: tauri::State<'_, Arc<AppState>>,
    days: u32,
    device: Option<String>,
) -> Result<Vec<DayCategoryOut>, String> {
    state
        .db
        .get_daily_categories(days, &device)
        .map_err(|e| e.to_string())
}

/// 在系统文件管理器中打开指定路径（导出后便于用户定位备份文件）
///
/// macOS: `open -R <path>`；Windows: `explorer /select,<path>`；Linux: `xdg-open <dir>`
#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .status()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path))
            .status()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        let dir = std::path::Path::new(&path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());
        std::process::Command::new("xdg-open")
            .arg(dir)
            .status()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn get_hourly_buckets(
    state: tauri::State<'_, Arc<AppState>>,
    date: String,
    device: Option<String>,
) -> Result<Vec<HourlyBucketOut>, String> {
    state
        .db
        .get_hourly_buckets(&date, &device)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_app_ranking(
    state: tauri::State<'_, Arc<AppState>>,
    date: String,
    device: Option<String>,
) -> Result<Vec<AppRankingOut>, String> {
    state
        .db
        .get_app_ranking(&date, &device)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_categories(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<CategoryOut>, String> {
    state.db.get_categories().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_sessions(
    state: tauri::State<'_, Arc<AppState>>,
    date: String,
) -> Result<Vec<SessionOut>, String> {
    state.db.get_sessions(&date).map_err(|e| e.to_string())
}

/// 趋势对比：周/月 的「本期 vs 上一周期（环比）」+「去年同期（同比，仅月份）」
///
/// `device` 为空表示合并全部设备，否则只看某台设备。
#[tauri::command]
pub fn get_trends(
    state: tauri::State<'_, Arc<AppState>>,
    period: String,
    device: Option<String>,
) -> Result<TrendsOut, String> {
    let (cur_start, cur_end, prev_start, prev_end, yoy_start, yoy_end, cur_label, prev_label) =
        period_ranges(&period);
    let current = state
        .db
        .period_summary(&cur_start, &cur_end, &device, &cur_label)
        .map_err(|e| e.to_string())?;
    let prev = state
        .db
        .period_summary(&prev_start, &prev_end, &device, &prev_label)
        .map_err(|e| e.to_string())?;
    let yoy: Option<PeriodStat> = if period == "month" {
        Some(
            state
                .db
                .period_summary(&yoy_start, &yoy_end, &device, "去年同期")
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };
    let delta_total_pct = if prev.total_seconds > 0 {
        (current.total_seconds - prev.total_seconds) as f64 / prev.total_seconds as f64 * 100.0
    } else {
        0.0
    };
    Ok(TrendsOut {
        period,
        current,
        prev,
        yoy,
        delta_total_pct,
    })
}

#[tauri::command]
pub fn set_idle_threshold(
    state: tauri::State<'_, Arc<AppState>>,
    secs: u64,
) -> Result<bool, String> {
    *state.idle_threshold.lock().unwrap() = secs;
    Ok(true)
}

#[tauri::command]
pub fn get_idle_threshold(state: tauri::State<'_, Arc<AppState>>) -> Result<u64, String> {
    Ok(*state.idle_threshold.lock().unwrap())
}

/// 单日 CSV / JSON 导出（保持原有功能）
#[tauri::command]
pub fn export_data(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    date: String,
    format: String,
) -> Result<ExportResult, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let exports = dir.join("exports");
    std::fs::create_dir_all(&exports).ok();
    let ext = if format == "json" { "json" } else { "csv" };
    let path = exports.join(format!("screentime_{}.{}", date, ext));
    if ext == "csv" {
        state.db.export_csv(&path, &date).map_err(|e| e.to_string())?;
    } else {
        let ranking = state.db.get_app_ranking(&date, &None).map_err(|e| e.to_string())?;
        let json = serde_json::to_string_pretty(&ranking).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())?;
    }
    Ok(ExportResult {
        path: path.to_string_lossy().to_string(),
    })
}

/// 全量导出（含设备标签），用于跨设备数据合并
#[tauri::command]
pub fn export_all(app: tauri::AppHandle) -> Result<ExportResult, String> {
    let bundle = state_export(&app)?;
    let json = serde_json::to_string_pretty(&bundle).map_err(|e| e.to_string())?;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let exports = dir.join("exports");
    std::fs::create_dir_all(&exports).ok();
    let file = format!("screentime_export_{}.json", today_str());
    let path = exports.join(file);
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(ExportResult {
        path: path.to_string_lossy().to_string(),
    })
}

// 辅助：取 AppState 并导出全量包（export_all 用）
fn state_export(app: &tauri::AppHandle) -> Result<ExportBundle, String> {
    let state = app.state::<Arc<AppState>>();
    state.db.export_all().map_err(|e| e.to_string())
}

/// 导入全量数据并合并（按 start_at+app_id+device 去重）
///
/// 入参为前端读取到的导出文件 JSON 文本（避免跨平台文件路径沙箱问题）。
#[tauri::command]
pub fn import_data(
    app: tauri::AppHandle,
    content: String,
) -> Result<usize, String> {
    let bundle: ExportBundle = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let state = app.state::<Arc<AppState>>();
    state.db.import_data(&bundle).map_err(|e| e.to_string())
}

/// 清理超过保留天数的旧数据
#[tauri::command]
pub fn prune_data(
    state: tauri::State<'_, Arc<AppState>>,
    days: u32,
) -> Result<usize, String> {
    let n = state.db.prune_old(days).map_err(|e| e.to_string())?;
    let _ = state.db.set_setting("data_retention_days", &days.to_string());
    Ok(n)
}

/// 查询当前系统权限状态（辅助功能 / 屏幕录制）
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
        PermissionStatus {
            accessibility: true,
            screen_capture: true,
        }
    }
}

/// 打开系统设置中对应的隐私权限面板（macOS）
#[tauri::command]
pub fn open_privacy_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();
    }
}

// ===================== 分类规则管理 =====================

/// 返回全部分类规则（供前端管理界面展示与编辑）
#[tauri::command]
pub fn get_rules(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<RuleOut>, String> {
    state.db.get_rules_out().map_err(|e| e.to_string())
}

/// 新增规则；返回新规则 id
#[tauri::command]
pub fn add_rule(
    state: tauri::State<'_, Arc<AppState>>,
    field: String,
    match_type: String,
    pattern: String,
    category_id: String,
    priority: i32,
) -> Result<i64, String> {
    let id = state
        .db
        .insert_rule(&field, &match_type, &pattern, &category_id, priority)
        .map_err(|e| e.to_string())?;
    reload_rules(&state);
    Ok(id)
}

/// 更新规则（按 id）
#[tauri::command]
pub fn update_rule(
    state: tauri::State<'_, Arc<AppState>>,
    id: i64,
    field: String,
    match_type: String,
    pattern: String,
    category_id: String,
    priority: i32,
    enabled: bool,
) -> Result<bool, String> {
    state
        .db
        .update_rule(id, &field, &match_type, &pattern, &category_id, priority, enabled)
        .map_err(|e| e.to_string())?;
    reload_rules(&state);
    Ok(true)
}

/// 删除规则（按 id）
#[tauri::command]
pub fn delete_rule(state: tauri::State<'_, Arc<AppState>>, id: i64) -> Result<bool, String> {
    state.db.delete_rule(id).map_err(|e| e.to_string())?;
    reload_rules(&state);
    Ok(true)
}

/// 按当前规则重算所有已记录应用的分类，返回更新数量
#[tauri::command]
pub fn reclassify_all(state: tauri::State<'_, Arc<AppState>>) -> Result<usize, String> {
    let rules = state.rules.lock().unwrap();
    state.db.reclassify_all(&rules).map_err(|e| e.to_string())
}

/// 把数据库中的规则重新载入内存缓存
fn reload_rules(state: &tauri::State<'_, Arc<AppState>>) {
    if let Ok(rules) = state.db.load_rules() {
        *state.rules.lock().unwrap() = rules;
    }
}

// ===================== 开机自启 =====================

/// 设置开机自启：同时写入操作系统自启项与本地偏好（settings 表）
#[tauri::command]
pub fn set_autostart(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    enabled: bool,
) -> Result<bool, String> {
    let mgr = app.autolaunch();
    if enabled {
        mgr.enable().map_err(|e| e.to_string())?;
    } else {
        mgr.disable().map_err(|e| e.to_string())?;
    }
    state
        .db
        .set_setting("autostart", if enabled { "true" } else { "false" })
        .map_err(|e| e.to_string())?;
    Ok(true)
}

/// 读取操作系统层面的自启开关状态
#[tauri::command]
pub fn is_autostart(app: tauri::AppHandle) -> Result<bool, String> {
    Ok(app.autolaunch().is_enabled().unwrap_or(false))
}

/// 读取本地保存的自启用户偏好（首次运行为 None）
#[tauri::command]
pub fn get_autostart_pref(state: tauri::State<'_, Arc<AppState>>) -> Option<bool> {
    state.db.get_setting("autostart").map(|v| v == "true")
}

// ===================== 多设备合并 =====================

/// 列出所有设备（含本机），供前端设备切换器使用
#[tauri::command]
pub fn get_devices(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<DeviceInfo>, String> {
    let id = state.device_id.clone();
    let name = state
        .db
        .get_setting("device_name")
        .unwrap_or_else(|| id.clone());
    state.db.get_devices(&id, &name).map_err(|e| e.to_string())
}

/// 读取全部设置项（设置页用）
#[tauri::command]
pub fn get_settings(state: tauri::State<'_, Arc<AppState>>) -> Result<SettingsOut, String> {
    let device_id = state.device_id.clone();
    let device_name = state
        .db
        .get_setting("device_name")
        .unwrap_or_else(|| device_id.clone());
    let idle = *state.idle_threshold.lock().unwrap();
    let retention: u32 = state
        .db
        .get_setting("data_retention_days")
        .and_then(|v| v.parse().ok())
        .unwrap_or(365);
    let autostart = state
        .db
        .get_setting("autostart")
        .map(|v| v == "true")
        .unwrap_or(false);
    Ok(SettingsOut {
        device_id,
        device_name,
        idle_threshold: idle,
        data_retention_days: retention,
        sample_interval: SAMPLE_INTERVAL,
        autostart,
    })
}

/// 保存设置项（设备名 / 空闲阈值 / 保留天数）
#[tauri::command]
pub fn save_settings(
    state: tauri::State<'_, Arc<AppState>>,
    idle_threshold: u64,
    device_name: String,
    data_retention_days: u32,
) -> Result<bool, String> {
    *state.idle_threshold.lock().unwrap() = idle_threshold;
    state
        .db
        .set_setting("idle_threshold", &idle_threshold.to_string())
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting("device_name", &device_name)
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting(
            &format!("device_name:{}", state.device_id),
            &device_name,
        )
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting("data_retention_days", &data_retention_days.to_string())
        .map_err(|e| e.to_string())?;
    Ok(true)
}

/// 后台采样循环：每 SAMPLE_INTERVAL 秒采集一次前台应用，按 app 切换与空闲阈值切分 session
///
/// 分类使用内存中的可配置规则（`state.rules`），实现「自动归类」而非导出后人工整理。
/// 写入的时段携带本机 device_id，用于多设备合并。
async fn sampling_loop(state: Arc<AppState>) {
    let mut ticker = tokio::time::interval(StdDuration::from_secs(SAMPLE_INTERVAL));
    loop {
        ticker.tick().await;
        if !*state.tracking.lock().unwrap() {
            break;
        }
        let fg = match state.tracker.get_foreground_app() {
            Ok(a) => a,
            Err(_) => continue,
        };
        let idle = state.tracker.get_idle_seconds().unwrap_or(0);
        let threshold = *state.idle_threshold.lock().unwrap();
        let now = Local::now();
        let platform = platform_name();
        // 用规则引擎分类（窗口标题 / 进程名 / 路径 / 包名综合判定）
        let category = classify_app(&fg, &state.rules.lock().unwrap());

        // 阶段1：在锁内判断是否需要关闭旧 session
        let to_finalize: Option<(i64, String, DateTime<Local>, DateTime<Local>)> = {
            let mut cur = state.current.lock().unwrap();
            let finalize = match cur.as_ref() {
                None => false,
                Some(a) => {
                    let same = a.app.process_name == fg.process_name
                        && a.app.exe_path == fg.exe_path;
                    !same || idle >= threshold
                }
            };
            if finalize {
                cur.take()
                    .map(|a| (a.app_id, a.category_id, a.started_at, a.last_input_at))
            } else {
                if let Some(a) = cur.as_mut() {
                    a.last_input_at = now;
                }
                None
            }
        };

        // 阶段2：关闭旧 session（已释放锁，可安全写库）
        if let Some((app_id, cat, started, last_input)) = to_finalize {
            let dur = (now - started).num_seconds().max(0) as i64;
            let idle_dur = (now - last_input).num_seconds().max(0) as i64;
            let effective = (dur - idle_dur).max(0);
            if effective >= MIN_SESSION_SECS {
                let date = started.format("%Y-%m-%d").to_string();
                let _ = state.db.insert_session(
                    app_id,
                    &cat,
                    &started.to_rfc3339(),
                    &now.to_rfc3339(),
                    effective,
                    &date,
                    fg.window_title.as_deref(),
                    &state.device_id,
                );
            }
        }

        // 阶段3：若无进行中 session 则新建
        {
            let mut cur = state.current.lock().unwrap();
            if cur.is_none() {
                let app_id = state
                    .db
                    .upsert_app(
                        &fg.name,
                        &fg.process_name,
                        fg.exe_path.as_deref(),
                        &category,
                        platform,
                    )
                    .unwrap_or(0);
                *cur = Some(ActiveSession {
                    app: fg,
                    app_id,
                    category_id: category,
                    started_at: now,
                    last_input_at: now,
                });
            }
        }
    }
}

// 让 AppError 能直接通过 ? 转成 command 返回的 String
impl From<AppError> for String {
    fn from(e: AppError) -> Self {
        e.to_string()
    }
}

/// 计算周/月对比所需的时间区间（RFC3339 字符串）
///
/// 返回 (本期起, 本期止, 上期起, 上期止, 去年同期起, 去年同期止, 本期标签, 上期标签)
fn period_ranges(
    period: &str,
) -> (String, String, String, String, String, String, String, String) {
    let now = Local::now();
    let today = now.date_naive();
    // 把「某天某时刻」转成本地时区的 DateTime<Local>（与 sessions.start_at 存储的 RFC3339 格式一致，可直接比较）
    let to_local = |d: NaiveDate, h: u32, mi: u32, s: u32| {
        d.and_hms_opt(h, mi, s)
            .unwrap()
            .and_local_timezone(Local)
            .single()
            .unwrap()
    };
    if period == "month" {
        let y = today.year();
        let m = today.month();
        // 本期：本月 1 号 00:00 → 现在
        let cur_start = to_local(NaiveDate::from_ymd_opt(y, m, 1).unwrap(), 0, 0, 0);
        let cur_end = now;
        let (py, pm) = if m == 1 { (y - 1, 12) } else { (y, m - 1) };
        // 上期：上月 1 号 → 上月最后一天 23:59:59
        let prev_start = to_local(NaiveDate::from_ymd_opt(py, pm, 1).unwrap(), 0, 0, 0);
        let prev_last = last_day_of_month(py, pm);
        let prev_end = to_local(NaiveDate::from_ymd_opt(py, pm, prev_last).unwrap(), 23, 59, 59);
        // 同比：去年同月
        let yoy_y = y - 1;
        let yoy_start = to_local(NaiveDate::from_ymd_opt(yoy_y, m, 1).unwrap(), 0, 0, 0);
        let yoy_last = last_day_of_month(yoy_y, m);
        let yoy_end = to_local(NaiveDate::from_ymd_opt(yoy_y, m, yoy_last).unwrap(), 23, 59, 59);
        (
            cur_start.to_rfc3339(),
            cur_end.to_rfc3339(),
            prev_start.to_rfc3339(),
            prev_end.to_rfc3339(),
            yoy_start.to_rfc3339(),
            yoy_end.to_rfc3339(),
            format!("{y}年{m}月"),
            format!("{py}年{pm}月"),
        )
    } else {
        // 周：以本周一为起点（周一=0），本期=本周一至现在，上期=上周一至本周一
        let wd = today.weekday().num_days_from_monday() as i64;
        let this_mon = today - Duration::days(wd);
        let cur_start = to_local(this_mon, 0, 0, 0);
        let cur_end = now;
        let prev_mon = this_mon - Duration::days(7);
        let prev_start = to_local(prev_mon, 0, 0, 0);
        // 开区间上界即本周一 00:00
        let prev_end = to_local(this_mon, 0, 0, 0);
        (
            cur_start.to_rfc3339(),
            cur_end.to_rfc3339(),
            prev_start.to_rfc3339(),
            prev_end.to_rfc3339(),
            String::new(),
            String::new(),
            "本周".to_string(),
            "上周".to_string(),
        )
    }
}

/// 某年某月的最后一天
fn last_day_of_month(year: i32, month: u32) -> u32 {
    let next_month = if month == 12 { 1 } else { month + 1 };
    let next_year = if month == 12 { year + 1 } else { year };
    let first_of_next = NaiveDate::from_ymd_opt(next_year, next_month, 1).unwrap();
    (first_of_next - Duration::days(1))
        .day()
}

/// 生成本地日期串 YYYY-MM-DD（导出文件名用）
fn today_str() -> String {
    let d = Local::now();
    format!(
        "{}-{:02}-{:02}",
        d.year(),
        d.month(),
        d.day()
    )
}

// 让 Weekday 在编译期被使用（防止未使用导入告警），实际用于 num_days_from_monday
#[allow(dead_code)]
fn _weekday_marker(_: Weekday) {}

/// Windows WebView2 运行时检测结果
#[derive(serde::Serialize, Clone)]
pub struct Webview2Status {
    /// 操作系统
    pub os: String,
    /// 是否可用（macOS/Linux 永远 true）
    pub available: bool,
    /// 当前安装的 WebView2 版本（未安装时为空字符串）
    pub version: String,
    /// 用户可读的安装提示文案（仅当 available=false 时有内容）
    pub hint: String,
}

/// 前端启动时调用，提示用户安装 WebView2 永驻版
/// 仅 Windows 平台做实际检查（读注册表 EdgeUpdate clients 的 pv 值），macOS/Linux 不依赖 WebView2
#[tauri::command]
pub fn check_webview2(app: tauri::AppHandle) -> Webview2Status {
    #[cfg(target_os = "windows")]
    {
        let _ = app;
        // Edge WebView2 Runtime 安装时会在以下注册表键写入版本号
        // HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\ClientState\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}\pv
        // 同时 Edge 浏览器自身也会写 HKLM\SOFTWARE\Microsoft\Edge\BLBeacon\version
        // 优先读取 WebView2 专属键；读取失败时回退到 Edge 浏览器版本
        let version = read_webview2_version();
        if let Some(v) = version {
            return Webview2Status {
                os: "windows".to_string(),
                available: true,
                version: v,
                hint: String::new(),
            };
        }
        Webview2Status {
            os: "windows".to_string(),
            available: false,
            version: String::new(),
            hint: "未检测到 WebView2 运行时，请先安装 Microsoft Edge WebView2 Runtime（永驻版）后再运行本应用：\nhttps://developer.microsoft.com/en-us/microsoft-edge/webview2/".to_string(),
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        Webview2Status {
            os: std::env::consts::OS.to_string(),
            available: true,
            version: "n/a".to_string(),
            hint: String::new(),
        }
    }
}

/// 读取 Windows 注册表查询 WebView2 Runtime 版本
/// 使用 `reg query` 命令，失败时返回 None
#[cfg(target_os = "windows")]
fn read_webview2_version() -> Option<String> {
    use std::process::Command;
    // 先尝试 WebView2 Runtime 自己的 ClientState
    let keys = [
        r"HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\ClientState\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        r"HKLM\SOFTWARE\Microsoft\EdgeUpdate\ClientState\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
    ];
    for key in keys {
        let output = Command::new("reg")
            .args(["query", key, "/v", "pv"])
            .output()
            .ok()?;
        if !output.status.success() {
            continue;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        // 形如 `    pv    REG_SZ    110.0.1587.50`
        for line in text.lines() {
            if line.trim_start().starts_with("pv") {
                if let Some(v) = line.split_whitespace().nth(2) {
                    return Some(v.to_string());
                }
            }
        }
    }
    // 回退到 Edge 浏览器版本（Win11 自带 Edge 即代表 WebView2 就绪）
    let output = Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Edge\BLBeacon",
            "/v",
            "version",
        ])
        .output()
        .ok()?;
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            if line.trim_start().starts_with("version") {
                if let Some(v) = line.split_whitespace().nth(2) {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// 打开 Microsoft WebView2 下载页（用户从错误弹窗点「去下载」时调用）
#[tauri::command]
pub fn open_webview2_download() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        open::that("https://developer.microsoft.com/en-us/microsoft-edge/webview2/")
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
