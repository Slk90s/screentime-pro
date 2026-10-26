<template>
  <!--
    Dashboard 主视图：组合总览卡片、柱状图、堆叠图、排行榜、趋势、设置

    v1.2.4（2026-10-22）：新增「自动检查更新」的首页提示横条。
      - 位置：统计概览页工具栏之下、所选日期之上（扫一眼能看到，且不遮挡图表）
      - 触发：启动 15s 后静默调用一次 auto_check_update（后端做 24h 节流）
      - 可关闭：关闭时把版本号写入 update_dismissed_version，同版本不再提示
      - 失败静默：断网 / GitHub 被拦 / API 限流一律不打扰用户，只写日志
  -->
  <div class="dashboard">
    <!-- 顶部标签页：统计概览 / 趋势对比 / 分类规则 / 设置 -->
    <div class="tabs">
      <button :class="{ active: tab === 'stats' }" @click="tab = 'stats'">{{ t("nav.stats") }}</button>
      <button :class="{ active: tab === 'trends' }" @click="tab = 'trends'">{{ t("nav.trends") }}</button>
      <button :class="{ active: tab === 'rules' }" @click="tab = 'rules'">{{ t("nav.rules") }}</button>
      <button :class="{ active: tab === 'settings' }" @click="tab = 'settings'">{{ t("nav.settings") }}</button>
    </div>

    <!-- 标签页一：统计概览 -->
    <div v-if="tab === 'stats'">
      <div class="toolbar">
        <!-- 时间范围：今天 / 近 7/14/30 天 -->
        <div class="range">
          <button :class="{ active: range === 0 }" @click="selectToday">{{ t("range.today") }}</button>
          <button :class="{ active: range === 7 }" @click="range = 7">{{ t("range.nDays", { range: 7 }) }}</button>
          <button :class="{ active: range === 14 }" @click="range = 14">{{ t("range.nDays", { range: 14 }) }}</button>
          <button :class="{ active: range === 30 }" @click="range = 30">{{ t("range.nDays", { range: 30 }) }}</button>
        </div>
        <!-- 设备切换（多设备合并） -->
        <DeviceSwitcher v-model="device" :devices="devices" />
        <!--
          历史：原「导出 CSV」按钮已删除。
          设置页「备份与多设备合并」板块有完整「导出备份」功能（JSON 全量 + 路径展示 + 复制），
          与本按钮功能重叠且功能更弱。v0.3.1 起移除。
        -->
      </div>

      <!--
        v1.2.4（2026-10-22）：自动检查更新的提示横条。
        放在工具栏之下、所选日期之上——这是「概览页顶部」，用户扫一眼就能看到，
        又不遮挡任何图表；且它属于一次性通知，不该占据工具栏的常驻位置。
        只在 has_update 为真时出现；关闭后写 dismissed_version，同版本不再提示。
      -->
      <div v-if="updateBanner" class="update-banner" role="status">
        <span class="ub-icon"><AppIcon name="download" :size="15" /></span>
        <div class="ub-text">
          <b>{{ t("dashboard.updateBanner.title", { latest: updateBanner.latest }) }}</b>
          <span>{{ t("dashboard.updateBanner.desc", { current: updateBanner.current }) }}</span>
        </div>
        <button class="ub-action" @click="goDownload">
          {{ t("dashboard.updateBanner.action") }}
        </button>
        <button
          class="ub-close"
          :title="t('dashboard.updateBanner.dismissTitle')"
          :aria-label="t('dashboard.updateBanner.dismiss')"
          @click="dismissBanner"
        >
          <AppIcon name="x" :size="14" />
        </button>
      </div>

      <!-- 所选日期（点击上方柱状图的某一天切换；默认今天） -->
      <div class="seldate">
        <span>{{ t("dashboard.viewDate") }}：<b>{{ selectedDate }}</b></span>
        <button class="pick-btn" @click="showPicker = true" :title="t('dashboard.select')"><AppIcon name="calendar" /> {{ t("dashboard.select") }}</button>
        <button v-if="selectedDate !== todayStr()" class="today" @click="backToToday">{{ t("dashboard.backToToday") }}</button>
        <span class="tip">{{ t("dashboard.clickBarTip") }}</span>
      </div>

      <OverviewCard :overview="overview" :range="range" />

      <div class="row">
        <DailyBarChart :summaries="daily" :by-category="dayCats" :categories="categories" @select="onPickDay" />
        <HourlyStackedChart :buckets="hourly" :categories="categories" />
      </div>

      <AppRanking :ranking="ranking" :categories="categories" :range="range" />
    </div>

    <!-- 标签页二：周/月同比分析 -->
    <TrendsView v-else-if="tab === 'trends'" :device="device" :devices="devices" />

    <!-- 标签页三：分类规则管理（自动归类引擎） -->
    <RulesView v-else-if="tab === 'rules'" />

    <!-- 标签页四：设置（设备名 / 阈值 / 保留 / 备份合并） -->
    <SettingsView v-else />

    <!-- 日历选择器（全局浮层） -->
    <DatePicker
      :visible="showPicker"
      :value="selectedDate"
      @select="onPickDate"
      @goto-today="onGotoToday"
      @close="showPicker = false"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch } from "vue";
