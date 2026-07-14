// 浏览器预览模式下的模拟数据
//
// 当应用以普通网页方式打开（非 Tauri 运行时）时，前端 API 层会自动调用这里的
// `mock()` 返回假数据，方便在不编译 Rust 的情况下调试 UI 与图表。
import type {
  AppRankingOut,
  CategoryOut,
  DailySummaryOut,
  DayCategoryOut,
  DeviceInfo,
  ExportResult,
  HourlyBucketOut,
  OverviewOut,
  PeriodStat,
  SettingsOut,
  TrendsOut,
} from "../types";

// 分类字典种子（与后端 sql/seed_categories.sql 对应）

// 分类规则模拟种子（与后端 sql/seed_rules.sql 对应，供浏览器预览用）
export const mockRules = [
  { id: 1, field: "process_name", match_type: "contains", pattern: "wechat", category_id: "social", priority: 0, enabled: true },
  { id: 2, field: "process_name", match_type: "contains", pattern: "chrome", category_id: "productivity", priority: 0, enabled: true },
  { id: 3, field: "process_name", match_type: "contains", pattern: "code", category_id: "dev", priority: 0, enabled: true },
  { id: 4, field: "window_title", match_type: "contains", pattern: "netflix", category_id: "entertainment", priority: 5, enabled: true },
];

export const mockCategories: CategoryOut[] = [
  { id: "social", name: "社交", color: "#FF7E27" },
  { id: "productivity", name: "效率与财务", color: "#378ADD" },
  { id: "creative", name: "创意", color: "#D4537E" },
  { id: "entertainment", name: "娱乐", color: "#BA7517" },
  { id: "dev", name: "开发", color: "#3B6D11" },
  { id: "game", name: "游戏", color: "#993C1D" },
  { id: "other", name: "其他", color: "#888780" },
];

function rand(min: number, max: number): number {
  return Math.floor(Math.random() * (max - min + 1)) + min;
}

