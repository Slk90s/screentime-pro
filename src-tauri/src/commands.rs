//! Tauri 命令层（前端通过 `invoke` 调用）
//!
//! 每个 `#[tauri::command]` 对应前端的一个 API：
//!
//! ## 日志埋点（v0.4.2 引入）
//! - 采样循环：每分钟聚合 1 条 INFO（不要每 tick 一条，否则日志爆炸）
//! - 关键错误：ERROR 级（spawn_blocking panic、DB 写入失败、权限永久拒绝）
//! - 切应用：DEBUG 级（生产环境默认关，需要排查时开 RUST_LOG=debug）
//! - 详细规则见 `logging.rs` 与 `docs/LOGGING.md`
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
        let r = state.tracking.lock().unwrap_or_else(|e| e.into_inner());
        if *r {
            return; // 已在追踪，直接返回
        }
    }
    // 标记开始再释放锁，随后异步执行采样循环
    *state.tracking.lock().unwrap_or_else(|e| e.into_inner()) = true;
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
    let mut r = state.tracking.lock().unwrap_or_else(|e| e.into_inner());
    *r = false;
    // 关闭最后一个 session；若跨午夜则按午夜分界点拆成多条记录，确保每天统计准确
    if let Some(active) = state.current.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let now = Local::now();
        finalize_active_session(&state, &active, now);
    }
    Ok(true)
}