import { useI18n } from "vue-i18n";
import OverviewCard from "../components/OverviewCard.vue";
import DailyBarChart from "../components/DailyBarChart.vue";
import HourlyStackedChart from "../components/HourlyStackedChart.vue";
import AppRanking from "../components/AppRanking.vue";
import DeviceSwitcher from "../components/DeviceSwitcher.vue";
import DatePicker from "../components/DatePicker.vue";
import RulesView from "./Rules.vue";
import TrendsView from "./Trends.vue";
import SettingsView from "./Settings.vue";
import { tracker } from "../api/tracker";
import { todayStr } from "../utils/format";
import type {
  AppRankingOut,
  AutoUpdateInfo,
  CategoryOut,
  DailySummaryOut,
  DayCategoryOut,
  DeviceInfo,
  HourlyBucketOut,
  OverviewOut,
} from "../types";
import { log } from "@/lib/logger";

const { t } = useI18n();

// 当前激活的标签页
const tab = ref<"stats" | "trends" | "rules" | "settings">("stats");

// 时间范围（天）
const range = ref(7);
// 当前选中的设备（"" = 合并全部设备）
const device = ref<string>("");
// 已知设备列表（供切换器与趋势页使用）
const devices = ref<DeviceInfo[]>([]);

// 当前查看的日期（默认今天；点击柱状图某天可切换）—— 每次通过 todayStr() 实时获取当前日期
const selectedDate = ref(todayStr());
// 日历选择器显示状态
const showPicker = ref(false);
const overview = ref<OverviewOut>({
  date: selectedDate.value,
  total_seconds: 0,
  app_count: 0,
  most_used_app: null,
  most_used_seconds: 0,
  pickup_count: 0,
});
const daily = ref<DailySummaryOut[]>([]);
const dayCats = ref<DayCategoryOut[]>([]);
const hourly = ref<HourlyBucketOut[]>([]);
const ranking = ref<AppRankingOut[]>([]);
const categories = ref<CategoryOut[]>([]);

// 加载时间范围内的汇总（按天柱状图 + 分类字典），与所选日期无关
// range=0 表示「今天」：daily 用 days=1 取今日一行（SQLite LIMIT），dayCats 用 days=0 自然只取今天
async function loadRange() {
  categories.value = await tracker.categories();
  const dailyDays = range.value === 0 ? 1 : range.value;
  const catDays = range.value === 0 ? 0 : range.value;
  daily.value = await tracker.daily(dailyDays, device.value);
  dayCats.value = await tracker.dailyCategories(catDays, device.value);
}
// 加载所选「某天 / 某范围」的详情（总览 / 小时分布 / 排行）
// opts.days 缺省时取当前 range：range=0 单日（用 selectedDate），range>0 范围聚合
// opts.date 缺省时取 selectedDate
async function loadDetails(opts?: { days?: number; date?: string }) {
  const days = opts?.days ?? range.value;
  const date = opts?.date ?? selectedDate.value;
  const dev = device.value;
  overview.value = await tracker.overview(days, date, dev);
  hourly.value = await tracker.hourly(date, dev);
  ranking.value = await tracker.ranking(days, date, dev);
}
// 点击柱状图某天 -> 切换查看日期并强制以单日模式刷新当天详情
function onPickDay(d: string) {
  selectedDate.value = d;
  showPicker.value = false;
  loadDetails({ days: 0, date: d });
}
// 从日历选择器选中日期 -> 强制单日模式
function onPickDate(d: string) {
  selectedDate.value = d;
  showPicker.value = false;
  loadDetails({ days: 0, date: d });
}
// v0.7.0：日历点「今天」——同步跳到今天所在月并刷新当天数据，但保持选择器打开
// （让「日历视图跳转到今天」的视觉效果可见；与 Dashboard 顶部「回到今天」按钮互补）
function onGotoToday() {
  selectedDate.value = todayStr();
  loadDetails({ days: 0, date: selectedDate.value });
}
// 回到今天（每次调用实时取系统当前日期，解决跨天后仍显示旧日期的 bug）
function backToToday() {
  selectedDate.value = todayStr();
  showPicker.value = false;
  loadDetails({ days: 0, date: selectedDate.value });
}
// 顶部「今天」按钮：同时切换范围 + 刷新查看日期为当天
// 注意：range 已是 0（单日）时 watch([range, device]) 不会触发，必须显式刷新，
// 否则「切到历史日期后再点今天」页面仍停留在历史那天（与 backToToday 对齐）。
function selectToday() {
  range.value = 0;
  selectedDate.value = todayStr();
  loadDetails({ days: 0, date: selectedDate.value });
}

