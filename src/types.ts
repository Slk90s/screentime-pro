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
  // v0.8.2（2026-09-17）：截图总开关透传（浮窗尾部截图按钮的显隐依据；
  // 权威源是 screenshot 模块配置，本字段只读不回写）
  screenshot_enabled: boolean;
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

// ===== v0.8.0（2026-09-16）：屏幕截图 =====

/** 截图配置（字段名与 Rust `ScreenshotConfig` 一致，Tauri v2 不转换结构体字段名） */
export interface ScreenshotConfig {
  enabled: boolean;
  /** 全局快捷键（global-hotkey 语法，如 CmdOrCtrl+Shift+A） */
  shortcut: string;
  /** 确认时默认动作 = 复制到剪贴板 */
  auto_copy: boolean;
  /** 确认时同时落盘归档 */
  auto_save: boolean;
  corner_radius: number;
  shadow: boolean;
  /** 历史 FIFO 上限 */
  max_count: number;
  /**
   * 取字引擎（v0.9.0）：`system` = 系统内置（WinRT）/ `enhanced` = PaddleOCR-ONNX 本地模型。
   * Rust 侧读侧有兜底：未知值一律回 `system`，所以这里即使拿到脏值也不会让取字不可用。
   */
  ocr_engine: OcrEngineKind;
}

/** 取字引擎类型（与 Rust `ocr_engine::EngineKind` 对齐） */
export type OcrEngineKind = "system" | "enhanced";

/**
 * 取字引擎信息（Rust `screenshot::ocr_engine::EngineInfo`）。
 * 设置页用它决定「增强」选项是否可选、以及显示随包体积。
 */
export interface OcrEngineInfo {
  /** 当前选中的引擎 */
  kind: OcrEngineKind;
  /** 标准引擎在本平台是否可用（Windows = WinRT；macOS = Vision；Linux 暂无） */
  system_available: boolean;
  /** 标准引擎的具体实现（"winrt" / "vision"；无系统引擎时为空串） */
  system_engine: "winrt" | "vision" | "";
  /** 增强引擎是否可用（运行库 + 模型齐备，选了就能直接用） */
  enhanced_ready: boolean;
  /**
   * 增强引擎**模型**是否齐备（三端随包；false = 安装不完整）。
   * 这是「增强」选项能否被选中的判据 —— 不要用 enhanced_ready，见下。
   */
  enhanced_models_ready: boolean;
  /**
   * 增强引擎**运行库**是否就绪。
   *
   * ⚠️ macOS / Linux 的运行库**不随包**，缺失是正常初始状态（首次使用后台下载）。
   * 若拿 `enhanced_ready`（= 运行库 && 模型）当「可选中」判据，mac 用户会死锁：
   * 运行库没下载 → 选项置灰 → 而下载只在选中后才触发 → 永远选不中。
   */
  enhanced_runtime_ready: boolean;
  /** 运行库绝对路径（缺失为 null，便于排障） */
  ort_lib: string | null;
  /** 模型目录（缺失为 null） */
  models_dir: string | null;
  /** 增强引擎随包体积（MB） */
  enhanced_size_mb: number;
}

/**
 * 保存截图配置后的**快捷键真实注册结果**（Rust `screenshot::ApplyResult`）。
 * 初版 setConfig 无论成败都返回 true，导致「填了被占用的组合 → 界面说成功、
 * 快捷键其实是死的」；现在失败原因会带回来给设置页提示。
 */
export interface ShortcutApplyResult {
  applied: boolean;
  shortcut: string;
  error: string | null;
}

/** 遮罩窗收到「本次截图就绪」时的元信息（整屏图另由 screenshot_frame 拉取） */
export interface CaptureReadyPayload {
  width: number;
  height: number;
  physical_width: number;
  physical_height: number;
  scale_factor: number;
  default_radius: number;
  default_shadow: boolean;
}

/** 截图历史条目（图片本体在 screenshots/ 目录，此处只有索引） */
export interface ScreenshotOut {
  id: number;
  file_name: string;
  width: number;
  height: number;
  bytes: number;
  created_at: string;
}

/** 截图提交结果 */
export interface ScreenshotResult {
  id: number | null;
  file_name: string | null;
  path: string | null;
  width: number;
  height: number;
  copied: boolean;
  saved: boolean;
}

/** 取字（本地离线 OCR）结果 */
export interface OcrOut {
  /** 识别出的完整文本，多行以 \n 分隔 */
  text: string;
  /** 参与识别的图像尺寸（物理像素） */
  width: number;
  height: number;
  /** 本次生效的识别语言标签（如 zh-Hans-CN）；增强引擎为 PP-OCR 的 `ch` */
  language: string | null;
  /** 本次真正生效的引擎：system（系统内置）/ enhanced（PaddleOCR-ONNX） */
  engine: OcrEngineKind;
  /** 识别出的行数（增强引擎按行输出；标准引擎只有整段文本，为 0） */
  lines: number;
}
