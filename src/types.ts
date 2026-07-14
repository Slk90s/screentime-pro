// 与后端 src-tauri/src/db/models.rs 中的 serde 结构一一对应

export interface AppInfoOut {
  id: number;
  name: string;
  process_name: string;
  category_id: string;
  icon_base64?: string | null;
}

export interface SessionOut {
  id: number;
  app_id: number;
  app_name: string;
  category_id: string;
  start_at: string;
  end_at: string;
  duration_seconds: number;
}

export interface DailySummaryOut {
  date: string;
  total_seconds: number;
  app_count: number;
}

// 按天 × 分类的时长明细（iOS 风格堆叠柱状图用）
export interface DayCategoryOut {
  date: string;
  category_id: string;
  total_seconds: number;
}

export interface HourlyBucketOut {
  hour: number;
  category_id: string;
  total_seconds: number;
}

export interface AppRankingOut {
  app_id: number;
  app_name: string;
  category_id: string;
  total_seconds: number;
  session_count: number;
  icon_base64?: string | null;
}

export interface CategoryOut {
  id: string;
  name: string;
  color: string;
}

export interface OverviewOut {
  date: string;
  total_seconds: number;
  app_count: number;
  most_used_app?: string | null;
  most_used_seconds: number;
  pickup_count: number;
  /** 日均时长（秒）：仅范围聚合模式（days>0）时由后端计算，单日模式为 0 */
  avg_daily_seconds?: number;
}

// ⚠️ Tauri 2 自动把 Rust 字段转 camelCase 给 JS，所以 TS 类型必须用 camelCase。
// 否则 vue-tsc 会报 TS2551 「Property 'X' does not exist on type ...」，
// 且运行时访问 snake_case 字段会得到 undefined（v0.4.0 之前就是这样导致
// 「已记录 X秒」永不显示）。
export interface CurrentForegroundOut {
  name: string;
  processName: string;
  categoryId: string;
  idleSeconds: number;
  tracking: boolean;
  windowTitle?: string | null;
  sessionSeconds: number; // 当前前台应用已连续运行时长（秒）
}

export interface ExportResult {
  path: string;
}

// 系统权限状态（对应后端 PermissionStatus）
// accessibility=辅助功能权限（空闲检测必须），screen_capture=屏幕录制权限
export interface PermissionStatus {
  accessibility: boolean;
  screen_capture: boolean;
}

// WebView2 运行时检测结果（仅 Windows 真正生效，其他平台 available=true）
export interface Webview2Status {
  os: string;
  available: boolean;
  version: string;
  hint: string;
}

// 检查更新结果（GitHub Releases API，对应后端 UpdateInfo）
export interface UpdateInfo {
  current: string;
  latest: string;
  has_update: boolean;
  url: string;
  notes: string;
}

// 单设备聚合统计（用于「按设备清理」弹窗，对应后端 DeviceStats）
export interface DeviceStats {
  device_id: string;
  device_name: string;
  total_seconds: number;
  session_count: number;
  earliest_date: string;
  latest_date: string;
}

// 分类规则（对应后端 RuleOut / classification_rules 表）
// field: 匹配字段（process_name/window_title/exe_path/bundle_id/name）
// match_type: contains(包含)/equals(相等)/prefix(前缀)/suffix(后缀)/regex(正则)
export interface RuleOut {
  id: number;
  field: string;
  match_type: string;
  pattern: string;
  category_id: string;
  priority: number;
  enabled: boolean;
}

// ===== 周/月同比分析（对应后端 TrendsOut / PeriodStat 等）=====
export interface CategorySeconds {
  category_id: string;
  total_seconds: number;
}
export interface AppSeconds {
  app_name: string;
  category_id: string;
  total_seconds: number;
}
// 单个统计周期的聚合结果
export interface PeriodStat {
  label: string;
  total_seconds: number;
  app_count: number;
  by_category: CategorySeconds[];
  top_apps: AppSeconds[];
}
// 趋势对比输出：本期 / 上期(环比) / 去年同期(同比)
export interface TrendsOut {
  period: string; // "week" | "month"
  current: PeriodStat;
  prev: PeriodStat;
  yoy?: PeriodStat | null;
  delta_total_pct: number; // 本期相对上期的时长变化百分比
}

// ===== 多设备合并（对应后端 DeviceInfo / SettingsOut）=====
export interface DeviceInfo {
  id: string;
  name: string;
}
// 设置项（设置页用）
export interface SettingsOut {
  device_id: string;
  device_name: string;
  idle_threshold: number; // 空闲阈值（秒）
  data_retention_days: number; // 数据保留天数
  sample_interval: number; // 采样间隔（秒）
  autostart: boolean; // 是否开机自启
}

// ===== 全量导出 / 导入合并（对应后端 ExportBundle）=====
export interface ExportApp {
  name: string;
  process_name: string;
  exe_path: string;
  category_id: string;
  platform: string;
}
export interface ExportSession {
  app_process: string;
  app_platform: string;
  start_at: string;
  end_at: string;
  duration_seconds: number;
  date: string;
  window_title: string;
  device: string;
}
export interface ExportBundle {
  version: number;
  exported_at: string;
  devices: Record<string, string>;
  apps: ExportApp[];
  sessions: ExportSession[];
}