// v0.7.9（2026-09-12）加固：
//   原实现是「赋值 + 空 catch」——① 若后端返回 null/undefined，会被直接赋给 devices，
//   `v-for` 静默渲染 0 个按钮（首页只剩「全部设备」）；② catch 为空，出问题无迹可查。
//   现改为：只在拿到数组时覆盖（失败保留旧值），并留痕便于定位。
async function loadDevices() {
  try {
    const list = await tracker.devices();
    if (Array.isArray(list)) {
      devices.value = list;
    } else {
      log.error("[devices] get_devices 返回非数组：", list);
    }
  } catch (e) {
    log.error("[devices] get_devices 调用失败：", e);
  }
}

// 历史：doExport 已移除，导出功能统一在「设置 → 备份与多设备合并」

// ===== v1.2.4（2026-10-22）：自动检查更新的首页提示 =====
//
// 设计取舍（为什么是「延迟 + 24h 节流 + 失败静默」）：
//   ① 延迟 15s：启动瞬间在做 DB 迁移 / 采样循环 / 建窗，此时叠一个网络请求会拖慢首屏。
//      15s 后界面已稳定，用户完全无感。
//   ② 24h 节流在后端做（不在前端），因为重装/清缓存不会重置它，且手动检查不写入
//      节流时间戳——两者互不干扰。
//   ③ 失败静默：见 `auto_check_update` 的注释。这里连 catch 都只写 debug 日志，
//      绝不进 UI。
const updateBanner = ref<AutoUpdateInfo | null>(null);

/** 定时器句柄：组件卸载时要清掉，否则窗口关闭后仍会触发一次 IPC */
let updateTimer: ReturnType<typeof setTimeout> | null = null;
/** v1.2.10：周期复查定时器句柄（见 onMounted 内说明） */
let updateInterval: ReturnType<typeof setInterval> | null = null;

/**
 * 自动检查的启动延时（毫秒）。等首屏数据和图表都画完再发请求，避免和首屏抢带宽/主线程。
 */
const UPDATE_FIRST_DELAY_MS = 15_000;

/**
 * v1.2.10（2026-10-26）：**周期复查间隔 = 6 小时**，与后端
 * `UPDATE_CHECK_INTERVAL_SECS` 保持一致（后端 6h 节流，前端 6h 触发；
 * 前端比后端慢或等速都能命中，快于后端则会被节流挡掉，只多一次 IPC 开销）。
 *
 * 为什么必须有这个 setInterval（这是「版本 27 没检测到更新」的主因）：
 * 原来只有 `setTimeout(..., 15_000)` 一次。而本应用是**常驻托盘**的
 * （lib.rs 的 on_window_event 里 CloseRequested → prevent_close + hide），
 * 用户点「×」只是隐藏窗口，WebView 不销毁、进程不退、onMounted 不会再跑。
 * 于是「进程启动 + 15 秒」是这个进程**一生**唯一的一次检查时机：
 *   - 实测样本：v1.2.4 从 10-23 07:54Z 连续运行 3 天 22 小时，期间 0 次检查，
 *     v1.2.8 于 10-26 11:54 发布时那个进程完全不知道。
 *   - 用户说的「打开应用后没检测到更新」= 从托盘唤出窗口（不是重启进程），
 *     所以在旧实现下永远不会触发。
 * 加上周期复查后，最坏延迟从「永不」压到 6 小时。
 */