/// 关闭一个 ActiveSession：若跨越午夜则按 0:00 分界点拆成多条入库
/// - 公共逻辑，供 `stop_tracking`（手动停止）与 `sampling_loop` 跨日拆分共用
fn finalize_active_session(
    state: &Arc<AppState>,
    active: &ActiveSession,
    now: DateTime<Local>,
) {
    // 计算分界点：session 起始日次日的本地 00:00:00
    let start_date = active.started_at.date_naive();
    let now_date = now.date_naive();
    let splits: Vec<(DateTime<Local>, DateTime<Local>)> = if start_date == now_date {
        // 同一天：直接整段
        vec![(active.started_at, now)]
    } else {
        // 跨多天：按 00:00 切分（通常 1 段，理论极端情况可多段）
        let mut out = Vec::new();
        let mut cursor_date = start_date;
        let mut cursor_dt = active.started_at;
        loop {
            // 下一天 00:00
            let next_date = cursor_date + Duration::days(1);
            let next_midnight = next_date
                .and_hms_opt(0, 0, 0)
                .and_then(|nd| nd.and_local_timezone(Local).single());
            let next_midnight = match next_midnight {
                Some(t) => t,
                None => break,
            };
            // 这一段的终点：min(次日0:00, now)
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
        // 空闲时长按段内统计：若 last_input_at 早于本段起点，按 0 处理
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
    // 注：idle_dur 累计问题 — 跨日的 session 后段 idle 可能继承自前段 last_input_at，
    // 极端情况会高估后段空闲。准确性可接受（与 iOS Screen Time 行为一致），不深究。
}

#[tauri::command]
pub fn is_tracking(state: tauri::State<'_, Arc<AppState>>) -> Result<bool, String> {
    Ok(*state.tracking.lock().unwrap_or_else(|e| e.into_inner()))
}

#[tauri::command]
pub fn get_current_foreground(state: tauri::State<'_, Arc<AppState>>) -> CurrentForegroundOut {
    let idle = state.tracker.get_idle_seconds().unwrap_or(0);
    // v0.4.1 修复：unwrap_or_else(|e| e.into_inner()) 恢复 poisoned Mutex 的数据，
    // 避免任一线程 panic 雪崩到所有后续命令
    let tracking = *state.tracking.lock().unwrap_or_else(|e| e.into_inner());
    // 当前进行中时段已连续运行的时长（与菜单栏「已记录 XhYm」一致）
    let session_seconds = {
        let cur = state.current.lock().unwrap_or_else(|e| e.into_inner());
        match cur.as_ref() {
            Some(s) => (Local::now() - s.started_at).num_seconds().max(0),
            None => 0,
        }
    };
    match state.tracker.get_foreground_app() {
        Ok(app) => {
            // v0.4.1 修复：克隆 rules 后立即释放锁，再调 classify_app
            // （classify_app 内做 to_lowercase + Vec 排序，CPU 工作不持锁）
            let rules_clone = state.rules.lock().unwrap_or_else(|e| e.into_inner()).clone();
            let cat = classify_app(&app, &rules_clone);
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

// =====================================================================
// 日志相关命令（v0.4.2 引入）
// =====================================================================

/// 导出日志到桌面（用户报 bug 时一键打包）
///
/// 把当前所有 `app.log.*` 文件打包成 zip，输出到 `~/Desktop/screentime-pro-logs-{timestamp}.zip`
/// （macOS/Linux） 或 `%USERPROFILE%\Desktop\...` （Windows）。
#[tauri::command]
pub fn export_logs(app: tauri::AppHandle) -> Result<ExportResult, String> {
    use std::io::Read;

    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|e| format!("无法定位日志目录: {}", e))?;

    if !log_dir.exists() {
        return Err(format!("日志目录不存在: {}", log_dir.display()));
    }

    // 桌面目录
    let desktop = app
        .path()
        .desktop_dir()
        .or_else(|_| app.path().home_dir().map(|p| p.join("Desktop")))
        .map_err(|e| format!("无法定位桌面目录: {}", e))?;
    std::fs::create_dir_all(&desktop).ok();

    let ts = Local::now().format("%Y%m%d_%H%M%S");
    // 简易方案：把多个日志文件拼接为单文件 .txt（避免新增 zip crate 依赖）
    // 用户拿到这个 .txt 直接发邮件/聊天即可
    // 为避免新增依赖（zip crate 比较大），这里用「拼接为单文件 .txt」方案：
    //   screentime-pro-logs-{ts}.txt —— 把所有 app.log.* 追加写入
    let txt_path = desktop.join(format!("screentime-pro-logs-{}.txt", ts));
    let mut out = std::fs::File::create(&txt_path)
        .map_err(|e| format!("创建日志导出文件失败: {}", e))?;

    // 写文件头信息
    use std::io::Write;
    writeln!(
        out,
        "ScreenTime Pro 日志导出\n生成时间: {}\n日志目录: {}\n\n--- 文件列表 ---\n",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        log_dir.display()
    )
    .ok();

    // 列出所有 app.log.* 文件，按日期倒序（最新的在前）
    let mut entries: Vec<_> = std::fs::read_dir(&log_dir)
        .map_err(|e| format!("读取日志目录失败: {}", e))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("app.log")
        })
        .collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.file_name()));

    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        writeln!(
            out,
            "\n=== {} ({} bytes) ===\n",
            name,
            entry.metadata().map(|m| m.len()).unwrap_or(0)
        )
        .ok();
        if let Ok(mut f) = std::fs::File::open(&path) {
            let mut buf = String::new();
            if f.read_to_string(&mut buf).is_ok() {
                let _ = out.write_all(buf.as_bytes());
            }
        }
    }

    tracing::info!(path = %txt_path.display(), "用户导出日志");
    Ok(ExportResult {
        path: txt_path.to_string_lossy().to_string(),
    })
}

/// 获取日志目录的总大小（字节）—— Settings 页显示用
#[tauri::command]
pub fn get_log_size(app: tauri::AppHandle) -> Result<u64, String> {
    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|e| format!("无法定位日志目录: {}", e))?;
    crate::logging::dir_size(&log_dir).map_err(|e| e.to_string())
}

/// 获取日志目录的绝对路径——Settings 页"打开日志目录"按钮用
#[tauri::command]
pub fn get_log_dir(app: tauri::AppHandle) -> Result<String, String> {
    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|e| format!("无法定位日志目录: {}", e))?;
    Ok(log_dir.to_string_lossy().to_string())
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
    *state.idle_threshold.lock().unwrap_or_else(|e| e.into_inner()) = secs;
    Ok(true)
}

