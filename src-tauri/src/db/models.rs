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