const UPDATE_RECHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

async function checkUpdateSilently() {
  try {
    const info = await tracker.autoCheckUpdate();
    // has_update 已经包含了「已是最新 / 检查失败 / 该版本已忽略」三种情况，直接判它即可
    if (info?.has_update && info.latest) {
      updateBanner.value = info;
    }
  } catch (e) {
    // 理论上 autoCheckUpdate 不会 reject（后端把所有失败折叠成 has_update:false），
    // 但 IPC 层本身仍可能抛（如命令未注册）。静默处理，不影响首页任何功能。
    log.debug("[update] 自动检查更新静默失败：", e);
  }
}

/** 打开 GitHub Release 页（复用既有的 open_url 命令，协议白名单在 Rust 侧校验） */
async function goDownload() {
  const url = updateBanner.value?.url;
  if (!url) return;
  try {
    await tracker.openUrl(url);
  } catch (e) {
    log.warn("[update] 打开下载页失败：", e);
  }
}

/** 关闭横幅：记住这个版本号，同版本不再提示（更高版本仍会提示） */
async function dismissBanner() {
  const v = updateBanner.value?.latest;
  updateBanner.value = null; // 先隐藏，避免等 IPC 往返造成的点击延迟感
  if (!v) return;
  try {
    await tracker.dismissUpdate(v);
  } catch (e) {
    // 写失败只是「下次还会再提示」，不影响本次关闭的视觉效果，故不打扰用户
    log.warn("[update] 记录关闭的版本失败：", e);
  }
}

onMounted(() => {
  loadDevices();
  loadRange();
  loadDetails();
  // v1.2.10：首次延时 15s（等首屏画完）+ 此后每 6h 复查一次。
  // 见 UPDATE_RECHECK_INTERVAL_MS 的注释：应用常驻托盘，单次 setTimeout
  // 会让「一次检查」变成这个进程唯一的检查时机，新版本永远不会被发现。
  updateTimer = setTimeout(checkUpdateSilently, UPDATE_FIRST_DELAY_MS);
  updateInterval = setInterval(checkUpdateSilently, UPDATE_RECHECK_INTERVAL_MS);
});
// 卸载时清掉定时器：否则用户 15s 内切走或关窗后，回调仍会打一次 IPC
onBeforeUnmount(() => {
  if (updateTimer) {
    clearTimeout(updateTimer);
    updateTimer = null;
  }
  // v1.2.10：周期定时器同样必须清，否则组件卸载后每 6h 仍会打一次 IPC
  // （Dashboard 目前是主窗口常驻组件，但 keep-alive/条件渲染后就会泄漏）
  if (updateInterval) {
    clearInterval(updateInterval);
    updateInterval = null;
  }
});
// 切换时间范围或设备时重新拉取概览数据
watch([range, device], () => {
  loadRange();
  loadDetails();
});
</script>