function dateMinusDays(i: number): string {
  const d = new Date();
  d.setDate(d.getDate() - i);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(
    d.getDate()
  ).padStart(2, "0")}`;
}

export function mock(cmd: string, args?: Record<string, unknown>): unknown {
  switch (cmd) {
    case "get_categories":
      return mockCategories;
    case "get_rules":
      return mockRules;
    case "add_rule":
      return mockRules.length + 1;
    case "update_rule":
    case "delete_rule":
    case "set_autostart":
    case "start_tracking":
    case "stop_tracking":
      return true;
    case "reclassify_all":
      return 0;
    case "is_autostart":
      return false;
    case "get_autostart_pref":
      return null;
    case "get_overview": {
      const days = (args?.days as number) || 0;
      const date = (args?.date as string) || dateMinusDays(0);
      // 范围模式（days>0）按天数放大，模拟累计；单日模式 scale=1
      const scale = days === 0 ? 1 : Math.max(1, days / 7);
      const total = (rand(6, 12) * 3600 + rand(0, 59) * 60) * scale;
      const avg = days > 0 ? Math.round(total / days) : 0;
      return {
        date: days === 0 ? date : `近${days}天`,
        total_seconds: total,
        app_count: rand(8, 20),
        most_used_app: "微信",
        most_used_seconds: (rand(2, 4) * 3600) * scale,
        pickup_count: rand(40, 90),
        avg_daily_seconds: avg,
      } as OverviewOut;
    }
    case "get_daily_summaries": {
      const days = (args?.days as number) || 7;
      const arr: DailySummaryOut[] = [];
      for (let i = days - 1; i >= 0; i--) {
        arr.push({
          date: dateMinusDays(i),
          total_seconds: rand(5, 13) * 3600,
          app_count: rand(8, 20),
        });
      }
      return arr;
    }
    case "get_daily_categories": {
      const days = (args?.days as number) || 7;
      const cats = ["social", "productivity", "creative", "dev", "entertainment", "other"];
      const arr: DayCategoryOut[] = [];
      for (let i = days - 1; i >= 0; i--) {
        const date = dateMinusDays(i);
        cats.forEach((c) => {
          const v = rand(0, c === "other" ? 1200 : 5400);
          if (v > 60) arr.push({ date, category_id: c, total_seconds: v });
        });
      }
      return arr;
    }
    case "get_hourly_buckets": {
      const arr: HourlyBucketOut[] = [];
      const cats = ["social", "productivity", "creative", "dev", "entertainment", "other"];
      for (let h = 0; h < 24; h++) {
        const active = h >= 9 && h <= 21;
        cats.forEach((c) => {
          const v = rand(0, active ? 1800 : 300);
          if (v > 0) arr.push({ hour: h, category_id: c, total_seconds: v });
        });
      }
      return arr;
    }
    case "get_app_ranking": {
      const days = (args?.days as number) || 0;
      const scale = days === 0 ? 1 : Math.max(1, days / 7);
      const apps = [
        { name: "微信", cat: "social" },
        { name: "Chrome", cat: "productivity" },
        { name: "VS Code", cat: "dev" },
        { name: "备忘录", cat: "productivity" },
        { name: "Spotify", cat: "entertainment" },
        { name: "Photoshop", cat: "creative" },
        { name: "Terminal", cat: "dev" },
        { name: "Safari", cat: "productivity" },
      ];
      const arr: AppRankingOut[] = apps
        .map((a, i) => ({
          app_id: i + 1,
          app_name: a.name,
          category_id: a.cat,
          total_seconds: (rand(1, 5) * 3600 + rand(0, 59) * 60) * scale,
          session_count: rand(5, 40),
          icon_base64: null,
        }))
        .sort((x, y) => y.total_seconds - x.total_seconds);
      return arr;
    }
    case "is_tracking":
      return false;
    case "get_current_foreground":
      return {
        name: "微信",
        process_name: "WeChat",
        category_id: "social",
        idle_seconds: 0,
        tracking: false,
        window_title: "文件传输助手",
        session_seconds: 1830,
      };
    case "get_idle_threshold":
      return 300;
    // ===== 周/月同比分析 =====
    case "get_trends": {
      const period = (args?.period as string) || "week";
      const build = (label: string, scale: number): PeriodStat => ({
        label,
        total_seconds: Math.floor(scale * (6 + Math.random() * 4)) * 3600,
        app_count: 12 + Math.floor(Math.random() * 8),
        by_category: mockCategories.map((c, i) => ({
          category_id: c.id,
          total_seconds: Math.floor(scale * (1 + Math.random() * 3) * (6 - i)) * 600,
        })),
        top_apps: [
          { app_name: "微信", category_id: "social", total_seconds: Math.floor(scale * 3.2 * 3600) },
          { app_name: "Chrome", category_id: "productivity", total_seconds: Math.floor(scale * 2.8 * 3600) },
          { app_name: "VS Code", category_id: "dev", total_seconds: Math.floor(scale * 2.1 * 3600) },
          { app_name: "备忘录", category_id: "productivity", total_seconds: Math.floor(scale * 1.4 * 3600) },
          { app_name: "Spotify", category_id: "entertainment", total_seconds: Math.floor(scale * 1.0 * 3600) },
        ],
      });
      const current = build(period === "month" ? "本月" : "本周", 1);
      const prev = build(period === "month" ? "上月" : "上周", 0.9);
      const yoy = period === "month" ? build("去年同期", 0.8) : null;
      const delta = prev.total_seconds > 0
        ? ((current.total_seconds - prev.total_seconds) / prev.total_seconds) * 100
        : 0;
      return {
        period,
        current,
        prev,
        yoy,
        delta_total_pct: Math.round(delta * 10) / 10,
      } as TrendsOut;
    }
    // ===== 全量导出 / 导入合并 =====
    case "export_all":
      // mock：浏览器预览场景，路径用 file:// 协议路径占位
      return {
        path: `~/Library/Application Support/com.screentime.pro/exports/screentime_backup${args?.deviceId ? "_" + (args.deviceId as string).slice(0, 12) : ""}_mock.json`,
      } as ExportResult;
    case "backup_and_prune_device":
      return {
        backup_path: `~/Library/Application Support/com.screentime.pro/exports/screentime_backup_${((args?.deviceId as string) || "unknown").slice(0, 12)}_pre_purge_mock.json`,
        deleted_count: 0, // 浏览器预览场景没真数据
      };
    case "reveal_path":
      return null;
    case "import_data":
      return 128; // 模拟导入合并了 128 条记录
    case "prune_data":
      return 0;
    // ===== 多设备合并 =====
    case "get_devices": {
      const arr: DeviceInfo[] = [
        { id: "a1b2c3d4e5f6", name: "我的 MacBook Pro" },
        { id: "f6e5d4c3b2a1", name: "办公室 Windows 台式机" },
      ];
      return arr;
    }
    case "list_devices_with_stats": {
      // 模拟每个设备的统计数据（用于「按设备清理」弹窗预览）
      return [
        {
          device_id: "a1b2c3d4e5f6",
          device_name: "我的 MacBook Pro",
          total_seconds: 86400 * 3 + 7200,
          session_count: 142,
          earliest_date: "2026-05-01",
          latest_date: "2026-07-08",
        },
        {
          device_id: "f6e5d4c3b2a1",
          device_name: "办公室 Windows 台式机",
          total_seconds: 86400 + 3600,
          session_count: 38,
          earliest_date: "2026-06-15",
          latest_date: "2026-07-05",
        },
      ];
    }
    case "get_settings": {
      const s: SettingsOut = {
        device_id: "a1b2c3d4e5f6",
        device_name: "我的 MacBook Pro",
        idle_threshold: 300,
        data_retention_days: 365,
        sample_interval: 2,
        autostart: true,
      };
      return s;
    }
    case "save_settings":
      return true;
    case "check_for_update":
      // 浏览器预览场景：模拟「已是最新版本」
      return {
        current: "0.4.0",
        latest: "0.4.0",
        has_update: false,
        url: "https://github.com/Slk90s/screentime-pro/releases/latest",
        notes: "mock: 浏览器预览场景无网络，返回假数据",
      };
    case "open_url":
      // 浏览器预览场景：mock 不真的打开，只 console.log
      console.log("[mock] open_url:", args?.url);
      return null;
    default:
      return null;
  }
}
