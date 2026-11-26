/**
 * 设计思路：在非 Tauri 环境提供模拟统计与配置数据，支撑界面和图表脱离 Rust 独立调试。
 * 修改历史：2026-11-03 @v1.2.12: 新增 - 补充文件头设计说明与修改历史。
 * 2026-10-30 @v1.2.13: 修复 - 规则 mock 真实增删改；补独立磁盘查询并移除不存在的命令。
 */
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
  DiskVolume,
  ExportResult,
  HourlyBucketOut,
  KeyboardConfigOut,
  KeyboardStatsOut,
  KeyboardSummaryOut,
  NetworkProbeConfig,
  NetworkStatusOut,
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
for (let i = 5; i <= 34; i++) {
  mockRules.push({
    id: i,
    field: i % 4 === 0 ? "window_title" : "process_name",
    match_type: i % 3 === 0 ? "prefix" : "contains",
    pattern: `app-${String(i).padStart(2, "0")}`,
    category_id: ["social", "productivity", "creative", "entertainment", "dev", "education", "game", "other"][i % 8],
    priority: i % 5,
    enabled: i % 6 !== 0,
  });
}

const mockDiskVolumes: DiskVolume[] = [
  { mount: "C:", total_bytes: 200_000_000_000, used_bytes: 110_000_000_000, avail_bytes: 90_000_000_000, is_system: true },
  { mount: "D:", total_bytes: 400_000_000_000, used_bytes: 100_000_000_000, avail_bytes: 300_000_000_000, is_system: false },
];

const mockNetworkProbeConfig: NetworkProbeConfig = {
  enabled: false,
  public_ip_enabled: false,
};
const mockKeyboardConfig: KeyboardConfigOut = {
  enabled: true,
  status: "ready",
  reason: null,
};
let mockKeyboardCleared = false;

function mockNetworkStatus(): NetworkStatusOut {
  const enabled = mockNetworkProbeConfig.enabled;
  const publicIpEnabled = mockNetworkProbeConfig.public_ip_enabled;
  return {
    link: "up",
    // 预览默认覆盖最完整的 VPN 标签：VPN · Wi-Fi（TUN）。
    medium: "vpn",
    band: "ghz5",
    vpn: true,
    // 同时开启系统代理，预览「TUN + 系统代理」组合标签。
    proxy: true,
    physical_medium: "wifi",
    tunnel: "tun",
    local_ip: "192.168.31.42",
    internet: enabled ? "online" : "disabled",
    latency_p50_ms: enabled ? 28.4 : null,
    latency_p95_ms: enabled ? 71.8 : null,
    jitter_ms: enabled ? 6.2 : null,
    failure_pct: enabled ? 4.2 : null,
    packet_loss_pct: enabled ? 0.8 : null,
    attempts: enabled ? 24 : 0,
    successes: enabled ? 23 : 0,
    sampled_at_ms: enabled ? Date.now() : 0,
    public_ip: publicIpEnabled ? "203.0.113.42" : null,
    region: publicIpEnabled ? "Shanghai, CN" : null,
  };
}

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

function mockKeyboardStats(period: string, anchor: string): KeyboardStatsOut {
  const trend = mock("get_trends", { period, anchor }) as TrendsOut;
  const empty = !mockKeyboardConfig.enabled || mockKeyboardCleared;
  const daily = trend.current.daily.map((day, index) => ({
    date: day.date,
    count: empty ? 0 : 260 + ((index * 137 + Number(day.date.slice(-2)) * 41) % 920),
  }));
  const total = daily.reduce((sum, day) => sum + day.count, 0);
  const previousTotal = empty ? 0 : Math.round(total * 0.88);
  const topCounts = [0.24, 0.08, 0.065, 0.052, 0.043, 0.036, 0.029, 0.022];
  const keyIds = [0x2c, 0x04, 0x16, 0x07, 0x09, 0x0d, 0x0e, 0x0f];
  return {
    status: mockKeyboardConfig.enabled ? "ready" : "disabled",
    anchor,
    current_from: trend.current_from,
    current_to: trend.current_to,
    prev_from: trend.prev_from,
    prev_to: trend.prev_to,
    total_count: total,
    prev_total_count: previousTotal,
    delta_pct: previousTotal > 0 ? Math.round(((total - previousTotal) / previousTotal) * 1000) / 10 : 0,
    daily,
    keys: empty ? [] : keyIds.map((key_id, index) => ({
      key_id,
      count: Math.round(total * topCounts[index]),
    })),
  };
}