<style scoped>
.dashboard {
  display: flex;
  flex-direction: column;
}
/* 标签页样式 */
.tabs {
  display: flex;
  gap: 8px;
  margin-bottom: 16px;
}
.tabs button {
  border: 1px solid var(--border);
  background: var(--card);
  color: var(--text-dim);
  padding: 6px 16px;
  border-radius: 8px;
  font-size: 13px;
  cursor: pointer;
}
.tabs button.active {
  background: var(--brand, #ff7e27);
  color: #fff;
  border-color: transparent;
}
.toolbar {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 16px;
  flex-wrap: wrap;
}
/* ===== v1.2.4（2026-10-22）：自动更新提示横条 =====
   配色刻意用 accent 系：既区别于「错误」（红）与「普通信息」（灰），
   又不至于像警告条那样刺眼——它只是通知，不是需要立即处理的问题。
   ⚠️ 这里刻意**不用 `color-mix()`**：它需要 Chromium 111+，而本项目
   `tauri.conf.json` 声明的 `minimumWebview2Version` 是 **110.0.1587.50**，
   在恰好 110 的机器上整条声明会失效（背景变透明、文字压桌面）。
   `--accent` 是 #ff7e27 系列的橙色，用固定的 rgba 近似即可，效果等价。 */
.update-banner {
  display: flex;
  /* ⚠️ 必须是 flex-start 而不是 center：窄窗口下文字会折成两行，
     若用 center 则图标、按钮都会相对「整块文字」垂直居中，视觉上图标沉到第二行、
     按钮悬在半空（实测 360px 宽时明显错位）。改用 flex-start 让各项都贴顶对齐，
     再把图标自身下移 1px 与首行基线对齐。 */
  align-items: flex-start;
  gap: 10px;
  padding: 9px 12px;
  margin-bottom: 14px;
  border: 1px solid rgba(255, 126, 39, 0.38);
  border-radius: 10px;
  background: rgba(255, 126, 39, 0.1);
  color: var(--text);
  font-size: 13px;
  line-height: 1.45;
}
.ub-icon {
  flex: none;
  /* 与首行文字视觉对齐：行高 1.45×13px≈18.9px，图标 15px，下移 1.5px 居中于首行 */
  margin-top: 1.5px;
  display: inline-flex;
  align-items: center;
  color: var(--accent);
}
.ub-text {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 2px 8px;
}
.ub-text b {
  font-weight: 600;
}
.ub-text span {
  color: var(--muted);
  font-size: 12px;
}
.ub-action {
  flex: none;
  /* 与首行文字对齐（见 .update-banner 的 flex-start 说明）。
     首行基线区高约 18.9px，按钮高 12.5×1.45+10 ≈ 28px，上移 4px 让文字居中于首行 */
  margin-top: -4px;
  border: none;
  background: var(--accent);
  color: #fff;
  padding: 5px 12px;
  border-radius: 7px;
  font-size: 12.5px;
  font-weight: 500;
  /* 按钮文案不折行：中文「查看更新」短，但英文 "View update" 在极窄容器里可能被拆成两行，
     造成按钮高度跳动。主窗口 minWidth=860 时不会触发，这里只是兜底。 */
  white-space: nowrap;
  cursor: pointer;
  transition: background 0.15s ease;
}
.ub-action:hover {
  background: var(--accent-strong);
}
.ub-close {
  flex: none;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  /* 24px 高 vs 首行 18.9px：上移 2.5px 让叉号与首行文字居中对齐 */
  margin-top: -2.5px;
  padding: 0;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--muted);
  cursor: pointer;
  transition:
    background 0.15s ease,
    color 0.15s ease;
}
.ub-close:hover {
  background: rgba(128, 128, 128, 0.18);
  color: var(--text);
}
/* 时间范围按钮 */
.range {
  display: inline-flex;
  gap: 4px;
  /* v0.7.9（2026-09-12）：改用主题令牌，深色主题下选中项不再白字压白底 */
  background: var(--seg-bg);
  padding: 3px;
  border-radius: 10px;
}
.range button {
  border: none;
  background: transparent;
  padding: 6px 12px;
  border-radius: 8px;
  font-size: 13px;
  color: var(--muted);
  cursor: pointer;
}
.range button.active {
  background: var(--seg-active-bg);
  color: var(--text);
  font-weight: 500;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
}
.export {
  margin-left: auto;
  border: 1px solid var(--border);
  background: var(--card);
  padding: 7px 14px;
  border-radius: 8px;
  font-size: 13px;
  cursor: pointer;
}
/* 所选日期指示条 */
.seldate {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 14px;
  font-size: 13px;
  color: var(--text-dim);
}
.seldate b {
  color: var(--text);
  font-weight: 600;
}
.seldate .today {
  border: 1px solid var(--brand, #ff7e27);
  color: var(--brand, #ff7e27);
  background: transparent;
  padding: 3px 12px;
  border-radius: 7px;
  font-size: 12px;
  cursor: pointer;
}
.seldate .pick-btn {
  border: 1px solid var(--border, #ddd);
  background: var(--card, #fff);
  color: var(--text-dim, #666);
  padding: 3px 12px;
  border-radius: 7px;
  font-size: 12px;
  cursor: pointer;
}
.seldate .pick-btn:hover {
  border-color: var(--brand, #ff7e27);
  color: var(--brand, #ff7e27);
}
.seldate .tip {
  margin-left: auto;
  color: var(--muted);
  font-size: 12px;
}
</style>
