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
  avg_daily_seconds?: number;
}

export interface MonthSummaryOut {
  year: number;
  month: number;
  total_seconds: number;
  active_days: number;
  days_in_month: number;
  avg_daily_seconds: number;
  top_app?: string | null;
  top_app_seconds: number;
  busiest_date?: string | null;
  busiest_seconds: number;
}

export interface CurrentForegroundOut {
  name: string;
  process_name: string;
  category_id: string;
  idle_seconds: number;
  tracking: boolean;
  window_title?: string | null;
  session_seconds: number;
}

export interface ExportResult {
  path: string;
}

export interface BackupConfig {
  enabled: boolean;
  path: string;
  keep_days: number;
  last_date: string;
}

export interface PermissionStatus {
  accessibility: boolean;
  screen_capture: boolean;
}

export interface Webview2Status {
  os: string;
  available: boolean;
  version: string;
  hint: string;
}

export interface UpdateInfo {
  current: string;
  latest: string;
  has_update: boolean;
  url: string;
  notes: string;
}

export interface DeviceStats {
  device_id: string;
  device_name: string;
  total_seconds: number;
  session_count: number;
  earliest_date: string;
  latest_date: string;
}

export interface RuleOut {
  id: number;
  field: string;
  match_type: string;
  pattern: string;
  category_id: string;
  priority: number;
  enabled: boolean;
}

export interface CategorySeconds {
  category_id: string;
  total_seconds: number;
}
export interface AppSeconds {
  app_name: string;
  category_id: string;
  total_seconds: number;
}
export interface PeriodStat {
  label: string;
  total_seconds: number;
  app_count: number;
  by_category: CategorySeconds[];
  top_apps: AppSeconds[];
}
export interface TrendsOut {
  period: string;
  current: PeriodStat;
  prev: PeriodStat;
  yoy?: PeriodStat | null;
  delta_total_pct: number;
}

export interface DeviceInfo {
  id: string;
  name: string;
}
export interface SettingsOut {
  device_id: string;
  device_name: string;
  idle_threshold: number;
  data_retention_days: number;
  sample_interval: number;
  autostart: boolean;
}

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

// ===== v0.7.6：状态栏配置（取代 v0.7.5 单字段 metrics_enabled）=====

export interface StatusBarConfig {
  enabled: boolean;
  show_cpu: boolean;
  show_mem: boolean;
  // v0.7.7（2026-09-10）：磁盘占用开关（三平台均已支持采样，默认关）
  show_disk: boolean;
  show_net: boolean;
  // v0.7.6（2026-09-10）：悬浮指标条独立开关（与托盘状态栏总开关互不依赖）
  float_enabled: boolean;
}

// ===== v0.7.5 / v0.7.6：状态栏系统指标 =====

export interface MetricsOut {
  supported: boolean;
  enabled: boolean;
  cpu_usage: number;
  memory_usage: number;
  memory_used_bytes: number;
  memory_total_bytes: number;
  disk_usage: number;
  disk_used_bytes: number;
  disk_total_bytes: number;
  // v0.7.6 新增：网络速率（bytes/sec）
  net_rx_bps: number;
  net_tx_bps: number;
}
