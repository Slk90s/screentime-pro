// 前端与 Rust 后端的通信封装
//
// 设计要点：双模运行
// - 在 Tauri 运行时内：通过 `invoke` 调用真实 Rust 命令
// - 在普通浏览器内（仅看 UI / 调试）：自动走 `mock` 假数据
// 这样前端可以脱离 Rust 编译单独预览，互不阻塞。

import { invoke } from "@tauri-apps/api/core";
import type {
  AppRankingOut,
  CategoryOut,
  CurrentForegroundOut,
  DailySummaryOut,
  DayCategoryOut,
  DeviceInfo,
  ExportResult,
  HourlyBucketOut,
  OverviewOut,
  PermissionStatus,
  RuleOut,
  SessionOut,
  SettingsOut,
  TrendsOut,
  Webview2Status,
} from "../types";
import { mock } from "./mock";

// 检测是否运行在 Tauri 运行时内
declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}
const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

// 统一调用入口：不在 Tauri 内就返回 mock 数据
async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri) {
    return mock(cmd, args) as T;
  }
  return invoke<T>(cmd, args);
}

// 对外暴露的 API 集合（对应后端 commands.rs 中的各个命令）
export const tracker = {
  // 开始 / 停止 / 查询追踪状态
  start: () => call<boolean>("start_tracking"),
  stop: () => call<boolean>("stop_tracking"),
  isTracking: () => call<boolean>("is_tracking"),
  // 实时前台应用（用于 UI 显示「正在记录：XXX」，验证确实在采集其他软件）
  current: () => call<CurrentForegroundOut>("get_current_foreground"),
  // 各维度统计（支持按设备过滤：device 为空表示合并全部设备）
  overview: (date: string, device?: string) =>
    call<OverviewOut>("get_overview", { date, device: device ?? null }),
  daily: (days: number, device?: string) =>
    call<DailySummaryOut[]>("get_daily_summaries", { days, device: device ?? null }),
  // 按天 × 分类明细（iOS 风格堆叠柱状图）
  dailyCategories: (days: number, device?: string) =>
    call<DayCategoryOut[]>("get_daily_categories", { days, device: device ?? null }),
  hourly: (date: string, device?: string) =>
    call<HourlyBucketOut[]>("get_hourly_buckets", { date, device: device ?? null }),
  ranking: (date: string, device?: string) =>
    call<AppRankingOut[]>("get_app_ranking", { date, device: device ?? null }),
  categories: () => call<CategoryOut[]>("get_categories"),
  sessions: (date: string) => call<SessionOut[]>("get_sessions", { date }),
  // 空闲阈值配置
  setIdle: (secs: number) => call<boolean>("set_idle_threshold", { secs }),
  getIdle: () => call<number>("get_idle_threshold"),
  // 数据导出（CSV / JSON）
  exportData: (date: string, format: string) =>
    call<ExportResult>("export_data", { date, format }),
  // 权限查询与引导（macOS）
  checkPermissions: () => call<PermissionStatus>("check_permissions"),
  openPrivacySettings: () => call<void>("open_privacy_settings"),
  // ===== WebView2 运行时检测（仅 Windows 真正生效） =====
  checkWebview2: () => call<Webview2Status>("check_webview2"),
  openWebview2Download: () => call<void>("open_webview2_download"),
  // ===== 周/月同比分析 =====
  trends: (period: string, device?: string) =>
    call<TrendsOut>("get_trends", { period, device: device ?? null }),
  // ===== 全量导出 / 导入合并 =====
  exportAll: () => call<ExportResult>("export_all"),
  importData: (content: string) => call<number>("import_data", { content }),
  pruneData: (days: number) => call<number>("prune_data", { days }),
  // 在系统文件管理器中打开路径（导出后定位备份文件）
  revealPath: (path: string) => call<void>("reveal_path", { path }),
  // ===== 多设备合并 =====
  devices: () => call<DeviceInfo[]>("get_devices"),
  getSettings: () => call<SettingsOut>("get_settings"),
  // 注意：Tauri 命令参数在 JS 侧为 camelCase（idleThreshold 等），必须用 camelCase 键名传参
  saveSettings: (s: {
    idleThreshold: number;
    deviceName: string;
    dataRetentionDays: number;
  }) => call<boolean>("save_settings", s),
  // ===== 分类规则引擎 =====
  rules: () => call<RuleOut[]>("get_rules"),
  addRule: (r: {
    field: string;
    match_type: string;
    pattern: string;
    category_id: string;
    priority: number;
  }) => call<number>("add_rule", r),
  updateRule: (r: {
    id: number;
    field: string;
    match_type: string;
    pattern: string;
    category_id: string;
    priority: number;
    enabled: boolean;
  }) => call<boolean>("update_rule", r),
  deleteRule: (id: number) => call<boolean>("delete_rule", { id }),
  reclassify: () => call<number>("reclassify_all"),
  // ===== 开机自启 =====
  setAutostart: (enabled: boolean) => call<boolean>("set_autostart", { enabled }),
  isAutostart: () => call<boolean>("is_autostart"),
  getAutostartPref: () => call<boolean | null>("get_autostart_pref"),
};