#[tauri::command]
pub fn get_idle_threshold(state: tauri::State<'_, Arc<AppState>>) -> Result<u64, String> {
    Ok(*state.idle_threshold.lock().unwrap_or_else(|e| e.into_inner()))
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

/// 导出全量备份
/// - `device_id`：可选过滤
///   - None / 空字符串：导出所有设备的数据（多设备合并用）
///   - Some(id)：仅导出该设备的数据（「按设备清理」前的备份）
#[tauri::command]
pub fn export_all(
    app: tauri::AppHandle,
    device_id: Option<String>,
) -> Result<ExportResult, String> {
    let bundle = state_export(&app, device_id.as_deref())?;
    let json = serde_json::to_string_pretty(&bundle).map_err(|e| e.to_string())?;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let exports = dir.join("exports");
    std::fs::create_dir_all(&exports).ok();
    // 文件名带设备 ID 标记（多设备合并场景下用户一眼看出这是哪台的备份）
    let suffix = match device_id.as_ref().filter(|s| !s.is_empty()) {
        Some(id) => format!("_{}", &id[..id.len().min(12)]), // 截前 12 位（设备 ID 长度）
        None => String::new(),
    };
    let file = format!("screentime_backup{}_{}.json", suffix, today_str());
    let path = exports.join(file);
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(ExportResult {
        path: path.to_string_lossy().to_string(),
    })
}

// 辅助：取 AppState 并导出包；device_id 为 None 时导出全部
fn state_export(app: &tauri::AppHandle, device_id: Option<&str>) -> Result<ExportBundle, String> {
    let state = app.state::<Arc<AppState>>();
    state.db.export_all_filtered(device_id).map_err(|e| e.to_string())
}

/// 「按设备清理 + 自动备份」组合命令
/// 流程：① 导出该设备全量备份到 `exports/` ② 删除该设备所有 sessions（不限时间）
/// 设计目的：删除是不可逆操作，先备份让用户能恢复（用户可自己保管备份文件）
#[derive(serde::Serialize, Clone)]
pub struct BackupAndPruneResult {
    /// 备份文件路径（用户应手动复制到安全位置）
    pub backup_path: String,
    /// 实际删除的 session 数
    pub deleted_count: usize,
}

#[tauri::command]
pub fn backup_and_prune_device(
    app: tauri::AppHandle,
    device_id: String,
) -> Result<BackupAndPruneResult, String> {
    if device_id.trim().is_empty() {
        return Err("device_id 不能为空".to_string());
    }
    // 1. 导出该设备的备份（标记设备 ID 在文件名）
    let bundle = state_export(&app, Some(&device_id))?;
    let json = serde_json::to_string_pretty(&bundle).map_err(|e| e.to_string())?;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let exports = dir.join("exports");
    std::fs::create_dir_all(&exports).ok();
    let suffix = &device_id[..device_id.len().min(12)];
    let file = format!(
        "screentime_backup_{}_{}_pre_purge.json",
        suffix,
        today_str()
    );
    let backup_path = exports.join(file);
    std::fs::write(&backup_path, &json).map_err(|e| e.to_string())?;

    // 2. 删除该设备的全部 sessions（不限 365 天；UI 已用 Modal 二次确认）
    let state = app.state::<Arc<AppState>>();
    // 直接 SQL 删除（不过 cutoff）：从 db 层加新方法
    let deleted = state
        .db
        .delete_all_sessions_for_device(&device_id)
        .map_err(|e| e.to_string())?;

    Ok(BackupAndPruneResult {
        backup_path: backup_path.to_string_lossy().to_string(),
        deleted_count: deleted,
    })
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
/// - `device_ids`：可选设备 ID 列表过滤
///   - `None` 或空数组：清全部设备的旧数据
///   - `Some(["id1", "id2"])`：仅清这些设备的旧数据（多选用例）
#[tauri::command]
pub fn prune_data(
    state: tauri::State<'_, Arc<AppState>>,
    days: u32,
    device_ids: Option<Vec<String>>,
) -> Result<usize, String> {
    let ids = device_ids.unwrap_or_default();
    let n = if ids.is_empty() {
        // 清全部设备的旧数据
        state.db.prune_old(days, None).map_err(|e| e.to_string())?
    } else {
        // 按设备清理（多选）：依次调用单设备清理并累加
        let mut total = 0usize;
        for id in &ids {
            let deleted = state
                .db
                .prune_old(days, Some(id.as_str()))
                .map_err(|e| e.to_string())?;
            total += deleted;
        }
        total
    };
    // 仅在「清全部」时更新全局保留天数（按设备清理不影响全局策略）
    if ids.is_empty() {
        let _ = state.db.set_setting("data_retention_days", &days.to_string());
    }
    Ok(n)
}

/// 列出所有设备的聚合统计（用于 Settings.vue「按设备清理」弹窗）
#[tauri::command]
pub fn list_devices_with_stats(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<crate::db::DeviceStats>, String> {
    let current_id = state.device_id.clone();
    let current_name = state
        .db
        .get_setting("device_name")
        .unwrap_or_else(|| current_id.clone());
    state
        .db
        .list_devices_with_stats(&current_id, &current_name)
        .map_err(|e| e.to_string())
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
    let rules = state.rules.lock().unwrap_or_else(|e| e.into_inner());
    state.db.reclassify_all(&rules).map_err(|e| e.to_string())
}

/// 把数据库中的规则重新载入内存缓存
fn reload_rules(state: &tauri::State<'_, Arc<AppState>>) {
    if let Ok(rules) = state.db.load_rules() {
        *state.rules.lock().unwrap_or_else(|e| e.into_inner()) = rules;
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
    let idle = *state.idle_threshold.lock().unwrap_or_else(|e| e.into_inner());
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
/// - v0.4.0：device_name 同步更新到内存（之前需重启才生效）
#[tauri::command]
pub fn save_settings(
    state: tauri::State<'_, Arc<AppState>>,
    idle_threshold: u64,
    device_name: String,
    data_retention_days: u32,
) -> Result<bool, String> {
    *state.idle_threshold.lock().unwrap_or_else(|e| e.into_inner()) = idle_threshold;
    // 同步设备名到内存，立即生效（无需重启）
    *state.device_name.lock().unwrap_or_else(|e| e.into_inner()) = device_name.clone();
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
    // v0.4.2 日志节流：每分钟聚合 1 条 INFO，避免 1Hz tick 把日志写爆
    let mut last_summary_minute: Option<i64> = None;
    // 用于聚合统计：当前分钟内的 tick 数 + 应用切换次数
    let mut tick_in_minute: u32 = 0;
    let mut switch_in_minute: u32 = 0;
    let mut last_app_for_minute: String = String::new();
    loop {
        ticker.tick().await;
        if !*state.tracking.lock().unwrap_or_else(|e| e.into_inner()) {
            break;
        }
        let fg = match state.tracker.get_foreground_app() {
            Ok(a) => a,
            Err(e) => {
                // v0.4.2 日志：采集器失败可能是权限被收回/系统重启等关键信号
                tracing::warn!(error = %e, "采集前台应用失败");
                continue;
            }
        };
        let idle = state.tracker.get_idle_seconds().unwrap_or(0);
        // DEBUG 级：每次 tick 的切应用详情（生产默认关，排查时 RUST_LOG=debug）
        tracing::debug!(
            app = %fg.process_name,
            idle = idle,
            "foreground tick"
        );
        let threshold = *state.idle_threshold.lock().unwrap_or_else(|e| e.into_inner());
        let now = Local::now();
        // 分钟聚合：累计到下一分钟开头统一写一条 INFO
        let current_minute = now.timestamp() / 60;
        if last_summary_minute != Some(current_minute) {
            if let Some(_m) = last_summary_minute {
                tracing::info!(
                    ticks = tick_in_minute,
                    switches = switch_in_minute,
                    last_app = %last_app_for_minute,
                    "采样循环分钟摘要"
                );
            }
            last_summary_minute = Some(current_minute);
            tick_in_minute = 0;
            switch_in_minute = 0;
        }
        tick_in_minute += 1;
        if !last_app_for_minute.is_empty() && last_app_for_minute != fg.process_name {
            switch_in_minute += 1;
        }
        last_app_for_minute = fg.process_name.clone();
        let platform = platform_name();
        // 用规则引擎分类（窗口标题 / 进程名 / 路径 / 包名综合判定）
        // v0.4.0：规则匹配前先用 categorizer 本地字典 + Wikipedia 联网查 + 缓存；
        // 用户自定义规则仍优先（classify_app 按 priority 倒序评估）
        // v0.4.1 关键 bugfix #3：categorizer::lookup_category 内部走 Wikipedia
        // HTTP（最多 8s）。v0.4.0 改为同步版本后会在**采样循环里**直接阻塞
        // tokio worker thread，导致 `get_current_foreground` 等同线程命令全部
        // 卡死 + 2s 采样节拍被破坏。改为 `spawn_blocking` 派发到 blocking
        // thread pool，sampling_loop 立刻继续 tick 不被阻塞。
        let category = {
            // 先尝试用户规则（高 priority 优先，不联网）
            let user_cat = classify_app(&fg, &state.rules.lock().unwrap_or_else(|e| e.into_inner()));
            if user_cat != "other" {
                user_cat
            } else {
                // 命中本地字典的常见软件（60+ 词条）也直接返回，避免联网
                if let Some(c) = crate::categorizer::lookup_local_only(
                    &fg.process_name,
                    fg.exe_path.as_deref(),
                    &fg.name,
                ) {
                    c
                } else {
                    // 真正需要联网的少数情况：丢到 blocking pool
                    // （注意：用户规则已不在这里查，所以不持 state.rules 锁）
                    let pn = fg.process_name.clone();
                    let nm = fg.name.clone();
                    let ep = fg.exe_path.clone();
                    // CategoryCache 内部用 Mutex 保护，已经线程安全，
                    // 可以直接 clone 引用给 spawn_blocking（无需 Arc 包一层）
                    let cache = state.category_cache.clone();
                    let other = tauri::async_runtime::spawn_blocking(move || {
                        crate::categorizer::lookup_category(&pn, ep.as_deref(), &nm, &cache)
                    })
                    .await
                    .unwrap_or_else(|e| {
                        tracing::error!(error = %e, "spawn_blocking(lookup_category) 失败");
                        "other".to_string()
                    });
                    other
                }
            }
        };

        // 阶段1：在锁内判断是否需要关闭旧 session
        // 触发关闭的三个条件：①应用切换 ②空闲超阈值 ③跨午夜（电脑不关机+托盘常驻的核心场景）
        // 跨午夜时把当前 session 用旧日期落库，再由阶段3按新日期开新 session，
        // 避免整段跨日时长被错算到「打开当天」。
        let to_finalize: Option<ActiveSession> = {
            let mut cur = state.current.lock().unwrap_or_else(|e| e.into_inner());
            let finalize = match cur.as_ref() {
                None => false,
                Some(a) => {
                    let same = a.app.process_name == fg.process_name
                        && a.app.exe_path == fg.exe_path;
                    // 跨日期：session 起始日 ≠ 当前日（凌晨后第一次 tick 即触发）
                    let date_changed = a.started_at.date_naive() != now.date_naive();
                    !same || idle >= threshold || date_changed
                }
            };
            if finalize {
                cur.take()
            } else {
                if let Some(a) = cur.as_mut() {
                    a.last_input_at = now;
                }
                None
            }
        };

        // 阶段2：关闭旧 session（已释放锁，可安全写库）
        // 跨午夜场景：当日 session 落库到昨日 date，下一 tick 由阶段3按新日期建新 session
        if let Some(active) = to_finalize {
            finalize_active_session(&state, &active, now);
        }

        // 阶段3：若无进行中 session 则新建
        {
            let mut cur = state.current.lock().unwrap_or_else(|e| e.into_inner());
            if cur.is_none() {
                let process_name = fg.process_name.clone();
                // v0.4.1 修复：DB 写入失败时**不能**静默用 app_id=0——会导致后续
                // insert_session 触发 FK 约束失败，整段 session 数据丢失且 UI 无感知。
                // 改为：失败时跳过本 tick 的 session 创建（让采样循环下一 tick 再试），
                // 并打印 stderr 供用户诊断。
                let app_id = match state.db.upsert_app(
                    &fg.name,
                    &fg.process_name,
                    fg.exe_path.as_deref(),
                    &category,
                    platform,
                ) {
                    Ok(id) => id,
                    Err(e) => {
                        tracing::error!(
                            error = %e,
                            process = %fg.process_name,
                            name = %fg.name,
                            "upsert_app 失败，跳过本 tick"
                        );
                        continue;
                    }
                };
                *cur = Some(ActiveSession {
                    app: fg,
                    app_id,
                    category_id: category.clone(),
                    started_at: now,
                    last_input_at: now,
                });
                // 自动归类引擎的「清单」补全：如果是全新 process_name 且尚未有规则，
                // 后台自动插入一条低优先级（priority=0）规则，分类用当前 classify 结果。
                // 这样用户能在「分类规则」页面看到所有用过的应用并按需调整。
                // 用 tokio::spawn 异步写库，避免阻塞采样循环。
                let already_covered = state
                    .rules
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|r| r.enabled && r.field == "process_name" && r.pattern == process_name);
                if !already_covered {
                    let st = Arc::clone(&state);
                    let pn = process_name.clone();
                    let cat = category.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Ok(rule_id) = st
                            .db
                            .insert_rule("process_name", "equals", &pn, &cat, 0)
                        {
                            // 重新加载内存缓存（让后续采样循环立即看到新规则）
                            if let Ok(new_rules) = st.db.load_rules() {
                                if let Ok(mut guard) = st.rules.lock() {
                                    *guard = new_rules;
                                }
                            }
                            let _ = rule_id;
                        }
                    });
                }
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
    //
    // v0.4.1 修复：DST fall-back 当天模糊时刻 `and_local_timezone().single()` 返回 None，
    // 原代码直接 unwrap 会 panic；改为取 latest()（DST 回拨后的下一个时刻，比 single 更稳）。
    // 同步修 `and_hms_opt` 用 ok_or_else 兜底（虽然 0:00:00 永远合法，但 year 9999 边界的
    // from_ymd_opt 可能 None；统一兜底避免 panic）。
    let to_local = |d: NaiveDate, h: u32, mi: u32, s: u32| -> DateTime<Local> {
        d.and_hms_opt(h, mi, s)
            .and_then(|dt| dt.and_local_timezone(Local).latest())
            .unwrap_or_else(|| now)
    };
    // 把「年月日」构造为 NaiveDate 失败时 fallback 到 today（year 10000 边界等极端情况）
    let ymd = |y: i32, m: u32, d: u32| -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap_or(today)
    };
    if period == "month" {
        let y = today.year();
        let m = today.month();
        // 本期：本月 1 号 00:00 → 现在
        let cur_start = to_local(ymd(y, m, 1), 0, 0, 0);
        let cur_end = now;
        let (py, pm) = if m == 1 { (y - 1, 12) } else { (y, m - 1) };
        // 上期：上月 1 号 → 上月最后一天 23:59:59
        let prev_start = to_local(ymd(py, pm, 1), 0, 0, 0);
        let prev_last = last_day_of_month(py, pm);
        let prev_end = to_local(ymd(py, pm, prev_last), 23, 59, 59);
        // 同比：去年同月
        let yoy_y = y - 1;
        let yoy_start = to_local(ymd(yoy_y, m, 1), 0, 0, 0);
        let yoy_last = last_day_of_month(yoy_y, m);
        let yoy_end = to_local(ymd(yoy_y, m, yoy_last), 23, 59, 59);
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

/// 用系统默认浏览器打开 URL
/// - Tauri 2 的 WebView 默认拦截 `<a target="_blank">`，所以需要通过 Rust 跳出去
/// - 用于「检查更新」结果里的「前往下载」按钮（指向 GitHub Release 页面）
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    // 简易防护：只允许 http(s) 协议，避免被诱导执行本地命令
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(format!("不允许的 URL 协议：{}", url));
    }
    // 跨平台：用 cfg 切换实现，避免 macOS/Linux 上 `open` crate 不可见
    #[cfg(target_os = "windows")]
    {
        open::that(&url).map_err(|e| format!("打开 URL 失败：{}", e))
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("打开 URL 失败：{}", e))?;
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("打开 URL 失败：{}", e))?;
        Ok(())
    }
}

/// 检查更新结果（前端 Settings.vue「检查更新」按钮用）
#[derive(serde::Serialize, Clone)]
pub struct UpdateInfo {
    /// 当前版本（来自 tauri.conf.json）
    pub current: String,
    /// 远端最新版本（GitHub latest release tag）
    pub latest: String,
    /// 远端是否有更新（按 SemVer 严格比较，仅 0.X.Y 之间）
    pub has_update: bool,
    /// 最新 release 的 HTML 页面 URL（点击跳转下载）
    pub url: String,
    /// release notes / body（前端可截断展示）
    pub notes: String,
}

/// 从 GitHub Releases API 拉取最新版本，与当前版本（tauri.conf.json）对比
///
/// 设计：
/// - 不阻塞 UI：前端按钮点击后由 Rust 在 Tokio 线程中跑 HTTP GET
/// - 超时 10s：网络不通时快速失败，不让用户干等
/// - 不解析 SemVer：仅按「点分三段 → 转数字 → 逐位比较」；预发布版（-alpha/-beta）按字符串前缀比较忽略（即视为 < 对应数字）
#[tauri::command]
pub async fn check_for_update(app: tauri::AppHandle) -> Result<UpdateInfo, String> {
    // 从 tauri.conf.json 读当前版本（编译期常量更稳妥，但运行时读配置也够用）
    let current = app
        .config()
        .version
        .clone()
        .unwrap_or_else(|| "0.0.0".to_string());

    // 调 GitHub Releases Atom feed（无 60/hr rate limit，比 REST API 稳）
    // 用 atom feed 替代 /repos/.../releases/latest：
    //   - REST API 未授权每小时 60 次限制 + UA 严格 → 容易触发 403 rate limit
    //   - Atom feed 是公开订阅源，无频率限制，对 User-Agent 不严
    //   - 失败时回退到 REST API
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("ScreenTime-Pro-Update-Checker")
        .build()
        .map_err(|e| format!("构建 HTTP 客户端失败: {}", e))?;

    let mut latest_info: Option<(String, String, String)> = None; // (version, url, body)
    let mut last_err: Option<String> = None;

    // 策略 1：Atom feed（最稳）
    match client
        .get("https://github.com/Slk90s/screentime-pro/releases.atom")
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            match resp.text().await {
                Ok(xml) => {
                    if let Some(info) = parse_atom_latest_release(&xml) {
                        latest_info = Some(info);
                    } else {
                        last_err = Some("Atom feed 解析失败（未找到 release 条目）".to_string());
                    }
                }
                Err(e) => last_err = Some(format!("Atom feed 读取失败: {}", e)),
            }
        }
        Ok(resp) => {
            last_err = Some(format!("Atom feed 返回 HTTP {}", resp.status().as_u16()));
        }
        Err(e) => last_err = Some(format!("Atom feed 请求失败: {}", e)),
    }

    // 策略 2：REST API（fallback）
    if latest_info.is_none() {
        match client
            .get("https://api.github.com/repos/Slk90s/screentime-pro/releases/latest")
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                #[derive(serde::Deserialize)]
                struct GhRelease {
                    tag_name: String,
                    html_url: String,
                    body: Option<String>,
                }
                match resp.json::<GhRelease>().await {
                    Ok(gh) => {
                        let latest = gh.tag_name.trim_start_matches('v').to_string();
                        latest_info = Some((latest, gh.html_url, gh.body.unwrap_or_default()));
                    }
                    Err(e) => last_err = Some(format!("REST API 解析失败: {}", e)),
                }
            }
            Ok(resp) => {
                // 把 status 取出后再读 body（避免 borrow moved value）
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                // v0.4.1 修复：用 chars().take(N) 截断避免 &body[..200] 跨 UTF-8 边界 panic
                // （GitHub 错误体常含中文/emoji，按字节切可能在多字节字符中间）
                let body_short: String = body.chars().take(200).collect();
                let body_short = if body.len() > body_short.len() {
                    format!("{}…", body_short)
                } else {
                    body
                };
                last_err = Some(format!("REST API 返回 HTTP {}：{}", status.as_u16(), body_short));
            }
            Err(e) => last_err = Some(format!("REST API 请求失败: {}", e)),
        }
    }

    let (latest, url, notes) = match latest_info {
        Some(v) => v,
        None => {
            return Err(last_err.unwrap_or_else(|| "未知错误：所有检查更新策略均失败".to_string()));
        }
    };
    let has_update = semver_gt(&latest, &current);
    Ok(UpdateInfo {
        current,
        latest,
        has_update,
        url,
        notes,
    })
}

