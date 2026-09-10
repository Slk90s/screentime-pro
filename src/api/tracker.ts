/**
 * tracker.ts
 * 设计思路：前端与 Rust 后端的通信封装。双模运行——Tauri 内通过 `invoke` 调用真实 Rust 命令，
 * 普通浏览器内（仅看 UI / 调试）自动走 `mock` 假数据，使前端可脱离 Rust 编译单独预览。
 */

import { invoke } from "@tauri-apps/api/core";
import type {
  AppRankingOut,
  CategoryOut,
  CurrentForegroundOut,
  DailySummaryOut,
  DayCategoryOut,
  DeviceInfo,
  DeviceStats,
  ExportResult,
  HourlyBucketOut,
  MetricsOut,
  MonthSummaryOut,
  OverviewOut,
  PermissionStatus,
  RuleOut,
  SessionOut,
  SettingsOut,
  StatusBarConfig,
  TrendsOut,
  UpdateInfo,
  Webview2Status,
  BackupConfig,
} from "../types";
import { mock } from "./mock";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}
export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri) {
    return mock(cmd, args) as T;
  }
  return invoke<T>(cmd, args);
}

export const tracker = {
  start: () => call<boolean>("start_tracking"),
  stop: () => call<boolean>("stop_tracking"),
  isTracking: () => call<boolean>("is_tracking"),
  current: () => call<CurrentForegroundOut>("get_current_foreground"),
  overview: (days: number, date: string, device?: string) =>
    call<OverviewOut>("get_overview", { days, date, device: device ?? null }),
  daily: (days: number, device?: string) =>
    call<DailySummaryOut[]>("get_daily_summaries", { days, device: device ?? null }),
  dailyCategories: (days: number, device?: string) =>
    call<DayCategoryOut[]>("get_daily_categories", { days, device: device ?? null }),
  hourly: (date: string, device?: string) =>
    call<HourlyBucketOut[]>("get_hourly_buckets", { date, device: device ?? null }),
  ranking: (days: number, date: string, device?: string) =>
    call<AppRankingOut[]>("get_app_ranking", { days, date, device: device ?? null }),
  categories: () => call<CategoryOut[]>("get_categories"),
  monthSummary: (year: number, month: number, device?: string) =>
    call<MonthSummaryOut>("get_month_summary", {
      year,
      month,
      device: device ?? null,
    }),
  sessions: (date: string) => call<SessionOut[]>("get_sessions", { date }),
  setIdle: (secs: number) => call<boolean>("set_idle_threshold", { secs }),
  getIdle: () => call<number>("get_idle_threshold"),
  checkPermissions: () => call<PermissionStatus>("check_permissions"),
  openPrivacySettings: () => call<void>("open_privacy_settings"),
  checkWebview2: () => call<Webview2Status>("check_webview2"),
  openWebview2Download: () => call<void>("open_webview2_download"),
  checkUpdate: () => call<UpdateInfo>("check_for_update"),
  openUrl: (url: string) => call<void>("open_url", { url }),
  trends: (period: string, device?: string) =>
    call<TrendsOut>("get_trends", { period, device: device ?? null }),
  exportAll: (deviceId?: string) =>
    call<ExportResult>("export_all", { deviceId: deviceId ?? null }),
  backupAndPruneDevice: (deviceId: string) =>
    call<{ backup_path: string; deleted_count: number }>("backup_and_prune_device", {
      deviceId,
    }),
  importData: (content: string) => call<number>("import_data", { content }),
  getBackupConfig: () => call<BackupConfig>("get_backup_config"),
  saveBackupConfig: (s: { enabled: boolean; path: string; keepDays: number }) =>
    call<void>("save_backup_config", s),
  runBackupNow: () => call<ExportResult>("run_backup_now"),
  pruneData: (days: number, deviceIds?: string[]) =>
    call<number>("prune_data", {
      days,
      deviceIds: deviceIds && deviceIds.length > 0 ? deviceIds : null,
    }),
  revealPath: (path: string) => call<void>("reveal_path", { path }),
  devices: () => call<DeviceInfo[]>("get_devices"),
  devicesWithStats: () => call<DeviceStats[]>("list_devices_with_stats"),
  getSettings: () => call<SettingsOut>("get_settings"),
  saveSettings: (s: {
    idleThreshold: number;
    deviceName: string;
    dataRetentionDays: number;
  }) => call<boolean>("save_settings", s),
  rules: () => call<RuleOut[]>("get_rules"),
  addRule: (r: {
    field: string;
    matchType: string;
    pattern: string;
    categoryId: string;
    priority: number;
  }) => call<number>("add_rule", r),
  updateRule: (r: {
    id: number;
    field: string;
    matchType: string;
    pattern: string;
    categoryId: string;
    priority: number;
    enabled: boolean;
  }) => call<boolean>("update_rule", r),
  deleteRule: (id: number) => call<boolean>("delete_rule", { id }),
  reclassify: () => call<number>("reclassify_all"),
  setAutostart: (enabled: boolean) => call<boolean>("set_autostart", { enabled }),
  isAutostart: () => call<boolean>("is_autostart"),
  getAutostartPref: () => call<boolean | null>("get_autostart_pref"),
  // ===== v0.7.5：状态栏系统指标（v0.7.6 起扩展网络字段，向后兼容）=====
  getSystemMetrics: () => call<MetricsOut>("get_system_metrics"),
  setSystemMetricsEnabled: (enabled: boolean) =>
    call<boolean>("set_system_metrics_enabled", { enabled }),
  getSystemMetricsEnabled: () => call<boolean>("get_system_metrics_enabled"),
  // ===== v0.7.6：状态栏配置（总开关 + 3 个子项）=====
  getStatusBarConfig: () => call<StatusBarConfig>("get_status_bar_config"),
  setStatusBarConfig: (config: StatusBarConfig) =>
    call<boolean>("set_status_bar_config", { config }),
};