export function mock(cmd: string, args?: Record<string, unknown>): unknown {
  switch (cmd) {
    case "screenshot_commit":
      // 浏览器预览没有 Rust 活动帧；明确失败，不能伪造剪贴板/落盘成功。
      throw new Error("浏览器预览没有活动截图会话，无法提交");
    case "screenshot_cancel":
      // 与 Rust 的过期取消一致：预览没有活动原生会话，安全 no-op。
      return null;
    case "get_categories":
      return mockCategories;
    case "get_rules":
      return mockRules.map((rule) => ({ ...rule }));
    case "get_rules_page": {
      const keyword = String(args?.keyword ?? "").trim().toLowerCase();
      const pageSize = Math.max(1, Math.min(100, Number(args?.pageSize) || 20));
      const sortKey = String(args?.sortKey ?? "category_id");
      const direction = args?.sortDir === "desc" ? -1 : 1;
      const filtered = mockRules.filter((rule) =>
        !keyword || `${rule.pattern} ${rule.category_id}`.toLowerCase().includes(keyword),
      );
      filtered.sort((a, b) => {
        const av = (a as unknown as Record<string, unknown>)[sortKey];
        const bv = (b as unknown as Record<string, unknown>)[sortKey];
        const cmp = typeof av === "number" && typeof bv === "number"
          ? av - bv
          : String(av ?? "").localeCompare(String(bv ?? ""), "zh-CN", { sensitivity: "base" });
        return cmp === 0 ? a.id - b.id : cmp * direction;
      });
      const pageCount = Math.max(1, Math.ceil(filtered.length / pageSize));
      const page = Math.max(1, Math.min(pageCount, Number(args?.page) || 1));
      return {
        items: filtered.slice((page - 1) * pageSize, page * pageSize).map((rule) => ({ ...rule })),
        total: filtered.length,
        page,
        page_size: pageSize,
      };
    }
    case "add_rule": {
      const id = Math.max(0, ...mockRules.map((rule) => rule.id)) + 1;
      mockRules.push({
        id,
        field: args?.field as string,
        match_type: args?.matchType as string,
        pattern: args?.pattern as string,
        category_id: args?.categoryId as string,
        priority: args?.priority as number,
        enabled: args?.enabled as boolean,
      });
      return id;
    }
    case "update_rule": {
      const rule = args?.rule as Record<string, unknown>;
      const index = mockRules.findIndex((item) => item.id === rule.id);
      if (index < 0) throw new Error("规则不存在");
      mockRules[index] = {
        id: rule.id as number,
        field: rule.field as string,
        match_type: rule.match_type as string,
        pattern: rule.pattern as string,
        category_id: rule.category_id as string,
        priority: rule.priority as number,
        enabled: rule.enabled as boolean,
      };
      return true;
    }
    case "delete_rule": {
      const index = mockRules.findIndex((rule) => rule.id === args?.id);
      if (index < 0) throw new Error("规则不存在");
      mockRules.splice(index, 1);
      return true;
    }
    case "set_autostart":
    case "start_tracking":
      return true;
    case "reclassify_all":
      return 0;
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
    case "get_month_summary": {
      const year = (args?.year as number) || new Date().getFullYear();
      const month = (args?.month as number) || new Date().getMonth() + 1;
      const leap = (year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0));
      const md = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
      const daysInMonth = md[(month - 1) as number] || 30;
      const activeDays = Math.floor(daysInMonth * 0.7);
      const total = activeDays * (4 + Math.random() * 4) * 3600;
      return {
        year,
        month,
        total_seconds: Math.floor(total),
        active_days: activeDays,
        days_in_month: daysInMonth,
        avg_daily_seconds: Math.floor(total / activeDays),
        top_app: "VS Code",
        top_app_seconds: Math.floor(total * 0.3),
        busiest_date: `${year}-${String(month).padStart(2, "0")}-${String(
          Math.floor(daysInMonth / 2),
        ).padStart(2, "0")}`,
        busiest_seconds: Math.floor(9 * 3600),
      } as import("../types").MonthSummaryOut;
    }
    case "get_current_foreground":
      return {
        name: "微信",
        process_name: "WeChat",
        category_id: "social",
        idle_seconds: 0,
        tracking: false,
        window_title: "文件传输助手",
        bundle_id: null,
        session_seconds: 1830,
      };
    case "check_permissions":
      return { accessibility: true, screen_capture: true };
    case "get_status_bar_config":
      return {
        enabled: true,
        show_cpu: true,
        show_mem: true,
        show_disk: false,
        show_net: true,
        float_enabled: true,
        screenshot_enabled: true,
        float_glass: false,
      };
    case "get_system_metrics":
      return {
        supported: true,
        enabled: true,
        cpu_usage: 0.23,
        memory_usage: 0.61,
        memory_used_bytes: 10_000_000_000,
        memory_total_bytes: 16_000_000_000,
        disk_usage: 0.55,
        disk_used_bytes: 110_000_000_000,
        disk_total_bytes: 200_000_000_000,
        disk_volumes: mockDiskVolumes,
        net_rx_bps: 125_000,
        net_tx_bps: 42_000,
      };
    case "get_disk_volumes":
      return mockDiskVolumes;
    case "get_network_status":
      return mockNetworkStatus();
    case "get_network_probe_config":
      return { ...mockNetworkProbeConfig };
    case "set_network_probe_config": {
      const enabled = args?.enabled;
      const publicIpEnabled = args?.publicIpEnabled;
      if (typeof enabled !== "boolean" || typeof publicIpEnabled !== "boolean") {
        throw new Error("set_network_probe_config 需要 enabled 和 public_ip_enabled");
      }
      mockNetworkProbeConfig.enabled = enabled;
      mockNetworkProbeConfig.public_ip_enabled = publicIpEnabled;
      return null;
    }
    case "get_log_size":
      return 1024 * 1024;
    // ⚠️ v1.1.1 删除 `case "get_idle_threshold"`：该 command 已由工作区他人改动移除，
    // 残留会让 mock 与真实命令列表漂移（生产不走 mock，运行时无害）。
    // ===== 周/月同比分析 =====
    case "get_trends": {
      const period = (args?.period as string) || "week";
      const now = new Date();
      const pad = (n: number) => String(n).padStart(2, "0");
      const fmt = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
      const anchor = (args?.anchor as string) ||
        (period === "month" ? `${now.getFullYear()}-${pad(now.getMonth() + 1)}` : fmt(now));
      const dates: string[] = [];
      let prevFrom: Date;
      let prevTo: Date;
      if (period === "month") {
        const [year, month] = anchor.split("-").map(Number);
        const isCurrent = year === now.getFullYear() && month === now.getMonth() + 1;
        const count = isCurrent ? now.getDate() : new Date(year, month, 0).getDate();
        for (let day = 1; day <= count; day++) dates.push(fmt(new Date(year, month - 1, day)));
        prevFrom = new Date(year, month - 2, 1);
        prevTo = isCurrent
          ? new Date(year, month - 1, Math.min(now.getDate(), new Date(year, month - 1, 0).getDate()))
          : new Date(year, month - 1, 0);
      } else {
        const picked = new Date(`${anchor}T00:00:00`);
        const monday = new Date(picked);
        monday.setDate(picked.getDate() - ((picked.getDay() + 6) % 7));
        const isCurrent = fmt(monday) === fmt(new Date(now.getFullYear(), now.getMonth(), now.getDate() - ((now.getDay() + 6) % 7)));
        const count = isCurrent ? ((now.getDay() + 6) % 7) + 1 : 7;
        for (let i = 0; i < count; i++) {
          const day = new Date(monday);
          day.setDate(monday.getDate() + i);
          dates.push(fmt(day));
        }
        prevFrom = new Date(monday);
        prevFrom.setDate(monday.getDate() - 7);
        prevTo = new Date(monday);
        prevTo.setDate(monday.getDate() - 1);
      }
      const daily = dates.map((date, i) => ({
        date,
        total_seconds: ((i * 7 + Number(date.slice(-2)) * 3) % 9) * 1500 + (i % 3) * 900,
      }));
      const build = (label: string, scale: number, withDaily = false): PeriodStat => ({
        label,
        total_seconds: withDaily
          ? daily.reduce((sum, day) => sum + day.total_seconds, 0)
          : Math.floor(scale * 8) * 3600,
        app_count: 12 + Math.floor(scale * 5),
        by_category: mockCategories.map((c, i) => ({
          category_id: c.id,
          total_seconds: Math.floor(scale * (1 + (i % 3)) * (6 - i)) * 600,
        })),
        top_apps: [
          { app_name: "微信", category_id: "social", total_seconds: Math.floor(scale * 3.2 * 3600) },
          { app_name: "Chrome", category_id: "productivity", total_seconds: Math.floor(scale * 2.8 * 3600) },
          { app_name: "VS Code", category_id: "dev", total_seconds: Math.floor(scale * 2.1 * 3600) },
          { app_name: "备忘录", category_id: "productivity", total_seconds: Math.floor(scale * 1.4 * 3600) },
          { app_name: "Spotify", category_id: "entertainment", total_seconds: Math.floor(scale * 1.0 * 3600) },
        ],
        daily: withDaily ? daily : [],
      });
      const currentStat = build(period === "month" ? "本月" : "本周", 1, true);
      const prev = build(period === "month" ? "上月" : "上周", 0.9);
      const yoy = period === "month" ? build("去年同期", 0.8) : null;
      const delta = prev.total_seconds > 0
        ? ((currentStat.total_seconds - prev.total_seconds) / prev.total_seconds) * 100
        : 0;
      return {
        period,
        anchor,
        current_from: dates[0],
        current_to: dates[dates.length - 1],
        prev_from: fmt(prevFrom),
        prev_to: fmt(prevTo),
        current: currentStat,
        prev,
        yoy,
        delta_total_pct: Math.round(delta * 10) / 10,
      } as TrendsOut;
    }
    case "get_keyboard_summary": {
      const days = Math.max(0, Number(args?.days) || 0);
      const total = !mockKeyboardConfig.enabled || mockKeyboardCleared
        ? 0
        : days === 0 ? 1842 : days * 1150;
      return {
        status: mockKeyboardConfig.enabled ? "ready" : "disabled",
        total_count: total,
        daily_average: days > 0 ? Math.round(total / days) : total,
        top_key: total > 0 ? { key_id: 0x2c, count: Math.round(total * 0.24) } : null,
      } as KeyboardSummaryOut;
    }
    case "get_keyboard_stats":
      return mockKeyboardStats(
        (args?.period as string) || "week",
        (args?.anchor as string) || "",
      );
    case "get_keyboard_config":
      return { ...mockKeyboardConfig };
    case "set_keyboard_enabled": {
      const enabled = args?.enabled;
      if (typeof enabled !== "boolean") throw new Error("set_keyboard_enabled 需要 boolean enabled");
      mockKeyboardConfig.enabled = enabled;
      mockKeyboardConfig.status = enabled ? "ready" : "disabled";
      if (enabled) mockKeyboardCleared = false;
      return { ...mockKeyboardConfig };
    }
    case "clear_keyboard_stats":
      mockKeyboardCleared = true;
      return null;
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
    // ===== v0.7.2 本地自动备份 =====
    case "get_backup_config":
      return {
        enabled: false,
        path: "",
        keep_days: 30,
        last_date: "",
      } as import("../types").BackupConfig;
    case "save_backup_config":
      return null;
    case "run_backup_now":
      return {
        path: `~/Library/Application Support/com.screentime.pro/exports/screentime_backup_mock.json`,
      } as ExportResult;
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
    case "auto_check_update":
      // v1.2.4：浏览器预览场景模拟「发现新版本」，便于直接调样式。
      // 改 has_update 为 false 即可验证「无更新时不渲染横幅」。
      return {
        has_update: true,
        current: "1.2.3",
        latest: "1.2.4",
        url: "https://github.com/Slk90s/screentime-pro/releases/latest",
      };
    case "dismiss_update":
      console.log("[mock] dismiss_update:", args?.version);
      return true;
    case "open_url":
      // 浏览器预览场景：mock 不真的打开，只 console.log
      console.log("[mock] open_url:", args?.url);
      return null;
    case "float_glass_support":
      // v1.2.2：浏览器预览没有系统合成器，按"不支持"返回，设置页会置灰开关
      // v1.2.8：补 native_blur / translucency，与真实返回体字段保持一一对应
      return {
        supported: false,
        tier: "none",
        reason: "mock: 浏览器预览环境无系统玻璃材质",
        native_blur: false,
        translucency: 0,
      };
    default:
      return null;
  }
}
