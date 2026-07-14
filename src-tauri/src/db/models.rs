use serde::{Deserialize, Serialize};

/// 应用基础信息（输出给前端）
#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct AppInfoOut {
    pub id: i64,
    pub name: String,
    pub process_name: String,
    pub category_id: String,
    /// base64 编码的 PNG 图标（可选，缺失时前端用占位图）
    pub icon_base64: Option<String>,
}

/// 单条使用时段记录
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

/// 按天聚合的总览
#[derive(Debug, Clone, Serialize)]
pub struct DailySummaryOut {
    pub date: String,
    pub total_seconds: i64,
    pub app_count: i64,
}

/// 按天 × 分类的时长明细（iOS 风格堆叠柱状图用）
#[derive(Debug, Clone, Serialize)]
pub struct DayCategoryOut {
    pub date: String,
    pub category_id: String,
    pub total_seconds: i64,
}

/// 24 小时 × 分类的堆叠桶
#[derive(Debug, Clone, Serialize)]
pub struct HourlyBucketOut {
    pub hour: u32,
    pub category_id: String,
    pub total_seconds: i64,
}

/// App 使用时长排行项
#[derive(Debug, Clone, Serialize)]
pub struct AppRankingOut {
    pub app_id: i64,
    pub app_name: String,
    pub category_id: String,
    pub total_seconds: i64,
    pub session_count: i64,
    pub icon_base64: Option<String>,
}

/// 分类字典项
#[derive(Debug, Clone, Serialize)]
pub struct CategoryOut {
    pub id: String,
    pub name: String,
    pub color: String,
}

/// 当日/范围总览卡片
#[derive(Debug, Clone, Serialize)]
pub struct OverviewOut {
    pub date: String,
    pub total_seconds: i64,
    pub app_count: i64,
    pub most_used_app: Option<String>,
    pub most_used_seconds: i64,
    pub pickup_count: i64,
    /// 日均时长（秒）：仅范围聚合模式（days>0）时计算，单日模式为 0
    pub avg_daily_seconds: i64,
}

/// 实时前台应用
#[derive(Debug, Clone, Serialize)]
pub struct CurrentForegroundOut {
    pub name: String,
    pub process_name: String,
    pub category_id: String,
    pub idle_seconds: u64,
    pub tracking: bool,
    /// 前台窗口标题（macOS 需屏幕录制权限、Windows 默认可取；用于细粒度分类）
    pub window_title: Option<String>,
    /// 当前前台应用已连续运行的时长（秒）；无进行中时段时为 0
    pub session_seconds: i64,
}

/// 导出结果
#[derive(Debug, Clone, Serialize)]
pub struct ExportResult {
    pub path: String,
}

/// 分类规则（输出给前端管理界面，对应 classification_rules 表）
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

/// 系统权限状态（前端据此提示用户授权）
///
/// - `accessibility`：辅助功能权限，空闲检测必须
/// - `screen_capture`：屏幕录制权限，窗口标题级采集必须（当前未用到，预留）
/// 非 macOS 平台默认两项均为 true（无需授权）。
#[derive(Debug, Clone, Serialize)]
pub struct PermissionStatus {
    pub accessibility: bool,
    pub screen_capture: bool,
}

/// 周期内在某分类上的累计时长（周/月对比用）
#[derive(Debug, Clone, Serialize)]
pub struct CategorySeconds {
    pub category_id: String,
    pub total_seconds: i64,
}

/// 周期内单应用累计时长（Top 应用榜）
#[derive(Debug, Clone, Serialize)]
pub struct AppSeconds {
    pub app_name: String,
    pub category_id: String,
    pub total_seconds: i64,
}

/// 某一周期（本周/本月/上一周期/去年同期）的聚合统计
#[derive(Debug, Clone, Serialize)]
pub struct PeriodStat {
    /// 周期标签（如「2026年7月」「本周」）
    pub label: String,
    /// 周期总有效时长（秒）
    pub total_seconds: i64,
    /// 周期使用过的应用数
    pub app_count: i64,
    /// 各分类时长占比（降序）
    pub by_category: Vec<CategorySeconds>,
    /// Top 应用（前 10，降序）
    pub top_apps: Vec<AppSeconds>,
}

/// 趋势对比结果：本周期 vs 上一周期（环比）+ 去年同期（同比，仅月份）
#[derive(Debug, Clone, Serialize)]
pub struct TrendsOut {
    /// 周期类型：week | month
    pub period: String,
    /// 当前周期统计
    pub current: PeriodStat,
    /// 上一周期统计（环比基准）
    pub prev: PeriodStat,
    /// 去年同期统计（同比基准，仅月份有值）
    pub yoy: Option<PeriodStat>,
    /// 总时长环比变化百分比：(current - prev) / prev * 100
    pub delta_total_pct: f64,
}

/// 设备信息（多设备合并：区分不同机器的数据来源）
#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    /// 设备唯一 id（安装时生成，写入 settings）
    pub id: String,
    /// 设备展示名（默认取主机名，可改）
    pub name: String,
}

/// 设置项聚合（前端设置页读取/保存）
#[derive(Debug, Clone, Serialize)]
pub struct SettingsOut {
    /// 本机设备 id
    pub device_id: String,
    /// 本机设备名
    pub device_name: String,
    /// 空闲阈值（秒）：超过该时长无操作视为「离开」
    pub idle_threshold: u64,
    /// 数据保留天数（超过则清理）
    pub data_retention_days: u32,
    /// 采样间隔（秒，编译期固定，仅展示）
    pub sample_interval: u64,
    /// 是否开机自启
    pub autostart: bool,
}

/// 导出包中的应用条目（多设备合并导入用，按 process_name+platform 去重）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportApp {
    pub name: String,
    pub process_name: String,
    pub exe_path: Option<String>,
    pub category_id: String,
    pub platform: String,
}

/// 导出包中的时段条目（携带来源 device 标签，用于多设备区分）
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

/// 全量导出包：用于跨设备数据合并（隐私优先，纯本地文件交换）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportBundle {
    pub version: u32,
    pub exported_at: String,
    /// 设备 id -> 设备名 映射
    pub devices: std::collections::HashMap<String, String>,
    pub apps: Vec<ExportApp>,
    pub sessions: Vec<ExportSession>,
}