/// 从 GitHub Atom feed XML 中解析最新一条 release 的 tag/url/summary
/// Atom 结构示例：
///   <entry>
///     <title>Release v0.3.0</title>
///     <link href="https://github.com/Slk90s/screentime-pro/releases/tag/v0.3.0"/>
///     <summary>...</summary>
///   </entry>
fn parse_atom_latest_release(xml: &str) -> Option<(String, String, String)> {
    use regex::Regex;
    // 抓第一个 <entry>...</entry> 块
    let entry_re = Regex::new(r"<entry[\s\S]*?</entry>").ok()?;
    let entry = entry_re.find(xml)?;
    let block = entry.as_str();

    // title: <title>ScreenTime Pro v0.3.0</title> → "0.3.0"
    let title_re = Regex::new(r"<title[^>]*>([\s\S]*?)</title>").ok()?;
    let raw_title = title_re.captures(block)?.get(1)?.as_str().trim().to_string();
    // GitHub Atom feed 的 title 实际是 "ScreenTime Pro vX.Y.Z"（也可能用 "Release vX.Y.Z"）
    // → 逐步剥离常见前缀，最终保留 "vX.Y.Z" → 再去 v → "X.Y.Z"
    let tag = raw_title
        .trim_start_matches("ScreenTime Pro ")
        .trim_start_matches("Release ")
        .trim_start_matches("release ")
        .trim()
        .trim_start_matches('v')
        .to_string();

    // link: <link href="..."/> → url
    let link_re = Regex::new(r#"<link[^>]*href="([^"]+)""#).ok()?;
    let url = link_re.captures(block)?.get(1)?.as_str().to_string();

    // summary: <summary>...</summary>（可能没有，做 best-effort）
    let sum_re = Regex::new(r"<summary[^>]*>([\s\S]*?)</summary>").ok()?;
    let notes = sum_re
        .captures(block)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_default();

    if tag.is_empty() {
        return None;
    }
    Some((tag, url, notes))
}

/// SemVer 字符串比较：仅 0.X.Y 之间逐位数字比较，剪掉前缀 'v'
/// 返回 latest > current
fn semver_gt(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.split('.')
            .filter_map(|p| p.split('-').next().unwrap_or("").parse::<u32>().ok())
            .collect()
    };
    let l = parse(latest);
    let c = parse(current);
    for i in 0..3 {
        let lv = l.get(i).copied().unwrap_or(0);
        let cv = c.get(i).copied().unwrap_or(0);
        if lv > cv {
            return true;
        }
        if lv < cv {
            return false;
        }
    }
    false
}
