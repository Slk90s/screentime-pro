//! db/models.rs
//! IPC 与导出用的数据传输对象（DTO）集合——后端返回给前端 / 写入备份 JSON 的结构体定义。
//!
//! 约定：
//! - 这些结构体**只做序列化**（`#[derive(Serialize)]`），不参与数据库表映射；
//!   数据库实体与 SQL 在 `db/mod.rs`，命令层在 `commands.rs`。
//! - 字段命名沿用 DB/API 的 `snake_case`（见 CONVENTIONS-screentime-pro.md 四层命名），
//!   由全局拦截器约定透传，**勿改成 camelCase**。
//! - `ExportBundle` 三件套（`ExportApp` / `ExportSession` / `ExportBundle`）是备份 JSON 的
//!   对外契约，字段增删需同步 `commands::import_data` 的解析逻辑与 `docs/`。
//! - `MetricsOut` 的百分比字段是 **0.0~1.0 分数**，前端展示须 ×100（见 metrics.rs 教训）。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct AppInfoOut {
    pub id: i64,
    pub name: String,
    pub process_name: String,
    pub category_id: String,
    pub icon_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionOut {
    pub id: i64,
    pub app_id: i64,
    pub app_name: String,
    pub category_id: String,
    pub start_at: String,
    pub end_at: String,
    pub duration_seconds: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailySummaryOut {
    pub date: String,
    pub total_seconds: i64,
    pub app_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DayCategoryOut {
    pub date: String,
    pub category_id: String,
    pub total_seconds: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct HourlyBucketOut {
    pub hour: u32,
    pub category_id: String,
    pub total_seconds: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppRankingOut {
    pub app_id: i64,
    pub app_name: String,
    pub category_id: String,
    pub total_seconds: i64,
    pub session_count: i64,
    pub icon_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CategoryOut {
    pub id: String,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverviewOut {
    pub date: String,
    pub total_seconds: i64,
    pub app_count: i64,
    pub most_used_app: Option<String>,
    pub most_used_seconds: i64,
    pub pickup_count: i64,
    pub avg_daily_seconds: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonthSummaryOut {
    pub year: i32,
    pub month: i32,
    pub total_seconds: i64,
    pub active_days: i64,
    pub days_in_month: i64,
    pub avg_daily_seconds: i64,
    pub top_app: Option<String>,
    pub top_app_seconds: i64,
    pub busiest_date: Option<String>,
    pub busiest_seconds: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CurrentForegroundOut {
    pub name: String,
    pub process_name: String,
    pub category_id: String,
    pub idle_seconds: u64,
    pub tracking: bool,
    pub window_title: Option<String>,
    pub bundle_id: Option<String>,
    pub session_seconds: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportResult {
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleOut {
    pub id: i64,
    pub field: String,
    pub match_type: String,
    pub pattern: String,
    pub category_id: String,
    pub priority: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PermissionStatus {
    pub accessibility: bool,
    pub screen_capture: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CategorySeconds {
    pub category_id: String,
    pub total_seconds: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppSeconds {
    pub app_name: String,
    pub category_id: String,
    pub total_seconds: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeriodStat {
    pub label: String,
    pub total_seconds: i64,
    pub app_count: i64,
    pub by_category: Vec<CategorySeconds>,
    pub top_apps: Vec<AppSeconds>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrendsOut {
    pub period: String,
    pub current: PeriodStat,
    pub prev: PeriodStat,
    pub yoy: Option<PeriodStat>,
    pub delta_total_pct: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SettingsOut {
    pub device_id: String,
    pub device_name: String,
    pub idle_threshold: u64,
    pub data_retention_days: u32,
    pub sample_interval: u64,
    pub autostart: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportApp {
    pub name: String,
    pub process_name: String,
    pub exe_path: Option<String>,
    pub category_id: String,
    pub platform: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportSession {
    pub app_process: String,
    pub app_platform: String,
    pub start_at: String,
    pub end_at: String,
    pub duration_seconds: i64,
    pub date: String,
    pub window_title: Option<String>,
    pub device: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportBundle {
    pub version: u32,
    pub exported_at: String,
    pub devices: std::collections::HashMap<String, String>,
    pub apps: Vec<ExportApp>,
    pub sessions: Vec<ExportSession>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsOut {
    pub supported: bool,
    pub enabled: bool,
    pub cpu_usage: f32,
    pub memory_usage: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub disk_usage: f32,
    pub disk_used_bytes: u64,
    pub disk_total_bytes: u64,
    // v0.7.6 新增：网络速率
    pub net_rx_bps: f64,
    pub net_tx_bps: f64,
}

/// v0.8.0（2026-09-16）：截图历史条目。
///
/// 图片本体落在 `<app_data_dir>/screenshots/`，本表只存索引（保持 DB 轻量）。
/// 字段名即 IPC 返回值（Tauri v2 不转换返回值），前端按 snake_case 读取。
#[derive(Debug, Clone, Serialize)]
pub struct ScreenshotOut {
    pub id: i64,
    pub file_name: String,
    pub width: u32,
    pub height: u32,
    pub bytes: i64,
    pub created_at: String,
}
