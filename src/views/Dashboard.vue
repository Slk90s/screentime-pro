<template>
  <!-- Dashboard 主视图：组合总览卡片、柱状图、堆叠图、排行榜、趋势、设置 -->
  <div class="dashboard">
    <!-- 顶部标签页：统计概览 / 趋势对比 / 分类规则 / 设置 -->
    <div class="tabs">
      <button :class="{ active: tab === 'stats' }" @click="tab = 'stats'">统计概览</button>
      <button :class="{ active: tab === 'trends' }" @click="tab = 'trends'">趋势对比</button>
      <button :class="{ active: tab === 'rules' }" @click="tab = 'rules'">分类规则</button>
      <button :class="{ active: tab === 'settings' }" @click="tab = 'settings'">设置</button>
    </div>

    <!-- 标签页一：统计概览 -->
    <div v-if="tab === 'stats'">
      <div class="toolbar">
        <!-- 时间范围：今天 / 近 7/14/30 天 -->
        <div class="range">
          <button :class="{ active: range === 0 }" @click="selectToday">今天</button>
          <button :class="{ active: range === 7 }" @click="range = 7">近7天</button>
          <button :class="{ active: range === 14 }" @click="range = 14">近14天</button>
          <button :class="{ active: range === 30 }" @click="range = 30">近30天</button>
        </div>
        <!-- 设备切换（多设备合并） -->
        <DeviceSwitcher v-model="device" :devices="devices" />
        <!--
          历史：原「导出 CSV」按钮已删除。
          设置页「备份与多设备合并」板块有完整「导出备份」功能（JSON 全量 + 路径展示 + 复制），
          与本按钮功能重叠且功能更弱。v0.3.1 起移除。
        -->
      </div>

      <!-- 所选日期（点击上方柱状图的某一天切换；默认今天） -->
      <div class="seldate">
        <span>查看日期：<b>{{ selectedDate }}</b></span>
        <button class="pick-btn" @click="showPicker = true" title="选择指定日期">📅 选择</button>
        <button v-if="selectedDate !== todayStr()" class="today" @click="backToToday">回到今天</button>
        <span class="tip">点击上方柱状图的某一天，可查看当天详情</span>
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
      @close="showPicker = false"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, watch } from "vue";
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
  CategoryOut,
  DailySummaryOut,
  DayCategoryOut,
  DeviceInfo,
  HourlyBucketOut,
  OverviewOut,
} from "../types";

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
// 回到今天（每次调用实时取系统当前日期，解决跨天后仍显示旧日期的 bug）
function backToToday() {
  selectedDate.value = todayStr();
  showPicker.value = false;
  loadDetails({ days: 0, date: selectedDate.value });
}
// 顶部「今天」按钮：同时切换范围 + 刷新查看日期为当天
function selectToday() {
  range.value = 0;
  selectedDate.value = todayStr();
}

async function loadDevices() {
  try {
    devices.value = await tracker.devices();
  } catch {
    devices.value = [];
  }
}

// 历史：doExport 已移除，导出功能统一在「设置 → 备份与多设备合并」

onMounted(() => {
  loadDevices();
  loadRange();
  loadDetails();
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
/* 时间范围按钮 */
.range {
  display: inline-flex;
  gap: 4px;
  background: #f0f0f3;
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
  background: #fff;
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
